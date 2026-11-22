//! The mod manager (round MM1): Project OPFOR's mods - the Pacific War, its menus, the Vietnam opt-in, the
//! Onslaught console ports, the map editor's exports, NavKit's path data and stock-MP's AI per level - enabled,
//! disabled, uninstalled and verified one at a time on the install ledger, and DICE's originals put back per file or
//! for the whole game, without a rebuild and without losing anything the ledger keeps.
//!
//! ```text
//! <game>\opfor\mods\stash\<unit>.json     a disabled unit: an owner's contributions, a level folder's files
//! <game>\opfor\mods\blobs\<sha256>.bin    the bytes a disabled owner wrote (a hard link of the file it replaced)
//! <game>\opfor\mods\folders\<path>        a disabled level folder's files (moved, their write times kept)
//! <game>\opfor\mods\.op.json              the operation being carried out (every step, with all its data)
//! <game>\opfor\mods\.lock                 {"pid", "started"} while an operation runs
//! ```
//!
//! **Disable** takes a mod's units out: each owner's files (the install ledger's revert: its prior back), top layer
//! first, after the owners built on them - its **dependents**, named and disabled with it only with a confirmation
//! (`--yes`) - then its level folders, moved aside. What the owner wrote is kept (`stash`, `blobs`), so **enable**
//! puts it back as it was, byte for byte and with its write time (the DLL's markers compare it), on the file it was
//! built on: a file that is not that any more is refused in words, but a settings file (`.ini`) gets the edits made
//! since carried over. **Uninstall** is a disable whose kept copy is dropped. **restore-original** takes every layer
//! off the files given (or every mod off the game) and keeps them, so the mods come back with `enable`.
//!
//! Every operation is planned and checked before its first write, then written as `.op.json` with everything it needs
//! (the kept bytes linked in first), then carried out step by step; each step can run again, so a kill at any point
//! is finished by the next command (or `bfbc2 mods resume`). Each ledger step is one journalled commit of the ledger
//! (spec `transforms/install.md`, XINST-GL-2).

pub mod catalog;
pub mod ini;
pub mod migrate;
mod plan;
pub mod verify;

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use bc2_gamefs::ledger::{Hook, Interrupt, Ledger, LedgerError};
use bc2_gamefs::liveness::pid_alive;
use bc2_gamefs::{atomic, sha256};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub use plan::Change;

/// The stash records' format.
pub const STASH_FORMAT: &str = "opfor-mods-stash/1";
/// The operation journal's format.
pub const OP_FORMAT: &str = "opfor-mods-op/1";
const OP: &str = ".op.json";
const LOCK: &str = ".lock";

/// The mod manager's folder of a game copy.
#[must_use]
pub fn default_root(game: &Path) -> PathBuf {
    game.join("opfor").join("mods")
}

/// What did not happen.
#[derive(Debug)]
pub enum ModError {
    /// Checks failed: nothing was written. Each line says why.
    Refused(Vec<String>),
    /// Other mods would change too (dependents, requirements, the owners of a file): run again with `--yes`.
    Confirm(Vec<String>),
    /// Another operation runs.
    Locked(String),
    /// A step failed part way: the next command (or `bfbc2 mods resume`) carries the operation on.
    Io(String),
    /// A test hook stopped the process at a step (the journal and the lock left as a kill leaves them).
    Crashed,
}

impl fmt::Display for ModError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(c) | Self::Confirm(c) => write!(f, "{}", c.join("\n")),
            Self::Locked(m) | Self::Io(m) => f.write_str(m),
            Self::Crashed => f.write_str("stopped by the test hook"),
        }
    }
}

impl std::error::Error for ModError {}

fn from_ledger(e: LedgerError) -> ModError {
    match e {
        LedgerError::Refused(c) => ModError::Refused(c),
        LedgerError::Locked(m) => ModError::Locked(m),
        LedgerError::Crashed => ModError::Crashed,
        e => ModError::Io(e.to_string()),
    }
}

fn ioe(p: &Path, e: &io::Error) -> ModError {
    ModError::Io(format!("{}: {e}", p.display()))
}

// ------------------------------------------------------------------------------------------------- records

/// A sha256 and a size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shasize {
    /// The sha256 (hex).
    pub sha: String,
    /// The size in bytes.
    pub size: u64,
}

