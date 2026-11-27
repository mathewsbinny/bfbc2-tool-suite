//! Project OPFOR's WW2 pictures (round OR4, "OPFOR in Rust" stage 4): the NumPy and Pillow work of its picture
//! tools, pixel for pixel - so the WW2 rebuild's pictures stay the bytes its oracle holds.
//!
//! | Project OPFOR (Python until OR4) | here |
//! |---|---|
//! | `pictures.py`: `render`, `framed`, `bleed`, `down` (the loadout pictures' renderer) | [`weapon`] |
//! | `ww2_loading.py`: `picture`, `level_picture`, `check`, `level_check` | [`loading`] |
//! | `weapon_icons.py`: `dxt5_alpha`, `silhouette`, `fit`; `minimap_icons.py`: `box_colours`, `paint` | [`icons`] |
//! | Pillow's `Image.resize` (`LANCZOS`, `BOX`) for 8-bit pictures, `Image.crop` | [`resample`] |
//! | `type95_icon.py`: `silhouette` (round OR14; `onslaught/console_icons.py` uses it too) | [`side`] |
//! | Pillow's `ImageDraw.polygon(xy, fill=ink)` on an 8-bit picture | [`polygon`] |
//!
//! The DXT encoding stays the suite's `texwrite` (its principal axes `numpy.linalg.eigh`'s to the bit, computed in
//! [`crate::eigh3`] since round OR14); the preview sheets
//! (Pillow's text) stay OPFOR's Python.

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::needless_range_loop,
    clippy::doc_markdown,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::chunks_exact_to_as_chunks,
    clippy::too_many_lines,
    clippy::manual_is_multiple_of,
    clippy::if_not_else,
    reason = "pixel loops over pictures whose sizes are checked on entry; NumPy's names and its exact float compares \
              kept for review against the Python"
)]

pub mod icons;
pub mod loading;
pub mod polygon;
pub mod resample;
pub mod side;
pub mod weapon;

/// An 8-bit picture: `h` rows of `w` pixels of `ch` channels, row-major (a NumPy `(h, w, ch)` `uint8` array).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pic {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// Channels (1, 3 or 4).
    pub ch: usize,
    /// The pixels.
    pub px: Vec<u8>,
}

impl Pic {
    /// A black picture (every channel 0).
    #[must_use]
    pub fn new(w: usize, h: usize, ch: usize) -> Self {
        Self {
            w,
            h,
            ch,
            px: vec![0; w * h * ch],
        }
    }

    /// A picture of these pixels, `None` when their count is not `w * h * ch`.
    #[must_use]
    pub fn of(w: usize, h: usize, ch: usize, px: Vec<u8>) -> Option<Self> {
        (px.len() == w * h * ch).then_some(Self { w, h, ch, px })
    }
}

/// A float picture (`(h, w, ch)` `float64`), what NumPy computes between the 8-bit ends.
#[derive(Debug, Clone, PartialEq)]
pub struct FPic {
    /// Columns.
    pub w: usize,
    /// Rows.
    pub h: usize,
    /// Channels.
    pub ch: usize,
    /// The values.
    pub v: Vec<f64>,
}

impl FPic {
    /// A picture of zeros.
    #[must_use]
    pub fn zeros(w: usize, h: usize, ch: usize) -> Self {
        Self {
            w,
            h,
            ch,
            v: vec![0.0; w * h * ch],
        }
    }
}

/// NumPy's sum of a contiguous `float64` run (`pairwise_sum`: under 8 values in order, up to 128 in eight running
/// sums, more split in two halves at a multiple of 8) - `np.mean` / `np.sum` over a whole contiguous array.
#[must_use]
pub fn pairwise_sum(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut res = -0.0;
        for &x in a {
            res += x;
        }
        res
    } else if n <= 128 {
        let mut r = [0.0f64; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - (n % 8) {
            for j in 0..8 {
                r[j] += a[i + j];
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        while i < n {
            res += a[i];
            i += 1;
        }
        res
    } else {
        let mut n2 = n / 2;
        n2 -= n2 % 8;
        pairwise_sum(&a[..n2]) + pairwise_sum(&a[n2..])
    }
}

/// `np.clip(v, lo, hi)` of one value (NaN stays NaN).
#[must_use]
pub fn clip(v: f64, lo: f64, hi: f64) -> f64 {
    if v.is_nan() {
        return v;
    }
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// `.astype(np.uint8)` of a value already clipped to 0..255 (truncated; NaN as NumPy's cast gives it on x86: 0).
#[must_use]
pub fn to_u8(v: f64) -> u8 {
    if v.is_nan() { 0 } else { v as u8 }
}

/// `np.roll(np.roll(m, dy, 0), dx, 1)[y, x]` of an `h x w` grid: the value at `((y - dy) mod h, (x - dx) mod w)`.
#[must_use]
pub fn rolled(h: usize, w: usize, y: usize, x: usize, dy: isize, dx: isize) -> usize {
    let yy = (y as isize - dy).rem_euclid(h as isize) as usize;
    let xx = (x as isize - dx).rem_euclid(w as isize) as usize;
    yy * w + xx
}

/// The PSNR in dB of a picture against another (`10 log10(255² / max(mse, 1e-9))`, the mean of the squared
/// differences of every channel, summed as NumPy sums a contiguous array).
#[must_use]
pub fn psnr(a: &[f64], b: &[f64]) -> f64 {
    let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).collect();
    let mse = pairwise_sum(&d) / d.len() as f64;
    10.0 * (255.0f64 * 255.0 / mse.max(1e-9)).log10()
}
