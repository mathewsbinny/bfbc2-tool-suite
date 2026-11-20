//! Battlefield 1943's Xbox 360 bundles as `bf1943import.Source` opens them (round O5, for `bf1943import`'s command
//! line and texqa's BF1943 sources): the bundles under the folder in `os.walk`'s order, each checked as the BF1943
//! tool suite's `fbrb1943.Bundle` checks a whole file (the bundles it refuses left out), the entries by lower-case
//! name (the first bundle holding a name wins), an entry's bytes from its bundle's payload.
//!
//! **The index** (`Source._load_index` / `_save_index`, `%LOCALAPPDATA%\bfbc2-tool-suite\cache\bf1943source`): a
//! valid index's rows stand for the bundles it names unchanged (size and modification time); when the open would write
//! the index (a bundle checked, one gone, no index yet), the command line is handed to the Python, which writes it -
//! its pickle is the Python's own bytes. With `BF1943_SOURCE_INDEX=0` no index is read or written.
//!
//! **The reader.** `fbrb1943.py` is the BF1943 tool suite's, found as `bf1943import.fbrb1943` finds it; this port is
//! of the version whose SHA-1 is [`READER_SHA1`]. Another version (or `BF1943_SOURCE_WHOLE`, the reference reader) is
//! handed to the Python.

#![allow(
    clippy::indexing_slicing,
    reason = "fbrb1943.Bundle's reads: every index after its bound is checked, as the reader checks them"
)]

use std::collections::HashMap;
use std::io::Read as _;

use bc2_pyrt::pickle::{self, Value};

use crate::fsx;
use crate::pyos;
use crate::pyproc::{Fail, PyErr, PyResult};

/// `fbrb1943.py`'s SHA-1 (bf1943-tool-suite e56d878): the reader this port follows.
pub const READER_SHA1: &str = "ad8238401cd380f834b2b1ae30606a8ca4783db3";
/// `bf1943import.INDEX_VERSION`
const INDEX_VERSION: i64 = 1;

/// `Bf1943ImportError(message)`, with its class's `what_to_do`.
pub fn import_error(message: impl Into<String>) -> Fail {
    let mut e = PyErr::new("Bf1943ImportError", message);
    e.what_to_do = Some(
        "Check the names with \"list\", the --source folder (BF1943's extracted dist\\xenon) and --out; python \
         bf1943import.py --help shows the commands."
            .to_owned(),
    );
    Fail::Err(e)
}

/// A directory entry (`fbrb1943.Bundle.entries`' dicts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub kind: String,
    pub flags: u32,
    pub offset: u32,
    pub size: u32,
}

/// What `Source` keeps of a bundle (`_BundleInfo`).
#[derive(Debug, Clone)]
pub struct Bundle {
    pub path: String,
    pub entries: Vec<Entry>,
    pub compressed: bool,
    pub payload_at: u64,
    pub payload_len: u64,
}

/// `bf1943import.Source(folder)`.
pub struct Source {
    pub folder: String,
    /// rel -> bundle, in the order opened
    pub bundles: Vec<(String, Bundle)>,
    /// lower-case name -> (bundle index, entry index)
    pub entries: HashMap<String, (usize, usize)>,
    payload: Option<(usize, Vec<u8>)>,
}

