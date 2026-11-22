//! Round NKS-rest's per-feature fits of DICE's Kynapse generator rules (`spec/transforms/kynapse_generation.md`):
//! read-only, counts and histograms only, no content printed.
//!
//!   cargo run --release -p bc2-formats --example kynapse_fit [-- <levels folder>]
//!
//! The folder defaults to Project OPFOR BFBC2's `game\Output\win32\ai\levels` (`OPFOR_ROOT`, else
//! `project-opfor-bfbc2` beside the suite). Each fit names the rule it backs.
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

fn class(m: &kymesh::Mesh, vehicle: bool) -> &'static str {
    if vehicle {
        "veh"
    } else if (m.pitch - 0.2).abs() < 1e-4 {
        "det"
    } else {
        "inf"
    }
}

fn near(a: f32, b: f32, eps: f32) -> bool {
    (a - b).abs() <= eps
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

/// Distance between segments ab and cd (2-D).
fn seg_seg(a: P2, b: P2, c: P2, d: P2) -> f64 {
    if seg_hit(a, b, c, d) {
        return 0.0;
    }
    pt_seg(a, c, d)
        .min(pt_seg(b, c, d))
        .min(pt_seg(c, a, b))
        .min(pt_seg(d, a, b))
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
    id: u32,
    nodes: Vec<hpd::Node>,
    edges: Vec<hpd::Edge>,
    mesh: kymesh::Mesh,
}

fn main() {
    let mut levels: Vec<_> = std::fs::read_dir(levels_dir())
        .expect("levels folder")
        .flatten()
        .collect();
    levels.sort_by_key(std::fs::DirEntry::file_name);
    let mut c = C::default();
    for l in levels {
        let lname = l.file_name().to_string_lossy().to_lowercase();
        let mut boxes: Vec<(bool, [f32; 4], f32)> = Vec::new();
        for (kind, veh) in [("infantry", false), ("vehicle", true)] {
            let p = l.path().join(kind).join("pathdata.hpd");
            let Ok(data) = std::fs::read(&p) else { continue };
            let f = hpd::parse(&data).expect("path data");
            let secs: Vec<Sec> = f
                .sectors
                .iter()
                .map(|s| Sec {
                    id: s.id,
                    nodes: s.nodes.clone(),
                    edges: s.edges.clone(),
                    mesh: kymesh::parse(s.mesh).expect("mesh"),
                })
                .collect();
            c.inc(if veh { "files.veh" } else { "files.inf" });
            let gb = fit_file(&lname, veh, &secs, &mut c);
            boxes.push((veh, gb, if veh { 1.0 } else { 0.8 }));
        }
        // KYG-RECT-1: the infantry and vehicle grounds of one level against each other.
        if let (Some(i), Some(v)) = (boxes.iter().find(|b| !b.0), boxes.iter().find(|b| b.0)) {
            let d = [v.1[0] - i.1[0], i.1[1] - v.1[1], v.1[2] - i.1[2], i.1[3] - v.1[3]];
            println!(
                "rect {lname}: infantry x {:.1}..{:.1} z {:.1}..{:.1}; vehicle inside it by {:.1} / {:.1} / {:.1} / {:.1} m",
                i.1[0], i.1[1], i.1[2], i.1[3], d[0], d[1], d[2], d[3]
            );
            for x in d {
                c.inc(&format!(
                    "rect.veh_inside_inf_{}",
                    if x < -1.0 {
                        "out"
                    } else if x < 2.6 {
                        "0_2.5m"
                    } else {
                        "more"
                    }
                ));
            }
        }
    }
    println!("--- totals");
    for (k, v) in &c.0 {
        println!("{k} {v}");
    }
}

/// One file: every fit; returns the outline's box (min x, max x, min z, max z).
fn fit_file(name: &str, veh: bool, secs: &[Sec], c: &mut C) -> [f32; 4] {
    let mut gb = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
    for s in secs {
        let cl = class(&s.mesh, veh);
        for a in &s.mesh.areas {
            for v in a.outline.iter().flatten() {
                gb = [gb[0].min(v[0]), gb[1].max(v[0]), gb[2].min(v[2]), gb[3].max(v[2])];
            }
        }
        fit_midline(&s.mesh, cl, c);
        fit_nodes(s, cl, c);
        fit_edges(s, cl, c);
    }
    if !veh {
        fit_detailed(name, secs, c);
    }
    // KYG-EDGE-1: one-way edges over the whole file, border copies joined by their masked id.
    let mut ids: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for s in secs {
        for e in &s.edges {
            ids.insert((
                s.nodes[e.src as usize].id & 0x7FFF_FFFF,
                s.nodes[e.dst as usize].id & 0x7FFF_FFFF,
            ));
        }
    }
    let t = if veh { "veh" } else { "inf" };
    for s in secs {
        for e in &s.edges {
            let (na, nb) = (&s.nodes[e.src as usize], &s.nodes[e.dst as usize]);
            let back = ids.contains(&(nb.id & 0x7FFF_FFFF, na.id & 0x7FFF_FFFF));
            let kind = match (na.id >> 31, nb.id >> 31) {
                (0, 0) => "oo",
                (1, 1) => "bb",
                _ => "ob",
            };
            c.inc(&format!("fileedge.{t}.{}.{kind}", if back { "two" } else { "one" }));
            if !back {
                let dy = nb.pos[1] - na.pos[1];
                let band = if dy < -0.3 {
                    "down"
                } else if dy > 0.3 {
                    "up"
                } else {
                    "level"
                };
                c.inc(&format!("fileedge.{t}.one.dy_{band}"));
            }
        }
    }
    gb
}

/// An area's record size in bytes (`kymesh::write`'s layout).
fn area_bytes(a: &kymesh::Area) -> usize {
    let lines = |l: &[Vec<[f32; 3]>]| {
        if l.is_empty() {
            0
        } else {
            4 * (l.len() + 1) + 12 * l.iter().map(Vec::len).sum::<usize>()
        }
    };
    let f = if a.cells.is_empty() {
        0
    } else {
        16 * (a.cells.len() + 1) + 12 * a.cells.iter().map(|x| x.points.len()).sum::<usize>()
    };
    0x50 + 0x20 * a.portals.len() + 0x2C * a.border_portals.len() + lines(&a.d_lines) + lines(&a.outline) + f
}

/// An area's cells: the sample points (whole multiples of the pitch) inside its E + B + C boundary by the
/// even-odd rule, as the runtime's inside test (KYRT-AREA-1) counts them.
fn area_cells(a: &kymesh::Area, p: f32) -> usize {
    area_cells_c(a, p).0
}

/// The cells (as `area_cells`) and their centroid.
fn area_cells_c(a: &kymesh::Area, p: f32) -> (usize, P2) {
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
    if segs.is_empty() {
        return (0, [0.0, 0.0]);
    }
    let p = f64::from(p);
    let (mut sx, mut sz) = (0.0f64, 0.0f64);
    let z0 = segs.iter().map(|s| s.0[1].min(s.1[1])).fold(f64::MAX, f64::min);
    let z1 = segs.iter().map(|s| s.0[1].max(s.1[1])).fold(f64::MIN, f64::max);
    let mut n = 0usize;
    let mut xs: Vec<f64> = Vec::new();
    for k in (z0 / p).ceil() as i64..=(z1 / p).floor() as i64 {
        let z = k as f64 * p;
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
                let c0 = (w[0] / p).ceil() as i64;
                let c1 = (w[1] / p).floor() as i64;
                if c1 >= c0 {
                    let k = (c1 - c0 + 1) as usize;
                    n += k;
                    sx += (c0 + c1) as f64 * p / 2.0 * k as f64;
                    sz += z * k as f64;
                }
            }
        }
    }
    if n == 0 {
        return (0, [0.0, 0.0]);
    }
    (n, [sx / n as f64, sz / n as f64])
}

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

