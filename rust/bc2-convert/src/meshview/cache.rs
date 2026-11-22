//! `meshview.Index`'s game copy and cache (`bfbc2-mesh-viewer\meshview.py`; rounds O3 and GR3): the stock bundles
//! (`meshview._bundles`, Package before Dist, OPFOR's own levels left out), their signature, and the index's cache
//! `work\tools_out\meshview_index.pkl` read and written as the Python writes it (its pickle, byte for byte), so the
//! Python tools, `bfbc2.exe meshview` and the map editor's asset preview share one cache.
//!
//! Moved here from `bc2-cli`'s `gameindex` in round GR3 so the editor reads the same index.

#![allow(
    clippy::many_single_char_names,
    reason = "the pickle's keys and values: k, v, b, n, d"
)]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::LazyLock;

use bc2_formats::meshindex;
use bc2_pyrt::pickle::{self, Value};
use bc2_pyrt::pickle_write;
use bc2_pyrt::py::join;
use regex::Regex;

/// `meshview.CACHE_VERSION`.
pub const CACHE_VERSION: i64 = 3;

/// A bundle's signature: (path, `st_size`, `st_mtime_ns`).
pub type Sig = (String, u64, i128);

static SKIP: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^levels/(ons_|opfor_)").ok());
static INSTALLED: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?s)^opfor-.+-installed\.json\z").ok());

/// `os.walk(top)` with the folders sorted as `_bundles` sorts them: each folder's (path, sub-folder names, file
/// names), top-down; `visit` returns false to prune the folder's sub-folders.
fn walk(top: &str, visit: &mut dyn FnMut(&str, &[String]) -> bool) {
    let Ok(rd) = std::fs::read_dir(top) else { return };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let path = join(top, &[&name]);
        let is_dir = std::fs::metadata(&path).is_ok_and(|m| m.is_dir());
        if is_dir {
            // os.walk does not descend into symbolic links (followlinks=False)
            let link = e.file_type().is_ok_and(|t| t.is_symlink());
            dirs.push((name, link));
        } else {
            files.push(name);
        }
    }
    dirs.sort();
    if !visit(top, &files) {
        return;
    }
    for (d, link) in dirs {
        if !link {
            walk(&join(top, &[&d]), visit);
        }
    }
}

/// `meshview._bundles(root)`: (level-relative folder, bundle path) of the stock bundles, Package before Dist.
#[must_use]
pub fn bundles(root: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for w in ["Package", "Dist\\win32"] {
        let top = join(root, &["game", w]);
        let n = top.chars().count();
        walk(&top, &mut |dp, files| {
            let rel = if dp == top {
                ".".to_owned()
            } else {
                dp.chars()
                    .skip(n + 1)
                    .collect::<String>()
                    .replace('\\', "/")
                    .to_lowercase()
            };
            if SKIP.as_ref().is_some_and(|r| r.is_match(&rel)) {
                return true;
            }
            if rel.starts_with("levels/")
                && files
                    .iter()
                    .any(|f| INSTALLED.as_ref().is_some_and(|r| r.is_match(&f.to_lowercase())))
            {
                return false;
            }
            let mut fs: Vec<&String> = files.iter().collect();
            fs.sort();
            for f in fs {
                if f.to_lowercase().ends_with(".fbrb") {
                    out.push((rel.clone(), join(dp, &[f])));
                }
            }
            true
        });
    }
    out
}

/// `os.stat(path)`'s `st_mtime_ns` as Python gives it on Windows (the file time since 1970 in nanoseconds; C's
/// truncation toward zero before 1970).
#[must_use]
pub fn mtime_ns(m: &std::fs::Metadata) -> i128 {
    let t = m.modified().unwrap_or(std::time::UNIX_EPOCH);
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i128::from(d.as_secs()) * 1_000_000_000 + i128::from(d.subsec_nanos()),
        Err(e) => {
            let d = e.duration();
            -(i128::from(d.as_secs()) * 1_000_000_000 + i128::from(d.subsec_nanos()))
        }
    }
}

