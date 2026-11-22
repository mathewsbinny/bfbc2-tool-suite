//! `navbuild.trace_edges` and `navbuild.trace_emit` (with `bridge_union`): an area's boundary, side by side, then
//! merged into the runs of the mesh's portals, border portals and outline (round E3c).
//!
//! The Python keeps each area's sides in a `defaultdict(list)` keyed `(kind, target, axis, line, dir)` whose rows are
//! `(along, y)`; its insertion order is the order the runs come out in, so [`EdgeRows`] keeps the keys in insertion
//! order too. A line is a half-sample value (`i + di * 0.5`): held here doubled, as an integer ([`EdgeKey::line2`]).
//! The positions are computed with the reference's operations in `f64` (exact the same), the run order by the
//! reference's sort (`(along, y)` tuples, stable).

use std::collections::HashMap;
use std::hash::Hash;

use crate::{Cell, FastMap, FastSet, Level, NavError, Rule};

/// A map that keeps its keys in insertion order (a Python `dict`): a key set again keeps its place.
#[derive(Debug, Clone, PartialEq)]
pub struct Ordered<K: Eq + Hash + Copy, V> {
    keys: Vec<K>,
    vals: Vec<V>,
    at: FastMap<K, usize>,
}

impl<K: Eq + Hash + Copy, V> Default for Ordered<K, V> {
    fn default() -> Self {
        Ordered {
            keys: Vec::new(),
            vals: Vec::new(),
            at: FastMap::default(),
        }
    }
}

impl<K: Eq + Hash + Copy, V> Ordered<K, V> {
    /// The value of `k`, if set.
    #[must_use]
    pub fn get(&self, k: &K) -> Option<&V> {
        self.at.get(k).and_then(|&n| self.vals.get(n))
    }

    /// The value of `k` to change, if set.
    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        let n = *self.at.get(k)?;
        self.vals.get_mut(n)
    }

    /// The value of `k`, set to `make()` first when it has none (`defaultdict`'s `d[k]`).
    #[allow(clippy::indexing_slicing)] // `at` holds only indices of `vals`
    pub fn entry(&mut self, k: K, make: impl FnOnce() -> V) -> &mut V {
        let n = if let Some(&n) = self.at.get(&k) {
            n
        } else {
            self.keys.push(k);
            self.vals.push(make());
            self.at.insert(k, self.keys.len() - 1);
            self.keys.len() - 1
        };
        &mut self.vals[n]
    }

    /// `d[k] = v`: a new key goes last, a known one keeps its place.
    pub fn insert(&mut self, k: K, v: V) {
        if let Some(x) = self.get_mut(&k) {
            *x = v;
        } else {
            self.keys.push(k);
            self.vals.push(v);
            self.at.insert(k, self.keys.len() - 1);
        }
    }

    /// Set from pairs in order; `None` when a key comes twice.
    pub fn from_pairs(pairs: impl IntoIterator<Item = (K, V)>) -> Option<Self> {
        let mut out = Ordered::default();
        for (k, v) in pairs {
            if out.get(&k).is_some() {
                return None;
            }
            out.insert(k, v);
        }
        Some(out)
    }

    /// The keys and values in insertion order.
    pub fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.keys.iter().zip(&self.vals)
    }

    /// The keys and values to change, in insertion order.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.keys.iter().zip(self.vals.iter_mut())
    }

    /// The number of keys.
    #[must_use]
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// No keys.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }
}

/// What lies across a side: `trace_edges`' kinds and their targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// `'B'`: a portal to the area of this index (same sector).
    Portal(usize),
    /// `'C'`: a border portal, out of the sector window in direction (di, dj).
    Border(i64, i64),
    /// `'E'`: outline.
    Outline,
}

/// A key of an area's edge rows: `(kind, target, axis, line, dir)`, the line doubled (`2 * i + di`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EdgeKey {
    /// Kind and target.
    pub kind: Kind,
    /// `'x'` (a line x = const) or `'z'`.
    pub x_axis: bool,
    /// The line, doubled: the reference's `i + di * 0.5` is `line2 / 2`.
    pub line2: i64,
    /// The side's direction along the axis (+1 / -1).
    pub dir: i64,
}

