//! Whole-file writes that never leave a half-written target, the suite's one implementation (FbrbTool / DbxEdit's
//! `AtomicFile`, `bfbc2core.atomicio`, the ledger, the caches, the editor's saves all call it): the data goes to a
//! temporary file beside the target ([`temp_name`]: `<target>.tmp-<process>-<n>`, unique per process and per
//! write, so two threads or two processes writing one path never share a temporary file), is flushed to disk
//! (`sync_all`; [`write_rebuildable`] leaves that out for outputs rebuilt from their inputs, such as an unpacked
//! bundle's files), and is moved over the target ([`replace`]). A write cut short leaves the old file as it was and
//! at most a stray `*.tmp-*` (the cache prunes treat those as a dead writer's; [`remove_dead_temps`] clears a killed
//! writer's beside one file). [`locked`] is the lock between processes the same writers share.
//!
//! On Windows the move fails while another process has the target open without delete sharing - for a moment,
//! typically the virus scanner or the search indexer reading a file just written. That stopped a whole WW2 rebuild
//! once (DbxEdit, 2026-11-17: "Unable to remove the file to be replaced"), so the move is tried again for up to about
//! three seconds before the error is returned.

use std::ffi::OsString;
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

static WRITES: AtomicU64 = AtomicU64::new(0);

/// A fresh temporary name beside `target`: `<target>.tmp-<process id>-<n>`, `n` counting this process's writes.
#[must_use]
pub fn temp_name(target: &Path) -> PathBuf {
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let mut s: OsString = target.as_os_str().to_owned();
    s.push(format!(".tmp-{}-{n}", std::process::id()));
    PathBuf::from(s)
}

/// Where an atomic write failed.
#[derive(Debug)]
pub enum WriteError {
    /// Making or writing the temporary file (its path given; the target is untouched).
    Temp(PathBuf, io::Error),
    /// Moving the finished temporary file over the target (the temporary file is removed).
    Replace(io::Error),
}

impl WriteError {
    /// The I/O error, wherever it happened.
    #[must_use]
    pub fn io(&self) -> &io::Error {
        match self {
            Self::Temp(_, e) | Self::Replace(e) => e,
        }
    }
}

impl fmt::Display for WriteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Temp(p, e) => write!(f, "{}: {e}", p.display()),
            Self::Replace(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for WriteError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.io())
    }
}

impl From<WriteError> for io::Error {
    fn from(e: WriteError) -> Self {
        match e {
            WriteError::Temp(_, e) | WriteError::Replace(e) => e,
        }
    }
}

/// `parts` (one after another) written to `target` atomically: a [`temp_name`] beside it, `sync_all`, then
/// [`replace`]. The target's folder must exist (callers that make it do so first, as each did before).
///
/// # Errors
/// [`WriteError::Temp`] when the temporary file cannot be made or written, [`WriteError::Replace`] when it cannot be
/// moved; the temporary file is removed either way.
pub fn write(target: &Path, parts: &[&[u8]]) -> Result<(), WriteError> {
    write_parts(target, parts, true)
}

/// [`write`] without the flush to disk, for outputs rebuilt from their inputs when lost - an unpacked bundle's
/// thousands of files (FbrbTool's `unpack`), where `sync_all` costs about a millisecond a file. Still whole or not at
/// all for every reader: the move happens only after the data is written.
///
/// # Errors
/// As [`write`].
pub fn write_rebuildable(target: &Path, parts: &[&[u8]]) -> Result<(), WriteError> {
    write_parts(target, parts, false)
}

fn write_parts(target: &Path, parts: &[&[u8]], sync: bool) -> Result<(), WriteError> {
    fill_and_move(
        target,
        |f| {
            for p in parts {
                f.write_all(p)?;
            }
            Ok(())
        },
        sync,
    )
}

/// [`write`] with the temporary file filled by `fill` (a streamed writer: a bundle, a zip); `fill`'s value returned.
///
/// # Errors
/// As [`write`]; an error `fill` returns is a [`WriteError::Temp`].
pub fn write_with<T>(target: &Path, fill: impl FnOnce(&mut fs::File) -> io::Result<T>) -> Result<T, WriteError> {
    fill_and_move(target, fill, true)
}

