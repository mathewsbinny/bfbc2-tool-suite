//! `bfbc2-bf1943-import/emitteratlas.py` (round F2b): one emitter atlas for a level whose emitters were authored
//! against several atlases - the build ([`build`]: cells, deduplication, packing, composition, the emitters'
//! fields re-pointed and written), its proof ([`prove`]) and [`same_but_uv`]. The reference's module text says
//! what each step does; this is that code step for step, on the same integers, and its floats as Python and NumPy
//! compute them:
//!
//! - the probe search compares the windows' exact variances (integers); NumPy's `std` of a full 8 x 8 window is
//!   exact (the reference's own argument, E3r) and a cut window's is within 1e-13 of the exact value, so a choice is
//!   refused only when two different windows' variances are equal or closer than 1e-9 (their order would then be
//!   NumPy's rounding);
//! - a correlation (`np.corrcoef`) is the exact rational value of integer data; a decision on it (`>= SAME`, a value
//!   rounded to 4 places) is refused when the exact value lies within 1e-9 of the boundary;
//! - Python's `round`, `min` / `max`, floor division and slicing rules where the reference uses them.
//!
//! [`Atlas::Error`] carries the reference's `EmitterAtlasError` message; [`Atlas::Refused`] is returned where the
//! reference raises anything else, prints a value with Python's `repr`, or decides on a float this port cannot
//! decide the same way for certain - the caller then runs the reference.

#![allow(clippy::many_single_char_names)]
// the reference's names (t, c, k, q, s ...), kept to read side by side
// Indexing here is into this module's own tables - the two channels, cells by the indices it hands out, the BC5
// palettes (3-bit indices into eight values), the packing grid's summed-area table sized from the grid it walks -;
// values read from the inputs go through `get`.
#![allow(clippy::indexing_slicing)]
// The reference's numbers are Python's doubles: every conversion and comparison below is the reference's own
// (integers from images and atlases far below 2^53; floats compared as Python compares them).
#![allow(clippy::cast_precision_loss, clippy::float_cmp)]

use std::collections::HashMap;

use bc2_formats::dbx::{self, Body, CONTAINER, DbxFile, Element, TEXT};

use crate::pyfmt::{OrderedMap, round_even, round_to};

/// The 8-bit format and BC5.
pub const L8: u32 = 10;
/// BC5.
pub const BC5: u32 = 20;
/// Cells on an 8-unit grid.
pub const GRID: i64 = 8;
/// Levels 0..K copied exactly.
pub const K: usize = 3;
/// Levels 0..D read only an image's own texels.
pub const D: usize = 1;
/// Tried after the `like` atlas's size.
pub const ALPHA_SIZES: [(i64, i64); 2] = [(4096, 2048), (4096, 4096)];
/// Tried after the `like` atlas's size.
pub const NORMAL_SIZES: [(i64, i64); 1] = [(1024, 1024)];
/// The alpha fields.
pub const UV: [&str; 4] = ["TextureOriginU", "TextureOriginV", "AnimationStepU", "AnimationStepV"];
/// The normal-map fields.
pub const NM: [&str; 4] = [
    "NmTextureOriginU",
    "NmTextureOriginV",
    "NmAnimationStepU",
    "NmAnimationStepV",
];
/// `ALIGN`.
pub const ALIGN: [i64; 5] = [128, 64, 32, 16, GRID];
/// `SAME` (BC5 copies counted as one image).
pub const SAME: f64 = 0.995;
/// `PASS` (the proof's bar).
pub const PASS: f64 = 0.99;
/// The texture resource's header size.
pub const HEADER: usize = 92;
/// How close to a boundary an exact value may lie before the decision is left to the reference.
const MARGIN: f64 = 1e-9;

/// A step's outcome other than its result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Atlas {
    /// The reference's `EmitterAtlasError` with its message.
    Error(String),
    /// Left to the reference.
    Refused,
}

/// A result of this module.
pub type R<T> = Result<T, Atlas>;

fn err<T>(m: impl Into<String>) -> R<T> {
    Err(Atlas::Error(m.into()))
}

/// The channels: 0 alpha, 1 normals.
pub const CHANNELS: [&str; 2] = ["alpha", "normals"];

fn fields(ch: usize) -> [&'static str; 4] {
    if ch == 0 { UV } else { NM }
}

// ------------------------------------------------------------------------------------------------ textures

/// One level: (h, w, depth) units, row-major (texels of format 10: depth 1; BC5 blocks: depth 16).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Level {
    /// Rows.
    pub h: usize,
    /// Columns.
    pub w: usize,
    /// Bytes per unit.
    pub d: usize,
    /// The units.
    pub data: Vec<u8>,
}

impl Level {
    fn zeros(h: usize, w: usize, d: usize) -> Self {
        Self {
            h,
            w,
            d,
            data: vec![0; h * w * d],
        }
    }

    fn unit(&self, y: usize, x: usize) -> &[u8] {
        let o = (y * self.w + x) * self.d;
        self.data.get(o..o + self.d).unwrap_or_default()
    }

    /// `lv[y0:y1, x0:x1]` with Python's slicing rules (negative indices count from the end, clamped).
    fn slice(&self, y0: i64, y1: i64, x0: i64, x1: i64) -> Level {
        let (ya, yb) = py_range(y0, y1, self.h);
        let (xa, xb) = py_range(x0, x1, self.w);
        let mut out = Level::zeros(yb - ya, xb - xa, self.d);
        for y in ya..yb {
            let src = self
                .data
                .get((y * self.w + xa) * self.d..(y * self.w + xb) * self.d)
                .unwrap_or_default();
            let o = (y - ya) * out.w * self.d;
            if let Some(dst) = out.data.get_mut(o..o + src.len()) {
                dst.copy_from_slice(src);
            }
        }
        out
    }
}

/// Python's `range(len)[start:stop]` bounds.
fn py_range(start: i64, stop: i64, len: usize) -> (usize, usize) {
    let n = len as i64;
    let norm = |v: i64| -> i64 { if v < 0 { (v + n).max(0) } else { v.min(n) } };
    let (a, b) = (norm(start), norm(stop));
    (a as usize, b.max(a) as usize)
}

/// Python's `a // b` (b > 0).
fn fdiv(a: i64, b: i64) -> i64 {
    a.div_euclid(b)
}

/// Python's `-(-a // b)`: the ceiling (b > 0).
fn cdiv(a: i64, b: i64) -> i64 {
    -((-a).div_euclid(b))
}

/// A PC `ITexture` as units per level.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tex {
    /// Format: 10 or 20.
    pub fmt: u32,
    /// Flags.
    pub flags: u32,
    /// Width.
    pub w: i64,
    /// Height.
    pub h: i64,
    /// The levels.
    pub levels: Vec<Level>,
}

impl Tex {
    /// Units per texel side.
    #[must_use]
    pub fn unit(&self) -> i64 {
        if self.fmt == BC5 { 4 } else { 1 }
    }

    fn level(&self, m: usize) -> R<&Level> {
        self.levels.get(m).ok_or(Atlas::Refused)
    }
}

fn u32_at(b: &[u8], o: usize) -> R<u32> {
    let s: [u8; 4] = b
        .get(o..o + 4)
        .ok_or(Atlas::Refused)?
        .try_into()
        .map_err(|_| Atlas::Refused)?;
    Ok(u32::from_le_bytes(s))
}

/// `read_tex(res)`.
///
/// # Errors
/// The reference's error for another format; refused where it raises otherwise.
pub fn read_tex(res: &[u8]) -> R<Tex> {
    let mut hd = [0u32; 8];
    for (i, x) in hd.iter_mut().enumerate() {
        *x = u32_at(res, 4 * i)?;
    }
    let [_ver, typ, fmt, flags, w, h, _depth, mips] = hd;
    if (fmt != L8 && fmt != BC5) || typ != 0 {
        return err(format!(
            "an emitter atlas of format {fmt}, type {typ} (8-bit 10 or BC5 20 expected)"
        ));
    }
    let mut sizes = [0u32; 15];
    for (i, x) in sizes.iter_mut().enumerate() {
        *x = u32_at(res, 32 + 4 * i)?;
    }
    if mips > 15 {
        return Err(Atlas::Refused);
    }
    let mut out = Vec::with_capacity(mips as usize);
    let mut pos = HEADER;
    for (m, &size) in sizes.iter().enumerate().take(mips as usize) {
        let lw = (w.checked_shr(m as u32).unwrap_or(0)).max(1) as usize;
        let lh = (h.checked_shr(m as u32).unwrap_or(0)).max(1) as usize;
        let end = (pos + size as usize).min(res.len());
        let data = res.get(pos.min(res.len())..end).unwrap_or_default();
        pos += size as usize;
        if fmt == L8 {
            let pitch = lw.max(data.len() / lh);
            if data.len() < pitch * lh {
                return Err(Atlas::Refused);
            }
            let mut lv = Level::zeros(lh, lw, 1);
            for y in 0..lh {
                let row = data.get(y * pitch..y * pitch + lw).ok_or(Atlas::Refused)?;
                if let Some(dst) = lv.data.get_mut(y * lw..y * lw + lw) {
                    dst.copy_from_slice(row);
                }
            }
            out.push(lv);
        } else {
            let (bw, bh) = (lw.div_ceil(4).max(1), lh.div_ceil(4).max(1));
            let d = data.get(..bw * bh * 16).ok_or(Atlas::Refused)?;
            out.push(Level {
                h: bh,
                w: bw,
                d: 16,
                data: d.to_vec(),
            });
        }
    }
    Ok(Tex {
        fmt,
        flags,
        w: i64::from(w),
        h: i64::from(h),
        levels: out,
    })
}

