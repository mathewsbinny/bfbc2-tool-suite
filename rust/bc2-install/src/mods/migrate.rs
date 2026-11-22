//! The one-time migration of the install markers from before the install ledger (Project OPFOR's
//! `opforcore\migrate.py`, round DF2), in the suite: `bfbc2 mods migrate <game>` (the plan) and `--apply`.
//!
//! Before DF2 every tool kept its own markers beside the game file it wrote: `<file>.opfor-<stage>-orig` (the file
//! before the stage), `<file>.opfor-last` (`<stage> <sha256>` of the last writer), `<file>.opfor-<stage>-applied`,
//! NavKit's `.opfor-orig` / `.opfor-new`, stock-MP's `.opfor-stockmp-orig` / `-applied`, the import's `.opfor-orig`.
//! Per file the backups, oldest first (their write time), are the stack of installs: each backup the prior of its
//! stage, the next backup (or the file, for the last) what it wrote. The owners are named as the tools name them now
//! (`stages.owner_of`). A file that fails a check is refused in words and left exactly as it is; `--apply` keeps each
//! backup as a ledger blob (a hard link), writes the records (the ledger's lock and journal), then moves every old
//! marker to `<game>\opfor\legacy-markers\<path>` (kept, never deleted). Run again after an interruption, it goes on
//! from the records it wrote.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use bc2_gamefs::ledger::{FORMAT, Ledger, norm_rel, valid_owner};
use bc2_gamefs::sha256;
use regex::Regex;
use serde_json::{Value, json};

use super::{ModError, from_ledger, ioe, mtime_ns};
use crate::run::LEGACY;

/// Where the old markers are moved (`migrate.ARCHIVE`).
pub const ARCHIVE: &str = "opfor/legacy-markers";
/// Stages whose backup is kept with the archive only (`migrate.AUX_STAGES`: `wake_install`'s copy before its manifest
/// repair).
const AUX_STAGES: &[&str] = &["manifestfix"];
/// Whole files built elsewhere: an out-of-order revert splices them (`migrate.INDEPENDENT`).
const INDEPENDENT: &[&str] = &["navkit", "stockmp"];
const LEVEL_DIRS: &[&str] = &["package/levels/", "dist/win32/levels/", "output/win32/ai/levels/"];
const NOTE: &str = "migrated from the old install markers (DF2)";

static BACKUP: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used, reason = "a constant pattern, checked by the tests")]
    Regex::new(r"(?i)^\.opfor-(?:([a-z0-9_.-]+)-)?orig$").expect("valid")
});
static LOADING: LazyLock<Regex> = LazyLock::new(|| {
    #[allow(clippy::expect_used, reason = "a constant pattern")]
    Regex::new(r"^loading-(.+)$").expect("valid")
});

/// One layer of a file's stack.
#[derive(Debug, Clone)]
pub struct Layer {
    /// The owner.
    pub owner: String,
    /// The marker holding its prior (None: `.opfor-new`, no original).
    pub backup: Option<PathBuf>,
    /// The marker's write time.
    pub time: u64,
    /// The prior's sha256.
    pub prior_sha: Option<String>,
    /// What it wrote.
    pub value_sha: String,
}

/// One file's plan.
#[derive(Debug, Clone, Default)]
pub struct FilePlan {
    /// The game-relative file.
    pub rel: String,
    /// Its marker files' names.
    pub markers: Vec<String>,
    /// Its stack, bottom first.
    pub layers: Vec<Layer>,
    /// Why it is refused ('' when it is not).
    pub refused: String,
    /// Notes.
    pub notes: Vec<String>,
    /// Recorded by an earlier run (only the markers left to archive).
    pub done: bool,
}

/// `stages.level_of`.
fn level_of(rel: &str) -> Option<String> {
    let low = rel.replace('\\', "/").to_lowercase();
    for d in LEVEL_DIRS {
        if let Some(rest) = low.strip_prefix(d) {
            let parts: Vec<&str> = rest.split('/').collect();
            return (parts.len() > 1).then(|| parts.first().copied().unwrap_or_default().to_owned());
        }
    }
    None
}

/// `stages.owner_of`, refused in words for a name the ledger does not take.
fn owner_of(stage: &str, level: Option<&str>) -> Result<String, String> {
    let o = format!(
        "{}{}",
        stage.to_lowercase(),
        level.map_or_else(String::new, |l| format!(":{}", l.to_lowercase()))
    );
    if valid_owner(&o) {
        Ok(o)
    } else {
        Err(format!("not a stage name: {o:?}"))
    }
}

