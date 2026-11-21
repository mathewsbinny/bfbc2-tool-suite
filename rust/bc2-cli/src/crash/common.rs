//! What `exeref.py` and `crashreader.py` share: the symbols file (`read_symbols`), the game copy's exe
//! (`crashreader.find_exe`), Python's `int()` of the options, its whitespace, `ntpath`'s joins and the local time
//! of a file's modification time (`datetime.fromtimestamp`, `time.localtime`).

use std::path::Path;
use std::sync::LazyLock;

use bc2_harness::exe::Syms;
use regex::Regex;

use crate::pyfmt::str_repr;
use crate::pyproc::{Fail, Py, PyErr, PyResult, os_error, suite_root};

/// `crashreaderlib.py`'s source (its `read_symbols`' lines name the warnings it logs; round RT3 moved the library
/// there from the retired `crashreader.py`).
pub const CRASHREADER: &str =
    include_str!("../../../../bfbc2-automated-map-and-bot-test-harness/crash-reader/crashreaderlib.py");

/// The crash reader's folder, as `crashreader.HERE` (the suite's).
pub fn here() -> String {
    let root = suite_root();
    let root = abspath(&root.to_string_lossy());
    join(&join(&root, "bfbc2-automated-map-and-bot-test-harness"), "crash-reader")
}

/// `str.isspace()`.
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

/// `s.split()`.
pub fn split(s: &str) -> Vec<&str> {
    s.split(py_space).filter(|p| !p.is_empty()).collect()
}

/// Whether the Python and the regex crate read `\s` alike in `s`: `\x1c`..`\x1f` are Python's whitespace only.
pub fn regex_safe(s: &str) -> bool {
    !s.chars().any(|c| ('\u{1c}'..='\u{1f}').contains(&c))
}

fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// `ntpath.splitdrive` for a drive letter (`C:`) or a UNC share (`\\server\share`).
fn splitdrive(p: &str) -> (&str, &str) {
    let mut it = p.chars();
    let (first, second) = (it.next(), it.next());
    if let (Some(a), Some(':')) = (first, second) {
        let k = a.len_utf8() + 1;
        return (&p[..k], &p[k..]);
    }
    if first.is_some_and(is_sep) && second.is_some_and(is_sep) {
        // \\server\share: the drive is up to the separator after the share
        let rest = &p[2..];
        if let Some(i) = rest.find(is_sep) {
            let after = &rest[i + 1..];
            let j = after.find(is_sep).unwrap_or(after.len());
            let k = 2 + i + 1 + j;
            return (&p[..k], &p[k..]);
        }
        return (p, "");
    }
    ("", p)
}

/// `ntpath.join(a, b)` for a relative `b` without a drive.
pub fn join(a: &str, b: &str) -> String {
    if a.is_empty() || a.ends_with(is_sep) || (a.len() == 2 && a.ends_with(':')) {
        format!("{a}{b}")
    } else {
        format!("{a}\\{b}")
    }
}

/// `ntpath.dirname(p)`.
pub fn dirname(p: &str) -> String {
    let (d, rest) = splitdrive(p);
    let i = rest.rfind(is_sep).map_or(0, |i| i + 1);
    let head = &rest[..i];
    let stripped = head.trim_end_matches(is_sep);
    let head = if stripped.is_empty() { head } else { stripped };
    format!("{d}{head}")
}

/// `ntpath.abspath(p)` for an absolute path (separators made `\`, `.` and `..` resolved); a relative one is
/// joined to the working folder first.
pub fn abspath(p: &str) -> String {
    let p = if splitdrive(p).0.is_empty() {
        std::env::current_dir().map_or_else(|_| p.to_owned(), |c| join(&c.to_string_lossy(), p))
    } else {
        p.to_owned()
    };
    let p = p.replace('/', "\\");
    let (d, rest) = splitdrive(&p);
    let mut parts: Vec<&str> = Vec::new();
    for part in rest.split('\\') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            x => parts.push(x),
        }
    }
    format!("{d}\\{}", parts.join("\\"))
}

