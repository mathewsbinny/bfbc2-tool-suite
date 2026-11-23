//! NavKit's walkability end to end (round NK4): what `navbuild.stage` did from the sample file to the kept surfaces,
//! ported from the Python (`bfbc2-auto-navkit/tools/navwalk.py`, `navsample.py`, `navedits.py`, `navbuild.py`),
//! which was retired after the parity run (`rust/BENCHMARKS.md`, "NK4"):
//!
//! | Python | here |
//! |---|---|
//! | `navsample.load` (the records into `Samples`) | [`Samples::load`] |
//! | `navwalk.coarse` | [`Samples::coarse`] |
//! | `navedits.surface_positions`, `region_mask` (matplotlib's `Path.contains_points`), `edits_for`'s masks | [`edit_masks`], [`points_in_polygon`] |
//! | `navbuild.water_tops`, `deep_water` | [`water_tops`], [`deep_water`] |
//! | `navbuild.outside_bounds` | [`outside_bounds`] |
//! | `navwalk.walkable` with `near_points`, `ladder_ends`, `objective_floor`, its NumPy glue (the masks, the gradients, the seeds, the floods, the ladders, the unseeds, the roof islands) | [`walkable`] |
//! | `navbuild.stage`'s walkability part (load, coarse, edits, water, extent, walkable) | [`stage`] |
//!
//! The rules it calls ([`crate::rules`], [`Level::flood`], [`graph::roof_drop`]) were ported before (E2a, E3c, E3h).
//!
//! **Floats.** Everything is computed as the Python computed it, bit for bit: the heights as `float64`, the normals
//! and gradients in the dtype the run holds them (`float32` from a sample file), with NumPy 2's rules (NEP 50: a
//! Python float meeting a `float32` array or scalar - in a comparison too - is cast to `float32` first: [`weak`]);
//! Python's `round` half to even; Python's `min` / `max` (the first of equals, NaN as it falls). No sum over many
//! values reaches an answer (the one median takes the mean of two values), and no set or dict iteration order: the
//! Python's sets were only ever tested for membership or counted, so the port needs no rule of order (BENCHMARKS
//! "NK4", "Python's order").
//!
//! The trigonometric constants (`cos(radians(slope)) - 0.01`, `tan(radians(slope))`, ...) are computed by the caller
//! in Python (the C library's functions, as before) and passed in [`WalkSettings`].
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::verbose_bit_mask,
    reason = "the reference's names and expressions"
)]

use std::sync::Arc;

use crate::graph::{self, RoofCfg, RoofGrid};
use crate::rules::{Grid, LipTests, Tri};
use crate::{Floats, Level, NavError, matched_flags, navsample, pymath};

/// A point (x, y, z).
pub type P3 = (f64, f64, f64);

fn shape(what: impl Into<String>) -> NavError {
    NavError::Shape(what.into())
}

/// Python's `int(round(v))` on a float: half to even; an error where Python raises (NaN, infinity).
fn round_int(v: f64) -> Result<i64, NavError> {
    let r = v.round_ties_even();
    if r.is_finite() && r.abs() < 9.0e18 {
        Ok(r as i64)
    } else {
        Err(shape("a point whose column index is not a number"))
    }
}

/// `int(math.floor(v))` / `int(math.ceil(v))`: an error where Python raises.
fn floor_int(v: f64) -> Result<i64, NavError> {
    let r = v.floor();
    if r.is_finite() && r.abs() < 9.0e18 {
        Ok(r as i64)
    } else {
        Err(shape("a bound that is not a number"))
    }
}

fn ceil_int(v: f64) -> Result<i64, NavError> {
    floor_int(-v).map(|x| -x)
}

/// Python's `min` / `max` of three floats (the first unless a later one is less / greater).
fn py_min3(a: f64, b: f64, c: f64) -> f64 {
    let m = if b < a { b } else { a };
    if c < m { c } else { m }
}

fn py_max3(a: f64, b: f64, c: f64) -> f64 {
    let m = if b > a { b } else { a };
    if c > m { c } else { m }
}

/// NumPy's `maximum` (NaN when either is).
fn np_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() { f64::NAN } else { a.max(b) }
}

/// Python's float `x % y` (the sign of y; `None` for y = 0).
fn py_mod(x: f64, y: f64) -> Option<f64> {
    if y == 0.0 {
        return None;
    }
    let r = x % y;
    Some(if r == 0.0 {
        0.0f64.copysign(y)
    } else if (r < 0.0) != (y < 0.0) {
        r + y
    } else {
        r
    })
}

/// A Python float `c` as NumPy 2 compares it with an array of the run's dtype (NEP 50: a `float32` array casts it to
/// `float32`); `f32_` says the array is `float32`.
#[must_use]
pub fn weak(c: f64, f32_: bool) -> f64 {
    if f32_ { f64::from(c as f32) } else { c }
}

/// A `float32` height widened to `float64` with its bits kept, a NaN's too (a signalling NaN stays signalling, where a
/// conversion would quiet it): [`narrow`] gives back the file's exact bits (a damaged file's NaN heights; every
/// comparison treats both NaNs alike).
#[must_use]
pub fn widen(v: f32) -> f64 {
    if v.is_nan() {
        let b = v.to_bits();
        f64::from_bits((u64::from(b >> 31) << 63) | (0x7ff << 52) | (u64::from(b & 0x7f_ffff) << 29))
    } else {
        f64::from(v)
    }
}

/// [`widen`]'s inverse for a value widened from `float32`.
#[must_use]
pub fn narrow(v: f64) -> f32 {
    if v.is_nan() {
        let b = v.to_bits();
        f32::from_bits((((b >> 63) as u32) << 31) | (0xff << 23) | (((b >> 29) & 0x7f_ffff) as u32))
    } else {
        v as f32
    }
}

/// The normals of a run, in their dtype.
#[derive(Debug, Clone, PartialEq)]
pub enum Normals {
    /// `float32` (a sample file's).
    F32(Vec<[f32; 3]>),
    /// `float64`.
    F64(Vec<[f64; 3]>),
}

impl Normals {
    /// The number of surfaces.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Normals::F32(v) => v.len(),
            Normals::F64(v) => v.len(),
        }
    }

    /// No surfaces.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Component `a` of surface k as `f64` (exact).
    fn at(&self, k: usize, a: usize) -> f64 {
        match self {
            Normals::F32(v) => v.get(k).and_then(|n| n.get(a)).map_or(f64::NAN, |&x| f64::from(x)),
            Normals::F64(v) => v.get(k).and_then(|n| n.get(a)).copied().unwrap_or(f64::NAN),
        }
    }

    /// `normal[k, 1] >= c` with a Python float c (cast to the normals' dtype first).
    fn y_ge(&self, k: usize, c: f64) -> bool {
        self.at(k, 1) >= weak(c, matches!(self, Normals::F32(_)))
    }

    /// `normal[:, 1]` as [`Floats`] (`narrow_lips`' flat test).
    fn ys(&self) -> Floats {
        match self {
            Normals::F32(v) => Floats::F32(v.iter().map(|n| n[1]).collect()),
            Normals::F64(v) => Floats::F64(v.iter().map(|n| n[1]).collect()),
        }
    }

    /// `walkable`'s first gradients: `ny = np.maximum(normal[:, 1], 0.05)`, `gx = -normal[:, 0] / ny`,
    /// `gz = -normal[:, 2] / ny`, in the normals' dtype.
    fn gradients(&self) -> (Floats, Floats) {
        match self {
            Normals::F32(v) => {
                let ny = |n: &[f32; 3]| if n[1].is_nan() { f32::NAN } else { n[1].max(0.05f32) };
                (
                    Floats::F32(v.iter().map(|n| -n[0] / ny(n)).collect()),
                    Floats::F32(v.iter().map(|n| -n[2] / ny(n)).collect()),
                )
            }
            Normals::F64(v) => {
                let ny = |n: &[f64; 3]| np_max(n[1], 0.05);
                (
                    Floats::F64(v.iter().map(|n| -n[0] / ny(n)).collect()),
                    Floats::F64(v.iter().map(|n| -n[2] / ny(n)).collect()),
                )
            }
        }
    }

    fn pick(&self, idx: &[usize]) -> Normals {
        match self {
            Normals::F32(v) => Normals::F32(idx.iter().filter_map(|&k| v.get(k).copied()).collect()),
            Normals::F64(v) => Normals::F64(idx.iter().filter_map(|&k| v.get(k).copied()).collect()),
        }
    }
}

/// A sample run (`navsample.Samples`): the header's values, per column the range of its surfaces, per surface its
/// height (held as `float64`; `y_f32`: the run's heights are `float32`), normal, flags and hit words.
#[derive(Debug, Clone)]
pub struct Samples {
    /// The grid's pitch (m).
    pub pitch: f64,
    /// The grid's x origin.
    pub min_x: f64,
    /// The grid's z origin.
    pub min_z: f64,
    /// Columns along x.
    pub nx: usize,
    /// Columns along z.
    pub nz: usize,
    /// `y_top, y_bottom, radius, cap_bottom, cap_top, ray_radius`.
    pub header: [f64; 6],
    /// Per column its first surface (`nx * nz + 1` values).
    pub start: Arc<[usize]>,
    /// Heights.
    pub y: Arc<[f64]>,
    /// The heights' dtype is `float32`.
    pub y_f32: bool,
    /// Normals.
    pub normal: Normals,
    /// Capsule flags.
    pub flags: Vec<u8>,
    /// The hit words (`None`: none in the run - version 1 - or none given).
    pub hit: Option<Vec<[u32; 3]>>,
}

/// The header (`navsample.HEADER`, `<4sI3f2i6fI`): version, pitch, min x, min z, nx, nz, the other six floats.
type Header = (u32, [f64; 3], i64, i64, [f64; 6]);