/// `gzip.decompress(data)` (Python 3.13: every member, zero padding between members stripped), or None where it
/// raises.
pub fn gunzip(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        if data.is_empty() {
            return Some(out);
        }
        if data.len() < 2 || data[..2] != [0x1f, 0x8b] {
            return None;
        }
        let head = data.get(..10)?;
        if head[2] != 8 {
            return None;
        }
        let flag = head[3];
        let mut p = 10usize;
        if flag & 4 != 0 {
            let n = usize::from(u16::from_le_bytes(data.get(p..p + 2)?.try_into().ok()?));
            p += 2;
            // _read_exact of the extra field
            data.get(p..p + n)?;
            p += n;
        }
        for bit in [8u8, 16] {
            if flag & bit != 0 {
                let z = data.get(p..)?.iter().position(|&c| c == 0)?;
                p += z + 1;
            }
        }
        if flag & 2 != 0 {
            data.get(p..p + 2)?;
            p += 2;
        }
        let body = data.get(p..)?;
        let mut d = flate2::Decompress::new(false);
        let mut member = Vec::new();
        loop {
            member.reserve(1 << 20);
            let before = d.total_in();
            let st = d
                .decompress_vec(
                    body.get(usize::try_from(before).ok()?..)?,
                    &mut member,
                    flate2::FlushDecompress::None,
                )
                .ok()?;
            if st == flate2::Status::StreamEnd {
                break;
            }
            if d.total_in() == before && member.len() < member.capacity() {
                return None; // the stream ends before its end marker
            }
        }
        let rest = body.get(usize::try_from(d.total_in()).ok()?..)?;
        let tail = rest.get(..8)?;
        let crc = u32::from_le_bytes(tail[..4].try_into().ok()?);
        let len = u32::from_le_bytes(tail[4..8].try_into().ok()?);
        let mut h = flate2::Crc::new();
        h.update(&member);
        if crc != h.sum() || len != (member.len() as u64 & 0xFFFF_FFFF) as u32 {
            return None;
        }
        out.extend_from_slice(&member);
        let after = &rest[8..];
        let skip = after.iter().position(|&c| c != 0).unwrap_or(after.len());
        data = &after[skip..];
    }
}

fn be(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)?.try_into().ok().map(u32::from_be_bytes)
}

/// `fbrb1943.Bundle(raw)`'s verdict: (entries, compressed, payload start, payload length, the payload), None where
/// it raises `ValueError` (the bundle is refused).
pub fn check(raw: &[u8]) -> Option<(Vec<Entry>, bool, u64, Vec<u8>)> {
    if raw.get(..4) != Some(b"FbRB".as_slice()) || raw.len() < 8 {
        return None;
    }
    let cut = be(raw, 4)? as usize;
    if 8 + cut > raw.len() {
        return None;
    }
    let dir = gunzip(&raw[8..8 + cut])?;
    if dir.len() < 12 || be(&dir, 0)? != 2 {
        return None;
    }
    let strlen = be(&dir, 4)? as usize;
    if 12 + strlen > dir.len() {
        return None;
    }
    let count = be(&dir, 8 + strlen)? as usize;
    let entries_at = 12 + strlen;
    let tail_at = entries_at.checked_add(count.checked_mul(24)?)?;
    if tail_at + 5 != dir.len() {
        return None;
    }
    let compressed = dir[tail_at] != 0;
    let body = &raw[8 + cut..];
    let payload = if compressed { gunzip(body)? } else { body.to_vec() };
    if be(&dir, tail_at + 1)? as usize != payload.len() {
        return None;
    }
    // cstr: a NUL inside the table (directory.find(0, 8 + offset, entries_at - 4)), ASCII
    let cstr = |offset: u32| -> Option<String> {
        let start = 8 + offset as usize;
        let end = entries_at - 4;
        let z = dir.get(start..end)?.iter().position(|&c| c == 0)?;
        let s = &dir[start..start + z];
        if !s.is_ascii() {
            return None;
        }
        Some(String::from_utf8_lossy(s).into_owned())
    };
    let mut entries = Vec::with_capacity(count);
    let mut pos: u64 = 0;
    for k in 0..count {
        let at = entries_at + 24 * k;
        let w = |i: usize| be(&dir, at + 4 * i);
        let (name, flags, offset, size, size2, kind) = (w(0)?, w(1)?, w(2)?, w(3)?, w(4)?, w(5)?);
        let e = Entry {
            name: cstr(name)?,
            kind: cstr(kind)?,
            flags,
            offset,
            size,
        };
        if u64::from(e.offset) != pos || size != size2 {
            return None;
        }
        pos += u64::from(size);
        entries.push(e);
    }
    if pos != payload.len() as u64 {
        return None;
    }
    Some((entries, compressed, 8 + cut as u64, payload))
}

