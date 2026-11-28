//! Project OPFOR's whole-file stage operations on the install ledger (`opforcore.stages.Stages`, through
//! `console_common`'s `check_install` / `install` / `check_revert` / `revert` / `installed` / `source_bundle`), for an
//! installer that owns shared files as one owner: every operation first rolls an interrupted commit back (a kill, a
//! power cut) and refuses files that still carry the markers of before the ledger; its refusals read as OPFOR's.
//!
//! The WW2 install's run ([`crate::run`], round OR1) keeps its own copies of these steps; the map export's install
//! ([`crate::mapedit`], round OR6) uses this module.

use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;

use bc2_gamefs::ledger::{Interrupt, Ledger, LedgerError, norm_rel};
use bc2_gamefs::sha256;

use crate::run::legacy_markers;

/// An operation stopped (Python's `SystemExit` / `BackupRefused`): the message.
pub type Stop = String;

const MIGRATE_HINT: &str = r"python src\tools\opforcore\migrate.py --apply";

/// How a run stops at its n-th write step, as a kill would (tests): the process ends (exit 9), or - in a test inside
/// one process - the step fails with the ledger's lock and journal left as a crash leaves them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrashAt {
    /// `BC2_INSTALL_CRASH_AT=<n>`: the process exits (code 9) before the n-th write step.
    Exit(u32),
    /// The n-th write step returns [`CRASHED`] (the caller's test then plays the dead process).
    Return(u32),
}

/// The stop a [`CrashAt::Return`] step gives.
pub const CRASHED: &str = "stopped by the test hook (a kill at this write step)";

/// The write steps of one run and where it stops.
#[derive(Debug, Clone, Default)]
pub struct Steps {
    at: Option<CrashAt>,
    n: Rc<Cell<u32>>,
}

impl Steps {
    /// `BC2_INSTALL_CRASH_AT` read from the environment.
    #[must_use]
    pub fn from_env() -> Self {
        Self::new(
            std::env::var("BC2_INSTALL_CRASH_AT")
                .ok()
                .and_then(|v| v.trim().parse().ok())
                .map(CrashAt::Exit),
        )
    }

    /// A run stopping at `at` (None: never).
    #[must_use]
    pub fn new(at: Option<CrashAt>) -> Self {
        Self {
            at,
            n: Rc::new(Cell::new(0)),
        }
    }

    /// The write steps counted so far.
    #[must_use]
    pub fn count(&self) -> u32 {
        self.n.get()
    }

    /// Counts a write step outside the ledger.
    ///
    /// # Errors
    /// [`CRASHED`] at the step a [`CrashAt::Return`] names.
    pub fn step(&self, label: &str) -> Result<(), Stop> {
        if hit(&self.n, self.at, label) {
            return Err(CRASHED.to_owned());
        }
        Ok(())
    }
}

/// Counts a step; true when a [`CrashAt::Return`] stops here (an [`CrashAt::Exit`] ends the process).
fn hit(n: &Cell<u32>, at: Option<CrashAt>, label: &str) -> bool {
    n.set(n.get() + 1);
    match at {
        Some(CrashAt::Exit(k)) if k == n.get() => {
            eprintln!("BC2_INSTALL_CRASH_AT={k}: stopped before {label}");
            std::process::exit(9);
        }
        Some(CrashAt::Return(k)) => k == n.get(),
        _ => false,
    }
}

/// The ledger operations of one game copy, as `opforcore.stages` does them.
#[derive(Debug)]
pub struct Stages {
    game: String,
    root: Option<String>,
    steps: Steps,
    resumed: bool,
}

/// `os.path.abspath` (Windows: `GetFullPathNameW`).
#[must_use]
pub fn abspath(p: &str) -> String {
    std::path::absolute(p).map_or_else(|_| p.to_owned(), |a| a.to_string_lossy().into_owned())
}

fn refused(what: &str, c: &[String]) -> Stop {
    format!(
        "{what} refused - nothing was written:\n{}",
        c.iter().map(|x| format!("  {x}")).collect::<Vec<_>>().join("\n")
    )
}

impl Stages {
    /// The game copy's ledger (`<game>\opfor\ledger`), its write steps counted by `steps`.
    #[must_use]
    pub fn new(game: &str, steps: Steps) -> Self {
        Self {
            game: game.to_owned(),
            root: None,
            steps,
            resumed: false,
        }
    }

