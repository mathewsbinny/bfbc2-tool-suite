//! NavKit's offline sampler in Rust (round NKS-deep C): the in-game sampler's output (OPFOR's `[dev] navsample`,
//! `src\dll\navsample.cpp`) computed from a level's own data without starting the game - the port of
//! `bfbc2-auto-navkit/tools/navsample_offline.py`'s sampling (`Grid`, `raster_hits`, `cast_columns`, `Intervals`,
//! `capsule_blocked`, `terrain_blocks`, `sample_grid`, `write`).
//!
//! | Python | here |
//! |---|---|
//! | `Grid`, `Grid.from_box`, `Grid.like` | [`Grid`], [`Grid::from_box`], [`Grid::like`] |
//! | `level_geometry`, `destroyed_parts`, `part_names`, `load_level` | [`level::level_geometry`], [`level::destroyed_parts`], [`level::part_names`], [`level::load_level`] |
//! | `sample_grid` (with `raster_hits`, `cast_columns`, `capsule_blocked`, `terrain_blocks`) | [`sample`] |
//! | `write` / `file_bytes` | [`Samples::file_bytes`] |
//! | `compare` | [`compare::compare`] |
//! | `run_level`, `sample_level`, `stage_level`, `main`'s `like` / `validate` | [`level`] |
//!
//! **What a sample is** (`navsample.cpp`, and how it is reproduced): a sphere (radius [`RAY_RADIUS`]) cast down from
//! y top; after a hit at y the next cast starts at y - [`SKIP`]; a cast that hits within [`AT_ONCE`] of its start is
//! not recorded and the start steps down [`SOLID_STEP`], at most [`MAX_SOLID`] in a row. The hits come from every
//! collision triangle rasterised onto the columns (a triangle's ranges of columns, then the line through each column
//! centre against it, or the sphere touching it within its radius); the height field is solid (nothing under it is
//! recorded), convex pieces are solid, mesh triangles are hit from both sides. Each recorded surface gets the
//! entity's test ([`Body`]): the soldier's capsule (the reference's), or a vehicle's cylinder (new, KYG-PAR-2's
//! vehicle settings).
//!
//! **Bit for bit with the reference.** Each (triangle, column) pair's arithmetic is NumPy's ([`geom`]); the cast's
//! and the capsule's float keys (`column * K + height`) are computed as the reference computes them, so the hit each
//! search finds is the same; hits tied in height keep the reference's order (mesh, then convex, then terrain, each by
//! triangle). The work is split into bands of rows, each band on its own core: every answer the reference computes
//! for a column depends only on that column's pairs (its keys never reach another column's), so the order of the
//! work does not change it.
//!
//! **The API takes the level in memory** ([`Geometry`], [`offline::Field`]): the level driver reads them from a
//! prepared level ([`level`]), the editor from a stage or a sculpted terrain.

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    reason = "column indices as the reference's float keys; floored floats as the reference's int casts"
)]
#![allow(
    clippy::indexing_slicing,
    reason = "column, surface and triangle indices within the arrays they were made for (the grid's, a band's, \
              the geometry's)"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::neg_cmp_op_on_partial_ord,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::doc_markdown,
    clippy::map_unwrap_or,
    clippy::missing_panics_doc,
    clippy::verbose_bit_mask,
    clippy::items_after_statements,
    reason = "the reference's names (g, p, t, x, z, y, n) and steps; `!(a <= b)` is the reference's test, true for a \
              NaN; exact float ties are what NumPy compares"
)]

pub mod compare;
pub mod geom;
pub mod level;

use std::io::Write as _;

use rayon::prelude::*;

use crate::offline::Field;
use geom::{Tri, det, line_tri_interval, norm, raster_pair, tri_cross, unit_normal};

/// The cast's sphere radius (`RAY_RADIUS`).
pub const RAY_RADIUS: f64 = 0.05;
/// The gap below a hit where the next cast starts (`SKIP`).
pub const SKIP: f64 = 0.1;
/// The step down after a cast that started inside geometry (`SOLID_STEP`).
pub const SOLID_STEP: f64 = 0.5;
/// At most this far stepped down in a row (`MAX_SOLID`).
pub const MAX_SOLID: f64 = 10.0;
/// A hit this close below the start: the cast started inside geometry (`AT_ONCE`).
pub const AT_ONCE: f64 = RAY_RADIUS + 0.02;
/// Surfaces a column records at most (`MAX_LAYERS`).
pub const MAX_LAYERS: u32 = 32;
/// The soldier's capsule radius (`RADIUS`).
pub const RADIUS: f64 = 0.4;
/// The soldier's height (`HEIGHT`).
pub const HEIGHT: f64 = 1.9;
/// The normal run's capsule bottom (StepMax, `CAP_BOTTOMS['']`).
pub const BOTTOM: f64 = 0.28;
/// The high run's (`CAP_BOTTOMS['_hi']`).
pub const BOTTOM_HIGH: f64 = 0.6;
/// The cast's reach past a convex piece's top face (`CONVEX_GROW`).
pub const CONVEX_GROW: f64 = 2.0 * RAY_RADIUS;
/// The cast's reach past a mesh triangle's edge (`EDGE_GROW`).
pub const EDGE_GROW: f64 = RAY_RADIUS;

/// The terrain's kind (`KIND_TERRAIN`).
pub const KIND_TERRAIN: u8 = 0;
/// A mesh triangle's (`KIND_MESH`).
pub const KIND_MESH: u8 = crate::hulls::KIND_MESH;
/// A convex piece's (`KIND_CONVEX`).
pub const KIND_CONVEX: u8 = crate::hulls::KIND_CONVEX;
/// The in-game file's hit vtables by kind (`VTABLE_OF_KIND`): terrain, MOPP mesh, convex.
pub const VTABLE_OF_KIND: [u32; 3] = [0x0145_4D14, 0x0148_3B38, 0x0148_64B0];
/// The second word of every in-game record (`RAY_SHAPE_VTABLE`).
pub const RAY_SHAPE_VTABLE: u32 = 0x0148_52B8;

