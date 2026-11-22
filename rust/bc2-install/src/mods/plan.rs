//! The mod manager's operations planned: every check before the first write, the steps in their order.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use bc2_gamefs::sha256;
use serde::Serialize;
use serde_json::Value;

use super::catalog::{self, SETTINGS, Setting};
use super::{
    FolderFile, FolderStash, InFile, Kept, Manager, ModError, Op, OwnerStash, STASH_FORMAT, Shasize, Step, from_ledger,
    ini, mtime_ns, new_id, tree,
};
use crate::run::legacy_markers;

/// One mod's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// Everything of it is in the game copy.
    Enabled,
    /// Everything of it is kept aside.
    Disabled,
    /// Some of it is kept aside (a file restored to DICE's, an interrupted operation).
    Partly,
}

/// A mod as the list shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    /// Its id.
    pub id: String,
    /// What it is.
    pub title: String,
    /// Its state.
    pub state: State,
    /// Its units in the game copy: `owner <o> (<n> files)`, `folder <f>`, `setting <file> [s] k=v`.
    pub installed: Vec<String>,
    /// Its units kept aside.
    pub kept: Vec<String>,
    /// The mods it needs.
    pub requires: Vec<String>,
}

/// What the game copy holds, read once.
struct View {
    recs: BTreeMap<String, Value>,
    stash: BTreeMap<String, OwnerStash>,
    fstash: BTreeMap<String, FolderStash>,
    folders: Vec<(String, String, &'static str)>,
}

fn s<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn contributions(rec: &Value) -> Vec<Value> {
    rec.get("contributions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn value_sha(c: &Value) -> Option<String> {
    c.get("value").and_then(|v| s(v, "sha")).map(str::to_owned)
}

impl Manager {
    fn view(&self) -> Result<View, ModError> {
        Ok(View {
            recs: self.ledger().records().map_err(from_ledger)?,
            stash: self.owner_stashes()?,
            fstash: self.folder_stashes()?,
            folders: catalog::folders(&self.game),
        })
    }

    fn setting_on(&self, st: &Setting) -> Option<bool> {
        let p = self.path(st.file);
        let text = String::from_utf8(std::fs::read(p).ok()?).ok()?;
        Some(crate::text::ini_value(&text, st.section, st.key).map_or(st.default_on, |v| v == st.on))
    }

    /// Every mod of the game copy (installed, kept aside, or a setting), sorted.
    ///
    /// # Errors
    /// A damaged ledger or stash record.
    pub fn list(&self) -> Result<Vec<Change>, ModError> {
        let v = self.view()?;
        let mut out: BTreeMap<String, Change> = BTreeMap::new();
        let entry = |id: &str| -> Change {
            Change {
                id: id.to_owned(),
                title: catalog::title(id),
                state: State::Enabled,
                installed: Vec::new(),
                kept: Vec::new(),
                requires: catalog::requires(id).iter().map(|r| (*r).to_owned()).collect(),
            }
        };
        for (o, r) in &v.recs {
            let m = catalog::mod_of_owner(o);
            let e = out.entry(m.clone()).or_insert_with(|| entry(&m));
            e.installed
                .push(format!("owner {o} ({} files)", contributions(r).len()));
        }
        for (o, st) in &v.stash {
            let e = out.entry(st.mod_id.clone()).or_insert_with(|| entry(&st.mod_id));
            e.kept.push(format!("owner {o} ({} files)", st.contributions.len()));
        }
        for (f, _, m) in &v.folders {
            out.entry((*m).to_owned())
                .or_insert_with(|| entry(m))
                .installed
                .push(format!("folder {f}"));
        }
        for st in v.fstash.values() {
            out.entry(st.mod_id.clone())
                .or_insert_with(|| entry(&st.mod_id))
                .kept
                .push(format!("folder {}", st.folder));
        }
        for st in SETTINGS {
            if let Some(on) = self.setting_on(st) {
                let e = out.entry(st.id.to_owned()).or_insert_with(|| entry(st.id));
                let line = format!("setting {} [{}] {}", st.file, st.section, st.key);
                if on {
                    e.installed.push(line);
                } else {
                    e.kept.push(line);
                }
            }
        }
        for c in out.values_mut() {
            c.state = match (c.installed.is_empty(), c.kept.is_empty()) {
                (_, true) => State::Enabled,
                (true, false) => State::Disabled,
                (false, false) => State::Partly,
            };
        }
        Ok(out.into_values().collect())
    }

    /// The mods a name selects (a group: `onslaught`, `navkit`, `stockmp`).
    ///
    /// # Errors
    /// No such mod; a damaged record.
    pub fn select(&self, names: &[String]) -> Result<Vec<String>, ModError> {
        let known: Vec<String> = self.list()?.into_iter().map(|c| c.id).collect();
        let mut out = Vec::new();
        let mut bad = Vec::new();
        for n in names {
            let got = catalog::select(n, &known);
            if got.is_empty() {
                bad.push(format!(
                    "{n}: no such mod in this game copy (bfbc2 mods list: {})",
                    known.join(", ")
                ));
            }
            out.extend(got);
        }
        if !bad.is_empty() {
            return Err(ModError::Refused(bad));
        }
        out.sort();
        out.dedup();
        Ok(out)
    }

    // ---------------------------------------------------------------------------------------------- disable

    /// Disables the mods (their dependents too, with `yes`). The lines say what was done.
    ///
    /// # Errors
    /// [`ModError::Confirm`] naming the dependents without `yes`; [`ModError::Refused`] (nothing written); a step
    /// that failed (carried on by the next command).
    pub fn disable(&self, mods: &[String], yes: bool) -> Result<Vec<String>, ModError> {
        self.locked(|lines| {
            let mods = &self.select(mods)?;
            let v = self.view()?;
            let (set, why) = self.cascade(&v, mods);
            if !why.is_empty() && !yes {
                return Err(ModError::Confirm(confirm_lines("disable", mods, &why)));
            }
            let mut op = self.plan_out(&v, &set, "disable")?;
            if op.steps.is_empty() {
                lines.push(format!("{}: disabled already", mods.join(", ")));
                return Ok(());
            }
            lines.extend(why.iter().map(|w| format!("also disabled: {w}")));
            op.what = format!("disable {}", set.iter().cloned().collect::<Vec<_>>().join(", "));
            self.run(&op, lines)?;
            lines.push(format!(
                "disabled: {}",
                set.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
            Ok(())
        })
        .map(|((), l)| l)
    }

    /// Uninstalls the mods: a disable whose kept copies are dropped (dependents are only disabled). The WW2 opt-out
    /// keeps the bundleManifest repair as its own install.
    ///
    /// # Errors
    /// As [`Manager::disable`].
    pub fn uninstall(&self, mods: &[String], yes: bool) -> Result<Vec<String>, ModError> {
        self.locked(|lines| {
            let mods = &self.select(mods)?;
            let v = self.view()?;
            let (set, why) = self.cascade(&v, mods);
            if !yes {
                let mut w = vec![format!(
                    "uninstall {}: their files go back to what was there before them and the kept copies are \
                     dropped (installing again needs their build)",
                    mods.join(", ")
                )];
                w.extend(why.iter().map(|x| format!("also disabled (kept, not dropped): {x}")));
                return Err(ModError::Confirm(w));
            }
            let mut op = self.plan_out(&v, &set, "uninstall")?;
            let targets: BTreeSet<&String> = mods.iter().collect();
            for (o, st) in &self.owner_stashes()? {
                if targets.contains(&st.mod_id) {
                    op.steps.push(Step::Drop {
                        name: super::owner_stash_name(o),
                    });
                }
            }
            for o in v.recs.keys() {
                if targets.contains(&catalog::mod_of_owner(o)) {
                    op.steps.push(Step::Drop {
                        name: super::owner_stash_name(o),
                    });
                }
            }
            for f in v.fstash.values() {
                if targets.contains(&f.mod_id) {
                    op.steps.push(Step::Drop {
                        name: super::folder_stash_name(&f.folder),
                    });
                }
            }
            for (f, _, m) in &v.folders {
                if targets.contains(&(*m).to_owned()) {
                    op.steps.push(Step::Drop {
                        name: super::folder_stash_name(f),
                    });
                }
            }
            if mods.iter().any(|m| m == "ww2") {
                op.steps.push(Step::ManifestFix);
            }
            op.what = format!("uninstall {}", mods.join(", "));
            self.run(&op, lines)?;
            lines.push(format!("uninstalled: {}", mods.join(", ")));
            Ok(())
        })
        .map(|((), l)| l)
    }

    /// The mods `mods` with every enabled mod whose files are built on theirs or which needs one of them; the
    /// reasons for the ones added.
    fn cascade(&self, v: &View, mods: &[String]) -> (BTreeSet<String>, Vec<String>) {
        let mut set: BTreeSet<String> = mods.iter().cloned().collect();
        let mut why = Vec::new();
        let led = self.ledger();
        loop {
            let mut added = false;
            for (o, r) in &v.recs {
                if !set.contains(&catalog::mod_of_owner(o)) {
                    continue;
                }
                for c in contributions(r) {
                    let f = s(&c, "file").unwrap_or_default();
                    let Ok(lay) = led.file_layers(f) else { continue };
                    let Some(i) = lay.iter().position(|(x, _)| x == o) else {
                        continue;
                    };
                    for (above, _) in lay.iter().skip(i + 1) {
                        let m = catalog::mod_of_owner(above);
                        if set.insert(m.clone()) {
                            why.push(format!("{m} ({f}: {above}'s install is built on {o}'s)"));
                            added = true;
                        }
                    }
                }
            }
            let enabled: BTreeSet<String> = self.list().map_or_default(|l| {
                l.into_iter()
                    .filter(|c| c.state != State::Disabled)
                    .map(|c| c.id)
                    .collect()
            });
            for (m, reqs) in catalog::REQUIRES {
                if !set.contains(*m)
                    && enabled.contains(*m)
                    && let Some(r) = reqs.iter().find(|r| set.contains(**r))
                {
                    set.insert((*m).to_owned());
                    why.push(format!("{m} (it needs {r})"));
                    added = true;
                }
            }
            if !added {
                return (set, why);
            }
        }
    }

    /// The steps taking `set`'s mods out (their owners top layer first, then their folders, then their settings off).
    fn plan_out(&self, v: &View, set: &BTreeSet<String>, action: &str) -> Result<Op, ModError> {
        let pairs: Vec<(String, String)> = v
            .recs
            .iter()
            .filter(|(o, _)| set.contains(&catalog::mod_of_owner(o)))
            .flat_map(|(o, r)| {
                contributions(r)
                    .into_iter()
                    .map(move |c| (o.clone(), s(&c, "file").unwrap_or_default().to_owned()))
            })
            .collect();
        let mut op = self.plan_takeout(v, &pairs, action)?;
        for (f, rec, m) in &v.folders {
            if !set.contains(&(*m).to_owned()) {
                continue;
            }
            let mut files = Vec::new();
            for rel in tree(&self.game, f) {
                let p = self.path(&rel);
                let old = legacy_markers(&p);
                if !old.is_empty() {
                    return Err(migrate_first(&rel, &old));
                }
                let sha = sha256::file(&p.to_string_lossy()).map_err(|e| super::ioe(&p, &e))?;
                files.push(FolderFile {
                    rel,
                    sha,
                    size: std::fs::metadata(&p).map_or(0, |m| m.len()),
                    mtime_ns: mtime_ns(&p).unwrap_or(0),
                });
            }
            op.steps.push(Step::MoveOut {
                entry: FolderStash {
                    format: STASH_FORMAT.to_owned(),
                    folder: f.clone(),
                    record: rec.clone(),
                    mod_id: (*m).to_owned(),
                    op: op.id.clone(),
                    seq: 0,
                    files,
                },
            });
        }
        for st in SETTINGS {
            if set.contains(st.id) && self.setting_on(st) == Some(true) {
                op.steps.push(Step::Setting {
                    file: st.file.to_owned(),
                    section: st.section.to_owned(),
                    key: st.key.to_owned(),
                    value: st.off.to_owned(),
                });
            }
        }
        Ok(op)
    }

    /// The steps taking each (owner, file) layer out, top layer first, every layer above one taken out being among
    /// them; what each wrote is kept first.
    #[allow(
        clippy::too_many_lines,
        clippy::many_single_char_names,
        reason = "the checks, the order, the kept bytes"
    )]
    fn plan_takeout(&self, v: &View, pairs: &[(String, String)], action: &str) -> Result<Op, ModError> {
        let id = new_id();
        let led = self.ledger();
        let mut refused = Vec::new();
        // the stacks of the files concerned
        let mut stacks: BTreeMap<String, Vec<(String, Value)>> = BTreeMap::new();
        for (o, f) in pairs {
            let low = f.to_lowercase();
            if stacks.contains_key(&low) {
                continue;
            }
            let p = self.path(f);
            let old = legacy_markers(&p);
            if !old.is_empty() {
                return Err(migrate_first(f, &old));
            }
            if let Ok(sl) = led.slot_holders(f)
                && let Some((so, what)) = sl.first()
            {
                refused.push(format!(
                    "{f}: {so} holds {what} in it: only Project OPFOR's ledger takes those out ({o})"
                ));
                continue;
            }
            let lay = led.file_layers(f).map_err(from_ledger)?;
            if let Some((top, c)) = lay.last() {
                let now = if p.is_file() {
                    sha256::file(&p.to_string_lossy()).ok()
                } else {
                    None
                };
                if now != value_sha(c) {
                    refused.push(format!(
                        "{f}: changed outside the install ledger since {top} installed it - put back what {top} \
                         wrote (or restore the file), then run again"
                    ));
                }
            }
            stacks.insert(low, lay);
        }
        let want: BTreeSet<(String, String)> = pairs.iter().map(|(o, f)| (o.clone(), f.to_lowercase())).collect();
        for (low, lay) in &stacks {
            if let Some(i) = lay.iter().position(|(o, _)| want.contains(&(o.clone(), low.clone()))) {
                for (above, _) in lay.iter().skip(i + 1) {
                    if !want.contains(&(above.clone(), low.clone())) {
                        refused.push(format!("{low}: {above} sits on it and is not part of this operation"));
                    }
                }
            }
        }
        if !refused.is_empty() {
            return Err(ModError::Refused(refused));
        }
        // the order: a layer goes when nothing above it is left
        let mut left: Vec<(String, String)> = pairs.to_vec();
        let mut gone: BTreeSet<(String, String)> = BTreeSet::new();
        let mut groups: Vec<(String, Vec<String>)> = Vec::new();
        let mut seq = v
            .stash
            .values()
            .flat_map(|s| s.contributions.iter().map(|c| c.seq))
            .chain(v.fstash.values().map(|f| f.seq))
            .max()
            .unwrap_or(0);
        let mut kept: BTreeMap<String, Vec<Kept>> = BTreeMap::new();
        let mut restates: Vec<(usize, Step)> = Vec::new();
        while !left.is_empty() {
            let is_top = |(o, f): &(String, String)| {
                let low = f.to_lowercase();
                stacks.get(&low).is_some_and(|lay| {
                    lay.iter()
                        .rev()
                        .find(|(x, _)| !gone.contains(&(x.clone(), low.clone())))
                        .is_some_and(|(x, _)| x == o)
                })
            };
            let last = groups.last().map(|g| g.0.clone());
            let pick = left
                .iter()
                .position(|p| Some(&p.0) == last.as_ref() && is_top(p))
                .or_else(|| left.iter().position(is_top));
            let Some(i) = pick else {
                return Err(ModError::Refused(vec![
                    "the layers to take out cannot be ordered (a damaged ledger stack): bfbc2 mods verify".to_owned(),
                ]));
            };
            let (o, f) = left.remove(i);
            let low = f.to_lowercase();
            let lay = stacks.get(&low).cloned().unwrap_or_default();
            let pos = lay.iter().position(|(x, _)| *x == o).unwrap_or(0);
            let c = lay.get(pos).map_or(Value::Null, |x| x.1.clone());
            let is_file_top = pos + 1 == lay.len();
            // what the layer wrote: the file (the top), else the prior of the layer above
            let vsha = value_sha(&c).unwrap_or_default();
            let src = if is_file_top {
                self.path(&f)
            } else {
                let above = lay.get(pos + 1).map_or(Value::Null, |x| x.1.clone());
                match above.get("prior").and_then(|p| s(p, "blob")) {
                    Some(b) => led.blob_path(b),
                    None => {
                        return Err(ModError::Refused(vec![format!(
                            "{f}: the ledger keeps no copy of what {o} wrote (the layer above created the file)"
                        )]));
                    }
                }
            };
            self.link_blob(&src, &vsha)?;
            let prior_v = c.get("prior").cloned().unwrap_or(Value::Null);
            let mut prior = s(&prior_v, "sha").map(|sha| Shasize {
                sha: sha.to_owned(),
                size: prior_v.get("size").and_then(Value::as_u64).unwrap_or(0),
            });
            let ini_file = f.to_lowercase().ends_with(".ini");
            if ini_file && let Some(b) = s(&prior_v, "blob") {
                self.link_blob(&led.blob_path(b), b)?;
            }
            // an owner's own ini keys alone go back (the edits since kept)
            let mut restate = None;
            let owned = catalog::OWNED_INI_KEYS
                .iter()
                .find(|(st, file, _)| o.split(':').next() == Some(*st) && f.eq_ignore_ascii_case(file));
            if let (Some((_, _, keys)), Some(p)) = (owned, prior.clone())
                && is_file_top
            {
                let cur = String::from_utf8(self.blob(&vsha)?).ok();
                let old = String::from_utf8(self.blob(&p.sha)?).ok();
                if let (Some(cur), Some(old)) = (cur, old) {
                    let new = ini::without_owned(&cur, &old, keys);
                    if new != old {
                        let sha = self.put_blob(new.as_bytes())?;
                        prior = Some(Shasize {
                            sha: sha.clone(),
                            size: new.len() as u64,
                        });
                        restate = Some(Step::RestatePrior {
                            owner: o.clone(),
                            file: f.clone(),
                            sha,
                        });
                    }
                }
            }
            seq += 1;
            kept.entry(o.clone()).or_default().push(Kept {
                file: f.clone(),
                value: Shasize {
                    sha: vsha,
                    size: c
                        .get("value")
                        .and_then(|v| v.get("size"))
                        .and_then(Value::as_u64)
                        .unwrap_or(0),
                },
                prior,
                derived: c
                    .get("extra")
                    .and_then(|e| e.get("derived"))
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                mtime_ns: if is_file_top { mtime_ns(&self.path(&f)) } else { None },
                seq,
            });
            gone.insert((o.clone(), low));
            match groups.last_mut() {
                Some(g) if g.0 == o => g.1.push(f),
                _ => groups.push((o, vec![f])),
            }
            if let Some(r) = restate {
                restates.push((groups.len() - 1, r));
            }
        }
        let mut steps = Vec::new();
        for (o, ks) in &kept {
            steps.push(Step::StashOwner {
                entry: OwnerStash {
                    format: STASH_FORMAT.to_owned(),
                    owner: o.clone(),
                    mod_id: catalog::mod_of_owner(o),
                    op: id.clone(),
                    note: v.recs.get(o).and_then(|r| s(r, "note")).unwrap_or_default().to_owned(),
                    contributions: ks.clone(),
                },
            });
        }
        for (i, (o, files)) in groups.into_iter().enumerate() {
            for (_, r) in restates.iter().filter(|(g, _)| *g == i) {
                steps.push(r.clone());
            }
            steps.push(Step::Uninstall { owner: o, files });
        }
        Ok(Op {
            format: super::OP_FORMAT.to_owned(),
            id,
            action: action.to_owned(),
            what: String::new(),
            steps,
        })
    }

    // ---------------------------------------------------------------------------------------------- enable

    /// Enables the mods from what was kept: the mods disabled with them too (unless `only`), the mods they need with
    /// `yes`. The lines say what was done.
    ///
    /// # Errors
    /// [`ModError::Confirm`] naming the mods added without `yes`; [`ModError::Refused`] when a kept file was built on a
    /// file that is not there any more, or a mod is not kept (nothing written).
    pub fn enable(&self, mods: &[String], only: bool, yes: bool) -> Result<Vec<String>, ModError> {
        self.locked(|lines| {
            let mods = &self.select(mods)?;
            let v = self.view()?;
            let mut set: BTreeSet<String> = mods.iter().cloned().collect();
            let mut with_them = Vec::new();
            let mut why = Vec::new();
            if !only {
                let ops: BTreeSet<&String> = v
                    .stash
                    .values()
                    .filter(|s| set.contains(&s.mod_id))
                    .map(|s| &s.op)
                    .chain(v.fstash.values().filter(|f| set.contains(&f.mod_id)).map(|f| &f.op))
                    .collect();
                let with: BTreeSet<String> = v
                    .stash
                    .values()
                    .filter(|s| ops.contains(&s.op))
                    .map(|s| s.mod_id.clone())
                    .chain(
                        v.fstash
                            .values()
                            .filter(|f| ops.contains(&f.op))
                            .map(|f| f.mod_id.clone()),
                    )
                    .collect();
                for m in with {
                    if set.insert(m.clone()) {
                        with_them.push(format!("{m} (disabled with them)"));
                    }
                }
            }
            let list = self.list()?;
            let off = |m: &str| list.iter().any(|c| c.id == m && c.state != State::Enabled);
            let mut more = true;
            while more {
                more = false;
                for m in set.clone() {
                    for r in catalog::requires(&m) {
                        if off(r) && set.insert((*r).to_owned()) {
                            why.push(format!("{r} ({m} needs it)"));
                            more = true;
                        }
                    }
                }
            }
            if !why.is_empty() && !yes {
                return Err(ModError::Confirm(confirm_lines("enable", mods, &why)));
            }
            let op = self.plan_in(&v, &set)?;
            if op.steps.is_empty() {
                lines.push(format!("{}: enabled already (nothing kept aside)", mods.join(", ")));
                return Ok(());
            }
            lines.extend(with_them.iter().chain(&why).map(|w| format!("also enabled: {w}")));
            self.run(&op, lines)?;
            lines.push(format!(
                "enabled: {}",
                set.iter().cloned().collect::<Vec<_>>().join(", ")
            ));
            Ok(())
        })
        .map(|((), l)| l)
    }

    /// The steps putting `set`'s kept units back: level folders, then the owners' files (the last out first), then
    /// settings on.
    #[allow(
        clippy::too_many_lines,
        clippy::many_single_char_names,
        reason = "the checks against the simulated files, then the steps"
    )]
    fn plan_in(&self, v: &View, set: &BTreeSet<String>) -> Result<Op, ModError> {
        let id = new_id();
        let mut steps = Vec::new();
        let mut refused = Vec::new();
        let led = self.ledger();
        for f in v.fstash.values().filter(|f| set.contains(&f.mod_id)) {
            for x in &f.files {
                if self.path(&x.rel).exists() && !self.kept_path(&x.rel).is_file() {
                    continue; // moved back already (an interrupted enable)
                }
                if self.path(&x.rel).exists() {
                    refused.push(format!("{}: there is a file there now - it is not put over", x.rel));
                } else if !self.kept_path(&x.rel).is_file() {
                    refused.push(format!(
                        "{}: the kept copy is missing ({})",
                        x.rel,
                        self.kept_path(&x.rel).display()
                    ));
                }
            }
            steps.push(Step::MoveIn {
                folder: f.folder.clone(),
            });
        }
        // every kept contribution, the last out first
        let mut all: Vec<(u64, String, Kept, String)> = Vec::new();
        for st in v.stash.values().filter(|s| set.contains(&s.mod_id)) {
            for c in &st.contributions {
                all.push((c.seq, st.owner.clone(), c.clone(), st.note.clone()));
            }
        }
        all.sort_by_key(|a| std::cmp::Reverse(a.0));
        // the files as they will be: sha and bytes (bytes read on demand)
        let mut state: BTreeMap<String, (Option<String>, Option<Vec<u8>>)> = BTreeMap::new();
        let mut groups: Vec<(String, bool, String, Vec<InFile>)> = Vec::new();
        for (_, owner, c, note) in &all {
            let low = c.file.to_lowercase();
            let p = self.path(&c.file);
            let old = legacy_markers(&p);
            if !old.is_empty() {
                return Err(migrate_first(&c.file, &old));
            }
            if !state.contains_key(&low) {
                let now = std::fs::read(&p).ok();
                state.insert(low.clone(), (now.as_deref().map(sha256::hex), now));
            }
            if let Ok(sl) = led.slot_holders(&c.file)
                && let Some((so, what)) = sl.first()
            {
                refused.push(format!(
                    "{}: {so} holds {what} in it - a whole file is not put over it",
                    c.file
                ));
                continue;
            }
            if self.blob(&c.value.sha).is_err() {
                refused.push(format!(
                    "{}: {owner}'s kept copy is missing or damaged (bfbc2 mods verify)",
                    c.file
                ));
                continue;
            }
            let (cur_sha, cur_bytes) = state.get(&low).cloned().unwrap_or((None, None));
            let want = c.prior.as_ref().map(|p| p.sha.clone());
            let mut put = InFile {
                file: c.file.clone(),
                sha: c.value.sha.clone(),
                mtime_ns: c.mtime_ns,
            };
            if cur_sha.as_deref() == Some(c.value.sha.as_str())
                && led
                    .file_layers(&c.file)
                    .ok()
                    .and_then(|l| l.last().map(|x| x.0.clone()))
                    .as_deref()
                    == Some(owner.as_str())
            {
                // in place already (an interrupted enable)
            } else if cur_sha != want {
                let rebased = (Path::new(&low)
                    .extension()
                    .is_some_and(|e| e.eq_ignore_ascii_case("ini")))
                .then(|| {
                    let base = c.prior.as_ref().and_then(|p| self.blob(&p.sha).ok())?;
                    let (b, n, o) = (
                        String::from_utf8(base).ok()?,
                        String::from_utf8(cur_bytes.clone()?).ok()?,
                        String::from_utf8(self.blob(&c.value.sha).ok()?).ok()?,
                    );
                    Some(ini::rebase(&b, &n, &o))
                })
                .flatten();
                let Some(r) = rebased else {
                    refused.push(format!(
                        "{}: {owner}'s kept copy was built on {} but the file is {} now - enable what was under it \
                         first (bfbc2 mods status), or install {} again",
                        c.file,
                        show(want.as_deref()),
                        show(cur_sha.as_deref()),
                        catalog::mod_of_owner(owner)
                    ));
                    continue;
                };
                put.sha = self.put_blob(r.as_bytes())?;
                put.mtime_ns = None;
            }
            let bytes = self.blob(&put.sha).ok();
            state.insert(low, (Some(put.sha.clone()), bytes));
            match groups.last_mut() {
                Some(g) if g.0 == *owner && g.1 == c.derived => g.3.push(put),
                _ => groups.push((owner.clone(), c.derived, note.clone(), vec![put])),
            }
        }
        if !refused.is_empty() {
            return Err(ModError::Refused(refused));
        }
        let mut done: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (owner, derived, note, files) in groups {
            done.entry(owner.clone())
                .or_default()
                .extend(files.iter().map(|f| f.file.clone()));
            steps.push(Step::Install {
                owner,
                derived,
                note,
                files,
            });
        }
        for (owner, files) in done {
            steps.push(Step::Unstash { owner, files });
        }
        for st in SETTINGS {
            if set.contains(st.id) && self.setting_on(st) == Some(false) {
                steps.push(Step::Setting {
                    file: st.file.to_owned(),
                    section: st.section.to_owned(),
                    key: st.key.to_owned(),
                    value: st.on.to_owned(),
                });
            }
        }
        Ok(Op {
            format: super::OP_FORMAT.to_owned(),
            id,
            action: "enable".to_owned(),
            what: format!("enable {}", set.iter().cloned().collect::<Vec<_>>().join(", ")),
            steps,
        })
    }

