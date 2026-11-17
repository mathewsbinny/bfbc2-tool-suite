//! NavKit's path-data generator (`bfbc2-auto-navkit/tools/navbuild.py`): the grid walks a build spends its time
//! in, ported from the Python reference, which stays the fallback (`rust/BENCHMARKS.md`, "NavKit's grid walks").
//!
//! A build asks, a hundred million times on a small level, which kept surface of a neighbouring sample column a
//! surface links to (`Level.neighbour`, with `Level.connected` and `navwalk.step_fits`); four callers make almost
//! all of those calls, and each is ported here whole, so the calls stay in Rust:
//!
//! | Python (`navbuild.py`) | here |
//! |---|---|
//! | `Level.surfaces`, `Level.connected`, `Level.neighbour` | [`Level::surfaces`], [`Level::connected`], [`Level::neighbour`] |
//! | `components` (an area's 8-connected pieces) | [`Level::components`] |
//! | `line_walkable` with `_crosses_d` | [`Level::line_walkable`] |
//! | `build_graph`'s search from one route-node group to another's nearest node | [`Level::search`] |
//! | `trace_edges`' neighbour of each side of an area's cells | [`Level::side_neighbours`] |
//! | `build_areas` with `bisect` (a sector's areas) | [`Level::build_areas`] |
//! | `build_graph`'s centre and portal cells (its `min` over an area's cells) | [`pick::centre_cell`], [`pick::nearest_cells`] |
//!
//! `navwalk.walkable`'s per-surface rules (`headroom`, `stair_slope`, `narrow_lips`, the stair landings,
//! `near_objects`, `on_objects`) are in [`rules`], on a sample run ([`rules::Grid`]); CPython's own float functions
//! they need (`math.hypot`, `x ** 2`) in [`pymath`].
//!
//! **Bit for bit.** The answers are integers (surfaces, cells), but they come from float comparisons, so the floats
//! are computed as the reference computes them: the heights in `f64`, the gradients in the dtype the build gives
//! them (`float32` for normals-derived gradients, `float64` where the walkability rules replaced some), with NumPy 2's
//! promotion rules (NEP 50: a Python float or int meeting a `float32` scalar becomes `float32`) - see [`Num`].
//! Python's `round` (half to even) is [`f64::round_ties_even`]. Any input the reference would fail on (an index
//! out of range) is an error here, and the caller then runs the reference.

#![deny(unsafe_code)]
// cell indices and small Python ints become floats exactly where the reference makes them floats
#![allow(clippy::cast_precision_loss)]

use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};

use thiserror::Error;

pub mod graph;
pub mod navsample;
pub mod pick;
pub mod pymath;
pub mod route;
pub mod rules;
pub mod sector;
pub mod trace;
pub mod window;

/// A fast hasher for the integer keys of the walks' maps (`FxHash`'s rotate, xor and multiply): no map whose iteration
/// order reaches an answer uses it - they are looked up only.
#[derive(Debug, Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

impl FxHasher {
    fn add(&mut self, w: u64) {
        self.hash = (self.hash.rotate_left(5) ^ w).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

impl Hasher for FxHasher {
    fn write(&mut self, bytes: &[u8]) {
        let (words, rest) = bytes.as_chunks::<8>();
        for w in words {
            self.add(u64::from_le_bytes(*w));
        }
        for &b in rest {
            self.add(u64::from(b));
        }
    }

    fn write_u8(&mut self, v: u8) {
        self.add(u64::from(v));
    }

    fn write_u32(&mut self, v: u32) {
        self.add(u64::from(v));
    }

    fn write_u64(&mut self, v: u64) {
        self.add(v);
    }

    fn write_usize(&mut self, v: usize) {
        self.add(v as u64);
    }

    fn write_i64(&mut self, v: i64) {
        self.add(v as u64);
    }

    fn finish(&self) -> u64 {
        self.hash
    }
}

/// A `HashMap` with [`FxHasher`].
pub type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<FxHasher>>;

/// A `HashSet` with [`FxHasher`].
pub type FastSet<T> = HashSet<T, BuildHasherDefault<FxHasher>>;

/// Why a call is refused (the Python reference would raise; the binding then runs it).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum NavError {
    /// The arrays do not describe one grid (lengths, start offsets).
    #[error("level arrays do not agree: {0}")]
    Shape(String),
    /// A surface index outside the level.
    #[error("surface {0} is not in the level")]
    Surface(i64),
    /// A cell the call needs is not in the cell map.
    #[error("no cell known for surface {0}")]
    Cell(i64),
}

/// A NumPy scalar of the dtype the reference computes in: `float32` or `float64`. The arithmetic follows NumPy 2:
/// two `float32` give `float32`, a `float64` on either side gives `float64`; a Python number (`f64` / `i64` here)
/// takes the scalar's dtype (rounded to it first).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Num {
    /// `numpy.float32`.
    F32(f32),
    /// `numpy.float64`.
    F64(f64),
}

