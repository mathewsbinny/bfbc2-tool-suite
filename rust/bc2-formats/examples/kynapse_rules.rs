//! Round NKS-rest2's fits of DICE's Kynapse generator rules over DICE's path data alone
//! (`spec/transforms/kynapse_generation.md`; the fits against the levels' geometry are `bc2-scene`'s
//! `kynapse_geom`): read-only, counts and histograms only, no content printed.
//!
//!   cargo run --release -p bc2-formats --example kynapse_rules -- [--only <fit>] [--level <name>] [<levels folder>]
//!
//! Fits: `cover` (KYG-NODE-5: how far each sampled point of the ground is from a node it sees; KYG-NODE-1: the centroid
//! of each node's share), `spacing` (KYG-NODE-4: the nearest other node, nodes sharing a coordinate, the half-step
//! lattice), `midline` and `holes` (KYG-AREA-2: the crossings of a midline and what lies between them, holes across a
//! midline), `pairs` (KYG-EDGE-3: the pairs the 1.2-spanner leaves unexplained against a cell-level reach test, step
//! pairs and degrees). The folder defaults to
//! Project OPFOR BFBC2's `game\Output\win32\ai\levels` (`OPFOR_ROOT`, else `project-opfor-bfbc2` beside the suite).
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::cast_lossless,
    clippy::type_complexity,
    clippy::iter_over_hash_type,
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

use std::collections::{BTreeMap, HashMap};
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

type P2 = [f64; 2];

fn xz(v: &[f32; 3]) -> P2 {
    [f64::from(v[0]), f64::from(v[2])]
}

fn sub(a: P2, b: P2) -> P2 {
    [a[0] - b[0], a[1] - b[1]]
}

fn len(a: P2) -> f64 {
    a[0].hypot(a[1])
}

fn cross(a: P2, b: P2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

/// Proper or touching intersection of segments ab and cd (2-D).
fn seg_hit(a: P2, b: P2, c: P2, d: P2) -> bool {
    let r = sub(b, a);
    let s = sub(d, c);
    let den = cross(r, s);
    if den.abs() < 1e-12 {
        return false;
    }
    let qp = sub(c, a);
    let t = cross(qp, s) / den;
    let u = cross(qp, r) / den;
    t > 1e-6 && t < 1.0 - 1e-6 && (-1e-6..=1.0 + 1e-6).contains(&u)
}

struct Sec {
    nodes: Vec<hpd::Node>,
    edges: Vec<hpd::Edge>,
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
            let secs: Vec<Sec> = f
                .sectors
                .iter()
                .map(|s| Sec {
                    nodes: s.nodes.clone(),
                    edges: s.edges.clone(),
                    mesh: kymesh::parse(s.mesh).expect("mesh"),
                })
                .collect();
            for s in &secs {
                let cl = class(&s.mesh, veh);
                if run("cover") {
                    fit_cover(s, cl, &mut c);
                }
                if run("spacing") {
                    fit_spacing(s, cl, &mut c);
                }
                if run("midline") {
                    fit_midline(s, cl, &mut c);
                }
                if run("holes") {
                    fit_holes(s, cl, &mut c);
                }
                if run("pairs") {
                    fit_pairs(s, cl, &mut c);
                }
            }
        }
    }
    println!("--- totals");
    for (k, v) in &c.0 {
        println!("{k} {v}");
    }
}

/// Outline segments (x, z ends, height) on a grid of `cell` m.
struct Segs {
    s: Vec<(P2, P2, f32)>,
    grid: HashMap<(i64, i64), Vec<usize>>,
    cell: f64,
}

impl Segs {
    fn of(m: &kymesh::Mesh, with_d: bool, cell: f64) -> Self {
        let mut s: Vec<(P2, P2, f32)> = Vec::new();
        for a in &m.areas {
            let lines = a.outline.iter().chain(a.d_lines.iter().filter(|_| with_d));
            for line in lines {
                for w in line.windows(2) {
                    s.push((xz(&w[0]), xz(&w[1]), w[0][1].min(w[1][1])));
                }
            }
        }
        let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
        for (i, sg) in s.iter().enumerate() {
            let (x0, x1) = (sg.0[0].min(sg.1[0]), sg.0[0].max(sg.1[0]));
            let (z0, z1) = (sg.0[1].min(sg.1[1]), sg.0[1].max(sg.1[1]));
            for gx in (x0 / cell).floor() as i64..=(x1 / cell).floor() as i64 {
                for gz in (z0 / cell).floor() as i64..=(z1 / cell).floor() as i64 {
                    grid.entry((gx, gz)).or_default().push(i);
                }
            }
        }
        Self { s, grid, cell }
    }

    /// Whether the straight line ab crosses a segment whose height is in [ylo, yhi].
    fn blocked(&self, a: P2, b: P2, ylo: f32, yhi: f32) -> bool {
        let d = len(sub(b, a));
        let steps = (d / (self.cell / 2.0)).ceil().max(1.0) as usize;
        let mut last = (i64::MIN, i64::MIN);
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let q = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
            let (gx, gz) = ((q[0] / self.cell).floor() as i64, (q[1] / self.cell).floor() as i64);
            if (gx, gz) == last {
                continue;
            }
            last = (gx, gz);
            for dx in -1..=1 {
                for dz in -1..=1 {
                    if let Some(list) = self.grid.get(&(gx + dx, gz + dz)) {
                        for &si in list {
                            let (s0, s1, y) = self.s[si];
                            if y < ylo || y > yhi {
                                continue;
                            }
                            if seg_hit(a, b, s0, s1) {
                                return true;
                            }
                        }
                    }
                }
            }
        }
        false
    }
}