    /// The ledger kept in `root` instead of `<game>\opfor\ledger` (tests, OPFOR's `Stages(game, root)`).
    #[must_use]
    pub fn with_root(game: &str, root: &str, steps: Steps) -> Self {
        Self {
            root: Some(root.to_owned()),
            ..Self::new(game, steps)
        }
    }

    /// The game copy (as given).
    #[must_use]
    pub fn game(&self) -> &str {
        &self.game
    }

    /// The write steps.
    #[must_use]
    pub fn steps(&self) -> &Steps {
        &self.steps
    }

    /// The game copy's ledger, its write steps counted.
    #[must_use]
    pub fn ledger(&self) -> Ledger {
        let mut led = match &self.root {
            Some(r) => Ledger::with_root(Path::new(&self.game), Path::new(&abspath(r))),
            None => Ledger::new(Path::new(&self.game)),
        };
        if self.steps.at.is_some() {
            let (n, at) = (Rc::clone(&self.steps.n), self.steps.at);
            led.set_hook(Some(Box::new(move |label: &str| {
                if hit(&n, at, label) {
                    Err(Interrupt::Crash)
                } else {
                    Ok(())
                }
            })));
        }
        led
    }

    /// A game file's ledger name (`Package/async/startup-00.fbrb`).
    ///
    /// # Errors
    /// The file is not inside the game copy.
    pub fn rel(&self, path: &str) -> Result<String, Stop> {
        let full = abspath(path);
        let root = abspath(&self.game);
        let r = crate::pytext::relpath(&full, &root);
        if r.starts_with("..") || Path::new(&r).is_absolute() {
            return Err(format!("{path}: not inside the game copy {root}"));
        }
        norm_rel(&r).map_err(|e| e.to_string())
    }

    /// `Stages.resume` once a run, then `_ready`: an interrupted commit rolled back (the lines say what was), and
    /// the files' old markers refused.
    ///
    /// # Errors
    /// Another install runs (a live lock), the roll-back failed, or a file has old markers.
    pub fn ready(&mut self, paths: &[&str], log: &mut dyn FnMut(&str)) -> Result<Vec<String>, Stop> {
        if !self.resumed {
            self.resume(log)?;
            self.resumed = true;
        }
        let mut out = Vec::new();
        for p in paths {
            let rel = self.rel(p)?;
            let full = self.ledger().path(&rel);
            let old = legacy_markers(&full);
            if !old.is_empty() {
                return Err(format!(
                    "{} has install markers from before the install ledger ({}): migrate them first - {MIGRATE_HINT} \
                     (nothing was written)",
                    full.display(),
                    old.join(", ")
                ));
            }
            out.push(rel);
        }
        Ok(out)
    }

    /// `Stages.resume`: an interrupted commit (its process gone) rolled back from the journal, a stale lock without
    /// one removed; what was rolled back (empty: nothing to do, or only a lock removed).
    ///
    /// # Errors
    /// Another install runs (a live lock), or the roll-back failed.
    pub fn resume(&mut self, log: &mut dyn FnMut(&str)) -> Result<Vec<String>, Stop> {
        let mut led = self.ledger();
        if !(led.lock_present() || led.interrupted()) {
            return Ok(Vec::new());
        }
        if let Some(m) = led.live_lock() {
            return Err(m);
        }
        let done = led
            .recover(false)
            .map_err(|e| format!("an interrupted install could not be rolled back: {e}"))?;
        if done.is_empty() {
            log("  resumed: a lock left by an interrupted install was removed (nothing had been written)");
        }
        for d in &done {
            log(&format!("  resumed: an interrupted install was rolled back - {d}"));
        }
        Ok(done)
    }

    /// `Stages.installed`: the game files the owner holds (game-relative, its record's order).
    ///
    /// # Errors
    /// A damaged record.
    pub fn installed(&self, owner: &str) -> Result<Vec<String>, Stop> {
        self.ledger().owner_files(owner).map_err(|e| e.to_string())
    }