    // ---------------------------------------------------------------------------------------------- originals

    /// Every layer of the files given taken off (each owner's other files kept), or with `all` every mod but the
    /// settings disabled: DICE's files back (or none where an install created one). What came off is kept: the
    /// mods come back with `enable`.
    ///
    /// # Errors
    /// [`ModError::Confirm`] naming the mods concerned without `yes`; [`ModError::Refused`] (nothing written).
    pub fn restore_original(&self, files: &[String], all: bool, yes: bool) -> Result<Vec<String>, ModError> {
        self.locked(|lines| {
            let v = self.view()?;
            let op = if all {
                let set: BTreeSet<String> = self
                    .list()?
                    .into_iter()
                    .filter(|c| c.state != State::Disabled && !SETTINGS.iter().any(|s| s.id == c.id))
                    .map(|c| c.id)
                    .collect();
                if !yes {
                    return Err(ModError::Confirm(vec![format!(
                        "restore-original --all disables every mod: {} (kept: bfbc2 mods enable brings each back)",
                        set.iter().cloned().collect::<Vec<_>>().join(", ")
                    )]));
                }
                let mut op = self.plan_out(&v, &set, "restore-original")?;
                op.steps.retain(|s| !matches!(s, Step::Setting { .. }));
                "restore-original --all".clone_into(&mut op.what);
                op
            } else {
                let led = self.ledger();
                let mut pairs = Vec::new();
                let mut owners = BTreeSet::new();
                for f in files {
                    let rel = bc2_gamefs::ledger::norm_rel(f).map_err(from_ledger)?;
                    if v.folders
                        .iter()
                        .any(|(d, _, _)| rel.to_lowercase().starts_with(&format!("{}/", d.to_lowercase())))
                    {
                        return Err(ModError::Refused(vec![format!(
                            "{rel}: a file of a mod's level folder (not one of DICE's): disable the mod instead"
                        )]));
                    }
                    let lay = led.file_layers(&rel).map_err(from_ledger)?;
                    if lay.is_empty() {
                        lines.push(format!("{rel}: no install holds it (it is as it was)"));
                    }
                    for (o, _) in lay {
                        owners.insert(format!("{o} ({})", catalog::mod_of_owner(&o)));
                        pairs.push((o, rel.clone()));
                    }
                }
                if pairs.is_empty() {
                    return Ok(());
                }
                if !yes {
                    return Err(ModError::Confirm(vec![format!(
                        "restore-original takes these installs off {}: {} (kept: bfbc2 mods enable <mod> puts them \
                         back)",
                        files.join(", "),
                        owners.iter().cloned().collect::<Vec<_>>().join(", ")
                    )]));
                }
                let mut op = self.plan_takeout(&v, &pairs, "restore-original")?;
                op.what = format!("restore-original {}", files.join(", "));
                op
            };
            if op.steps.is_empty() {
                lines.push("nothing installed: every file is as it was".to_owned());
                return Ok(());
            }
            self.run(&op, lines)?;
            lines.push("restored: the files are as they were before any install".to_owned());
            Ok(())
        })
        .map(|((), l)| l)
    }
}

fn show(sha: Option<&str>) -> String {
    sha.map_or_else(|| "no file".to_owned(), |s| format!("sha {}", s.get(..12).unwrap_or(s)))
}

fn migrate_first(rel: &str, old: &[String]) -> ModError {
    ModError::Refused(vec![format!(
        "{rel} has install markers from before the install ledger ({}): migrate them first - bfbc2 mods migrate \
         <game> (a dry run), then with --apply",
        old.join(", ")
    )])
}

fn confirm_lines(what: &str, mods: &[String], why: &[String]) -> Vec<String> {
    let mut out = vec![format!("{what} {} changes other mods too:", mods.join(", "))];
    out.extend(why.iter().map(|w| format!("  {w}")));
    out.push("run again with --yes to go ahead".to_owned());
    out
}