/// The sample points (whole multiples of `step`) inside the area's E + B + C boundary by the even-odd rule.
fn area_points(a: &kymesh::Area, step: f64) -> Vec<P2> {
    let mut segs: Vec<(P2, P2)> = Vec::new();
    for l in &a.outline {
        for w in l.windows(2) {
            segs.push((xz(&w[0]), xz(&w[1])));
        }
    }
    for b in &a.portals {
        segs.push((xz(&b.p0), xz(&b.p1)));
    }
    for b in &a.border_portals {
        segs.push((xz(&b.p0), xz(&b.p1)));
    }
    let mut out = Vec::new();
    if segs.is_empty() {
        return out;
    }
    let z0 = segs.iter().map(|s| s.0[1].min(s.1[1])).fold(f64::MAX, f64::min);
    let z1 = segs.iter().map(|s| s.0[1].max(s.1[1])).fold(f64::MIN, f64::max);
    let mut xs: Vec<f64> = Vec::new();
    for k in (z0 / step).ceil() as i64..=(z1 / step).floor() as i64 {
        let z = k as f64 * step;
        xs.clear();
        for (s0, s1) in &segs {
            let (lo, hi) = if s0[1] < s1[1] { (s0, s1) } else { (s1, s0) };
            if z >= lo[1] && z < hi[1] {
                xs.push(lo[0] + (hi[0] - lo[0]) * (z - lo[1]) / (hi[1] - lo[1]));
            }
        }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for w in xs.chunks(2) {
            if w.len() == 2 {
                for i in (w[0] / step).ceil() as i64..=(w[1] / step).floor() as i64 {
                    out.push([i as f64 * step, z]);
                }
            }
        }
    }
    out
}

fn yoff(cl: &str) -> f32 {
    if cl == "veh" { 2.0 } else { 0.95 }
}

fn dmax(cl: &str) -> f64 {
    if cl == "veh" { 50.0 } else { 15.0 }
}

// ---------------------------------------------------------------- KYG-NODE-1: coverage

/// For every sampled point of the ground (0.8 m apart; 1 m on vehicle files), the distance to the nearest node of the
/// sector at the area's height that it sees (no outline between), and to the nearest at all.
fn fit_cover(s: &Sec, cl: &str, c: &mut C) {
    let m = &s.mesh;
    let segs = Segs::of(m, false, 4.0);
    let step = if cl == "veh" { 1.0 } else { 0.8 };
    let ng = 10.0f64;
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, n) in s.nodes.iter().enumerate() {
        grid.entry((
            (f64::from(n.pos[0]) / ng).floor() as i64,
            (f64::from(n.pos[2]) / ng).floor() as i64,
        ))
        .or_default()
        .push(i);
    }
    let reach = 3i64;
    let mut cand: Vec<(f64, usize)> = Vec::new();
    let mut worst = 0.0f64;
    // Each node's share of the ground (the points whose nearest seen node it is): their count and sum.
    let mut share: Vec<(usize, P2)> = vec![(0, [0.0, 0.0]); s.nodes.len()];
    for a in &m.areas {
        let ay = a
            .outline
            .first()
            .and_then(|l| l.first())
            .map_or(a.values[3] + yoff(cl), |v| v[1]);
        let (glo, ghi) = (a.values[3] - 0.5, a.values[2]);
        for p in area_points(a, step) {
            cand.clear();
            let (gx, gz) = ((p[0] / ng).floor() as i64, (p[1] / ng).floor() as i64);
            for dx in -reach..=reach {
                for dz in -reach..=reach {
                    if let Some(list) = grid.get(&(gx + dx, gz + dz)) {
                        for &i in list {
                            let n = &s.nodes[i];
                            let g = n.pos[1] - yoff(cl);
                            if g < glo || g > ghi {
                                continue;
                            }
                            cand.push((len(sub(xz(&n.pos), p)), i));
                        }
                    }
                }
            }
            cand.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            let any = cand.first().map_or(f64::MAX, |x| x.0);
            let mut seen = f64::MAX;
            for &(d, i) in &cand {
                let n = &s.nodes[i];
                let (ylo, yhi) = (ay.min(n.pos[1]) - 2.0, ay.max(n.pos[1]) + 1.0);
                if !segs.blocked(p, xz(&n.pos), ylo, yhi) {
                    seen = d;
                    share[i].0 += 1;
                    share[i].1 = [share[i].1[0] + p[0], share[i].1[1] + p[1]];
                    break;
                }
            }
            let b = |d: f64| {
                if d == f64::MAX {
                    "none".to_owned()
                } else {
                    format!("{:03}", (d * 2.0).floor().min(80.0) as i64)
                }
            };
            c.inc(&format!("cover.{cl}.seen_halfm_{}", b(seen)));
            c.inc(&format!("cover.{cl}.any_halfm_{}", b(any)));
            let sq = square(m);
            if p[0] >= sq[0] && p[0] < sq[0] + 100.0 && p[1] >= sq[1] && p[1] < sq[1] + 100.0 {
                c.inc(&format!("cover.{cl}.square_seen_halfm_{}", b(seen)));
            }
            if seen != f64::MAX {
                worst = worst.max(seen);
            }
        }
    }
    c.inc(&format!("cover.{cl}.sector_worst_m_{:03}", worst.floor() as i64));
    // Centroidal test: an ordinary node well inside the sector's square against the centroid of its share.
    let side = 100.0f64;
    let sq = square(m);
    for (i, n) in s.nodes.iter().enumerate() {
        if n.id & 0x8000_0000 != 0 {
            continue;
        }
        let p = xz(&n.pos);
        let inner =
            p[0] > sq[0] + 10.0 && p[0] < sq[0] + side - 10.0 && p[1] > sq[1] + 10.0 && p[1] < sq[1] + side - 10.0;
        if !inner {
            continue;
        }
        let (k, sum) = share[i];
        if k == 0 {
            c.inc(&format!("centroid.{cl}.no_share"));
            continue;
        }
        let cen = [sum[0] / k as f64, sum[1] / k as f64];
        let d = len(sub(cen, p));
        c.inc(&format!(
            "centroid.{cl}.dist_dm_{:03}",
            (d * 10.0).floor().min(60.0) as i64
        ));
    }
}