/// `texwrite.l8_bytes(level)`: rows padded to 4 bytes when narrower.
fn l8_bytes(lv: &Level) -> Vec<u8> {
    if lv.w >= 4 {
        return lv.data.clone();
    }
    let mut out = vec![0u8; lv.h * 4];
    for y in 0..lv.h {
        if let (Some(dst), Some(src)) = (out.get_mut(y * 4..y * 4 + lv.w), lv.data.get(y * lv.w..y * lv.w + lv.w)) {
            dst.copy_from_slice(src);
        }
    }
    out
}

/// `write_tex(t)`.
///
/// # Errors
/// Refused for more than 15 levels (the reference's `TexWriteError`).
pub fn write_tex(t: &Tex) -> R<Vec<u8>> {
    if t.levels.len() > 15 {
        return Err(Atlas::Refused);
    }
    let data: Vec<Vec<u8>> = t
        .levels
        .iter()
        .map(|lv| if t.fmt == L8 { l8_bytes(lv) } else { lv.data.clone() })
        .collect();
    let mut out = Vec::new();
    let w32 = |v: i64| u32::try_from(v).map_err(|_| Atlas::Refused);
    for v in [3, 0, t.fmt, t.flags, w32(t.w)?, w32(t.h)?, 0, data.len() as u32] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for i in 0..15 {
        let n = data.get(i).map_or(0, Vec::len) as u32;
        out.extend_from_slice(&n.to_le_bytes());
    }
    for d in &data {
        out.extend_from_slice(d);
    }
    Ok(out)
}

/// One decoded level: (h, w, channels) integers.
#[derive(Debug, Clone, Default)]
pub struct Texels {
    h: usize,
    w: usize,
    c: usize,
    v: Vec<i32>,
}

impl Texels {
    /// `a[y0:y1, x0:x1]` (Python's slicing) as a flat list in C order, with its shape.
    fn crop(&self, y0: i64, y1: i64, x0: i64, x1: i64) -> (usize, usize, Vec<i32>) {
        let (ya, yb) = py_range(y0, y1, self.h);
        let (xa, xb) = py_range(x0, x1, self.w);
        let mut out = Vec::with_capacity((yb - ya) * (xb - xa) * self.c);
        for y in ya..yb {
            out.extend_from_slice(
                self.v
                    .get((y * self.w + xa) * self.c..(y * self.w + xb) * self.c)
                    .unwrap_or_default(),
            );
        }
        (yb - ya, xb - xa, out)
    }
}

/// `texture.decode`'s BC5 alpha block (eight values, 3-bit indices).
fn bc5_channel(a: &[u8]) -> [i32; 16] {
    let a0 = i32::from(a.first().copied().unwrap_or(0));
    let a1 = i32::from(a.get(1).copied().unwrap_or(0));
    let mut pal = [0i32; 8];
    pal[0] = a0;
    pal[1] = a1;
    let big = a0 > a1;
    for k in 1..7 {
        pal[k + 1] = ((7 - k as i32) * a0 + k as i32 * a1) / 7;
    }
    if !big {
        for k in 1..5 {
            pal[k + 1] = ((5 - k as i32) * a0 + k as i32 * a1) / 5;
        }
        pal[6] = 0;
        pal[7] = 255;
    }
    let mut bits = 0u64;
    for k in 0..6 {
        bits |= u64::from(a.get(2 + k).copied().unwrap_or(0)) << (8 * k);
    }
    let mut out = [0i32; 16];
    for (k, o) in out.iter_mut().enumerate() {
        *o = pal[((bits >> (3 * k)) & 7) as usize];
    }
    out
}

/// `texels(t, m)`: level m decoded.
fn decode(t: &Tex, m: usize) -> R<Texels> {
    let lv = t.level(m)?;
    if t.fmt == L8 {
        return Ok(Texels {
            h: lv.h,
            w: lv.w,
            c: 1,
            v: lv.data.iter().map(|&b| i32::from(b)).collect(),
        });
    }
    let lw = (t.w >> m).max(1) as usize;
    let lh = (t.h >> m).max(1) as usize;
    let (bw, bh) = (lv.w, lv.h);
    let (fw, fh) = (bw * 4, bh * 4);
    let mut img = vec![0i32; fh * fw * 2];
    for by in 0..bh {
        for bx in 0..bw {
            let b = lv.unit(by, bx);
            let r = bc5_channel(b.get(..8).unwrap_or_default());
            let g = bc5_channel(b.get(8..).unwrap_or_default());
            for k in 0..16 {
                let (y, x) = (by * 4 + k / 4, bx * 4 + k % 4);
                let o = (y * fw + x) * 2;
                if let Some(px) = img.get_mut(o..o + 2) {
                    px.copy_from_slice(&[r[k], g[k]]);
                }
            }
        }
    }
    let (h, w) = (lh.min(fh), lw.min(fw));
    let mut v = Vec::with_capacity(h * w * 2);
    for y in 0..h {
        v.extend_from_slice(img.get(y * fw * 2..(y * fw + w) * 2).unwrap_or_default());
    }
    Ok(Texels { h, w, c: 2, v })
}

/// Decoded levels, kept (a texture is not changed once read).
#[derive(Default)]
struct Cache {
    map: HashMap<(usize, usize), Texels>,
}

impl Cache {
    fn texels(&mut self, id: usize, t: &Tex, m: usize) -> R<&Texels> {
        if let std::collections::hash_map::Entry::Vacant(e) = self.map.entry((id, m)) {
            e.insert(decode(t, m)?);
        }
        self.map.get(&(id, m)).ok_or(Atlas::Refused)
    }
}

// ------------------------------------------------------------------------------------------------ exact statistics

/// The decision `corrcoef(a, b) >= bound` (or the value to round) of two equally long integer lists: the exact
/// correlation; `None` (refused) when a list is constant is handled by the caller.
#[derive(Debug, Clone, Copy)]
struct Corr {
    sxy: i128,
    sxx: i128,
    syy: i128,
}

impl Corr {
    fn of(a: &[i32], b: &[i32]) -> Self {
        let n = a.len() as i128;
        let (mut sa, mut sb, mut saa, mut sbb, mut sab) = (0i128, 0i128, 0i128, 0i128, 0i128);
        for (&x, &y) in a.iter().zip(b) {
            let (x, y) = (i128::from(x), i128::from(y));
            sa += x;
            sb += y;
            saa += x * x;
            sbb += y * y;
            sab += x * y;
        }
        Self {
            sxy: n * sab - sa * sb,
            sxx: n * saa - sa * sa,
            syy: n * sbb - sb * sb,
        }
    }

    /// The correlation as a double (within a few ulps of the exact value).
    fn value(self) -> f64 {
        let den = ((self.sxx as f64) * (self.syy as f64)).sqrt();
        (self.sxy as f64 / den).clamp(-1.0, 1.0)
    }
}

/// `x >= bound` for an exact value known to within a few ulps, `None` when it is too close to call.
fn at_least(x: f64, bound: f64) -> Option<bool> {
    ((x - bound).abs() > MARGIN).then_some(x >= bound)
}

/// `round(x, 4)` of NumPy's value of an exact quantity known to within a few ulps, `None` near a rounding boundary.
fn round4(x: f64) -> Option<f64> {
    let s = x * 1e4;
    let frac = s - s.floor();
    ((frac - 0.5).abs() > MARGIN * 1e4).then(|| round_to(x, 4))
}

/// A window's exact variance numerator `n * sum(x^2) - sum(x)^2` (the variance is that / n^2).
fn var_num(v: &[i64]) -> i128 {
    let n = v.len() as i128;
    let (mut s, mut ss) = (0i128, 0i128);
    for &x in v {
        s += i128::from(x);
        ss += i128::from(x) * i128::from(x);
    }
    n * ss - s * s
}

// ------------------------------------------------------------------------------------------------ emitters

/// The paths of a `TextureInfo`'s fields in the owned tree: instance index, `TextureInfo` index, field index.
type Path = (usize, usize, usize);

/// One `TextureInfo` (`Info`).
#[derive(Debug, Clone)]
pub struct Info {
    /// The instance's guid.
    pub instance: String,
    /// The `TextureInfo`'s name.
    pub name: String,
    elements: Vec<(&'static str, Path)>,
    /// The fields' values (float32 as stored), UV then NM.
    pub values: Vec<(&'static str, f64)>,
    /// `AnimationFrameCount`.
    pub frames: i64,
    /// `AnimationFrameColumnCount`.
    pub columns: i64,
    /// `EnableAnimation`.
    pub animated: bool,
    /// `Texture`'s ref.
    pub texture: Option<String>,
    /// `Normalmap`'s ref.
    pub normalmap: Option<String>,
}

impl Info {
    fn v(&self, k: &str) -> f64 {
        self.values.iter().find(|(n, _)| *n == k).map_or(0.0, |x| x.1)
    }
}

fn attr<'f>(f: &'f DbxFile, e: &Element, name: &str) -> Option<&'f str> {
    e.attributes
        .iter()
        .find(|(k, _)| f.str(*k) == name)
        .map(|(_, v)| f.str(*v))
}