// ---------------------------------------------------------------- KYG-AREA-2: the midline cut

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

type Q = (i64, i64);

fn q(v: &[f32; 3]) -> Q {
    (
        (f64::from(v[0]) * 1000.0).round() as i64,
        (f64::from(v[2]) * 1000.0).round() as i64,
    )
}

/// The outline as an undirected vertex graph (lines joined at shared points).
fn outline_graph(lines: &[Vec<[f32; 3]>]) -> HashMap<Q, Vec<Q>> {
    let mut g: HashMap<Q, Vec<Q>> = HashMap::new();
    for line in lines {
        for w in line.windows(2) {
            let (a, b) = (q(&w[0]), q(&w[1]));
            if a == b {
                continue;
            }
            g.entry(a).or_default().push(b);
            g.entry(b).or_default().push(a);
        }
    }
    g
}

/// Closed components of the outline graph (every vertex of degree 2): their boxes in metres.
fn closed_loops(lines: &[Vec<[f32; 3]>]) -> Vec<[f64; 4]> {
    let g = outline_graph(lines);
    let mut seen: std::collections::HashSet<Q> = std::collections::HashSet::new();
    let mut keys: Vec<Q> = g.keys().copied().collect();
    keys.sort_unstable();
    let mut out = Vec::new();
    for k in keys {
        if !seen.insert(k) {
            continue;
        }
        let mut stack = vec![k];
        let mut closed = true;
        let mut bb = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
        let mut n = 0;
        while let Some(v) = stack.pop() {
            n += 1;
            let (x, z) = (v.0 as f64 / 1000.0, v.1 as f64 / 1000.0);
            bb = [bb[0].min(x), bb[1].max(x), bb[2].min(z), bb[3].max(z)];
            let nb = &g[&v];
            if nb.len() != 2 {
                closed = false;
            }
            for w in nb {
                if seen.insert(*w) {
                    stack.push(*w);
                }
            }
        }
        if closed && n >= 3 {
            out.push(bb);
        }
    }
    out
}

/// The area's holes: closed outline loops that are not its outer boundary (the largest loop of an area
/// without portals).
fn hole_boxes(a: &kymesh::Area) -> Vec<[f64; 4]> {
    let mut loops = closed_loops(&a.outline);
    if a.portals.is_empty() && a.border_portals.is_empty() && !loops.is_empty() {
        let (i, _) = loops
            .iter()
            .enumerate()
            .max_by(|x, y| {
                ((x.1[1] - x.1[0]) * (x.1[3] - x.1[2]))
                    .partial_cmp(&((y.1[1] - y.1[0]) * (y.1[3] - y.1[2])))
                    .unwrap()
            })
            .unwrap();
        loops.remove(i);
    }
    loops
}

fn holes(a: &kymesh::Area) -> usize {
    hole_boxes(a).len()
}

