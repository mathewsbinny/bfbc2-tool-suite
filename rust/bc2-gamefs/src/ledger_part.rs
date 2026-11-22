//! The install ledger's operations for the mod manager (round MM1, `bc2_install::mods`): a revert of some of an
//! owner's files, a setting edited through every layer of a file, and the records of the one-time migration of the
//! markers from before the ledger.
//!
//! - [`Ledger::uninstall_files`]: OPFOR's `Ledger.uninstall(owner, files=...)` - the owner's contributions to the
//!   files given taken out (its prior back, or spliced from under an independent layer), its other files kept.
//! - [`Ledger::edit_through`]: a user setting (`opfor.ini`'s `[patches] vietnam`) set in the file and in every
//!   state the ledger keeps of it (each layer's prior and value), so no later revert, in any order, takes the
//!   setting back. One journalled commit.
//! - [`Ledger::write_migrated`]: `opforcore.migrate.apply`'s ledger half - the backups kept as blobs (a hard link,
//!   else a copy, checked by sha256), then every new record in one journalled commit.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::{Ledger, LedgerError, Plan, Records, ioe, norm_rel, owner_file, read_opt, s, sha_of, valid_owner};
use crate::sha256;

impl Ledger {
    /// The owner's contributions to `rels` taken out (each file's prior back, or spliced from under an independent
    /// layer; refused under a derived one); its other contributions kept. A plan noting `not installed` when the
    /// owner holds none of them.
    ///
    /// # Errors
    /// [`LedgerError::Refused`] with every conflict (nothing written); a held lock; a write that failed (rolled back).
    pub fn uninstall_files(&mut self, owner: &str, rels: &[String], dry_run: bool) -> Result<Plan, LedgerError> {
        if !valid_owner(owner) {
            return Err(LedgerError::Damaged(format!("not an owner name: {owner:?}")));
        }
        let want: BTreeSet<String> = rels
            .iter()
            .map(|r| norm_rel(r).map(|r| r.to_lowercase()))
            .collect::<Result<_, _>>()?;
        let mut plan = Plan {
            owner: owner.to_owned(),
            action: "uninstall".to_owned(),
            ..Plan::default()
        };
        if !dry_run {
            self.lock(&mut plan.notes)?;
        }
        let r = self.part_locked(owner, &want, dry_run, &mut plan);
        if !dry_run && !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r.map(|()| plan)
    }