fn tag<'f>(f: &'f DbxFile, e: &Element) -> &'f str {
    f.str(e.tag)
}

/// Python's `str()` of an optional attribute: `None` prints as `None`.
fn py_str(v: Option<&str>) -> String {
    v.map_or_else(|| "None".to_owned(), str::to_owned)
}

/// `_f32(e)`.
fn f32_of(f: &DbxFile, e: &Element) -> R<f64> {
    let (size, vals) = dbx::numbers(e.raw()).map_err(|_| Atlas::Refused)?;
    if size != 4 || vals.len() != 1 {
        return err(format!("{}: not one 4-byte number", py_str(attr(f, e, "name"))));
    }
    let v: [u8; 4] = vals
        .first()
        .copied()
        .unwrap_or_default()
        .try_into()
        .map_err(|_| Atlas::Refused)?;
    Ok(f64::from(f32::from_be_bytes(v)))
}

/// `dbx.value(e)`.
fn value(f: &DbxFile, e: &Element) -> R<String> {
    if e.kind == TEXT {
        if let Body::Text(t) = e.body {
            return Ok(f.str(t).to_owned());
        }
        return Err(Atlas::Refused);
    }
    dbx::value(f, e).map_err(|_| Atlas::Refused)
}

/// `_int(e)`: `int(float(dbx.value(e)))`.
fn int_of(f: &DbxFile, e: &Element) -> R<i64> {
    let t = value(f, e)?;
    let s = t.trim();
    if s.contains('_') || s.is_empty() {
        return Err(Atlas::Refused);
    }
    let v: f64 = s.parse().map_err(|_| Atlas::Refused)?;
    if !v.is_finite() || v.abs() > 4e18 {
        return Err(Atlas::Refused);
    }
    Ok(v.trunc() as i64)
}

fn instances(f: &DbxFile) -> R<Vec<(usize, &Element)>> {
    let root = f.roots.first().ok_or(Atlas::Refused)?;
    let Body::Container(kids) = &root.body else {
        return Err(Atlas::Refused);
    };
    Ok(kids
        .iter()
        .enumerate()
        .filter(|(_, e)| tag(f, e) == "instance")
        .collect())
}

/// `infos(f)`.
///
/// # Errors
/// The reference's error for a `TextureInfo` without its fields; refused where it raises otherwise.
pub fn infos(f: &DbxFile) -> R<Vec<Info>> {
    let mut out = Vec::new();
    for (ii, inst) in instances(f)? {
        for (ci, c) in inst.children().iter().enumerate() {
            if c.kind != CONTAINER || !attr(f, c, "name").unwrap_or("").starts_with("TextureInfo") {
                continue;
            }
            // {name or '': element}, the last of a name winning
            let mut el: HashMap<&str, usize> = HashMap::new();
            for (k, x) in c.children().iter().enumerate() {
                el.insert(attr(f, x, "name").unwrap_or(""), k);
            }
            let need = UV
                .iter()
                .chain(&NM)
                .chain(&["AnimationFrameCount", "AnimationFrameColumnCount"]);
            if !need.clone().all(|k| el.contains_key(k)) {
                return err(format!(
                    "{} {}: a TextureInfo without its fields",
                    py_str(attr(f, inst, "type")),
                    py_str(attr(f, c, "name"))
                ));
            }
            let get = |k: &str| -> R<&Element> {
                let i = *el.get(k).ok_or(Atlas::Refused)?;
                c.children().get(i).ok_or(Atlas::Refused)
            };
            let mut elements = Vec::new();
            let mut values = Vec::new();
            for k in UV.iter().chain(&NM) {
                let i = *el.get(k).ok_or(Atlas::Refused)?;
                elements.push((*k, (ii, ci, i)));
            }
            for k in UV.iter().chain(&NM) {
                values.push((*k, f32_of(f, get(k)?)?));
            }
            let frames = int_of(f, get("AnimationFrameCount")?)?;
            let columns = int_of(f, get("AnimationFrameColumnCount")?)?;
            let animated = match el.get("EnableAnimation") {
                Some(_) => value(f, get("EnableAnimation")?)? == "true",
                None => false,
            };
            let refs = |k: &str| -> R<Option<String>> {
                match el.get(k) {
                    Some(_) => Ok(attr(f, get(k)?, "ref").map(str::to_owned)),
                    None => Ok(None),
                }
            };
            out.push(Info {
                instance: attr(f, inst, "guid").unwrap_or("").to_owned(),
                name: attr(f, c, "name").unwrap_or("").to_owned(),
                elements,
                values,
                frames,
                columns,
                animated,
                texture: refs("Texture")?,
                normalmap: refs("Normalmap")?,
            });
        }
    }
    Ok(out)
}

/// `spawns_frames(f)`.
///
/// # Errors
/// Refused where the reference raises.
pub fn spawns_frames(f: &DbxFile) -> R<bool> {
    for (_, inst) in instances(f)? {
        if attr(f, inst, "type").unwrap_or("").ends_with("SpawnAnimationFrameData") {
            let en = inst.children().iter().find(|c| attr(f, c, "name") == Some("Enable"));
            match en {
                None => return Ok(true),
                Some(e) => {
                    if value(f, e)? == "true" {
                        return Ok(true);
                    }
                }
            }
        }
    }
    Ok(false)
}

/// `grid(info, spawn)`.
fn grid(info: &Info, spawn: bool) -> (i64, i64) {
    if !(info.animated || spawn) || info.frames <= 1 {
        return (1, 1);
    }
    let cols = info.columns.max(1);
    (cols.min(info.frames), cdiv(info.frames, cols))
}

/// `drawn(info, channel)`.
fn drawn(info: &Info, ch: usize) -> bool {
    let reference = if ch == 0 { &info.texture } else { &info.normalmap };
    if let Some(r) = reference {
        let l = r.to_lowercase();
        if l.is_empty() || l == "null" {
            return false;
        }
    }
    let [ku, kv, ksu, ksv] = fields(ch);
    let (o_u, o_v, s_u, s_v) = (info.v(ku), info.v(kv), info.v(ksu), info.v(ksv));
    let whole = (s_u == 0.0 && s_v == 0.0) || (s_u == 1.0 && s_v == 1.0);
    !(o_u == 0.0 && o_v == 0.0 && whole)
}

/// Python's `sorted((a, b))` of two floats (NaN refused by the caller).
fn sorted2(a: f64, b: f64) -> (f64, f64) {
    if b < a { (b, a) } else { (a, b) }
}

/// A texel rectangle (x0, y0, x1, y1).
pub type Rect = [f64; 4];

/// `rect(info, channel, spawn, w, h)`.
fn rect(info: &Info, ch: usize, spawn: bool, w: i64, h: i64) -> R<Rect> {
    let [ku, kv, ksu, ksv] = fields(ch);
    let (o_u, o_v, s_u, s_v) = (info.v(ku), info.v(kv), info.v(ksu), info.v(ksv));
    let (cols, rows) = grid(info, spawn);
    let (wf, hf) = (w as f64, h as f64);
    let xs = sorted2(o_u * wf, (o_u + s_u * cols as f64) * wf);
    let ys = sorted2(o_v * hf, (o_v + s_v * rows as f64) * hf);
    let r = [xs.0, ys.0, xs.1, ys.1];
    if r.iter().any(|v| !v.is_finite() || v.abs() > 1e12) {
        return Err(Atlas::Refused);
    }
    Ok(r)
}

// ------------------------------------------------------------------------------------------------ cells

/// A cell: a rectangle of a source atlas in level-0 units.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    /// The source's index.
    pub source: usize,
    /// The source key's place among the keys sorted (the reference sorts cells by the key).
    pub rank: usize,
    /// Left.
    pub x: i64,
    /// Top.
    pub y: i64,
    /// Width.
    pub w: i64,
    /// Height.
    pub h: i64,
    /// Deduplicated: drawn from this cell (an index into the channel's cells) at (dx, dy).
    pub into: Option<usize>,
    /// The offset in `into`.
    pub dx: i64,
    /// The offset in `into`.
    pub dy: i64,
    /// Kept: its place in the merged atlas.
    pub at: Option<(i64, i64)>,
    /// The units its uses read at levels 0..D.
    pub core: [i64; 4],
}

/// `unit_rect(r, unit, uw, uh)`.
fn unit_rect(r: &Rect, unit: i64, uw: i64, uh: i64) -> R<[i64; 4]> {
    let u = unit as f64;
    let x0 = ((fdiv((r[0] / u).floor() as i64, GRID) - 1) * GRID).max(0);
    let y0 = ((fdiv((r[1] / u).floor() as i64, GRID) - 1) * GRID).max(0);
    let x1 = ((cdiv((r[2] / u).ceil() as i64, GRID) + 1) * GRID).min(uw);
    let y1 = ((cdiv((r[3] / u).ceil() as i64, GRID) + 1) * GRID).min(uh);
    if x1 <= x0 || y1 <= y0 {
        return Err(Atlas::Refused); // the reference's message prints the rectangle's floats
    }
    Ok([x0, y0, x1, y1])
}

