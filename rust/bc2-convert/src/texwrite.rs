//! PC textures from pictures, ported from `bfbc2-model-tools/texwrite.py` (round F2a): the mip levels (`levels`,
//! `_down`), the DXT1 colour blocks (`_color_blocks`, with and without 1-bit alpha), DXT5's alpha blocks
//! (`_alpha_blocks`), the decoders of the alpha (`alpha1`, `alpha5`) and the 8-bit atlas step (`l8_down`).
//!
//! Every value is the reference's to the bit: the sums are NumPy's own orders (a block's mean and covariance summed
//! texel by texel from the first; a projection on the axis `(x*a + y*b) + z*c`, as `einsum` sums with the axis
//! NumPy's eigensolver returns; a squared error over three channels `(r + g) + b`), each rounding as NumPy's
//! (`np.round`: half to even). One step stays NumPy's: the principal axis of each block's covariance is
//! `numpy.linalg.eigh`'s (LAPACK `dsyevd` through OpenBLAS, whose kernels' use of fused multiply-adds depends on
//! the processor, so its last bits are not portable): [`Prepared::covariances`] gives the matrices (exact sums of
//! products of the texels' integers, or of the filled-in means of a block with transparent texels), the caller
//! solves them, and [`Prepared::finish`] encodes with the axes.

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

/// A picture: `h` rows of `w` pixels of `ch` channels (`uint8`, row-major).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Rows.
    pub h: usize,
    /// Columns.
    pub w: usize,
    /// Channels.
    pub ch: usize,
    /// The pixels.
    pub px: Vec<u8>,
}

/// NumPy's `np.round` on a double (half to even).
fn round(v: f64) -> f64 {
    v.round_ties_even()
}

