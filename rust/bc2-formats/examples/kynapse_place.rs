//! Round NKS-deep B's fits of DICE's node placement (`spec/transforms/kynapse_generation.md`, KYG-NODE-7): the
//! propagation that `BFBC2Game.exe`'s map builder runs (`kymapbuilderrgg.cpp`: every node sends NbSectors probes,
//! one every 2 pi / NbSectors radians, DistPropagation along the ground) and the simplification after it (edges
//! collapsed to their middles). Read-only over DICE's path data, counts and histograms only.
//!
//!   cargo run --release -p bc2-formats --example kynapse_place -- [--only <fit>] [--level <name>] [<levels folder>]
//!
//! Fits: `angles` (the direction of node pairs exactly one propagation step apart: on the NbSectors directions or
//! not; KYG-NODE-7). The folder defaults to Project OPFOR BFBC2's `game\Output\win32\ai\levels` (`OPFOR_ROOT`, else `project-opfor-bfbc2`
//! beside the suite).
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_lossless,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::float_cmp,
    missing_docs
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bc2_formats::{hpd, kymesh};

/// Named counters, printed sorted.
#[derive(Default)]
struct C(BTreeMap<String, u64>);

impl C {
    fn add(&mut self, k: &str, n: u64) {
        *self.0.entry(k.to_owned()).or_default() += n;
    }
    fn inc(&mut self, k: &str) {
        self.add(k, 1);
    }
}

struct Sec {
    nodes: Vec<hpd::Node>,
    mesh: kymesh::Mesh,
}

fn class(m: &kymesh::Mesh, vehicle: bool) -> &'static str {
    if vehicle {
        "veh"
    } else if (m.pitch - 0.2).abs() < 1e-4 {
        "det"
    } else {
        "inf"
    }
}

/// (DistPropagation, NbSectors) of a class (KYG-PAR-1, KYG-PAR-2).
fn prop(cl: &str) -> (f64, u32) {
    match cl {
        "veh" => (15.35, 64),
        "det" => (0.85, 128),
        _ => (3.35, 32),
    }
}

fn levels_dir(given: Option<&str>) -> PathBuf {
    if let Some(a) = given {
        return PathBuf::from(a);
    }
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    root.join("game").join("Output").join("win32").join("ai").join("levels")
}

/// Ordinary nodes (no border bit).
fn ordinary(s: &Sec) -> Vec<&hpd::Node> {
    s.nodes.iter().filter(|n| n.id & 0x8000_0000 == 0).collect()
}

/// The pairs of ordinary nodes of a sector at the same floor whose horizontal distance is `d` within `tol`.
fn pairs_at<'a>(ord: &[&'a hpd::Node], d: f64, tol: f64) -> Vec<(&'a hpd::Node, &'a hpd::Node)> {
    let mut out = Vec::new();
    for (i, a) in ord.iter().enumerate() {
        for b in ord.iter().skip(i + 1) {
            if (a.pos[1] - b.pos[1]).abs() > 3.0 {
                continue;
            }
            let dx = f64::from(a.pos[0]) - f64::from(b.pos[0]);
            let dz = f64::from(a.pos[2]) - f64::from(b.pos[2]);
            if (dx.hypot(dz) - d).abs() <= tol {
                out.push((*a, *b));
            }
        }
    }
    out
}

/// The angular distance (degrees) of the pair's direction from the nearest of `n` directions from 0.
fn off_grid(a: &hpd::Node, b: &hpd::Node, n: u32) -> f64 {
    let dx = f64::from(b.pos[0]) - f64::from(a.pos[0]);
    let dz = f64::from(b.pos[2]) - f64::from(a.pos[2]);
    let ang = dz.atan2(dx).to_degrees().rem_euclid(360.0);
    let step = 360.0 / f64::from(n);
    let r = ang.rem_euclid(step);
    r.min(step - r)
}

/// `angles`: pairs exactly one propagation step apart (+- 2 mm) against pairs between 1.02 and 1.10 steps (control),
/// by how far their direction lies from the NbSectors directions.
fn fit_angles(s: &Sec, cl: &str, c: &mut C) {
    let (step, n) = prop(cl);
    let ord = ordinary(s);
    for (tag, d, tol) in [("step", step, 0.002), ("ctrl", step * 1.06, step * 0.04)] {
        for (a, b) in pairs_at(&ord, d, tol) {
            let off = off_grid(a, b, n);
            let bin = if off < 0.01 {
                "a_000.01"
            } else if off < 0.1 {
                "b_000.1"
            } else if off < 1.0 {
                "c_001"
            } else {
                "d_more"
            };
            c.inc(&format!("angles.{cl}.{tag}.{bin}"));
            // half the directions (n / 2) as a check that n itself is the count
            let half = off_grid(a, b, n / 2);
            if off < 0.01 {
                c.inc(&format!(
                    "angles.{cl}.{tag}.on_n_{}",
                    if half < 0.01 { "even" } else { "odd" }
                ));
            }
        }
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let value = |k: &str| args.iter().position(|a| a == k).and_then(|i| args.get(i + 1)).cloned();
    let only = value("--only");
    let level = value("--level").map(|l| l.to_lowercase());
    let flagged: Vec<String> = [only.clone(), level.clone()].into_iter().flatten().collect();
    let folder = args
        .iter()
        .find(|a| !a.starts_with("--") && !flagged.contains(a))
        .cloned();
    let run = |f: &str| only.as_deref().is_none_or(|o| o == f);
    let mut levels: Vec<_> = std::fs::read_dir(levels_dir(folder.as_deref()))
        .expect("levels folder")
        .flatten()
        .collect();
    levels.sort_by_key(std::fs::DirEntry::file_name);
    let mut c = C::default();
    for l in levels {
        let lname = l.file_name().to_string_lossy().to_lowercase();
        if level.as_ref().is_some_and(|w| *w != lname) {
            continue;
        }
        for (kind, veh) in [("infantry", false), ("vehicle", true)] {
            let p = l.path().join(kind).join("pathdata.hpd");
            let Ok(data) = std::fs::read(&p) else { continue };
            let f = hpd::parse(&data).expect("path data");
            for s in &f.sectors {
                let s = Sec {
                    nodes: s.nodes.clone(),
                    mesh: kymesh::parse(s.mesh).expect("mesh"),
                };
                let cl = class(&s.mesh, veh);
                if run("angles") {
                    fit_angles(&s, cl, &mut c);
                }
            }
        }
    }
    println!("--- totals");
    for (k, v) in &c.0 {
        println!("{k} {v}");
    }
}
