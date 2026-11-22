//! The caches' budget, one implementation (round PR1b-2; until then `bfbc2.exe`'s `budget` / `caches` and
//! `bfbc2core.caches` each had their own): the suite's caches and Project OPFOR BFBC2's project caches ([`SPECS`]),
//! where each lives ([`folder`]), its cap ([`cap`]), and the pruning - one cache brought under its cap ([`prune`]),
//! every cache together under `BFBC2_CACHE_TOTAL_GB` ([`prune_total`], at most once per 15 minutes across processes:
//! [`maybe_prune_total`]), the least recently used entries first, a file deleted only while it is as the scan saw it.
//! `bfbc2.exe caches` and `bc2_native` (`bfbc2core.caches`' prune, clear and usage) run this.
//!
//! The rules are `bfbc2core.caches`' (round D1, proven against it by the caches group's recorded answers): a cache's
//! entries are its files with one of its suffixes (hidden files, links and temporary files left out), "used" is the
//! later of their access and modification times, ties broken by path; temporary files of writers that died (an hour
//! old) and a cache's bookkeeping markers (30 days old) are deleted first; the folder's `.prune.lock` (the same lock
//! between Python and Rust processes, [`crate::atomic::locked`]) is held while it is pruned, and `.pruned` stamped.

use std::collections::HashSet;
use std::fs::{self, File, FileTimes, OpenOptions};
use std::io;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// How often a cache (and the overall budget) is pruned at most, seconds (`caches.PRUNE_EVERY`).
pub const PRUNE_EVERY: f64 = 900.0;
/// The overall cap's variable, GiB (default 20).
pub const TOTAL_ENV: &str = "BFBC2_CACHE_TOTAL_GB";
/// The overall pruning's lock, in the cache root.
pub const TOTAL_LOCK: &str = ".prune-total.lock";
/// The overall pruning's stamp, in the cache root.
pub const TOTAL_STAMP: &str = ".pruned-total";
/// A cache folder's pruning lock.
pub const LOCK: &str = ".prune.lock";
/// A cache folder's pruning stamp.
pub const STAMP: &str = ".pruned";
/// A temporary file this old belongs to a writer that died, seconds.
pub const STALE_TMP: f64 = 3600.0;
/// A cache's bookkeeping markers are deleted this old, seconds (`Spec.marker_age`).
pub const MARKER_AGE: f64 = 30.0 * 86_400.0;
/// A MiB.
pub const MB: f64 = 1_048_576.0;
/// A GiB.
pub const GB: f64 = 1_073_741_824.0;

/// One cache: its name, folder variable (a suite cache), entry suffixes, whether it is off without `LOCALAPPDATA`,
/// its folder under Project OPFOR BFBC2's (a project cache), its cap and its bookkeeping markers.
#[derive(Debug)]
pub struct Spec {
    /// The name (`--prune <name>`).
    pub name: &'static str,
    /// The folder's variable (`0` / `off`: no cache), empty for a project cache.
    pub env: &'static str,
    /// The entries' file name endings.
    pub suffixes: &'static [&'static str],
    /// Off when `LOCALAPPDATA` is not set.
    pub off_without_localappdata: bool,
    /// A project cache's folder under Project OPFOR BFBC2's (empty: a suite cache).
    pub project: &'static [&'static str],
    /// The cap's variable: a number of `cap_unit` bytes, fractions allowed; else `default_cap`.
    pub cap_env: &'static str,
    /// The cap's unit.
    pub cap_unit: f64,
    /// The cap without the variable.
    pub default_cap: f64,
    /// Small bookkeeping files (deleted by a clear, and by the pruning once older than [`MARKER_AGE`]).
    pub markers: &'static [&'static str],
}

/// `navdata.load`'s entries.
const NAVDATA: &[&str] = &[".navdata.pkl", ".npz"];

const fn suite(
    name: &'static str,
    env: &'static str,
    suffixes: &'static [&'static str],
    cap_env: &'static str,
    cap_unit: f64,
    default_cap: f64,
) -> Spec {
    Spec {
        name,
        env,
        suffixes,
        off_without_localappdata: false,
        project: &[],
        cap_env,
        cap_unit,
        default_cap,
        markers: &[],
    }
}

