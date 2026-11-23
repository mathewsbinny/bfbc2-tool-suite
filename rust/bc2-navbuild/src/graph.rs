//! `navwalk.roof_islands` (after its islands) and `build_graph`'s coverage nodes and Gabriel pruning
//! (`navbuild.add_cover_nodes`, `navbuild.gabriel_prune`; round E3c).
//!
//! The reference computes these with NumPy arrays: `np.hypot` is the C library's `hypot` (as Rust's [`f64::hypot`]),
//! an array's `** 2` is NumPy's square (`x * x`), a Python float's `** 2` is [`pymath::sq`], `math.hypot` is
//! [`pymath::hypot`]. Where the reference looks at every node or point, these look only at the ones a spatial grid
//! puts near (a superset of the ones the test can accept, then the reference's own test); the answers are minima or
//! "any", which do not depend on the order the candidates are seen in.

use rayon::prelude::*;

use crate::trace::P3;
use crate::{FastMap, pymath};

/// The roof rule's settings (`navwalk.ROOF_RAISE`, `ROOF_SHARE`, `ROOF_NEAR`, `ROOF_NEAR_DY`).
#[derive(Debug, Clone, Copy)]
pub struct RoofCfg {
    /// A kept surface this high (m) over its column's lowest is raised.
    pub raise: f64,
    /// An island with this share of raised surfaces is a roof candidate.
    pub share: f64,
    /// Anchored ground within this distance (m, plan) ...
    pub near: f64,
    /// ... and this height difference keeps a candidate.
    pub near_dy: f64,
}

/// The sample grid `roof_drop` reads: columns, heights (`float64`), the grid's origin and pitch.
#[derive(Debug, Clone, Copy)]
pub struct RoofGrid<'a> {
    /// Columns along x.
    pub nx: usize,
    /// Columns along z.
    pub nz: usize,
    /// Per column its first surface (`nx * nz + 1` values).
    pub start: &'a [usize],
    /// Heights per surface.
    pub y: &'a [f64],
    /// The grid's x origin.
    pub min_x: f64,
    /// The grid's z origin.
    pub min_z: f64,
    /// The pitch.
    pub pitch: f64,
}

/// What `roof_drop` decided: the surfaces dropped, and the counts `ROOF_STATS` adds (`None`: the reference returned
/// before counting - fewer than two islands).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoofDrop {
    /// Per surface: dropped.
    pub out: Vec<bool>,
    /// (islands, islands dropped, surfaces dropped); `dropped` and `surfaces` are `None` when no island was a
    /// candidate (the reference counts only the islands then).
    pub stats: Option<(usize, Option<(usize, usize)>)>,
}

/// `np.minimum.reduceat(y, start[:-1].clip(max=len(y) - 1))[c]` for every column c: the column's lowest height, with
/// NumPy's rules for the indices (an index not below the next gives the value at the index; the last runs to the end).
fn lowest(g: &RoofGrid<'_>) -> Vec<f64> {
    let n = g.y.len();
    let cols = g.nx * g.nz;
    let idx: Vec<usize> = (0..cols)
        .map(|c| g.start.get(c).copied().unwrap_or(0).min(n.saturating_sub(1)))
        .collect();
    let min = |a: f64, b: f64| if a.is_nan() || b.is_nan() { f64::NAN } else { a.min(b) };
    (0..cols)
        .map(|c| {
            let a = idx.get(c).copied().unwrap_or(0);
            let b = if c + 1 < cols {
                idx.get(c + 1).copied().unwrap_or(n)
            } else {
                n
            };
            if b > a {
                g.y.get(a..b)
                    .map_or(f64::NAN, |s| s.iter().copied().reduce(min).unwrap_or(f64::NAN))
            } else {
                g.y.get(a).copied().unwrap_or(f64::NAN)
            }
        })
        .collect()
}