/// The vehicle's settings (KYG-PAR-2, DICE's `AI/Pathfinding/Vehicles`): EntityRadius, EntityHeight, StepMax, Pitch.
pub const VEHICLE_RADIUS: f64 = 2.5;
/// EntityHeight of the vehicle type.
pub const VEHICLE_HEIGHT: f64 = 4.0;
/// StepMax of the vehicle type.
pub const VEHICLE_STEP: f64 = 0.4;
/// Pitch of the vehicle type.
pub const VEHICLE_PITCH: f64 = 1.0;

/// The capsule's lowest and highest sphere centres above the surface (`capsule_axis`): the fitted `CAPSULE_AXIS` of
/// the two soldier runs, else `navsample.cpp`'s nominal capsule.
#[must_use]
pub fn capsule_axis(bottom: f64, radius: f64, height: f64) -> (f64, f64) {
    // round(float(bottom), 2): the two keys are exact at two decimals
    let r2 = crate::pymath::round_to(bottom, 2);
    if r2 == 0.28 {
        (0.95, 1.5)
    } else if r2 == 0.6 {
        (1.15, 1.5)
    } else {
        (bottom + radius, height - radius)
    }
}

/// The entity each recorded surface is tested for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Body {
    /// The soldier's capsule (`navsample.cpp`'s query): radius, height, the capsule's bottom (StepMax) - the
    /// reference's test.
    Capsule {
        /// Its radius.
        radius: f64,
        /// Its top above the surface.
        height: f64,
        /// Its bottom above the surface (StepMax).
        bottom: f64,
    },
    /// A vehicle (KYG-PAR-2: CollisionModel cylinder): a vertical cylinder of the radius from `step` to `height`
    /// above the surface meets no collision triangle, and the terrain under it rises no more than `step` above the
    /// surface's own plane (slopes are the build's GroundSlope rule, not the body's). Not validated: no in-game
    /// vehicle-shaped run exists (NKG3).
    Cylinder {
        /// EntityRadius.
        radius: f64,
        /// EntityHeight.
        height: f64,
        /// StepMax.
        step: f64,
    },
}

impl Body {
    /// The soldier's capsule with its bottom.
    #[must_use]
    pub fn soldier(bottom: f64) -> Self {
        Body::Capsule {
            radius: RADIUS,
            height: HEIGHT,
            bottom,
        }
    }

    /// The vehicle's cylinder (KYG-PAR-2).
    #[must_use]
    pub fn vehicle() -> Self {
        Body::Cylinder {
            radius: VEHICLE_RADIUS,
            height: VEHICLE_HEIGHT,
            step: VEHICLE_STEP,
        }
    }

    /// Its radius (how far from a column a triangle can block it).
    #[must_use]
    pub fn radius(&self) -> f64 {
        match *self {
            Body::Capsule { radius, .. } | Body::Cylinder { radius, .. } => radius,
        }
    }

    /// The header's (radius, bottom, top).
    #[must_use]
    pub fn header(&self) -> (f64, f64, f64) {
        match *self {
            Body::Capsule { radius, height, bottom } => (radius, bottom, height),
            Body::Cylinder { radius, height, step } => (radius, step, height),
        }
    }
}

/// A sample grid (`Grid`): columns at `min + k * pitch`, j-major; `mask` the columns to sample (`None`: all).
#[derive(Debug, Clone, PartialEq)]
pub struct Grid {
    /// The pitch (a `float32`'s value).
    pub pitch: f64,
    /// The first column's x.
    pub min_x: f64,
    /// The first row's z.
    pub min_z: f64,
    /// Columns along x.
    pub nx: usize,
    /// Rows along z.
    pub nz: usize,
    /// Where the casts stop.
    pub y_bottom: f64,
    /// Where they start.
    pub y_top: f64,
    /// `(nz, nx)` j-major: the columns sampled.
    pub mask: Option<Vec<bool>>,
}

/// Python's slice bounds `[a:b]` of a length-`n` axis, as a range.
fn py_slice(a: i64, b: i64, n: i64) -> std::ops::Range<usize> {
    let fix = |v: i64| if v < 0 { (v + n).max(0) } else { v.min(n) };
    let (a, b) = (fix(a), fix(b));
    (a as usize)..(b.max(a) as usize)
}

impl Grid {
    /// `Grid.from_box(pitch, box, y, rects)`: aligned to whole multiples of the pitch as `navsample.cpp`'s `Begin()`
    /// (in `float32`); `rects` (detailed sectors' squares) mask the columns by `navsample.cpp`'s 1 m grid.
    #[must_use]
    pub fn from_box(pitch: f64, bx: [f64; 4], y: [f64; 2], rects: Option<&[[f64; 4]]>) -> Self {
        let p = pitch as f32;
        let min_x = ((bx[0] as f32) / p).floor() * p;
        let min_z = ((bx[1] as f32) / p).floor() * p;
        let nx = (((bx[2] as f32) - min_x) / p).ceil() as i64 + 1;
        let nz = (((bx[3] as f32) - min_z) / p).ceil() as i64 + 1;
        let mut g = Grid {
            pitch: f64::from(p),
            min_x: f64::from(min_x),
            min_z: f64::from(min_z),
            nx: nx.max(0) as usize,
            nz: nz.max(0) as usize,
            y_bottom: y[0],
            y_top: y[1],
            mask: None,
        };
        if let Some(rects) = rects.filter(|r| !r.is_empty()) {
            let w = (bx[2] - g.min_x).ceil() as i64 + 2;
            let h = (bx[3] - g.min_z).ceil() as i64 + 2;
            let (wu, hu) = (w.max(0) as usize, h.max(0) as usize);
            let mut m1 = vec![false; wu * hu];
            for r in rects {
                let i0 = 0.max((r[0] - g.min_x).floor() as i64);
                let j0 = 0.max((r[1] - g.min_z).floor() as i64);
                let i1 = (w - 1).min((r[2] - g.min_x).ceil() as i64);
                let j1 = (h - 1).min((r[3] - g.min_z).ceil() as i64);
                for j in py_slice(j0, j1 + 1, h) {
                    for i in py_slice(i0, i1 + 1, w) {
                        m1[j * wu + i] = true;
                    }
                }
            }
            let ii: Vec<i64> = (0..g.nx)
                .map(|i| ((g.min_x + i as f64 * g.pitch) - g.min_x).floor() as i64)
                .collect();
            let jj: Vec<i64> = (0..g.nz)
                .map(|j| ((g.min_z + j as f64 * g.pitch) - g.min_z).floor() as i64)
                .collect();
            let mut mask = vec![false; g.nx * g.nz];
            for (j, &mj) in jj.iter().enumerate() {
                if mj < 0 || mj >= h {
                    continue;
                }
                for (i, &mi) in ii.iter().enumerate() {
                    if mi >= 0 && mi < w {
                        mask[j * g.nx + i] = m1[mj as usize * wu + mi as usize];
                    }
                }
            }
            g.mask = Some(mask);
        }
        g
    }

