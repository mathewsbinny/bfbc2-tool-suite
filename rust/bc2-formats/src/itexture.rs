//! PC `ITexture` resources written from pixels (round TG2): the header (version 3, type 0, format, flags, width,
//! height, depth 0, level count, 15 level sizes: 92 bytes; `bfbc2-console-convert/FORMAT.md` section 2), the mip
//! chain made by 2 x 2 box filtering, and the two formats DICE's terrain colour / occlusion maps use (LIGHT-BAKE-2):
//! DXT1 (format 0: 8 bytes a 4 x 4 block, four-colour blocks) and L8 (format 10: one byte a texel, a row padded to 4
//! bytes). One encoder for the generated terrain's map (`bc2-terragen`) and the editor's occlusion re-bake (E5-rest).
//!
//! DICE's own maps are the proof: a level size list equal to DICE's on every terrain map of the game copy, and DICE's
//! map decoded, encoded here and decoded again within a few levels of 255 (`tests/itexture_corpus.rs`).
//!
//! ```
//! use bc2_formats::itexture::{self, Format, Picture};
//!
//! let pic = Picture { w: 8, h: 8, rgb: vec![[200, 100, 50]; 64] };
//! let res = itexture::encode(&pic, Format::Dxt1, 2, true);
//! let t = itexture::Header::read(&res).unwrap();
//! assert_eq!((t.format, t.width, t.levels), (0, 8, 4));
//! let back = itexture::decode_level(&res, 0).unwrap();
//! assert!(back.rgb.iter().all(|p| p[0].abs_diff(200) <= 4 && p[2].abs_diff(50) <= 4));
//! ```

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    reason = "indices into fixed-size arrays (a block's 16 texels, 4 palette colours, 3 channels, 8 bytes) bounded by               their loops; a picture's w, h, x, y and a colour's r, g, b as the formats name them"
)]

/// The header's size.
pub const HEADER: usize = 92;

/// A texture format this module writes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Format 0: DXT1 (BC1), opaque four-colour blocks.
    Dxt1,
    /// Format 10: L8, one byte a texel (the red channel).
    L8,
}

impl Format {
    /// The header's format word.
    #[must_use]
    pub const fn code(self) -> u32 {
        match self {
            Self::Dxt1 => 0,
            Self::L8 => 10,
        }
    }

    /// The format of a header's word (`None` for the formats not written here).
    #[must_use]
    pub const fn from_code(c: u32) -> Option<Self> {
        match c {
            0 => Some(Self::Dxt1),
            10 => Some(Self::L8),
            _ => None,
        }
    }

    /// The bytes of a level of `w` x `h` texels as DICE stores it: DXT1 whole 4 x 4 blocks; L8 rows padded to 4
    /// bytes (texwrite's `l8_bytes`: `MP_005`'s 2 x 2 level is 8 bytes).
    #[must_use]
    pub const fn level_bytes(self, w: u32, h: u32) -> usize {
        match self {
            Self::Dxt1 => (w.div_ceil(4) * h.div_ceil(4) * 8) as usize,
            Self::L8 => (w.next_multiple_of(4) * h) as usize,
        }
    }
}

/// A picture: `h` rows of `w` RGB texels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// The texels, row by row.
    pub rgb: Vec<[u8; 3]>,
}

impl Picture {
    /// The texel at (`x`, `y`), clamped to the picture.
    #[must_use]
    pub fn at(&self, x: usize, y: usize) -> [u8; 3] {
        let (x, y) = (x.min(self.w.saturating_sub(1)), y.min(self.h.saturating_sub(1)));
        self.rgb.get(y * self.w + x).copied().unwrap_or_default()
    }

    /// The next mip level: each texel the rounded mean of the 2 x 2 texels under it (a side of 1 stays 1).
    #[must_use]
    pub fn half(&self) -> Self {
        let (w, h) = ((self.w / 2).max(1), (self.h / 2).max(1));
        let mut rgb = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let mut s = [0u32; 3];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let p = self.at(2 * x + dx, 2 * y + dy);
                    for c in 0..3 {
                        s[c] += u32::from(p[c]);
                    }
                }
                rgb.push(s.map(|v| ((v + 2) / 4) as u8));
            }
        }
        Self { w, h, rgb }
    }
}

