//! The game copy's install ledger (round DF2): what each tool installed into the game copy, so a revert gives back
//! exactly what was there before that tool, in any order, and an interrupted install or revert can be finished.
//!
//! The store (format `opfor-ledger/1`) is shared by every tool that writes the game copy: OPFOR's stages, the WW2
//! and map export installs, the mod manager, the suite's NavKit and stock-MP (spec `transforms/install.md`,
//! XINST-LED). Since round OR15 this is its only implementation: Project OPFOR's `opforcore.ledger` was retired and
//! `bfbc2core.gameledger` calls this one (the `check` module holds both Python readers' rules). It writes
//! **whole-file** contributions (kind `file`); records of the slot kinds, which only OPFOR's retired ledger wrote, are
//! read, kept as they are (only a contribution's `prior` / `below` is rewritten when a splice moves it) and forgotten
//! on request.
//!
//! ```text
//! <game>\opfor\ledger\<owner>.json   one record per owner (':' in the name is '~' in the file name)
//! <game>\opfor\ledger\blobs\<sha256>.bin   the bytes that were there before an owner (shared by sha)
//! <game>\opfor\ledger\.lock          {"pid", "started"} while an operation runs (created exclusively)
//! <game>\opfor\ledger\.txn.json, .txn\   the journal of the commit being written (pre-images by sha)
//! <game>\opfor\ledger\.dirty         a rollback failed: recover() is needed
//! ```
//!
//! Per game file the owners form a stack: the bottom one's `prior` is what was there before any owner (DICE's
//! original, or nothing), each one's `value` the sha256 it wrote, its `below` the owner it sits on. Install puts a new
//! layer on top (refused when the file changed outside the ledger); a re-install of the top owner keeps its prior.
//! Uninstall of the top owner writes its prior back; of a buried owner splices it out (the owner above inherits its
//! prior) - unless the owner above is `derived` (built from the file it found), then it is refused, naming what to
//! revert first. Every check comes before any write; the writes are journalled (pre-images) and each game file is
//! written beside itself, flushed to disk and moved over the old one (retried while another process holds it).

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::{atomic, sha256};

/// The record format.
pub const FORMAT: &str = "opfor-ledger/1";
/// The journal format.
pub const JOURNAL_FORMAT: &str = "opfor-ledger-txn/1";
/// The fixed temporary name older versions wrote a file to beside its target (now [`atomic::temp_name`]'s
/// per-process name): never taken for a legacy marker, and removed by a recovery as a dead writer's.
pub const TMP: &str = ".opfor-ledger-tmp";
const LOCK: &str = ".lock";
const JOURNAL: &str = ".txn.json";
const JOURNAL_DIR: &str = ".txn";
const DIRTY: &str = ".dirty";

/// The ledger folder of a game copy.
#[must_use]
pub fn default_root(game: &Path) -> PathBuf {
    game.join("opfor").join("ledger")
}

/// A ledger operation that did not happen (or did not finish).
#[derive(Debug)]
pub enum LedgerError {
    /// Checks failed: nothing was written. Each line says why, in plain words.
    Refused(Vec<String>),
    /// Another operation holds the lock (or a stale lock could not be cleared).
    Locked(String),
    /// A damaged record or journal, a bad owner or path.
    Damaged(String),
    /// A write failed; the commit was rolled back (or, the message says, could not be).
    Io(String),
    /// A test hook stopped the process at a write (no rollback, the journal and lock left as a crash leaves them).
    Crashed,
}

impl fmt::Display for LedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(c) => write!(f, "{}", c.join("; ")),
            Self::Locked(m) | Self::Damaged(m) | Self::Io(m) => f.write_str(m),
            Self::Crashed => f.write_str("stopped by the test hook"),
        }
    }
}

impl std::error::Error for LedgerError {}

/// What a test hook does at a write step.
#[derive(Debug)]
pub enum Interrupt {
    /// The write fails with this error (the commit rolls back).
    Fail(io::Error),
    /// The process "dies": the operation returns [`LedgerError::Crashed`] at once, leaving the journal and the lock.
    Crash,
}

/// A test hook: called with a label before each write step (`blob <sha>`, `game <rel>`, `remove <rel>`,
/// `record <owner>`, `journal`).
pub type Hook = Box<dyn FnMut(&str) -> Result<(), Interrupt>>;

/// What an install / uninstall / adopt did or would do.
#[derive(Debug, Default, Clone)]
pub struct Plan {
    /// The owner.
    pub owner: String,
    /// `install`, `uninstall`, `adopt`.
    pub action: String,
    /// Game files written (`write <rel>`) or removed (`remove <rel>`), and records.
    pub writes: Vec<String>,
    /// Notes in plain words (splices, unchanged files, what a recover rolled back first).
    pub notes: Vec<String>,
    /// Whether it was written (false: a dry run, or nothing to do).
    pub applied: bool,
}

/// One file a legacy install (from before the ledger) put there: adopted with the bytes that were there before it.
#[derive(Debug, Clone)]
pub struct Adopt {
    /// The game-relative path.
    pub rel: String,
    /// The file before the legacy install (its backup's bytes); None: there was none.
    pub prior: Option<Vec<u8>>,
}

/// A game-relative path with `/` separators; refuses absolute paths and `..`. Each part's trailing dots and spaces are
/// dropped, as Windows does.
///
/// # Errors
/// Not a path inside the game copy.
pub fn norm_rel(rel: &str) -> Result<String, LedgerError> {
    let r = rel.replace('\\', "/");
    let r = r.trim_matches('/');
    let bad = || LedgerError::Damaged(format!("not a path inside the game copy: {rel:?}"));
    let b = r.as_bytes();
    if r.is_empty() || rel.starts_with('/') || rel.starts_with('\\') || (b.len() >= 2 && b.get(1) == Some(&b':')) {
        return Err(bad());
    }
    let mut parts = Vec::new();
    for p in r.split('/') {
        let p = if p == "." || p == ".." {
            p
        } else {
            p.trim_end_matches([' ', '.'])
        };
        if p.is_empty() || p == "." || p == ".." {
            return Err(bad());
        }
        parts.push(p);
    }
    Ok(parts.join("/"))
}

/// Whether `owner` is a valid owner name (`[a-z0-9][a-z0-9_.-]*`, parts joined by `:`).
#[must_use]
pub fn valid_owner(owner: &str) -> bool {
    !owner.is_empty()
        && owner.split(':').all(|p| {
            let mut c = p.chars();
            c.next().is_some_and(|f| f.is_ascii_lowercase() || f.is_ascii_digit())
                && c.all(|x| x.is_ascii_lowercase() || x.is_ascii_digit() || matches!(x, '_' | '.' | '-'))
        })
}

/// A record's file name.
#[must_use]
pub fn owner_file(owner: &str) -> String {
    format!("{}.json", owner.replace(':', "~"))
}

