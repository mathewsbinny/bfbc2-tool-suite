//! The map editor's gameplay tools on real levels (`bc2_levelwrite::gameplay`, `EDITOR_ARCHITECTURE.md` 3.4): a flag
//! added with its unit and deleted again on Valparaiso (`MP_001`), a spawn point added to a spawner, an M-COM added
//! to and taken from a Rush stage, the round's rules; every operation from the project's log, the partition read
//! back after it is written (a DBX round trip). The game-tier sweeps check the rules the tools rest on over every
//! stock level of a mode: each flag's unit is its own (GAME-CQ-11) and each Rush stage counter counts its stage's
//! M-COMs (GAME-RUSH-10).
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); skipped without it. Writes nothing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::doc_markdown,
    clippy::float_cmp,
    clippy::panic,
    missing_docs
)]

use std::collections::HashSet;
use std::path::PathBuf;

use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::gameplay::{self, Rule, Rules, Team};
use bc2_levelwrite::placement::Mode;
use bc2_levelwrite::project::VERSION;
use bc2_levelwrite::write::LevelWriter;
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

/// The event connections of a partition: (source, source event, target, target event), upper-case guids.
fn connections(w: &LevelWriter) -> Vec<(String, String, String, String)> {
    let mut out = Vec::new();
    for a in w.connection_arrays() {
        for &c in w.d.children(a).unwrap_or_default() {
            let end = |f: &[u8]| {
                let r = w.d.child(c, f).and_then(|x| w.d.attr(x, b"ref")).unwrap_or_default();
                String::from_utf8_lossy(r.rsplit(|&b| b == b'/').next().unwrap_or(r)).to_ascii_uppercase()
            };
            let ev = |f: &[u8]| {
                w.d.child(c, f)
                    .and_then(|x| w.d.child(x, b"Name"))
                    .and_then(|x| w.d.text(x))
                    .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned())
            };
            out.push((end(b"Source"), ev(b"SourceEvent"), end(b"Target"), ev(b"TargetEvent")));
        }
    }
    out
}

fn text(w: &LevelWriter, guid: &str, field: &[u8]) -> String {
    let e = w.find(guid).unwrap();
    w.d.child(e, field)
        .and_then(|x| w.d.text(x))
        .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned())
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