// ---------------------------------------------------------------- KYG-AREA-2: what lies between the two stretches

/// Even-odd inside test on the area's E + B + C segments.
fn inside(a: &kymesh::Area, p: P2) -> bool {
    let mut odd = false;
    let mut test = |s0: P2, s1: P2| {
        if (s0[1] > p[1]) != (s1[1] > p[1]) {
            let x = s0[0] + (s1[0] - s0[0]) * (p[1] - s0[1]) / (s1[1] - s0[1]);
            if x > p[0] {
                odd = !odd;
            }
        }
    };
    for l in &a.outline {
        for w in l.windows(2) {
            test(xz(&w[0]), xz(&w[1]));
        }
    }
    for b in &a.portals {
        test(xz(&b.p0), xz(&b.p1));
    }
    for b in &a.border_portals {
        test(xz(&b.p0), xz(&b.p1));
    }
    odd
}

fn find(p: &mut [usize], x: usize) -> usize {
    let mut r = x;
    while p[r] != r {
        r = p[r];
    }
    p[x] = r;
    r
}

/// Area boxes (min x, max x, min z, max z) over E, B and C.
fn area_box(a: &kymesh::Area) -> [f64; 4] {
    let mut bb = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    let mut add = |v: &[f32; 3]| {
        bb = [
            bb[0].min(f64::from(v[0])),
            bb[1].max(f64::from(v[0])),
            bb[2].min(f64::from(v[2])),
            bb[3].max(f64::from(v[2])),
        ];
    };
    for v in a.outline.iter().flatten() {
        add(v);
    }
    for b in &a.portals {
        add(&b.p0);
        add(&b.p1);
    }
    for b in &a.border_portals {
        add(&b.p0);
        add(&b.p1);
    }
    bb
}