fn fill_and_move<T>(
    target: &Path,
    fill: impl FnOnce(&mut fs::File) -> io::Result<T>,
    sync: bool,
) -> Result<T, WriteError> {
    let tmp = temp_name(target);
    let filled = fs::File::create(&tmp).and_then(|mut f| {
        let v = fill(&mut f)?;
        f.flush()?;
        if sync {
            f.sync_all()?;
        }
        Ok(v)
    });
    let v = match filled {
        Ok(v) => v,
        Err(e) => {
            let _ = fs::remove_file(&tmp);
            return Err(WriteError::Temp(tmp, e));
        }
    };
    if let Err(e) = replace(&tmp, target) {
        let _ = fs::remove_file(&tmp);
        return Err(WriteError::Replace(e));
    }
    Ok(v)
}

/// The writing process of a temporary file named for `target` (`<target>.tmp-<pid>-<n>` or the older
/// `<target>.tmp-<pid>`), None for any other name.
#[must_use]
pub fn temp_writer(target_name: &str, name: &str) -> Option<u32> {
    let rest = name.strip_prefix(target_name)?.strip_prefix(".tmp-")?;
    let (pid, n) = rest.split_once('-').unwrap_or((rest, "0"));
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if digits(pid) && digits(n) {
        pid.parse().ok()
    } else {
        None
    }
}

/// Removes the temporary files beside `target` ([`temp_name`]'s) whose writing process `alive` says has ended: what
/// a write cut short by a killed process left (a transaction's recovery calls it for each file it restores). Names
/// match without case, as Windows does. Returns how many were removed.
pub fn remove_dead_temps(target: &Path, alive: impl Fn(u32) -> bool) -> usize {
    let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
        return 0;
    };
    let dir = if dir.as_os_str().is_empty() {
        Path::new(".")
    } else {
        dir
    };
    let low = name.to_string_lossy().to_lowercase();
    let Ok(rd) = fs::read_dir(dir) else {
        return 0;
    };
    let mut n = 0;
    for e in rd.flatten() {
        let f = e.file_name().to_string_lossy().to_lowercase();
        if let Some(pid) = temp_writer(&low, &f)
            && !alive(pid)
            && fs::remove_file(e.path()).is_ok()
        {
            n += 1;
        }
    }
    n
}

