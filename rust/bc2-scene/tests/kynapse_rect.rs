//! KYG-RECT-1 (`spec/transforms/kynapse_generation.md`) checked on the campaign levels: DICE's path data stops at the
//! box of the level's soldier and vehicle spawns grown by 25 m. Game tier (run by `run_tests.py game`; ignored by a
//! plain `cargo test`):
//!
//!   cargo test --release -p bc2-scene --test kynapse_rect -- --ignored --nocapture
//!
//! It reads DICE's `Output\win32\ai\levels\sp_*\{infantry,vehicle}\pathdata.hpd` in Project OPFOR BFBC2's game copy
//! (`OPFOR_ROOT`, else `project-opfor-bfbc2` beside the suite) and each level's partition as NavKit prepared it
//! (`work\levels\<level>\dist`); a level that is not prepared is skipped with a note. The measurements behind the rule
//! are `examples/kynapse_geom.rs`'s `rect` fit.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

use std::fs;
use std::path::{Path, PathBuf};

use bc2_formats::{hpd, kymesh};
use bc2_scene::scene;

fn opfor() -> PathBuf {
    std::env::var_os("OPFOR_ROOT").filter(|v| !v.is_empty()).map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("..")
                .join("project-opfor-bfbc2")
        },
        PathBuf::from,
    )
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> =
        fs::read_dir(dir).map_or_else(|_| Vec::new(), |r| r.flatten().map(|e| e.path()).collect());
    out.sort();
    out
}

/// The box (min x, max x, min z, max z) of the level's soldier spawns (alternates included) and vehicle spawns.
fn spawn_box(root: &Path, lv: &str) -> Option<[f64; 4]> {
    let base = root.join("work").join("levels").join(lv);
    let bases = vec![base.join("pkg"), base.join("dist")];
    let partition = bases.iter().flat_map(|b| files_in(&b.join("levels"))).find(|p| {
        p.file_name()
            .is_some_and(|n| n.to_string_lossy().to_lowercase() == format!("{lv}.dbx"))
    })?;
    let name = partition.file_stem()?.to_string_lossy().into_owned();
    let sc = scene::load(&scene::LoadOptions::new(name, partition, bases)).ok()?;
    let mut b = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for e in &sc.entities {
        if (e.category == "soldier spawn" || e.category == "vehicle spawn")
            && let Some(p) = e.position()
        {
            b = [b[0].min(p[0]), b[1].max(p[0]), b[2].min(p[2]), b[3].max(p[2])];
        }
    }
    (b[0] <= b[1]).then_some(b)
}

/// The box of DICE's ground: every outline point of the file.
fn ground_box(path: &Path) -> Option<[f64; 4]> {
    let data = fs::read(path).ok()?;
    let f = hpd::parse(&data).expect("path data");
    let mut b = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for s in &f.sectors {
        let m = kymesh::parse(s.mesh).expect("mesh");
        for a in &m.areas {
            for v in a.outline.iter().flatten() {
                let (x, z) = (f64::from(v[0]), f64::from(v[2]));
                b = [b[0].min(x), b[1].max(x), b[2].min(z), b[3].max(z)];
            }
        }
    }
    Some(b)
}

// spec: KYG-RECT-1
#[test]
#[ignore = "game tier: reads DICE's path data and the prepared campaign levels"]
fn ground_stops_25_m_beyond_the_spawns() {
    let root = opfor();
    let ai = root.join("game").join("Output").join("win32").join("ai").join("levels");
    let levels: Vec<String> = files_in(&ai)
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_lowercase())
        .filter(|n| n.starts_with("sp_"))
        .collect();
    if levels.is_empty() {
        println!("skipped: no game copy's path data (OPFOR_ROOT or beside the suite)");
        return;
    }
    // per kind: sides measured, sides at the box (the ground's last cells), least slack
    let mut tally = [(0usize, 0usize, f64::MAX); 2];
    for lv in &levels {
        let Some(e) = spawn_box(&root, lv) else {
            println!("{lv}: skipped, not prepared");
            continue;
        };
        for (k, kind) in ["infantry", "vehicle"].into_iter().enumerate() {
            let Some(g) = ground_box(&ai.join(lv).join(kind).join("pathdata.hpd")) else {
                continue;
            };
            // How far each side of the ground lies inside the spawns' box grown by 25 m.
            let slack = [
                g[0] - (e[0] - 25.0),
                (e[1] + 25.0) - g[1],
                g[2] - (e[2] - 25.0),
                (e[3] + 25.0) - g[3],
            ];
            println!(
                "{lv} {kind}: {:.2} / {:.2} / {:.2} / {:.2} m inside",
                slack[0], slack[1], slack[2], slack[3]
            );
            // the ground's last cells: within one pitch (0.8 infantry, 1.0 vehicle) of the least inset
            let (least, pitch) = if k == 0 { (0.1, 0.8) } else { (2.2, 1.0) };
            for s in slack {
                tally[k].0 += 1;
                tally[k].2 = tally[k].2.min(s);
                if s < least + pitch + 0.05 {
                    tally[k].1 += 1;
                }
                assert!(
                    s >= least,
                    "{lv} {kind}: the ground passes the spawns' box + 25 m ({s:.2} m inside)"
                );
            }
        }
    }
    let (n, at, least) = tally[0];
    println!(
        "infantry: {at} of {n} sides end at the box, the least inset {least:.2} m; vehicle: {} of {}, {:.2} m",
        tally[1].1, tally[1].0, tally[1].2
    );
    if n > 0 {
        assert!(at * 100 >= n * 85, "infantry: under 85 % of the sides end at the box");
    }
}