/// An `ITexture`'s header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    /// The format word.
    pub format: u32,
    /// The flags word (0 - 3 on DICE's terrain maps).
    pub flags: u32,
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// Mip levels.
    pub levels: u32,
    /// The levels' sizes in bytes (15 slots).
    pub sizes: [u32; 15],
}

fn word(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

impl Header {
    /// Reads a PC texture's header (version 3, type 0).
    #[must_use]
    pub fn read(d: &[u8]) -> Option<Self> {
        if word(d, 0)? != 3 || word(d, 4)? != 0 {
            return None;
        }
        let mut sizes = [0u32; 15];
        for (k, s) in sizes.iter_mut().enumerate() {
            *s = word(d, 32 + 4 * k)?;
        }
        Some(Self {
            format: word(d, 8)?,
            flags: word(d, 12)?,
            width: word(d, 16)?,
            height: word(d, 20)?,
            levels: word(d, 28)?,
            sizes,
        })
    }

    /// Where level `k` starts in the file.
    #[must_use]
    pub fn level_start(&self, k: usize) -> usize {
        HEADER + self.sizes.iter().take(k).map(|s| *s as usize).sum::<usize>()
    }
}

/// The level sizes of a full chain of `levels` from `w` x `h` (each level half the one before, at least 1).
#[must_use]
pub fn level_sizes(f: Format, w: u32, h: u32, levels: u32) -> Vec<usize> {
    (0..levels)
        .map(|k| f.level_bytes((w >> k).max(1), (h >> k).max(1)))
        .collect()
}

/// The full chain's level count: down to 1 x 1.
#[must_use]
pub fn full_chain(w: u32, h: u32) -> u32 {
    w.max(h).max(1).ilog2() + 1
}

// ---------------------------------------------------------------------------------------------------------- DXT1

fn to565(c: [i32; 3]) -> u16 {
    let r = ((c[0].clamp(0, 255) * 31 + 127) / 255) as u16;
    let g = ((c[1].clamp(0, 255) * 63 + 127) / 255) as u16;
    let b = ((c[2].clamp(0, 255) * 31 + 127) / 255) as u16;
    (r << 11) | (g << 5) | b
}

fn from565(c: u16) -> [i32; 3] {
    let (r, g, b) = (i32::from(c >> 11), i32::from((c >> 5) & 63), i32::from(c & 31));
    [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (b << 3) | (b >> 2)]
}

/// The four colours of a four-colour block (`c0 > c1`), or the three and black of the other mode.
fn palette(c0: u16, c1: u16) -> [[i32; 3]; 4] {
    let (a, b) = (from565(c0), from565(c1));
    if c0 > c1 {
        [
            a,
            b,
            [0, 1, 2].map(|k| (2 * a[k] + b[k]) / 3),
            [0, 1, 2].map(|k| (a[k] + 2 * b[k]) / 3),
        ]
    } else {
        [a, b, [0, 1, 2].map(|k| i32::midpoint(a[k], b[k])), [0, 0, 0]]
    }
}

fn dist(a: [i32; 3], b: [i32; 3]) -> i32 {
    (0..3).map(|k| (a[k] - b[k]).pow(2)).sum()
}

/// One opaque DXT1 block from its 16 texels (row by row): the endpoints the texels' extremes along their widest
/// channel range span (inset by a sixteenth), then each texel the nearest of the four colours; a flat block is one
/// colour. Integer arithmetic only: the same bytes on every machine.
#[must_use]
pub fn dxt1_block(px: &[[u8; 3]; 16]) -> [u8; 8] {
    let p: Vec<[i32; 3]> = px.iter().map(|c| c.map(i32::from)).collect();
    let lo = [0, 1, 2].map(|k| p.iter().map(|c| c[k]).min().unwrap_or(0));
    let hi = [0, 1, 2].map(|k| p.iter().map(|c| c[k]).max().unwrap_or(0));
    // the axis: the widest channel's direction, with the others' covariance sign
    let axis_k = (0..3).max_by_key(|&k| (hi[k] - lo[k], 3 - k)).unwrap_or(0);
    let mean = [0, 1, 2].map(|k| p.iter().map(|c| c[k]).sum::<i32>() / 16);
    let mut dir = [0i32; 3];
    for k in 0..3 {
        let cov: i64 = p
            .iter()
            .map(|c| i64::from(c[k] - mean[k]) * i64::from(c[axis_k] - mean[axis_k]))
            .sum();
        dir[k] = if k == axis_k || cov >= 0 { 1 } else { -1 };
    }
    let mut e0 = [0i32; 3];
    let mut e1 = [0i32; 3];
    for k in 0..3 {
        let (a, b) = if dir[k] > 0 { (hi[k], lo[k]) } else { (lo[k], hi[k]) };
        let inset = (a - b) / 16;
        e0[k] = a - inset;
        e1[k] = b + inset;
    }
    let (mut c0, mut c1) = (to565(e0), to565(e1));
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
    }
    let mut out = [0u8; 8];
    out[..2].copy_from_slice(&c0.to_le_bytes());
    out[2..4].copy_from_slice(&c1.to_le_bytes());
    if c0 == c1 {
        return out; // every index 0: the one colour
    }
    let pal = palette(c0, c1);
    let mut bits = 0u32;
    for (i, c) in p.iter().enumerate() {
        let best = (0..4u32).min_by_key(|&k| (dist(*c, pal[k as usize]), k)).unwrap_or(0);
        bits |= best << (2 * i);
    }
    out[4..].copy_from_slice(&bits.to_le_bytes());
    out
}