/// A flag added on Valparaiso (MP_001, Conquest) as the tool adds it - a copy of the flag with the smallest unit,
/// its unit carried 60 m - wired as its template: every event connection of the template's unit has its copy, with
/// the unit's own objects mapped to their copies and the shared ones kept; lettered E (the minimap's `Sid`), owned
/// by the chosen team. Deleted again with its unit: the level has its instances and connections back.
// spec: GAME-CQ-11
#[test]
fn a_flag_added_wired_and_deleted_on_valparaiso() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "mp_001").unwrap();
    let w0 = LevelWriter::parse(&base).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let template = gameplay::flag_template(&w0).unwrap();
    let members = gameplay::unit(&w0, &template);
    println!(
        "template {} ({}): unit {} {:?}",
        template,
        text(&w0, &template, b"Name"),
        members.len(),
        modelayer::by_type(&w0, &members)
    );
    println!("letters {:?}", gameplay::letters(&w0));
    assert_eq!(gameplay::next_letter(&w0), 'E', "after Valparaiso's A - D");
    let by = modelayer::by_type(&w0, &members);
    for t in [
        "MapMarkerEntityData",
        "SoldierSpawnEntityData",
        "AlternateSpawnEntityData",
        "TeamFilterEntityData",
        "MessageEntityData",
    ] {
        assert!(by.contains_key(t), "the flag's unit holds its {t}: {by:?}");
    }

    let mut p = MapProject::new("Gameplay tools", "mp_001", "levels/MP_001", &base, Profile::Bfbc2);
    let mut at = w0.transform(&template).unwrap()[3];
    at[0] += 60.0;
    let (op, flag) = {
        let n = p.next_n();
        let name = p.name.clone();
        gameplay::add_flag(&w0, at, Team::Team2, &|s| bc2_levelwrite::new_guid(&name, n, s)).unwrap()
    };
    p.push(op, "Add flag E");
    assert_eq!(p.reader_needed(), 9, "a flag added needs version 9 (round E4)");
    let applied = bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap();
    let w = round_trip(&applied.bytes);
    assert!(gameplay::flags(&w).contains(&flag));
    assert_eq!(gameplay::team_of(&w, &flag), Some(Team::Team2));
    assert_eq!(text(&w, &flag, b"Name"), "E: Flag E");
    assert!(
        gameplay::letters(&w).contains(&"E".to_owned()),
        "{:?}",
        gameplay::letters(&w)
    );
    let copy_unit = gameplay::unit(&w, &flag);
    assert_eq!(
        copy_unit.len(),
        members.len(),
        "the copy's unit is as large as its template's"
    );
    // the wiring mirrors the template's: map the template's unit to the copy's (in order) and compare
    let map: std::collections::HashMap<String, String> =
        members.iter().cloned().zip(copy_unit.iter().cloned()).collect();
    let all = connections(&w);
    let set: HashSet<_> = all.iter().cloned().collect();
    let theirs: Vec<_> = connections(&w0)
        .into_iter()
        .filter(|(s, _, t, _)| map.contains_key(s) || map.contains_key(t))
        .collect();
    for (s, se, t, te) in &theirs {
        let m = |g: &String| map.get(g).cloned().unwrap_or_else(|| g.clone());
        assert!(
            set.contains(&(m(s), se.clone(), m(t), te.clone())),
            "{s} {se} -> {t} {te} has its copy"
        );
    }
    let mine = all
        .iter()
        .filter(|(s, _, t, _)| copy_unit.contains(s) || copy_unit.contains(t))
        .count();
    assert_eq!(mine, theirs.len(), "no connection more than the template's");
    // the members carried: the zone keeps its shape around the new flag
    let d = |a: [f32; 3], b: [f32; 3]| ((a[0] - b[0]).powi(2) + (a[2] - b[2]).powi(2)).sqrt();
    for (old, new) in members.iter().zip(&copy_unit) {
        if let (Some(o), Some(n)) = (w0.transform(old), w.transform(new)) {
            let (ro, rn) = (w0.transform(&template).unwrap()[3], w.transform(&flag).unwrap()[3]);
            assert!(
                (d(o[3], ro) - d(n[3], rn)).abs() < 0.01,
                "{old} keeps its distance to the flag"
            );
        }
    }
    println!(
        "added: {} instances, {} connections ({} of them the copy's)",
        applied.placed + applied.duplicated,
        all.len() - connections(&w0).len(),
        mine
    );

    // deleted with its unit (one operation): the level as it was, instance for instance and connection for connection
    let del = gameplay::delete_unit(&w, &schema, &flag).unwrap();
    let Op::Delete { guids } = &del else { panic!("{del:?}") };
    assert_eq!(guids.len(), copy_unit.len(), "the whole unit is the copy's own");
    p.push(del, "");
    let applied = bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap();
    let back = round_trip(&applied.bytes);
    assert_eq!(back.guids(), w0.guids());
    assert_eq!(connections(&back), connections(&w0));

    // refusals: the last base of a team, the last flag
    let us = gameplay::bases(&w0, Team::Team1);
    assert_eq!(us.len(), 1);
    let e = gameplay::delete_unit(&w0, &schema, &us[0]).unwrap_err();
    assert!(e.to_string().contains("last base"), "{e}");
    let mut solo = w0.clone();
    for f in gameplay::flags(&w0).iter().skip(1) {
        let Op::Delete { guids } = gameplay::delete_unit(&solo, &schema, f).unwrap() else {
            panic!()
        };
        for g in guids {
            solo.delete(&g).unwrap();
        }
    }
    let last = gameplay::flags(&solo);
    assert_eq!(last.len(), 1);
    let e = gameplay::delete_unit(&solo, &schema, &last[0]).unwrap_err();
    assert!(e.to_string().contains("last flag"), "{e}");
}