    /// `Grid.like(s)`: the grid of a sample file's header (its float32 values); `counts` given and more than half
    /// the columns empty (a detailed-zone run): only the columns it sampled (`navsample_offline`'s `like`).
    #[must_use]
    pub fn like(h: &crate::navsample::Header, counts: Option<&[u8]>) -> Self {
        let mut g = Grid {
            pitch: h.pitch,
            min_x: h.min_x,
            min_z: h.min_z,
            nx: h.nx.max(0) as usize,
            nz: h.nz.max(0) as usize,
            y_bottom: h.y_bottom,
            y_top: h.y_top,
            mask: None,
        };
        if let Some(c) = counts.filter(|c| c.len() == g.nx * g.nz && !c.is_empty()) {
            let empty = c.len() - c.iter().filter(|&&n| n > 0).count();
            if empty as f64 / c.len() as f64 > 0.5 {
                g.mask = Some(c.iter().map(|&n| n > 0).collect());
            }
        }
        g
    }

    /// Columns in all.
    #[must_use]
    pub fn ncols(&self) -> usize {
        self.nx * self.nz
    }

    /// Column `i`'s x (`grid.min_x + i * grid.pitch`).
    #[inline]
    #[must_use]
    pub fn x(&self, i: usize) -> f64 {
        self.min_x + i as f64 * self.pitch
    }

    /// Row `j`'s z.
    #[inline]
    #[must_use]
    pub fn z(&self, j: usize) -> f64 {
        self.min_z + j as f64 * self.pitch
    }

    fn sampled(&self, col: usize) -> bool {
        self.mask.as_ref().is_none_or(|m| m.get(col).copied().unwrap_or(false))
    }
}

/// `_ranges(lo, hi, origin, pitch, n)` for one triangle: the columns whose centre lies in `[lo, hi]`.
fn ranges(lo: f64, hi: f64, origin: f64, pitch: f64, n: usize) -> (i64, i64) {
    let a = ((lo - origin) / pitch - 1e-9).ceil() as i64;
    let b = ((hi - origin) / pitch + 1e-9).floor() as i64;
    (a.max(0), b.min(n as i64 - 1))
}

/// A level's collision as the sampler sees it (`Geometry` after `bodies` and `intact`): world-space triangles, each
/// one's kind, convex piece id (`-1` for mesh triangles) and the kind of its body's root shape (the hit vtable).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Geometry {
    /// The triangles.
    pub tris: Vec<Tri>,
    /// [`KIND_MESH`] or [`KIND_CONVEX`].
    pub kind: Vec<u8>,
    /// The placed convex piece's id, `-1` for mesh triangles.
    pub solid: Vec<i32>,
    /// The kind of the body's root shape (`vt`).
    pub vt: Vec<u8>,
}

impl Geometry {
    /// Triangles in all.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tris.len()
    }

    /// No triangles.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tris.is_empty()
    }

    /// The arrays agree in length.
    #[must_use]
    pub fn consistent(&self) -> bool {
        let n = self.tris.len();
        self.kind.len() == n && self.solid.len() == n && self.vt.len() == n
    }
}

/// The surfaces one cast found (shared by the runs of one [`sample`] call): per column the count, per surface its
/// height (`float32`), the quantised normal and the hit kind.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Surfaces {
    /// Surfaces per column (j-major).
    pub counts: Vec<u8>,
    /// Per surface: height.
    pub y: Vec<f32>,
    /// Per surface: the normal as the file stores it (x / y / z times 127, half to even, clipped).
    pub normal: Vec<[i8; 3]>,
    /// Per surface: the hit's kind ([`KIND_TERRAIN`], [`KIND_MESH`], [`KIND_CONVEX`]).
    pub kind: Vec<u8>,
}

/// One run's output (`navsample.Samples` as `sample_grid` makes it): the header, the cast's surfaces and this run's
/// flags.
#[derive(Debug, Clone, PartialEq)]
pub struct Samples {
    /// The header (`navsample.Header`'s values).
    pub header: crate::navsample::Header,
    /// The surfaces (shared with the other runs of the same cast).
    pub surfaces: std::sync::Arc<Surfaces>,
    /// Per surface: the flags (bit 0: blocked).
    pub flags: Vec<u8>,
}

impl std::ops::Deref for Samples {
    type Target = Surfaces;

    fn deref(&self) -> &Surfaces {
        &self.surfaces
    }
}

impl Samples {
    /// Surfaces in all.
    #[must_use]
    pub fn len(&self) -> usize {
        self.y.len()
    }