/// `migrate._owner_for`.
fn owner_for(rel: &str, stage: Option<&str>) -> Result<String, String> {
    let lv = level_of(rel);
    let Some(stage) = stage else {
        let low = rel.to_lowercase();
        if low.ends_with("/pathdata.hpd") && low.starts_with("output/win32/ai/levels/") && lv.is_some() {
            return owner_of("navkit", lv.as_deref());
        }
        return Ok("import".to_owned());
    };
    let low = stage.to_lowercase();
    if let Some(m) = LOADING.captures(&low).and_then(|c| c.get(1)) {
        return owner_of("loading", Some(m.as_str()));
    }
    owner_of(&low, lv.as_deref())
}

/// `migrate.scan`: {game-relative file: its old marker names} (the `opfor` folder, links and junctions not walked).
#[must_use]
pub fn scan(game: &Path) -> BTreeMap<String, Vec<String>> {
    let mut out: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let skip = game.join("opfor");
    let mut stack = vec![game.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        let mut files = Vec::new();
        for e in rd.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_symlink() {
                continue;
            }
            if ft.is_dir() {
                if !e.path().as_os_str().eq_ignore_ascii_case(skip.as_os_str()) {
                    stack.push(e.path());
                }
            } else {
                files.push(e.file_name().to_string_lossy().into_owned());
            }
        }
        let low: BTreeMap<String, String> = files.iter().map(|f| (f.to_lowercase(), f.clone())).collect();
        for f in &files {
            let lf = f.to_lowercase();
            let Some(i) = lf.find(".opfor-").filter(|i| *i > 0) else {
                continue;
            };
            let Some(suffix) = f.get(i..) else { continue };
            if !LEGACY.is_match(suffix) {
                continue;
            }
            let stem = f.get(..i).unwrap_or_default();
            let base = low.get(&stem.to_lowercase()).cloned();
            if base.is_none()
                && ![".opfor-new", "-orig", "-applied", ".opfor-last"]
                    .iter()
                    .any(|x| lf.ends_with(x))
            {
                continue;
            }
            let target = base.unwrap_or_else(|| stem.to_owned());
            let Ok(r) = d.join(&target).strip_prefix(game).map(Path::to_path_buf) else {
                continue;
            };
            out.entry(r.to_string_lossy().replace('\\', "/"))
                .or_default()
                .push(f.clone());
        }
    }
    for v in out.values_mut() {
        v.sort();
    }
    out
}

fn read_text(p: &Path) -> String {
    fs::read(p).map_or_default(|d| String::from_utf8_lossy(&d).into_owned())
}

