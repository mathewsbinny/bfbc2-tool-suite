//! One install ledger (round OR15, audit AU5-ARC-3): the reading, health check and forgetting that Project OPFOR's
//! `opforcore.ledger` did, so this crate is the only implementation of the store. Until OR15 three programs read and
//! wrote it - `opforcore.ledger` (OPFOR's stages), `bfbc2core.gameledger` (the suite's NavKit / stock-MP / texqa) and
//! this module - all in format `opfor-ledger/1`; the two Python ones now call this one (`bc2_native.ledger_call`).
//!
//! - [`validate`]: every rule either Python reader refused a record by (`Record.from_json`,
//!   `Contribution.from_json`) and this crate's own, so a ledger any of the three wrote is read here or refused in
//!   words, never half-read. [`Ledger::records`] refuses the first damaged record; [`Ledger::scan`] reports them all.
//! - [`Ledger::status_entries`]: OPFOR's `Ledger.status()` (per contribution: ok / buried / drift / missing, a
//!   damaged record as `corrupt`, an interrupted commit as `interrupted`).
//! - [`Ledger::check`]: a read-only report for `bfbc2 gameledger check` and `bfbc2 doctor` - the records, the blobs a
//!   revert needs (present, their sha256 right), the journal and the lock.
//! - [`Ledger::forget`] / [`Ledger::forget_under`]: OPFOR's `Ledger.forget` and `Stages.forget_under` - an owner
//!   dropped without touching a game file (a damaged record too), owners stacked on it keeping their priors.
//!
//! The slot kinds (`mft_block`, `manifest_line`, `ini`, `bundle_entry`, `bundle_format`, `tree`) were only ever
//! written by OPFOR's own tests: every install that shipped wrote whole files (`Stages.install`'s `t.file`, the
//! migration, NavKit, stock-MP; spec XINST-LED-9). They are read, validated, shown and kept as they are; a revert of
//! an owner holding one is refused in words and [`Ledger::forget`] drops it (the bytes stay).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::Path;

use serde_json::Value;

use super::{
    DIRTY, FORMAT, JOURNAL, Ledger, LedgerError, Plan, Records, ioe, new_txn, norm_rel, owner_file, read_opt, s,
    sha_of, valid_owner,
};
use crate::sha256;

/// The contribution kinds of format `opfor-ledger/1`.
pub const KINDS: [&str; 7] = [
    "mft_block",
    "manifest_line",
    "ini",
    "bundle_entry",
    "bundle_format",
    "file",
    "tree",
];

/// The text kinds (their value and prior are text).
const TEXT_KINDS: [&str; 3] = ["mft_block", "manifest_line", "ini"];

/// Why a record is not one: the first rule it breaks, in words (`None`: a well-formed record). `file_name` is the
/// record's file name, which must be its owner's.
#[must_use]
pub fn validate(v: &Value, file_name: &str) -> Option<String> {
    let Some(obj) = v.as_object() else {
        return Some("not a JSON object".to_owned());
    };
    if s(v, "format") != Some(FORMAT) {
        return Some(format!(
            "format {}, expected {FORMAT}",
            obj.get("format").map_or_else(|| "missing".to_owned(), Value::to_string)
        ));
    }
    let owner = s(v, "owner").unwrap_or_default();
    if !valid_owner(owner) {
        return Some(format!(
            "a bad owner {}",
            obj.get("owner").map_or_else(|| "(none)".to_owned(), Value::to_string)
        ));
    }
    if owner_file(owner) != file_name {
        return Some(format!("its owner {owner} does not match its file name"));
    }
    for k in ["txn", "installed"] {
        if s(v, k).is_none() {
            return Some(format!("no {k:?}"));
        }
    }
    let Some(cs) = v.get("contributions").and_then(Value::as_array) else {
        return Some("no contributions list".to_owned());
    };
    for (i, c) in cs.iter().enumerate() {
        if let Some(why) = contribution_fault(c) {
            return Some(format!("contribution {}: {why}", i + 1));
        }
    }
    None
}