fn fit_midline(m: &kymesh::Mesh, cl: &str, c: &mut C) {
    let p = m.pitch;
    let t = 1.0 / m.resolution;
    let n = m.areas.len();
    let mut parent: Vec<usize> = (0..n).collect();
    // Midline portals: (area, target, axis, coordinate along the line from, to).
    let mut runs: HashMap<(usize, usize), Vec<(f32, f32)>> = HashMap::new();
    let mut cut_axis: HashMap<usize, u8> = HashMap::new();
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
                let o = if axis == 0 { 2 } else { 0 };
                if near(b.p0[k], b.p1[k], 1e-3) && near(b.p0[k], mid(a, axis), 0.01) {
                    let r = find(&mut parent, ai);
                    let q = find(&mut parent, b.target);
                    parent[r] = q;
                    cut_axis.insert(ai, axis as u8);
                    let (lo, hi) = (b.p0[o].min(b.p1[o]), b.p0[o].max(b.p1[o]));
                    runs.entry((ai.min(b.target), ai.max(b.target)))
                        .or_default()
                        .push((lo, hi));
                }
            }
        }
    }
    // Groups of areas joined across a midline.
    let mut groups: HashMap<usize, Vec<usize>> = HashMap::new();
    for i in 0..n {
        let r = find(&mut parent, i);
        groups.entry(r).or_default().push(i);
    }
    let bbox = |idx: &[usize]| {
        let mut bb = [f32::MAX, f32::MIN, f32::MAX, f32::MIN];
        for &i in idx {
            for v in m.areas[i].outline.iter().flatten() {
                bb = [bb[0].min(v[0]), bb[1].max(v[0]), bb[2].min(v[2]), bb[3].max(v[2])];
            }
            for q in &m.areas[i].portals {
                for v in [q.p0, q.p1] {
                    bb = [bb[0].min(v[0]), bb[1].max(v[0]), bb[2].min(v[2]), bb[3].max(v[2])];
                }
            }
            for q in &m.areas[i].border_portals {
                for v in [q.p0, q.p1] {
                    bb = [bb[0].min(v[0]), bb[1].max(v[0]), bb[2].min(v[2]), bb[3].max(v[2])];
                }
            }
        }
        bb
    };
    // Pair runs: separate stretches of midline portals between the same two areas.
    let mut multi_run_pairs: HashMap<usize, bool> = HashMap::new();
    for ((a, _), mut segs) in runs {
        segs.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        let mut k = 0;
        let mut end = f32::MIN;
        for (lo, hi) in segs {
            if lo > end + 0.01 {
                k += 1;
            }
            end = end.max(hi);
        }
        // The pair's portals are stored on both sides: two runs per side means two separate stretches.
        if k >= 2 {
            let r = find(&mut parent, a);
            multi_run_pairs.insert(r, true);
        }
    }
    let mut tile_cut: HashMap<(u32, u32), bool> = HashMap::new();
    let mut tile_uncut_straddlers: HashMap<(u32, u32), u32> = HashMap::new();
    let mut tile_bytes: HashMap<(u32, u32), usize> = HashMap::new();
    for a in &m.areas {
        *tile_bytes.entry((a.ids[1], a.ids[2])).or_default() += area_bytes(a);
    }
    for (root, idx) in &groups {
        let bb = bbox(idx);
        let tile = (m.areas[idx[0]].ids[1], m.areas[idx[0]].ids[2]);
        let bytes: usize = idx.iter().map(|&i| area_bytes(&m.areas[i])).sum();
        let cells: usize = idx.iter().map(|&i| area_cells(&m.areas[i], p)).sum();
        let full = ((t / p) * (t / p)) as usize;
        let cell_band = format!("{:03}", cells * 100 / full.max(1));
        let (w, h) = (bb[1] - bb[0], bb[3] - bb[2]);
        let nv: usize = idx
            .iter()
            .map(|&i| m.areas[i].outline.iter().map(Vec::len).sum::<usize>())
            .sum();
        if idx.len() > 1 {
            // A cut group.
            let axis = idx.iter().find_map(|i| cut_axis.get(i)).copied().unwrap_or(9);
            c.inc(&format!("mid.{cl}.cut_groups"));
            tile_cut.insert(tile, true);
            c.inc(&format!("mid.{cl}.cut_kb_{:03}", bytes / 1024));
            c.inc(&format!("mid.{cl}.cut_cells_pct_{cell_band}"));
            let longer = u8::from(w <= h);
            if axis == longer {
                c.inc(&format!("mid.{cl}.cut_on_longer"));
            }
            if multi_run_pairs.contains_key(root) {
                c.inc(&format!("mid.{cl}.cut_two_stretches"));
            }
            let hole_after: usize = idx.iter().map(|&i| holes(&m.areas[i])).sum();
            if hole_after > 0 {
                c.inc(&format!("mid.{cl}.cut_piece_with_hole"));
            }
            c.inc(&format!("mid.{cl}.cut_nv_{}", bucket(nv)));
            c.inc(&format!("mid.{cl}.cut_span_{}", bucket_m(w.max(h), t)));
        } else {
            let a = &m.areas[idx[0]];
            // Does the area straddle its tile's midline on either axis (cells on both sides)?
            let mx = mid(a, 0);
            let mz = mid(a, 1);
            let sx = bb[0] < mx - p * 0.6 && bb[1] > mx + p * 0.6;
            let sz = bb[2] < mz - p * 0.6 && bb[3] > mz + p * 0.6;
            if sx || sz {
                c.inc(&format!("mid.{cl}.uncut_straddlers"));
                *tile_uncut_straddlers.entry(tile).or_default() += 1;
                c.inc(&format!("mid.{cl}.uncut_kb_{:03}", bytes / 1024));
                c.inc(&format!("mid.{cl}.uncut_cells_pct_{cell_band}"));
                if holes(a) > 0 {
                    c.inc(&format!("mid.{cl}.uncut_straddler_with_hole"));
                }
                c.inc(&format!("mid.{cl}.uncut_nv_{}", bucket(nv)));
                c.inc(&format!("mid.{cl}.uncut_span_{}", bucket_m(w.max(h), t)));
            } else if holes(a) > 0 {
                c.inc(&format!("mid.{cl}.unstraddling_with_hole"));
            }
            for hb in hole_boxes(a) {
                let hx = hb[0] < f64::from(mx) - 0.01 && hb[1] > f64::from(mx) + 0.01;
                let hz = hb[2] < f64::from(mz) - 0.01 && hb[3] > f64::from(mz) + 0.01;
                c.inc(&format!(
                    "mid.{cl}.uncut_hole_{}",
                    if hx || hz {
                        "crossing_a_midline"
                    } else {
                        "off_the_midlines"
                    }
                ));
            }
        }
    }
    for (tile, b) in &tile_bytes {
        let cut = tile_cut.contains_key(tile);
        c.inc(&format!(
            "mid.{cl}.tile_{}_kb_{:03}",
            if cut { "cut" } else { "uncut" },
            b / 1024
        ));
        if cut {
            c.add(
                &format!("mid.{cl}.cut_tiles_uncut_straddlers"),
                u64::from(tile_uncut_straddlers.get(tile).copied().unwrap_or(0)),
            );
            c.inc(&format!("mid.{cl}.cut_tiles"));
        } else if tile_uncut_straddlers.contains_key(tile) {
            c.inc(&format!("mid.{cl}.uncut_tiles_with_straddlers"));
        }
    }
}

