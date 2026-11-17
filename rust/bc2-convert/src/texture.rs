//! Xbox 360 -> PC textures (`XenonTexture` resources to `ITexture`), ported from
//! `bfbc2-console-convert/texture.py` (`convert`); the format: that folder's `FORMAT.md` section 2.
//!
//! The console keeps a texture's blocks in the Xenos GPU's tiled order (32 x 32-block tiles, the smallest mip levels
//! packed into one tile, cube faces level by level) with 16-bit words byte-swapped; the PC keeps them in rows, level
//! after level (face after face). The blocks are copied as they are, except three console-only formats, which are
//! decoded: DXT5A (3) to the PC's 8-bit format 10, RGB565 (5) and big-endian A8R8G8B8 (9) to B8G8R8A8 (9).
//!
//! | Python (`texture.py`) | here |
//! |---|---|
//! | `convert(b)` | [`convert`] |
//! | `tiled(x, y, pitch, log_bpp)` | [`tiled`] |
//! | `packed(w, h, mip, edge)` | [`packed`] |
//! | `layout(w, h, mips, fmt, slices)` | [`layout`] |
//! | `ConvertError` | [`TextureError::Refused`] (same message) |
//!
//! Every byte is the reference's: the work is integer only. Where the reference would read past the end of the
//! data (a layout that points outside it, which no texture of either game does), the port stops with
//! [`TextureError::OutOfRange`] rather than imitate Python's short slices; the Python binding then runs the
//! reference, so what a caller gets is the reference's answer in every case.
//!
//! ```
//! use bc2_convert::texture;
//!
//! // an 8 x 8 L8 texture, one level: the console's 32 x 32 tile (1,024 bytes, a 4 KB storage)
//! let mut b = Vec::new();
//! for v in [3u32, 0, 10, 0, 8, 8, 0, 1] {
//!     b.extend(v.to_be_bytes());
//! }
//! b.extend([0u8; 60]);
//! b.extend(vec![7u8; 4096]);
//! let pc = texture::convert(&b)?;
//! assert_eq!(pc.len(), 92 + 64);
//! assert!(pc[92..].iter().all(|&v| v == 7));
//! # Ok::<(), texture::TextureError>(())
//! ```

use rayon::prelude::*;
use thiserror::Error;

/// The header: 8 `u32` (version, type, format, flags, width, height, depth, mip count) and 15 level sizes.
pub const HEADER: usize = 92;
/// Texture type 1: a cube map (six faces).
pub const CUBE: u32 = 1;
const DXT5A: u32 = 3;
const RGB565: u32 = 5;
const BGRA8: u32 = 9;
const L8: u32 = 10;
/// Header flag bit 1: DICE's build halves the texture for the console (`FORMAT.md` 2.1).
const HALVED_ON_CONSOLE: u32 = 2;

/// Why a texture is not converted.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TextureError {
    /// The reference's `ConvertError`, with its message.
    #[error("{0}")]
    Refused(String),
    /// The layout reads past the end of the data (the reference reads short slices there; see the module docs).
    #[error("the layout reads past the end of the texture data")]
    OutOfRange,
}

fn refused(msg: String) -> TextureError {
    TextureError::Refused(msg)
}

/// (bytes per block, block edge in texels) of a format code both platforms share; `None` for any other.
#[must_use]
pub fn format(fmt: u32) -> Option<(usize, usize)> {
    Some(match fmt {
        0 | 18 | 3 => (8, 4),
        1 | 2 | 19 | 20 => (16, 4),
        13 => (8, 1),
        5 => (2, 1),
        10 => (1, 1),
        9 => (4, 1),
        _ => return None,
    })
}

fn log2c(v: usize) -> usize {
    let mut n = 0;
    while (1usize << n) < v {
        n += 1;
    }
    n
}

fn align(v: usize, a: usize) -> usize {
    v.div_ceil(a) * a
}

