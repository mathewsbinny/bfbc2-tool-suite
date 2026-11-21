//! Round O3's `caches` group: `bfbc2 caches ...`, the command line of `python -m bfbc2core.caches`
//! (`core\bfbc2core\caches.py`): each cache's folder, size and cap (`--list`, the default), `--prune` / `--clear` of
//! the caches, the project caches and the logs (the pruned logs archived first: `logs.rs`), the overall budget, the
//! D1 `--report` (caches, budget, logs, every checkout's Rust build folder, Project OPFOR's `work\` by kind) and
//! `--prune-targets [--yes]`. The cache table and the overall budget's rules are `budget.rs`'s (one copy, shared with
//! the tools that read bundles).
//!
//! `argparse` is ported; its texts at the console's width or `COLUMNS` are [`crate::argfmt`]'s (round RT2: before it
//! only the pipe's 80 columns). Handed to the Python: option abbreviations, `--`, single-dash options other than `-h`, a flag given a value
//! (`--yes=1`); and a folder lock or log archive that cannot be written (nothing is deleted then).

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::indexing_slicing,
    reason = "Python's floats of byte counts and its int() of them; slices within the lengths just found"
)]

use std::collections::HashSet;
use std::fmt::Write as _;

use crate::budget::{self, MARKER_AGE, MB, SPECS, Spec};
use crate::logs::{self, Done, Seen};
use crate::pylog;
use crate::pyos::files::{self as fsx, Entry};
use crate::pyproc::{Fail, Py, PyErr, PyResult, Tool, suite_root};
use bc2_pyrt::py::{abspath, basename, dirname, isdir, isfile, join};

/// The group's tools.
pub const TOOLS: &[Tool] = &[Tool {
    name: "caches",
    script: "core\\bfbc2core\\caches.py",
    log: None,
    run,
}];

const LOCK: &str = ".prune.lock";
const STAMP: &str = ".pruned";
/// `STALE_TMP`: a temporary file not written to for this long is a dead writer's
const STALE_TMP: f64 = 3600.0;
const LOGS: &str = "logs";
const TARGET_RECENT: f64 = 86_400.0;
const CARGO_MARKS: [&str; 2] = ["CACHEDIR.TAG", ".rustc_info.json"];
const KINDS: [&str; 6] = ["input", "output", "cache", "scratch", "backup", "unknown"];

// ------------------------------------------------------------------------------------------------ the store

fn spec(name: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.name == name)
}

/// `cap(name)`: `int(_number(cap_env, default) * unit)`.
fn cap(s: &Spec) -> u64 {
    (pylog::number(s.cap_env, s.default_cap) * s.cap_unit) as u64
}

/// `total_cap()`.
fn total_cap() -> u64 {
    (pylog::number(budget::TOTAL_ENV, 20.0) * 1_073_741_824.0) as u64
}

/// `project_root(given)`: Project OPFOR BFBC2's folder as the tools find it, or None.
fn project_root(given: Option<&str>) -> Option<String> {
    bc2_menu::common::find_opfor(given).ok()
}

/// `folder(name, opfor)`.
fn folder(s: &Spec, opfor: Option<&str>) -> Option<String> {
    if s.project.is_empty() {
        budget::folder(s, None)
    } else {
        budget::folder(s, project_root(opfor).as_deref())
    }
}

/// `caches._Scan`.
#[derive(Default)]
struct Scan {
    entries: Vec<Seen>,
    temps: Vec<Seen>,
    markers: Vec<Seen>,
}

fn seen(e: Entry, st: fsx::Stat) -> Seen {
    Seen {
        path: e.path,
        size: st.size,
        mtime_ns: st.mtime_ns,
        used: st.atime.max(st.mtime),
    }
}

/// `caches._scan(top, suffixes, markers)`.
fn scan(top: &str, suffixes: &[&str], markers: &[&str]) -> Scan {
    let mut out = Scan::default();
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
            let name = e.name.clone();
            if budget::is_temp(&name) {
                out.temps.push(seen(e, st));
            } else if suffixes.iter().any(|s| name.ends_with(s)) {
                out.entries.push(seen(e, st));
            } else if !markers.is_empty() && markers.iter().any(|m| name.ends_with(m)) {
                out.markers.push(seen(e, st));
            }
        }
    }
    out
}

