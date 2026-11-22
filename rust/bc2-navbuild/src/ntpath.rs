//! The level driver's `os`, `os.path` (`ntpath`) and `glob` (round NK6): the paths NavKit's driver spells
//! ([`crate::inputs`], [`crate::apply`]) the same way the command line (`bc2-cli`'s `pyos`, the Python's own `os`)
//! spells them, so a build's log lines and side files are the same bytes whether `bfbc2.exe navkit` or the editor
//! runs it. Paths are Python strs; a path the port cannot spell as Python would (a UNC path, a name that is not
//! Unicode, a pattern with `[`) gives [`Fail::Handoff`].
//!
//! **Errors** ([`Fail`]): the exception the Python raised ([`Fail::Raised`]: its class name, its `str()`, the file an
//! `OSError` names), a hand-off ([`Fail::Handoff`]: the command line gives the run to the Python; the editor reports
//! it), a cancelled build ([`Fail::Cancelled`]) and a line sink that failed ([`Fail::Sink`]: its own error is the
//! sink's to keep).

// path parts indexed within the lengths just checked; Python's float of a nanosecond count
#![allow(
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::missing_errors_doc,
    clippy::assigning_clones,
    clippy::unnecessary_wraps,
    clippy::format_push_string
)]

use std::fs;
use std::io;
use std::path::Path;

/// An exception as the Python raised it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raised {
    /// Its class name (`NavKitError`, `FileNotFoundError`, `ValueError` ...).
    pub kind: String,
    /// Its `str()`.
    pub msg: String,
    /// The file an `OSError` names.
    pub filename: Option<String>,
}

/// Why a driver step stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// The Python's exception.
    Raised(Raised),
    /// An input the port does not read as the Python does: the command line hands the run to the Python.
    Handoff,
    /// The build was cancelled (the editor's Cancel, or its window closed).
    Cancelled,
    /// The line sink failed (the command line's stdout); the sink keeps its own error.
    Sink,
}

impl std::fmt::Display for Fail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Fail::Raised(r) => write!(f, "{}", r.msg),
            Fail::Handoff => f.write_str("an input the Rust port does not read as NavKit's Python did"),
            Fail::Cancelled => f.write_str("cancelled"),
            Fail::Sink => f.write_str("the output could not be written"),
        }
    }
}

impl std::error::Error for Fail {}

/// A step's answer.
pub type Res<T> = Result<T, Fail>;

/// The exception `kind(msg)`.
#[must_use]
pub fn raise(kind: &str, msg: impl Into<String>) -> Fail {
    Fail::Raised(Raised {
        kind: kind.to_owned(),
        msg: msg.into(),
        filename: None,
    })
}

/// A `NavKitError(msg)`.
#[must_use]
pub fn navkit_error(msg: impl Into<String>) -> Fail {
    raise("NavKitError", msg)
}

/// The `OSError` Python raises for an I/O error on `path` (the kinds the tools meet; another hands off).
#[must_use]
pub fn os_error(path: &str, e: &io::Error) -> Fail {
    let (kind, errno, text) = match e.kind() {
        io::ErrorKind::NotFound => ("FileNotFoundError", 2, "No such file or directory"),
        io::ErrorKind::PermissionDenied => ("PermissionError", 13, "Permission denied"),
        io::ErrorKind::InvalidFilename | io::ErrorKind::InvalidInput => ("OSError", 22, "Invalid argument"),
        _ => return Fail::Handoff,
    };
    Fail::Raised(Raised {
        kind: kind.to_owned(),
        msg: format!("[Errno {errno}] {text}: {}", str_repr(path)),
        filename: Some(path.to_owned()),
    })
}

/// `repr(s)` of a str: single quotes unless the text has a `'` and no `"`; `\\`, `\n`, `\r`, `\t` and the quote
/// escaped, ASCII control characters as `\xNN` (the paths and names the driver prints; other characters as they are).
#[must_use]
pub fn str_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(s.len() + 2);
    out.push(q);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == q => {
                out.push('\\');
                out.push(c);
            }
            c if (c as u32) < 0x20 || (0x7F..0xA0).contains(&(c as u32)) => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

/// Python's whitespace (`str.strip()`).
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

/// `s.strip()`.
#[must_use]
pub fn strip(s: &str) -> &str {
    s.trim_matches(py_space)
}

/// Text read in universal-newline mode (`\r\n` and `\r` as `\n`).
#[must_use]
pub fn universal(t: &str) -> String {
    if t.contains('\r') {
        t.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        t.to_owned()
    }
}