const fn project(
    name: &'static str,
    suffixes: &'static [&'static str],
    folder: &'static [&'static str],
    cap_env: &'static str,
    default_cap: f64,
) -> Spec {
    Spec {
        name,
        env: "",
        suffixes,
        off_without_localappdata: false,
        project: folder,
        cap_env,
        cap_unit: MB,
        default_cap,
        markers: &[],
    }
}

/// Every cache: `caches.CACHES`, then `PROJECT_CACHES` (those with a project folder).
pub const SPECS: &[Spec] = &[
    Spec {
        off_without_localappdata: true,
        ..suite("hlsl", "BFBC2_HLSL_CACHE", &[".bin"], "BFBC2_HLSL_CACHE_MB", MB, 1024.0)
    },
    Spec {
        markers: &[".seen"],
        ..suite(
            "fbrb",
            "BFBC2_FBRB_CACHE",
            &[".points", ".payload"],
            "BFBC2_FBRB_CACHE_GB",
            GB,
            16.0,
        )
    },
    suite(
        "collision",
        "BFBC2_COLLISION_CACHE",
        &[".bin"],
        "BFBC2_COLLISION_CACHE_MB",
        MB,
        256.0,
    ),
    suite(
        "effectport",
        "BFBC2_EFFECTPORT_CACHE",
        &[".pkl"],
        "BFBC2_EFFECTPORT_CACHE_MB",
        MB,
        256.0,
    ),
    suite(
        "culltree",
        "BFBC2_CULLTREE_CACHE",
        &[".pkl"],
        "BFBC2_CULLTREE_CACHE_MB",
        MB,
        2048.0,
    ),
    suite(
        "bf1943source",
        "BF1943_SOURCE_INDEX",
        &[".pkl", ".points"],
        "BF1943_SOURCE_INDEX_MB",
        MB,
        64.0,
    ),
    suite(
        "shadercover",
        "BFBC2_SHADERCOVER_CACHE",
        &[".pkl"],
        "BFBC2_SHADERCOVER_CACHE_MB",
        MB,
        64.0,
    ),
    suite(
        "renderqa",
        "BFBC2_RENDERQA_CACHE",
        &[".pkl"],
        "BFBC2_RENDERQA_CACHE_MB",
        MB,
        64.0,
    ),
    project(
        "navstudio",
        NAVDATA,
        &["work", "navstudio", "cache"],
        "BFBC2_NAVSTUDIO_CACHE_MB",
        2048.0,
    ),
    project(
        "harness",
        NAVDATA,
        &["work", "harness", "cache"],
        "BFBC2_HARNESS_CACHE_MB",
        3072.0,
    ),
    project(
        "navwater",
        NAVDATA,
        &["work", "navwater", "cache"],
        "BFBC2_NAVWATER_CACHE_MB",
        1024.0,
    ),
    project(
        "editor",
        &[".dbx", ".res", ".xml", ".json"],
        &["work", "editor", "levels"],
        "BFBC2_EDITOR_CACHE_MB",
        2048.0,
    ),
    project(
        "meshview",
        &["meshview_index.pkl"],
        &["work", "tools_out"],
        "BFBC2_MESHVIEW_CACHE_MB",
        256.0,
    ),
];

/// The cache of this name.
#[must_use]
pub fn spec(name: &str) -> Option<&'static Spec> {
    SPECS.iter().find(|s| s.name == name)
}

// ------------------------------------------------------------------------------------------------ paths

/// `os.path.join` (Windows) of a folder and plain names.
#[must_use]
pub fn join(a: &str, names: &[&str]) -> String {
    let mut out = a.to_owned();
    for n in names {
        if out.is_empty() || out.ends_with(['\\', '/']) || (out.len() == 2 && out.ends_with(':')) {
            out.push_str(n);
        } else {
            out.push(std::path::MAIN_SEPARATOR);
            out.push_str(n);
        }
    }
    out
}