/// One kept contribution of a disabled owner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Kept {
    /// The game-relative file.
    pub file: String,
    /// What the owner wrote (its bytes in `blobs`).
    pub value: Shasize,
    /// What it was built on (None: the owner created the file); a settings file's bytes are kept too.
    pub prior: Option<Shasize>,
    /// Built from the file it found (every OPFOR stage).
    pub derived: bool,
    /// The file's write time when it went out (nanoseconds since 1970; None: it was under another layer).
    pub mtime_ns: Option<u64>,
    /// The order it went out in (the last out comes back first).
    pub seq: u64,
}

/// A disabled owner's record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerStash {
    /// [`STASH_FORMAT`].
    pub format: String,
    /// The ledger owner.
    pub owner: String,
    /// Its mod.
    #[serde(rename = "mod")]
    pub mod_id: String,
    /// The operation that took it out (the mods disabled together come back together).
    pub op: String,
    /// The ledger record's note.
    pub note: String,
    /// What it wrote, file by file.
    pub contributions: Vec<Kept>,
}

/// One file of a disabled level folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderFile {
    /// The game-relative file.
    pub rel: String,
    /// Its sha256 and size.
    pub sha: String,
    /// Its size.
    pub size: u64,
    /// Its write time.
    pub mtime_ns: u64,
}

/// A disabled level folder (installed beside the ledger, with its own record).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderStash {
    /// [`STASH_FORMAT`].
    pub format: String,
    /// The game-relative folder.
    pub folder: String,
    /// Its install record's name.
    pub record: String,
    /// Its mod.
    #[serde(rename = "mod")]
    pub mod_id: String,
    /// The operation that took it out.
    pub op: String,
    /// The order it went out in.
    pub seq: u64,
    /// Its files.
    pub files: Vec<FolderFile>,
}

/// A file a step installs from the kept bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InFile {
    /// The game-relative file.
    pub file: String,
    /// Its bytes' sha256 (in `blobs`).
    pub sha: String,
    /// The write time to give it (None: new bytes, a settings file with edits carried over).
    pub mtime_ns: Option<u64>,
}

/// One step of an operation; each can run again with the same result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "do", rename_all = "kebab-case")]
pub enum Step {
    /// An owner's contributions kept (merged into its stash record).
    StashOwner {
        /// The record (its contributions to add).
        entry: OwnerStash,
    },
    /// The owner's file gets a new prior before it goes out: its own ini keys back, the edits since kept.
    RestatePrior {
        /// The owner.
        owner: String,
        /// The file.
        file: String,
        /// The new prior (in `blobs`).
        sha: String,
    },
    /// Some of an owner's files reverted through the ledger.
    Uninstall {
        /// The owner.
        owner: String,
        /// The files.
        files: Vec<String>,
    },
    /// A level folder moved aside.
    MoveOut {
        /// Its record.
        entry: FolderStash,
    },
    /// Kept files installed as the owner's through the ledger.
    Install {
        /// The owner.
        owner: String,
        /// Built from the file found.
        derived: bool,
        /// The record's note.
        note: String,
        /// The files.
        files: Vec<InFile>,
    },
    /// A level folder moved back.
    MoveIn {
        /// The game-relative folder.
        folder: String,
    },
    /// An owner's kept files that are back dropped from its stash record.
    Unstash {
        /// The owner.
        owner: String,
        /// The files.
        files: Vec<String>,
    },
    /// A setting set in the file, in every state the ledger keeps of it and in the kept copies.
    Setting {
        /// The game-relative file.
        file: String,
        /// The section.
        section: String,
        /// The key.
        key: String,
        /// The value.
        value: String,
    },
    /// A kept unit dropped for good (an uninstall).
    Drop {
        /// The stash record's name.
        name: String,
    },
    /// The bundleManifest repair kept as its own install (the WW2 opt-out's, AU4-SM-6).
    ManifestFix,
}

/// An operation: its steps, written before the first.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Op {
    /// [`OP_FORMAT`].
    pub format: String,
    /// Its id.
    pub id: String,
    /// `disable`, `enable`, `uninstall`, `restore-original`, `set`.
    pub action: String,
    /// What it is for, in words.
    pub what: String,
    /// The steps.
    pub steps: Vec<Step>,
}