/// `navwalk.roof_islands` after `islands_of`: `comp` the island of each surface (-1: not kept), `count` the islands,
/// `keep` / `terrain` per surface, `anchors` the anchor surfaces. `None` when an input does not fit the grid.
#[must_use]
#[allow(clippy::too_many_lines, clippy::many_single_char_names)]
pub fn roof_drop(
    g: &RoofGrid<'_>,
    keep: &[bool],
    terrain: &[bool],
    comp: &[i64],
    count: usize,
    anchors: &[usize],
    cfg: RoofCfg,
) -> Option<RoofDrop> {
    let len = g.y.len();
    let cols = g.nx * g.nz;
    if keep.len() != len || terrain.len() != len || comp.len() != len || g.start.len() != cols + 1 {
        return None;
    }
    if g.start.last() != Some(&len) || g.start.windows(2).any(|w| w.first() > w.get(1)) {
        return None;
    }
    if comp.iter().any(|&c| c >= count as i64 || c < -1) || anchors.iter().any(|&k| k >= len) {
        return None;
    }
    let mut out = vec![false; len];
    if count < 2 {
        return Some(RoofDrop { out, stats: None });
    }
    let mut col_of = Vec::with_capacity(len);
    for (c, w) in g.start.windows(2).enumerate() {
        if let [a, b] = *w {
            col_of.extend(std::iter::repeat_n(c, b - a));
        }
    }
    let low = lowest(g);
    let isl = |k: usize| comp.get(k).copied().unwrap_or(-1);
    let mut size = vec![0usize; count];
    let mut raised = vec![0usize; count];
    for k in 0..len {
        let (Some(&kp), Some(&y), Some(&t), Some(&c)) = (keep.get(k), g.y.get(k), terrain.get(k), col_of.get(k)) else {
            return None;
        };
        if !kp {
            continue;
        }
        // comp[keep]: every kept surface has an island (the reference's bincount fails on a -1)
        let ci = usize::try_from(isl(k)).ok()?;
        *size.get_mut(ci)? += 1;
        if !t && y - low.get(c).copied().unwrap_or(f64::NAN) >= cfg.raise {
            *raised.get_mut(ci)? += 1;
        }
    }
    let mut anchored = vec![false; count];
    for &k in anchors {
        if let Ok(ci) = usize::try_from(isl(k)) {
            *anchored.get_mut(ci)? = true;
        }
    }
    // np.argmax: the first of the largest
    let big = size
        .iter()
        .enumerate()
        .fold(
            (0usize, 0usize),
            |(bi, bs), (i, &s)| if s > bs { (i, s) } else { (bi, bs) },
        )
        .0;
    *anchored.get_mut(big)? = true;
    let cand: Vec<bool> = (0..count)
        .map(|c| {
            let share = raised.get(c).copied().unwrap_or(0) as f64 / size.get(c).copied().unwrap_or(0).max(1) as f64;
            !anchored.get(c).copied().unwrap_or(true) && share >= cfg.share
        })
        .collect();
    if !cand.iter().any(|&c| c) {
        return Some(RoofDrop {
            out,
            stats: Some((count, None)),
        });
    }
    let at = |k: usize| {
        let c = col_of.get(k).copied().unwrap_or(0);
        let (i, j) = (c % g.nx.max(1), c / g.nx.max(1));
        (i, j, g.min_x + i as f64 * g.pitch, g.min_z + j as f64 * g.pitch)
    };
    let bucket = |x: f64, z: f64| ((x / cfg.near).floor() as i64, (z / cfg.near).floor() as i64);
    let mut buck: FastMap<(i64, i64), Vec<P3>> = FastMap::default();
    for k in 0..len {
        let ci = isl(k);
        if ci >= 0 && keep.get(k) == Some(&true) && anchored.get(ci as usize) == Some(&true) {
            let (_, _, x, z) = at(k);
            buck.entry(bucket(x, z))
                .or_default()
                .push((x, z, g.y.get(k).copied().unwrap_or(0.0)));
        }
    }
    let every = ((0.8 / g.pitch).round_ties_even() as i64).max(1) as usize;
    let mut near = vec![false; count];
    for k in 0..len {
        let ci = isl(k);
        if ci < 0 || keep.get(k) != Some(&true) || !cand.get(ci as usize).copied().unwrap_or(false) {
            continue;
        }
        let ci = ci as usize;
        if near.get(ci).copied().unwrap_or(true) {
            continue;
        }
        let (i, j, x, z) = at(k);
        if i % every != 0 || j % every != 0 {
            continue;
        }
        let yk = g.y.get(k).copied().unwrap_or(0.0);
        let (bx, bz) = bucket(x, z);
        let found = (-1..=1).any(|dx| {
            (-1..=1).any(|dz| {
                buck.get(&(bx + dx, bz + dz)).is_some_and(|pts| {
                    pts.iter().any(|&(px, pz, py)| {
                        (py - yk).abs() <= cfg.near_dy && pymath::hypot(px - x, pz - z) <= cfg.near
                    })
                })
            })
        });
        if found && let Some(v) = near.get_mut(ci) {
            *v = true;
        }
    }
    let drop_c: Vec<bool> = cand.iter().zip(&near).map(|(&c, &n)| c && !n).collect();
    let mut surfaces = 0;
    for (k, o) in out.iter_mut().enumerate() {
        if let Ok(ci) = usize::try_from(isl(k))
            && keep.get(k) == Some(&true)
            && drop_c.get(ci) == Some(&true)
        {
            *o = true;
            surfaces += 1;
        }
    }
    let dropped = drop_c.iter().filter(|&&d| d).count();
    Some(RoofDrop {
        out,
        stats: Some((count, Some((dropped, surfaces)))),
    })
}

