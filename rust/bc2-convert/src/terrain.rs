//! Xbox 360 terrain resources in BFBC2's PC formats, ported from `bfbc2-console-convert/terrain.py` (its docstring
//! describes every format) and `soot.py` (round F2a).
//!
//! | Python | here |
//! |---|---|
//! | `terrain.heightfield` / `sub_heightfield` / `quadtree_nodes` | [`heightfield`], [`quadtree_nodes`] |
//! | `terrain.water_mesh`, `material_map`, `material_names` | [`water_mesh`], [`material_map`], [`material_names`] |
//! | `terrain.visual_terrain`, `visual_terrain_parts`, `road_strips` (`_Walk`) | [`visual_terrain`], [`visual_terrain_parts`], [`road_strips`] |
//! | `terrain.visual_water`, `visual_water_parts` (`_dec3n`) | [`visual_water`], [`visual_water_parts`] |
//! | `soot.soot_mesh` | [`soot_mesh`] |
//!
//! The bytes are the reference's to the bit: values swapped by their width (floats as their bits, as NumPy's
//! byte-order casts keep them), the water's DEC3N vectors decoded as NumPy does (`x / 511.0` in doubles, stored as
//! a float, that float rounded to a half). Where the reference raises its own `TerrainError` / `SootError` this
//! returns [`Refusal::Error`] with the same message; where it would fail otherwise (`struct.error`, a NumPy buffer
//! too small, a division by zero) [`Refusal::Reference`]: the caller runs the Python, which raises what it raises.

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::chunks_exact_to_as_chunks,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::approx_constant,
    clippy::option_option,
    clippy::type_complexity,
    clippy::similar_names,
    clippy::manual_bit_width,
    clippy::verbose_bit_mask,
    clippy::needless_range_loop,
    clippy::nonminimal_bool,
    clippy::result_unit_err,
    clippy::map_unwrap_or,
    clippy::format_push_string,
    clippy::unnested_or_patterns,
    clippy::unnecessary_wraps,
    clippy::doc_markdown,
    reason = "every index is checked against the data's length first (the walks' need(), the decoders' block counts, the plans' key counts); the reference's names, comparisons, casts and arithmetic as it has them (a float midpoint, an exact float compare and a table constant are its values, not approximations)"
)]

use crate::mesh::f64_to_f16;

/// Why a resource is not converted here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The reference's own error (`TerrainError` / `SootError`), with its message.
    Error(String),
    /// A failure the reference reports another way: run the reference.
    Reference,
}

type Res<T> = Result<T, Refusal>;

fn fail<T>(msg: impl Into<String>) -> Res<T> {
    Err(Refusal::Error(msg.into()))
}

/// The tail's second part of a main heightfield, by size (`FIXED`).
#[must_use]
pub const fn fixed(n: u32) -> Option<usize> {
    match n {
        1024 => Some(62_474),
        2048 => Some(256_012),
        _ => None,
    }
}

/// The nodes of a sub-terrain heightfield's min / max quadtree (`quadtree_nodes`).
#[must_use]
pub fn quadtree_nodes(n: u64) -> u64 {
    let levels = u64::BITS - (n / 16).leading_zeros();
    (0..levels)
        .map(|k| 4u64.saturating_pow(k))
        .fold(0u64, u64::saturating_add)
}

fn be32(x: &[u8], at: usize) -> Res<u32> {
    let b = x
        .get(at..at.checked_add(4).ok_or(Refusal::Reference)?)
        .ok_or(Refusal::Reference)?;
    Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
}

