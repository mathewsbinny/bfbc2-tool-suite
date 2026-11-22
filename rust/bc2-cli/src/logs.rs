//! The log folder's pruning and archive as `bfbc2core.caches` keeps them (round O3): `prune_logs` (each tool's 20
//! newest days, then the oldest files beyond the per-tool cap and the overall cap; never the session's own files nor,
//! for the caps, a file written in the last 10 minutes), `_archive_logs` (the files a prune removes zipped into
//! `<logs>-archive\logs-<time>.zip` first, the archives kept under their own cap), `clear_logs` and `logs_usage`.
//! Every Python tool prunes the folder at start-up (`errors.setup`); `bfbc2.exe` does the same for the tools it runs
//! (`pyproc`), and `bfbc2 caches` for its `--prune` / `--clear` of `logs`.
//!
//! The archive's members are the files' bytes, names and times as Python's `zipfile` writes them (deflated); the
//! compressed bytes are zlib-rs' rather than zlib's (both inflate to the same files).

use std::collections::{HashMap, HashSet};
use std::io::{self, Write as _};

use crate::pylog::{self, LOG_NAME};
use crate::pyos::files::{self as fsx, Stat};

const MB: f64 = 1_048_576.0;
/// `caches.LOCK`
pub const LOCK: &str = ".prune.lock";
/// `LOG_KEEP_DAYS`, `LOG_RECENT`
pub const LOG_KEEP_DAYS: usize = 20;
pub const LOG_RECENT: f64 = 600.0;
pub const LOGS_ENV: &str = "BFBC2_LOGS_MB";
pub const LOG_TOOL_ENV: &str = "BFBC2_LOG_TOOL_MB";
pub const LOG_FILE_ENV: &str = "BFBC2_LOG_FILE_MB";
const LOG_ARCHIVE_ENV: &str = "BFBC2_LOG_ARCHIVE_MB";

/// `caches.Result`: what a prune or clear did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Done {
    pub kept_bytes: u64,
    pub kept_files: u64,
    pub deleted_bytes: u64,
    pub deleted_files: u64,
    pub busy: u64,
    pub stale: u64,
}

/// `caches._File`: a file as the scan saw it.
#[derive(Debug, Clone)]
pub struct Seen {
    pub path: String,
    pub size: u64,
    pub mtime_ns: i128,
    /// `_File.used`: the later of its access and modification times (logs: the modification time)
    pub used: f64,
}

impl Seen {
    fn from(path: String, st: &Stat, used: f64) -> Self {
        Self {
            path,
            size: st.size,
            mtime_ns: st.mtime_ns,
            used,
        }
    }

    /// `_delete_if_unchanged(f)`.
    pub fn delete_if_unchanged(&self, check: bool) -> Option<bool> {
        fsx::delete_if_unchanged(&self.path, self.size, self.mtime_ns, check)
    }
}

/// `log_caps()`: (all logs, one tool's, one file's) caps in bytes.
pub fn log_caps() -> (u64, u64, u64) {
    let c = |env: &str, d: f64| (pylog::number(env, d) * MB) as u64;
    (c(LOGS_ENV, 200.0), c(LOG_TOOL_ENV, 100.0), c(LOG_FILE_ENV, 20.0))
}

/// `_log_files(top)`: (file, tool, date) of every log in top (not below it; links and other files left out).
pub fn log_files(top: &str) -> Vec<(Seen, String, String)> {
    let mut out = Vec::new();
    let (Ok(entries), Some(re)) = (fsx::scandir(top), LOG_NAME.as_ref()) else {
        return out;
    };
    for e in entries {
        let Some(m) = re.captures(&e.name) else { continue };
        if e.link || !e.file {
            continue;
        }
        let Some(st) = e.stat else { continue };
        let tool = m.name("tool").map_or("", |t| t.as_str()).to_owned();
        let date = m.name("date").map_or("", |t| t.as_str()).to_owned();
        out.push((Seen::from(e.path, &st, st.mtime), tool, date));
    }
    out
}

/// `logs_usage(top)`: (bytes, files) of the logs.
pub fn logs_usage(top: &str) -> (u64, usize) {
    let files = log_files(top);
    (files.iter().map(|f| f.0.size).sum(), files.len())
}

/// `os.path.normcase(os.path.abspath(p))`.
fn key(p: &str) -> String {
    fsx::normcase(&bc2_pyrt::py::abspath(p))
}