/// `os.path.isfile(p)`.
pub fn isfile(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_file()
}

/// `crashreader.find_exe()`: `paths.find_game_exe(start=<the harness folder>)`: `OPFOR_ROOT`'s
/// `game\BFBC2Game.exe`, else that of a `project-opfor-bfbc2` folder in the harness folder or any folder above it.
pub fn find_exe() -> Option<String> {
    let mut cands: Vec<String> = Vec::new();
    if let Ok(v) = std::env::var("OPFOR_ROOT")
        && !v.is_empty()
    {
        cands.push(v);
    }
    let mut d = abspath(&dirname(&here()));
    loop {
        cands.push(join(&d, "project-opfor-bfbc2"));
        let up = dirname(&d);
        if up == d {
            break;
        }
        d = up;
    }
    cands
        .into_iter()
        .map(|c| join(&join(&c, "game"), "BFBC2Game.exe"))
        .find(|p| isfile(p))
}

static SYMBOL_LINE: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"^\s*(?:0x)?([0-9A-Fa-f]{6,8})\s+([^|]+?)\s*(?:\|\s*(.*))?$").ok());

/// `crashreader.read_symbols(paths)`: address -> (name, hint), later files and lines winning. A file that does not
/// decode as UTF-8 (the Python raises `UnicodeDecodeError` at a position of its own reading) goes to the Python.
///
/// A path of `None` is the Python's `True` (`exeref.take` of a `--symbols` without a value): `os.path.isfile(True)`
/// stats file descriptor 1 (stdout, a pipe or a console: not a file) and warns once, which errors.setup logs.
pub fn read_symbols(py: &mut Py, paths: &[Option<String>]) -> PyResult<Syms> {
    let mut syms = Syms::new();
    let re = SYMBOL_LINE.as_ref().ok_or(Fail::Python)?;
    if paths.iter().any(Option::is_none) {
        // the line of `if not os.path.isfile(p):` in crashreaderlib.read_symbols
        let src = CRASHREADER.replace("\r\n", "\n");
        let def = src.find("def read_symbols").unwrap_or(0);
        let at = src[def..].find("if not os.path.isfile(p):").map_or(0, |k| def + k);
        let lineno = src[..at].matches('\n').count() + 1;
        py.log(
            "WARNING",
            format!(
                "{}:{lineno}: RuntimeWarning: bool is used as a file descriptor\n  if not os.path.isfile(p):\n",
                join(&here(), "crashreaderlib.py")
            ),
        );
    }
    for p in paths.iter().flatten() {
        if !isfile(p) {
            continue;
        }
        let data = std::fs::read(p).map_err(|e| os_error(p, &e))?;
        let text = String::from_utf8(data).map_err(|_| Fail::Python)?;
        if !regex_safe(&text) {
            return Err(Fail::Python);
        }
        // universal newlines: \r\n, \r and \n end a line
        let text = text.replace("\r\n", "\n").replace('\r', "\n");
        for raw in text.split_inclusive('\n') {
            let line = raw.split('#').next().unwrap_or("").trim_end_matches(py_space);
            if let Some(m) = re.captures(line) {
                let (Some(a), Some(n)) = (m.get(1), m.get(2)) else {
                    continue;
                };
                let Ok(addr) = u32::from_str_radix(a.as_str(), 16) else {
                    continue;
                };
                let hint = m.get(3).map_or("", |h| h.as_str());
                syms.insert(addr, (strip(n.as_str()).to_owned(), strip(hint).to_owned()));
            }
        }
    }
    Ok(syms)
}