/// A spawn point added to a flag's spawner (one operation: the point placed and linked into the spawner's
/// `AlternativeSpawnPoints`), then deleted: the spawner lists it, then not.
// spec: GAME-SPAWN-1
#[test]
fn a_spawn_point_added_and_deleted() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "mp_001").unwrap();
    let w0 = LevelWriter::parse(&base).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let flag = gameplay::flags(&w0)[0].clone();
    let spawner = gameplay::spawners(&w0, &flag)
        .into_iter()
        .find(|s| !gameplay::points(&w0, s).is_empty())
        .unwrap();
    let before = gameplay::points(&w0, &spawner);
    let mut at = w0.transform(&before[0]).unwrap();
    at[3][2] += 4.0;
    let pos = at[3];
    let mut p = MapProject::new("Gameplay tools", "mp_001", "levels/MP_001", &base, Profile::Bfbc2);
    let n = p.next_n();
    let (op, point) = gameplay::add_spawn_point(&w0, &spawner, pos, &|s| {
        bc2_levelwrite::new_guid("Gameplay tools", n, s)
    })
    .unwrap();
    println!("{}", op.describe());
    p.push(op, "Add spawn point");
    let w = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
    let after = gameplay::points(&w, &spawner);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after.last(), Some(&point));
    assert_eq!(w.transform(&point), Some(at));
    // linking it twice is refused; unlinking leaves it placed
    let mut v = w.clone();
    let e = gameplay::link_point(&mut v, &spawner, &point, true).unwrap_err();
    assert!(e.to_string().contains("already"), "{e}");
    gameplay::link_point(&mut v, &spawner, &point, false).unwrap();
    assert_eq!(gameplay::points(&v, &spawner), before);
    let e = gameplay::link_point(&mut v, &point, &spawner, true).unwrap_err();
    assert!(e.to_string().contains("not a soldier spawner"), "{e}");
    // deleted: the spawner's list as it was
    let del = gameplay::delete_unit(&w, &schema, &point).unwrap();
    assert_eq!(
        del,
        Op::Delete {
            guids: vec![point.clone()]
        }
    );
    p.push(del, "");
    let back = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
    assert_eq!(gameplay::points(&back, &spawner), before);
}

/// An M-COM added to a Rush stage (MP_012GR) and taken out again: the stage has three, every counter of the stage
/// counts three (GAME-RUSH-10), the stages are played in the same order; deleted, the level has its two again.
// spec: GAME-RUSH-10
#[test]
fn an_mcom_added_to_a_rush_stage_and_taken_out() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "mp_012gr").unwrap();
    let w0 = LevelWriter::parse(&base).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let stages = modelayer::stages(&w0);
    let (play, uncertain) = modelayer::rush_play(&w0);
    assert!(!uncertain);
    let first = play[0].mcoms.clone();
    assert_eq!(first.len(), 2);
    let counters = gameplay::stage_counters(&w0, &first);
    assert_ne!(counters.len(), 0, "the stage has counters");
    for (c, by) in &counters {
        assert_eq!(gameplay::trigger_amount(&w0, &schema, c), Some(by.len() as i64));
    }
    let mut at = w0.transform(&first[0]).unwrap()[3];
    at[0] += 15.0;
    let mut p = MapProject::new("Gameplay tools", "mp_012gr", "levels/MP_012GR", &base, Profile::Bfbc2);
    let n = p.next_n();
    let (op, mcom) = gameplay::add_mcom(&w0, &schema, &first, at, &|s| {
        bc2_levelwrite::new_guid("Gameplay tools", n, s)
    })
    .unwrap();
    p.push(op, "Add M-COM");
    let w = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
    let stage = gameplay::stage_of(&w, &mcom).unwrap();
    assert_eq!(stage.len(), 3, "{stage:?}");
    for (c, by) in gameplay::stage_counters(&w, &stage) {
        assert_eq!(by.len(), 3, "{c} is reached by the new M-COM too");
        assert_eq!(gameplay::trigger_amount(&w, &schema, &c), Some(3));
    }
    let (play2, uncertain2) = modelayer::rush_play(&w);
    assert!(!uncertain2);
    assert_eq!(play2.len(), play.len());
    assert!(play2[0].mcoms.contains(&mcom), "the new M-COM is played with its stage");
    assert_eq!(modelayer::stages(&w).len(), stages.len());
    let unit = gameplay::unit(&w, &mcom);
    println!("the new M-COM's unit: {:?}", modelayer::by_type(&w, &unit));
    assert!(modelayer::by_type(&w, &unit).contains_key("GameInteractionEntityData"));

    // taken out: the stage's two, its counters counting two again, the instances as the level had them
    p.push(gameplay::delete_unit(&w, &schema, &mcom).unwrap(), "");
    let back = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
    assert_eq!(back.guids(), w0.guids());
    for (c, by) in gameplay::stage_counters(&back, &first) {
        assert_eq!(gameplay::trigger_amount(&back, &schema, &c), Some(by.len() as i64));
    }
    // the last M-COM of a Squad Rush stage stays
    let sr = LevelWriter::parse(&export::level_partition(&game, "mp_012sr").unwrap()).unwrap();
    let sr_schema = Schema::scan(Some(&sr.d));
    let one = modelayer::stages(&sr)[0][0].clone();
    let e = gameplay::delete_unit(&sr, &sr_schema, &one).unwrap_err();
    assert!(e.to_string().contains("last M-COM"), "{e}");
}