/// Along each tile midline an area group crosses (a cut group: across its midline portals; an uncut area: on both
/// sides), the gaps between the stretches where it crosses, and what fills each gap: the ground of another area
/// at a height near the group's ("ground"), or no area at all ("none").
fn fit_midline(s: &Sec, cl: &str, c: &mut C) {
    let m = &s.mesh;
    let p = f64::from(m.pitch);
    let t = 1.0 / f64::from(m.resolution);
    let n = m.areas.len();
    let boxes: Vec<[f64; 4]> = m.areas.iter().map(area_box).collect();
    let mid = |a: &kymesh::Area, axis: usize| -> f64 {
        let lo = f64::from(m.rect[if axis == 0 { 0 } else { 2 }]) + f64::from(a.ids[if axis == 0 { 1 } else { 2 }]) * t
            - p / 2.0;
        lo + t / 2.0
    };
    let mut parent: Vec<usize> = (0..n).collect();
    let mut cut_axis: HashMap<usize, usize> = HashMap::new();
    for (ai, a) in m.areas.iter().enumerate() {
        for b in &a.portals {
            let tg = &m.areas[b.target];
            if tg.ids[1] != a.ids[1] || tg.ids[2] != a.ids[2] {
                continue;
            }
            for axis in [0usize, 1] {
                let k = if axis == 0 { 0 } else { 2 };
                if (b.p0[k] - b.p1[k]).abs() < 1e-3 && (f64::from(b.p0[k]) - mid(a, axis)).abs() < 0.01 {
                    let r = find(&mut parent, ai);
                    let q = find(&mut parent, b.target);
                    parent[r] = q;
                    cut_axis.insert(ai, axis);
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut roots: Vec<usize> = groups.keys().copied().collect();
    roots.sort_unstable();
    for r in roots {
        let idx = &groups[&r];
        let a0 = &m.areas[idx[0]];
        let cut = idx.len() > 1;
        let axes: Vec<usize> = if cut {
            idx.iter().filter_map(|i| cut_axis.get(i)).copied().take(1).collect()
        } else {
            vec![0, 1]
        };
        let (ylo, yhi) = (
            idx.iter().map(|&i| m.areas[i].values[3]).fold(f32::MAX, f32::min),
            idx.iter().map(|&i| m.areas[i].values[2]).fold(f32::MIN, f32::max),
        );
        let tile = (a0.ids[1], a0.ids[2]);
        for axis in axes {
            let mv = mid(a0, axis);
            // The tile's span along the midline.
            let o0 = f64::from(m.rect[if axis == 0 { 2 } else { 0 }])
                + f64::from(if axis == 0 { tile.1 } else { tile.0 }) * t
                - p / 2.0;
            let steps = (t / (p / 2.0)).round() as usize;
            let here = |q: f64, side: f64| -> bool {
                let pt = if axis == 0 { [mv + side, q] } else { [q, mv + side] };
                idx.iter().any(|&i| {
                    let bb = boxes[i];
                    pt[0] >= bb[0] && pt[0] <= bb[1] && pt[1] >= bb[2] && pt[1] <= bb[3] && inside(&m.areas[i], pt)
                })
            };
            let mut runs: Vec<(usize, usize)> = Vec::new();
            let mut start = None;
            for k in 0..=steps {
                let q = o0 + (k as f64 + 0.5) * (p / 2.0);
                let on = k < steps && here(q, -p / 4.0) && here(q, p / 4.0);
                match (on, start) {
                    (true, None) => start = Some(k),
                    (false, Some(s0)) => {
                        runs.push((s0, k));
                        start = None;
                    }
                    _ => {}
                }
            }
            if runs.is_empty() {
                continue;
            }
            let kind = if cut { "cut" } else { "uncut" };
            c.inc(&format!("mid2.{cl}.{kind}_crossing_{}", runs.len().min(5)));
            for w in runs.windows(2) {
                // The gap between two stretches: what fills its middle.
                let k = usize::midpoint(w[0].1, w[1].0);
                let q = o0 + (k as f64 + 0.5) * (p / 2.0);
                let pt = if axis == 0 { [mv, q] } else { [q, mv] };
                let mut fill = "none";
                for (j, b) in m.areas.iter().enumerate() {
                    if idx.contains(&j) {
                        continue;
                    }
                    let bb = boxes[j];
                    if pt[0] < bb[0] || pt[0] > bb[1] || pt[1] < bb[2] || pt[1] > bb[3] {
                        continue;
                    }
                    if b.values[3] > yhi + 0.5 || b.values[2] < ylo - 0.5 {
                        continue;
                    }
                    if inside(b, pt) {
                        fill = if (b.ids[1], b.ids[2]) == tile {
                            "ground_same_tile"
                        } else {
                            "ground_other_tile"
                        };
                        break;
                    }
                }
                let width = (w[1].0 - w[0].1) as f64 * p / 2.0;
                c.inc(&format!("mid2.{cl}.{kind}.gap_{fill}"));
                c.inc(&format!(
                    "mid2.{cl}.{kind}.gap_{fill}_width_{}",
                    if width < 2.0 {
                        "lt2m"
                    } else if width < 5.0 {
                        "2_5m"
                    } else {
                        "ge5m"
                    }
                ));
            }
        }
    }
}

/// The holes of a set of areas taken together: the sample points (multiples of the pitch) in their joined box that
/// none of them holds and that cannot be reached from the box's border, as 4-connected components; for each, whether
/// it has points on both sides of the x midline `mx` and of the z midline `mz`.
fn holes_of(m: &kymesh::Mesh, idx: &[usize], boxes: &[[f64; 4]], mx: f64, mz: f64) -> Vec<(bool, bool, usize)> {
    let p = f64::from(m.pitch);
    let mut bb = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
    for &i in idx {
        let b = boxes[i];
        bb = [bb[0].min(b[0]), bb[1].max(b[1]), bb[2].min(b[2]), bb[3].max(b[3])];
    }
    let i0 = (bb[0] / p).floor() as i64 - 1;
    let i1 = (bb[1] / p).ceil() as i64 + 1;
    let j0 = (bb[2] / p).floor() as i64 - 1;
    let j1 = (bb[3] / p).ceil() as i64 + 1;
    let (w, h) = ((i1 - i0 + 1) as usize, (j1 - j0 + 1) as usize);
    if w * h > 4_000_000 {
        return Vec::new();
    }
    let mut full = vec![false; w * h];
    for j in 0..h {
        for i in 0..w {
            let pt = [(i0 + i as i64) as f64 * p, (j0 + j as i64) as f64 * p];
            full[j * w + i] = idx.iter().any(|&k| {
                let b = boxes[k];
                pt[0] >= b[0] && pt[0] <= b[1] && pt[1] >= b[2] && pt[1] <= b[3] && inside(&m.areas[k], pt)
            });
        }
    }
    // 0 unknown, 1 outside (reached from the border), 2 hole.
    let mut mark = vec![0u8; w * h];
    let mut stack: Vec<usize> = Vec::new();
    for j in 0..h {
        for i in 0..w {
            if (i == 0 || j == 0 || i == w - 1 || j == h - 1) && !full[j * w + i] {
                mark[j * w + i] = 1;
                stack.push(j * w + i);
            }
        }
    }
    let nb = |k: usize| -> [Option<usize>; 4] {
        let (i, j) = (k % w, k / w);
        [
            (i > 0).then(|| k - 1),
            (i + 1 < w).then(|| k + 1),
            (j > 0).then(|| k - w),
            (j + 1 < h).then(|| k + w),
        ]
    };
    while let Some(k) = stack.pop() {
        for q in nb(k).into_iter().flatten() {
            if !full[q] && mark[q] == 0 {
                mark[q] = 1;
                stack.push(q);
            }
        }
    }
    let mut out = Vec::new();
    for k0 in 0..w * h {
        if full[k0] || mark[k0] != 0 {
            continue;
        }
        mark[k0] = 2;
        stack.push(k0);
        let (mut lx, mut hx, mut lz, mut hz) = (false, false, false, false);
        let mut n = 0;
        let mut filled = 0;
        while let Some(k) = stack.pop() {
            n += 1;
            let x = (i0 + (k % w) as i64) as f64 * p;
            let z = (j0 + (k / w) as i64) as f64 * p;
            if m.areas.iter().enumerate().any(|(q, a)| {
                let b = boxes[q];
                !idx.contains(&q) && x >= b[0] && x <= b[1] && z >= b[2] && z <= b[3] && inside(a, [x, z])
            }) {
                filled += 1;
            }
            lx |= x < mx;
            hx |= x > mx;
            lz |= z < mz;
            hz |= z > mz;
            for q in nb(k).into_iter().flatten() {
                if !full[q] && mark[q] == 0 {
                    mark[q] = 2;
                    stack.push(q);
                }
            }
        }
        out.push((
            lx && hx,
            lz && hz,
            if filled == 0 {
                0
            } else if filled == n {
                2
            } else {
                1
            },
        ));
    }
    out
}

/// KYG-AREA-2: every area's holes, and each cut group's holes once joined: do they cross a tile midline?
fn fit_holes(s: &Sec, cl: &str, c: &mut C) {
    let m = &s.mesh;
    let p = f64::from(m.pitch);
    let t = 1.0 / f64::from(m.resolution);
    let n = m.areas.len();
    let boxes: Vec<[f64; 4]> = m.areas.iter().map(area_box).collect();
    let mid = |a: &kymesh::Area, axis: usize| -> f64 {
        let lo = f64::from(m.rect[if axis == 0 { 0 } else { 2 }]) + f64::from(a.ids[if axis == 0 { 1 } else { 2 }]) * t
            - p / 2.0;
        lo + t / 2.0
    };
    let mut parent: Vec<usize> = (0..n).collect();
    for (ai, a) in m.areas.iter().enumerate() {
        for b in &a.portals {
            let tg = &m.areas[b.target];
            if tg.ids[1] != a.ids[1] || tg.ids[2] != a.ids[2] {
                continue;
            }
            for axis in [0usize, 1] {
                let k = if axis == 0 { 0 } else { 2 };
                if (b.p0[k] - b.p1[k]).abs() < 1e-3 && (f64::from(b.p0[k]) - mid(a, axis)).abs() < 0.01 {
                    let r = find(&mut parent, ai);
                    let q = find(&mut parent, b.target);
                    parent[r] = q;
                }
            }
        }
    }
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let mut roots: Vec<usize> = groups.keys().copied().collect();
    roots.sort_unstable();
    for r in roots {
        let idx = &groups[&r];
        let a0 = &m.areas[idx[0]];
        let (mx, mz) = (mid(a0, 0), mid(a0, 1));
        // Each area alone.
        for &i in idx {
            for (hx, hz, fill) in holes_of(m, &[i], &boxes, mx, mz) {
                let k = match (hx, hz) {
                    (false, false) => "in_a_quarter",
                    (true, false) => "across_x_mid",
                    (false, true) => "across_z_mid",
                    (true, true) => "across_both",
                };
                let f = ["void", "part_ground", "ground"][fill];
                c.inc(&format!("holes.{cl}.area_hole_{k}"));
                c.inc(&format!("holes.{cl}.area_hole_{k}.{f}"));
            }
        }
        if idx.len() > 1 {
            c.inc(&format!("holes.{cl}.cut_groups"));
            let hs = holes_of(m, idx, &boxes, mx, mz);
            let crossing = hs.iter().filter(|h| h.0 || h.1).count();
            for h in hs.iter().filter(|h| h.0 || h.1) {
                c.inc(&format!(
                    "holes.{cl}.cut_group_hole_across.{}",
                    ["void", "part_ground", "ground"][h.2]
                ));
            }
            c.inc(&format!(
                "holes.{cl}.cut_group_joined_{}",
                if crossing > 0 {
                    "has_a_hole_across_a_midline"
                } else if hs.is_empty() {
                    "no_hole"
                } else {
                    "holes_in_quarters_only"
                }
            ));
        }
    }
}

// ---------------------------------------------------------------- KYG-EDGE-3: the pairs the spanner leaves unexplained

/// Ordinary pairs within DistEdgeMax (and 3 m in height) that see each other (no outline or D line between): the
/// edges, and the pairs with no edge and no route of at most 1.2 times their length (the spanner's misses), by
/// length, height step, how many areas and tiles the line passes, and the endpoints' degrees.
fn fit_pairs(s: &Sec, cl: &str, c: &mut C) {
    let m = &s.mesh;
    let nodes = &s.nodes;
    let n = nodes.len();
    let dm = dmax(cl);
    let segs = Segs::of(m, true, 4.0);
    let mut has: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &s.edges {
        let (a, b) = (e.src as usize, e.dst as usize);
        has.insert((a.min(b), a.max(b)));
        if !adj[a].contains(&b) {
            adj[a].push(b);
        }
        if !adj[b].contains(&a) {
            adj[b].push(a);
        }
    }
    let d3 = |i: usize, j: usize| -> f64 {
        let (a, b) = (&nodes[i].pos, &nodes[j].pos);
        let v = [f64::from(a[0] - b[0]), f64::from(a[1] - b[1]), f64::from(a[2] - b[2])];
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    };
    let route = |i: usize, j: usize, limit: f64| -> f64 {
        let mut best: HashMap<usize, f64> = HashMap::new();
        let mut heap = std::collections::BinaryHeap::new();
        best.insert(i, 0.0);
        heap.push((std::cmp::Reverse(0u64), i));
        while let Some((std::cmp::Reverse(k), u)) = heap.pop() {
            let du = k as f64 / 1e6;
            if du > best.get(&u).copied().unwrap_or(f64::MAX) + 1e-9 {
                continue;
            }
            if u == j {
                return du;
            }
            for &v in &adj[u] {
                if (u == i && v == j) || (u == j && v == i) {
                    continue;
                }
                let dv = du + d3(u, v);
                if dv <= limit && dv < best.get(&v).copied().unwrap_or(f64::MAX) {
                    best.insert(v, dv);
                    heap.push((std::cmp::Reverse((dv * 1e6) as u64), v));
                }
            }
        }
        f64::MAX
    };
    let tsz = 1.0 / f64::from(m.resolution);
    let tile_of = |p: P2| -> (i64, i64) {
        (
            ((p[0] - f64::from(m.rect[0])) / tsz).floor() as i64,
            ((p[1] - f64::from(m.rect[2])) / tsz).floor() as i64,
        )
    };
    let boxes: Vec<[f64; 4]> = m.areas.iter().map(area_box).collect();
    let mut by_tile: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
    for (ai, a) in m.areas.iter().enumerate() {
        by_tile.entry((a.ids[1], a.ids[2])).or_default().push(ai);
    }
    let area_at = |p: P2, g: f32| -> Option<usize> {
        let (tx, tz) = tile_of(p);
        for dx in [0i64, -1, 1] {
            for dz in [0i64, -1, 1] {
                let (kx, kz) = (tx + dx, tz + dz);
                if kx < 0 || kz < 0 {
                    continue;
                }
                let Some(list) = by_tile.get(&(kx as u32, kz as u32)) else {
                    continue;
                };
                for &ai in list {
                    let b = boxes[ai];
                    if p[0] < b[0] || p[0] > b[1] || p[1] < b[2] || p[1] > b[3] {
                        continue;
                    }
                    let a = &m.areas[ai];
                    if g < a.values[3] - 0.7 || g > a.values[2] + 0.7 {
                        continue;
                    }
                    if inside(a, p) {
                        return Some(ai);
                    }
                }
            }
        }
        None
    };
    let deg: Vec<usize> = adj.iter().map(Vec::len).collect();
    // Step-pair nodes: ordinary nodes one step (GraphAccuracy + 0.35 m) from another along x or z, sharing the other
    // coordinate exactly.
    let step = match cl {
        "veh" => 15.35f32,
        "det" => 0.85,
        _ => 3.35,
    };
    let mut stepper = vec![false; n];
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (&nodes[i].pos, &nodes[j].pos);
            let pair = (a[0] == b[0] && ((a[2] - b[2]).abs() - step).abs() < 0.002)
                || (a[2] == b[2] && ((a[0] - b[0]).abs() - step).abs() < 0.002);
            if pair {
                stepper[i] = true;
                stepper[j] = true;
            }
        }
    }
    for i in 0..n {
        if nodes[i].id & 0x8000_0000 != 0 {
            continue;
        }
        for j in i + 1..n {
            if nodes[j].id & 0x8000_0000 != 0 {
                continue;
            }
            let (pa, pb) = (xz(&nodes[i].pos), xz(&nodes[j].pos));
            let d = len(sub(pb, pa));
            if d > dm || (nodes[i].pos[1] - nodes[j].pos[1]).abs() > 3.0 {
                continue;
            }
            let edge = has.contains(&(i, j));
            let (ya, yb) = (nodes[i].pos[1], nodes[j].pos[1]);
            if segs.blocked(pa, pb, ya.min(yb) - 2.0, ya.max(yb) + 1.0) {
                continue;
            }
            let kind = if edge {
                "edge"
            } else {
                let dd = d3(i, j);
                if route(i, j, dd * 1.2) <= dd * 1.2 {
                    continue;
                }
                "miss"
            };
            // The line's areas and tiles (every 0.4 m), and whether it stays on the mesh.
            let steps = (d / 0.4).ceil().max(1.0) as usize;
            let mut areas: Vec<usize> = Vec::new();
            let mut tiles: Vec<(i64, i64)> = Vec::new();
            let mut off = 0usize;
            for k in 0..=steps {
                let t = k as f64 / steps as f64;
                let q = [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t];
                let g = ya + (yb - ya) * t as f32 - yoff(cl);
                match area_at(q, g) {
                    Some(ai) => {
                        if areas.last() != Some(&ai) {
                            areas.push(ai);
                        }
                    }
                    None => off += 1,
                }
                let tl = tile_of(q);
                if tiles.last() != Some(&tl) {
                    tiles.push(tl);
                }
            }
            // The cell test: every sample cell (pitch-sized, centred on a sample point) the line passes over holds
            // ground of an area at the line's height.
            let p = f64::from(m.pitch);
            let fine = (d / (p / 10.0)).ceil().max(1.0) as usize;
            let mut last = (i64::MIN, i64::MIN);
            let mut bad = 0usize;
            for k in 0..=fine {
                let t = k as f64 / fine as f64;
                let q = [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t];
                let cell = ((q[0] / p).round() as i64, (q[1] / p).round() as i64);
                if cell == last {
                    continue;
                }
                last = cell;
                let g = ya + (yb - ya) * t as f32 - yoff(cl);
                if area_at([cell.0 as f64 * p, cell.1 as f64 * p], g).is_none() {
                    bad += 1;
                }
            }
            c.inc(&format!(
                "pairs.{cl}.{kind}.cells_off_{}",
                match bad {
                    0 => "0",
                    1 => "1",
                    2..=3 => "2_3",
                    _ => "4plus",
                }
            ));
            c.inc(&format!("pairs.{cl}.{kind}"));
            c.inc(&format!(
                "pairs.{cl}.{kind}.step_pair_end_{}",
                u8::from(stepper[i]) + u8::from(stepper[j])
            ));
            c.inc(&format!(
                "pairs.{cl}.{kind}.maxdeg_le4_{}",
                u8::from(deg[i].max(deg[j]) <= 4)
            ));
            c.inc(&format!("pairs.{cl}.{kind}.len_m_{:02}", d.floor() as i64));
            c.inc(&format!(
                "pairs.{cl}.{kind}.dy_dm_{:02}",
                (f64::from((ya - yb).abs()) * 10.0).floor().min(30.0) as i64
            ));
            c.inc(&format!("pairs.{cl}.{kind}.areas_{:02}", areas.len().min(20)));
            c.inc(&format!("pairs.{cl}.{kind}.tiles_{}", tiles.len().min(5)));
            c.inc(&format!(
                "pairs.{cl}.{kind}.off_mesh_{}",
                if off == 0 {
                    "none"
                } else if off <= 2 {
                    "1_2"
                } else {
                    "more"
                }
            ));
            c.inc(&format!("pairs.{cl}.{kind}.maxdeg_{:02}", deg[i].max(deg[j]).min(30)));
            c.inc(&format!("pairs.{cl}.{kind}.mindeg_{:02}", deg[i].min(deg[j]).min(30)));
            let idgap = nodes[i].id.abs_diff(nodes[j].id);
            c.inc(&format!("pairs.{cl}.{kind}.idgap_{}", bucket_n(idgap as usize)));
        }
    }
}

fn bucket_n(n: usize) -> &'static str {
    match n {
        0..=9 => "0000",
        10..=99 => "0010",
        100..=999 => "0100",
        _ => "1000",
    }
}

/// The distance from p to the nearest outline vertex and the nearest outline segment of the areas at height `y`
/// (outline heights within 2.5 m).
fn outline_near(m: &kymesh::Mesh, p: P2, y: f32) -> (f64, f64) {
    let mut dv = f64::MAX;
    let mut ds = f64::MAX;
    for a in &m.areas {
        for l in &a.outline {
            for w in l.windows(2) {
                if (w[0][1] - y).abs() > 2.5 {
                    continue;
                }
                let (s0, s1) = (xz(&w[0]), xz(&w[1]));
                dv = dv.min(len(sub(p, s0))).min(len(sub(p, s1)));
                ds = ds.min(pt_seg(p, s0, s1));
            }
        }
    }
    (dv, ds)
}

/// Distance from point p to segment ab (2-D).
fn pt_seg(p: P2, a: P2, b: P2) -> f64 {
    let ab = sub(b, a);
    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
    let t = if l2 < 1e-12 {
        0.0
    } else {
        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0)
    };
    len(sub(p, [a[0] + ab[0] * t, a[1] + ab[1] * t]))
}

