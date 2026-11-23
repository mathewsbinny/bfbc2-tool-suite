//! Whole Rush stages added and taken out (`bc2_levelwrite::rushstage`, `EDITOR_ARCHITECTURE.md` 3.4): on a synthetic
//! Rush level (CI, no game data) and on Port Valdez Rush (`MP_012GR`) - a stage added after the last and played
//! last with its copies as the last stage's objects were, taken out again; a stage taken out of the middle and the
//! first; every operation from the project's log, the partition read back after it is written (a DBX round trip).
//! The game-tier sweep checks DICE's stage wiring (GAME-RUSH-11) on every stock Rush and Squad Rush level and the
//! tools on each: a stage added and removed again, every stage taken out once.
//!
//! The real-level cases read OPFOR's game copy (beside the suite, or `OPFOR_GAME`) and are skipped without it.
//! Nothing is written.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::panic,
    clippy::cast_possible_wrap,
    clippy::doc_markdown,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_precision_loss,
    clippy::iter_over_hash_type,
    clippy::bool_assert_comparison,
    clippy::assert_is_empty,
    missing_docs
)]

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::ops::{Connection, FieldValue, Operation};
use bc2_levelwrite::project::VERSION;
use bc2_levelwrite::rushstage;
use bc2_levelwrite::write::{Applied, LevelWriter, apply_one};
use bc2_levelwrite::{MapProject, Op, Profile, Schema, modelayer};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// The partition written and read back: the same bytes again (the DBX round trip).
fn round_trip(bytes: &[u8]) -> LevelWriter {
    let w = LevelWriter::parse(bytes).unwrap();
    assert_eq!(
        w.write().unwrap(),
        bytes,
        "the partition reads back and writes the same bytes"
    );
    w
}

fn applied(w: &LevelWriter, schema: &Schema, op: &Op) -> LevelWriter {
    let mut v = w.clone();
    let o = Operation {
        n: 1,
        by: String::new(),
        when: String::new(),
        label: String::new(),
        op: op.clone(),
    };
    apply_one(&mut v, &o, schema, &mut Applied::default()).unwrap();
    v
}

fn base_count(w: &LevelWriter, schema: &Schema) -> i64 {
    let st = rushstage::stages(w).unwrap();
    match w.get_field(schema, &st.defenders, "BaseCount").unwrap().1 {
        Some(FieldValue::Int(i)) => i,
        other => panic!("{other:?}"),
    }
}

/// The stages' states (settled) of the objects of `keep`.
fn states_of(w: &LevelWriter, keep: &HashSet<String>) -> Vec<HashMap<String, bool>> {
    rushstage::stages(w)
        .unwrap()
        .states
        .into_iter()
        .map(|s| s.into_iter().filter(|(g, _)| keep.contains(g)).collect())
        .collect()
}