/// The stash record name of an owner / a folder.
#[must_use]
pub fn owner_stash_name(owner: &str) -> String {
    format!("owner~{}", owner.replace(':', "~"))
}

/// The stash record name of a level folder.
#[must_use]
pub fn folder_stash_name(folder: &str) -> String {
    format!("folder~{}", folder.to_lowercase().replace(['/', '\\'], "~"))
}

// ------------------------------------------------------------------------------------------------- the manager

struct Steps {
    n: u32,
    crash_at: Option<u32>,
    hook: Option<Hook>,
}

/// The mod manager of one game copy.
pub struct Manager {
    /// The game copy.
    pub game: PathBuf,
    /// Its folder (`<game>\opfor\mods`).
    pub root: PathBuf,
    steps: Rc<RefCell<Steps>>,
}

impl fmt::Debug for Manager {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Manager")
            .field("game", &self.game)
            .finish_non_exhaustive()
    }
}

fn count(st: &RefCell<Steps>, label: &str) -> Result<(), Interrupt> {
    let mut s = st.borrow_mut();
    s.n += 1;
    if s.crash_at == Some(s.n) {
        eprintln!("BC2_MODS_CRASH_AT={}: stopped before {label}", s.n);
        std::process::exit(9);
    }
    match s.hook.as_mut() {
        Some(h) => h(label),
        None => Ok(()),
    }
}

impl Manager {
    /// The mod manager of `game`. `BC2_MODS_CRASH_AT=<n>` (tests) ends the process (exit 9) before the n-th write
    /// step - its own and the ledger's - as a kill would.
    #[must_use]
    pub fn new(game: &Path) -> Self {
        Self {
            game: game.to_path_buf(),
            root: default_root(game),
            steps: Rc::new(RefCell::new(Steps {
                n: 0,
                crash_at: std::env::var("BC2_MODS_CRASH_AT")
                    .ok()
                    .and_then(|v| v.trim().parse().ok()),
                hook: None,
            })),
        }
    }

    /// A test hook called with a label before every write step (the manager's and the ledger's).
    pub fn set_hook(&mut self, hook: Option<Hook>) {
        self.steps.borrow_mut().hook = hook;
    }

    /// The write steps counted so far.
    #[must_use]
    pub fn steps_taken(&self) -> u32 {
        self.steps.borrow().n
    }

    fn step(&self, label: &str) -> Result<(), ModError> {
        count(&self.steps, label).map_err(|i| match i {
            Interrupt::Crash => ModError::Crashed,
            Interrupt::Fail(e) => ModError::Io(format!("{label}: {e}")),
        })
    }

    /// The game copy's ledger, its write steps counted with the manager's.
    #[must_use]
    pub fn ledger(&self) -> Ledger {
        let mut led = Ledger::new(&self.game);
        let st = Rc::clone(&self.steps);
        led.set_hook(Some(Box::new(move |label: &str| {
            count(&st, &format!("ledger {label}"))
        })));
        led
    }

