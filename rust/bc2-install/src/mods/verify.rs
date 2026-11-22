//! `bfbc2 mods verify`: every game file the ledger holds checked against its record (each layer's write, each prior
//! kept, DICE's original - the bottom layer's prior - intact by its sha256), every installed level folder against its
//! own install record, every kept copy against its sha256, the files with markers from before the ledger, and, when a
//! baseline was taken (`bfbc2 mods baseline`), every other game file against it.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use bc2_gamefs::sha256;
use rayon::prelude::*;
use serde_json::Value;

use super::{Manager, ModError, catalog, from_ledger, migrate};

/// What a verify found.
#[derive(Debug, Default)]
pub struct Report {
    /// One line per file or kept unit checked.
    pub lines: Vec<String>,
    /// The problems (also in `lines`, marked `FAILED`).
    pub problems: usize,
    /// Files checked.
    pub files: usize,
}

impl Report {
    fn ok(&mut self, line: &str) {
        self.files += 1;
        self.lines.push(format!("ok      {line}"));
    }
    fn bad(&mut self, line: &str) {
        self.files += 1;
        self.problems += 1;
        self.lines.push(format!("FAILED  {line}"));
    }
}

/// The baseline file (`bfbc2 mods baseline`): sha256 of every game file no install holds.
const BASELINE: &str = "baseline.json";

fn sha_file(p: &Path) -> Option<String> {
    sha256::file(&p.to_string_lossy()).ok()
}