impl EdgeKey {
    /// The line as the reference holds it (`i + di * 0.5`).
    #[must_use]
    pub fn line(&self) -> f64 {
        self.line2 as f64 / 2.0
    }
}

/// One area's edge rows: per key the sides `(along, y)` in order.
pub type EdgeRows = Ordered<EdgeKey, Vec<(i64, f64)>>;

/// A side of a cell: (x axis, doubled line, dir, along) - `trace_edges`' `side`.
type Side = (bool, i64, i64, i64);

/// What `trace_edges` found across a side (its `side_kind`): `'I'` interior or a kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SideKind {
    Interior,
    Of(Kind),
}

/// A one-way portal candidate: (source area, target area, key on the source, along).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OneWay {
    /// The source area.
    pub src: usize,
    /// The target area.
    pub target: usize,
    /// The portal's key on the source area.
    pub key: EdgeKey,
    /// The side's position along the line.
    pub along: i64,
}

/// The sector-wide settings `trace_edges` / `trace_emit` read.
#[derive(Debug, Clone, Copy)]
pub struct TraceCfg {
    /// The sector's sample origin (global i, j of local 0, 0).
    pub gi0: i64,
    /// See `gi0`.
    pub gj0: i64,
    /// The sector window's local sample range (start, stop).
    pub window: (i64, i64),
    /// `cfg.y_offset`.
    pub y_offset: f64,
    /// `LINK_RULE`.
    pub rule: Rule,
}

const DIRS4: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

fn side_of(i: i64, j: i64, di: i64, dj: i64) -> Side {
    if di != 0 {
        (true, 2 * i + di, di, j)
    } else {
        (false, 2 * j + dj, dj, i)
    }
}

/// `area_of` as the build makes it: each cell's surface to its area's index (the last area holding it).
#[must_use]
pub fn area_of(areas: &[Vec<Cell>]) -> FastMap<usize, usize> {
    let mut out = FastMap::default();
    out.reserve(areas.iter().map(Vec::len).sum());
    for (ai, cells) in areas.iter().enumerate() {
        for c in cells {
            out.insert(c.2, ai);
        }
    }
    out
}

