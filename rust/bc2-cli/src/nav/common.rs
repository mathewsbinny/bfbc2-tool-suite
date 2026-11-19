//! What the nav group's command lines share: `ntpath`'s join / split, `os.walk`, `int()` of an argument, a list's
//! index and `bytes.hex(' ')`.

use std::fmt::Write as _;

use std::path::Path;

use crate::pyfmt::str_repr;
use crate::pyproc::{Fail, PyErr, PyResult};

fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// `ntpath.join(a, b)` for a `b` that is neither absolute nor has a drive (None: a part the port leaves to Python).
pub fn join(a: &str, b: &str) -> Option<String> {
    if b.starts_with(is_sep) || b.chars().nth(1) == Some(':') {
        return None;
    }
    let bare_drive = a.len() == 2 && a.ends_with(':');
    if a.is_empty() || a.ends_with(is_sep) || bare_drive {
        Some(format!("{a}{b}"))
    } else {
        Some(format!("{a}\\{b}"))
    }
}

/// `ntpath.join(a, *parts)`.
pub fn join_all<'a>(a: &str, parts: impl IntoIterator<Item = &'a str>) -> Option<String> {
    let mut out = a.to_owned();
    for p in parts {
        out = join(&out, p)?;
    }
    Some(out)
}

/// `ntpath.splitdrive(p)` for a drive letter or a UNC `\\server\share` (None: another form).
fn splitdrive(p: &str) -> Option<(&str, &str)> {
    match p.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => return Some(p.split_at(2)),
        [a, b, ..] if is_sep(char::from(*a)) && is_sep(char::from(*b)) => return None,
        _ => {}
    }
    Some(("", p))
}

/// `ntpath.split(p)`: (head, tail).
pub fn split(p: &str) -> Option<(String, String)> {
    let (d, rest) = splitdrive(p)?;
    let i = rest.rfind(is_sep).map_or(0, |k| k + 1);
    let (head, tail) = rest.split_at(i);
    let trimmed = head.trim_end_matches(is_sep);
    let head = if trimmed.is_empty() { head } else { trimmed };
    Some((format!("{d}{head}"), tail.to_owned()))
}

/// `os.path.dirname(p)`.
pub fn dirname(p: &str) -> Option<String> {
    split(p).map(|s| s.0)
}

/// `os.path.basename(p)`.
pub fn basename(p: &str) -> Option<String> {
    split(p).map(|s| s.1)
}

/// `os.path.isdir(p)`.
pub fn isdir(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_dir()
}

/// `os.path.isfile(p)`.
pub fn isfile(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_file()
}

/// `os.walk(top)` top-down: each folder's (path, folders, files) in the listing's order (`FindFirstFile`'s, as
/// `os.scandir`); a folder that cannot be listed is skipped; links to folders are listed but not entered.
pub fn walk(top: &str, out: &mut Vec<(String, Vec<String>, Vec<String>)>) -> PyResult<()> {
    let Ok(rd) = std::fs::read_dir(top) else {
        return Ok(());
    };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for e in rd.flatten() {
        let name = e.file_name().into_string().map_err(|_| Fail::Python)?;
        let is_dir = std::fs::metadata(e.path()).is_ok_and(|m| m.is_dir());
        if is_dir { dirs.push(name) } else { files.push(name) }
    }
    out.push((top.to_owned(), dirs.clone(), files));
    for d in dirs {
        let p = join(top, &d).ok_or(Fail::Python)?;
        let link = std::fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink());
        if !link {
            walk(&p, out)?;
        }
    }
    Ok(())
}

fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `int(s)` of an argument: Python's rules for ASCII digits (whitespace around, a sign, `_` between digits);
/// `ValueError` as Python words it; other digits are left to Python.
pub fn py_int(s: &str) -> PyResult<i64> {
    let bad = || {
        Fail::Err(PyErr::new(
            "ValueError",
            format!("invalid literal for int() with base 10: {}", str_repr(s)),
        ))
    };
    let t = s.trim_matches(py_space);
    let (neg, body) = match t.strip_prefix('-') {
        Some(b) => (true, b),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if body.is_empty() {
        return Err(bad());
    }
    if !body.is_ascii() {
        return Err(Fail::Python);
    }
    if body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return Err(bad());
    }
    let mut v: i64 = 0;
    for c in body.chars() {
        if c == '_' {
            continue;
        }
        let d = c.to_digit(10).ok_or_else(bad)?;
        v = v
            .checked_mul(10)
            .and_then(|v| v.checked_add(i64::from(d)))
            .ok_or(Fail::Python)?;
    }
    Ok(if neg { -v } else { v })
}

/// `list[i]` with Python's negative indexes; `IndexError` past either end.
pub fn index<T>(v: &[T], i: i64) -> PyResult<&T> {
    let n = i64::try_from(v.len()).unwrap_or(i64::MAX);
    let k = if i < 0 { i + n } else { i };
    usize::try_from(k)
        .ok()
        .and_then(|k| v.get(k))
        .ok_or_else(|| Fail::Err(PyErr::new("IndexError", "list index out of range")))
}

/// `args[i]` of the command line (`IndexError` when it is not there).
pub fn arg(args: &[String], i: usize) -> PyResult<&str> {
    args.get(i)
        .map(String::as_str)
        .ok_or_else(|| Fail::Err(PyErr::new("IndexError", "list index out of range")))
}

/// `list[:n]`'s length for a list of `len` items.
pub fn head_len(len: usize, n: i64) -> usize {
    let l = i64::try_from(len).unwrap_or(i64::MAX);
    let k = if n < 0 { (l + n).max(0) } else { n.min(l) };
    usize::try_from(k).unwrap_or(0)
}

/// `b.hex(' ')`.
pub fn hex_spaced(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect::<Vec<_>>().join(" ")
}

/// `repr(bytes)`: `b'...'`.
pub fn bytes_repr(b: &[u8]) -> String {
    let q = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut out = format!("b{q}");
    for &c in b {
        match c {
            b'\\' => out.push_str("\\\\"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            c if char::from(c) == q => {
                out.push('\\');
                out.push(q);
            }
            0x20..=0x7e => out.push(char::from(c)),
            c => {
                let _ = write!(out, "\\x{c:02x}");
            }
        }
    }
    out.push(q);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_and_ints_as_python() {
        assert_eq!(join("a", "b").as_deref(), Some("a\\b"));
        assert_eq!(join("a\\", "b").as_deref(), Some("a\\b"));
        assert_eq!(join("C:", "b").as_deref(), Some("C:b"));
        assert_eq!(split("C:\\x\\y"), Some(("C:\\x".to_owned(), "y".to_owned())));
        assert_eq!(split("C:\\y"), Some(("C:\\".to_owned(), "y".to_owned())));
        assert_eq!(split("x\\\\y"), Some(("x".to_owned(), "y".to_owned())));
        assert_eq!(split("y"), Some((String::new(), "y".to_owned())));
        assert_eq!(py_int(" +2 ").ok(), Some(2));
        assert_eq!(py_int("1_0").ok(), Some(10));
        assert!(matches!(py_int("1__0"), Err(Fail::Err(_))));
        assert_eq!(py_int("-7").ok(), Some(-7));
        assert_eq!(head_len(5, -1), 4);
        assert_eq!(head_len(5, 9), 5);
        assert_eq!(bytes_repr(b"Havok\0'"), "b\"Havok\\x00'\"");
    }
}
