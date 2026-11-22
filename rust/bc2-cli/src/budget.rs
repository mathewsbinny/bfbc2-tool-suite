//! The caches' overall budget as `bfbc2core.fbrb._budget` runs it after a bundle read (round D1's
//! `caches.maybe_prune_total`, round O3): every suite cache and project cache together brought under
//! `BFBC2_CACHE_TOTAL_GB` (default 20 GiB), the least recently used entries of any cache deleted first, at most once
//! per 15 minutes across processes (the cache root's `.pruned-total` stamp), under the root's `.prune-total.lock`.
//! A Python tool that reads a bundle runs it once (its first read); `bfbc2.exe`'s ports call [`after_bundle_read`]
//! where the Python's `fbrb.read` / `read_many` would. Never fails (a cache that cannot be pruned is only bigger).
//!
//! The cache table is `caches.CACHES` / `PROJECT_CACHES` (the caches group's `--report` / `--prune` use the same
//! rules).

use std::collections::HashSet;

use crate::pylog;
use crate::pyos::files as fsx;

/// `caches.PRUNE_EVERY`
pub const PRUNE_EVERY: f64 = 900.0;
pub const TOTAL_ENV: &str = "BFBC2_CACHE_TOTAL_GB";
pub const TOTAL_LOCK: &str = ".prune-total.lock";
pub const TOTAL_STAMP: &str = ".pruned-total";

/// One cache of `caches.CACHES` / `PROJECT_CACHES`: name, folder variable (suite caches), entry suffixes, off without
/// `LOCALAPPDATA`, its folder under Project OPFOR BFBC2's (project caches).
pub struct Spec {
    pub name: &'static str,
    pub env: &'static str,
    pub suffixes: &'static [&'static str],
    pub off_without_localappdata: bool,
    pub project: &'static [&'static str],
    /// the cap: `cap_env` (a number of `cap_unit` bytes, fractions allowed), else `default_cap`
    pub cap_env: &'static str,
    pub cap_unit: f64,
    pub default_cap: f64,
    /// small bookkeeping files (deleted by `--clear`, by the pruning once older than `MARKER_AGE`)
    pub markers: &'static [&'static str],
}

/// `caches.MB`, the GiB of fbrb's cap
pub const MB: f64 = 1_048_576.0;
const GB: f64 = 1_073_741_824.0;
/// `Spec.marker_age`'s default (every cache's)
pub const MARKER_AGE: f64 = 30.0 * 86_400.0;

/// navdata.load's entries
const NAVDATA: &[&str] = &[".navdata.pkl", ".npz"];
/// `caches.CACHES`, then `PROJECT_CACHES` (the project caches: `project` set).
pub const SPECS: &[Spec] = &[
    Spec {
        name: "hlsl",
        env: "BFBC2_HLSL_CACHE",
        suffixes: &[".bin"],
        off_without_localappdata: true,
        project: &[],
        cap_env: "BFBC2_HLSL_CACHE_MB",
        cap_unit: MB,
        default_cap: 1024.0,
        markers: &[],
    },
    Spec {
        name: "fbrb",
        env: "BFBC2_FBRB_CACHE",
        suffixes: &[".payload"],
        off_without_localappdata: false,
        project: &[],
        cap_env: "BFBC2_FBRB_CACHE_GB",
        cap_unit: GB,
        default_cap: 16.0,
        markers: &[".seen"],
    },
    Spec {
        name: "collision",
        env: "BFBC2_COLLISION_CACHE",
        suffixes: &[".bin"],
        off_without_localappdata: false,
        project: &[],
        cap_env: "BFBC2_COLLISION_CACHE_MB",
        cap_unit: MB,
        default_cap: 256.0,
        markers: &[],
    },
    Spec {
        name: "effectport",
        env: "BFBC2_EFFECTPORT_CACHE",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
        cap_env: "BFBC2_EFFECTPORT_CACHE_MB",
        cap_unit: MB,
        default_cap: 256.0,
        markers: &[],
    },
    Spec {
        name: "culltree",
        env: "BFBC2_CULLTREE_CACHE",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
        cap_env: "BFBC2_CULLTREE_CACHE_MB",
        cap_unit: MB,
        default_cap: 2048.0,
        markers: &[],
    },
    Spec {
        name: "bf1943source",
        env: "BF1943_SOURCE_INDEX",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
        cap_env: "BF1943_SOURCE_INDEX_MB",
        cap_unit: MB,
        default_cap: 64.0,
        markers: &[],
    },
    Spec {
        name: "shadercover",
        env: "BFBC2_SHADERCOVER_CACHE",
        suffixes: &[".pkl"],
        off_without_localappdata: false,
        project: &[],
        cap_env: "BFBC2_SHADERCOVER_CACHE_MB",
        cap_unit: MB,
        default_cap: 64.0,
        markers: &[],
    },
    Spec {
        name: "navstudio",
        env: "",
        suffixes: NAVDATA,
        off_without_localappdata: false,
        project: &["work", "navstudio", "cache"],
        cap_env: "BFBC2_NAVSTUDIO_CACHE_MB",
        cap_unit: MB,
        default_cap: 2048.0,
        markers: &[],
    },
    Spec {
        name: "harness",
        env: "",
        suffixes: NAVDATA,
        off_without_localappdata: false,
        project: &["work", "harness", "cache"],
        cap_env: "BFBC2_HARNESS_CACHE_MB",
        cap_unit: MB,
        default_cap: 3072.0,
        markers: &[],
    },
    Spec {
        name: "navwater",
        env: "",
        suffixes: NAVDATA,
        off_without_localappdata: false,
        project: &["work", "navwater", "cache"],
        cap_env: "BFBC2_NAVWATER_CACHE_MB",
        cap_unit: MB,
        default_cap: 1024.0,
        markers: &[],
    },
    Spec {
        name: "editor",
        env: "",
        suffixes: &[".dbx", ".res", ".xml", ".json"],
        off_without_localappdata: false,
        project: &["work", "editor", "levels"],
        cap_env: "BFBC2_EDITOR_CACHE_MB",
        cap_unit: MB,
        default_cap: 2048.0,
        markers: &[],
    },
    Spec {
        name: "meshview",
        env: "",
        suffixes: &["meshview_index.pkl"],
        off_without_localappdata: false,
        project: &["work", "tools_out"],
        cap_env: "BFBC2_MESHVIEW_CACHE_MB",
        cap_unit: MB,
        default_cap: 256.0,
        markers: &[],
    },
];