fn le32(x: &[u8], at: usize) -> Res<u32> {
    let b = x
        .get(at..at.checked_add(4).ok_or(Refusal::Reference)?)
        .ok_or(Refusal::Reference)?;
    Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

/// `_swap(x, out, dt, count, at, to)`: `count` values of `width` bytes byte-swapped from `x[at..]` into
/// `out[to..]` (NumPy's buffer too small: [`Refusal::Reference`]).
fn swap(x: &[u8], out: &mut [u8], width: usize, count: usize, at: usize, to: usize) -> Res<()> {
    let size = count.checked_mul(width).ok_or(Refusal::Reference)?;
    let src = x
        .get(at..at.checked_add(size).ok_or(Refusal::Reference)?)
        .ok_or(Refusal::Reference)?;
    let dst = out.get_mut(to..to + size).ok_or(Refusal::Reference)?;
    for (d, s) in dst.chunks_exact_mut(width).zip(src.chunks_exact(width)) {
        for (a, b) in d.iter_mut().zip(s.iter().rev()) {
            *a = *b;
        }
    }
    Ok(())
}

fn bit_length(n: u64) -> u32 {
    u64::BITS - n.leading_zeros()
}

/// A sub-terrain heightfield (`sub_heightfield`).
fn sub_heightfield(x: &[u8], n: u32, bfbc2: bool) -> Res<Vec<u8>> {
    let n_ = n as usize;
    let head = if bfbc2 { 0x31 } else { 0x2D };
    let t = head + 2 * n_ * n_;
    let q = 4 * quadtree_nodes(u64::from(n)) as usize;
    let tail = x.len() as i64 - t as i64;
    if tail != q as i64 {
        return fail(format!(
            "sub-terrain heightfield {n}: a tail of {tail} bytes, not the {q} of its quadtree"
        ));
    }
    let mut out = vec![0u8; 0x31 + 2 * n_ * n_ + q];
    swap(x, &mut out, 4, 6, 0, 0)?;
    swap(x, &mut out, 4, 3, 0x18, 0x18)?;
    swap(x, &mut out, 2, 2, 0x24, 0x24)?;
    let h = [le32(&out, 0x18)?, le32(&out, 0x1C)?, le32(&out, 0x20)?];
    if (h[1], h[2]) != (bit_length(u64::from(n)).wrapping_sub(4), 0) {
        return fail(format!(
            "sub-terrain heightfield {n}: header ({}, {}, {})",
            h[0], h[1], h[2]
        ));
    }
    swap(x, &mut out, 2, n_ * n_, head, 0x31)?;
    swap(x, &mut out, 2, q / 2, t, 0x31 + 2 * n_ * n_)?;
    Ok(out)
}

/// An Xbox `TerrainHeightfield` (BFBC2's, or with `bfbc2` false Battlefield 1943's) as BFBC2's PC file
/// (`heightfield(x, source)`).
///
/// # Errors
/// See the module.
pub fn heightfield(x: &[u8], bfbc2: bool) -> Res<Vec<u8>> {
    let n = be32(x, 0x18)?;
    if n == 64 || n == 128 {
        return sub_heightfield(x, n, bfbc2);
    }
    let Some(fx) = fixed(n) else {
        return fail(format!("heightfield size {n}: only [1024, 2048] are known"));
    };
    let n_ = n as usize;
    let head = if bfbc2 { 0x31 } else { 0x2D };
    let t = (head + 2 * n_ * n_) as i64;
    let len = x.len() as i64;
    let region = len - t - 4 * n_ as i64;
    let a = region - fx as i64;
    if a < 0 || region.rem_euclid(2) != 0 {
        return fail(format!(
            "heightfield: a tail of {} bytes does not fit size {n}",
            len - t
        ));
    }
    let (t, region) = (t as usize, region as usize);
    let mut out = vec![0u8; 0x31 + 2 * n_ * n_ + 4 * n_ + region];
    swap(x, &mut out, 4, 6, 0, 0)?;
    swap(x, &mut out, 4, 3, 0x18, 0x18)?;
    swap(x, &mut out, 2, 2, 0x24, 0x24)?;
    if bfbc2 {
        swap(x, &mut out, 4, 1, 0x28, 0x28)?;
        out[0x2C] = *x.get(0x2C).ok_or(Refusal::Reference)?;
        let said = be32(x, 0x2D)?;
        if i64::from(said) != a {
            return fail(format!(
                "heightfield: the header says the first section is {said} bytes, the tail {a}"
            ));
        }
    }
    out[0x2D..0x31].copy_from_slice(&(a as u32).to_le_bytes());
    let big_t = 0x31 + 2 * n_ * n_;
    swap(x, &mut out, 2, n_ * n_, head, 0x31)?;
    swap(x, &mut out, 4, n_, t, big_t)?;
    swap(x, &mut out, 2, region / 2, t + 4 * n_, big_t + 4 * n_)?;
    Ok(out)
}

/// An Xbox `WaterMesh` as BFBC2's PC file (`water_mesh`).
///
/// # Errors
/// See the module.
pub fn water_mesh(x: &[u8]) -> Res<Vec<u8>> {
    if !x.len().is_multiple_of(4) {
        return fail(format!("water mesh: {} bytes, not 4-byte values", x.len()));
    }
    let mut out = x.to_vec();
    for c in out.chunks_exact_mut(4) {
        c.reverse();
    }
    Ok(out)
}

/// An Xbox `TerrainMaterialMap` as BFBC2's PC file (`material_map`).
///
/// # Errors
/// See the module.
pub fn material_map(x: &[u8]) -> Res<Vec<u8>> {
    let mut out = x.to_vec();
    if x.len() < 6 {
        return Err(Refusal::Reference);
    }
    let b = u16::from_be_bytes([x[4], x[5]]);
    out[0..4].reverse();
    out[4..6].reverse();
    let mut o = 6usize;
    for _ in 0..b {
        let l = be32(x, o)? as usize;
        out[o..o + 4].reverse();
        o += 4 + l;
        if o + 512 > x.len() {
            return fail(format!(
                "material map: block runs past the end ({} > {})",
                o + 512,
                x.len()
            ));
        }
        swap(x, &mut out, 2, 256, o, o)?;
        o += 512;
    }
    while o < x.len() {
        let l = be32(x, o)? as usize;
        out[o..o + 4].reverse();
        o += 4 + l;
    }
    if o != x.len() {
        return fail(format!("material map: the names end at {o}, the file at {}", x.len()));
    }
    Ok(out)
}

/// The material names of a PC `TerrainMaterialMap` (`material_names`), latin-1 bytes each.
///
/// # Errors
/// [`Refusal::Reference`] where the reference's `struct` reads fail.
pub fn material_names(pc: &[u8]) -> Res<Vec<Vec<u8>>> {
    if pc.len() < 6 {
        return Err(Refusal::Reference);
    }
    let b = u16::from_le_bytes([pc[4], pc[5]]);
    let mut o = 6usize;
    for _ in 0..b {
        o = o
            .checked_add(4 + le32(pc, o)? as usize + 512)
            .ok_or(Refusal::Reference)?;
    }
    let mut names = Vec::new();
    while o < pc.len() {
        let l = le32(pc, o)? as usize;
        let from = (o + 4).min(pc.len());
        let to = (o + 4 + l).min(pc.len());
        names.extend(
            pc[from..to]
                .split(|&c| c == 0)
                .filter(|s| !s.is_empty())
                .map(<[u8]>::to_vec),
        );
        o += 4 + l;
    }
    Ok(names)
}

/// A named part of a walked resource: (name, start, end).
pub type Part = (String, usize, usize);

/// `_Walk`: a resource walked field by field, each field written little-endian into a copy.
struct Walk<'a> {
    x: &'a [u8],
    out: Vec<u8>,
    o: usize,
    what: &'static str,
    big: bool,
    parts: Vec<Part>,
    at: usize,
    name: Option<String>,
}

