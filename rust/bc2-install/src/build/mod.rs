//! The WW2 level build (round OR2, "OPFOR in Rust" stage 2): Project OPFOR's `wake_install.build` and the parts it
//! calls that are not the shader glue (stage 3) - the inputs (the stock index, our builds' files), the level's own
//! partitions and root files, the closure, the staging folders and their packs, ww2_common and the shared bundles'
//! patches, the description and the menus' level list, and the model-group and icon gates.
//!
//! OPFOR keeps its decisions (the level's bindings, the roots of its builds, the remaps, the donors) and passes them
//! as data; the rules are here. Every function gives what the Python it replaced gave - the same files, the same
//! report lines in the same order - and the WW2 install oracle (`compare_oracle.py --install`) holds the whole.
//!
//! | Project OPFOR (`wake_install.py`, Python until OR2) | here |
//! |---|---|
//! | `stock_bundles`, `ondemand_bundles`, `stock_index` (the pickled cache), `masked_names`, `live_entries`, `our_files`, `primary_type` | [`index`] |
//! | `closure` | [`closure`] |
//! | `write_stage`, `fetch_stock`, `pack_level` / `finish_packs`, `build_ww2_common`, `patch_bundle`, `tree_files` | [`stage`] |
//! | `model_group_check` (`havok_parts`, `group_ids`, `group_members`, `group_expects`, `member_mismatch`, `loaded_names`), `level_names`, `icon_gate` | [`gates`] |
//! | `description`, `air_superiority_mode`, `level_list_items` / `level_list_xml` / `level_list_query` | [`describe`] |
//! | `edit_level`, `team_refs`, `remap_targets`, `patch_refs`, `persistence_ids`, `weapon_persistence`, `patch_ours`, `kit_voices`, `sky_patch`, `root_files`, `mesh_flags`, `ondemand_waves`, `emitter_atlas`, `undergrowth_files` | [`levelfiles`] |
//! | `terrain_files`, `undergrowth_scales` / `undergrowth_blocks` / `_undergrowth_uses` (and the suite's `terrain._Walk` it used) | [`terrainfiles`] |

#![allow(
    clippy::implicit_hasher,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::type_complexity,
    clippy::format_push_string,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    clippy::struct_field_names,
    clippy::map_unwrap_or,
    reason = "the Python's names, maps and lower-case names (every name the build compares is lower case already);               its report lines built piece by piece"
)]

pub mod closure;
pub mod describe;
pub mod gates;
pub mod index;
pub mod levelfiles;

pub mod stage;
pub mod terrainfiles;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// A build step's failure, in words (Python's `SystemExit` message, or the exception it raised as
/// `<Type>: <message>`).
pub type BuildError = String;

/// The result of a build step.
pub type Result<T> = std::result::Result<T, BuildError>;

/// `<non-resource>`: a partition's bundle type.
pub const NONRES: &str = "<non-resource>";

/// `os.path.join(base, *rel.split('/'))` on Windows: the parts joined with `\`.
#[must_use]
pub fn pjoin(base: &str, rel: &str) -> String {
    let mut out = base.to_owned();
    for part in rel.split('/') {
        if !(out.is_empty() || out.ends_with(['\\', '/'])) {
            out.push('\\');
        }
        out.push_str(part);
    }
    out
}

/// `os.path.dirname` of a Windows path.
#[must_use]
pub fn dirname(p: &str) -> &str {
    match p.rfind(['\\', '/']) {
        Some(0) => p.get(..1).unwrap_or_default(),
        Some(i) => {
            let head = p.get(..i).unwrap_or_default();
            let t = head.trim_end_matches(['\\', '/']);
            if t.is_empty() || t.ends_with(':') {
                p.get(..=i).unwrap_or_default()
            } else {
                t
            }
        }
        None => "",
    }
}

/// `os.walk(top)`'s files in its order (top-down: a folder's files in the order the system lists them, then each
/// sub-folder in that order): their paths (`<folder>\<name>`). Nothing when top is missing.
#[must_use]
pub fn walk(top: &str) -> Vec<String> {
    let mut out = Vec::new();
    walk_into(top, &mut out);
    out
}

fn walk_into(dir: &str, out: &mut Vec<String>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    let mut dirs = Vec::new();
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = pjoin(dir, &name);
        if Path::new(&p).is_dir() {
            dirs.push(p);
        } else {
            out.push(p);
        }
    }
    for d in dirs {
        walk_into(&d, out);
    }
}

/// `os.path.relpath(p, top).replace(os.sep, '/')` for a path under top.
#[must_use]
pub fn rel_slash(p: &str, top: &str) -> String {
    crate::pytext::relpath(p, top).replace('\\', "/")
}

/// The bytes of a file, or the error in Python's words.
///
/// # Errors
/// The file cannot be read.
pub fn read(p: &str) -> Result<Vec<u8>> {
    fs::read(p).map_err(|e| format!("{p}: {e}"))
}

/// Writes a file whole (its folder made first), as `open(p, 'wb').write(data)`.
///
/// # Errors
/// The file cannot be written.
pub fn write(p: &str, data: &[u8]) -> Result<()> {
    let parent = dirname(p);
    if !parent.is_empty() {
        fs::create_dir_all(parent).map_err(|e| format!("{parent}: {e}"))?;
    }
    fs::write(p, data).map_err(|e| format!("{p}: {e}"))
}

