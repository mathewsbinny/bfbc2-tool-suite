//! CPython's float functions where they are not the C library's, bit for bit: the walkability rules and the graph's
//! cell picking compute with them, and a last-bit difference can flip a threshold.
//!
//! - `math.hypot(x, y)` and `math.dist(p, q)` are CPython's own `vector_norm` (3.8+: scaled, compensated, a
//!   differential correction), not the C library's `hypot`: [`hypot`], [`dist3`], [`dist2`].
//! - `x // w` on floats is CPython's `float_floor_div` (fmod, then the quotient rounded): [`floordiv`].
//! - `x ** 2` on a Python float calls the C library's `pow(|x|, 2.0)`, which is not always `x * x` (1 in 2,000 here):
//!   [`sq`] calls the same `pow`, kept from being folded into a multiply.
//! - `math.atan` is the C library's (`f64::atan` calls the same function); `math.degrees` multiplies by
//!   `180 / pi` as [`f64::to_degrees`] does.
#![allow(clippy::many_single_char_names, reason = "CPython's names")]

use std::hint::black_box;

/// `math.frexp(x)[1]` for a finite x > 0.
fn frexp_exp(x: f64) -> i32 {
    let e = ((x.to_bits() >> 52) & 0x7ff) as i32;
    if e == 0 {
        // subnormal: scaled by 2 ** 54 (exact)
        return frexp_exp(x * f64::from_bits((1023 + 54) << 52)) - 54;
    }
    e - 1022
}

/// `math.ldexp(1.0, n)` for -1074 <= n <= 1023.
fn pow2(n: i32) -> f64 {
    if n >= -1022 {
        f64::from_bits(((n + 1023) as u64) << 52)
    } else {
        f64::from_bits(1u64 << (n + 1074))
    }
}

/// Dekker's split (`dl_split`).
fn split(x: f64) -> (f64, f64) {
    let t = x * 134_217_729.0; // 2 ** 27 + 1
    let hi = t - (t - x);
    (hi, x - hi)
}

/// Dekker's exact product (`dl_mul`): hi + lo = x * y.
fn mul(x: f64, y: f64) -> (f64, f64) {
    let (xh, xl) = split(x);
    let (yh, yl) = split(y);
    let p = xh * yh;
    let q = xh * yl + xl * yh;
    let z = p + q;
    (z, p - z + q + xl * yl)
}

/// `dl_fast_sum` (|a| >= |b|): s + t = a + b.
fn fast_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    (s, (a - s) + b)
}

/// CPython's `vector_norm` of non-negative finite values with their maximum `max` (> 0).
fn vector_norm(vec: &[f64], max: f64) -> f64 {
    let max_e = frexp_exp(max);
    if max_e < -1023 {
        // ldexp(1.0, -max_e) would overflow: subnormals to normals first
        let scaled: Vec<f64> = vec.iter().map(|v| v / f64::MIN_POSITIVE).collect();
        return f64::MIN_POSITIVE * vector_norm(&scaled, max / f64::MIN_POSITIVE);
    }
    let scale = pow2(-max_e);
    let (mut csum, mut frac1, mut frac2) = (1.0f64, 0.0f64, 0.0f64);
    for &v in vec {
        let x = v * scale;
        let (hi, lo) = mul(x, x);
        let (s, t) = fast_sum(csum, hi);
        csum = s;
        frac1 += lo;
        frac2 += t;
    }
    let mut h = (csum - 1.0 + (frac1 + frac2)).sqrt();
    let (hi, lo) = mul(-h, h);
    let (s, t) = fast_sum(csum, hi);
    csum = s;
    frac1 += lo;
    frac2 += t;
    let x = csum - 1.0 + (frac1 + frac2);
    h += x / (2.0 * h);
    h / scale
}

/// CPython's `math.hypot` / `math.dist` over the absolute values `v` (its `vector_norm` with the checks before it).
fn norm(v: &[f64]) -> f64 {
    let mut max = 0.0f64;
    let mut nan = false;
    for &a in v {
        nan |= a.is_nan();
        if a > max {
            max = a;
        }
    }
    if max.is_infinite() {
        return max;
    }
    if nan {
        return f64::NAN;
    }
    if max == 0.0 || v.len() <= 1 {
        return max;
    }
    vector_norm(v, max)
}

/// `math.hypot(x, y)`.
#[must_use]
pub fn hypot(x: f64, y: f64) -> f64 {
    norm(&[x.abs(), y.abs()])
}

/// `math.dist(p, q)` for two points of three coordinates.
#[must_use]
pub fn dist3(p: (f64, f64, f64), q: (f64, f64, f64)) -> f64 {
    norm(&[(p.0 - q.0).abs(), (p.1 - q.1).abs(), (p.2 - q.2).abs()])
}