/// `os.path.expanduser('~')`: `USERPROFILE`, else `HOMEDRIVE` + `HOMEPATH`, else `~`.
#[must_use]
pub fn home_dir() -> String {
    if let Ok(p) = std::env::var("USERPROFILE") {
        return p;
    }
    match std::env::var("HOMEPATH") {
        Ok(hp) => join(&std::env::var("HOMEDRIVE").unwrap_or_default(), &[&hp]),
        Err(_) => "~".to_owned(),
    }
}

fn local_app_data() -> Option<String> {
    std::env::var("LOCALAPPDATA").ok().filter(|v| !v.is_empty())
}

/// A cache's folder as its owner resolves it, `None` when it is off (or a project cache without the project).
#[must_use]
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
        None if s.name == "fbrb" => join(&home_dir(), &[".cache"]),
        None => home_dir(),
    };
    Some(join(&base, &["bfbc2-tool-suite", "cache", s.name]))
}

/// The suite caches' common folder, where the overall pruning keeps its lock and stamp.
#[must_use]
pub fn cache_root() -> String {
    join(
        &local_app_data().unwrap_or_else(home_dir),
        &["bfbc2-tool-suite", "cache"],
    )
}

/// A writer's temporary file (`<name>.tmp`, `<name>.tmp-<pid>-<n>`).
#[must_use]
#[allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the names the suite's writers make, compared as they are"
)]
pub fn is_temp(name: &str) -> bool {
    name.ends_with(".tmp") || name.contains(".tmp-")
}

/// A number of a variable as Python's `float()` reads the plain forms (`2`, `0.5`, `1e3`), else `default`; a negative
/// or infinite number is `default`.
#[must_use]
pub fn number(env: &str, default: f64) -> f64 {
    match std::env::var(env)
        .ok()
        .and_then(|t| t.trim().replace('_', "").parse::<f64>().ok())
    {
        Some(v) if v >= 0.0 && v.is_finite() => v,
        _ => default,
    }
}

/// A cache's cap in bytes.
#[must_use]
pub fn cap(s: &Spec) -> u64 {
    (number(s.cap_env, s.default_cap) * s.cap_unit) as u64
}

/// The overall cap in bytes ([`TOTAL_ENV`]).
#[must_use]
pub fn total_cap() -> u64 {
    (number(TOTAL_ENV, 20.0) * GB) as u64
}

// ------------------------------------------------------------------------------------------------ files

/// The fields of a file's `stat` the pruning reads (the times as Python's floats and `st_mtime_ns`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stat {
    /// Bytes.
    pub size: u64,
    /// `st_mtime_ns`.
    pub mtime_ns: i128,
    /// `st_mtime`, seconds.
    pub mtime: f64,
    /// `st_atime`, seconds.
    pub atime: f64,
}

/// (seconds, nanoseconds) of a time as Python's `FILE_TIME_to_time_t_nsec` splits it.
fn split(t: SystemTime) -> (i64, i64) {
    match t.duration_since(UNIX_EPOCH) {
        Ok(d) => (d.as_secs() as i64, i64::from(d.subsec_nanos())),
        Err(e) => {
            // before 1970: C's division truncates toward zero (seconds and nanoseconds both negative)
            let d = e.duration();
            (-(d.as_secs() as i64), -i64::from(d.subsec_nanos()))
        }
    }
}

#[allow(clippy::cast_precision_loss, reason = "the float Python makes of the seconds")]
fn float_time(t: SystemTime) -> f64 {
    let (s, ns) = split(t);
    s as f64 + ns as f64 * 1e-9
}

impl Stat {
    /// The stat of these metadata.
    #[must_use]
    pub fn of(m: &fs::Metadata) -> Self {
        let mt = m.modified().unwrap_or(UNIX_EPOCH);
        let at = m.accessed().unwrap_or(mt);
        let (s, ns) = split(mt);
        Self {
            size: m.len(),
            mtime_ns: i128::from(s) * 1_000_000_000 + i128::from(ns),
            mtime: float_time(mt),
            atime: float_time(at),
        }
    }
}

/// The stat of a file (links followed).
///
/// # Errors
/// It cannot be read.
pub fn stat(path: &str) -> io::Result<Stat> {
    fs::metadata(path).map(|m| Stat::of(&m))
}