/// Python's `int(s, base)` (base 0, 10 or 16) of ASCII text, or its `ValueError`; text Python reads with other
/// digits (Unicode ones) goes to the Python.
pub fn py_int(s: &str, base: u32) -> PyResult<i128> {
    let bad = || -> Fail {
        PyErr::new(
            "ValueError",
            format!("invalid literal for int() with base {base}: {}", str_repr(s)),
        )
        .into()
    };
    let t = strip(s);
    if !t.is_ascii() {
        return Err(Fail::Python);
    }
    let (neg, t) = match t.as_bytes().first() {
        Some(b'-') => (true, &t[1..]),
        Some(b'+') => (false, &t[1..]),
        _ => (false, t),
    };
    let low = t.to_ascii_lowercase();
    let (radix, digits, prefixed) = match base {
        16 => match low.strip_prefix("0x") {
            Some(d) => (16, d.to_owned(), true),
            None => (16, low.clone(), false),
        },
        0 => {
            if let Some(d) = low.strip_prefix("0x") {
                (16, d.to_owned(), true)
            } else if let Some(d) = low.strip_prefix("0o") {
                (8, d.to_owned(), true)
            } else if let Some(d) = low.strip_prefix("0b") {
                (2, d.to_owned(), true)
            } else {
                (10, low.clone(), false)
            }
        }
        _ => (10, low.clone(), false),
    };
    // underscores: one at a time between digits (after a base prefix, one may lead)
    let mut body = digits.as_str();
    if prefixed && let Some(r) = body.strip_prefix('_') {
        body = r;
    }
    if body.is_empty() || body.starts_with('_') || body.ends_with('_') || body.contains("__") {
        return Err(bad());
    }
    let clean: String = body.chars().filter(|&c| c != '_').collect();
    if !clean.chars().all(|c| c.is_digit(radix)) {
        return Err(bad());
    }
    if base == 0 && !prefixed && clean.len() > 1 && clean.starts_with('0') && clean.chars().any(|c| c != '0') {
        return Err(bad());
    }
    let v = i128::from_str_radix(&clean, radix).map_err(|_| Fail::Python)?;
    Ok(if neg { -v } else { v })
}

/// `f'{v:08X}'` (and its kin) of a Python int that may be negative: the sign, then the digits padded to `width`
/// in all.
pub fn hex_width(v: i128, width: usize) -> String {
    if v < 0 {
        format!("-{:0w$X}", v.unsigned_abs(), w = width.saturating_sub(1))
    } else {
        format!("{v:0width$X}")
    }
}

/// `f'{v:#x}'` of a Python int.
pub fn hex_alt(v: i128) -> String {
    if v < 0 {
        format!("-{:#x}", v.unsigned_abs())
    } else {
        format!("{v:#x}")
    }
}

/// `os.path.getmtime(p)`: the modification time as Python's float (`sec + nsec * 1e-9`).
#[allow(clippy::cast_precision_loss)] // Python's own float of the seconds (a time_t far below 2^52)
pub fn getmtime(path: &str) -> PyResult<f64> {
    let meta = std::fs::metadata(path).map_err(|e| os_error(path, &e))?;
    let modified = meta.modified().map_err(|_| Fail::Python)?;
    let secs = |d: std::time::Duration| d.as_secs() as f64 + f64::from(d.subsec_nanos()) * 1e-9;
    Ok(match modified.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => secs(d),
        Err(e) => -secs(e.duration()),
    })
}

/// A local time's fields (C's `struct tm`).
#[derive(Debug, Clone, Copy, Default)]
pub struct Tm {
    pub year: i32,
    pub mon: i32,
    pub day: i32,
    pub hour: i32,
    pub min: i32,
    pub sec: i32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
#[allow(clippy::struct_field_names)] // C's own names
struct CTm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
}

#[cfg(windows)]
#[allow(unsafe_code)]
unsafe extern "C" {
    fn _localtime64_s(tm: *mut CTm, time: *const i64) -> i32;
}

