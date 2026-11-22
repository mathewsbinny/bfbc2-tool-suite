//! The backup guard (Project OPFOR's `wake_install.ww2_marks`, `before_wake`, `guard_backup`,
//! `ini_backup_refresh`): what `--revert` puts back must be the file from before the install, so the copy the ledger
//! keeps (or would take now) may not hold the install's own content. What counts as that content is the plan's
//! (OPFOR's decision, [`MarksSpec`]); the rules are these.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use bc2_formats::fbrb;
use bc2_gamefs::ledger::Ledger;
use bc2_gamefs::sha256;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::pytext::strip;
use crate::text::{ini_value, ini_with_key, ini_with_level, ini_without_key};

/// What marks a file as holding the install's own content (OPFOR's `WW2_ENTRY`, `WW2_TEXT`, `UI_EDGE_MARK`,
/// `WW2_LIST_MARK`, the names build's folder).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MarksSpec {
    /// A bundle entry name of the install's own (matched at the start, as Python's `re.match`).
    pub entry_re: String,
    /// A line of the install's own in a text file (`package.mft`, `bundleManifest`, the ini files).
    pub text_re: String,
    /// The entries whose data are searched for `ui_mark` (`systems/ui/shaders/`).
    pub ui_prefix: String,
    /// The install's entry name inside those databases (`PacificMinimap`).
    pub ui_mark: String,
    /// A folder of the install's own files: an entry byte-identical to one of them is the install's
    /// (`work\ww2\names`).
    pub own_root: String,
    /// The menus' level list entry (`LevelListQuery.dbx`).
    pub list_entry: String,
    /// A level of the install named in that list's bytes.
    pub list_re: String,
}

/// The compiled guard of one install.
#[derive(Debug)]
pub struct Guard {
    /// The game copy.
    pub game: PathBuf,
    /// The ledger owner of the shared files (`wake`).
    pub owner: String,
    /// `opfor.ini` (as the plan spells its path).
    pub ini: String,
    entry: Regex,
    text: Regex,
    list: regex::bytes::Regex,
    spec: MarksSpec,
    own: BTreeMap<String, PathBuf>,
}

/// A refusal in plain words (Python's `SystemExit` / `BackupRefused`): nothing was written.
pub type Refusal = String;

fn read_head(p: &Path, n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    let got = fs::File::open(p).and_then(|mut f| f.read(&mut b)).unwrap_or(0);
    b.truncate(got);
    b
}

/// `tree_files`: (name relative to top with `/`, path) of every file under top, sorted (none when missing).
#[must_use]
pub fn tree_files(top: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let mut stack = vec![top.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.is_file()
                && let Ok(rel) = p.strip_prefix(top)
            {
                out.push((rel.to_string_lossy().replace('\\', "/"), p));
            }
        }
    }
    out.sort();
    out
}

impl Guard {
    /// The guard of the plan's spec.
    ///
    /// # Errors
    /// A pattern that does not compile.
    pub fn new(game: &Path, owner: &str, ini: &str, spec: &MarksSpec) -> Result<Self, String> {
        let entry = Regex::new(&spec.entry_re).map_err(|e| format!("marks entry_re: {e}"))?;
        let text = Regex::new(&spec.text_re).map_err(|e| format!("marks text_re: {e}"))?;
        let list =
            regex::bytes::Regex::new(&format!("(?-u){}", spec.list_re)).map_err(|e| format!("marks list_re: {e}"))?;
        let own = tree_files(Path::new(&spec.own_root))
            .into_iter()
            .map(|(r, p)| (r.to_lowercase(), p))
            .collect();
        Ok(Self {
            game: game.to_path_buf(),
            owner: owner.to_owned(),
            ini: ini.to_owned(),
            entry,
            text,
            list,
            spec: spec.clone(),
            own,
        })
    }

    /// Whether a text holds the install's own lines (`WW2_TEXT.search`).
    #[must_use]
    pub fn text_marked(&self, text: &str) -> bool {
        self.text.is_match(text)
    }

