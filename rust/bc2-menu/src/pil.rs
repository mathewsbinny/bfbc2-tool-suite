//! The picture operations Project OPFOR's menu art uses (round OR5), with the pixels Pillow 12 gives: Lanczos resizing
//! (through premultiplied alpha, as `Image.resize` does an RGBA image), cropping, pasting through a picture's own
//! alpha, `alpha_composite`, DXT5 (BC3) decoding as Pillow's DDS reader decodes it, and PNG in and out.
//!
//! Pillow's C code is the reference: `Resample.c` (the filter's coefficients in doubles, normalised to 22-bit fixed
//! point, a horizontal pass then a vertical one), `Convert.c` (`rgba2rgbA` / `rgbA2rgba`), `Paste.c`
//! (`paste_mask_RGBA`), `AlphaComposite.c` and `BcnDecode.c`. `tests/pil_parity.py` holds them against the installed
//! Pillow on random pictures.

#![allow(
    clippy::chunks_exact_to_as_chunks,
    clippy::needless_range_loop,
    clippy::decimal_bitwise_operands,
    reason = "Pillow's loops and constants, written as its C code writes them"
)]
#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    reason = "Pillow's and the retired Python's names (x, y, w, h, a ...) kept for review against them; each index is bounded by the picture's size, as theirs"
)]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    reason = "Pillow's integer and double arithmetic, as its C code does it"
)]

use crate::error::{MenuError, MenuResult};

/// An RGBA picture, 8 bits a channel, rows top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rgba {
    pub w: usize,
    pub h: usize,
    pub px: Vec<u8>,
}

impl Rgba {
    /// A picture of one colour.
    #[must_use]
    pub fn filled(w: usize, h: usize, c: [u8; 4]) -> Self {
        let mut px = Vec::with_capacity(w * h * 4);
        for _ in 0..w * h {
            px.extend_from_slice(&c);
        }
        Self { w, h, px }
    }

    fn at(&self, x: usize, y: usize) -> usize {
        (y * self.w + x) * 4
    }

    /// `crop((x0, y0, x1, y1))`: what lies outside the picture is transparent black, as Pillow's crop gives it.
    #[must_use]
    pub fn crop(&self, x0: i64, y0: i64, x1: i64, y1: i64) -> Self {
        let (w, h) = ((x1 - x0).max(0) as usize, (y1 - y0).max(0) as usize);
        let mut out = Self::filled(w, h, [0; 4]);
        for y in 0..h {
            let sy = y0 + y as i64;
            if sy < 0 || sy >= self.h as i64 {
                continue;
            }
            for x in 0..w {
                let sx = x0 + x as i64;
                if sx < 0 || sx >= self.w as i64 {
                    continue;
                }
                let (s, d) = (self.at(sx as usize, sy as usize), out.at(x, y));
                out.px[d..d + 4].copy_from_slice(&self.px[s..s + 4]);
            }
        }
        out
    }

    /// `transpose(FLIP_LEFT_RIGHT)`.
    #[must_use]
    pub fn flip_lr(&self) -> Self {
        let mut out = self.clone();
        for y in 0..self.h {
            for x in 0..self.w {
                let (s, d) = (self.at(x, y), out.at(self.w - 1 - x, y));
                out.px[d..d + 4].copy_from_slice(&self.px[s..s + 4]);
            }
        }
        out
    }

    /// The overlap of `src` placed at (`x`, `y`) with this picture: (dst x, dst y, src x, src y, width, height).
    fn overlap(&self, src: &Self, x: i64, y: i64) -> Option<(usize, usize, usize, usize, usize, usize)> {
        let (dx0, dy0) = (x.max(0), y.max(0));
        let (dx1, dy1) = (
            (x + src.w as i64).min(self.w as i64),
            (y + src.h as i64).min(self.h as i64),
        );
        if dx1 <= dx0 || dy1 <= dy0 {
            return None;
        }
        Some((
            dx0 as usize,
            dy0 as usize,
            (dx0 - x) as usize,
            (dy0 - y) as usize,
            (dx1 - dx0) as usize,
            (dy1 - dy0) as usize,
        ))
    }

    /// `paste(src, (x, y))`: the pixels copied.
    pub fn paste(&mut self, src: &Self, x: i64, y: i64) {
        let Some((dx, dy, sx, sy, w, h)) = self.overlap(src, x, y) else {
            return;
        };
        for r in 0..h {
            let (d, s) = (self.at(dx, dy + r), src.at(sx, sy + r));
            self.px[d..d + w * 4].copy_from_slice(&src.px[s..s + w * 4]);
        }
    }