/// Now, seconds since 1970 (`time.time()`).
#[must_use]
pub fn now() -> f64 {
    float_time(SystemTime::now())
}

#[cfg(windows)]
fn open_attrs(path: &str) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    // FILE_WRITE_ATTRIBUTES, as Python's utime opens the file
    OpenOptions::new().access_mode(0x100).open(path)
}

#[cfg(not(windows))]
fn open_attrs(path: &str) -> io::Result<File> {
    OpenOptions::new().write(true).open(path)
}

/// A file's access and modification times set to now (`os.utime(path)`).
///
/// # Errors
/// The file cannot be opened for its attributes.
pub fn utime_now(path: &str) -> io::Result<()> {
    let now = SystemTime::now();
    open_attrs(path)?.set_times(FileTimes::new().set_accessed(now).set_modified(now))
}

/// An empty marker file stamped now.
///
/// # Errors
/// It cannot be made.
pub fn touch_marker(path: &str) -> io::Result<()> {
    File::create(path)?;
    utime_now(path)
}

/// A file removed: `Ok(true)` removed, `Ok(false)` gone already.
///
/// # Errors
/// Anything else (in use without delete sharing, ...).
pub fn remove(path: &str) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Deletes a file if it still has the size and time a scan saw (any time when not `check`): `Some(true)` deleted,
/// `Some(false)` kept (in use, or changed), `None` gone before.
#[must_use]
pub fn delete_if_unchanged(path: &str, size: u64, mtime_ns: i128, check: bool) -> Option<bool> {
    if check {
        match stat(path) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
            Err(_) => return Some(false),
            Ok(st) if st.size != size || st.mtime_ns != mtime_ns => return Some(false),
            Ok(_) => {}
        }
    }
    match remove(path) {
        Ok(true) => Some(true),
        Ok(false) => None,
        Err(_) => Some(false),
    }
}

/// A folder's entry as `os.scandir` gives it: its path, name, kind and stat (the listing's own, links not followed).
#[derive(Debug, Clone)]
pub struct Entry {
    /// `os.path.join(top, name)`.
    pub path: String,
    /// The file name.
    pub name: String,
    /// A symbolic link or junction: never followed.
    pub link: bool,
    /// A folder (not a link).
    pub dir: bool,
    /// A file (not a link).
    pub file: bool,
    /// Its stat, when it could be read.
    pub stat: Option<Stat>,
}

/// A folder's entries in the listing's order.
///
/// # Errors
/// The folder cannot be listed.
pub fn scandir(top: &str) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for e in fs::read_dir(top)? {
        let Ok(e) = e else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        let path = join(top, &[&name]);
        let (link, dir, file) = match e.file_type() {
            Ok(t) => (t.is_symlink(), t.is_dir(), t.is_file()),
            Err(_) => (false, false, false),
        };
        let stat = e.metadata().ok().map(|m| Stat::of(&m));
        out.push(Entry {
            path,
            name,
            link,
            dir: dir && !link,
            file: file && !link,
            stat,
        });
    }
    Ok(out)
}

/// `os.path.normcase` (Windows): lower case, `/` as `\`.
#[must_use]
pub fn normcase(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

fn absolute(p: &str) -> String {
    std::path::absolute(p).map_or_else(|_| p.to_owned(), |a| a.to_string_lossy().into_owned())
}

// ------------------------------------------------------------------------------------------------ pruning

/// A file as a scan saw it.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    /// Its path.
    pub path: String,
    /// Bytes.
    pub size: u64,
    /// `st_mtime_ns`.
    pub mtime_ns: i128,
    /// The later of its access and modification times.
    pub used: f64,
}

impl Seen {
    fn of(e: Entry, st: Stat) -> Self {
        Self {
            path: e.path,
            size: st.size,
            mtime_ns: st.mtime_ns,
            used: st.atime.max(st.mtime),
        }
    }

    /// Deleted if it is still as the scan saw it (see [`delete_if_unchanged`]).
    #[must_use]
    pub fn delete_if_unchanged(&self, check: bool) -> Option<bool> {
        delete_if_unchanged(&self.path, self.size, self.mtime_ns, check)
    }
}