/// Points in square buckets of `size` metres (plan), for "which points lie in this box".
struct Buckets {
    size: f64,
    cells: FastMap<(i64, i64), Vec<usize>>,
}

impl Buckets {
    fn new(size: f64) -> Self {
        Buckets {
            size,
            cells: FastMap::default(),
        }
    }

    fn key(&self, v: f64) -> i64 {
        // a NaN or a far point goes to an end bucket: still found by a box reaching there
        let b = (v / self.size).floor();
        if b.is_nan() { 0 } else { b.clamp(-1e15, 1e15) as i64 }
    }

    fn add(&mut self, n: usize, x: f64, z: f64) {
        let key = (self.key(x), self.key(z));
        self.cells.entry(key).or_default().push(n);
    }

    /// Every point in buckets touching the box [x0, x1] x [z0, z1] (a superset of the points inside).
    fn visit(&self, x0: f64, x1: f64, z0: f64, z1: f64, mut f: impl FnMut(usize)) {
        let (a0, a1, b0, b1) = (self.key(x0), self.key(x1), self.key(z0), self.key(z1));
        if (a1 - a0 + 1).saturating_mul(b1 - b0 + 1) > (self.cells.len() as i64).max(1) * 4 {
            for &n in self.cells.values().flatten() {
                f(n);
            }
            return;
        }
        for a in a0..=a1 {
            for b in b0..=b1 {
                for &n in self.cells.get(&(a, b)).into_iter().flatten() {
                    f(n);
                }
            }
        }
    }
}

/// One area for [`cover_nodes`]: its cells' positions (`(ox + i * pitch, y + y_offset, oz + j * pitch)`).
pub type CoverArea = Vec<(f64, f64, f64)>;