/// A version 3 run read from its file a block of records at a time ([`stream_v3`]).
struct Streamed {
    grid: [f64; 3],
    nx: usize,
    nz: usize,
    header: [f64; 6],
    counts: usize,
    start: Vec<usize>,
    y: Vec<f64>,
    flags: Vec<u8>,
    normal: Vec<[f32; 3]>,
    hit: Vec<[u32; 3]>,
}

/// A version 3 sample file read without holding the whole file (round NK4: a pitch-0.2 run is 100-650 MB, and the
/// stage reads two): the header, the counts the trailer points at, then the records in blocks. `full`: the normals and
/// hit words too (a high or narrow run needs only heights and flags). `Ok(None)` for any file that is not a whole
/// version 3 run as [`navsample::V3`] reads it: the caller reads it whole and reports what is wrong as before.
#[allow(clippy::too_many_lines)] // the reads in their order
fn stream_v3(path: &std::path::Path, full: bool) -> Result<Option<Streamed>, NavError> {
    use std::io::{Read, Seek, SeekFrom};
    let io = |e: std::io::Error| shape(format!("{}: {e}", path.display()));
    let Ok(mut f) = std::fs::File::open(path) else {
        return Ok(None);
    };
    let len = f.metadata().map_err(io)?.len();
    let hs = navsample::HEADER_SIZE as u64;
    if len < hs + 12 {
        return Ok(None);
    }
    let mut head = [0u8; navsample::HEADER_SIZE];
    f.read_exact(&mut head).map_err(io)?;
    let Ok((version, grid, nx, nz, header)) = header(&head) else {
        return Ok(None);
    };
    let (Ok(unx), Ok(unz)) = (usize::try_from(nx), usize::try_from(nz)) else {
        return Ok(None);
    };
    let Some(cols) = unx.checked_mul(unz) else {
        return Ok(None);
    };
    if version != 3 {
        return Ok(None);
    }
    let mut tail = [0u8; 12];
    f.seek(SeekFrom::End(-12)).map_err(io)?;
    f.read_exact(&mut tail).map_err(io)?;
    let (at, magic) = tail.split_at(8);
    if magic != b"CNT3" {
        return Ok(None);
    }
    let at = u64::from_le_bytes(at.try_into().map_err(|_| shape("trailer"))?);
    if at.checked_add(cols as u64).is_none_or(|end| end > len) {
        return Ok(None);
    }
    let mut counts = vec![0u8; cols];
    f.seek(SeekFrom::Start(at)).map_err(io)?;
    f.read_exact(&mut counts).map_err(io)?;
    let n: usize = counts.iter().map(|&c| usize::from(c)).sum();
    if hs + 20 * n as u64 > len {
        return Ok(None);
    }
    let mut start = Vec::with_capacity(cols + 1);
    let mut total = 0usize;
    start.push(0);
    for &c in &counts {
        total += usize::from(c);
        start.push(total);
    }
    drop(counts);
    let mut y = Vec::with_capacity(n);
    let mut flags = Vec::with_capacity(n);
    let (mut normal, mut hit) = if full {
        (Vec::with_capacity(n), Vec::with_capacity(n))
    } else {
        (Vec::new(), Vec::new())
    };
    f.seek(SeekFrom::Start(hs)).map_err(io)?;
    let mut buf = vec![0u8; 20 << 16];
    let mut left = n;
    while left > 0 {
        let take = left.min(1 << 16);
        let block = buf.get_mut(..20 * take).ok_or_else(|| shape("block"))?;
        f.read_exact(block).map_err(io)?;
        for r in block.as_chunks::<20>().0 {
            let [
                y0,
                y1,
                y2,
                y3,
                a,
                b,
                d,
                fl,
                h0,
                h1,
                h2,
                h3,
                h4,
                h5,
                h6,
                h7,
                h8,
                h9,
                h10,
                h11,
            ] = *r;
            y.push(widen(f32::from_le_bytes([y0, y1, y2, y3])));
            flags.push(fl);
            if full {
                normal.push([navsample::normal(a), navsample::normal(b), navsample::normal(d)]);
                hit.push([
                    u32::from_le_bytes([h0, h1, h2, h3]),
                    u32::from_le_bytes([h4, h5, h6, h7]),
                    u32::from_le_bytes([h8, h9, h10, h11]),
                ]);
            }
        }
        left -= take;
    }
    Ok(Some(Streamed {
        grid,
        nx: unx,
        nz: unz,
        header,
        counts: cols,
        start,
        y,
        flags,
        normal,
        hit,
    }))
}

/// A file's arrays: column starts, heights, normals, flags, hit words, the columns read.
type Parsed = (
    Vec<usize>,
    Vec<f64>,
    Vec<[f32; 3]>,
    Vec<u8>,
    Option<Vec<[u32; 3]>>,
    usize,
);

/// A water mesh: vertices, triangles (vertex indices).
pub type WaterMesh = (Vec<[f64; 3]>, Vec<[i64; 3]>);

/// `objective_floor`'s answer: the surfaces to add, (points, surfaces) for the statistics.
type Added = (Vec<bool>, Option<(usize, usize)>);

fn header(data: &[u8]) -> Result<Header, NavError> {
    let bad = || shape("not a navsample file");
    let h = data.get(..navsample::HEADER_SIZE).ok_or_else(bad)?;
    let u32_at = |o: usize| h.get(o..o + 4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes);
    let f32_at = |o: usize| u32_at(o).map(|v| f64::from(f32::from_bits(v)));
    if h.get(..4) != Some(b"ONS1".as_slice()) {
        return Err(bad());
    }
    let version = u32_at(4).ok_or_else(bad)?;
    if !(1..=3).contains(&version) {
        return Err(bad());
    }
    let f = |o: usize| f32_at(o).ok_or_else(bad);
    let i = |o: usize| u32_at(o).map(|v| i64::from(v as i32)).ok_or_else(bad);
    Ok((
        version,
        [f(8)?, f(12)?, f(16)?],
        i(20)?,
        i(24)?,
        [f(28)?, f(32)?, f(36)?, f(40)?, f(44)?, f(48)?],
    ))
}