/// `prune_logs(top, keep, caps=caps)` (`clear_logs`: caps (0, 0, 0)). Err: the folder's lock or the archive could not
/// be written (nothing deleted then), as the Python raises its `OSError`.
#[allow(clippy::too_many_lines, reason = "the reference's one function, kept in one piece")]
pub fn prune_logs(top: &str, keep: &[String], caps: Option<(u64, u64, u64)>) -> io::Result<Done> {
    let mut r = Done::default();
    if !bc2_pyrt::py::isdir(top) {
        return Ok(r);
    }
    let (total_cap, tool_cap, _) = caps.unwrap_or_else(log_caps);
    let keep_set: HashSet<String> = keep.iter().map(|k| key(k)).collect();
    let now = fsx::now();
    let lock = bc2_pyrt::py::join(top, &[LOCK]);
    fsx::locked(&lock, || -> io::Result<Done> {
        let files = log_files(top);
        let kept: HashSet<String> = files
            .iter()
            .filter(|(f, _, _)| keep_set.contains(&key(&f.path)))
            .map(|(f, _, _)| fsx::normcase(&f.path))
            .collect();
        let mut gone: HashSet<String> = HashSet::new();
        let mut chosen: Vec<Seen> = Vec::new();
        #[allow(clippy::cast_precision_loss, reason = "the reference's mtime_ns / 1e9")]
        let mut choose = |f: &Seen, held: bool, gone: &mut HashSet<String>| -> bool {
            if kept.contains(&fsx::normcase(&f.path)) || (held && now - (f.mtime_ns as f64) / 1e9 < LOG_RECENT) {
                return false;
            }
            gone.insert(f.path.clone());
            chosen.push(f.clone());
            true
        };
        // by tool, in the order the tools first appear (a dict's)
        let mut order: Vec<&str> = Vec::new();
        let mut by_tool: HashMap<&str, Vec<(&Seen, &str)>> = HashMap::new();
        for (f, tool, date) in &files {
            if !by_tool.contains_key(tool.as_str()) {
                order.push(tool);
            }
            by_tool.entry(tool).or_default().push((f, date));
        }
        for tool in &order {
            let Some(items) = by_tool.get(tool) else { continue };
            let mut days: Vec<&str> = items
                .iter()
                .map(|(_, d)| *d)
                .collect::<HashSet<_>>()
                .into_iter()
                .collect();
            days.sort_unstable_by(|a, b| b.cmp(a));
            days.truncate(LOG_KEEP_DAYS);
            for (f, d) in items {
                if !days.contains(d) {
                    choose(f, false, &mut gone);
                }
            }
            let mut left: Vec<&Seen> = items
                .iter()
                .map(|(f, _)| *f)
                .filter(|f| !gone.contains(&f.path))
                .collect();
            left.sort_by(|a, b| (a.mtime_ns, &a.path).cmp(&(b.mtime_ns, &b.path)));
            let mut size: u64 = left.iter().map(|f| f.size).sum();
            for f in left {
                if size <= tool_cap {
                    break;
                }
                if choose(f, true, &mut gone) {
                    size -= f.size;
                }
            }
        }
        let mut left: Vec<&Seen> = files
            .iter()
            .map(|(f, _, _)| f)
            .filter(|f| !gone.contains(&f.path))
            .collect();
        left.sort_by(|a, b| (a.mtime_ns, &a.path).cmp(&(b.mtime_ns, &b.path)));
        let mut size: u64 = left.iter().map(|f| f.size).sum();
        for f in &left {
            if size <= total_cap {
                break;
            }
            if choose(f, true, &mut gone) {
                size -= f.size;
            }
        }
        // archive first, then delete: a log the caps remove is kept in the archive (never lost)
        if !chosen.is_empty() {
            archive_logs(top, &chosen)?;
        }
        let mut busy = 0;
        for f in &chosen {
            match f.delete_if_unchanged(true) {
                Some(false) => {
                    busy += 1;
                    size += f.size;
                }
                Some(true) => {
                    r.deleted_bytes += f.size;
                    r.deleted_files += 1;
                }
                None => {}
            }
        }
        r.busy = busy;
        r.kept_bytes = size;
        let in_gone = left.iter().filter(|f| gone.contains(&f.path)).count() as u64;
        r.kept_files = left.len() as u64 - in_gone + busy;
        Ok(r)
    })?
}

/// `log_archive_folder(top)`: `<logs>-archive` beside the log folder.
pub fn archive_folder(top: &str) -> String {
    format!("{}-archive", fsx::normpath(top))
}