/// The sector's 100 m square (min x, min z), to within a pitch: the mesh rectangle starts at most one pitch before it
/// (KYM-HDR-2).
fn square(m: &kymesh::Mesh) -> P2 {
    [f64::from(m.rect[0] + m.pitch), f64::from(m.rect[2] + m.pitch)]
}

// ---------------------------------------------------------------- KYG-NODE-1: spacing

/// The nearest other ordinary node of the sector at a similar height (within 3 m): its distance.
fn fit_spacing(s: &Sec, cl: &str, c: &mut C) {
    let ord: Vec<&hpd::Node> = s.nodes.iter().filter(|n| n.id & 0x8000_0000 == 0).collect();
    for (i, a) in ord.iter().enumerate() {
        let mut best = f64::MAX;
        for (j, b) in ord.iter().enumerate() {
            if i == j || (a.pos[1] - b.pos[1]).abs() > 3.0 {
                continue;
            }
            best = best.min(len(sub(xz(&a.pos), xz(&b.pos))));
        }
        let k = if best == f64::MAX {
            "none".to_owned()
        } else {
            format!("{:03}", (best * 10.0).floor().min(200.0) as i64)
        };
        c.inc(&format!("spacing.{cl}.nn_dm_{k}"));
    }
    // The step lattice: node pairs whose x and z offsets are both whole multiples of half the step
    // (GraphAccuracy + 0.35), joined into groups.
    let ga = match cl {
        "veh" => 15.0f64,
        "det" => 0.5,
        _ => 3.0,
    };
    let half = f64::midpoint(ga, 0.35);
    let on = |d: f64| {
        let k = (d / half).round();
        (d - k * half).abs() < 0.002
    };
    let mut parent: Vec<usize> = (0..ord.len()).collect();
    let root = find;
    let mut linked = vec![false; ord.len()];
    for i in 0..ord.len() {
        for j in i + 1..ord.len() {
            let (a, b) = (ord[i], ord[j]);
            let dx = f64::from(a.pos[0] - b.pos[0]);
            let dz = f64::from(a.pos[2] - b.pos[2]);
            if dx.abs() > 4.0 * dmax(cl) || dz.abs() > 4.0 * dmax(cl) {
                continue;
            }
            if on(dx) && on(dz) {
                linked[i] = true;
                linked[j] = true;
                let (ri, rj) = (root(&mut parent, i), root(&mut parent, j));
                parent[ri] = rj;
                let kx = (dx.abs() / half).round() as i64;
                let kz = (dz.abs() / half).round() as i64;
                if kx.min(kz) == 0 && kx.max(kz) == 2 {
                    // The pair's centre against the outline: nearest vertex and nearest segment.
                    let cen = [
                        f64::from(a.pos[0] + b.pos[0]) / 2.0,
                        f64::from(a.pos[2] + b.pos[2]) / 2.0,
                    ];
                    let (dv, ds) = outline_near(&s.mesh, cen, a.pos[1]);
                    c.inc(&format!(
                        "steplat.{cl}.pair_centre_vertex_cm_{:03}",
                        (dv * 20.0).floor().min(60.0) as i64 * 5
                    ));
                    c.inc(&format!(
                        "steplat.{cl}.pair_centre_seg_cm_{:03}",
                        (ds * 20.0).floor().min(60.0) as i64 * 5
                    ));
                    for (tag, p) in [("end", xz(&a.pos)), ("end", xz(&b.pos))] {
                        let (dv, ds) = outline_near(&s.mesh, p, a.pos[1]);
                        c.inc(&format!(
                            "steplat.{cl}.pair_{tag}_vertex_cm_{:03}",
                            (dv * 20.0).floor().min(60.0) as i64 * 5
                        ));
                        c.inc(&format!(
                            "steplat.{cl}.pair_{tag}_seg_cm_{:03}",
                            (ds * 20.0).floor().min(60.0) as i64 * 5
                        ));
                    }
                }
                c.inc(&format!(
                    "steplat.{cl}.pair_k_{:02}_{:02}",
                    kx.min(kz).min(20),
                    kx.max(kz).min(20)
                ));
            }
        }
    }
    let mut sizes: HashMap<usize, usize> = HashMap::new();
    for i in 0..ord.len() {
        let r = root(&mut parent, i);
        *sizes.entry(r).or_default() += 1;
    }
    for (i, l) in linked.iter().enumerate() {
        c.inc(&format!("steplat.{cl}.nodes_{}", if *l { "linked" } else { "alone" }));
        if *l {
            let r = root(&mut parent, i);
            c.inc(&format!("steplat.{cl}.node_in_group_of_{:03}", sizes[&r].min(100)));
        }
    }
    // Nodes that share a coordinate exactly (f32): how often, and how far apart along the other axis.
    for axis in [0usize, 2] {
        let o = 2 - axis;
        let mut by: HashMap<u32, Vec<&hpd::Node>> = HashMap::new();
        for n in &ord {
            by.entry(n.pos[axis].to_bits()).or_default().push(n);
        }
        let an = if axis == 0 { "x" } else { "z" };
        for g in by.values() {
            c.inc(&format!("share.{cl}.{an}.group_of_{:02}", g.len().min(20)));
            if g.len() < 2 {
                continue;
            }
            for (i, a) in g.iter().enumerate() {
                for b in &g[i + 1..] {
                    let d = f64::from((a.pos[o] - b.pos[o]).abs());
                    c.inc(&format!(
                        "share.{cl}.{an}.gap_cm_{:04}",
                        ((d * 20.0).floor() * 5.0).min(3000.0) as i64
                    ));
                    c.inc(&format!(
                        "share.{cl}.gap_mm_{:05}",
                        (d * 1000.0).round().min(99999.0) as i64
                    ));
                    let ids = a.id.abs_diff(b.id);
                    c.inc(&format!("share.{cl}.{an}.id_gap_{:02}", ids.min(20)));
                }
            }
        }
    }
}
