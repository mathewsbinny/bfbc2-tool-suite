//! NavKit's detail features of one area (round NK3): the **broken sides**, the **D lines** and the **F cells**
//! (`KYNAPSE_SPEC.md` 4.5, "D lines and F cells - the generator rule"). The Python (`navdetail.py`'s
//! `broken_sides`, `f_cell_sets`, `d_sides`, `corner_cuts`, `d_lines`, `_components`, `_quarters`, `_loops`,
//! `_simplify`, `f_cells`, `_f_point`) was retired in round NK3 against every call of every regression build
//! (`rust/BENCHMARKS.md` "NK3").
//!
//! DICE's rule, measured on its files:
//! - a **broken side** is a side between two straight-neighbour cells of the area whose ground heights differ by more
//!   than the link step (0.6 m at pitch 0.8), unless the caller leaves it out (a portal on it; a detailed zone's
//!   object cell). A 45-degree bridge's half cell (its legs, toward the area cells it touches) breaks the same way,
//!   with its own height, on both sides;
//! - an **F cell** is a cell whose two sides along one axis are both broken; the F cells of an axis form polygons
//!   (8-connected groups, 45-degree bridges through the empty cells beside a diagonal touch), each stored twice:
//!   `-axis`, every corner's height from the cells on the `-axis` side, then `+axis`;
//! - a **D line** is a broken side that is no F cell's side along its axis, stored once per side with its owner cell
//!   on the right, unit sides chained where the chain is unambiguous, collinear vertices dropped, each vertex at the
//!   mean height of the owner-side cells touching it; a cell owning D on two perpendicular sides has its corner cut
//!   (its own line the diagonal across it).
//!
//! **Order.** Everything here is ordered by the cells' and sides' grid coordinates, never by a hash: the sides and
//! cells are `BTreeSet`s, the unit segments sorted by their ends, the F groups by their first cell. The Python left
//! two choices to the iteration order of its sets (CPython's: hashes of the integer tuples and the set's probing), and
//! the spec now defines both (`rust/BENCHMARKS.md` "NK3": the first never occurs in the regression builds, the second
//! at 17 vertices of 15 levels; DICE's files hold neither shape):
//! - a loop of an F group's boundary that starts at a pinch (two boundary edges leaving its start vertex: a second
//!   piece or a hole of the group) leaves along the edge whose end vertex is least (x, then z); at a pinch on the way
//!   the loop takes the sharpest right turn, as before;
//! - an F polygon's vertex with no area cell on its side and no F cell touching it (the centre of an empty cell where
//!   45-degree bridges meet) takes the mean height of the nearest F cells (half-unit distance |dx| + |dz|) that lie
//!   furthest toward the record's side; the Python took the nearest cell its set happened to give first.
//!
//! **Floats** are computed as the Python computed them (sums left to right from zero, `/ len`, then `+ 0.95`; world
//! positions `ox + v * pitch / 2`), so the answers are the reference's bit for bit.

use std::collections::{BTreeMap, BTreeSet};

use crate::FastMap;

/// Stored heights are ground + `EntityHeight` / 2, as the outline's.
pub const Y_OFFSET: f64 = 0.95;

/// The four sides of a cell, in the order the reference visits them.
const SIDES: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

/// A cell of the sector's sample grid: `(i, j)`, local indices.
pub type GridCell = (i64, i64);

/// A side of a cell: the cell and the direction `(di, dj)` to the neighbour across it.
pub type Side = (GridCell, (i64, i64));

/// A world point `(x, y, z)`.
pub type Point = [f64; 3];

/// A vertex in half units: `(2 i +- 1, 2 j +- 1)` are a cell's corners, `(2 i, 2 j)` its centre.
type Half = (i64, i64);

/// The settings of one area's pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetailSettings {
    /// The grid's pitch (m).
    pub pitch: f64,
    /// The world x of cell column 0's centre.
    pub ox: f64,
    /// The world z of cell row 0's centre.
    pub oz: f64,
    /// The link step: a side breaks when its two heights differ by more (m).
    pub step: f64,
    /// F cells (off: their sides are D lines; detailed zones).
    pub with_f: bool,
    /// `CUT_CORNERS`: a cell with D on exactly two perpendicular sides has its own line across it.
    pub cut_corners: bool,
    /// `CHAIN_TURNS`: D lines continue round corners (off: straight runs only).
    pub chain_turns: bool,
}