/// The round's rules: DICE's on Valparaiso (Conquest) and Port Valdez (Rush) are the stock rules of their mode
/// (GAME-RULES-1); other values set on every object holding them and read back; a rule the mode has no object for is
/// refused in words.
// spec: GAME-RULES-1
#[test]
fn round_rules_stock_set_and_refused() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "mp_001").unwrap();
    let w0 = LevelWriter::parse(&base).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let stock = gameplay::stock(Mode::Conquest, Profile::Bfbc2);
    assert_eq!(gameplay::rules_of(&w0, &schema), stock);
    let rush = LevelWriter::parse(&export::level_partition(&game, "mp_012gr").unwrap()).unwrap();
    let rush_schema = Schema::scan(Some(&rush.d));
    assert_eq!(
        gameplay::rules_of(&rush, &rush_schema),
        gameplay::stock(Mode::Rush, Profile::Bfbc2)
    );
    // the choices hold DICE's value as the first of each rule's
    for c in gameplay::choices(Mode::Conquest, Profile::Bfbc2) {
        assert_eq!(stock.get(c.rule), c.dice.first().copied(), "{}", c.name);
    }
    let mut to = Rules::default();
    to.set(Rule::Tickets, Some(350.0));
    to.set(Rule::CaptureS, Some(25.0));
    let mut p = MapProject::new("Gameplay tools", "mp_001", "levels/MP_001", &base, Profile::Bfbc2);
    p.push(Op::SetRules { from: Some(stock), to }, "");
    assert_eq!(p.operations[0].name(), "Round rules: 350 tickets, a flag taken in 25 s");
    let applied = bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap();
    assert_eq!(applied.fields, 2 + 2 * gameplay::flags(&w0).len());
    let w = round_trip(&applied.bytes);
    let now = gameplay::rules_of(&w, &schema);
    assert_eq!(
        (now.tickets, now.loss_per_min, now.capture_s),
        (Some(350), Some(10.0), Some(25.0))
    );
    let mut kills = Rules::default();
    kills.set(Rule::Kills, Some(40.0));
    let mut v = w0.clone();
    let e = gameplay::set_rules(&mut v, &schema, &kills).unwrap_err();
    assert!(e.to_string().contains("no kill counter"), "{e}");
    let mut neg = Rules::default();
    neg.set(Rule::Tickets, Some(-1.0));
    let e = gameplay::set_rules(&mut v, &schema, &neg).unwrap_err();
    assert!(e.to_string().contains("0 or more"), "{e}");
}