impl<'a> Walk<'a> {
    fn new(x: &'a [u8], what: &'static str, big: bool) -> Self {
        Self {
            x,
            out: x.to_vec(),
            o: 0,
            what,
            big,
            parts: Vec::new(),
            at: 0,
            name: None,
        }
    }

    fn mark(&mut self, name: Option<String>) {
        if let Some(n) = self.name.take() {
            self.parts.push((n, self.at, self.o));
        }
        self.name = name;
        self.at = self.o;
    }

    fn need(&self, n: u128) -> Res<()> {
        if self.o as u128 + n > self.x.len() as u128 {
            return fail(format!(
                "{}: runs past the end at {:#x} (+{n} > {} bytes)",
                self.what,
                self.o,
                self.x.len()
            ));
        }
        Ok(())
    }

    /// `count` records of the given (width, count) fields: each value swapped on an Xbox file.
    fn arr(&mut self, fields: &[(usize, usize)], count: u64) -> Res<usize> {
        let item: usize = fields.iter().map(|(w, c)| w * c).sum();
        self.need(u128::from(count) * item as u128)?;
        let size = count as usize * item;
        let at = self.o;
        if self.big {
            let dst = &mut self.out[at..at + size];
            let src = &self.x[at..at + size];
            for (d, s) in dst.chunks_exact_mut(item.max(1)).zip(src.chunks_exact(item.max(1))) {
                let mut p = 0;
                for &(w, c) in fields {
                    for _ in 0..c {
                        for k in 0..w {
                            d[p + k] = s[p + w - 1 - k];
                        }
                        p += w;
                    }
                }
            }
        }
        self.o += size;
        Ok(at)
    }

