//! `pyos::files` (round O5: round O3's `fsx.rs`, folded into round O4's `pyos`): the file system as the suite's Python sees it through `os` and `bfbc2core.atomicio` (round O3): a file's `stat`
//! (`st_size`, `st_mtime_ns`, the float `st_mtime` / `st_atime`), `os.utime`, `os.remove` that tells "gone" from
//! "in use", `atomicio.locked` (the same lock between Python and Rust processes) and `atomicio.replace`'s tries.

use std::io;

/// `os.stat` / `os.scandir` / `os.utime` / `os.remove` and `caches._delete_if_unchanged` as the suite's Python sees
/// them: the caches' budget's own ([`bc2_gamefs::budget`], round PR1b-2: one implementation for the budget and these
/// command lines).
pub use bc2_gamefs::budget::{Stat, delete_if_unchanged, now, scandir, stat, utime_now};

/// `atomicio.locked(path)` ([`bc2_gamefs::atomic::locked`]).
pub fn locked<T>(path: &str, f: impl FnOnce() -> T) -> io::Result<T> {
    bc2_gamefs::atomic::locked(std::path::Path::new(path), f)
}

/// `atomicio.replace(tmp, path)` ([`bc2_gamefs::atomic::replace`]: 11 tries, waits growing by 50 ms, while another
/// process holds the target for a moment).
pub fn replace(tmp: &str, path: &str) -> io::Result<()> {
    bc2_gamefs::atomic::replace(std::path::Path::new(tmp), std::path::Path::new(path))
}

/// `atomicio.write_bytes(path, data)` ([`bc2_gamefs::atomic::write`]): beside the target as `<path>.tmp-<pid>-<n>`,
/// flushed to disk, moved over it. The error names the temporary file when it could not be written, else the target.
pub fn write_atomic(path: &str, data: &[u8]) -> Result<(), (String, io::Error)> {
    bc2_gamefs::atomic::write(std::path::Path::new(path), &[data]).map_err(|e| match e {
        bc2_gamefs::atomic::WriteError::Temp(tmp, e) => (tmp.to_string_lossy().into_owned(), e),
        bc2_gamefs::atomic::WriteError::Replace(e) => (path.to_owned(), e),
    })
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