/// The bundles' signature (`Index.__init__`'s `sig`); `None` when a bundle cannot be stat'ed.
#[must_use]
pub fn signature(bundles: &[String]) -> Option<Vec<Sig>> {
    bundles
        .iter()
        .map(|b| {
            let m = std::fs::metadata(b).ok()?;
            Some((b.clone(), m.len(), mtime_ns(&m)))
        })
        .collect()
}

/// The index's cache file: `<root>\work\tools_out\meshview_index.pkl`.
#[must_use]
pub fn cache_path(root: &str) -> String {
    join(root, &["work", "tools_out", "meshview_index.pkl"])
}

/// Whether a cache's `sig` is this game copy's.
#[must_use]
pub fn same_sig(v: Option<Value>, sig: &[Sig]) -> bool {
    let Some(Value::List(l)) = v else { return false };
    let l = l.borrow();
    l.len() == sig.len()
        && l.iter().zip(sig).all(|(entry, (bundle, size, ns))| match entry {
            Value::Tuple(t) => match t.as_slice() {
                [Value::Str(path), got_size, got_ns] => {
                    &**path == bundle.as_str()
                        && got_size.as_int().is_some_and(|s| i128::from(s) == i128::from(*size))
                        && got_ns.as_int().is_some_and(|m| i128::from(m) == *ns)
                }
                _ => false,
            },
            _ => false,
        })
}

pub use super::Tables;

/// The cache's tables when its version and signature are this game copy's; `None` when missing, old or damaged.
#[must_use]
pub fn read(path: &str, sig: &[Sig]) -> Option<Tables> {
    let data = std::fs::read(path).ok()?;
    let cache = pickle::loads(&data).ok()?;
    if cache.get("version").and_then(|v| v.as_int()) != Some(CACHE_VERSION) || !same_sig(cache.get("sig"), sig) {
        return None;
    }
    let d = cache.get("data")?;
    let mut out = Tables::default();
    let Value::Dict(w) = d.get("where")? else { return None };
    for (k, v) in w.borrow().iter() {
        let (Value::Str(k), Value::Tuple(pair)) = (k, v) else {
            return None;
        };
        let [Value::Str(b), Value::Str(n)] = pair.as_slice() else {
            return None;
        };
        out.where_.push((k.to_string(), b.to_string(), n.to_string()));
    }
    let Value::Dict(p) = d.get("parts")? else { return None };
    for (k, v) in p.borrow().iter() {
        let (Value::Str(k), Value::Bytes(b)) = (k, v) else {
            return None;
        };
        out.parts.push((k.to_string(), b.to_vec()));
    }
    let Value::Dict(e) = d.get("entries")? else { return None };
    for (k, v) in e.borrow().iter() {
        let (Value::Str(k), Value::List(l)) = (k, v) else {
            return None;
        };
        let names = l
            .borrow()
            .iter()
            .map(|s| s.as_str().map(str::to_owned))
            .collect::<Option<Vec<_>>>()?;
        out.entries.push((k.to_string(), names));
    }
    Some(out)
}

