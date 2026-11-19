//! Python's pieces the ported command lines lean on (round O2's `menucommon` twin, moved here in round O3): an
//! ordered dict, `str.strip` / `split` / `splitlines`, text files as Python reads and writes them, and `ntpath`.

#![allow(
    clippy::indexing_slicing,
    reason = "each index is bounded by the check before it, as the reference's"
)]

use std::collections::HashMap;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use crate::err::{self, PyResult};
/// A Python dict: insertion order kept, a key set again keeps its place.
#[derive(Debug, Clone)]
pub struct OMap<K, V> {
    pub items: Vec<(K, V)>,
    ix: HashMap<K, usize>,
}

impl<K: Eq + Hash + Clone, V> Default for OMap<K, V> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            ix: HashMap::new(),
        }
    }
}

impl<K: Eq + Hash + Clone, V> OMap<K, V> {
    pub fn insert(&mut self, k: K, v: V) {
        if let Some(&i) = self.ix.get(&k) {
            if let Some(e) = self.items.get_mut(i) {
                e.1 = v;
            }
        } else {
            self.ix.insert(k.clone(), self.items.len());
            self.items.push((k, v));
        }
    }

    pub fn get(&self, k: &K) -> Option<&V> {
        self.ix.get(k).and_then(|&i| self.items.get(i)).map(|e| &e.1)
    }

    pub fn get_mut(&mut self, k: &K) -> Option<&mut V> {
        match self.ix.get(k) {
            Some(&i) => self.items.get_mut(i).map(|e| &mut e.1),
            None => None,
        }
    }

    pub fn contains(&self, k: &K) -> bool {
        self.ix.contains_key(k)
    }

    /// `setdefault`.
    #[allow(clippy::needless_pass_by_value)] // dict.setdefault(k, v()): the key is stored when new
    pub fn entry_or(&mut self, k: K, v: impl FnOnce() -> V) -> &mut V {
        if !self.ix.contains_key(&k) {
            self.insert(k.clone(), v());
        }
        let i = self.ix.get(&k).copied().unwrap_or(0);
        &mut self.items[i].1
    }

    /// `pop(k, None)`.
    pub fn remove(&mut self, k: &K) {
        if self.ix.remove(k).is_some() {
            self.items.retain(|e| &e.0 != k);
            self.ix = self.items.iter().enumerate().map(|(i, e)| (e.0.clone(), i)).collect();
        }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.items.iter().map(|e| &e.0)
    }
}

/// BIGF members (or a `.apt` / `.const` pair): `{name: bytes}`.
pub type Members = OMap<String, Vec<u8>>;

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
    s.split(py_space).filter(|t| !t.is_empty()).collect()
}

/// `s.splitlines()` (the line ends dropped).
pub fn splitlines(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        let end = match c {
            '\r' => {
                if let Some(&(_, '\n')) = it.peek() {
                    it.next();
                    i + 2
                } else {
                    i + 1
                }
            }
            '\n' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}' => {
                i + c.len_utf8()
            }
            _ => continue,
        };
        out.push(&s[start..i]);
        start = end;
    }
    if start < s.len() {
        out.push(&s[start..]);
    }
    out
}

/// A text file read as `open(path, encoding='utf-8', errors='replace').read()` does (universal newlines).
pub fn read_text(path: &str) -> PyResult<String> {
    let b = err::read_file(path)?;
    let t = String::from_utf8_lossy(&b);
    Ok(t.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `open(path, 'w', encoding='utf-8').write(text)` (`\n` written as `\r\n`).
pub fn write_text(path: &str, text: &str) -> PyResult<()> {
    err::write_file(path, text.replace('\n', "\r\n").as_bytes())
}

pub fn is_sep(c: char) -> bool {
    c == '\\' || c == '/'
}

/// `ntpath.splitdrive(p)[0]`'s length (a drive letter or a UNC share).
pub fn drive_len(p: &str) -> usize {
    let b = p.as_bytes();
    if b.len() >= 2 && b[1] == b':' {
        return 2;
    }
    if b.len() >= 2 && is_sep(char::from(b[0])) && is_sep(char::from(b[1])) {
        // \\server\share
        let rest = &p[2..];
        if let Some(i) = rest.find(is_sep) {
            let after = &rest[i + 1..];
            let j = after.find(is_sep).unwrap_or(after.len());
            if i > 0 && j > 0 {
                return 2 + i + 1 + j;
            }
        }
    }
    0
}

/// `ntpath.split(p)`: (head, tail).
pub fn nt_split(p: &str) -> (String, String) {
    let d = drive_len(p);
    let (drive, rest) = p.split_at(d);
    let i = rest.rfind(is_sep).map_or(0, |i| i + 1);
    let (head, tail) = rest.split_at(i);
    let trimmed = head.trim_end_matches(is_sep);
    let head = if trimmed.is_empty() { head } else { trimmed };
    (format!("{drive}{head}"), tail.to_owned())
}

/// `os.path.basename`.
pub fn basename(p: &str) -> String {
    nt_split(p).1
}

/// `os.path.dirname`.
pub fn dirname(p: &str) -> String {
    nt_split(p).0
}

/// `os.path.splitext`: (root, ext).
pub fn splitext(p: &str) -> (String, String) {
    let sep = p.rfind(is_sep).map_or(0, |i| i + 1);
    let dot = p.rfind('.').filter(|&d| d >= sep);
    if let Some(d) = dot {
        // leading dots of the name are not an extension
        if p[sep..d].chars().any(|c| c != '.') {
            return (p[..d].to_owned(), p[d..].to_owned());
        }
    }
    (p.to_owned(), String::new())
}

/// `os.path.join(a, *names)` for names that are neither absolute nor carry a drive.
pub fn join(a: &str, names: &[&str]) -> String {
    let mut out = a.to_owned();
    for n in names {
        if out.is_empty() || out.ends_with(is_sep) || (out.len() == 2 && out.ends_with(':')) {
            out.push_str(n);
        } else {
            out.push('\\');
            out.push_str(n);
        }
    }
    out
}

/// `os.path.isfile`.
pub fn isfile(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_file()
}

/// `os.path.isdir`.
pub fn isdir(p: &str) -> bool {
    !p.is_empty() && Path::new(p).is_dir()
}

/// `os.path.abspath` (Windows: `GetFullPathNameW`, as `std::path::absolute`).
pub fn abspath(p: &str) -> String {
    std::path::absolute(p).map_or_else(|_| p.to_owned(), |a| a.to_string_lossy().into_owned())
}

/// The suite whose Python answers a hand-off: `BFBC2_CLI_SUITE` (the tests' reference copy), else the first folder
/// above the binary with `suite_paths.py`, else the checkout it was built in.
pub fn suite_root() -> PathBuf {
    if let Some(p) = std::env::var_os("BFBC2_CLI_SUITE").filter(|p| !p.is_empty()) {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        for dir in exe.ancestors().skip(1) {
            if dir.join("suite_paths.py").is_file() {
                return dir.to_path_buf();
            }
        }
    }
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}