    fn part_locked(
        &mut self,
        owner: &str,
        want: &BTreeSet<String>,
        dry_run: bool,
        plan: &mut Plan,
    ) -> Result<(), LedgerError> {
        let mut recs = self.records()?;
        let Some(old) = recs.get(owner).cloned() else {
            plan.notes.push("not installed".to_owned());
            return Ok(());
        };
        let cs = Self::contributions(&old).to_vec();
        let (out, keep): (Vec<Value>, Vec<Value>) = cs
            .into_iter()
            .partition(|c| s(c, "file").is_some_and(|f| want.contains(&f.to_lowercase())));
        if out.is_empty() {
            plan.notes.push("not installed".to_owned());
            return Ok(());
        }
        let mut work: BTreeMap<String, Option<Vec<u8>>> = BTreeMap::new();
        let mut writes: BTreeMap<String, (String, Option<Vec<u8>>)> = BTreeMap::new();
        let mut dirty: BTreeSet<String> = BTreeSet::new();
        let mut conflicts: Vec<String> = Vec::new();
        for c in &out {
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
        if !conflicts.is_empty() {
            return Err(LedgerError::Refused(conflicts));
        }
        writes.retain(|_, (rel, d)| read_opt(&self.path(rel)).ok().flatten() != *d);
        for (rel, d) in writes.values() {
            plan.writes
                .push(format!("{} {rel}", if d.is_some() { "write" } else { "remove" }));
        }
        if dry_run {
            return Ok(());
        }
        let rec = (!keep.is_empty()).then(|| {
            let mut r = old.clone();
            if let Some(m) = r.as_object_mut() {
                m.insert("contributions".to_owned(), Value::Array(keep));
            }
            r
        });
        let txn = super::new_txn();
        let mut targets: Vec<(&str, String)> = writes.values().map(|(rel, _)| ("game", rel.clone())).collect();
        targets.push(("ledger", owner_file(owner)));
        for o in &dirty {
            targets.push(("ledger", owner_file(o)));
        }
        self.begin(owner, "uninstall", &txn, &targets)?;
        let r = (|| -> Result<(), LedgerError> {
            for (rel, d) in writes.values() {
                let p = self.path(rel);
                if let Some(d) = d {
                    self.step(&format!("game {rel}"))?;
                    super::write_atomic(&p, d).map_err(|e| ioe(&p, &e))?;
                } else {
                    self.step(&format!("remove {rel}"))?;
                    super::remove_file(&p).map_err(|e| ioe(&p, &e))?;
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
        self.finish(r, &mut recs, owner, rec, plan)
    }

    /// A commit's end: the journal cleared and unused blobs dropped, or the commit rolled back.
    fn finish(
        &self,
        r: Result<(), LedgerError>,
        recs: &mut Records,
        owner: &str,
        rec: Option<Value>,
        plan: &mut Plan,
    ) -> Result<(), LedgerError> {
        match r {
            Ok(()) => {
                self.clear_journal();
                match rec {
                    Some(rec) => {
                        recs.insert(owner.to_owned(), rec);
                    }
                    None => {
                        recs.remove(owner);
                    }
                }
                self.gc(recs);
                plan.applied = true;
                Ok(())
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }

    /// The states the ledger keeps of rel, bottom first: the bottom layer's prior, then each layer's value (the
    /// next one's prior, the top one's the file); None for a state with no file. With the layers (owner, index).
    #[allow(clippy::type_complexity, reason = "the states and their layers, read once")]
    fn states(&self, recs: &Records, rel: &str) -> Result<(Vec<(String, usize)>, Vec<Option<Vec<u8>>>), String> {
        let (layers, _) = Self::layers(recs, rel);
        let st = Self::stack(recs, &layers);
        let mut out: Vec<Option<Vec<u8>>> = Vec::new();
        for (i, l) in st.iter().enumerate() {
            let c = Self::contribution(recs, l).cloned().unwrap_or(Value::Null);
            if i == 0 {
                out.push(match c.get("prior").and_then(|p| s(p, "blob")) {
                    None => None,
                    Some(b) => Some(self.blob(b)?),
                });
            }
            out.push(match st.get(i + 1) {
                Some(next) => {
                    let n = Self::contribution(recs, next).cloned().unwrap_or(Value::Null);
                    match n.get("prior").and_then(|p| s(p, "blob")) {
                        None => None,
                        Some(b) => Some(self.blob(b)?),
                    }
                }
                None => read_opt(&self.path(rel)).map_err(|e| e.to_string())?,
            });
            // a layer's value must be what the ledger recorded it wrote
            let want = c.get("value").and_then(sha_of);
            if out.last().and_then(|d| d.as_deref().map(sha256::hex)).as_deref() != want {
                return Err(if st.get(i + 1).is_some() {
                    format!("{rel}: the ledger's copy above {} is not what {} wrote", l.0, l.0)
                } else {
                    format!("{rel}: changed outside the ledger since {} installed it", l.0)
                });
            }
        }
        Ok((st, out))
    }

    /// `edit` applied to rel and to every state the ledger keeps of it (each layer's prior and value), in one
    /// journalled commit: a setting the user chose that no revert should take back. A state with no file is left
    /// without one. Returns whether anything changed (false: every state had the setting already).
    ///
    /// # Errors
    /// [`LedgerError::Refused`] when rel changed outside the ledger, holds other owners' slots or a kept state is
    /// missing; a held lock; a write that failed (rolled back).
    pub fn edit_through(&mut self, rel: &str, edit: &dyn Fn(&[u8]) -> Vec<u8>) -> Result<bool, LedgerError> {
        let rel = norm_rel(rel)?;
        let mut notes = Vec::new();
        self.lock(&mut notes)?;
        let r = self.edit_locked(&rel, edit);
        if !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r
    }

    fn edit_locked(&mut self, rel: &str, edit: &dyn Fn(&[u8]) -> Vec<u8>) -> Result<bool, LedgerError> {
        let mut recs = self.records()?;
        let (_, slots) = Self::layers(&recs, rel);
        if !slots.is_empty() {
            return Err(LedgerError::Refused(vec![format!(
                "{rel}: {} holds parts of it (Project OPFOR's ledger): edit it with that tool",
                slots.join(", ")
            )]));
        }
        let (st, states) = self.states(&recs, rel).map_err(|e| LedgerError::Refused(vec![e]))?;
        if st.is_empty() {
            // no owner: the file alone
            let Some(cur) = read_opt(&self.path(rel))? else {
                return Ok(false);
            };
            let new = edit(&cur);
            if new == cur {
                return Ok(false);
            }
            self.step(&format!("game {rel}"))?;
            let p = self.path(rel);
            super::write_atomic(&p, &new).map_err(|e| ioe(&p, &e))?;
            return Ok(true);
        }
        let new: Vec<Option<Vec<u8>>> = states.iter().map(|d| d.as_deref().map(edit)).collect();
        if new == states {
            return Ok(false);
        }
        let mut blobs: BTreeMap<String, Vec<u8>> = BTreeMap::new();
        let mut dirty: BTreeSet<String> = BTreeSet::new();
        for (i, l) in st.iter().enumerate() {
            let prior = new.get(i).cloned().flatten();
            let value = new.get(i + 1).cloned().flatten();
            let Some(c) = recs
                .get_mut(&l.0)
                .and_then(|r| r.get_mut("contributions"))
                .and_then(Value::as_array_mut)
                .and_then(|a| a.get_mut(l.1))
                .and_then(Value::as_object_mut)
            else {
                continue;
            };
            if let Some(p) = &prior {
                let sha = sha256::hex(p);
                blobs.insert(sha.clone(), p.clone());
                c.insert("prior".to_owned(), json!({ "sha": sha, "size": p.len(), "blob": sha }));
            }
            if let Some(v) = &value {
                c.insert("value".to_owned(), json!({ "sha": sha256::hex(v), "size": v.len() }));
            }
            dirty.insert(l.0.clone());
        }
        let file = new.last().cloned().flatten();
        let txn = super::new_txn();
        let first = st.first().map_or_default(|l| l.0.clone());
        let mut targets: Vec<(&str, String)> = vec![("game", rel.to_owned())];
        for o in &dirty {
            targets.push(("ledger", owner_file(o)));
        }
        self.begin(&first, "edit", &txn, &targets)?;
        let r = (|| -> Result<(), LedgerError> {
            for (sha, data) in &blobs {
                let p = self.blob_path(sha);
                if !p.is_file() {
                    self.step(&format!("blob {sha}"))?;
                    Self::store_write(&p, data)?;
                }
            }
            if let Some(d) = &file {
                self.step(&format!("game {rel}"))?;
                let p = self.path(rel);
                super::write_atomic(&p, d).map_err(|e| ioe(&p, &e))?;
            }
            for o in &dirty {
                self.step(&format!("record {o}"))?;
                self.write_record(o, recs.get(o))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                self.clear_journal();
                self.gc(&recs);
                Ok(true)
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }

    /// `opforcore.migrate.apply`'s ledger half: each `(sha256, backup)` kept as a blob (a hard link to the backup,
    /// else a copy; checked by sha256), then `records` (full `opfor-ledger/1` records, by owner) written in one
    /// journalled commit. The caller checked the plan (migrate's rules) under no lock; the lock is taken here, an
    /// interrupted commit rolled back first.
    ///
    /// # Errors
    /// A backup that changed while it was kept; a held lock; a write that failed (rolled back).
    pub fn write_migrated(
        &mut self,
        records: &BTreeMap<String, Value>,
        blobs: &[(String, PathBuf)],
    ) -> Result<(), LedgerError> {
        let mut notes = Vec::new();
        self.lock(&mut notes)?;
        let r = self.migrated_locked(records, blobs);
        if !matches!(r, Err(LedgerError::Crashed)) {
            self.unlock();
        }
        r
    }

    fn migrated_locked(
        &mut self,
        records: &BTreeMap<String, Value>,
        blobs: &[(String, PathBuf)],
    ) -> Result<(), LedgerError> {
        for (sha, src) in blobs {
            let dst = self.blob_path(sha);
            if dst.is_file() && sha256::file(&dst.to_string_lossy()).ok().as_deref() == Some(sha.as_str()) {
                continue;
            }
            self.step(&format!("blob {sha}"))?;
            keep_blob(src, &dst, sha)?;
        }
        if records.is_empty() {
            return Ok(());
        }
        let txn = super::new_txn();
        let targets: Vec<(&str, String)> = records.keys().map(|o| ("ledger", owner_file(o))).collect();
        self.begin("migrate", "migrate", &txn, &targets)?;
        let r = (|| -> Result<(), LedgerError> {
            for (o, rec) in records {
                self.step(&format!("record {o}"))?;
                self.write_record(o, Some(rec))?;
            }
            Ok(())
        })();
        match r {
            Ok(()) => {
                self.clear_journal();
                Ok(())
            }
            Err(LedgerError::Crashed) => Err(LedgerError::Crashed),
            Err(e) => Err(self.abort(&e)),
        }
    }
}

/// `migrate._blob`: the backup's bytes as the blob `dst` - a hard link (no second copy on disk), else a copy -
/// beside it first, checked by sha256, then moved over it.
fn keep_blob(src: &Path, dst: &Path, sha: &str) -> Result<(), LedgerError> {
    if let Some(d) = dst.parent() {
        fs::create_dir_all(d).map_err(|e| ioe(d, &e))?;
    }
    let tmp = crate::atomic::temp_name(dst);
    if fs::hard_link(src, &tmp).is_err() {
        fs::copy(src, &tmp).map_err(|e| ioe(src, &e))?;
    }
    if sha256::file(&tmp.to_string_lossy()).ok().as_deref() != Some(sha) {
        let _ = fs::remove_file(&tmp);
        return Err(LedgerError::Io(format!(
            "{} changed while it was being kept",
            src.display()
        )));
    }
    crate::atomic::replace(&tmp, dst).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        ioe(dst, &e)
    })
}