/// `core_rect(r, unit, uw, uh)`.
fn core_rect(r: &Rect, unit: i64, uw: i64, uh: i64) -> [i64; 4] {
    let u = unit as f64;
    [
        (((r[0] - 2.0) / u).floor() as i64).max(0),
        (((r[1] - 2.0) / u).floor() as i64).max(0),
        (((r[2] + 2.0) / u).ceil() as i64).min(uw),
        (((r[3] + 2.0) / u).ceil() as i64).min(uh),
    ]
}

/// `merge(cells)`.
fn merge(cells: &[Cell]) -> Vec<Cell> {
    let mut seen: OrderedMap<(usize, i64, i64, i64, i64), Cell> = OrderedMap::new();
    for c in cells {
        let k = (c.source, c.x, c.y, c.w, c.h);
        if let Some(m) = seen.get_mut(&k) {
            let (a, b) = (m.core, c.core);
            m.core = [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])];
        } else {
            seen.insert(
                k,
                Cell {
                    into: None,
                    dx: 0,
                    dy: 0,
                    at: None,
                    ..c.clone()
                },
            );
        }
    }
    seen.iter().map(|(_, c)| c.clone()).collect()
}

/// `_crop(t, m, x, y, w, h)`.
fn crop(t: &Tex, m: usize, x: i64, y: i64, w: i64, h: i64) -> R<Level> {
    let s = 1i64 << m;
    Ok(t.level(m)?
        .slice(fdiv(y, s), fdiv(y + h, s), fdiv(x, s), fdiv(x + w, s)))
}

/// `_box(t, m, b)`.
fn bx(t: &Tex, m: usize, b: &[i64; 4]) -> R<Level> {
    let s = 1i64 << m;
    Ok(t.level(m)?
        .slice(fdiv(b[1], s), cdiv(b[3], s), fdiv(b[0], s), cdiv(b[2], s)))
}

/// `same_image(a, b, m, ba, bb)`; `None`: refused.
fn same_image(
    a: (usize, &Tex),
    b: (usize, &Tex),
    m: usize,
    ba: &[i64; 4],
    bb: &[i64; 4],
    cache: &mut Cache,
) -> R<bool> {
    let (x, y) = (bx(a.1, m, ba)?, bx(b.1, m, bb)?);
    if (x.h, x.w, x.d) != (y.h, y.w, y.d) {
        return Ok(false);
    }
    if x.data == y.data {
        return Ok(true);
    }
    if a.1.fmt == L8 {
        return Ok(false);
    }
    let (s, u) = (1i64 << m, a.1.unit());
    let crop_of = |t: &Texels, q: &[i64; 4]| {
        t.crop(
            fdiv(q[1] * u, s),
            cdiv(q[3] * u, s),
            fdiv(q[0] * u, s),
            cdiv(q[2] * u, s),
        )
    };
    let (ha, wa, da) = crop_of(cache.texels(a.0, a.1, m)?, ba);
    let (hb, wb, db) = crop_of(cache.texels(b.0, b.1, m)?, bb);
    if (ha, wa) != (hb, wb) {
        return Ok(false);
    }
    let constant = |v: &[i32]| v.iter().all(|&z| Some(&z) == v.first());
    if constant(&da) || constant(&db) {
        return Ok(false);
    }
    at_least(Corr::of(&da, &db).value(), SAME).ok_or(Atlas::Refused)
}

/// The probe: (py, px) of the core's most varied 8 x 8 window at even offsets, as the reference's scan chooses it.
fn probe_at(c0: &Level, qw: i64, qh: i64) -> R<(usize, usize)> {
    let full = qh >= GRID && qw >= GRID && c0.h as i64 >= qh && c0.w as i64 >= qw;
    let mut best: Option<(i128, Vec<i64>, usize, usize)> = None;
    let mut yy = 0i64;
    while yy <= (qh - GRID).max(0) {
        let mut xx = 0i64;
        while xx <= (qw - GRID).max(0) {
            let win = c0.slice(yy, yy + GRID, xx, xx + GRID);
            let vals: Vec<i64> = win.data.iter().map(|&b| i64::from(b)).collect();
            let n = vals.len() as i128;
            let v = var_num(&vals);
            match &best {
                None => best = Some((v, vals, yy as usize, xx as usize)),
                Some((bv, bvals, _, _)) => {
                    let (bn, cur) = (bvals.len() as i128, v);
                    // variances v / n^2 against bv / bn^2 (the windows of one core have the same size)
                    if bn != n {
                        return Err(Atlas::Refused);
                    }
                    if full {
                        if cur > *bv {
                            best = Some((v, vals, yy as usize, xx as usize));
                        }
                    } else if cur == *bv {
                        if vals != *bvals {
                            return Err(Atlas::Refused);
                        }
                    } else {
                        let (a, b) = (cur as f64, *bv as f64);
                        if (a - b).abs() <= MARGIN * a.abs().max(b.abs()) {
                            return Err(Atlas::Refused);
                        }
                        if cur > *bv {
                            best = Some((v, vals, yy as usize, xx as usize));
                        }
                    }
                }
            }
            xx += 2;
        }
        yy += 2;
    }
    Ok(best.map_or((0, 0), |b| (b.2, b.3)))
}

/// `dedupe(cells, tex)`: the kept cells' indices (largest first); the others' `into` set.
fn dedupe(cells: &mut [Cell], tex: &[Option<Tex>], ids: &[usize], cache: &mut Cache) -> R<Vec<usize>> {
    let mut order: Vec<usize> = (0..cells.len()).collect();
    order.sort_by(|&a, &b| {
        let (ca, cb) = (&cells[a], &cells[b]);
        (-ca.w * ca.h, ca.rank, ca.y, ca.x).cmp(&(-cb.w * cb.h, cb.rank, cb.y, cb.x))
    });
    let mut kept: Vec<usize> = Vec::new();
    for ci in order {
        let c = cells.get(ci).cloned().ok_or(Atlas::Refused)?;
        let t = tex.get(c.source).and_then(Option::as_ref).ok_or(Atlas::Refused)?;
        let tid = *ids.get(c.source).ok_or(Atlas::Refused)?;
        let q = c.core;
        let (qw, qh) = (q[2] - q[0], q[3] - q[1]);
        let c0 = bx(t, 0, &q)?;
        let (py, px) = probe_at(&c0, qw, qh)?;
        let probe = c0.slice(py as i64, py as i64 + GRID, px as i64, px as i64 + GRID);
        let need = if t.fmt == L8 { 1.0 } else { 0.9 };
        let mut found: Option<(usize, i64, i64)> = None;
        for &ki in &kept {
            let k = cells.get(ki).cloned().ok_or(Atlas::Refused)?;
            if k.source == c.source && k.x <= q[0] && k.y <= q[1] && q[2] <= k.x + k.w && q[3] <= k.y + k.h {
                found = Some((ki, c.x - k.x, c.y - k.y));
                break;
            }
            if k.w < qw || k.h < qh {
                continue;
            }
            let kt = tex.get(k.source).and_then(Option::as_ref).ok_or(Atlas::Refused)?;
            let kid = *ids.get(k.source).ok_or(Atlas::Refused)?;
            let k0 = crop(kt, 0, k.x, k.y, k.w, k.h)?;
            let mut hit = None;
            let oy_start = -(fdiv(q[1] - c.y, GRID)) * GRID;
            let x_first = -(fdiv(q[0] - c.x, GRID)) * GRID;
            let mut oy = oy_start;
            'scan: while oy < k.h - (q[3] - c.y) + 1 {
                let mut ox = x_first;
                while ox < k.w - (q[2] - c.x) + 1 {
                    let (ky, kx) = (oy + q[1] - c.y + py as i64, ox + q[0] - c.x + px as i64);
                    if ky >= 0 && kx >= 0 {
                        let pk = k0.slice(ky, ky + probe.h as i64, kx, kx + probe.w as i64);
                        if (pk.h, pk.w) == (probe.h, probe.w) && pk.h * pk.w > 0 {
                            let n = pk.h * pk.w;
                            let eq = (0..n)
                                .filter(|&i| {
                                    pk.data.get(i * pk.d..(i + 1) * pk.d)
                                        == probe.data.get(i * probe.d..(i + 1) * probe.d)
                                })
                                .count();
                            // float((pk == probe).all(-1).mean()) < need: count / n exact in a double
                            let share = eq as f64 / n as f64;
                            if share >= need {
                                let mv = (k.x + ox - c.x, k.y + oy - c.y);
                                let moved = [q[0] + mv.0, q[1] + mv.1, q[2] + mv.0, q[3] + mv.1];
                                let mut all = true;
                                for m in 0..=D {
                                    if !same_image((kid, kt), (tid, t), m, &moved, &q, cache)? {
                                        all = false;
                                        break;
                                    }
                                }
                                if all {
                                    hit = Some((ox, oy));
                                    break 'scan;
                                }
                            }
                        } else if pk.h * pk.w == 0 && probe.h * probe.w == 0 {
                            return Err(Atlas::Refused); // an empty probe: the mean of nothing
                        }
                    }
                    ox += GRID;
                }
                oy += GRID;
            }
            if let Some((ox, oy)) = hit {
                found = Some((ki, ox, oy));
                break;
            }
        }
        let cm = cells.get_mut(ci).ok_or(Atlas::Refused)?;
        match found {
            Some((ki, dx, dy)) => {
                cm.into = Some(ki);
                cm.dx = dx;
                cm.dy = dy;
            }
            None => kept.push(ci),
        }
    }
    Ok(kept)
}