    /// `paste(src, (x, y), src)`: each channel blended by the source's alpha (`Paste.c` `paste_mask_RGBA`).
    pub fn paste_masked(&mut self, src: &Self, x: i64, y: i64) {
        let Some((dx, dy, sx, sy, w, h)) = self.overlap(src, x, y) else {
            return;
        };
        for r in 0..h {
            for c in 0..w {
                let (d, s) = (self.at(dx + c, dy + r), src.at(sx + c, sy + r));
                let m = u32::from(src.px[s + 3]);
                for i in 0..4 {
                    let v = u32::from(self.px[d + i]) * (255 - m) + u32::from(src.px[s + i]) * m;
                    self.px[d + i] = div255(v) as u8;
                }
            }
        }
    }

    /// `alpha_composite(src)` at (0, 0), both the same size (`AlphaComposite.c`).
    pub fn alpha_composite(&mut self, src: &Self) {
        const PB: u32 = 7;
        let shift = |a: u32| ((a >> 8) + a) >> 8;
        for (d, s) in self.px.chunks_exact_mut(4).zip(src.px.chunks_exact(4)) {
            let sa = u32::from(s[3]);
            if sa == 0 {
                continue;
            }
            let blend = u32::from(d[3]) * (255 - sa);
            let outa255 = sa * 255 + blend;
            let coef1 = sa * 255 * 255 * (1 << PB) / outa255;
            let coef2 = 255 * (1 << PB) - coef1;
            for i in 0..3 {
                let t = u32::from(s[i]) * coef1 + u32::from(d[i]) * coef2;
                d[i] = (shift(t + (0x80 << PB)) >> PB) as u8;
            }
            d[3] = shift(outa255 + 0x80) as u8;
        }
    }

    /// `resize((w, h), Image.Resampling.LANCZOS)` of an RGBA picture: premultiplied, resampled, unpremultiplied.
    #[must_use]
    pub fn resize_lanczos(&self, w: usize, h: usize) -> Self {
        if (w, h) == (self.w, self.h) {
            return self.clone();
        }
        let mut pre = self.clone();
        for p in pre.px.chunks_exact_mut(4) {
            let a = u32::from(p[3]);
            for c in &mut p[..3] {
                *c = muldiv255(u32::from(*c), a) as u8;
            }
        }
        let out = resample(&pre, w, h);
        let mut out = out;
        for p in out.px.chunks_exact_mut(4) {
            let a = u32::from(p[3]);
            if a != 255 && a != 0 {
                for c in &mut p[..3] {
                    *c = ((255 * u32::from(*c)) / a).min(255) as u8;
                }
            }
        }
        out
    }
}

fn div255(v: u32) -> u32 {
    let t = v + 128;
    ((t >> 8) + t) >> 8
}

fn muldiv255(a: u32, b: u32) -> u32 {
    div255(a * b)
}

const PRECISION_BITS: u32 = 32 - 8 - 2;

fn lanczos(x: f64) -> f64 {
    fn sinc(x: f64) -> f64 {
        if x == 0.0 {
            return 1.0;
        }
        let x = x * std::f64::consts::PI;
        x.sin() / x
    }
    if (-3.0..3.0).contains(&x) {
        sinc(x) * sinc(x / 3.0)
    } else {
        0.0
    }
}

/// `precompute_coeffs` + `normalize_coeffs_8bpc`: (ksize, bounds (min, count), fixed-point coefficients).
fn coeffs(in_size: usize, out_size: usize) -> (usize, Vec<(usize, usize)>, Vec<i32>) {
    let scale = in_size as f64 / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = 3.0 * filterscale;
    let ksize = support.ceil() as usize * 2 + 1;
    let mut bounds = Vec::with_capacity(out_size);
    let mut kk = vec![0i32; out_size * ksize];
    let mut k = vec![0f64; ksize];
    for xx in 0..out_size {
        let center = (xx as f64 + 0.5) * scale;
        let ss = 1.0 / filterscale;
        let xmin = ((center - support + 0.5) as i64).max(0);
        let xmax = ((center + support + 0.5) as i64).min(in_size as i64) - xmin;
        let (xmin, xmax) = (xmin as usize, xmax.max(0) as usize);
        let mut ww = 0.0;
        for (x, kx) in k.iter_mut().enumerate().take(xmax) {
            let w = lanczos(((x + xmin) as f64 - center + 0.5) * ss);
            *kx = w;
            ww += w;
        }
        for (x, kx) in k.iter_mut().enumerate() {
            let v = if x < xmax {
                if ww == 0.0 { *kx } else { *kx / ww }
            } else {
                0.0
            };
            let f = v * f64::from(1u32 << PRECISION_BITS);
            kk[xx * ksize + x] = if v < 0.0 { (-0.5 + f) as i32 } else { (0.5 + f) as i32 };
        }
        bounds.push((xmin, xmax));
    }
    (ksize, bounds, kk)
}

