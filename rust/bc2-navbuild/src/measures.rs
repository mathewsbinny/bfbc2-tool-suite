//! NavKit's DICE-parity measures (`navparity.run_level`, round E4e): `navdiff.compare_sector` (cells matched by
//! height, areas by shape, boundaries as grid pieces, route nodes by position, edges and costs) and
//! `ordercheck.PathData`'s areas with the crack rule and its `on_area` test - 9.6 of `sp_004_b`'s 11.7 s of measures.
//! The meshes are parsed here (`bc2_formats::kymesh`, E3o's reader) and the cells sampled here (`parity::area_cells`),
//! so a file's areas never become Python objects for these measures.
//!
//! **The same counts.** Python's `round(x)` is half to even ([`f64::round_ties_even`]); `round(x, n)` is compared
//! through the exact integer it rounds `x * 10^n` to ([`round_scaled`]: two of them are equal exactly when the
//! rounded floats are, for the coordinates of a level); `x ** 2` is [`pymath::sq`]; sorted tuples compare as
//! Python's do (floats, then integers). Where the reference would raise (a value that is not finite reaching
//! `round`) or a mesh the core does not read, `None`: the Python runs and raises what it raises.

use std::collections::{HashMap, HashSet, VecDeque};

use bc2_formats::kymesh::{self, Area, Mesh, Vec3};
use rayon::prelude::*;

use crate::parity::area_cells;
use crate::pymath;
use crate::trace::P3;

/// `round(x * 10^digits)` exactly as CPython's `round(x, digits)` decides it (`double_round`: the decimal digits of
/// x's exact value, an exact tie to even), as the integer; `None` for a value that is not finite or too large.
#[must_use]
#[allow(clippy::many_single_char_names, reason = "pymath::round3's names")]
pub fn round_scaled(x: f64, scale: u32) -> Option<i64> {
    if !x.is_finite() {
        return None;
    }
    if x == 0.0 {
        return Some(0);
    }
    let bits = x.to_bits();
    let neg = bits >> 63 == 1;
    let exp = i64::try_from((bits >> 52) & 0x7ff).ok()?;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, e) = if exp == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp - 1075)
    };
    let num = u128::from(mant) * u128::from(scale);
    let n: u128 = if e >= 0 {
        if e > 60 {
            return None;
        }
        num << e
    } else {
        let s = u32::try_from(-e).ok()?;
        if s >= 127 {
            0
        } else {
            let q = num >> s;
            let r = num - (q << s);
            let half = 1u128 << (s - 1);
            if r > half || (r == half && q & 1 == 1) {
                q + 1
            } else {
                q
            }
        }
    };
    let n = i64::try_from(n).ok()?;
    if n > (1i64 << 53) {
        return None;
    }
    Some(if neg { -n } else { n })
}

/// Python's `round(x)` of a float: half to even, as an integer (`None`: not finite, or beyond 2^62).
fn round_int(x: f64) -> Option<i64> {
    let r = x.round_ties_even();
    (r.is_finite() && r.abs() < 4.0e18).then_some(r as i64)
}

fn p3(v: Vec3) -> P3 {
    (f64::from(v[0]), f64::from(v[1]), f64::from(v[2]))
}

/// `navdiff.segments`: outline lines, then portals, then border portals.
fn segments(a: &Area) -> Vec<(P3, P3)> {
    let mut out = Vec::new();
    for l in &a.outline {
        out.extend(l.iter().zip(l.iter().skip(1)).map(|(&a, &b)| (p3(a), p3(b))));
    }
    out.extend(a.portals.iter().map(|q| (p3(q.p0), p3(q.p1))));
    out.extend(a.border_portals.iter().map(|q| (p3(q.p0), p3(q.p1))));
    out
}

type Piece = ((i64, i64), (i64, i64));