/// An F record: its direction (`-0.0` zero components, as DICE stores them) and its closed polygon.
#[derive(Debug, Clone, PartialEq)]
pub struct FCell {
    /// `(-1, -0, -0)`, `(1, -0, -0)`, `(-0, -0, -1)` or `(-0, -0, 1)`.
    pub direction: Point,
    /// The polygon, counter-clockwise, its first point repeated at the end.
    pub points: Vec<Point>,
}

/// One area's detail features.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Features {
    /// The broken sides (with a bridge's half-cell legs, both ways).
    pub broken: BTreeSet<Side>,
    /// The x-F cells and the z-F cells.
    pub fx: BTreeSet<GridCell>,
    /// The z-F cells.
    pub fz: BTreeSet<GridCell>,
    /// The broken sides that are D sides (no F cell's side along its axis).
    pub d_sides: BTreeSet<Side>,
    /// The D polylines (world points), owner on the right.
    pub lines: Vec<Vec<Point>>,
    /// The F records: all `-x`, then `+x`, then `-z`, then `+z` (per axis: `-axis` records of every group, then
    /// `+axis`).
    pub cells: Vec<FCell>,
}

/// A side that steps more than the link step: the caller decides whether it breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Candidate {
    /// A side between two area cells (`eligible` and `breakable` are asked).
    Side(Side),
    /// A bridge half cell's leg toward an area cell (`breakable` is asked); it breaks both ways.
    Leg(Side),
}

/// The area's cells and the bridges' half cells: heights by cell.
struct Heights {
    cells: FastMap<GridCell, f64>,
    /// The cells with the half cells added (the first leg's height of each), for the D lines.
    with_half: FastMap<GridCell, f64>,
}

impl Heights {
    fn new(cells: &[(GridCell, f64)], half: &[(Side, f64)]) -> Self {
        let cells: FastMap<GridCell, f64> = cells.iter().copied().collect();
        let mut with_half = cells.clone();
        for &((c, _), y) in half {
            with_half.entry(c).or_insert(y);
        }
        Self { cells, with_half }
    }
}

/// The sides of the area stepping more than `step`, in the reference's order (cells in the given order, each side
/// in `SIDES` order; then the half cells' legs in the given order). `cells` holds each cell once.
#[must_use]
pub fn candidates(cells: &[(GridCell, f64)], half: &[(Side, f64)], step: f64) -> Vec<Candidate> {
    candidates_by(cells, half, step, |_| None)
}

/// [`candidates`], with `test` deciding a side between two area cells where it answers (DICE's terrain rule,
/// KYG-DF-1: the ground's slope across the side) and the step deciding where it does not.
#[must_use]
pub fn candidates_by(
    cells: &[(GridCell, f64)],
    half: &[(Side, f64)],
    step: f64,
    test: impl Fn(Side) -> Option<bool>,
) -> Vec<Candidate> {
    let at: FastMap<GridCell, f64> = cells.iter().copied().collect();
    let mut out = Vec::new();
    for &((i, j), y) in cells {
        for (di, dj) in SIDES {
            if at
                .get(&(i + di, j + dj))
                .is_some_and(|&n| test(((i, j), (di, dj))).unwrap_or((n - y).abs() > step))
            {
                out.push(Candidate::Side(((i, j), (di, dj))));
            }
        }
    }
    for &(((i, j), (di, dj)), y) in half {
        if at.get(&(i + di, j + dj)).is_some_and(|&n| (n - y).abs() > step) {
            out.push(Candidate::Leg(((i, j), (di, dj))));
        }
    }
    out
}