impl Samples {
    /// The number of surfaces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.y.len()
    }

    /// No surfaces.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.y.is_empty()
    }

    /// The run checked: the column starts describe the surfaces, every array has a value per surface.
    ///
    /// # Errors
    /// [`NavError::Shape`] when they do not.
    pub fn check(&self) -> Result<(), NavError> {
        let n = self.len();
        let cols = self.nx.checked_mul(self.nz).ok_or_else(|| shape("grid too large"))?;
        if self.start.len() != cols + 1
            || self.start.first() != Some(&0)
            || self.start.last() != Some(&n)
            || self.start.windows(2).any(|w| w.first() > w.get(1))
        {
            return Err(shape("the column starts do not describe the surfaces"));
        }
        if self.normal.len() != n || self.flags.len() != n || self.hit.as_ref().is_some_and(|h| h.len() != n) {
            return Err(shape("the sample arrays differ in length"));
        }
        Ok(())
    }

    /// `navsample.load(path)`: the run and the columns read (an interrupted version 1 / 2 run reads fewer than
    /// nx * nz; the rest are empty). A version 1 run has no hit words: `hit` is `None` (the Python held an empty
    /// array, which `walkable` could not use).
    ///
    /// # Errors
    /// [`NavError::Shape`] where `navsample.load` raises (the file cannot be read, another magic or version, records
    /// or counts cut short); the caller lets `navsample.load` say why in its own words.
    pub fn load(path: &std::path::Path) -> Result<(Samples, usize), NavError> {
        if let Some(got) = Self::load_v3(path)? {
            return Ok(got);
        }
        let data = std::fs::read(path).map_err(|e| shape(format!("{}: {e}", path.display())))?;
        Self::parse(&data)
    }

    /// [`Samples::load`] of a whole version 3 file, read a block of records at a time; `Ok(None)` for any other file
    /// (`navsample.load` then reads it whole, as before).
    ///
    /// # Errors
    /// [`NavError::Shape`] when the file cannot be read once it was found to be a version 3 run.
    pub fn load_v3(path: &std::path::Path) -> Result<Option<(Samples, usize)>, NavError> {
        if let Some(v) = stream_v3(path, true)? {
            let read = v.counts;
            let s = Samples {
                pitch: v.grid[0],
                min_x: v.grid[1],
                min_z: v.grid[2],
                nx: v.nx,
                nz: v.nz,
                header: v.header,
                start: v.start.into(),
                y: v.y.into(),
                y_f32: true,
                normal: Normals::F32(v.normal),
                flags: v.flags,
                hit: Some(v.hit),
            };
            s.check()?;
            return Ok(Some((s, read)));
        }
        Ok(None)
    }

    /// [`Samples::load`] on the file's bytes.
    ///
    /// # Errors
    /// As [`Samples::load`].
    pub fn parse(data: &[u8]) -> Result<(Samples, usize), NavError> {
        let (version, [pitch, min_x, min_z], nx, nz, rest) = header(data)?;
        let (unx, unz) = (
            usize::try_from(nx).map_err(|_| shape("a negative grid size"))?,
            usize::try_from(nz).map_err(|_| shape("a negative grid size"))?,
        );
        let (start, y, normal, flags, hit, read): Parsed = if version == 3 {
            let v = navsample::V3::new(data, nx, nz)?;
            let mut start = Vec::with_capacity(v.counts.len() + 1);
            let mut total = 0usize;
            start.push(0);
            for &c in v.counts {
                total += usize::from(c);
                start.push(total);
            }
            let mut y = Vec::with_capacity(v.records.len());
            let mut normal = Vec::with_capacity(v.records.len());
            let mut flags = Vec::with_capacity(v.records.len());
            let mut hit = Vec::with_capacity(v.records.len());
            for r in v.records {
                let [
                    y0,
                    y1,
                    y2,
                    y3,
                    a,
                    b,
                    d,
                    f,
                    h0,
                    h1,
                    h2,
                    h3,
                    h4,
                    h5,
                    h6,
                    h7,
                    h8,
                    h9,
                    h10,
                    h11,
                ] = *r;
                y.push(widen(f32::from_le_bytes([y0, y1, y2, y3])));
                normal.push([navsample::normal(a), navsample::normal(b), navsample::normal(d)]);
                flags.push(f);
                hit.push([
                    u32::from_le_bytes([h0, h1, h2, h3]),
                    u32::from_le_bytes([h4, h5, h6, h7]),
                    u32::from_le_bytes([h8, h9, h10, h11]),
                ]);
            }
            (start, y, normal, flags, Some(hit), v.counts.len())
        } else {
            let cols = unx.checked_mul(unz).ok_or_else(|| shape("grid too large"))?;
            let r = navsample::parse_runs(data, navsample::HEADER_SIZE, cols, version)?;
            let start = r
                .start
                .iter()
                .map(|&v| usize::try_from(v).map_err(|_| shape("a negative column start")))
                .collect::<Result<Vec<usize>, NavError>>()?;
            let normal = r.normal.as_chunks::<3>().0.to_vec();
            let hit = (version == 2).then(|| r.hit.as_chunks::<3>().0.to_vec());
            (
                start,
                r.y.iter().map(|&v| widen(v)).collect(),
                normal,
                r.flags,
                hit,
                r.read,
            )
        };
        let s = Samples {
            pitch,
            min_x,
            min_z,
            nx: unx,
            nz: unz,
            header: rest,
            start: start.into(),
            y: y.into(),
            y_f32: true,
            normal: Normals::F32(normal),
            flags,
            hit,
        };
        s.check()?;
        Ok((s, read))
    }

    /// `navwalk.coarse(f)`: every (`to_pitch` / pitch)-th column of a fine run, keeping all surfaces of the column,
    /// at pitch `to_pitch` (`navwalk.PITCH`).
    ///
    /// # Errors
    /// [`NavError::Shape`] for a step Python could not take (not a positive whole number).
    pub fn coarse(&self, to_pitch: f64) -> Result<Samples, NavError> {
        let step = round_int(to_pitch / self.pitch)?;
        if step <= 0 {
            return Err(shape("coarse: a step that is not positive"));
        }
        let step = step as usize;
        let shrink = |n: usize| if n == 0 { 0 } else { (n - 1) / step + 1 };
        let (nx, nz) = (shrink(self.nx), shrink(self.nz));
        let mut start = Vec::with_capacity(nx * nz + 1);
        start.push(0usize);
        let mut picked: Vec<usize> = Vec::new();
        for j in 0..nz {
            for i in 0..nx {
                let c = (j * step) * self.nx + i * step;
                let (a, b) = (
                    self.start.get(c).copied().unwrap_or(0),
                    self.start.get(c + 1).copied().unwrap_or(0),
                );
                picked.extend(a..b);
                start.push(picked.len());
            }
        }
        let y: Vec<f64> = picked.iter().filter_map(|&k| self.y.get(k).copied()).collect();
        Ok(Samples {
            pitch: to_pitch,
            min_x: self.min_x,
            min_z: self.min_z,
            nx,
            nz,
            header: self.header,
            start: start.into(),
            y: y.into(),
            y_f32: self.y_f32,
            normal: self.normal.pick(&picked),
            flags: picked.iter().filter_map(|&k| self.flags.get(k).copied()).collect(),
            hit: self
                .hit
                .as_ref()
                .filter(|h| !h.is_empty())
                .map(|h| picked.iter().filter_map(|&k| h.get(k).copied()).collect()),
        })
    }

    /// The run's grid for the walkability rules (its arrays shared).
    ///
    /// # Errors
    /// [`NavError::Shape`] when the column starts do not describe the surfaces.
    pub fn grid(&self) -> Result<Grid, NavError> {
        Grid::from_shared(
            self.nx,
            self.nz,
            Arc::clone(&self.start),
            Arc::clone(&self.y),
            self.pitch,
            self.min_x,
            self.min_z,
        )
    }

    fn column(&self, i: i64, j: i64) -> std::ops::Range<usize> {
        match (usize::try_from(i), usize::try_from(j)) {
            (Ok(i), Ok(j)) if i < self.nx && j < self.nz => {
                let c = j * self.nx + i;
                self.start.get(c).copied().unwrap_or(0)..self.start.get(c + 1).copied().unwrap_or(0)
            }
            _ => 0..0,
        }
    }

    fn yk(&self, k: usize) -> f64 {
        self.y.get(k).copied().unwrap_or(f64::NAN)
    }

    /// Every surface's column, in surface order.
    fn columns(&self) -> impl Iterator<Item = usize> + '_ {
        self.start
            .windows(2)
            .enumerate()
            .flat_map(|(c, w)| std::iter::repeat_n(c, w.get(1).copied().unwrap_or(0) - w.first().copied().unwrap_or(0)))
    }

    /// Every surface's (x, z): `min_x + (col % nx) * pitch`, `min_z + (col // nx) * pitch` (`surface_positions`).
    fn positions(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let nx = self.nx.max(1);
        self.columns().map(move |c| {
            (
                self.min_x + (c % nx) as f64 * self.pitch,
                self.min_z + (c / nx) as f64 * self.pitch,
            )
        })
    }
}

/// A walkability edit region (`navedits.region_mask`): a box (x0, y0, z0, x1, y1, z1), or a polygon in (x, z) with
/// a height range.
#[derive(Debug, Clone, PartialEq)]
pub enum Region {
    /// `{'box': [x0, y0, z0, x1, y1, z1]}`.
    Box([f64; 6]),
    /// `{'polygon': [[x, z], ...], 'y': [lo, hi]}`.
    Polygon {
        /// The height range.
        y: (f64, f64),
        /// The outline's vertices (x, z).
        poly: Vec<(f64, f64)>,
    },
}

/// matplotlib's `Path(poly).contains_points(points)` (radius 0, no transform: `_path.h`'s `point_in_path_impl`, the
/// crossing test on one subpath closed back to its first vertex): fewer than three vertices contain nothing.
#[must_use]
pub fn points_in_polygon(poly: &[(f64, f64)], tx: f64, ty: f64) -> bool {
    let Some(&(sx, sy)) = poly.first() else {
        return false;
    };
    if poly.len() < 3 || !(tx.is_finite() && ty.is_finite()) {
        return false;
    }
    let (mut vtx0, mut vty0) = (sx, sy);
    let mut yflag0 = vty0 >= ty;
    let mut inside = false;
    for &(vtx1, vty1) in poly.iter().skip(1).chain(std::iter::once(&(sx, sy))) {
        let yflag1 = vty1 >= ty;
        if yflag0 != yflag1 && (((vty1 - ty) * (vtx0 - vtx1) >= (vtx1 - tx) * (vty0 - vty1)) == yflag1) {
            inside = !inside;
        }
        yflag0 = yflag1;
        vtx0 = vtx1;
        vty0 = vty1;
    }
    inside
}

/// NumPy's `min` / `max` over a column (NaN when any value is).
fn np_extent(v: impl Iterator<Item = f64>) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for x in v {
        if x.is_nan() {
            return (f64::NAN, f64::NAN);
        }
        lo = lo.min(x);
        hi = hi.max(x);
    }
    (lo, hi)
}

/// `navedits.region_mask` for every surface of `s` (its (x, z) as `surface_positions` gives them, its height as
/// `float64`).
///
/// # Errors
/// [`NavError::Shape`] for a polygon without vertices (NumPy's `min` of an empty array raises).
pub fn region_mask(s: &Samples, region: &Region) -> Result<Vec<bool>, NavError> {
    region_mask_at(s.positions().zip(s.y.iter()).map(|((x, z), &y)| (x, z, y)), region)
}

/// `navedits.region_mask(x, z, y, region)` for points (x, z, y).
///
/// # Errors
/// As [`region_mask`].
pub fn region_mask_at(points: impl Iterator<Item = (f64, f64, f64)>, region: &Region) -> Result<Vec<bool>, NavError> {
    match region {
        Region::Box([x0, y0, z0, x1, y1, z1]) => Ok(points
            .map(|(x, z, y)| x >= *x0 && x <= *x1 && z >= *z0 && z <= *z1 && y >= *y0 && y <= *y1)
            .collect()),
        Region::Polygon { y: (lo, hi), poly } => {
            if poly.is_empty() {
                return Err(shape("a polygon without vertices"));
            }
            // a NaN vertex makes NumPy's extent NaN: no point passes the box test
            let (px0, px1) = np_extent(poly.iter().map(|p| p.0));
            let (pz0, pz1) = np_extent(poly.iter().map(|p| p.1));
            Ok(points
                .map(|(x, z, y)| {
                    y >= *lo
                        && y <= *hi
                        && x >= px0
                        && x <= px1
                        && z >= pz0
                        && z <= pz1
                        && points_in_polygon(poly, x, z)
                })
                .collect())
        }
    }
}

/// `navedits.edits_for`'s masks: for the walkability regions in order (`true`: walkable, `false`: unwalkable), the
/// surfaces forced walkable and unwalkable (later operations win).
///
/// # Errors
/// As [`region_mask`].
pub fn edit_masks(s: &Samples, ops: &[(bool, Region)]) -> Result<(Vec<bool>, Vec<bool>), NavError> {
    let n = s.len();
    let (mut fw, mut fu) = (vec![false; n], vec![false; n]);
    for (walk, region) in ops {
        let m = region_mask(s, region)?;
        let (on, off) = if *walk { (&mut fw, &mut fu) } else { (&mut fu, &mut fw) };
        for ((a, b), &v) in on.iter_mut().zip(off.iter_mut()).zip(&m) {
            if v {
                *a = true;
                *b = false;
            }
        }
    }
    Ok((fw, fu))
}

