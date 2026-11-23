//! Round NKS-deep E's check of KYG-SEED-2 on DICE's files: the seed points are made nodes unlocked, so a seed the
//! simplification never moved is a node on the sample grid; KYG-SEED-1's perimeter seeds lie on the lines 1.6 m
//! inside the 100 m squares. Counts DICE's ordinary nodes on the grid (both coordinates within 5 mm of a whole pitch)
//! by where they lie: on a perimeter line (x or z modulo 100 at 1.6 or 98.4 m), on the lines two pitches inside
//! (detailed: 0.4 / 99.6 m), or elsewhere; and the grid points' own share of those lines. Read-only, counts only.
//!
//!   cargo run --release -p bc2-formats --example kynapse_seeds [-- <levels folder>]
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bc2_formats::{hpd, kymesh};

fn levels_dir() -> PathBuf {
    if let Some(a) = std::env::args().nth(1) {
        return PathBuf::from(a);
    }
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    root.join("game").join("Output").join("win32").join("ai").join("levels")
}

fn on_grid(v: f64, p: f64) -> bool {
    let r = v / p;
    (r - r.round()).abs() * p <= 0.005
}

fn line_at(v: f64, at: f64) -> bool {
    let m = v.rem_euclid(100.0);
    (m - at).abs() <= 0.005 || (m - (100.0 - at)).abs() <= 0.005
}

fn main() {
    let mut levels: Vec<_> = std::fs::read_dir(levels_dir())
        .expect("levels folder")
        .flatten()
        .collect();
    levels.sort_by_key(std::fs::DirEntry::file_name);
    let mut c: BTreeMap<String, u64> = BTreeMap::new();
    let mut add = |k: String| *c.entry(k).or_default() += 1;
    for l in levels {
        for (kind, cl0) in [("infantry", "inf"), ("vehicle", "veh")] {
            let path = l.path().join(kind).join("pathdata.hpd");
            let Ok(data) = std::fs::read(&path) else { continue };
            let f = hpd::parse(&data).expect("path data");
            for s in &f.sectors {
                let m = kymesh::parse(s.mesh).expect("mesh");
                let p = f64::from(m.pitch);
                let cl = if cl0 == "veh" {
                    "veh"
                } else if p < 0.5 {
                    "det"
                } else {
                    "coarse"
                };
                let inner = 2.0 * p;
                for n in &s.nodes {
                    if n.id & 0x8000_0000 != 0 {
                        continue;
                    }
                    let (x, z) = (f64::from(n.pos[0]), f64::from(n.pos[2]));
                    add(format!("{cl}.nodes"));
                    if !(on_grid(x, p) && on_grid(z, p)) {
                        continue;
                    }
                    let place = if line_at(x, 1.6) || line_at(z, 1.6) {
                        "line_1.6"
                    } else if line_at(x, inner) || line_at(z, inner) {
                        "line_2pitch"
                    } else {
                        "elsewhere"
                    };
                    add(format!("{cl}.on_grid.{place}"));
                }
            }
        }
    }
    for (k, v) in &c {
        println!("{k} {v}");
    }
    println!(
        "grid points of a 100 m square on the 1.6 m lines: coarse 4 x 125 of 125 x 125 = 3.2 %, detailed 4 x 500 of 500 x 500 = 0.8 %"
    );
}
