//! A new flag's own texts and soldier spawners added to a flag (`bc2_levelwrite::gameplay`, `EDITOR_ARCHITECTURE.md`
//! 3.4): a flag added takes its letter's capture messages (`HUD_MP_CQ_TOOK_BASE_<letter>`) and spawn-name key; the
//! export words the keys the game has no text for (GAME-CQ-12); a flag renamed; a spawner added to a flag with its
//! spawn points, wired as the flag's own, and deleted again. On a synthetic level (CI) and on Valparaiso (`MP_001`,
//! read from OPFOR's game copy beside the suite or `OPFOR_GAME`; skipped without it). Nothing is written.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::panic,
    clippy::doc_markdown,
    missing_docs
)]

use std::collections::HashSet;
use std::path::PathBuf;

use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::gameplay::{self, Team};
use bc2_levelwrite::ops::Operation;
use bc2_levelwrite::write::{Applied, LevelWriter, apply_one};
use bc2_levelwrite::{Op, Schema};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
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
    let bytes = v.write().unwrap();
    let back = LevelWriter::parse(&bytes).unwrap();
    assert_eq!(back.write().unwrap(), bytes, "the partition reads back");
    back
}

fn text(w: &LevelWriter, guid: &str, field: &[u8]) -> String {
    let e = w.find(guid).unwrap();
    w.d.child(e, field)
        .and_then(|x| w.d.text(x))
        .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned())
}

/// The members of a unit of a type.
fn of_type(w: &LevelWriter, unit: &[String], ty: &str) -> Vec<String> {
    unit.iter()
        .filter(|g| w.type_of(w.find(g).unwrap()).ends_with(ty))
        .cloned()
        .collect()
}

/// Flags added one after another (`n` of them), each the tool's flag; returns the level and the flags.
fn add_flags(w0: &LevelWriter, schema: &Schema, n: usize) -> (LevelWriter, Vec<String>) {
    let mut w = w0.clone();
    let mut flags = Vec::new();
    for k in 0..n {
        let mut at = w.transform(&gameplay::flags(&w)[0]).unwrap()[3];
        at[0] += 40.0 * (k as f32 + 1.0);
        let (op, f) = gameplay::add_flag(&w, at, Team::TeamNeutral, &|s| {
            bc2_levelwrite::new_guid("Flag texts", 10 + k as u64, s)
        })
        .unwrap();
        w = applied(&w, schema, &op);
        flags.push(f);
    }
    (w, flags)
}

/// DICE's capture-message keys: A to F.
fn dice_has(k: &str) -> bool {
    ["TOOK", "LOST"]
        .iter()
        .any(|v| ('A'..='F').any(|l| k == format!("HUD_MP_CQ_{v}_BASE_{l}")))
}

/// A flag's texts on a synthetic level (CI): the copy's capture messages name its letter (`..._C`), a fifth letter
/// past DICE's six is worded by the export (G: GOLF), a flag renamed in DICE's form.
// spec: GAME-CQ-12
#[test]
fn a_new_flags_texts_on_a_synthetic_level() {
    let _gate = crate::gate();
    let w0 = LevelWriter::parse(&crate::gameplay_tools::synthetic()).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let (w, flags) = add_flags(&w0, &schema, 5);
    assert_eq!(gameplay::letters(&w), ["A", "B", "C", "D", "E", "F", "G"]);
    let unit = gameplay::unit(&w, &flags[0]);
    let msgs = of_type(&w, &unit, ".MessageEntityData");
    assert_eq!(msgs.len(), 2);
    for m in &msgs {
        assert_eq!(text(&w, m, b"MessageSid"), "HUD_MP_CQ_TOOK_BASE_C");
        assert!(text(&w, m, b"Name").ends_with("Taking base C"), "{}", text(&w, m, b"Name"));
    }
    let mut v = w.clone();
    let (names, keys) = gameplay::flag_texts(&mut v, &schema, "MAPEDIT_TEST", &dice_has).unwrap();
    assert!(names.is_empty(), "the synthetic spawners have no spawn-name key");
    assert_eq!(keys, ["HUD_MP_CQ_TOOK_BASE_G"], "only G is past DICE's");
    assert_eq!(gameplay::nato('G'), Some("GOLF"));
    assert_eq!(gameplay::nato('Z'), Some("ZULU"));
    assert_eq!(gameplay::nato('a'), None);
    // renamed: DICE's form, the marker too; the same name again is refused
    let op = gameplay::rename_flag(&w, &flags[0], "Harbor").unwrap();
    let r = applied(&w, &schema, &op);
    assert_eq!(text(&r, &flags[0], b"Name"), "C: Harbor");
    assert_eq!(text(&r, &flags[0], b"ControlPointName"), "C: Harbor");
    let m = gameplay::marker_of(&r, &flags[0]).unwrap();
    assert_eq!(text(&r, &m, b"Name"), "C: Harbor");
    assert_eq!(text(&r, &m, b"Sid"), "C", "the letter stays");
    let e = gameplay::rename_flag(&r, &flags[0], "C: Harbor").unwrap_err();
    assert!(e.to_string().contains("already"), "{e}");
    let e = gameplay::rename_flag(&r, &flags[0], "  ").unwrap_err();
    assert!(e.to_string().contains("not empty"), "{e}");
}