/// `np.clip(v, lo, hi)`.
fn clip(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

/// One mip step (`_down`): 2 x 2 box filter (rows first, then columns), normals renormalised (3 channels).
fn down(img: &Image, normal: bool) -> Option<Image> {
    let (mut h, mut w, ch) = (img.h, img.w, img.ch);
    let mut a: Vec<f64> = img.px.iter().map(|&v| f64::from(v)).collect();
    if h > 1 {
        if !h.is_multiple_of(2) {
            return None;
        }
        let mut b = vec![0f64; (h / 2) * w * ch];
        for y in 0..h / 2 {
            for x in 0..w * ch {
                b[y * w * ch + x] = (a[2 * y * w * ch + x] + a[(2 * y + 1) * w * ch + x]) / 2.0;
            }
        }
        a = b;
        h /= 2;
    }
    if w > 1 {
        if !w.is_multiple_of(2) {
            return None;
        }
        let mut b = vec![0f64; h * (w / 2) * ch];
        for y in 0..h {
            for x in 0..w / 2 {
                for c in 0..ch {
                    b[(y * (w / 2) + x) * ch + c] =
                        (a[(y * w + 2 * x) * ch + c] + a[(y * w + 2 * x + 1) * ch + c]) / 2.0;
                }
            }
        }
        a = b;
        w /= 2;
    }
    if normal {
        if ch != 3 {
            return None;
        }
        for p in a.chunks_exact_mut(3) {
            let n = [p[0] / 127.5 - 1.0, p[1] / 127.5 - 1.0, p[2] / 127.5 - 1.0];
            let norm = ((n[0] * n[0] + n[1] * n[1]) + n[2] * n[2]).sqrt();
            let d = norm.max(1e-6);
            for (q, v) in p.iter_mut().zip(n) {
                *q = (v / d + 1.0) * 127.5;
            }
        }
    }
    Some(Image {
        h,
        w,
        ch,
        px: a.iter().map(|&v| clip(round(v), 0.0, 255.0) as u8).collect(),
    })
}

/// The picture's first `channels` channels and its mip levels down to 1 x 1 (`levels`); `None` where the reference
/// would fail (fewer channels, odd sides) or normals of other than three channels.
#[must_use]
pub fn levels(img: &Image, normal: bool, channels: usize) -> Option<Vec<Image>> {
    if img.ch < channels || img.px.len() != img.h * img.w * img.ch {
        return None;
    }
    let mut first = Vec::with_capacity(img.h * img.w * channels);
    for p in img.px.chunks_exact(img.ch.max(1)) {
        first.extend_from_slice(&p[..channels]);
    }
    let mut out = vec![Image {
        h: img.h,
        w: img.w,
        ch: channels,
        px: first,
    }];
    while let Some(last) = out.last()
        && last.h.max(last.w) > 1
    {
        let next = down(last, normal)?;
        out.push(next);
    }
    Some(out)
}

/// `_blocks(img, channels)`: 4 x 4 blocks (edge-padded to whole blocks, at least one), texels row by row; the
/// blocks on all cores.
fn blocks(img: &Image) -> Vec<[[f64; 4]; 16]> {
    use rayon::prelude::*;
    let ph = img.h.div_ceil(4).max(1) * 4;
    let pw = img.w.div_ceil(4).max(1) * 4;
    let (bw, n) = (pw / 4, (ph / 4) * (pw / 4));
    (0..n)
        .into_par_iter()
        .map(|i| {
            let (by, bx) = (i / bw, i % bw);
            std::array::from_fn(|t| {
                let y = (by * 4 + t / 4).min(img.h.saturating_sub(1));
                let x = (bx * 4 + t % 4).min(img.w.saturating_sub(1));
                let mut v = [0f64; 4];
                for (c, s) in v.iter_mut().enumerate().take(img.ch) {
                    *s = f64::from(img.px[(y * img.w + x) * img.ch + c]);
                }
                v
            })
        })
        .collect()
}

type Rgb = [[f64; 3]; 16];

fn mean(b: &Rgb) -> [f64; 3] {
    let mut s = b[0];
    for t in &b[1..] {
        for c in 0..3 {
            s[c] += t[c];
        }
    }
    [s[0] / 16.0, s[1] / 16.0, s[2] / 16.0]
}

fn centred(b: &Rgb, m: [f64; 3]) -> Rgb {
    std::array::from_fn(|k| std::array::from_fn(|c| b[k][c] - m[c]))
}

fn covariance(d: &Rgb) -> [f64; 9] {
    let mut s = [0f64; 9];
    for (i, si) in s.iter_mut().enumerate() {
        let (r, c) = (i / 3, i % 3);
        let mut acc = d[0][r] * d[0][c];
        for t in &d[1..] {
            acc += t[r] * t[c];
        }
        *si = acc;
    }
    s
}

/// `_565`: a colour (0-255) as 5:6:5 and the colour it stands for.
fn rgb565(c: [f64; 3]) -> (u32, [f64; 3]) {
    let r = clip(round(c[0] * 31.0 / 255.0), 0.0, 31.0) as i32;
    let g = clip(round(c[1] * 63.0 / 255.0), 0.0, 63.0) as i32;
    let b = clip(round(c[2] * 31.0 / 255.0), 0.0, 31.0) as i32;
    (
        ((r << 11) | (g << 5) | b) as u32,
        [
            f64::from(r * 255 / 31),
            f64::from(g * 255 / 63),
            f64::from(b * 255 / 31),
        ],
    )
}

fn sq3(a: [f64; 3], p: [f64; 3]) -> f64 {
    let e: [f64; 3] = std::array::from_fn(|c| (a[c] - p[c]) * (a[c] - p[c]));
    (e[0] + e[1]) + e[2]
}

/// The projections of a centred block on an axis, their minimum and maximum.
fn extent(d: &Rgb, axis: [f64; 3]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for t in d {
        let v = (t[0] * axis[0] + t[1] * axis[1]) + t[2] * axis[2];
        lo = lo.min(v);
        hi = hi.max(v);
    }
    (lo, hi)
}

fn end_colour(m: [f64; 3], axis: [f64; 3], t: f64) -> [f64; 3] {
    std::array::from_fn(|c| clip(m[c] + axis[c] * t, 0.0, 255.0))
}

fn block_bytes(c0: u32, c1: u32, idx: &[u32; 16], shift: u32) -> [u8; 8] {
    let bits = idx
        .iter()
        .enumerate()
        .fold(0u32, |s, (k, &i)| s | (i << (shift * k as u32)));
    let mut o = [0u8; 8];
    o[0..2].copy_from_slice(&(c0 as u16).to_le_bytes());
    o[2..4].copy_from_slice(&(c1 as u16).to_le_bytes());
    o[4..8].copy_from_slice(&bits.to_le_bytes());
    o
}

/// A four-colour block (`_color_blocks` without transparency).
fn four_colour(b: &Rgb, axis: [f64; 3]) -> [u8; 8] {
    let m = mean(b);
    let d = centred(b, m);
    let (tmin, tmax) = extent(&d, axis);
    let (mut c0, mut p0) = rgb565(end_colour(m, axis, tmax));
    let (mut c1, mut p1) = rgb565(end_colour(m, axis, tmin));
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
        std::mem::swap(&mut p0, &mut p1);
    }
    let pal: [[f64; 3]; 4] = [
        p0,
        p1,
        std::array::from_fn(|c| (2.0 * p0[c] + p1[c]) / 3.0),
        std::array::from_fn(|c| (p0[c] + 2.0 * p1[c]) / 3.0),
    ];
    let mut idx = [0u32; 16];
    if c0 != c1 {
        for (k, t) in b.iter().enumerate() {
            idx[k] = argmin(pal.iter().map(|p| sq3(*t, *p)));
        }
    }
    block_bytes(c0, c1, &idx, 2)
}