/// What the tools must do on a Rush level: a stage added after the last is played last, the stages before as they
/// were, its copies in play as the last stage's objects were (the last stage's own out of play); taken out again,
/// the level's instances, stage states and connections are back (the last stage's counter keeps the wiring a middle
/// stage's has: what it gained, listed); each stage taken out leaves the others played in their order with their
/// states (a combat area switched through the removed stage's timers may differ: counted). Returns (copies, wires,
/// connections the counter kept, states that differ after a stage taken out).
fn check_level(lv: &str, w0: &LevelWriter, schema: &Schema) -> (usize, usize, usize, usize) {
    let st = rushstage::stages(w0).unwrap();
    let n = st.stages.len();
    let all: HashSet<String> = w0.guids().into_iter().collect();
    let before = states_of(w0, &all);
    let last = &st.stages[n - 1];
    let t = w0.transform(&last.mcoms[0]).unwrap()[3];
    let added = rushstage::add_stage(
        w0,
        schema,
        [t[0] + 80.0, t[1], t[2] + 40.0],
        &|s| bc2_levelwrite::new_guid("Rush stages", 1, s),
        &|_, _| None,
    )
    .unwrap_or_else(|e| panic!("{lv}: {e}"));
    let w = applied(w0, schema, &added.op);
    let (play, uncertain) = modelayer::rush_play(&w);
    assert!(!uncertain, "{lv}: the stages are played in one order");
    assert_eq!(play.len(), n + 1, "{lv}");
    assert_eq!(play[n].mcoms, added.mcoms, "{lv}: the new stage is played last");
    for k in 0..n {
        assert_eq!(play[k].mcoms, st.play[k].mcoms, "{lv}: stage {} as before", k + 1);
    }
    assert_eq!(base_count(&w, schema), base_count(w0, schema) + 1, "{lv}: BaseCount");
    let now = states_of(&w, &all);
    for k in 0..n {
        assert_eq!(now[k], before[k], "{lv}: stage {} plays as before", k + 1);
    }
    // the new stage: the last stage's own out of play, everything else as during the last stage
    for (g, on) in &before[n - 1] {
        if !last.set.contains(g) {
            assert_eq!(
                now[n].get(g).copied().unwrap_or(true),
                *on,
                "{lv}: {g} during the new stage"
            );
        }
    }
    for g in last
        .own
        .iter()
        .filter(|g| !w0.type_of(w0.find(g).unwrap()).ends_with(".MultipleTriggerEntityData"))
    {
        assert_eq!(
            now[n].get(g).copied().unwrap_or(true),
            false,
            "{lv}: the last stage's {g} out of play"
        );
    }
    // a forward base DICE's last stage switches on as the round ends (MP_006's: the base of a stage the level does
    // not have) belongs to the new stage once there is one
    let c0 = rushstage::connections(w0);
    let dormant: HashSet<String> = c0
        .iter()
        .filter(|c| c.source == last.base && c.source_event == "OnTrigger" && c.target_event == "Enable")
        .filter(|c| before.iter().all(|s| s.get(&c.target) == Some(&false)))
        .flat_map(|c| modelayer::cluster(w0, &c.target))
        .collect();
    let st1 = rushstage::stages(&w).unwrap();
    let originals: Vec<&String> = st1.stages[n].set.iter().filter(|g| all.contains(*g)).collect();
    assert!(
        originals.iter().all(|g| dormant.contains(*g)),
        "{lv}: the new stage's set is the copies"
    );
    assert_eq!(
        st1.stages[n].set.len() - originals.len(),
        added.copies,
        "{lv}: every copy in the new stage"
    );
    // taken out again
    let back = applied(&w, schema, &rushstage::remove_stage(&w, schema, n).unwrap());
    // the instances again (a dormant forward base goes with the added stage)
    let kept_guids: HashSet<String> = back.guids().into_iter().collect();
    let lost: Vec<String> = w0.guids().into_iter().filter(|g| !kept_guids.contains(g)).collect();
    for g in &lost {
        assert!(dormant.contains(g), "{lv}: {g} is back");
    }
    assert_eq!(
        back.guids(),
        w0.guids().into_iter().filter(|g| !lost.contains(g)).collect::<Vec<_>>(),
        "{lv}: the level's instances again"
    );
    let left: HashSet<String> = all.iter().filter(|g| !lost.contains(g)).cloned().collect();
    assert_eq!(
        states_of(&back, &left),
        states_of(w0, &left),
        "{lv}: the stages play as before"
    );
    assert_eq!(base_count(&back, schema), base_count(w0, schema));
    let c1 = rushstage::connections(&back);
    let set0: HashSet<&Connection> = c0.iter().collect();
    assert!(
        c0.iter()
            .filter(|c| !lost.contains(&c.source) && !lost.contains(&c.target))
            .all(|c| c1.contains(c)),
        "{lv}: every connection of the level is back"
    );
    if !lost.is_empty() {
        println!("{lv}: the round's-end forward base went with the added stage: {lost:?}");
    }
    let kept: Vec<&Connection> = c1.iter().filter(|c| !set0.contains(c)).collect();
    for c in &kept {
        assert_eq!(c.source, last.base, "{lv}: {} is the last counter's", c.describe());
    }
    // every stage taken out
    let mut differ = 0usize;
    for k in 0..n {
        let v = applied(w0, schema, &rushstage::remove_stage(w0, schema, k).unwrap());
        let (play, uncertain) = modelayer::rush_play(&v);
        assert!(!uncertain, "{lv}: stage {} out", k + 1);
        let left: Vec<&Vec<String>> = (0..n).filter(|j| *j != k).map(|j| &st.play[j].mcoms).collect();
        assert_eq!(play.iter().map(|s| &s.mcoms).collect::<Vec<_>>(), left, "{lv}");
        assert_eq!(base_count(&v, schema), base_count(w0, schema) - 1);
        let here: HashSet<String> = v.guids().into_iter().collect();
        let after = states_of(&v, &here);
        let mut j = 0;
        for (old, s) in before.iter().enumerate() {
            if old == k {
                continue;
            }
            // an object whose state the play leaves uncertain (the play's raw state and the counters' last word
            // differ: DICE's timers and counters acting on it at one stage's end) is not compared
            let raw = &st.play[old].enabled;
            let certain = |g: &String, on: &bool| raw.get(g).copied().unwrap_or(true) == *on;
            for (g, on) in s.iter().filter(|(g, on)| here.contains(*g) && certain(g, on)) {
                if after[j].get(g).copied().unwrap_or(true) == *on {
                    continue;
                }
                // the bases, counters and spawns play as before; a combat area or another object switched through
                // the removed stage's timers (Vietnam's hold-back areas) is counted and listed
                let ty = w0.type_of(w0.find(g).unwrap());
                let short = ty.rsplit('.').next().unwrap_or(&ty).to_owned();
                assert!(
                    ![
                        "CapturePointEntityData",
                        "MultipleTriggerEntityData",
                        "SoldierSpawnEntityData"
                    ]
                    .contains(&short.as_str()),
                    "{lv}: stage {} out, {short} {g} during stage {}",
                    k + 1,
                    old + 1
                );
                println!(
                    "{lv}: stage {} out, {short} {g} during stage {} differs",
                    k + 1,
                    old + 1
                );
                differ += 1;
            }
            j += 1;
        }
    }
    (added.copies, added.wires, kept.len(), differ)
}

