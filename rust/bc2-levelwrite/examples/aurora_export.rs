//! Round E3's first export (`EDITOR_ARCHITECTURE.md` 3.12, "First export"): a small scripted project on Operation
//! Aurora (`SP_01`) and its export file, made from the game copy's data (nothing written there).
//!
//! `cargo run -p bc2-levelwrite --example aurora_export -- <game folder> <out folder>`
//!
//! The project moves the two slotted objects nearest the campaign's player start 10 m east and deletes the next
//! two (by distance, then guid: the same objects every run); the export makes it `MAPEDIT_AURORA_CQ`, Conquest with
//! `MP_009CQ`'s layer: the two bases and two of its three flags placed between the beach and the inland end of the
//! campaign's first combat area (`CombatArea6`, kept as the map's), heights following the ground.
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing
)]

use std::path::PathBuf;

use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::mpconvert::{Anchor, LayerSpec};
use bc2_levelwrite::ops::{Moved, Op};
use bc2_levelwrite::{LevelWriter, MapProject, Profile};

/// The campaign's player start (`Wyatt PLAYER`, `SoldierSpawnEntityData` 48FA4AFB...).
const START: [f32; 3] = [-738.42, 5.09, 456.96];
/// `CombatArea6`, enabled at the campaign's start (55 ha: x -983..-142, z -97..1026).
const AREA: &str = "E4C7B1C2-BB9F-481A-B3CC-F67E4E5CB088";

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let game = Game::open(&PathBuf::from(&a[1])).expect("game");
    let out = PathBuf::from(&a[2]);
    std::fs::create_dir_all(&out).unwrap();
    let bundles = game.loaded("sp_01", "level");
    let live = game.live(&bundles).unwrap();
    let base = export::read_found(&[live.get("levels/sp_01.dbx").expect("SP_01")])
        .unwrap()
        .pop()
        .unwrap();
    let w = LevelWriter::parse(&base).unwrap();
    // slotted placed objects by distance from the start
    let mut objs: Vec<(f32, String, String)> =
        w.d.of_type(b".ReferenceObjectData")
            .into_iter()
            .filter(|&e| bc2_levelwrite::culltree::slot_of(&w, e) > 0)
            .filter_map(|e| {
                let g = String::from_utf8_lossy(w.d.attr(e, b"guid")?).to_ascii_uppercase();
                let p = w.transform(&g)?[3];
                let bp =
                    w.d.child(e, b"Blueprint")
                        .and_then(|x| w.d.attr(x, b"ref"))
                        .map_or_default(|r| String::from_utf8_lossy(r).into_owned());
                Some(((p[0] - START[0]).hypot(p[2] - START[2]), g, bp))
            })
            .collect();
    objs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
    let mut p = MapProject::new("Aurora Conquest", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let moved: Vec<Moved> = objs
        .iter()
        .take(2)
        .map(|(_, g, _)| {
            let from = w.transform(g).unwrap();
            let mut to = from;
            to[3][0] += 10.0;
            Moved {
                guid: g.clone(),
                from,
                to,
                placement: None,
            }
        })
        .collect();
    p.push(Op::Move { items: moved }, "E3 proof: two objects 10 m east");
    let gone: Vec<String> = objs.iter().skip(2).take(2).map(|x| x.1.clone()).collect();
    p.push(Op::Delete { guids: gone }, "E3 proof: two objects deleted");
    for (d, g, bp) in objs.iter().take(4) {
        println!("{g} {d:6.1} m {bp}");
    }
    let path = out.join(p.file_name());
    p.save(&path).unwrap();
    // the layer: bases at the beach and inland, flags between them
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_AURORA_CQ".into(),
        title: "Operation Aurora (Conquest)".into(),
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
            combat_area: AREA.into(),
        },
    };
    std::fs::write(
        out.join("export.json"),
        serde_json::to_string_pretty(&spec).unwrap() + "\n",
    )
    .unwrap();
    println!("{}", path.display());
}
