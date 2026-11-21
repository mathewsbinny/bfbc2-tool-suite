//! Round VW2's proofs on real data (`EDITOR_ARCHITECTURE.md` 3.12, "Round VW2"): the mode view layer's Rush stages
//! as they are played, against L1's own simulation (`bfbc2-map-editor/placement/scripts/l1_measure.py` `simulate`,
//! run on the same game copy: each stage's M-COMs and the bases and combat areas enabled while it is played); the
//! weather states of a level and the export's weather choice. Reads OPFOR's game copy (found beside the suite, or
//! `OPFOR_GAME`), writes only to a temporary folder; skipped where there is no game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::doc_markdown,
    clippy::panic
)]

use std::collections::HashSet;
use std::path::PathBuf;

use bc2_levelwrite::LevelWriter;
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::modelayer;

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

fn level(game: &Game, lv: &str) -> LevelWriter {
    LevelWriter::parse(&export::level_partition(game, lv).unwrap()).unwrap()
}

fn names(w: &LevelWriter, guids: &[String]) -> Vec<String> {
    guids
        .iter()
        .map(|g| {
            let e = w.find(g).unwrap();
            let t = |f: &[u8]| {
                w.d.child(e, f)
                    .and_then(|x| w.d.text(x))
                    .map(|t| String::from_utf8_lossy(t).into_owned())
                    .filter(|t| !t.is_empty())
            };
            t(b"LocationName").or_else(|| t(b"Name")).unwrap_or_default()
        })
        .collect()
}

/// Each Rush stage as it is played - its M-COMs, the bases (capture points) and combat areas enabled while it is -
/// as L1's simulation of the same levels gives them (run 2026-11-21 on OPFOR's game copy: `l1_stages.py`), in the
/// same play order; every M-COM in one stage, as [`modelayer::stages`] groups them.
// spec: GAME-RUSH-9
#[test]
fn rush_stages_as_played_match_l1() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    // L1's bases per stage (LocationName, else Name), in the partition's order; 2 combat areas each stage
    let want: [(&str, &[&[&str]]); 4] = [
        (
            "mp_005gr",
            &[
                &["RUS Base", "South Island"],
                &["SI", "Stranded Ship"],
                &["SII", "North Island"],
                &["North Island", "US Harbor"],
            ],
        ),
        (
            "mp_005sr",
            &[&["US Base", "Stranded ship"], &["Stranded ship", "RU Base"]],
        ),
        (
            "nam_mp_002r",
            &[
                &["NVA Fish Market", "US drop zone"],
                &["US Fish Market", "NVA Shrine"],
                &["US Shrine", "NVA Temple"],
            ],
        ),
        (
            "bc1_oasis_gr",
            &[
                &["US Base", "Farm"],
                &["Plantation", "Farm"],
                &["Suburbs", "Plantation"],
                &["Streets", "Suburbs"],
                &["Plaza", "Suburbs"],
            ],
        ),
    ];
    for (lv, stages) in want {
        let w = level(&game, lv);
        let v = modelayer::layer_view(&w);
        assert!(
            !v.uncertain,
            "{lv}: the play order is certain (as L1 found on every DICE Rush level)"
        );
        assert_eq!(v.stages.len(), stages.len(), "{lv}: stages");
        let groups: Vec<HashSet<String>> = modelayer::stages(&w)
            .into_iter()
            .map(|g| g.into_iter().collect())
            .collect();
        for (k, (s, bases)) in v.stages.iter().zip(stages).enumerate() {
            let got = names(&w, &s.bases);
            eprintln!(
                "{lv} stage {}: {} M-COMs, bases {got:?}, {} combat areas, {} of the layer's {} hidden",
                k + 1,
                s.mcoms.len(),
                s.combat_areas.len(),
                s.hidden.len(),
                v.layer.len()
            );
            assert_eq!(got, *bases, "{lv} stage {}: the bases in play", k + 1);
            assert_eq!(
                s.combat_areas.len(),
                2,
                "{lv} stage {}: the combat areas in play",
                k + 1
            );
            let mine: HashSet<String> = s.mcoms.iter().cloned().collect();
            assert!(
                groups.contains(&mine),
                "{lv} stage {}: its M-COMs are one of the stages' groups",
                k + 1
            );
            // what the stage hides: the other stages' M-COMs, never its own or its bases
            for (j, o) in v.stages.iter().enumerate() {
                for m in &o.mcoms {
                    assert_eq!(
                        s.hidden.contains(m),
                        j != k,
                        "{lv} stage {}: M-COM of stage {}",
                        k + 1,
                        j + 1
                    );
                }
            }
            for b in s.bases.iter().chain(&s.combat_areas) {
                assert!(!s.hidden.contains(b), "{lv} stage {}: {b} in play but hidden", k + 1);
            }
            assert!(
                s.hidden.iter().all(|h| v.layer.contains(h)),
                "{lv}: only the layer is hidden"
            );
        }
    }
    // a Conquest level: its layer, no stages
    let cq = modelayer::layer_view(&level(&game, "mp_009cq"));
    assert_eq!((cq.layer.len(), cq.stages.len()), (238, 0));
}