/// What a pruning or clear did in a cache.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Done {
    /// The entries' bytes left.
    pub kept_bytes: u64,
    /// The entries left.
    pub kept_files: u64,
    /// The entries' bytes deleted.
    pub deleted_bytes: u64,
    /// The entries deleted.
    pub deleted_files: u64,
    /// Entries in use (or changed since the scan), kept.
    pub busy: u64,
    /// Temporary files of dead writers and old markers deleted.
    pub stale: u64,
}

/// A cache folder's files: its entries, writers' temporary files and bookkeeping markers.
#[derive(Debug, Clone, Default)]
pub struct Scan {
    /// The entries.
    pub entries: Vec<Seen>,
    /// Writers' temporary files.
    pub temps: Vec<Seen>,
    /// Bookkeeping markers.
    pub markers: Vec<Seen>,
}

/// The files under `top` (every folder below it; hidden files and links left out).
#[must_use]
pub fn scan(top: &str, suffixes: &[&str], markers: &[&str]) -> Scan {
    let mut out = Scan::default();
    let mut stack = vec![top.to_owned()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = scandir(&d) else { continue };
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
            if is_temp(&name) {
                out.temps.push(Seen::of(e, st));
            } else if suffixes.iter().any(|s| name.ends_with(s)) {
                out.entries.push(Seen::of(e, st));
            } else if !markers.is_empty() && markers.iter().any(|m| name.ends_with(m)) {
                out.markers.push(Seen::of(e, st));
            }
        }
    }
    out
}

#[allow(
    clippy::cast_precision_loss,
    reason = "Python's `mtime_ns / 1e9`, a float as it makes it"
)]
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

/// Deletes `f` (as the scan saw it) for the pruning: `total` and `r` follow.
fn take(f: &Seen, total: &mut u64, r: &mut Done) {
    match f.delete_if_unchanged(true) {
        Some(false) => r.busy += 1,
        gone => {
            *total -= f.size;
            if gone == Some(true) {
                r.deleted_bytes += f.size;
                r.deleted_files += 1;
            }
        }
    }
}

/// The file name of a path.
fn file_name(p: &str) -> &str {
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

/// How a folder is pruned: its entries and markers, how old a marker may get, the entries never deleted
/// (`keep`, as paths), and whether the bundle cache's superseded points go first whatever the cap.
#[derive(Debug, Clone, Default)]
pub struct Rules<'a> {
    /// The entries' file name endings.
    pub suffixes: &'a [&'a str],
    /// The markers' file name endings.
    pub markers: &'a [&'a str],
    /// A marker is deleted this old, seconds.
    pub marker_age: f64,
    /// Entries never deleted.
    pub keep: &'a [String],
    /// The bundle cache's folder: its superseded points files go first, whatever the cap.
    pub superseded: bool,
    /// Pruned only when the folder's stamp is older than this many seconds (checked again under the lock: another
    /// process may have pruned it while this one waited); `None`: always.
    pub unless_within: Option<f64>,
}

fn stamp_within(top: &str, secs: Option<f64>) -> bool {
    secs.is_some_and(|w| stat(&join(top, &[STAMP])).is_ok_and(|st| now() - st.mtime < w))
}