fn argmin(v: impl Iterator<Item = f64>) -> u32 {
    let mut best = (0u32, f64::INFINITY);
    for (i, x) in v.enumerate() {
        if i == 0 || x < best.1 {
            best = (i as u32, x);
        }
    }
    best.0
}

/// A three-colour block with transparent texels (index 3).
fn three_colour(b: &Rgb, clear: u16, axis: [f64; 3]) -> [u8; 8] {
    let m = mean(b);
    let d = centred(b, m);
    let (tmin, tmax) = extent(&d, axis);
    let (mut c0, mut p0) = rgb565(end_colour(m, axis, tmin));
    let (mut c1, mut p1) = rgb565(end_colour(m, axis, tmax));
    if c0 > c1 {
        std::mem::swap(&mut c0, &mut c1);
        std::mem::swap(&mut p0, &mut p1);
    }
    let pal: [[f64; 3]; 3] = [p0, p1, std::array::from_fn(|c| (p0[c] + p1[c]) / 2.0)];
    let mut idx = [0u32; 16];
    for (k, t) in b.iter().enumerate() {
        idx[k] = if clear & (1 << k) != 0 {
            3
        } else {
            argmin(pal.iter().map(|p| sq3(*t, *p)))
        };
    }
    block_bytes(c0, c1, &idx, 2)
}

/// DXT5's alpha half of a block (`_alpha_blocks`): the lower-error eight- or six-value palette.
#[must_use]
pub fn alpha_block(a: &[f64; 16]) -> [u8; 8] {
    let mx = a.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let mn = a.iter().copied().fold(f64::INFINITY, f64::min);
    let (hi, lo) = (round(mx) as i64, round(mn) as i64);
    let mut pal8 = [0i64; 8];
    pal8[0] = hi;
    pal8[1] = lo;
    for k in 1..7 {
        pal8[k as usize + 1] = ((7 - k) * hi + k * lo).div_euclid(7);
    }
    let near = |pal: &[i64; 8], v: f64| argmin(pal.iter().map(|&p| (v - p as f64).abs()));
    let mut idx: [u32; 16] = std::array::from_fn(|k| near(&pal8, a[k]));
    if hi == lo {
        idx = [0; 16];
    }
    let interior: Vec<f64> = a.iter().copied().filter(|&v| v > 0.0 && v < 255.0).collect();
    let (a0, a1) = if interior.is_empty() {
        (0i64, 255i64)
    } else {
        (
            interior.iter().copied().fold(255f64, f64::min) as i64,
            interior.iter().copied().fold(0f64, f64::max) as i64,
        )
    };
    let mut pal6 = [0i64; 8];
    pal6[0] = a0;
    pal6[1] = a1;
    for k in 1..5 {
        pal6[k as usize + 1] = ((5 - k) * a0 + k * a1).div_euclid(5);
    }
    pal6[6] = 0;
    pal6[7] = 255;
    let idx6: [u32; 16] = std::array::from_fn(|k| near(&pal6, a[k]));
    let err = |pal: &[i64; 8], idx: &[u32; 16]| -> f64 {
        a.iter()
            .zip(idx)
            .map(|(&v, &i)| (v - pal[i as usize] as f64).powi(2))
            .sum()
    };
    let six = err(&pal6, &idx6) < err(&pal8, &idx);
    let (hi, lo, idx) = if six { (a0, a1, idx6) } else { (hi, lo, idx) };
    let bits = idx
        .iter()
        .enumerate()
        .fold(0u64, |s, (k, &i)| s | (u64::from(i) << (3 * k)));
    let mut o = [0u8; 8];
    o[0] = hi as u8;
    o[1] = lo as u8;
    o[2..8].copy_from_slice(&bits.to_le_bytes()[..6]);
    o
}