/// `navdiff._pieces_loop` (and `pieces`, the same arithmetic in NumPy): segments as grid pieces in half-pitch units,
/// direction-free, sorted and without repeats (a set). `None` where `round` would raise.
fn pieces(segs: &[(P3, P3)], pitch: f64) -> Option<Vec<Piece>> {
    let half = pitch / 2.0;
    let mut out = Vec::new();
    for &(p0, p1) in segs {
        let (x0, z0, x1, z1) = (p0.0 / half, p0.2 / half, p1.0 / half, p1.2 / half);
        let steps = round_int((x1 - x0).abs().max((z1 - z0).abs()))?.max(1);
        if steps > 1 << 24 {
            return None;
        }
        let n = steps as f64;
        for k in 0..steps {
            let (kf, kn) = (k as f64, (k + 1) as f64);
            let a = (round_int(x0 + (x1 - x0) * kf / n)?, round_int(z0 + (z1 - z0) * kf / n)?);
            let b = (round_int(x0 + (x1 - x0) * kn / n)?, round_int(z0 + (z1 - z0) * kn / n)?);
            out.push(if a <= b { (a, b) } else { (b, a) });
        }
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

/// A route node and an edge as the measures read them.
#[derive(Debug, Clone)]
pub struct Graph {
    /// Node positions.
    pub nodes: Vec<P3>,
    /// Edges: (source, destination, tag), indices into `nodes`.
    pub edges: Vec<(usize, usize, u32)>,
}

/// One sector of a file for the measures: its mesh, its cells per area and its route graph.
#[derive(Debug, Clone)]
pub struct Sector {
    mesh: Mesh,
    cells: Vec<Vec<((i64, i64), f64)>>,
    graph: Graph,
}

/// The settings `navdiff` compares with (its module constants: the vehicle comparison changes the grid).
#[derive(Debug, Clone, Copy)]
pub struct DiffCfg {
    /// `navdiff.GRID`.
    pub grid: f64,
    /// `navdiff.Y_OFFSET`.
    pub y_offset: f64,
    /// `navdiff.STACK_TOL`.
    pub stack_tol: f64,
    /// `navdiff.NODE_XZ`.
    pub node_xz: f64,
    /// `navdiff.NODE_Y`.
    pub node_y: f64,
}

impl Sector {
    /// The sector from its mesh record and route graph; `clip` drops cells outside it (`navdiff.diff`'s reference
    /// clip). `None`: a mesh the core does not read, a boundary that is not all finite, an edge out of range.
    #[must_use]
    pub fn new(mesh: &[u8], graph: Graph, cfg: &DiffCfg, clip: Option<[f64; 4]>) -> Option<Self> {
        let mesh = kymesh::parse(mesh).ok()?;
        if graph
            .edges
            .iter()
            .any(|&(s, d, _)| s >= graph.nodes.len() || d >= graph.nodes.len())
        {
            return None;
        }
        let finite = |p: &P3| p.0.is_finite() && p.1.is_finite() && p.2.is_finite();
        let mut cells = Vec::with_capacity(mesh.areas.len());
        for a in &mesh.areas {
            let segs = segments(a);
            if !segs.iter().all(|(a, b)| finite(a) && finite(b)) {
                return None;
            }
            let mut c = area_cells(&segs, cfg.grid, cfg.y_offset);
            if let Some([x0, z0, x1, z1]) = clip {
                c.retain(|&((i, j), _)| {
                    let (x, z) = (i as f64 * cfg.grid, j as f64 * cfg.grid);
                    x0 <= x && x <= x1 && z0 <= z && z <= z1
                });
            }
            cells.push(c);
        }
        Some(Sector { mesh, cells, graph })
    }

    /// Cells over all areas.
    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.iter().map(Vec::len).sum()
    }

    /// Areas.
    #[must_use]
    pub fn area_count(&self) -> usize {
        self.mesh.areas.len()
    }
}

/// `compare_sector`'s answer: its counts in the reference's key order, and the cells only one side has.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SectorDiff {
    /// (name, value) in `compare_sector`'s order.
    pub counts: Vec<(&'static str, i64)>,
    /// `only_ref`: reference cells without a match, in order.
    pub only_ref: Vec<(i64, i64)>,
    /// `only_cand`: candidate cells without a match, in order.
    pub only_cand: Vec<(i64, i64)>,
}

/// Cells by (i, j): the keys in the order first seen, and each key's (area, height) list.
type ByIj = (Vec<(i64, i64)>, HashMap<(i64, i64), Vec<(usize, f64)>>);

fn by_ij(cells: &[Vec<((i64, i64), f64)>]) -> ByIj {
    let mut order = Vec::new();
    let mut map: HashMap<(i64, i64), Vec<(usize, f64)>> = HashMap::new();
    for (a, c) in cells.iter().enumerate() {
        for &(ij, y) in c {
            map.entry(ij)
                .or_insert_with(|| {
                    order.push(ij);
                    Vec::new()
                })
                .push((a, y));
        }
    }
    (order, map)
}

/// `navdiff.match_cells`: reference (area, cell) -> candidate area, and the candidate (area, cell)s used.
#[allow(clippy::type_complexity)]
fn match_cells(
    rc: &[Vec<((i64, i64), f64)>],
    cc: &[Vec<((i64, i64), f64)>],
    tol: f64,
) -> (HashMap<(usize, (i64, i64)), usize>, HashSet<(usize, (i64, i64))>) {
    let (_, by_c) = by_ij(cc);
    let (order, by_r) = by_ij(rc);
    let mut to_cand = HashMap::new();
    let mut used = HashSet::new();
    for ij in order {
        let (Some(refs), Some(cands)) = (by_r.get(&ij), by_c.get(&ij)) else {
            continue;
        };
        if let ([(a, _)], [(b, _)]) = (refs.as_slice(), cands.as_slice()) {
            to_cand.insert((*a, ij), *b);
            used.insert((*b, ij));
            continue;
        }
        let mut pairs: Vec<(f64, usize, usize)> = refs
            .iter()
            .flat_map(|&(a, yr)| cands.iter().map(move |&(b, yc)| ((yr - yc).abs(), a, b)))
            .collect();
        pairs.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
        let (mut done_r, mut done_c) = (HashSet::new(), HashSet::new());
        for (d, a, b) in pairs {
            if d > tol || done_r.contains(&a) || done_c.contains(&b) {
                continue;
            }
            done_r.insert(a);
            done_c.insert(b);
            to_cand.insert((a, ij), b);
            used.insert((b, ij));
        }
    }
    (to_cand, used)
}

type Shape = (i64, Vec<Piece>, i64, i64);

fn area_pitch(a: &Area) -> f64 {
    f64::from(a.values[1])
}

fn shape(a: &Area) -> Option<Shape> {
    let pitch = area_pitch(a);
    Some((
        round_scaled(pitch, 100)?,
        pieces(&segments(a), pitch)?,
        round_int(f64::from(a.values[3]) * 2.0)?,
        round_int(f64::from(a.values[2]) * 2.0)?,
    ))
}

fn outline_segs(a: &Area) -> Vec<(P3, P3)> {
    a.outline
        .iter()
        .flat_map(|l| l.iter().zip(l.iter().skip(1)).map(|(&a, &b)| (p3(a), p3(b))))
        .collect()
}

type BorderKey = (Piece, (i64, i64, i64));

fn borders(a: &Area, pitch: f64) -> Option<Vec<BorderKey>> {
    let mut out = Vec::new();
    for b in &a.border_portals {
        let d = p3(b.direction);
        let dk = (
            round_scaled(d.0, 1000)?,
            round_scaled(d.1, 1000)?,
            round_scaled(d.2, 1000)?,
        );
        for k in pieces(&[(p3(b.p0), p3(b.p1))], pitch)? {
            out.push((k, dk));
        }
    }
    out.sort_unstable();
    out.dedup();
    Some(out)
}

/// `navdiff.compare_sector(ref, cand)`; `None` where the reference raises.
#[must_use]
#[allow(clippy::float_cmp, reason = "the reference's tie rule: an equal distance, exactly")]
#[allow(clippy::too_many_lines)]
pub fn compare_sector(r: &Sector, c: &Sector, cfg: &DiffCfg) -> Option<SectorDiff> {
    let (ra, ca) = (&r.mesh.areas, &c.mesh.areas);
    let detailed = |areas: &[Area]| areas.iter().filter(|a| area_pitch(a) < 0.5).count() as i64;
    let (rc, cc) = (&r.cells, &c.cells);
    let (to_cand, used) = match_cells(rc, cc, cfg.stack_tol);
    let mut out = SectorDiff::default();
    for (a, cells) in rc.iter().enumerate() {
        out.only_ref.extend(
            cells
                .iter()
                .filter(|(ij, _)| !to_cand.contains_key(&(a, *ij)))
                .map(|(ij, _)| *ij),
        );
    }
    for (b, cells) in cc.iter().enumerate() {
        out.only_cand.extend(
            cells
                .iter()
                .filter(|(ij, _)| !used.contains(&(b, *ij)))
                .map(|(ij, _)| *ij),
        );
    }
    // areas by shape
    let mut by_shape: HashMap<Shape, VecDeque<usize>> = HashMap::new();
    for (b, a) in ca.iter().enumerate() {
        by_shape.entry(shape(a)?).or_default().push_back(b);
    }
    let mut area_map: Vec<(usize, usize)> = Vec::new();
    let (mut same, mut split, mut other) = (0i64, 0i64, 0i64);
    for (ai, a) in ra.iter().enumerate() {
        if let Some(b) = by_shape.get_mut(&shape(a)?).and_then(VecDeque::pop_front) {
            area_map.push((ai, b));
            same += 1;
            continue;
        }
        let targets: HashSet<usize> = rc
            .get(ai)?
            .iter()
            .filter_map(|(ij, _)| to_cand.get(&(ai, *ij)).copied())
            .collect();
        if targets.len() > 1 {
            split += 1;
        } else {
            other += 1;
        }
    }
    let map: HashMap<usize, usize> = area_map.iter().copied().collect();
    let (mut outline_same, mut portals_same, mut borders_same) = (0i64, 0i64, 0i64);
    for &(ai, bj) in &area_map {
        let (ar, ac) = (ra.get(ai)?, ca.get(bj)?);
        let pitch = area_pitch(ar);
        outline_same += i64::from(pieces(&outline_segs(ar), pitch)? == pieces(&outline_segs(ac), pitch)?);
        let ports = |a: &Area| -> Vec<(P3, P3)> { a.portals.iter().map(|p| (p3(p.p0), p3(p.p1))).collect() };
        let (pr, pc) = (pieces(&ports(ar), pitch)?, pieces(&ports(ac), pitch)?);
        if pr == pc {
            let mut tr: Vec<i64> = ar
                .portals
                .iter()
                .map(|p| map.get(&p.target).map_or(-1, |&t| t as i64))
                .collect();
            let mut tc: Vec<i64> = ac.portals.iter().map(|p| p.target as i64).collect();
            tr.sort_unstable();
            tc.sort_unstable();
            portals_same += i64::from(tr == tc);
        }
        borders_same += i64::from(borders(ar, pitch)? == borders(ac, pitch)?);
    }
    // route nodes: one to one, the closest still free within NODE_XZ / NODE_Y
    let key = |p: &P3| Some((round_int(p.0 / 0.5)?, round_int(p.2 / 0.5)?));
    let mut cand_nodes: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (k, p) in c.graph.nodes.iter().enumerate() {
        cand_nodes.entry(key(p)?).or_default().push(k);
    }
    let mut node_map: HashMap<usize, usize> = HashMap::new();
    let mut taken: HashSet<usize> = HashSet::new();
    for (idx, n) in r.graph.nodes.iter().enumerate() {
        let (kx, kz) = key(n)?;
        let mut best: Option<(usize, f64)> = None;
        for dx in [-1, 0, 1] {
            for dz in [-1, 0, 1] {
                for &k in cand_nodes.get(&(kx + dx, kz + dz)).map_or(&[][..], Vec::as_slice) {
                    let m = c.graph.nodes.get(k)?;
                    if taken.contains(&k)
                        || (m.0 - n.0).abs() > cfg.node_xz
                        || (m.2 - n.2).abs() > cfg.node_xz
                        || (m.1 - n.1).abs() > cfg.node_y
                    {
                        continue;
                    }
                    let d = pymath::sq(m.0 - n.0) + pymath::sq(m.1 - n.1) + pymath::sq(m.2 - n.2);
                    if best.is_none_or(|(_, bd)| d < bd || (d == bd && k == idx)) {
                        best = Some((k, d));
                    }
                }
            }
        }
        if let Some((k, _)) = best {
            node_map.insert(idx, k);
            taken.insert(k);
        }
    }
    let mut cand_edges: HashMap<(usize, usize), Vec<u32>> = HashMap::new();
    for &(s, d, t) in &c.graph.edges {
        cand_edges.entry((s, d)).or_default().push(t);
    }
    let (mut edges_same, mut costs_same) = (0i64, 0i64);
    for &(s, d, t) in &r.graph.edges {
        let (Some(&s), Some(&d)) = (node_map.get(&s), node_map.get(&d)) else {
            continue;
        };
        let Some(tags) = cand_edges.get_mut(&(s, d)).filter(|t| !t.is_empty()) else {
            continue;
        };
        edges_same += 1;
        let at = tags.iter().position(|&x| x == t);
        costs_same += i64::from(at.is_some());
        tags.remove(at.unwrap_or(0));
    }
    let len = |v: usize| v as i64;
    out.counts = vec![
        ("cells_ref", len(r.cell_count())),
        ("cells_cand", len(c.cell_count())),
        ("cells_same", len(to_cand.len())),
        ("areas_ref", len(ra.len())),
        ("areas_cand", len(ca.len())),
        ("areas_same", same),
        ("areas_split", split),
        ("areas_other", other),
        ("outline_same", outline_same),
        ("portals_same", portals_same),
        ("borders_same", borders_same),
        ("nodes_ref", len(r.graph.nodes.len())),
        ("nodes_cand", len(c.graph.nodes.len())),
        ("nodes_same", len(node_map.len())),
        ("edges_ref", len(r.graph.edges.len())),
        ("edges_same", edges_same),
        ("costs_same", costs_same),
        ("detailed_ref", detailed(ra)),
        ("detailed_cand", detailed(ca)),
    ];
    out.counts.extend(paired_graph(r, c)?);
    Some(out)
}

/// The nearest-node pairing's tolerance by the reference sector's pitch (`navdiff.PAIR_TOL`): half of KYG-NODE-7's
/// DistPropagation, 0.425 m detailed, 1.675 m coarse, 7.675 m vehicle.
#[must_use]
pub fn pair_tolerance(pitch: f64) -> f64 {
    if pitch < 0.5 {
        0.425
    } else if pitch < 0.95 {
        1.675
    } else {
        7.675
    }
}

/// `navdiff.NODE_NEAR_DY`.
const NODE_NEAR_DY: f64 = 1.0;
/// `navdiff.DIST_BINS` and `DIST_KEYS` (the last key: none within the last bound).
const DIST_BINS: [f64; 7] = [0.05, 0.1, 0.25, 0.5, 1.0, 2.0, 4.0];
const DIST_KEYS: [&str; 8] = [
    "nd_005", "nd_010", "nd_025", "nd_050", "nd_100", "nd_200", "nd_400", "nd_far",
];
/// `navdiff.COST_TOL`.
const COST_TOL: i64 = 10;

fn floor_key(x: f64) -> Option<i64> {
    let f = x.floor();
    (f.is_finite() && f.abs() < 4.0e18).then_some(f as i64)
}

/// `navdiff.paired_graph` (round NKS-deep E): every reference node paired one to one with a candidate node, nearest
/// first in plan within [`pair_tolerance`] and `NODE_NEAR_DY` in height; the reference edges whose partners hold the
/// edge, those with the cost within a tenth, and the histogram of each reference node's nearest candidate node.
fn paired_graph(r: &Sector, c: &Sector) -> Option<Vec<(&'static str, i64)>> {
    let tol = pair_tolerance(f64::from(r.mesh.pitch));
    let reach = tol.max(DIST_BINS[DIST_BINS.len() - 1]);
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (k, p) in c.graph.nodes.iter().enumerate() {
        grid.entry((floor_key(p.0 / reach)?, floor_key(p.2 / reach)?))
            .or_default()
            .push(k);
    }
    let mut hist = [0i64; 8];
    let mut pairs: Vec<(f64, usize, usize)> = Vec::new();
    for (i, n) in r.graph.nodes.iter().enumerate() {
        let (gx, gz) = (floor_key(n.0 / reach)?, floor_key(n.2 / reach)?);
        let mut best = f64::INFINITY;
        for dx in [-1, 0, 1] {
            for dz in [-1, 0, 1] {
                for &k in grid.get(&(gx + dx, gz + dz)).map_or(&[][..], Vec::as_slice) {
                    let m = c.graph.nodes.get(k)?;
                    if (m.1 - n.1).abs() > NODE_NEAR_DY {
                        continue;
                    }
                    let d2 = (m.0 - n.0) * (m.0 - n.0) + (m.2 - n.2) * (m.2 - n.2);
                    best = best.min(d2);
                    if d2 <= tol * tol {
                        pairs.push((d2, i, k));
                    }
                }
            }
        }
        let b = DIST_BINS.iter().position(|u| best <= u * u).unwrap_or(DIST_BINS.len());
        *hist.get_mut(b)? += 1;
    }
    pairs.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)));
    let mut partner: HashMap<usize, usize> = HashMap::new();
    let mut taken: HashSet<usize> = HashSet::new();
    for &(_, i, k) in &pairs {
        if !partner.contains_key(&i) && !taken.contains(&k) {
            partner.insert(i, k);
            taken.insert(k);
        }
    }
    let mut cand_edges: HashMap<(usize, usize), Vec<i64>> = HashMap::new();
    for &(s, d, t) in &c.graph.edges {
        cand_edges.entry((s, d)).or_default().push(i64::from(t & 0x7FFF_FFFF));
    }
    let (mut edges, mut costs) = (0i64, 0i64);
    for &(s, d, t) in &r.graph.edges {
        let (Some(&s), Some(&d)) = (partner.get(&s), partner.get(&d)) else {
            continue;
        };
        let Some(got) = cand_edges.get_mut(&(s, d)).filter(|g| !g.is_empty()) else {
            continue;
        };
        let want = i64::from(t & 0x7FFF_FFFF);
        let at = got
            .iter()
            .enumerate()
            .min_by_key(|&(_, &c)| ((c - want).abs(), c))
            .map(|(n, _)| n)?;
        let best_c = got.remove(at);
        edges += 1;
        costs += i64::from(COST_TOL * (best_c - want).abs() <= want);
    }
    let mut out = vec![
        ("nodes_paired", partner.len() as i64),
        ("edges_paired", edges),
        ("costs_paired", costs),
    ];
    out.extend(DIST_KEYS.iter().zip(hist).map(|(&k, v)| (k, v)));
    Some(out)
}