/// Valparaiso (MP_001): a flag E added takes DICE's `HUD_MP_CQ_TOOK_BASE_E` / `LOST_BASE_E` and the spawn key
/// `MP_001_E`, which the game has no text for: the export names it `<LEVEL>_E` with the flag's name ("E: FLAG E"),
/// DICE's own flags keep theirs. Flags F and G: G's messages are worded from DICE's F in every language that says
/// FOXTROT (English "WE HAVE TAKEN POSITION GOLF", German "WIR HABEN POSITION GOLF ...").
// spec: GAME-CQ-12
#[test]
fn a_new_flags_texts_on_valparaiso() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let w0 = LevelWriter::parse(&export::level_partition(&game, "mp_001").unwrap()).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let langs = bc2_levelwrite::names::Languages::read(&game);
    let has = |k: &str| langs.text_of(k).is_some();
    assert!(has("HUD_MP_CQ_TOOK_BASE_F") && !has("HUD_MP_CQ_TOOK_BASE_G"), "DICE's keys: A to F");
    let (w, flags) = add_flags(&w0, &schema, 3);
    let unit = gameplay::unit(&w, &flags[0]);
    let sids: HashSet<String> = of_type(&w, &unit, ".MessageEntityData")
        .iter()
        .map(|m| text(&w, m, b"MessageSid"))
        .collect();
    assert_eq!(
        sids,
        HashSet::from(["HUD_MP_CQ_TOOK_BASE_E".to_owned(), "HUD_MP_CQ_LOST_BASE_E".to_owned()])
    );
    let spawns = of_type(&w, &unit, ".SoldierSpawnEntityData");
    assert!(!spawns.is_empty());
    for s in &spawns {
        assert_eq!(text(&w, s, b"LocationNameSid"), "MP_001_E");
        assert_eq!(text(&w, s, b"Name"), "E: Flag E");
    }
    let mut v = w.clone();
    let (names, keys) = gameplay::flag_texts(&mut v, &schema, "MAPEDIT_VALPO", &has).unwrap();
    println!("spawn names {names:?}, message keys {keys:?}");
    assert_eq!(
        names,
        [
            ("MAPEDIT_VALPO_E".to_owned(), "E: FLAG E".to_owned()),
            ("MAPEDIT_VALPO_F".to_owned(), "F: FLAG F".to_owned()),
            ("MAPEDIT_VALPO_G".to_owned(), "G: FLAG G".to_owned()),
        ]
    );
    for s in &spawns {
        assert_eq!(text(&v, s, b"LocationNameSid"), "MAPEDIT_VALPO_E");
    }
    for f in gameplay::flags(&w0) {
        for s in gameplay::spawners(&v, &f) {
            if v.type_of(v.find(&s).unwrap()).ends_with(".SoldierSpawnEntityData") {
                assert!(text(&v, &s, b"LocationNameSid").starts_with("MP_001_"), "DICE's flags keep theirs");
            }
        }
    }
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(sorted, ["HUD_MP_CQ_LOST_BASE_G", "HUD_MP_CQ_TOOK_BASE_G"]);
    let (strings, by_language) = bc2_levelwrite::names::flag_message_texts(&game.bundle("game:overlay-00.fbrb"), &keys);
    println!("{strings:?}\n{by_language:?}");
    let took = |v: &[(String, String)]| v.iter().find(|x| x.0 == "HUD_MP_CQ_TOOK_BASE_G").map(|x| x.1.clone());
    assert_eq!(took(&strings).as_deref(), Some("WE HAVE TAKEN POSITION GOLF"));
    let de = took(by_language.get("german").unwrap()).unwrap();
    assert!(de.starts_with("WIR HABEN POSITION GOLF"), "{de}");
    assert!(!by_language.contains_key("russian"), "Russian says FOXTROT its own way: the English stands");
}