/// How a level is encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    /// DXT1 from three channels (`dxt1(rgb)`), or from four whose fourth is 1-bit alpha (`dxt1(rgba)`).
    Dxt1,
    /// DXT5: the alpha half, then the colour half (`dxt5(rgba)`).
    Dxt5,
}

/// A level's blocks ready for the eigensolver: the colour blocks (transparent texels already filled with the
/// block's opaque mean), the transparent masks when the level has a transparent texel, the alpha values (DXT5).
#[derive(Debug, Clone)]
pub struct Prepared {
    enc: Encoding,
    colour: Vec<Rgb>,
    clear: Option<Vec<u16>>,
    alpha: Vec<[f64; 16]>,
}

impl Prepared {
    /// A level's blocks (`dxt1` / `dxt5` up to the eigensolver); `None` for a picture of other channels than the
    /// encoding reads (3 or 4 for DXT1, 4 for DXT5).
    #[must_use]
    pub fn new(img: &Image, enc: Encoding) -> Option<Self> {
        if !(img.ch == 3 || img.ch == 4) || (enc == Encoding::Dxt5 && img.ch != 4) {
            return None;
        }
        let blk = blocks(img);
        let mut colour: Vec<Rgb> = blk
            .iter()
            .map(|b| std::array::from_fn(|k| [b[k][0], b[k][1], b[k][2]]))
            .collect();
        let alpha: Vec<[f64; 16]> = if img.ch == 4 {
            blk.iter().map(|b| std::array::from_fn(|k| b[k][3])).collect()
        } else {
            Vec::new()
        };
        let mut clear = None;
        if enc == Encoding::Dxt1 && img.ch == 4 {
            let masks: Vec<u16> = alpha
                .iter()
                .map(|a| (0..16).fold(0u16, |m, k| if a[k] < 128.0 { m | (1 << k) } else { m }))
                .collect();
            if masks.iter().any(|&m| m != 0) {
                for (b, &m) in colour.iter_mut().zip(&masks) {
                    let opaque = |k: usize| if m & (1 << k) == 0 { 1.0 } else { 0.0 };
                    let cnt = (0..16).filter(|&k| m & (1 << k) == 0).count();
                    let mut s: [f64; 3] = std::array::from_fn(|c| b[0][c] * opaque(0));
                    for (k, t) in b.iter().enumerate().skip(1) {
                        for c in 0..3 {
                            s[c] += t[c] * opaque(k);
                        }
                    }
                    let div = cnt.max(1) as f64;
                    let fill: [f64; 3] = std::array::from_fn(|c| s[c] / div);
                    for (k, t) in b.iter_mut().enumerate() {
                        if m & (1 << k) != 0 {
                            *t = fill;
                        }
                    }
                }
                clear = Some(masks);
            }
        }
        Some(Self {
            enc,
            colour,
            clear,
            alpha,
        })
    }

    /// Blocks.
    #[must_use]
    pub fn len(&self) -> usize {
        self.colour.len()
    }

    /// No blocks (never: a level has at least one).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.colour.is_empty()
    }

    /// Each block's covariance (3 x 3, row-major), what `numpy.linalg.eigh` solves.
    #[must_use]
    pub fn covariances(&self) -> Vec<[f64; 9]> {
        use rayon::prelude::*;
        self.colour
            .par_iter()
            .map(|b| covariance(&centred(b, mean(b))))
            .collect()
    }

    /// The level's blocks, each block's principal axis given (the eigenvector of its largest eigenvalue).
    #[must_use]
    pub fn finish(&self, axes: &[[f64; 3]]) -> Vec<u8> {
        use rayon::prelude::*;
        let per: Vec<[u8; 16]> = self
            .colour
            .par_iter()
            .zip(axes)
            .enumerate()
            .map(|(i, (b, &axis))| {
                let mut o = [0u8; 16];
                let colour = match &self.clear {
                    Some(m) if m[i] != 0 => three_colour(b, m[i], axis),
                    _ => four_colour(b, axis),
                };
                if self.enc == Encoding::Dxt5 {
                    o[..8].copy_from_slice(&alpha_block(&self.alpha[i]));
                    o[8..].copy_from_slice(&colour);
                } else {
                    o[..8].copy_from_slice(&colour);
                }
                o
            })
            .collect();
        let size = if self.enc == Encoding::Dxt5 { 16 } else { 8 };
        let mut out = Vec::with_capacity(per.len() * size);
        for o in &per {
            out.extend_from_slice(&o[..size]);
        }
        out
    }
}