/// One area's features: [`candidates`], each kept where `keep` says it breaks, then [`features`].
///
/// # Errors
///
/// `keep`'s error, which stops the pass.
pub fn detail_features<E>(
    cells: &[(GridCell, f64)],
    half: &[(Side, f64)],
    s: &DetailSettings,
    keep: impl FnMut(&Candidate) -> Result<bool, E>,
) -> Result<Features, E> {
    detail_features_by(cells, half, s, |_| None, keep)
}

/// [`detail_features`] on [`candidates_by`]'s sides.
///
/// # Errors
///
/// `keep`'s error, which stops the pass.
pub fn detail_features_by<E>(
    cells: &[(GridCell, f64)],
    half: &[(Side, f64)],
    s: &DetailSettings,
    test: impl Fn(Side) -> Option<bool>,
    mut keep: impl FnMut(&Candidate) -> Result<bool, E>,
) -> Result<Features, E> {
    let mut broken = BTreeSet::new();
    for c in candidates_by(cells, half, s.step, test) {
        if keep(&c)? {
            match c {
                Candidate::Side(side) => {
                    broken.insert(side);
                }
                Candidate::Leg(((i, j), (di, dj))) => {
                    broken.insert(((i, j), (di, dj)));
                    broken.insert(((i + di, j + dj), (-di, -dj)));
                }
            }
        }
    }
    Ok(features(cells, half, broken, s))
}

/// The features from the broken sides: the F cells (`with_f`), the D sides and their lines, the F records.
#[must_use]
pub fn features(
    cells: &[(GridCell, f64)],
    half: &[(Side, f64)],
    broken: BTreeSet<Side>,
    s: &DetailSettings,
) -> Features {
    if broken.is_empty() {
        return Features::default();
    }
    let h = Heights::new(cells, half);
    let (fx, fz) = if s.with_f {
        f_cell_sets(&broken, &h.cells)
    } else {
        Default::default()
    };
    let d_sides = d_sides(&broken, &fx, &fz);
    let cuts = if s.cut_corners {
        corner_cuts(&d_sides)
    } else {
        BTreeMap::new()
    };
    let lines = d_lines(&d_sides, &h.with_half, s, &cuts);
    let mut fcells = f_cells(&fx, Axis::X, &h.cells, s);
    fcells.extend(f_cells(&fz, Axis::Z, &h.cells, s));
    Features {
        broken,
        fx,
        fz,
        d_sides,
        lines,
        cells: fcells,
    }
}

/// (x-F cells, z-F cells): the area's cells whose two sides along x (z) are both broken.
fn f_cell_sets(broken: &BTreeSet<Side>, cells: &FastMap<GridCell, f64>) -> (BTreeSet<GridCell>, BTreeSet<GridCell>) {
    let both = |c: GridCell, a: (i64, i64), b: (i64, i64)| {
        broken.contains(&(c, a)) && broken.contains(&(c, b)) && cells.contains_key(&c)
    };
    let fx = broken
        .iter()
        .filter(|&&(c, sd)| sd == (1, 0) && both(c, (1, 0), (-1, 0)))
        .map(|&(c, _)| c)
        .collect();
    let fz = broken
        .iter()
        .filter(|&&(c, sd)| sd == (0, 1) && both(c, (0, 1), (0, -1)))
        .map(|&(c, _)| c)
        .collect();
    (fx, fz)
}

/// The broken sides that are no F cell's side along its axis (on either side of the line).
fn d_sides(broken: &BTreeSet<Side>, fx: &BTreeSet<GridCell>, fz: &BTreeSet<GridCell>) -> BTreeSet<Side> {
    broken
        .iter()
        .filter(|&&((i, j), (di, dj))| {
            !(di != 0 && (fx.contains(&(i, j)) || fx.contains(&(i + di, j))))
                && !(dj != 0 && (fz.contains(&(i, j)) || fz.contains(&(i, j + dj))))
        })
        .copied()
        .collect()
}

/// Python's `sum(hs) / len(hs) + Y_OFFSET` (the sum from zero, left to right).
fn mean_up(hs: &[f64]) -> f64 {
    let mut sum = 0.0;
    for &h in hs {
        sum += h;
    }
    sum / hs.len() as f64 + Y_OFFSET
}

