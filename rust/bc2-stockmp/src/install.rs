//! `stockmp.py install`'s readings of the build output and the game folder (round O3): the files of `out\game`
//! (`os.walk` + `relpath`), the install markers (`*.opfor-stockmp-applied`: the SHA-256 of the file written),
//! `installed_files` (the installation record in the game folder) and `pipeline.check_report` (out\game against its
//! `build_report.json`). The copying and its prints are `bfbc2.exe`'s (`bc2-cli`'s `stockmp` group).
//!
//! Where Windows' path rules or Python's readers would see something the port does not reproduce - a name
//! `GetFullPathNameW` / `normcase` changes (a trailing dot or space, a cased letter outside the ranges below), a link
//! or junction, a marker that is not ASCII, a report that is not a JSON object the reference reads the same way - the
//! reading gives [`Fault::Refuse`] and the caller hands the command line to the Python.

use std::collections::HashMap;
use std::fs;
use std::io;

use crate::sha256;

/// `pipeline.BACKUP`: the game file `--apply` replaced, kept beside it.
pub const BACKUP: &str = ".opfor-stockmp-orig";
/// `pipeline.APPLIED`: the marker beside a file `--apply` wrote, holding its SHA-256.
pub const APPLIED: &str = ".opfor-stockmp-applied";

/// Why a reading did not give the reference's answer.
#[derive(Debug)]
pub enum Fault {
    /// The `OSError` the reference raises: its path and the error.
    Io(String, io::Error),
    /// The reference answers here (see the module's notes).
    Refuse(&'static str),
}

fn io_at(path: &str) -> impl FnOnce(io::Error) -> Fault + '_ {
    move |e| Fault::Io(path.to_owned(), e)
}

/// `ntpath.join(a, b)` for a relative `b` without a drive.
#[must_use]
pub fn join(a: &str, b: &str) -> String {
    if a.is_empty() || a.ends_with('\\') || a.ends_with('/') || (a.len() == 2 && a.ends_with(':')) {
        format!("{a}{b}")
    } else {
        format!("{a}\\{b}")
    }
}

/// A character `ntpath.normcase` (`LCMapStringEx`, invariant locale) lowers as Rust's simple lower-casing does: ASCII,
/// uncased characters, and the Latin, Greek and Cyrillic letters below U+0530 but the dotted capital I.
fn case_safe(c: char) -> bool {
    if c.is_ascii() {
        return true;
    }
    let mut lo = c.to_lowercase();
    let mut up = c.to_uppercase();
    let uncased = lo.next() == Some(c) && lo.next().is_none() && up.next() == Some(c) && up.next().is_none();
    uncased || (u32::from(c) < 0x530 && c != '\u{130}' && c.to_lowercase().count() == 1)
}

/// A file or folder name the path rules keep as it is.
fn plain(name: &str) -> bool {
    !name.is_empty() && !name.ends_with('.') && !name.ends_with(' ') && name.chars().all(case_safe)
}

/// `os.path.normcase` of a name made of [`plain`] parts.
#[must_use]
pub fn normcase(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

/// `os.path.normpath` of a relative path (the report's keys): `/` as `\`, empty and `.` parts dropped, `..` taking
/// out the part before it. Refused: a drive, a root, or a part [`plain`] does not keep.
fn normpath_rel(p: &str) -> Result<String, Fault> {
    let p = p.replace('/', "\\");
    if p.starts_with('\\') || p.contains(':') {
        return Err(Fault::Refuse("a report key with a drive or root"));
    }
    let mut parts: Vec<&str> = Vec::new();
    for c in p.split('\\') {
        match c {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            c if plain(c) => parts.push(c),
            _ => return Err(Fault::Refuse("a report key normpath changes")),
        }
    }
    Ok(if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("\\")
    })
}

/// The files under `top` as `os.path.relpath(os.path.join(dirpath, f), top)` of `os.walk(top)` (nothing for a missing
/// folder), in `os.walk`'s order.
///
/// # Errors
/// A link, a junction or a name the path rules change: refused.
pub fn walk_rel(top: &str) -> Result<Vec<String>, Fault> {
    let mut out = Vec::new();
    let Ok(meta) = fs::symlink_metadata(top) else {
        return Ok(out);
    };
    if meta.file_type().is_symlink() {
        return Err(Fault::Refuse("a linked folder"));
    }
    if !meta.is_dir() || top.is_empty() {
        return Ok(out);
    }
    let mut stack: Vec<(String, String)> = vec![(top.to_owned(), String::new())];
    while let Some((dir, rel)) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        let mut dirs = Vec::new();
        for entry in rd {
            let entry = entry.map_err(|_| Fault::Refuse("a folder not listed"))?;
            let ft = entry.file_type().map_err(|_| Fault::Refuse("a file type"))?;
            if ft.is_symlink() {
                return Err(Fault::Refuse("a link"));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Fault::Refuse("a name not UTF-8"))?;
            if !plain(&name) {
                return Err(Fault::Refuse("a name the path rules change"));
            }
            let r = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}\\{name}")
            };
            if ft.is_dir() {
                dirs.push((join(&dir, &name), r));
            } else {
                out.push(r);
            }
        }
        stack.extend(dirs.into_iter().rev());
    }
    Ok(out)
}

