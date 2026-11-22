//! The WW2 rebuild's lock (Project OPFOR's `rebuild_all.locked`, round OR0: `work\ww2\_rebuild\rebuild.lock`), taken
//! by the install too (round OR1): an install reads the build's outputs, so it must not run while a rebuild writes
//! them, nor a rebuild start while an install runs.
//!
//! The lock file is JSON: `pid`, `host`, `started`, `checkout`, `command` (as `rebuild_all` writes it) and, when OPFOR's
//! Python driver holds it for its run, a `token` the driver hands to `bfbc2 install` (`--lock-token`), which then
//! runs under that lock instead of taking its own. A lock whose process is gone (on this machine) is taken over with
//! a note; any other is refused in words.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use bc2_formats::gameledger::{host_name, pid_alive};
use serde_json::{Value, json};

/// The lock's file name in `work\ww2\_rebuild`.
pub const LOCK: &str = "rebuild.lock";

/// A held lock: removed when dropped (only when this process took it).
#[derive(Debug)]
pub struct Held {
    path: Option<PathBuf>,
    /// A stale lock taken over: the line to print.
    pub note: Option<String>,
}

impl Drop for Held {
    fn drop(&mut self) {
        if let Some(p) = self.path.take() {
            let _ = fs::remove_file(p);
        }
    }
}

fn info(path: &Path) -> Option<Value> {
    let d = fs::read(path).ok()?;
    Some(serde_json::from_slice(&d).unwrap_or_else(|_| json!({})))
}

fn who(i: &Value) -> String {
    let g = |k: &str| {
        i.get(k).map_or_else(
            || "?".to_owned(),
            |v| v.as_str().map_or_else(|| v.to_string(), str::to_owned),
        )
    };
    format!(
        "pid {} on {} since {}, from {}: {}",
        g("pid"),
        i.get("host")
            .and_then(Value::as_str)
            .filter(|h| !h.is_empty())
            .unwrap_or("?"),
        g("started"),
        g("checkout"),
        g("command")
    )
}

fn now_local() -> String {
    // the lock's start time is for people; the ledger's UTC clock is the one the suite has without a time crate
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    format!("{secs} (unix seconds)")
}

/// Takes the lock at `path` for this process (or, with `token`, checks that the lock there is the one OPFOR's driver
/// took for this run). `checkout` and `command` describe the run.
///
/// # Errors
/// Another live run holds it (exit 3 in words), or the file cannot be written.
pub fn take(path: &Path, token: Option<&str>, checkout: &str, command: &str) -> Result<Held, String> {
    if let Some(t) = token {
        return match info(path) {
            Some(i) if i.get("token").and_then(Value::as_str) == Some(t) => Ok(Held { path: None, note: None }),
            Some(i) => Err(busy(path, &i)),
            None => Err(format!(
                "rebuild lock {}: --lock-token given but no lock there (the driver's run ended?) - nothing done",
                path.display()
            )),
        };
    }
    if let Some(d) = path.parent() {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let mut note = None;
    for _ in 0..2 {
        match fs::OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(mut f) => {
                let body = json!({ "pid": std::process::id(), "host": host_name().unwrap_or_default(),
                                   "started": now_local(), "checkout": checkout, "command": command });
                if let Err(e) = f.write_all(body.to_string().as_bytes()) {
                    drop(f);
                    let _ = fs::remove_file(path);
                    return Err(format!("{}: {e}", path.display()));
                }
                return Ok(Held {
                    path: Some(path.to_path_buf()),
                    note,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                let i = info(path).unwrap_or_else(|| json!({}));
                let pid = i.get("pid").and_then(Value::as_i64);
                let same_host = match (i.get("host").and_then(Value::as_str), host_name()) {
                    (Some(h), Some(me)) => h.eq_ignore_ascii_case(&me),
                    _ => false,
                };
                if let Some(p) = pid
                    && same_host
                    && !pid_alive(p)
                {
                    note = Some(format!(
                        "rebuild lock {}: left by a rebuild that is no longer running ({}) - taken over",
                        path.display(),
                        who(&i)
                    ));
                    let _ = fs::remove_file(path);
                    continue;
                }
                return Err(busy(path, &i));
            }
            Err(e) => return Err(format!("{}: {e}", path.display())),
        }
    }
    Err(format!("rebuild lock {}: could not take it", path.display()))
}

fn busy(path: &Path, i: &Value) -> String {
    format!(
        "Another WW2 rebuild or install is running ({}). An install reads the build's outputs and a rebuild writes \
         them, so this install does not start. Wait for it to finish; if none is running (a process on another \
         machine, or an unreadable lock), delete {}.",
        who(i),
        path.display()
    )
}