/// `math.dist(p, q)` for two points of two coordinates.
#[must_use]
pub fn dist2(p: (f64, f64), q: (f64, f64)) -> f64 {
    norm(&[(p.0 - q.0).abs(), (p.1 - q.1).abs()])
}

/// `x // w` on Python floats (CPython's `float_floor_div`, NumPy's `floor_divide` the same): `None` for w = 0
/// (`ZeroDivisionError`).
#[must_use]
pub fn floordiv(x: f64, w: f64) -> Option<f64> {
    if w == 0.0 {
        return None;
    }
    let m = x % w; // fmod
    let mut div = (x - m) / w;
    if m != 0.0 && (w < 0.0) != (m < 0.0) {
        div -= 1.0;
    }
    Some(if div == 0.0 {
        0.0f64.copysign(x / w)
    } else {
        let f = div.floor();
        if div - f > 0.5 { f + 1.0 } else { f }
    })
}

/// `x ** 2` on a Python float: the C library's `pow(|x|, 2.0)` (CPython's `float_pow` takes the sign off for an
/// integral power first).
#[must_use]
pub fn sq(x: f64) -> f64 {
    if x.is_nan() {
        return x;
    }
    if x == 0.0 {
        return 0.0;
    }
    x.abs().powf(black_box(2.0))
}

/// `math.degrees(math.atan(math.hypot(gx, gz)))`: the slope (degrees) of a gradient.
#[must_use]
pub fn slope_deg(gx: f64, gz: f64) -> f64 {
    hypot(gx, gz).atan().to_degrees()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn hypot_edges() {
        assert_eq!(hypot(3.0, 4.0), 5.0);
        assert_eq!(hypot(-3.0, 4.0), 5.0);
        assert_eq!(hypot(0.0, -0.0), 0.0);
        assert_eq!(hypot(0.0, 2.5), 2.5);
        assert!(hypot(f64::NAN, 1.0).is_nan());
        assert_eq!(hypot(f64::NAN, f64::NEG_INFINITY), f64::INFINITY);
        // subnormals (both scaling branches) and the largest values
        let tiny = f64::from_bits(1);
        assert_eq!(hypot(tiny, 0.0), tiny);
        assert_eq!(hypot(3.0 * tiny, 4.0 * tiny), 5.0 * tiny);
        let sub = f64::MIN_POSITIVE / 4.0;
        assert_eq!(hypot(3.0 * sub, 4.0 * sub), 5.0 * sub);
        assert_eq!(hypot(f64::MAX, 0.0), f64::MAX);
        assert_eq!(frexp_exp(1.0), 1);
        assert_eq!(frexp_exp(0.75), 0);
        assert_eq!(frexp_exp(tiny), -1073);
        assert_eq!(pow2(-1074), tiny);
        assert_eq!(pow2(1023), 2f64.powi(1023));
    }

    #[test]
    fn dists_and_floordiv() {
        assert_eq!(dist3((0.0, 0.0, 0.0), (1.0, 2.0, 2.0)), 3.0);
        assert_eq!(dist3((1.0, 1.0, 1.0), (1.0, 1.0, 1.0)), 0.0);
        assert!(dist3((f64::NAN, 0.0, 0.0), (0.0, 0.0, 0.0)).is_nan());
        assert_eq!(dist2((0.0, 0.0), (3.0, -4.0)), 5.0);
        assert_eq!(floordiv(7.0, 2.0), Some(3.0));
        assert_eq!(floordiv(-7.0, 2.0), Some(-4.0));
        assert_eq!(floordiv(7.0, -2.0), Some(-4.0));
        assert_eq!(floordiv(-0.5, 15.0), Some(-1.0));
        assert_eq!(floordiv(0.0, 15.0).map(f64::to_bits), Some(0.0f64.to_bits()));
        assert_eq!(floordiv(-0.0, 15.0).map(f64::to_bits), Some((-0.0f64).to_bits()));
        assert_eq!(floordiv(1.0, 0.0), None);
        // (x - fmod) / w is not always integral: CPython rounds it to the nearer integer
        assert_eq!(floordiv(1.0, 0.1), Some(9.0));
    }

    #[test]
    fn squares_and_degrees() {
        assert_eq!(sq(3.0), 9.0);
        assert_eq!(sq(-3.0), 9.0);
        assert_eq!(sq(-0.0).to_bits(), 0.0f64.to_bits());
        assert!(sq(f64::NAN).is_nan());
        assert_eq!(sq(f64::NEG_INFINITY), f64::INFINITY);
        assert_eq!(180.0 / std::f64::consts::PI, 1.0f64.to_degrees());
        assert_eq!(slope_deg(1.0, 0.0), 45.0);
        assert_eq!(slope_deg(0.0, 0.0), 0.0);
    }
}
