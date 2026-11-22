//! The registration and settings rules of a level install, as text in and text out (Project OPFOR's
//! `wake_install.mft_*`, `bm_with_level`, `ini_*`, `maps_with_levels`, ported with their exact output): one
//! implementation for the WW2 install, its tests and the tools that read the same files.
//!
//! `package.mft` and `bundleManifest` are latin-1 (read and written byte for byte: [`crate::pytext::latin1`]);
//! `opfor.ini` and `maps.ini` UTF-8. Line ends are kept as the file has them.

use std::fmt::Write as _;
use std::sync::LazyLock;

use regex::Regex;

use crate::pytext::{rstrip, rstrip_nl, splitlines, splitlines_keep, strip};

/// `wake_install.INI_SECTION`: a section header on a line of its own (a comment may follow).
static INI_SECTION: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*\[\s*([^\]]+?)\s*\]\s*(?:[;#].*)?$"));
/// `wake_install.INI_COMMENT`.
static INI_COMMENT: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*[;#]"));
/// `wake_install.MANIFEST_FIX`: a bundleManifest line missing its third field.
static MANIFEST_FIX: LazyLock<Regex> = LazyLock::new(|| re(r"^levels/[^\t\r]+\t[0-9a-f-]{36}$"));
static LEVEL_KEY: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)^(\s*level\s*=\s*)(.*)$"));

#[allow(clippy::expect_used, reason = "the patterns are constants, checked by the tests")]
fn re(p: &str) -> Regex {
    Regex::new(p).expect("a valid pattern")
}

/// The null GUID field of a level's bundleManifest lines.
pub const NULL_GUID: &str = "00000000-0000-0000-0000-000000000000";

/// One bundleManifest line of a level's bundle (`levels/<lv>/<kind>\t<null guid>\t1`).
#[must_use]
pub fn bm_line(lv: &str, kind: &str) -> String {
    format!("levels/{lv}/{kind}\t{NULL_GUID}\t1")
}

/// A refusal of a text rule (Python's `SystemExit`): the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextError(pub String);

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for TextError {}

// ------------------------------------------------------------------------------------------------ package.mft

/// `mft_parse`: the head lines and the blocks (a block: a `BundleSource` line and the lines after it up to a blank
/// line or the next `BundleSource`; a stray line between blocks is a block of its own). Lines without their ends.
#[must_use]
pub fn mft_parse(text: &str) -> (Vec<String>, Vec<Vec<String>>) {
    let mut head = Vec::new();
    let mut blocks: Vec<Vec<String>> = Vec::new();
    let mut open = false;
    for line in text.replace("\r\n", "\n").split('\n') {
        let st = strip(line);
        if st.starts_with("BundleSource ") {
            blocks.push(vec![rstrip(line).to_owned()]);
            open = true;
        } else if st.is_empty() {
            open = false;
        } else if open {
            if let Some(b) = blocks.last_mut() {
                b.push(rstrip(line).to_owned());
            }
        } else if blocks.is_empty() {
            head.push(rstrip(line).to_owned());
        } else {
            blocks.push(vec![rstrip(line).to_owned()]);
            open = true;
        }
    }
    (head, blocks)
}

/// `mft_text`: head and blocks joined with CRLF, a blank line between them, a CRLF at the end.
#[must_use]
pub fn mft_text(head: &[String], blocks: &[Vec<String>]) -> String {
    let mut parts = Vec::new();
    if !head.is_empty() {
        parts.push(head.join("\r\n"));
    }
    parts.extend(blocks.iter().map(|b| b.join("\r\n")));
    parts.join("\r\n\r\n") + "\r\n"
}

fn is_own_block(first: &str, lv: &str) -> bool {
    let s: Vec<char> = strip(first).chars().collect();
    let p: Vec<char> = format!("BundleSource game:levels/{lv}/").chars().collect();
    s.len() >= p.len() && s.iter().zip(&p).all(|(a, b)| a.to_lowercase().eq(b.to_lowercase()))
}

/// `mft_with_level`: (the new text, or None when nothing changes; the report line) with the level's blocks as
/// `want` (block texts) - each once, at the end; every other block kept as it is.
#[must_use]
pub fn mft_with_level(text: &str, lv: &str, want: &[String]) -> (Option<String>, String) {
    let (head, blocks) = mft_parse(text);
    let have: Vec<&Vec<String>> = blocks
        .iter()
        .filter(|b| b.first().is_some_and(|f| is_own_block(f, lv)))
        .collect();
    let want_b: Vec<Vec<String>> = want
        .iter()
        .map(|w| w.replace("\r\n", "\n").split('\n').map(str::to_owned).collect())
        .collect();
    if have.len() == want_b.len() && have.iter().zip(&want_b).all(|(a, b)| *a == b) {
        return (None, format!("package.mft: the level's {} blocks there", want_b.len()));
    }
    let mut out: Vec<Vec<String>> = blocks
        .iter()
        .filter(|b| !b.first().is_some_and(|f| is_own_block(f, lv)))
        .cloned()
        .collect();
    let n_have = have.len();
    out.extend(want_b.iter().cloned());
    (
        Some(mft_text(&head, &out)),
        format!(
            "package.mft: the level's blocks written ({n_have} there before, {} now)",
            want_b.len()
        ),
    )
}

// ------------------------------------------------------------------------------------------------ bundleManifest

/// `bm_with_level`: (the new text or None, the lines added, the lines repaired - a line missing its third field
/// gets `\t1`).
#[must_use]
pub fn bm_with_level(text: &str, lv: &str, kinds: &[String]) -> (Option<String>, Vec<String>, Vec<String>) {
    let lines: Vec<&str> = text.split('\n').collect();
    let add: Vec<String> = kinds
        .iter()
        .map(|k| bm_line(lv, k))
        .filter(|l| !lines.contains(&l.as_str()))
        .collect();
    let broken: Vec<String> = lines
        .iter()
        .filter(|l| MANIFEST_FIX.is_match(l))
        .map(|l| (*l).to_owned())
        .collect();
    if add.is_empty() && broken.is_empty() {
        return (None, add, broken);
    }
    let mut keep: Vec<String> = lines
        .iter()
        .filter(|l| !l.is_empty())
        .map(|l| {
            if MANIFEST_FIX.is_match(l) {
                format!("{l}\t1")
            } else {
                (*l).to_owned()
            }
        })
        .collect();
    keep.extend(add.iter().cloned());
    (Some(keep.join("\n") + "\n"), add, broken)
}

/// Whether a bundleManifest line misses its third field (`MANIFEST_FIX`).
#[must_use]
pub fn manifest_line_broken(line: &str) -> bool {
    MANIFEST_FIX.is_match(line)
}

/// Python's `repr` of a list of strings (the bundleManifest report line's `[...]`).
#[must_use]
pub fn py_list_repr(items: &[String]) -> String {
    let parts: Vec<String> = items.iter().map(|s| py_repr(s)).collect();
    format!("[{}]", parts.join(", "))
}

/// Python's `repr` of a string.
#[must_use]
pub fn py_repr(s: &str) -> String {
    let q = if s.contains('\'') && !s.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::new();
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
            c if (c as u32) < 0x20 || c as u32 == 0x7f || (0x80..0xa0).contains(&(c as u32)) => {
                let _ = write!(out, "\\x{:02x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push(q);
    out
}

// ------------------------------------------------------------------------------------------------ ini files

fn section_of(body: &str) -> Option<String> {
    INI_SECTION
        .captures(body)
        .and_then(|m| m.get(1))
        .map(|g| g.as_str().to_lowercase())
}

/// `ini_value`: `[section] key`'s value (the first such line, stripped); None when absent.
#[must_use]
pub fn ini_value(text: &str, section: &str, key: &str) -> Option<String> {
    let (section, key) = (section.to_lowercase(), key.to_lowercase());
    let mut cur: Option<String> = None;
    for line in splitlines(text) {
        if let Some(s) = section_of(line) {
            cur = Some(s);
            continue;
        }
        if cur.as_deref() == Some(section.as_str())
            && !INI_COMMENT.is_match(line)
            && let Some((k, v)) = line.split_once('=')
            && strip(k).to_lowercase() == key
        {
            return Some(strip(v).to_owned());
        }
    }
    None
}

/// `ini_with_level`: the text with its `[dev]` section's `level` key set to value (the first level line of
/// `[dev]`, its spelling, the spaces around `=` and its line end kept).
///
/// # Errors
/// No `[dev] level=` line (`opfor.ini: no [dev] level= line`).
pub fn ini_with_level(text: &str, value: &str) -> Result<String, TextError> {
    let mut lines: Vec<String> = splitlines_keep(text).into_iter().map(str::to_owned).collect();
    let mut section: Option<String> = None;
    for line in &mut lines {
        let body = rstrip_nl(line).to_owned();
        if let Some(s) = section_of(&body) {
            section = Some(s);
            continue;
        }
        if section.as_deref() != Some("dev") || INI_COMMENT.is_match(&body) {
            continue;
        }
        if let Some(k) = LEVEL_KEY.captures(&body).and_then(|m| m.get(1)) {
            let end = line.get(body.len()..).unwrap_or_default().to_owned();
            *line = format!("{}{value}{end}", k.as_str());
            return Ok(lines.concat());
        }
    }
    Err(TextError("opfor.ini: no [dev] level= line".to_owned()))
}

fn key_line(key: &str, assign: bool) -> Regex {
    let k = regex::escape(key);
    if assign {
        re(&format!(r"(?i)^(\s*{k}\s*=\s*)(.*)$"))
    } else {
        re(&format!(r"(?i)^\s*{k}\s*="))
    }
}

/// `ini_without_key`: the text without `[section] key`'s lines (every other line kept as it is).
#[must_use]
pub fn ini_without_key(text: &str, section: &str, key: &str) -> String {
    let section = section.to_lowercase();
    let pat = key_line(key, false);
    let mut cur: Option<String> = None;
    let mut out = String::new();
    for line in splitlines_keep(text) {
        let body = rstrip_nl(line);
        if let Some(s) = section_of(body) {
            cur = Some(s);
        } else if cur.as_deref() == Some(section.as_str()) && pat.is_match(body) {
            continue;
        }
        out.push_str(line);
    }
    out
}

/// `ini_with_key`: the text with `[section] key = value` - the key's first line in the section replaced (its spelling
/// kept), else a line added after the section's last key line, else the section added at the end.
#[must_use]
pub fn ini_with_key(text: &str, section: &str, key: &str, value: &str) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let sec = section.to_lowercase();
    let pat = key_line(key, true);
    let mut lines: Vec<String> = splitlines_keep(text).into_iter().map(str::to_owned).collect();
    let mut cur: Option<String> = None;
    let mut last: Option<usize> = None;
    for (i, line) in lines.iter_mut().enumerate() {
        let body = rstrip_nl(line).to_owned();
        if let Some(s) = section_of(&body) {
            if s == sec {
                last = Some(i);
            }
            cur = Some(s);
            continue;
        }
        if cur.as_deref() != Some(sec.as_str()) || INI_COMMENT.is_match(&body) || strip(&body).is_empty() {
            continue;
        }
        last = Some(i);
        if let Some(k) = pat.captures(&body).and_then(|m| m.get(1)) {
            let end = line.get(body.len()..).unwrap_or_default().to_owned();
            *line = format!("{}{value}{end}", k.as_str());
            return lines.concat();
        }
    }
    let add = format!("{key}={value}{nl}");
    let Some(last) = last else {
        let sep = if text.is_empty() || text.ends_with(['\n', '\r']) {
            ""
        } else {
            nl
        };
        return format!("{text}{sep}{nl}[{section}]{nl}{add}");
    };
    if let Some(l) = lines.get_mut(last)
        && !l.ends_with(['\n', '\r'])
    {
        l.push_str(nl);
    }
    lines.insert(last + 1, add);
    lines.concat()
}

/// A level's `maps.ini` section: `[<NAME>]`, `name=<title>`, `<browser mode>=Levels/<NAME>`.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct MapsLevel {
    /// The level's name (`WAKE_ISLAND_S`).
    pub name: String,
    /// Its title (`Wake Island`).
    pub title: String,
    /// The Server Browser's mode key (`Conquest`, `Air Superiority`).
    pub browser_mode: String,
}

/// `maps_with_levels`: `maps.ini` with one section per level - every section named in `strip` (lower case) and the
/// marker line out first, then the marker and the sections appended. Other sections kept as they are.
#[must_use]
pub fn maps_with_levels(text: &str, levels: &[MapsLevel], strip_names: &[String], mark: &str) -> String {
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let mut out: Vec<&str> = Vec::new();
    let mut skip = false;
    for line in splitlines_keep(text) {
        let body = rstrip_nl(line);
        if let Some(s) = section_of(body) {
            skip = strip_names.contains(&s);
        }
        if skip || strip(body) == mark {
            continue;
        }
        out.push(line);
    }
    while out.last().is_some_and(|l| strip(l).is_empty()) {
        out.pop();
    }
    let mut new = out.concat();
    if !new.is_empty() && !new.ends_with(['\n', '\r']) {
        new.push_str(nl);
    }
    if levels.is_empty() {
        return new;
    }
    let blocks: Vec<String> = levels
        .iter()
        .map(|lv| {
            format!(
                "[{n}]{nl}name={t}{nl}{m}=Levels/{n}{nl}",
                n = lv.name,
                t = lv.title,
                m = lv.browser_mode
            )
        })
        .collect();
    format!("{new}{nl}{mark}{nl}{}", blocks.join(nl))
}

/// `pacific_levels_value`: the level names as `Levels/<NAME>`, comma-separated.
#[must_use]
pub fn levels_value(names: &[String]) -> String {
    names
        .iter()
        .map(|n| format!("Levels/{n}"))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
