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
//! **The reader** is the suite's one BF1943 bundle reader, `bc2_formats::bf1943` (round PR1b-1-rest): its
//! `check` is `fbrb1943.Bundle`'s verdict for the version whose SHA-1 is [`READER_SHA1`] (`fbrb1943.py` is found as
//! `bf1943import.fbrb1943` finds it; another version, or `BF1943_SOURCE_WHOLE`, the reference reader, is handed to the
//! Python), and an entry's bytes come through its restart points ([`Source::data`]).

#![allow(
    clippy::indexing_slicing,
    reason = "the index rows' six fields, their count checked first"
)]

use std::collections::HashMap;
use std::io::Read as _;

use bc2_pyrt::pickle::{self, Value};

use crate::pyos;
use crate::pyos::files as fsx;
use crate::pyproc::{Fail, PyErr, PyResult};

pub use bc2_formats::bf1943::{Entry, READER_SHA1, check};

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

/// What `Source` keeps of a bundle (`_BundleInfo`).
#[derive(Debug, Clone)]
pub struct Bundle {
    pub path: String,
    pub entries: Vec<Entry>,
    pub compressed: bool,
    pub payload_at: u64,
    pub payload_len: u64,
    /// The file's size and modification time (ns) at the open.
    pub size: u64,
    pub mtime_ns: i128,
}

/// `bf1943import.Source(folder)`.
pub struct Source {
    pub folder: String,
    /// rel -> bundle, in the order opened
    pub bundles: Vec<(String, Bundle)>,
    /// lower-case name -> (bundle index, entry index)
    pub entries: HashMap<String, (usize, usize)>,
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
                        Some(c) => (size, mtime, Some(c.entries), c.compressed, c.payload_at, c.payload_len),
                        None => (size, mtime, None, false, 0, 0),
                    }
                }
            };
            if st.is_some() {
                fresh_keys.push(rel);
            }
            let (size, mtime_ns, entries, compressed, payload_at, payload_len) = row;
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
                    size: u64::try_from(size).unwrap_or(0),
                    mtime_ns,
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

    /// `Source.data(key)`: the entry's bytes (a compressed payload's through the process's restart points,
    /// `bc2_formats::bf1943`, kept on disk beside the index). None where the Python's read would raise (the file
    /// changed or cannot be read).
    pub fn data(&self, key: &str) -> Option<Vec<u8>> {
        let &(bi, ei) = self.entries.get(key)?;
        let (_, b) = self.bundles.get(bi)?;
        let e = b.entries.get(ei)?;
        let st = fsx::stat(&b.path).ok()?;
        if st.size != b.size || st.mtime_ns != b.mtime_ns {
            return None;
        }
        if !b.compressed {
            let mut f = std::fs::File::open(&b.path).ok()?;
            std::io::Seek::seek(&mut f, std::io::SeekFrom::Start(b.payload_at + u64::from(e.offset))).ok()?;
            let mut out = Vec::new();
            f.take(u64::from(e.size)).read_to_end(&mut out).ok()?;
            return (out.len() == e.size as usize).then_some(out);
        }
        let r = bc2_formats::bf1943::PayloadRef {
            path: b.path.clone().into(),
            size: b.size,
            mtime_ns: b.mtime_ns,
            payload_at: b.payload_at,
            payload_len: b.payload_len,
        };
        let store = index_dir().map(|d| bc2_gamefs::points::Folder(d.into()));
        bc2_formats::bf1943::global()
            .read(
                &r,
                u64::from(e.offset),
                u64::from(e.size),
                store.as_ref().map(|s| s as &dyn bc2_formats::bf1943::PointsStore),
                bc2_formats::bf1943::Budgets::from_env(),
            )
            .ok()
            .map(|(d, _)| d)
    }
}