/// `_drop_stale(scan, now, marker_age, r)`.
fn drop_stale(temps: &[Seen], markers: &[Seen], now: f64, marker_age: f64, r: &mut Done) {
    let all = temps
        .iter()
        .map(|t| (t, STALE_TMP))
        .chain(markers.iter().map(|m| (m, marker_age)));
    for (f, age) in all {
        if now - (f.mtime_ns as f64) / 1e9 > age && f.delete_if_unchanged(true) == Some(true) {
            r.stale += 1;
        }
    }
}

fn by_use(a: &Seen, b: &Seen) -> std::cmp::Ordering {
    a.used.total_cmp(&b.used).then_with(|| a.path.cmp(&b.path))
}

/// `prune(top, cap, suffixes, markers=, marker_age=)`: under the folder's lock, then its `.pruned` stamp.
fn prune(top: &str, cap_bytes: u64, s: &Spec) -> PyResult<Done> {
    if !isdir(top) {
        return Ok(Done::default());
    }
    fsx::locked(&join(top, &[LOCK]), || {
        let mut r = Done::default();
        let sc = scan(top, s.suffixes, s.markers);
        drop_stale(&sc.temps, &sc.markers, fsx::now(), MARKER_AGE, &mut r);
        let mut total: u64 = sc.entries.iter().map(|f| f.size).sum();
        // round QW1: the bundle cache's superseded payloads (a bundle rewritten at the same path) go first, whatever
        // the cap; caches.py's prune does the same through the same bc2_formats function
        let gone: HashSet<String> = if s.name == "fbrb" {
            bc2_formats::fbrb::cache::superseded(std::path::Path::new(top))
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect()
        } else {
            HashSet::new()
        };
        let mut order: Vec<&Seen> = Vec::new();
        for f in &sc.entries {
            if !gone.contains(&basename(&f.path)) {
                order.push(f);
                continue;
            }
            match f.delete_if_unchanged(true) {
                Some(false) => {
                    r.busy += 1;
                    order.push(f);
                }
                gone_now => {
                    total -= f.size;
                    if gone_now == Some(true) {
                        r.deleted_bytes += f.size;
                        r.deleted_files += 1;
                    }
                }
            }
        }
        order.sort_by(|a, b| by_use(a, b));
        for f in order {
            if total <= cap_bytes {
                break;
            }
            match f.delete_if_unchanged(true) {
                Some(false) => r.busy += 1,
                gone => {
                    total -= f.size;
                    if gone == Some(true) {
                        r.deleted_bytes += f.size;
                        r.deleted_files += 1;
                    }
                }
            }
        }
        r.kept_bytes = total;
        r.kept_files = sc.entries.len() as u64 - r.deleted_files;
        let _ = fsx::touch_marker(&join(top, &[STAMP]));
        r
    })
    .map_err(|_| Fail::Python)
}

/// `maybe_prune(name, top)` after a tool wrote to its cache (round CT1: culltree's row cache): the folder pruned to
/// the cache's cap unless its `.pruned` stamp says it was in the last 15 minutes, then the overall budget
/// (`maybe_prune_total`, once per process). Never fails: a cache that cannot be pruned is only bigger.
#[cfg_attr(
    not(feature = "cull"),
    allow(dead_code, reason = "the cull group's row cache is its only user")
)]
pub fn maybe_prune(name: &str, top: &str) {
    let Some(s) = spec(name) else { return };
    if !isdir(top) {
        return;
    }
    let stamp = join(top, &[STAMP]);
    if fsx::stat(&stamp).is_ok_and(|st| fsx::now() - st.mtime < budget::PRUNE_EVERY) {
        return;
    }
    let _ = prune(top, cap(s), s);
    budget::after_bundle_read();
}

/// `clear(top, suffixes, markers)`.
fn clear(top: &str, s: &Spec) -> PyResult<Done> {
    if !isdir(top) {
        return Ok(Done::default());
    }
    fsx::locked(&join(top, &[LOCK]), || {
        let mut r = Done::default();
        let sc = scan(top, s.suffixes, s.markers);
        drop_stale(&sc.temps, &[], fsx::now(), 0.0, &mut r);
        for f in &sc.markers {
            let _ = f.delete_if_unchanged(false);
        }
        for f in &sc.entries {
            match f.delete_if_unchanged(false) {
                Some(false) => {
                    r.busy += 1;
                    r.kept_bytes += f.size;
                    r.kept_files += 1;
                }
                Some(true) => {
                    r.deleted_bytes += f.size;
                    r.deleted_files += 1;
                }
                None => {}
            }
        }
        r
    })
    .map_err(|_| Fail::Python)
}