fn clip8(v: i32) -> u8 {
    (v >> PRECISION_BITS).clamp(0, 255) as u8
}

/// `ImagingResampleInner` with Lanczos on 4 bands of 8 bits.
fn resample(src: &Rgba, w: usize, h: usize) -> Rgba {
    let (kh, bh, kkh) = coeffs(src.w, w);
    let (kv, bv, kkv) = coeffs(src.h, h);
    let need_h = w != src.w;
    let need_v = h != src.h;
    let first = bv.first().map_or(0, |b| b.0);
    let last = bv.last().map_or(0, |b| b.0 + b.1);
    let mut cur = src.clone();
    let mut off = 0;
    if need_h {
        let rows = last - first;
        let mut t = Rgba::filled(w, rows, [0; 4]);
        for yy in 0..rows {
            for (xx, &(xmin, xmax)) in bh.iter().enumerate() {
                let k = &kkh[xx * kh..xx * kh + kh];
                let mut ss = [1i32 << (PRECISION_BITS - 1); 4];
                for x in 0..xmax {
                    let p = cur.at(x + xmin, yy + first);
                    for (c, s) in ss.iter_mut().enumerate() {
                        *s = s.wrapping_add(i32::from(cur.px[p + c]).wrapping_mul(k[x]));
                    }
                }
                let d = t.at(xx, yy);
                for (c, s) in ss.iter().enumerate() {
                    t.px[d + c] = clip8(*s);
                }
            }
        }
        cur = t;
        off = first;
    }
    if need_v {
        let mut t = Rgba::filled(cur.w, h, [0; 4]);
        for (yy, &(ymin, ymax)) in bv.iter().enumerate() {
            let k = &kkv[yy * kv..yy * kv + kv];
            for xx in 0..cur.w {
                let mut ss = [1i32 << (PRECISION_BITS - 1); 4];
                for y in 0..ymax {
                    let p = cur.at(xx, y + ymin - off);
                    for (c, s) in ss.iter_mut().enumerate() {
                        *s = s.wrapping_add(i32::from(cur.px[p + c]).wrapping_mul(k[y]));
                    }
                }
                let d = t.at(xx, yy);
                for (c, s) in ss.iter().enumerate() {
                    t.px[d + c] = clip8(*s);
                }
            }
        }
        cur = t;
    }
    cur
}

// ------------------------------------------------------------------------------------------------ DXT5