/// `Index._write`'s pickle: `{'version': 3, 'sig': sig, 'data': {'where', 'parts', 'entries'}}` with the objects the
/// reference shares shared (each bundle path: its sig tuple's and its entries'; a layout record: every part using it;
/// the empty bytes object).
#[must_use]
pub fn cache_bytes(ix: &meshindex::MeshIndex, sig: &[Sig]) -> Option<Vec<u8>> {
    let s = |x: &str| Value::Str(Rc::from(x));
    let dict = |items: Vec<(Value, Value)>| Value::Dict(Rc::new(RefCell::new(items)));
    let list = |items: Vec<Value>| Value::List(Rc::new(RefCell::new(items)));
    let tuple = |items: Vec<Value>| Value::Tuple(Rc::new(items));
    let paths: Vec<Rc<str>> = sig.iter().map(|(b, _, _)| Rc::from(b.as_str())).collect();
    let sig_v = list(
        sig.iter()
            .zip(&paths)
            .map(|((_, size, ns), p)| {
                tuple(vec![
                    Value::Str(Rc::clone(p)),
                    pickle_write::int(i128::from(*size)),
                    pickle_write::int(*ns),
                ])
            })
            .collect(),
    );
    let mut where_ = Vec::with_capacity(ix.where_.len());
    for (key, bi, name) in &ix.where_ {
        let p = paths.get(*bi)?;
        where_.push((s(key), tuple(vec![Value::Str(Rc::clone(p)), s(name)])));
    }
    let empty: Rc<[u8]> = Rc::from(&b""[..]);
    let layouts: Vec<Rc<[u8]>> = ix
        .layouts
        .iter()
        .map(|l| {
            if l.is_empty() {
                Rc::clone(&empty)
            } else {
                Rc::from(l.as_slice())
            }
        })
        .collect();
    let mut parts = Vec::with_capacity(ix.parts.len());
    for (key, id) in &ix.parts {
        parts.push((s(key), Value::Bytes(Rc::clone(layouts.get(*id)?))));
    }
    let entries = ix
        .entries
        .iter()
        .map(|(k, tex)| (s(k), list(tex.iter().map(|t| s(t)).collect())))
        .collect();
    let top = dict(vec![
        (s("version"), Value::Int(CACHE_VERSION)),
        (s("sig"), sig_v),
        (
            s("data"),
            dict(vec![
                (s("where"), dict(where_)),
                (s("parts"), dict(parts)),
                (s("entries"), dict(entries)),
            ]),
        ),
    ]);
    pickle_write::dumps(&top).ok()
}

/// A built index's tables (as [`read`] gives them from its cache).
#[must_use]
pub fn tables(ix: &meshindex::MeshIndex, bundles: &[String]) -> Tables {
    Tables {
        where_: ix
            .where_
            .iter()
            .map(|(k, bi, n)| (k.clone(), bundles.get(*bi).cloned().unwrap_or_default(), n.clone()))
            .collect(),
        parts: ix
            .parts
            .iter()
            .map(|(k, id)| (k.clone(), ix.layouts.get(*id).cloned().unwrap_or_default()))
            .collect(),
        entries: ix.entries.clone(),
    }
}

/// `atomicio.write_bytes(path, data)`: [`bc2_formats::atomic::write`] with the folder made first.
fn write_atomic(path: &str, data: &[u8]) -> std::io::Result<()> {
    let path = std::path::Path::new(path);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Ok(bc2_formats::atomic::write(path, &[data])?)
}

/// The index of the game copy at `root` (its signature's bundles): from the cache when current, else built in Rust
/// (`meshindex::build`, minutes the first time) and written to the cache under its lock. `say` hears
/// "indexing the game copy" before a build.
///
/// # Errors
/// A bundle cannot be stat'ed, or the build meets a bundle or database the reference would leave out (the Python's
/// own build then reports it: `bfbc2.exe meshview find x`).
pub fn load(root: &str, say: &mut dyn FnMut(&str)) -> Result<Tables, String> {
    let bundles: Vec<String> = bundles(root).into_iter().map(|(_, b)| b).collect();
    let sig = signature(&bundles).ok_or("a bundle of the game copy could not be read (stat)")?;
    let path = cache_path(root);
    if let Some(t) = read(&path, &sig) {
        return Ok(t);
    }
    bc2_formats::atomic::locked(std::path::Path::new(&format!("{path}.lock")), || {
        if let Some(t) = read(&path, &sig) {
            return Ok(t);
        }
        say("indexing the game copy (once; cached) ...");
        let built = meshindex::build(&bundles).map_err(|e| {
            format!("the mesh index could not be built here ({e}); `bfbc2.exe meshview find x` builds it and says why")
        })?;
        if let Some(data) = cache_bytes(&built, &sig) {
            let _ = write_atomic(&path, &data);
        }
        Ok(tables(&built, &bundles))
    })
    .map_err(|e| format!("{path}.lock: {e}"))?
}