/// `usage(top, suffixes, markers)`: (bytes of entries, entries, bytes of temporary files and markers).
fn usage(top: &str, s: &Spec) -> (u64, usize, u64) {
    if !isdir(top) {
        return (0, 0, 0);
    }
    let sc = scan(top, s.suffixes, s.markers);
    let sum = |v: &[Seen]| v.iter().map(|f| f.size).sum::<u64>();
    (sum(&sc.entries), sc.entries.len(), sum(&sc.temps) + sum(&sc.markers))
}

/// `members(opfor)`: (spec, folder) of every cache the budget counts, a folder named twice counted once.
fn members(opfor: Option<&str>) -> Vec<(&'static Spec, String)> {
    let mut out = Vec::new();
    let mut keys = HashSet::new();
    for s in SPECS {
        let Some(top) = folder(s, opfor) else { continue };
        if !isdir(&top) {
            continue;
        }
        if keys.insert(fsx::normcase(&abspath(&top))) {
            out.push((s, top));
        }
    }
    out
}

/// `prune_total(opfor=opfor)`: {name: what it did there}, in the members' order.
fn prune_total(opfor: Option<&str>) -> PyResult<Vec<(&'static str, Done)>> {
    let cap_bytes = total_cap();
    let root = budget::cache_root();
    std::fs::create_dir_all(&root).map_err(|_| Fail::Python)?;
    fsx::locked(&join(&root, &[budget::TOTAL_LOCK]), || {
        let mut files: Vec<(Seen, usize)> = Vec::new();
        let mut out: Vec<(&'static str, Done)> = Vec::new();
        for (s, top) in members(opfor) {
            let k = out.len();
            files.extend(scan(&top, s.suffixes, &[]).entries.into_iter().map(|f| (f, k)));
            out.push((s.name, Done::default()));
        }
        let mut total: u64 = files.iter().map(|f| f.0.size).sum();
        let mut order: Vec<&(Seen, usize)> = files.iter().collect();
        order.sort_by(|a, b| by_use(&a.0, &b.0));
        for (f, k) in order {
            if total <= cap_bytes {
                break;
            }
            let Some(r) = out.get_mut(*k).map(|o| &mut o.1) else {
                continue;
            };
            match f.delete_if_unchanged(true) {
                Some(false) => r.busy += 1,
                gone => {
                    total -= f.size;
                    if gone == Some(true) {
                        r.deleted_bytes += f.size;
                        r.deleted_files += 1;
                    }
                }
            }
        }
        for (f, k) in &files {
            if std::path::Path::new(&f.path).exists()
                && let Some(o) = out.get_mut(*k)
            {
                o.1.kept_bytes += f.size;
                o.1.kept_files += 1;
            }
        }
        let _ = fsx::touch_marker(&join(&root, &[budget::TOTAL_STAMP]));
        out
    })
    .map_err(|_| Fail::Python)
}

// ------------------------------------------------------------------------------------------------ the folders

/// `native.SUITE`: the suite this command line belongs to, absolute.
fn suite() -> String {
    abspath(&suite_root().to_string_lossy())
}

fn env_stripped(name: &str) -> Option<String> {
    let v = std::env::var(name).ok()?;
    let v = bc2_pyrt::py::strip(&v).to_owned();
    (!v.is_empty()).then_some(v)
}

/// `native.targets_root()`.
fn targets_root() -> String {
    match env_stripped("BFBC2_RUST_TARGETS") {
        Some(v) => abspath(&v),
        None => join(&dirname(&suite()), &["rust-targets"]),
    }
}

/// `native.target_dir()`.
fn target_dir() -> String {
    let s = suite();
    let Some(v) = env_stripped("CARGO_TARGET_DIR") else {
        return join(&targets_root(), &[&basename(&fsx::normpath(&s))]);
    };
    let d = bc2_pyrt::py::drive_len(&v);
    let joined = if d > 0 {
        v
    } else if v.starts_with(['\\', '/']) {
        // ntpath.join: a rooted name keeps the first part's drive
        format!("{}{v}", &s[..bc2_pyrt::py::drive_len(&s)])
    } else {
        join(&s, &["rust", &v])
    };
    fsx::normpath(&joined)
}

/// `tree_size(top)`: (bytes, files); links and junctions not followed.
fn tree_size(top: &str) -> (u64, u64) {
    let (mut size, mut n) = (0, 0);
    let mut stack = vec![top.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = fsx::scandir(&d) else { continue };
        for e in entries {
            if e.link {
                continue;
            }
            if e.dir {
                stack.push(e.path);
            } else if e.file
                && let Some(st) = e.stat
            {
                size += st.size;
                n += 1;
            }
        }
    }
    (size, n)
}

/// `target_folders()`: (name, folder, its checkout exists) of every cargo build folder in the build root.
fn target_folders() -> Vec<(String, String, bool)> {
    let root = targets_root();
    let checkouts = dirname(&suite());
    let mut names: HashSet<String> = HashSet::new();
    if let Ok(es) = fsx::scandir(&checkouts) {
        for e in es {
            if e.dir && isdir(&join(&e.path, &["rust"])) {
                names.insert(e.name.to_lowercase());
            }
        }
    }
    let has = |name: &str| {
        let n = name.to_lowercase();
        names.contains(&n) || names.iter().any(|c| c.ends_with(&format!("-{n}")))
    };
    let mut out = Vec::new();
    if let Ok(mut es) = fsx::scandir(&root) {
        es.sort_by_key(|e| e.name.to_lowercase());
        for e in es {
            if !e.dir || e.link {
                continue;
            }
            if !CARGO_MARKS.iter().any(|m| isfile(&join(&e.path, &[m]))) {
                continue;
            }
            let h = has(&e.name);
            out.push((e.name, e.path, h));
        }
    }
    out
}

/// `_last_write(top)`: the latest modification time of top, its entries and theirs.
fn last_write(top: &str) -> PyResult<f64> {
    let mut t = 0.0f64;
    let subs = fsx::scandir(top).map_err(|_| Fail::Python)?;
    let mut dirs = vec![top.to_owned()];
    dirs.extend(subs.into_iter().filter(|e| e.dir).map(|e| e.path));
    for d in dirs {
        if let Ok(st) = fsx::stat(&d) {
            t = t.max(st.mtime);
            if let Ok(es) = fsx::scandir(&d) {
                for e in es {
                    if let Some(st) = e.stat {
                        t = t.max(st.mtime);
                    }
                }
            }
        }
    }
    Ok(t)
}

/// `str(OSError)` as Windows' Python words it: `[WinError N] <message>: '<path>'`.
fn os_error_text(e: &std::io::Error, path: &str) -> String {
    let text = e.to_string();
    let msg = text
        .split(" (os error")
        .next()
        .unwrap_or(&text)
        .trim_end_matches('.')
        .to_owned();
    match e.raw_os_error() {
        Some(code) => format!("[WinError {code}] {msg}: {}", bc2_pyrt::fmt::str_repr(path)),
        None => msg,
    }
}

/// `prune_targets(delete)`.
fn prune_targets(delete: bool) -> PyResult<Vec<(String, String, String)>> {
    let mut out = Vec::new();
    for (name, top, has) in target_folders() {
        if has {
            continue;
        }
        if fsx::now() - last_write(&top)? < TARGET_RECENT {
            out.push((
                name,
                top,
                "kept: written in the last day (a build may be running)".to_owned(),
            ));
            continue;
        }
        if !delete {
            out.push((name, top, "orphan".to_owned()));
            continue;
        }
        match std::fs::remove_dir_all(&top) {
            Ok(()) => out.push((name, top, "deleted".to_owned())),
            Err(e) => {
                let what = format!("kept: {}", os_error_text(&e, &top));
                out.push((name, top, what));
            }
        }
    }
    Ok(out)
}

/// `WORK_KINDS`.
fn work_kind(name: &str) -> &'static str {
    match name {
        "onslaught" | "xbox" | "xbox_dlc" | "navsample" | "mapedit" => "input",
        "levels" | "harness" | "navstudio" | "testrun" | "tools_out" | "editor" | "import" => "cache",
        "ww2" | "navkit" | "navsample_offline" | "navkit_offline" | "navwater" | "navvehicle" | "navbuild"
        | "stockmp" | "bf1943" | "playtest" | "audit" | "aptgraft" => "output",
        "console_deps" | "console_input" | "console_loading" | "console_loadset" | "console_mode"
        | "console_soldiers" | "console_vehicles" | "console_cull" | "console_markv" | "console_icons"
        | "console_shaders" | "combat_a" | "combat_b" | "sectoroff" | "menus" => "scratch",
        "console_hud" | "backups" | "logs-archive" => "backup",
        _ => "unknown",
    }
}