/// `\` or `/`.
#[must_use]
pub fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// `ntpath.splitdrive(p)` for a drive letter (`C:`); a UNC path (`\\server\share`) hands off.
pub fn splitdrive(p: &str) -> Res<(&str, &str)> {
    match p.as_bytes() {
        [d, b':', ..] if d.is_ascii_alphabetic() => Ok(p.split_at(2)),
        [a, b, ..] if is_sep(char::from(*a)) && is_sep(char::from(*b)) => Err(Fail::Handoff),
        _ => Ok(("", p)),
    }
}

/// `ntpath.join(a, *parts)`.
pub fn join(a: &str, parts: &[&str]) -> Res<String> {
    let (mut drive, mut path) = {
        let (d, r) = splitdrive(a)?;
        (d.to_owned(), r.to_owned())
    };
    for p in parts {
        let (pd, pp) = splitdrive(p)?;
        if pp.starts_with(is_sep) {
            if !pd.is_empty() || drive.is_empty() {
                drive = pd.to_owned();
            }
            path = pp.to_owned();
            continue;
        } else if !pd.is_empty() && !pd.eq_ignore_ascii_case(&drive) {
            drive = pd.to_owned();
            path = pp.to_owned();
            continue;
        } else if !pd.is_empty() {
            drive = pd.to_owned();
        }
        if !path.is_empty() && !path.ends_with(is_sep) {
            path.push('\\');
        }
        path.push_str(pp);
    }
    if !path.is_empty() && !path.starts_with(is_sep) && !drive.is_empty() && !drive.ends_with(':') {
        return Ok(format!("{drive}\\{path}"));
    }
    Ok(format!("{drive}{path}"))
}

/// `os.path.join(a, b)`.
pub fn join2(a: &str, b: &str) -> Res<String> {
    join(a, &[b])
}

/// `ntpath.split(p)`: (head, tail).
pub fn split(p: &str) -> Res<(String, String)> {
    let (d, rest) = splitdrive(p)?;
    let i = rest.rfind(is_sep).map_or(0, |k| k + 1);
    let (head, tail) = rest.split_at(i);
    let trimmed = head.trim_end_matches(is_sep);
    let head = if trimmed.is_empty() { head } else { trimmed };
    Ok((format!("{d}{head}"), tail.to_owned()))
}

/// `os.path.dirname(p)`.
pub fn dirname(p: &str) -> Res<String> {
    split(p).map(|s| s.0)
}

/// `os.path.basename(p)`.
pub fn basename(p: &str) -> Res<String> {
    split(p).map(|s| s.1)
}

/// `os.path.splitext(p)`: (root, ext).
#[must_use]
pub fn splitext(p: &str) -> (String, String) {
    let sep = p.rfind(is_sep).map_or(0, |k| k + 1);
    let name = &p[sep..];
    if let Some(dot) = name.rfind('.') {
        let lead = name.len() - name.trim_start_matches('.').len();
        if dot >= lead {
            let at = sep + dot;
            return (p[..at].to_owned(), p[at..].to_owned());
        }
    }
    (p.to_owned(), String::new())
}

/// `ntpath.normpath(p)`.
pub fn normpath(p: &str) -> Res<String> {
    let p = p.replace('/', "\\");
    let (drive, rest) = splitdrive(&p)?;
    let root = if rest.starts_with('\\') { "\\" } else { "" };
    let mut comps: Vec<&str> = Vec::new();
    for c in rest.split('\\') {
        match c {
            "" | "." => {}
            ".." => {
                if comps.last().is_some_and(|l| *l != "..") {
                    comps.pop();
                } else if root.is_empty() {
                    comps.push("..");
                }
            }
            c => comps.push(c),
        }
    }
    let out = format!("{drive}{root}{}", comps.join("\\"));
    Ok(if out.is_empty() { ".".to_owned() } else { out })
}

fn getcwd() -> Res<String> {
    std::env::current_dir()
        .ok()
        .and_then(|d| d.into_os_string().into_string().ok())
        .ok_or(Fail::Handoff)
}

/// `os.path.abspath(p)`.
pub fn abspath(p: &str) -> Res<String> {
    if p.is_empty() {
        return normpath(&getcwd()?);
    }
    let (d, rest) = splitdrive(p)?;
    if !rest.starts_with(is_sep) {
        let cwd = getcwd()?;
        if !d.is_empty() && !cwd.to_lowercase().starts_with(&d.to_lowercase()) {
            return Err(Fail::Handoff);
        }
        return normpath(&join2(&cwd, rest)?);
    }
    if d.is_empty() {
        let cwd = getcwd()?;
        let (cd, _) = splitdrive(&cwd)?;
        return normpath(&format!("{cd}{rest}"));
    }
    normpath(p)
}