/// A soldier spawner added to Valparaiso's flag A (one operation): a copy of the flag's spawner with its spawn
/// points, linked into the flag's `Spawners`, wired to the flag's events as the template is; deleted with its unit,
/// the level's instances and the flag's spawners are as they were.
// spec: GAME-SPAWN-1
#[test]
fn a_spawner_added_to_a_flag_and_deleted() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let w0 = LevelWriter::parse(&export::level_partition(&game, "mp_001").unwrap()).unwrap();
    let schema = Schema::scan(Some(&w0.d));
    let flag = gameplay::flags(&w0)[0].clone();
    let before = gameplay::spawners(&w0, &flag);
    let template = before
        .iter()
        .find(|s| w0.type_of(w0.find(s).unwrap()).ends_with(".SoldierSpawnEntityData"))
        .unwrap()
        .clone();
    let mut at = w0.transform(&template).unwrap()[3];
    at[2] += 25.0;
    let (op, new) = gameplay::add_spawner(&w0, &flag, at, Team::TeamNeutral, &|s| {
        bc2_levelwrite::new_guid("Flag texts", 30, s)
    })
    .unwrap();
    println!("{}", op.describe());
    let w = applied(&w0, &schema, &op);
    let after = gameplay::spawners(&w, &flag);
    assert_eq!(after.len(), before.len() + 1);
    assert_eq!(after.last(), Some(&new));
    assert_eq!(gameplay::points(&w, &new).len(), gameplay::points(&w0, &template).len());
    assert_eq!(w.transform(&new).unwrap()[3], at);
    let wires = |w: &LevelWriter, g: &str| {
        bc2_levelwrite::rushstage::connections(w)
            .into_iter()
            .filter(|c| c.source == g || c.target == g)
            .count()
    };
    assert_eq!(wires(&w, &new), wires(&w0, &template), "wired as the flag's own spawner");
    let del = gameplay::delete_unit(&w, &schema, &new).unwrap();
    let back = applied(&w, &schema, &del);
    assert_eq!(back.guids(), w0.guids());
    assert_eq!(gameplay::spawners(&back, &flag), before);
    let e = gameplay::add_spawner(&w0, &template, at, Team::Team1, &|s| s.to_owned()).unwrap_err();
    assert!(e.to_string().contains("not a capture point"), "{e}");
}

/// Game tier: DICE's flags' texts name their letters (GAME-CQ-12) on every stock Conquest level of each set: each
/// capture message key of a flag's unit is `HUD_MP_CQ_TOOK_BASE_<letter>` / `HUD_MP_CQ_LOST_BASE_<letter>` of the
/// flag's marker letter, and each of its soldier spawners' `LocationNameSid` ends in `_<letter>` with a text in the
/// game. Counted per set; the keys that do not follow the form are printed.
// spec: GAME-CQ-12
#[test]
#[ignore = "game tier: every stock Conquest level; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn every_conquest_flags_texts_name_its_letter() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let langs = bc2_levelwrite::names::Languages::read(&game);
    let modes = bc2_levelwrite::profile::modes(&game).unwrap();
    let (mut flags, mut msgs, mut msgs_ok, mut spawns, mut spawns_ok, mut texts) = (0, 0, 0, 0, 0, 0);
    for (set, by_mode) in &modes {
        for lv in by_mode.get("conquest").into_iter().flatten() {
            let w = LevelWriter::parse(&export::level_partition(&game, lv).unwrap()).unwrap();
            for f in gameplay::flags(&w) {
                let Some(l) = gameplay::marker_of(&w, &f).map(|m| text(&w, &m, b"Sid")) else {
                    continue;
                };
                if l.len() != 1 {
                    continue;
                }
                flags += 1;
                let unit = gameplay::unit(&w, &f);
                for m in of_type(&w, &unit, ".MessageEntityData") {
                    let k = text(&w, &m, b"MessageSid");
                    if !k.starts_with("HUD_MP_CQ_") {
                        continue;
                    }
                    msgs += 1;
                    if k == format!("HUD_MP_CQ_TOOK_BASE_{l}") || k == format!("HUD_MP_CQ_LOST_BASE_{l}") {
                        msgs_ok += 1;
                    } else {
                        println!("{set:?} {lv} flag {l}: message {k}");
                    }
                }
                for s in gameplay::spawners(&w, &f) {
                    if !w.type_of(w.find(&s).unwrap()).ends_with(".SoldierSpawnEntityData") {
                        continue;
                    }
                    let k = text(&w, &s, b"LocationNameSid");
                    spawns += 1;
                    if k.ends_with(&format!("_{l}")) {
                        spawns_ok += 1;
                    } else {
                        println!("{set:?} {lv} flag {l}: spawn name {k}");
                    }
                    if langs.text_of(&k).is_some() {
                        texts += 1;
                    }
                }
            }
        }
    }
    println!(
        "{flags} lettered flags; capture messages {msgs_ok} of {msgs} name the letter; spawn names {spawns_ok} of \
         {spawns} end in it, {texts} with a text"
    );
    assert!(flags >= 60, "{flags}");
    assert_eq!(msgs_ok, msgs, "every capture message names its flag's letter");
}