// ------------------------------------------------------------------------------------------------ synthetic

/// A synthetic Rush level as DICE wires one (GAME-RUSH-11): three stages of two M-COMs, each with its base counter
/// (stage 1's enabled, each enabling the next), a team-2 base and a combat area per stage switched by the counters,
/// the defenders' life counter counting the stages.
fn synthetic() -> Vec<u8> {
    use crate::export_rules::{Lb, at, boolean, int, rect, text};
    let mut b = Lb::new();
    let defenders = b.add(
        "LifeCounterEntityData",
        vec![
            text("TeamId", "Team2"),
            boolean("Attacker", false),
            int("BaseCount", 3),
            at([0.0; 3]),
        ],
        true,
    );
    b.add(
        "LifeCounterEntityData",
        vec![
            text("TeamId", "Team1"),
            boolean("Attacker", true),
            int("StartingLifeCount", 100),
            at([0.0; 3]),
        ],
        true,
    );
    let hq = b.spawner("Team1", &[[0.0, 0.0, -300.0]], 20.0);
    b.capture_point("US HQ", "Team1", true, 0, [0.0, 0.0, -300.0], &[hq], 20.0, 0, None);
    let mut bases: Vec<String> = Vec::new();
    let mut areas: Vec<String> = Vec::new();
    for k in 0..3 {
        let z = 150.0 * k as f32;
        let first = k == 0;
        let base = b.add(
            "MultipleTriggerEntityData",
            vec![
                text("Name", &format!("Base{}", k + 1)),
                boolean("Enabled", first),
                int("TriggerAmount", 2),
            ],
            true,
        );
        let s = b.spawner("Team2", &[[0.0, 0.0, z + 40.0]], 20.0);
        let cp = b.capture_point(
            &format!("Base {}", k + 1),
            "Team2",
            true,
            0,
            [0.0, 0.0, z + 40.0],
            std::slice::from_ref(&s),
            20.0,
            0,
            Some(first),
        );
        let area = b.area("Team2", false, first, &rect(-100.0, z - 100.0, 100.0, z + 100.0));
        for x in [-20.0f32, 20.0] {
            let arm = b.add(
                "GameInteractionEntityData",
                vec![text("Name", "Arm"), boolean("Enabled", true), at([x, 0.0, z])],
                false,
            );
            let m = b.add(
                "ReferenceObjectData",
                vec![
                    crate::export_rules::rf(
                        "ReferencedObject",
                        "objects/objects/mpobjective/mpobjective/4356a5fc-9d73-4dd1-b75d-86ef74313a7f",
                    ),
                    at([x, 0.0, z]),
                ],
                true,
            );
            b.event(&m, "onDestroyed", &arm, "Disable");
            b.event(&m, "onDestroyed", &base, "Trigger");
        }
        b.event(&base, "OnTrigger", &defenders, "DestructBase");
        b.event(&base, "OnTrigger", &cp, "Disable");
        b.event(&base, "OnTrigger", &area, "Disable");
        b.event(&cp, "OnEnabled", &s, "Enable");
        if !first {
            b.event(&base, "OnEnabled", &cp, "Enable");
        }
        bases.push(base);
        areas.push(area);
    }
    for k in 0..2 {
        b.event(&bases[k], "OnTrigger", &bases[k + 1], "Enable");
        b.event(&bases[k], "OnTrigger", &areas[k + 1], "Enable");
    }
    b.bytes()
}