type Order = fn(&Cell) -> (i64, i64);

fn orders() -> [Order; 4] {
    [
        |c| (-c.h, -c.w),
        |c| (-c.w * c.h, -c.h),
        |c| (-c.w, -c.h),
        |c| (-c.w.max(c.h), -c.h),
    ]
}

/// `pack(kept, uw, uh, aligns)`: places the kept cells (sets `at`) or `false`.
fn pack(cells: &mut [Cell], kept: &[usize], uw: i64, uh: i64, aligns: &[i64]) -> R<bool> {
    let (gw, gh) = (fdiv(uw, GRID), fdiv(uh, GRID));
    for &i in kept {
        let c = cells.get(i).ok_or(Atlas::Refused)?;
        if fdiv(c.h, GRID) > gh || fdiv(c.w, GRID) > gw {
            return Ok(false);
        }
    }
    let (gwu, ghu) = (gw.max(0) as usize, gh.max(0) as usize);
    for order in orders() {
        for by_column in [false, true] {
            let mut occ = vec![0i32; ghu * gwu];
            let mut at: HashMap<usize, (i64, i64)> = HashMap::new();
            let mut sorted: Vec<usize> = kept.to_vec();
            sorted.sort_by(|&a, &b| {
                let (ca, cb) = (&cells[a], &cells[b]);
                (order(ca), ca.rank, ca.y, ca.x).cmp(&(order(cb), cb.rank, cb.y, cb.x))
            });
            let mut complete = true;
            for &ci in &sorted {
                let c = cells.get(ci).ok_or(Atlas::Refused)?;
                let (ch, cw) = (fdiv(c.h, GRID) as usize, fdiv(c.w, GRID) as usize);
                if ch == 0 || cw == 0 {
                    return Err(Atlas::Refused); // s[:-0]: the reference's shapes do not match
                }
                // the summed-area table and the windows that are free
                let mut s = vec![0i64; (ghu + 1) * (gwu + 1)];
                for y in 0..ghu {
                    for x in 0..gwu {
                        let v = i64::from(occ[y * gwu + x]) + s[y * (gwu + 1) + x + 1] + s[(y + 1) * (gwu + 1) + x]
                            - s[y * (gwu + 1) + x];
                        s[(y + 1) * (gwu + 1) + x + 1] = v;
                    }
                }
                let (wy, wx) = (ghu + 1 - ch, gwu + 1 - cw);
                let free = |y: usize, x: usize| {
                    let at = |yy: usize, xx: usize| s[yy * (gwu + 1) + xx];
                    at(y + ch, x + cw) - at(y, x + cw) - at(y + ch, x) + at(y, x) == 0
                };
                let mut frees: Vec<(i64, i64)> = Vec::new();
                if by_column {
                    for x in 0..wx {
                        for y in 0..wy {
                            if free(y, x) {
                                frees.push((x as i64, y as i64));
                            }
                        }
                    }
                } else {
                    for y in 0..wy {
                        for x in 0..wx {
                            if free(y, x) {
                                frees.push((x as i64, y as i64));
                            }
                        }
                    }
                }
                if frees.is_empty() {
                    complete = false;
                    break;
                }
                let mut pick = None;
                for &align in aligns {
                    pick = frees
                        .iter()
                        .find(|&&(x, y)| {
                            (x * GRID - c.x).rem_euclid(align) == 0 && (y * GRID - c.y).rem_euclid(align) == 0
                        })
                        .copied();
                    if pick.is_some() {
                        break;
                    }
                }
                let (x, y) = pick.ok_or(Atlas::Refused)?;
                for yy in y as usize..y as usize + ch {
                    for xx in x as usize..x as usize + cw {
                        if let Some(o) = occ.get_mut(yy * gwu + xx) {
                            *o = 1;
                        }
                    }
                }
                at.insert(ci, (x * GRID, y * GRID));
            }
            if complete {
                for &ci in kept {
                    if let Some(c) = cells.get_mut(ci) {
                        c.at = at.get(&ci).copied();
                    }
                }
                return Ok(true);
            }
        }
    }
    Ok(false)
}

/// `texwrite.l8_down(level)`.
fn l8_down(lv: &Level) -> R<Level> {
    if !lv.h.is_multiple_of(2) || !lv.w.is_multiple_of(2) {
        return Err(Atlas::Refused); // the reference's slices would not broadcast
    }
    let (h, w) = (lv.h / 2, lv.w / 2);
    let mut out = Level::zeros(h, w, 1);
    let g = |y: usize, x: usize| i32::from(lv.data.get(y * lv.w + x).copied().unwrap_or(0));
    for y in 0..h {
        for x in 0..w {
            let v = (g(2 * y, 2 * x) + g(2 * y + 1, 2 * x) + g(2 * y, 2 * x + 1) + g(2 * y + 1, 2 * x + 1) + 2) / 4;
            if let Some(o) = out.data.get_mut(y * w + x) {
                *o = v as u8;
            }
        }
    }
    Ok(out)
}

/// `texwrite.atlas_levels(shorter)`.
fn atlas_levels(shorter: i64) -> usize {
    let mut n = 0usize;
    while (1i64 << n) < shorter {
        n += 1;
    }
    n
}

/// `compose(kept, tex, fmt, w, h, levels)`.
fn compose(cells: &[Cell], kept: &[usize], tex: &[Option<Tex>], fmt: u32, w: i64, h: i64, levels: usize) -> R<Tex> {
    let unit = if fmt == BC5 { 4 } else { 1 };
    let (uw, uh) = (fdiv(w, unit) as usize, fdiv(h, unit) as usize);
    let depth = if fmt == L8 { 1 } else { 16 };
    let mut owner = vec![-1i64; uw * uh];
    for (i, &ci) in kept.iter().enumerate() {
        let c = cells.get(ci).ok_or(Atlas::Refused)?;
        let (ax, ay) = c.at.ok_or_else(|| Atlas::Error("an unplaced cell".into()))?;
        let (ya, yb) = py_range(ay, ay + c.h, uh);
        let (xa, xb) = py_range(ax, ax + c.w, uw);
        for y in ya..yb {
            for x in xa..xb {
                if let Some(o) = owner.get_mut(y * uw + x) {
                    *o = i as i64;
                }
            }
        }
    }
    let mut out: Vec<Level> = Vec::new();
    for m in 0..levels {
        let s = 1i64 << m;
        let (lw, lh) = if fmt == L8 {
            ((w >> m).max(1) as usize, (h >> m).max(1) as usize)
        } else {
            (
                (((w >> m).max(1) + 3) / 4).max(1) as usize,
                (((h >> m).max(1) + 3) / 4).max(1) as usize,
            )
        };
        let lv = if m <= K {
            let mut lv = Level::zeros(lh, lw, depth);
            for &ci in kept {
                let c = cells.get(ci).ok_or(Atlas::Refused)?;
                let (ax, ay) = c.at.ok_or(Atlas::Refused)?;
                let st = tex.get(c.source).and_then(Option::as_ref).ok_or(Atlas::Refused)?;
                let src = crop(st, m, c.x, c.y, c.w, c.h)?;
                let (y0, x0) = (fdiv(ay, s), fdiv(ax, s));
                // lv[y0:y0 + src.h, x0:x0 + src.w] = src: the slice must be the source's shape
                let (ya, yb) = py_range(y0, y0 + src.h as i64, lh);
                let (xa, xb) = py_range(x0, x0 + src.w as i64, lw);
                if (yb - ya, xb - xa) != (src.h, src.w) || src.d != depth {
                    return Err(Atlas::Refused);
                }
                for y in 0..src.h {
                    let row = src
                        .data
                        .get(y * src.w * depth..(y + 1) * src.w * depth)
                        .unwrap_or_default();
                    let o = ((ya + y) * lw + xa) * depth;
                    if let Some(dst) = lv.data.get_mut(o..o + row.len()) {
                        dst.copy_from_slice(row);
                    }
                }
            }
            lv
        } else if fmt == L8 {
            l8_down(out.last().ok_or(Atlas::Refused)?)?
        } else {
            let mut lv = Level::zeros(lh, lw, depth);
            for by in 0..lh as i64 {
                for bx2 in 0..lw as i64 {
                    let cy = (by * s + s / 2).min(uh as i64 - 1);
                    let cx = (bx2 * s + s / 2).min(uw as i64 - 1);
                    if cy < 0 || cx < 0 {
                        return Err(Atlas::Refused);
                    }
                    let i = *owner.get(cy as usize * uw + cx as usize).ok_or(Atlas::Refused)?;
                    if i < 0 {
                        continue;
                    }
                    let c = cells
                        .get(*kept.get(i as usize).ok_or(Atlas::Refused)?)
                        .ok_or(Atlas::Refused)?;
                    let (ax, ay) = c.at.ok_or(Atlas::Refused)?;
                    let st = tex.get(c.source).and_then(Option::as_ref).ok_or(Atlas::Refused)?;
                    let (sy, sx) = ((c.y + cy - ay) >> m, (c.x + cx - ax) >> m);
                    if let Some(sl) = st.levels.get(m)
                        && sy >= 0
                        && sx >= 0
                        && (sy as usize) < sl.h
                        && (sx as usize) < sl.w
                    {
                        let u = sl.unit(sy as usize, sx as usize).to_vec();
                        let o = (by as usize * lw + bx2 as usize) * depth;
                        if let Some(dst) = lv.data.get_mut(o..o + depth) {
                            dst.copy_from_slice(&u);
                        }
                    }
                }
            }
            lv
        };
        out.push(lv);
    }
    Ok(Tex {
        fmt,
        flags: 0,
        w,
        h,
        levels: out,
    })
}