/// `bf1943import.default_source()`: `BF1943_XENON`, else Project Rising Sun's workspace beside the suite.
pub fn default_source(suite: &str) -> String {
    if let Ok(v) = std::env::var("BF1943_XENON")
        && !v.is_empty()
    {
        return v;
    }
    bc2_pyrt::py::join(
        &bc2_pyrt::py::dirname(suite),
        &["operation-rising-sun", "work", "extracted", "dist", "xenon"],
    )
}

/// `bf1943import.fbrb1943()`'s folder: beside the suite first, then `BF1943_SUITE_FBRB`; the reader's SHA-1 checked
/// (another version is the Python's to read).
fn reader(suite: &str) -> PyResult<String> {
    let here = bc2_pyrt::py::join(
        &bc2_pyrt::py::dirname(suite),
        &["bf1943-tool-suite", "bf1943-fbrb-tool"],
    );
    let there = std::env::var("BF1943_SUITE_FBRB").unwrap_or_default();
    for p in [here, there] {
        if !p.is_empty() && pyos::isfile(&bc2_pyrt::py::join(&p, &["fbrb1943.py"])) {
            let file = bc2_pyrt::py::join(&p, &["fbrb1943.py"]);
            let data = std::fs::read(&file).map_err(|_| Fail::Python)?;
            if pyos::sha1_hex(&data) != READER_SHA1 {
                return Err(Fail::Python);
            }
            return Ok(file);
        }
    }
    Err(import_error(
        "the BF1943 tool suite (bf1943-tool-suite\\bf1943-fbrb-tool\\fbrb1943.py) was not found next to this suite: \
         its bundle reader reads the Xbox 360 bundles",
    ))
}

/// `bf1943import.index_dir()`.
fn index_dir() -> Option<String> {
    let v = std::env::var("BF1943_SOURCE_INDEX").unwrap_or_default();
    if v == "0" || v.to_lowercase() == "off" {
        return None;
    }
    if !v.is_empty() {
        return Some(v);
    }
    let base = std::env::var("LOCALAPPDATA")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(crate::pylog::home_dir);
    Some(bc2_pyrt::py::join(
        &base,
        &["bfbc2-tool-suite", "cache", "bf1943source"],
    ))
}

/// An index row: (size, `mtime_ns`, entries or None, compressed, payload start, payload length).
type Row = (i128, i128, Option<Vec<Entry>>, bool, u64, u64);

fn int(v: &Value) -> Option<i128> {
    v.as_int().map(i128::from)
}

/// The rows of a valid index, or None (missing, old, damaged, another reader or folder: the Python checks again).
#[allow(
    clippy::many_single_char_names,
    reason = "the reference's _load_index names: v, f, e and its small readers"
)]
fn load_index(p: &str, reader_sig: &str, folder_nc: &str) -> Option<HashMap<String, Row>> {
    let data = std::fs::read(p).ok()?;
    let v = pickle::loads(&data).ok()?;
    let text = |x: &Value| x.as_str().map(str::to_owned);
    if v.get("version").as_ref().and_then(Value::as_int) != Some(INDEX_VERSION)
        || v.get("reader").as_ref().and_then(text).as_deref() != Some(reader_sig)
        || v.get("folder").as_ref().and_then(text).as_deref() != Some(folder_nc)
    {
        return None;
    }
    let mut out = HashMap::new();
    for (k, row) in v.get("bundles")?.items() {
        let rel = k.as_str()?.to_owned();
        let f = row.elements();
        if f.len() != 6 {
            return None;
        }
        let entries = if matches!(f[2], Value::None) {
            None
        } else {
            let mut es = Vec::new();
            for e in f[2].elements() {
                let s = |k: &str| e.get(k).as_ref().and_then(text);
                let n = |k: &str| {
                    e.get(k)
                        .as_ref()
                        .and_then(Value::as_int)
                        .and_then(|x| u32::try_from(x).ok())
                };
                es.push(Entry {
                    name: s("name")?,
                    kind: s("type")?,
                    flags: n("flags")?,
                    offset: n("offset")?,
                    size: n("size")?,
                });
            }
            Some(es)
        };
        let compressed = match &f[3] {
            Value::Bool(b) => *b,
            _ => return None,
        };
        out.insert(
            rel,
            (
                int(&f[0])?,
                int(&f[1])?,
                entries,
                compressed,
                u64::try_from(int(&f[4])?).ok()?,
                u64::try_from(int(&f[5])?).ok()?,
            ),
        );
    }
    Some(out)
}