fn normcase(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

/// `os.path.relpath(path, start)`.
pub fn relpath(path: &str, start: &str) -> Res<String> {
    if path.is_empty() {
        return Err(raise("ValueError", "no path specified"));
    }
    let a = abspath(path)?;
    let b = abspath(start)?;
    let (ad, ar) = splitdrive(&a)?;
    let (bd, br) = splitdrive(&b)?;
    if normcase(ad) != normcase(bd) {
        return Err(raise(
            "ValueError",
            format!("path is on mount '{bd}', start on mount '{ad}'"),
        ));
    }
    let sl: Vec<&str> = br.split('\\').filter(|x| !x.is_empty()).collect();
    let pl: Vec<&str> = ar.split('\\').filter(|x| !x.is_empty()).collect();
    let mut i = 0;
    while i < sl.len() && i < pl.len() && normcase(sl[i]) == normcase(pl[i]) {
        i += 1;
    }
    let mut rel: Vec<&str> = vec![".."; sl.len() - i];
    rel.extend_from_slice(&pl[i..]);
    if rel.is_empty() {
        return Ok(".".to_owned());
    }
    Ok(rel.join("\\"))
}

/// `os.path.isdir(p)`.
#[must_use]
pub fn isdir(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_dir()
}

/// `os.path.isfile(p)`.
#[must_use]
pub fn isfile(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_file()
}

fn lexists(p: &str) -> bool {
    !p.is_empty() && fs::symlink_metadata(p).is_ok()
}

/// `os.path.getsize(p)`.
pub fn getsize(p: &str) -> Res<u64> {
    fs::metadata(p).map(|m| m.len()).map_err(|e| os_error(p, &e))
}

/// `os.stat(p).st_mtime_ns`.
pub fn mtime_ns(p: &str) -> Res<i128> {
    let m = fs::metadata(p).map_err(|e| os_error(p, &e))?;
    let t = m.modified().map_err(|_| Fail::Handoff)?;
    Ok(match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => d.as_nanos() as i128,
        Err(e) => -(e.duration().as_nanos() as i128),
    })
}

/// `os.path.getmtime(p)` (the nanoseconds as Python makes a float of them).
pub fn getmtime(p: &str) -> Res<f64> {
    let ns = mtime_ns(p)?;
    let secs = ns.div_euclid(1_000_000_000);
    let rem = ns.rem_euclid(1_000_000_000);
    Ok(secs as f64 + rem as f64 * 1e-9)
}

/// The names in a folder in `os.scandir`'s order, with whether each is a folder (following links).
pub fn scandir(dir: &str) -> Res<Vec<(String, bool)>> {
    let rd = fs::read_dir(if dir.is_empty() { "." } else { dir }).map_err(|e| os_error(dir, &e))?;
    let mut out = Vec::new();
    for e in rd {
        let e = e.map_err(|_| Fail::Handoff)?;
        let name = e.file_name().into_string().map_err(|_| Fail::Handoff)?;
        let is_dir = fs::metadata(e.path()).is_ok_and(|m| m.is_dir());
        out.push((name, is_dir));
    }
    Ok(out)
}

/// `os.listdir(dir)`.
pub fn listdir(dir: &str) -> Res<Vec<String>> {
    Ok(scandir(dir)?.into_iter().map(|(n, _)| n).collect())
}

/// One folder of `os.walk(top)`: (path, folders, files).
pub type WalkRow = (String, Vec<String>, Vec<String>);

/// `os.walk(top)` top-down (symbolic links to folders listed, not entered; junctions entered).
pub fn walk(top: &str) -> Res<Vec<WalkRow>> {
    let mut out = Vec::new();
    walk_into(top, &mut out)?;
    Ok(out)
}

fn walk_into(top: &str, out: &mut Vec<WalkRow>) -> Res<()> {
    let Ok(entries) = scandir(top) else {
        return Ok(());
    };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for (name, is_dir) in entries {
        if is_dir {
            dirs.push(name);
        } else {
            files.push(name);
        }
    }
    out.push((top.to_owned(), dirs.clone(), files));
    for d in dirs {
        let p = join2(top, &d)?;
        if !is_symlink(&p) {
            walk_into(&p, out)?;
        }
    }
    Ok(())
}

/// `os.path.islink(p)`: a symbolic link (not a junction).
fn is_symlink(p: &str) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::FileTypeExt as _;
        fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink_dir() || m.file_type().is_symlink_file())
            && !fs::read_link(p).is_ok_and(|t| t.to_string_lossy().starts_with("\\??\\"))
    }
    #[cfg(not(windows))]
    {
        fs::symlink_metadata(p).is_ok_and(|m| m.file_type().is_symlink())
    }
}

fn has_magic(s: &str) -> bool {
    s.contains(['*', '?', '['])
}

