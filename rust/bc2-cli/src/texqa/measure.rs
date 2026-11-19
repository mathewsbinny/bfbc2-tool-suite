//! texqa's measures with NumPy's arithmetic: a full reduction (`a.mean()`, `a.std()`) sums by NumPy's pairwise
//! rule (blocks of 8 accumulators up to 128, halves above), a reduction over axis 0 of an (N, C) array with C > 1
//! (`mean(0)`, `std(0)`) row by row, the SSIM windows' `mean((1, 3))` as 8 pairwise rows of 8 added in order; each
//! sum starts from 0.0 (the add identity). Python floats round with `round()` (correctly rounded); NumPy float64
//! scalars (the PSNR) round as `np.round` (x * 100, rint, / 100).
//!
//! The one measure NumPy leaves to BLAS, the correlation (`np.corrcoef`: a `dot` whose summation order is the
//! BLAS kernel's), is computed here in double-double and accepted only when its 4-decimal rounding cannot differ
//! within BLAS's error bound; otherwise the run is handed to the Python ([`Fail::Python`]).

use crate::pyproc::{Fail, PyResult};

/// NumPy's `pairwise_sum` (float64).
pub fn pw(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut r = 0.0;
        for &x in a {
            r += x;
        }
        return r;
    }
    if n <= 128 {
        let mut r = [0.0f64; 8];
        r.copy_from_slice(&a[..8]);
        let mut i = 8;
        while i < n - n % 8 {
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
        return res;
    }
    let mut n2 = n / 2;
    n2 -= n2 % 8;
    pw(&a[..n2]) + pw(&a[n2..])
}

/// `a.mean()` of a full array.
pub fn mean(a: &[f64]) -> f64 {
    (0.0 + pw(a)) / a.len() as f64
}

/// `a.std()` of a full array.
pub fn std(a: &[f64]) -> f64 {
    let m = mean(a);
    let d: Vec<f64> = a.iter().map(|x| (x - m) * (x - m)).collect();
    ((0.0 + pw(&d)) / a.len() as f64).sqrt()
}

/// `v.reshape(-1, c).mean(0)`.
pub fn mean0(v: &[f64], c: usize) -> Vec<f64> {
    let n = v.len() / c;
    if c == 1 {
        return vec![mean(v)];
    }
    (0..c)
        .map(|k| {
            let mut s = 0.0;
            for i in 0..n {
                s += v[i * c + k];
            }
            s / n as f64
        })
        .collect()
}

/// `v.reshape(-1, c).std(0)`.
pub fn std0(v: &[f64], c: usize) -> Vec<f64> {
    let n = v.len() / c;
    if c == 1 {
        return vec![std(v)];
    }
    let m = mean0(v, c);
    (0..c)
        .map(|k| {
            let mut s = 0.0;
            for i in 0..n {
                let d = v[i * c + k] - m[k];
                s += d * d;
            }
            (s / n as f64).sqrt()
        })
        .collect()
}

/// `np.round(x, d)` of a float64 scalar.
pub fn np_round(x: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (x * f).round_ties_even() / f
}

/// `round(x, n)` of a Python float.
pub fn py_round(x: f64, n: usize) -> f64 {
    crate::pyfmt::round_to(x, n)
}

/// An (h, w, c) float64 array in C order.
#[derive(Debug, Clone, PartialEq)]
pub struct Arr {
    pub h: usize,
    pub w: usize,
    pub c: usize,
    pub d: Vec<f64>,
}

impl Arr {
    /// `a[..., idx].astype(float64)` of an RGBA level.
    pub fn of(img: &super::decode::Img, idx: &[usize]) -> Arr {
        let mut d = Vec::with_capacity(img.w * img.h * idx.len());
        for p in img.px.chunks_exact(4) {
            for &i in idx {
                d.push(f64::from(p[i]));
            }
        }
        Arr {
            h: img.h,
            w: img.w,
            c: idx.len(),
            d,
        }
    }

    /// `a[..., idx]` (a copy, as NumPy's fancy index).
    pub fn pick(&self, idx: &[usize]) -> Arr {
        let mut d = Vec::with_capacity(self.h * self.w * idx.len());
        for p in self.d.chunks_exact(self.c) {
            for &i in idx {
                d.push(p[i]);
            }
        }
        Arr {
            h: self.h,
            w: self.w,
            c: idx.len(),
            d,
        }
    }

