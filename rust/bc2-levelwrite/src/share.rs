//! A map project made ready to share (round OR15, IN1: portable and anonymised project sharing): a copy of the
//! project in a folder of its own that names no PC, no user and no place on this PC, and opens on another PC as it is.
//!
//! What goes into the copy: `<name>.mapedit.json` with
//! - every operation's `by` (the PC's name, `author()`) as `author 1`, `author 2`, ... in the order they first made
//!   an edit (the history still tells the authors apart, not who they are);
//! - every string that names a place on a PC (`C:\...`, `\\server\...`, `/home/...`: a kept file's `source`, any
//!   other path a field holds) as its last part, the file's name;
//! - the PC's and the user's names wherever else they appear, replaced ([`bc2_gamefs::privacy::anonymise`]);
//!
//! and the files the project keeps beside itself: the kept files (an imported height map, a colour map) and the
//! brush strokes' tiles (`<project>.tiles\`), byte for byte (the user's own data, never game content). Left out:
//! the autosave journal and the lock (they name the PC). After writing, the copy's JSON is read back and refused in
//! words if it still holds an absolute path or a name ([`bc2_gamefs::privacy::leaks`]); the copy loads as a project
//! ([`crate::project::MapProject::from_json`]).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use bc2_gamefs::privacy::{anonymise, is_absolute_path, leaks};
use serde_json::Value;

use crate::project::{MapProject, write_bytes};

/// What a share wrote: the copy's project file, the files copied beside it, what was changed (in words).
#[derive(Debug, Clone, Default)]
pub struct Shared {
    /// The copy's project file.
    pub project: PathBuf,
    /// The files copied beside it (relative to the folder).
    pub files: Vec<String>,
    /// The changes made for privacy, in words (`3 operations' authors`, ...).
    pub changes: Vec<String>,
}

fn last_part(s: &str) -> String {
    s.rsplit(['\\', '/']).find(|p| !p.is_empty()).unwrap_or("").to_owned()
}

/// The JSON with authors numbered, paths cut to their file names and names replaced; counts of each into `n`.
fn clean(v: &mut Value, authors: &mut BTreeMap<String, String>, n: &mut [usize; 3]) {
    match v {
        Value::Object(m) => {
            for (k, x) in m.iter_mut() {
                if k == "by"
                    && let Value::String(s) = x
                {
                    let next = authors.len() + 1;
                    let a = authors
                        .entry(s.clone())
                        .or_insert_with(|| format!("author {next}"))
                        .clone();
                    if *s != a {
                        n[0] += 1;
                    }
                    *s = a;
                    continue;
                }
                clean(x, authors, n);
            }
        }
        Value::Array(a) => {
            for x in a {
                clean(x, authors, n);
            }
        }
        Value::String(s) => {
            if is_absolute_path(s) {
                *s = last_part(s);
                n[1] += 1;
            } else {
                let a = anonymise(s);
                if a != *s {
                    *s = a;
                    n[2] += 1;
                }
            }
        }
        _ => {}
    }
}

fn copy_tree(src: &Path, dst: &Path, rel: &str, out: &mut Vec<String>) -> Result<(), String> {
    let rd = fs::read_dir(src).map_err(|e| format!("{}: {e}", src.display()))?;
    let mut entries: Vec<_> = rd.flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for e in entries {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = e.path();
        let r = format!("{rel}/{name}");
        if p.is_dir() {
            copy_tree(&p, &dst.join(&name), &r, out)?;
        } else {
            let data = fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            write_bytes(&dst.join(&name), &data).map_err(|e| format!("{}: {e}", dst.join(&name).display()))?;
            out.push(r);
        }
    }
    Ok(())
}