/// `navbuild.water_tops`: the water surface height over every column of an `nx` x `nz` grid at `pitch` from
/// (`min_x`, `min_z`) (-inf where there is no water); `vertices` and `triangles` the level's water mesh (indices as
/// NumPy takes them: a negative one counts from the end).
///
/// # Errors
/// [`NavError::Shape`] where the Python raises: an index past the vertices, a triangle whose bounds are not finite.
pub fn water_tops(
    nx: usize,
    nz: usize,
    pitch: f64,
    min_x: f64,
    min_z: f64,
    vertices: &[[f64; 3]],
    triangles: &[[i64; 3]],
) -> Result<Vec<f64>, NavError> {
    let mut top = vec![f64::NEG_INFINITY; nx * nz];
    let len = vertices.len() as i64;
    let vertex = |i: i64| {
        let at = if i < 0 { i + len } else { i };
        usize::try_from(at)
            .ok()
            .and_then(|a| vertices.get(a))
            .copied()
            .ok_or_else(|| shape("a water triangle's index past the vertices"))
    };
    for t in triangles {
        let [[ax, ay, az], [bx, by, bz], [cx, cy, cz]] = [vertex(t[0])?, vertex(t[1])?, vertex(t[2])?];
        let i0 = floor_int((py_min3(ax, bx, cx) - min_x) / pitch)?.max(0);
        let i1 = ceil_int((py_max3(ax, bx, cx) - min_x) / pitch)?.min(nx as i64 - 1);
        let j0 = floor_int((py_min3(az, bz, cz) - min_z) / pitch)?.max(0);
        let j1 = ceil_int((py_max3(az, bz, cz) - min_z) / pitch)?.min(nz as i64 - 1);
        if i0 > i1 || j0 > j1 {
            continue;
        }
        let d = (bz - cz) * (ax - cx) + (cx - bx) * (az - cz);
        if d.abs() < 1e-12 {
            continue;
        }
        for j in j0..=j1 {
            let z = min_z + j as f64 * pitch;
            for i in i0..=i1 {
                let x = min_x + i as f64 * pitch;
                let l1 = ((bz - cz) * (x - cx) + (cx - bx) * (z - cz)) / d;
                let l2 = ((cz - az) * (x - cx) + (ax - cx) * (z - cz)) / d;
                let l3 = 1.0 - l1 - l2;
                let h = if l1 >= -1e-9 && l2 >= -1e-9 && l3 >= -1e-9 {
                    l1 * ay + l2 * by + l3 * cy
                } else {
                    f64::NEG_INFINITY
                };
                if let Some(v) = top.get_mut(j as usize * nx + i as usize) {
                    *v = np_max(*v, h);
                }
            }
        }
    }
    Ok(top)
}

/// `navbuild.deep_water`'s answer from the columns' water tops: the surfaces more than `limit` m under the water.
#[must_use]
pub fn deep_water(s: &Samples, top: &[f64], limit: f64) -> Vec<bool> {
    s.columns()
        .zip(s.y.iter())
        .map(|(c, &y)| top.get(c).copied().unwrap_or(f64::NEG_INFINITY) - y > limit)
        .collect()
}

/// `navbuild.outside_bounds`: the surfaces whose column lies outside (min x, min z, max x, max z), edges inclusive
/// within 1 mm.
#[must_use]
pub fn outside_bounds(s: &Samples, b: [f64; 4]) -> Vec<bool> {
    let e = 1e-3;
    s.positions()
        .map(|(x, z)| x < b[0] - e || x > b[2] + e || z < b[1] - e || z > b[3] + e)
        .collect()
}

/// Object triangles as `walkable` takes them: in `float64`, and whether the array was `float32` (`on_objects`'
/// bounding-box filter compares in the array's dtype).
#[derive(Debug, Clone, Default)]
pub struct Tris {
    /// The triangles.
    pub tris: Vec<Tri>,
    /// The array was `float32`.
    pub from_f32: bool,
}

/// A second sampling run over the same grid (the high or narrow capsule): what `matched_flags` reads.
#[derive(Debug, Clone, Default)]
pub struct Capsule {
    /// Per column its first surface.
    pub start: Vec<usize>,
    /// Heights (`float64`).
    pub y: Vec<f64>,
    /// Capsule flags.
    pub flags: Vec<u8>,
}

impl Capsule {
    /// A run's file, read for what `matched_flags` reads (a version 3 file in blocks, without its normals and hit
    /// words; others whole).
    ///
    /// # Errors
    /// As [`Samples::load`].
    pub fn load(path: &std::path::Path) -> Result<Capsule, NavError> {
        if let Some(v) = stream_v3(path, false)? {
            return Ok(Capsule {
                start: v.start,
                y: v.y,
                flags: v.flags,
            });
        }
        Samples::load(path).map(|(s, _)| Capsule::of(&s))
    }

    /// The run's arrays `matched_flags` reads.
    #[must_use]
    pub fn of(s: &Samples) -> Capsule {
        Capsule {
            start: s.start.to_vec(),
            y: s.y.to_vec(),
            flags: s.flags.clone(),
        }
    }
}

/// A ladder (`ladders.py`): its box's low and high corners.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ladder {
    /// (x, y, z) low.
    pub lo: [f64; 3],
    /// (x, y, z) high.
    pub hi: [f64; 3],
}

/// `navwalk`'s module settings and the trigonometric constants of a call (computed in Python).
#[derive(Debug, Clone, Copy)]
#[allow(clippy::struct_excessive_bools)] // the module's switches
pub struct WalkSettings {
    /// `CHARACTERS_ABSENT`.
    pub characters_absent: bool,
    /// `HEADROOM_BAND`.
    pub headroom_band: Option<(f64, f64)>,
    /// `STEEP_TERRAIN`.
    pub steep_terrain: bool,
    /// `cos(radians(STEEP_TERRAIN_MAX))`.
    pub steep_max_cos: f64,
    /// `STEEP_TERRAIN_SPAN`.
    pub steep_span: f64,
    /// `LIP_WIDTH` (`None`: off - None or 0).
    pub lip_width: Option<i64>,
    /// `LIP_OBJECT_RISE`.
    pub lip_object_rise: Option<f64>,
    /// `bool(LIP_OBJECT_SLOPE_MAX)`.
    pub lip_object_slope_max: bool,
    /// `LIP_OBJECT_FLAT_ALL` (round NKS-deep D): every object lip, at any steepness, needs flat floor on both sides
    /// (`lip_flat_cos`); `false`: only those steeper than `steep_max_cos`.
    pub lip_object_flat_all: bool,
    /// `cos(radians(LIP_OBJECT_FLAT))`.
    pub lip_flat_cos: Option<f64>,
    /// `ROOF_PRUNE`.
    pub roof_prune: bool,
    /// `ROOF_RAISE`, `ROOF_SHARE`, `ROOF_NEAR`, `ROOF_NEAR_DY`.
    pub roof: RoofCfg,
    /// `DYNAMIC_ABSENT_HEADROOM`.
    pub dynamic_absent_headroom: bool,
    /// `DYNAMIC_FLOOR`.
    pub dynamic_floor: Option<f64>,
    /// `OBJECTIVE_FLOOR`.
    pub objective_floor: bool,
    /// `OBJECTIVE_REACH`.
    pub objective_reach: f64,
    /// `SECTOR`.
    pub sector: f64,
    /// `LADDER_REACH`.
    pub ladder_reach: f64,
    /// `LADDER_FOOT`.
    pub ladder_foot: (f64, f64),
    /// `LADDER_TOP`.
    pub ladder_top: (f64, f64),
    /// `TERRAIN_SHAPE_VTABLE`.
    pub terrain_vtable: u32,
    /// `CHARACTER_SHAPE_VTABLE`.
    pub character_vtable: u32,
    /// The call's `slope` (degrees).
    pub slope: f64,
    /// `cos(radians(slope)) - 0.01`.
    pub slope_cos: f64,
    /// `tan(radians(slope))`.
    pub slope_tan: f64,
}

impl WalkSettings {
    /// `navwalk`'s shipped settings for a call at `slope` degrees (`navwalk._settings(slope)`): the trigonometric
    /// constants from the C library's `cos` / `tan` (`black_box`: computed at run time, not folded by the compiler),
    /// as Python's `math` computes them.
    #[must_use]
    pub fn navkit(slope: f64) -> WalkSettings {
        let rad = |d: f64| std::hint::black_box(d).to_radians();
        WalkSettings {
            characters_absent: true,
            headroom_band: Some((0.5, 2.0)),
            steep_terrain: false,
            steep_max_cos: rad(55.0).cos(),
            steep_span: 0.8,
            lip_width: Some(2),
            lip_object_rise: Some(0.3),
            lip_object_slope_max: false,
            lip_object_flat_all: false,
            lip_flat_cos: Some(rad(15.0).cos()),
            roof_prune: true,
            roof: RoofCfg {
                raise: 2.0,
                share: 0.5,
                near: 2.0,
                near_dy: 2.5,
            },
            dynamic_absent_headroom: true,
            dynamic_floor: Some(0.3),
            objective_floor: true,
            objective_reach: 1.5,
            sector: 100.0,
            ladder_reach: 1.5,
            ladder_foot: (-1.0, 1.0),
            ladder_top: (-1.5, 0.5),
            terrain_vtable: 0x0145_4D14,
            character_vtable: 0x0148_3378,
            slope,
            slope_cos: rad(slope).cos() - 0.01,
            slope_tan: rad(slope).tan(),
        }
    }
}