/// One DXT5 block's 16 pixels as Pillow's `BcnDecode.c` decodes them (`decode_bc3_alpha`, `decode_bc1_color` with
/// separate alpha).
#[must_use]
pub fn bc3_block(b: &[u8; 16]) -> [[u8; 4]; 16] {
    let mut out = [[0u8; 4]; 16];
    let (a0, a1) = (u32::from(b[0]), u32::from(b[1]));
    let mut a = [0u32; 8];
    a[0] = a0;
    a[1] = a1;
    if a0 > a1 {
        for (k, v) in a.iter_mut().enumerate().skip(2) {
            let i = k as u32 - 1;
            *v = ((7 - i) * a0 + i * a1) / 7;
        }
    } else {
        for (k, v) in a.iter_mut().enumerate().skip(2).take(4) {
            let i = k as u32 - 1;
            *v = ((5 - i) * a0 + i * a1) / 5;
        }
        a[6] = 0;
        a[7] = 255;
    }
    let lut1 = u32::from(b[2]) | u32::from(b[3]) << 8 | u32::from(b[4]) << 16;
    let lut2 = u32::from(b[5]) | u32::from(b[6]) << 8 | u32::from(b[7]) << 16;
    for n in 0..8 {
        out[n][3] = a[(7 & (lut1 >> (3 * n))) as usize] as u8;
        out[8 + n][3] = a[(7 & (lut2 >> (3 * n))) as usize] as u8;
    }
    let c0 = u32::from(u16::from_le_bytes([b[8], b[9]]));
    let c1 = u32::from(u16::from_le_bytes([b[10], b[11]]));
    let lut = u32::from_le_bytes([b[12], b[13], b[14], b[15]]);
    let expand = |c: u32| {
        let (r, g, bb) = ((c >> 11) & 0x1F, (c >> 5) & 0x3F, c & 0x1F);
        [(r << 3) | (r >> 2), (g << 2) | (g >> 4), (bb << 3) | (bb >> 2)]
    };
    let (p0, p1) = (expand(c0), expand(c1));
    let mut p = [[0u32; 3]; 4];
    p[0] = p0;
    p[1] = p1;
    for i in 0..3 {
        p[2][i] = (2 * p0[i] + p1[i]) / 3;
        p[3][i] = (p0[i] + 2 * p1[i]) / 3;
    }
    for (n, o) in out.iter_mut().enumerate() {
        let c = p[(3 & (lut >> (2 * n))) as usize];
        o[0] = c[0] as u8;
        o[1] = c[1] as u8;
        o[2] = c[2] as u8;
    }
    out
}

/// Linear DXT5 blocks (row-major, little-endian) -> the picture, as `dds_decode` gave it.
pub fn dxt5_decode(data: &[u8], w: usize, h: usize) -> MenuResult<Rgba> {
    let (bw, bh) = (w.div_ceil(4), h.div_ceil(4));
    if data.len() < bw * bh * 16 {
        return Err(MenuError::Menu(format!(
            "DXT5 data cut short: {} bytes for {w}x{h}",
            data.len()
        )));
    }
    let mut out = Rgba::filled(w, h, [0; 4]);
    for by in 0..bh {
        for bx in 0..bw {
            let o = (by * bw + bx) * 16;
            let mut blk = [0u8; 16];
            blk.copy_from_slice(&data[o..o + 16]);
            for (n, px) in bc3_block(&blk).iter().enumerate() {
                let (x, y) = (bx * 4 + n % 4, by * 4 + n / 4);
                if x < w && y < h {
                    let d = out.at(x, y);
                    out.px[d..d + 4].copy_from_slice(px);
                }
            }
        }
    }
    Ok(out)
}

/// `dxt5_encode(rgba)` of `onslaught_art.py`: each 4x4 block's alpha from its max / min (an 8-value ramp, the
/// nearest value, the first on a tie) and its colour between the brightest and darkest visible pixel by luminance
/// (a 4-value ramp, the nearest by squared distance). `w` and `h` are multiples of 4; the blocks row-major.
#[must_use]
pub fn dxt5_encode(img: &Rgba) -> Vec<[u8; 16]> {
    let (bw, bh) = (img.w / 4, img.h / 4);
    let mut out = Vec::with_capacity(bw * bh);
    for by in 0..bh {
        for bx in 0..bw {
            let mut px = [[0i64; 4]; 16];
            for (n, p) in px.iter_mut().enumerate() {
                let o = img.at(bx * 4 + n % 4, by * 4 + n / 4);
                for c in 0..4 {
                    p[c] = i64::from(img.px[o + c]);
                }
            }
            out.push(encode_block(&px));
        }
    }
    out
}

fn argmin<I: IntoIterator<Item = i64>>(it: I) -> usize {
    let mut best = (0, i64::MAX);
    for (i, v) in it.into_iter().enumerate() {
        if v < best.1 {
            best = (i, v);
        }
    }
    best.0
}