/// Xenos 2D tiling (`XGAddress2DTiledOffset`): block (x, y) of a surface `pitch` blocks wide -> block index.
#[must_use]
pub fn tiled(x: usize, y: usize, pitch: usize, log_bpp: u32) -> usize {
    let aw = (pitch + 31) & !31;
    let macro_ = ((x >> 5) + (y >> 5) * (aw >> 5)) << (log_bpp + 7);
    let micro = ((x & 7) + ((y & 6) << 2)) << log_bpp;
    let off = macro_ + ((micro & !15) << 1) + (micro & 15) + ((y & 8) << (3 + log_bpp)) + ((y & 1) << 4);
    (((off & !511) << 3)
        + ((off & 0x1C0) << 2)
        + (off & 63)
        + ((y & 16) << 7)
        + (((((y & 8) >> 2) + (x >> 3)) & 3) << 6))
        >> log_bpp
}

/// Block offset (x, y) of mip level `mip` inside the packed tail tile, or `None` when the level has its own
/// storage. A level is packed once its shorter side is 16 texels or less.
#[must_use]
pub fn packed(w: usize, h: usize, mip: usize, edge: usize) -> Option<(usize, usize)> {
    let (lw, lh) = (log2c(w), log2c(h));
    let ls = lw.min(lh);
    if ls > 4 + mip {
        return None;
    }
    let base = ls.saturating_sub(4);
    let pm = mip - base;
    let (x, y) = if pm < 3 {
        if lw > lh { (0, 16 >> pm) } else { (16 >> pm, 0) }
    } else if lw > lh {
        ((1usize << (lw - base)) >> (pm - 2), 0)
    } else {
        (0, (1usize << (lh - base)) >> (pm - 2))
    };
    Some((x / edge, y / edge))
}

/// Where a level lies in the console data: its storage's byte start, pitch in blocks, and block offset (x, y).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Place {
    /// Byte start of the level's storage.
    pub start: usize,
    /// Pitch in blocks.
    pub pitch: usize,
    /// Block offset on x (the packed tail tile).
    pub x: usize,
    /// Block offset on y.
    pub y: usize,
}

/// Per slice, per level: where the level lies in the console data; and the data's size. Level 0 of every slice
/// first, then level by level (each level all slices); every storage 4 KB aligned. `None` for a format
/// [`format`] does not know.
#[must_use]
#[allow(clippy::many_single_char_names)] // the reference's names
pub fn layout(w: usize, h: usize, mips: usize, fmt: u32, slices: usize) -> Option<(Vec<Vec<Place>>, usize)> {
    let (bpb, edge) = format(fmt)?;
    let blocks = |v: usize| v.div_ceil(edge).max(1);
    let (bw, bh) = (blocks(w), blocks(h));
    let base_pitch = align(bw, 32);
    let base_size = align(base_pitch * align(bh, 32) * bpb, 4096);
    let mut out: Vec<Vec<Place>> = vec![Vec::with_capacity(mips); slices];
    let mut pos = base_size * slices;
    let mut tail: Option<(usize, usize, usize)> = None;
    for m in 0..mips {
        let pk = packed(w, h, m, edge);
        if m == 0 {
            if pk.is_some() {
                tail = Some((0, base_pitch, base_size));
            }
            let (x, y) = pk.unwrap_or((0, 0));
            for (s, levels) in out.iter_mut().enumerate() {
                levels.push(Place {
                    start: s * base_size,
                    pitch: base_pitch,
                    x,
                    y,
                });
            }
            continue;
        }
        let sbw = blocks(((1usize << log2c(w)) >> m).max(1));
        let sbh = blocks(((1usize << log2c(h)) >> m).max(1));
        let pitch = align(sbw, 32);
        let size = align(pitch * align(sbh, 32) * bpb, 4096);
        match pk {
            None => {
                for (s, levels) in out.iter_mut().enumerate() {
                    levels.push(Place {
                        start: pos + s * size,
                        pitch,
                        x: 0,
                        y: 0,
                    });
                }
                pos += size * slices;
            }
            Some((x, y)) => {
                let t = *tail.get_or_insert_with(|| {
                    let t = (pos, pitch, size);
                    pos += size * slices;
                    t
                });
                for (s, levels) in out.iter_mut().enumerate() {
                    levels.push(Place {
                        start: t.0 + s * t.2,
                        pitch: t.1,
                        x,
                        y,
                    });
                }
            }
        }
    }
    Some((out, pos))
}