impl Manager {
    /// Every check (see the module).
    ///
    /// # Errors
    /// A damaged ledger or stash record.
    #[allow(clippy::too_many_lines, reason = "one block per kind of check")]
    pub fn verify(&self) -> Result<Report, ModError> {
        let mut r = Report::default();
        let led = self.ledger();
        if led.interrupted() {
            r.bad("the install ledger: an interrupted commit (bfbc2 gameledger recover)");
        }
        if self.interrupted() {
            r.bad("the mod manager: an interrupted operation (bfbc2 mods resume)");
        }
        let recs = led.records().map_err(from_ledger)?;
        let game = self.game.clone();
        // the files the ledger holds
        let mut files: BTreeMap<String, String> = BTreeMap::new();
        for rec in recs.values() {
            for c in rec.get("contributions").and_then(Value::as_array).into_iter().flatten() {
                if let Some(f) = c.get("file").and_then(Value::as_str) {
                    if c.get("kind").and_then(Value::as_str) == Some("file") {
                        files.insert(f.to_lowercase(), f.to_owned());
                    } else {
                        r.lines.push(format!(
                            "skipped {f}: {} slot of Project OPFOR's ledger (checked by its own tools)",
                            c.get("kind").and_then(Value::as_str).unwrap_or("?")
                        ));
                    }
                }
            }
        }
        let checks: Vec<(String, Result<String, String>)> = files
            .values()
            .collect::<Vec<_>>()
            .par_iter()
            .map(|f| ((*f).clone(), check_stack(&game, f)))
            .collect();
        for (f, res) in checks {
            match res {
                Ok(m) => r.ok(&format!("{f}: {m}")),
                Err(m) => r.bad(&format!("{f}: {m}")),
            }
        }
        // installed level folders against their records
        for (folder, rec, m) in catalog::folders(&self.game) {
            let rp = self.path(&format!("{folder}/{rec}"));
            let Some(map) = fs::read(&rp)
                .ok()
                .and_then(|d| serde_json::from_slice::<Value>(&d).ok())
            else {
                r.bad(&format!("{folder}: its record {rec} cannot be read"));
                continue;
            };
            // a WW2 record maps a kind to its bundle's sha; the map editor's has `installed` {path: sha}
            let pairs: Vec<(String, String)> = if let Some(i) = map.get("installed").and_then(Value::as_object) {
                i.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.replace('\\', "/"), s.to_owned())))
                    .collect()
            } else {
                map.as_object()
                    .into_iter()
                    .flatten()
                    .filter_map(|(k, v)| v.as_str().map(|s| (format!("{k}-00.fbrb"), s.to_owned())))
                    .collect()
            };
            let mut bad = Vec::new();
            for (name, want) in &pairs {
                let p = self.path(&format!("{folder}/{name}"));
                if sha_file(&p).as_deref() != Some(want.as_str()) {
                    bad.push(name.clone());
                }
            }
            if bad.is_empty() {
                r.ok(&format!(
                    "{folder}: {} bundles as its install recorded ({m})",
                    pairs.len()
                ));
            } else {
                r.bad(&format!(
                    "{folder}: {} not as its install recorded ({m})",
                    bad.join(", ")
                ));
            }
        }
        // kept copies
        for (o, s) in self.owner_stashes()? {
            let mut bad = Vec::new();
            for c in &s.contributions {
                if sha_file(&self.blob_path(&c.value.sha)).as_deref() != Some(c.value.sha.as_str()) {
                    bad.push(c.file.clone());
                }
            }
            if bad.is_empty() {
                r.ok(&format!("kept {o} ({}): {} files", s.mod_id, s.contributions.len()));
            } else {
                r.bad(&format!(
                    "kept {o} ({}): missing or damaged: {}",
                    s.mod_id,
                    bad.join(", ")
                ));
            }
        }
        for s in self.folder_stashes()?.values() {
            let bad: Vec<&str> = s
                .files
                .iter()
                .filter(|f| sha_file(&self.kept_path(&f.rel)).as_deref() != Some(f.sha.as_str()))
                .map(|f| f.rel.as_str())
                .collect();
            if bad.is_empty() {
                r.ok(&format!("kept {} ({}): {} files", s.folder, s.mod_id, s.files.len()));
            } else {
                r.bad(&format!(
                    "kept {} ({}): missing or damaged: {}",
                    s.folder,
                    s.mod_id,
                    bad.join(", ")
                ));
            }
        }
        // markers from before the ledger
        let old = migrate::scan(&self.game);
        if !old.is_empty() {
            r.bad(&format!(
                "{} files have install markers from before the ledger: bfbc2 mods migrate (dry run), then --apply",
                old.len()
            ));
        }
        // the baseline
        let bp = self.root.join(BASELINE);
        if let Some(base) = fs::read(&bp)
            .ok()
            .and_then(|d| serde_json::from_slice::<BTreeMap<String, String>>(&d).ok())
        {
            let held: BTreeSet<String> = files.keys().cloned().collect();
            let rows: Vec<(&String, &String)> =
                base.iter().filter(|(k, _)| !held.contains(&k.to_lowercase())).collect();
            let checked: Vec<(String, bool)> = rows
                .par_iter()
                .map(|(k, v)| {
                    (
                        (*k).clone(),
                        sha_file(&game_path(&game, k)).as_deref() == Some(v.as_str()),
                    )
                })
                .collect();
            let bad: Vec<&String> = checked.iter().filter(|(_, ok)| !ok).map(|(k, _)| k).collect();
            if bad.is_empty() {
                r.ok(&format!(
                    "baseline: {} other game files as when it was taken",
                    checked.len()
                ));
            } else {
                for b in bad {
                    r.bad(&format!("{b}: not as in the baseline ({})", bp.display()));
                }
            }
        } else {
            r.lines.push(format!(
                "note    no baseline of the other game files ({}): bfbc2 mods baseline <game> takes one",
                bp.display()
            ));
        }
        Ok(r)
    }
}

fn game_path(game: &Path, rel: &str) -> std::path::PathBuf {
    rel.split('/').fold(game.to_path_buf(), |p, x| p.join(x))
}