impl Num {
    /// The value as `f64` (exact for both).
    #[must_use]
    pub fn get(self) -> f64 {
        match self {
            Num::F32(v) => f64::from(v),
            Num::F64(v) => v,
        }
    }

    fn add(self, o: Num) -> Num {
        match (self, o) {
            (Num::F32(a), Num::F32(b)) => Num::F32(a + b),
            (a, b) => Num::F64(a.get() + b.get()),
        }
    }

    /// `self / v` with a Python number v.
    fn div_py(self, v: f64) -> Num {
        match self {
            Num::F32(a) => Num::F32(a / v as f32),
            Num::F64(a) => Num::F64(a / v),
        }
    }

    /// `self * v` with a Python int.
    fn mul_int(self, v: i64) -> Num {
        match self {
            Num::F32(a) => Num::F32(a * v as f32),
            Num::F64(a) => Num::F64(a * v as f64),
        }
    }

    /// `v * self` with a Python float.
    fn py_mul(v: f64, s: Num) -> Num {
        match s {
            Num::F32(a) => Num::F32(v as f32 * a),
            Num::F64(a) => Num::F64(v * a),
        }
    }
}

/// A per-surface float array in the dtype the reference holds it in.
#[derive(Debug, Clone, PartialEq)]
pub enum Floats {
    /// `float32`.
    F32(Vec<f32>),
    /// `float64`.
    F64(Vec<f64>),
}

impl Floats {
    fn len(&self) -> usize {
        match self {
            Floats::F32(v) => v.len(),
            Floats::F64(v) => v.len(),
        }
    }

    fn at(&self, k: usize) -> Num {
        match self {
            Floats::F32(v) => Num::F32(v.get(k).copied().unwrap_or(0.0)),
            Floats::F64(v) => Num::F64(v.get(k).copied().unwrap_or(0.0)),
        }
    }
}

/// Which surfaces of neighbouring columns link (`navbuild.LINK_RULE`, `AREA_LINK`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// `'slope'`: the step rule on the local gradients (`navwalk.step_fits`).
    Slope,
    /// `'raw'`: the raw height step (`link_step`); a diagonal through either side column.
    Raw,
    /// `'terrain'`: raw between two terrain surfaces, the slope rule otherwise.
    Terrain,
}

impl Rule {
    /// The rule of a name as `navbuild` spells it; `None` for any other.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Rule> {
        match name {
            "slope" => Some(Rule::Slope),
            "raw" => Some(Rule::Raw),
            "terrain" => Some(Rule::Terrain),
            _ => None,
        }
    }
}

/// The level's kept surfaces on the sample grid (`navbuild.Level`): per column the range of its surfaces, per
/// surface its height, gradients, flags.
#[derive(Debug, Clone)]
pub struct Level {
    nx: usize,
    nz: usize,
    start: Vec<usize>,
    keep: Vec<bool>,
    y: Vec<f64>,
    gx: Floats,
    gz: Floats,
    stair: Vec<bool>,
    terrain: Vec<bool>,
    col_of: Vec<usize>,
    pitch: f64,
    step: f64,
    link_step: f64,
}