/// `walkable`'s arguments (`navwalk.walkable`).
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_excessive_bools)] // the call's switches
pub struct WalkInputs {
    /// `forbid_tris`.
    pub forbid: Option<Tris>,
    /// `dynamic_tris`.
    pub dynamic: Option<Tris>,
    /// `stair_tris`.
    pub stair_tris: Option<Tris>,
    /// `exclude_tris`.
    pub exclude: Option<Tris>,
    /// `exclude_footprint_tris`.
    pub exclude_foot: Option<Tris>,
    /// `high_capsule`.
    pub high: Option<Capsule>,
    /// `narrow_capsule`.
    pub narrow: Option<Capsule>,
    /// `step`.
    pub step: f64,
    /// `terrain_seeds_only`.
    pub terrain_seeds_only: bool,
    /// `stairs`.
    pub stairs: bool,
    /// `raised_bars`.
    pub raised_bars: bool,
    /// `forbid_tol`.
    pub forbid_tol: f64,
    /// `extra_seeds`.
    pub extra_seeds: Vec<P3>,
    /// `force_walkable`.
    pub force_walkable: Option<Vec<bool>>,
    /// `force_unwalkable`.
    pub force_unwalkable: Option<Vec<bool>>,
    /// `unseeds`.
    pub unseeds: Vec<P3>,
    /// `soldier_points`.
    pub soldier_points: Vec<P3>,
    /// `ladders`.
    pub ladders: Vec<Ladder>,
    /// `anchors`.
    pub anchors: Vec<P3>,
    /// `objective_points`.
    pub objective_points: Vec<P3>,
}

/// What a `walkable` call adds to `navwalk`'s statistics dicts.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WalkStats {
    /// `STEEP_STATS`: (key, increment) in the order the Python updated them.
    pub steep: Vec<(&'static str, usize)>,
    /// `roof_drop`'s counts for `ROOF_STATS` (`None`: nothing to add).
    pub roof: Option<(usize, Option<(usize, usize)>)>,
    /// `OBJECTIVE_STATS`: (points, surfaces), when a point was counted.
    pub objective: Option<(usize, usize)>,
}

/// `walkable`'s answer (`return_gradients`, `return_stairs`, `grid_out`): the kept surfaces, and the floods' level,
/// which holds the gradients and stair flags ([`Walked::gradients`]).
#[derive(Debug, Clone)]
pub struct Walked {
    /// Kept surfaces.
    pub keep: Vec<bool>,
    /// The floods' level (keep = the rules' walkable surfaces, terrain none, link step 0.6).
    pub level: Level,
    /// The statistics.
    pub stats: WalkStats,
    /// Per ladder, its foot and top among the kept surfaces (`ladder_ends(s, keep, keep, lad)`: the stage's ladder
    /// links).
    pub ladder_pairs: Vec<(Option<usize>, Option<usize>)>,
}

impl Walked {
    /// The step rule's gradients gx, gz (`float32` from the normals, `float64` once a rule replaced some) and the
    /// stair flags.
    #[must_use]
    pub fn gradients(&self) -> (&Floats, &Floats, &[bool]) {
        self.level.gradients()
    }
}

/// The narrow-capsule rule's opposite sides: left / right, front / back and the two diagonals.
const OPPOSITE: [((i64, i64), (i64, i64)); 4] = [
    ((1, 0), (-1, 0)),
    ((0, 1), (0, -1)),
    ((1, 1), (-1, -1)),
    ((1, -1), (-1, 1)),
];

fn count(v: &[bool]) -> usize {
    v.iter().filter(|&&b| b).count()
}

fn or_into(dst: &mut [bool], src: impl IntoIterator<Item = bool>) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d |= s;
    }
}

fn and_not(dst: &mut [bool], src: &[bool]) {
    for (d, &s) in dst.iter_mut().zip(src) {
        *d &= !s;
    }
}

/// `np.where(mask, a, b)` with `a` `float64`: `float64` (b's values widened exactly), made in b's place when b is
/// `float64` already.
fn where_f64(mask: &[bool], a: &[f64], b: Floats) -> Floats {
    let mut out: Vec<f64> = match b {
        Floats::F64(v) => v,
        Floats::F32(v) => v.into_iter().map(f64::from).collect(),
    };
    for ((o, &m), &x) in out.iter_mut().zip(mask).zip(a) {
        if m {
            *o = x;
        }
    }
    Floats::F64(out)
}

/// `navwalk.on_objects(s, tris, tol)`: the bounding-box filter (in the triangles' dtype: NumPy casts the Python
/// floats to `float32` for a `float32` array), then the rule.
fn on_objects(s: &Samples, grid: &Grid, tris: Option<&Tris>, tol: f64) -> Result<Vec<bool>, NavError> {
    let n = s.len();
    let Some(t) = tris.filter(|t| !t.tris.is_empty()) else {
        return Ok(vec![false; n]);
    };
    let f = t.from_f32;
    let (x0, x1) = (weak(s.min_x - 2.0, f), weak(s.min_x + s.nx as f64 * s.pitch + 2.0, f));
    let (z0, z1) = (weak(s.min_z - 2.0, f), weak(s.min_z + s.nz as f64 * s.pitch + 2.0, f));
    let kept: Vec<Tri> = t
        .tris
        .iter()
        .filter(|tri| {
            let (xl, xh) = np_extent(tri.iter().map(|v| v[0]));
            let (zl, zh) = np_extent(tri.iter().map(|v| v[2]));
            xh >= x0 && xl <= x1 && zh >= z0 && zl <= z1
        })
        .copied()
        .collect();
    if kept.is_empty() {
        return Ok(vec![false; n]);
    }
    grid.on_objects(&kept, tol)
}

fn near_objects(
    grid: &Grid,
    n: usize,
    tris: Option<&Tris>,
    grow: f64,
    below: f64,
    above: f64,
) -> Result<Vec<bool>, NavError> {
    match tris.filter(|t| !t.tris.is_empty()) {
        Some(t) => grid.near_objects(&t.tris, grow, below, above),
        None => Ok(vec![false; n]),
    }
}

/// `navwalk.near_points(s, points, grow, below, above)` (`walkable` takes 0.9, 0.5, 0.5).
///
/// # Errors
/// [`NavError::Shape`] where the Python raises (a point whose column index is not a number).
pub fn near_points(s: &Samples, points: &[P3], grow: f64, below: f64, above: f64) -> Result<Vec<bool>, NavError> {
    let mut out = vec![false; s.len()];
    let r = (grow / s.pitch + 1e-9).floor();
    if !r.is_finite() {
        return Err(shape("near_points: a reach that is not a number"));
    }
    let r = r as i64;
    for &(px, py, pz) in points {
        let (i0, j0) = (
            round_int((px - s.min_x) / s.pitch)?,
            round_int((pz - s.min_z) / s.pitch)?,
        );
        for dj in (-r - 1)..=(r + 1) {
            for di in (-r - 1)..=(r + 1) {
                let (a, b) = (i0 + di, j0 + dj);
                if !(0 <= a && a < s.nx as i64 && 0 <= b && b < s.nz as i64) {
                    continue;
                }
                if pymath::hypot(s.min_x + a as f64 * s.pitch - px, s.min_z + b as f64 * s.pitch - pz) > grow + 1e-9 {
                    continue;
                }
                for q in s.column(a, b) {
                    let y = s.yk(q);
                    if py - below <= y
                        && y <= py + above
                        && let Some(o) = out.get_mut(q)
                    {
                        *o = true;
                    }
                }
            }
        }
    }
    Ok(out)
}

/// `navwalk.ladder_ends(s, ok, keep, lad)`: (foot surface, top surface).
///
/// # Errors
/// [`NavError::Shape`] where the Python raises (a bound that is not a number).
pub fn ladder_ends(
    s: &Samples,
    ok: &[bool],
    keep: &[bool],
    lad: &Ladder,
    set: &WalkSettings,
) -> Result<(Option<usize>, Option<usize>), NavError> {
    let (lo, hi, reach) = (lad.lo, lad.hi, set.ladder_reach);
    let i0 = floor_int((lo[0] - reach - s.min_x) / s.pitch)?;
    let i1 = ceil_int((hi[0] + reach - s.min_x) / s.pitch)?;
    let j0 = floor_int((lo[2] - reach - s.min_z) / s.pitch)?;
    let j1 = ceil_int((hi[2] + reach - s.min_z) / s.pitch)?;
    let mut out = [None, None];
    let bands = [
        (lo[1] + set.ladder_foot.0, lo[1] + set.ladder_foot.1),
        (hi[1] + set.ladder_top.0, hi[1] + set.ladder_top.1),
    ];
    for (slot, (y0, y1)) in out.iter_mut().zip(bands) {
        let (y0, y1) = (weak(y0, s.y_f32), weak(y1, s.y_f32));
        let mut best: Option<((bool, f64), usize)> = None;
        for j in j0.max(0)..=j1.min(s.nz as i64 - 1) {
            for i in i0.max(0)..=i1.min(s.nx as i64 - 1) {
                let (x, z) = (s.min_x + i as f64 * s.pitch, s.min_z + j as f64 * s.pitch);
                let d = pymath::hypot(py_max3(lo[0] - x, 0.0, x - hi[0]), py_max3(lo[2] - z, 0.0, z - hi[2]));
                if d > reach {
                    continue;
                }
                for k in s.column(i, j) {
                    let y = s.yk(k);
                    if ok.get(k).copied().unwrap_or(false) && y0 <= y && y <= y1 {
                        let key = (!keep.get(k).copied().unwrap_or(false), d);
                        // Python's tuple order: False before True, then the distance
                        let less = |a: (bool, f64), b: (bool, f64)| (!a.0 && b.0) || (a.0 == b.0 && a.1 < b.1);
                        if best.is_none_or(|(bk, _)| less(key, bk)) {
                            best = Some((key, k));
                        }
                    }
                }
            }
        }
        *slot = best.map(|b| b.1);
    }
    let [foot, top] = out;
    Ok((foot, top))
}