/// A D unit segment, directed (owner on the right), in half units, with its two vertex heights.
#[derive(Debug, Clone, Copy)]
struct Seg {
    a: Half,
    b: Half,
    ya: f64,
    yb: f64,
}

/// A D side as a directed unit segment: each vertex at the mean height of the owner-side cells touching it along
/// the line.
fn unit(side: Side, h: &FastMap<GridCell, f64>) -> Seg {
    let ((i, j), (di, dj)) = side;
    let (mut pa, mut pb, ta, tb, rev) = if di != 0 {
        let line = 2 * i + di;
        ((line, 2 * j - 1), (line, 2 * j + 1), 2 * j - 1, 2 * j + 1, di == 1) // owner on -x: runs -z
    } else {
        let line = 2 * j + dj;
        ((2 * i - 1, line), (2 * i + 1, line), 2 * i - 1, 2 * i + 1, dj == -1) // owner on +z: runs -x
    };
    let y_at = |t2: i64| {
        let hs: Vec<f64> = [(t2 - 1).div_euclid(2), (t2 + 1).div_euclid(2)]
            .iter()
            .map(|&t| if di != 0 { (i, t) } else { (t, j) })
            .filter_map(|c| h.get(&c).copied())
            .collect();
        mean_up(&hs)
    };
    let (mut ya, mut yb) = (y_at(ta), y_at(tb));
    if rev {
        std::mem::swap(&mut pa, &mut pb);
        std::mem::swap(&mut ya, &mut yb);
    }
    Seg { a: pa, b: pb, ya, yb }
}

/// `CUT_CORNERS`: the cells owning D on exactly two sides, perpendicular to each other, with those two sides
/// (sorted).
fn corner_cuts(sides: &BTreeSet<Side>) -> BTreeMap<GridCell, [(i64, i64); 2]> {
    let mut own: BTreeMap<GridCell, Vec<(i64, i64)>> = BTreeMap::new();
    for &(c, sd) in sides {
        own.entry(c).or_default().push(sd);
    }
    own.into_iter()
        .filter_map(|(c, sds)| match sds.as_slice() {
            &[s1, s2] if (s1.0 == 0) != (s2.0 == 0) => Some((c, if s1 <= s2 { [s1, s2] } else { [s2, s1] })),
            _ => None,
        })
        .collect()
}

/// The diagonal D segment of a cut corner, the cell on the right: between its two corners next to the cut one, each
/// at the mean height of the cell and its neighbour along that side's line.
#[allow(
    clippy::many_single_char_names,
    reason = "the reference's names: cell (i, j), sides sx / sz, ends a / b"
)]
fn cut(c: GridCell, sds: [(i64, i64); 2], h: &FastMap<GridCell, f64>) -> Seg {
    let (i, j) = c;
    // the x side, then the z side (a stable sort on "is a z side")
    let [s1, s2] = sds;
    let ((sx, _), (_, sz)) = if s1.0 == 0 && s2.0 != 0 { (s2, s1) } else { (s1, s2) };
    let mut a = (2 * i + sx, 2 * j - sz); // far end of the x side
    let mut b = (2 * i - sx, 2 * j + sz); // far end of the z side
    let own = h.get(&c).copied();
    let y = |other: GridCell| {
        let hs: Vec<f64> = own.into_iter().chain(h.get(&other).copied()).collect();
        mean_up(&hs)
    };
    let (mut ya, mut yb) = (y((i, j - sz)), y((i - sx, j)));
    // the cell's remaining half is on the far side from the cut corner: it must be on the right
    let (dx, dz) = (b.0 - a.0, b.1 - a.1);
    if dz * -sx + -dx * -sz < 0 {
        std::mem::swap(&mut a, &mut b);
        std::mem::swap(&mut ya, &mut yb);
    }
    Seg { a, b, ya, yb }
}

fn world(s: &DetailSettings, p: Half, y: f64) -> Point {
    [s.ox + p.0 as f64 * s.pitch / 2.0, y, s.oz + p.1 as f64 * s.pitch / 2.0]
}