/// The arrays and settings a [`Level`] is made from (`navbuild.Level`'s fields, `cfg.pitch`, `cfg.link_step`).
#[derive(Debug, Clone)]
pub struct LevelData {
    /// Columns along x.
    pub nx: usize,
    /// Columns along z.
    pub nz: usize,
    /// Per column the index of its first surface; `nx * nz + 1` values.
    pub start: Vec<usize>,
    /// Kept (walkable) per surface.
    pub keep: Vec<bool>,
    /// Height per surface (`Level.y`, `float64`).
    pub y: Vec<f64>,
    /// Gradient along x per surface (`Level.gx`).
    pub gx: Floats,
    /// Gradient along z per surface (`Level.gz`).
    pub gz: Floats,
    /// Stair surface flags.
    pub stair: Vec<bool>,
    /// Terrain surface flags.
    pub terrain: Vec<bool>,
    /// The sample grid's pitch (m).
    pub pitch: f64,
    /// The step rule's tolerance (m).
    pub step: f64,
    /// The raw rule's largest step (m).
    pub link_step: f64,
}

/// One cell of an area: (local i, local j, surface).
pub type Cell = (i64, i64, usize);

/// One area of a sector: its tile (tx, tz) and its cells ([`Level::build_areas`]).
pub type Area = ((i64, i64), Vec<Cell>);

/// A surface given to [`Level::components`]: (surface, local i, local j).
pub type Member = (usize, i64, i64);