/// Game tier: every flag of every stock Conquest level (each set) has a unit of its own - its zone, marker, spawners
/// and announcement logic - that no other capture point's unit shares (GAME-CQ-11).
// spec: GAME-CQ-11
#[test]
#[ignore = "game tier: every stock Conquest level; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn every_conquest_flag_has_a_unit_of_its_own() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let modes = bc2_levelwrite::profile::modes(&game).unwrap();
    let (mut levels, mut flags, mut filters) = (0, 0, 0);
    for (set, by_mode) in &modes {
        for lv in by_mode.get("conquest").into_iter().flatten() {
            let w = LevelWriter::parse(&export::level_partition(&game, lv).unwrap()).unwrap();
            let cps: Vec<String> =
                w.d.of_type(b".CapturePointEntityData")
                    .into_iter()
                    .map(|e| String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap()).to_ascii_uppercase())
                    .collect();
            let units: Vec<Vec<String>> = cps.iter().map(|c| gameplay::unit(&w, c)).collect();
            for (k, f) in cps.iter().enumerate() {
                if !gameplay::flags(&w).contains(f) {
                    continue;
                }
                flags += 1;
                let u: HashSet<&String> = units[k].iter().collect();
                for (j, other) in units.iter().enumerate() {
                    if j != k {
                        assert!(
                            other.iter().all(|g| !u.contains(g)),
                            "{set:?} {lv}: {f}'s unit shares with {}",
                            cps[j]
                        );
                    }
                }
                let by = modelayer::by_type(&w, &units[k]);
                assert!(by.contains_key("MapMarkerEntityData"), "{lv} {f}: {by:?}");
                filters += by.get("TeamFilterEntityData").copied().unwrap_or(0);
            }
            levels += 1;
        }
    }
    println!("{levels} Conquest levels, {flags} flags, {filters} team filters in their units");
    assert!(levels >= 16, "{levels}");
}

/// Game tier: every counter a Rush or Squad Rush stage's M-COMs reach counts them - its `TriggerAmount` is the number
/// of the stage's M-COMs whose `onDestroyed` reaches it - on every stock level of each set (GAME-RUSH-10).
// spec: GAME-RUSH-10
#[test]
#[ignore = "game tier: every stock Rush and Squad Rush level; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn every_rush_stage_counter_counts_its_mcoms() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let modes = bc2_levelwrite::profile::modes(&game).unwrap();
    let (mut levels, mut counters) = (0, 0);
    for by_mode in modes.values() {
        for (mode, lvs) in by_mode {
            if !["rush", "goldrush", "squadrush"].contains(&mode.as_str()) {
                continue;
            }
            for lv in lvs {
                let w = LevelWriter::parse(&export::level_partition(&game, lv).unwrap()).unwrap();
                let schema = Schema::scan(Some(&w.d));
                for stage in modelayer::stages(&w) {
                    for (c, by) in gameplay::stage_counters(&w, &stage) {
                        assert_eq!(
                            gameplay::trigger_amount(&w, &schema, &c),
                            Some(by.len() as i64),
                            "{lv}: counter {c}"
                        );
                        counters += 1;
                    }
                }
                levels += 1;
            }
        }
    }
    println!("{levels} Rush and Squad Rush levels, {counters} stage counters");
    assert!(levels >= 30, "{levels}");
}

