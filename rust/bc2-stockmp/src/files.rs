//! The two folder readings of `validate.py`: the index of an unpacked bundle's partitions (`_folder_index`) and the
//! packed bundle against the built folder (`check_packed`), on all cores.
//!
//! Both follow `os.walk` and Windows' path rules (`ntpath.join`, `relpath`, `normcase`, `normpath`) for the names
//! these folders hold; a name those rules would change (a trailing dot or space, `.` / `..` parts, not ASCII), a
//! link or junction, or any file that cannot be read is refused, and the check then runs in Python.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use rayon::prelude::*;

use crate::{R, Refuse};

/// `ntpath.join(a, b)` for a plain name `b`.
fn join(a: &str, b: &str) -> String {
    if a.is_empty() || a.ends_with('\\') || a.ends_with('/') || (a.len() == 2 && a.ends_with(':')) {
        format!("{a}{b}")
    } else {
        format!("{a}\\{b}")
    }
}

/// A file or folder name Windows' path rules keep as it is (ASCII, no trailing dot or space, not `.` or `..`).
fn plain(name: &str) -> bool {
    name.is_ascii() && !name.is_empty() && !name.ends_with('.') && !name.ends_with(' ') && name != "." && name != ".."
}

/// Every file under `top` as `(path as os.walk joins it, the path's parts below top)`, in `os.walk`'s order.
fn walk(top: &str) -> R<Vec<(String, Vec<String>)>> {
    let mut out = Vec::new();
    let Ok(meta) = fs::symlink_metadata(top) else {
        return Ok(out); // os.walk of a missing folder yields nothing
    };
    if !meta.is_dir() {
        return if meta.file_type().is_symlink() {
            Err(Refuse("a link"))
        } else {
            Ok(out)
        };
    }
    // pre-order, each folder's files before its subfolders, subfolders in listing order
    let mut stack: Vec<(String, Vec<String>)> = vec![(top.to_owned(), Vec::new())];
    while let Some((dir, parts)) = stack.pop() {
        let rd = fs::read_dir(&dir).map_err(|_| Refuse("a folder not listed"))?;
        let mut dirs = Vec::new();
        for entry in rd {
            let entry = entry.map_err(|_| Refuse("a folder not listed"))?;
            let ft = entry.file_type().map_err(|_| Refuse("a file type"))?;
            if ft.is_symlink() {
                return Err(Refuse("a link"));
            }
            let name = entry
                .file_name()
                .into_string()
                .map_err(|_| Refuse("a name not UTF-8"))?;
            if !plain(&name) {
                return Err(Refuse("a name the path rules change"));
            }
            let mut p = parts.clone();
            p.push(name.clone());
            if ft.is_dir() {
                dirs.push((join(&dir, &name), p));
            } else {
                out.push((join(&dir, &name), p));
            }
        }
        stack.extend(dirs.into_iter().rev());
    }
    Ok(out)
}

/// `validate._folder_index(top)`: lower-case partition path (no `.dbx`, `/` between parts) -> file path, over every
/// `.dbx` under `top` (the first file of a path kept; none of a case-insensitive folder has two).
///
/// # Errors
/// A name, link or folder the module refuses.
pub fn folder_index(top: &str) -> R<Vec<(String, String)>> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out = Vec::new();
    for (path, parts) in walk(top)? {
        let file = parts.last().map_or_default(String::as_str);
        if !file.to_ascii_lowercase().ends_with(".dbx") {
            continue;
        }
        let rel = parts.join("/");
        let key = rel.get(..rel.len() - 4).unwrap_or_default().to_ascii_lowercase();
        if seen.insert(key.clone()) {
            out.push((key, path));
        } else {
            return Err(Refuse("two files of one partition path"));
        }
    }
    Ok(out)
}

/// `validate.check_packed`'s reading: `(files compared, files that differ)` - every file of `build_dir` but its
/// `fbrb.manifest` against the bundle's live entry of the same path (Windows compares names without case).
///
/// # Errors
/// A bundle or file that cannot be read, or a name the module refuses.
pub fn packed_check(bundle: &str, build_dir: &str) -> R<(usize, usize)> {
    let header = bc2_formats::fbrb::entries_path(Path::new(bundle)).map_err(|_| Refuse("the bundle is not read"))?;
    let live: Vec<&str> = header
        .entries
        .iter()
        .filter(|e| !e.is_deleted())
        .map(|e| e.name.as_str())
        .collect();
    let mut keys = Vec::with_capacity(live.len());
    for n in &live {
        let p = n.replace('/', "\\");
        if !p.split('\\').all(plain) || p.contains(':') {
            return Err(Refuse("an entry name normpath changes"));
        }
        keys.push(p.to_ascii_lowercase());
    }
    let lowered: Vec<String> = live.iter().map(|n| n.to_lowercase()).collect();
    let data =
        bc2_formats::fbrb::read_path(Path::new(bundle), &lowered).map_err(|_| Refuse("the bundle is not read"))?;
    let mut files: HashMap<&str, &[u8]> = HashMap::with_capacity(keys.len());
    for (k, d) in keys.iter().zip(&data) {
        if files.insert(k.as_str(), d.as_slice()).is_some() {
            return Err(Refuse("two entries of one path"));
        }
    }
    let on_disk: Vec<(String, Vec<String>)> = walk(build_dir)?
        .into_iter()
        .filter(|(_, parts)| !(parts.len() == 1 && parts.first().map(String::as_str) == Some("fbrb.manifest")))
        .collect();
    let results: Vec<R<bool>> = on_disk
        .par_iter()
        .map(|(path, parts)| {
            let key = parts.join("\\").to_ascii_lowercase();
            let Some(want) = files.get(key.as_str()) else {
                return Ok(false);
            };
            let got = fs::read(path).map_err(|_| Refuse("a file not read"))?;
            Ok(got == *want)
        })
        .collect();
    let mut bad = 0;
    for r in results {
        bad += usize::from(!r?);
    }
    Ok((on_disk.len(), bad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_as_ntpath() {
        assert_eq!(join("C:\\a", "b"), "C:\\a\\b");
        assert_eq!(join("C:\\a\\", "b"), "C:\\a\\b");
        assert_eq!(join("C:", "b"), "C:b");
        assert!(plain("x.dbx") && !plain("x.") && !plain("..") && !plain("\u{e9}"));
    }

    #[test]
    fn index_and_compare() {
        let dir = std::env::temp_dir().join(format!("bc2_stockmp_files_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(dir.join("Sub")).ok();
        fs::write(dir.join("Sub").join("A.dbx"), b"a").ok();
        fs::write(dir.join("b.txt"), b"b").ok();
        fs::write(dir.join("fbrb.manifest"), b"m").ok();
        let top = dir.to_string_lossy().into_owned();
        let idx = folder_index(&top).ok();
        assert_eq!(
            idx.map(|v| v.into_iter().map(|(k, _)| k).collect::<Vec<_>>()),
            Some(vec!["sub/a".to_owned()])
        );
        let bundle = bc2_formats::fbrb::build(&[("Sub/A.dbx", "<non-resource>", b"a"), ("b.txt", "T", b"x")], true);
        let bp = dir.with_extension("fbrb");
        fs::write(&bp, bundle).ok();
        assert_eq!(packed_check(&bp.to_string_lossy(), &top), Ok((2, 1)));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_file(&bp);
        assert_eq!(folder_index("Z:\\no such folder here"), Ok(Vec::new()));
    }
}