fn live(game: &Game, lv: &str) -> std::collections::HashMap<String, export::Found> {
    let mut b = game.loaded(lv, "level");
    b.extend(game.startup());
    game.live(&b).unwrap()
}

/// The levels' weather as the export reads it (spec RNDR-WTHR-5, -6): the entity in the level partition (Wake Island)
/// or placed (MP_005), the system's states, `ActiveState` and the spawn switch, as WX0's inventory has them.
// spec: RNDR-WTHR-5, RNDR-WTHR-6
#[test]
fn levels_weather_as_the_export_reads_it() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    for (lv, blueprint, states, active, spawn) in [
        (
            "mp_005",
            Some("weather/r_mp005/weathersystementity_mp005"),
            25,
            0,
            Some(2),
        ),
        ("mp_002", Some("weather/r_mp02/weathersystementity_mp02"), 7, 2, Some(2)),
        (
            "ons_mp_002",
            Some("weather/r_mp02/weathersystementity_mp02"),
            7,
            2,
            Some(5),
        ),
        ("wake_island_s", None, 5, 0, None),
        ("coral_sea", None, 5, 0, Some(4)),
    ] {
        let w = level(&game, lv);
        let lw = bc2_levelwrite::weather::level_weather(&w, &live(&game, lv)).unwrap();
        eprintln!(
            "{lv}: {:?} names {} ({} states) and {} (ActiveState {}), spawn switch {:?}",
            lw.blueprint,
            lw.system,
            lw.states.len(),
            lw.settings,
            lw.active,
            lw.spawn.as_ref().map(|x| x.1)
        );
        assert_eq!(lw.blueprint.as_deref(), blueprint, "{lv}");
        assert_eq!(lw.states.len(), states, "{lv}");
        assert_eq!(lw.active, active, "{lv}");
        assert_eq!(lw.spawn.map(|x| x.1), spawn, "{lv}");
    }
}

/// The night template (VW2c, the user's choice 2026-11-21): `ons_mp_002_night` of MP_002's system, in the Onslaught
/// Package, still resolves - the state and its six components through MP_002's bundles. When Onslaught is re-ported
/// this test names the template to re-point.
// spec: RNDR-WTHR-9
#[test]
fn the_night_template_resolves() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let (lv, state) = bc2_levelwrite::weather::NIGHT_TEMPLATE;
    let l = live(&game, lv);
    let w = level(&game, lv);
    let lw = bc2_levelwrite::weather::level_weather(&w, &l).unwrap();
    assert!(
        lw.states.iter().any(|s| s == state),
        "{state} is a state of {lv}'s system: {:?}",
        lw.states
    );
    let f = l
        .get(&format!("{state}.dbx"))
        .unwrap_or_else(|| panic!("{state}: in no bundle of {lv}"));
    let data = export::read_found(&[f]).unwrap().pop().unwrap();
    let d = bc2_formats::dbx::edit::EditDbx::parse(&data).unwrap();
    let mut parts = Vec::new();
    for r in d.roots().to_vec() {
        for e in d.walk(r) {
            if let Some(v) = d.attr(e, b"ref")
                && let Some((p, _)) = String::from_utf8_lossy(v).rsplit_once('/')
            {
                parts.push(p.to_ascii_lowercase());
            }
        }
    }
    parts.sort();
    parts.dedup();
    eprintln!("{state} ({}): {parts:?}", f.0.display());
    assert!(parts.len() >= 6, "the state's components");
    for p in &parts {
        assert!(l.contains_key(&format!("{p}.dbx")), "{p} resolves");
    }
    assert!(
        f.0.to_string_lossy().to_ascii_lowercase().contains("package"),
        "the template lives in the Onslaught Package's bundle"
    );
}