    /// `ww2_marks`: the install's own content in a file (empty when none): in a bundle (by its magic) the entries
    /// the spec names; in a text file its lines (sorted, at most 20).
    ///
    /// # Errors
    /// A bundle that cannot be read.
    pub fn marks(&self, path: &Path) -> Result<Vec<String>, String> {
        if !path.is_file() {
            return Ok(Vec::new());
        }
        if read_head(path, 4) != b"FbRB" {
            let data = fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let text = String::from_utf8_lossy(&data);
            let set: BTreeSet<String> = self
                .text
                .find_iter(&text)
                .map(|m| strip(m.as_str()).to_owned())
                .collect();
            return Ok(set.into_iter().take(20).collect());
        }
        let h = fbrb::entries_path(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let live: Vec<&str> = h
            .entries
            .iter()
            .filter(|e| !e.is_deleted())
            .map(|e| e.name.as_str())
            .collect();
        let mut hits: Vec<String> = live
            .iter()
            .filter(|n| self.entry.find(n).is_some_and(|m| m.start() == 0))
            .map(|n| (*n).to_owned())
            .collect();
        let ui_pre = self.spec.ui_prefix.to_lowercase();
        let ui: Vec<&str> = live
            .iter()
            .copied()
            .filter(|n| n.to_lowercase().starts_with(&ui_pre))
            .collect();
        let own: Vec<&str> = live
            .iter()
            .copied()
            .filter(|n| self.own.contains_key(&n.to_lowercase()))
            .collect();
        let lists: Vec<&str> = live
            .iter()
            .copied()
            .filter(|n| n.to_lowercase() == self.spec.list_entry.to_lowercase())
            .collect();
        let mut want: Vec<&str> = Vec::new();
        for n in ui.iter().chain(&own).chain(&lists) {
            if !want.contains(n) {
                want.push(n);
            }
        }
        let data: BTreeMap<&str, Vec<u8>> = if want.is_empty() {
            BTreeMap::new()
        } else {
            let got = bc2_gamefs::bundles::read_path(path, &want).map_err(|e| format!("{}: {e}", path.display()))?;
            want.iter().copied().zip(got).collect()
        };
        let mark = self.spec.ui_mark.as_bytes();
        hits.extend(
            ui.iter()
                .filter(|n| {
                    data.get(*n)
                        .is_some_and(|d| d.windows(mark.len().max(1)).any(|w| w == mark))
                })
                .map(|n| (*n).to_owned()),
        );
        hits.extend(
            lists
                .iter()
                .filter(|n| data.get(*n).is_some_and(|d| self.list.is_match(d)))
                .map(|n| (*n).to_owned()),
        );
        for n in &own {
            if let Some(p) = self.own.get(&n.to_lowercase())
                && let Ok(f) = fs::read(p)
                && data.get(n) == Some(&f)
            {
                hits.push((*n).to_owned());
            }
        }
        Ok(hits)
    }

    fn rel(&self, path: &str) -> Option<String> {
        let p = Path::new(path);
        let r = p.strip_prefix(&self.game).ok()?;
        bc2_gamefs::ledger::norm_rel(&r.to_string_lossy()).ok()
    }

    /// `before_wake` / `stages.before_path`: a path to the file as it was before the owner wrote it - the old marker
    /// backup `<file>.opfor-<stage>-orig`, else the ledger's copy when the owner holds the file, else the file
    /// itself (also when the owner created it).
    #[must_use]
    pub fn before(&self, path: &str) -> String {
        let legacy = format!("{path}.opfor-{}-orig", self.owner.split(':').next().unwrap_or_default());
        if Path::new(&legacy).is_file() {
            return legacy;
        }
        let Some(rel) = self.rel(path) else {
            return path.to_owned();
        };
        let led = Ledger::new(&self.game);
        match led.file_layers(&rel) {
            Ok(layers) => {
                for (o, c) in layers {
                    if o == self.owner {
                        return c
                            .get("prior")
                            .and_then(|p| p.get("blob"))
                            .and_then(serde_json::Value::as_str)
                            .map_or_else(|| path.to_owned(), |b| led.blob_path(b).to_string_lossy().into_owned());
                    }
                }
                path.to_owned()
            }
            Err(_) => path.to_owned(),
        }
    }

    /// `Stages.is_top`: the owner wrote the file last and it is still that write.
    #[must_use]
    pub fn is_top(&self, path: &str) -> bool {
        let Some(rel) = self.rel(path) else { return false };
        let Ok(layers) = Ledger::new(&self.game).file_layers(&rel) else {
            return false;
        };
        let Some((o, c)) = layers.last() else { return false };
        if *o != self.owner {
            return false;
        }
        let want = c
            .get("value")
            .and_then(|v| v.get("sha"))
            .and_then(serde_json::Value::as_str);
        Path::new(path).is_file() && sha256::file(path).ok().as_deref() == want
    }

    /// `ini_backup_refresh`: the `opfor.ini` copy from before the install brought up to date with edits made since
    /// (the launcher writes the file without a record): None unless the copy is clean, the file holds only the
    /// install's own keys of its content and the owner is not its last writer. The keys are `[dev] level` and
    /// `[gamemode] pacific_levels`, back at the copy's values (a line the copy never had left out).
    ///
    /// # Errors
    /// A file that cannot be read; the ini without a `[dev] level=` line.
    pub fn ini_refresh(&self, path: &str) -> Result<Option<String>, Refusal> {
        if path != self.ini {
            return Ok(None);
        }
        let backup = if Path::new(path).is_file() {
            self.before(path)
        } else {
            path.to_owned()
        };
        if backup == path
            || !Path::new(&backup).is_file()
            || self.is_top(path)
            || !self.marks(Path::new(&backup))?.is_empty()
            || self.marks(Path::new(path))?.is_empty()
        {
            return Ok(None);
        }
        let read = |p: &str| -> Result<String, Refusal> {
            let d = fs::read(p).map_err(|e| format!("{p}: {e}"))?;
            String::from_utf8(d).map_err(|e| format!("{p}: not UTF-8 ({e})"))
        };
        let (cur, old) = (read(path)?, read(&backup)?);
        let Some(level) = ini_value(&old, "dev", "level") else {
            return Ok(None);
        };
        let mut new = ini_with_level(&cur, &level).map_err(|e| e.0)?;
        let pre = ini_value(&old, "gamemode", "pacific_levels");
        let now = ini_value(&new, "gamemode", "pacific_levels");
        if pre != now {
            new = match pre {
                None => ini_without_key(&new, "gamemode", "pacific_levels"),
                Some(p) => ini_with_key(&new, "gamemode", "pacific_levels", &p),
            };
        }
        if self.text_marked(&new) {
            return Ok(None);
        }
        Ok(Some(new))
    }

    /// `guard_backup`: refused (nothing written) when the copy the ledger would keep for path holds the install's own
    /// content - its existing copy does, or, when the copy would be taken now, the file itself does.
    ///
    /// # Errors
    /// The refusal, in words, with the way to recover.
    pub fn check(&self, path: &str) -> Result<(), Refusal> {
        if self.ini_refresh(path)?.is_some() {
            return Ok(());
        }
        let backup = self.before(path);
        let held = backup != path;
        let redo = held && self.is_top(path);
        let show = |bad: &[String]| {
            format!(
                "{}{}",
                bad.iter().take(4).cloned().collect::<Vec<_>>().join(", "),
                if bad.len() > 4 { " ..." } else { "" }
            )
        };
        if held {
            let bad = self.marks(Path::new(&backup))?;
            if !bad.is_empty() {
                return Err(format!(
                    "{backup}: holds the WW2 install's own content ({}) - it is not the file from before the Pacific \
                     War install, and --revert would leave the WW2 content in. Nothing was written. Recover: put the \
                     file as it was before the WW2 install there (for Package\\async\\startup-00.fbrb: python \
                     src\\tools\\ww2\\ww2_backup_repair.py builds and verifies it and prints the copy command), then \
                     run again.",
                    show(&bad)
                ));
            }
        }
        if redo || held {
            return Ok(());
        }
        let bad = self.marks(Path::new(path))?;
        if !bad.is_empty() {
            return Err(format!(
                "{path}: already holds the WW2 install's own content ({}) but the install ledger does not record the \
                 WW2 install on it - recording the file as it is for the one before the WW2 install would keep the \
                 WW2 content in the opfor-out state. Nothing was written. Recover: put the file from before the WW2 \
                 install back (startup-00: ww2_backup_repair.py), or revert the other tool first; then run again.",
                show(&bad)
            ));
        }
        Ok(())
    }
}