/// `compare_sector` of every pair, in parallel.
#[must_use]
pub fn compare_many(pairs: &[(&Sector, &Sector)], cfg: &DiffCfg) -> Option<Vec<SectorDiff>> {
    pairs.par_iter().map(|(r, c)| compare_sector(r, c, cfg)).collect()
}

// ------------------------------------------------------------------------------------- ordercheck.PathData's areas

/// One area for `on_area`: its bounds (x, z, ground y) and its boundary after the crack rule.
#[derive(Debug, Clone)]
pub struct OrderArea {
    bounds: [f64; 6],
    segs: Vec<(P3, P3)>,
}

/// `ordercheck.drop_cracks`: coincident segment pairs (in plan, at 2 decimals) cancel; odd counts stay.
fn drop_cracks(segs: Vec<(P3, P3)>) -> Option<Vec<(P3, P3)>> {
    let key = |s: &(P3, P3)| -> Option<Piece> {
        let a = (round_scaled(s.0.0, 100)?, round_scaled(s.0.2, 100)?);
        let b = (round_scaled(s.1.0, 100)?, round_scaled(s.1.2, 100)?);
        Some(if a <= b { (a, b) } else { (b, a) })
    };
    let keys: Vec<_> = segs.iter().map(key).collect::<Option<Vec<_>>>()?;
    let mut n: HashMap<Piece, usize> = HashMap::new();
    for k in &keys {
        *n.entry(*k).or_default() += 1;
    }
    Some(
        segs.into_iter()
            .zip(keys)
            .filter(|(_, k)| n.get(k).is_some_and(|c| c % 2 == 1))
            .map(|(s, _)| s)
            .collect(),
    )
}