impl Source {
    /// `Source(folder)`; `suite`: the suite folder (`paths.SUITE`).
    pub fn open(folder: &str, suite: &str) -> PyResult<Self> {
        if std::env::var("BF1943_SOURCE_WHOLE").is_ok_and(|v| !v.is_empty() && v != "0") {
            return Err(Fail::Python); // WholeSource, the reference
        }
        if !pyos::isdir(folder) {
            return Err(import_error(format!(
                "no BF1943 source folder at {folder} (pass --source: the extracted dist\\xenon folder)"
            )));
        }
        let abs = pyos::abspath(folder)?;
        let reader_file = reader(suite)?;
        // the bundle files in os.walk's order, each folder's files sorted
        let mut files: Vec<(String, String)> = Vec::new();
        for (dp, _dirs, mut fs) in pyos::walk(&abs)? {
            fs.sort();
            for f in fs {
                #[allow(
                    clippy::case_sensitive_file_extension_comparisons,
                    reason = "Python's f.endswith('.fbrb') is case-sensitive: an .FBRB file is not a bundle to it"
                )]
                let bundle = f.ends_with(".fbrb");
                if bundle {
                    let path = pyos::join2(&dp, &f)?;
                    let rel = pyos::relpath(&path, &abs)?.replace('\\', "/");
                    files.push((rel, path));
                }
            }
        }
        let folder_nc = pyos::normcase(&abs);
        let index_path = index_dir().map(|d| {
            let key = pyos::sha1_hex(folder_nc.as_bytes());
            bc2_pyrt::py::join(&d, &[&format!("{}.pkl", &key[..20])])
        });
        let reader_sig = pyos::sha1_hex(&std::fs::read(&reader_file).map_err(|_| Fail::Python)?);
        let index = index_path
            .as_deref()
            .and_then(|p| load_index(p, &reader_sig, &folder_nc));
        if let (Some(p), Some(_)) = (&index_path, &index) {
            // caches.touch(p): marked used unless within the last minute
            if let Ok(st) = fsx::stat(p)
                && fsx::now() - st.mtime >= 60.0
            {
                let _ = fsx::utime_now(p);
            }
        }
        let index = index.unwrap_or_default();
        let mut fresh_keys: Vec<&str> = Vec::new();
        let mut checked = false;
        let mut src = Self {
            folder: abs.clone(),
            bundles: Vec::new(),
            entries: HashMap::new(),
            payload: None,
        };
        for (rel, path) in &files {
            let st = fsx::stat(path).ok();
            let row = match (st, index.get(rel)) {
                (Some(st), Some(row)) if row.0 == i128::from(st.size) && row.1 == st.mtime_ns => row.clone(),
                _ => {
                    checked = true;
                    let raw = std::fs::read(path).map_err(|_| Fail::Python)?;
                    let (size, mtime) = st.map_or((-1, -1), |s| (i128::from(s.size), s.mtime_ns));
                    match check(&raw) {
                        Some((entries, compressed, at, payload)) => {
                            (size, mtime, Some(entries), compressed, at, payload.len() as u64)
                        }
                        None => (size, mtime, None, false, 0, 0),
                    }
                }
            };
            if st.is_some() {
                fresh_keys.push(rel);
            }
            let (_, _, entries, compressed, payload_at, payload_len) = row;
            let Some(entries) = entries else { continue };
            let bi = src.bundles.len();
            for (ei, e) in entries.iter().enumerate() {
                src.entries.entry(e.name.to_lowercase()).or_insert((bi, ei));
            }
            src.bundles.push((
                rel.clone(),
                Bundle {
                    path: path.clone(),
                    entries,
                    compressed,
                    payload_at,
                    payload_len,
                },
            ));
        }
        if src.entries.is_empty() {
            return Err(import_error(format!("{folder}: no BF1943 bundles in it")));
        }
        // the index would be written (fresh != index): the Python writes its own pickle
        if index_path.is_some()
            && (checked || fresh_keys.len() != index.len() || fresh_keys.iter().any(|k| !index.contains_key(*k)))
        {
            return Err(Fail::Python);
        }
        Ok(src)
    }

    /// `Source.find(filters)`: the lower-case names holding every filter, sorted.
    pub fn find(&self, filters: &[String]) -> Vec<String> {
        let fl: Vec<String> = filters.iter().map(|f| f.to_lowercase().replace('\\', "/")).collect();
        let mut out: Vec<String> = self
            .entries
            .keys()
            .filter(|k| fl.iter().all(|f| k.contains(f.as_str())))
            .cloned()
            .collect();
        out.sort();
        out
    }

    /// The bundle and entry of a lower-case name.
    pub fn entry(&self, key: &str) -> Option<(&str, &Entry)> {
        let &(bi, ei) = self.entries.get(key)?;
        let (rel, b) = self.bundles.get(bi)?;
        Some((rel.as_str(), b.entries.get(ei)?))
    }

    /// `Source.data(key)`: the entry's bytes (its bundle's payload decoded once and kept until another bundle is
    /// read). None where the Python's read would raise (the file changed or cannot be read).
    pub fn data(&mut self, key: &str) -> Option<Vec<u8>> {
        let &(bi, ei) = self.entries.get(key)?;
        let (_, b) = self.bundles.get(bi)?;
        let e = b.entries.get(ei)?.clone();
        if !b.compressed {
            let mut f = std::fs::File::open(&b.path).ok()?;
            std::io::Seek::seek(&mut f, std::io::SeekFrom::Start(b.payload_at + u64::from(e.offset))).ok()?;
            let mut out = Vec::new();
            f.take(u64::from(e.size)).read_to_end(&mut out).ok()?;
            return (out.len() == e.size as usize).then_some(out);
        }
        if self.payload.as_ref().is_none_or(|(k, _)| *k != bi) {
            self.payload = None;
            let raw = std::fs::read(&b.path).ok()?;
            let body = raw.get(usize::try_from(b.payload_at).ok()?..)?;
            let p = gunzip(body)?;
            if p.len() as u64 != b.payload_len {
                return None;
            }
            self.payload = Some((bi, p));
        }
        let (_, p) = self.payload.as_ref()?;
        let start = e.offset as usize;
        Some(p.get(start..start + e.size as usize)?.to_vec())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn gz(data: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn gzip_as_python() {
        assert_eq!(gunzip(b"").unwrap(), b"");
        let mut two = gz(b"hello ");
        two.extend([0, 0, 0]);
        two.extend(gz(b"world"));
        assert_eq!(gunzip(&two).unwrap(), b"hello world");
        let one = gz(b"abc");
        assert!(gunzip(&one[..one.len() - 3]).is_none());
        let mut bad = one.clone();
        bad.push(1);
        assert!(gunzip(&bad).is_none());
    }
}