fn bucket(n: usize) -> String {
    let b = match n {
        0..=31 => 0,
        32..=63 => 32,
        64..=127 => 64,
        128..=255 => 128,
        256..=511 => 256,
        _ => 512,
    };
    format!("{b:03}")
}

fn bucket_m(span: f32, t: f32) -> String {
    format!("{:03}", ((span / t) * 10.0).floor() as i32)
}

// ---------------------------------------------------------------- KYG-NODE-1: node placement

fn fit_nodes(s: &Sec, cl: &str, c: &mut C) {
    let m = &s.mesh;
    let r = f64::from(m.radius);
    // Corner vertices of the outline: (vertex, unit to previous, unit to next).
    let mut corners: Vec<(P2, P2, P2, f32)> = Vec::new();
    for a in &m.areas {
        let g = outline_graph(&a.outline);
        let y = a.outline.first().and_then(|l| l.first()).map_or(0.0, |v| v[1]);
        let mut keys: Vec<Q> = g.keys().copied().collect();
        keys.sort_unstable();
        for k in keys {
            let nb = &g[&k];
            if nb.len() != 2 {
                continue;
            }
            let v = [k.0 as f64 / 1000.0, k.1 as f64 / 1000.0];
            let e1 = [nb[0].0 as f64 / 1000.0 - v[0], nb[0].1 as f64 / 1000.0 - v[1]];
            let e2 = [nb[1].0 as f64 / 1000.0 - v[0], nb[1].1 as f64 / 1000.0 - v[1]];
            let (l1, l2) = (len(e1), len(e2));
            if l1 < 1e-6 || l2 < 1e-6 {
                continue;
            }
            let e1 = [e1[0] / l1, e1[1] / l1];
            let e2 = [e2[0] / l2, e2[1] / l2];
            if cross(e1, e2).abs() < 1e-6 {
                continue;
            }
            corners.push((v, e1, e2, y));
        }
    }
    let cell = 2.0f64;
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, k) in corners.iter().enumerate() {
        grid.entry(((k.0[0] / cell).floor() as i64, (k.0[1] / cell).floor() as i64))
            .or_default()
            .push(i);
    }
    let mut hits = vec![false; corners.len()];
    for n in &s.nodes {
        let border = n.id & 0x8000_0000 != 0;
        let tag = if border { "b" } else { "o" };
        let p = [f64::from(n.pos[0]), f64::from(n.pos[2])];
        let (gx, gz) = ((p[0] / cell).floor() as i64, (p[1] / cell).floor() as i64);
        let mut best: Option<(f64, usize, f64, f64)> = None;
        let mut nearest = f64::MAX;
        for dx in -1..=1 {
            for dz in -1..=1 {
                let Some(list) = grid.get(&(gx + dx, gz + dz)) else {
                    continue;
                };
                for &i in list {
                    let (v, e1, e2, vy) = corners[i];
                    if (vy - n.pos[1]).abs() > 2.5 {
                        continue;
                    }
                    let d = sub(p, v);
                    nearest = nearest.min(len(d));
                    // d = -a e1 - b e2
                    let det = cross(e1, e2);
                    let a = -cross(d, e2) / det;
                    let b = -cross(e1, d) / det;
                    let err = (a - b).abs();
                    if a > 0.0 && b > 0.0 && best.is_none_or(|x| err < x.0) {
                        best = Some((err, i, a, b));
                    }
                }
            }
        }
        c.inc(&format!("node.{cl}.{tag}.all"));
        if nearest < 2.0 {
            c.inc(&format!("node.{cl}.{tag}.corner_within_2m"));
        }
        if let Some((err, i, a, b)) = best {
            let mean = f64::midpoint(a, b);
            if err < 0.02 {
                c.inc(&format!(
                    "node.{cl}.{tag}.offset_{:04}",
                    (mean / r * 100.0).round() as i64
                ));
                if (mean - r).abs() < 0.02 {
                    c.inc(&format!("node.{cl}.{tag}.at_corner_offset_r"));
                    hits[i] = true;
                }
            }
        }
    }
    // Recall: reflex corners (walkable side outside the angle) with a node at the offset.
    let with_node = hits.iter().filter(|h| **h).count();
    c.add(&format!("node.{cl}.corners_with_node"), with_node as u64);
    c.add(&format!("node.{cl}.corners"), corners.len() as u64);
    // Lattices: the fractional part of the node coordinates on a few periods (uniform = no lattice).
    for n in &s.nodes {
        if n.id & 0x8000_0000 != 0 {
            continue;
        }
        for q in [0.05f64, 0.1, 0.2, 0.25, 0.4, 0.5, 1.0] {
            let rel = |v: f32| {
                let t = (f64::from(v) - f64::from(m.rect[0])) / q;
                t - t.floor()
            };
            c.inc(&format!(
                "lattice.{cl}.q{q}.x_{}",
                (rel(n.pos[0]) * 10.0).floor() as i64
            ));
        }
    }
    // Nodes against their area: how many per area, and the distance to the area's cell centroid.
    let cent: Vec<(usize, P2)> = m.areas.iter().map(|a| area_cells_c(a, m.pitch)).collect();
    let mut per_area = vec![0u32; m.areas.len()];
    let mut last_d = vec![0.0f64; m.areas.len()];
    let mut last_p = vec![[0.0f64; 2]; m.areas.len()];
    for n in &s.nodes {
        let p = [f64::from(n.pos[0]), f64::from(n.pos[2])];
        let y = n.pos[1] - if cl == "veh" { 2.0 } else { 0.95 };
        let mut found = None;
        for (ai, a) in m.areas.iter().enumerate() {
            if y < a.values[3] - 0.3 || y > a.values[2] + 0.3 {
                continue;
            }
            if inside(a, p) {
                found = Some(ai);
                break;
            }
        }
        let tag = if n.id & 0x8000_0000 != 0 { "b" } else { "o" };
        match found {
            Some(ai) => {
                per_area[ai] += 1;
                let d = len(sub(p, cent[ai].1));
                last_d[ai] = d;
                last_p[ai] = p;
                c.inc(&format!(
                    "narea.{cl}.{tag}.centroid_dist_pitch_{:03}",
                    ((d / f64::from(m.pitch)) * 2.0).floor().min(99.0) as i64
                ));
            }
            None => c.inc(&format!("narea.{cl}.{tag}.no_area")),
        }
    }
    for (ai, k) in per_area.iter().enumerate() {
        c.inc(&format!("narea.{cl}.areas_with_{}_nodes", (*k).min(5)));
        if *k == 1 {
            c.inc(&format!(
                "narea.{cl}.single_node_centroid_cm_{:03}",
                (last_d[ai] * 100.0 / 5.0).floor().min(40.0) as i64 * 5
            ));
            let a = &m.areas[ai];
            let mut pts: Vec<P2> = Vec::new();
            for l in &a.outline {
                pts.extend(l.iter().map(xz));
            }
            for b in &a.portals {
                pts.push(xz(&b.p0));
                pts.push(xz(&b.p1));
            }
            for b in &a.border_portals {
                pts.push(xz(&b.p0));
                pts.push(xz(&b.p1));
            }
            if !pts.is_empty() {
                let k = pts.len() as f64;
                let vm = [
                    pts.iter().map(|q| q[0]).sum::<f64>() / k,
                    pts.iter().map(|q| q[1]).sum::<f64>() / k,
                ];
                let bb = [
                    pts.iter().map(|q| q[0]).fold(f64::MAX, f64::min),
                    pts.iter().map(|q| q[0]).fold(f64::MIN, f64::max),
                    pts.iter().map(|q| q[1]).fold(f64::MAX, f64::min),
                    pts.iter().map(|q| q[1]).fold(f64::MIN, f64::max),
                ];
                let bc = [f64::midpoint(bb[0], bb[1]), f64::midpoint(bb[2], bb[3])];
                for (tag, q) in [("vertex_mean", vm), ("box_centre", bc)] {
                    let d = len(sub(last_p[ai], q));
                    c.inc(&format!(
                        "narea.{cl}.single_node_{tag}_cm_{:03}",
                        (d * 100.0 / 5.0).floor().min(40.0) as i64 * 5
                    ));
                }
            }
        }
    }
    // Medial axis: a node's two nearest obstacle points in different directions, at equal distances.
    let mut segs: Vec<(P2, P2, f32)> = Vec::new();
    for a in &m.areas {
        for l in &a.outline {
            for w in l.windows(2) {
                segs.push((xz(&w[0]), xz(&w[1]), w[0][1]));
            }
        }
    }
    let gc = 4.0f64;
    let mut sg: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, sgm) in segs.iter().enumerate() {
        let (x0, x1) = (sgm.0[0].min(sgm.1[0]), sgm.0[0].max(sgm.1[0]));
        let (z0, z1) = (sgm.0[1].min(sgm.1[1]), sgm.0[1].max(sgm.1[1]));
        for gx in (x0 / gc).floor() as i64..=(x1 / gc).floor() as i64 {
            for gz in (z0 / gc).floor() as i64..=(z1 / gc).floor() as i64 {
                sg.entry((gx, gz)).or_default().push(i);
            }
        }
    }
    let medial = |p: P2, y: f32| -> Option<(f64, f64)> {
        let mut near: Vec<(f64, P2)> = Vec::new();
        let (gx, gz) = ((p[0] / gc).floor() as i64, (p[1] / gc).floor() as i64);
        for dx in -3..=3 {
            for dz in -3..=3 {
                let Some(list) = sg.get(&(gx + dx, gz + dz)) else {
                    continue;
                };
                for &i in list {
                    let (a, b, sy) = segs[i];
                    if (sy - y).abs() > 2.5 {
                        continue;
                    }
                    let ab = sub(b, a);
                    let l2 = ab[0] * ab[0] + ab[1] * ab[1];
                    let t = if l2 < 1e-12 {
                        0.0
                    } else {
                        (((p[0] - a[0]) * ab[0] + (p[1] - a[1]) * ab[1]) / l2).clamp(0.0, 1.0)
                    };
                    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t];
                    near.push((len(sub(p, q)), q));
                }
            }
        }
        near.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        let (d1, q1) = *near.first()?;
        if d1 < 1e-6 {
            return None;
        }
        let u1 = [(q1[0] - p[0]) / d1, (q1[1] - p[1]) / d1];
        let (d2, _) = near.iter().find(|(d, q)| {
            let u = [(q[0] - p[0]) / d, (q[1] - p[1]) / d];
            u[0] * u1[0] + u[1] * u1[1] < 0.5
        })?;
        Some((d1, *d2))
    };
    for n in &s.nodes {
        if n.id & 0x8000_0000 != 0 {
            continue;
        }
        let p = [f64::from(n.pos[0]), f64::from(n.pos[2])];
        let y = n.pos[1];
        for (tag, q) in [("node", p), ("control", [p[0] + 0.6 * r, p[1] + 0.9 * r])] {
            match medial(q, y) {
                Some((d1, d2)) => {
                    let ratio = (d2 - d1) / d2;
                    c.inc(&format!("medial.{cl}.{tag}.ratio_{:02}", (ratio * 20.0).floor() as i64));
                    c.inc(&format!(
                        "medial.{cl}.{tag}.d1_{:03}",
                        ((d1 / r) * 2.0).floor().min(99.0) as i64
                    ));
                }
                None => c.inc(&format!("medial.{cl}.{tag}.open")),
            }
        }
    }
}

