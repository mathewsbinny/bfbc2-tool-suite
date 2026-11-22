//! The staged plan (`work\ww2\staged.json`): what a good `--plan` staged, so `--apply-staged` installs it without
//! building it again while nothing it depends on changed (Project OPFOR's `ww2_level_install.save_staged` /
//! `load_staged` / `fingerprint`, QA C12).
//!
//! The record holds the plan itself, every staged file's sha256 and the input fingerprint: a digest over every file of
//! `work\ww2` but the staging's own outputs (by path, size and modification time), the code files the plan ran
//! (missing: -1) and the game copy's bundles (`.fbrb`). The fingerprint is Python's, line for line.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use bc2_formats::sha256::{self, Sha256};
use rayon::prelude::*;
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::plan::Plan;

/// The record's format tag.
pub const FORMAT: &str = "bfbc2-install-staged/1";

/// A staged file and its sha256.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct File {
    /// The staged file.
    pub path: String,
    /// Its sha256.
    pub sha: String,
}

/// The staged record.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Record {
    /// [`FORMAT`].
    pub format: String,
    /// The input fingerprint ([`fingerprint`]).
    pub fingerprint: String,
    /// Every staged file the install copies (the levels', `ww2_common`'s, the shared patches').
    pub files: Vec<File>,
    /// The plan.
    pub plan: Plan,
}

fn mtime_ns(m: &fs::Metadata) -> i128 {
    m.modified().ok().map_or(0, |t| match t.duration_since(UNIX_EPOCH) {
        Ok(d) => i128::try_from(d.as_nanos()).unwrap_or(0),
        Err(e) => -i128::try_from(e.duration().as_nanos()).unwrap_or(0),
    })
}

/// `os.walk` top-down of root: per folder its files (sorted) hashed as `<relative path, lower case>|<size>|<mtime
/// ns>`, then its folders (sorted). `skip_top` names left out at the top; `keep` the file names hashed.
fn walk(h: &mut Sha256, root: &Path, keep: &dyn Fn(&str) -> bool, skip_top: Option<&Regex>) {
    let mut stack: Vec<PathBuf> = vec![root.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        let mut dirs: Vec<String> = Vec::new();
        let mut files: Vec<String> = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if d == root && skip_top.is_some_and(|r| r.is_match(&name)) {
                continue;
            }
            if e.path().is_dir() {
                dirs.push(name);
            } else {
                files.push(name);
            }
        }
        files.sort();
        for f in &files {
            if !keep(f) {
                continue;
            }
            let p = d.join(f);
            let Ok(m) = fs::metadata(&p) else { continue };
            let rel = p.strip_prefix(root).map_or_else(|_| p.clone(), Path::to_path_buf);
            h.update(
                format!(
                    "{}|{}|{}\n",
                    rel.to_string_lossy().to_lowercase(),
                    m.len(),
                    mtime_ns(&m)
                )
                .as_bytes(),
            );
        }
        dirs.sort();
        // depth first, in the sorted order: pushed last to first
        for x in dirs.iter().rev() {
            stack.push(d.join(x));
        }
    }
}

/// The input fingerprint of a plan (`ww2_level_install.fingerprint`).
///
/// # Errors
/// The `not_input` pattern does not compile.
pub fn fingerprint(plan: &Plan) -> Result<String, String> {
    let skip = Regex::new(&format!("(?i)^(?:{})$", plan.staged.not_input)).map_err(|e| format!("not_input: {e}"))?;
    let mut h = Sha256::default();
    walk(&mut h, Path::new(&plan.staged.w), &|_| true, Some(&skip));
    h.update(b"--code--\n");
    for p in &plan.staged.code {
        let m = fs::metadata(p).ok().filter(fs::Metadata::is_file);
        let (size, time) = m.map_or((-1, -1), |m| (i128::from(m.len()), mtime_ns(&m)));
        h.update(format!("{}|{size}|{time}\n", p.to_lowercase()).as_bytes());
    }
    h.update(b"--game--\n");
    // the extension as Python compares it: case-sensitive
    #[allow(
        clippy::case_sensitive_file_extension_comparisons,
        reason = "Python's f.endswith('.fbrb')"
    )]
    walk(&mut h, Path::new(&plan.game), &|f| f.ends_with(".fbrb"), None);
    Ok(h.hexdigest())
}

