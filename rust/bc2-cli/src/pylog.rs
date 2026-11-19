//! The tools' day logs as `bfbc2core.dbxedit` / `fbrbtool` keep them: `log_path` (`caches.day_log` under
//! `%LOCALAPPDATA%\bfbc2-tool-suite\logs`) and `_log` (the start line and the tool's lines, appended).

use std::fmt::Write as _;
use std::fs::{self, OpenOptions};
use std::io::Write as _;
use std::sync::LazyLock;

use regex::Regex;

use crate::clock;

/// `caches.LOG_FILE_ENV`, `caches.LOG_FILE_MB`, `caches.MB`.
const LOG_FILE_ENV: &str = "BFBC2_LOG_FILE_MB";
const LOG_FILE_MB: f64 = 20.0;
const MB: f64 = 1_048_576.0;

/// `caches.LOG_NAME` (`re.IGNORECASE`; Python's `\d` is Unicode's decimal digits, as the regex crate's).
pub static LOG_NAME: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r"(?i)^(?P<tool>.+)-(?P<date>\d{4}-\d{2}-\d{2})(?:\.(?P<part>\d+))?\.log$").ok());

static DIGIT: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^\d$").ok());

fn is_decimal(c: char) -> bool {
    let mut b = [0u8; 4];
    DIGIT.as_ref().is_some_and(|r| r.is_match(c.encode_utf8(&mut b)))
}

/// A Unicode decimal digit's value (`int()` of it): decimal digits come in runs of ten from 0 to 9, adjacent runs
/// each starting at 0, so the value is the distance from the run's first digit, modulo ten.
fn digit_value(c: char) -> Option<u32> {
    if c.is_ascii_digit() {
        return c.to_digit(10);
    }
    if !is_decimal(c) {
        return None;
    }
    let mut start = c as u32;
    while let Some(p) = start.checked_sub(1).and_then(char::from_u32) {
        if !is_decimal(p) {
            break;
        }
        start -= 1;
    }
    Some((c as u32 - start) % 10)
}