    /// `Stages.source`: the file a stage builds from - the ledger's copy from before the owner while the owner wrote
    /// the file last and it is still that write (a redo never builds on its own output), else the file.
    ///
    /// # Errors
    /// A damaged record, a missing blob.
    pub fn source(&mut self, owner: &str, path: &str, log: &mut dyn FnMut(&str)) -> Result<String, Stop> {
        self.ready(&[], log)?;
        let rel = self.rel(path)?;
        let led = self.ledger();
        let lay = led.file_layers(&rel).map_err(|e| e.to_string())?;
        let Some((top, c)) = lay.last() else {
            return Ok(led.path(&rel).to_string_lossy().into_owned());
        };
        let p = led.path(&rel);
        let wrote = c
            .get("value")
            .and_then(|v| v.get("sha"))
            .and_then(serde_json::Value::as_str);
        let now = if p.is_file() {
            sha256::file(&p.to_string_lossy()).ok()
        } else {
            None
        };
        if top == owner
            && now.is_some()
            && now.as_deref() == wrote
            && let Some(Some(blob)) = led.before(owner, &rel).map_err(|e| e.to_string())?
        {
            if !blob.is_file() {
                return Err(format!(
                    "{}: the ledger's copy of a file before an install is missing",
                    blob.display()
                ));
            }
            return Ok(blob.to_string_lossy().into_owned());
        }
        Ok(p.to_string_lossy().into_owned())
    }