/// The stage tools on a synthetic Rush level (CI, no game data): the three stages as DICE wires them, a fourth added
/// after the last and played last with its base and combat area in play as the third's were, taken out again; each
/// stage taken out; a project with a stage added needs this editor's version (the `connect` operation).
// spec: GAME-RUSH-11
// spec: XLOG-GAME-3
#[test]
fn stages_added_and_taken_out_on_a_synthetic_level() {
    let _gate = crate::gate();
    let base = synthetic();
    let w0 = LevelWriter::parse(&base).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let st = rushstage::stages(&w0).unwrap();
    assert_eq!(st.stages.len(), 3);
    for (k, s) in st.stages.iter().enumerate() {
        assert_eq!(s.mcoms.len(), 2);
        // its M-COMs, arms, counter, base with its spawner and point, combat area with its four corners
        assert_eq!(
            s.set.len(),
            2 + 2 + 1 + 3 + 5,
            "stage {}: {:?}",
            k + 1,
            modelayer::by_type(&w0, &s.set)
        );
    }
    let (copies, wires, kept, differ) = check_level("synthetic", &w0, &schema);
    assert_eq!(differ, 0);
    println!("synthetic: {copies} copies, {wires} wires, {kept} connections kept on the last counter");
    // the log: one operation, the project's version, the partition read back
    let mut p = MapProject::new("Rush stages", "v1test", "levels/V1TEST", &base, Profile::Bfbc2);
    let n = p.next_n();
    let s = rushstage::add_stage(
        &w0,
        &schema,
        [0.0, 0.0, 450.0],
        &|g| bc2_levelwrite::new_guid("Rush stages", n, g),
        &|_, _| None,
    )
    .unwrap();
    p.push(s.op, "Add Rush stage 4");
    assert_eq!(p.reader_needed(), VERSION, "a stage added needs this editor's version");
    assert_eq!(VERSION, 10);
    let w = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
    assert_eq!(modelayer::rush_play(&w).0.len(), 4);
    let m = w.transform(&s.mcoms[0]).unwrap()[3];
    assert_eq!(
        m,
        [-20.0, 0.0, 450.0],
        "the copies moved with the M-COMs' middle to the click"
    );
    p.push(rushstage::remove_stage(&w, &schema, 1).unwrap(), "Remove Rush stage 2");
    let v = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
    assert_eq!(modelayer::rush_play(&v).0.len(), 3);
    // refusals: a level without stages, the only stage
    let cq = LevelWriter::parse(&crate::gameplay_tools::synthetic()).unwrap();
    let e = rushstage::add_stage(&cq, &schema, [0.0; 3], &|g| g.to_owned(), &|_, _| None).unwrap_err();
    assert!(e.to_string().contains("no Rush stages"), "{e}");
    let mut one = w0.clone();
    for k in [2usize, 1] {
        one = applied(&one, &schema, &rushstage::remove_stage(&one, &schema, k).unwrap());
    }
    let e = rushstage::remove_stage(&one, &schema, 0).unwrap_err();
    assert!(e.to_string().contains("keeps one stage"), "{e}");
    let e = rushstage::add_stage(&one, &schema, [0.0; 3], &|g| g.to_owned(), &|_, _| None).unwrap_err();
    assert!(e.to_string().contains("one stage"), "{e}");
}

/// The Connect operation (CI): an event connection added as a copy of one with the same event names, refused twice,
/// taken out again; the partition as it was.
// spec: XLOG-GAME-3
#[test]
fn connections_added_and_taken_out() {
    let _gate = crate::gate();
    let base = synthetic();
    let w0 = LevelWriter::parse(&base).unwrap();
    let st = rushstage::stages(&w0).unwrap();
    let c = Connection {
        source: st.stages[2].base.clone(),
        source_event: "OnTrigger".into(),
        target: st.stages[0].base.clone(),
        target_event: "Disable".into(),
    };
    let mut w = w0.clone();
    w.connect(std::slice::from_ref(&c), true).unwrap();
    assert!(rushstage::connections(&w).contains(&c));
    let e = w.connect(std::slice::from_ref(&c), true).unwrap_err();
    assert!(e.to_string().contains("there already"), "{e}");
    let odd = Connection {
        target_event: "NoSuchEvent".into(),
        ..c.clone()
    };
    w.connect(std::slice::from_ref(&odd), true).unwrap();
    assert!(rushstage::connections(&w).contains(&odd), "a new event name is set");
    w.connect(&[c.clone(), odd], false).unwrap();
    assert_eq!(rushstage::connections(&w), rushstage::connections(&w0));
    assert_eq!(w.guids(), w0.guids());
    let e = w.connect(&[c], false).unwrap_err();
    assert!(e.to_string().contains("no connection"), "{e}");
}