fn be32(b: &[u8], i: usize) -> u32 {
    let mut v = [0u8; 4];
    if let Some(s) = b.get(4 * i..4 * i + 4) {
        v.copy_from_slice(s);
    }
    u32::from_be_bytes(v)
}

/// The PC header: 8 `u32` and the 15 level sizes (those after the levels given are 0).
fn header(words: [u32; 8], sizes: &[usize]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER);
    for v in words {
        out.extend(v.to_le_bytes());
    }
    for i in 0..15 {
        out.extend((sizes.get(i).copied().unwrap_or(0) as u32).to_le_bytes());
    }
    out
}

/// 16-bit words byte-swapped, as the console stores them (a last odd byte kept).
fn swap16(data: &mut [u8]) {
    for pair in data.as_chunks_mut::<2>().0 {
        pair.swap(0, 1);
    }
}

/// An 8-bit level (h rows of w bytes) as the PC keeps it: each row padded with zeros to a multiple of 4 bytes.
fn rows4(level: &[u8], w: usize, h: usize) -> Vec<u8> {
    let pitch = w.div_ceil(4) * 4;
    if pitch == w {
        return level.to_vec();
    }
    let mut out = vec![0u8; pitch * h];
    for (row, src) in out.chunks_exact_mut(pitch).zip(level.chunks(w)) {
        for (d, &v) in row.iter_mut().zip(src) {
            *d = v;
        }
    }
    out
}

fn slice(data: &[u8], at: usize, n: usize) -> Result<&[u8], TextureError> {
    data.get(at..at + n).ok_or(TextureError::OutOfRange)
}

fn array<const N: usize>(data: &[u8], at: usize) -> Result<[u8; N], TextureError> {
    data.get(at..at + N)
        .and_then(|s| s.try_into().ok())
        .ok_or(TextureError::OutOfRange)
}

/// A console `XenonTexture` resource -> the PC `ITexture` resource.
///
/// # Errors
/// [`TextureError::Refused`] where the reference raises `ConvertError` (with its message), and
/// [`TextureError::OutOfRange`] where it would read past the data.
pub fn convert(b: &[u8]) -> Result<Vec<u8>, TextureError> {
    if b.len() < HEADER {
        return Err(refused("shorter than a texture header".into()));
    }
    let [ver, typ, fmt, flags, w, h, depth, mips] = std::array::from_fn(|i| be32(b, i));
    if !(0 < w && w <= 16384 && 0 < h && h <= 16384 && (1..=15).contains(&mips)) {
        return Err(refused(format!(
            "invalid texture dimensions {w} x {h} / {mips} mip levels"
        )));
    }
    if mips > 32 - w.max(h).leading_zeros() {
        return Err(refused(format!("{mips} mip levels exceed the {w} x {h} chain")));
    }
    if !(typ == 0 || typ == CUBE) || depth > 1 {
        return Err(refused(format!(
            "texture type {typ} / depth {depth} is not converted (2D and cube maps only)"
        )));
    }
    let t = Texture {
        b,
        ver,
        typ,
        fmt,
        flags,
        w: w as usize,
        h: h as usize,
        depth,
        mips: mips as usize,
    };
    match fmt {
        DXT5A => t.dxt5a(),
        RGB565 => t.rgb565(),
        BGRA8 => t.argb8(),
        _ => t.blocks(),
    }
}

/// Every texture of `items` converted on all cores (rayon's pool), in order.
#[must_use]
pub fn convert_all(items: &[&[u8]]) -> Vec<Result<Vec<u8>, TextureError>> {
    items.par_iter().map(|b| convert(b)).collect()
}

struct Texture<'a> {
    b: &'a [u8],
    ver: u32,
    typ: u32,
    fmt: u32,
    flags: u32,
    w: usize,
    h: usize,
    depth: u32,
    mips: usize,
}