/// `Contribution.from_json`'s rules (and a whole file's sha, this crate's).
fn contribution_fault(c: &Value) -> Option<String> {
    if !c.is_object() {
        return Some("not an object".to_owned());
    }
    let kind = s(c, "kind").unwrap_or_default();
    if !KINDS.contains(&kind) {
        return Some(format!(
            "an unknown kind {}",
            c.get("kind").map_or_else(|| "(none)".to_owned(), Value::to_string)
        ));
    }
    let (Some(file), Some(_)) = (s(c, "file"), s(c, "slot")) else {
        return Some(format!("a {kind} contribution without its file / slot"));
    };
    if norm_rel(file).ok().as_deref() != Some(file) {
        return Some(format!("a {kind} contribution with a bad file name {file:?}"));
    }
    match c.get("below") {
        None | Some(Value::Null) => {}
        Some(Value::String(b)) if valid_owner(b) => {}
        Some(b) => return Some(format!("a {kind} contribution with a bad below {b}")),
    }
    let extra = match c.get("extra") {
        None | Some(Value::Null) => None,
        Some(e) if e.is_object() => Some(e),
        Some(_) => return Some(format!("a {kind} contribution with a bad extra")),
    };
    let (value, prior) = (
        c.get("value").unwrap_or(&Value::Null),
        c.get("prior").unwrap_or(&Value::Null),
    );
    if kind == "ini" && !(extra.and_then(|e| s(e, "section")).is_some() && extra.and_then(|e| s(e, "key")).is_some()) {
        return Some("an ini contribution without its section / key".to_owned());
    }
    if TEXT_KINDS.contains(&kind) {
        if !(value.is_string() && (prior.is_null() || prior.is_string())) {
            return Some(format!("a {kind} contribution whose value / prior is not text"));
        }
        return None;
    }
    if !value.is_object() {
        return Some(format!("a {kind} contribution without a value"));
    }
    if matches!(kind, "bundle_entry" | "file") && !prior.is_null() && s(prior, "blob").is_none() {
        return Some(format!("a {kind} contribution whose prior names no blob"));
    }
    if kind == "tree" && !value.get("files").is_some_and(Value::is_object) {
        return Some("a tree contribution without its files".to_owned());
    }
    if kind == "file" && sha_of(value).is_none() {
        return Some("a file contribution without its value's sha".to_owned());
    }
    None
}

/// One line of [`Ledger::status_entries`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The owner (`?` for a journal that cannot be read).
    pub owner: String,
    /// The contribution's kind, or `record` (a damaged record), `journal` (an interrupted commit).
    pub kind: String,
    /// The game file (the record's or journal's file name for those).
    pub file: String,
    /// The slot (`''` for whole files).
    pub slot: String,
    /// `ok`, `buried`, `drift`, `missing`, `kept` (a slot kind), `corrupt`, `interrupted`.
    pub state: String,
    /// More in words (`under <owner>`, why a record is damaged, ...).
    pub detail: String,
}

/// What [`Ledger::check`] found.
#[derive(Debug, Clone, Default)]
pub struct Report {
    /// Owners read, with their contribution counts.
    pub owners: BTreeMap<String, usize>,
    /// Contributions by kind.
    pub kinds: BTreeMap<String, usize>,
    /// Damaged records: (file name, why).
    pub damaged: Vec<(String, String)>,
    /// Blobs a revert needs that are missing or damaged, in words.
    pub blobs: Vec<String>,
    /// An interrupted commit (journal, `.dirty`), in words.
    pub interrupted: Option<String>,
    /// The lock while one is there, in words.
    pub lock: Option<String>,
    /// The slot-kind contributions kept as they are (a revert of their owner is refused): `<owner> <kind> <file>`.
    pub kept: Vec<String>,
}

impl Report {
    /// Whether every record reads and every revert has its bytes (a held lock or kept slots are no fault).
    #[must_use]
    pub fn healthy(&self) -> bool {
        self.damaged.is_empty() && self.blobs.is_empty() && self.interrupted.is_none()
    }

