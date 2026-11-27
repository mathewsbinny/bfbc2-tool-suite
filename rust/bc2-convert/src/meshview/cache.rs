//! `meshview.Index`'s game copy and cache (`bfbc2-mesh-viewer\meshview.py`; rounds O3, GR3 and PR1b-2-rest): the
//! stock bundles (`meshview._bundles`, Package before Dist, OPFOR's own levels left out) and the index from its cache
//! `work\tools_out\meshview_index.bin` ([`bc2_gamefs::meshindex::load`]), so the Python tools (through `bc2_native`),
//! `bfbc2.exe meshview` and the map editor's asset preview share one cache. Round PR1b-2-rest retired the Python's
//! pickle (`meshview_index.pkl`) and the Python-runtime emulation this module read and wrote it with.
//!
//! Moved here from `bc2-cli`'s `gameindex` in round GR3 so the editor reads the same index.

use std::sync::LazyLock;

use bc2_gamefs::meshindex;
use regex::Regex;

/// `os.path.join(a, *names)` for plain names (Windows: a separator unless `a` is empty, ends in one or is a drive).
fn join(a: &str, names: &[&str]) -> String {
    let mut out = a.to_owned();
    for n in names {
        let bare = out.is_empty() || out.ends_with(['\\', '/']) || (out.len() == 2 && out.ends_with(':'));
        if !bare {
            out.push('\\');
        }
        out.push_str(n);
    }
    out
}

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
            dirs.push(name);
        } else {
            files.push(name);
        }
    }
    dirs.sort();
    if !visit(top, &files) {
        return;
    }
    // os.walk does not descend into symbolic links (followlinks=False) but does into junctions (os.path.islink is
    // false for them), which is how a round's checkout or a test's game copy shares folders; the walk follows every
    // folder link (a directory symbolic link needs administrator rights, and none is made here). Before round OR10
    // it followed none: a junctioned level's bundles were missing from the index
    for d in dirs {
        walk(&join(top, &[&d]), visit);
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

/// The index's cache file: `<root>\work\tools_out\meshview_index.bin` ([`meshindex::CACHE_NAME`]).
#[must_use]
pub fn cache_path(root: &str) -> String {
    join(root, &["work", "tools_out", meshindex::CACHE_NAME])
}

pub use super::Tables;

/// An index's tables, each bundle by its path.
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

/// The index of the game copy at `root`: from its cache when current, else built and cached
/// ([`meshindex::load`]: minutes the first time). `say` hears "indexing the game copy" before a build and what was
/// left out.
///
/// # Errors
/// A bundle cannot be stat'ed, or the cache's lock cannot be taken.
pub fn load(root: &str, say: &mut dyn FnMut(&str)) -> Result<Tables, meshindex::LoadError> {
    let bundles: Vec<String> = bundles(root).into_iter().map(|(_, b)| b).collect();
    let got = meshindex::load(&bundles, std::path::Path::new(&cache_path(root)), say)?;
    Ok(tables(&got.index, &bundles))
}