    /// One channel as a flat array (a strided view's values in order).
    pub fn chan(&self, k: usize) -> Vec<f64> {
        self.d.chunks_exact(self.c).map(|p| p[k]).collect()
    }

    fn get(&self, y: usize, x: usize) -> &[f64] {
        let o = (y * self.w + x) * self.c;
        &self.d[o..o + self.c]
    }

    /// An orientation of `ORIENT` (a view; read in its own C order).
    pub fn orient(&self, how: usize) -> Arr {
        let (h, w) = (self.h, self.w);
        let (nh, nw) = if how >= 3 { (w, h) } else { (h, w) };
        let mut d = Vec::with_capacity(self.d.len());
        for i in 0..nh {
            for j in 0..nw {
                let (y, x) = match how {
                    0 => (h - 1 - i, j),         // flipped vertically
                    1 => (i, w - 1 - j),         // mirrored
                    2 => (h - 1 - i, w - 1 - j), // rotated 180
                    3 => (j, i),                 // transposed
                    4 => (j, w - 1 - i),         // rotated 90 (np.rot90 k=1)
                    _ => (h - 1 - j, i),         // rotated 270 (k=3)
                };
                d.extend_from_slice(self.get(y, x));
            }
        }
        Arr {
            h: nh,
            w: nw,
            c: self.c,
            d,
        }
    }
}

/// `box(a)`: None where NumPy's broadcast fails (an odd side over 3).
pub fn box_(a: &Arr) -> PyResult<Arr> {
    let mut a = a.clone();
    if a.h > 1 {
        let (top, half) = (a.h.div_ceil(2), a.h / 2);
        let rows: Vec<usize> = if half == top {
            (0..top).collect()
        } else if half == 1 {
            vec![0; top]
        } else {
            return Err(Fail::Python);
        };
        let row = a.w * a.c;
        let mut d = Vec::with_capacity(top * row);
        for (i, &r1) in rows.iter().enumerate() {
            let (p, q) = (
                &a.d[2 * i * row..2 * i * row + row],
                &a.d[(2 * r1 + 1) * row..(2 * r1 + 2) * row],
            );
            d.extend(p.iter().zip(q).map(|(x, y)| (x + y) / 2.0));
        }
        a = Arr {
            h: top,
            w: a.w,
            c: a.c,
            d,
        };
    }
    if a.w > 1 {
        let (left, half) = (a.w.div_ceil(2), a.w / 2);
        if half != left && half != 1 {
            return Err(Fail::Python);
        }
        let mut d = Vec::with_capacity(a.h * left * a.c);
        for y in 0..a.h {
            for j in 0..left {
                let j1 = if half == left { j } else { 0 };
                let (p, q) = (a.get(y, 2 * j), a.get(y, 2 * j1 + 1));
                d.extend(p.iter().zip(q).map(|(x, y)| (x + y) / 2.0));
            }
        }
        a = Arr {
            h: a.h,
            w: left,
            c: a.c,
            d,
        };
    }
    Ok(a)
}

/// `to_size(a, h, w)`.
pub fn to_size(a: &Arr, h: usize, w: usize) -> PyResult<Option<Arr>> {
    let mut a = a.clone();
    while a.h > h && a.w > w {
        a = box_(&a)?;
    }
    Ok(if a.h == h && a.w == w { Some(a) } else { None })
}

/// `common(a, b)`.
pub fn common(a: &Arr, b: &Arr) -> PyResult<Option<(Arr, Arr)>> {
    if (a.h, a.w) == (b.h, b.w) {
        return Ok(Some((a.clone(), b.clone())));
    }
    if a.h >= b.h && a.w >= b.w {
        return Ok(to_size(a, b.h, b.w)?.map(|x| (x, b.clone())));
    }
    if b.h >= a.h && b.w >= a.w {
        return Ok(to_size(b, a.h, a.w)?.map(|y| (a.clone(), y)));
    }
    Ok(None)
}

/// `psnr(a, b)`: inf, or the dB rounded as NumPy rounds its float64.
pub fn psnr(a: &[f64], b: &[f64]) -> f64 {
    let mse = if a.is_empty() {
        0.0
    } else {
        let d: Vec<f64> = a.iter().zip(b).map(|(x, y)| (x - y) * (x - y)).collect();
        mean(&d)
    };
    if mse == 0.0 {
        f64::INFINITY
    } else {
        np_round(10.0 * (255.0f64 * 255.0 / mse).log10(), 2)
    }
}