    /// No surfaces.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.y.is_empty()
    }

    /// Writes the version-3 file (`navsample_offline.write`: `navsample::write_v3`'s layout) to `w`, record by record
    /// (the reference's intermediate arrays are not made).
    ///
    /// # Errors
    /// A header value past `float32` or a grid past 32 bits (where `write_v3` refuses), or `w`'s error.
    pub fn write_to(&self, w: &mut dyn std::io::Write) -> std::io::Result<()> {
        let h = &self.header;
        let bad = |what: &str| std::io::Error::new(std::io::ErrorKind::InvalidData, format!("navsample write: {what}"));
        let f = |v: f64| -> std::io::Result<[u8; 4]> {
            let x = v as f32;
            if v.is_nan() || (x.is_infinite() && v.is_finite()) {
                return Err(bad("a header value past float32"));
            }
            Ok(x.to_le_bytes())
        };
        let i = |v: i64| i32::try_from(v).map_err(|_| bad("a grid size past 32 bits"));
        let mut w = std::io::BufWriter::with_capacity(1 << 20, w);
        w.write_all(b"ONS1")?;
        w.write_all(&3u32.to_le_bytes())?;
        for v in [h.pitch, h.min_x, h.min_z] {
            w.write_all(&f(v)?)?;
        }
        w.write_all(&i(h.nx)?.to_le_bytes())?;
        w.write_all(&i(h.nz)?.to_le_bytes())?;
        for v in [h.y_top, h.y_bottom, h.radius, h.cap_bottom, h.cap_top, h.ray_radius] {
            w.write_all(&f(v)?)?;
        }
        w.write_all(&0u32.to_le_bytes())?;
        let mut rec = [0u8; 20];
        rec[12..16].copy_from_slice(&RAY_SHAPE_VTABLE.to_le_bytes());
        for k in 0..self.y.len() {
            rec[..4].copy_from_slice(&self.y[k].to_le_bytes());
            for (d, &c) in rec[4..7].iter_mut().zip(&self.normal[k]) {
                *d = c as u8;
            }
            rec[7] = self.flags[k];
            rec[8..12].copy_from_slice(&VTABLE_OF_KIND[usize::from(self.kind[k].min(2))].to_le_bytes());
            w.write_all(&rec)?;
        }
        let at = (crate::navsample::HEADER_SIZE + 20 * self.y.len()) as u64;
        w.write_all(&self.counts)?;
        w.write_all(&at.to_le_bytes())?;
        w.write_all(b"CNT3")?;
        w.flush()
    }

    /// The version-3 file's bytes ([`Samples::write_to`] into memory).
    ///
    /// # Errors
    /// A header value past `float32` or a grid past 32 bits.
    pub fn file_bytes(&self) -> Result<Vec<u8>, crate::NavError> {
        let mut out = Vec::with_capacity(crate::navsample::HEADER_SIZE + 20 * self.y.len() + self.counts.len() + 12);
        self.write_to(&mut out)
            .map_err(|e| crate::NavError::Shape(e.to_string()))?;
        Ok(out)
    }
}

/// `sample_grid`'s normal quantisation as the file ends up with it: `float32(clip(rint(n * 127), -127, 127) / 127)`,
/// then the writer's `rint(that * 127)` in `float32`, clipped.
#[must_use]
pub fn quantise(n: f64) -> i8 {
    let q = (geom::clip((n * 127.0).round_ties_even(), -127.0, 127.0) / 127.0) as f32;
    (q * 127.0).round_ties_even().clamp(-127.0, 127.0) as i8
}

/// The terrain's height and normal at `(x, z)` (`terrain_at(hf, x, z)`; height NaN outside).
#[must_use]
#[allow(clippy::many_single_char_names, reason = "the reference's names")]
pub fn terrain_at(f: &Field<'_>, x: f64, z: f64) -> (f64, [f64; 3]) {
    let y = f.height(x, z);
    let n = f.size as i64;
    let u = (x - f.min.0) / f.cell.0;
    let w = (z - f.min.1) / f.cell.1;
    let i = (u.floor() as i64).clamp(0, (n - 2).max(0));
    let j = (w.floor() as i64).clamp(0, (n - 2).max(0));
    let (fu, fw) = (u - i as f64, w - j as f64);
    let at = |jj: i64, ii: i64| f.heights.get((jj * n + ii) as usize).copied().unwrap_or(f32::NAN);
    let (a, b, c, d) = (at(j, i), at(j, i + 1), at(j + 1, i), at(j + 1, i + 1));
    let upper = fu > fw;
    let gx = f64::from(if upper { b - a } else { d - c }) / f.cell.0;
    let gz = f64::from(if upper { d - b } else { c - a }) / f.cell.1;
    let v = [-gx, 1.0, -gz];
    let l = norm(v);
    (y, [v[0] / l, v[1] / l, v[2] / l])
}

/// The capsule's ring points under its lower hemisphere (`RINGS`): the centre, then radii 0.1 .. 0.4 in 8 or 16
/// directions.
#[must_use]
pub fn rings() -> Vec<(f64, f64)> {
    let mut pts = vec![(0.0, 0.0)];
    for r in [0.1, 0.2, 0.3, 0.35, 0.4] {
        let n = if r < 0.25 { 8 } else { 16 };
        for k in 0..n {
            let a = 2.0 * std::f64::consts::PI * f64::from(k) / f64::from(n);
            pts.push((r * a.cos(), r * a.sin()));
        }
    }
    pts
}

/// The terrain test of one run: ring offsets and their lifts (`terrain_blocks`' `lift`), or the cylinder's ring
/// and its plane test.
#[derive(Debug, Clone)]
enum TerrainTest {
    Rings { rx: Vec<f64>, rz: Vec<f64>, lift: Vec<f64> },
    Plane { rx: Vec<f64>, rz: Vec<f64>, step: f64 },
}

