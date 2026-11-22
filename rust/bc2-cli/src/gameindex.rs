//! `meshview.Index` as the command lines use it (round O3): `shaderdb.find_opfor`, the stock bundles
//! (`meshview._bundles`) and their signature, and the index taken from its cache (`work\tools_out\meshview_index.pkl`,
//! read with `bc2_pyrt::pickle`) when it is this game copy's. A missing, old or damaged cache is built by the Python
//! (minutes, once; it writes the cache): the command line is handed to it ([`Fail::Python`]).

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

use bc2_convert::meshview::cache;
use bc2_formats::{atomic, meshindex};
use bc2_pyrt::pickle::{self, Value};
use bc2_pyrt::py::{abspath, dirname, isdir, join, suite_root};

use crate::pyos::files as fsx;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// `shaderdb.find_opfor(given)`: given, `OPFOR_ROOT`, then `project-opfor-bfbc2` beside the suite (only there); the
/// first with a `game` folder, absolute. Err: `shaderdb.ShaderDbError`.
pub fn find_opfor(given: Option<&str>) -> PyResult<String> {
    let mut cands: Vec<String> = Vec::new();
    cands.extend(given.filter(|g| !g.is_empty()).map(str::to_owned));
    if let Ok(v) = std::env::var("OPFOR_ROOT")
        && !v.is_empty()
    {
        cands.push(v);
    }
    let suite = abspath(&suite_root().to_string_lossy());
    cands.push(join(&dirname(&suite), &["project-opfor-bfbc2"]));
    for c in cands {
        if isdir(&join(&c, &["game"])) {
            return Ok(abspath(&c));
        }
    }
    Err(Fail::Err(PyErr::new(
        "shaderdb.ShaderDbError",
        "Project OPFOR BFBC2 not found (no game copy): put the suite next to the project-opfor-bfbc2 folder, set \
         OPFOR_ROOT, or give --opfor",
    )))
}

/// `meshview._bundles(root)`: (level-relative folder, bundle path) of the stock bundles, Package before Dist
/// (`bc2_convert::meshview::cache`, shared with the map editor's asset preview since round GR3).
pub fn bundles(root: &str) -> Vec<(String, String)> {
    cache::bundles(root)
}

/// The index's `where`: entry name (lower case) -> (bundle, entry name), in the cache's order (the strings shared
/// with the pickle read, never copied: the cache holds some 200,000 of them).
pub struct Index {
    rows: Vec<(Rc<str>, Rc<str>, Rc<str>)>,
    #[cfg_attr(not(feature = "meshview"), allow(dead_code, reason = "meshview's lookups"))]
    at: HashMap<Rc<str>, usize>,
}

impl Index {
    fn new(rows: Vec<(Rc<str>, Rc<str>, Rc<str>)>) -> Self {
        let at = rows.iter().enumerate().map(|(i, r)| (Rc::clone(&r.0), i)).collect();
        Self { rows, at }
    }

    /// The entry names (lower case), in the dict's order.
    #[cfg_attr(not(feature = "meshview"), allow(dead_code, reason = "meshview's"))]
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().map(|r| &*r.0)
    }

    /// `where.get(key)`: (bundle, entry name).
    #[cfg_attr(not(feature = "meshview"), allow(dead_code, reason = "meshview's"))]
    pub fn get(&self, key: &str) -> Option<(&str, &str)> {
        self.at.get(key).and_then(|&i| self.rows.get(i)).map(|r| (&*r.1, &*r.2))
    }

    /// The bundles of `where.values()`.
    #[cfg_attr(not(feature = "bf1943"), allow(dead_code, reason = "dbxcatalogue's"))]
    pub fn bundles(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().map(|r| &*r.1)
    }
}

/// `Index._cached(path, sig)`: the index from the cache when it is this game copy's; None when missing, old or
/// damaged (the reference builds it then).
fn cached(path: &str, sig: &[(String, u64, i128)]) -> Option<Index> {
    let data = std::fs::read(path).ok()?;
    let cache = pickle::loads(&data).ok()?;
    if cache.get("version").and_then(|v| v.as_int()) != Some(cache::CACHE_VERSION)
        || !cache::same_sig(cache.get("sig"), sig)
    {
        return None;
    }
    let table = cache.get("data").and_then(|d| d.get("where"))?;
    let Value::Dict(table) = table else { return None };
    let mut rows = Vec::with_capacity(table.borrow().len());
    for (key, value) in table.borrow().iter() {
        let (Value::Str(key), Value::Tuple(pair)) = (key, value) else {
            return None;
        };
        let [Value::Str(bundle), Value::Str(name)] = pair.as_slice() else {
            return None;
        };
        rows.push((Rc::clone(key), Rc::clone(bundle), Rc::clone(name)));
    }
    let ix = Index::new(rows);
    // the rest of the tree (the parts' and entries' tables) is let go with the process, not freed object by object
    std::mem::forget(cache);
    Some(ix)
}

/// `meshview.Index(root)`: from its cache when it is this game copy's, else built here (`bc2_formats::meshindex`,
/// under the cache's lock, "indexing the game copy (once; cached) ..." on stderr) and written to the cache as the
/// reference writes it (its pickle, byte for byte). [`Fail::Python`]: a bundle's `os.stat` fails, or the build meets
/// what the reference would leave out or fail on (an unreadable bundle or database), or the cache cannot be written -
/// the reference's own messages then.
pub fn load(py: &mut Py, root: &str) -> PyResult<Index> {
    let bundles: Vec<String> = bundles(root).into_iter().map(|(_, b)| b).collect();
    let mut sig: Vec<(String, u64, i128)> = Vec::new();
    for b in &bundles {
        let st = fsx::stat(b).map_err(|_| Fail::Python)?;
        sig.push((b.clone(), st.size, st.mtime_ns));
    }
    let path = cache::cache_path(root);
    if let Some(ix) = cached(&path, &sig) {
        return Ok(ix);
    }
    atomic::locked(Path::new(&format!("{path}.lock")), || -> PyResult<Index> {
        if let Some(ix) = cached(&path, &sig) {
            return Ok(ix);
        }
        py.eprint("indexing the game copy (once; cached) ...");
        let built = meshindex::build(&bundles).map_err(|_| Fail::Python)?;
        if built.databases > 0 {
            crate::budget::after_bundle_read();
        }
        let data = cache::cache_bytes(&built, &sig).ok_or(Fail::Python)?;
        std::fs::create_dir_all(dirname(&path)).map_err(|_| Fail::Python)?;
        atomic::write(Path::new(&path), &[&data]).map_err(|_| Fail::Python)?;
        let paths: Vec<Rc<str>> = bundles.iter().map(|b| Rc::from(b.as_str())).collect();
        let mut rows = Vec::with_capacity(built.where_.len());
        for (key, bi, name) in &built.where_ {
            let b = paths.get(*bi).cloned().unwrap_or_else(|| Rc::from(""));
            rows.push((Rc::from(key.as_str()), b, Rc::from(name.as_str())));
        }
        Ok(Index::new(rows))
    })
    .map_err(|_| Fail::Python)?
}