/// The D polylines from the D sides: unit segments chained end to start where the chain is unambiguous (one way
/// on, not back along the facing twin), collinear vertices dropped; the cut corners' diagonals last, lines of their
/// own.
fn d_lines(
    sides: &BTreeSet<Side>,
    h: &FastMap<GridCell, f64>,
    s: &DetailSettings,
    cuts: &BTreeMap<GridCell, [(i64, i64); 2]>,
) -> Vec<Vec<Point>> {
    let mut segs: Vec<Seg> = sides
        .iter()
        .filter(|(c, sd)| !cuts.get(c).is_some_and(|sds| sds.contains(sd)))
        .map(|&side| unit(side, h))
        .collect();
    // (start, end) is unique: the reference's sort on (start, end, ys, ye)
    segs.sort_by_key(|g| (g.a, g.b));
    let mut starts: FastMap<Half, Vec<usize>> = FastMap::default();
    for (k, g) in segs.iter().enumerate() {
        starts.entry(g.a).or_default().push(k);
    }
    let n = segs.len();
    let mut prv_count = vec![0usize; n];
    let mut cand: Vec<Vec<usize>> = Vec::with_capacity(n);
    for g in &segs {
        let dir = (g.b.0 - g.a.0, g.b.1 - g.a.1);
        let c: Vec<usize> = starts.get(&g.b).map_or_else(Vec::new, |ts| {
            ts.iter()
                .copied()
                .filter(|&t| {
                    segs.get(t)
                        .is_some_and(|o| o.b != g.a && (s.chain_turns || (o.b.0 - g.b.0, o.b.1 - g.b.1) == dir))
                })
                .collect()
        });
        for &t in &c {
            if let Some(p) = prv_count.get_mut(t) {
                *p += 1;
            }
        }
        cand.push(c);
    }
    let mut nxt: Vec<Option<usize>> = vec![None; n];
    let mut has_prev = vec![false; n];
    for (k, c) in cand.iter().enumerate() {
        if let &[t] = c.as_slice()
            && prv_count.get(t) == Some(&1)
        {
            if let Some(x) = nxt.get_mut(k) {
                *x = Some(t);
            }
            if let Some(x) = has_prev.get_mut(t) {
                *x = true;
            }
        }
    }
    let mut out = Vec::new();
    let mut used = vec![false; n];
    let chain_from = |k: usize, used: &mut Vec<bool>| {
        let mut chain = vec![k];
        if let Some(u) = used.get_mut(k) {
            *u = true;
        }
        while let Some(Some(t)) = chain.last().and_then(|&l| nxt.get(l)).copied() {
            if used.get(t).copied().unwrap_or(true) {
                break;
            }
            if let Some(u) = used.get_mut(t) {
                *u = true;
            }
            chain.push(t);
        }
        emit(&chain, &segs, s)
    };
    for k in 0..n {
        if used.get(k) == Some(&false) && has_prev.get(k) == Some(&false) {
            out.push(chain_from(k, &mut used));
        }
    }
    for k in 0..n {
        // closed loops (round one cell)
        if used.get(k) == Some(&false) {
            out.push(chain_from(k, &mut used));
        }
    }
    for (&c, &sds) in cuts {
        let g = cut(c, sds, h);
        out.push(vec![world(s, g.a, g.ya), world(s, g.b, g.yb)]);
    }
    out
}