impl TerrainTest {
    fn of(body: &Body) -> Self {
        match *body {
            Body::Capsule { radius, height, bottom } => {
                let r = rings();
                let a0 = capsule_axis(bottom, radius, height).0;
                let lift = r
                    .iter()
                    .map(|&(x, z)| {
                        let d = x.hypot(z);
                        a0 - geom::nmax(radius * radius - d * d, 0.0).sqrt()
                    })
                    .collect();
                TerrainTest::Rings {
                    rx: r.iter().map(|p| p.0).collect(),
                    rz: r.iter().map(|p| p.1).collect(),
                    lift,
                }
            }
            Body::Cylinder { radius, step, .. } => {
                let (mut rx, mut rz) = (vec![0.0], vec![0.0]);
                let rings = (radius / 0.5).ceil().max(1.0) as u32;
                for k in 1..=rings {
                    let r = radius * f64::from(k) / f64::from(rings);
                    for d in 0..32 {
                        let a = 2.0 * std::f64::consts::PI * f64::from(d) / 32.0;
                        rx.push(r * a.cos());
                        rz.push(r * a.sin());
                    }
                }
                TerrainTest::Plane { rx, rz, step }
            }
        }
    }

    /// Does the terrain block a surface at `(x, z, y)` with normal `n`?
    fn blocks(&self, f: &Field<'_>, x: f64, z: f64, y: f64, n: [f64; 3]) -> bool {
        match self {
            TerrainTest::Rings { rx, rz, lift } => rx.iter().zip(rz).zip(lift).any(|((&dx, &dz), &l)| {
                let h = f.height(x + dx, z + dz);
                !h.is_nan() && h - (y + l) > 1e-4
            }),
            TerrainTest::Plane { rx, rz, step } => {
                if n[1] <= 1e-6 {
                    return true;
                }
                rx.iter().zip(rz).any(|(&dx, &dz)| {
                    let h = f.height(x + dx, z + dz);
                    let plane = y - (n[0] * dx + n[2] * dz) / n[1];
                    !h.is_nan() && h - plane > *step
                })
            }
        }
    }
}

/// The 32 directions of the vehicle cylinder's prism (its inscribed 32-gon: within 1.2 cm of the circle at 2.5 m).
fn prism(radius: f64) -> Vec<(f64, f64)> {
    (0..32)
        .map(|k| {
            let a = 2.0 * std::f64::consts::PI * f64::from(k) / 32.0;
            (radius * a.cos(), radius * a.sin())
        })
        .collect()
}

/// The heights a triangle reaches inside the vertical prism around `(px, pz)` (`None`: it does not reach in): the
/// triangle clipped by each side's half-space (Sutherland-Hodgman), its corners' lowest and highest y.
fn prism_interval(t: &Tri, px: f64, pz: f64, sides: &[(f64, f64)]) -> Option<(f64, f64)> {
    let mut poly: Vec<[f64; 3]> = t.to_vec();
    let n = sides.len();
    for k in 0..n {
        let (ax, az) = sides[k];
        let (bx, bz) = sides[(k + 1) % n];
        // inside: left of the edge a -> b (counter-clockwise polygon on x / z)
        let side = |p: &[f64; 3]| (bx - ax) * (p[2] - pz - az) - (bz - az) * (p[0] - px - ax);
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (p, q) = (poly[i], poly[(i + 1) % poly.len()]);
            let (sp, sq) = (side(&p), side(&q));
            if sp >= 0.0 {
                out.push(p);
            }
            if (sp >= 0.0) != (sq >= 0.0) {
                let s = sp / (sp - sq);
                out.push([
                    p[0] + s * (q[0] - p[0]),
                    p[1] + s * (q[1] - p[1]),
                    p[2] + s * (q[2] - p[2]),
                ]);
            }
        }
        poly = out;
        if poly.is_empty() {
            return None;
        }
    }
    let lo = poly.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
    let hi = poly.iter().map(|p| p[1]).fold(f64::NEG_INFINITY, f64::max);
    Some((lo, hi))
}

/// Per triangle: what the band loop needs (the raster and body column ranges, the normal, `det`).
struct Prep {
    /// Raster ranges `(i0, i1, j0, j1)` (the group's grow).
    raster: (i64, i64, i64, i64),
    /// `tri_n`.
    unit: [f64; 3],
    /// `det`.
    det: f64,
    /// Kept by `raster_hits` (`norm(cross) > 1e-12`).
    keep: bool,
}

/// One recorded surface while a band is sampled.
#[derive(Debug, Clone, Copy)]
struct Surface {
    y: f64,
    n: [f64; 3],
    kind: u8,
}

/// One band's output: per column its surface count, and the surfaces (with their runs' flags).
struct Band {
    counts: Vec<u8>,
    surfaces: Vec<(f32, [i8; 3], u8, u32)>,
    columns_with_hits: usize,
    pairs: u64,
}

/// What [`sample`] counted (the reference's log lines).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    /// Columns with at least one hit in the cast's range.
    pub columns_with_hits: usize,
    /// Surfaces recorded.
    pub surfaces: usize,
    /// Triangle-column pairs the body test computed.
    pub pairs: u64,
}

/// Rows per band (each band one task).
const BAND: usize = 8;