fn encode_block(px: &[[i64; 4]; 16]) -> [u8; 16] {
    let mut out = [0u8; 16];
    let a0 = px.iter().map(|p| p[3]).max().unwrap_or(0);
    let a1 = px.iter().map(|p| p[3]).min().unwrap_or(0);
    let mut ramp = [a0, a1, 0, 0, 0, 0, 0, 0];
    for k in 1..7i64 {
        ramp[k as usize + 1] = ((7 - k) * a0 + k * a1) / 7;
    }
    let mut bits = 0u64;
    for (k, p) in px.iter().enumerate() {
        let ai = argmin(ramp.iter().map(|r| (p[3] - r).abs())) as u64;
        bits |= ai << (3 * k);
    }
    out[0] = a0 as u8;
    out[1] = a1 as u8;
    for k in 0..6 {
        out[2 + k] = ((bits >> (8 * k)) & 0xFF) as u8;
    }
    let lum = |p: &[i64; 4]| p[0] * 299 + p[1] * 587 + p[2] * 114;
    // argmax / argmin over visible pixels (an invisible one counts as -1 / 1 << 30): the first on a tie
    let mut hi_i = 0;
    let mut hi_v = i64::MIN;
    let mut lo_i = 0;
    let mut lo_v = i64::MAX;
    for (i, p) in px.iter().enumerate() {
        let vis = p[3] > 0;
        let h = if vis { lum(p) } else { -1 };
        let l = if vis { lum(p) } else { 1 << 30 };
        if h > hi_v {
            hi_v = h;
            hi_i = i;
        }
        if l < lo_v {
            lo_v = l;
            lo_i = i;
        }
    }
    let (mut hi, mut lo) = (px[hi_i], px[lo_i]);
    let to565 = |c: &[i64; 4]| ((c[0] >> 3) << 11) | ((c[1] >> 2) << 5) | (c[2] >> 3);
    let (mut c0, mut c1) = (to565(&hi), to565(&lo));
    if c0 < c1 {
        std::mem::swap(&mut c0, &mut c1);
        std::mem::swap(&mut hi, &mut lo);
    }
    let back = |c: i64| [(c >> 11 & 31) * 255 / 31, (c >> 5 & 63) * 255 / 63, (c & 31) * 255 / 31];
    let (e0, e1) = (back(c0), back(c1));
    let mut pal = [[0i64; 3]; 4];
    pal[0] = e0;
    pal[1] = e1;
    for i in 0..3 {
        pal[2][i] = (2 * e0[i] + e1[i]) / 3;
        pal[3][i] = (e0[i] + 2 * e1[i]) / 3;
    }
    let mut cb = 0u32;
    for (k, p) in px.iter().enumerate() {
        let ci = if c0 == c1 {
            0
        } else {
            argmin(pal.iter().map(|q| (0..3).map(|i| (p[i] - q[i]).pow(2)).sum::<i64>()))
        };
        cb |= (ci as u32) << (2 * k);
    }
    out[8] = (c0 & 0xFF) as u8;
    out[9] = (c0 >> 8) as u8;
    out[10] = (c1 & 0xFF) as u8;
    out[11] = (c1 >> 8) as u8;
    out[12..16].copy_from_slice(&cb.to_le_bytes());
    out
}