/// A small synthetic Conquest level (no game data): two bases, two flags each with a map marker (its letter), a
/// spawner of three points, two team filters its `OnCaptured` reaches and their messages, a voice-over line both
/// flags' filters start (shared), two ticket counters.
pub(crate) fn synthetic() -> Vec<u8> {
    use crate::export_rules::{Lb, at, boolean, float, int, refs, rf, text};
    let mut b = Lb::new();
    let vo = b.add(
        "VoiceOverEntityData",
        vec![text("Name", "flag_captured"), at([0.0; 3])],
        true,
    );
    for (team, z) in [("Team1", -200.0), ("Team2", 200.0)] {
        let s = b.spawner(team, &[[0.0, 0.0, z], [5.0, 0.0, z]], 20.0);
        b.capture_point(&format!("HQ {team}"), team, true, 0, [0.0, 0.0, z], &[s], 20.0, 0, None);
    }
    for (k, (letter, x)) in [("A", -60.0f32), ("B", 60.0)].into_iter().enumerate() {
        let marker = b.add(
            "MapMarkerEntityData",
            vec![text("Name", letter), at([x, 2.0, 0.0]), text("Sid", letter)],
            true,
        );
        let s = b.spawner(
            "TeamNeutral",
            &[[x + 10.0, 0.0, 0.0], [x - 10.0, 0.0, 0.0], [x, 0.0, 10.0]],
            20.0,
        );
        let cp = b.add(
            "CapturePointEntityData",
            vec![
                text("Name", letter),
                text("LocationName", letter),
                text("ControlPointName", letter),
                rf("MapMarker", &marker),
                text("InitialOwnerTeam", "TeamNeutral"),
                boolean("UnableToChangeTeam", false),
                int("Index", i32::try_from(k).unwrap() + 1),
                int("TimeToGetControl", 20),
                int("TimeToLoseControl", 20),
                float("CaptureRadius", 15.0),
                refs("Spawners", &[s]),
                at([x, 0.0, 0.0]),
            ],
            true,
        );
        for team in ["Team1", "Team2"] {
            let f = b.add(
                "TeamFilterEntityData",
                vec![text("AllowTeam", team), at([x, 4.0, 0.0])],
                true,
            );
            let m = b.add(
                "MessageEntityData",
                vec![
                    text("MessageSid", &format!("HUD_MP_CQ_TOOK_BASE_{letter}")),
                    text("Name", &format!("{team} - Taking base {letter}")),
                    at([x, 5.0, 0.0]),
                ],
                true,
            );
            b.event(&cp, "OnCaptured", &f, "Impulse");
            b.event(&f, "OnImpulse", &m, "SendMessage");
            b.event(&f, "OnImpulse", &vo, "Start");
        }
        b.event(&cp, "OnFlagInTop", &marker, "StopBlinkMarker");
    }
    for t in ["Team1", "Team2"] {
        b.add(
            "TicketCounterEntityData",
            vec![
                text("TeamId", t),
                int("InitialTicketCount", 200),
                float("TicketLossPerMin", 10.0),
                at([0.0; 3]),
            ],
            true,
        );
    }
    b.bytes()
}