/// The C runtime's `localtime` of a `time_t` (what Python's `time.localtime` and `datetime.fromtimestamp` call), or
/// None where it fails (Python raises `OSError` there: the Python answers).
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn localtime(t: i64) -> Option<Tm> {
    let mut tm = CTm::default();
    // SAFETY: _localtime64_s writes the struct it is given (C's struct tm: nine ints) from the time it reads.
    let rc = unsafe { _localtime64_s(&raw mut tm, &raw const t) };
    (rc == 0).then_some(Tm {
        year: tm.tm_year + 1900,
        mon: tm.tm_mon + 1,
        day: tm.tm_mday,
        hour: tm.tm_hour,
        min: tm.tm_min,
        sec: tm.tm_sec,
    })
}

#[cfg(not(windows))]
pub fn localtime(_t: i64) -> Option<Tm> {
    None
}

/// `round()` half to even of a float (`_PyTime_RoundHalfEven`).
fn round_half_even(x: f64) -> f64 {
    let r = x.round();
    #[allow(clippy::float_cmp)] // an exact half, as _PyTime_RoundHalfEven tests it
    let half = (x - r).abs() == 0.5;
    if half { 2.0 * (x / 2.0).round() } else { r }
}

/// `datetime.fromtimestamp(t).strftime('%Y-%m-%d %H:%M:%S')`: the seconds as Python splits them (microseconds
/// rounded half to even, carrying into the second).
pub fn fromtimestamp(t: f64) -> PyResult<String> {
    if !t.is_finite() {
        return Err(Fail::Python);
    }
    let mut int = t.trunc();
    let mut frac = round_half_even((t - int) * 1e6);
    if frac >= 1e6 {
        frac -= 1e6;
        int += 1.0;
    } else if frac < 0.0 {
        int -= 1.0;
    }
    let _ = frac;
    if int.abs() > 1e15 {
        return Err(Fail::Python);
    }
    let tm = localtime(int as i64).ok_or(Fail::Python)?;
    Ok(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        tm.year, tm.mon, tm.day, tm.hour, tm.min, tm.sec
    ))
}

/// `time.localtime(t)` (the time floored to a second): the time of day in seconds.
pub fn seconds_of_day(t: f64) -> PyResult<i64> {
    if !t.is_finite() || t.abs() > 1e15 {
        return Err(Fail::Python);
    }
    let tm = localtime(t.floor() as i64).ok_or(Fail::Python)?;
    Ok(i64::from(tm.hour) * 3600 + i64::from(tm.min) * 60 + i64::from(tm.sec))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ints_as_python_reads_them() {
        assert_eq!(py_int("0x10", 16), Ok(16));
        assert_eq!(py_int("10", 16), Ok(16));
        assert_eq!(py_int(" 0o20010_060 ", 0), Ok(0o20_010_060));
        assert_eq!(py_int("0x_1f", 0), Ok(31));
        assert_eq!(py_int("-5", 0), Ok(-5));
        assert!(py_int("010", 0).is_err());
        assert_eq!(py_int("000", 0), Ok(0));
        assert!(py_int("1__0", 10).is_err());
        assert!(py_int("", 10).is_err());
        assert_eq!(
            py_int("abc", 10),
            Err(Fail::Err(PyErr::new(
                "ValueError",
                "invalid literal for int() with base 10: 'abc'"
            )))
        );
    }

    #[test]
    fn paths_as_ntpath() {
        assert_eq!(dirname("C:\\a\\b.exe"), "C:\\a");
        assert_eq!(dirname("C:\\b.exe"), "C:\\");
        assert_eq!(dirname("b.exe"), "");
        assert_eq!(dirname("a\\\\b"), "a");
        assert_eq!(abspath("C:\\a\\..\\b\\.\\c"), "C:\\b\\c");
        assert_eq!(join("C:", "x"), "C:x");
        assert_eq!(hex_width(-5, 8), "-0000005");
        assert_eq!(hex_alt(-16), "-0x10");
    }
}