/// `ordercheck.PathData.areas` of a file's meshes (in order), with `y_offset` (`ordercheck.Y_OFFSET`); `None` where
/// the reference raises (a value `round` cannot take, a mesh the core does not read).
#[must_use]
pub fn order_areas(meshes: &[&[u8]], y_offset: f64) -> Option<Vec<OrderArea>> {
    let per: Vec<Vec<OrderArea>> = meshes
        .par_iter()
        .map(|m| {
            let mesh = kymesh::parse(m).ok()?;
            let mut out = Vec::new();
            for a in &mesh.areas {
                let segs = drop_cracks(segments(a))?;
                if segs.is_empty() {
                    continue;
                }
                let ends = || segs.iter().flat_map(|&(a, b)| [a, b]);
                let mn = |f: fn(&P3) -> f64| ends().map(|p| f(&p)).fold(f64::INFINITY, f64::min);
                let mx = |f: fn(&P3) -> f64| ends().map(|p| f(&p)).fold(f64::NEG_INFINITY, f64::max);
                if ends().any(|p| p.0.is_nan() || p.1.is_nan() || p.2.is_nan()) {
                    return None;
                }
                out.push(OrderArea {
                    bounds: [
                        mn(|p| p.0),
                        mx(|p| p.0),
                        mn(|p| p.2),
                        mx(|p| p.2),
                        mn(|p| p.1) - y_offset,
                        mx(|p| p.1) - y_offset,
                    ],
                    segs,
                });
            }
            Some(out)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(per.into_iter().flatten().collect())
}

/// `ordercheck.PathData.on_area(p, ytol)` over the areas.
#[must_use]
#[allow(
    clippy::float_cmp,
    reason = "the reference skips horizontal segments by exact equality"
)]
pub fn on_area(areas: &[OrderArea], p: P3, ytol: f64) -> bool {
    let (x, y, z) = p;
    for a in areas {
        let [x0, x1, z0, z1, y0, y1] = a.bounds;
        if !(x0 <= x && x <= x1 && z0 <= z && z <= z1 && y0 - ytol <= y && y <= y1 + ytol) {
            continue;
        }
        let mut inside = false;
        for &((ax, _, az), (bx, _, bz)) in &a.segs {
            if az == bz {
                continue;
            }
            if az.min(bz) <= z && z < az.max(bz) && x < ax + (z - az) * (bx - ax) / (bz - az) {
                inside = !inside;
            }
        }
        if inside {
            return true;
        }
    }
    false
}

