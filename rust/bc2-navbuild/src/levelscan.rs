//! NavKit's scan of a prepared level's converted blueprints (`bfbc2-auto-navkit/tools/levelobjects.py`, round E4e):
//! `blueprint_flags`, `mesh_aliases` and `havok_index` each walked the level's and the common bundles' folders with
//! `glob('**/...')` and read every `.xml` file (`navkit.py prepare`: 33,828 opens, 600 of `sp_009`'s 887 s). Here one
//! walk finds the files in `glob`'s order and every `.xml` file is read once, on all cores; the Python makes its
//! dicts from the records, so its string handling (paths made relative and lower case) stays the reference's.
//!
//! **The same records.** `glob.glob(os.path.join(base, '**', pattern), recursive=True)`: the folders in pre-order
//! (the base, then each sub-folder as listed, followed by its own sub-folders), names starting with `.` skipped,
//! each folder's matching files in listing order (`os.scandir` and `std::fs::read_dir` both list with
//! `FindFirstFileW` / `FindNextFileW` on Windows), the pattern matched without regard to case (`fnmatch` on Windows).
//! The text as `open(f, encoding='utf-8', errors='replace').read()` reads it (invalid bytes replaced, line endings
//! made `\n`); the tests on it as the Python writes them, `xmlinst.field_ref` as its regular expressions match.

use std::fs;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

/// What the scan found in one `.xml` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XmlRecord {
    /// The file's path relative to its base (as found, `\`-separated on Windows).
    pub rel: String,
    /// `blueprint_flags`' entry: `(ForbidPathfinding, ExcludeFromPathfinding, AllowPathdataSeeds, dynamic,
    /// DisablePathfinding)` - `None` where the file has neither `ForbidPathfinding` nor the dynamic model type.
    pub flags: Option<(bool, bool, bool, bool, String)>,
    /// `xmlinst.field_ref(text, 'Mesh')`: the first `Mesh` field's `ref`, if any.
    pub mesh: Option<String>,
}

fn hidden(name: &str) -> bool {
    name.starts_with('.')
}

fn sorted_entries(dir: &Path) -> Vec<(String, PathBuf, bool)> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    rd.filter_map(Result::ok)
        .filter_map(|e| {
            let name = e.file_name().to_str()?.to_owned();
            // is_dir as os.DirEntry.is_dir(): from the listing, following a link (a junction to a folder is a folder)
            let ft = e.file_type().ok()?;
            let is_dir = if ft.is_symlink() {
                fs::metadata(e.path()).is_ok_and(|m| m.is_dir())
            } else {
                ft.is_dir()
            };
            Some((name, e.path(), is_dir))
        })
        .collect()
}

/// Every folder under `dir` as glob's `**` lists them (pre-order, hidden names skipped), each with its listing.
type Folder = (String, Vec<(String, PathBuf, bool)>);

fn walk(dir: &Path, rel: &str, folders: &mut Vec<Folder>) {
    let entries = sorted_entries(dir);
    let subs: Vec<(String, PathBuf)> = entries
        .iter()
        .filter(|(n, _, d)| *d && !hidden(n))
        .map(|(n, p, _)| (n.clone(), p.clone()))
        .collect();
    folders.push((rel.to_owned(), entries));
    for (name, path) in subs {
        let sub = if rel.is_empty() {
            name
        } else {
            format!("{rel}{}{name}", std::path::MAIN_SEPARATOR)
        };
        walk(&path, &sub, folders);
    }
}

fn folders_of(base: &Path) -> Vec<Folder> {
    let mut folders = Vec::new();
    if base.is_dir() {
        walk(base, "", &mut folders);
    }
    folders
}

/// The files of `folders` matching `'*' + suffix`, relative to the base, in glob's order.
fn matching(folders: &[Folder], suffix: &str) -> Vec<(String, PathBuf)> {
    let suffix = suffix.to_lowercase();
    let mut out = Vec::new();
    for (rel, entries) in folders {
        for (name, path, _is_dir) in entries {
            // '*' + suffix: no leading '.', the suffix without regard to case (a folder matching it counts too, as
            // glob's does - none does)
            if hidden(name) || !name.to_lowercase().ends_with(&suffix) {
                continue;
            }
            let r = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}{}{name}", std::path::MAIN_SEPARATOR)
            };
            out.push((r, path.clone()));
        }
    }
    out
}

/// `glob.glob(os.path.join(base, '**', '*' + suffix), recursive=True)` as paths relative to `base`, in glob's order.
#[must_use]
pub fn glob_files(base: &Path, suffix: &str) -> Vec<(String, PathBuf)> {
    matching(&folders_of(base), suffix)
}