/// `navbuild.trace_edges` (with `SYMMETRIC_PORTALS` = `symmetric`): every area's boundary sides, and the one-way
/// portal candidates.
///
/// # Errors
/// [`NavError::Shape`] for an `area_of` value that is no area (the reference fails on it).
#[allow(clippy::too_many_lines)]
pub fn trace_edges<S: std::hash::BuildHasher>(
    level: &Level,
    areas: &[Vec<Cell>],
    area_of: &HashMap<usize, usize, S>,
    cfg: TraceCfg,
    symmetric: bool,
) -> Result<(Vec<EdgeRows>, Vec<OneWay>), NavError> {
    let inside = |v: i64| cfg.window.0 <= v && v < cfg.window.1;
    let mut per_area: Vec<EdgeRows> = Vec::with_capacity(areas.len());
    let mut side_kind: Vec<FastMap<Side, SideKind>> = Vec::with_capacity(areas.len());
    // target area -> {(source area, side): y of the target's surface there}, both in insertion order
    let mut mirror: Ordered<usize, Ordered<(usize, Side), f64>> = Ordered::default();
    for (ai, cells) in areas.iter().enumerate() {
        let mut edges = EdgeRows::default();
        let mut sides: FastMap<Side, SideKind> = FastMap::default();
        sides.reserve(cells.len() * 2);
        let sides_n = level.side_neighbours(cells, cfg.gi0, cfg.gj0, cfg.rule);
        for (ci, &(i, j, k)) in cells.iter().enumerate() {
            let yk = level.y(k) + cfg.y_offset;
            for (d, (di, dj)) in DIRS4.into_iter().enumerate() {
                let side = side_of(i, j, di, dj);
                let n = sides_n.get(4 * ci + d).copied().flatten();
                let kind = match n {
                    Some(n) if inside(i + di) && inside(j + dj) => match area_of.get(&n) {
                        Some(&t) if t == ai => {
                            sides.insert(side, SideKind::Interior);
                            continue;
                        }
                        Some(&t) => Kind::Portal(t),
                        None => Kind::Outline,
                    },
                    Some(_) => Kind::Border(di, dj),
                    None => Kind::Outline,
                };
                let (x_axis, line2, dir, along) = side;
                edges
                    .entry(
                        EdgeKey {
                            kind,
                            x_axis,
                            line2,
                            dir,
                        },
                        Vec::new,
                    )
                    .push((along, yk));
                sides.insert(side, SideKind::Of(kind));
                if let (Kind::Portal(t), Some(n)) = (kind, n) {
                    let back = (x_axis, line2, -dir, along);
                    mirror
                        .entry(t, Ordered::default)
                        .insert((ai, back), level.y(n) + cfg.y_offset);
                }
            }
        }
        per_area.push(edges);
        side_kind.push(sides);
    }
    let mut one_way = Vec::new();
    if symmetric {
        let no_area = |t: usize| NavError::Shape(format!("area {t} is not an area of the sector"));
        for (&t, need) in mirror.iter() {
            if t >= per_area.len() {
                return Err(no_area(t));
            }
            for (&(src, side), &y) in need.iter() {
                let (x_axis, line2, d, along) = side;
                let there = side_kind
                    .get(t)
                    .and_then(|m| m.get(&side))
                    .copied()
                    .unwrap_or(SideKind::Of(Kind::Outline));
                if there == SideKind::Of(Kind::Portal(src)) {
                    continue;
                }
                if there != SideKind::Of(Kind::Outline) {
                    let key = EdgeKey {
                        kind: Kind::Portal(t),
                        x_axis,
                        line2,
                        dir: -d,
                    };
                    if per_area
                        .get(src)
                        .and_then(|e| e.get(&key))
                        .is_some_and(|rows| rows.iter().any(|r| r.0 == along))
                    {
                        one_way.push(OneWay {
                            src,
                            target: t,
                            key,
                            along,
                        });
                    }
                    continue;
                }
                let Some(edges) = per_area.get_mut(t) else {
                    return Err(no_area(t));
                };
                let outline = EdgeKey {
                    kind: Kind::Outline,
                    x_axis,
                    line2,
                    dir: d,
                };
                if let Some(rows) = edges.get_mut(&outline) {
                    rows.retain(|r| r.0 != along);
                }
                edges
                    .entry(
                        EdgeKey {
                            kind: Kind::Portal(src),
                            x_axis,
                            line2,
                            dir: d,
                        },
                        Vec::new,
                    )
                    .push((along, y));
                if let Some(m) = side_kind.get_mut(t) {
                    m.insert(side, SideKind::Of(Kind::Portal(src)));
                }
            }
        }
    }
    Ok((per_area, one_way))
}

/// A point (x, y, z).
pub type P3 = (f64, f64, f64);

/// A border portal's run: its two ends, its direction (di, dj), the first and last `along` and the line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderRun {
    /// The run's start.
    pub p0: P3,
    /// The run's end.
    pub p1: P3,
    /// The direction out of the sector.
    pub dir: (i64, i64),
    /// The first side's position along the line.
    pub first: i64,
    /// The last side's position along the line.
    pub last: i64,
    /// The line (`i + di * 0.5`).
    pub line: f64,
}

/// One area's traced boundary (`trace_emit`'s (portals, borders, outline)).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Emitted {
    /// Portals: (p0, p1, target area).
    pub portals: Vec<(P3, P3, usize)>,
    /// Border portals.
    pub borders: Vec<BorderRun>,
    /// Outline segments.
    pub outline: Vec<(P3, P3)>,
}

/// A quarter of an empty cell beside a bridge: (cell i, j, base side (sx, sz)).
type Quarter = (i64, i64, (i64, i64));

/// A quarter's bridge: (bridge index, ya, yb, side toward a, side toward b).
type QuarterOf = (usize, f64, f64, (i64, i64), (i64, i64));

/// A bridge: (i, j, di, dj) of its cell A; B is (i + di, j + dj).
type Bridge = (i64, i64, i64, i64);

/// The sides a bridge covers: (x axis, doubled line, along).
type Covered = FastSet<(bool, i64, i64)>;