/// `luma(a)`: (h, w).
pub fn luma(a: &Arr) -> Vec<f64> {
    a.d.chunks_exact(a.c)
        .map(|p| {
            if a.c >= 3 {
                p[0] * 0.299 + p[1] * 0.587 + p[2] * 0.114
            } else {
                let mut s = 0.0;
                for &x in p {
                    s += x;
                }
                s / a.c as f64
            }
        })
        .collect()
}

/// `ssim(a, b)`.
pub fn ssim(a: &Arr, b: &Arr) -> Option<f64> {
    let (x, y) = (luma(a), luma(b));
    let (h, w) = (a.h / 8 * 8, a.w / 8 * 8);
    if h == 0 || w == 0 {
        return None;
    }
    let (gh, gw) = (h / 8, w / 8);
    let full_w = a.w;
    // a window's mean((1, 3)) of f(texel)
    let win = |f: &dyn Fn(usize) -> f64, i: usize, j: usize| -> f64 {
        let mut s = 0.0;
        for k in 0..8 {
            let row: Vec<f64> = (0..8).map(|l| f((i * 8 + k) * full_w + j * 8 + l)).collect();
            s += pw(&row);
        }
        s / 64.0
    };
    let c1 = f64::from_bits(0x401A_028F_5C28_F5C4);
    let c2 = f64::from_bits(0x404D_42E1_47AE_147A);
    let mut s = Vec::with_capacity(gh * gw);
    for i in 0..gh {
        for j in 0..gw {
            let mx = win(&|p| x[p], i, j);
            let my = win(&|p| y[p], i, j);
            let vx = win(&|p| (x[p] - mx) * (x[p] - mx), i, j);
            let vy = win(&|p| (y[p] - my) * (y[p] - my), i, j);
            let cov = win(&|p| (x[p] - mx) * (y[p] - my), i, j);
            s.push(((2.0 * mx * my + c1) * (2.0 * cov + c2)) / ((mx * mx + my * my + c1) * (vx + vy + c2)));
        }
    }
    Some(py_round(mean(&s), 4))
}

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

/// The sum of `x[i] * y[i]` in double-double, rounded.
fn dot_dd(x: &[f64], y: &[f64]) -> f64 {
    let (mut hi, mut lo) = (0.0f64, 0.0f64);
    for (a, b) in x.iter().zip(y) {
        let p = a * b;
        let pe = a.mul_add(*b, -p);
        let (s, e) = two_sum(hi, p);
        hi = s;
        lo += e + pe;
    }
    let (s, e) = two_sum(hi, lo);
    s + e
}

/// `correlation(a, b)` of two arrays' values in order; `Err(Fail::Python)` where BLAS's summation order could
/// change the rounded answer.
pub fn correlation(x: &[f64], y: &[f64]) -> PyResult<f64> {
    if std(x) == 0.0 || std(y) == 0.0 {
        return Ok(if x == y { 1.0 } else { 0.0 });
    }
    let n = x.len();
    let (mx, my) = (mean(x), mean(y));
    let cx: Vec<f64> = x.iter().map(|v| v - mx).collect();
    let cy: Vec<f64> = y.iter().map(|v| v - my).collect();
    let k = 1.0 / (n as f64 - 1.0);
    let (sxx, syy, sxy) = (dot_dd(&cx, &cx), dot_dd(&cy, &cy), dot_dd(&cx, &cy));
    let r = ((sxy * k) / (sxx * k).sqrt()) / (syy * k).sqrt();
    let margin = 4.0 * (n as f64 + 16.0) * f64::EPSILON + 1e-13;
    let f = |v: f64| py_round(v.clamp(-1.0, 1.0), 4);
    let (lo, hi) = (f(r - margin), f(r + margin));
    if lo != hi || !r.is_finite() {
        return Err(Fail::Python);
    }
    Ok(f(r))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounds_as_numpy_and_python() {
        assert!((np_round(13.685, 2) - 13.68).abs() < 1e-12);
        assert!((py_round(13.685, 2) - 13.69).abs() < 1e-12);
    }
}
