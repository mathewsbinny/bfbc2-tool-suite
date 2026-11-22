//! Project OPFOR's whole-file stage operations on the ledger (round OR1, "OPFOR in Rust" stage 1): what
//! `opforcore.stages.Stages` does for the WW2 install, so `bc2-install` writes the records OPFOR's Python wrote.
//!
//! - [`Ledger::install_keep`]: `Stages.install` - a transaction with `keep_others`: the owner's contributions to the
//!   files not given stay as they are, a file whose bytes are the owner's already is kept in place, and the record
//!   lists the untouched contributions, then the kept ones, then the new ones (`ledger._plan_and_write`).
//! - [`Ledger::restate`]: `Stages.restate` - the owner on top takes the file as it is now for its write (a writer
//!   outside the ledger edited it), optionally with a new prior.
//! - [`Ledger::file_layers`]: the `file` layers of one file, bottom first (`Stages._layers`).
//!
//! The checks and messages are OPFOR's (`describe()` is `file <rel>`), so a refusal reads the same from either side.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Value, json};

use super::{Ledger, LedgerError, Plan, Records, derived, norm_rel, s, sha_of, utc_now, valid_owner};
use crate::sha256;

/// OPFOR's `_show` of a file value: `sha <12 hex>`, or `absent`.
fn show(v: Option<&str>) -> String {
    v.map_or_else(|| "absent".to_owned(), |s| format!("sha {}", s.get(..12).unwrap_or(s)))
}

fn describe(rel: &str) -> String {
    format!("file {rel}")
}

fn is_file_on(c: &Value, low: &str) -> bool {
    s(c, "kind") == Some("file") && s(c, "file").map(str::to_lowercase).as_deref() == Some(low)
}

impl Ledger {
    /// The `file` layers on `rel`, bottom first: (owner, contribution).
    ///
    /// # Errors
    /// A damaged record, a bad path.
    pub fn file_layers(&self, rel: &str) -> Result<Vec<(String, Value)>, LedgerError> {
        let rel = norm_rel(rel)?;
        let recs = self.records()?;
        let (layers, _) = Self::layers(&recs, &rel);
        Ok(Self::stack(&recs, &layers)
            .into_iter()
            .filter_map(|l| Self::contribution(&recs, &l).cloned().map(|c| (l.0, c)))
            .collect())
    }

    /// The other owners holding slots of another kind than `file` in `rel` (Project OPFOR's registration slots).
    ///
    /// # Errors
    /// A damaged record, a bad path.
    pub fn slot_holders(&self, rel: &str) -> Result<Vec<(String, String)>, LedgerError> {
        let low = norm_rel(rel)?.to_lowercase();
        let recs = self.records()?;
        let mut out = Vec::new();
        for (o, r) in &recs {
            for c in Self::contributions(r) {
                if s(c, "kind") != Some("file") && s(c, "file").map(str::to_lowercase).as_deref() == Some(low.as_str())
                {
                    let slot = s(c, "slot").unwrap_or_default();
                    let what = format!(
                        "{} {}{}",
                        s(c, "kind").unwrap_or("?"),
                        s(c, "file").unwrap_or_default(),
                        if slot.is_empty() {
                            String::new()
                        } else {
                            format!(" [{slot}]")
                        }
                    );
                    out.push((o.clone(), what));
                }
            }
        }
        Ok(out)
    }

    /// `Stages.install(owner, files, derived=...)`: one journalled commit adding `files` (game-relative path, bytes,
    /// in order) to the owner's record; its other contributions stay. Refused (nothing written) on any conflict.
    ///
    /// # Errors
    /// [`LedgerError::Refused`] with every conflict; a held lock; a write that failed (rolled back).
    pub fn install_keep(
        &mut self,
        owner: &str,
        files: &[(String, Vec<u8>)],
        is_derived: bool,
        note: &str,
        dry_run: bool,
    ) -> Result<Plan, LedgerError> {
        if !valid_owner(owner) {
            return Err(LedgerError::Damaged(format!("not an owner name: {owner:?}")));
        }
        let mut plan = Plan {
            owner: owner.to_owned(),
            action: "install".to_owned(),
            ..Plan::default()
        };
        if !dry_run {
            self.lock(&mut plan.notes)?;
        }
        let r = self.keep_locked(owner, files, is_derived, note, dry_run, &mut plan);
        if !dry_run && !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r.map(|()| plan)
    }