// ------------------------------------------------------------------------------------------------ the build

/// One drawn `TextureInfo` channel (`Use`).
#[derive(Debug, Clone)]
pub struct Use {
    /// The emitter partition's name.
    pub emitter: String,
    /// The instance's guid.
    pub instance: String,
    /// The `TextureInfo`'s name.
    pub info: String,
    /// 0 alpha, 1 normals.
    pub channel: usize,
    /// The source's index.
    pub source: usize,
    /// Its rectangle in texels.
    pub rect: Rect,
    /// The merged cell drawing it (an index into its channel's cells).
    pub cell: usize,
    /// The move in texels.
    pub mv: (i64, i64),
    /// The fields' old values.
    pub old: [f64; 4],
    /// The fields' new values.
    pub new: [f64; 4],
}

/// An emitter: its name, partition bytes and source index.
pub type Emitter = (String, Vec<u8>, usize);

/// A source atlas pair (the alpha atlas; the normals or none).
pub type Source = (Vec<u8>, Option<Vec<u8>>);

/// What `build` gives.
#[derive(Debug, Clone)]
pub struct Built {
    /// The merged atlases (PC `ITexture` resources).
    pub alpha: Vec<u8>,
    /// The merged normals.
    pub normals: Vec<u8>,
    /// Each emitter's partition, in the emitters' order.
    pub partitions: Vec<(String, Vec<u8>)>,
    /// The uses, in the reference's order.
    pub uses: Vec<Use>,
    /// Per channel: every merged cell, then the kept ones' indices.
    pub cells: [(Vec<Cell>, Vec<usize>); 2],
    /// Per channel the atlas size.
    pub sizes: [(i64, i64); 2],
    /// The report.
    pub report: Vec<String>,
    /// `TextureInfo` channels left as they are.
    pub left: Vec<String>,
}

/// Python's `repr` of a string the port prints (plain ASCII only; anything else is left to the reference).
fn repr(s: &str) -> R<String> {
    if s.bytes().all(|b| (0x20..0x7F).contains(&b) && b != b'\'' && b != b'\\') {
        Ok(format!("'{s}'"))
    } else {
        Err(Atlas::Refused)
    }
}

fn like(res: &[u8]) -> R<(u32, u32, i64, i64)> {
    Ok((
        u32_at(res, 8)?,
        u32_at(res, 12)?,
        i64::from(u32_at(res, 16)?),
        i64::from(u32_at(res, 20)?),
    ))
}

/// Python's `'%.0%'`-style `{x:.0%}`: x times 100 in a double, then fixed with no decimals (ties to even).
fn percent0(x: f64) -> String {
    format!("{}%", crate::pyfmt::fixed(x * 100.0, 0))
}