/// `alpha1(level, w, h)`: the 1-bit alpha of a DXT1 level, `h` rows of `w` (0 or 255); `None` where the reference
/// raises.
#[must_use]
pub fn alpha1(level: &[u8], w: usize, h: usize) -> Option<Vec<i64>> {
    let (bw, bh) = (w.div_ceil(4).max(1), h.div_ceil(4).max(1));
    let data = level.get(..bw * bh * 8)?;
    let mut out = vec![0i64; h.min(bh * 4) * w.min(bw * 4)];
    let ow = w.min(bw * 4);
    for (i, b) in data.chunks_exact(8).enumerate() {
        let c0 = u16::from_le_bytes([b[0], b[1]]);
        let c1 = u16::from_le_bytes([b[2], b[3]]);
        let bits = u32::from_le_bytes([b[4], b[5], b[6], b[7]]);
        for k in 0..16 {
            let (y, x) = ((i / bw) * 4 + k / 4, (i % bw) * 4 + k % 4);
            if y < h && x < w {
                out[y * ow + x] = if c0 <= c1 && (bits >> (2 * k)) & 3 == 3 { 0 } else { 255 };
            }
        }
    }
    Some(out)
}

/// `alpha5(level, w, h)`: the alpha of a DXT5 level; `None` where the reference raises.
#[must_use]
pub fn alpha5(level: &[u8], w: usize, h: usize) -> Option<Vec<i64>> {
    let (bw, bh) = (w.div_ceil(4).max(1), h.div_ceil(4).max(1));
    let data = level.get(..bw * bh * 16)?;
    let ow = w.min(bw * 4);
    let mut out = vec![0i64; h.min(bh * 4) * ow];
    for (i, b) in data.chunks_exact(16).enumerate() {
        let (a0, a1) = (i64::from(b[0]), i64::from(b[1]));
        let big = a0 > a1;
        let pal: [i64; 8] = std::array::from_fn(|j| {
            let k = j as i64 - 1;
            match j {
                0 => a0,
                1 => a1,
                _ if big => ((7 - k) * a0 + k * a1).div_euclid(7),
                2..=5 => ((5 - k) * a0 + k * a1).div_euclid(5),
                6 => 0,
                _ => 255,
            }
        });
        let mut bits = 0u64;
        for k in 0..6 {
            bits |= u64::from(b[2 + k]) << (8 * k);
        }
        for k in 0..16 {
            let (y, x) = ((i / bw) * 4 + k / 4, (i % bw) * 4 + k % 4);
            if y < h && x < w {
                out[y * ow + x] = pal[((bits >> (3 * k)) & 7) as usize];
            }
        }
    }
    Some(out)
}

/// `l8_down(level)`: each texel the rounded mean of its 2 x 2 block; `None` for odd sides (the reference's shapes
/// do not match).
#[must_use]
pub fn l8_down(px: &[u8], h: usize, w: usize) -> Option<Vec<u8>> {
    if !h.is_multiple_of(2) || !w.is_multiple_of(2) || px.len() != h * w {
        return None;
    }
    let at = |y: usize, x: usize| i32::from(px[y * w + x]);
    let mut out = Vec::with_capacity(h * w / 4);
    for y in (0..h).step_by(2) {
        for x in (0..w).step_by(2) {
            out.push(((at(y, x) + at(y + 1, x) + at(y, x + 1) + at(y + 1, x + 1) + 2) / 4) as u8);
        }
    }
    Some(out)
}

// ------------------------------------------------------------------------------------------------- packing

/// The ITexture header's size: version 3, type 0, format, flags, width, height, depth 0, the level count, 15 level
/// sizes (the console converter's `FORMAT.md` section 2).
pub const HEADER: usize = 92;