/// A DXT1 block's 16 texels (row by row).
#[must_use]
pub fn dxt1_unblock(b: &[u8; 8]) -> [[u8; 3]; 16] {
    let c0 = u16::from_le_bytes([b[0], b[1]]);
    let c1 = u16::from_le_bytes([b[2], b[3]]);
    let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let pal = palette(c0, c1);
    std::array::from_fn(|i| pal[((bits >> (2 * i)) & 3) as usize].map(|v| v.clamp(0, 255) as u8))
}

/// A level's bytes in `f` (DXT1 blocks row by row, edge texels repeated into partial blocks; L8 the red channel).
#[must_use]
pub fn encode_level(pic: &Picture, f: Format) -> Vec<u8> {
    match f {
        Format::L8 => {
            let stride = pic.w.next_multiple_of(4);
            let mut out = vec![0u8; stride * pic.h];
            for y in 0..pic.h {
                for x in 0..pic.w {
                    if let Some(v) = out.get_mut(y * stride + x) {
                        *v = pic.at(x, y)[0];
                    }
                }
            }
            out
        }
        Format::Dxt1 => {
            let (bw, bh) = (pic.w.div_ceil(4), pic.h.div_ceil(4));
            let mut out = Vec::with_capacity(bw * bh * 8);
            for by in 0..bh {
                for bx in 0..bw {
                    let block: [[u8; 3]; 16] = std::array::from_fn(|i| pic.at(4 * bx + i % 4, 4 * by + i / 4));
                    out.extend(dxt1_block(&block));
                }
            }
            out
        }
    }
}

/// A whole resource: `pic` as level 0 and, with `mips`, the full chain down to 1 x 1, under `flags`.
#[must_use]
pub fn encode(pic: &Picture, f: Format, flags: u32, mips: bool) -> Vec<u8> {
    let count = if mips {
        full_chain(pic.w as u32, pic.h as u32)
    } else {
        1
    };
    let mut levels = Vec::with_capacity(count as usize);
    let mut cur = pic.clone();
    for k in 0..count {
        if k > 0 {
            cur = cur.half();
        }
        levels.push(encode_level(&cur, f));
    }
    let mut out = Vec::with_capacity(HEADER + levels.iter().map(Vec::len).sum::<usize>());
    for v in [3, 0, f.code(), flags, pic.w as u32, pic.h as u32, 0, count] {
        out.extend(v.to_le_bytes());
    }
    for k in 0..15 {
        out.extend((levels.get(k).map_or(0, Vec::len) as u32).to_le_bytes());
    }
    for l in &levels {
        out.extend(l);
    }
    out
}