/// The legacy install markers beside a file (`<name>.opfor-*`, from before the ledger), as suffixes (`.opfor-orig`).
#[must_use]
pub fn legacy_markers(path: &Path) -> Vec<String> {
    let (Some(dir), Some(name)) = (path.parent(), path.file_name()) else {
        return Vec::new();
    };
    let pre = format!("{}.opfor-", name.to_string_lossy().to_lowercase());
    let mut out = Vec::new();
    if let Ok(rd) = fs::read_dir(dir) {
        for e in rd.flatten() {
            let f = e.file_name().to_string_lossy().into_owned();
            let low = f.to_lowercase();
            if low.starts_with(&pre) && !low.ends_with(TMP) && e.path().is_file() {
                out.push(f.get(pre.len() - ".opfor-".len()..).unwrap_or_default().to_owned());
            }
        }
    }
    out.sort();
    out
}

// ------------------------------------------------------------------------------------------------- helpers

fn ioe(path: &Path, e: &io::Error) -> LedgerError {
    LedgerError::Io(format!("{}: {e}", path.display()))
}

/// The UTC time as `YYYY-MM-DDTHH:MM:SSZ` (and the compact form for transaction ids).
#[allow(clippy::many_single_char_names, reason = "the civil-date algorithm's own names")]
fn utc_now() -> (String, String) {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let (hour, min, sec) = (rem / 3600, rem / 60 % 60, rem % 60);
    // days to civil (Howard Hinnant's algorithm)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    (
        format!("{y:04}-{m:02}-{d:02}T{hour:02}:{min:02}:{sec:02}Z"),
        format!("{y:04}{m:02}{d:02}T{hour:02}{min:02}{sec:02}Z"),
    )
}

fn new_txn() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tag = sha256::hex(format!("{n}-{}", std::process::id()).as_bytes());
    format!("{}-{}", utc_now().1, tag.get(..8).unwrap_or_default())
}

fn read_opt(p: &Path) -> Result<Option<Vec<u8>>, LedgerError> {
    match fs::read(p) {
        Ok(d) => Ok(Some(d)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ioe(p, &e)),
    }
}

/// Retries `f` while the error is a moment's lock by another process (as [`atomic::replace`]).
fn retry(mut f: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut n = 1;
    loop {
        match f() {
            Err(e) if n < atomic::TRIES && atomic::is_transient(&e) => {
                std::thread::sleep(std::time::Duration::from_millis(50 * u64::from(n)));
                n += 1;
            }
            r => return r,
        }
    }
}

/// `data` to `path` with [`atomic::write`] (its folder made first): a per-process temporary name beside it, flushed
/// to disk, then moved over `path` (retried while the target is held). The live file is never moved aside; a
/// failure leaves it as it was. (Older versions used the fixed `<path>` + [`TMP`], which two writers of one path
/// would have shared.)
///
/// # Errors
/// The write or the move failed (the temporary file is removed).
pub fn write_atomic(path: &Path, data: &[u8]) -> io::Result<()> {
    if let Some(d) = path.parent() {
        fs::create_dir_all(d)?;
    }
    atomic::write(path, &[data]).map_err(io::Error::from)
}

fn remove_file(path: &Path) -> io::Result<()> {
    retry(|| match fs::remove_file(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        r => r,
    })
}

use crate::liveness::pid_alive;

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

/// A contribution's sha (its value's, or its prior's).
fn sha_of(v: &Value) -> Option<&str> {
    v.get("sha").and_then(Value::as_str)
}

fn derived(c: &Value) -> bool {
    c.get("extra")
        .and_then(|e| e.get("derived"))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

// ------------------------------------------------------------------------------------------------- ledger

type Records = BTreeMap<String, Value>;

/// The ledger of one game copy.
pub struct Ledger {
    /// The game copy.
    pub game: PathBuf,
    /// The store (`<game>\opfor\ledger` unless given).
    pub root: PathBuf,
    hook: Option<Hook>,
    crash_at: Option<u32>,
    steps: u32,
}

impl fmt::Debug for Ledger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Ledger")
            .field("game", &self.game)
            .field("root", &self.root)
            .finish_non_exhaustive()
    }
}

/// Where a layer sits: (owner, index of the contribution in its record).
type Layer = (String, usize);

impl Ledger {
    /// The ledger of the game copy `game` (its store `<game>\opfor\ledger`). `BC2_LEDGER_CRASH_AT=<n>` (tests) ends
    /// the process at the n-th write step, as a kill would.
    #[must_use]
    pub fn new(game: &Path) -> Self {
        Self::with_root(game, &default_root(game))
    }

    /// The ledger with its store elsewhere (tests).
    #[must_use]
    pub fn with_root(game: &Path, root: &Path) -> Self {
        let crash_at = std::env::var("BC2_LEDGER_CRASH_AT")
            .ok()
            .and_then(|v| v.trim().parse().ok());
        Self {
            game: game.to_path_buf(),
            root: root.to_path_buf(),
            hook: None,
            crash_at,
            steps: 0,
        }
    }

    /// Sets a test hook called before each write step.
    pub fn set_hook(&mut self, hook: Option<Hook>) {
        self.hook = hook;
    }

    fn step(&mut self, label: &str) -> Result<(), LedgerError> {
        self.steps += 1;
        if self.crash_at == Some(self.steps) {
            eprintln!("BC2_LEDGER_CRASH_AT={}: stopped before {label}", self.steps);
            std::process::exit(9);
        }
        if let Some(h) = self.hook.as_mut() {
            match h(label) {
                Ok(()) => {}
                Err(Interrupt::Fail(e)) => return Err(LedgerError::Io(format!("{label}: {e}"))),
                Err(Interrupt::Crash) => return Err(LedgerError::Crashed),
            }
        }
        Ok(())
    }

