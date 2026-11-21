//! Round E3's proof on real data: Operation Aurora (`SP_01`) exported as a new Conquest level and checked offline
//! (`EDITOR_ARCHITECTURE.md` 3.12). Reads OPFOR's game copy (found beside the suite, or `OPFOR_GAME`), writes only a
//! temporary stage folder; skipped where there is no game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stderr
)]

use std::path::PathBuf;

use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::mpconvert::{Anchor, LayerSpec};
use bc2_levelwrite::ops::{Moved, Op};
use bc2_levelwrite::{LevelWriter, MapProject, Profile};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

#[test]
fn sp_01_exports_as_a_new_conquest_level() {
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let live = game.live(&game.loaded("sp_01", "level")).unwrap();
    let base = export::read_found(&[live.get("levels/sp_01.dbx").unwrap()])
        .unwrap()
        .pop()
        .unwrap();
    let w = LevelWriter::parse(&base).unwrap();
    // two slotted objects moved, two deleted (the first four slotted objects in the file)
    let objs: Vec<String> =
        w.d.of_type(b".ReferenceObjectData")
            .into_iter()
            .filter(|&e| bc2_levelwrite::culltree::slot_of(&w, e) > 0)
            .map(|e| String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap()).to_ascii_uppercase())
            .take(4)
            .collect();
    let mut p = MapProject::new("E3 test", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let items = objs[..2]
        .iter()
        .map(|g| {
            let from = w.transform(g).unwrap();
            let mut to = from;
            to[3][2] += 7.0;
            Moved {
                guid: g.clone(),
                from,
                to,
                placement: None,
            }
        })
        .collect();
    p.push(Op::Move { items }, "");
    p.push(
        Op::Delete {
            guids: objs[2..].to_vec(),
        },
        "",
    );
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_E3TEST_CQ".into(),
        folder: String::new(),
        title: "E3 test".into(),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor: "mp_009cq".into(),
            bases: vec![
                Anchor {
                    from: "US Base".into(),
                    at: [-700.0, 0.0, 430.0],
                },
                Anchor {
                    from: "RUS Base".into(),
                    at: [-560.0, 0.0, -20.0],
                },
            ],
            flags: vec![
                Anchor {
                    from: "A: Crash Site".into(),
                    at: [-660.0, 0.0, 270.0],
                },
                Anchor {
                    from: "B: Hill".into(),
                    at: [-600.0, 0.0, 120.0],
                },
            ],
            combat_area: "E4C7B1C2-BB9F-481A-B3CC-F67E4E5CB088".into(),
        },
        // E3's export: the BFBC2 profile, the base's names and loading picture
        profile: Profile::Bfbc2,
        identity: None,
    };
    let out = std::env::temp_dir().join(format!("bc2_levelwrite_export_{}", std::process::id()));
    let rec = export::build(&game, &p, "test", &spec, &out).unwrap();
    assert_eq!(rec.level, "MAPEDIT_E3TEST_CQ");
    assert_eq!(rec.files.len(), export::KINDS.len());
    assert!(
        rec.report.iter().any(|l| l.starts_with("cull tree: 2 objects")),
        "{:?}",
        rec.report
    );
    let r = export::check(&game, &out).unwrap();
    let _ = std::fs::remove_dir_all(&out);
    assert!(r.ok(), "{:#?}", r.checks);
    // a stock name is refused
    let mut stock = spec.clone();
    stock.level = "MP_005".into();
    assert!(export::build(&game, &p, "test", &stock, &out).is_err());
    let _ = std::fs::remove_dir_all(&out);
}