// ------------------------------------------------------------------------------------------------ navdf

/// A D side's key (`navdf.features`: class, axis, line, along, side) or an F cell's (class, axis, sign, i, j).
type DfKey = (u8, u8, i64, i64, i64);

/// Heights by key, the keys in the order first found.
type DfMap = Vec<(DfKey, Vec<f64>)>;

/// `navdf.features` of a file's meshes: D sides and F cells by key (heights in the order found), and the counts
/// (`d_n`, `d_d`, `f_n`, `f_d`: lines and records per class).
#[derive(Debug, Clone, Default)]
pub struct DfFeatures {
    d: DfMap,
    f: DfMap,
    /// `[d_n, d_d, f_n, f_d]`.
    pub counts: [i64; 4],
}

fn push(map: &mut DfMap, at: &mut HashMap<DfKey, usize>, key: DfKey, y: f64) {
    let k = *at.entry(key).or_insert_with(|| {
        map.push((key, Vec::new()));
        map.len() - 1
    });
    if let Some(e) = map.get_mut(k) {
        e.1.push(y);
    }
}

/// `navdf._in_poly`.
#[allow(clippy::many_single_char_names, reason = "the reference's names")]
fn in_poly(x: f64, z: f64, pts: &[Vec3]) -> bool {
    let mut inside = false;
    let n = pts.len();
    for k in 0..n {
        let (Some(a), Some(b)) = (pts.get(k), pts.get((k + 1) % n)) else {
            break;
        };
        let (ax, az, bx, bz) = (f64::from(a[0]), f64::from(a[2]), f64::from(b[0]), f64::from(b[2]));
        if (az > z) != (bz > z) && x < ax + (z - az) * (bx - ax) / (bz - az) {
            inside = !inside;
        }
    }
    inside
}