/// The staged files of a plan: the levels' bundles, `ww2_common`'s, the shared patches'.
#[must_use]
pub fn staged_paths(plan: &Plan) -> Vec<String> {
    let mut out: Vec<String> = plan
        .levels
        .iter()
        .flat_map(|l| l.built.iter().map(|(_, p)| p.clone()))
        .collect();
    out.extend(plan.common.built.iter().map(|(_, p)| p.clone()));
    out.extend(plan.shared.iter().map(|s| s.built.clone()));
    out
}

/// The sha256 of each file, on all cores (None: unreadable).
#[must_use]
pub fn digests(paths: &[String]) -> Vec<Option<String>> {
    paths.par_iter().map(|p| sha256::file(p).ok()).collect()
}

/// Writes the staged record (a temporary file, then a rename: a plan cut short leaves no truncated record).
///
/// # Errors
/// A staged file that cannot be read, or the record cannot be written.
pub fn save(plan: &Plan) -> Result<(), String> {
    let paths = staged_paths(plan);
    let mut files = Vec::new();
    for (p, d) in paths.iter().zip(digests(&paths)) {
        files.push(File {
            path: p.clone(),
            sha: d.ok_or_else(|| format!("{p}: cannot be read"))?,
        });
    }
    let rec = Record {
        format: FORMAT.to_owned(),
        fingerprint: fingerprint(plan)?,
        files,
        plan: plan.clone(),
    };
    let text = serde_json::to_string_pretty(&rec).map_err(|e| e.to_string())?;
    let out = Path::new(&plan.staged.path);
    let tmp = out.with_file_name(format!(
        "{}.tmp-{}",
        out.file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        std::process::id()
    ));
    fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
    bc2_formats::atomic::replace(&tmp, out).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        format!("{}: {e}", out.display())
    })
}

/// The last plan's record when nothing it depends on changed since: Ok(Err(why)) when there is none, an input
/// changed, or a staged file is not the one the plan made.
///
/// # Errors
/// Never in practice (reserved for an unreadable pattern in the record).
pub fn load(path: &Path) -> Result<Result<Record, String>, String> {
    if !path.is_file() {
        return Ok(Err(
            "--apply-staged: no staged plan (work\\ww2\\staged.json) - run wake_install.py --plan first".to_owned(),
        ));
    }
    let rec: Record = match fs::read(path)
        .map_err(|e| e.to_string())
        .and_then(|d| serde_json::from_slice(&d).map_err(|e| e.to_string()))
    {
        Ok(r) => r,
        Err(e) => {
            return Ok(Err(format!(
                "--apply-staged: the staged plan cannot be read ({e}) - run wake_install.py --plan again"
            )));
        }
    };
    if rec.format != FORMAT || rec.fingerprint != fingerprint(&rec.plan)? {
        return Ok(Err(
            "--apply-staged: something the plan depends on changed since it ran (a build output, the \
                       tools or the game copy) - run wake_install.py --plan again, or --apply"
                .to_owned(),
        ));
    }
    let paths: Vec<String> = rec.files.iter().map(|f| f.path.clone()).collect();
    let bad: Vec<&String> = rec
        .files
        .iter()
        .zip(digests(&paths))
        .filter(|(f, d)| d.as_deref() != Some(f.sha.as_str()))
        .map(|(f, _)| &f.path)
        .collect();
    if !bad.is_empty() {
        let first: Vec<String> = bad.iter().take(5).map(|p| crate::text::py_repr(p)).collect();
        return Ok(Err(format!(
            "--apply-staged: staged files changed or missing since the plan: [{}] - run the plan again",
            first.join(", ")
        )));
    }
    Ok(Ok(rec))
}