    fn u32s(&mut self, count: u64) -> Res<usize> {
        self.arr(&[(4, 1)], count)
    }

    fn u32(&mut self) -> Res<u32> {
        let at = self.u32s(1)?;
        let b = [self.x[at], self.x[at + 1], self.x[at + 2], self.x[at + 3]];
        Ok(if self.big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }

    fn value(&self, at: usize) -> u32 {
        let b = [self.x[at], self.x[at + 1], self.x[at + 2], self.x[at + 3]];
        if self.big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    }

    fn string(&mut self) -> Res<&'a [u8]> {
        let x = self.x;
        let Some(e) = x.get(self.o..).and_then(|r| r.iter().position(|&c| c == 0)) else {
            return fail(format!("{}: a name at {:#x} has no end", self.what, self.o));
        };
        let s = &x[self.o..self.o + e];
        self.o += e + 1;
        Ok(s)
    }

    fn done(&mut self) -> Res<()> {
        self.mark(None);
        if self.o != self.x.len() {
            return fail(format!(
                "{}: the data ends at {:#x}, the file at {:#x}",
                self.what,
                self.o,
                self.x.len()
            ));
        }
        Ok(())
    }
}

const ROAD_VERTEX: [(usize, usize); 2] = [(4, 5), (2, 4)];

fn walk_visual_terrain(x: &[u8], big: bool) -> Res<Walk<'_>> {
    let mut w = Walk::new(x, "visual terrain", big);
    w.mark(Some("blocks".into()));
    let n = w.u32()?;
    if !(1..=256).contains(&n) {
        return fail(format!("visual terrain: {n} terrain blocks - not a VisualTerrain?"));
    }
    for i in 0..2 * n {
        w.mark(Some(format!("roads {i}")));
        for _ in 0..w.u32()? {
            w.string()?;
            w.string()?;
            w.string()?;
            w.u32s(2)?;
            w.arr(&[(4, 1)], 6)?;
            w.u32s(1)?;
        }
        let (v, b, idx) = (w.u32()?, w.u32()?, w.u32()?);
        if u64::from(b) != 28 * u64::from(v) {
            return fail(format!(
                "visual terrain: road vertices of {b} bytes for {v} (not 28 each)"
            ));
        }
        w.arr(&ROAD_VERTEX, u64::from(v))?;
        w.u32s(u64::from(idx))?;
    }
    w.mark(Some("patch shaders".into()));
    for _ in 0..w.u32()? {
        w.string()?;
        w.string()?;
        w.u32s(2)?;
        for _ in 0..w.u32()? {
            for _ in 0..w.u32()? {
                // mesh, a second string (`<mesh>/<guid>` on NAM_MP_003, else empty), shader (VEG-UG-2, TERR-VT-3):
                // the second is a string, not one byte, so a set one walks (round SPG2-rest2)
                w.string()?;
                w.string()?;
                w.string()?;
                w.arr(&[(4, 1)], 12)?;
                w.arr(&[(1, 1)], 2)?;
            }
        }
    }
    if w.u32()? != n {
        return fail("visual terrain: the patch maps do not match the road blocks in number");
    }
    for i in 0..n {
        w.mark(Some(format!("patch map {i}")));
        w.arr(&[(4, 1)], 6)?;
        let at = w.u32s(5)?;
        let cells = u64::from(w.value(at + 16));
        w.mark(Some(format!("patch map {i} cells")));
        w.arr(&[(2, 1)], 4 * cells * cells)?;
        w.arr(&[(1, 1)], 4 * cells * cells)?;
        let w2 = u64::from(w.u32()?);
        w.arr(&[(1, 1)], w2 * w2)?;
        w.mark(Some(format!("patch map {i} names")));
        w.string()?;
        w.string()?;
    }
    Ok(w)
}