/// The project at `project` copied, anonymised, into the folder `out` (made; refused when it holds anything).
///
/// # Errors
/// The project does not read, `out` is not empty, a file cannot be copied, or the copy still names a place or a
/// person (in words; the copy is then removed).
pub fn share(project: &Path, out: &Path) -> Result<Shared, String> {
    let text = fs::read_to_string(project).map_err(|e| format!("{}: {e}", project.display()))?;
    MapProject::from_json(&text).map_err(|e| format!("{}: {e}", project.display()))?;
    if out.is_dir() && fs::read_dir(out).map_err(|e| e.to_string())?.next().is_some() {
        return Err(format!("{}: not empty - share into a new folder", out.display()));
    }
    let mut v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let mut authors = BTreeMap::new();
    let mut n = [0usize; 3];
    clean(&mut v, &mut authors, &mut n);
    let name = project
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let dst = out.join(&name);
    let mut body = serde_json::to_string_pretty(&v).map_err(|e| e.to_string())?;
    body.push('\n');
    let left = leaks(&body);
    let paths = {
        let mut p = Vec::new();
        find_paths(&v, &mut p);
        p
    };
    if !left.is_empty() || !paths.is_empty() {
        return Err(format!(
            "{}: the shared copy would still hold {} - not written",
            project.display(),
            left.into_iter()
                .chain(paths.into_iter().map(|p| format!("the path {p:?}")))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    MapProject::from_json(&body).map_err(|e| format!("the shared copy does not load as a project: {e}"))?;
    write_bytes(&dst, body.as_bytes()).map_err(|e| format!("{}: {e}", dst.display()))?;
    let mut files = Vec::new();
    let here = project.parent().unwrap_or_else(|| Path::new("."));
    // the kept files: every "file" a kept-file object names (a height map, a colour map), beside the project
    let mut kept = Vec::new();
    kept_files(&v, &mut kept);
    for f in kept {
        let src = here.join(&f);
        if src.is_file() {
            let data = fs::read(&src).map_err(|e| format!("{}: {e}", src.display()))?;
            write_bytes(&out.join(&f), &data).map_err(|e| e.to_string())?;
            files.push(f);
        }
    }
    let tiles = PathBuf::from(format!("{}.tiles", project.display()));
    if tiles.is_dir() {
        copy_tree(
            &tiles,
            &out.join(format!("{name}.tiles")),
            &format!("{name}.tiles"),
            &mut files,
        )?;
    }
    let mut changes = Vec::new();
    if n[0] > 0 {
        changes.push(format!(
            "{} operations' authors numbered ({} authors)",
            n[0],
            authors.len()
        ));
    }
    if n[1] > 0 {
        changes.push(format!("{} paths cut to their file names", n[1]));
    }
    if n[2] > 0 {
        changes.push(format!("{} texts with a name of this PC or its user replaced", n[2]));
    }
    changes.push("the autosave journal and the lock left out".to_owned());
    Ok(Shared {
        project: dst,
        files,
        changes,
    })
}

fn find_paths(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => m.values().for_each(|x| find_paths(x, out)),
        Value::Array(a) => a.iter().for_each(|x| find_paths(x, out)),
        Value::String(s) if is_absolute_path(s) => out.push(s.clone()),
        _ => {}
    }
}

/// The `file` of every object that also has `source` and `sha1` (a [`crate::project::KeptFile`]).
fn kept_files(v: &Value, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            if let (Some(Value::String(f)), true, true) =
                (m.get("file"), m.contains_key("source"), m.contains_key("sha1"))
                && !f.contains(['\\', '/'])
                && !f.starts_with('.')
                && !out.contains(f)
            {
                out.push(f.clone());
            }
            m.values().for_each(|x| kept_files(x, out));
        }
        Value::Array(a) => a.iter().for_each(|x| kept_files(x, out)),
        _ => {}
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn authors_are_numbered_and_paths_cut() {
        let mut v = json!({ "ops": [{ "by": "DESK-A", "n": 1 }, { "by": "DESK-B" }, { "by": "DESK-A" }],
                            "kept": { "file": "height.png", "source": r"C:\Users\x\Pictures\height.png", "sha1": "a" },
                            "note": "levels/mp_005" });
        let (mut a, mut n) = (BTreeMap::new(), [0; 3]);
        clean(&mut v, &mut a, &mut n);
        assert_eq!(v["ops"][0]["by"], "author 1");
        assert_eq!(v["ops"][1]["by"], "author 2");
        assert_eq!(v["ops"][2]["by"], "author 1");
        assert_eq!(v["kept"]["source"], "height.png");
        assert_eq!(v["note"], "levels/mp_005");
        assert_eq!(n[..2], [3, 1]);
        let mut k = Vec::new();
        kept_files(&v, &mut k);
        assert_eq!(k, ["height.png"]);
    }
}