/// The text as Python's `open(path, encoding='utf-8', errors='replace').read()` gives it.
fn read_text(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    let s = String::from_utf8_lossy(&bytes);
    // universal newlines: \r\n and a lone \r become \n
    Some(s.replace("\r\n", "\n").replace('\r', "\n"))
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `re.findall(r'(\w+)="([^"]*)"', s)` as a dict (a later name wins).
fn attrs(s: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut k = 0;
    while k < chars.len() {
        // the longest run of word characters starting here followed by ="
        let Some(&(start, c)) = chars.get(k) else { break };
        if !is_word(c) {
            k += 1;
            continue;
        }
        let mut e = k;
        while chars.get(e).is_some_and(|&(_, c)| is_word(c)) {
            e += 1;
        }
        let name_end = chars.get(e).map_or(s.len(), |&(i, _)| i);
        let rest = s.get(name_end..).unwrap_or("");
        if let Some(v) = rest.strip_prefix("=\"")
            && let Some(close) = v.find('"')
        {
            let name = s.get(start..name_end).unwrap_or("").to_owned();
            let value = v.get(..close).unwrap_or("").to_owned();
            let next = name_end + 2 + close + 1;
            if let Some(slot) = out.iter_mut().find(|(n, _)| *n == name) {
                slot.1 = value;
            } else {
                out.push((name, value));
            }
            k = chars.partition_point(|&(i, _)| i < next);
            continue;
        }
        // no match from this run's start: the regex tries the next position (a shorter suffix of the run cannot
        // be followed by =" either, so skip the run)
        k = e.max(k + 1);
    }
    out
}

/// `xmlinst.field_ref(body, name)`: the ref of the first `<field ...>` named `name` that has one.
#[must_use]
pub fn field_ref(body: &str, name: &str) -> Option<String> {
    let mut at = 0;
    while let Some(p) = body.get(at..).and_then(|b| b.find("<field")) {
        let s = at + p + "<field".len();
        at = s;
        // \b: the next character is not a word character (or the text ends)
        if body.get(s..).and_then(|r| r.chars().next()).is_some_and(is_word) {
            continue;
        }
        // ([^>]*?)/?> : up to the first '>', a '/' before it left out
        let Some(gt) = body.get(s..).and_then(|r| r.find('>')) else {
            break;
        };
        let mut inner = body.get(s..s + gt).unwrap_or("");
        at = s + gt + 1;
        if let Some(i) = inner.strip_suffix('/') {
            inner = i;
        }
        let a = attrs(inner);
        let get = |k: &str| a.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone());
        if get("name").as_deref() == Some(name)
            && let Some(r) = get("ref")
        {
            return Some(r);
        }
    }
    None
}

/// `blueprint_flags`' and `mesh_aliases`' reading of one file's text.
#[must_use]
pub fn scan_text(rel: String, t: &str) -> XmlRecord {
    let dynamic = t.contains("type=\"GameSharedResources.DynamicModelEntityData\"");
    let flags = (t.contains("ForbidPathfinding") || dynamic).then(|| {
        let key = "<field name=\"DisablePathfinding\">";
        let mut dis = String::new();
        let mut at = 0;
        while let Some(p) = t.get(at..).and_then(|r| r.find(key)) {
            let v = at + p + key.len();
            let letters: String = t
                .get(v..)
                .unwrap_or("")
                .chars()
                .take_while(char::is_ascii_alphabetic)
                .collect();
            if t.get(v + letters.len()..).is_some_and(|r| r.starts_with('<')) {
                dis = letters;
                break;
            }
            at = v;
        }
        (
            t.contains("<field name=\"ForbidPathfinding\">true"),
            t.contains("<field name=\"ExcludeFromPathfinding\">true"),
            t.contains("<field name=\"AllowPathdataSeeds\">true"),
            dynamic,
            dis,
        )
    });
    XmlRecord {
        rel,
        flags,
        mesh: field_ref(t, "Mesh"),
    }
}

/// Every `.xml` file under each base (glob's order), read once on all cores: the records of those with flags or a
/// `Mesh` reference; and every `*_havok.res` file. Per base: (xml records, havok files relative to the base);
/// `None`: a file that cannot be read (the Python raises there).
#[must_use]
pub fn scan(bases: &[PathBuf]) -> Option<Vec<(Vec<XmlRecord>, Vec<String>)>> {
    bases
        .iter()
        .map(|base| {
            let folders = folders_of(base);
            let xml = matching(&folders, ".xml");
            let records: Vec<Option<XmlRecord>> = xml
                .into_par_iter()
                .map(|(rel, path)| {
                    let t = read_text(&path)?;
                    Some(scan_text(rel, &t))
                })
                .collect();
            let records: Vec<XmlRecord> = records
                .into_iter()
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .filter(|r| r.flags.is_some() || r.mesh.is_some())
                .collect();
            let havok = matching(&folders, "_havok.res").into_iter().map(|(r, _)| r).collect();
            Some((records, havok))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing, reason = "tests of small known texts")]
mod tests {
    use super::*;

    #[test]
    fn field_refs_and_flags() {
        let t = "<instance><field name=\"Mesh\" ref=\"a/b_mesh/x\" /><field ref=\"c\" name=\"Mesh\"/></instance>";
        assert_eq!(field_ref(t, "Mesh").as_deref(), Some("a/b_mesh/x"));
        let t = "<fields name=\"Mesh\" ref=\"no\"/><field ref=\"yes\" name=\"Mesh\">";
        assert_eq!(field_ref(t, "Mesh").as_deref(), Some("yes"));
        assert_eq!(field_ref("<field name=\"Mesh\">x</field>", "Mesh"), None);
        let r = scan_text(
            "a.xml".into(),
            "<field name=\"ForbidPathfinding\">true</field><field name=\"DisablePathfinding\">PdtWhenAlive</field>",
        );
        assert_eq!(r.flags, Some((true, false, false, false, "PdtWhenAlive".into())));
        assert_eq!(scan_text("b.xml".into(), "<field name=\"Other\">1</field>").flags, None);
    }
}