/// `navbuild.bridge_union`: the covered sides (as (x axis, doubled line, along)) and the outline segments
/// `((ax, az, ya), (bx, bz, yb))` in local sample units, of an area's bridges.
fn bridge_union(
    level: &Level,
    bridges: &[Bridge],
    cell_k: &FastMap<(i64, i64), usize>,
    y_offset: f64,
) -> (Covered, Vec<(P3, P3)>) {
    let yof = |c: (i64, i64)| cell_k.get(&c).map_or(0.0, |&k| level.y(k)) + y_offset;
    // (bridge, ya, yb, toward a, toward b) per quarter, first one wins
    let mut quarters: Ordered<Quarter, QuarterOf> = Ordered::default();
    for (bi, &(i, j, di, dj)) in bridges.iter().enumerate() {
        let (ya, yb) = (yof((i, j)), yof((i + di, j + dj)));
        for ((ei, ej), ta, tb) in [((i, j + dj), (0, -dj), (di, 0)), ((i + di, j), (-di, 0), (0, dj))] {
            for side in [ta, tb] {
                quarters.entry((ei, ej, side), || (bi, ya, yb, ta, tb));
            }
        }
    }
    let corners = |ei: i64, ej: i64, (sx, sz): (i64, i64)| {
        if sx != 0 {
            [(2 * ei + sx, 2 * ej - 1), (2 * ei + sx, 2 * ej + 1)]
        } else {
            [(2 * ei - 1, 2 * ej + sz), (2 * ei + 1, 2 * ej + sz)]
        }
    };
    let mut count: FastMap<((i64, i64), i64, i64), u32> = FastMap::default();
    let mut drop = FastSet::default();
    for (&(ei, ej, side), _) in quarters.iter() {
        for c in corners(ei, ej, side) {
            *count.entry((c, ei, ej)).or_insert(0) += 1;
        }
        let (sx, sz) = side;
        drop.insert(if sx != 0 {
            (true, 2 * ei + sx, ej)
        } else {
            (false, 2 * ej + sz, ei)
        });
    }
    let mut segs = Vec::new();
    let mut done: FastSet<(i64, i64, usize)> = FastSet::default();
    let mut emitted: FastSet<((i64, i64), i64, i64)> = FastSet::default();
    for (&(ei, ej, _side), &(bi, ya, yb, ta, tb)) in quarters.iter() {
        if !done.insert((ei, ej, bi)) {
            continue;
        }
        let pick = |toward: (i64, i64)| -> Vec<(i64, i64)> {
            if quarters.get(&(ei, ej, toward)).is_none() {
                return Vec::new();
            }
            corners(ei, ej, toward)
                .into_iter()
                .filter(|&c| count.get(&(c, ei, ej)) == Some(&1) && !emitted.contains(&(c, ei, ej)))
                .collect()
        };
        let (ca, cb) = (pick(ta), pick(tb));
        emitted.extend(ca.iter().chain(&cb).map(|&c| (c, ei, ej)));
        #[allow(clippy::manual_midpoint)] // the reference's (ya + yb) / 2.0
        let ctr = (ei as f64, ej as f64, (ya + yb) / 2.0);
        let half = |c: (i64, i64), y: f64| (c.0 as f64 / 2.0, c.1 as f64 / 2.0, y);
        if let ([a], [b]) = (ca.as_slice(), cb.as_slice()) {
            segs.push((half(*a, ya), half(*b, yb)));
        } else {
            segs.extend(ca.iter().map(|&c| (half(c, ya), ctr)));
            segs.extend(cb.iter().map(|&c| (ctr, half(c, yb))));
        }
    }
    (drop, segs)
}

/// `navbuild.trace_emit` with `BRIDGE_UNION`: per area its portals, border portals and outline, from its edge rows
/// (consumed: the rows are sorted and the bridged outline sides dropped, as the reference does in place); (ox, oz) the
/// sector's origin in metres, `pitch` the grid's.
#[must_use]
pub fn trace_emit(
    level: &Level,
    areas: &[Vec<Cell>],
    per_area: Vec<EdgeRows>,
    cfg: TraceCfg,
    ox: f64,
    oz: f64,
    pitch: f64,
) -> Vec<Emitted> {
    trace_emit_cut(level, areas, per_area, cfg, (ox, oz, pitch), None)
}