/// One cache folder brought under `cap_bytes` (under its lock; its stamp touched): stale temporary files and markers
/// first, then the superseded points (with [`Rules::superseded`]), then the least recently used entries; entries in
/// [`Rules::keep`] never. `None` when [`Rules::unless_within`] says it was pruned lately; a folder that is not
/// there is left alone.
///
/// # Errors
/// The folder's lock could not be taken.
pub fn prune_folder(top: &str, cap_bytes: u64, rules: &Rules<'_>) -> io::Result<Option<Done>> {
    if !Path::new(top).is_dir() {
        return Ok(Some(Done::default()));
    }
    if stamp_within(top, rules.unless_within) {
        return Ok(None);
    }
    crate::atomic::locked(Path::new(&join(top, &[LOCK])), || {
        if stamp_within(top, rules.unless_within) {
            return None;
        }
        let first: HashSet<String> = if rules.superseded {
            crate::bundles::superseded(Path::new(top))
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect()
        } else {
            HashSet::new()
        };
        let keep: HashSet<String> = rules.keep.iter().map(|k| normcase(&absolute(k))).collect();
        let kept = |f: &Seen| !keep.is_empty() && keep.contains(&normcase(&absolute(&f.path)));
        let mut r = Done::default();
        let sc = scan(top, rules.suffixes, rules.markers);
        drop_stale(&sc.temps, &sc.markers, now(), rules.marker_age, &mut r);
        let mut total: u64 = sc.entries.iter().map(|f| f.size).sum();
        let mut order: Vec<&Seen> = Vec::new();
        for f in &sc.entries {
            if !first.contains(file_name(&f.path)) || kept(f) {
                order.push(f);
                continue;
            }
            let busy = r.busy;
            take(f, &mut total, &mut r);
            if r.busy > busy {
                order.push(f);
            }
        }
        order.sort_by(|a, b| by_use(a, b));
        for f in order {
            if total <= cap_bytes {
                break;
            }
            if kept(f) {
                continue;
            }
            take(f, &mut total, &mut r);
        }
        r.kept_bytes = total;
        r.kept_files = sc.entries.len() as u64 - r.deleted_files;
        let _ = touch_marker(&join(top, &[STAMP]));
        Some(r)
    })
}

/// [`prune_folder`] of cache `s` with its own rules (the bundle cache's superseded points first).
///
/// # Errors
/// As [`prune_folder`].
pub fn prune(top: &str, cap_bytes: u64, s: &Spec) -> io::Result<Done> {
    let rules = Rules {
        suffixes: s.suffixes,
        markers: s.markers,
        marker_age: MARKER_AGE,
        superseded: s.name == "fbrb",
        ..Rules::default()
    };
    prune_folder(top, cap_bytes, &rules).map(Option::unwrap_or_default)
}

