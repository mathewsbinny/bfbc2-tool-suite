//! Round A1's calibration of the site rules on DICE's own levels: XVAL-PLACE-2 to 4 run on every object of each stock
//! level of the game copy the editor can open with its collision (DICE's spawn points, vehicle spawners, flags and
//! M-COMs as DICE placed them: the rules must hold on them), with what each finding measured.
//!
//! ```text
//! cargo run --release -p bc2-editor --example a1_calibrate -- [<level>...] [--opfor <folder>]
//! ```
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::collapsible_if
)]

use std::collections::{BTreeMap, HashSet};

use bc2_editor::levels;
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::write::LevelWriter;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let root = levels::find_opfor(value("--opfor").as_deref().map(std::path::Path::new)).unwrap();
    let game = Game::open(&root.join("game")).unwrap();
    let mut wanted: Vec<String> = args.iter().filter(|a| !a.starts_with("--")).cloned().collect();
    if let Some(o) = value("--opfor") {
        wanted.retain(|a| *a != o);
    }
    if wanted.is_empty() {
        wanted = levels::list_levels(&root)
            .into_iter()
            .filter(|l| l.prepared && l.lv.starts_with("mp_"))
            .map(|l| l.lv)
            .collect();
    }
    let mut totals: BTreeMap<String, usize> = BTreeMap::new();
    let mut objects = [0usize; 3];
    for lv in wanted {
        let Ok(bytes) = export::level_partition(&game, &lv) else {
            println!("{lv}: no partition");
            continue;
        };
        let w = LevelWriter::parse(&bytes).unwrap();
        let Ok(found) = levels::load_options(&root, &lv) else {
            println!("{lv}: not prepared");
            continue;
        };
        let scene = bc2_scene::scene::load(&found.options).unwrap();
        let skip: HashSet<String> = bc2_levelwrite::modelayer::layer(&w);
        let site = if scene.heightfield.is_some() {
            bc2_levelwrite::site::from_scene(&scene, &skip)
        } else {
            let (hf, water) = export::ground_of(&game, &lv, &w).unwrap();
            bc2_levelwrite::site::with_ground(&scene, &skip, hf, water.as_ref())
        };
        let f = bc2_levelwrite::validate::site_rules(&w, None, &site);
        // DICE's own placements measured: how deep spawn points are in objects, how far under the ground, the room
        // of each vehicle spawner by class, whether boats stand over water
        for e in w.d.of_type(b".AlternateSpawnEntityData") {
            let Some(r) = w.transform_at(e) else { continue };
            let p = r[3].map(f64::from);
            if let Some((n, d)) = site.buried(p) {
                println!("  BURIED {d:.2} {n}");
            }
            if let Some(g) = site.ground(p[0], p[2]) {
                if p[1] < g - 1.0 {
                    println!("  UNDER {:.2}", g - p[1]);
                }
            }
        }
        for (g, e, classes) in bc2_levelwrite::placement::vehicle_spawners(&w) {
            let Some(r) = w.transform_at(e) else { continue };
            let p = r[3].map(f64::from);
            let c = classes.join(",");
            let wet = site.water(p[0], p[2]).map(|s| s - site.ground(p[0], p[2]).unwrap_or(s));
            println!(
                "  VEH {c} clear {:.1} water {:?} buried {:?} ground {:?} y {:.1} {g}",
                site.clearance(p, 8.0),
                wet.map(|x| (x * 10.0).round() / 10.0),
                site.buried(p).map(|b| (b.1 * 10.0).round() / 10.0),
                site.ground(p[0], p[2]).map(|x| (x * 10.0).round() / 10.0),
                p[1]
            );
        }
        let n = |t: &[u8]| w.d.of_type(t).len();
        objects[0] += n(b".AlternateSpawnEntityData");
        objects[1] += n(b".VehicleSpawnEntityData");
        objects[2] += n(b".CapturePointEntityData");
        println!(
            "{lv}: {} spawn points, {} vehicle spawners, {} capture points; {} findings",
            n(b".AlternateSpawnEntityData"),
            n(b".VehicleSpawnEntityData"),
            n(b".CapturePointEntityData"),
            f.len()
        );
        for x in &f {
            println!("    {}", x.line());
            if x.code == "XVAL-PLACE-2" || x.code == "XVAL-PLACE-3" {
                if let Some(r) = x.objects.first().and_then(|g| w.transform(g)) {
                    let p = r[3].map(f64::from);
                    println!(
                        "      at ({:.1}, {:.1}, {:.1}), ground {:?}, water {:?}: {:?}",
                        p[0],
                        p[1],
                        p[2],
                        site.ground(p[0], p[2]).map(|g| (g * 10.0).round() / 10.0),
                        site.water(p[0], p[2]),
                        site.explain(p)
                    );
                }
            }
            *totals.entry(x.code.clone()).or_default() += 1;
        }
    }
    println!(
        "totals over {} spawn points, {} vehicle spawners, {} capture points: {totals:?}",
        objects[0], objects[1], objects[2]
    );
}