/// `fbrb.read(bundle, names)`: each named entry's bytes, by the name as asked (`KeyError` for one not there).
///
/// # Errors
/// The bundle cannot be read, or a name is not in it.
pub fn bundle_read(bundle: &str, names: &[String]) -> Result<HashMap<String, Vec<u8>>> {
    let lowered: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let data = bc2_gamefs::bundles::read_path(bundle, &lowered).map_err(|e| format!("{bundle}: {e}"))?;
    Ok(names.iter().cloned().zip(data).collect())
}

/// `fbrb.read(bundle, [name])[name]`.
///
/// # Errors
/// As [`bundle_read`].
pub fn bundle_one(bundle: &str, name: &str) -> Result<Vec<u8>> {
    bundle_read(bundle, &[name.to_owned()])?
        .remove(name)
        .ok_or_else(|| format!("KeyError: '{name}'"))
}

/// `dbx.to_xml(data)` (DbxEdit's XML).
///
/// # Errors
/// A damaged file (`ValueError`).
pub fn to_xml(data: &[u8]) -> Result<String> {
    bc2_formats::dbx::to_xml(data).map_err(|e| format!("ValueError: {e}"))
}

/// An insertion-ordered map of names, as Python's `dict` keeps them: a key set again keeps its place, `pop` removes
/// it.
#[derive(Debug, Clone)]
pub struct OMap<V> {
    items: Vec<Option<(String, V)>>,
    at: HashMap<String, usize>,
}

impl<V> Default for OMap<V> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            at: HashMap::new(),
        }
    }
}

impl<V> OMap<V> {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `d[k] = v`.
    pub fn insert(&mut self, k: &str, v: V) {
        if let Some(&i) = self.at.get(k)
            && let Some(Some(slot)) = self.items.get_mut(i)
        {
            slot.1 = v;
            return;
        }
        self.at.insert(k.to_owned(), self.items.len());
        self.items.push(Some((k.to_owned(), v)));
    }

    /// `d.setdefault(k, v)`: true when it was set.
    pub fn set_default(&mut self, k: &str, v: V) -> bool {
        if self.at.contains_key(k) {
            return false;
        }
        self.insert(k, v);
        true
    }

    /// `d.get(k)`.
    #[must_use]
    pub fn get(&self, k: &str) -> Option<&V> {
        let i = *self.at.get(k)?;
        self.items.get(i)?.as_ref().map(|(_, v)| v)
    }

    /// `d[k]` to change.
    pub fn get_mut(&mut self, k: &str) -> Option<&mut V> {
        let i = *self.at.get(k)?;
        self.items.get_mut(i)?.as_mut().map(|(_, v)| v)
    }

    /// `k in d`.
    #[must_use]
    pub fn contains(&self, k: &str) -> bool {
        self.at.contains_key(k)
    }

    /// `d.pop(k, None)`.
    pub fn remove(&mut self, k: &str) -> Option<V> {
        let i = self.at.remove(k)?;
        self.items.get_mut(i)?.take().map(|(_, v)| v)
    }

    /// `len(d)`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.at.len()
    }

    /// `not d`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.at.is_empty()
    }

    /// `d.items()` in order.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.items.iter().flatten().map(|(k, v)| (k.as_str(), v))
    }

    /// `d.keys()` in order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.iter().map(|(k, _)| k)
    }

    /// The items, in order.
    #[must_use]
    pub fn into_vec(self) -> Vec<(String, V)> {
        self.items.into_iter().flatten().collect()
    }
}

/// Python's slice `s[k:]` of an ASCII-safe string by characters, `k` possibly negative.
#[must_use]
pub fn py_tail(s: &str, k: isize) -> String {
    let n = s.chars().count() as isize;
    let start = if k < 0 { (n + k).max(0) } else { k.min(n) };
    s.chars().skip(start as usize).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_as_python() {
        assert_eq!(pjoin(r"C:\w", "a/b.dbx"), r"C:\w\a\b.dbx");
        assert_eq!(pjoin(r"C:\w\", "a"), r"C:\w\a");
        assert_eq!(dirname(r"C:\w\a\b.dbx"), r"C:\w\a");
        assert_eq!(dirname(r"C:\x"), r"C:\");
        assert_eq!(py_tail("abcdef", 2), "cdef");
        assert_eq!(py_tail("abc", -2), "bc");
        assert_eq!(py_tail("abc", -9), "abc");
    }

    #[test]
    fn ordered_map_as_dict() {
        let mut m = OMap::new();
        m.insert("b", 1);
        m.insert("a", 2);
        m.insert("b", 3);
        assert!(!m.set_default("a", 9));
        assert_eq!(
            m.iter().map(|(k, v)| (k.to_owned(), *v)).collect::<Vec<_>>(),
            [("b".into(), 3), ("a".into(), 2)]
        );
        assert_eq!(m.remove("b"), Some(3));
        m.insert("b", 4);
        assert_eq!(m.keys().collect::<Vec<_>>(), ["a", "b"]);
        assert_eq!(m.len(), 2);
    }
}
