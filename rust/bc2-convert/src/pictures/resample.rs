//! Pillow's `Image.resize` for 8-bit pictures (`Resample.c`, Pillow 12): the two-pass convolution with the filter's
//! coefficients in fixed point (22 fraction bits), horizontal first, then vertical - the pictures OPFOR's WW2 tools
//! scaled with Pillow (the loading and map pictures, the weapon icons) are the same pixels here. An RGBA picture is
//! premultiplied first (`RGBA` -> `RGBa`: `MULDIV255`) and divided back after (`RGBa` -> `RGBA`: `255 * c / a`, kept
//! where alpha is 0 or 255), as `Image.resize` converts it.

use super::Pic;

/// A resampling filter (`Image.Resampling`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    /// `BOX`: support 0.5.
    Box,
    /// `LANCZOS`: a 3-lobed Lanczos (truncated sinc), support 3.
    Lanczos,
}

const PRECISION_BITS: u32 = 32 - 8 - 2;

fn sinc(x: f64) -> f64 {
    if x == 0.0 {
        return 1.0;
    }
    let x = x * std::f64::consts::PI;
    x.sin() / x
}

impl Filter {
    fn support(self) -> f64 {
        match self {
            Self::Box => 0.5,
            Self::Lanczos => 3.0,
        }
    }

    fn weight(self, x: f64) -> f64 {
        match self {
            Self::Box => {
                if x > -0.5 && x <= 0.5 {
                    1.0
                } else {
                    0.0
                }
            }
            Self::Lanczos => {
                if (-3.0..3.0).contains(&x) {
                    sinc(x) * sinc(x / 3.0)
                } else {
                    0.0
                }
            }
        }
    }
}

/// The coefficients of one pass (`precompute_coeffs` + `normalize_coeffs_8bpc`): per output sample its first input
/// sample, its count, and `ksize` fixed-point weights.
struct Coeffs {
    ksize: usize,
    bounds: Vec<(usize, usize)>,
    kk: Vec<i32>,
}

fn coeffs(in_size: usize, in0: f64, in1: f64, out_size: usize, filter: Filter) -> Coeffs {
    let scale = (in1 - in0) / out_size as f64;
    let filterscale = scale.max(1.0);
    let support = filter.support() * filterscale;
    let ksize = (support.ceil() as usize) * 2 + 1;
    let mut bounds = Vec::with_capacity(out_size);
    let mut kk = vec![0i32; out_size * ksize];
    let mut k = vec![0.0f64; ksize];
    for xx in 0..out_size {
        let center = in0 + (xx as f64 + 0.5) * scale;
        let ss = 1.0 / filterscale;
        // C's (int) cast: toward zero
        let xmin = ((center - support + 0.5) as i64).max(0);
        let xmax = ((center + support + 0.5) as i64).min(in_size as i64) - xmin;
        let xmax = xmax.clamp(0, ksize as i64) as usize;
        let xmin = xmin as usize;
        let mut ww = 0.0;
        for (x, slot) in k.iter_mut().enumerate().take(xmax) {
            let w = filter.weight((x as f64 + xmin as f64 - center + 0.5) * ss);
            *slot = w;
            ww += w;
        }
        let row = &mut kk[xx * ksize..(xx + 1) * ksize];
        for (x, out) in row.iter_mut().enumerate() {
            let mut w = if x < xmax { k[x] } else { 0.0 };
            if x < xmax && ww != 0.0 {
                w /= ww;
            }
            let f = w * f64::from(1u32 << PRECISION_BITS);
            *out = if w < 0.0 { (-0.5 + f) as i32 } else { (0.5 + f) as i32 };
        }
        bounds.push((xmin, xmax));
    }
    Coeffs { ksize, bounds, kk }
}

fn clip8(v: i64) -> u8 {
    (v >> PRECISION_BITS).clamp(0, 255) as u8
}

/// One horizontal pass over rows `first..first + rows` of `src` (`ImagingResampleHorizontal_8bpc`).
fn horizontal(src: &Pic, out_w: usize, first: usize, rows: usize, c: &Coeffs) -> Pic {
    let ch = src.ch;
    let mut out = Pic::new(out_w, rows, ch);
    for yy in 0..rows {
        let srow = (yy + first) * src.w * ch;
        for xx in 0..out_w {
            let (xmin, xmax) = c.bounds[xx];
            let k = &c.kk[xx * c.ksize..xx * c.ksize + xmax];
            for b in 0..ch {
                let mut s: i64 = 1 << (PRECISION_BITS - 1);
                for (x, &w) in k.iter().enumerate() {
                    s += i64::from(src.px[srow + (xmin + x) * ch + b]) * i64::from(w);
                }
                out.px[(yy * out_w + xx) * ch + b] = clip8(s);
            }
        }
    }
    out
}