/// The surfaces of column (i, j) within `tol` of `yref` (`walkable`'s `same_layer`).
fn same_layer(s: &Samples, i: i64, j: i64, yref: f64, tol: f64) -> impl Iterator<Item = usize> + '_ {
    s.column(i, j).filter(move |&n| (s.yk(n) - yref).abs() <= tol)
}

fn cell(s: &Samples, k: usize, col_of: &[usize]) -> (i64, i64) {
    let c = col_of.get(k).copied().unwrap_or(0);
    ((c % s.nx.max(1)) as i64, (c / s.nx.max(1)) as i64)
}

/// `np.median` of values (sorted; the mean of the middle two for an even count).
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n == 0 {
        return f64::NAN;
    }
    if n % 2 == 1 {
        v.get(n / 2).copied().unwrap_or(f64::NAN)
    } else {
        let (a, b) = (
            v.get(n / 2 - 1).copied().unwrap_or(f64::NAN),
            v.get(n / 2).copied().unwrap_or(f64::NAN),
        );
        // NumPy's mean of the two: their sum, divided by 2 (not f64::midpoint's halves)
        #[expect(clippy::manual_midpoint, reason = "np.median's (a + b) / 2")]
        let m = (a + b) / 2.0;
        m
    }
}

/// `navwalk.objective_floor`: the surfaces to add for the AI order positions lying on a forbidden object, and the
/// statistics (points, surfaces) when a point was counted.
#[allow(clippy::too_many_lines)] // the reference's steps in one function
fn objective_floor(
    s: &Samples,
    onf: &[bool],
    ok: &[bool],
    character: &[bool],
    slope_ok: &[bool],
    points: &[P3],
    reach: f64,
) -> Result<Added, NavError> {
    let flag = |v: &[bool], k: usize| v.get(k).copied().unwrap_or(false);
    let mut add = vec![false; s.len()];
    let mut stats: Option<(usize, usize)> = None;
    let r = ceil_int(reach / s.pitch)?;
    for &(px, py, pz) in points {
        let (i0, j0) = (
            round_int((px - s.min_x) / s.pitch)?,
            round_int((pz - s.min_z) / s.pitch)?,
        );
        let mut cols: Vec<std::ops::Range<usize>> = Vec::new();
        for dj in -r..=r {
            for di in -r..=r {
                let (i, j) = (i0 + di, j0 + dj);
                if 0 <= i
                    && i < s.nx as i64
                    && 0 <= j
                    && j < s.nz as i64
                    && pymath::hypot(di as f64, dj as f64) * s.pitch <= reach
                {
                    cols.push(s.column(i, j));
                }
            }
        }
        if !cols
            .iter()
            .flat_map(Clone::clone)
            .any(|n| flag(onf, n) && -0.5 <= s.yk(n) - py && s.yk(n) - py <= 2.5)
        {
            continue;
        }
        let under_top = |n: usize, ns: &std::ops::Range<usize>| {
            ns.clone()
                .any(|m| flag(onf, m) && 0.3 <= s.yk(m) - s.yk(n) && s.yk(m) - s.yk(n) <= 2.0)
        };
        let under: Vec<f64> = cols
            .iter()
            .flat_map(|ns| ns.clone().map(move |n| (n, ns)))
            .filter(|&(n, ns)| flag(slope_ok, n) && (s.yk(n) - py).abs() <= 1.5 && under_top(n, ns))
            .map(|(n, _)| s.yk(n))
            .collect();
        if under.is_empty() {
            continue;
        }
        // np.unique(np.round(under / 0.1).astype(int), return_counts=True)
        let mut bins: Vec<i64> = under
            .iter()
            .map(|&v| {
                let r = (v / 0.1).round_ties_even();
                if r.is_finite() { r as i64 } else { i64::MIN }
            })
            .collect();
        bins.sort_unstable();
        let mut b: Vec<(i64, usize)> = Vec::new();
        for v in bins {
            match b.last_mut() {
                Some(last) if last.0 == v => last.1 += 1,
                _ => b.push((v, 1)),
            }
        }
        let most = b.iter().map(|x| x.1).max().unwrap_or(0);
        let lim = if 0.25 * most as f64 > 2.0 {
            0.25 * most as f64
        } else {
            2.0
        };
        let mut layers: Vec<f64> = b
            .iter()
            .filter(|x| x.1 as f64 >= lim)
            .map(|x| x.0 as f64 * 0.1)
            .collect();
        if layers.is_empty() {
            layers = b.iter().map(|x| x.0 as f64 * 0.1).collect();
        }
        // np.argmin: the first of the least
        let mut nearest = f64::NAN;
        let mut best = f64::INFINITY;
        for &l in &layers {
            let d = (l - py).abs();
            if nearest.is_nan() || d < best {
                if d.is_nan() {
                    nearest = l;
                    break;
                }
                best = d;
                nearest = l;
            }
        }
        let mut near: Vec<f64> = under.iter().copied().filter(|v| (v - nearest).abs() <= 0.2).collect();
        let floor = median(&mut near);
        let mut n_add = 0usize;
        for ns in &cols {
            for n in ns.clone() {
                if !flag(ok, n)
                    && !flag(character, n)
                    && flag(slope_ok, n)
                    && (s.yk(n) - floor).abs() <= 0.3
                    && (!flag(onf, n) || under_top(n, ns))
                {
                    if let Some(a) = add.get_mut(n) {
                        *a = true;
                    }
                    n_add += 1;
                }
            }
        }
        let st = stats.get_or_insert((0, 0));
        st.0 += 1;
        st.1 += n_add;
    }
    Ok((add, stats))
}

