//! Finding files under a folder as Python's `glob.glob(os.path.join(base, '**', pattern), recursive=True)` finds
//! them, in the same order: the folder's own matches first, then each subfolder's (recursively, depth first), each
//! folder's entries in the order the file system lists them. Names starting with `.` are skipped (glob's hidden
//! files), folders reached through links are followed, and a folder that cannot be listed is skipped. Matching is
//! on the lower-case name (Windows' `fnmatch`), here by suffix.
//!
//! The subfolders are listed in parallel; the result's order does not depend on it.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;

/// A file found: its path relative to the base folder (`/`-separated, lower case), its full path and its size and
/// modification time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Found {
    /// The path relative to the base folder, `/`-separated and lower-case.
    pub rel: String,
    /// The full path (the base joined with the entry names, as `os.path.join` makes it).
    pub path: PathBuf,
    /// Its size and modification time, from the listing (no extra call on Windows); `None` when the system gave none.
    pub stamp: Option<Stamp>,
}

/// A file's size and modification time (nanoseconds since 1970), what the alias cache is keyed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Stamp {
    /// The size in bytes.
    pub size: u64,
    /// The modification time in nanoseconds since 1970-01-01.
    pub modified: u128,
}

impl Stamp {
    /// The stamp of a file's metadata (`None` without a modification time, or one before 1970).
    #[must_use]
    pub fn of(m: &fs::Metadata) -> Option<Self> {
        let modified = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
        Some(Self {
            size: m.len(),
            modified,
        })
    }
}

/// Every entry under `base` whose lower-case name ends with one of `suffixes` (lower case), per suffix in walk
/// order: `result[k]` holds the matches of `suffixes[k]`.
#[must_use]
pub fn find(base: &Path, suffixes: &[&str]) -> Vec<Vec<Found>> {
    let mut out = vec![Vec::new(); suffixes.len()];
    walk(base, "", suffixes, &mut out);
    out
}

fn walk(dir: &Path, rel: &str, suffixes: &[&str], out: &mut [Vec<Found>]) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let lower = name.to_lowercase();
        let child_rel = if rel.is_empty() {
            lower.clone()
        } else {
            format!("{rel}/{lower}")
        };
        for (k, suffix) in suffixes.iter().enumerate() {
            if lower.ends_with(suffix)
                && let Some(list) = out.get_mut(k)
            {
                list.push(Found {
                    rel: child_rel.clone(),
                    path: path.clone(),
                    stamp: entry.metadata().ok().as_ref().and_then(Stamp::of),
                });
            }
        }
        // glob's is_dir(): links are followed
        let is_dir = match entry.file_type() {
            Ok(t) if t.is_symlink() => fs::metadata(&path).is_ok_and(|m| m.is_dir()),
            Ok(t) => t.is_dir(),
            Err(_) => false,
        };
        if is_dir {
            subdirs.push((path, child_rel));
        }
    }
    let below: Vec<Vec<Vec<Found>>> = subdirs
        .par_iter()
        .map(|(path, rel)| {
            let mut found = vec![Vec::new(); suffixes.len()];
            walk(path, rel, suffixes, &mut found);
            found
        })
        .collect();
    for found in below {
        for (list, more) in out.iter_mut().zip(found) {
            list.extend(more);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn walks_in_glob_order() {
        let root = std::env::temp_dir().join(format!("bc2_scene_walk_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        for d in ["b/c", "a", ".hidden"] {
            fs::create_dir_all(root.join(d)).unwrap();
        }
        for f in [
            "x_Havok.res",
            "a/y_havok.RES",
            "b/c/z_havok.res",
            "b/w.xml",
            ".hidden/q_havok.res",
            ".h_havok.res",
        ] {
            fs::write(root.join(f), b"").unwrap();
        }
        let found = find(&root, &["_havok.res", ".xml"]);
        let rels: Vec<&str> = found[0].iter().map(|f| f.rel.as_str()).collect();
        assert_eq!(rels, vec!["x_havok.res", "a/y_havok.res", "b/c/z_havok.res"]);
        assert_eq!(found[1][0].rel, "b/w.xml");
        assert_eq!(found[1][0].path, root.join("b").join("w.xml"));
        assert_eq!(found[1][0].stamp.unwrap().size, 0);
        assert_eq!(find(&root.join("missing"), &[".xml"])[0].len(), 0);
        fs::remove_dir_all(&root).unwrap();
    }
}