const DIRS8: [(i64, i64); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
const DIRS4: [(i64, i64); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];

impl Level {
    /// The level of `d`, its arrays checked against each other.
    ///
    /// # Errors
    /// [`NavError::Shape`] when the lengths or the column starts disagree.
    pub fn new(d: LevelData) -> Result<Level, NavError> {
        let n = d.y.len();
        let cols = d.nx * d.nz;
        if d.start.len() != cols + 1 {
            return Err(NavError::Shape(format!(
                "{} column starts for {cols} columns",
                d.start.len()
            )));
        }
        if d.start.first() != Some(&0) || d.start.last() != Some(&n) || d.start.windows(2).any(|w| w.first() > w.get(1))
        {
            return Err(NavError::Shape("column starts are not 0 .. surfaces, ascending".into()));
        }
        for (name, len) in [
            ("keep", d.keep.len()),
            ("gx", d.gx.len()),
            ("gz", d.gz.len()),
            ("stair", d.stair.len()),
            ("terrain", d.terrain.len()),
        ] {
            if len != n {
                return Err(NavError::Shape(format!("{name}: {len} values for {n} surfaces")));
            }
        }
        let mut col_of = Vec::with_capacity(n);
        for (c, (a, b)) in d.start.iter().zip(d.start.iter().skip(1)).enumerate() {
            col_of.extend(std::iter::repeat_n(c, b - a));
        }
        Ok(Level {
            nx: d.nx,
            nz: d.nz,
            start: d.start,
            keep: d.keep,
            y: d.y,
            gx: d.gx,
            gz: d.gz,
            stair: d.stair,
            terrain: d.terrain,
            col_of,
            pitch: d.pitch,
            step: d.step,
            link_step: d.link_step,
        })
    }

    /// The number of surfaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.y.len()
    }

    /// No surfaces at all.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.y.is_empty()
    }

    /// A surface index from the caller, checked.
    ///
    /// # Errors
    /// [`NavError::Surface`] when it is not a surface of the level.
    pub fn surface(&self, k: i64) -> Result<usize, NavError> {
        usize::try_from(k)
            .ok()
            .filter(|&k| k < self.len())
            .ok_or(NavError::Surface(k))
    }

    /// The height of surface k (`Level.y[k]`; 0 outside the level).
    #[must_use]
    pub fn y(&self, k: usize) -> f64 {
        self.y.get(k).copied().unwrap_or(0.0)
    }

    fn flag(v: &[bool], k: usize) -> bool {
        v.get(k).copied().unwrap_or(false)
    }

    fn column(&self, gi: i64, gj: i64) -> Option<std::ops::Range<usize>> {
        let (i, j) = (usize::try_from(gi).ok()?, usize::try_from(gj).ok()?);
        if i >= self.nx || j >= self.nz {
            return None;
        }
        let c = j * self.nx + i;
        Some(self.start.get(c).copied()?..self.start.get(c + 1).copied()?)
    }

    /// The kept surfaces of column (gi, gj), in order; none outside the grid (`Level.surfaces`).
    pub fn surfaces(&self, gi: i64, gj: i64) -> impl Iterator<Item = usize> + '_ {
        self.column(gi, gj)
            .into_iter()
            .flatten()
            .filter(|&k| Self::flag(&self.keep, k))
    }

    /// `navwalk.step_fits` for surfaces k and n (direction di, dj): the height change within `step` of the rise
    /// the mean gradient predicts, or of a stair surface's own.
    fn step_fits(&self, dy: f64, k: usize, n: usize, di: i64, dj: i64) -> bool {
        let (gk0, gk1, gn0, gn1) = (self.gx.at(k), self.gz.at(k), self.gx.at(n), self.gz.at(n));
        let mean = gk0
            .add(gn0)
            .div_py(2.0)
            .mul_int(di)
            .add(gk1.add(gn1).div_py(2.0).mul_int(dj));
        if (dy - Num::py_mul(self.pitch, mean).get()).abs() <= self.step {
            return true;
        }
        [
            (gk0, gk1, Self::flag(&self.stair, k)),
            (gn0, gn1, Self::flag(&self.stair, n)),
        ]
        .iter()
        .any(|&(g0, g1, st)| {
            st && (dy - Num::py_mul(self.pitch, g0.mul_int(di).add(g1.mul_int(dj))).get()).abs() <= self.step
        })
    }

    /// Surface n (direction di, dj from k) links to k by the rule (`Level.connected`).
    #[must_use]
    pub fn connected(&self, k: usize, n: usize, di: i64, dj: i64, rule: Rule) -> bool {
        let (yk, yn) = (self.y(k), self.y(n));
        if rule == Rule::Slope
            || (rule == Rule::Terrain && !(Self::flag(&self.terrain, k) && Self::flag(&self.terrain, n)))
        {
            return self.step_fits(yn - yk, k, n, di, dj);
        }
        let ls = self.link_step;
        if di == 0 || dj == 0 {
            return (yn - yk).abs() <= ls;
        }
        // diagonal: through either side column, over any surface there (walkable or not)
        let c = self.col_of.get(k).copied().unwrap_or(0);
        let (gi, gj) = ((c % self.nx.max(1)) as i64, (c / self.nx.max(1)) as i64);
        [(gi + di, gj), (gi, gj + dj)].iter().any(|&(a, b)| {
            self.column(a, b).into_iter().flatten().any(|m| {
                let ym = self.y(m);
                (ym - yk).abs() <= ls && (yn - ym).abs() <= ls
            })
        })
    }

    /// The kept surface in column (gi + di, gj + dj) connected to surface k, or `None` (`Level.neighbour`): the
    /// first one in `prefer` when given, else the nearest in height (the first of equals).
    #[must_use]
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    pub fn neighbour(
        &self,
        k: usize,
        gi: i64,
        gj: i64,
        di: i64,
        dj: i64,
        prefer: Option<&FastSet<usize>>,
        rule: Rule,
    ) -> Option<usize> {
        let mut best = None;
        let mut bd = 9e9;
        let yk = self.y(k);
        for n in self.surfaces(gi + di, gj + dj) {
            if self.connected(k, n, di, dj, rule) {
                if prefer.is_some_and(|p| p.contains(&n)) {
                    return Some(n);
                }
                let d = (self.y(n) - yk).abs();
                if d < bd {
                    best = Some(n);
                    bd = d;
                }
            }
        }
        best
    }

    /// The 8-connected components of `members` (surface, local i, local j; in the reference's dict order) under
    /// `rule`, each a list of cells in visiting order (`navbuild.components`). An area is single-layer in plan: a
    /// surface whose column the component already holds starts its own.
    #[must_use]
    pub fn components(&self, members: &[(usize, i64, i64)], gi0: i64, gj0: i64, rule: Rule) -> Vec<Vec<Cell>> {
        // a dict: a surface given twice keeps its first place and its last cell
        let mut at: FastMap<usize, (i64, i64)> = FastMap::default();
        at.reserve(members.len());
        let mut order = Vec::with_capacity(members.len());
        for &(k, i, j) in members {
            if at.insert(k, (i, j)).is_none() {
                order.push(k);
            }
        }
        let mut seen: FastSet<usize> = FastSet::default();
        seen.reserve(order.len());
        let mut out = Vec::new();
        for k0 in order {
            if !seen.insert(k0) {
                continue;
            }
            let mut comp = Vec::new();
            let mut q = VecDeque::from([k0]);
            let mut cols: FastSet<(i64, i64)> = FastSet::default();
            if let Some(&c) = at.get(&k0) {
                cols.insert(c);
            }
            while let Some(k) = q.pop_front() {
                let Some(&(i, j)) = at.get(&k) else { continue };
                comp.push((i, j, k));
                for (di, dj) in DIRS8 {
                    let Some(n) = self.neighbour(k, gi0 + i, gj0 + j, di, dj, None, rule) else {
                        continue;
                    };
                    if let Some(&cn) = at.get(&n)
                        && !seen.contains(&n)
                        && cn == (i + di, j + dj)
                        && !cols.contains(&cn)
                    {
                        seen.insert(n);
                        cols.insert(cn);
                        q.push_back(n);
                    }
                }
            }
            out.push(comp);
        }
        out
    }

    /// `navbuild.build_areas`: the areas of one sector, each `(tile, cells)`, in the reference's order. The kept
    /// surfaces of the window's columns (`window` = start .. stop, step 1; z rows outer) are put in tiles of `tile`
    /// samples (`tiles` per axis) counted from `start`; each tile's surfaces are split into 8-connected components
    /// under `rule` (tiles in sorted order), and with `max_area` each component is bisected until it spans at most
    /// that many samples per axis (`bisect`).
    ///
    /// # Errors
    /// [`NavError::Shape`] when `tile` or `max_area` is under 1 (the reference fails or does not end).
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    pub fn build_areas(
        &self,
        gi0: i64,
        gj0: i64,
        window: (i64, i64),
        start: (i64, i64),
        tile: i64,
        tiles: i64,
        max_area: Option<i64>,
        rule: Rule,
    ) -> Result<Vec<Area>, NavError> {
        if tile < 1 || max_area.is_some_and(|m| m < 1) {
            return Err(NavError::Shape(format!("tile {tile}, max area {max_area:?}")));
        }
        let mut by_tile: std::collections::BTreeMap<(i64, i64), Vec<Member>> = std::collections::BTreeMap::new();
        for j in window.0..window.1 {
            for i in window.0..window.1 {
                for k in self.surfaces(gi0 + i, gj0 + j) {
                    let tx = ((i - start.0).max(0) / tile).min(tiles - 1);
                    let tz = ((j - start.1).max(0) / tile).min(tiles - 1);
                    by_tile.entry((tx, tz)).or_default().push((k, i, j));
                }
            }
        }
        let mut out = Vec::new();
        for (tkey, members) in by_tile {
            for comp in self.components(&members, gi0, gj0, rule) {
                let mut pieces = Vec::new();
                match max_area {
                    Some(m) => self.bisect(comp, gi0, gj0, m, rule, &mut pieces),
                    None => pieces.push(comp),
                }
                out.extend(pieces.into_iter().map(|p| (tkey, p)));
            }
        }
        Ok(out)
    }

    /// `navbuild.bisect`: a component spanning more than `max_area` samples on an axis is cut at the middle of its
    /// longer extent, each half split into components again, recursively.
    fn bisect(&self, comp: Vec<Cell>, gi0: i64, gj0: i64, max_area: i64, rule: Rule, out: &mut Vec<Vec<Cell>>) {
        let ext = |f: fn(&Cell) -> i64| {
            let it = comp.iter().map(f);
            (it.clone().min().unwrap_or(0), it.max().unwrap_or(0))
        };
        let ((i0, i1), (j0, j1)) = (ext(|c| c.0), ext(|c| c.1));
        if i1 - i0 < max_area && j1 - j0 < max_area {
            out.push(comp);
            return;
        }
        let (on_i, mid) = if i1 - i0 >= j1 - j0 {
            (true, (i0 + i1 + 1).div_euclid(2))
        } else {
            (false, (j0 + j1 + 1).div_euclid(2))
        };
        let below = |c: &Cell| if on_i { c.0 < mid } else { c.1 < mid };
        let halves: [Vec<(usize, i64, i64)>; 2] = [
            comp.iter().filter(|c| below(c)).map(|&(i, j, k)| (k, i, j)).collect(),
            comp.iter().filter(|c| !below(c)).map(|&(i, j, k)| (k, i, j)).collect(),
        ];
        for h in halves {
            for sub in self.components(&h, gi0, gj0, rule) {
                self.bisect(sub, gi0, gj0, max_area, rule, out);
            }
        }
    }

    /// For each cell of an area and each side (+x, -x, +z, -z, in that order), the neighbour across it under
    /// `rule`, preferring the area's own surfaces (`trace_edges`' calls).
    #[must_use]
    pub fn side_neighbours(&self, cells: &[Cell], gi0: i64, gj0: i64, rule: Rule) -> Vec<Option<usize>> {
        let own: FastSet<usize> = cells.iter().map(|c| c.2).collect();
        let mut out = Vec::with_capacity(cells.len() * 4);
        for &(i, j, k) in cells {
            for (di, dj) in DIRS4 {
                out.push(self.neighbour(k, gi0 + i, gj0 + j, di, dj, Some(&own), rule));
            }
        }
        out
    }

    /// Whether the step from k (column gi, gj) to nk crosses a D side (`navbuild._crosses_d`): a straight step
    /// across one; a diagonal step when the straight line passes through the side column it leaves first
    /// (`xfirst`: `Some(true)` = (di, 0), `Some(false)` = (0, dj), `None` = through the corner, either way).
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    fn crosses_d<S: std::hash::BuildHasher>(
        &self,
        blocked: &HashSet<(usize, usize), S>,
        k: usize,
        nk: usize,
        gi: i64,
        gj: i64,
        di: i64,
        dj: i64,
        xfirst: Option<bool>,
        rule: Rule,
    ) -> bool {
        if di == 0 || dj == 0 {
            return blocked.contains(&(k, nk));
        }
        let ways: &[(i64, i64)] = match xfirst {
            Some(true) => &[(di, 0)],
            Some(false) => &[(0, dj)],
            None => &[(di, 0), (0, dj)],
        };
        ways.iter().any(|&(a, b)| {
            self.neighbour(k, gi, gj, a, b, None, rule)
                .is_some_and(|s| blocked.contains(&(k, s)) || blocked.contains(&(s, nk)))
        })
    }

    /// A straight line from local sample a to b (i, j) over connected kept surfaces, starting on k0 and ending
    /// on k1 or within 0.6 m of its height; it does not step across a D side in `blocked` (`line_walkable`).
    #[must_use]
    #[expect(clippy::too_many_arguments, reason = "the reference's arguments")]
    pub fn line_walkable<S: std::hash::BuildHasher>(
        &self,
        k0: usize,
        a: (i64, i64),
        b: (i64, i64),
        gi0: i64,
        gj0: i64,
        k1: usize,
        blocked: &HashSet<(usize, usize), S>,
        rule: Rule,
    ) -> bool {
        let ((i0, j0), (i1, j1)) = (a, b);
        let n = (i1 - i0).abs().max((j1 - j0).abs());
        let (mut k, mut ci, mut cj) = (k0, i0, j0);
        for t in 1..=n {
            let ti = i0 + (((i1 - i0) * t) as f64 / n as f64).round_ties_even() as i64;
            let tj = j0 + (((j1 - j0) * t) as f64 / n as f64).round_ties_even() as i64;
            let (di, dj) = (ti - ci, tj - cj);
            if di == 0 && dj == 0 {
                continue;
            }
            let Some(nk) = self.neighbour(k, gi0 + ci, gj0 + cj, di, dj, None, rule) else {
                return false;
            };
            if !blocked.is_empty() {
                let mut xfirst = None;
                if di != 0 && dj != 0 {
                    // where the straight line crosses the column line between ci and ci + di: still in row cj -> x first
                    let zc =
                        j0 as f64 + (j1 - j0) as f64 * (ci as f64 + di as f64 * 0.5 - i0 as f64) / (i1 - i0) as f64;
                    let off = (zc - cj as f64).abs();
                    xfirst = if (off - 0.5).abs() < 1e-9 {
                        None
                    } else {
                        Some(off < 0.5)
                    };
                }
                if self.crosses_d(blocked, k, nk, gi0 + ci, gj0 + cj, di, dj, xfirst, rule) {
                    return false;
                }
            }
            (k, ci, cj) = (nk, ti, tj);
        }
        k == k1 || (self.y(k) - self.y(k1)).abs() <= 0.6
    }

    /// `build_graph`'s search from one group of route nodes to another group's node: breadth first from the
    /// group's surfaces (`starts`, in order) over neighbouring kept surfaces (8 directions, columns of the sector
    /// window `window` only), until a surface in `targets` is reached. `cells` is the build's surface -> local cell
    /// map: a surface's cell is read from it, and every surface reached is entered with its cell when it has none
    /// (the reference's `setdefault`, whether or not a target is found). Returns the path from a start surface to
    /// the target, or `None`.
    ///
    /// # Errors
    /// [`NavError::Cell`] when a surface taken from the queue has no cell.
    #[expect(clippy::too_many_arguments, reason = "the reference's state")]
    pub fn search(
        &self,
        starts: &[usize],
        targets: &dyn Fn(usize) -> bool,
        cells: &mut dyn CellMap,
        window: (i64, i64),
        gi0: i64,
        gj0: i64,
        rule: Rule,
    ) -> Result<Option<Vec<usize>>, NavError> {
        let mut prev: FastMap<usize, Option<usize>> = starts.iter().map(|&k| (k, None)).collect();
        let mut queue: VecDeque<usize> = starts.iter().copied().collect();
        let inside = |v: i64| window.0 <= v && v < window.1;
        while let Some(k) = queue.pop_front() {
            let (i, j) = cells.get(k).ok_or(NavError::Cell(k as i64))?;
            for di in -1..=1 {
                for dj in -1..=1 {
                    if (di == 0 && dj == 0) || !inside(i + di) || !inside(j + dj) {
                        continue;
                    }
                    let Some(n) = self.neighbour(k, gi0 + i, gj0 + j, di, dj, None, rule) else {
                        continue;
                    };
                    if prev.contains_key(&n) {
                        continue;
                    }
                    prev.insert(n, Some(k));
                    cells.set_default(n, (i + di, j + dj));
                    if targets(n) {
                        let mut path = vec![n];
                        while let Some(&Some(p)) = path.last().and_then(|l| prev.get(l)) {
                            path.push(p);
                        }
                        path.reverse();
                        return Ok(Some(path));
                    }
                    queue.push_back(n);
                }
            }
        }
        Ok(None)
    }

    /// The surfaces of the columns around k's (8 directions, z rows outer as `navwalk` walks them), each with its
    /// direction: `(di, dj, surfaces)`.
    fn around(&self, k: usize) -> impl Iterator<Item = (i64, i64, std::ops::Range<usize>)> + '_ {
        let c = self.col_of.get(k).copied().unwrap_or(0);
        let (i, j) = ((c % self.nx.max(1)) as i64, (c / self.nx.max(1)) as i64);
        (-1..=1).flat_map(move |dj| {
            (-1..=1).filter_map(move |di| {
                if di == 0 && dj == 0 {
                    return None;
                }
                self.column(i + di, j + dj).map(|r| (di, dj, r))
            })
        })
    }

    /// `navwalk.walkable`'s flood: from the surfaces in `queue` (kept already), every surface the step rule joins
    /// that is walkable here (this level's `keep`, the walkability rules' `ok`) and not yet in `kept` is kept, in
    /// breadth-first order.
    pub fn flood(&self, kept: &mut [bool], queue: impl IntoIterator<Item = usize>) {
        let mut q: VecDeque<usize> = queue.into_iter().collect();
        while let Some(k) = q.pop_front() {
            let yk = self.y(k);
            let next: Vec<(i64, i64, std::ops::Range<usize>)> = self.around(k).collect();
            for (di, dj, col) in next {
                for n in col {
                    if kept.get(n).copied().unwrap_or(true) || !Self::flag(&self.keep, n) {
                        continue;
                    }
                    if self.step_fits(self.y(n) - yk, k, n, di, dj)
                        && let Some(v) = kept.get_mut(n)
                    {
                        *v = true;
                        q.push_back(n);
                    }
                }
            }
        }
    }

    /// `navwalk.islands_of`: the island of each surface this level keeps (-1 elsewhere), surfaces joined by the
    /// step rule as the flood joins them, numbered in surface order; and the number of islands.
    #[must_use]
    pub fn islands(&self) -> (Vec<i64>, usize) {
        let mut comp = vec![-1i64; self.len()];
        let mut cid = 0usize;
        for k0 in 0..self.len() {
            if !Self::flag(&self.keep, k0) || comp.get(k0).is_some_and(|&c| c >= 0) {
                continue;
            }
            if let Some(c) = comp.get_mut(k0) {
                *c = cid as i64;
            }
            let mut q = VecDeque::from([k0]);
            while let Some(k) = q.pop_front() {
                let yk = self.y(k);
                let next: Vec<(i64, i64, std::ops::Range<usize>)> = self.around(k).collect();
                for (di, dj, col) in next {
                    for n in col {
                        if comp.get(n).is_none_or(|&c| c >= 0) || !Self::flag(&self.keep, n) {
                            continue;
                        }
                        if self.step_fits(self.y(n) - yk, k, n, di, dj)
                            && let Some(c) = comp.get_mut(n)
                        {
                            *c = cid as i64;
                            q.push_back(n);
                        }
                    }
                }
            }
            cid += 1;
        }
        (comp, cid)
    }
}

