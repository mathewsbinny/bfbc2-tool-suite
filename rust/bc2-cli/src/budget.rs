//! The caches' overall budget as a bundle read asks for it (round O3): `bfbcore.fbrb._budget` after `fbrb.read`
//! calls `caches.maybe_prune_total()`: at most once per `PRUNE_EVERY` seconds across processes (the cache root's
//! `.pruned-total` stamp), every cache's entries together - the suite's caches that are on, then Project OPFOR's
//! project caches - brought under `BFBC2_CACHE_TOTAL_GB` (default 20 GiB), the least recently used first, under the
//! root's `.prune-total.lock`. A port of `caches.prune_total` / `maybe_prune_total` / `members` / `_scan` for the
//! tools `bfbc2.exe` runs that read bundles (the caches group's command line has the rest of `bfbc2core.caches`).

use std::collections::HashSet;

use bc2_pyrt::py::{abspath, isdir, join};

use crate::fsx;
use crate::logs::Seen;
use crate::pylog;

const PRUNE_EVERY: f64 = 900.0;
const TOTAL_LOCK: &str = ".prune-total.lock";
const TOTAL_STAMP: &str = ".pruned-total";

/// A cache as `caches.Spec` names it: its folder's variable, its entries' suffixes, its project folder.
struct Spec {
    name: &'static str,
    env: &'static str,
    suffixes: &'static [&'static str],
    off_without_localappdata: bool,
    project: &'static [&'static str],
}

const NAVDATA: &[&str] = &[".navdata.pkl", ".npz"];

/// `CACHES` then `PROJECT_CACHES`, in their order.
const SPECS: [Spec; 11] = [
    Spec {
        name: "hlsl",
        env: "BFBC2_HLSL_CACHE",
        suffixes: &[".bin"],
        off_without_localappdata: true,
        project: &[],
    },
    Spec {
        name: "fbrb",
        env: "BFBC2_FBRB_CACHE",
        suffixes: &[".payload"],
        off_without_localappdata: false,
        project: &[],
    },
    Spec {
        name: "collision",
        env: "BFBC2_COLLISION_CACHE",
        suffixes: &[".bin"],
        off_without_localappdata: false,
        project: &[],
    },
    Spec {
        name: "effectport",
        env: "BFBC2_EFFECTPORT_CACHE",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
    },
    Spec {
        name: "culltree",
        env: "BFBC2_CULLTREE_CACHE",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
    },
    Spec {
        name: "bf1943source",
        env: "BF1943_SOURCE_INDEX",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
    },
    Spec {
        name: "shadercover",
        env: "BFBC2_SHADERCOVER_CACHE",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
    },
    Spec {
        name: "navstudio",
        env: "",
        suffixes: NAVDATA,
        off_without_localappdata: false,
        project: &["work", "navstudio", "cache"],
    },
    Spec {
        name: "harness",
        env: "",
        suffixes: NAVDATA,
        off_without_localappdata: false,
        project: &["work", "harness", "cache"],
    },
    Spec {
        name: "navwater",
        env: "",
        suffixes: NAVDATA,
        off_without_localappdata: false,
        project: &["work", "navwater", "cache"],
    },
    Spec {
        name: "editor",
        env: "",
        suffixes: &[".dbx", ".res", ".xml", ".json"],
        off_without_localappdata: false,
        project: &["work", "editor", "levels"],
    },
];

fn localappdata() -> Option<String> {
    std::env::var("LOCALAPPDATA").ok().filter(|v| !v.is_empty())
}

/// `caches.folder(name)`: the cache's folder as its owner resolves it; None when it is off (or, for a project
/// cache, when no project folder is found).
fn folder(s: &Spec) -> Option<String> {
    if !s.project.is_empty() {
        let root = bc2_menu::common::find_opfor(None).ok()?;
        return Some(join(&root, s.project));
    }
    let v = std::env::var(s.env).unwrap_or_default();
    if v == "0" || v.to_lowercase() == "off" {
        return None;
    }
    if !v.is_empty() {
        return Some(v);
    }
    let base = match localappdata() {
        Some(b) => b,
        None if s.off_without_localappdata => return None,
        None if s.name == "fbrb" => join(&pylog::home_dir(), &[".cache"]),
        None => pylog::home_dir(),
    };
    Some(join(&base, &["bfbc2-tool-suite", "cache", s.name]))
}

/// `caches.cache_root()`.
fn cache_root() -> String {
    join(
        &localappdata().unwrap_or_else(pylog::home_dir),
        &["bfbc2-tool-suite", "cache"],
    )
}

/// `caches.is_temp` (case-sensitive, as the reference).
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the reference compares case-sensitively"
)]
fn is_temp(name: &str) -> bool {
    name.ends_with(".tmp") || name.contains(".tmp-")
}

/// `caches._scan(top, suffixes).entries`: depth first as its stack pops, links and junctions never followed, names
/// starting with '.' and writers' temporary files left out.
fn scan(top: &str, suffixes: &[&str]) -> Vec<Seen> {
    let mut out = Vec::new();
    let mut stack = vec![top.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fsx::scandir(&d) else { continue };
        for e in entries {
            if e.link {
                continue;
            }
            if e.dir {
                stack.push(e.path);
                continue;
            }
            if e.name.starts_with('.') || !e.file {
                continue;
            }
            let Some(st) = e.stat else { continue };
            if !is_temp(&e.name) && suffixes.iter().any(|s| e.name.ends_with(s)) {
                out.push(Seen {
                    path: e.path,
                    size: st.size,
                    mtime_ns: st.mtime_ns,
                    used: st.atime.max(st.mtime),
                });
            }
        }
    }
    out
}

/// `caches.members()`: (spec, folder) of every cache the budget counts, a folder named twice counted once.
fn members() -> Vec<(&'static Spec, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for s in &SPECS {
        let Some(top) = folder(s) else { continue };
        if !isdir(&top) {
            continue;
        }
        if seen.insert(fsx::normcase(&abspath(&top))) {
            out.push((s, top));
        }
    }
    out
}

/// `caches.prune_total()`: every cache's entries together brought under the overall budget, under the root's lock.
fn prune_total() -> std::io::Result<()> {
    let cap = (pylog::number("BFBC2_CACHE_TOTAL_GB", 20.0) * 1_073_741_824.0) as u64;
    let root = cache_root();
    std::fs::create_dir_all(&root)?;
    fsx::locked(&join(&root, &[TOTAL_LOCK]), || {
        let mut files: Vec<Seen> = Vec::new();
        for (s, top) in members() {
            files.extend(scan(&top, s.suffixes));
        }
        let mut total: u64 = files.iter().map(|f| f.size).sum();
        files.sort_by(|a, b| a.used.total_cmp(&b.used).then_with(|| a.path.cmp(&b.path)));
        for f in &files {
            if total <= cap {
                break;
            }
            if f.delete_if_unchanged(true) == Some(false) {
                continue;
            }
            total -= f.size;
        }
        let _ = fsx::touch_marker(&join(&root, &[TOTAL_STAMP]));
    })
}

/// `fbrb._budget()` (once per process) / `caches.maybe_prune_total()`: [`prune_total`] unless another process ran it
/// in the last 15 minutes (the root's stamp). Never fails.
pub fn after_bundle_read() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let stamp = join(&cache_root(), &[TOTAL_STAMP]);
        if fsx::stat(&stamp).is_ok_and(|st| fsx::now() - st.mtime < PRUNE_EVERY) {
            return;
        }
        let _ = prune_total();
    });
}