type WorkRow = (String, &'static str, u64, u64);

/// `work_by_kind(opfor)`: (folder, kind, bytes, files), biggest first.
fn work_by_kind(opfor: Option<&str>) -> PyResult<Vec<WorkRow>> {
    let Some(root) = project_root(opfor) else {
        return Ok(Vec::new());
    };
    let work = join(&root, &["work"]);
    if !isdir(&work) {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for e in fsx::scandir(&work).map_err(|_| Fail::Python)? {
        let kind = work_kind(&e.name);
        if e.link {
            out.push((e.name, kind, 0, 0));
        } else if e.dir {
            let (b, n) = tree_size(&e.path);
            out.push((e.name, kind, b, n));
        } else {
            let size = e.stat.map_or(0, |s| s.size);
            out.push((e.name, kind, size, 1));
        }
    }
    out.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ printing

/// `_size(n)`.
fn size(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    let mut v = n as f64;
    for unit in ["KB", "MB"] {
        v /= 1024.0;
        if v < 1024.0 {
            return format!("{v:.1} {unit}");
        }
    }
    v /= 1024.0;
    format!("{v:.1} GB")
}

/// `_done(n, r)`.
fn done(n: &str, r: &Done) -> String {
    let mut s = format!(
        "{n:<11} deleted {} file(s), {}; left {} file(s), {}",
        r.deleted_files,
        size(r.deleted_bytes),
        r.kept_files,
        size(r.kept_bytes)
    );
    if r.busy > 0 {
        let _ = write!(s, "; {} in use, kept", r.busy);
    }
    if r.stale > 0 {
        let _ = write!(s, "; {} stale temporary file(s) or marker(s) deleted", r.stale);
    }
    s
}

fn off(s: &Spec, hint: bool) -> String {
    if s.project.is_empty() {
        format!("off ({}=0)", s.env)
    } else if hint {
        "no project folder (--opfor, OPFOR_ROOT)".to_owned()
    } else {
        "no project folder".to_owned()
    }
}

/// `report(opfor, work)`.
fn report(py: &mut Py, opfor: Option<&str>, work: bool) -> PyResult<()> {
    let mut total = 0u64;
    py.print("caches")?;
    for s in SPECS {
        let Some(top) = folder(s, opfor) else {
            py.print(&format!("  {:<13} {}", s.name, off(s, false)))?;
            continue;
        };
        let (b, files, extra) = usage(&top, s);
        total += b + extra;
        py.print(&format!(
            "  {:<13} {:>9} in {files:>6} file(s)   cap {:>9}   {top}",
            s.name,
            size(b + extra),
            size(cap(s))
        ))?;
    }
    py.print(&format!(
        "  {:<13} {:>9}                    budget {:>9} ({}, GB)",
        "all caches",
        size(total),
        size(total_cap()),
        budget::TOTAL_ENV
    ))?;
    let (b, files) = logs::logs_usage(&pylog::logs_dir());
    py.print(&format!(
        "  {LOGS:<13} {:>9} in {files:>6} file(s)   cap {:>9}   {}",
        size(b),
        size(logs::log_caps().0),
        pylog::logs_dir()
    ))?;
    py.print(&format!(
        "Rust build folders ({}; this checkout: {})",
        targets_root(),
        target_dir()
    ))?;
    let rows = target_folders();
    for (name, top, has) in &rows {
        let (sz, _) = tree_size(top);
        let what = if *has {
            "checkout there"
        } else {
            "checkout gone: --prune-targets"
        };
        py.print(&format!("  {name:<28} {:>9}   {what}", size(sz)))?;
    }
    if rows.is_empty() {
        py.print("  (none)")?;
    }
    let old = join(&suite(), &["rust", "target"]);
    if isdir(&old) {
        py.print(&format!(
            "  this checkout's old rust\\target: {} (README.md, \"Where Rust builds\": how to move it)",
            size(tree_size(&old).0)
        ))?;
    }
    if !work {
        return Ok(());
    }
    let rows2 = work_by_kind(opfor)?;
    if rows2.is_empty() {
        return py.print("Project OPFOR's work\\: not found (--opfor, OPFOR_ROOT)");
    }
    let root = project_root(opfor).unwrap_or_default();
    py.print(&format!("Project OPFOR's work\\ ({}) by kind", join(&root, &["work"])))?;
    let mut all = 0;
    for kind in KINDS {
        let of: Vec<&WorkRow> = rows2.iter().filter(|r| r.1 == kind).collect();
        if of.is_empty() {
            continue;
        }
        let sum: u64 = of.iter().map(|r| r.2).sum();
        all += sum;
        let names: Vec<String> = of
            .iter()
            .filter(|r| r.2 > 0)
            .map(|r| format!("{} {}", r.0, size(r.2)))
            .collect();
        let shown: Vec<&str> = names.iter().take(12).map(String::as_str).collect();
        py.print(&format!(
            "  {kind:<8} {:>9}   {}{}",
            size(sum),
            shown.join(", "),
            if names.len() > 12 { " ..." } else { "" }
        ))?;
    }
    py.print(&format!("  {:<8} {:>9}", "all", size(all)))
}

// ------------------------------------------------------------------------------------------------ argparse

const PROG: &str = "python -m bfbc2core.caches";
const USAGE: &str = "usage: python -m bfbc2core.caches [-h] [--list | --prune | --clear |
                                  --report | --prune-targets] [--yes]
                                  [--no-work] [--opfor DIR]
                                  [name ...]
";
const HELP: &str = "
The suite's on-disk caches, the project caches in Project OPFOR BFBC2's work\\
folder and the logs: list them, prune them to their caps, or clear them.

positional arguments:
  name             caches to act on: hlsl, fbrb, collision, effectport,
                   culltree, bf1943source, shadercover, navstudio, harness,
                   navwater, editor, logs

options:
  -h, --help       show this help message and exit
  --list           each cache: folder, size, files, cap (the default)
  --prune          delete the least recently used files beyond each cap (every
                   cache, project cache and the logs when no name)
  --clear          delete the cached files (every suite cache when no name;
                   project caches and logs only when named)
  --report         every cache with its size and cap, the overall budget, the
                   logs, the Rust build folders, Project OPFOR's work\\ by kind
  --prune-targets  list the Rust build folders whose checkout is gone (--yes:
                   delete them)
  --yes            with --prune-targets: delete the folders listed
  --no-work        with --report: leave out work\\ (its scan takes a while)
  --opfor DIR      Project OPFOR BFBC2's folder (default: as the tools find
                   it)
";
const GROUP: [&str; 5] = ["--list", "--prune", "--clear", "--report", "--prune-targets"];
const FLAGS: [&str; 7] = [
    "--list",
    "--prune",
    "--clear",
    "--report",
    "--prune-targets",
    "--yes",
    "--no-work",
];

#[derive(Default)]
struct Args {
    mode: Option<&'static str>,
    yes: bool,
    no_work: bool,
    opfor: Option<String>,
    names: Vec<String>,
}

/// `parser.error(message)`: usage (at the console's width) and message on stderr, exit code 2.
fn error(py: &mut Py, message: &str) -> PyResult<Option<Args>> {
    py.eprint(crate::argfmt::usage_or(PROG, USAGE)?.trim_end_matches('\n'));
    py.eprint(&format!("{PROG}: error: {message}"));
    Err(Fail::Err(PyErr::new("SystemExit", "2")))
}

/// Whether `argparse` takes an argument as an option string (it starts with '-', is not '-' alone and is not a
/// negative number: no option of this parser looks like one).
fn is_option(a: &str) -> bool {
    let Some(rest) = a.strip_prefix('-') else { return false };
    if rest.is_empty() {
        return false;
    }
    let digits = |s: &str| s.chars().all(|c| c.is_ascii_digit());
    let neg = match rest.split_once('.') {
        None => digits(rest),
        Some((i, f)) => digits(i) && !f.is_empty() && digits(f),
    };
    !neg
}

/// `ap.parse_args(argv)`: Some(args), None after `-h` (the help printed), or the error.
fn parse(py: &mut Py, argv: &[String]) -> PyResult<Option<Args>> {
    let mut a = Args::default();
    let mut names_done = false;
    let mut extras: Vec<String> = Vec::new();
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        if !is_option(arg) {
            // a run of values: the first fills `names` (nargs='*'), a later one is left over
            let mut j = i;
            while j < argv.len() && !is_option(&argv[j]) {
                j += 1;
            }
            if names_done {
                extras.extend(argv[i..j].iter().cloned());
            } else {
                a.names = argv[i..j].to_vec();
                names_done = true;
            }
            i = j;
            continue;
        }
        if arg == "-h" || arg == "--help" {
            py.write(&crate::argfmt::help_or(PROG, &format!("{USAGE}{HELP}"))?)?;
            return Ok(None);
        }
        if arg == "--" || !arg.starts_with("--") {
            return Err(Fail::Python);
        }
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) => (n, Some(v)),
            None => (arg, None),
        };
        if name == "--opfor" {
            match inline {
                Some(v) => a.opfor = Some(v.to_owned()),
                None => match argv.get(i + 1) {
                    Some(v) if !is_option(v) => {
                        a.opfor = Some(v.clone());
                        i += 1;
                    }
                    _ => return error(py, "argument --opfor: expected one argument"),
                },
            }
            i += 1;
            continue;
        }
        if let Some(&flag) = FLAGS.iter().find(|f| **f == name) {
            if inline.is_some() {
                return Err(Fail::Python);
            }
            if GROUP.contains(&flag) {
                if let Some(prev) = a.mode
                    && prev != flag
                {
                    return error(py, &format!("argument {flag}: not allowed with argument {prev}"));
                }
                a.mode = Some(flag);
            } else if flag == "--yes" {
                a.yes = true;
            } else {
                a.no_work = true;
            }
            i += 1;
            continue;
        }
        // an abbreviation argparse would take (or call ambiguous) is left to it
        if FLAGS
            .iter()
            .chain(["--opfor", "--help"].iter())
            .any(|o| o.starts_with(name))
        {
            return Err(Fail::Python);
        }
        extras.push(arg.to_owned());
        i += 1;
    }
    if !extras.is_empty() {
        return error(py, &format!("unrecognized arguments: {}", extras.join(" ")));
    }
    Ok(Some(a))
}