/// `texwrite.resource(fmt, flags, w, h, data)` (and `texture`'s last step, round PR1b-2-rest): the header and the
/// levels as they are, largest first; `None` for more than 15 levels.
#[must_use]
pub fn resource(fmt: u32, flags: u32, w: u32, h: u32, levels: &[&[u8]]) -> Option<Vec<u8>> {
    if levels.len() > 15 {
        return None;
    }
    let mut out = Vec::with_capacity(HEADER + levels.iter().map(|l| l.len()).sum::<usize>());
    for v in [3, 0, fmt, flags, w, h, 0, u32::try_from(levels.len()).ok()?] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for k in 0..15 {
        let n = levels.get(k).map_or(0, |l| l.len());
        out.extend_from_slice(&u32::try_from(n).ok()?.to_le_bytes());
    }
    for l in levels {
        out.extend_from_slice(l);
    }
    Some(out)
}

/// `texwrite.wrapped(res, kind)`: the on-demand bundles' form - `RES\x02`, the type name, zeros to 56 bytes, the
/// payload's size twice (u32), the payload. `None`: a size past u32.
#[must_use]
pub fn wrapped(res: &[u8], kind: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(64 + res.len());
    out.extend_from_slice(b"RES\x02");
    out.extend_from_slice(kind);
    out.resize(out.len().max(56), 0);
    let n = u32::try_from(res.len()).ok()?.to_le_bytes();
    out.extend_from_slice(&n);
    out.extend_from_slice(&n);
    out.extend_from_slice(res);
    Some(out)
}

/// `texwrite.l8_bytes(level)`: an 8-bit level's bytes as DICE stores them - a row under 4 texels padded to 4 bytes
/// with zeros (mp_005's 2 x 2 level is 8 bytes); `px` is `h` x `w` bytes.
#[must_use]
pub fn l8_bytes(px: &[u8], h: usize, w: usize) -> Option<Vec<u8>> {
    if px.len() != h.checked_mul(w)? {
        return None;
    }
    if w >= 4 {
        return Some(px.to_vec());
    }
    let mut out = vec![0u8; h * 4];
    for (r, row) in px.chunks(w.max(1)).take(h).enumerate() {
        out.get_mut(r * 4..r * 4 + row.len())?.copy_from_slice(row);
    }
    Some(out)
}

#[cfg(test)]
mod packing_tests {
    use super::*;

    #[test]
    fn packed_as_the_python_packs() {
        let r = resource(0, 1, 4, 4, &[&[1u8; 8][..]]).unwrap_or_default();
        assert_eq!(r.len(), HEADER + 8);
        assert_eq!(&r[..12], &[3, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&r[28..36], &[1, 0, 0, 0, 8, 0, 0, 0]);
        assert!(resource(0, 1, 4, 4, &[&[][..]; 16]).is_none());
        let w = wrapped(b"abc", b"ITexture").unwrap_or_default();
        assert_eq!((&w[..12], w.len()), (&b"RES\x02ITexture"[..], 56 + 8 + 3));
        assert_eq!(&w[56..64], &[3, 0, 0, 0, 3, 0, 0, 0]);
        assert_eq!(l8_bytes(&[1, 2, 3, 4], 2, 2), Some(vec![1, 2, 0, 0, 3, 4, 0, 0]));
        assert_eq!(l8_bytes(&[9; 8], 2, 4), Some(vec![9; 8]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_block_is_one_colour() {
        let img = Image {
            h: 4,
            w: 4,
            ch: 3,
            px: vec![100; 48],
        };
        let p = Prepared::new(&img, Encoding::Dxt1).unwrap_or_else(|| unreachable!());
        assert_eq!(p.covariances(), vec![[0.0; 9]]);
        let out = p.finish(&[[0.0, 0.0, 1.0]]);
        assert_eq!(out.get(4..8), Some(&[0u8; 4][..]));
    }

    #[test]
    fn mips_down_to_one() {
        let img = Image {
            h: 4,
            w: 2,
            ch: 3,
            px: vec![7; 24],
        };
        let lv = levels(&img, false, 3).unwrap_or_default();
        assert_eq!(
            lv.iter().map(|l| (l.h, l.w)).collect::<Vec<_>>(),
            vec![(4, 2), (2, 1), (1, 1)]
        );
    }

    #[test]
    fn l8_rounds_up_halves() {
        assert_eq!(l8_down(&[1, 2, 3, 4], 2, 2), Some(vec![3]));
    }
}