/// `migrate.plan_file`.
#[allow(clippy::too_many_lines, reason = "migrate.plan_file's checks, in its order")]
fn plan_file(game: &Path, rel: &str, markers: &[String], recs: &BTreeMap<String, Value>) -> FilePlan {
    let mut fp = FilePlan {
        rel: rel.to_owned(),
        markers: markers.to_vec(),
        ..FilePlan::default()
    };
    let path = rel.split('/').fold(game.to_path_buf(), |p, x| p.join(x));
    let base = path.file_name().map_or_default(|b| b.to_string_lossy().into_owned());
    if !path.is_file() {
        "the file is missing (its markers are kept where they are)".clone_into(&mut fp.refused);
        return fp;
    }
    let held: Vec<String> = recs
        .iter()
        .filter(|(_, r)| {
            r.get("contributions").and_then(Value::as_array).is_some_and(|cs| {
                cs.iter().any(|c| {
                    c.get("file")
                        .and_then(Value::as_str)
                        .is_some_and(|f| f.eq_ignore_ascii_case(rel))
                })
            })
        })
        .map(|(o, _)| o.clone())
        .collect();
    let dir = path.parent().map_or_default(Path::to_path_buf);
    let mut layers: Vec<Layer> = Vec::new();
    let mut last: Option<Vec<String>> = None;
    let mut applied: BTreeMap<String, String> = BTreeMap::new();
    for m in markers {
        let suffix = m.get(base.len()..).unwrap_or_default();
        let mp = dir.join(m);
        let sl = suffix.to_lowercase();
        if let Some(c) = BACKUP.captures(suffix) {
            let stage = c.get(1).map(|x| x.as_str().to_owned());
            if stage
                .as_deref()
                .is_some_and(|s| AUX_STAGES.contains(&s.to_lowercase().as_str()))
            {
                continue;
            }
            match owner_for(rel, stage.as_deref()) {
                Ok(owner) => layers.push(Layer {
                    owner,
                    time: mtime_ns(&mp).unwrap_or(0),
                    prior_sha: sha256::file(&mp.to_string_lossy()).ok(),
                    backup: Some(mp),
                    value_sha: String::new(),
                }),
                Err(e) => {
                    fp.refused = e;
                    return fp;
                }
            }
        } else if sl == ".opfor-new" {
            match owner_for(rel, None) {
                Ok(owner) => layers.push(Layer {
                    owner,
                    time: mtime_ns(&mp).unwrap_or(0),
                    prior_sha: None,
                    backup: None,
                    value_sha: String::new(),
                }),
                Err(e) => {
                    fp.refused = e;
                    return fp;
                }
            }
        } else if sl == ".opfor-last" {
            last = Some(read_text(&mp).split_whitespace().map(str::to_owned).collect());
        } else if sl.ends_with("-applied") {
            let k = suffix
                .get(".opfor-".len()..suffix.len() - "-applied".len())
                .unwrap_or_default()
                .to_lowercase();
            applied.insert(k, read_text(&mp).trim().to_owned());
        }
    }
    if layers.is_empty() {
        fp.notes
            .push("no backup: nothing to record (the markers are archived)".to_owned());
        fp.done = held.is_empty();
        return fp;
    }
    if !held.is_empty() {
        let mine: std::collections::BTreeSet<&String> = held.iter().collect();
        let theirs: std::collections::BTreeSet<&String> = layers.iter().map(|l| &l.owner).collect();
        // a backup whose owner holds no record: a different install (an interrupted run moves markers one by one,
        // so the backups left may be fewer than the records: those are recorded already)
        if !theirs.is_subset(&mine) {
            fp.refused = format!(
                "ledger owners already hold the file ({}) beside the old markers",
                held.join(", ")
            );
            return fp;
        }
        fp.done = true;
        fp.layers = layers;
        fp.notes
            .push("recorded by an earlier run: only the markers are left to archive".to_owned());
        return fp;
    }
    layers.sort_by_key(|l| l.time);
    let mut times: Vec<u64> = layers.iter().map(|l| l.time).collect();
    times.dedup();
    if times.len() != layers.len() {
        "two backups have the same write time: their order cannot be told".clone_into(&mut fp.refused);
        return fp;
    }
    let mut owners: Vec<&String> = layers.iter().map(|l| &l.owner).collect();
    owners.sort();
    owners.dedup();
    if owners.len() != layers.len() {
        fp.refused = format!(
            "two backups map to one owner ({})",
            layers.iter().map(|l| l.owner.clone()).collect::<Vec<_>>().join(", ")
        );
        return fp;
    }
    if layers.iter().skip(1).any(|l| l.backup.is_none()) {
        "NavKit's .opfor-new (no original) above another backup: the order is not one NavKit makes"
            .clone_into(&mut fp.refused);
        return fp;
    }
    let now = sha256::file(&path.to_string_lossy()).unwrap_or_default();
    let n = layers.len();
    for i in 0..n {
        let v = if i + 1 < n {
            layers.get(i + 1).and_then(|l| l.prior_sha.clone()).unwrap_or_default()
        } else {
            now.clone()
        };
        if let Some(l) = layers.get_mut(i) {
            l.value_sha = v;
        }
    }
    let top = layers.last().cloned();
    if let (Some(last), Some(top)) = (last, &top) {
        let [stage, s] = last.as_slice() else {
            fp.refused = format!("{base}.opfor-last is not \"<stage> <sha>\": {:?}", last.join(" "));
            return fp;
        };
        if s.to_lowercase() != now {
            fp.refused = format!(
                "{base}.opfor-last names sha {} ({stage}) but the file is sha {}: it was changed after the last \
                 recorded install (or truncated)",
                s.get(..12).unwrap_or(s),
                now.get(..12).unwrap_or(&now)
            );
            return fp;
        }
        let sl = stage.to_lowercase();
        if stage != "reverted" && sl != top.owner && Some(sl.as_str()) != top.owner.split(':').next() {
            fp.refused = format!(
                "{base}.opfor-last names {stage} as the last writer, but the newest backup is {}'s",
                top.owner
            );
            return fp;
        }
    }
    if let Some(sm) = applied.get("stockmp") {
        for l in &layers {
            if l.owner.starts_with("stockmp:") && *sm != l.value_sha {
                fp.refused = format!(
                    "stock-MP's -applied record names sha {}, but its layer wrote {}: the file was changed after \
                     stock-MP installed it (a NavKit restore over it?) - put stock-MP's file back or delete its \
                     markers deliberately",
                    sm.get(..12).unwrap_or(sm),
                    l.value_sha.get(..12).unwrap_or(&l.value_sha)
                );
                return fp;
            }
        }
    }
    for (st, s) in &applied {
        if !layers.iter().any(|l| l.value_sha == *s) && *s != now {
            fp.notes.push(format!(
                "{st}'s -applied record (sha {}) names no recorded state (informational)",
                s.get(..12).unwrap_or(s)
            ));
        }
    }
    fp.layers = layers;
    fp
}