/// `raise SystemExit(message)`: the message on stderr, exit code 1.
fn system_exit(py: &mut Py, message: &str) -> i32 {
    py.eprint(message);
    1
}

/// `--prune` / `--clear` of the names.
fn prune_or_clear(py: &mut Py, a: &Args, names: &[String], prune_mode: bool) -> PyResult<i32> {
    let opfor = a.opfor.as_deref().filter(|o| !o.is_empty());
    for n in names {
        if n == LOGS {
            let caps = if prune_mode { None } else { Some((0, 0, 0)) };
            let r = logs::prune_logs(&pylog::logs_dir(), &[], caps).map_err(|_| Fail::Python)?;
            py.print(&done(n, &r))?;
            continue;
        }
        let Some(s) = spec(n) else { continue };
        let Some(top) = folder(s, opfor) else {
            py.print(&format!("{n:<11} {}", off(s, true)))?;
            continue;
        };
        let r = if prune_mode {
            prune(&top, cap(s), s)?
        } else {
            clear(&top, s)?
        };
        py.print(&done(n, &r))?;
    }
    if prune_mode && a.names.is_empty() {
        for (n, r) in prune_total(opfor)? {
            if r.deleted_files > 0 || r.busy > 0 {
                py.print(&done(&format!("{n} (budget)"), &r))?;
            }
        }
        py.print(&format!(
            "all caches within the overall budget of {} ({})",
            size(total_cap()),
            budget::TOTAL_ENV
        ))?;
    }
    if !prune_mode && a.names.is_empty() {
        let proj: Vec<&str> = SPECS
            .iter()
            .filter(|s| !s.project.is_empty())
            .map(|s| s.name)
            .chain([LOGS])
            .collect();
        py.print(&format!(
            "(project caches and logs are cleared only by name: {})",
            proj.join(", ")
        ))?;
    }
    Ok(0)
}