    /// A game file's path.
    #[must_use]
    pub fn path(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.game.clone(), |p, part| p.join(part))
    }

    /// A blob's path.
    #[must_use]
    pub fn blob_path(&self, sha: &str) -> PathBuf {
        self.root.join("blobs").join(format!("{sha}.bin"))
    }

    /// A blob's bytes, checked against its sha256.
    ///
    /// # Errors
    /// Missing, unreadable or damaged (its sha256 differs), in words.
    pub fn blob_bytes(&self, sha: &str) -> Result<Vec<u8>, String> {
        self.blob(sha)
    }

    fn blob(&self, sha: &str) -> Result<Vec<u8>, String> {
        let p = self.blob_path(sha);
        let d = fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
        if sha256::hex(&d) != sha {
            return Err(format!("{}: damaged (its sha256 differs)", p.display()));
        }
        Ok(d)
    }

    // ---------------------------------------------------------------------------------------------- records

    /// Every owner's record, by owner.
    ///
    /// # Errors
    /// A record that cannot be read ([`LedgerError::Damaged`], naming the file).
    pub fn records(&self) -> Result<Records, LedgerError> {
        let (recs, bad) = self.scan()?;
        if let Some((f, why)) = bad.iter().next() {
            return Err(self.damaged(f, why));
        }
        Ok(recs)
    }

    fn contributions(rec: &Value) -> &[Value] {
        rec.get("contributions")
            .and_then(Value::as_array)
            .map_or(&[], Vec::as_slice)
    }

    fn contribution<'a>(recs: &'a Records, l: &Layer) -> Option<&'a Value> {
        recs.get(&l.0).and_then(|r| Self::contributions(r).get(l.1))
    }

    /// The `file` layers on rel (every owner), and the other owners holding other kinds of slot in it.
    fn layers(recs: &Records, rel: &str) -> (Vec<Layer>, Vec<String>) {
        let low = rel.to_lowercase();
        let mut layers = Vec::new();
        let mut slots = Vec::new();
        for (o, r) in recs {
            for (i, c) in Self::contributions(r).iter().enumerate() {
                if s(c, "file").map(str::to_lowercase) != Some(low.clone()) {
                    continue;
                }
                if s(c, "kind") == Some("file") {
                    layers.push((o.clone(), i));
                } else if !slots.contains(o) {
                    slots.push(o.clone());
                }
            }
        }
        (layers, slots)
    }

    /// The layers bottom to top (following `below`); a layer nobody sits on is the top.
    fn stack(recs: &Records, layers: &[Layer]) -> Vec<Layer> {
        let mut out: Vec<Layer> = Vec::new();
        let mut cur: Option<String> = None;
        for _ in 0..=layers.len() {
            let next = layers.iter().find(|l| {
                Self::contribution(recs, l).and_then(|c| s(c, "below").map(str::to_owned)) == cur
                    && !out.iter().any(|o| o.0 == l.0)
            });
            match next {
                Some(l) => {
                    cur = Some(l.0.clone());
                    out.push(l.clone());
                }
                None => break,
            }
        }
        // layers the chain does not reach (a damaged stack) go on top, so nothing is ever missed
        for l in layers {
            if !out.iter().any(|o| o.0 == l.0) {
                out.push(l.clone());
            }
        }
        out
    }

    /// The owners holding rel as a whole file, bottom first.
    ///
    /// # Errors
    /// A damaged record, a bad path.
    pub fn holders(&self, rel: &str) -> Result<Vec<String>, LedgerError> {
        let rel = norm_rel(rel)?;
        let recs = self.records()?;
        let (layers, _) = Self::layers(&recs, &rel);
        Ok(Self::stack(&recs, &layers).into_iter().map(|l| l.0).collect())
    }

    /// The files an owner installed (game-relative), in its record's order.
    ///
    /// # Errors
    /// A damaged record.
    pub fn owner_files(&self, owner: &str) -> Result<Vec<String>, LedgerError> {
        let recs = self.records()?;
        Ok(recs.get(owner).map_or_else(Vec::new, |r| {
            Self::contributions(r)
                .iter()
                .filter_map(|c| s(c, "file").map(str::to_owned))
                .collect()
        }))
    }

    /// What was there before any owner installed rel: the bottom layer's prior (its blob), None when an owner put
    /// it where there was none; with no owner the file itself (None when missing). DICE's original, kept once.
    ///
    /// # Errors
    /// A damaged record, a bad path.
    pub fn original(&self, rel: &str) -> Result<Option<PathBuf>, LedgerError> {
        let rel = norm_rel(rel)?;
        let recs = self.records()?;
        let (layers, _) = Self::layers(&recs, &rel);
        let st = Self::stack(&recs, &layers);
        let Some(bottom) = st.first() else {
            let p = self.path(&rel);
            return Ok(p.is_file().then_some(p));
        };
        Ok(self.prior_path(Self::contribution(&recs, bottom)))
    }

    fn prior_path(&self, c: Option<&Value>) -> Option<PathBuf> {
        let prior = c?.get("prior")?;
        let blob = s(prior, "blob")?;
        Some(self.blob_path(blob))
    }

    /// What was there before `owner` installed rel: None when the owner does not hold it, Some(None) when there was
    /// nothing, Some(Some(blob path)) otherwise.
    ///
    /// # Errors
    /// A damaged record, a bad path.
    pub fn before(&self, owner: &str, rel: &str) -> Result<Option<Option<PathBuf>>, LedgerError> {
        let rel = norm_rel(rel)?;
        let recs = self.records()?;
        let (layers, _) = Self::layers(&recs, &rel);
        Ok(layers
            .iter()
            .find(|l| l.0 == owner)
            .map(|l| self.prior_path(Self::contribution(&recs, l))))
    }

    /// One line per contribution: `<owner> <file> <state>` (ok / buried under X / changed outside the ledger /
    /// missing), the interrupted journal first when there is one.
    ///
    /// # Errors
    /// A damaged record.
    pub fn status(&self) -> Result<Vec<String>, LedgerError> {
        let mut out = Vec::new();
        if self.root.join(JOURNAL).is_file() || self.root.join(DIRTY).is_file() {
            out.push(format!(
                "interrupted: {} - run recover",
                self.root.join(JOURNAL).display()
            ));
        }
        let recs = self.records()?;
        for (o, r) in &recs {
            for c in Self::contributions(r) {
                let file = s(c, "file").unwrap_or_default();
                let kind = s(c, "kind").unwrap_or_default();
                let state = if kind == "file" {
                    let (layers, _) = Self::layers(&recs, file);
                    let above = layers
                        .iter()
                        .find(|l| Self::contribution(&recs, l).and_then(|d| s(d, "below")) == Some(o.as_str()));
                    if let Some(a) = above {
                        format!("buried under {}", a.0)
                    } else {
                        match read_opt(&self.path(file))? {
                            None => "missing".to_owned(),
                            Some(d) if Some(sha256::hex(&d).as_str()) == c.get("value").and_then(sha_of) => {
                                "ok".to_owned()
                            }
                            Some(_) => "changed outside the ledger".to_owned(),
                        }
                    }
                } else {
                    format!("{kind} slot, kept as written (no shipped install wrote this kind)")
                };
                out.push(format!("{o} {file} {state}"));
            }
        }
        Ok(out)
    }

    // ---------------------------------------------------------------------------------------------- lock

    fn lock_info(&self) -> Option<Value> {
        let p = self.root.join(LOCK);
        let d = fs::read(&p).ok()?;
        let age = fs::metadata(&p)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .map_or(0.0, |d| d.as_secs_f64());
        let mut v: Value = serde_json::from_slice(&d).unwrap_or_else(|_| {
            String::from_utf8_lossy(&d)
                .trim()
                .parse::<i64>()
                .map_or_else(|_| json!({}), |p| json!({ "pid": p }))
        });
        if let Some(m) = v.as_object_mut() {
            m.insert("age".to_owned(), json!(age));
        } else {
            v = json!({ "age": age });
        }
        Some(v)
    }

    fn lock_stale(info: &Value) -> bool {
        let Some(pid) = info.get("pid").and_then(Value::as_i64) else {
            return info.get("age").and_then(Value::as_f64).unwrap_or(0.0) > 60.0;
        };
        if let (Some(h), Some(me)) = (s(info, "host"), crate::liveness::host_name())
            && !h.is_empty()
            && !h.eq_ignore_ascii_case(&me)
        {
            return false;
        }
        !pid_alive(pid)
    }

    /// Takes the lock; a stale one (its process gone) is cleared first, and an interrupted commit's journal rolled
    /// back (the notes say what was).
    fn lock(&mut self, notes: &mut Vec<String>) -> Result<(), LedgerError> {
        fs::create_dir_all(&self.root).map_err(|e| ioe(&self.root, &e))?;
        let p = self.root.join(LOCK);
        for _ in 0..2 {
            match fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(mut f) => {
                    let body = json!({ "pid": std::process::id(), "host": crate::liveness::host_name().unwrap_or_default(),
                                      "started": utc_now().0 });
                    if let Err(e) = f.write_all(body.to_string().as_bytes()) {
                        drop(f);
                        let _ = fs::remove_file(&p);
                        return Err(ioe(&p, &e));
                    }
                    drop(f);
                    if let Err(e) = self.recover_locked(notes) {
                        self.unlock();
                        return Err(e);
                    }
                    return Ok(());
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let info = self.lock_info().unwrap_or_else(|| json!({}));
                    if !Self::lock_stale(&info) {
                        return Err(LedgerError::Locked(format!(
                            "{}: held by pid {} since {}: another install / revert is running - try again when it \
                             has finished",
                            p.display(),
                            info.get("pid").map_or_else(|| "?".to_owned(), Value::to_string),
                            s(&info, "started").unwrap_or("?")
                        )));
                    }
                    notes.push(format!(
                        "a stale lock (pid {} is gone: an install or revert was interrupted) was cleared",
                        info.get("pid").map_or_else(|| "?".to_owned(), Value::to_string)
                    ));
                    remove_file(&p).map_err(|e| ioe(&p, &e))?;
                }
                Err(e) => return Err(ioe(&p, &e)),
            }
        }
        Err(LedgerError::Locked(format!("{}: could not be taken", p.display())))
    }

    fn unlock(&self) {
        let _ = remove_file(&self.root.join(LOCK));
    }

    // ---------------------------------------------------------------------------------------------- journal

    fn target_path(&self, base: &str, rel: &str) -> Result<PathBuf, LedgerError> {
        let rel = norm_rel(rel)?;
        let top = match base {
            "game" => &self.game,
            "ledger" => &self.root,
            _ => return Err(LedgerError::Damaged(format!("journal target base {base:?}"))),
        };
        Ok(rel.split('/').fold(top.clone(), |p, part| p.join(part)))
    }

    fn store_write(path: &Path, data: &[u8]) -> Result<(), LedgerError> {
        write_atomic(path, data).map_err(|e| ioe(path, &e))
    }

    /// The journal: every target's pre-image recorded (fsync'd) before the first write.
    fn begin(&mut self, owner: &str, action: &str, txn: &str, targets: &[(&str, String)]) -> Result<(), LedgerError> {
        self.step("journal")?;
        let d = self.root.join(JOURNAL_DIR);
        if d.is_dir() {
            fs::remove_dir_all(&d).map_err(|e| ioe(&d, &e))?;
        }
        fs::create_dir_all(&d).map_err(|e| ioe(&d, &e))?;
        let mut entries = Vec::new();
        let mut seen = BTreeSet::new();
        for (base, rel) in targets {
            if !seen.insert((*base, rel.to_lowercase())) {
                continue;
            }
            let p = self.target_path(base, rel)?;
            let pre = match read_opt(&p)? {
                None => Value::Null,
                Some(data) => {
                    let sha = sha256::hex(&data);
                    let bp = d.join(format!("{sha}.bin"));
                    if !bp.is_file() {
                        Self::store_write(&bp, &data)?;
                    }
                    json!({ "sha": sha, "size": data.len() })
                }
            };
            entries.push(json!({ "base": base, "path": rel, "pre": pre }));
        }
        let j = json!({ "format": JOURNAL_FORMAT, "owner": owner, "action": action, "txn": txn,
                        "pid": std::process::id(), "started": utc_now().0, "targets": entries });
        let text = serde_json::to_string_pretty(&j).unwrap_or_default() + "\n";
        Self::store_write(&self.root.join(JOURNAL), text.as_bytes())
    }

    fn clear_journal(&self) {
        let _ = remove_file(&self.root.join(JOURNAL));
        let _ = fs::remove_dir_all(self.root.join(JOURNAL_DIR));
        let _ = remove_file(&self.root.join(DIRTY));
    }

    fn rollback(&self, j: &Value) -> Result<Vec<String>, LedgerError> {
        let d = self.root.join(JOURNAL_DIR);
        let mut done = Vec::new();
        let mut errors = Vec::new();
        let targets = j.get("targets").and_then(Value::as_array).cloned().unwrap_or_default();
        for t in targets.iter().rev() {
            let r = (|| -> Result<Option<String>, LedgerError> {
                let (base, rel) = (s(t, "base").unwrap_or_default(), s(t, "path").unwrap_or_default());
                let p = self.target_path(base, rel)?;
                let mut tmp = p.as_os_str().to_owned();
                tmp.push(TMP);
                let _ = fs::remove_file(PathBuf::from(tmp));
                atomic::remove_dead_temps(&p, |pid| pid_alive(i64::from(pid)));
                match t.get("pre").and_then(|p| sha_of(p).map(str::to_owned)) {
                    None => {
                        if p.is_file() {
                            remove_file(&p).map_err(|e| ioe(&p, &e))?;
                            return Ok(Some(format!("{base}:{rel} removed")));
                        }
                        Ok(None)
                    }
                    Some(sha) => {
                        if read_opt(&p)?.is_some_and(|cur| sha256::hex(&cur) == sha) {
                            return Ok(None);
                        }
                        let bp = d.join(format!("{sha}.bin"));
                        let data = fs::read(&bp).map_err(|e| ioe(&bp, &e))?;
                        if sha256::hex(&data) != sha {
                            return Err(LedgerError::Damaged(format!(
                                "{}: the saved pre-image is damaged",
                                bp.display()
                            )));
                        }
                        write_atomic(&p, &data).map_err(|e| ioe(&p, &e))?;
                        Ok(Some(format!("{base}:{rel} restored")))
                    }
                }
            })();
            match r {
                Ok(Some(m)) => done.push(m),
                Ok(None) => {}
                Err(e) => errors.push(e.to_string()),
            }
        }
        if errors.is_empty() {
            Ok(done)
        } else {
            Err(LedgerError::Io(errors.join("; ")))
        }
    }

    /// Under the lock: an interrupted commit's journal rolled back (resumable), and its notes.
    fn recover_locked(&self, notes: &mut Vec<String>) -> Result<(), LedgerError> {
        let jp = self.root.join(JOURNAL);
        let dirty = self.root.join(DIRTY).is_file();
        if !jp.is_file() {
            if dirty {
                self.clear_journal();
            }
            return Ok(());
        }
        let j: Value = fs::read(&jp)
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .filter(|j: &Value| s(j, "format") == Some(JOURNAL_FORMAT))
            .ok_or_else(|| {
                LedgerError::Damaged(format!(
                    "{}: damaged journal: the files of the interrupted commit cannot be rolled back automatically - \
                     restore the game files, then delete the journal",
                    jp.display()
                ))
            })?;
        match self.rollback(&j) {
            Ok(done) => {
                notes.push(format!(
                    "an interrupted {} of {} (txn {}) was rolled back first{}",
                    s(&j, "action").unwrap_or("?"),
                    s(&j, "owner").unwrap_or("?"),
                    s(&j, "txn").unwrap_or("?"),
                    if done.is_empty() {
                        String::new()
                    } else {
                        format!(": {}", done.join(", "))
                    }
                ));
                self.clear_journal();
                Ok(())
            }
            Err(e) => {
                let _ = Self::store_write(
                    &self.root.join(DIRTY),
                    format!("{} recover: {e}\n", utc_now().0).as_bytes(),
                );
                Err(LedgerError::Io(format!(
                    "an interrupted commit could not be rolled back ({e}) - free the files (close the game) and \
                     run recover again"
                )))
            }
        }
    }

    /// Rolls an interrupted commit back (clearing a stale lock first); `force` clears a lock whose pid is alive
    /// (only when no install is running). What was restored.
    ///
    /// # Errors
    /// The lock is held, or a file still cannot be written.
    pub fn recover(&mut self, force: bool) -> Result<Vec<String>, LedgerError> {
        if force {
            let _ = remove_file(&self.root.join(LOCK));
        }
        let mut notes = Vec::new();
        if !self.root.is_dir() {
            return Ok(notes);
        }
        self.lock(&mut notes)?;
        self.unlock();
        Ok(notes)
    }

    // ---------------------------------------------------------------------------------------------- engine

    fn current(&self, work: &mut BTreeMap<String, Option<Vec<u8>>>, rel: &str) -> Result<Option<Vec<u8>>, LedgerError> {
        let k = rel.to_lowercase();
        if let Some(v) = work.get(&k) {
            return Ok(v.clone());
        }
        let v = read_opt(&self.path(rel))?;
        work.insert(k, v.clone());
        Ok(v)
    }

    /// Takes the owner's contribution c out (in memory): splice when someone sits on it, else the prior back.
    #[allow(clippy::too_many_arguments, reason = "the commit's working state, passed apart")]
    fn take_out(
        &self,
        recs: &mut Records,
        owner: &str,
        c: &Value,
        work: &mut BTreeMap<String, Option<Vec<u8>>>,
        writes: &mut BTreeMap<String, (String, Option<Vec<u8>>)>,
        dirty: &mut BTreeSet<String>,
        conflicts: &mut Vec<String>,
        notes: &mut Vec<String>,
    ) -> Result<(), LedgerError> {
        let rel = s(c, "file").unwrap_or_default().to_owned();
        if s(c, "kind") != Some("file") {
            conflicts.push(format!(
                "{rel}: {owner} holds a {} slot there (Project OPFOR's slot ledger, which no shipped install used):                  it is kept as written and not taken out - drop {owner} with bfbc2 gameledger forget <game> {owner}                  (the game files stay as they are)",
                s(c, "kind").unwrap_or("?")
            ));
            return Ok(());
        }
        let (layers, _) = Self::layers(recs, &rel);
        let above = layers
            .iter()
            .find(|l| l.0 != owner && Self::contribution(recs, l).and_then(|d| s(d, "below")) == Some(owner))
            .cloned();
        if let Some((o, i)) = above {
            let d = Self::contribution(recs, &(o.clone(), i))
                .cloned()
                .unwrap_or(Value::Null);
            if derived(&d) {
                conflicts.push(format!("{rel}: {o} was built on {owner}'s file: revert {o} first"));
                return Ok(());
            }
            if let Some(dc) = recs
                .get_mut(&o)
                .and_then(|r| r.get_mut("contributions"))
                .and_then(Value::as_array_mut)
                .and_then(|a| a.get_mut(i))
                .and_then(Value::as_object_mut)
            {
                dc.insert("prior".to_owned(), c.get("prior").cloned().unwrap_or(Value::Null));
                dc.insert("below".to_owned(), c.get("below").cloned().unwrap_or(Value::Null));
            }
            dirty.insert(o.clone());
            notes.push(format!(
                "{rel}: taken out from under {o}'s install, which now puts back what was there before {owner}"
            ));
            return Ok(());
        }
        let cur = self.current(work, &rel)?;
        let want = c.get("value").and_then(sha_of).unwrap_or_default();
        match &cur {
            None => {
                conflicts.push(format!("{rel}: missing (deleted since {owner} installed it)"));
                return Ok(());
            }
            Some(d) if sha256::hex(d) != want => {
                conflicts.push(format!(
                    "{rel}: changed outside the ledger since {owner} installed it - not reverted"
                ));
                return Ok(());
            }
            Some(_) => {}
        }
        let data = match c.get("prior").and_then(|p| s(p, "blob")) {
            None => None,
            Some(b) => match self.blob(b) {
                Ok(d) => Some(d),
                Err(e) => {
                    conflicts.push(format!(
                        "{rel}: the file from before {owner} is missing from the ledger ({e})"
                    ));
                    return Ok(());
                }
            },
        };
        work.insert(rel.to_lowercase(), data.clone());
        writes.insert(rel.to_lowercase(), (rel, data));
        Ok(())
    }

    /// Installs `files` (game-relative path, bytes) as `owner`'s whole set: files of its earlier install not in the
    /// set are taken out in the same commit. `derived`: the bytes were built from the file found there (a buried
    /// layer under it cannot be taken out alone). `dry_run`: the checks and the plan, nothing written.
    ///
    /// # Errors
    /// [`LedgerError::Refused`] with every conflict (nothing written); a held lock; a write that failed (rolled back).
    pub fn install(
        &mut self,
        owner: &str,
        files: &[(String, Vec<u8>)],
        derived: bool,
        note: &str,
        dry_run: bool,
    ) -> Result<Plan, LedgerError> {
        self.commit(owner, Some(files), derived, note, dry_run)
    }

    /// Takes every contribution of `owner` out (splice under another owner's independent install; refused under a
    /// derived one). Nothing installed: a plan that says so.
    ///
    /// # Errors
    /// As [`Ledger::install`].
    pub fn uninstall(&mut self, owner: &str, dry_run: bool) -> Result<Plan, LedgerError> {
        self.commit(owner, None, false, "", dry_run)
    }

    fn commit(
        &mut self,
        owner: &str,
        files: Option<&[(String, Vec<u8>)]>,
        is_derived: bool,
        note: &str,
        dry_run: bool,
    ) -> Result<Plan, LedgerError> {
        if !valid_owner(owner) {
            return Err(LedgerError::Damaged(format!("not an owner name: {owner:?}")));
        }
        let mut plan = Plan {
            owner: owner.to_owned(),
            action: if files.is_some() { "install" } else { "uninstall" }.to_owned(),
            ..Plan::default()
        };
        if !dry_run {
            self.lock(&mut plan.notes)?;
        } else if self.root.join(JOURNAL).is_file() {
            plan.notes
                .push("an interrupted commit's journal is there: it is rolled back first".to_owned());
        }
        let r = self.commit_locked(owner, files, is_derived, note, dry_run, &mut plan);
        if !dry_run && !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r.map(|()| plan)
    }

    #[allow(clippy::too_many_lines, reason = "one commit: the plan, then the journalled writes")]
    fn commit_locked(
        &mut self,
        owner: &str,
        files: Option<&[(String, Vec<u8>)]>,
        is_derived: bool,
        note: &str,
        dry_run: bool,
        plan: &mut Plan,
    ) -> Result<(), LedgerError> {
        let mut recs = self.records()?;
        let old = recs.get(owner).cloned();
        if files.is_none() && old.is_none() {
            plan.notes.push("not installed".to_owned());
            return Ok(());
        }
        let mut work: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        let mut writes: BTreeMap<String, (String, Option<Vec<u8>>)> = BTreeMap::new();
        let mut dirty: BTreeSet<String> = BTreeSet::new();
        let mut conflicts: Vec<String> = Vec::new();
        let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut new_cs: Vec<Value> = Vec::new();
        let mut unchanged = true;
        let new_rels: Vec<(String, &[u8])> = match files {
            Some(fs_) => fs_
                .iter()
                .map(|(r, d)| norm_rel(r).map(|r| (r, d.as_slice())))
                .collect::<Result<_, _>>()?,
            None => Vec::new(),
        };
        let mut seen = BTreeSet::new();
        for (r, _) in &new_rels {
            if !seen.insert(r.to_lowercase()) {
                return Err(LedgerError::Damaged(format!("{r}: given twice")));
            }
        }
        // the owner's old contributions to files not in the new set: taken out
        let old_cs: Vec<Value> = old.as_ref().map_or_else(Vec::new, |r| Self::contributions(r).to_vec());
        for c in &old_cs {
            let rel = s(c, "file").unwrap_or_default();
            if !seen.contains(&rel.to_lowercase()) {
                unchanged = false;
                let mut notes = Vec::new();
                self.take_out(
                    &mut recs,
                    owner,
                    c,
                    &mut work,
                    &mut writes,
                    &mut dirty,
                    &mut conflicts,
                    &mut notes,
                )?;
                plan.notes.extend(notes);
            }
        }
        for (rel, data) in &new_rels {
            let (layers, slots) = Self::layers(&recs, rel);
            let slots: Vec<&String> = slots.iter().filter(|o| *o != owner).collect();
            if !slots.is_empty() {
                conflicts.push(format!(
                    "{rel}: {} holds parts of it (Project OPFOR's ledger): a whole-file install would overwrite them",
                    slots.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                ));
                continue;
            }
            let cur = self.current(&mut work, rel)?;
            let cur_sha = cur.as_deref().map(sha256::hex);
            let new_sha = sha256::hex(data);
            let mine = layers.iter().find(|l| l.0 == owner).cloned();
            let (prior, below) = if let Some(m) = mine {
                let c = Self::contribution(&recs, &m).cloned().unwrap_or(Value::Null);
                if let Some(a) = layers
                    .iter()
                    .find(|l| l.0 != owner && Self::contribution(&recs, l).and_then(|d| s(d, "below")) == Some(owner))
                {
                    conflicts.push(format!(
                        "{rel}: {} was installed after {owner}: revert {} first",
                        a.0, a.0
                    ));
                    continue;
                }
                if cur_sha.is_some() && cur_sha.as_deref() != c.get("value").and_then(sha_of) {
                    conflicts.push(format!(
                        "{rel}: changed outside the ledger since {owner} installed it - not overwritten"
                    ));
                    continue;
                }
                if cur_sha.is_none() {
                    plan.notes
                        .push(format!("{rel}: deleted since {owner} installed it - installed again"));
                } else if c.get("value").and_then(sha_of) == Some(new_sha.as_str()) && derived(&c) == is_derived {
                    new_cs.push(c);
                    plan.notes.push(format!("{rel}: already installed"));
                    continue;
                }
                (
                    c.get("prior").cloned().unwrap_or(Value::Null),
                    c.get("below").cloned().unwrap_or(Value::Null),
                )
            } else {
                let st = Self::stack(&recs, &layers);
                let mut below = Value::Null;
                if let Some(top) = st.last() {
                    let tc = Self::contribution(&recs, top).cloned().unwrap_or(Value::Null);
                    if cur_sha.as_deref() != tc.get("value").and_then(sha_of) {
                        conflicts.push(format!(
                            "{rel}: changed outside the ledger since {} installed it - not overwritten",
                            top.0
                        ));
                        continue;
                    }
                    below = json!(top.0);
                }
                let prior = match &cur {
                    None => Value::Null,
                    Some(d) => {
                        let sha = sha256::hex(d);
                        blobs.insert(sha.clone(), d.clone());
                        json!({ "sha": sha, "size": d.len(), "blob": sha })
                    }
                };
                (prior, below)
            };
            unchanged = false;
            new_cs.push(json!({ "kind": "file", "file": rel, "slot": "",
                                "value": { "sha": new_sha, "size": data.len() },
                                "prior": prior, "below": below, "stack": true,
                                "extra": { "derived": is_derived } }));
            work.insert(rel.to_lowercase(), Some(data.to_vec()));
            writes.insert(rel.to_lowercase(), (rel.clone(), Some(data.to_vec())));
        }
        if !conflicts.is_empty() {
            return Err(LedgerError::Refused(conflicts));
        }
        let old_note = old.as_ref().and_then(|r| s(r, "note")).unwrap_or_default();
        if files.is_some() && old.is_some() && unchanged && dirty.is_empty() && note == old_note {
            plan.notes.push("unchanged: nothing written".to_owned());
            return Ok(());
        }
        // a file whose final bytes are what is there already is not written
        writes.retain(|k, (rel, d)| {
            let _ = k;
            read_opt(&self.path(rel)).ok().flatten() != *d
        });
        for (rel, d) in writes.values() {
            plan.writes
                .push(format!("{} {rel}", if d.is_some() { "write" } else { "remove" }));
        }
        if dry_run {
            return Ok(());
        }
        let (now, _) = utc_now();
        let rec = files.map(|_| {
            json!({ "format": FORMAT, "owner": owner, "txn": new_txn(), "installed": now, "note": note,
                    "contributions": new_cs })
        });
        let txn = rec
            .as_ref()
            .and_then(|r| s(r, "txn").map(str::to_owned))
            .unwrap_or_else(new_txn);
        let mut targets: Vec<(&str, String)> = writes.values().map(|(rel, _)| ("game", rel.clone())).collect();
        targets.push(("ledger", owner_file(owner)));
        for o in &dirty {
            targets.push(("ledger", owner_file(o)));
        }
        self.begin(owner, &plan.action, &txn, &targets)?;
        let r = (|| -> Result<(), LedgerError> {
            for (sha, data) in &blobs {
                let p = self.blob_path(sha);
                if !p.is_file() {
                    self.step(&format!("blob {sha}"))?;
                    Self::store_write(&p, data)?;
                }
            }
            for (rel, d) in writes.values() {
                let p = self.path(rel);
                if let Some(d) = d {
                    self.step(&format!("game {rel}"))?;
                    write_atomic(&p, d).map_err(|e| ioe(&p, &e))?;
                } else {
                    self.step(&format!("remove {rel}"))?;
                    remove_file(&p).map_err(|e| ioe(&p, &e))?;
                }
            }
            self.step(&format!("record {owner}"))?;
            self.write_record(owner, rec.as_ref())?;
            for o in &dirty {
                if o != owner {
                    self.step(&format!("record {o}"))?;
                    self.write_record(o, recs.get(o))?;
                }
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                self.clear_journal();
                if let Some(rec) = rec {
                    recs.insert(owner.to_owned(), rec);
                } else {
                    recs.remove(owner);
                }
                self.gc(&recs);
                plan.applied = true;
                Ok(())
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }

    /// A failed commit: rolled back from the journal; the error to return.
    fn abort(&self, e: &LedgerError) -> LedgerError {
        let jp = self.root.join(JOURNAL);
        let j: Option<Value> = fs::read(&jp).ok().and_then(|d| serde_json::from_slice(&d).ok());
        match j.map(|j| self.rollback(&j)) {
            Some(Ok(_)) | None => {
                self.clear_journal();
                LedgerError::Io(format!("{e} - nothing changed: every file written was put back"))
            }
            Some(Err(r)) => {
                let _ = Self::store_write(
                    &self.root.join(DIRTY),
                    format!("{} {e}\nrollback: {r}\n", utc_now().0).as_bytes(),
                );
                LedgerError::Io(format!(
                    "{e}, and its rollback failed too ({r}): the game copy is half-written. Free the files (close \
                     the game), then run recover (bfbc2 gameledger recover <game>)"
                ))
            }
        }
    }

    fn write_record(&self, owner: &str, rec: Option<&Value>) -> Result<(), LedgerError> {
        let p = self.root.join(owner_file(owner));
        let empty = rec.is_none_or(|r| Self::contributions(r).is_empty());
        if empty {
            return remove_file(&p).map_err(|e| ioe(&p, &e));
        }
        // Python's json.dumps(indent=1): one space per level
        let mut buf = Vec::new();
        let mut ser =
            serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b" "));
        serde::Serialize::serialize(rec.unwrap_or(&Value::Null), &mut ser)
            .map_err(|e| LedgerError::Io(format!("{}: {e}", p.display())))?;
        buf.push(b'\n');
        Self::store_write(&p, &buf)
    }

    /// Blobs no record names any more are deleted (every kind's: any "blob" key in a value or a prior).
    fn gc(&self, recs: &Records) {
        let d = self.root.join("blobs");
        let Ok(rd) = fs::read_dir(&d) else { return };
        let mut used = BTreeSet::new();
        for r in recs.values() {
            for c in Self::contributions(r) {
                for k in ["value", "prior"] {
                    if let Some(b) = c.get(k).and_then(|v| s(v, "blob")) {
                        used.insert(b.to_owned());
                    }
                }
            }
        }
        for e in rd.flatten() {
            let f = e.file_name().to_string_lossy().into_owned();
            if let Some(sha) = f.strip_suffix(".bin")
                && !used.contains(sha)
            {
                let _ = fs::remove_file(e.path());
            }
        }
    }

    /// Records an install made before the ledger: `owner` installed each file as it is now, over `prior` (the legacy
    /// backup's bytes, or nothing). The legacy marker files `remove` (game-relative) are deleted in the same
    /// journalled commit. Refused when another owner holds one of the files already, or a file is missing.
    ///
    /// # Errors
    /// As [`Ledger::install`].
    pub fn adopt(&mut self, owner: &str, entries: &[Adopt], remove: &[String]) -> Result<Plan, LedgerError> {
        if !valid_owner(owner) {
            return Err(LedgerError::Damaged(format!("not an owner name: {owner:?}")));
        }
        let mut plan = Plan {
            owner: owner.to_owned(),
            action: "adopt".to_owned(),
            ..Plan::default()
        };
        self.lock(&mut plan.notes)?;
        let r = self.adopt_locked(owner, entries, remove, &mut plan);
        if !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r.map(|()| plan)
    }

    fn adopt_locked(
        &mut self,
        owner: &str,
        entries: &[Adopt],
        remove: &[String],
        plan: &mut Plan,
    ) -> Result<(), LedgerError> {
        let mut recs = self.records()?;
        let mut conflicts = Vec::new();
        let mut cs: Vec<Value> = recs
            .get(owner)
            .map_or_else(Vec::new, |r| Self::contributions(r).to_vec());
        let mut blobs = BTreeMap::new();
        for a in entries {
            let rel = norm_rel(&a.rel)?;
            let (layers, slots) = Self::layers(&recs, &rel);
            if let Some(o) = layers.first().map(|l| &l.0).or_else(|| slots.first()) {
                conflicts.push(format!("{rel}: {o} holds it in the ledger already"));
                continue;
            }
            let Some(cur) = read_opt(&self.path(&rel))? else {
                conflicts.push(format!("{rel}: missing - the legacy install's file is gone"));
                continue;
            };
            let prior = match &a.prior {
                None => Value::Null,
                Some(d) => {
                    let sha = sha256::hex(d);
                    blobs.insert(sha.clone(), d.clone());
                    json!({ "sha": sha, "size": d.len(), "blob": sha })
                }
            };
            cs.push(json!({ "kind": "file", "file": rel, "slot": "",
                            "value": { "sha": sha256::hex(&cur), "size": cur.len() },
                            "prior": prior, "below": null, "stack": true, "extra": { "derived": false } }));
            plan.writes.push(format!("adopt {rel}"));
        }
        if !conflicts.is_empty() {
            return Err(LedgerError::Refused(conflicts));
        }
        let txn = new_txn();
        let rec = json!({ "format": FORMAT, "owner": owner, "txn": txn, "installed": utc_now().0,
                          "note": "adopted from the markers of before the install ledger", "contributions": cs });
        let removes: Vec<String> = remove.iter().map(|r| norm_rel(r)).collect::<Result<_, _>>()?;
        let mut targets: Vec<(&str, String)> = vec![("ledger", owner_file(owner))];
        targets.extend(removes.iter().map(|r| ("game", r.clone())));
        plan.writes.extend(removes.iter().map(|r| format!("remove {r}")));
        self.begin(owner, "adopt", &txn, &targets)?;
        let r = (|| -> Result<(), LedgerError> {
            for (sha, data) in &blobs {
                let p = self.blob_path(sha);
                if !p.is_file() {
                    self.step(&format!("blob {sha}"))?;
                    Self::store_write(&p, data)?;
                }
            }
            self.step(&format!("record {owner}"))?;
            self.write_record(owner, Some(&rec))?;
            for r in &removes {
                self.step(&format!("remove {r}"))?;
                let p = self.path(r);
                remove_file(&p).map_err(|e| ioe(&p, &e))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                self.clear_journal();
                recs.insert(owner.to_owned(), rec);
                self.gc(&recs);
                plan.applied = true;
                Ok(())
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }
}

/// The families of install markers from before the ledger that the suite's own installers adopt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Legacy {
    /// NavKit: `<file>.opfor-orig` (DICE's file) / `<file>.opfor-new` (there was none).
    NavKit,
    /// stock-MP: `<file>.opfor-stockmp-orig` / `-applied` (the sha256 written), leftovers `-new` / `-prev`.
    StockMp,
}

impl Legacy {
    fn own(self) -> &'static [&'static str] {
        match self {
            Self::NavKit => &[".opfor-orig", ".opfor-new"],
            Self::StockMp => &[
                ".opfor-stockmp-orig",
                ".opfor-stockmp-applied",
                ".opfor-stockmp-new",
                ".opfor-stockmp-prev",
            ],
        }
    }
}

/// The refusal for a file with another family's legacy markers beside it.
#[must_use]
pub fn legacy_refusal(rel: &str, markers: &[String]) -> String {
    format!(
        "{rel} has install markers from before the install ledger ({}): migrate them first - Project OPFOR: python \
         src\\tools\\opforcore\\migrate.py --apply",
        markers.join(", ")
    )
}

impl Ledger {
    /// Adopts the legacy markers of `family` beside the game files `rels` as `owner`'s install, removing them (one
    /// journalled commit). None when there were none. Refused in words when another family's markers are there, or
    /// the markers do not describe one finished install (a changed file, an interrupted commit, a backup without
    /// its record).
    ///
    /// # Errors
    /// [`LedgerError::Refused`]; as [`Ledger::adopt`].
    #[allow(clippy::too_many_lines, reason = "one rule per marker family and state, in words")]
    pub fn adopt_legacy(&mut self, owner: &str, rels: &[String], family: Legacy) -> Result<Option<Plan>, LedgerError> {
        let mut entries = Vec::new();
        let mut remove = Vec::new();
        let mut conflicts = Vec::new();
        for rel in rels {
            let rel = norm_rel(rel)?;
            let p = self.path(&rel);
            let marks = legacy_markers(&p);
            if marks.is_empty() {
                continue;
            }
            let low: Vec<String> = marks.iter().map(|m| m.to_lowercase()).collect();
            if low.iter().any(|m| !family.own().contains(&m.as_str())) {
                conflicts.push(legacy_refusal(&rel, &marks));
                continue;
            }
            let has = |m: &str| low.iter().any(|x| x == m);
            let mark_path = |m: &str| {
                let mut s = p.as_os_str().to_owned();
                s.push(m);
                PathBuf::from(s)
            };
            remove.extend(marks.iter().map(|m| format!("{rel}{m}")));
            if self.holders(&rel)?.iter().any(|h| h == owner) {
                continue; // stale markers beside the owner's own install in the ledger: only removed
            }
            let cur = read_opt(&p)?;
            match family {
                Legacy::NavKit => {
                    if cur.is_none() {
                        conflicts.push(format!(
                            "{rel}: NavKit's markers ({}) but no installed file: restore by hand (copy \
                             {rel}.opfor-orig back, or delete the markers when the level had no path data)",
                            marks.join(", ")
                        ));
                        continue;
                    }
                    let prior = if has(".opfor-orig") {
                        Some(fs::read(mark_path(".opfor-orig")).map_err(|e| ioe(&p, &e))?)
                    } else {
                        None
                    };
                    entries.push(Adopt {
                        rel: rel.clone(),
                        prior,
                    });
                }
                Legacy::StockMp => {
                    if has(".opfor-stockmp-prev") {
                        conflicts.push(format!(
                            "{rel}: a stock-MP install was interrupted before the install ledger \
                             ({rel}.opfor-stockmp-prev is the file from before it): put it back as {rel} (or delete \
                             it when {rel} is the file to keep), then run again"
                        ));
                        continue;
                    }
                    let applied = if has(".opfor-stockmp-applied") {
                        Some(
                            String::from_utf8_lossy(
                                &fs::read(mark_path(".opfor-stockmp-applied")).map_err(|e| ioe(&p, &e))?,
                            )
                            .trim()
                            .to_owned(),
                        )
                    } else {
                        None
                    };
                    match (applied, &cur) {
                        (None, _) if has(".opfor-stockmp-orig") => {
                            conflicts.push(format!(
                                "{rel}: a stock-MP backup ({rel}.opfor-stockmp-orig) with no install record: if \
                                 {rel} is the game's own file, delete the backup; if it is stock-MP's, copy the \
                                 backup over it first; then run again"
                            ));
                        }
                        (None, _) => {} // only a leftover staged copy: removed
                        (Some(_), None) => {
                            conflicts.push(format!(
                                "{rel}: stock-MP's record but no installed file: copy {rel}.opfor-stockmp-orig back \
                                 (if there is one) and delete the markers, then run again"
                            ));
                        }
                        (Some(a), Some(d)) => {
                            if a != sha256::hex(d) {
                                conflicts.push(format!(
                                    "{rel}: changed after stock-MP's install (its .opfor-stockmp-applied does not \
                                     match): copy {rel}.opfor-stockmp-orig over it to take it back to the game's \
                                     own file, or delete the stock-MP markers to keep it as it is; then run again"
                                ));
                                continue;
                            }
                            let prior = if has(".opfor-stockmp-orig") {
                                Some(fs::read(mark_path(".opfor-stockmp-orig")).map_err(|e| ioe(&p, &e))?)
                            } else {
                                None
                            };
                            entries.push(Adopt {
                                rel: rel.clone(),
                                prior,
                            });
                        }
                    }
                }
            }
        }
        if !conflicts.is_empty() {
            return Err(LedgerError::Refused(conflicts));
        }
        if remove.is_empty() {
            return Ok(None);
        }
        self.adopt(owner, &entries, &remove).map(Some)
    }
}

#[cfg(test)]
#[path = "ledger_tests.rs"]
mod tests;

// round OR1: Project OPFOR's stage operations (install with the owner's other files kept, restate)
#[path = "ledger_keep.rs"]
mod keep;

// round MM1: the mod manager's operations (some of an owner's files out, a setting edited through every layer, the
// migration's records)
#[path = "ledger_part.rs"]
mod part;

// round OR15: the one ledger - every reader's rules, status, the health check, forget (audit AU5-ARC-3)
#[path = "ledger_check.rs"]
mod check;
pub use check::{Entry, KINDS, Report, validate};
