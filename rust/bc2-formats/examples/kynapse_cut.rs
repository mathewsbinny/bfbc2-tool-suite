//! Round NKS-deep E's fit of KYG-AREA-4's trigger on DICE's files: each area's in-place record size as the map
//! builder's size test counts it (the "Kynogon InPlace Mesh" serialiser run for its size, 0x0134EDA0 called from
//! 0x011AD550), from the stored record's counts, against the midline cuts DICE made. Read-only, counts only.
//!
//!   cargo run --release -p bc2-formats --example kynapse_cut [-- <levels folder>]
//!
//! The in-place size of an area: 100 + 28 per portal segment (A, B) + 52 per C entry + 20 per D / E chain + 12 per
//! D / E segment + 32 per F polygon + 12 per F segment. The stored B portals are one segment each; the stored D and
//! E lines are chains of n points (n - 1 segments); an F cell of n points is a polygon of n - 1 segments. The
//! stored C portals (sector border) are counted three ways (variants): as outline segments of one chain, as portal
//! segments, or not at all. A cut group (areas of one tile joined across the tile's midline by B portals, round
//! NKS-rest's groups) is merged back: its pieces' sizes summed, less 100 per extra piece and 28 per midline portal
//! segment between pieces (both sides' records hold one).
#![allow(
    clippy::print_stdout,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::too_many_lines,
    clippy::iter_over_hash_type,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

use std::collections::{BTreeMap, HashMap};
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

/// Cell edges along a segment: its Manhattan length in pitches (at least 1).
fn edges(a: [f32; 3], b: [f32; 3], p: f32) -> i64 {
    ((((a[0] - b[0]).abs() + (a[2] - b[2]).abs()) / p).round() as i64).max(1)
}

/// The in-place size of one area. `v`: C border portals counted as outline segments (bit 0 clear: as portal
/// segments when bit 1 set, else not at all); bit 2: portal segments counted per cell edge; bit 3: outline / D
/// segments counted per cell edge (the unsimplified chains).
#[must_use]
fn inplace(a: &kymesh::Area, v: u8) -> i64 {
    let p = a.values[1];
    let cells_o = v & 8 != 0;
    let cells_p = v & 4 != 0;
    let chains = |l: &[Vec<[f32; 3]>]| -> (i64, i64) {
        let n = l.iter().filter(|c| c.len() >= 2).count() as i64;
        let segs: i64 = l
            .iter()
            .map(|c| {
                if cells_o {
                    c.windows(2).map(|w| edges(w[0], w[1], p)).sum::<i64>()
                } else {
                    c.len().saturating_sub(1) as i64
                }
            })
            .sum();
        (n, segs)
    };
    let (nd, sd) = chains(&a.d_lines);
    let (mut ne, mut se) = chains(&a.outline);
    let seg = |p0: [f32; 3], p1: [f32; 3]| if cells_p { edges(p0, p1, p) } else { 1 };
    let mut portal: i64 = a.portals.iter().map(|q| seg(q.p0, q.p1)).sum();
    let c: i64 = a.border_portals.iter().map(|q| seg(q.p0, q.p1)).sum();
    match v & 3 {
        0 => {
            se += c;
            ne += i64::from(c > 0);
        }
        1 => portal += c,
        _ => {}
    }
    let nf = a.cells.len() as i64;
    let sf: i64 = a.cells.iter().map(|x| x.points.len().saturating_sub(1) as i64).sum();
    100 + 28 * portal + 20 * (nd + ne) + 12 * (sd + se) + 32 * nf + 12 * sf
}

/// KYG-AREA-4's cut line of two areas' union along coordinate `k` (0 x, 2 z): the cells' integer box (sample
/// points at whole pitches from the record's rectangle, the boundary half a pitch outside them), mid = (min + max)
/// >> 1, the line between cell mid and mid + 1.
fn box_line(m: &kymesh::Mesh, a: &kymesh::Area, b: &kymesh::Area, k: usize) -> f32 {
    let p = m.pitch;
    let o = m.rect[if k == 0 { 0 } else { 2 }];
    let (mut lo, mut hi) = (f32::MAX, f32::MIN);
    for ar in [a, b] {
        let pts = ar
            .outline
            .iter()
            .flatten()
            .copied()
            .chain(ar.portals.iter().flat_map(|q| [q.p0, q.p1]))
            .chain(ar.border_portals.iter().flat_map(|q| [q.p0, q.p1]));
        for v in pts {
            lo = lo.min(v[k]);
            hi = hi.max(v[k]);
        }
    }
    let i0 = ((lo + p / 2.0 - o) / p).round() as i64;
    let i1 = ((hi - p / 2.0 - o) / p).round() as i64;
    let mid = (i0 + i1) >> 1;
    o + (mid as f32 + 0.5) * p
}

fn near(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
}

fn find(p: &mut [usize], x: usize) -> usize {
    let mut r = x;
    while p[r] != r {
        r = p[r];
    }
    let mut y = x;
    while p[y] != r {
        let n = p[y];
        p[y] = r;
        y = n;
    }
    r
}