/// The gameplay tools on a synthetic level (CI, no game data): a flag added is the template's unit copied (its
/// marker lettered C, its own filters and messages) and wired as the template - its filters start the shared
/// voice-over line too, which is not copied; deleted, the level's instances and connections are back; the rules
/// read as DICE's and set; a project with the tools' operations needs this editor's version (a version-8 file loads
/// as it is).
// spec: GAME-CQ-11
// spec: GAME-RULES-1
#[test]
fn the_tools_on_a_synthetic_level() {
    let _gate = crate::gate();
    let base = synthetic();
    let w0 = LevelWriter::parse(&base).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let flags = gameplay::flags(&w0);
    assert_eq!(flags.len(), 2);
    assert_eq!(gameplay::next_letter(&w0), 'C');
    let template = gameplay::flag_template(&w0).unwrap();
    let members = gameplay::unit(&w0, &template);
    let by = modelayer::by_type(&w0, &members);
    // the flag, its marker, its spawner and three points, two filters, two messages; not the shared voice-over
    assert_eq!(members.len(), 10, "{by:?}");
    assert!(!by.contains_key("VoiceOverEntityData"), "{by:?}");
    let mut p = MapProject::new("Synthetic", "v1test", "levels/V1TEST", &base, Profile::Bfbc2);
    let n = p.next_n();
    let (op, flag) = gameplay::add_flag(&w0, [0.0, 0.0, 80.0], Team::TeamNeutral, &|s| {
        bc2_levelwrite::new_guid("Synthetic", n, s)
    })
    .unwrap();
    p.push(op, "Add flag C");
    let applied = bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap();
    let w = round_trip(&applied.bytes);
    assert_eq!(gameplay::flags(&w).len(), 3);
    assert_eq!(gameplay::letters(&w), ["A", "B", "C"]);
    assert_eq!(text(&w, &flag, b"ControlPointName"), "C: Flag C");
    let copy = gameplay::unit(&w, &flag);
    assert_eq!(copy.len(), members.len());
    let vo = w
        .guids()
        .into_iter()
        .find(|g| w.type_of(w.find(g).unwrap()).ends_with("VoiceOverEntityData"))
        .unwrap();
    let starts = connections(&w).into_iter().filter(|c| c.2 == vo).count();
    assert_eq!(
        starts, 6,
        "both old flags' two filters and the new flag's two start the shared line"
    );
    assert_eq!(w.transform(&flag).unwrap()[3], [0.0, 0.0, 80.0]);
    // the spawn points carried with the flag
    let s = gameplay::spawners(&w, &flag);
    let pts = gameplay::points(&w, &s[0]);
    let xs: Vec<[f32; 3]> = pts.iter().map(|g| w.transform(g).unwrap()[3]).collect();
    let (old_root, old_pts) = (
        w0.transform(&template).unwrap()[3],
        gameplay::points(&w0, &gameplay::spawners(&w0, &template)[0]),
    );
    for (g, x) in old_pts.iter().zip(&xs) {
        let o = w0.transform(g).unwrap()[3];
        for c in 0..3 {
            assert!(
                (o[c] - old_root[c] - x[c] + [0.0, 0.0, 80.0][c]).abs() < 1e-4,
                "{o:?} {x:?}"
            );
        }
    }
    // deleted: the base's instances and connections
    p.push(gameplay::delete_unit(&w, &schema, &flag).unwrap(), "");
    let back = round_trip(&bc2_levelwrite::apply(&base, &p.operations, &schema).unwrap().bytes);
    assert_eq!(back.guids(), w0.guids());
    assert_eq!(connections(&back), connections(&w0));
    // rules: DICE's Conquest, set to 250 / 5 / 30 s on both counters and both flags
    assert_eq!(
        gameplay::rules_of(&w0, &schema),
        gameplay::stock(Mode::Conquest, Profile::Bfbc2)
    );
    let to = Rules {
        tickets: Some(250),
        loss_per_min: Some(5.0),
        capture_s: Some(30.0),
        ..Rules::default()
    };
    let mut v = w0.clone();
    assert_eq!(gameplay::set_rules(&mut v, &schema, &to).unwrap(), 2 + 2 + 2 * 2);
    assert_eq!(gameplay::rules_of(&v, &schema), to);
    // the project's version: a version-8 file loads as it is; the tools' operations need version 9 (round E4)
    assert_eq!(p.reader_needed(), 9);
    let mut raw: serde_json::Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
    let ops = raw["operations"].clone();
    raw["operations"] = serde_json::Value::Array(Vec::new());
    raw["version"] = serde_json::Value::from(8);
    raw["min_reader"] = serde_json::Value::from(1);
    let old = MapProject::from_json(&raw.to_string()).unwrap();
    assert_eq!(old.version, VERSION);
    raw["operations"] = ops;
    let again = MapProject::from_json(&raw.to_string()).unwrap();
    assert_eq!(again.operations, p.operations);
}