/// [`trace_emit`], with each border portal's run also cut where `along` reaches 0 or `square` (the sector square's
/// lines, `square` its samples across) when `square` is given: the pieces beyond the square lie in a diagonal
/// neighbour's square and link there (KYN-LINK-1).
#[must_use]
pub fn trace_emit_cut(
    level: &Level,
    areas: &[Vec<Cell>],
    per_area: Vec<EdgeRows>,
    cfg: TraceCfg,
    (ox, oz, pitch): (f64, f64, f64),
    square: Option<i64>,
) -> Vec<Emitted> {
    let mut out = Vec::with_capacity(areas.len());
    for (cells, mut edges) in areas.iter().zip(per_area) {
        // 45-degree bridges between diagonal-only neighbours of this area, across an open corner only
        let cell_k: FastMap<(i64, i64), usize> = cells.iter().map(|c| ((c.0, c.1), c.2)).collect();
        let mut bridges = Vec::new();
        for &(i, j, k) in cells {
            for (di, dj) in [(1, 1), (1, -1)] {
                let Some(&kd) = cell_k.get(&(i + di, j + dj)) else {
                    continue;
                };
                if cell_k.contains_key(&(i + di, j)) || cell_k.contains_key(&(i, j + dj)) {
                    continue;
                }
                let (gi, gj) = (cfg.gi0, cfg.gj0);
                let open = [
                    (k, i, j, di, 0),
                    (k, i, j, 0, dj),
                    (kd, i + di, j + dj, -di, 0),
                    (kd, i + di, j + dj, 0, -dj),
                ]
                .into_iter()
                .all(|(kk, a, b, x, z)| level.neighbour(kk, gi + a, gj + b, x, z, None, cfg.rule).is_none());
                if open {
                    bridges.push((i, j, di, dj));
                }
            }
        }
        let mut em = Emitted::default();
        let (drop, dsegs) = bridge_union(level, &bridges, &cell_k, cfg.y_offset);
        for ((ax, az, ya), (bx, bz, yb)) in dsegs {
            em.outline.push((
                (ox + ax * pitch, ya, oz + az * pitch),
                (ox + bx * pitch, yb, oz + bz * pitch),
            ));
        }
        for (key, rows) in edges.iter_mut() {
            if key.kind == Kind::Outline {
                rows.retain(|r| !drop.contains(&(key.x_axis, key.line2, r.0)));
            }
        }
        for (key, runs) in edges.iter_mut() {
            runs.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then(a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            });
            let line = key.line();
            let cut_here = |t: i64| matches!(key.kind, Kind::Border(..)) && square.is_some_and(|s| t == 0 || t == s);
            let mut start = 0;
            for r in 1..=runs.len() {
                let (Some(&first), Some(&prev)) = (runs.get(start), runs.get(r - 1)) else {
                    continue;
                };
                if runs.get(r).is_some_and(|x| x.0 == prev.0 + 1 && !cut_here(x.0)) {
                    continue;
                }
                let (a, b) = (first.0 as f64 - 0.5, prev.0 as f64 + 0.5);
                let (ya, yb) = (first.1, prev.1);
                let (p0, p1) = if key.x_axis {
                    (
                        (ox + line * pitch, ya, oz + a * pitch),
                        (ox + line * pitch, yb, oz + b * pitch),
                    )
                } else {
                    (
                        (ox + a * pitch, ya, oz + line * pitch),
                        (ox + b * pitch, yb, oz + line * pitch),
                    )
                };
                match key.kind {
                    Kind::Portal(t) => em.portals.push((p0, p1, t)),
                    Kind::Border(di, dj) => em.borders.push(BorderRun {
                        p0,
                        p1,
                        dir: (di, dj),
                        first: first.0,
                        last: prev.0,
                        line,
                    }),
                    Kind::Outline => em.outline.push((p0, p1)),
                }
                start = r;
            }
        }
        out.push(em);
    }
    out
}
