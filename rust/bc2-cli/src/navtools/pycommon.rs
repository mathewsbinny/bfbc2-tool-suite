//! What round O4's `navtools` and `harness` groups share (included by both with `#[path]`, so either builds alone):
//! Python's `int()` / `float()` of an argument, `str()` / `repr()` of JSON values, `format(x, '.Nf')`, list slices,
//! NavKit's `find_opfor` (`bfbc2core.paths`) and `original_file` (`navkit.py`).

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
use crate::pyos;
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

/// `bfbc2core.paths.walk_up(start)`: `<start>\project-opfor-bfbc2`, then the same in each folder above.
fn walk_up(start: &str) -> PyResult<Vec<String>> {
    let mut d = pyos::abspath(start)?;
    let mut out = vec![pyos::join2(&d, "project-opfor-bfbc2")?];
    loop {
        let up = pyos::dirname(&d)?;
        if up == d {
            break;
        }
        d = up;
        out.push(pyos::join2(&d, "project-opfor-bfbc2")?);
    }
    Ok(out)
}

/// The suite folder the Python tools sit in (NavKit's `find_opfor` walks up from it).
pub fn suite() -> PyResult<String> {
    let s = crate::pyproc::suite_root()
        .into_os_string()
        .into_string()
        .map_err(|_| Fail::Python)?;
    pyos::abspath(&s)
}

/// `bfbc2core.paths.find_opfor(given, start=<the suite>)`: Ok(folder) or Err(GameCopyNotFound's text).
pub fn find_opfor_text(given: Option<&str>, env: bool) -> PyResult<Result<String, String>> {
    let mut looked: Vec<String> = Vec::new();
    for c in [
        given.map(str::to_owned),
        if env { std::env::var("OPFOR_ROOT").ok() } else { None },
    ]
    .into_iter()
    .flatten()
    {
        if !c.is_empty() {
            looked.push(c);
        }
    }
    looked.extend(walk_up(&suite()?)?);
    for c in &looked {
        if pyos::isdir(&pyos::join2(c, "game")?) {
            return Ok(Ok(pyos::abspath(c)?));
        }
    }
    Ok(Err(format!(
        "Project OPFOR BFBC2 not found - pass --opfor <folder> or set OPFOR_ROOT (looked for a game\\ folder in: {})",
        looked.join(", ")
    )))
}

/// `navkit.find_opfor(given)`: the folder, or `NavKitError` with `GameCopyNotFound`'s text (`kind` names the class as
/// the caller's traceback or report shows it).
pub fn find_opfor(given: Option<&str>, kind: &str) -> PyResult<String> {
    find_opfor_text(given, true)?.map_err(|m| Fail::Err(PyErr::new(kind, m)))
}

/// `navkit.original_file(p)`: DICE's original of a game file (the earliest OPFOR backup), or None.
pub fn original_file(p: &str) -> PyResult<Option<String>> {
    let (d, base) = pyos::split(p)?;
    let Ok(listing) = pyos::listdir(if d.is_empty() { "." } else { &d }) else {
        return Ok(None);
    };
    // {f.lower(): f}: the first position of a lower-case name, the last spelling
    let mut names: Vec<(String, String)> = Vec::new();
    for f in listing {
        let low = f.to_lowercase();
        if let Some(slot) = names.iter_mut().find(|(k, _)| *k == low) {
            slot.1 = f;
        } else {
            names.push((low, f));
        }
    }
    let pre = format!("{}.opfor-", base.to_lowercase());
    let ours: Vec<&(String, String)> = names.iter().filter(|(k, _)| k.starts_with(&pre)).collect();
    let has = |k: &str| ours.iter().any(|(o, _)| o == k);
    if has(&format!("{pre}new")) {
        let own = format!("{pre}orig");
        return Ok(match ours.iter().find(|(o, _)| *o == own) {
            Some((_, f)) => Some(pyos::join2(&d, f)?),
            None => None,
        });
    }
    if ours.iter().any(|(o, _)| {
        o.strip_suffix("-applied")
            .is_some_and(|stem| !has(&format!("{stem}-orig")))
    }) {
        return Ok(None);
    }
    let mut best: Option<((f64, bool), String)> = None;
    for (o, f) in &ours {
        if !o.ends_with("-orig") {
            continue;
        }
        let b = pyos::join2(&d, f)?;
        if !pyos::isfile(&b) {
            continue;
        }
        let key = (pyos::getmtime(&b)?, !b.to_lowercase().ends_with(".opfor-orig"));
        if best
            .as_ref()
            .is_none_or(|(k, _)| key.0 < k.0 || (key.0 == k.0 && !key.1 && k.1))
        {
            best = Some((key, b));
        }
    }
    if let Some((_, b)) = best {
        return Ok(Some(b));
    }
    Ok(pyos::isfile(p).then(|| p.to_owned()))
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