/// Game tier: the user's first target - Operation Aurora (`SP_01`) as a BFBC2 Conquest level - with the gameplay
/// tools' edits (a flag E added, a spawn point added to its spawner, the rules set to 250 tickets and 25 s) exported
/// by the export's own path (`export::build`, `mapexport build`'s) and held by its stage check (`export::check`); the
/// staged partition reads back byte for byte and holds the new flag wired, its spawn point and the rules. What the
/// game does with them stays unproven until a playtest (in-game tests are paused).
// spec: GAME-CQ-11, GAME-RULES-1
#[test]
#[ignore = "game tier: a whole staged export; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn aurora_conquest_with_the_tools_edits_exports() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "sp_01").unwrap();
    let mut p = MapProject::new("Gameplay tools export", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Bfbc2, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let (cur, _) = export::current(&game, &p).unwrap();
    let schema = Schema::scan(Some(&cur.d));
    let flags0 = gameplay::flags(&cur);
    let template = gameplay::flag_template(&cur).unwrap();
    let mut at = cur.transform(&template).unwrap()[3];
    at[2] += 50.0;
    let n = p.next_n();
    let id = p.id.clone();
    let (op, flag) = gameplay::add_flag(&cur, at, Team::TeamNeutral, &|s| bc2_levelwrite::new_guid(&id, n, s)).unwrap();
    p.push(op, "Add flag");
    let (cur, _) = export::current(&game, &p).unwrap();
    let spawner = gameplay::spawners(&cur, &flag)
        .into_iter()
        .find(|s| cur.type_of(cur.find(s).unwrap()).ends_with("SoldierSpawnEntityData"))
        .unwrap();
    let n = p.next_n();
    let (op, point) = gameplay::add_spawn_point(&cur, &spawner, [at[0] + 6.0, at[1], at[2] + 6.0], &|s| {
        bc2_levelwrite::new_guid(&id, n, s)
    })
    .unwrap();
    p.push(op, "Add spawn point");
    let now = gameplay::rules_of(&cur, &schema);
    let mut to = Rules::default();
    to.set(Rule::Tickets, Some(250.0));
    to.set(Rule::CaptureS, Some(25.0));
    p.push(Op::SetRules { from: Some(now), to }, "");
    let spec = export::ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_GAMEPLAY_AURORA".into(),
        folder: String::new(),
        title: "Aurora gameplay tools".into(),
        mode: "conquest".into(),
        layer: bc2_levelwrite::mpconvert::LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Bfbc2,
        identity: None,
        path_data: false,
    };
    let tmp = std::env::temp_dir().join(format!("bc2_gameplay_tools_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let stage = tmp.join("stage");
    let t = std::time::Instant::now();
    let rec = export::build(&game, &p, "test", &spec, &stage).unwrap();
    println!("built in {:.1} s", t.elapsed().as_secs_f64());
    let report = export::check(&game, &stage).unwrap();
    for (code, ok, what) in report.lines() {
        if !ok {
            println!("FAIL {code} {what}");
        }
    }
    assert!(report.ok(), "the stage check holds the export");
    let name = format!("Levels/{}.dbx", rec.level);
    let level = bc2_gamefs::bundles::read_path(stage.join("level-00.fbrb"), &[&name]).unwrap();
    let w = round_trip(&level[0]);
    let flags = gameplay::flags(&w);
    assert_eq!(flags.len(), flags0.len() + 1, "{flags:?}");
    assert!(flags.contains(&flag));
    assert!(gameplay::points(&w, &spawner).contains(&point));
    let schema = Schema::scan(Some(&w.d));
    let rules = gameplay::rules_of(&w, &schema);
    assert_eq!((rules.tickets, rules.capture_s), (Some(250), Some(25.0)));
    assert_eq!(
        gameplay::unit(&w, &flag).len(),
        gameplay::unit(&w, &template).len() + 1,
        "the new flag's unit: its template's and the new spawn point"
    );
    // its texts (round E4-rest, GAME-CQ-12): the deploy screen's name under the level's own key, in the record the
    // install adds to every language
    let letter = text(&w, &gameplay::marker_of(&w, &flag).unwrap(), b"Sid");
    let key = format!("{}_{letter}", rec.level);
    assert_eq!(text(&w, &spawner, b"LocationNameSid"), key);
    assert!(
        rec.strings.contains(&(key.clone(), format!("{letter}: FLAG {letter}"))),
        "{:?}",
        rec.strings
    );
    println!("flag texts: {:?}", rec.strings);
    println!(
        "staged {}: {} flags, letters {:?}, {}",
        rec.level,
        flags.len(),
        gameplay::letters(&w),
        rules.describe()
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