/// Level `k` of a DXT1 or L8 resource decoded (L8 into all three channels); `None` for another format or a short
/// file.
#[must_use]
pub fn decode_level(d: &[u8], k: usize) -> Option<Picture> {
    let h = Header::read(d)?;
    let f = Format::from_code(h.format)?;
    let (w, hh) = ((h.width >> k).max(1) as usize, (h.height >> k).max(1) as usize);
    let at = h.level_start(k);
    let bytes = d.get(at..at + f.level_bytes(w as u32, hh as u32))?;
    let mut rgb = vec![[0u8; 3]; w * hh];
    match f {
        Format::L8 => {
            let stride = w.next_multiple_of(4);
            for y in 0..hh {
                for x in 0..w {
                    let v = *bytes.get(y * stride + x)?;
                    *rgb.get_mut(y * w + x)? = [v; 3];
                }
            }
        }
        Format::Dxt1 => {
            let bw = w.div_ceil(4);
            for (bi, b) in bytes.as_chunks::<8>().0.iter().enumerate() {
                let (bx, by) = (bi % bw, bi / bw);
                let px = dxt1_unblock(b);
                for (i, p) in px.iter().enumerate() {
                    let (x, y) = (4 * bx + i % 4, 4 * by + i / 4);
                    if x < w && y < hh {
                        *rgb.get_mut(y * w + x)? = *p;
                    }
                }
            }
        }
    }
    Some(Picture { w, h: hh, rgb })
}

/// A rectangle of texels: columns `x0 .. x1`, rows `y0 .. y1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// First column.
    pub x0: usize,
    /// First row.
    pub y0: usize,
    /// One past the last column.
    pub x1: usize,
    /// One past the last row.
    pub y1: usize,
}

impl Rect {
    /// The rectangle a level lower in the chain covers (halved outwards).
    #[must_use]
    pub fn half(self) -> Self {
        Self {
            x0: self.x0 / 2,
            y0: self.y0 / 2,
            x1: self.x1.div_ceil(2),
            y1: self.y1.div_ceil(2),
        }
    }

    /// Whether it covers no texel.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.x1 <= self.x0 || self.y1 <= self.y0
    }
}

/// A DXT1 or L8 resource with level 0's texels in `region` replaced by `pic`'s (`pic` the whole level 0) and each
/// lower level remade over the part the region reaches (2 x 2 box filtering of the level above as it now is), every
/// other byte kept (round E5-rest: the editor's colour / occlusion paint and re-bake, TEDIT-OCC-1). DXT1 re-encodes
/// only the 4 x 4 blocks the region touches, from the texels as they decode with the region's replaced. `None` when
/// the resource is not DXT1 or L8, or `pic` is not its level 0's size.
#[must_use]
pub fn splice(res: &[u8], pic: &Picture, region: Rect) -> Option<Vec<u8>> {
    let h = Header::read(res)?;
    let f = Format::from_code(h.format)?;
    if (pic.w, pic.h) != (h.width as usize, h.height as usize) {
        return None;
    }
    let mut out = res.to_vec();
    let mut region = Rect {
        x0: region.x0.min(pic.w),
        y0: region.y0.min(pic.h),
        x1: region.x1.min(pic.w),
        y1: region.y1.min(pic.h),
    };
    let mut cur: Option<Picture> = None;
    for k in 0..h.levels as usize {
        if region.is_empty() {
            break;
        }
        // this level as it decodes, with the region's texels the new ones (level 0: the picture's; below: the box
        // filter of the level above as it now is)
        let mut lvl = decode_level(res, k)?;
        let made = match &cur {
            Some(above) => above.half(),
            None => pic.clone(),
        };
        for y in region.y0..region.y1 {
            for x in region.x0..region.x1 {
                let v = made.at(x, y);
                if let Some(t) = lvl.rgb.get_mut(y * lvl.w + x) {
                    *t = v;
                }
            }
        }
        let start = h.level_start(k);
        match f {
            Format::L8 => {
                let stride = lvl.w.next_multiple_of(4);
                for y in region.y0..region.y1 {
                    for x in region.x0..region.x1 {
                        if let Some(b) = out.get_mut(start + y * stride + x) {
                            *b = lvl.at(x, y)[0];
                        }
                    }
                }
            }
            Format::Dxt1 => {
                let bw = lvl.w.div_ceil(4);
                for by in region.y0 / 4..region.y1.div_ceil(4) {
                    for bx in region.x0 / 4..region.x1.div_ceil(4) {
                        let block: [[u8; 3]; 16] = std::array::from_fn(|i| lvl.at(4 * bx + i % 4, 4 * by + i / 4));
                        let at = start + (by * bw + bx) * 8;
                        out.get_mut(at..at + 8)?.copy_from_slice(&dxt1_block(&block));
                    }
                }
            }
        }
        cur = Some(lvl);
        region = region.half();
    }
    Some(out)
}