// ---------------------------------------------------------------- KYG-EDGE-1: edge pairs and one-way edges

fn fit_edges(s: &Sec, cl: &str, c: &mut C) {
    let m = &s.mesh;
    let nodes = &s.nodes;
    let n = nodes.len();
    let dmax = if cl == "veh" { 50.0 } else { 15.0 };
    let ga = match cl {
        "veh" => 15.0,
        "det" => 0.5,
        _ => 3.0,
    };
    let mut has: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    for e in &s.edges {
        has.insert((e.src, e.dst));
    }
    // One-way edges: height and kind.
    for e in &s.edges {
        let (a, b) = (&nodes[e.src as usize], &nodes[e.dst as usize]);
        let dy = b.pos[1] - a.pos[1];
        let one = !has.contains(&(e.dst, e.src));
        let kind = match (a.id >> 31, b.id >> 31) {
            (0, 0) => "oo",
            (1, 1) => "bb",
            (0, 1) => "ob",
            _ => "bo",
        };
        let w = if one { "one" } else { "two" };
        c.inc(&format!("edge.{cl}.{w}.{kind}"));
        let band = if dy < -1.0 {
            "down_gt1"
        } else if dy < -0.3 {
            "down_03_1"
        } else if dy <= 0.3 {
            "level"
        } else if dy <= 1.0 {
            "up_03_1"
        } else {
            "up_gt1"
        };
        c.inc(&format!("edge.{cl}.{w}.dy_{band}"));
        if one && a.c & 0x1_0000 != 0 {
            c.inc(&format!("edge.{cl}.one.src_nonowner"));
        }
        if one && b.c & 0x1_0000 != 0 {
            c.inc(&format!("edge.{cl}.one.dst_nonowner"));
        }
        if !one && b.c & 0x1_0000 != 0 {
            c.inc(&format!("edge.{cl}.two.dst_nonowner"));
        }
    }
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];
    for e in &s.edges {
        let (a, b) = (e.src as usize, e.dst as usize);
        if !adj[a].contains(&b) {
            adj[a].push(b);
        }
        if !adj[b].contains(&a) {
            adj[b].push(a);
        }
    }
    // Obstacle segments: the outline and the D lines.
    let mut segs: Vec<(P2, P2, f32)> = Vec::new();
    for a in &m.areas {
        for line in a.outline.iter().chain(a.d_lines.iter()) {
            for w in line.windows(2) {
                segs.push((xz(&w[0]), xz(&w[1]), w[0][1].min(w[1][1])));
            }
        }
    }
    let cell = 4.0f64;
    let mut sg: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, sgm) in segs.iter().enumerate() {
        let (x0, x1) = (sgm.0[0].min(sgm.1[0]), sgm.0[0].max(sgm.1[0]));
        let (z0, z1) = (sgm.0[1].min(sgm.1[1]), sgm.0[1].max(sgm.1[1]));
        for gx in (x0 / cell).floor() as i64..=(x1 / cell).floor() as i64 {
            for gz in (z0 / cell).floor() as i64..=(z1 / cell).floor() as i64 {
                sg.entry((gx, gz)).or_default().push(i);
            }
        }
    }
    let ng = 15.0f64;
    let mut nodegrid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, x) in nodes.iter().enumerate() {
        nodegrid
            .entry((
                (f64::from(x.pos[0]) / ng).floor() as i64,
                (f64::from(x.pos[2]) / ng).floor() as i64,
            ))
            .or_default()
            .push(i);
    }
    let reach = (dmax / ng).ceil() as i64;
    // Areas by tile, with their boxes, for the on-mesh test of a straight line.
    let tsz = 1.0 / f64::from(m.resolution);
    let mut by_tile: HashMap<(u32, u32), Vec<(usize, [f64; 4])>> = HashMap::new();
    for (ai, a) in m.areas.iter().enumerate() {
        let mut bb = [f64::MAX, f64::MIN, f64::MAX, f64::MIN];
        for v in a.outline.iter().flatten() {
            bb = [
                bb[0].min(f64::from(v[0])),
                bb[1].max(f64::from(v[0])),
                bb[2].min(f64::from(v[2])),
                bb[3].max(f64::from(v[2])),
            ];
        }
        for q in a
            .portals
            .iter()
            .map(|b| [b.p0, b.p1])
            .chain(a.border_portals.iter().map(|b| [b.p0, b.p1]))
        {
            for v in q {
                bb = [
                    bb[0].min(f64::from(v[0])),
                    bb[1].max(f64::from(v[0])),
                    bb[2].min(f64::from(v[2])),
                    bb[3].max(f64::from(v[2])),
                ];
            }
        }
        by_tile.entry((a.ids[1], a.ids[2])).or_default().push((ai, bb));
    }
    let yoff = if cl == "veh" { 2.0f32 } else { 0.95 };
    let on_mesh = |p: P2, y: f32| -> bool {
        let tx = ((p[0] - f64::from(m.rect[0])) / tsz)
            .floor()
            .clamp(0.0, f64::from(m.tiles_x - 1)) as u32;
        let tz = ((p[1] - f64::from(m.rect[2])) / tsz)
            .floor()
            .clamp(0.0, f64::from(m.tiles_z - 1)) as u32;
        for dx in [0i32, -1, 1] {
            for dz in [0i32, -1, 1] {
                let key = ((tx as i32 + dx).max(0) as u32, (tz as i32 + dz).max(0) as u32);
                let Some(list) = by_tile.get(&key) else { continue };
                for (ai, bb) in list {
                    if p[0] < bb[0] || p[0] > bb[1] || p[1] < bb[2] || p[1] > bb[3] {
                        continue;
                    }
                    let a = &m.areas[*ai];
                    let g = y - yoff;
                    if g < a.values[3] - 0.7 || g > a.values[2] + 0.7 {
                        continue;
                    }
                    if inside(a, p) {
                        return true;
                    }
                }
            }
        }
        false
    };
    let d3 = |i: usize, j: usize| -> f64 {
        let (a, b) = (&nodes[i].pos, &nodes[j].pos);
        let v = [f64::from(a[0] - b[0]), f64::from(a[1] - b[1]), f64::from(a[2] - b[2])];
        (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt()
    };
    // Graph distance from i to j without the direct edge, bounded by `limit`.
    let graph_dist = |i: usize, j: usize, limit: f64| -> f64 {
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
    let mut seen_cells: Vec<(i64, i64)> = Vec::new();
    for i in 0..n {
        let a = &nodes[i];
        let pa = [f64::from(a.pos[0]), f64::from(a.pos[2])];
        let (gx, gz) = ((pa[0] / ng).floor() as i64, (pa[1] / ng).floor() as i64);
        for dx in -reach..=reach {
            for dz in -reach..=reach {
                let Some(list) = nodegrid.get(&(gx + dx, gz + dz)) else {
                    continue;
                };
                for &j in list {
                    if j <= i {
                        continue;
                    }
                    let b = &nodes[j];
                    if (a.id >> 31) == 1 && (b.id >> 31) == 1 {
                        continue;
                    }
                    if (a.id >> 31) != (b.id >> 31) {
                        continue;
                    }
                    let pb = [f64::from(b.pos[0]), f64::from(b.pos[2])];
                    let d = len(sub(pb, pa));
                    if d > dmax || (a.pos[1] - b.pos[1]).abs() > 3.0 {
                        continue;
                    }
                    let edge = has.contains(&(i as u32, j as u32)) || has.contains(&(j as u32, i as u32));
                    // Visibility against the outline (2-D, segments near the pair's heights).
                    seen_cells.clear();
                    let steps = (d / (cell / 2.0)).ceil().max(1.0) as usize;
                    for k in 0..=steps {
                        let t = k as f64 / steps as f64;
                        let q = [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t];
                        let key = ((q[0] / cell).floor() as i64, (q[1] / cell).floor() as i64);
                        if !seen_cells.contains(&key) {
                            seen_cells.push(key);
                        }
                    }
                    let ylo = a.pos[1].min(b.pos[1]) - 2.0;
                    let yhi = a.pos[1].max(b.pos[1]) + 1.0;
                    let mut blocked = false;
                    'o: for key in &seen_cells {
                        if let Some(list) = sg.get(key) {
                            for &si in list {
                                let (s0, s1, y) = segs[si];
                                if y < ylo || y > yhi {
                                    continue;
                                }
                                if seg_hit(pa, pb, s0, s1) {
                                    blocked = true;
                                    break 'o;
                                }
                            }
                        }
                    }
                    let vis = if blocked { "blocked" } else { "visible" };
                    let e = if edge { "edge" } else { "none" };
                    c.inc(&format!("pair.{cl}.{vis}.{e}"));
                    if blocked {
                        continue;
                    }
                    // The graph's route between them without a direct edge: its excess over the straight line.
                    let dd = d3(i, j);
                    let g = graph_dist(i, j, dd + 3.0 * ga);
                    let excess = if g == f64::MAX {
                        "over3ga".to_owned()
                    } else {
                        format!("{:03}", ((g - dd) / ga * 20.0).floor() as i64)
                    };
                    c.inc(&format!("pair.{cl}.visible.{e}.graph_excess_x20ga_{excess}"));
                    let ratio = if g == f64::MAX {
                        99
                    } else {
                        ((g / dd - 1.0) * 100.0).floor().min(99.0) as i64
                    };
                    c.inc(&format!("pair.{cl}.visible.{e}.graph_ratio_pct_{ratio:02}"));
                    // The spanner rule's misses: the line's on-mesh test (every 0.4 m inside an area at its height).
                    let check = (!edge && ratio >= 20) || (edge && (i + j) % 20 == 0);
                    if check {
                        let steps = (d / 0.4).ceil().max(1.0) as usize;
                        let mut ok = true;
                        for k in 1..steps {
                            let t = k as f64 / steps as f64;
                            let q = [pa[0] + (pb[0] - pa[0]) * t, pa[1] + (pb[1] - pa[1]) * t];
                            let y = a.pos[1] + (b.pos[1] - a.pos[1]) * t as f32;
                            if !on_mesh(q, y) {
                                ok = false;
                                break;
                            }
                        }
                        let tag = if edge { "edge_sample" } else { "none_over_1_2" };
                        c.inc(&format!(
                            "pair.{cl}.mesh.{tag}.{}",
                            if ok { "on_mesh" } else { "leaves_mesh" }
                        ));
                        if ok {
                            // Clearance: the line's distance from the outline (E only, then E + D).
                            let (x0, x1) = (pa[0].min(pb[0]) - 3.0, pa[0].max(pb[0]) + 3.0);
                            let (z0, z1) = (pa[1].min(pb[1]) - 3.0, pa[1].max(pb[1]) + 3.0);
                            let mut clear = f64::MAX;
                            for gx in (x0 / cell).floor() as i64..=(x1 / cell).floor() as i64 {
                                for gz in (z0 / cell).floor() as i64..=(z1 / cell).floor() as i64 {
                                    let Some(list) = sg.get(&(gx, gz)) else { continue };
                                    for &si in list {
                                        let (s0, s1, y) = segs[si];
                                        if y < ylo || y > yhi {
                                            continue;
                                        }
                                        clear = clear.min(seg_seg(pa, pb, s0, s1));
                                    }
                                }
                            }
                            let r = f64::from(m.radius);
                            c.inc(&format!(
                                "pair.{cl}.clear.{tag}.r_x10_{:02}",
                                ((clear / r) * 10.0).floor().min(30.0) as i64
                            ));
                        }
                    }
                    // A two-edge route through a third node, and how much longer it is.
                    let mut detour = f64::MAX;
                    for &w in &adj[i] {
                        if w != j && adj[j].contains(&w) {
                            let pw = [f64::from(nodes[w].pos[0]), f64::from(nodes[w].pos[2])];
                            detour = detour.min(len(sub(pw, pa)) + len(sub(pb, pw)) - d);
                        }
                    }
                    let band = if detour == f64::MAX {
                        "none".to_owned()
                    } else {
                        format!("{:03}", ((detour / ga) * 20.0).floor().min(99.0) as i64)
                    };
                    c.inc(&format!("pair.{cl}.visible.{e}.detour_per_ga_x20_{band}"));
                }
            }
        }
    }
}