fn floor_int(x: f64) -> Option<i64> {
    let r = x.floor();
    (r.is_finite() && r.abs() < 4.0e18).then_some(r as i64)
}

fn ceil_int(x: f64) -> Option<i64> {
    let r = x.ceil();
    (r.is_finite() && r.abs() < 4.0e18).then_some(r as i64)
}

/// `navdf.features(meshes, clip)`; `None` where the reference raises (an empty D line, a value `round` or `int`
/// cannot take) or for a mesh the core does not read.
#[must_use]
#[allow(clippy::many_single_char_names, reason = "the reference's names")]
pub fn df_features(meshes: &[&[u8]], clip: Option<[f64; 4]>) -> Option<DfFeatures> {
    let inside = |x: f64, z: f64| clip.is_none_or(|c| c[0] <= x && x <= c[2] && c[1] <= z && z <= c[3]);
    let mut out = DfFeatures::default();
    let (mut at_d, mut at_f) = (HashMap::new(), HashMap::new());
    for m in meshes {
        let mesh = kymesh::parse(m).ok()?;
        for a in &mesh.areas {
            let p = area_pitch(a);
            let c = u8::from(p < 0.5); // 0: normal ('n'), 1: detailed ('d')
            let h = p / 2.0;
            for line in &a.d_lines {
                let first = line.first()?;
                if !inside(f64::from(first[0]), f64::from(first[2])) {
                    continue;
                }
                *out.counts.get_mut(usize::from(c))? += 1;
                for w in line.windows(2) {
                    let (p0, p1) = (p3(*w.first()?), p3(*w.get(1)?));
                    let (dx, dz) = (p1.0 - p0.0, p1.2 - p0.2);
                    let l = pymath::hypot(dx, dz);
                    if l < 1e-6 || (dx.abs() > 1e-3 && dz.abs() > 1e-3) {
                        continue; // a zero-length piece; a diagonal (the reference counts it, reported nowhere)
                    }
                    let n = round_int(l / p)?.max(1);
                    for k in 0..n {
                        let t = (k as f64 + 0.5) / n as f64;
                        let (x, z, y) = (p0.0 + dx * t, p0.2 + dz * t, p0.1 + (p1.1 - p0.1) * t);
                        let key = if dx.abs() < 1e-3 {
                            (
                                c,
                                0,
                                round_int(x / h)?,
                                round_int(z / p)?,
                                if dz > 0.0 { 1 } else { -1 },
                            )
                        } else {
                            (
                                c,
                                1,
                                round_int(z / h)?,
                                round_int(x / p)?,
                                if dx < 0.0 { 1 } else { -1 },
                            )
                        };
                        push(&mut out.d, &mut at_d, key, y);
                    }
                }
            }
            for cell in &a.cells {
                let pts = &cell.points;
                let Some(first) = pts.first() else { continue };
                if !inside(f64::from(first[0]), f64::from(first[2])) {
                    continue;
                }
                *out.counts.get_mut(2 + usize::from(c))? += 1;
                if pts.iter().any(|q| q.iter().any(|v| v.is_nan())) {
                    return None;
                }
                let axis = u8::from(f64::from(cell.direction[0]).abs() <= 0.5); // 0: 'x', 1: 'z'
                let sign = if f64::from(cell.direction[0]) + f64::from(cell.direction[2]) > 0.0 {
                    1
                } else {
                    -1
                };
                let xs = pts.iter().map(|q| f64::from(q[0]));
                let zs = pts.iter().map(|q| f64::from(q[2]));
                let (xmin, xmax) = (
                    xs.clone().fold(f64::INFINITY, f64::min),
                    xs.fold(f64::NEG_INFINITY, f64::max),
                );
                let (zmin, zmax) = (
                    zs.clone().fold(f64::INFINITY, f64::min),
                    zs.fold(f64::NEG_INFINITY, f64::max),
                );
                let y = pts.iter().fold(0.0, |acc, q| acc + f64::from(q[1])) / pts.len() as f64;
                for i in floor_int(xmin / p)?..=ceil_int(xmax / p)? {
                    for j in floor_int(zmin / p)?..=ceil_int(zmax / p)? {
                        if in_poly(i as f64 * p + p * 1.37e-3, j as f64 * p + p * 0.71e-3, pts) {
                            push(&mut out.f, &mut at_f, (c, axis, sign, i, j), y);
                        }
                    }
                }
            }
        }
    }
    Some(out)
}