/// `open(path).read()` of a marker (the locale's code page; universal newlines). Refused when it is not ASCII.
///
/// # Errors
/// The `OSError` of opening it; a marker that is not ASCII: refused.
pub fn read_marker(path: &str) -> Result<String, Fault> {
    let b = fs::read(path).map_err(io_at(path))?;
    if !b.is_ascii() {
        return Err(Fault::Refuse("a marker that is not ASCII"));
    }
    let t = String::from_utf8_lossy(&b).into_owned();
    Ok(t.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `str.strip()` of ASCII text.
#[must_use]
pub fn strip(t: &str) -> &str {
    t.trim_matches(|c: char| matches!(c, '\t' | '\n' | '\u{0b}' | '\u{0c}' | '\r' | '\u{1c}'..='\u{1f}' | ' '))
}

/// `open(marker).read().strip() == pipeline.sha(file)`.
///
/// # Errors
/// As [`read_marker`] and the file's reading.
pub fn marker_matches(marker: &str, file: &str) -> Result<bool, Fault> {
    let m = read_marker(marker)?;
    let h = sha256::file(file).map_err(io_at(file))?;
    Ok(strip(&m) == h)
}

/// `os.path.isfile`.
#[must_use]
pub fn isfile(p: &str) -> bool {
    !p.is_empty() && std::path::Path::new(p).is_file()
}

/// `stockmp.installed_files(game, lv)`: the paths (relative to the game folder) of the files `--apply` installed for
/// the level - every `*.opfor-stockmp-applied` marker under the game folder whose path has a part named as the level.
///
/// # Errors
/// As [`walk_rel`].
pub fn installed_files(game: &str, lv: &str) -> Result<Vec<String>, Fault> {
    let lv = lv.to_lowercase();
    Ok(walk_rel(game)?
        .into_iter()
        .filter(|rel| rel.ends_with(APPLIED))
        .map(|rel| rel[..rel.len() - APPLIED.len()].to_owned())
        .filter(|rel| rel.replace('\\', "/").split('/').any(|c| c.to_lowercase() == lv))
        .collect())
}

/// `pipeline.check_report(paths)`: the problems of `<root>\out\game` against `<root>\build_report.json`; empty when
/// out\game is the reported build.
///
/// # Errors
/// A report the reference reads in its own words (not JSON, not an object, unreadable), a name the path rules
/// change: refused; a file of the build that cannot be read: its `OSError`.
pub fn check_report(root: &str) -> Result<Vec<String>, Fault> {
    let report = join(root, "build_report.json");
    let raw = match fs::read(&report) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound && !std::path::Path::new(&report).exists() => {
            return Ok(vec![
                "no build_report.json (the last build did not finish) - run build".to_owned(),
            ]);
        }
        Err(_) => return Err(Fault::Refuse("a report not read")),
    };
    let rep: serde_json::Value = serde_json::from_slice(&raw).map_err(|_| Fault::Refuse("a report not JSON"))?;
    let serde_json::Value::Object(rep) = rep else {
        return Err(Fault::Refuse("a report not an object"));
    };
    let empty = serde_json::Map::new();
    let outputs = match rep.get("outputs") {
        None | Some(serde_json::Value::Null | serde_json::Value::Bool(false)) => &empty,
        Some(serde_json::Value::String(s)) if s.is_empty() => &empty,
        Some(serde_json::Value::Array(a)) if a.is_empty() => &empty,
        Some(serde_json::Value::Number(n)) if n.as_f64() == Some(0.0) => &empty,
        Some(serde_json::Value::Object(o)) => o,
        Some(_) => return Err(Fault::Refuse("outputs not a mapping")),
    };
    let mut listed: HashMap<String, Option<String>> = HashMap::new();
    for (k, v) in outputs {
        let key = normcase(&normpath_rel(k)?);
        let h = v.as_str().map(str::to_owned);
        if listed.insert(key, h).is_some() {
            // two keys of one path: which one wins depends on the report's order (Python's dict, the last)
            return Err(Fault::Refuse("two report keys of one path"));
        }
    }
    let out = join(&join(root, "out"), "game");
    let have: Vec<String> = walk_rel(&out)?
        .into_iter()
        .map(|r| normcase(&format!("out\\game\\{r}")))
        .collect();
    let mut extra: Vec<&String> = have.iter().filter(|k| !listed.contains_key(*k)).collect();
    extra.sort();
    extra.dedup();
    let mut problems: Vec<String> = extra.iter().map(|k| format!("{k}: not in the build report")).collect();
    let mut keys: Vec<(&String, &Option<String>)> = listed.iter().collect();
    keys.sort_by(|a, b| a.0.cmp(b.0));
    for (k, h) in keys {
        if have.contains(k) {
            let p = join(root, k);
            let got = sha256::file(&p).map_err(io_at(&p))?;
            if h.as_deref() != Some(got.as_str()) {
                problems.push(format!("{k}: differs from the build report"));
            }
        } else {
            problems.push(format!("{k}: in the build report, missing"));
        }
    }
    Ok(problems)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_as_ntpath() {
        assert_eq!(normpath_rel("out/game/./a//b").ok(), Some("out\\game\\a\\b".to_owned()));
        assert_eq!(normpath_rel("a/../../b").ok(), Some("..\\b".to_owned()));
        assert_eq!(normpath_rel("").ok(), Some(".".to_owned()));
        assert!(normpath_rel("C:x").is_err() && normpath_rel("a./b").is_err());
        assert!(case_safe('\u{e9}') && case_safe('\u{436}') && !case_safe('\u{130}') && case_safe('\u{4e2d}'));
        assert_eq!(strip(" \tab\n"), "ab");
    }
}