/// A chain of unit segments as a polyline: a joint at the mean of both segments' heights, collinear vertices
/// dropped.
fn emit(chain: &[usize], segs: &[Seg], s: &DetailSettings) -> Vec<Point> {
    let mut pts: Vec<(Half, f64)> = Vec::with_capacity(chain.len() + 1);
    for (q, g) in chain.iter().filter_map(|&k| segs.get(k)).enumerate() {
        if q == 0 {
            pts.push((g.a, g.ya));
        } else if let Some(last) = pts.last_mut() {
            // the reference's (a + b) / 2.0, not f64::midpoint
            #[allow(clippy::manual_midpoint, reason = "the reference's float operations")]
            {
                last.1 = (last.1 + g.ya) / 2.0;
            }
        }
        pts.push((g.b, g.yb));
    }
    let mut keep: Vec<(Half, f64)> = pts.first().copied().into_iter().collect();
    for w in pts.windows(3) {
        let (Some(&(p0, _)), [_, (p1, y1), (p2, _)]) = (keep.last(), w) else {
            continue;
        };
        let d0 = (p1.0 - p0.0, p1.1 - p0.1);
        let d1 = (p2.0 - p1.0, p2.1 - p1.1);
        if d0.0 * d1.1 - d0.1 * d1.0 != 0 {
            keep.push((*p1, *y1));
        }
    }
    if pts.len() > 1
        && let Some(&last) = pts.last()
    {
        keep.push(last);
    }
    keep.into_iter().map(|(p, y)| world(s, p, y)).collect()
}

/// An F record's axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Axis {
    X,
    Z,
}

/// 8-connected groups of cells, each sorted; groups in order of their first cell (x, then z: DICE's order).
fn components(cells: &BTreeSet<GridCell>) -> Vec<Vec<GridCell>> {
    let mut seen: BTreeSet<GridCell> = BTreeSet::new();
    let mut out = Vec::new();
    for &c in cells {
        if !seen.insert(c) {
            continue;
        }
        let (mut comp, mut stack) = (Vec::new(), vec![c]);
        while let Some((i, j)) = stack.pop() {
            comp.push((i, j));
            for di in -1..=1 {
                for dj in -1..=1 {
                    let n = (i + di, j + dj);
                    if cells.contains(&n) && seen.insert(n) {
                        stack.push(n);
                    }
                }
            }
        }
        comp.sort_unstable();
        out.push(comp);
    }
    out
}

/// Quarter triangles `(cell, base side)` covering a group: its cells, plus for each 45-degree bridge (two cells
/// touching only at a corner) the two quarters of each empty cell beside that corner whose bases touch it.
fn quarters(comp: &[GridCell]) -> BTreeSet<Side> {
    let cs: BTreeSet<GridCell> = comp.iter().copied().collect();
    let mut q = BTreeSet::new();
    for &(i, j) in comp {
        for sd in SIDES {
            q.insert(((i, j), sd));
        }
    }
    for &(i, j) in comp {
        for (di, dj) in [(1, 1), (1, -1)] {
            if cs.contains(&(i + di, j + dj)) && !cs.contains(&(i + di, j)) && !cs.contains(&(i, j + dj)) {
                q.insert(((i, j + dj), (0, -dj)));
                q.insert(((i, j + dj), (di, 0)));
                q.insert(((i + di, j), (-di, 0)));
                q.insert(((i + di, j), (0, dj)));
            }
        }
    }
    q
}

/// The turn from direction `a` to `b` (radians; negative: right): the sort key at a pinch.
fn turn(a: (i64, i64), b: (i64, i64)) -> f64 {
    let cross = (a.0 * b.1 - a.1 * b.0) as f64;
    let dot = (a.0 * b.0 + a.1 * b.1) as f64;
    cross.atan2(dot)
}