/// `migrate.plan`: every file with old markers, planned (nothing written).
///
/// # Errors
/// A damaged ledger record.
pub fn plan(game: &Path) -> Result<Vec<FilePlan>, ModError> {
    let recs = Ledger::new(game).records().map_err(from_ledger)?;
    Ok(scan(game)
        .iter()
        .map(|(rel, ms)| plan_file(game, rel, ms, &recs))
        .collect())
}

/// `migrate.show`: the plan's lines.
#[must_use]
pub fn show(fps: &[FilePlan]) -> Vec<String> {
    let mut out = Vec::new();
    for fp in fps {
        if !fp.refused.is_empty() {
            out.push(format!("REFUSED {}: {}", fp.rel, fp.refused));
        } else if !fp.layers.is_empty() {
            let chain: Vec<&str> = fp.layers.iter().map(|l| l.owner.as_str()).collect();
            let orig = fp
                .layers
                .first()
                .and_then(|l| l.prior_sha.clone())
                .unwrap_or_else(|| "none: no file".to_owned());
            out.push(format!(
                "{} {}: original (sha {}) -> {}",
                if fp.done { "recorded" } else { "migrate " },
                fp.rel,
                orig.get(..12).unwrap_or(&orig),
                chain.join(" -> ")
            ));
        } else {
            out.push(format!("archive  {}: {}", fp.rel, fp.markers.join(", ")));
        }
        out.extend(fp.notes.iter().map(|n| format!("           note: {n}")));
    }
    out
}

fn size(p: &Path) -> u64 {
    fs::metadata(p).map_or(0, |m| m.len())
}

/// `migrate.apply`: every file that passes the checks migrated; (files migrated, files refused) and the lines. A
/// crash hook / `BC2_MODS_CRASH_AT` counts the ledger's steps and each marker moved.
///
/// # Errors
/// The ledger is locked or damaged; a write failed (the ledger's commit rolled back; run again).
pub fn apply(mgr: &super::Manager) -> Result<(usize, usize, Vec<String>), ModError> {
    mgr.locked(|lines| apply_locked(mgr, lines))
        .map(|((m, r), l)| (m, r, l))
}