/// The default `--list`.
fn list(py: &mut Py, a: &Args, names: &[String]) -> PyResult<i32> {
    let opfor = a.opfor.as_deref().filter(|o| !o.is_empty());
    let mut total = 0u64;
    for n in names {
        if n == LOGS {
            let (b, files) = logs::logs_usage(&pylog::logs_dir());
            total += b;
            let (all_cap, tool_cap, file_cap) = logs::log_caps();
            py.print(&format!(
                "{n:<11} {:>9} in {files:>6} file(s)   cap {:>9} ({}, MB; per tool {} {}, per file {} {})   {}",
                size(b),
                size(all_cap),
                logs::LOGS_ENV,
                size(tool_cap),
                logs::LOG_TOOL_ENV,
                size(file_cap),
                logs::LOG_FILE_ENV,
                pylog::logs_dir()
            ))?;
            continue;
        }
        let Some(s) = spec(n) else { continue };
        let Some(top) = folder(s, opfor) else {
            py.print(&format!("{n:<11} {}", off(s, true)))?;
            continue;
        };
        let (b, files, extra) = usage(&top, s);
        total += b + extra;
        let unit = if (s.cap_unit - MB).abs() < 1.0 { "MB" } else { "GB" };
        py.print(&format!(
            "{n:<11} {:>9} in {files:>6} file(s)   cap {:>9} ({}, {unit})   {top}{}",
            size(b + extra),
            size(cap(s)),
            s.cap_env,
            if s.project.is_empty() { "" } else { "   (project cache)" }
        ))?;
    }
    py.print(&format!(
        "{:<11} {:>9}   (the caches' overall budget: {}, {})",
        "all",
        size(total),
        size(total_cap()),
        budget::TOTAL_ENV
    ))?;
    Ok(0)
}