/// `navwalk.walkable(s, ...)` with `return_gradients`, `return_stairs` and `grid_out`: the kept surfaces by rules
/// 1-4 and the structure rules, the hand edits, the seeds, the floods, the ladders, the unseeds and the roof islands
/// (the Python's steps in its order; see the module's notes on floats).
///
/// # Errors
/// [`NavError::Shape`] where the Python raises (arrays of other lengths, a point or bound that is not a number,
/// triangles the rules refuse).
#[allow(clippy::too_many_lines)]
pub fn walkable(s: &Samples, inp: &WalkInputs, set: &WalkSettings) -> Result<Walked, NavError> {
    s.check()?;
    let n = s.len();
    let grid = s.grid()?;
    let (_, _, col_of) = grid.shared();
    let flag = |v: &[bool], k: usize| v.get(k).copied().unwrap_or(false);
    let flag2: Vec<bool> = s.flags.iter().map(|&f| f & 2 == 0).collect();
    let mut stats = WalkStats::default();
    let (mut gx, mut gz) = s.normal.gradients();
    let mut stair = vec![false; n];
    let terrain: Vec<bool> = match &s.hit {
        Some(h) => h.iter().map(|w| w[0] == set.terrain_vtable).collect(),
        None => vec![true; n],
    };
    let mut clear: Vec<bool> = s.flags.iter().map(|&f| f & 3 == 0).collect();
    let character: Vec<bool> = match (&s.hit, set.characters_absent) {
        (Some(h), true) => h.iter().map(|w| w[0] == set.character_vtable).collect(),
        _ => vec![false; n],
    };
    if character.iter().any(|&c| c) {
        let near = grid.near_characters(&character, 0.9, 2.2, 0.3)?;
        or_into(&mut clear, near.iter().zip(&flag2).map(|(&a, &b)| a && b));
    }
    if !inp.soldier_points.is_empty() && set.characters_absent {
        let near = near_points(s, &inp.soldier_points, 0.9, 0.5, 0.5)?;
        or_into(&mut clear, near.iter().zip(&flag2).map(|(&a, &b)| a && b));
    }
    for t in [inp.dynamic.as_ref(), inp.exclude.as_ref()] {
        if t.is_some_and(|t| !t.tris.is_empty()) {
            let near = near_objects(&grid, n, t, 0.5, 2.2, 0.3)?;
            or_into(&mut clear, near.iter().zip(&flag2).map(|(&a, &b)| a && b));
        }
    }
    let slope_ok: Vec<bool> = (0..n).map(|k| s.normal.y_ge(k, set.slope_cos)).collect();
    let mut ok: Vec<bool> = (0..n)
        .map(|k| flag(&clear, k) && flag(&slope_ok, k) && !flag(&character, k))
        .collect();
    if set.steep_terrain {
        let steep: Vec<bool> = (0..n)
            .map(|k| {
                flag(&terrain, k)
                    && flag(&clear, k)
                    && !flag(&character, k)
                    && !flag(&ok, k)
                    && s.normal.y_ge(k, set.steep_max_cos)
            })
            .collect();
        if steep.iter().any(|&v| v) {
            let r = round_int(set.steep_span / s.pitch)?.max(1);
            let t = 1.2 * r as f64 * s.pitch;
            let tol = if t > 0.7 { t } else { 0.7 };
            let (gs_t, tgx, tgz) = grid.stair_slope(&steep, r, tol)?;
            let accept: Vec<bool> = (0..n)
                .map(|k| flag(&steep, k) && gs_t.get(k).is_some_and(|&g| g <= set.slope))
                .collect();
            or_into(&mut ok, accept.iter().copied());
            gx = where_f64(&accept, &tgx, gx);
            gz = where_f64(&accept, &tgz, gz);
            stats.steep.push(("accepted", count(&accept)));
            stats.steep.push(("candidates", count(&steep)));
        }
    }
    if let Some(width) = set.lip_width
        && s.pitch < 0.5
    {
        let steep_c: Vec<bool> = (0..n)
            .map(|k| flag(&clear, k) && !flag(&character, k) && !flag(&ok, k) && s.normal.y_ge(k, set.steep_max_cos))
            .collect();
        let cand: Vec<bool> = (0..n).map(|k| flag(&terrain, k) && flag(&steep_c, k)).collect();
        let mut lips = grid.narrow_lips(&ok, &cand, set.slope_tan, width, LipTests::default())?;
        if let Some(rise) = set.lip_object_rise {
            let obj_c: Vec<bool> = (0..n)
                .map(|k| !flag(&terrain, k) && flag(&clear, k) && !flag(&character, k) && !flag(&ok, k))
                .collect();
            let c1: Vec<bool> = (0..n).map(|k| flag(&obj_c, k) && flag(&steep_c, k)).collect();
            let ny1 = if set.lip_object_flat_all { Some(s.normal.ys()) } else { None };
            let more = grid.narrow_lips(
                &ok,
                &c1,
                set.slope_tan,
                width,
                LipTests {
                    rise: Some(rise),
                    flat: ny1.as_ref().and_then(|ny| set.lip_flat_cos.map(|c| (ny, c))),
                },
            )?;
            or_into(&mut lips, more);
            if !set.lip_object_slope_max {
                let c2: Vec<bool> = (0..n).map(|k| flag(&obj_c, k) && !flag(&steep_c, k)).collect();
                let ny = s.normal.ys();
                let more = grid.narrow_lips(
                    &ok,
                    &c2,
                    set.slope_tan,
                    width,
                    LipTests {
                        rise: Some(rise),
                        flat: set.lip_flat_cos.map(|c| (&ny, c)),
                    },
                )?;
                or_into(&mut lips, more);
            }
        }
        if lips.iter().any(|&v| v) {
            let (_gs, lgx, lgz) = grid.stair_slope(&lips, 1, 0.7)?;
            or_into(&mut ok, lips.iter().copied());
            gx = where_f64(&lips, &lgx, gx);
            gz = where_f64(&lips, &lgz, gz);
        }
        stats.steep.push(("lips", count(&lips)));
    }
    let on_stair: Vec<bool> = match inp.stair_tris.as_ref().filter(|t| !t.tris.is_empty()) {
        Some(t) => {
            let near = grid.near_objects(&t.tris, 0.4, 0.6, 0.6)?;
            (0..n)
                .map(|k| flag(&near, k) && !flag(&terrain, k) && flag(&flag2, k))
                .collect()
        }
        None => vec![false; n],
    };
    if inp.stairs {
        let cand: Vec<bool> = (0..n).map(|k| !flag(&terrain, k) && flag(&flag2, k)).collect();
        let (gs, hgx, hgz) = grid.stair_slope(&cand, 1, 0.7)?;
        let g = |k: usize| gs.get(k).copied().unwrap_or(f64::NAN);
        stair = (0..n)
            .map(|k| flag(&cand, k) && g(k) >= 12.0 && g(k) <= set.slope)
            .collect();
        let stair_obj: Vec<bool> = (0..n).map(|k| flag(&on_stair, k) && g(k) <= 45.0).collect();
        or_into(&mut ok, stair_obj.iter().copied());
        or_into(&mut stair, (0..n).map(|k| flag(&stair_obj, k) && g(k) >= 12.0));
        drop((gs, stair_obj));
        // on stairs the treads are flat: the incline from the neighbour heights (the arrays freed as they are used)
        gx = where_f64(&stair, &hgx, gx);
        drop(hgx);
        gz = where_f64(&stair, &hgz, gz);
        drop(hgz);
        let hi: Vec<u8> = match &inp.high {
            Some(h) => matched_flags(s.nx * s.nz, &s.start, &s.y, &h.start, &h.y, &h.flags)?,
            None => s.flags.clone(),
        };
        let landing = grid.landing(&stair, &cand)?;
        or_into(
            &mut ok,
            (0..n).map(|k| {
                (flag(&stair, k) || (flag(&landing, k) && flag(&slope_ok, k))) && hi.get(k).is_some_and(|&f| f & 3 == 0)
            }),
        );
    }
    let onf = on_objects(s, &grid, inp.forbid.as_ref(), inp.forbid_tol)?;
    if inp.raised_bars {
        let mut bar = vec![false; n];
        for k in (0..n).filter(|&k| flag(&onf, k) && !flag(&terrain, k)) {
            let (i, j) = cell(s, k, &col_of);
            for dj in -1..=1 {
                for di in -1..=1 {
                    for m in s.column(i + di, j + dj) {
                        let dy = s.yk(k) - s.yk(m);
                        if (0.7..=1.9).contains(&dy)
                            && flag(&flag2, m)
                            && let Some(b) = bar.get_mut(m)
                        {
                            *b = true;
                        }
                    }
                }
            }
        }
        or_into(
            &mut ok,
            (0..n).map(|k| flag(&bar, k) && flag(&slope_ok, k) && !flag(&onf, k)),
        );
    }
    if let Some(nar) = &inp.narrow {
        let nf = matched_flags(s.nx * s.nz, &s.start, &s.y, &nar.start, &nar.y, &nar.flags)?;
        let base = ok.clone();
        for k in 0..n {
            if flag(&base, k)
                || nf.get(k).is_none_or(|&f| f & 3 != 0)
                || !flag(&slope_ok, k)
                || !flag(&flag2, k)
                || flag(&onf, k)
            {
                continue;
            }
            let (i, j) = cell(s, k, &col_of);
            let yk = s.yk(k);
            for (d1, d2) in OPPOSITE {
                if same_layer(s, i + d1.0, j + d1.1, yk, 0.7).any(|m| flag(&base, m))
                    && same_layer(s, i + d2.0, j + d2.1, yk, 0.7).any(|m| flag(&base, m))
                {
                    if let Some(o) = ok.get_mut(k) {
                        *o = true;
                    }
                    break;
                }
            }
        }
    }
    and_not(&mut ok, &onf);
    let dynamic = inp.dynamic.as_ref().filter(|t| !t.tris.is_empty());
    let mut ondyn = vec![false; n];
    if dynamic.is_some() {
        ondyn = on_objects(s, &grid, dynamic, 0.2)?;
        let graze = grid.near_objects(dynamic.map_or(&[][..], |t| &t.tris), 0.1, -0.05, 0.0)?;
        or_into(&mut ondyn, (0..n).map(|k| flag(&graze, k) && !flag(&slope_ok, k)));
    }
    and_not(&mut ok, &ondyn);
    if inp.exclude.as_ref().is_some_and(|t| !t.tris.is_empty()) {
        let on = on_objects(s, &grid, inp.exclude.as_ref(), 0.2)?;
        and_not(&mut ok, &on);
    }
    if inp.exclude_foot.as_ref().is_some_and(|t| !t.tris.is_empty()) {
        let near = near_objects(&grid, n, inp.exclude_foot.as_ref(), 0.3, 1.0, 0.5)?;
        and_not(&mut ok, &near);
    }
    if let Some((lo, hi)) = set.headroom_band {
        let ignore: Vec<bool> = if set.dynamic_absent_headroom {
            (0..n).map(|k| flag(&character, k) || flag(&ondyn, k)).collect()
        } else {
            character.clone()
        };
        let h = grid.headroom(Some(&ignore))?;
        for (o, &v) in ok.iter_mut().zip(&h) {
            if v >= lo && v < hi {
                *o = false;
            }
        }
    }
    if let Some(df) = set.dynamic_floor
        && ondyn.iter().any(|&v| v)
    {
        let base = ok.clone();
        for k in (0..n).filter(|&k| flag(&ondyn, k) && flag(&flag2, k) && flag(&slope_ok, k) && !flag(&onf, k)) {
            let (i, j) = cell(s, k, &col_of);
            let yk = s.yk(k);
            let c = col_of.get(k).copied().unwrap_or(0);
            let (a, b) = (
                s.start.get(c).copied().unwrap_or(0),
                s.start.get(c + 1).copied().unwrap_or(0),
            );
            if (a..b).any(|m| !flag(&ondyn, m) && 0.0 < yk - s.yk(m) && yk - s.yk(m) <= df) {
                continue;
            }
            let found = (-1..=1).any(|di: i64| {
                (-1..=1).any(|dj: i64| {
                    (di != 0 || dj != 0) && same_layer(s, i + di, j + dj, yk, df).any(|m| flag(&base, m))
                })
            });
            if found && let Some(o) = ok.get_mut(k) {
                *o = true;
            }
        }
    }
    and_not(&mut ok, &character);
    if set.objective_floor && !inp.objective_points.is_empty() && onf.iter().any(|&v| v) {
        let (add, st) = objective_floor(
            s,
            &onf,
            &ok,
            &character,
            &slope_ok,
            &inp.objective_points,
            set.objective_reach,
        )?;
        or_into(&mut ok, add);
        stats.objective = st;
    }
    if let Some(fw) = &inp.force_walkable {
        if fw.len() != n {
            return Err(shape("force_walkable: not a flag per surface"));
        }
        or_into(&mut ok, fw.iter().copied());
    }
    if let Some(fu) = &inp.force_unwalkable {
        if fu.len() != n {
            return Err(shape("force_unwalkable: not a flag per surface"));
        }
        and_not(&mut ok, fu);
    }
    // the seeds on the sector-edge sample lines
    // round((x) % SECTOR, 1) in (1.6, 98.4): Python's equality, exact
    #[expect(clippy::float_cmp, reason = "the reference's `in (1.6, 98.4)`")]
    let edge_line = |v: f64| {
        py_mod(v, set.sector)
            .map(|r| pymath::round_to(r, 1))
            .is_some_and(|r| r == 1.6 || r == 98.4)
    };
    let per_i: Vec<bool> = (0..s.nx).map(|i| edge_line(s.min_x + i as f64 * s.pitch)).collect();
    let per_j: Vec<bool> = (0..s.nz).map(|j| edge_line(s.min_z + j as f64 * s.pitch)).collect();
    let mut keep = vec![false; n];
    let mut queue: Vec<usize> = Vec::new();
    for k in 0..n {
        let seedable = flag(&ok, k) && (!inp.terrain_seeds_only || flag(&terrain, k));
        if !seedable {
            continue;
        }
        let (i, j) = cell(s, k, &col_of);
        if per_i.get(i as usize).copied().unwrap_or(false) || per_j.get(j as usize).copied().unwrap_or(false) {
            if let Some(v) = keep.get_mut(k) {
                *v = true;
            }
            queue.push(k);
        }
    }
    for &p in &inp.extra_seeds {
        if let Some(k) = grid.column_pick(p, &ok, -1.5, 0.5)?
            && !flag(&keep, k)
        {
            if let Some(v) = keep.get_mut(k) {
                *v = true;
            }
            queue.push(k);
        }
    }
    let level = Level::on_grid(&grid, ok.clone(), gx, gz, stair, vec![false; n], s.pitch, inp.step, 0.6)?;
    level.flood(&mut keep, queue);
    if !inp.ladders.is_empty() {
        let mut done = vec![false; inp.ladders.len()];
        let mut changed = true;
        while changed {
            changed = false;
            for (li, lad) in inp.ladders.iter().enumerate() {
                if done.get(li).copied().unwrap_or(true) {
                    continue;
                }
                let (foot, top) = ladder_ends(s, &ok, &keep, lad, set)?;
                let (kf, kt) = (
                    foot.is_some_and(|f| flag(&keep, f)),
                    top.is_some_and(|t| flag(&keep, t)),
                );
                let far = if kf && !kt {
                    top
                } else if kt && !kf {
                    foot
                } else {
                    None
                };
                let Some(far) = far else {
                    if kf
                        && kt
                        && let Some(d) = done.get_mut(li)
                    {
                        *d = true;
                    }
                    continue;
                };
                if let Some(v) = keep.get_mut(far) {
                    *v = true;
                }
                level.flood_bfs(&mut keep, [far]);
                if let Some(d) = done.get_mut(li) {
                    *d = true;
                }
                changed = true;
            }
        }
    }
    for &(sx, sy, sz) in &inp.unseeds {
        let (i, j) = (
            round_int((sx - s.min_x) / s.pitch)?,
            round_int((sz - s.min_z) / s.pitch)?,
        );
        if !(0 <= i && i < s.nx as i64 && 0 <= j && j < s.nz as i64) {
            continue;
        }
        let mut first: Option<(usize, f64)> = None;
        for m in s.column(i, j) {
            let d = s.yk(m) - sy;
            if flag(&keep, m) && (-1.5..=0.5).contains(&d) && first.is_none_or(|(_, b)| d.abs() < b) {
                first = Some((m, d.abs()));
            }
        }
        let Some((first, _)) = first else { continue };
        let mut one = vec![false; n];
        if let Some(v) = one.get_mut(first) {
            *v = true;
        }
        level.with_keep(keep.clone())?.flood(&mut one, [first]);
        and_not(&mut keep, &one);
    }
    if set.roof_prune {
        let mut anchors: Vec<usize> = Vec::new();
        for &p in &inp.anchors {
            if let Some(k) = grid.column_pick(p, &keep, -2.5, 2.5)? {
                anchors.push(k);
            }
        }
        for lad in &inp.ladders {
            let (foot, top) = ladder_ends(s, &ok, &keep, lad, set)?;
            anchors.extend([foot, top].into_iter().flatten().filter(|&e| flag(&keep, e)));
        }
        let (comp, count_) = level.with_keep(keep.clone())?.islands();
        let rg = RoofGrid {
            nx: s.nx,
            nz: s.nz,
            start: &s.start,
            y: &s.y,
            min_x: s.min_x,
            min_z: s.min_z,
            pitch: s.pitch,
        };
        let got = graph::roof_drop(&rg, &keep, &terrain, &comp, count_, &anchors, set.roof)
            .ok_or_else(|| shape("roof islands: arrays that do not fit the grid"))?;
        and_not(&mut keep, &got.out);
        stats.roof = got.stats;
    }
    // the stage's ladder links: each ladder's kept foot and top (ladder_ends(s, keep, keep, lad))
    let ladder_pairs = inp
        .ladders
        .iter()
        .map(|lad| ladder_ends(s, &keep, &keep, lad, set))
        .collect::<Result<Vec<_>, NavError>>()?;
    Ok(Walked {
        keep,
        level,
        stats,
        ladder_pairs,
    })
}