/// `str.isspace()`: the characters Python's `strip()` removes.
fn py_space(c: char) -> bool {
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

/// Python's `float(text)` of an environment value, or None where it raises `ValueError`: Unicode digits read as
/// their ASCII ones, underscores allowed only between digits, the rest as Rust reads a float (`inf`, `nan`,
/// `infinity`, exponents, either case).
fn py_float(text: &str) -> Option<f64> {
    let s: String = text
        .trim_matches(py_space)
        .chars()
        .map(|c| match digit_value(c) {
            Some(v) if !c.is_ascii_digit() => char::from_digit(v, 10).unwrap_or(c),
            _ => c,
        })
        .collect();
    let chars: Vec<char> = s.chars().collect();
    for (i, c) in chars.iter().enumerate() {
        if *c == '_' {
            let before = i
                .checked_sub(1)
                .and_then(|j| chars.get(j))
                .is_some_and(char::is_ascii_digit);
            let after = chars.get(i + 1).is_some_and(char::is_ascii_digit);
            if !(before && after) {
                return None;
            }
        }
    }
    let plain: String = chars.into_iter().filter(|c| *c != '_').collect();
    // Rust takes forms Python does not ("1e" is refused by both; Rust refuses nothing Python takes but the above)
    plain.parse::<f64>().ok()
}

/// `caches.log_caps()[2]`: one log file's cap in bytes, `int(_number(BFBC2_LOG_FILE_MB, 20) * MB)` (as a float
/// holding that integer).
pub fn file_cap() -> f64 {
    (number(LOG_FILE_ENV, LOG_FILE_MB) * MB).trunc()
}

/// `ntpath.join(a, b)` for a name `b` that is neither absolute nor has a drive: a separator added unless `a` is
/// empty, ends with one or is a bare drive (`C:`).
fn join(a: &str, b: &str) -> String {
    let bare_drive = a.len() == 2 && a.ends_with(':') && a.chars().next().is_some_and(|c| c.is_ascii_alphabetic());
    if a.is_empty() || a.ends_with('\\') || a.ends_with('/') || bare_drive {
        format!("{a}{b}")
    } else {
        format!("{a}\\{b}")
    }
}

fn env_nonempty(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// `os.path.expanduser('~')` on Windows: `USERPROFILE`, else `HOMEDRIVE` + `HOMEPATH`, else `~` itself.
fn home() -> String {
    if let Ok(p) = std::env::var("USERPROFILE") {
        return p;
    }
    match std::env::var("HOMEPATH") {
        Ok(hp) => join(&std::env::var("HOMEDRIVE").unwrap_or_default(), &hp),
        Err(_) => "~".to_owned(),
    }
}

/// `caches.log_part`.
fn log_part(top: &str, tool: &str, date: &str, part: u128) -> String {
    if part == 0 {
        join(top, &format!("{tool}-{date}.log"))
    } else {
        join(top, &format!("{tool}-{date}.{part}.log"))
    }
}

/// `caches.day_log(top, tool, date)`: the day's last part while it is under the per-file cap, else the next part.
fn day_log(top: &str, tool: &str, date: &str) -> String {
    let limit = file_cap();
    let mut last: u128 = 0;
    if let (Ok(entries), Some(re)) = (fs::read_dir(top), LOG_NAME.as_ref()) {
        for e in entries.flatten() {
            let Some(name) = e.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            let Some(m) = re.captures(&name) else { continue };
            if m.name("tool").map(|t| t.as_str()) != Some(tool) || m.name("date").map(|d| d.as_str()) != Some(date) {
                continue;
            }
            let part = m.name("part").map_or(0, |p| {
                p.as_str().chars().fold(0u128, |n, c| {
                    n.saturating_mul(10)
                        .saturating_add(u128::from(digit_value(c).unwrap_or(0)))
                })
            });
            last = last.max(part);
        }
    }
    let p = log_part(top, tool, date, last);
    let full = fs::metadata(&p).is_ok_and(|m| {
        let cap = limit.max(1.0);
        cap < 18_446_744_073_709_551_616.0 && m.len() >= cap as u64
    });
    if full {
        log_part(top, tool, date, last.saturating_add(1))
    } else {
        p
    }
}

/// `caches._number(env, default)`: the variable read as Python's `float`, else (unset, unreadable, negative, inf or
/// nan) the default.
pub fn number(env: &str, default: f64) -> f64 {
    match std::env::var(env).ok().and_then(|t| py_float(&t)) {
        Some(v) if v >= 0.0 && v != f64::INFINITY => v,
        _ => default,
    }
}

/// `errors.log_dir()` / `caches.logs_folder()`: `%LOCALAPPDATA%\bfbc2-tool-suite\logs` (not made here).
pub fn logs_dir() -> String {
    let base = env_nonempty("LOCALAPPDATA").unwrap_or_else(home);
    join(&join(&base, "bfbc2-tool-suite"), "logs")
}

/// `os.path.expanduser('~')`.
#[allow(dead_code, reason = "the caches group's (in progress)")]
pub fn home_dir() -> String {
    home()
}

/// `dbxedit.log_path()` / `fbrbtool.log_path()`: today's log file of `tool`, or None when the folder cannot be made.
pub fn log_path(tool: &str) -> Option<String> {
    let d = logs_dir();
    fs::create_dir_all(&d).ok()?;
    Some(day_log(&d, tool, &clock::now().date()))
}

/// `_log(path, args, lines)`: the start line and the tool's lines, stamped now, appended to the log (an error
/// writing is ignored: the command still ran).
pub fn append(path: Option<&str>, tool: &str, args: &[String], lines: &[(String, String)]) {
    let Some(path) = path else { return };
    let stamp = clock::now().stamp();
    let mut text = format!(
        "{stamp} INFO    --- {tool} started (in process): {} ---\r\n",
        args.join(" ")
    );
    for (level, message) in lines {
        let _ = write!(text, "{stamp} {level:<7} {message}\r\n");
    }
    if let Ok(mut f) = OpenOptions::new().append(true).create(true).open(path) {
        let _ = f.write_all(text.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_as_python_reads_them() {
        assert_eq!(py_float(" 1_0 "), Some(10.0));
        assert_eq!(py_float("1__0"), None);
        assert_eq!(py_float("_1"), None);
        assert_eq!(py_float("\u{0663}.5"), Some(3.5));
        assert_eq!(py_float("\u{1c}2\u{3000}"), Some(2.0));
        assert!(py_float("nan").is_some_and(f64::is_nan));
        assert_eq!(py_float("x"), None);
    }

    #[test]
    fn digits_and_joins() {
        assert_eq!(digit_value('\u{0669}'), Some(9));
        assert_eq!(digit_value('\u{1D7D9}'), Some(1)); // a mathematical digit: runs of ten side by side
        assert_eq!(digit_value('a'), None);
        assert_eq!(join("C:", "x"), "C:x");
        assert_eq!(join("C:\\", "x"), "C:\\x");
        assert_eq!(join("a/", "x"), "a/x");
        assert_eq!(join("a", "x"), "a\\x");
    }
}