/// An Xbox `VisualTerrain` as BFBC2's PC file (`visual_terrain`).
///
/// # Errors
/// See the module.
pub fn visual_terrain(x: &[u8]) -> Res<Vec<u8>> {
    let mut w = walk_visual_terrain(x, true)?;
    w.done()?;
    Ok(w.out)
}

/// A PC `VisualTerrain`'s parts (`visual_terrain_parts`): (name, start, end) each.
///
/// # Errors
/// See the module.
pub fn visual_terrain_parts(pc: &[u8]) -> Res<Vec<Part>> {
    let mut w = walk_visual_terrain(pc, false)?;
    w.done()?;
    Ok(w.parts)
}

/// One road or decal strip of a road block (`road_strips`' dicts).
#[derive(Debug, Clone, PartialEq)]
pub struct Strip<'a> {
    /// Its shader, second shader and mask (latin-1 bytes).
    pub names: [&'a [u8]; 3],
    /// First index and triangle count.
    pub first: u32,
    /// Its triangles.
    pub triangles: u32,
    /// Six floats as stored.
    pub bounds: [f32; 6],
    /// Its layer.
    pub layer: u32,
}

/// A road block: its strips, the vertex bytes and the index bytes (little-endian u32s).
pub type RoadBlock<'a> = (Vec<Strip<'a>>, &'a [u8], &'a [u8]);

/// A PC `VisualTerrain`'s road blocks (`road_strips`).
///
/// # Errors
/// See the module.
pub fn road_strips(pc: &[u8]) -> Res<Vec<RoadBlock<'_>>> {
    let mut w = Walk::new(pc, "visual terrain", false);
    let n = w.u32()?;
    if !(1..=256).contains(&n) {
        return fail(format!("visual terrain: {n} terrain blocks - not a VisualTerrain?"));
    }
    let f32_at = |at: usize| f32::from_le_bytes([pc[at], pc[at + 1], pc[at + 2], pc[at + 3]]);
    let mut out = Vec::new();
    for _ in 0..2 * n {
        let mut strips = Vec::new();
        for _ in 0..w.u32()? {
            let names = [w.string()?, w.string()?, w.string()?];
            let at = w.u32s(2)?;
            let (first, triangles) = (w.value(at), w.value(at + 4));
            let at = w.arr(&[(4, 1)], 6)?;
            let mut bounds = [0f32; 6];
            for (k, b) in bounds.iter_mut().enumerate() {
                *b = f32_at(at + 4 * k);
            }
            let at = w.u32s(1)?;
            strips.push(Strip {
                names,
                first,
                triangles,
                bounds,
                layer: w.value(at),
            });
        }
        let (v, b, idx) = (w.u32()?, w.u32()?, w.u32()?);
        if u64::from(b) != 28 * u64::from(v) {
            return fail(format!(
                "visual terrain: road vertices of {b} bytes for {v} (not 28 each)"
            ));
        }
        let at = w.arr(&ROAD_VERTEX, u64::from(v))?;
        let verts = &pc[at..w.o];
        let at = w.u32s(u64::from(idx))?;
        out.push((strips, verts, &pc[at..w.o]));
    }
    Ok(out)
}

/// `_dec3n` of one value: x, y, z, w as halves (NumPy: doubles stored as floats, the floats cast to halves).
fn dec3n(u: u32) -> [u16; 4] {
    let mut out = [0u16; 4];
    for (i, sh) in [0u32, 10, 20].into_iter().enumerate() {
        let v = ((u >> sh) & 0x3FF) as i32;
        let s = if v >= 512 { v - 1024 } else { v };
        let f = (f64::from(s) / 511.0) as f32;
        out[i] = f64_to_f16(f64::from(f));
    }
    out[3] = f64_to_f16(f64::from(((u >> 30) & 3) as f32));
    if u & 0x3FFF_FFFF == 0 {
        let m = f64_to_f16(-1.5);
        out[0] = m;
        out[1] = m;
        out[2] = m;
    }
    out
}