#[derive(Default)]
struct Stats {
    /// class -> variant -> (sizes of uncut areas, merged sizes of cut groups)
    by: BTreeMap<(String, u8), (Vec<i64>, Vec<i64>)>,
}

fn class(m: &kymesh::Mesh, vehicle: bool) -> &'static str {
    if vehicle {
        "veh"
    } else if (m.pitch - 0.2).abs() < 1e-4 {
        "det"
    } else {
        "coarse"
    }
}

fn mesh(m: &kymesh::Mesh, cl: &str, boxmid: bool, st: &mut Stats) {
    let p = m.pitch;
    let t = 1.0 / m.resolution;
    let n = m.areas.len();
    let mut parent: Vec<usize> = (0..n).collect();
    let mut internal: HashMap<usize, i64> = HashMap::new();
    let mut internal_cells: HashMap<usize, i64> = HashMap::new();
    let mid = |a: &kymesh::Area, axis: usize| -> f32 {
        let lo = m.rect[if axis == 0 { 0 } else { 2 }] + a.ids[if axis == 0 { 1 } else { 2 }] as f32 * t - p / 2.0;
        lo + t / 2.0
    };
    for (ai, a) in m.areas.iter().enumerate() {
        for b in &a.portals {
            let tg = &m.areas[b.target];
            if tg.ids[1] != a.ids[1] || tg.ids[2] != a.ids[2] {
                continue;
            }
            for axis in [0usize, 1] {
                let k = if axis == 0 { 0 } else { 2 };
                let line = if boxmid { box_line(m, a, tg, k) } else { mid(a, axis) };
                if near(b.p0[k], b.p1[k], 1e-3) && near(b.p0[k], line, 0.01) {
                    let r = find(&mut parent, ai);
                    let q = find(&mut parent, b.target);
                    parent[r] = q;
                    *internal.entry(ai).or_default() += 1;
                    *internal_cells.entry(ai).or_default() += edges(b.p0, b.p1, p);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    for idx in groups.values() {
        for v in [0u8, 1, 2, 4, 5, 8, 12, 13] {
            let e = st
                .by
                .entry((format!("{cl}{}", if boxmid { "-box" } else { "" }), v))
                .or_default();
            if idx.len() == 1 {
                e.0.push(inplace(&m.areas[idx[0]], v));
            } else {
                let sum: i64 = idx.iter().map(|&i| inplace(&m.areas[i], v)).sum();
                let map = if v & 4 != 0 { &internal_cells } else { &internal };
                let inner: i64 = idx.iter().map(|&i| map.get(&i).copied().unwrap_or(0)).sum();
                e.1.push(sum - 100 * (idx.len() as i64 - 1) - 28 * inner);
            }
        }
    }
}

fn pct(v: &[i64], q: f64) -> i64 {
    if v.is_empty() {
        return 0;
    }
    v[((v.len() - 1) as f64 * q).round() as usize]
}

fn main() {
    let mut levels: Vec<_> = std::fs::read_dir(levels_dir())
        .expect("levels folder")
        .flatten()
        .collect();
    levels.sort_by_key(std::fs::DirEntry::file_name);
    let mut st = Stats::default();
    let mut files = 0;
    for l in levels {
        for (kind, veh) in [("infantry", false), ("vehicle", true)] {
            let path = l.path().join(kind).join("pathdata.hpd");
            let Ok(data) = std::fs::read(&path) else { continue };
            let f = hpd::parse(&data).expect("path data");
            files += 1;
            for s in &f.sectors {
                let m = kymesh::parse(s.mesh).expect("mesh");
                mesh(&m, class(&m, veh), false, &mut st);
                mesh(&m, class(&m, veh), true, &mut st);
            }
        }
    }
    println!("{files} files");
    for ((cl, v), (uncut, cut)) in &mut st.by {
        uncut.sort_unstable();
        cut.sort_unstable();
        let over = uncut.iter().filter(|&&x| x >= 8192).count();
        let under = cut.iter().filter(|&&x| x < 8192).count();
        println!(
            "{cl} C-as-{v}: uncut {} (max {}, p99 {}, p999 {}, >= 8192: {over}); cut groups {} (min {}, p01 {}, p10 {}, \
             p50 {}, < 8192: {under})",
            uncut.len(),
            uncut.last().copied().unwrap_or(0),
            pct(uncut, 0.99),
            pct(uncut, 0.999),
            cut.len(),
            cut.first().copied().unwrap_or(0),
            pct(cut, 0.01),
            pct(cut, 0.10),
            pct(cut, 0.5)
        );
        for th in [4096, 6144, 7168, 8192, 9216, 10240, 12288] {
            let a = uncut.iter().filter(|&&x| x >= th).count();
            let b = cut.iter().filter(|&&x| x < th).count();
            println!("    threshold {th}: uncut at or over {a}, cut groups under {b}");
        }
    }
}