/// `sample_grid(grid, geom, hf, bottoms)`: one cast over the grid, one body test per entry of `bodies`; a
/// [`Samples`] per body, in order, and the counts. `field`: the level's height field (`None`: no terrain).
///
/// # Errors
/// [`crate::NavError::Shape`] when the geometry's arrays disagree, the mask does not fit the grid or a column
/// records more than 255 surfaces (the file's count byte), or more than 32 bodies are asked for.
pub fn sample(
    grid: &Grid,
    geo: &Geometry,
    field: Option<&Field<'_>>,
    bodies: &[Body],
) -> Result<(Vec<Samples>, Stats), crate::NavError> {
    let shape = |w: &str| crate::NavError::Shape(format!("offline sampler: {w}"));
    if !geo.consistent() {
        return Err(shape("the geometry's arrays differ in length"));
    }
    if grid.mask.as_ref().is_some_and(|m| m.len() != grid.ncols()) {
        return Err(shape("the mask does not fit the grid"));
    }
    if bodies.len() > 32 {
        return Err(shape("more than 32 bodies"));
    }
    let reach = bodies.iter().map(Body::radius).fold(0.0f64, f64::max);
    let nb = grid.nz.div_ceil(BAND);
    let mut in_band: Vec<Vec<u32>> = vec![Vec::new(); nb];
    for (k, t) in geo.tris.iter().enumerate() {
        let zs = [t[0][2], t[1][2], t[2][2]];
        // the widest of the raster's grows and the bodies' radii
        let r = reach.max(CONVEX_GROW);
        let (j0, j1) = ranges(min3(zs) - r, max3(zs) + r, grid.min_z, grid.pitch, grid.nz);
        if j1 < j0 {
            continue;
        }
        for b in (j0 as usize / BAND)..=(j1 as usize / BAND).min(nb.saturating_sub(1)) {
            in_band[b].push(k as u32);
        }
    }
    let tests: Vec<TerrainTest> = bodies.iter().map(TerrainTest::of).collect();
    let sides: Vec<Option<Vec<(f64, f64)>>> = bodies
        .iter()
        .map(|b| match *b {
            Body::Cylinder { radius, .. } => Some(prism(radius)),
            Body::Capsule { .. } => None,
        })
        .collect();
    let ctx = Ctx {
        grid,
        geo,
        field,
        bodies,
        tests: &tests,
        sides: &sides,
    };
    let bands: Vec<Band> = in_band
        .par_iter()
        .enumerate()
        .map(|(b, tris)| ctx.band(b, tris))
        .collect::<Result<_, _>>()?;
    drop(in_band);
    let total: usize = bands.iter().map(|b| b.surfaces.len()).sum();
    let mut surf = Surfaces {
        counts: Vec::with_capacity(grid.ncols()),
        y: Vec::with_capacity(total),
        normal: Vec::with_capacity(total),
        kind: Vec::with_capacity(total),
    };
    let mut flags: Vec<Vec<u8>> = bodies.iter().map(|_| Vec::with_capacity(total)).collect();
    let mut stats = Stats::default();
    // each band's records moved out and its memory freed as it goes
    for b in bands {
        surf.counts.extend_from_slice(&b.counts);
        stats.columns_with_hits += b.columns_with_hits;
        stats.pairs += b.pairs;
        for (yy, n, k, f) in b.surfaces {
            surf.y.push(yy);
            surf.normal.push(n);
            surf.kind.push(k);
            for (r, fl) in flags.iter_mut().enumerate() {
                fl.push(u8::from(f >> r & 1 == 1));
            }
        }
    }
    stats.surfaces = surf.y.len();
    let surf = std::sync::Arc::new(surf);
    let out = bodies
        .iter()
        .zip(flags)
        .map(|(body, flags)| {
            let (radius, bottom, top) = body.header();
            Samples {
                header: crate::navsample::Header {
                    pitch: f64::from(grid.pitch as f32),
                    min_x: f64::from(grid.min_x as f32),
                    min_z: f64::from(grid.min_z as f32),
                    nx: grid.nx as i64,
                    nz: grid.nz as i64,
                    y_top: grid.y_top,
                    y_bottom: grid.y_bottom,
                    radius,
                    cap_bottom: bottom,
                    cap_top: top,
                    ray_radius: RAY_RADIUS,
                },
                surfaces: std::sync::Arc::clone(&surf),
                flags,
            }
        })
        .collect();
    Ok((out, stats))
}

fn min3(v: [f64; 3]) -> f64 {
    geom::nmin(geom::nmin(v[0], v[1]), v[2])
}

fn max3(v: [f64; 3]) -> f64 {
    geom::nmax(geom::nmax(v[0], v[1]), v[2])
}

struct Ctx<'a> {
    grid: &'a Grid,
    geo: &'a Geometry,
    field: Option<&'a Field<'a>>,
    bodies: &'a [Body],
    tests: &'a [TerrainTest],
    sides: &'a [Option<Vec<(f64, f64)>>],
}

/// A candidate hit while a band is rasterised: its column in the band, height, normal, kind.
#[derive(Debug, Clone, Copy)]
struct Cand {
    col: u32,
    y: f64,
    n: [f64; 3],
    kind: u8,
}

