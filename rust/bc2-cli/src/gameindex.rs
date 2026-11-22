//! `meshview.Index` as the command lines use it (round O3): `shaderdb.find_opfor`, the stock bundles
//! (`meshview._bundles`), and the index from its cache (`work\tools_out\meshview_index.bin`), or built here and
//! cached when the game copy changed ([`bc2_gamefs::meshindex::load`], round PR1b-2-rest: the Python's pickle and
//! its build retired, so nothing is handed to the Python any more).

use std::collections::HashMap;
use std::path::Path;

use bc2_convert::meshview::cache;
use bc2_gamefs::meshindex;

use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// `shaderdb.find_opfor(given)`: given, `OPFOR_ROOT`, then `project-opfor-bfbc2` beside the suite (only there); the
/// first with a `game` folder, absolute ([`bc2_gamefs::game::find_opfor`]). Err: `shaderdb.ShaderDbError`.
pub fn find_opfor(given: Option<&str>) -> PyResult<String> {
    let suite = bc2_pyrt::py::suite_root();
    bc2_gamefs::game::find_opfor(given, true, bc2_gamefs::game::Beside::Only(&suite))
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|_| {
            Fail::Err(PyErr::new(
                "shaderdb.ShaderDbError",
                "Project OPFOR BFBC2 not found (no game copy): put the suite next to the project-opfor-bfbc2 folder, \
                 set OPFOR_ROOT, or give --opfor",
            ))
        })
}

/// `meshview._bundles(root)`: (level-relative folder, bundle path) of the stock bundles, Package before Dist
/// (`bc2_convert::meshview::cache`, shared with the map editor's asset preview since round GR3).
pub fn bundles(root: &str) -> Vec<(String, String)> {
    cache::bundles(root)
}

/// The index's `where`: entry name (lower case) -> (bundle, entry name), in the index's order.
pub struct Index {
    bundles: Vec<String>,
    rows: Vec<(String, usize, String)>,
    #[cfg_attr(not(feature = "meshview"), allow(dead_code, reason = "meshview's lookups"))]
    at: HashMap<String, usize>,
}

impl Index {
    fn new(bundles: Vec<String>, ix: meshindex::MeshIndex) -> Self {
        let rows = ix.where_;
        let at = rows.iter().enumerate().map(|(i, r)| (r.0.clone(), i)).collect();
        Self { bundles, rows, at }
    }

    /// The entry names (lower case), in the dict's order.
    #[cfg_attr(not(feature = "meshview"), allow(dead_code, reason = "meshview's"))]
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.rows.iter().map(|r| r.0.as_str())
    }

    /// `where.get(key)`: (bundle, entry name).
    #[cfg_attr(not(feature = "meshview"), allow(dead_code, reason = "meshview's"))]
    pub fn get(&self, key: &str) -> Option<(&str, &str)> {
        let r = self.rows.get(*self.at.get(key)?)?;
        Some((self.bundles.get(r.1)?.as_str(), r.2.as_str()))
    }

    /// The bundles of `where.values()`.
    #[cfg_attr(not(feature = "bf1943"), allow(dead_code, reason = "dbxcatalogue's"))]
    pub fn bundles(&self) -> impl Iterator<Item = &str> {
        self.rows
            .iter()
            .filter_map(|r| self.bundles.get(r.1).map(String::as_str))
    }
}

/// `meshview.Index(root)`: from its cache when it is this game copy's, else built here (under the cache's lock,
/// "indexing the game copy (once; cached) ..." on stderr, what was left out said and the index then not cached).
/// Err: a bundle cannot be stat'ed or the lock cannot be taken (`OSError`).
pub fn load(py: &mut Py, root: &str) -> PyResult<Index> {
    let bundles: Vec<String> = bundles(root).into_iter().map(|(_, b)| b).collect();
    let path = cache::cache_path(root);
    let got = meshindex::load(&bundles, Path::new(&path), &mut |s| py.eprint(s))
        .map_err(|e| Fail::Err(PyErr::new("OSError", e.to_string())))?;
    if got.built && got.index.databases > 0 {
        crate::budget::after_bundle_read();
    }
    Ok(Index::new(bundles, got.index))
}