/// One file's stack: the top layer's write is the file, each layer's write the prior of the one above, every prior
/// kept by its sha256; Ok(what was found) or Err(the problem).
fn check_stack(game: &Path, f: &str) -> Result<String, String> {
    let led = bc2_gamefs::ledger::Ledger::new(game);
    let lay = led.file_layers(f).map_err(|e| e.to_string())?;
    let mut prev_value: Option<Option<String>> = None;
    for (i, (o, c)) in lay.iter().enumerate() {
        let prior = c.get("prior").filter(|p| !p.is_null());
        let psha = prior
            .and_then(|p| p.get("sha"))
            .and_then(Value::as_str)
            .map(str::to_owned);
        if let Some(b) = prior.and_then(|p| p.get("blob")).and_then(Value::as_str)
            && sha_file(&led.blob_path(b)).as_deref() != Some(b)
        {
            return Err(format!(
                "the ledger's copy from before {o} is missing or damaged{}",
                if i == 0 { " (DICE's original)" } else { "" }
            ));
        }
        if let Some(pv) = &prev_value
            && *pv != psha
        {
            return Err(format!(
                "{o}'s prior is not what the layer below wrote (a damaged stack)"
            ));
        }
        prev_value = Some(
            c.get("value")
                .and_then(|v| v.get("sha"))
                .and_then(Value::as_str)
                .map(str::to_owned),
        );
    }
    let now = sha_file(&game_path(game, f));
    let top = prev_value.flatten();
    if now != top {
        return Err(format!(
            "changed outside the ledger (the file is {}, {} wrote {})",
            now.as_deref().map_or("missing", |s| s.get(..12).unwrap_or(s)),
            lay.last().map_or("?", |x| x.0.as_str()),
            top.as_deref().map_or("?", |s| s.get(..12).unwrap_or(s))
        ));
    }
    let orig = lay
        .first()
        .and_then(|(_, c)| c.get("prior").and_then(|p| p.get("sha")).and_then(Value::as_str))
        .map_or_else(
            || "none (an install created it)".to_owned(),
            |s| format!("sha {}", s.get(..12).unwrap_or(s)),
        );
    Ok(format!(
        "{} ({} layers; DICE's original {orig} kept)",
        lay.iter().map(|x| x.0.as_str()).collect::<Vec<_>>().join(" -> "),
        lay.len()
    ))
}

impl Manager {
    /// Takes the baseline: the sha256 of every game file no install holds and no mod's folder has (outside `opfor`),
    /// written to `<game>\opfor\mods\baseline.json`. The number of files.
    ///
    /// # Errors
    /// A damaged ledger record; the file cannot be written.
    pub fn baseline(&self) -> Result<usize, ModError> {
        let recs = self.ledger().records().map_err(from_ledger)?;
        let held: BTreeSet<String> = recs
            .values()
            .flat_map(|r| {
                r.get("contributions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default()
            })
            .filter_map(|c| c.get("file").and_then(Value::as_str).map(str::to_lowercase))
            .collect();
        let folders: Vec<String> = catalog::folders(&self.game)
            .into_iter()
            .map(|(f, _, _)| format!("{}/", f.to_lowercase()))
            .collect();
        let mut all = Vec::new();
        let mut stack = vec![self.game.clone()];
        let skip = self.game.join("opfor");
        while let Some(d) = stack.pop() {
            let Ok(rd) = fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let Ok(ft) = e.file_type() else { continue };
                if ft.is_symlink() {
                    continue;
                }
                let p = e.path();
                if ft.is_dir() {
                    if p != skip {
                        stack.push(p);
                    }
                } else if let Ok(rel) = p.strip_prefix(&self.game) {
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    let low = rel.to_lowercase();
                    if !held.contains(&low) && !folders.iter().any(|f| low.starts_with(f)) {
                        all.push(rel);
                    }
                }
            }
        }
        let game = self.game.clone();
        let rows: BTreeMap<String, String> = all
            .par_iter()
            .filter_map(|rel| sha_file(&game_path(&game, rel)).map(|s| (rel.clone(), s)))
            .collect();
        self.write_json(&self.root.join(BASELINE), &rows)?;
        Ok(rows.len())
    }
}