/// `navdf._match`: per key, heights paired greedily with the nearest (the first of equals) within `dy`.
fn df_match(r: &[&(DfKey, Vec<f64>)], c: &HashMap<DfKey, &Vec<f64>>, dy: f64, dys: &mut Vec<f64>) -> i64 {
    let mut hit = 0;
    for (key, ys) in r.iter().map(|e| (&e.0, &e.1)) {
        let mut cs: Vec<f64> = c.get(key).map_or_else(Vec::new, |v| (*v).clone());
        for &y in ys {
            if cs.is_empty() {
                break;
            }
            // min(range(len(cs)), key=...): the first least |cs[q] - y|
            let mut k = 0;
            let mut best = f64::NAN;
            for (q, &v) in cs.iter().enumerate() {
                let d = (v - y).abs();
                if q == 0 || d < best {
                    k = q;
                    best = d;
                }
            }
            if best <= dy {
                hit += 1;
                dys.push(best);
                cs.remove(k);
            }
        }
    }
    hit
}

/// One class's figures of `navdf.compare` (all, normal, detailed).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DfClass {
    /// D sides: reference, candidate, matched, the reference's with a candidate side within a line (`_near`).
    pub d: [i64; 4],
    /// F cells: reference, candidate, matched.
    pub f: [i64; 3],
}

/// `navdf.compare`'s figures: per class (all, normal, detailed), the matched D sides' median height difference
/// (all), the shared-ground recall (matched, total) when the candidate's ground is given, both files' counts.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct DfFigures {
    /// All, normal, detailed.
    pub classes: Vec<DfClass>,
    /// `sorted(dys)[len(dys) // 2]`.
    pub median_dy: Option<f64>,
    /// `_match(_shared(rd, ground), cd)`'s matches and `_shared`'s heights.
    pub shared: Option<(i64, i64)>,
    /// `[d_n, d_d, f_n, f_d]` of the reference and of the candidate.
    pub counts: [[i64; 4]; 2],
}

/// One area's cells: ((i, j), height) in order.
type AreaCells = Vec<((i64, i64), f64)>;

/// The ground cells by (i, j) (`navdf.ground_of`).
pub type Ground = HashMap<(i64, i64), Vec<f64>>;

fn df_pick(m: &DfMap, cls: Option<u8>) -> Vec<&(DfKey, Vec<f64>)> {
    m.iter().filter(|e| cls.is_none_or(|k| e.0.0 == k)).collect()
}

fn df_total(m: &[&(DfKey, Vec<f64>)]) -> i64 {
    m.iter().map(|e| e.1.len() as i64).sum()
}

/// `navdf._shared(rd, ground)`: the reference's D sides whose owner cell is candidate ground within `dy`.
fn df_shared(rd: &[&(DfKey, Vec<f64>)], g: &Ground, grid: f64, dy: f64) -> Option<DfMap> {
    let mut sh = Vec::new();
    for e in rd {
        let (key, ys) = (e.0, &e.1);
        let (cl, axis, line, along, side) = key;
        let pp = if cl == 1 { 0.2 } else { 0.8 };
        let owner = (line + side).div_euclid(2);
        let (x, z) = if axis == 0 {
            (owner as f64 * pp, along as f64 * pp)
        } else {
            (along as f64 * pp, owner as f64 * pp)
        };
        let ij = (round_int(x / grid)?, round_int(z / grid)?);
        let keep: Vec<f64> = ys
            .iter()
            .copied()
            .filter(|&y| {
                g.get(&ij)
                    .is_some_and(|gs| gs.iter().any(|gy| (gy + 0.95 - y).abs() <= dy))
            })
            .collect();
        if !keep.is_empty() {
            sh.push((key, keep));
        }
    }
    Some(sh)
}