/// One vertical pass (`ImagingResampleVertical_8bpc`), the bounds relative to `src`'s first row.
fn vertical(src: &Pic, out_h: usize, c: &Coeffs, shift: usize) -> Pic {
    let ch = src.ch;
    let w = src.w;
    let mut out = Pic::new(w, out_h, ch);
    for yy in 0..out_h {
        let (ymin, ymax) = c.bounds[yy];
        let ymin = ymin - shift;
        let k = &c.kk[yy * c.ksize..yy * c.ksize + ymax];
        for xx in 0..w {
            for b in 0..ch {
                let mut s: i64 = 1 << (PRECISION_BITS - 1);
                for (y, &kw) in k.iter().enumerate() {
                    s += i64::from(src.px[((ymin + y) * w + xx) * ch + b]) * i64::from(kw);
                }
                out.px[(yy * w + xx) * ch + b] = clip8(s);
            }
        }
    }
    out
}

/// `ImagingResample` of an 8-bit picture of any channel count, the whole picture as the box.
fn resample_bands(src: &Pic, out_w: usize, out_h: usize, filter: Filter) -> Pic {
    let need_h = out_w != src.w;
    let need_v = out_h != src.h;
    let ch = coeffs(src.w, 0.0, src.w as f64, out_w, filter);
    let cv = coeffs(src.h, 0.0, src.h as f64, out_h, filter);
    let first = cv.bounds.first().map_or(0, |b| b.0);
    let last = cv.bounds.last().map_or(0, |b| b.0 + b.1);
    let mut img = None;
    let mut shift = 0;
    if need_h {
        img = Some(horizontal(src, out_w, first, last.saturating_sub(first), &ch));
        shift = first;
    }
    if need_v {
        let from = img.as_ref().unwrap_or(src);
        return vertical(from, out_h, &cv, shift);
    }
    img.unwrap_or_else(|| src.clone())
}

/// `MULDIV255(a, b)`.
fn muldiv255(a: u32, b: u32) -> u8 {
    let t = a * b + 128;
    (((t >> 8) + t) >> 8) as u8
}

/// `Image.fromarray(pic).resize((w, h), filter)`: 1 (L), 3 (RGB) or 4 (RGBA) channels. An empty size or a picture
/// of no pixels gives an empty picture of that size (Pillow refuses: the callers never ask).
#[must_use]
pub fn resize(src: &Pic, w: usize, h: usize, filter: Filter) -> Pic {
    if w == 0 || h == 0 || src.w == 0 || src.h == 0 {
        return Pic::new(w, h, src.ch);
    }
    if w == src.w && h == src.h {
        return src.clone();
    }
    if src.ch != 4 {
        return resample_bands(src, w, h, filter);
    }
    // RGBA -> RGBa, resampled, -> RGBA
    let mut pre = src.clone();
    for p in pre.px.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        for v in &mut p[..3] {
            *v = muldiv255(u32::from(*v), a);
        }
    }
    let mut out = resample_bands(&pre, w, h, filter);
    for p in out.px.chunks_exact_mut(4) {
        let a = u32::from(p[3]);
        if a != 255 && a != 0 {
            for v in &mut p[..3] {
                *v = ((255 * u32::from(*v)) / a).min(255) as u8;
            }
        }
    }
    out
}

/// `img.crop((x0, y0, x1, y1))` inside the picture.
#[must_use]
pub fn crop(src: &Pic, x0: usize, y0: usize, x1: usize, y1: usize) -> Pic {
    let (x1, y1) = (x1.min(src.w), y1.min(src.h));
    let (w, h) = (x1.saturating_sub(x0), y1.saturating_sub(y0));
    let mut out = Pic::new(w, h, src.ch);
    for y in 0..h {
        let s = ((y0 + y) * src.w + x0) * src.ch;
        out.px[y * w * src.ch..(y + 1) * w * src.ch].copy_from_slice(&src.px[s..s + w * src.ch]);
    }
    out
}