/// `build(emitters, sources, alpha_like, normals_like)`; sources by index (the caller's dict order) with their keys.
///
/// # Errors
/// The reference's `EmitterAtlasError`s; refused where it raises otherwise (see the module's text).
#[expect(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn build(emitters: &[Emitter], sources: &[(String, Source)], alpha_like: &[u8], normals_like: &[u8]) -> R<Built> {
    let mut texs: [Vec<Option<Tex>>; 2] = [Vec::new(), Vec::new()];
    for (_, (a, n)) in sources {
        texs[0].push(Some(read_tex(a)?));
        texs[1].push(match n {
            Some(n) => Some(read_tex(n)?),
            None => None,
        });
    }
    let mut parsed: Vec<DbxFile> = Vec::with_capacity(emitters.len());
    for (_, d, _) in emitters {
        if d.is_empty() || !dbx::is_binary(d) {
            return Err(Atlas::Refused);
        }
        parsed.push(dbx::parse(d).map_err(|_| Atlas::Refused)?);
    }
    let mut keys: Vec<(&str, usize)> = sources.iter().enumerate().map(|(i, (k, _))| (k.as_str(), i)).collect();
    keys.sort_unstable();
    let mut rank = vec![0usize; sources.len()];
    for (r, (_, i)) in keys.iter().enumerate() {
        if let Some(x) = rank.get_mut(*i) {
            *x = r;
        }
    }
    let mut by_name: Vec<usize> = (0..emitters.len()).collect();
    by_name.sort_by(|&a, &b| emitters[a].0.cmp(&emitters[b].0));
    let mut uses: Vec<Use> = Vec::new();
    let mut left: Vec<String> = Vec::new();
    // raw cells per channel per source, the sources in first-use order
    let mut raw: [OrderedMap<usize, Vec<Cell>>; 2] = [OrderedMap::new(), OrderedMap::new()];
    let mut use_cells: Vec<Cell> = Vec::new();
    for &ei in &by_name {
        let (n, _, src) = &emitters[ei];
        let f = &parsed[ei];
        let Some((key, _)) = sources.get(*src) else {
            return Err(Atlas::Refused);
        };
        let spawn = spawns_frames(f)?;
        for info in infos(f)? {
            if info.values.iter().any(|(_, v)| v.is_nan()) {
                return Err(Atlas::Refused);
            }
            for ch in 0..2 {
                if !drawn(&info, ch) {
                    left.push(format!("{n} {} {} {}", info.instance, info.name, CHANNELS[ch]));
                    continue;
                }
                let Some(t) = texs[ch].get(*src).and_then(Option::as_ref) else {
                    return err(format!("{n}: source {} has no {} atlas", repr(key)?, CHANNELS[ch]));
                };
                let r = rect(&info, ch, spawn, t.w, t.h)?;
                let l0 = t.level(0)?;
                let (uw, uh) = (l0.w as i64, l0.h as i64);
                let [x0, y0, x1, y1] = unit_rect(&r, t.unit(), uw, uh)?;
                let c = Cell {
                    source: *src,
                    rank: *rank.get(*src).ok_or(Atlas::Refused)?,
                    x: x0,
                    y: y0,
                    w: x1 - x0,
                    h: y1 - y0,
                    into: None,
                    dx: 0,
                    dy: 0,
                    at: None,
                    core: core_rect(&r, t.unit(), uw, uh),
                };
                raw[ch].entry_default(*src).push(c.clone());
                let [ku, kv, ksu, ksv] = fields(ch);
                uses.push(Use {
                    emitter: n.clone(),
                    instance: info.instance.clone(),
                    info: info.name.clone(),
                    channel: ch,
                    source: *src,
                    rect: r,
                    cell: use_cells.len(),
                    mv: (0, 0),
                    old: [info.v(ku), info.v(kv), info.v(ksu), info.v(ksv)],
                    new: [0.0; 4],
                });
                use_cells.push(c);
            }
        }
    }
    let mut report = Vec::new();
    let mut cells_out: [(Vec<Cell>, Vec<usize>); 2] = [(Vec::new(), Vec::new()), (Vec::new(), Vec::new())];
    let mut sizes = [(0i64, 0i64); 2];
    let mut merged: [Option<Tex>; 2] = [None, None];
    let ids: Vec<usize> = (0..sources.len()).collect();
    let mut caches = [Cache::default(), Cache::default()];
    for ch in 0..2 {
        let (likeb, more): (&[u8], &[(i64, i64)]) = if ch == 0 {
            (alpha_like, &ALPHA_SIZES)
        } else {
            (normals_like, &NORMAL_SIZES)
        };
        let (fmt, flags, lw, lh) = like(likeb)?;
        let mut cells: Vec<Cell> = Vec::new();
        for (_, cs) in raw[ch].iter() {
            cells.extend(merge(cs));
        }
        for u in uses.iter_mut().filter(|u| u.channel == ch) {
            let c = use_cells.get(u.cell).ok_or(Atlas::Refused)?;
            u.cell = cells
                .iter()
                .position(|m| (m.source, m.x, m.y, m.w, m.h) == (c.source, c.x, c.y, c.w, c.h))
                .ok_or(Atlas::Refused)?;
        }
        let kept = dedupe(&mut cells, &texs[ch], &ids, &mut caches[ch])?;
        let unit = if fmt == BC5 { 4 } else { 1 };
        let mut size = None;
        let tries: Vec<(i64, i64)> = std::iter::once((lw, lh)).chain(more.iter().copied()).collect();
        for &(w, h) in &tries {
            let mut ok = false;
            for i in 0..ALIGN.len() {
                if pack(
                    &mut cells,
                    &kept,
                    fdiv(w, unit),
                    fdiv(h, unit),
                    ALIGN.get(i..).unwrap_or_default(),
                )? {
                    ok = true;
                    break;
                }
            }
            if ok {
                size = Some((w, h));
                break;
            }
        }
        let Some((w, h)) = size else {
            let list: Vec<String> = tries.iter().map(|(a, b)| format!("({a}, {b})")).collect();
            return err(format!(
                "{}: {} cells fit in none of [{}]",
                CHANNELS[ch],
                kept.len(),
                list.join(", ")
            ));
        };
        let levels = atlas_levels(w.min(h));
        let mut t = compose(&cells, &kept, &texs[ch], fmt, w, h, levels)?;
        t.flags = flags;
        let area: i64 = kept.iter().map(|&i| cells[i].w * cells[i].h).sum::<i64>() * unit * unit;
        let like_note = if (w, h) == (lw, lh) {
            String::new()
        } else {
            format!("; the like atlas: {lw} x {lh}")
        };
        report.push(format!(
            "{}: {} cells from {} sources, {} kept ({} found inside others), {} of {w} x {h} (format {fmt}, {levels} levels{like_note})",
            CHANNELS[ch],
            cells.len(),
            raw[ch].len(),
            kept.len(),
            cells.len() - kept.len(),
            percent0(area as f64 / (w * h) as f64),
        ));
        merged[ch] = Some(t);
        sizes[ch] = (w, h);
        cells_out[ch] = (cells, kept);
    }
    // re-point
    let mut by_emitter: OrderedMap<String, Vec<usize>> = OrderedMap::new();
    for (i, u) in uses.iter().enumerate() {
        by_emitter.entry_default(u.emitter.clone()).push(i);
    }
    let mut partitions = Vec::with_capacity(emitters.len());
    for (ei, (n, d, _)) in emitters.iter().enumerate() {
        let Some(list) = by_emitter.get(n).cloned() else {
            partitions.push((n.clone(), d.clone()));
            continue;
        };
        let f = parsed.get_mut(ei).ok_or(Atlas::Refused)?;
        let infos_now = infos(f)?;
        let mut index: HashMap<(String, String), usize> = HashMap::new();
        for (i, info) in infos_now.iter().enumerate() {
            index.insert((info.instance.clone(), info.name.clone()), i);
        }
        for ui in list {
            let u = uses.get(ui).cloned().ok_or(Atlas::Refused)?;
            let info = infos_now
                .get(*index.get(&(u.instance.clone(), u.info.clone())).ok_or(Atlas::Refused)?)
                .ok_or(Atlas::Refused)?;
            let st = texs[u.channel]
                .get(u.source)
                .and_then(Option::as_ref)
                .ok_or(Atlas::Refused)?;
            let mt = merged[u.channel].as_ref().ok_or(Atlas::Refused)?;
            let (cells, _) = &cells_out[u.channel];
            let cell = cells.get(u.cell).ok_or(Atlas::Refused)?;
            // place(): follow `into` to the kept cell
            let (mut c, mut dx, mut dy) = (cell, 0i64, 0i64);
            while let Some(i) = c.into {
                dx += c.dx;
                dy += c.dy;
                c = cells.get(i).ok_or(Atlas::Refused)?;
            }
            let (ax, ay) = c.at.ok_or_else(|| Atlas::Error("a cell without a place".into()))?;
            let (px, py) = (ax + dx, ay + dy);
            let (mx, my) = ((px - cell.x) * st.unit(), (py - cell.y) * st.unit());
            let (sw, sh, mw, mh) = (st.w as f64, st.h as f64, mt.w as f64, mt.h as f64);
            let new = [
                (u.old[0] * sw + mx as f64) / mw,
                (u.old[1] * sh + my as f64) / mh,
                u.old[2] * sw / mw,
                u.old[3] * sh / mh,
            ];
            let names = fields(u.channel);
            for (k, v) in names.iter().zip(new) {
                let b = v as f32;
                if !b.is_finite() && v.is_finite() {
                    return Err(Atlas::Refused); // struct.pack's OverflowError
                }
                if f64::from(b) != v {
                    return Err(Atlas::Refused); // the reference's message prints the value's repr
                }
                let (ii, ci, fi) = info
                    .elements
                    .iter()
                    .find(|(n, _)| n == k)
                    .map(|x| x.1)
                    .ok_or(Atlas::Refused)?;
                let e = element_mut(f, ii, ci, fi)?;
                let Body::Numbers(raw) = &mut e.body else {
                    return Err(Atlas::Refused);
                };
                let keep = raw.len().saturating_sub(4);
                raw.truncate(keep);
                raw.extend_from_slice(&b.to_be_bytes());
            }
            if let Some(um) = uses.get_mut(ui) {
                um.mv = (mx, my);
                um.new = new;
            }
        }
        partitions.push((n.clone(), dbx::write(f).map_err(|_| Atlas::Refused)?));
    }
    let [ma, mn] = merged;
    Ok(Built {
        alpha: write_tex(ma.as_ref().ok_or(Atlas::Refused)?)?,
        normals: write_tex(mn.as_ref().ok_or(Atlas::Refused)?)?,
        partitions,
        uses,
        cells: cells_out,
        sizes,
        report,
        left,
    })
}

fn element_mut(f: &mut DbxFile, ii: usize, ci: usize, fi: usize) -> R<&mut Element> {
    let root = f.roots.first_mut().ok_or(Atlas::Refused)?;
    let Body::Container(insts) = &mut root.body else {
        return Err(Atlas::Refused);
    };
    let inst = insts.get_mut(ii).ok_or(Atlas::Refused)?;
    let Body::Container(cs) = &mut inst.body else {
        return Err(Atlas::Refused);
    };
    let c = cs.get_mut(ci).ok_or(Atlas::Refused)?;
    let Body::Container(fs) = &mut c.body else {
        return Err(Atlas::Refused);
    };
    fs.get_mut(fi).ok_or(Atlas::Refused)
}

// ------------------------------------------------------------------------------------------------ proof

/// A proof row's level entry.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The level.
    pub level: usize,
    /// Texels compared.
    pub texels: usize,
    /// Equal everywhere.
    pub equal: bool,
    /// The share of equal texels, rounded to 4 places.
    pub equal_share: f64,
    /// The correlation, rounded to 4 places.
    pub correlation: f64,
}

/// One use's proof.
#[derive(Debug, Clone, PartialEq)]
pub struct Proof {
    /// The rectangle rounded to 3 places.
    pub rect: [f64; 4],
    /// The move.
    pub mv: (i64, i64),
    /// The size kept.
    pub size_kept: bool,
    /// Levels 0..D.
    pub near: Vec<Row>,
    /// The deeper levels.
    pub deep: Vec<Row>,
    /// ok.
    pub ok: bool,
    /// exact.
    pub exact: bool,
    /// Level 0's source crop: its int32 texels, little-endian, C order (the caller hashes them), or `None`.
    pub crop0: Option<Vec<u8>>,
}

/// Two crops of one shape: (a's texels, b's texels, h, w, channels).
type Crops = (Vec<i32>, Vec<i32>, usize, usize, usize);

/// `crops(t, m, r, move, other, margin)` as (a's crop, b's crop, h, w).
#[expect(clippy::too_many_arguments, reason = "the reference's arguments and the two caches")]
fn crops(
    st: (usize, &Tex),
    mt: (usize, &Tex),
    m: usize,
    r: &Rect,
    mv: (i64, i64),
    margin: i64,
    cs: &mut Cache,
    cm: &mut Cache,
) -> R<Option<Crops>> {
    let s = 1i64 << m;
    let sf = s as f64;
    cs.texels(st.0, st.1, m)?;
    cm.texels(mt.0, mt.1, m)?;
    let a = cs.map.get(&(st.0, m)).ok_or(Atlas::Refused)?;
    let b = cm.map.get(&(mt.0, m)).ok_or(Atlas::Refused)?;
    let (mx, my) = (fdiv(mv.0, s), fdiv(mv.1, s));
    let (mut x0, mut y0, mut x1, mut y1) = if margin >= 0 {
        (
            (r[0] / sf).floor() as i64 - margin,
            (r[1] / sf).floor() as i64 - margin,
            (r[2] / sf).ceil() as i64 + margin,
            (r[3] / sf).ceil() as i64 + margin,
        )
    } else {
        (
            (r[0] / sf).ceil() as i64,
            (r[1] / sf).ceil() as i64,
            (r[2] / sf).floor() as i64,
            (r[3] / sf).floor() as i64,
        )
    };
    x0 = x0.max(0).max(-mx);
    y0 = y0.max(0).max(-my);
    x1 = x1.min(a.w as i64).min(b.w as i64 - mx);
    y1 = y1.min(a.h as i64).min(b.h as i64 - my);
    if x1 <= x0 || y1 <= y0 {
        return Ok(None);
    }
    let (ha, wa, va) = a.crop(y0, y1, x0, x1);
    let (hb, wb, vb) = b.crop(y0 + my, y1 + my, x0 + mx, x1 + mx);
    if (ha, wa) != (hb, wb) {
        return Err(Atlas::Refused);
    }
    Ok(Some((va, vb, ha, wa, a.c)))
}