/// `navbuild.add_cover_nodes`: for each area of 4+ cells, in order, the cells to offer to `add` - the cell farthest
/// from every node on its floor (|dy| < 2.5 m), until none is farther than `cover`. `nodes` holds the route nodes'
/// positions so far; `add(area, cell)` places a node on the cell when it can (one per surface) and returns the
/// positions of the nodes it added (none: the surface already held one), which join `nodes`.
pub fn cover_nodes(
    areas: &[CoverArea],
    nodes: &mut Vec<(f64, f64, f64)>,
    cover: f64,
    add: &mut dyn FnMut(usize, usize) -> Vec<(f64, f64, f64)>,
) {
    let mut grid = Buckets::new(8.0);
    for (n, p) in nodes.iter().enumerate() {
        grid.add(n, p.0, p.2);
    }
    for (ai, c) in areas.iter().enumerate() {
        if c.len() < 4 {
            continue;
        }
        let fold = |f: fn(f64, f64) -> f64, sel: fn(&(f64, f64, f64)) -> f64| {
            c.iter()
                .map(sel)
                .reduce(|a, b| if a.is_nan() || b.is_nan() { f64::NAN } else { f(a, b) })
        };
        let (Some(xmin), Some(xmax), Some(zmin), Some(zmax)) = (
            fold(f64::min, |p| p.0),
            fold(f64::max, |p| p.0),
            fold(f64::min, |p| p.2),
            fold(f64::max, |p| p.2),
        ) else {
            continue;
        };
        let (x0, x1, z0, z1) = (xmin - cover, xmax + cover, zmin - cover, zmax + cover);
        let mut d = vec![f64::INFINITY; c.len()];
        let lower = |d: &mut [f64], q: (f64, f64, f64)| {
            for (dv, p) in d.iter_mut().zip(c) {
                if (p.1 - q.1).abs() < 2.5 {
                    let h = (p.0 - q.0).hypot(p.2 - q.2);
                    // np.minimum: NaN wins
                    *dv = if dv.is_nan() || h.is_nan() { f64::NAN } else { dv.min(h) };
                }
            }
        };
        let mut near = Vec::new();
        grid.visit(x0, x1, z0, z1, |n| near.push(n));
        for n in near {
            if let Some(&q) = nodes.get(n)
                && x0 <= q.0
                && q.0 <= x1
                && z0 <= q.2
                && q.2 <= z1
            {
                lower(&mut d, q);
            }
        }
        loop {
            // np.argmax: the first of the largest (a NaN counts as the largest, the first one)
            let mut w = 0usize;
            let mut best = f64::NEG_INFINITY;
            for (n, &v) in d.iter().enumerate() {
                if v.is_nan() {
                    w = n;
                    best = v;
                    break;
                }
                if n == 0 || v > best {
                    w = n;
                    best = v;
                }
            }
            if best <= cover || d.is_empty() {
                break;
            }
            let added = add(ai, w);
            let Some(&q) = c.get(w) else { break };
            if let Some(v) = d.get_mut(w) {
                *v = 0.0;
            }
            if !added.is_empty() {
                for p in added {
                    grid.add(nodes.len(), p.0, p.2);
                    nodes.push(p);
                }
                lower(&mut d, q);
            }
        }
    }
}

/// `navbuild.gabriel_prune`: of the route pairs `(a, b, d)` over nodes at `pos`, the indices of those kept, in the
/// reference's order - the Gabriel pairs (no other node within 2.5 m of height inside the circle on the pair as
/// diameter) in input order, then the dropped pairs, shortest first, that join groups the kept ones leave apart.
/// `None` for a pair naming no node, or a NaN length.
#[must_use]
pub fn gabriel_prune(pos: &[(f64, f64, f64)], pairs: &[(usize, usize, f64)]) -> Option<Vec<usize>> {
    if pairs
        .iter()
        .any(|&(a, b, d)| a >= pos.len() || b >= pos.len() || d.is_nan())
    {
        return None;
    }
    let mut grid = Buckets::new(4.0);
    for (n, p) in pos.iter().enumerate() {
        grid.add(n, p.0, p.2);
    }
    let gabriel: Vec<bool> = pairs
        .par_iter()
        .map(|&(a, b, d)| {
            let (Some(pa), Some(pb)) = (pos.get(a), pos.get(b)) else {
                return false;
            };
            // (P[a] + P[b]) / 2 as NumPy computes it (not f64::midpoint, which differs near the float range's ends)
            #[allow(clippy::manual_midpoint)]
            let m = ((pa.0 + pb.0) / 2.0, (pa.1 + pb.1) / 2.0, (pa.2 + pb.2) / 2.0);
            let r2 = pymath::sq(d / 2.0) - 1e-9;
            // a node inside has dx * dx < r2, so |dx| < |d| / 2 (+ rounding): the box holds every such node
            let r = d.abs() / 2.0 + 1.0;
            let mut inside = false;
            grid.visit(m.0 - r, m.0 + r, m.2 - r, m.2 + r, |n| {
                if inside || n == a || n == b {
                    return;
                }
                if let Some(p) = pos.get(n) {
                    let (dx, dz) = (p.0 - m.0, p.2 - m.2);
                    if (p.1 - m.1).abs() < 2.5 && dx * dx + dz * dz < r2 {
                        inside = true;
                    }
                }
            });
            !inside
        })
        .collect();
    let mut parent: Vec<usize> = (0..pos.len()).collect();
    let mut keep = Vec::new();
    let mut rest = Vec::new();
    for (n, (&(a, b, _), &g)) in pairs.iter().zip(&gabriel).enumerate() {
        if g {
            keep.push(n);
            let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
            if let Some(v) = parent.get_mut(ra) {
                *v = rb;
            }
        } else {
            rest.push(n);
        }
    }
    let len = |n: usize| pairs.get(n).map_or(0.0, |p| p.2);
    rest.sort_by(|&x, &y| len(x).partial_cmp(&len(y)).unwrap_or(std::cmp::Ordering::Equal));
    for n in rest {
        let Some(&(a, b, _)) = pairs.get(n) else { continue };
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        if ra != rb {
            if let Some(v) = parent.get_mut(ra) {
                *v = rb;
            }
            keep.push(n);
        }
    }
    Some(keep)
}