/// `navbuild.stage`'s walkability inputs beyond `walkable`'s: the sample file, the high and narrow runs, the
/// stage's pitch, the hand edits' regions, the water and the reference extent.
#[derive(Debug, Clone, Default)]
pub struct StageInputs {
    /// The sample file.
    pub samples: std::path::PathBuf,
    /// The high-capsule run's file.
    pub high: Option<std::path::PathBuf>,
    /// The narrow-capsule run's file.
    pub narrow: Option<std::path::PathBuf>,
    /// The stage's pitch (`cfg.pitch`): a run of another pitch is made coarse to `coarse_pitch`.
    pub pitch: f64,
    /// `navwalk.PITCH`, the pitch `coarse` makes.
    pub coarse_pitch: f64,
    /// The side file's walkability regions in order (`edits` given: `Some`, even when empty).
    pub edits: Option<Vec<(bool, Region)>>,
    /// The water mesh (vertices, triangles) and the depth limit, when both are given.
    pub water: Option<(WaterMesh, f64)>,
    /// The reference extent (min x, min z, max x, max z), when the build clips to it.
    pub bounds: Option<[f64; 4]>,
    /// The height field whose surface fills the run's sampling holes (`Switches::terrain_fill`), or `None`.
    pub terrain_fill: Option<Arc<bc2_formats::terrain::Heightfield>>,
}

/// What the stage logs before its walkability (`hand edits: ...`, `water: ...`, `reference extent ...`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StageCounts {
    /// Surfaces forced walkable / unwalkable by the hand edits (when edits were given).
    pub edits: Option<(usize, usize)>,
    /// Surfaces left out under water.
    pub water: Option<usize>,
    /// Surfaces left out outside the reference extent.
    pub outside: Option<usize>,
    /// Terrain surfaces filled into sampling holes (`Switches::terrain_fill`), when any were.
    pub terrain_filled: Option<usize>,
}

/// `navbuild.stage`'s walkability: the run loaded (made coarse when its pitch is not the stage's), the hand edits'
/// masks, the water and extent rules folded into the forced flags as the stage folds them, then [`walkable`] with
/// `inp` (its `force_walkable` / `force_unwalkable` are set here). Returns the run, the counts and the answer.
///
/// # Errors
/// [`NavError::Shape`] where the Python raises (the caller lets `navsample.load` explain a file it does not read).
pub fn stage(
    si: &StageInputs,
    mut inp: WalkInputs,
    set: &WalkSettings,
) -> Result<(Samples, StageCounts, Walked), NavError> {
    let (f, _) = Samples::load(&si.samples)?;
    let s = if (f.pitch - si.pitch).abs() > 1e-3 {
        f.coarse(si.coarse_pitch)?
    } else {
        f
    };
    let mut counts = StageCounts::default();
    let s = match si.terrain_fill.as_deref().and_then(|hf| crate::terrain_fill::fill(&s, hf, set.terrain_vtable)) {
        Some((filled, added)) => {
            counts.terrain_filled = Some(added);
            filled
        }
        None => s,
    };
    let (mut fw, mut fu): (Option<Vec<bool>>, Option<Vec<bool>>) = (None, None);
    if let Some(ops) = &si.edits {
        let (w, u) = edit_masks(&s, ops)?;
        let (nw, nu) = (count(&w), count(&u));
        counts.edits = Some((nw, nu));
        fw = (nw > 0).then_some(w);
        fu = (nu > 0).then_some(u);
    }
    if let Some(((verts, tris), limit)) = &si.water {
        let top = water_tops(s.nx, s.nz, s.pitch, s.min_x, s.min_z, verts, tris)?;
        let mut deep = deep_water(&s, &top, *limit);
        if deep.iter().any(|&v| v) {
            if let Some(w) = &fw {
                and_not(&mut deep, w);
            }
            fu = Some(match fu.take() {
                None => deep.clone(),
                Some(mut u) => {
                    or_into(&mut u, deep.iter().copied());
                    u
                }
            });
        }
        counts.water = Some(count(&deep));
    }
    if let Some(b) = si.bounds {
        let mut outside = outside_bounds(&s, b);
        if let Some(w) = &fw {
            and_not(&mut outside, w);
        }
        counts.outside = Some(count(&outside));
        fu = Some(match fu.take() {
            None => outside,
            Some(mut u) => {
                or_into(&mut u, outside);
                u
            }
        });
    }
    let load_capsule = |p: &Option<std::path::PathBuf>| -> Result<Option<Capsule>, NavError> {
        p.as_ref().map(|p| Capsule::load(p)).transpose()
    };
    inp.high = load_capsule(&si.high)?;
    inp.narrow = load_capsule(&si.narrow)?;
    inp.force_walkable = fw;
    inp.force_unwalkable = fu;
    let walked = walkable(&s, &inp, set)?;
    Ok((s, counts, walked))
}

#[cfg(test)]
#[path = "walk_tests.rs"]
mod tests;