/// `main(argv)`.
pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    let a = match parse(py, argv) {
        Ok(Some(a)) => a,
        Ok(None) => return Ok(0),
        Err(Fail::Err(e)) if e.kind == "SystemExit" => return Ok(2),
        Err(e) => return Err(e),
    };
    let known: Vec<&str> = SPECS.iter().map(|s| s.name).chain([LOGS]).collect();
    let bad: Vec<&str> = a
        .names
        .iter()
        .map(String::as_str)
        .filter(|n| !known.contains(n))
        .collect();
    if !bad.is_empty() {
        let msg = format!("unknown cache {}; the caches are {}", bad.join(", "), known.join(", "));
        return Ok(system_exit(py, &msg));
    }
    let mut names: Vec<String> = if a.names.is_empty() {
        known.iter().map(|s| (*s).to_owned()).collect()
    } else {
        a.names.clone()
    };
    if let Some(o) = a.opfor.as_deref().filter(|o| !o.is_empty())
        && !isdir(&join(o, &["game"]))
    {
        let msg = format!("--opfor {o}: no game\\ folder there (not Project OPFOR BFBC2's folder)");
        return Ok(system_exit(py, &msg));
    }
    match a.mode {
        Some("--report") => {
            report(py, a.opfor.as_deref().filter(|o| !o.is_empty()), !a.no_work)?;
            Ok(0)
        }
        Some("--prune-targets") => {
            let rows = prune_targets(a.yes)?;
            for (name, where_, what) in &rows {
                py.print(&format!("{name:<28} {what:<10} {where_}"))?;
            }
            if rows.is_empty() {
                py.print(&format!("no build folder without its checkout in {}", targets_root()))?;
            } else if !a.yes && rows.iter().any(|r| r.2 == "orphan") {
                py.print("(--yes deletes the orphans)")?;
            }
            Ok(0)
        }
        Some("--prune") => prune_or_clear(py, &a, &names, true),
        Some("--clear") => {
            if a.names.is_empty() {
                names = SPECS
                    .iter()
                    .filter(|s| s.project.is_empty())
                    .map(|s| s.name.to_owned())
                    .collect();
            }
            prune_or_clear(py, &a, &names, false)
        }
        _ => list(py, &a, &names),
    }
}