/// A route length ordered as a float ([`spanner_prune`]'s queue; never NaN there).
#[derive(PartialEq)]
struct Len(f64);

impl Eq for Len {}

impl PartialOrd for Len {
    fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Len {
    fn cmp(&self, o: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&o.0)
    }
}

/// DICE's edge rule (KYG-EDGE-3): of the route pairs `(a, b, d)`, the indices of a greedy `stretch`-spanner, in
/// input order. The pairs are taken shortest first (ties by input order); a pair is kept unless the pairs kept so far
/// already join its two nodes by a route at most `stretch` times its length `d`. A spanner keeps every group the
/// pairs join. `None` for a pair naming no node, or a NaN length.
#[must_use]
pub fn spanner_prune(n_nodes: usize, pairs: &[(usize, usize, f64)], stretch: f64) -> Option<Vec<usize>> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    if pairs
        .iter()
        .any(|&(a, b, d)| a >= n_nodes || b >= n_nodes || d.is_nan())
    {
        return None;
    }
    let mut order: Vec<usize> = (0..pairs.len()).collect();
    order.sort_by(|&x, &y| {
        let (dx, dy) = (pairs.get(x).map_or(0.0, |p| p.2), pairs.get(y).map_or(0.0, |p| p.2));
        dx.total_cmp(&dy).then(x.cmp(&y))
    });
    let mut adj: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n_nodes];
    let mut dist = vec![f64::INFINITY; n_nodes];
    let mut touched: Vec<usize> = Vec::new();
    let mut keep = Vec::new();
    for n in order {
        let Some(&(a, b, d)) = pairs.get(n) else { continue };
        if a == b {
            continue;
        }
        let limit = stretch * d;
        // Dijkstra from a over the kept pairs, no farther than the limit
        let mut heap = BinaryHeap::new();
        if let Some(v) = dist.get_mut(a) {
            *v = 0.0;
        }
        touched.push(a);
        heap.push(Reverse((Len(0.0), a)));
        let mut found = false;
        while let Some(Reverse((Len(du), u))) = heap.pop() {
            if du > dist.get(u).copied().unwrap_or(f64::INFINITY) {
                continue;
            }
            if u == b {
                found = true;
                break;
            }
            for &(w, l) in adj.get(u).map_or(&[][..], Vec::as_slice) {
                let dw = du + l;
                if dw <= limit && dw < dist.get(w).copied().unwrap_or(f64::INFINITY) {
                    if let Some(v) = dist.get_mut(w) {
                        *v = dw;
                    }
                    touched.push(w);
                    heap.push(Reverse((Len(dw), w)));
                }
            }
        }
        for t in touched.drain(..) {
            if let Some(v) = dist.get_mut(t) {
                *v = f64::INFINITY;
            }
        }
        if !found {
            keep.push(n);
            if let Some(v) = adj.get_mut(a) {
                v.push((b, d));
            }
            if let Some(v) = adj.get_mut(b) {
                v.push((a, d));
            }
        }
    }
    keep.sort_unstable();
    Some(keep)
}

/// Union-find's root of x, halving the path (the reference's `find`).
fn find(parent: &mut [usize], mut x: usize) -> usize {
    while let Some(&p) = parent.get(x) {
        if p == x {
            break;
        }
        let gp = parent.get(p).copied().unwrap_or(p);
        if let Some(v) = parent.get_mut(x) {
            *v = gp;
        }
        x = gp;
    }
    x
}