/// The boundary of a union of quarter triangles: closed loops of half-unit vertices, counter-clockwise (x right,
/// z up; inside on the left). Each loop starts at the least vertex left with an edge out of it, along the edge
/// whose end is least; at a pinch on the way it takes the sharpest right turn (the loops stay simple).
fn loops(quarters: &BTreeSet<Side>) -> Vec<Vec<Half>> {
    // each triangle's edges counter-clockwise; an edge also present the other way round is interior: the net count
    // of each direction
    let mut net: BTreeMap<(Half, Half), i64> = BTreeMap::new();
    for &((i, j), (sx, sz)) in quarters {
        let c = (2 * i, 2 * j);
        let (mut b0, mut b1) = if sx != 0 {
            ((2 * i + sx, 2 * j - 1), (2 * i + sx, 2 * j + 1))
        } else {
            ((2 * i - 1, 2 * j + sz), (2 * i + 1, 2 * j + sz))
        };
        if (b0.0 - c.0) * (b1.1 - c.1) - (b0.1 - c.1) * (b1.0 - c.0) < 0 {
            std::mem::swap(&mut b0, &mut b1);
        }
        for (p, r) in [(c, b0), (b0, b1), (b1, c)] {
            if p <= r {
                *net.entry((p, r)).or_default() += 1;
            } else {
                *net.entry((r, p)).or_default() -= 1;
            }
        }
    }
    let mut nxt: BTreeMap<Half, Vec<Half>> = BTreeMap::new();
    for (&(p, r), &n) in &net {
        let (from, to, times) = if n >= 0 { (p, r, n) } else { (r, p, -n) };
        for _ in 0..times {
            nxt.entry(from).or_default().push(to);
        }
    }
    let mut out = Vec::new();
    while let Some(p0) = nxt.iter().find(|(_, v)| !v.is_empty()).map(|(&p, _)| p) {
        let (mut lp, mut p, mut prev) = (vec![p0], p0, None::<Half>);
        while let Some(cand) = nxt.get_mut(&p) {
            let pick = match prev {
                None => cand.iter().enumerate().min_by_key(|&(_, r)| *r).map(|(k, _)| k),
                Some(pv) => {
                    let din = (p.0 - pv.0, p.1 - pv.1);
                    let key = |r: &Half| turn(din, (r.0 - p.0, r.1 - p.1));
                    cand.iter()
                        .enumerate()
                        .min_by(|(_, x), (_, y)| key(x).total_cmp(&key(y)))
                        .map(|(k, _)| k)
                }
            };
            let Some(k) = pick else { break };
            let r = cand.remove(k);
            prev = Some(p);
            p = r;
            if p == p0 {
                break;
            }
            lp.push(p);
        }
        out.push(lp);
    }
    out
}

/// A loop without its collinear vertices (a vertex where the boundary turns back is kept), rotated to start at
/// `keep` when that is one of its vertices, which is kept.
fn simplify(lp: &[Half], keep: Half) -> Vec<Half> {
    let n = lp.len();
    let k0 = lp.iter().position(|&p| p == keep).unwrap_or(0);
    let rot: Vec<Half> = lp.iter().cycle().skip(k0).take(n).copied().collect();
    let mut out = Vec::with_capacity(n);
    for (k, &p) in rot.iter().enumerate() {
        let (Some(&a), Some(&b)) = (rot.get((k + n - 1) % n), rot.get((k + 1) % n)) else {
            continue;
        };
        let d0 = (p.0 - a.0, p.1 - a.1);
        let d1 = (b.0 - p.0, b.1 - p.1);
        if k == 0 || d0.0 * d1.1 - d0.1 * d1.0 != 0 || d0.0 * d1.0 + d0.1 * d1.1 < 0 {
            out.push(p);
        }
    }
    out
}

/// The F records of the cells `fset` along x (`x`) or z, heights from `cells` (`navdetail.f_cells`).
#[must_use]
pub fn f_records(fset: &BTreeSet<GridCell>, x: bool, cells: &[(GridCell, f64)], s: &DetailSettings) -> Vec<FCell> {
    let at: FastMap<GridCell, f64> = cells.iter().copied().collect();
    f_cells(fset, if x { Axis::X } else { Axis::Z }, &at, s)
}