fn walk_visual_water(x: &[u8], big: bool) -> Res<(u32, usize, Walk<'_>)> {
    let stride: u32 = if big { 28 } else { 36 };
    if x.len() < 12 {
        return fail(format!("visual water: {} bytes", x.len()));
    }
    let rd = |at: usize| {
        let b = [x[at], x[at + 1], x[at + 2], x[at + 3]];
        if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        }
    };
    let (fmt, s, nb) = (rd(0), rd(4), rd(8));
    if fmt != 10 || s != stride || !nb.is_multiple_of(stride) || 12 + nb as usize > x.len() {
        return fail(format!(
            "visual water: vertex format {fmt}, {s} bytes each, {nb} bytes - expected format 10, {stride} bytes ({} file)",
            if big { "an Xbox 360" } else { "a PC" }
        ));
    }
    let mut w = Walk::new(x, "visual water", big);
    w.o = 12 + nb as usize;
    w.mark(Some("indices".into()));
    let ni = w.u32()?;
    if !ni.is_multiple_of(4) {
        return fail(format!("visual water: {ni} bytes of indices, not u32s"));
    }
    w.u32s(u64::from(ni / 4))?;
    w.mark(Some("materials".into()));
    for _ in 0..w.u32()? {
        w.string()?;
        w.string()?;
    }
    w.mark(Some("draw ranges".into()));
    let r = w.u32()?;
    w.u32s(4 * u64::from(r))?;
    w.arr(&[(4, 1)], 1)?;
    w.done()?;
    Ok((fmt, (nb / stride) as usize, w))
}

/// An Xbox `VisualWater` as BFBC2's PC file (`visual_water`): 28-byte vertices become 36 (normal and tangent
/// DEC3N decoded to four halves).
///
/// # Errors
/// See the module.
pub fn visual_water(x: &[u8]) -> Res<Vec<u8>> {
    let (fmt, count, w) = walk_visual_water(x, true)?;
    let mut out = Vec::with_capacity(12 + 36 * count + x.len());
    out.extend_from_slice(&fmt.to_le_bytes());
    out.extend_from_slice(&36u32.to_le_bytes());
    out.extend_from_slice(&((36 * count) as u32).to_le_bytes());
    for k in 0..count {
        let v = &x[12 + 28 * k..12 + 28 * (k + 1)];
        for p in 0..4 {
            out.extend_from_slice(&[v[2 * p + 1], v[2 * p]]);
        }
        for at in [8, 12] {
            let u = u32::from_be_bytes([v[at], v[at + 1], v[at + 2], v[at + 3]]);
            for h in dec3n(u) {
                out.extend_from_slice(&h.to_le_bytes());
            }
        }
        for p in 0..6 {
            out.extend_from_slice(&[v[16 + 2 * p + 1], v[16 + 2 * p]]);
        }
    }
    let a = w.parts.first().map_or(w.out.len(), |p| p.1);
    out.extend_from_slice(&w.out[a..]);
    Ok(out)
}

/// A PC `VisualWater`'s vertex count (36-byte vertices from byte 12) and other parts (`visual_water_parts`).
///
/// # Errors
/// See the module.
pub fn visual_water_parts(pc: &[u8]) -> Res<(usize, Vec<Part>)> {
    let (_, count, w) = walk_visual_water(pc, false)?;
    Ok((count, w.parts))
}

