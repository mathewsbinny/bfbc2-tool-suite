//! What round O4's `navtools` and `harness` groups share (included by both with `#[path]`, so either builds alone):
//! Python's `int()` / `float()` of an argument, `str()` / `repr()` of JSON values, `format(x, '.Nf')`, list slices,
//! and NavKit's `find_opfor` / `original_file` (re-exported from `crate::navshared`, which navkit uses too).

#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::float_cmp,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::type_complexity,
    reason = "a port of Python: indexes after their lengths are checked, ints as Python turns them into floats, == as Python compares floats, the Python's names and its functions' shape kept"
)]

use crate::pyfmt::{float_repr, str_repr};
use crate::pyjson::Value;
// (each including group uses its part: harness only find_opfor)
#[allow(unused_imports)]
pub use crate::navshared::{find_opfor, find_opfor_text, original_file};
use crate::pyproc::{Fail, PyErr, PyResult};

/// Python's whitespace for `int()` / `float()` (`str.isspace`).
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `int(s)`: ASCII digits, a sign, whitespace around, `_` between digits; `ValueError` as Python words it; other
/// digits are left to Python.
pub fn py_int(s: &str) -> PyResult<i64> {
    let bad = || {
        Fail::Err(PyErr::new(
            "ValueError",
            format!("invalid literal for int() with base 10: {}", str_repr(s)),
        ))
    };
    if !s.is_ascii() {
        return Err(Fail::Python);
    }
    let t = s.trim_matches(py_space);
    let (neg, body) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return Err(bad());
    }
    if !body.bytes().all(|c| c.is_ascii_digit() || c == b'_') {
        return Err(bad());
    }
    let digits: String = body.chars().filter(|c| *c != '_').collect();
    let v: i64 = digits.parse().map_err(|_| Fail::Python)?;
    Ok(if neg { -v } else { v })
}

/// `float(s)`: decimal text, `inf` / `infinity` / `nan` in any case, whitespace around; underscores and other
/// digits are left to Python; `ValueError` as Python words it.
pub fn py_float(s: &str) -> PyResult<f64> {
    let bad = || {
        Fail::Err(PyErr::new(
            "ValueError",
            format!("could not convert string to float: {}", str_repr(s)),
        ))
    };
    if !s.is_ascii() || s.contains('_') {
        return Err(Fail::Python);
    }
    let t = s.trim_matches(py_space);
    let (sign, body) = match t.as_bytes().first() {
        Some(b'-') => (-1.0, &t[1..]),
        Some(b'+') => (1.0, &t[1..]),
        _ => (1.0, t),
    };
    let low = body.to_ascii_lowercase();
    if low == "inf" || low == "infinity" {
        return Ok(sign * f64::INFINITY);
    }
    if low == "nan" {
        return Ok(f64::NAN.copysign(sign));
    }
    // Rust's grammar is Python's for plain decimals except these spellings
    if body.is_empty()
        || !body
            .bytes()
            .all(|c| c.is_ascii_digit() || matches!(c, b'.' | b'e' | b'E' | b'+' | b'-'))
        || !body.bytes().any(|c| c.is_ascii_digit())
    {
        return Err(bad());
    }
    body.parse::<f64>().map(|v| sign * v).map_err(|_| bad())
}

/// `repr()` of a JSON value as Python holds it.
pub fn repr(v: &Value) -> String {
    match v {
        Value::Null => "None".to_owned(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_owned(),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => float_repr(*f),
        Value::Str(s) => str_repr(s),
        Value::List(items) => format!("[{}]", items.iter().map(repr).collect::<Vec<_>>().join(", ")),
        Value::Dict(items) => format!(
            "{{{}}}",
            items
                .iter()
                .map(|(k, v)| format!("{}: {}", str_repr(k), repr(v)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

/// `str()` of a JSON value (an f-string's `{x}`).
pub fn py_str(v: &Value) -> String {
    match v {
        Value::Str(s) => s.clone(),
        other => repr(other),
    }
}

/// `format(x, '.{prec}f')` of a float.
pub fn fixed(x: f64, prec: usize) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    format!("{x:.prec$}")
}

/// A number's value as a float for `format(x, '.Nf')` (an int or a float; a bool or another type hands off).
pub fn number(v: &Value) -> PyResult<f64> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        _ => Err(Fail::Python),
    }
}

/// `lst[:n]`'s length for a list of `len` items (n may be negative).
pub fn head_len(len: usize, n: i64) -> usize {
    if n >= 0 {
        (n as usize).min(len)
    } else {
        len.saturating_sub(n.unsigned_abs() as usize)
    }
}

/// `f'{s:{w}s}'`: left-aligned, padded with spaces to `w` characters.
pub fn ljust(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.to_owned()
    } else {
        format!("{s}{}", " ".repeat(w - n))
    }
}

/// `f'{s:>{w}s}'`.
pub fn rjust(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w {
        s.to_owned()
    } else {
        format!("{}{s}", " ".repeat(w - n))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixed_as_python() {
        // format(x, '.Nf') rounds the exact binary value half to even
        assert_eq!(fixed(0.125, 2), "0.12");
        assert_eq!(fixed(0.375, 2), "0.38");
        assert_eq!(fixed(2.5, 0), "2");
        assert_eq!(fixed(1.5, 0), "2");
        assert_eq!(fixed(0.5, 0), "0");
        assert_eq!(fixed(-0.04, 1), "-0.0");
        assert_eq!(fixed(0.05, 1), "0.1");
        assert_eq!(fixed(f64::NAN, 1), "nan");
    }

    #[test]
    fn values_as_python() {
        let v = Value::List(vec![
            Value::Int(1),
            Value::Str("a'b".into()),
            Value::Null,
            Value::Float(0.5),
        ]);
        assert_eq!(repr(&v), "[1, \"a'b\", None, 0.5]");
        assert_eq!(py_str(&Value::Bool(true)), "True");
        assert_eq!(head_len(5, -2), 3);
        assert_eq!(head_len(5, 9), 5);
    }
}
