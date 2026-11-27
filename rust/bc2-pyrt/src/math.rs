//! CPython's float operations where they are not a plain Rust expression, bit for bit (round OR9: Project OPFOR's
//! console Onslaught stages pick cull slots by `math.dist` and doorway pre-destructions by `** 2` / `** 0.5`; a
//! last-bit difference can change which object is nearest).
//!
//! - `math.dist(p, q)` is CPython's own `vector_norm` (3.8+: scaled, compensated, a differential correction), not
//!   `sqrt` of the squares' sum: [`dist3`].
//! - `x ** 2` calls the C library's `pow(|x|, 2.0)` (CPython's `float_pow` takes the sign off an integral power), which
//!   is not always `x * x`: [`sq`]; `x ** 0.5` calls `pow(x, 0.5)`, not `sqrt`: [`pow`].
//!
//! The same `vector_norm` is in `bc2_navbuild::pymath`, which keeps its own so the navigation build does not link the
//! Python-runtime emulation (rust/BENCHMARKS.md "OR9", "What is left").

#![allow(clippy::many_single_char_names, reason = "CPython's names")]

use std::hint::black_box;

/// `math.frexp(x)[1]` for a finite x > 0.
fn frexp_exp(x: f64) -> i32 {
    let e = i32::try_from((x.to_bits() >> 52) & 0x7ff).unwrap_or(0);
    if e == 0 {
        // subnormal: scaled by 2 ** 54 (exact)
        return frexp_exp(x * f64::from_bits((1023 + 54) << 52)) - 54;
    }
    e - 1022
}

/// `math.ldexp(1.0, n)` for -1074 <= n <= 1023.
fn pow2(n: i32) -> f64 {
    if n >= -1022 {
        f64::from_bits(u64::try_from(n + 1023).unwrap_or(0) << 52)
    } else {
        f64::from_bits(1u64 << u32::try_from(n + 1074).unwrap_or(0))
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

/// CPython's `math.dist` over the absolute differences `v` (its `vector_norm` with the checks before it).
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

/// `math.dist(p, q)` for two points of three coordinates.
#[must_use]
pub fn dist3(p: [f64; 3], q: [f64; 3]) -> f64 {
    norm(&[(p[0] - q[0]).abs(), (p[1] - q[1]).abs(), (p[2] - q[2]).abs()])
}

/// `x ** 2` on a Python float: the C library's `pow(|x|, 2.0)`.
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

/// `x ** y` on two Python floats where x >= 0 (CPython's `float_pow`: the C library's `pow`, kept from being folded
/// into `sqrt` for y = 0.5).
#[must_use]
pub fn pow(x: f64, y: f64) -> f64 {
    x.powf(black_box(y))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_values() {
        assert_eq!(dist3([0.0, 0.0, 0.0], [3.0, 4.0, 12.0]).to_bits(), 13.0f64.to_bits());
        assert_eq!(dist3([1.0, 1.0, 1.0], [1.0, 1.0, 1.0]).to_bits(), 0.0f64.to_bits());
        assert!(dist3([f64::NAN, 0.0, 0.0], [0.0, 0.0, 0.0]).is_nan());
        assert_eq!(sq(-3.0).to_bits(), 9.0f64.to_bits());
        assert_eq!(pow(16.0, 0.5).to_bits(), 4.0f64.to_bits());
        // CPython 3.13: math.dist((0.1, 0.2, 0.3), (1.7, -2.9, 3.3)) == 4.601086828130937
        assert_eq!(dist3([0.1, 0.2, 0.3], [1.7, -2.9, 3.3]), 4.601_086_828_130_937);
    }
}