/// `correlation(a, b)` rounded to 4 places (the caller knows a != b), `None` when too close to call.
fn correlation4(a: &[i32], b: &[i32]) -> R<f64> {
    let constant = |v: &[i32]| v.iter().all(|&z| Some(&z) == v.first());
    if constant(a) || constant(b) {
        return Ok(0.0);
    }
    round4(Corr::of(a, b).value()).ok_or(Atlas::Refused)
}

/// The re-read state `prove` needs of each emitter: its uses' infos and its spawn flag.
type Reread = HashMap<String, HashMap<(String, String), Info>>;

/// `prove(result, emitters, sources)`: one proof per use, in order.
///
/// # Errors
/// Refused where the reference raises or a number cannot be decided the same way for certain.
#[expect(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn prove(built: &Built, emitters: &[Emitter], sources: &[(String, Source)]) -> R<Vec<Proof>> {
    let merged = [read_tex(&built.alpha)?, read_tex(&built.normals)?];
    let mut reread: Reread = HashMap::new();
    for (n, d) in &built.partitions {
        let f = dbx::parse(d).map_err(|_| Atlas::Refused)?;
        let mut m = HashMap::new();
        for i in infos(&f)? {
            m.insert((i.instance.clone(), i.name.clone()), i);
        }
        reread.insert(n.clone(), m);
    }
    let mut spawn: HashMap<String, bool> = HashMap::new();
    for (n, d, _) in emitters {
        let f = dbx::parse(d).map_err(|_| Atlas::Refused)?;
        spawn.insert(n.clone(), spawns_frames(&f)?);
    }
    let mut src_tex: HashMap<(usize, usize), Tex> = HashMap::new();
    let mut src_cache = Cache::default();
    let mut m_cache = [Cache::default(), Cache::default()];
    let mut out = Vec::with_capacity(built.uses.len());
    for u in &built.uses {
        let key = (u.channel, u.source);
        if let std::collections::hash_map::Entry::Vacant(e) = src_tex.entry(key) {
            let (_, (a, n)) = sources.get(u.source).ok_or(Atlas::Refused)?;
            let res = if u.channel == 0 {
                a
            } else {
                n.as_ref().ok_or(Atlas::Refused)?
            };
            e.insert(read_tex(res)?);
        }
        let st = src_tex.get(&key).ok_or(Atlas::Refused)?;
        let sid = u.channel * sources.len() + u.source;
        let mt = &merged[u.channel];
        let info = reread
            .get(&u.emitter)
            .and_then(|m| m.get(&(u.instance.clone(), u.info.clone())))
            .ok_or(Atlas::Refused)?;
        let r_new = rect(
            info,
            u.channel,
            *spawn.get(&u.emitter).ok_or(Atlas::Refused)?,
            mt.w,
            mt.h,
        )?;
        let dxf = r_new[0] - u.rect[0];
        let dyf = r_new[1] - u.rect[1];
        let mv = (round_even(dxf) as i64, round_even(dyf) as i64);
        let size_ok = ((r_new[2] - r_new[0]) - (u.rect[2] - u.rect[0])).abs() < 1e-3
            && ((r_new[3] - r_new[1]) - (u.rect[3] - u.rect[1])).abs() < 1e-3
            && (dxf - mv.0 as f64).abs() < 1e-3
            && (dyf - mv.1 as f64).abs() < 1e-3;
        let mut near = Vec::new();
        let mut deep = Vec::new();
        for m in 0..st.levels.len().min(mt.levels.len()) {
            let margin = if m <= D { 1 } else { -1 };
            let Some((a, b, h, w, c)) = crops(
                (sid, st),
                (u.channel, mt),
                m,
                &u.rect,
                mv,
                margin,
                &mut src_cache,
                &mut m_cache[u.channel],
            )?
            else {
                continue;
            };
            let equal = a == b;
            let n = h * w;
            let same_px = (0..n)
                .filter(|&i| a.get(i * c..(i + 1) * c) == b.get(i * c..(i + 1) * c))
                .count();
            let share = round_to(same_px as f64 / n as f64, 4);
            let corr = if equal { 1.0 } else { correlation4(&a, &b)? };
            let row = Row {
                level: m,
                texels: n,
                equal,
                equal_share: share,
                correlation: corr,
            };
            if m <= D {
                near.push(row);
            } else {
                deep.push(row);
            }
        }
        let ok = size_ok && !near.is_empty() && near.iter().all(|x| x.equal || x.correlation >= PASS);
        let exact = ok && near.iter().all(|x| x.equal);
        let c0 = crops(
            (sid, st),
            (u.channel, mt),
            0,
            &u.rect,
            mv,
            1,
            &mut src_cache,
            &mut m_cache[u.channel],
        )?;
        let crop0 = c0.map(|(a, ..)| a.iter().flat_map(|v| v.to_le_bytes()).collect());
        out.push(Proof {
            rect: [
                round_to(u.rect[0], 3),
                round_to(u.rect[1], 3),
                round_to(u.rect[2], 3),
                round_to(u.rect[3], 3),
            ],
            mv,
            size_kept: size_ok,
            near,
            deep,
            ok,
            exact,
            crop0,
        });
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ same_but_uv

/// `same_but_uv(old, new)`: the differences between two partitions' trees other than the UV fields.
///
/// # Errors
/// Refused where the reference raises (a file that does not parse).
pub fn same_but_uv(old: &[u8], new: &[u8]) -> R<Vec<String>> {
    let parse = |d: &[u8]| -> R<DbxFile> {
        if d.is_empty() || !dbx::is_binary(d) {
            return Err(Atlas::Refused);
        }
        dbx::parse(d).map_err(|_| Atlas::Refused)
    };
    let (a, b) = (parse(old)?, parse(new)?);
    let mut diffs = Vec::new();
    if a.roots.len() != b.roots.len() {
        return Ok(vec!["roots".to_owned()]);
    }
    for (x, y) in a.roots.iter().zip(&b.roots) {
        walk(&a, &b, x, y, String::new(), &mut diffs);
    }
    Ok(diffs)
}

fn text_of<'f>(f: &'f DbxFile, e: &Element) -> &'f str {
    match e.body {
        Body::Text(t) => f.str(t),
        _ => "",
    }
}

fn walk(fa: &DbxFile, fb: &DbxFile, x: &Element, y: &Element, path: String, diffs: &mut Vec<String>) {
    let attrs = |f: &DbxFile, e: &Element| -> Vec<(String, String)> {
        e.attributes
            .iter()
            .map(|(k, v)| (f.str(*k).to_owned(), f.str(*v).to_owned()))
            .collect()
    };
    if (tag(fa, x), x.kind, attrs(fa, x), text_of(fa, x)) != (tag(fb, y), y.kind, attrs(fb, y), text_of(fb, y)) {
        diffs.push(path);
        return;
    }
    let xn = attr(fa, x, "name");
    if x.raw() != y.raw() && !xn.is_some_and(|n| UV.contains(&n) || NM.contains(&n)) {
        diffs.push(format!(
            "{path}/{}",
            xn.filter(|s| !s.is_empty()).unwrap_or_else(|| tag(fa, x))
        ));
    }
    let (xc, yc) = (x.children(), y.children());
    if xc.len() != yc.len() {
        diffs.push(format!("{path} (children)"));
        return;
    }
    for (i, (p, q)) in xc.iter().zip(yc).enumerate() {
        let label = attr(fa, p, "name")
            .filter(|s| !s.is_empty())
            .or_else(|| attr(fa, p, "guid").filter(|s| !s.is_empty()))
            .unwrap_or_else(|| tag(fa, p));
        walk(fa, fb, p, q, format!("{path}/{label}[{i}]"), diffs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slicing_as_python() {
        assert_eq!(py_range(-2, 10, 5), (3, 5));
        assert_eq!(py_range(4, 2, 5), (4, 4));
        assert_eq!(fdiv(-3, 8), -1);
        assert_eq!(cdiv(9, 8), 2);
        assert_eq!(cdiv(-9, 8), -1);
    }

    #[test]
    fn exact_statistics() {
        assert_eq!(var_num(&[1, 1, 1, 1]), 0);
        let c = Corr::of(&[1, 2, 3, 4], &[2, 4, 6, 8]);
        assert!((c.value() - 1.0).abs() < 1e-15);
        assert_eq!(round4(0.123_44), Some(0.1234));
        assert_eq!(round4(0.123_450_000_000_1), None);
        assert_eq!(percent0(0.125), "12%");
        assert_eq!(atlas_levels(2048), 11);
    }

    #[test]
    fn bc5_blocks() {
        let b = [255u8, 0, 0, 0, 0, 0, 0, 0];
        assert_eq!(bc5_channel(&b)[0], 255);
        let six = [0u8, 255, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF];
        assert_eq!(bc5_channel(&six)[0], 255); // index 7 in 6-value mode
    }
}
