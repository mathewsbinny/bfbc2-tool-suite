//! Python's float arithmetic and texts as `config.py`, `pipeline.py` and `validate.py` use them, bit for bit (round
//! O5): `round(x, n)` and `f'{x:.nf}'` (the exact decimal value, ties to even), `sum()` of floats (CPython 3.12's
//! Neumaier-compensated sum), `math.hypot` (CPython's own `vector_norm`: `bc2_navbuild::pymath`), `math.atan2`,
//! `math.cos` / `math.sin` (the C library's, as `f64`'s methods call), `math.degrees`, `math.floor`, and the `repr`
//! of the values the messages show.

#![allow(
    clippy::many_single_char_names,
    reason = "the arithmetic's own names (x, n, e, m, q, r)"
)]

use crate::pyfmt::{float_repr, str_repr};
use crate::pyproc::{Fail, PyResult};

/// x * 10^n exactly, rounded half to even to an integer: (sign, magnitude); None for a value that is not finite or
/// too large for the exact computation (the reference handles those; no position comes near).
fn scaled(x: f64, n: u32) -> Option<(bool, u128)> {
    if !x.is_finite() || x.abs() >= 1e24 {
        return None;
    }
    let neg = x.is_sign_negative();
    if x == 0.0 {
        return Some((neg, 0));
    }
    let bits = x.to_bits();
    let exp = i64::try_from((bits >> 52) & 0x7ff).ok()?;
    let frac = bits & ((1u64 << 52) - 1);
    let (mant, e) = if exp == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp - 1075)
    };
    let num = u128::from(mant).checked_mul(10u128.checked_pow(n)?)?;
    let v = if e >= 0 {
        let s = u32::try_from(e).ok()?;
        if s > 40 {
            return None;
        }
        num.checked_shl(s)?
    } else {
        let s = u32::try_from(-e).ok()?;
        if s >= 127 {
            0
        } else {
            let q = num >> s;
            let r = num - (q << s);
            let half = 1u128 << (s - 1);
            if r > half || (r == half && q & 1 == 1) {
                q + 1
            } else {
                q
            }
        }
    };
    Some((neg, v))
}

/// `round(x, n)` of a Python float (n 0..=6).
pub fn round(x: f64, n: u32) -> PyResult<f64> {
    let (neg, v) = scaled(x, n).ok_or(Fail::Python)?;
    if v > (1u128 << 53) {
        return Err(Fail::Python);
    }
    #[allow(
        clippy::cast_precision_loss,
        reason = "v <= 2^53 and 10^n exact: one correctly rounded division"
    )]
    let r = v as f64 / 10f64.powi(i32::try_from(n).unwrap_or(0));
    Ok(if neg { -r } else { r })
}

/// `f'{x:.nf}'`.
pub fn fixed(x: f64, n: u32) -> PyResult<String> {
    let (neg, v) = scaled(x, n).ok_or(Fail::Python)?;
    let p = 10u128.pow(n);
    let sign = if neg { "-" } else { "" };
    if n == 0 {
        return Ok(format!("{sign}{v}"));
    }
    let w = usize::try_from(n).unwrap_or(0);
    Ok(format!("{sign}{}.{:0w$}", v / p, v % p))
}

/// `sum(xs)` of Python floats (start 0): CPython 3.12+'s compensated (Neumaier) sum.
pub fn sum(xs: impl IntoIterator<Item = f64>) -> f64 {
    let mut f = 0.0f64;
    let mut c = 0.0f64;
    for x in xs {
        let t = f + x;
        if f.abs() >= x.abs() {
            c += (f - t) + x;
        } else {
            c += (x - t) + f;
        }
        f = t;
    }
    if c != 0.0 && c.is_finite() {
        f += c;
    }
    f
}

/// `math.hypot(x, y)`.
pub fn hypot(x: f64, y: f64) -> f64 {
    bc2_navbuild::pymath::hypot(x, y)
}

/// `repr(tuple(xs))` of floats.
pub fn float_tuple(xs: &[f64]) -> String {
    let items: Vec<String> = xs.iter().map(|x| float_repr(*x)).collect();
    if items.len() == 1 {
        format!("({},)", items.join(", "))
    } else {
        format!("({})", items.join(", "))
    }
}

/// `repr(list_of_str)`.
pub fn str_list(xs: &[String]) -> String {
    let items: Vec<String> = xs.iter().map(|x| str_repr(x)).collect();
    format!("[{}]", items.join(", "))
}

/// `math.floor(x)` as an int (None: not finite or out of i64's range - the reference's error).
pub fn floor(x: f64) -> PyResult<i64> {
    let f = x.floor();
    if !f.is_finite() || f.abs() >= 9.0e18 {
        return Err(Fail::Python);
    }
    #[allow(clippy::cast_possible_truncation, reason = "checked above")]
    Ok(f as i64)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn rounding_as_cpython() {
        assert_eq!(round(0.0625, 3).ok(), Some(0.062));
        assert_eq!(round(2.675, 2).ok(), Some(2.67));
        assert_eq!(round(0.25, 1).ok(), Some(0.2));
        assert_eq!(round(0.35, 1).ok(), Some(0.3)); // 0.35 is a hair under the tie
        assert_eq!(round(-12.25, 1).ok(), Some(-12.2));
        assert_eq!(fixed(0.25, 1).ok().as_deref(), Some("0.2"));
        assert_eq!(fixed(-0.04, 1).ok().as_deref(), Some("-0.0"));
        assert_eq!(fixed(1e9, 1).ok().as_deref(), Some("1000000000.0"));
        assert_eq!(fixed(5.96, 1).ok().as_deref(), Some("6.0"));
        assert_eq!(fixed(3.0, 3).ok().as_deref(), Some("3.000"));
        assert_eq!(sum([0.1, 0.2, 0.3]), 0.6);
        assert_eq!(sum([1e100, 1.0, -1e100]), 1.0);
        assert_eq!(float_tuple(&[1.0, -2.5, 3.25]), "(1.0, -2.5, 3.25)");
        assert_eq!(floor(-0.5).ok(), Some(-1));
    }
}