// ---------------------------------------------------------------- KYG-DET-1: detailed sectors as blocks

fn fit_detailed(name: &str, secs: &[Sec], c: &mut C) {
    let det: std::collections::HashSet<(u32, u32)> = secs
        .iter()
        .filter(|s| (s.mesh.pitch - 0.2).abs() < 1e-4)
        .map(|s| (s.id >> 8, s.id & 0xFF))
        .collect();
    let all: std::collections::HashSet<(u32, u32)> = secs.iter().map(|s| (s.id >> 8, s.id & 0xFF)).collect();
    let mut seen: std::collections::HashSet<(u32, u32)> = std::collections::HashSet::new();
    let mut comps = 0;
    let mut rects = 0;
    let mut in_rect = 0;
    let mut sorted: Vec<_> = det.iter().copied().collect();
    sorted.sort_unstable();
    for &s in &sorted {
        if seen.contains(&s) {
            continue;
        }
        let mut stack = vec![s];
        let mut comp = Vec::new();
        seen.insert(s);
        while let Some(q) = stack.pop() {
            comp.push(q);
            for (dx, dz) in [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)] {
                let r = ((q.0 as i32 + dx) as u32, (q.1 as i32 + dz) as u32);
                if det.contains(&r) && seen.insert(r) {
                    stack.push(r);
                }
            }
        }
        comps += 1;
        let (x0, x1) = (
            comp.iter().map(|q| q.0).min().unwrap(),
            comp.iter().map(|q| q.0).max().unwrap(),
        );
        let (z0, z1) = (
            comp.iter().map(|q| q.1).min().unwrap(),
            comp.iter().map(|q| q.1).max().unwrap(),
        );
        let mut full = true;
        for x in x0..=x1 {
            for z in z0..=z1 {
                if all.contains(&(x, z)) && !det.contains(&(x, z)) {
                    full = false;
                }
            }
        }
        if full {
            rects += 1;
            in_rect += comp.len();
        }
    }
    println!(
        "det {name}: {} detailed sectors in {comps} blocks, {rects} blocks rectangular ({in_rect} sectors)",
        det.len()
    );
    c.add("det.sectors", det.len() as u64);
    c.add("det.blocks", comps as u64);
    c.add("det.blocks_rect", rects as u64);
    c.add("det.sectors_in_rect_blocks", in_rect as u64);
}