/// `xbox_untile_dxt(data, w, h)`: Xbox 360 tiled, big-endian DXT blocks (16 bytes) -> linear little-endian blocks.
pub fn xbox_untile_dxt(data: &[u8], w: usize, h: usize) -> MenuResult<Vec<u8>> {
    let (bw, bh) = (w / 4, h / 4);
    let tiled = |x: usize, y: usize, width: usize| -> usize {
        let aw = (width + 31) & !31;
        let lb = 4;
        let macro_ = ((x >> 5) + (y >> 5) * (aw >> 5)) << (lb + 7);
        let micro = ((x & 7) + ((y & 6) << 2)) << lb;
        let off = macro_ + ((micro & !15) << 1) + (micro & 15) + ((y & 8) << (3 + lb)) + ((y & 1) << 4);
        (((off & !511) << 3)
            + ((off & 448) << 2)
            + (off & 63)
            + ((y & 16) << 7)
            + (((((y & 8) >> 2) + (x >> 3)) & 3) << 6))
            >> lb
    };
    let mut out = Vec::with_capacity(bw * bh * 16);
    for y in 0..bh {
        for x in 0..bw {
            let s = tiled(x, y, bw) * 16;
            let blk = data.get(s..s + 16).ok_or_else(|| {
                MenuError::Menu(format!("Xbox 360 texture cut short ({} bytes, {w}x{h})", data.len()))
            })?;
            for k in 0..8 {
                out.push(blk[2 * k + 1]);
                out.push(blk[2 * k]);
            }
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ PNG

/// A PNG file as RGBA (`Image.open(path).convert('RGBA')` for the 8-bit RGBA, RGB, grey and palette PNGs).
pub fn png_read(path: &str) -> MenuResult<Rgba> {
    let f = std::fs::File::open(path).map_err(|e| MenuError::io(path, e))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(f));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let bad = |e: &dyn std::fmt::Display| MenuError::Menu(format!("{path}: not a readable PNG ({e})"));
    let mut r = dec.read_info().map_err(|e| bad(&e))?;
    let mut buf = vec![0; r.output_buffer_size().ok_or_else(|| bad(&"too large"))?];
    let info = r.next_frame(&mut buf).map_err(|e| bad(&e))?;
    let (w, h) = (info.width as usize, info.height as usize);
    let mut px = Vec::with_capacity(w * h * 4);
    let data = &buf[..info.buffer_size()];
    match info.color_type {
        png::ColorType::Rgba => px.extend_from_slice(data),
        png::ColorType::Rgb => data
            .chunks_exact(3)
            .for_each(|c| px.extend_from_slice(&[c[0], c[1], c[2], 255])),
        png::ColorType::GrayscaleAlpha => data
            .chunks_exact(2)
            .for_each(|c| px.extend_from_slice(&[c[0], c[0], c[0], c[1]])),
        png::ColorType::Grayscale => data.iter().for_each(|&c| px.extend_from_slice(&[c, c, c, 255])),
        png::ColorType::Indexed => return Err(bad(&"a palette PNG not expanded")),
    }
    if px.len() != w * h * 4 {
        return Err(bad(&"its size"));
    }
    Ok(Rgba { w, h, px })
}

/// The picture as an RGBA PNG file (written through `bc2_gamefs::atomic`).
pub fn png_write(path: &str, img: &Rgba) -> MenuResult<()> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.w as u32, img.h as u32);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let bad = |e: png::EncodingError| MenuError::Menu(format!("{path}: {e}"));
        let mut wr = enc.write_header().map_err(bad)?;
        wr.write_image_data(&img.px).map_err(bad)?;
    }
    bc2_gamefs::atomic::write(std::path::Path::new(path), &[&out])
        .map_err(|e| MenuError::Menu(format!("{path}: could not write ({e})")))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    //! Pillow's and numpy's answers (Pillow 12.3, numpy 2.5) on pictures from a fixed generator, recorded as digests
    //! by round OR5 (`rust/parity_menuart.py` holds the operations against Pillow on random pictures).
    use super::*;

    fn lcg_bytes(seed: u64, n: usize) -> Vec<u8> {
        let mut x = seed;
        (0..n)
            .map(|_| {
                x = x
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                (x >> 56) as u8
            })
            .collect()
    }

    fn picture(seed: u64, w: usize, h: usize) -> Rgba {
        let mut px = lcg_bytes(seed, w * h * 4);
        for p in px.chunks_exact_mut(4) {
            p[3] = match p[3] {
                a if a < 80 => 0,
                a if a > 170 => 255,
                a => a,
            };
        }
        Rgba { w, h, px }
    }

    fn digest(b: &[u8]) -> String {
        bc2_gamefs::sha256::hex(b)[..16].to_owned()
    }

    #[test]
    fn pillow_answers() {
        let a = picture(1, 37, 23);
        assert_eq!(digest(&a.resize_lanczos(61, 11).px), "e1aec48c11358cd9");
        assert_eq!(digest(&a.resize_lanczos(9, 40).px), "f68daee38e008ce2");
        assert_eq!(digest(&crate::modeart::cutout(&a).px), "2bb4e22cc4fa3aac");
        assert_eq!(digest(&crate::modeart::fit(&a).px), "0260943977f2f03e");
        let mut base = picture(2, 50, 30);
        let mut layer = Rgba::filled(50, 30, [0; 4]);
        layer.paste_masked(&a, -5, 12);
        base.alpha_composite(&layer);
        assert_eq!(digest(&base.px), "bb88bb365c5838f2");
        assert_eq!(
            digest(&dxt5_decode(&lcg_bytes(3, 2 * 3 * 16), 8, 12).unwrap().px),
            "142d049b9e75f25f"
        );
        assert_eq!(digest(&dxt5_encode(&picture(4, 8, 12)).concat()), "4d2fad1dd35c8106");
    }

    #[test]
    fn png_round_trip() {
        let p = std::env::temp_dir().join(format!("bc2menu-pil-{}.png", std::process::id()));
        let a = picture(5, 7, 3);
        png_write(&p.to_string_lossy(), &a).unwrap();
        assert_eq!(png_read(&p.to_string_lossy()).unwrap(), a);
        std::fs::remove_file(&p).unwrap();
    }
}