/// [`clear`] by its rules: every entry and marker deleted, stale temporary files too.
///
/// # Errors
/// The folder's lock could not be taken.
pub fn clear_files(top: &str, suffixes: &[&str], markers: &[&str]) -> io::Result<Done> {
    if !Path::new(top).is_dir() {
        return Ok(Done::default());
    }
    crate::atomic::locked(Path::new(&join(top, &[LOCK])), || {
        let mut r = Done::default();
        let sc = scan(top, suffixes, markers);
        drop_stale(&sc.temps, &[], now(), 0.0, &mut r);
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
}

/// Every entry, temporary file and marker of a cache deleted (under its lock); entries in use kept.
///
/// # Errors
/// The folder's lock could not be taken.
pub fn clear(top: &str, s: &Spec) -> io::Result<Done> {
    clear_files(top, s.suffixes, s.markers)
}

/// A cache's (bytes of entries, entries, bytes of temporary files and markers).
#[must_use]
pub fn usage(top: &str, s: &Spec) -> (u64, usize, u64) {
    if !Path::new(top).is_dir() {
        return (0, 0, 0);
    }
    let sc = scan(top, s.suffixes, s.markers);
    let sum = |v: &[Seen]| v.iter().map(|f| f.size).sum::<u64>();
    (sum(&sc.entries), sc.entries.len(), sum(&sc.temps) + sum(&sc.markers))
}

/// Every cache the overall budget counts, with its folder (one that is there; a folder named twice counted once).
#[must_use]
pub fn members(opfor: Option<&str>) -> Vec<(&'static Spec, String)> {
    let mut out = Vec::new();
    let mut keys = HashSet::new();
    for s in SPECS {
        let Some(top) = folder(s, opfor) else { continue };
        if !Path::new(&top).is_dir() {
            continue;
        }
        if keys.insert(normcase(&absolute(&top))) {
            out.push((s, top));
        }
    }
    out
}

/// Every cache's entries together brought under `cap_bytes`, the least recently used of any first (under the
/// cache root's lock; its stamp touched): what was done in each, in the members' order.
///
/// # Errors
/// The cache root cannot be made, or its lock taken.
pub fn prune_total(cap_bytes: u64, opfor: Option<&str>) -> io::Result<Vec<(&'static str, Done)>> {
    let root = cache_root();
    fs::create_dir_all(&root)?;
    crate::atomic::locked(Path::new(&join(&root, &[TOTAL_LOCK])), || {
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
            if let Some(r) = out.get_mut(*k).map(|o| &mut o.1) {
                take(f, &mut total, r);
            }
        }
        for (f, k) in &files {
            if Path::new(&f.path).exists()
                && let Some(o) = out.get_mut(*k)
            {
                o.1.kept_bytes += f.size;
                o.1.kept_files += 1;
            }
        }
        let _ = touch_marker(&join(&root, &[TOTAL_STAMP]));
        out
    })
}

/// [`prune_total`] under [`total_cap`] unless the cache root's stamp says it ran in the last [`PRUNE_EVERY`]
/// seconds (`None` then). Never fails: a cache that cannot be pruned is only bigger.
#[must_use]
pub fn maybe_prune_total(opfor: Option<&str>) -> Option<Vec<(&'static str, Done)>> {
    let stamp = join(&cache_root(), &[TOTAL_STAMP]);
    if stat(&stamp).is_ok_and(|st| now() - st.mtime < PRUNE_EVERY) {
        return None;
    }
    prune_total(total_cap(), opfor).ok()
}

/// One cache folder pruned to its cap unless its stamp says it was in the last [`PRUNE_EVERY`] seconds. Never
/// fails.
pub fn maybe_prune(s: &Spec, top: &str) {
    let rules = Rules {
        suffixes: s.suffixes,
        markers: s.markers,
        marker_age: MARKER_AGE,
        superseded: s.name == "fbrb",
        unless_within: Some(PRUNE_EVERY),
        ..Rules::default()
    };
    let _ = prune_folder(top, cap(s), &rules);
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn temp(name: &str) -> String {
        let d = std::env::temp_dir().join(format!("bc2-gamefs-budget-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d.to_string_lossy().into_owned()
    }

    #[test]
    fn the_table() {
        let names: Vec<&str> = SPECS.iter().map(|s| s.name).collect();
        assert_eq!(names.len(), 13);
        assert_eq!(spec("fbrb").unwrap().markers, [".seen"]);
        assert!(spec("hlsl").unwrap().off_without_localappdata);
        assert_eq!(spec("meshview").unwrap().project, ["work", "tools_out"]);
        assert_eq!(join("C:", &["x"]), "C:x");
        assert!(is_temp("a.pkl.tmp-12-3") && is_temp("a.tmp") && !is_temp("a.pkl"));
        assert_eq!(
            folder(spec("navstudio").unwrap(), Some("O")),
            Some(join("O", &["work", "navstudio", "cache"]))
        );
        assert_eq!(folder(spec("navstudio").unwrap(), None), None);
    }

    #[test]
    fn least_recently_used_first_under_the_cap() {
        let top = temp("lru");
        let s = spec("culltree").unwrap();
        for (i, n) in ["a.pkl", "b.pkl", "c.pkl"].iter().enumerate() {
            let p = join(&top, &[n]);
            fs::write(&p, vec![0u8; 100]).unwrap();
            let t = UNIX_EPOCH + std::time::Duration::from_secs(1_000_000 + i as u64 * 1000);
            File::options()
                .write(true)
                .open(&p)
                .unwrap()
                .set_times(FileTimes::new().set_accessed(t).set_modified(t))
                .unwrap();
        }
        fs::write(join(&top, &["x.pkl.tmp-1-1"]), b"t").unwrap();
        fs::write(join(&top, &["other.txt"]), b"o").unwrap();
        assert_eq!(usage(&top, s), (300, 3, 1));
        let r = prune(&top, 150, s).unwrap();
        assert_eq!(
            (r.deleted_files, r.deleted_bytes, r.kept_files, r.kept_bytes),
            (2, 200, 1, 100)
        );
        assert!(Path::new(&join(&top, &["c.pkl"])).exists() && !Path::new(&join(&top, &["a.pkl"])).exists());
        assert!(Path::new(&join(&top, &[STAMP])).exists());
        let c = clear(&top, s).unwrap();
        assert_eq!((c.deleted_files, c.kept_files), (1, 0));
        assert!(Path::new(&join(&top, &["other.txt"])).exists());
        let _ = fs::remove_dir_all(&top);
    }
}