#[allow(clippy::too_many_lines, reason = "migrate.apply's steps, in its order")]
fn apply_locked(mgr: &super::Manager, lines: &mut Vec<String>) -> Result<(usize, usize), ModError> {
    let game = &mgr.game;
    let mut led = mgr.ledger();
    if led.lock_present() || led.interrupted() {
        if let Some(m) = led.live_lock() {
            return Err(ModError::Locked(m));
        }
        for d in led.recover(false).map_err(from_ledger)? {
            lines.push(format!("  resumed: {d}"));
        }
    }
    let recs = led.records().map_err(from_ledger)?;
    let fps: Vec<FilePlan> = scan(game)
        .iter()
        .map(|(rel, ms)| plan_file(game, rel, ms, &recs))
        .collect();
    let mut new: BTreeMap<String, Value> = BTreeMap::new();
    let mut blobs: Vec<(String, PathBuf)> = Vec::new();
    let txn = || {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        format!("migrate-{n}")
    };
    for fp in &fps {
        if !fp.refused.is_empty() || fp.done || fp.layers.is_empty() {
            continue;
        }
        let path = fp.rel.split('/').fold(game.clone(), |p, x| p.join(x));
        let rel = norm_rel(&fp.rel).map_err(from_ledger)?;
        let mut below = Value::Null;
        for (i, l) in fp.layers.iter().enumerate() {
            let mut prior = Value::Null;
            if let (Some(b), Some(sha)) = (&l.backup, &l.prior_sha) {
                blobs.push((sha.clone(), b.clone()));
                prior = json!({ "sha": sha, "size": size(b), "blob": sha });
            }
            let sz = match fp.layers.get(i + 1) {
                None => size(&path),
                Some(n) => n.backup.as_deref().map_or(0, size),
            };
            let derived = !INDEPENDENT.iter().any(|p| l.owner.starts_with(p));
            let c = json!({ "kind": "file", "file": rel, "slot": "", "value": { "sha": l.value_sha, "size": sz },
                            "prior": prior, "below": below, "stack": true,
                            "extra": { "derived": derived, "migrated": true } });
            let r = new.entry(l.owner.clone()).or_insert_with(|| {
                recs.get(&l.owner).cloned().unwrap_or_else(|| {
                    json!({ "format": FORMAT, "owner": l.owner, "txn": txn(), "installed": now_utc(),
                            "note": NOTE, "contributions": [] })
                })
            });
            if let Some(a) = r.get_mut("contributions").and_then(Value::as_array_mut) {
                a.push(c);
            }
            below = json!(l.owner);
        }
    }
    if !new.is_empty() || !blobs.is_empty() {
        led.write_migrated(&new, &blobs).map_err(from_ledger)?;
    }
    let (mut moved, mut refused) = (0, 0);
    for fp in &fps {
        if !fp.refused.is_empty() {
            refused += 1;
            lines.push(format!("REFUSED {}: {}", fp.rel, fp.refused));
            continue;
        }
        let path = fp.rel.split('/').fold(game.clone(), |p, x| p.join(x));
        let folder = path.parent().map_or_default(Path::to_path_buf);
        let parent = Path::new(&fp.rel)
            .parent()
            .map_or_default(|p| p.to_string_lossy().replace('\\', "/"));
        let dest = format!("{ARCHIVE}/{parent}")
            .trim_end_matches('/')
            .split('/')
            .fold(game.clone(), |p, x| p.join(x));
        fs::create_dir_all(&dest).map_err(|e| ioe(&dest, &e))?;
        for m in &fp.markers {
            let src = folder.join(m);
            if !src.exists() {
                continue;
            }
            let mut to = dest.join(m);
            if to.exists() {
                to = dest.join(format!("{m}.{}", txn()));
            }
            mgr.step(&format!("archive {}", src.display()))?;
            fs::rename(&src, &to).map_err(|e| ioe(&src, &e))?;
        }
        moved += 1;
        let chain: Vec<&str> = fp.layers.iter().map(|l| l.owner.as_str()).collect();
        lines.push(format!(
            "migrated {}: {}",
            fp.rel,
            if chain.is_empty() {
                "no backup (markers archived)".to_owned()
            } else {
                chain.join(" -> ")
            }
        ));
    }
    Ok((moved, refused))
}

fn now_utc() -> String {
    // the ledger's own clock format (YYYY-MM-DDTHH:MM:SSZ), from its records' writer
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let days = i64::try_from(secs / 86_400).unwrap_or(0);
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem / 60 % 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_as_the_stages_name_them() {
        assert_eq!(
            owner_for("Package/levels/ons_mp_002/level-00.fbrb", Some("cull")).unwrap_or_default(),
            "cull:ons_mp_002"
        );
        assert_eq!(
            owner_for("Dist/win32/package.mft", Some("loadset")).unwrap_or_default(),
            "loadset"
        );
        assert_eq!(
            owner_for("Package/async/startup-00.fbrb", Some("loading-ons_mp_004")).unwrap_or_default(),
            "loading:ons_mp_004"
        );
        assert_eq!(owner_for("Dist/win32/package.mft", None).unwrap_or_default(), "import");
        assert_eq!(
            owner_for("Output/win32/ai/levels/mp_005/infantry/pathdata.hpd", None).unwrap_or_default(),
            "navkit:mp_005"
        );
        assert!(BACKUP.is_match(".opfor-minimap-edges-orig"));
        assert!(BACKUP.is_match(".opfor-orig"));
        assert!(!BACKUP.is_match(".opfor-last"));
    }
}