/// A colour as the 16-bit 5:6:5 value a raster keeps (the editor's colour / occlusion raster, round E5-rest).
#[must_use]
pub fn pack565(c: [u8; 3]) -> u16 {
    to565(c.map(i32::from))
}

/// A 16-bit 5:6:5 value as a colour (each channel's bits repeated into its low bits, as DXT1 decodes them).
#[must_use]
pub fn unpack565(v: u16) -> [u8; 3] {
    from565(v).map(|c| c.clamp(0, 255) as u8)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_splice_keeps_every_byte_outside_its_region() {
        let (w, h) = (64usize, 64usize);
        let rgb: Vec<[u8; 3]> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 3) as u8, (y * 3) as u8, 90]
            })
            .collect();
        let pic = Picture { w, h, rgb };
        for f in [Format::Dxt1, Format::L8] {
            let res = encode(&pic, f, 2, true);
            let none = Rect {
                x0: 0,
                y0: 0,
                x1: 0,
                y1: 0,
            };
            assert_eq!(splice(&res, &decode_level(&res, 0).unwrap(), none).unwrap(), res);
            let mut dark = decode_level(&res, 0).unwrap();
            for y in 8..16 {
                for x in 8..16 {
                    dark.rgb[y * w + x] = [0, 0, 0];
                }
            }
            let r = Rect {
                x0: 8,
                y0: 8,
                x1: 16,
                y1: 16,
            };
            let out = splice(&res, &dark, r).unwrap();
            assert_eq!(out.len(), res.len());
            let back = decode_level(&out, 0).unwrap();
            assert_eq!(back.at(10, 10), [0, 0, 0]);
            assert_eq!(back.at(40, 40), decode_level(&res, 0).unwrap().at(40, 40));
            // level 0's bytes outside the region's blocks unchanged; level 1 darker under it
            let hd = Header::read(&res).unwrap();
            let l0 = hd.level_start(0)..hd.level_start(1);
            let changed = out[l0.clone()].iter().zip(&res[l0]).filter(|(a, b)| a != b).count();
            let most = if f == Format::Dxt1 { 4 * 8 } else { 64 };
            assert!(changed > 0 && changed <= most, "{f:?}: {changed}");
            assert!(decode_level(&out, 1).unwrap().at(5, 5)[0] < 30);
        }
        assert_eq!(unpack565(pack565([255, 0, 128]))[0], 255);
    }

    #[test]
    fn a_gradient_round_trips_closely_in_both_formats() {
        let (w, h) = (64usize, 32usize);
        let rgb: Vec<[u8; 3]> = (0..w * h)
            .map(|i| {
                let (x, y) = (i % w, i / w);
                [(x * 4) as u8, (y * 8) as u8, ((x + y) * 2) as u8]
            })
            .collect();
        let pic = Picture { w, h, rgb };
        for f in [Format::Dxt1, Format::L8] {
            let res = encode(&pic, f, 2, true);
            let hd = Header::read(&res).unwrap();
            assert_eq!((hd.levels, hd.flags), (7, 2));
            let sizes = level_sizes(f, 64, 32, 7);
            assert_eq!(hd.sizes[..7].iter().map(|s| *s as usize).collect::<Vec<_>>(), sizes);
            assert_eq!(res.len(), HEADER + sizes.iter().sum::<usize>());
            let back = decode_level(&res, 0).unwrap();
            let worst = back
                .rgb
                .iter()
                .zip(&pic.rgb)
                .map(|(a, b)| a[0].abs_diff(b[0]))
                .max()
                .unwrap();
            assert!(worst <= if f == Format::L8 { 0 } else { 12 }, "{f:?}: {worst}");
            assert_eq!(decode_level(&res, 6).unwrap().w, 1);
        }
        // DICE's small L8 levels: a 2 x 2 level is 8 bytes
        assert_eq!(Format::L8.level_bytes(2, 2), 8);
        assert_eq!(Format::Dxt1.level_bytes(1, 1), 8);
    }

    #[test]
    fn flat_blocks_are_one_colour() {
        let b = dxt1_block(&[[10, 20, 30]; 16]);
        let px = dxt1_unblock(&b);
        assert!(px.iter().all(|p| p[0].abs_diff(10) <= 4 && p[1].abs_diff(20) <= 2));
    }
}