impl Texture<'_> {
    /// The console's data after the header, checked against the layout's size.
    fn data(&self, size: usize) -> Result<&[u8], TextureError> {
        let data = self.b.get(HEADER..).unwrap_or_default();
        if data.len() < size {
            return Err(refused(format!(
                "{} bytes of texture data, the layout needs {size}",
                data.len()
            )));
        }
        Ok(data)
    }

    fn only_2d(&self) -> Result<(), TextureError> {
        if self.typ == 0 {
            Ok(())
        } else {
            Err(refused(format!(
                "texture type {} in console format {} is not converted (2D only)",
                self.typ, self.fmt
            )))
        }
    }

    fn layout(&self, fmt: u32, slices: usize) -> Result<(Vec<Vec<Place>>, usize), TextureError> {
        layout(self.w, self.h, self.mips, fmt, slices).ok_or(TextureError::OutOfRange)
    }

    fn level_size(&self, m: usize) -> (usize, usize) {
        ((self.w >> m).max(1), (self.h >> m).max(1))
    }

    /// Console 3 (DXT5A: DXT5's alpha half alone) -> the PC's 8-bit format 10, decoded level by level.
    fn dxt5a(&self) -> Result<Vec<u8>, TextureError> {
        self.only_2d()?;
        let (where_, size) = self.layout(DXT5A, 1)?;
        let mut data = self.data(size)?.to_vec();
        swap16(&mut data);
        let mut sizes = Vec::with_capacity(self.mips);
        let mut body = Vec::new();
        for (m, p) in where_.first().into_iter().flatten().enumerate() {
            let (lw, lh) = self.level_size(m);
            let (bw, bh) = (lw.div_ceil(4).max(1), lh.div_ceil(4).max(1));
            let iw = bw * 4;
            let mut img = vec![0u8; iw * bh * 4];
            for y in 0..bh {
                for x in 0..bw {
                    let s = p.start + tiled(x + p.x, y + p.y, p.pitch, 3) * 8;
                    let px = dxt5a_block(array(&data, s)?);
                    for (r, row) in px.as_chunks::<4>().0.iter().enumerate() {
                        let at = (y * 4 + r) * iw + x * 4;
                        if let Some(dst) = img.get_mut(at..at + 4) {
                            dst.copy_from_slice(row);
                        }
                    }
                }
            }
            let mut level = Vec::with_capacity(lw * lh);
            for row in img.chunks_exact(iw).take(lh) {
                level.extend(row.iter().take(lw));
            }
            let level = rows4(&level, lw, lh);
            sizes.push(level.len());
            body.extend(level);
        }
        let words = [
            self.ver,
            self.typ,
            L8,
            self.flags & !HALVED_ON_CONSOLE,
            self.w as u32,
            self.h as u32,
            0,
            self.mips as u32,
        ];
        Ok([header(words, &sizes), body].concat())
    }

    /// Console 5 (RGB565) -> the PC's format 9 (bytes blue, green, red, alpha 255).
    fn rgb565(&self) -> Result<Vec<u8>, TextureError> {
        self.only_2d()?;
        let (where_, size) = self.layout(RGB565, 1)?;
        let mut data = self.data(size)?.to_vec();
        swap16(&mut data);
        let mut sizes = Vec::with_capacity(self.mips);
        let mut body = Vec::new();
        for (m, p) in where_.first().into_iter().flatten().enumerate() {
            let (lw, lh) = self.level_size(m);
            let mut level = Vec::with_capacity(lw * lh * 4);
            for y in 0..lh {
                for x in 0..lw {
                    let s = p.start + tiled(x + p.x, y + p.y, p.pitch, 1) * 2;
                    let [lo, hi] = array(&data, s)?;
                    let v = u32::from(lo) | (u32::from(hi) << 8);
                    let (r5, g6, b5) = ((v >> 11) & 31, (v >> 5) & 63, v & 31);
                    level.extend([
                        ((b5 << 3) | (b5 >> 2)) as u8,
                        ((g6 << 2) | (g6 >> 4)) as u8,
                        ((r5 << 3) | (r5 >> 2)) as u8,
                        255,
                    ]);
                }
            }
            sizes.push(level.len());
            body.extend(level);
        }
        let words = [
            self.ver,
            self.typ,
            BGRA8,
            self.flags,
            self.w as u32,
            self.h as u32,
            0,
            self.mips as u32,
        ];
        Ok([header(words, &sizes), body].concat())
    }

    /// Console 9 (big-endian A8R8G8B8 words, untiled as texels, no 16-bit swap) -> the PC's format 9.
    fn argb8(&self) -> Result<Vec<u8>, TextureError> {
        self.only_2d()?;
        let (where_, size) = self.layout(BGRA8, 1)?;
        let all = self.b.get(HEADER..).unwrap_or_default();
        let data = all.get(..size).unwrap_or(all);
        if data.len() < size {
            return Err(refused(format!(
                "{} bytes of texture data, the layout needs {size}",
                all.len()
            )));
        }
        let mut sizes = Vec::with_capacity(self.mips);
        let mut body = Vec::new();
        for (m, p) in where_.first().into_iter().flatten().enumerate() {
            let (lw, lh) = self.level_size(m);
            let mut level = Vec::with_capacity(lw * lh * 4);
            for y in 0..lh {
                for x in 0..lw {
                    let s = p.start + tiled(x + p.x, y + p.y, p.pitch, 2) * 4;
                    let [alpha, red, green, blue] = array(data, s)?;
                    level.extend([blue, green, red, alpha]);
                }
            }
            sizes.push(level.len());
            body.extend(level);
        }
        let words = [
            self.ver,
            self.typ,
            BGRA8,
            self.flags,
            self.w as u32,
            self.h as u32,
            0,
            self.mips as u32,
        ];
        Ok([header(words, &sizes), body].concat())
    }

    /// The block formats (and L8, RGBA16F): blocks copied in rows, level after level, face after face.
    fn blocks(&self) -> Result<Vec<u8>, TextureError> {
        let Some((bpb, edge)) = format(self.fmt) else {
            return Err(refused(format!(
                "texture format {} is not converted (FORMAT.md 2.4)",
                self.fmt
            )));
        };
        let log_bpp = match bpb {
            1 => 0,
            8 => 3,
            16 => 4,
            _ => return Err(TextureError::OutOfRange), // 2 and 4 bytes per block: the decoded formats, handled above
        };
        let slices = if self.typ == CUBE { 6 } else { 1 };
        let (where_, size) = self.layout(self.fmt, slices)?;
        let mut data = self.data(size)?.to_vec();
        if bpb > 1 {
            swap16(&mut data);
        }
        let mut sizes = Vec::with_capacity(self.mips);
        let mut body = Vec::new();
        for (s, levels) in where_.iter().enumerate() {
            for (m, p) in levels.iter().enumerate() {
                let (lw, lh) = self.level_size(m);
                let (lbw, lbh) = (lw.div_ceil(edge).max(1), lh.div_ceil(edge).max(1));
                let mut lev = Vec::with_capacity(lbw * lbh * bpb);
                for y in 0..lbh {
                    for x in 0..lbw {
                        let src = p.start + tiled(x + p.x, y + p.y, p.pitch, log_bpp) * bpb;
                        lev.extend_from_slice(slice(&data, src, bpb)?);
                    }
                }
                let lev = if bpb == 1 { rows4(&lev, lbw, lbh) } else { lev };
                if s == 0 {
                    sizes.push(lev.len());
                }
                body.extend(lev);
            }
        }
        let words = [
            self.ver,
            self.typ,
            self.fmt,
            self.flags,
            self.w as u32,
            self.h as u32,
            self.depth,
            self.mips as u32,
        ];
        Ok([header(words, &sizes), body].concat())
    }
}

/// One 8-byte single-channel block (two end values, 16 x 3-bit indices) -> 4 x 4 bytes, rows first.
fn dxt5a_block(block: [u8; 8]) -> [u8; 16] {
    let [a0, a1, rest @ ..] = block;
    let (a0, a1) = (u32::from(a0), u32::from(a1));
    let bits = rest
        .iter()
        .enumerate()
        .fold(0u64, |bits, (k, &v)| bits | (u64::from(v) << (8 * k)));
    let pal: [u32; 8] = std::array::from_fn(|i| {
        let i = i as u32;
        match i {
            0 => a0,
            1 => a1,
            _ if a0 > a1 => ((8 - i) * a0 + (i - 1) * a1) / 7,
            2..=5 => ((6 - i) * a0 + (i - 1) * a1) / 5,
            6 => 0,
            _ => 255,
        }
    });
    std::array::from_fn(|i| pal.get(((bits >> (3 * i)) & 7) as usize).copied().unwrap_or(0) as u8)
}

#[cfg(test)]
#[path = "texture_tests.rs"]
mod tests;