/// An Xbox `SootMesh` as BFBC2's PC file (`soot.soot_mesh`).
///
/// # Errors
/// See the module ([`Refusal::Error`]: `SootError`'s message).
pub fn soot_mesh(x: &[u8]) -> Res<Vec<u8>> {
    let len = x.len();
    let mut out = vec![0u8; len];
    let sw = |out: &mut Vec<u8>, name: &str, width: usize, n: usize, at: usize| -> Res<()> {
        if at as u128 + n as u128 * width as u128 > len as u128 {
            return fail(format!(
                "soot mesh: {n} x {name} at {at:#x} runs past the end ({len} bytes)"
            ));
        }
        swap(x, out, width, n, at, at)
    };
    if len == 16 {
        sw(&mut out, "u4", 4, 4, 0)?;
        return Ok(out);
    }
    let n = be32(x, 0x10)? as usize;
    sw(&mut out, "u4", 4, 5, 0)?;
    sw(&mut out, "f4", 4, 6 * n, 0x14)?;
    let h = 0x14 + 24 * n;
    sw(&mut out, "u4", 4, 4, h)?;
    let (vb, ib) = (be32(x, h + 4)? as usize, be32(x, h + 8)? as usize);
    let mut o = h + 16;
    let stride = be32(x, 4)? as usize;
    if stride == 0 {
        return Err(Refusal::Reference);
    }
    if !vb.is_multiple_of(stride) || !ib.is_multiple_of(2) || o + vb + ib + 256 != len {
        return fail(format!(
            "soot mesh: {vb} vertex and {ib} index bytes do not fit a {len}-byte file"
        ));
    }
    if stride != 12 {
        return fail(format!(
            "soot mesh: {stride}-byte vertices (only the 12-byte one is proven: every BFBC2 soot mesh has it; \
             BF1943's warehouse has 16)"
        ));
    }
    for k in 0..vb / 12 {
        let at = o + 12 * k;
        swap(x, &mut out, 2, 4, at, at)?;
        swap(x, &mut out, 4, 1, at + 8, at + 8)?;
    }
    o += vb;
    sw(&mut out, "u2", 2, ib / 2, o)?;
    sw(&mut out, "u4", 4, 64, o + ib)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quadtree() {
        assert_eq!(quadtree_nodes(128), 85);
        assert_eq!(quadtree_nodes(64), 21);
    }

    #[test]
    fn water_mesh_swaps_words() {
        assert_eq!(water_mesh(&[1, 2, 3, 4]), Ok(vec![4, 3, 2, 1]));
        assert!(matches!(water_mesh(&[1, 2, 3]), Err(Refusal::Error(_))));
    }

    #[test]
    fn empty_soot() {
        let x = [0, 0, 0, 18, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 0];
        let pc = soot_mesh(&x).unwrap_or_default();
        assert_eq!(pc.get(..8), Some(&[18, 0, 0, 0, 12, 0, 0, 0][..]));
    }

    /// A one-block visual terrain (Xbox byte order when `big`) whose one undergrowth entry has `second` as its second
    /// string.
    fn one_block_visual_terrain(second: &[u8], big: bool) -> Vec<u8> {
        let mut x = Vec::new();
        let u = |x: &mut Vec<u8>, v: u32| x.extend(if big { v.to_be_bytes() } else { v.to_le_bytes() });
        let s = |x: &mut Vec<u8>, t: &[u8]| {
            x.extend(t);
            x.push(0);
        };
        u(&mut x, 1);
        for _ in 0..2 {
            for _ in 0..4 {
                u(&mut x, 0); // no strips; V, V * 28, I all 0
            }
        }
        u(&mut x, 1);
        s(&mut x, b"Terrains/T/PatchMaskShaders/a");
        s(&mut x, b"Terrains/T/UgMaskShaders/a");
        u(&mut x, 2);
        u(&mut x, 0);
        u(&mut x, 1);
        u(&mut x, 1);
        s(&mut x, b"Levels/L/undergrowth/grass_Mesh");
        s(&mut x, second);
        s(&mut x, b"");
        for k in 0..12u32 {
            u(&mut x, (k as f32).to_bits());
        }
        x.extend([1, 0]);
        u(&mut x, 1);
        for _ in 0..6 {
            u(&mut x, 0);
        }
        for v in [66, 1, 15, 7, 1] {
            u(&mut x, v);
        }
        for _ in 0..4 {
            x.extend(if big { 3u16.to_be_bytes() } else { 3u16.to_le_bytes() });
        }
        x.extend([8, 0, 0, 0]);
        u(&mut x, 1);
        x.push(0);
        s(&mut x, b"hf");
        s(&mut x, b"atlas");
        x
    }

    // spec: VEG-UG-2, TERR-VT-3
    #[test]
    fn undergrowth_second_string_is_a_string() {
        for second in [&b""[..], b"Levels/L/undergrowth/grass_Mesh/0123456789abcdef"] {
            let xbox = one_block_visual_terrain(second, true);
            let pc = visual_terrain(&xbox).unwrap_or_default();
            assert_eq!(pc, one_block_visual_terrain(second, false), "second string {second:?}");
            let parts = visual_terrain_parts(&pc).unwrap_or_default();
            assert!(parts.iter().any(|p| p.0 == "patch shaders"), "second string {second:?}");
        }
    }

    #[test]
    fn dec3n_zero_vector() {
        assert_eq!(dec3n(0), [f64_to_f16(-1.5), f64_to_f16(-1.5), f64_to_f16(-1.5), 0]);
    }
}