/// `navdf.compare(ref, cand, clip, ground)`'s figures, from both files' features; `ground`: the candidate's cells
/// (`ground_of`), `grid`: `navdf.GRID`, `dy`: `navdf.DY`.
#[must_use]
pub fn df_compare(r: &DfFeatures, c: &DfFeatures, ground: Option<&Ground>, grid: f64, dy: f64) -> Option<DfFigures> {
    let mut out = DfFigures {
        counts: [r.counts, c.counts],
        ..DfFigures::default()
    };
    for cls in [None, Some(0u8), Some(1u8)] {
        let (rd, cd, rf, cf) = (
            df_pick(&r.d, cls),
            df_pick(&c.d, cls),
            df_pick(&r.f, cls),
            df_pick(&c.f, cls),
        );
        let cdm: HashMap<DfKey, &Vec<f64>> = cd.iter().map(|e| (e.0, &e.1)).collect();
        let cfm: HashMap<DfKey, &Vec<f64>> = cf.iter().map(|e| (e.0, &e.1)).collect();
        let mut dys = Vec::new();
        let hit = df_match(&rd, &cdm, dy, &mut dys);
        let fhit = df_match(&rf, &cfm, dy, &mut Vec::new());
        let mut near = 0;
        for e in &rd {
            let (cl, axis, line, along, side) = e.0;
            for &y in &e.1 {
                let found = [-2, 0, 2].iter().any(|dl| {
                    cdm.get(&(cl, axis, line + dl, along, side))
                        .is_some_and(|cs| cs.iter().any(|cy| (cy - y).abs() <= dy))
                });
                near += i64::from(found);
            }
        }
        out.classes.push(DfClass {
            d: [df_total(&rd), df_total(&cd), hit, near],
            f: [df_total(&rf), df_total(&cf), fhit],
        });
        if cls.is_none() {
            dys.sort_by(f64::total_cmp);
            out.median_dy = dys.get(dys.len() / 2).copied();
            if let Some(g) = ground {
                let sh = df_shared(&rd, g, grid, dy)?;
                let shr: Vec<&(DfKey, Vec<f64>)> = sh.iter().collect();
                out.shared = Some((df_match(&shr, &cdm, dy, &mut Vec::new()), df_total(&shr)));
            }
        }
    }
    Some(out)
}

/// `navdf.ground_of` of a file's sectors: every area's cells (`navdiff.area_cells` on `grid`) by (i, j), the heights
/// in the order of sectors, areas and cells. `None`: a mesh the core does not read or a boundary not all finite.
#[must_use]
pub fn ground_of(meshes: &[&[u8]], grid: f64, y_offset: f64) -> Option<Ground> {
    let finite = |p: &P3| p.0.is_finite() && p.1.is_finite() && p.2.is_finite();
    let per: Vec<Vec<AreaCells>> = meshes
        .par_iter()
        .map(|m| {
            let mesh = kymesh::parse(m).ok()?;
            mesh.areas
                .iter()
                .map(|a| {
                    let segs = segments(a);
                    segs.iter()
                        .all(|(a, b)| finite(a) && finite(b))
                        .then(|| area_cells(&segs, grid, y_offset))
                })
                .collect::<Option<Vec<_>>>()
        })
        .collect::<Option<Vec<_>>>()?;
    let mut g: Ground = HashMap::new();
    for &(ij, y) in per.iter().flatten().flatten() {
        g.entry(ij).or_default().push(y);
    }
    Some(g)
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "tests of small known values")]
mod tests {
    use super::*;

    #[test]
    fn rounding_as_cpython() {
        // round(x, 2) as CPython 3.13 gives it, times 100
        assert_eq!(round_scaled(0.125, 100), Some(12)); // a tie: to even
        assert_eq!(round_scaled(0.375, 100), Some(38));
        assert_eq!(round_scaled(2.675, 100), Some(267)); // 2.675 is a hair under
        assert_eq!(round_scaled(-1.005, 100), Some(-100));
        assert_eq!(round_scaled(f64::NAN, 100), None);
        assert_eq!(round_int(2.5), Some(2));
        assert_eq!(round_int(-3.5), Some(-4));
    }

    #[test]
    fn pieces_and_cracks() {
        let s = ((0.0, 0.0, 0.0), (1.6, 0.0, 0.0));
        let p = pieces(&[s], 0.8).unwrap_or_default();
        assert_eq!(
            p,
            vec![((0, 0), (1, 0)), ((1, 0), (2, 0)), ((2, 0), (3, 0)), ((3, 0), (4, 0))]
        );
        let back = ((1.6, 5.0, 0.0), (0.0, 1.0, 0.0));
        let kept = drop_cracks(vec![s, back, ((0.0, 0.0, 0.0), (0.0, 0.0, 1.0))]).unwrap_or_default();
        assert_eq!(kept.len(), 1);
    }

    #[test]
    fn inside_a_square() {
        let sq = vec![
            ((0.0, 1.0, 0.0), (2.0, 1.0, 0.0)),
            ((2.0, 1.0, 0.0), (2.0, 1.0, 2.0)),
            ((2.0, 1.0, 2.0), (0.0, 1.0, 2.0)),
            ((0.0, 1.0, 2.0), (0.0, 1.0, 0.0)),
        ];
        let a = OrderArea {
            bounds: [0.0, 2.0, 0.0, 2.0, 0.05, 0.05],
            segs: sq,
        };
        assert!(on_area(std::slice::from_ref(&a), (1.0, 0.0, 1.0), 2.5));
        assert!(!on_area(std::slice::from_ref(&a), (1.0, 9.0, 1.0), 2.5));
        assert!(!on_area(&[a], (3.0, 0.0, 1.0), 2.5));
    }
}