    #[allow(clippy::too_many_lines, reason = "ledger._plan_and_write's steps, in its order")]
    fn keep_locked(
        &mut self,
        owner: &str,
        files: &[(String, Vec<u8>)],
        is_derived: bool,
        note: &str,
        dry_run: bool,
        plan: &mut Plan,
    ) -> Result<(), LedgerError> {
        let mut recs = self.records()?;
        let old = recs.get(owner).cloned();
        let new: Vec<(String, &[u8])> = files
            .iter()
            .map(|(r, d)| norm_rel(r).map(|r| (r, d.as_slice())))
            .collect::<Result<_, _>>()?;
        let mut new_keys = BTreeSet::new();
        for (r, _) in &new {
            if !new_keys.insert(r.to_lowercase()) {
                return Err(LedgerError::Damaged(
                    "two contributions of one transaction to the same slot".to_owned(),
                ));
            }
        }
        let old_cs: Vec<Value> = old.as_ref().map_or_else(Vec::new, |r| Self::contributions(r).to_vec());
        let mut work: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        let mut conflicts: Vec<String> = Vec::new();
        let mut dirty: BTreeSet<String> = BTreeSet::new();
        // unchanged contributions are kept as they are (prior, below and position)
        let mut kept: Vec<Value> = Vec::new();
        let mut carried: BTreeSet<String> = BTreeSet::new();
        for (rel, data) in &new {
            let low = rel.to_lowercase();
            if let Some(c0) = old_cs.iter().find(|c| is_file_on(c, &low))
                && c0.get("value").and_then(sha_of) == Some(sha256::hex(data).as_str())
            {
                kept.push(c0.clone());
                carried.insert(low);
            }
        }
        let key_of = |c: &Value| -> Option<String> {
            (s(c, "kind") == Some("file") && s(c, "slot").unwrap_or_default().is_empty())
                .then(|| s(c, "file").unwrap_or_default().to_lowercase())
        };
        let untouched: Vec<Value> = old_cs
            .iter()
            .filter(|c| key_of(c).is_none_or(|k| !new_keys.contains(&k)))
            .cloned()
            .collect();
        let removing: Vec<Value> = old_cs
            .iter()
            .filter(|c| key_of(c).is_some_and(|k| new_keys.contains(&k) && !carried.contains(&k)))
            .cloned()
            .collect();
        for c in &kept {
            let rel = s(c, "file").unwrap_or_default().to_owned();
            if self.above_of(&recs, owner, &rel).is_none() {
                let cur = self.current(&mut work, &rel)?;
                if cur.as_deref().map(sha256::hex).as_deref() != c.get("value").and_then(sha_of) {
                    conflicts.push(format!(
                        "{}: changed outside the ledger since {owner} wrote it",
                        describe(&rel)
                    ));
                }
            }
        }
        // the owner's changed contributions out first (ledger._remove_all, splices allowed)
        for c in &removing {
            let rel = s(c, "file").unwrap_or_default().to_owned();
            if let Some((o, i)) = self.above_of(&recs, owner, &rel) {
                let d = Self::contribution(&recs, &(o.clone(), i))
                    .cloned()
                    .unwrap_or(Value::Null);
                if derived(&d) {
                    conflicts.push(format!(
                        "{}: {o} was built on {owner}'s file (it patched what {owner} wrote): revert {o} first",
                        describe(&rel)
                    ));
                    continue;
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
                plan.notes
                    .push(format!("{}: under {o}, which inherits its prior", describe(&rel)));
                continue;
            }
            let cur = self.current(&mut work, &rel)?;
            let cur_sha = cur.as_deref().map(sha256::hex);
            let want = c.get("value").and_then(sha_of);
            if cur_sha.as_deref() != want {
                conflicts.push(format!(
                    "{}: changed outside the ledger since {owner} wrote it (now {}, {owner} wrote {})",
                    describe(&rel),
                    show(cur_sha.as_deref()),
                    show(want)
                ));
                continue;
            }
            let data = match c.get("prior").and_then(|p| s(p, "blob")) {
                None => None,
                Some(b) => match self.blob(b) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        conflicts.push(format!(
                            "{}: the prior bytes are missing from the ledger ({e})",
                            describe(&rel)
                        ));
                        continue;
                    }
                },
            };
            work.insert(rel.to_lowercase(), data);
        }
        let mut added: Vec<Value> = Vec::new();
        let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut names: BTreeMap<String, String> = BTreeMap::new();
        for (rel, data) in &new {
            let low = rel.to_lowercase();
            names.insert(low.clone(), rel.clone());
            if carried.contains(&low) {
                continue;
            }
            let cur = self.current(&mut work, rel)?;
            let others: Records = recs
                .iter()
                .filter(|(o, _)| *o != owner)
                .map(|(o, r)| (o.clone(), r.clone()))
                .collect();
            let mixed = others.iter().find_map(|(o, r)| {
                Self::contributions(r)
                    .iter()
                    .find(|d| s(d, "kind") != Some("file") && s(d, "kind") != Some("tree") && is_on(d, &low))
                    .map(|d| (o.clone(), d.clone()))
            });
            if let Some((o, d)) = mixed {
                let slot = s(&d, "slot").unwrap_or_default();
                conflicts.push(format!(
                    "{}: {o} holds {} {}{} in the same file - a whole-file install and slot contributions cannot \
                     share a file (revert it first)",
                    describe(rel),
                    s(&d, "kind").unwrap_or("?"),
                    s(&d, "file").unwrap_or_default(),
                    if slot.is_empty() {
                        String::new()
                    } else {
                        format!(" [{slot}]")
                    }
                ));
                continue;
            }
            let (layers, _) = Self::layers(&others, rel);
            let tops: Vec<_> = layers
                .iter()
                .filter(|l| {
                    !layers
                        .iter()
                        .any(|e| Self::contribution(&others, e).and_then(|d| s(d, "below")) == Some(l.0.as_str()))
                })
                .collect();
            let cur_sha = cur.as_deref().map(sha256::hex);
            let mut below = Value::Null;
            if let Some(top) = tops.first() {
                let d = Self::contribution(&others, top).cloned().unwrap_or(Value::Null);
                if cur_sha.as_deref() != d.get("value").and_then(sha_of) {
                    conflicts.push(format!(
                        "{}: {}'s value there was changed outside the ledger (now {})",
                        describe(rel),
                        top.0,
                        show(cur_sha.as_deref())
                    ));
                    continue;
                }
                if !d.get("stack").and_then(Value::as_bool).unwrap_or(true) {
                    conflicts.push(format!("{}: held by {} (not stackable)", describe(rel), top.0));
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
            added.push(json!({ "kind": "file", "file": rel, "slot": "",
                               "value": { "sha": sha256::hex(data), "size": data.len() },
                               "prior": prior, "below": below, "stack": true,
                               "extra": { "derived": is_derived } }));
            work.insert(low, Some(data.to_vec()));
        }
        if !conflicts.is_empty() {
            return Err(LedgerError::Refused(conflicts));
        }
        // the files whose final bytes differ from the disk
        let mut writes: Vec<(String, Option<Vec<u8>>)> = Vec::new();
        for (low, d) in &work {
            let rel = names
                .get(low)
                .cloned()
                .or_else(|| {
                    old_cs
                        .iter()
                        .find(|c| is_file_on(c, low))
                        .and_then(|c| s(c, "file").map(str::to_owned))
                })
                .unwrap_or_else(|| low.clone());
            if super::read_opt(&self.path(&rel))? != *d {
                writes.push((rel, d.clone()));
            }
        }
        // the given order (the files first written first)
        writes.sort_by_key(|(r, _)| {
            new.iter()
                .position(|(n, _)| n.eq_ignore_ascii_case(r))
                .unwrap_or(usize::MAX)
        });
        for (rel, d) in &writes {
            plan.writes
                .push(format!("{} {rel}", if d.is_some() { "write" } else { "remove" }));
        }
        if old.is_some() && added.is_empty() && removing.is_empty() && dirty.is_empty() && writes.is_empty() {
            plan.notes.push("unchanged: nothing written".to_owned());
            plan.applied = !dry_run;
            return Ok(());
        }
        if dry_run {
            return Ok(());
        }
        let old_note = old.as_ref().and_then(|r| s(r, "note")).unwrap_or_default();
        let note = if note.is_empty() && old.is_some() {
            old_note
        } else {
            note
        };
        let mut cs = untouched;
        cs.extend(kept);
        cs.extend(added);
        let txn = super::new_txn();
        let rec = json!({ "format": super::FORMAT, "owner": owner, "txn": txn, "installed": utc_now().0,
                          "note": note, "contributions": cs });
        let mut targets: Vec<(&str, String)> = writes.iter().map(|(rel, _)| ("game", rel.clone())).collect();
        targets.push(("ledger", super::owner_file(owner)));
        for o in &dirty {
            targets.push(("ledger", super::owner_file(o)));
        }
        self.begin(owner, "install", &txn, &targets)?;
        let r = (|| -> Result<(), LedgerError> {
            for (sha, data) in &blobs {
                let p = self.blob_path(sha);
                if !p.is_file() {
                    self.step(&format!("blob {sha}"))?;
                    Self::store_write(&p, data)?;
                }
            }
            for (rel, d) in &writes {
                let p = self.path(rel);
                if let Some(d) = d {
                    self.step(&format!("game {rel}"))?;
                    super::write_atomic(&p, d).map_err(|e| super::ioe(&p, &e))?;
                } else {
                    self.step(&format!("remove {rel}"))?;
                    super::remove_file(&p).map_err(|e| super::ioe(&p, &e))?;
                }
            }
            self.step(&format!("record {owner}"))?;
            self.write_record(owner, Some(&rec))?;
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
                recs.insert(owner.to_owned(), rec);
                self.gc(&recs);
                plan.applied = true;
                Ok(())
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }

    /// The layer sitting on the owner's `file` contribution to rel (another owner's, its below the owner).
    fn above_of(&self, recs: &Records, owner: &str, rel: &str) -> Option<(String, usize)> {
        let _ = self;
        let (layers, _) = Self::layers(recs, rel);
        layers
            .into_iter()
            .find(|l| l.0 != owner && Self::contribution(recs, l).and_then(|d| s(d, "below")) == Some(owner))
    }

    /// `Stages.restate`: the owner, on top of rel, takes the file as it is now for its write; `why` (when not empty)
    /// is recorded as the contribution's `restated`; `keep_prior` false: the file before the owner becomes `prior`
    /// (None: there was none). Only the owner's record is written (journalled).
    ///
    /// # Errors
    /// [`LedgerError::Refused`] when the owner does not hold rel or another install sits on it; a held lock; a write that
    /// failed (rolled back).
    pub fn restate(
        &mut self,
        owner: &str,
        rel: &str,
        keep_prior: bool,
        prior: Option<&[u8]>,
        why: &str,
    ) -> Result<(), LedgerError> {
        let rel = norm_rel(rel)?;
        let mut notes = Vec::new();
        self.lock(&mut notes)?;
        let r = self.restate_locked(owner, &rel, (!keep_prior).then_some(prior), why);
        if !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r
    }

    #[allow(clippy::option_option, reason = "None: the prior kept; Some(None): there was none")]
    fn restate_locked(
        &mut self,
        owner: &str,
        rel: &str,
        prior: Option<Option<&[u8]>>,
        why: &str,
    ) -> Result<(), LedgerError> {
        let mut recs = self.records()?;
        let low = rel.to_lowercase();
        let idx = recs
            .get(owner)
            .and_then(|r| Self::contributions(r).iter().position(|c| is_file_on(c, &low)));
        let Some(i) = idx else {
            return Err(LedgerError::Refused(vec![format!(
                "{rel}: {owner} has not installed it"
            )]));
        };
        if self.above_of(&recs, owner, rel).is_some() {
            return Err(LedgerError::Refused(vec![format!(
                "{rel}: another install sits on {owner}'s write"
            )]));
        }
        let p = self.path(rel);
        let data = std::fs::read(&p).map_err(|e| super::ioe(&p, &e))?;
        let mut blob: Option<&[u8]> = None;
        if let Some(c) = recs
            .get_mut(owner)
            .and_then(|r| r.get_mut("contributions"))
            .and_then(Value::as_array_mut)
            .and_then(|a| a.get_mut(i))
            .and_then(Value::as_object_mut)
        {
            c.insert(
                "value".to_owned(),
                json!({ "sha": sha256::hex(&data), "size": data.len() }),
            );
            if !why.is_empty() {
                let extra = c.entry("extra").or_insert_with(|| json!({}));
                if let Some(e) = extra.as_object_mut() {
                    e.insert("restated".to_owned(), json!(why));
                }
            }
            if let Some(pr) = prior {
                c.insert(
                    "prior".to_owned(),
                    pr.map_or(Value::Null, |b| {
                        let sha = sha256::hex(b);
                        json!({ "sha": sha, "size": b.len(), "blob": sha })
                    }),
                );
                blob = pr;
            }
        }
        let txn = super::new_txn();
        self.begin(owner, "restate", &txn, &[("ledger", super::owner_file(owner))])?;
        let r = (|| -> Result<(), LedgerError> {
            if let Some(b) = blob {
                let bp = self.blob_path(&sha256::hex(b));
                if !bp.is_file() {
                    self.step(&format!("blob {}", sha256::hex(b)))?;
                    Self::store_write(&bp, b)?;
                }
            }
            self.step(&format!("record {owner}"))?;
            self.write_record(owner, recs.get(owner))?;
            Ok(())
        })();
        match r {
            Ok(()) => {
                self.clear_journal();
                self.gc(&recs);
                Ok(())
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }

    /// Whether an interrupted commit waits to be rolled back (its journal, or a failed rollback's mark).
    #[must_use]
    pub fn interrupted(&self) -> bool {
        self.root.join(super::JOURNAL).is_file() || self.root.join(super::DIRTY).is_file()
    }

    /// The lock's holder when another live process holds it (None: free or stale).
    #[must_use]
    pub fn live_lock(&self) -> Option<String> {
        let info = self.lock_info()?;
        if Self::lock_stale(&info) {
            return None;
        }
        Some(format!(
            "{}: held by pid {} since {}: another install / uninstall is running",
            self.root.join(super::LOCK).display(),
            info.get("pid").map_or_else(|| "?".to_owned(), Value::to_string),
            s(&info, "started").unwrap_or("?")
        ))
    }

    /// Whether a lock file is there at all (live or stale).
    #[must_use]
    pub fn lock_present(&self) -> bool {
        self.root.join(super::LOCK).is_file()
    }
}

fn is_on(c: &Value, low: &str) -> bool {
    s(c, "file").map(str::to_lowercase).as_deref() == Some(low)
}