// ------------------------------------------------------------------------------------------------ real levels

/// Port Valdez Rush (MP_012GR): a fifth stage added after the fourth (its M-COMs, bases, spawns, combat areas and
/// counters copied 80 m off), played last with its copies as the fourth stage's objects were; taken out again; each
/// stage taken out; Squad Rush (MP_012SR) likewise. Through the project's log, the partition read back.
// spec: GAME-RUSH-11
// spec: XLOG-GAME-3
#[test]
fn a_stage_added_and_taken_out_on_port_valdez() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    for lv in ["mp_012gr", "mp_012sr"] {
        let base = export::level_partition(&game, lv).unwrap();
        let w0 = LevelWriter::parse(&base).unwrap();
        let schema = Schema::scan(Some(&w0.d));
        let (copies, wires, kept, differ) = check_level(lv, &w0, &schema);
        assert_eq!(differ, 0, "{lv}");
        println!("{lv}: {copies} copies, {wires} wires, {kept} connections kept on the last counter");
        let mut p = MapProject::new(
            "Rush stages",
            lv,
            &format!("levels/{}", lv.to_ascii_uppercase()),
            &base,
            Profile::Bfbc2,
        );
        let n = p.next_n();
        let t = w0
            .transform(&modelayer::rush_play(&w0).0.last().unwrap().mcoms[0])
            .unwrap()[3];
        let s = rushstage::add_stage(
            &w0,
            &schema,
            [t[0] + 80.0, t[1], t[2]],
            &|g| bc2_levelwrite::new_guid("Rush stages", n, g),
            &|_, _| None,
        )
        .unwrap();
        p.push(s.op, "Add Rush stage");
        let w = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
        let k = modelayer::rush_play(&w0).0.len();
        assert_eq!(modelayer::rush_play(&w).0.len(), k + 1);
        p.push(rushstage::remove_stage(&w, &schema, k).unwrap(), "Remove Rush stage");
        let back = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
        assert_eq!(back.guids(), w0.guids());
    }
}

/// Game tier: DICE's Rush stage wiring (GAME-RUSH-11) on every stock Rush and Squad Rush level of each set - one base
/// counter per stage taking a base from the defenders, as many as their `BaseCount`, chained stage to stage - and the
/// tools on each: a stage added and taken out again, every stage taken out once ([`check_level`]). The Onslaught
/// install's `ons_mp_005gr`, wired otherwise, is refused in words.
// spec: GAME-RUSH-11
#[test]
#[ignore = "game tier: every stock Rush and Squad Rush level; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn every_rush_level_is_wired_stage_by_stage() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let modes = bc2_levelwrite::profile::modes(&game).unwrap();
    let (mut levels, mut stages, mut refused, mut differs) = (0, 0, Vec::new(), 0);
    for by_mode in modes.values() {
        for (mode, lvs) in by_mode {
            if !["rush", "goldrush", "squadrush"].contains(&mode.as_str()) {
                continue;
            }
            for lv in lvs {
                let w = LevelWriter::parse(&export::level_partition(&game, lv).unwrap()).unwrap();
                let schema = Schema::scan(Some(&w.d));
                let st = match rushstage::stages(&w) {
                    Ok(s) => s,
                    Err(e) => {
                        println!("{lv}: refused: {e}");
                        refused.push(lv.clone());
                        continue;
                    }
                };
                let n = st.stages.len();
                assert_eq!(base_count(&w, &schema), n as i64, "{lv}: BaseCount counts the stages");
                for (k, s) in st.stages.iter().enumerate() {
                    let types = modelayer::by_type(&w, &s.own);
                    if k > 0 {
                        for t in ["CapturePointEntityData", "CombatAreaTriggerEntityData"] {
                            assert!(types.contains_key(t), "{lv} stage {}: its own {t}: {types:?}", k + 1);
                        }
                    }
                }
                let (copies, wires, kept, differ) = check_level(lv, &w, &schema);
                println!(
                    "{lv}: {n} stages; a stage added: {copies} copies, {wires} wires; {kept} kept; {differ} differ"
                );
                differs += differ;
                levels += 1;
                stages += n;
            }
        }
    }
    println!("{levels} levels, {stages} stages; refused {refused:?}; {differs} states differ after a stage taken out");
    assert_eq!(
        refused,
        ["ons_mp_005gr"],
        "only the Onslaught install's level is wired otherwise"
    );
    assert!(levels >= 30, "{levels}");
}
