//! Moving a finished temporary file over its target, as the suite's writers do (FbrbTool / DbxEdit's `AtomicFile`,
//! `bfbc2core.atomicio`): a write cut short never leaves a truncated file where the old one was.
//!
//! On Windows the move fails while another process has the target open without delete sharing - for a moment,
//! typically the virus scanner or the search indexer reading a file just written. That stopped a whole WW2 rebuild
//! once (DbxEdit, 2026-11-17: "Unable to remove the file to be replaced"), so the move is tried again for up to about
//! three seconds before the error is returned.

use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

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
}
