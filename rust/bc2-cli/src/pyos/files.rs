//! `pyos::files` (round O5: round O3's `fsx.rs`, folded into round O4's `pyos`): the file system as the suite's Python sees it through `os` and `bfbc2core.atomicio` (round O3): a file's `stat`
//! (`st_size`, `st_mtime_ns`, the float `st_mtime` / `st_atime`), `os.utime`, `os.remove` that tells "gone" from
//! "in use", `atomicio.locked` (the same lock between Python and Rust processes) and `atomicio.replace`'s tries.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// `os.stat` / `DirEntry.stat()`'s fields the caches use.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stat {
    pub size: u64,
    /// `st_mtime_ns`
    pub mtime_ns: i128,
    /// `st_mtime`, `st_atime`: seconds as Python's floats (`sec + nsec * 1e-9`)
    pub mtime: f64,
    pub atime: f64,
}

/// (seconds, nanoseconds) the a time as Python's `FILE_TIME_to_time_t_nsec` splits it.
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
    pub fn the(m: &fs::Metadata) -> Self {
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

/// `os.stat(path)` (links followed).
pub fn stat(path: &str) -> io::Result<Stat> {
    fs::metadata(path).map(|m| Stat::the(&m))
}

/// `time.time()`.
pub fn now() -> f64 {
    float_time(SystemTime::now())
}

/// `os.utime(path)`: its access and modification times set to now.
pub fn utime_now(path: &str) -> io::Result<()> {
    let now = SystemTime::now();
    let f = open_attrs(path)?;
    f.set_times(FileTimes::new().set_accessed(now).set_modified(now))
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

/// `with open(path, 'wb'): pass` then `os.utime(path)`: an empty marker file stamped now.
pub fn touch_marker(path: &str) -> io::Result<()> {
    File::create(path)?;
    utime_now(path)
}

/// `os.remove(path)`: Ok(true) removed, Ok(false) gone already (`FileNotFoundError`), Err otherwise (in use without
/// delete sharing, ...).
pub fn remove(path: &str) -> io::Result<bool> {
    match fs::remove_file(path) {
        Ok(()) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// `caches._delete_if_unchanged`: deletes the file if it still has the size and time the scan saw (any time when
/// not `check`): Some(true) deleted, Some(false) kept (in use, or changed), None gone before.
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

/// `atomicio.locked(path)`: `f` run while holding the lock between processes on the lock file (created when missing,
/// kept afterwards; Python's `msvcrt.locking` the its first byte and this whole-file lock exclude each other).
pub fn locked<T>(path: &str, f: impl FnOnce() -> T) -> io::Result<T> {
    if let Some(dir) = std::path::Path::new(path).parent()
        && !dir.as_os_str().is_empty()
    {
        fs::create_dir_all(dir)?;
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    file.lock()?;
    let out = f();
    let _ = file.unlock();
    Ok(out)
}

/// `atomicio._transient`: a moment's lock by another process (access denied, sharing or lock violation).
fn transient(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// `atomicio.replace(tmp, path)`: `os.replace`, tried again (11 tries, waits growing by 50 ms) while another process
/// holds the target for a moment.
pub fn replace(tmp: &str, path: &str) -> io::Result<()> {
    let mut n = 1u64;
    loop {
        match fs::rename(tmp, path) {
            Ok(()) => return Ok(()),
            Err(e) if n < 11 && transient(&e) => {
                std::thread::sleep(Duration::from_millis(50 * n));
                n += 1;
            }
            Err(e) => return Err(e),
        }
    }
}

/// A directory entry as `os.scandir` gives it: its path (`os.path.join(top, name)`), name, kind and stat (the
/// directory listing's own data, links not followed).
pub struct Entry {
    pub path: String,
    pub name: String,
    /// a symbolic link or junction (`is_symlink() or is_junction()`): never followed
    pub link: bool,
    pub dir: bool,
    pub file: bool,
    pub stat: Option<Stat>,
}

/// `list(os.scandir(top))` in the listing's order; Err when the folder cannot be listed.
pub fn scandir(top: &str) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    for e in fs::read_dir(top)? {
        let Ok(e) = e else { continue };
        let name = e.file_name().to_string_lossy().into_owned();
        let path = bc2_pyrt::py::join(top, &[&name]);
        let (link, dir, file) = match e.file_type() {
            Ok(t) => (t.is_symlink(), t.is_dir(), t.is_file()),
            Err(_) => (false, false, false),
        };
        let stat = e.metadata().ok().map(|m| Stat::the(&m));
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

/// `os.path.normcase` (Windows): lower case, `/` as `\` - pyos's own (round O5: the two copies were the same).
pub use super::normcase;

/// `os.path.normpath` (Windows): separators as `\`, repeated ones and `.` parts dropped, `..` taking out the part
/// before it (never above the drive's root).
pub fn normpath(p: &str) -> String {
    if p.is_empty() {
        return ".".to_owned();
    }
    let p = p.replace('/', "\\");
    let d = bc2_pyrt::py::drive_len(&p);
    let (drive, rest) = p.split_at(d);
    let root = rest.starts_with('\\');
    let mut parts: Vec<&str> = Vec::new();
    for c in rest.split('\\') {
        match c {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !root {
                    parts.push("..");
                }
            }
            c => parts.push(c),
        }
    }
    let mut out = String::from(drive);
    if root {
        out.push('\\');
    }
    out.push_str(&parts.join("\\"));
    if out.is_empty() { ".".to_owned() } else { out }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normpaths_as_ntpath() {
        assert_eq!(normpath("C:/a//b/./c/../d"), "C:\\a\\b\\d");
        assert_eq!(normpath("a\\..\\..\\b"), "..\\b");
        assert_eq!(normpath("C:\\..\\x"), "C:\\x");
        assert_eq!(normpath(""), ".");
        assert_eq!(normpath("C:"), "C:");
        assert_eq!(normcase("C:/A/b"), "c:\\a\\b");
    }
}