/// F records of one axis: the `-axis` record of every group (in order of their first cell), then the `+axis` ones.
/// A `-x` record starts at its group's first cell's top-left corner, the others at its bottom-left corner.
fn f_cells(fset: &BTreeSet<GridCell>, axis: Axis, cells: &FastMap<GridCell, f64>, s: &DetailSettings) -> Vec<FCell> {
    let groups: Vec<(GridCell, Vec<Vec<Half>>)> = components(fset)
        .into_iter()
        .filter_map(|comp| Some((*comp.first()?, loops(&quarters(&comp)))))
        .collect();
    let mut out = Vec::new();
    for sign in [-1i64, 1] {
        let direction = match (axis, sign) {
            (Axis::X, -1) => [-1.0, -0.0, -0.0],
            (Axis::X, _) => [1.0, -0.0, -0.0],
            (Axis::Z, -1) => [-0.0, -0.0, -1.0],
            (Axis::Z, _) => [-0.0, -0.0, 1.0],
        };
        for ((i0, j0), lps) in &groups {
            let start = if axis == Axis::X && sign == -1 {
                (2 * i0 - 1, 2 * j0 + 1)
            } else {
                (2 * i0 - 1, 2 * j0 - 1)
            };
            for lp in lps {
                let mut pts = simplify(lp, start);
                if let Some(&first) = pts.first() {
                    pts.push(first);
                }
                out.push(FCell {
                    direction,
                    points: pts.iter().map(|&p| f_point(p, axis, sign, cells, fset, s)).collect(),
                });
            }
        }
    }
    out
}

/// An F polygon vertex as a world point; its height the mean of the area cells on the `sign` side of it along the
/// axis that touch it (a corner: the two cells beside it; a side's middle or a centre: the next cell); with none
/// there, the F cells touching the vertex, else the mean of [`nearest_f_cells`].
fn f_point(
    p: Half,
    axis: Axis,
    sign: i64,
    cells: &FastMap<GridCell, f64>,
    fset: &BTreeSet<GridCell>,
    s: &DetailSettings,
) -> Point {
    let (vx, vz) = p;
    let along = |v: i64| -> Vec<i64> {
        if v.rem_euclid(2) != 0 {
            vec![(v + sign).div_euclid(2)]
        } else {
            vec![v.div_euclid(2) + sign]
        }
    };
    let across = |v: i64| -> Vec<i64> {
        if v.rem_euclid(2) != 0 {
            vec![(v - 1).div_euclid(2), (v + 1).div_euclid(2)]
        } else {
            vec![v.div_euclid(2)]
        }
    };
    let (ci, cj) = match axis {
        Axis::X => (along(vx), across(vz)),
        Axis::Z => (across(vx), along(vz)),
    };
    let mut hs: Vec<f64> = ci
        .iter()
        .flat_map(|&a| cj.iter().map(move |&b| (a, b)))
        .filter_map(|c| cells.get(&c).copied())
        .collect();
    if hs.is_empty() {
        // the F cells touching the vertex, in (i, j) order
        let near = |v: i64| (v - 1).div_euclid(2)..=(v + 1).div_euclid(2);
        hs = near(vx)
            .flat_map(|a| near(vz).map(move |b| (a, b)))
            .filter(|&(a, b)| (2 * a - vx).abs() <= 1 && (2 * b - vz).abs() <= 1 && fset.contains(&(a, b)))
            .filter_map(|c| cells.get(&c).copied())
            .collect();
    }
    if hs.is_empty() {
        hs = nearest_f_cells(fset, p, axis, sign)
            .iter()
            .filter_map(|c| cells.get(c).copied())
            .collect();
    }
    let y = if hs.is_empty() { f64::NAN } else { mean_up(&hs) };
    [s.ox + vx as f64 * s.pitch / 2.0, y, s.oz + vz as f64 * s.pitch / 2.0]
}

/// The F cells nearest to vertex `p` (half units: the least |dx| + |dz|), of them those furthest toward the record's
/// side (`sign` along `axis`: its heights come from that side, as DICE's do), in (i, j) order.
fn nearest_f_cells(fset: &BTreeSet<GridCell>, p: Half, axis: Axis, sign: i64) -> Vec<GridCell> {
    let dist = |&(a, b): &GridCell| (2 * a - p.0).abs() + (2 * b - p.1).abs();
    let toward = |&(a, b): &GridCell| sign * if axis == Axis::X { a } else { b };
    let Some(best) = fset.iter().map(dist).min() else {
        return Vec::new();
    };
    let near: Vec<GridCell> = fset.iter().copied().filter(|c| dist(c) == best).collect();
    let far = near.iter().map(toward).max();
    near.into_iter().filter(|c| Some(toward(c)) == far).collect()
}

#[cfg(test)]
#[path = "detail_tests.rs"]
mod detail_tests;
