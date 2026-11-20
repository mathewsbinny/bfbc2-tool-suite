//! Round E3d's proofs on real data (`EDITOR_ARCHITECTURE.md` 3.12, "Round E3d"): the modes each era offers, read
//! from the stock levels; each mode's layer as the data has it (Rush's stages of M-COMs, Squad Deathmatch's squads);
//! a mode choice as an operation of a project's log (the base's layer out, the donor's in, an undo takes it back);
//! the completeness rules naming what is missing. The canary exports themselves are `examples/e3d_matrix.rs` (their
//! builds take minutes). Reads OPFOR's game copy (found beside the suite, or `OPFOR_GAME`), writes nothing; skipped
//! where there is no game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr,
    clippy::many_single_char_names,
    clippy::similar_names
)]

use std::path::PathBuf;

use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::modelayer;
use bc2_levelwrite::{LevelWriter, MapProject, Op, Profile};

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

/// The modes of each era, from the data, and the layer each mode's stock level has.
#[test]
fn each_eras_modes_and_their_layers() {
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let modes = bc2_levelwrite::profile::modes(&game).unwrap();
    let keys = |p: Profile| -> Vec<&str> { modes[&p].keys().map(String::as_str).collect() };
    assert_eq!(
        keys(Profile::Bfbc2),
        ["conquest", "goldrush", "onslaught", "squadrush", "teamdeathmatch"]
    );
    assert_eq!(
        keys(Profile::Bfbc1),
        ["conquest", "goldrush", "squadrush", "teamdeathmatch"]
    );
    assert_eq!(
        keys(Profile::Vietnam),
        ["conquest", "goldrush", "squadrush", "teamdeathmatch"]
    );
    assert_eq!(keys(Profile::Ww2), ["airsuperiority", "conquest"]);
    // Rush: four stages of two M-COMs; Squad Rush: two of one; Squad Deathmatch: its squads and kill counters
    let gr = modelayer::signature(&level(&game, "mp_005gr"));
    assert_eq!(gr.stages, [2, 2, 2, 2]);
    assert_eq!(gr.counters[1], 2, "the attackers' lives");
    let sr = modelayer::signature(&level(&game, "mp_005sr"));
    assert_eq!(sr.stages, [1, 1]);
    let sdm = modelayer::signature(&level(&game, "mp_009sdm"));
    assert_eq!((sdm.teams, sdm.counters[2]), (5, 4));
    let nam = modelayer::signature(&level(&game, "nam_mp_002r"));
    assert_eq!(nam.stages, [2, 2, 2]);
    // the layers as the closure measures them (module doc)
    assert_eq!(modelayer::layer(&level(&game, "mp_009cq")).len(), 238);
    assert_eq!(modelayer::layer(&level(&game, "mp_005gr")).len(), 821);
}

/// A mode choice is an operation: Atacama's Rush (`MP_005GR`) made a BFBC2 Conquest level - its Rush layer out, the
/// donor's Conquest layer in, complete; an undo gives the base back.
#[test]
fn rush_to_conquest_as_a_project_operation() {
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let base = export::level_partition(&game, "mp_005gr").unwrap();
    let mut p = MapProject::new("Rush as Conquest", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Bfbc2, "Conquest", None).unwrap();
    assert_eq!(c.mode, "conquest");
    assert!(!c.removed.is_empty() && !c.items.is_empty());
    assert!(c.fit.scale > 0.0 && c.fit.scale <= 1.0);
    let donor = c.donor.clone();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let (cur, _) = export::current(&game, &p).unwrap();
    let have = modelayer::signature(&cur);
    assert!(have.stages.is_empty(), "the Rush M-COMs are out: {:?}", have.stages);
    let d = modelayer::signature(&level(&game, &donor));
    assert!(modelayer::missing("conquest", &have, &d).is_empty(), "{have:?}");
    // every capture point deleted: the missing bases and flags named
    assert_eq!(
        modelayer::anchors(&cur).len(),
        have.bases.iter().sum::<usize>() + have.flags
    );
    let mut q = p.clone();
    let caps: Vec<String> = modelayer::anchors(&cur).into_iter().map(|a| a.0).collect();
    q.push(Op::Delete { guids: caps }, "all capture points");
    let (bare, _) = export::current(&game, &q).unwrap();
    let miss = modelayer::missing("conquest", &modelayer::signature(&bare), &d);
    assert!(miss.iter().any(|m| m.contains("team 1 has no base")), "{miss:?}");
    assert!(miss.iter().any(|m| m == "Conquest needs at least one flag"), "{miss:?}");
    // undone: the base as it was
    p.undo();
    let (back, _) = export::current(&game, &p).unwrap();
    assert_eq!(back.write().unwrap(), base);
    // a mode the era does not have, and a pending one, are refused in words
    let e = export::choose_mode(&game, &p, Profile::Ww2, "goldrush", None).unwrap_err();
    assert!(e.to_string().contains("no WW2"), "{e}");
    let e = export::choose_mode(&game, &p, Profile::Ww2, "airsuperiority", None).unwrap_err();
    assert!(e.to_string().contains("Air Superiority"), "{e}");
}