/// `f` run while holding the lock between processes on the lock file `path` (made with its folder when missing,
/// kept afterwards). The operating system drops the lock when its process ends, so a killed holder never leaves it
/// stuck; Python's `atomicio.locked` (`msvcrt.locking` on its first byte) and this whole-file lock exclude each other.
///
/// # Errors
/// The lock file cannot be made, opened or locked.
pub fn locked<T>(path: &Path, f: impl FnOnce() -> T) -> io::Result<T> {
    if let Some(dir) = path.parent()
        && !dir.as_os_str().is_empty()
    {
        fs::create_dir_all(dir)?;
    }
    let file = fs::OpenOptions::new()
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

/// How many times [`replace`] tries the move; the waits between tries grow by 50 ms each (about 2.75 s in all).
pub const TRIES: u32 = 11;

/// Whether an error is a moment's lock by another process (Windows: access denied, sharing or lock violation).
#[must_use]
pub fn is_transient(e: &io::Error) -> bool {
    e.kind() == io::ErrorKind::PermissionDenied || matches!(e.raw_os_error(), Some(5 | 32 | 33))
}

/// Moves `tmp` over `target` (replacing it), trying again while the error is [`is_transient`].
///
/// # Errors
/// The last error once the tries are spent, or the first error that is not transient. `tmp` is left for the caller
/// to remove.
pub fn replace(tmp: &Path, target: &Path) -> io::Result<()> {
    replace_with(tmp, target, |a, b| fs::rename(a, b))
}

/// [`replace`] with the move given (for tests).
///
/// # Errors
/// As [`replace`].
pub fn replace_with(tmp: &Path, target: &Path, mut mv: impl FnMut(&Path, &Path) -> io::Result<()>) -> io::Result<()> {
    let mut n = 1;
    loop {
        match mv(tmp, target) {
            Err(e) if n < TRIES && is_transient(&e) => {
                thread::sleep(Duration::from_millis(50 * u64::from(n)));
                n += 1;
            }
            r => return r,
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_moments_lock_is_waited_out() {
        let mut calls = 0;
        let r = replace_with(Path::new("a"), Path::new("b"), |_, _| {
            calls += 1;
            if calls < 3 {
                Err(io::Error::from_raw_os_error(32))
            } else {
                Ok(())
            }
        });
        assert!(r.is_ok());
        assert_eq!(calls, 3);
    }

    #[test]
    fn a_lasting_lock_is_returned() {
        let mut calls = 0;
        let r = replace_with(Path::new("a"), Path::new("b"), |_, _| {
            calls += 1;
            Err(io::Error::from(io::ErrorKind::PermissionDenied))
        });
        assert!(r.is_err());
        assert_eq!(calls, TRIES);
    }

    #[test]
    fn other_errors_are_not_retried() {
        let mut calls = 0;
        let r = replace_with(Path::new("a"), Path::new("b"), |_, _| {
            calls += 1;
            Err(io::Error::from(io::ErrorKind::NotFound))
        });
        assert!(r.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn a_real_file_is_replaced() {
        let d = std::env::temp_dir().join(format!("bc2_atomic_{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let (tmp, target) = (d.join("t.tmp"), d.join("t"));
        fs::write(&target, b"old").unwrap();
        fs::write(&tmp, b"new").unwrap();
        replace(&tmp, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!tmp.exists());
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn temp_names_differ_per_write() {
        let t = Path::new("x.bin");
        let (a, b) = (temp_name(t), temp_name(t));
        assert_ne!(a, b);
        let s = a.to_string_lossy().into_owned();
        assert!(s.starts_with(&format!("x.bin.tmp-{}-", std::process::id())), "{s}");
    }

    #[test]
    fn write_replaces_whole_and_leaves_no_temp() {
        let d = std::env::temp_dir().join(format!("bc2_atomic_w_{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let target = d.join("t");
        fs::write(&target, b"old").unwrap();
        write(&target, &[b"ne", b"w"]).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
        write_rebuildable(&target, &[b"newer"]).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"newer");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
        write(&target, &[b"new"]).unwrap();
        // a fill that fails leaves the old file and no temporary file
        let r = write_with(&target, |f| {
            f.write_all(b"half")?;
            Err::<(), _>(io::Error::other("cut short"))
        });
        assert!(matches!(r, Err(WriteError::Temp(..))));
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn threads_writing_one_path_never_share_a_temp() {
        let d = std::env::temp_dir().join(format!("bc2_atomic_t_{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        let target = d.join("t");
        thread::scope(|s| {
            for i in 0..4u8 {
                let target = &target;
                s.spawn(move || {
                    for _ in 0..10 {
                        write(target, &[&[i; 4096]]).unwrap();
                    }
                });
            }
        });
        let got = fs::read(&target).unwrap();
        assert_eq!(got.len(), 4096);
        assert!(got.iter().all(|&b| Some(&b) == got.first()));
        assert_eq!(fs::read_dir(&d).unwrap().count(), 1);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn dead_writers_temps_are_found_by_name() {
        assert_eq!(temp_writer("a.dbx", "a.dbx.tmp-12-3"), Some(12));
        assert_eq!(temp_writer("a.dbx", "a.dbx.tmp-12"), Some(12));
        assert_eq!(temp_writer("a.dbx", "a.dbx.tmp-x-3"), None);
        assert_eq!(temp_writer("a.dbx", "a.dbx.opfor-orig"), None);
        assert_eq!(temp_writer("a.dbx", "a.dbx2.tmp-1-1"), None);
        let d = std::env::temp_dir().join(format!("bc2_atomic_d_{}", std::process::id()));
        fs::create_dir_all(&d).unwrap();
        for n in ["t", "T.tmp-1-0", "t.tmp-2-5", "t.tmp-3", "t.tmp-x", "u.tmp-1-0"] {
            fs::write(d.join(n), b"").unwrap();
        }
        assert_eq!(remove_dead_temps(&d.join("t"), |pid| pid == 2), 2);
        let mut left: Vec<String> = fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["t", "t.tmp-2-5", "t.tmp-x", "u.tmp-1-0"]);
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn locked_makes_its_folder() {
        let d = std::env::temp_dir().join(format!("bc2_atomic_l_{}", std::process::id()));
        let lock = d.join("sub").join("x.lock");
        assert_eq!(locked(&lock, || 7).unwrap(), 7);
        assert!(lock.exists());
        fs::remove_dir_all(&d).unwrap();
    }
}