impl Ctx<'_> {
    /// Triangle `t`'s raster ranges (its group's grow), normal, `det` and `raster_hits`' keep test.
    fn prep(&self, t: usize) -> Prep {
        let tri = &self.geo.tris[t];
        let g = self.grid;
        let xs = [tri[0][0], tri[1][0], tri[2][0]];
        let zs = [tri[0][2], tri[1][2], tri[2][2]];
        let grow = if self.geo.kind[t] == KIND_MESH {
            EDGE_GROW
        } else {
            CONVEX_GROW
        };
        let ri = ranges(min3(xs) - grow, max3(xs) + grow, g.min_x, g.pitch, g.nx);
        let rj = ranges(min3(zs) - grow, max3(zs) + grow, g.min_z, g.pitch, g.nz);
        Prep {
            raster: (ri.0, ri.1, rj.0, rj.1),
            unit: unit_normal(tri),
            det: det(tri),
            keep: norm(tri_cross(tri)) > 1e-12,
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "the reference's sample_grid and cast_columns, one band"
    )]
    fn band(&self, b: usize, tris: &[u32]) -> Result<Band, crate::NavError> {
        let g = self.grid;
        let (j0, j1) = (b * BAND, ((b + 1) * BAND).min(g.nz));
        let ncol = (j1 - j0) * g.nx;
        let mut band = Band {
            counts: vec![0; ncol],
            surfaces: Vec::new(),
            columns_with_hits: 0,
            pairs: 0,
        };
        let local = |i: i64, j: i64| (j as usize - j0) * g.nx + i as usize;
        let global = |l: usize| j0 * g.nx + l;
        let any = (0..ncol).any(|l| g.sampled(global(l)));
        if !any {
            return Ok(band);
        }
        // the cast's candidates: mesh triangles, then convex ones (each by index), then the terrain
        let mut cands: Vec<Cand> = Vec::new();
        // convex face hits by (column, piece): the solid intervals
        let mut solid_hits: Vec<(u32, i32, f64)> = Vec::new();
        for group in [KIND_MESH, KIND_CONVEX] {
            let grow = if group == KIND_MESH { EDGE_GROW } else { CONVEX_GROW };
            for &t in tris {
                let t = t as usize;
                let is_mesh = self.geo.kind[t] == KIND_MESH;
                if is_mesh != (group == KIND_MESH) {
                    continue;
                }
                let p = self.prep(t);
                if !p.keep {
                    continue;
                }
                let (i0, i1, rj0, rj1) = p.raster;
                let tri = &self.geo.tris[t];
                for j in rj0.max(j0 as i64)..=rj1.min(j1 as i64 - 1) {
                    let pz = g.z(j as usize);
                    for i in i0..=i1 {
                        let l = local(i, j);
                        if !g.sampled(global(l)) {
                            continue;
                        }
                        let px = g.x(i as usize);
                        let Some(h) = raster_pair(tri, p.det, px, pz, grow) else {
                            continue;
                        };
                        if group == KIND_MESH {
                            let mut n = p.unit;
                            if n[1] < 0.0 {
                                n = [-n[0], -n[1], -n[2]];
                            }
                            let n = h.edge.unwrap_or(n);
                            cands.push(Cand {
                                col: l as u32,
                                y: h.y,
                                n,
                                kind: self.geo.vt[t],
                            });
                        } else {
                            if h.edge.is_none() {
                                solid_hits.push((l as u32, self.geo.solid[t], h.y));
                            }
                            // only the upward faces and the sphere's touches are hits
                            if p.unit[1] > 0.0 || h.edge.is_some() {
                                cands.push(Cand {
                                    col: l as u32,
                                    y: h.y,
                                    n: h.edge.unwrap_or(p.unit),
                                    kind: self.geo.vt[t],
                                });
                            }
                        }
                    }
                }
            }
        }
        // the terrain under every sampled column
        let mut ty = vec![f64::NAN; ncol];
        if let Some(f) = self.field {
            for (l, slot) in ty.iter_mut().enumerate() {
                let c = global(l);
                if !g.sampled(c) {
                    continue;
                }
                let (x, z) = (g.x(c % g.nx), g.z(c / g.nx));
                let (h, n) = terrain_at(f, x, z);
                *slot = h;
                if h < g.y_top && h > g.y_bottom {
                    cands.push(Cand {
                        col: l as u32,
                        y: h + RAY_RADIUS * (1.0 - n[1]),
                        n,
                        kind: KIND_TERRAIN,
                    });
                }
            }
        }
        // the cast runs from y top down to y bottom
        cands.retain(|c| c.y <= g.y_top + RAY_RADIUS && c.y >= g.y_bottom);
        // lexsort((-y, column)): stable, so ties keep the order above
        cands.sort_by(|a, b| {
            a.col
                .cmp(&b.col)
                .then_with(|| b.y.partial_cmp(&a.y).unwrap_or(std::cmp::Ordering::Equal))
        });
        // solid intervals per column (Intervals): merged by their float keys
        solid_hits.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
        let k2 = 2f64.powi((g.y_top - g.y_bottom + 200.0).log2().ceil() as i32);
        let off = 100.0 - g.y_bottom;
        let mut solids: Vec<(u32, f64, f64)> = Vec::new(); // column, key lo, key hi (merged)
        {
            let mut raw: Vec<(u32, f64, f64)> = Vec::new();
            let mut k = 0;
            while k < solid_hits.len() {
                let (c, s, _) = solid_hits[k];
                let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
                while k < solid_hits.len() && solid_hits[k].0 == c && solid_hits[k].1 == s {
                    lo = geom::nmin(lo, solid_hits[k].2);
                    hi = geom::nmax(hi, solid_hits[k].2);
                    k += 1;
                }
                let lo = geom::clip(lo - RAY_RADIUS, g.y_bottom - 50.0, g.y_top + 50.0);
                let hi = geom::clip(hi + RAY_RADIUS, g.y_bottom - 50.0, g.y_top + 50.0);
                if lo <= hi {
                    let cg = global(c as usize) as f64;
                    raw.push((c, cg * k2 + (lo + off), cg * k2 + (hi + off)));
                }
            }
            raw.sort_by(|a, b| {
                a.0.cmp(&b.0)
                    .then_with(|| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            });
            for (c, lo, hi) in raw {
                match solids.last_mut() {
                    Some(last) if last.0 == c && !(lo > last.2) => last.2 = geom::nmax(last.2, hi),
                    _ => solids.push((c, lo, hi)),
                }
            }
        }
        // the cast, column by column
        let kk = 4096.0;
        let y_off = g.y_top + 100.0;
        let mut surf: Vec<(usize, Surface)> = Vec::new();
        let (mut a, mut sa) = (0usize, 0usize);
        while a < cands.len() {
            let c = cands[a].col;
            let mut e = a;
            while e < cands.len() && cands[e].col == c {
                e += 1;
            }
            band.columns_with_hits += 1;
            let col = &cands[a..e];
            let cg = global(c as usize) as f64;
            let keys: Vec<f64> = col.iter().map(|h| cg * kk + (y_off - h.y)).collect();
            while sa < solids.len() && solids[sa].0 < c {
                sa += 1;
            }
            let mut sb = sa;
            while sb < solids.len() && solids[sb].0 == c {
                sb += 1;
            }
            let sol = &solids[sa..sb];
            let t = ty[c as usize];
            let (mut start, mut run, mut count) = (g.y_top, 0.0f64, 0u32);
            loop {
                let st = start;
                let q = cg * kk + (y_off - (st + RAY_RADIUS));
                let p = keys.partition_point(|&k| k < q);
                let have = p < keys.len();
                let y = if have { col[p].y } else { f64::NEG_INFINITY };
                let at_once = have && st - y <= AT_ONCE;
                let under = !t.is_nan() && st < t + RAY_RADIUS;
                let qs = cg * k2 + (st + off);
                let k = sol.partition_point(|s| s.1 <= qs);
                let inside = k > 0 && sol[k - 1].2 >= qs;
                let stop = under || (!have && !inside) || st <= g.y_bottom;
                if stop {
                    break;
                }
                if at_once || inside {
                    run += SOLID_STEP;
                    start -= SOLID_STEP;
                    if run > MAX_SOLID {
                        break;
                    }
                    continue;
                }
                surf.push((
                    c as usize,
                    Surface {
                        y,
                        n: col[p].n,
                        kind: col[p].kind,
                    },
                ));
                count += 1;
                start = y - SKIP;
                run = 0.0;
                if count >= MAX_LAYERS {
                    break;
                }
            }
            a = e;
        }
        // the body tests: every triangle near a column with surfaces
        let mut flags = vec![0u32; surf.len()];
        let mut first = vec![usize::MAX; ncol];
        let mut last = vec![0usize; ncol];
        for (k, (c, _)) in surf.iter().enumerate() {
            if first[*c] == usize::MAX {
                first[*c] = k;
            }
            last[*c] = k + 1;
        }
        let y_off2 = g.y_top + 200.0;
        let axes: Vec<Option<(f64, f64)>> = self
            .bodies
            .iter()
            .map(|b| match *b {
                Body::Capsule { radius, height, bottom } => Some(capsule_axis(bottom, radius, height)),
                Body::Cylinder { .. } => None,
            })
            .collect();
        let radii: Vec<f64> = self.bodies.iter().map(Body::radius).collect();
        let mut by_radius: Vec<f64> = radii.clone();
        by_radius.sort_by(f64::total_cmp);
        by_radius.dedup();
        for &t in tris {
            let t = t as usize;
            let tri = &self.geo.tris[t];
            let xs = [tri[0][0], tri[1][0], tri[2][0]];
            let zs = [tri[0][2], tri[1][2], tri[2][2]];
            for &r in &by_radius {
                let (i0, i1) = ranges(min3(xs) - r, max3(xs) + r, g.min_x, g.pitch, g.nx);
                let (bj0, bj1) = ranges(min3(zs) - r, max3(zs) + r, g.min_z, g.pitch, g.nz);
                for j in bj0.max(j0 as i64)..=bj1.min(j1 as i64 - 1) {
                    let pz = g.z(j as usize);
                    for i in i0..=i1 {
                        let l = local(i, j);
                        if first[l] == usize::MAX {
                            continue;
                        }
                        let px = g.x(i as usize);
                        let cg = global(l) as f64;
                        let mut interval = None;
                        for (run_i, body) in self.bodies.iter().enumerate() {
                            if radii[run_i] != r {
                                continue;
                            }
                            match (*body, axes[run_i]) {
                                (Body::Capsule { .. }, Some((c0, c1))) => {
                                    let (lo, hi) = *interval.get_or_insert_with(|| {
                                        band.pairs += 1;
                                        line_tri_interval(tri, px, pz, r)
                                    });
                                    if !(lo <= hi) {
                                        continue;
                                    }
                                    let ya = geom::clip(lo - c1, 1.0 - y_off2, kk - 1.0 - y_off2);
                                    let yb = geom::clip(hi - c0, 1.0 - y_off2, kk - 1.0 - y_off2);
                                    let (qa, qb) = (cg * kk + (ya + y_off2), cg * kk + (yb + y_off2));
                                    for (k, (_, s)) in surf.iter().enumerate().take(last[l]).skip(first[l]) {
                                        let key = cg * kk + (s.y + y_off2);
                                        if qa <= key && key <= qb {
                                            flags[k] |= 1 << run_i;
                                        }
                                    }
                                }
                                (Body::Cylinder { step, height, .. }, _) => {
                                    band.pairs += 1;
                                    let Some((lo, hi)) =
                                        prism_interval(tri, px, pz, self.sides[run_i].as_deref().unwrap_or(&[]))
                                    else {
                                        continue;
                                    };
                                    for (k, (_, s)) in surf.iter().enumerate().take(last[l]).skip(first[l]) {
                                        if hi >= s.y + step && lo <= s.y + height {
                                            flags[k] |= 1 << run_i;
                                        }
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        if let Some(f) = self.field {
            for (k, (c, s)) in surf.iter().enumerate() {
                let cgl = global(*c);
                let (x, z) = (g.x(cgl % g.nx), g.z(cgl / g.nx));
                for (r, test) in self.tests.iter().enumerate() {
                    if flags[k] >> r & 1 == 0 && test.blocks(f, x, z, s.y, s.n) {
                        flags[k] |= 1 << r;
                    }
                }
            }
        }
        for (k, (c, s)) in surf.iter().enumerate() {
            let n = &mut band.counts[*c];
            *n = n
                .checked_add(1)
                .ok_or_else(|| crate::NavError::Shape("more than 255 surfaces in a column".into()))?;
            band.surfaces.push((s.y as f32, s.n.map(quantise), s.kind, flags[k]));
        }
        Ok(band)
    }
}

#[cfg(test)]
mod tests;