    /// A game file's path.
    #[must_use]
    pub fn path(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.game.clone(), |p, x| p.join(x))
    }

    /// A kept blob's path.
    #[must_use]
    pub fn blob_path(&self, sha: &str) -> PathBuf {
        self.root.join("blobs").join(format!("{sha}.bin"))
    }

    fn stash_path(&self, name: &str) -> PathBuf {
        self.root.join("stash").join(format!("{name}.json"))
    }

    fn kept_path(&self, rel: &str) -> PathBuf {
        rel.split('/').fold(self.root.join("folders"), |p, x| p.join(x))
    }

    /// Whether an operation waits to be finished.
    #[must_use]
    pub fn interrupted(&self) -> bool {
        self.root.join(OP).is_file()
    }

    // ---------------------------------------------------------------------------------------------- stash

    /// Every disabled owner's record, by owner.
    ///
    /// # Errors
    /// A damaged record.
    pub fn owner_stashes(&self) -> Result<BTreeMap<String, OwnerStash>, ModError> {
        let mut out = BTreeMap::new();
        for (name, v) in self.stash_files()? {
            if name.starts_with("owner~") {
                let s: OwnerStash = serde_json::from_value(v)
                    .map_err(|e| ModError::Io(format!("{}: damaged ({e})", self.stash_path(&name).display())))?;
                out.insert(s.owner.clone(), s);
            }
        }
        Ok(out)
    }

    /// Every disabled level folder's record, by folder (lower case).
    ///
    /// # Errors
    /// A damaged record.
    pub fn folder_stashes(&self) -> Result<BTreeMap<String, FolderStash>, ModError> {
        let mut out = BTreeMap::new();
        for (name, v) in self.stash_files()? {
            if name.starts_with("folder~") {
                let s: FolderStash = serde_json::from_value(v)
                    .map_err(|e| ModError::Io(format!("{}: damaged ({e})", self.stash_path(&name).display())))?;
                out.insert(s.folder.to_lowercase(), s);
            }
        }
        Ok(out)
    }

    fn stash_files(&self) -> Result<Vec<(String, Value)>, ModError> {
        let d = self.root.join("stash");
        let rd = match fs::read_dir(&d) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(ioe(&d, &e)),
        };
        let mut names: Vec<String> = rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| Path::new(n).extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) && !n.starts_with('.'))
            .collect();
        names.sort();
        let mut out = Vec::new();
        for n in names {
            let p = d.join(&n);
            let v: Value = fs::read(&p)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .filter(|v: &Value| v.get("format").and_then(Value::as_str) == Some(STASH_FORMAT))
                .ok_or_else(|| ModError::Io(format!("{}: damaged stash record", p.display())))?;
            out.push((n.trim_end_matches(".json").to_owned(), v));
        }
        Ok(out)
    }

    #[allow(clippy::unused_self, reason = "the manager's writes, one place")]
    fn write_json(&self, p: &Path, v: &impl Serialize) -> Result<(), ModError> {
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).map_err(|e| ioe(d, &e))?;
        }
        let v = serde_json::to_value(v).map_err(|e| ModError::Io(e.to_string()))?;
        let text = serde_json::to_string_pretty(&v).map_err(|e| ModError::Io(e.to_string()))? + "\n";
        atomic::write(p, &[text.as_bytes()]).map_err(|e| ModError::Io(format!("{}: {e}", p.display())))
    }

    /// `data` kept as a blob (nothing when it is there already).
    fn put_blob(&self, data: &[u8]) -> Result<String, ModError> {
        let sha = sha256::hex(data);
        let p = self.blob_path(&sha);
        if !p.is_file() {
            if let Some(d) = p.parent() {
                fs::create_dir_all(d).map_err(|e| ioe(d, &e))?;
            }
            atomic::write(&p, &[data]).map_err(|e| ModError::Io(format!("{}: {e}", p.display())))?;
        }
        Ok(sha)
    }

    /// The file `src` kept as the blob `sha`: a hard link (no second copy on disk), else a copy; checked.
    fn link_blob(&self, src: &Path, sha: &str) -> Result<(), ModError> {
        let p = self.blob_path(sha);
        if p.is_file() {
            return Ok(());
        }
        if let Some(d) = p.parent() {
            fs::create_dir_all(d).map_err(|e| ioe(d, &e))?;
        }
        let tmp = atomic::temp_name(&p);
        if fs::hard_link(src, &tmp).is_err() {
            fs::copy(src, &tmp).map_err(|e| ioe(src, &e))?;
        }
        if sha256::file(&tmp.to_string_lossy()).ok().as_deref() != Some(sha) {
            let _ = fs::remove_file(&tmp);
            return Err(ModError::Refused(vec![format!(
                "{}: changed while it was being kept (it is not the bytes the ledger recorded)",
                src.display()
            )]));
        }
        atomic::replace(&tmp, &p).map_err(|e| {
            let _ = fs::remove_file(&tmp);
            ioe(&p, &e)
        })
    }

    /// A kept blob's bytes, checked.
    ///
    /// # Errors
    /// Missing or damaged.
    pub fn blob(&self, sha: &str) -> Result<Vec<u8>, ModError> {
        let p = self.blob_path(sha);
        let d = fs::read(&p).map_err(|e| ioe(&p, &e))?;
        if sha256::hex(&d) != sha {
            return Err(ModError::Io(format!("{}: damaged (its sha256 differs)", p.display())));
        }
        Ok(d)
    }

    // ---------------------------------------------------------------------------------------------- lock

    fn lock(&self) -> Result<(), ModError> {
        fs::create_dir_all(&self.root).map_err(|e| ioe(&self.root, &e))?;
        let p = self.root.join(LOCK);
        for _ in 0..2 {
            match fs::OpenOptions::new().write(true).create_new(true).open(&p) {
                Ok(mut f) => {
                    let body = json!({ "pid": std::process::id(), "started": now_secs() });
                    let r = f.write_all(body.to_string().as_bytes());
                    drop(f);
                    if let Err(e) = r {
                        let _ = fs::remove_file(&p);
                        return Err(ioe(&p, &e));
                    }
                    return Ok(());
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                    let info: Value = fs::read(&p)
                        .ok()
                        .and_then(|d| serde_json::from_slice(&d).ok())
                        .unwrap_or_else(|| json!({}));
                    let pid = info.get("pid").and_then(Value::as_i64);
                    let old = fs::metadata(&p)
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.elapsed().ok())
                        .is_some_and(|d| d > Duration::from_secs(60));
                    let stale = pid.map_or(old, |pid| !pid_alive(pid));
                    if !stale {
                        return Err(ModError::Locked(format!(
                            "{}: another mods operation is running (pid {}) - try again when it has finished",
                            p.display(),
                            pid.map_or_else(|| "?".to_owned(), |x| x.to_string())
                        )));
                    }
                    let _ = fs::remove_file(&p);
                }
                Err(e) => return Err(ioe(&p, &e)),
            }
        }
        Err(ModError::Locked(format!("{}: could not be taken", p.display())))
    }

    fn unlock(&self) {
        let _ = fs::remove_file(self.root.join(LOCK));
    }

    /// Runs `f` under the lock, an interrupted operation finished first (its lines in front).
    fn locked<T>(&self, f: impl FnOnce(&mut Vec<String>) -> Result<T, ModError>) -> Result<(T, Vec<String>), ModError> {
        self.lock()?;
        let mut lines = Vec::new();
        let r = self.resume_locked(&mut lines).and_then(|()| f(&mut lines));
        if !matches!(r, Err(ModError::Crashed)) {
            self.unlock();
        }
        r.map(|t| (t, lines))
    }

    /// An interrupted operation carried on to its end (nothing when there is none).
    ///
    /// # Errors
    /// A step still fails (its reason); the lock is held.
    pub fn resume(&self) -> Result<Vec<String>, ModError> {
        self.locked(|_| Ok(())).map(|((), l)| l)
    }

    fn resume_locked(&self, lines: &mut Vec<String>) -> Result<(), ModError> {
        let p = self.root.join(OP);
        if !p.is_file() {
            return Ok(());
        }
        let op: Op = fs::read(&p)
            .ok()
            .and_then(|d| serde_json::from_slice(&d).ok())
            .filter(|o: &Op| o.format == OP_FORMAT)
            .ok_or_else(|| {
                ModError::Io(format!(
                    "{}: a damaged operation journal - the mods named in `bfbc2 mods status` may be half done; \
                     delete it after checking",
                    p.display()
                ))
            })?;
        lines.push(format!("an interrupted {} ({}) is finished first", op.action, op.what));
        self.run_steps(&op, lines)
    }

    /// Writes the operation and carries it out.
    fn run(&self, op: &Op, lines: &mut Vec<String>) -> Result<(), ModError> {
        self.step("op")?;
        self.write_json(&self.root.join(OP), op)?;
        self.run_steps(op, lines)
    }

    fn run_steps(&self, op: &Op, lines: &mut Vec<String>) -> Result<(), ModError> {
        for st in &op.steps {
            self.run_step(st, &op.id, lines)?;
        }
        self.step("op done")?;
        let p = self.root.join(OP);
        fs::remove_file(&p).map_err(|e| ioe(&p, &e))?;
        self.gc()?;
        Ok(())
    }

    /// Blobs no stash record or operation names any more are deleted; empty folders under `folders` removed.
    fn gc(&self) -> Result<(), ModError> {
        let mut used = BTreeSet::new();
        for s in self.owner_stashes()?.values() {
            for c in &s.contributions {
                used.insert(c.value.sha.clone());
                if let Some(p) = &c.prior {
                    used.insert(p.sha.clone());
                }
            }
        }
        if let Ok(rd) = fs::read_dir(self.root.join("blobs")) {
            for e in rd.flatten() {
                let f = e.file_name().to_string_lossy().into_owned();
                if f.strip_suffix(".bin").is_some_and(|s| !used.contains(s)) || f.contains(".tmp-") {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
        remove_empty_dirs(&self.root.join("folders"));
        Ok(())
    }

    // ---------------------------------------------------------------------------------------------- steps

    #[allow(clippy::too_many_lines, reason = "one arm per step kind")]
    fn run_step(&self, st: &Step, op: &str, lines: &mut Vec<String>) -> Result<(), ModError> {
        match st {
            Step::StashOwner { entry } => {
                let name = owner_stash_name(&entry.owner);
                let mut cur = self
                    .owner_stashes()?
                    .remove(&entry.owner)
                    .unwrap_or_else(|| OwnerStash {
                        contributions: Vec::new(),
                        ..entry.clone()
                    });
                let mut changed = false;
                for c in &entry.contributions {
                    if !cur.contributions.iter().any(|x| x.file.eq_ignore_ascii_case(&c.file)) {
                        cur.contributions.push(c.clone());
                        changed = true;
                    }
                }
                op.clone_into(&mut cur.op);
                if changed || !self.stash_path(&name).is_file() {
                    self.step(&format!("stash {}", entry.owner))?;
                    self.write_json(&self.stash_path(&name), &cur)?;
                }
            }
            Step::RestatePrior { owner, file, sha } => {
                let layers = self.ledger().file_layers(file).map_err(from_ledger)?;
                let Some((_, c)) = layers.iter().find(|(o, _)| o == owner) else {
                    return Ok(());
                };
                if c.get("prior").and_then(|p| p.get("sha")).and_then(Value::as_str) == Some(sha.as_str()) {
                    return Ok(());
                }
                let prior = self.blob(sha)?;
                self.ledger()
                    .restate(owner, file, false, Some(&prior), "")
                    .map_err(from_ledger)?;
                lines.push(format!(
                    "{file}: {owner}'s keys go back, the edits made since it was installed are kept"
                ));
            }
            Step::Uninstall { owner, files } => {
                let held = self.ledger().owner_files(owner).map_err(from_ledger)?;
                let mine: Vec<String> = files
                    .iter()
                    .filter(|f| held.iter().any(|h| h.eq_ignore_ascii_case(f)))
                    .cloned()
                    .collect();
                if mine.is_empty() {
                    return Ok(());
                }
                let p = self
                    .ledger()
                    .uninstall_files(owner, &mine, false)
                    .map_err(from_ledger)?;
                for w in &p.writes {
                    lines.push(format!("{owner}: {w}"));
                }
            }
            Step::MoveOut { entry } => {
                let name = folder_stash_name(&entry.folder);
                if !self.stash_path(&name).is_file() {
                    self.step(&format!("stash {}", entry.folder))?;
                    let mut e = entry.clone();
                    op.clone_into(&mut e.op);
                    self.write_json(&self.stash_path(&name), &e)?;
                }
                for f in &entry.files {
                    let (src, dst) = (self.path(&f.rel), self.kept_path(&f.rel));
                    if src.is_file() {
                        self.step(&format!("move out {}", f.rel))?;
                        if let Some(d) = dst.parent() {
                            fs::create_dir_all(d).map_err(|e| ioe(d, &e))?;
                        }
                        if dst.is_file() {
                            fs::remove_file(&dst).map_err(|e| ioe(&dst, &e))?;
                        }
                        move_file(&src, &dst)?;
                    }
                }
                remove_empty_dirs(&self.path(&entry.folder));
                lines.push(format!("{}: moved aside ({} files)", entry.folder, entry.files.len()));
            }
            Step::Install {
                owner,
                derived,
                note,
                files,
            } => {
                let led = self.ledger();
                let mut need = Vec::new();
                for f in files {
                    let lay = led.file_layers(&f.file).map_err(from_ledger)?;
                    let done = lay.last().is_some_and(|(o, c)| {
                        o == owner && c.get("value").and_then(|v| v.get("sha")).and_then(Value::as_str) == Some(&f.sha)
                    });
                    if !done {
                        need.push((f.file.clone(), self.blob(&f.sha)?));
                    }
                }
                if !need.is_empty() {
                    self.ledger()
                        .install_keep(owner, &need, *derived, note, false)
                        .map_err(from_ledger)?;
                    for (f, _) in &need {
                        lines.push(format!("{owner}: write {f}"));
                    }
                }
                for f in files {
                    if let Some(t) = f.mtime_ns {
                        let p = self.path(&f.file);
                        if mtime_ns(&p) != Some(t) && sha256::file(&p.to_string_lossy()).ok().as_deref() == Some(&f.sha)
                        {
                            self.step(&format!("time {}", f.file))?;
                            set_mtime(&p, t)?;
                        }
                    }
                }
            }
            Step::MoveIn { folder } => {
                let name = folder_stash_name(folder);
                let Some(entry) = self.folder_stashes()?.remove(&folder.to_lowercase()) else {
                    return Ok(());
                };
                for f in &entry.files {
                    let (src, dst) = (self.kept_path(&f.rel), self.path(&f.rel));
                    if src.is_file() {
                        self.step(&format!("move in {}", f.rel))?;
                        if let Some(d) = dst.parent() {
                            fs::create_dir_all(d).map_err(|e| ioe(d, &e))?;
                        }
                        move_file(&src, &dst)?;
                    }
                }
                self.step(&format!("unstash {folder}"))?;
                let p = self.stash_path(&name);
                fs::remove_file(&p).map_err(|e| ioe(&p, &e))?;
                lines.push(format!("{folder}: back ({} files)", entry.files.len()));
            }
            Step::Unstash { owner, files } => {
                let Some(mut s) = self.owner_stashes()?.remove(owner) else {
                    return Ok(());
                };
                let before = s.contributions.len();
                s.contributions
                    .retain(|c| !files.iter().any(|f| f.eq_ignore_ascii_case(&c.file)));
                if s.contributions.len() == before {
                    return Ok(());
                }
                self.step(&format!("unstash {owner}"))?;
                let p = self.stash_path(&owner_stash_name(owner));
                if s.contributions.is_empty() {
                    fs::remove_file(&p).map_err(|e| ioe(&p, &e))?;
                } else {
                    self.write_json(&p, &s)?;
                }
            }
            Step::Setting {
                file,
                section,
                key,
                value,
            } => self.set_through(file, section, key, value, lines)?,
            Step::Drop { name } => {
                let p = self.stash_path(name);
                if let Some(f) = self
                    .folder_stashes()?
                    .values()
                    .find(|f| folder_stash_name(&f.folder) == *name)
                {
                    for x in &f.files {
                        let k = self.kept_path(&x.rel);
                        if k.is_file() {
                            self.step(&format!("drop {}", x.rel))?;
                            fs::remove_file(&k).map_err(|e| ioe(&k, &e))?;
                        }
                    }
                }
                if p.is_file() {
                    self.step(&format!("drop {name}"))?;
                    fs::remove_file(&p).map_err(|e| ioe(&p, &e))?;
                    lines.push(format!("{name}: its kept copy dropped"));
                }
            }
            Step::ManifestFix => self.manifest_fix(lines)?,
        }
        Ok(())
    }

    /// `[section] key=value` in the file (through every layer the ledger keeps) and in every kept copy of it.
    fn set_through(
        &self,
        file: &str,
        section: &str,
        key: &str,
        value: &str,
        lines: &mut Vec<String>,
    ) -> Result<(), ModError> {
        let edit = |d: &[u8]| -> Vec<u8> {
            match std::str::from_utf8(d) {
                Ok(t) => ini::set(t, section, key, value).into_bytes(),
                Err(_) => d.to_vec(),
            }
        };
        if self.path(file).is_file() && self.ledger().edit_through(file, &edit).map_err(from_ledger)? {
            lines.push(format!("{file}: [{section}] {key}={value}"));
        }
        for (owner, mut s) in self.owner_stashes()? {
            let mut changed = false;
            for c in &mut s.contributions {
                if !c.file.eq_ignore_ascii_case(file) {
                    continue;
                }
                let v = self.blob(&c.value.sha)?;
                let nv = edit(&v);
                if nv != v {
                    c.value = Shasize {
                        sha: self.put_blob(&nv)?,
                        size: nv.len() as u64,
                    };
                    c.mtime_ns = None;
                    changed = true;
                }
                if let Some(p) = &c.prior {
                    let pv = self.blob(&p.sha)?;
                    let np = edit(&pv);
                    if np != pv {
                        c.prior = Some(Shasize {
                            sha: self.put_blob(&np)?,
                            size: np.len() as u64,
                        });
                        changed = true;
                    }
                }
            }
            if changed {
                self.step(&format!("stash {owner}"))?;
                self.write_json(&self.stash_path(&owner_stash_name(&owner)), &s)?;
                lines.push(format!("{file}: {owner}'s kept copy has [{section}] {key}={value} too"));
            }
        }
        Ok(())
    }

    /// The opt-out's bundleManifest repair (`bc2_install::run`'s revert): a line without its third field gets `\t1`,
    /// as owner `manifestfix`.
    fn manifest_fix(&self, lines: &mut Vec<String>) -> Result<(), ModError> {
        const BM: &str = "Dist/win32/bundleManifest";
        let Ok(d) = fs::read(self.path(BM)) else { return Ok(()) };
        let text = crate::pytext::latin1(&d);
        let ls: Vec<&str> = text.split('\n').collect();
        if !ls.iter().any(|l| crate::text::manifest_line_broken(l)) {
            return Ok(());
        }
        let fixed: Vec<String> = ls
            .iter()
            .map(|l| {
                if crate::text::manifest_line_broken(l) {
                    format!("{l}\t1")
                } else {
                    (*l).to_owned()
                }
            })
            .collect();
        let data = crate::pytext::to_latin1(&fixed.join("\n"))
            .ok_or_else(|| ModError::Io("bundleManifest: a character outside latin-1".to_owned()))?;
        self.ledger()
            .install_keep("manifestfix", &[(BM.to_owned(), data)], true, "", false)
            .map_err(from_ledger)?;
        lines.push("kept the bundleManifest repair (levels/ons_mp_005gr/terrain's third field)".to_owned());
        Ok(())
    }
}

// ------------------------------------------------------------------------------------------------- files

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// A new operation id.
fn new_id() -> String {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_nanos());
    let tag = sha256::hex(format!("{n}-{}", std::process::id()).as_bytes());
    format!("{}-{}", now_secs(), tag.get(..8).unwrap_or_default())
}

/// A file's write time in nanoseconds since 1970.
#[must_use]
pub fn mtime_ns(p: &Path) -> Option<u64> {
    let t = fs::metadata(p).and_then(|m| m.modified()).ok()?;
    t.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
}

fn set_mtime(p: &Path, ns: u64) -> Result<(), ModError> {
    let t = UNIX_EPOCH + Duration::from_nanos(ns);
    let f = fs::OpenOptions::new().write(true).open(p).map_err(|e| ioe(p, &e))?;
    f.set_modified(t).map_err(|e| ioe(p, &e))
}

/// A rename on the same volume (retried while another process holds the file a moment).
fn move_file(src: &Path, dst: &Path) -> Result<(), ModError> {
    atomic::replace_with(src, dst, |a, b| fs::rename(a, b))
        .map_err(|e| ModError::Io(format!("{} -> {}: {e}", src.display(), dst.display())))
}

fn remove_empty_dirs(top: &Path) {
    let Ok(rd) = fs::read_dir(top) else { return };
    for e in rd.flatten() {
        if e.path().is_dir() {
            remove_empty_dirs(&e.path());
        }
    }
    let _ = fs::remove_dir(top);
}

/// Every file under a folder, game-relative (`/`), sorted.
fn tree(game: &Path, folder: &str) -> Vec<String> {
    let top = folder.split('/').fold(game.to_path_buf(), |p, x| p.join(x));
    let mut out = Vec::new();
    let mut stack = vec![top];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let ft = e.file_type();
            if ft.as_ref().is_ok_and(fs::FileType::is_symlink) {
                continue;
            }
            if p.is_dir() {
                stack.push(p);
            } else if let Ok(r) = p.strip_prefix(game) {
                out.push(r.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}