/// `fnmatch.fnmatch(name, pattern)` on Windows (both case-folded) for `*` and `?` patterns.
fn fnmatch(name: &str, pattern: &str) -> bool {
    let n: Vec<char> = normcase(name).chars().collect();
    let p: Vec<char> = normcase(pattern).chars().collect();
    let (mut i, mut j) = (0usize, 0usize);
    let (mut star, mut mark) = (None::<usize>, 0usize);
    while i < n.len() {
        if j < p.len() && (p[j] == '?' || p[j] == n[i]) {
            i += 1;
            j += 1;
        } else if j < p.len() && p[j] == '*' {
            star = Some(j);
            mark = i;
            j += 1;
        } else if let Some(s) = star {
            j = s + 1;
            mark += 1;
            i = mark;
        } else {
            return false;
        }
    }
    while j < p.len() && p[j] == '*' {
        j += 1;
    }
    j == p.len()
}

/// `glob.glob(pattern)` (not recursive) for `*` / `?` patterns; `[` hands off.
pub fn glob(pattern: &str) -> Res<Vec<String>> {
    if pattern.contains('[') {
        return Err(Fail::Handoff);
    }
    glob_in(pattern, false)
}

fn glob_in(pattern: &str, dironly: bool) -> Res<Vec<String>> {
    let (dir, base) = split(pattern)?;
    if !has_magic(pattern) {
        let ok = if base.is_empty() { isdir(&dir) } else { lexists(pattern) };
        return Ok(if ok { vec![pattern.to_owned()] } else { Vec::new() });
    }
    if dir.is_empty() {
        return glob1("", &base, dironly);
    }
    let dirs = if dir != pattern && has_magic(&dir) {
        glob_in(&dir, true)?
    } else {
        vec![dir]
    };
    let mut out = Vec::new();
    for d in dirs {
        let names = if has_magic(&base) {
            glob1(&d, &base, dironly)?
        } else if base.is_empty() {
            if isdir(&d) { vec![base.clone()] } else { Vec::new() }
        } else if lexists(&join2(&d, &base)?) {
            vec![base.clone()]
        } else {
            Vec::new()
        };
        for n in names {
            out.push(join2(&d, &n)?);
        }
    }
    Ok(out)
}

fn glob1(dir: &str, pattern: &str, dironly: bool) -> Res<Vec<String>> {
    let Ok(entries) = scandir(dir) else {
        return Ok(Vec::new());
    };
    let hidden = pattern.starts_with('.');
    Ok(entries
        .into_iter()
        .filter(|(n, d)| (!dironly || *d) && (hidden || !n.starts_with('.')) && fnmatch(n, pattern))
        .map(|(n, _)| n)
        .collect())
}

/// `os.makedirs(p, exist_ok=True)`.
pub fn makedirs(p: &str) -> Res<()> {
    if isdir(p) {
        return Ok(());
    }
    fs::create_dir_all(p).map_err(|e| os_error(p, &e))
}

/// `open(path, 'rb').read()`.
pub fn read(path: &str) -> Res<Vec<u8>> {
    fs::read(path).map_err(|e| os_error(path, &e))
}

/// `open(path, 'w', encoding='utf-8').write(text)` (`\n` as `\r\n`).
pub fn write_text_utf8(path: &str, text: &str) -> Res<()> {
    fs::write(path, text.replace('\n', "\r\n")).map_err(|e| os_error(path, &e))
}

/// `atomicio.write_bytes(path, data)`: whole or not at all ([`bc2_gamefs::atomic`]).
pub fn atomic_write(path: &str, data: &[u8]) -> Res<()> {
    bc2_gamefs::atomic::write(Path::new(path), &[data]).map_err(|e| match e {
        bc2_gamefs::atomic::WriteError::Temp(tmp, e) => os_error(&tmp.to_string_lossy(), &e),
        bc2_gamefs::atomic::WriteError::Replace(e) => os_error(path, &e),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn paths_as_ntpath() {
        assert_eq!(join("C:\\a", &["b", "c.hpd"]).unwrap(), "C:\\a\\b\\c.hpd");
        assert_eq!(join("C:\\a\\", &["D:x"]).unwrap(), "D:x");
        assert_eq!(split("C:\\a\\b.txt").unwrap(), ("C:\\a".to_owned(), "b.txt".to_owned()));
        assert_eq!(relpath("C:\\root\\work\\x.hpd", "C:\\root").unwrap(), "work\\x.hpd");
        assert_eq!(relpath("C:\\other\\x", "C:\\root\\a").unwrap(), "..\\..\\other\\x");
        assert_eq!(splitext("a\\b.c.d"), ("a\\b.c".to_owned(), ".d".to_owned()));
        assert_eq!(str_repr("it's"), "\"it's\"");
        assert_eq!(str_repr("C:\\a"), "'C:\\\\a'");
        assert!(fnmatch("MP_002.dbx", "*.DBX"));
        assert_eq!(strip("  x \r\n"), "x");
        assert_eq!(splitdrive("\\\\server\\share"), Err(Fail::Handoff));
    }
}