/// `_archive_logs(top, files)`: the files zipped into a new archive (`logs-<time>.zip`, written beside its name and
/// moved in only when complete); the archives beyond `BFBC2_LOG_ARCHIVE_MB` (default 1024 MiB) deleted, the oldest
/// first, never the new one. None when there was nothing to keep.
fn archive_logs(top: &str, files: &[Seen]) -> io::Result<Option<String>> {
    let files: Vec<&Seen> = files.iter().filter(|f| bc2_pyrt::py::isfile(&f.path)).collect();
    if files.is_empty() {
        return Ok(None);
    }
    let folder = archive_folder(top);
    std::fs::create_dir_all(&folder)?;
    let t = crate::clock::now();
    let stamp = format!("{}_{:02}{:02}{:02}", t.date(), t.hour, t.minute, t.second);
    let mut path = bc2_pyrt::py::join(&folder, &[&format!("logs-{stamp}.zip")]);
    let mut n = 1;
    while std::path::Path::new(&path).exists() {
        n += 1;
        path = bc2_pyrt::py::join(&folder, &[&format!("logs-{stamp}-{n}.zip")]);
    }
    let written = (|| -> io::Result<()> {
        let mut zip = Zip::default();
        for f in &files {
            let data = std::fs::read(&f.path)?;
            let st = fsx::stat(&f.path)?;
            let name = bc2_pyrt::py::basename(&f.path);
            zip.add(&name, &data, st.mtime, readonly(&f.path));
        }
        Ok(bc2_formats::atomic::write(
            std::path::Path::new(&path),
            &[&zip.finish()],
        )?)
    })();
    written?;
    let cap = (pylog::number(LOG_ARCHIVE_ENV, 1024.0) * MB) as u64;
    let mut olds: Vec<(i128, String, u64)> = Vec::new();
    for e in fsx::scandir(&folder)? {
        if e.name.to_lowercase().ends_with(".zip")
            && e.file
            && e.path != path
            && let Some(st) = e.stat
        {
            olds.push((st.mtime_ns, e.path, st.size));
        }
    }
    let mut size = std::fs::metadata(&path)?.len() + olds.iter().map(|o| o.2).sum::<u64>();
    olds.sort();
    for (_, p, s) in olds {
        if size <= cap {
            break;
        }
        if std::fs::remove_file(&p).is_ok() {
            size -= s;
        }
    }
    Ok(Some(path))
}

fn readonly(path: &str) -> bool {
    std::fs::metadata(path).is_ok_and(|m| m.permissions().readonly())
}

/// A ZIP archive as Python's `zipfile.ZipFile(..., 'w', ZIP_DEFLATED, compresslevel=6)` writes one from files
/// (`ZipInfo.from_file`: the local time of the file's modification, its mode in the external attributes, made on
/// Windows; sizes in the local header, no data descriptor).
#[derive(Default)]
struct Zip {
    out: Vec<u8>,
    central: Vec<u8>,
    count: u16,
}

impl Zip {
    #[allow(
        clippy::similar_names,
        reason = "the zip format's own names: name_b, the name's bytes"
    )]
    fn add(&mut self, name: &str, data: &[u8], mtime: f64, readonly: bool) {
        let (date, time) = dos_time(mtime);
        let mut enc = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(6));
        let _ = enc.write_all(data);
        let packed = enc.finish().unwrap_or_default();
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let flags: u16 = if name.is_ascii() { 0 } else { 0x800 };
        let mode: u32 = 0o100_000 | if readonly { 0o444 } else { 0o666 };
        let offset = self.out.len() as u32;
        let name_b = name.as_bytes();
        let mut head = Vec::new();
        head.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for v in [20u16, flags, 8, time, date] {
            head.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc.sum(), packed.len() as u32, data.len() as u32] {
            head.extend_from_slice(&v.to_le_bytes());
        }
        head.extend_from_slice(&(name_b.len() as u16).to_le_bytes());
        head.extend_from_slice(&0u16.to_le_bytes());
        head.extend_from_slice(name_b);
        self.out.extend_from_slice(&head);
        self.out.extend_from_slice(&packed);
        let c = &mut self.central;
        c.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        for v in [20u16, 20, flags, 8, time, date] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc.sum(), packed.len() as u32, data.len() as u32] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        for v in [name_b.len() as u16, 0, 0, 0, 0] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        c.extend_from_slice(&(mode << 16).to_le_bytes());
        c.extend_from_slice(&offset.to_le_bytes());
        c.extend_from_slice(name_b);
        self.count += 1;
    }

    fn finish(mut self) -> Vec<u8> {
        let at = self.out.len() as u32;
        let size = self.central.len() as u32;
        self.out.append(&mut self.central);
        self.out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        for v in [0u16, 0, self.count, self.count] {
            self.out.extend_from_slice(&v.to_le_bytes());
        }
        self.out.extend_from_slice(&size.to_le_bytes());
        self.out.extend_from_slice(&at.to_le_bytes());
        self.out.extend_from_slice(&0u16.to_le_bytes());
        self.out
    }
}

/// (date, time) in MS-DOS form of `time.localtime(t)[:6]`.
fn dos_time(t: f64) -> (u16, u16) {
    let l = crate::clock::local_of(t);
    let date = ((l.year.saturating_sub(1980)) << 9) | (l.month << 5) | l.day;
    let time = (l.hour << 11) | (l.minute << 5) | (l.second / 2);
    (date, time)
}