    /// `Stages.check_install`: why installing the owner's write of each file would be refused (before any build
    /// work): a slot another owner holds in it, the owner buried under a later install, a file changed outside the
    /// ledger since its top owner wrote it.
    ///
    /// # Errors
    /// A damaged record, old markers, a live lock.
    pub fn check_install(
        &mut self,
        owner: &str,
        paths: &[&str],
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<String>, Stop> {
        let rels = self.ready(paths, log)?;
        let led = self.ledger();
        let mut out = Vec::new();
        for rel in rels {
            let mixed = led.slot_holders(&rel).map_err(|e| e.to_string())?;
            if let Some((o, what)) = mixed.into_iter().find(|(o, _)| o != owner) {
                out.push(format!("{rel}: {o} holds {what} in it (revert {o} first)"));
                continue;
            }
            let lay = led.file_layers(&rel).map_err(|e| e.to_string())?;
            if let Some(i) = lay.iter().position(|(o, _)| o == owner)
                && i + 1 != lay.len()
                && let Some((above, _)) = lay.get(i + 1)
            {
                out.push(format!(
                    "{rel}: {above} was installed after {owner} - revert {above} first (an install of {owner} now \
                     would go under a layer built on it)"
                ));
                continue;
            }
            if let Some((o, c)) = lay.last() {
                let p = led.path(&rel);
                let now = if p.is_file() {
                    sha256::file(&p.to_string_lossy()).ok()
                } else {
                    None
                };
                let wrote = c
                    .get("value")
                    .and_then(|v| v.get("sha"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or("");
                if now.as_deref() != Some(wrote) {
                    out.push(format!(
                        "{rel}: changed outside the install ledger since {o} installed it ({}, {o} wrote sha {}) - \
                         put back what {o} wrote, or revert {o}",
                        now.map_or_else(
                            || "missing".to_owned(),
                            |n| format!("sha {}", n.get(..12).unwrap_or(&n))
                        ),
                        wrote.get(..12).unwrap_or(wrote)
                    ));
                }
            }
        }
        Ok(out)
    }

    /// `cc.check_install`: [`Stages::check_install`] as a stop (`<owner>: not installed - ...`).
    ///
    /// # Errors
    /// The refusal in words.
    pub fn require_install(&mut self, owner: &str, paths: &[&str], log: &mut dyn FnMut(&str)) -> Result<(), Stop> {
        let why = self.check_install(owner, paths, log)?;
        if why.is_empty() {
            Ok(())
        } else {
            Err(format!("{owner}: not installed - {}", why.join("; ")))
        }
    }

    /// `cc.install`: one journalled commit of the owner's writes (game file, bytes; derived: built from the file as
    /// the owner found it), stacked on the files' owners, its other files kept.
    ///
    /// # Errors
    /// Any conflict (nothing written), a failed write (rolled back), the test hook.
    pub fn install(&mut self, owner: &str, files: &[(String, Vec<u8>)], log: &mut dyn FnMut(&str)) -> Result<(), Stop> {
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        let rels = self.ready(&paths, log)?;
        let items: Vec<(String, Vec<u8>)> = rels.into_iter().zip(files.iter().map(|(_, d)| d.clone())).collect();
        match self.ledger().install_keep(owner, &items, true, "", false) {
            Ok(_) => Ok(()),
            Err(LedgerError::Crashed) => Err(CRASHED.to_owned()),
            Err(LedgerError::Refused(c)) => Err(refused(&format!("install of {owner}"), &c)),
            Err(e) => Err(format!("install of {owner} refused: {e}")),
        }
    }

    fn uninstall(
        &mut self,
        owner: &str,
        paths: Option<&[&str]>,
        dry_run: bool,
        log: &mut dyn FnMut(&str),
    ) -> Result<Result<(), Vec<String>>, Stop> {
        let mine: Vec<String> = match paths {
            None => self
                .installed(owner)?
                .into_iter()
                .map(|r| self.ledger().path(&r).to_string_lossy().into_owned())
                .collect(),
            Some(p) => p.iter().map(|s| (*s).to_owned()).collect(),
        };
        let refs: Vec<&str> = mine.iter().map(String::as_str).collect();
        let names = self.ready(&refs, log)?;
        let mut led = self.ledger();
        let r = match paths {
            None => led.uninstall(owner, dry_run),
            Some(_) => led.uninstall_files(owner, &names, dry_run),
        };
        match r {
            Ok(_) => Ok(Ok(())),
            Err(LedgerError::Crashed) => Err(CRASHED.to_owned()),
            Err(LedgerError::Refused(c)) => Ok(Err(c)),
            Err(e) => Err(format!("revert of {owner} refused: {e}")),
        }
    }

    /// `cc.check_revert`: refused (`<owner>: not reverted - ...`, nothing written) when reverting the owner (its
    /// writes to `paths`; None: all) would be.
    ///
    /// # Errors
    /// The refusal in words.
    pub fn require_revert(
        &mut self,
        owner: &str,
        paths: Option<&[&str]>,
        log: &mut dyn FnMut(&str),
    ) -> Result<(), Stop> {
        let why = match self.uninstall(owner, paths, true, log) {
            Ok(Ok(())) => return Ok(()),
            Ok(Err(c)) => c,
            Err(m) => vec![m],
        };
        Err(format!("{owner}: not reverted - {}", why.join("; ")))
    }

    /// `cc.revert`: the owner's files (those of `paths`; None: all) back as they were before it, or spliced out from
    /// under an independent layer; one journalled commit.
    ///
    /// # Errors
    /// Any conflict (nothing written), a failed write (rolled back), the test hook.
    pub fn revert(&mut self, owner: &str, paths: Option<&[&str]>, log: &mut dyn FnMut(&str)) -> Result<(), Stop> {
        match self.uninstall(owner, paths, false, log)? {
            Ok(()) => Ok(()),
            Err(c) => Err(refused(&format!("revert of {owner}"), &c)),
        }
    }

    // ------------------------------------------------------------------------- the rest of opforcore.stages (OR15)

    /// The `file` layers of a game file, bottom first: (owner, contribution).
    fn layers_of(&self, rel: &str) -> Result<Vec<(String, serde_json::Value)>, Stop> {
        self.ledger().file_layers(rel).map_err(|e| e.to_string())
    }

    fn blob_of(&self, prior: Option<&serde_json::Value>) -> Result<Option<String>, Stop> {
        let Some(b) = prior.and_then(|p| p.get("blob")).and_then(serde_json::Value::as_str) else {
            return Ok(None);
        };
        let p = self.ledger().blob_path(b);
        if !p.is_file() {
            return Err(format!(
                "{}: the ledger's copy of a file before an install is missing",
                p.display()
            ));
        }
        Ok(Some(p.to_string_lossy().into_owned()))
    }

    fn here(&self, rel: &str) -> Option<String> {
        let p = self.ledger().path(rel);
        p.is_file().then(|| p.to_string_lossy().into_owned())
    }

    /// `Stages.is_top`: the owner wrote the file last and it is still that write (a redo would rebuild from
    /// [`Stages::before`]).
    ///
    /// # Errors
    /// Not a game file, a damaged record.
    pub fn is_top(&self, owner: &str, path: &str) -> Result<bool, Stop> {
        let rel = self.rel(path)?;
        let lay = self.layers_of(&rel)?;
        let Some((top, c)) = lay.last() else {
            return Ok(false);
        };
        let p = self.ledger().path(&rel);
        let wrote = c
            .get("value")
            .and_then(|v| v.get("sha"))
            .and_then(serde_json::Value::as_str);
        Ok(top == owner && p.is_file() && sha256::file(&p.to_string_lossy()).ok().as_deref() == wrote)
    }

    /// `Stages.owners`: the owners installed on the file, bottom first.
    ///
    /// # Errors
    /// Not a game file, a damaged record.
    pub fn owners(&self, path: &str) -> Result<Vec<String>, Stop> {
        Ok(self.layers_of(&self.rel(path)?)?.into_iter().map(|(o, _)| o).collect())
    }

    /// `Stages.before`: the file as it was before the owner installed it - the ledger's copy, None when the owner
    /// created it, the file itself when the owner holds nothing there (None when it is missing).
    ///
    /// # Errors
    /// Not a game file, a damaged record, the ledger's copy missing.
    pub fn before(&self, owner: &str, path: &str) -> Result<Option<String>, Stop> {
        let rel = self.rel(path)?;
        for (o, c) in self.layers_of(&rel)? {
            if o == owner {
                return self.blob_of(c.get("prior"));
            }
        }
        Ok(self.here(&rel))
    }

    /// `Stages.original`: the file before any recorded install (the bottom layer's prior; None when an install
    /// created it), the file itself when no owner holds it.
    ///
    /// # Errors
    /// As [`Stages::before`].
    pub fn original(&self, path: &str) -> Result<Option<String>, Stop> {
        let rel = self.rel(path)?;
        match self.layers_of(&rel)?.first() {
            Some((_, c)) => self.blob_of(c.get("prior")),
            None => Ok(self.here(&rel)),
        }
    }

    /// `stages.before_path` (for readers; never recovers, locks or writes): the old marker backup
    /// `<file>.opfor-<stage>-orig` while the game copy is not migrated, else the ledger's copy from before the owner,
    /// else the file itself.
    #[must_use]
    pub fn before_path(path: &str, owner: &str) -> String {
        let stage = owner.split(':').next().unwrap_or(owner);
        let legacy = format!("{path}.opfor-{stage}-orig");
        if Path::new(&legacy).is_file() {
            return legacy;
        }
        let st = Self::new(&game_of(path), Steps::new(None));
        let found = st.rel(path).and_then(|rel| {
            for (o, c) in st.layers_of(&rel)? {
                if o == owner {
                    return st.blob_of(c.get("prior"));
                }
            }
            Ok(None)
        });
        found.ok().flatten().unwrap_or_else(|| path.to_owned())
    }

    /// `Stages.restated`: who rebuilt the owner's file outside the ledger (`restate`'s why), empty when it is the
    /// owner's own write.
    ///
    /// # Errors
    /// Not a game file, a damaged record.
    pub fn restated(&self, owner: &str, path: &str) -> Result<String, Stop> {
        let rel = self.rel(path)?;
        Ok(self
            .layers_of(&rel)?
            .into_iter()
            .find(|(o, _)| o == owner)
            .and_then(|(_, c)| {
                c.pointer("/extra/restated")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned)
            })
            .unwrap_or_default())
    }

    /// `Stages.install` in full: one commit of the owner's writes (`derived`: built from the file as the owner found
    /// it; `keep_others`: the owner's other files stay as recorded, else they are reverted in the same commit).
    ///
    /// # Errors
    /// Any conflict (nothing written), a failed write (rolled back), the test hook.
    #[allow(clippy::too_many_arguments, reason = "OPFOR's keyword arguments")]
    pub fn install_with(
        &mut self,
        owner: &str,
        files: &[(String, Vec<u8>)],
        derived: bool,
        keep_others: bool,
        note: &str,
        dry_run: bool,
        log: &mut dyn FnMut(&str),
    ) -> Result<bc2_gamefs::ledger::Plan, Stop> {
        let paths: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
        let rels = self.ready(&paths, log)?;
        let items: Vec<(String, Vec<u8>)> = rels.into_iter().zip(files.iter().map(|(_, d)| d.clone())).collect();
        let mut led = self.ledger();
        let r = if keep_others {
            led.install_keep(owner, &items, derived, note, dry_run)
        } else {
            led.install(owner, &items, derived, note, dry_run)
        };
        match r {
            Ok(p) => Ok(p),
            Err(LedgerError::Crashed) => Err(CRASHED.to_owned()),
            Err(LedgerError::Refused(c)) => Err(refused(&format!("install of {owner}"), &c)),
            Err(e) => Err(format!("install of {owner} refused: {e}")),
        }
    }

    /// `Stages.check_revert`: why reverting the owner (its writes to `paths`; None: all) would be refused; empty when
    /// it would go through (a stop is one of the reasons).
    pub fn check_revert(&mut self, owner: &str, paths: Option<&[&str]>, log: &mut dyn FnMut(&str)) -> Vec<String> {
        match self.uninstall(owner, paths, true, log) {
            Ok(Ok(())) => Vec::new(),
            Ok(Err(c)) => c,
            Err(m) => vec![m],
        }
    }

    /// `Stages.restate`: the owner (on top of the file) takes the file as it is now for its write; `prior`
    /// `Some(..)`: the file before the owner becomes that too (`Some(None)`: there was none). Only the record is
    /// written.
    ///
    /// # Errors
    /// The owner does not hold the file or is not on top, a held lock, a failed write.
    #[allow(clippy::option_option, reason = "None: the prior kept; Some(None): there was none")]
    pub fn restate(
        &mut self,
        owner: &str,
        path: &str,
        prior: Option<Option<&[u8]>>,
        why: &str,
        log: &mut dyn FnMut(&str),
    ) -> Result<(), Stop> {
        let rel = self.ready(&[path], log)?.remove(0);
        match self
            .ledger()
            .restate(owner, &rel, prior.is_none(), prior.flatten(), why)
        {
            Ok(()) => Ok(()),
            Err(LedgerError::Refused(c)) => Err(c.join("; ")),
            Err(LedgerError::Crashed) => Err(CRASHED.to_owned()),
            Err(e) => Err(e.to_string()),
        }
    }

    /// `Stages.forget`: the owner dropped from the ledger, no game file touched.
    ///
    /// # Errors
    /// A held lock, a refusal, a failed write.
    pub fn forget(&mut self, owner: &str, log: &mut dyn FnMut(&str)) -> Result<bc2_gamefs::ledger::Plan, Stop> {
        self.ready(&[], log)?;
        self.ledger()
            .forget(owner, false, false)
            .map_err(|e| format!("{owner}: {e}"))
    }

    /// `Stages.forget_under`: every owner whose files all lie under the folders dropped (no file touched).
    ///
    /// # Errors
    /// An owner also holds a file outside them (nothing dropped); a folder outside the game copy.
    pub fn forget_under(
        &mut self,
        folders: &[&str],
        dry_run: bool,
        log: &mut dyn FnMut(&str),
    ) -> Result<Vec<String>, Stop> {
        let tops: Vec<String> = folders.iter().map(|f| self.rel(f)).collect::<Result<_, _>>()?;
        if !dry_run {
            self.ready(&[], log)?;
        }
        match self.ledger().forget_under(&tops, dry_run) {
            Ok(d) => Ok(d),
            Err(LedgerError::Refused(c)) => Err(c.join("; ")),
            Err(e) => Err(e.to_string()),
        }
    }

    /// `Stages.status`: per owner and file its state (ok / buried / drift / missing / kept), or only for `paths`.
    ///
    /// # Errors
    /// A path outside the game copy, the store cannot be read.
    pub fn status(&self, paths: &[&str]) -> Result<Vec<String>, Stop> {
        let want: Vec<String> = paths
            .iter()
            .map(|p| self.rel(p).map(|r| r.to_lowercase()))
            .collect::<Result<_, _>>()?;
        let entries = self.ledger().status_entries().map_err(|e| e.to_string())?;
        Ok(entries
            .into_iter()
            .filter(|e| want.is_empty() || want.contains(&e.file.to_lowercase()))
            .map(|e| {
                let mut l = format!("{:28} {:11} {}", e.owner, e.state, e.file);
                if !e.detail.is_empty() {
                    l.push_str("  (");
                    l.push_str(&e.detail);
                    l.push(')');
                }
                l
            })
            .collect())
    }
}

/// `stages.game_of`: the game copy a game file is in - the folder holding its `Package` / `Dist` / `Output`; the
/// file's folder for a file at the top (`opfor.ini`, `maps.ini`).
#[must_use]
pub fn game_of(path: &str) -> String {
    let full = abspath(path);
    let parts: Vec<&str> = full.split(['\\', '/']).collect();
    for i in (1..parts.len().saturating_sub(1)).rev() {
        if parts
            .get(i)
            .is_some_and(|p| ["package", "dist", "output"].contains(&p.to_lowercase().as_str()))
        {
            return parts
                .get(..i)
                .map_or_else(String::new, |p| p.join(std::path::MAIN_SEPARATOR_STR));
        }
    }
    Path::new(&full)
        .parent()
        .map_or_else(String::new, |p| p.to_string_lossy().into_owned())
}