fn local_app_data() -> Option<String> {
    std::env::var("LOCALAPPDATA").ok().filter(|v| !v.is_empty())
}

fn join(a: &str, names: &[&str]) -> String {
    bc2_pyrt::py::join(a, names)
}

/// `caches.folder(name)`: the cache's folder as its owner resolves it; None when off (or no project folder).
pub fn folder(s: &Spec, opfor: Option<&str>) -> Option<String> {
    if !s.project.is_empty() {
        return opfor.map(|root| join(root, s.project));
    }
    let v = std::env::var(s.env).unwrap_or_default();
    if v == "0" || v.to_lowercase() == "off" {
        return None;
    }
    if !v.is_empty() {
        return Some(v);
    }
    let base = match local_app_data() {
        Some(b) => b,
        None if s.off_without_localappdata => return None,
        None if s.name == "fbrb" => join(&pylog::home_dir(), &[".cache"]),
        None => pylog::home_dir(),
    };
    Some(join(&base, &["bfbc2-tool-suite", "cache", s.name]))
}

/// `caches.cache_root()`.
pub fn cache_root() -> String {
    join(
        &local_app_data().unwrap_or_else(pylog::home_dir),
        &["bfbc2-tool-suite", "cache"],
    )
}

/// `caches.is_temp(name)`.
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the reference compares as it is"
)]
pub fn is_temp(name: &str) -> bool {
    name.ends_with(".tmp") || name.contains(".tmp-")
}

/// An entry of a cache as `caches._scan` sees it: (path, size, `mtime_ns`, used = the later of access and change).
struct Seen {
    path: String,
    size: u64,
    mtime_ns: i128,
    used: f64,
}

/// `caches._scan(top, suffixes).entries`.
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

/// `caches.prune_total()`: every cache's entries together under the budget.
fn prune_total() {
    let cap = (pylog::number(TOTAL_ENV, 20.0) * f64::from(1u32 << 30)) as u64;
    let root = cache_root();
    if std::fs::create_dir_all(&root).is_err() {
        return;
    }
    let opfor = bc2_menu::common::find_opfor(None).ok();
    let _ = fsx::locked(&join(&root, &[TOTAL_LOCK]), || {
        let mut files: Vec<Seen> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        for s in SPECS {
            let Some(top) = folder(s, opfor.as_deref()) else {
                continue;
            };
            if !bc2_pyrt::py::isdir(&top) {
                continue;
            }
            if seen.insert(fsx::normcase(&bc2_pyrt::py::abspath(&top))) {
                files.extend(scan(&top, s.suffixes));
            }
        }
        let mut total: u64 = files.iter().map(|f| f.size).sum();
        files.sort_by(|a, b| a.used.total_cmp(&b.used).then_with(|| a.path.cmp(&b.path)));
        for f in &files {
            if total <= cap {
                break;
            }
            match fsx::delete_if_unchanged(&f.path, f.size, f.mtime_ns, true) {
                Some(false) => {}
                _ => total -= f.size,
            }
        }
        let _ = fsx::touch_marker(&join(&root, &[TOTAL_STAMP]));
    });
}

/// `fbrb._budget()` after a bundle read in a ported tool's process: `caches.maybe_prune_total()` unless the root's
/// stamp says it ran in the last 15 minutes. The Python asks once per process; so does this (a second call does
/// nothing).
pub fn after_bundle_read() {
    static DONE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if DONE.swap(true, std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let stamp = join(&cache_root(), &[TOTAL_STAMP]);
    if let Ok(st) = fsx::stat(&stamp)
        && fsx::now() - st.mtime < PRUNE_EVERY
    {
        return;
    }
    prune_total();
}