    /// The report in plain lines.
    #[must_use]
    pub fn lines(&self) -> Vec<String> {
        let n: usize = self.owners.values().sum();
        let mut out = vec![format!(
            "{} owner(s), {n} contribution(s){}",
            self.owners.len(),
            if self.kinds.is_empty() {
                String::new()
            } else {
                format!(
                    " ({})",
                    self.kinds
                        .iter()
                        .map(|(k, c)| format!("{k} {c}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
        )];
        for (f, why) in &self.damaged {
            out.push(format!("damaged record {f}: {why}"));
        }
        out.extend(self.blobs.iter().map(|b| format!("missing bytes: {b}")));
        if let Some(i) = &self.interrupted {
            out.push(format!("interrupted: {i}"));
        }
        if let Some(l) = &self.lock {
            out.push(format!("lock: {l}"));
        }
        out.extend(self.kept.iter().map(|k| {
            format!(
                "kept as written: {k} (a slot kind no shipped install wrote: its revert is refused, forget drops it)"
            )
        }));
        out.push(if self.healthy() {
            "ledger: healthy".to_owned()
        } else {
            "ledger: needs attention (see above)".to_owned()
        });
        out
    }
}

fn key(c: &Value) -> (String, String, String) {
    (
        s(c, "kind").unwrap_or_default().to_owned(),
        s(c, "file").unwrap_or_default().to_lowercase(),
        s(c, "slot").unwrap_or_default().to_owned(),
    )
}

impl Ledger {
    /// Every record that reads, by owner, and the damaged ones (file name, why): nothing is left out unreported.
    ///
    /// # Errors
    /// The store folder cannot be listed, or a record cannot be read from disk.
    pub fn scan(&self) -> Result<(Records, BTreeMap<String, String>), LedgerError> {
        let mut out = Records::new();
        let mut bad = BTreeMap::new();
        let rd = match fs::read_dir(&self.root) {
            Ok(rd) => rd,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok((out, bad)),
            Err(e) => return Err(ioe(&self.root, &e)),
        };
        let mut names: Vec<String> = rd
            .flatten()
            .filter(|e| e.path().is_file())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|f| Path::new(f).extension().is_some_and(|e| e.eq_ignore_ascii_case("json")) && !f.starts_with('.'))
            .collect();
        names.sort();
        for f in names {
            let p = self.root.join(&f);
            let data = fs::read(&p).map_err(|e| ioe(&p, &e))?;
            let v: Value = match serde_json::from_slice(&data) {
                Ok(v) => v,
                Err(e) => {
                    bad.insert(f, format!("not JSON: {e}"));
                    continue;
                }
            };
            match validate(&v, &f) {
                Some(why) => {
                    bad.insert(f, why);
                }
                None => {
                    out.insert(s(&v, "owner").unwrap_or_default().to_owned(), v);
                }
            }
        }
        Ok((out, bad))
    }

    /// The refusal of a damaged record, in words.
    pub(super) fn damaged(&self, file: &str, why: &str) -> LedgerError {
        LedgerError::Damaged(format!(
            "damaged ledger record: {} ({why}) - restore it from a backup, or drop it with bfbc2 gameledger forget \
             <game> {} (no game file is touched)",
            self.root.join(file).display(),
            file.strip_suffix(".json").unwrap_or(file).replace('~', ":")
        ))
    }

    fn interrupted_entry(&self) -> Option<Entry> {
        let jp = self.root.join(JOURNAL);
        if !jp.is_file() && !self.root.join(DIRTY).is_file() {
            return None;
        }
        let j: Option<Value> = fs::read(&jp).ok().and_then(|d| serde_json::from_slice(&d).ok());
        let (owner, detail) = match &j {
            Some(j) => (
                s(j, "owner").unwrap_or("?").to_owned(),
                format!(
                    "{} txn {}: run bfbc2 gameledger recover <game>",
                    s(j, "action").unwrap_or("?"),
                    s(j, "txn").unwrap_or("?")
                ),
            ),
            None if jp.is_file() => ("?".to_owned(), format!("{}: damaged journal", jp.display())),
            None => (
                "?".to_owned(),
                "a rollback failed: run bfbc2 gameledger recover <game>".to_owned(),
            ),
        };
        Some(Entry {
            owner,
            kind: "journal".to_owned(),
            file: JOURNAL.to_owned(),
            slot: String::new(),
            state: "interrupted".to_owned(),
            detail,
        })
    }

    /// OPFOR's `Ledger.status()`: per contribution its state - `ok` (the file holds the owner's write), `buried`
    /// (another owner's sits on it), `drift` (changed outside the ledger: a revert would refuse it), `missing`, or
    /// `kept` (a slot kind: kept as written); a damaged record is one `corrupt` entry (the healthy owners are still
    /// reported), an interrupted commit one `interrupted` entry, first.
    ///
    /// # Errors
    /// The store or a game file cannot be read.
    pub fn status_entries(&self) -> Result<Vec<Entry>, LedgerError> {
        let mut out: Vec<Entry> = self.interrupted_entry().into_iter().collect();
        let (recs, bad) = self.scan()?;
        for (f, why) in &bad {
            out.push(Entry {
                owner: f.strip_suffix(".json").unwrap_or(f).replace('~', ":"),
                kind: "record".to_owned(),
                file: f.clone(),
                slot: String::new(),
                state: "corrupt".to_owned(),
                detail: why.clone(),
            });
        }
        for (o, r) in &recs {
            for c in Self::contributions(r) {
                let kind = s(c, "kind").unwrap_or_default();
                let file = s(c, "file").unwrap_or_default();
                let above = recs.iter().find_map(|(o2, r2)| {
                    (o2 != o
                        && Self::contributions(r2)
                            .iter()
                            .any(|d| key(d) == key(c) && s(d, "below") == Some(o.as_str())))
                    .then(|| o2.clone())
                });
                let (state, detail) = if let Some(a) = above {
                    ("buried", format!("under {a}"))
                } else if kind != "file" {
                    ("kept", format!("{kind} slot: kept as written"))
                } else {
                    match read_opt(&self.path(file))? {
                        None => ("missing", String::new()),
                        Some(d) if Some(sha256::hex(&d).as_str()) == c.get("value").and_then(sha_of) => {
                            ("ok", String::new())
                        }
                        Some(_) => ("drift", "changed outside the ledger".to_owned()),
                    }
                };
                out.push(Entry {
                    owner: o.clone(),
                    kind: kind.to_owned(),
                    file: file.to_owned(),
                    slot: s(c, "slot").unwrap_or_default().to_owned(),
                    state: state.to_owned(),
                    detail,
                });
            }
        }
        Ok(out)
    }

    /// The ledger's health, read only: every record, the blobs every revert needs, the journal and the lock.
    ///
    /// # Errors
    /// The store cannot be listed.
    pub fn check(&self) -> Result<Report, LedgerError> {
        let (recs, bad) = self.scan()?;
        let mut rep = Report {
            damaged: bad.into_iter().collect(),
            interrupted: self.interrupted_entry().map(|e| e.detail),
            lock: self.lock_info().map(|i| {
                format!(
                    "pid {} since {}{}",
                    i.get("pid").map_or_else(|| "?".to_owned(), Value::to_string),
                    s(&i, "started").unwrap_or("?"),
                    if Self::lock_stale(&i) {
                        " - its process is gone (stale: the next install clears it)"
                    } else {
                        " - an install or revert is running"
                    }
                )
            }),
            ..Report::default()
        };
        let mut seen = BTreeSet::new();
        for (o, r) in &recs {
            let cs = Self::contributions(r);
            rep.owners.insert(o.clone(), cs.len());
            for c in cs {
                let kind = s(c, "kind").unwrap_or_default();
                *rep.kinds.entry(kind.to_owned()).or_default() += 1;
                if kind != "file" {
                    rep.kept
                        .push(format!("{o} {kind} {}", s(c, "file").unwrap_or_default()));
                }
                for k in ["prior", "value"] {
                    let Some(b) = c.get(k).and_then(|v| s(v, "blob")) else {
                        continue;
                    };
                    if seen.insert(b.to_owned())
                        && let Err(e) = self.blob(b)
                    {
                        rep.blobs.push(format!(
                            "{o}'s {} ({e}): its revert would be refused",
                            s(c, "file").unwrap_or_default()
                        ));
                    }
                }
            }
        }
        Ok(rep)
    }

    /// OPFOR's `Ledger.forget`: the owner dropped from the ledger without touching a game file - a record that
    /// cannot be read, or an install that cannot be reverted any more. `only_missing`: only its contributions whose
    /// file is gone. What it wrote stays in the game files as if written outside the ledger; an owner stacked on it
    /// keeps its prior (its revert gives back what the forgotten one wrote) and now sits on the owner below.
    ///
    /// # Errors
    /// [`LedgerError::Refused`] (only the whole of a damaged record can be forgotten), a held lock, an interrupted
    /// commit (dry run), a failed write (rolled back).
    pub fn forget(&mut self, owner: &str, only_missing: bool, dry_run: bool) -> Result<Plan, LedgerError> {
        if !valid_owner(owner) {
            return Err(LedgerError::Damaged(format!("not an owner name: {owner:?}")));
        }
        let mut plan = Plan {
            owner: owner.to_owned(),
            action: "forget".to_owned(),
            ..Plan::default()
        };
        if dry_run {
            if self.interrupted() {
                return Err(LedgerError::Locked(
                    "an interrupted commit: run bfbc2 gameledger recover <game> first".to_owned(),
                ));
            }
            self.forget_locked(owner, only_missing, true, &mut plan)?;
            return Ok(plan);
        }
        self.lock(&mut plan.notes)?;
        let r = self.forget_locked(owner, only_missing, false, &mut plan);
        if !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r.map(|()| plan)
    }

    fn forget_locked(
        &mut self,
        owner: &str,
        only_missing: bool,
        dry_run: bool,
        plan: &mut Plan,
    ) -> Result<(), LedgerError> {
        let (mut recs, bad) = self.scan()?;
        let f = owner_file(owner);
        let rec = recs.get(owner).cloned();
        let Some(rec_v) = rec.as_ref() else {
            let Some(why) = bad.get(&f) else {
                plan.notes.push("not in the ledger".to_owned());
                return Ok(());
            };
            if only_missing {
                return Err(LedgerError::Refused(vec![format!(
                    "{f} cannot be read ({why}): only the whole record can be forgotten"
                )]));
            }
            plan.writes.push(format!("record {f} removed"));
            if dry_run {
                return Ok(());
            }
            return self.forget_write(owner, None, &BTreeSet::new(), &recs, bad.len() > 1, plan);
        };
        let cs = Self::contributions(rec_v).to_vec();
        let gone = |c: &Value| !self.path(s(c, "file").unwrap_or_default()).exists();
        let (drop, keep): (Vec<Value>, Vec<Value>) = cs.into_iter().partition(|c| !only_missing || gone(c));
        if drop.is_empty() {
            plan.notes.push("no contribution whose file is gone".to_owned());
            return Ok(());
        }
        let mut dirty = BTreeSet::new();
        for (o, r) in &mut recs {
            if o == owner {
                continue;
            }
            let Some(arr) = r.get_mut("contributions").and_then(Value::as_array_mut) else {
                continue;
            };
            for d in arr.iter_mut() {
                if s(d, "below") != Some(owner) {
                    continue;
                }
                let Some(mine) = drop.iter().find(|c| key(c) == key(d)) else {
                    continue;
                };
                let below = mine.get("below").cloned().unwrap_or(Value::Null);
                plan.notes.push(format!(
                    "{o}: {} {} keeps its prior and now sits on {}",
                    s(d, "kind").unwrap_or_default(),
                    s(d, "file").unwrap_or_default(),
                    below.as_str().unwrap_or("no owner")
                ));
                if let Some(m) = d.as_object_mut() {
                    m.insert("below".to_owned(), below);
                }
                dirty.insert(o.clone());
            }
        }
        plan.writes.push(format!(
            "record {f} {}",
            if keep.is_empty() { "removed" } else { "rewritten" }
        ));
        plan.writes
            .extend(dirty.iter().map(|o| format!("record {}", owner_file(o))));
        if dry_run {
            return Ok(());
        }
        let new_rec = (!keep.is_empty()).then(|| {
            let mut r = rec_v.clone();
            if let Some(m) = r.as_object_mut() {
                m.insert("contributions".to_owned(), Value::Array(keep));
            }
            r
        });
        match &new_rec {
            Some(r) => recs.insert(owner.to_owned(), r.clone()),
            None => recs.remove(owner),
        };
        self.forget_write(owner, new_rec.as_ref(), &dirty, &recs, !bad.is_empty(), plan)
    }

    /// The journalled write of a forget: the owner's record (None: removed) and the records of `dirty`; the blobs no
    /// record names any more deleted unless `others_damaged` (a damaged record may name blobs: kept).
    fn forget_write(
        &mut self,
        owner: &str,
        rec: Option<&Value>,
        dirty: &BTreeSet<String>,
        recs: &Records,
        others_damaged: bool,
        plan: &mut Plan,
    ) -> Result<(), LedgerError> {
        let mut targets: Vec<(&str, String)> = vec![("ledger", owner_file(owner))];
        targets.extend(dirty.iter().map(|o| ("ledger", owner_file(o))));
        self.begin(owner, "forget", &new_txn(), &targets)?;
        let r = (|| -> Result<(), LedgerError> {
            self.step(&format!("record {owner}"))?;
            self.write_record(owner, rec)?;
            for o in dirty {
                self.step(&format!("record {o}"))?;
                self.write_record(o, recs.get(o))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                self.clear_journal();
                if !others_damaged {
                    self.gc(recs);
                }
                plan.applied = true;
                Ok(())
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }

    /// `Stages.forget_under`: every owner whose files all lie under the folders (game-relative) dropped, no file
    /// touched (a re-import copied the source install's bundles over those folders, so what they wrote is gone).
    /// The owners dropped (or that would be, `dry_run`).
    ///
    /// # Errors
    /// [`LedgerError::Refused`] (nothing dropped) when an owner also holds a file outside them; as [`Ledger::forget`].
    pub fn forget_under(&mut self, folders: &[String], dry_run: bool) -> Result<Vec<String>, LedgerError> {
        let tops: Vec<String> = folders
            .iter()
            .map(|f| norm_rel(f).map(|r| format!("{}/", r.to_lowercase())))
            .collect::<Result<_, _>>()?;
        let recs = self.records()?;
        let (mut drop, mut mixed) = (Vec::new(), Vec::new());
        for (o, r) in &recs {
            let inside: Vec<bool> = Self::contributions(r)
                .iter()
                .map(|c| {
                    let f = s(c, "file").unwrap_or_default().to_lowercase();
                    tops.iter().any(|t| f.starts_with(t.as_str()))
                })
                .collect();
            if !inside.is_empty() && inside.iter().all(|b| *b) {
                drop.push(o.clone());
            } else if inside.iter().any(|b| *b) {
                mixed.push(o.clone());
            }
        }
        if !mixed.is_empty() {
            return Err(LedgerError::Refused(vec![format!(
                "{} also hold files outside {}: revert them first (nothing was dropped from the install ledger)",
                mixed.join(", "),
                tops.join(", ")
            )]));
        }
        if !dry_run {
            for o in &drop {
                self.forget(o, false, false)?;
            }
        }
        Ok(drop)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use serde_json::json;

    use super::*;

    fn rec(cs: &Value) -> Value {
        json!({ "format": FORMAT, "owner": "a", "txn": "t", "installed": "i", "note": "", "contributions": cs })
    }

    #[test]
    fn every_reader_rule_is_kept() {
        let file = json!({ "kind": "file", "file": "x.bin", "slot": "", "value": { "sha": "ab", "size": 1 },
                           "prior": null, "below": null, "stack": true, "extra": { "derived": true } });
        assert_eq!(validate(&rec(&json!([file])), "a.json"), None);
        let cases: [(Value, &str, &str); 10] = [
            (json!([]), "b.json", "does not match its file name"),
            (
                json!([{ "kind": "frob", "file": "x", "slot": "" }]),
                "a.json",
                "an unknown kind",
            ),
            (
                json!([{ "kind": "file", "file": "x" }]),
                "a.json",
                "without its file / slot",
            ),
            (
                json!([{ "kind": "file", "file": "../x", "slot": "", "value": {"sha": "a"} }]),
                "a.json",
                "bad file name",
            ),
            (
                json!([{ "kind": "file", "file": "x", "slot": "", "value": {"sha": "a"}, "below": "A B" }]),
                "a.json",
                "bad below",
            ),
            (
                json!([{ "kind": "ini", "file": "o.ini", "slot": "d/k", "value": "k=1" }]),
                "a.json",
                "section / key",
            ),
            (
                json!([{ "kind": "mft_block", "file": "p.mft", "slot": "s", "value": 3 }]),
                "a.json",
                "not text",
            ),
            (
                json!([{ "kind": "file", "file": "x", "slot": "", "value": {"sha": "a"}, "prior": {"sha": "b"} }]),
                "a.json",
                "names no blob",
            ),
            (
                json!([{ "kind": "tree", "file": "t", "slot": "", "value": {} }]),
                "a.json",
                "without its files",
            ),
            (
                json!([{ "kind": "file", "file": "x", "slot": "", "value": {} }]),
                "a.json",
                "value's sha",
            ),
        ];
        for (cs, f, want) in cases {
            let why = validate(&rec(&cs), f).unwrap_or_default();
            assert!(why.contains(want), "{cs}: {why:?} lacks {want:?}");
        }
        let mut no_txn = rec(&json!([]));
        no_txn.as_object_mut().unwrap().remove("txn");
        assert!(validate(&no_txn, "a.json").unwrap().contains("\"txn\""));
        assert!(validate(&json!([1]), "a.json").unwrap().contains("not a JSON object"));
        let mut old = rec(&json!([]));
        old["format"] = json!("opfor-ledger/0");
        assert!(validate(&old, "a.json").unwrap().contains("expected opfor-ledger/1"));
    }
}