/// `navwalk.matched_flags`: per surface of a sample run (`start` per column, heights `y`), the capsule flags of the
/// first surface of a second run over the same grid (`other_start`, `other_y`, `other_flags`) in the same column within
/// 0.05 m of its height; 1 where there is none. `cols` columns.
///
/// # Errors
/// [`NavError::Shape`] when a column start or a surface lies outside the arrays given.
pub fn matched_flags(
    cols: usize,
    start: &[usize],
    y: &[f64],
    other_start: &[usize],
    other_y: &[f64],
    other_flags: &[u8],
) -> Result<Vec<u8>, NavError> {
    let shape = || NavError::Shape("a column outside the sample arrays".into());
    let mut out = vec![1u8; y.len()];
    for c in 0..cols {
        let (a, b) = (*start.get(c).ok_or_else(shape)?, *start.get(c + 1).ok_or_else(shape)?);
        let (oa, ob) = (
            *other_start.get(c).ok_or_else(shape)?,
            *other_start.get(c + 1).ok_or_else(shape)?,
        );
        let (ys, oys, ofl) = (
            y.get(a..b).ok_or_else(shape)?,
            other_y.get(oa..ob).ok_or_else(shape)?,
            other_flags.get(oa..ob).ok_or_else(shape)?,
        );
        let dst = out.get_mut(a..b).ok_or_else(shape)?;
        for (o, &yk) in dst.iter_mut().zip(ys) {
            if let Some((_, &f)) = oys.iter().zip(ofl).find(|&(&yn, _)| (yn - yk).abs() < 0.05) {
                *o = f;
            }
        }
    }
    Ok(out)
}

/// The surface -> local cell map [`Level::search`] reads and extends (the build's `cell_ij`).
pub trait CellMap {
    /// The cell of surface k, if known.
    fn get(&self, k: usize) -> Option<(i64, i64)>;
    /// Enter k's cell unless it has one.
    fn set_default(&mut self, k: usize, cell: (i64, i64));
}

impl<S: std::hash::BuildHasher> CellMap for HashMap<usize, (i64, i64), S> {
    fn get(&self, k: usize) -> Option<(i64, i64)> {
        HashMap::get(self, &k).copied()
    }

    fn set_default(&mut self, k: usize, cell: (i64, i64)) {
        self.entry(k).or_insert(cell);
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

#[cfg(test)]
#[path = "e3c_tests.rs"]
mod e3c_tests;
