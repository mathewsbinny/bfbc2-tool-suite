//! Round O4's `navkit` group: NavKit's driver (`bfbc2-auto-navkit\navkit.py`), the editor's level prepare
//! (`rust\bc2-editor\prepare_level.py`) and the tools prepare runs (`levelobjects`, `ladders`) or `check` / `compare`
//! report with (`ordercheck`, `navcompare`).
//!
//! What stays the Python's: `sample` (it starts the game; `all` runs it as a child between its Rust steps). `apply` on a
//! DICE base is answered here since round NK1 (`apply.rs`); `build`, `all` and `apply` on a NavKit base since round
//! NK5 (`build.rs`: NavKit's whole build in `bc2_navbuild::build`, `--no-detail` included).

// Python's lists indexed as the reference indexes them (a bad index is its IndexError, checked where it can be);
// counts as Python's floats (far below 2^52); the reference's single-letter names (x, y, z, m, t ...) and its long
// step-by-step functions kept for side-by-side reading
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::type_complexity
)]

use crate::pyfmt;
use crate::pyproc::{Fail, PyErr, PyResult, Tool};

mod apply;
pub(crate) mod build;
mod driver;
mod ladders;
mod levelobjects;
mod navcompare;
// round O5: stockmp's generate and validate place and check the bots on its PathData (config.Snapper)
pub(crate) mod ordercheck;
mod prepare;
mod project;

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "navkit",
        script: "bfbc2-auto-navkit\\navkit.py",
        log: Some("navkit"),
        run: driver::navkit,
    },
    Tool {
        name: "prepare_level",
        script: "rust\\bc2-editor\\prepare_level.py",
        log: None,
        run: driver::prepare_level,
    },
    Tool {
        name: "levelobjects",
        script: "bfbc2-auto-navkit\\tools\\levelobjects.py",
        log: Some("levelobjects"),
        run: levelobjects::run,
    },
    Tool {
        name: "ladders",
        script: "bfbc2-auto-navkit\\tools\\ladders.py",
        log: None,
        run: ladders::run,
    },
    Tool {
        name: "ordercheck",
        script: "bfbc2-auto-navkit\\tools\\ordercheck.py",
        log: Some("ordercheck"),
        run: ordercheck::run,
    },
    Tool {
        name: "navcompare",
        script: "bfbc2-auto-navkit\\tools\\navcompare.py",
        log: Some("navcompare"),
        run: navcompare::run,
    },
];

/// A `NavKitError(msg)`.
pub fn navkit_error(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("NavKitError", msg))
}

/// Python's whitespace (`str.strip()` / `str.split()`).
pub fn py_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n'
            | '\u{0b}'
            | '\u{0c}'
            | '\r'
            | '\u{1c}'..='\u{1f}'
                | ' '
                | '\u{85}'
                | '\u{a0}'
                | '\u{1680}'
                | '\u{2000}'..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
    )
}

/// `s.strip()`.
pub fn strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// `s.splitlines()`.
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let brk = matches!(
            c,
            '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
        );
        if brk {
            out.push(&s[start..i]);
            let mut next = i + c.len_utf8();
            if c == '\r'
                && let Some((_, '\n')) = it.peek()
            {
                it.next();
                next += 1;
            }
            start = next;
        }
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// `float(text)`: `ValueError` as Python words it (underscores between digits and Python's whitespace around taken;
/// other forms as Rust reads a float, which takes nothing Python refuses but those).
pub fn py_float(text: &str) -> PyResult<f64> {
    let s = strip(text);
    let bad = || {
        Fail::Err(PyErr::new(
            "ValueError",
            format!("could not convert string to float: {}", pyfmt::str_repr(text)),
        ))
    };
    let ok_chars = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-' | '_'));
    if s.is_empty() || !ok_chars {
        return Err(bad());
    }
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            let before = i
                .checked_sub(1)
                .and_then(|j| chars.get(j))
                .is_some_and(char::is_ascii_digit);
            let after = chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if !(before && after) {
                return Err(bad());
            }
        }
    }
    let plain: String = chars.into_iter().filter(|c| *c != '_').collect();
    plain.parse::<f64>().map_err(|_| bad())
}

/// `[float(a) for a in text.split('/')]`.
pub fn floats_of(text: &str) -> PyResult<Vec<f64>> {
    text.split('/').map(py_float).collect()
}

/// `open(path, encoding='utf-8', errors='replace').read()`: invalid bytes as U+FFFD, `\r\n` and `\r` as `\n`.
pub fn read_text_replace(path: &str) -> PyResult<String> {
    let data = crate::pyos::read(path)?;
    Ok(universal(&String::from_utf8_lossy(&data)))
}

/// `open(path, encoding='utf-8').read()`: an invalid byte raises `UnicodeDecodeError` (its text as Python's for a
/// byte that starts no sequence; other cases hand off).
pub fn read_text_utf8(path: &str) -> PyResult<String> {
    let data = crate::pyos::read(path)?;
    match String::from_utf8(data) {
        Ok(t) => Ok(universal(&t)),
        Err(e) => {
            let u = e.utf8_error();
            let at = u.valid_up_to();
            let b = e.as_bytes().get(at).copied().unwrap_or(0);
            if (u.error_len() == Some(1) && (0x80..0xC2).contains(&b)) || b >= 0xF5 {
                Err(Fail::Err(PyErr::new(
                    "UnicodeDecodeError",
                    format!("'utf-8' codec can't decode byte 0x{b:02x} in position {at}: invalid start byte"),
                )))
            } else {
                Err(Fail::Python)
            }
        }
    }
}

/// Python's text mode newline translation on reading.
pub fn universal(t: &str) -> String {
    if t.contains('\r') {
        t.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        t.to_owned()
    }
}

/// `repr(list_of_str)`.
pub fn str_list_repr<S: AsRef<str>>(items: &[S]) -> String {
    let parts: Vec<String> = items.iter().map(|s| pyfmt::str_repr(s.as_ref())).collect();
    format!("[{}]", parts.join(", "))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn text_as_python() {
        assert_eq!(splitlines("a\r\nb\rc\n\nd"), vec!["a", "b", "c", "", "d"]);
        assert_eq!(splitlines("a\n"), vec!["a"]);
        assert_eq!(splitlines(""), Vec::<&str>::new());
        assert!((py_float(" 1_0.5 ").unwrap() - 10.5).abs() < 1e-12);
        assert_eq!(py_float("-inf").unwrap(), f64::NEG_INFINITY);
        assert!(py_float("1__0").is_err());
        assert_eq!(str_list_repr(&["a", "b'c"]), "['a', \"b'c\"]");
    }
}
