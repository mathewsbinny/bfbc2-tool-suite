//! Game files by name, in the unpacked bundles a level is prepared from (`renderqa.Files`' staged folders): each
//! folder's `fbrb.manifest` lists its entries (`name|type|flags|size...`); the first folder naming a file wins, as the
//! editor's own level loader orders them (Package, Dist, the level's terrain, the shared bundles).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// The files of a set of unpacked bundle folders.
#[derive(Debug, Clone, Default)]
pub struct Files {
    /// Lower-case name -> (path, FbrbTool type).
    entries: HashMap<String, (PathBuf, String)>,
}

impl Files {
    /// The files listed by each folder's `fbrb.manifest` (folders without one are skipped), earlier folders first.
    #[must_use]
    pub fn new(folders: &[PathBuf]) -> Self {
        let mut entries = HashMap::new();
        for top in folders {
            let Ok(text) = fs::read_to_string(top.join("fbrb.manifest")) else {
                continue;
            };
            for line in text.lines().skip(1) {
                let mut parts = line.split('|');
                let (Some(name), Some(kind)) = (parts.next(), parts.next()) else {
                    continue;
                };
                let path = name.split('/').fold(top.clone(), |p, s| p.join(s));
                entries
                    .entry(name.to_lowercase())
                    .or_insert_with(|| (path, kind.to_owned()));
            }
        }
        Self { entries }
    }

    /// Where a file is.
    #[must_use]
    pub fn path(&self, name: &str) -> Option<&Path> {
        self.entries.get(&name.to_lowercase()).map(|(p, _)| p.as_path())
    }

    /// A file's bytes, or `None` when it is not listed or cannot be read.
    #[must_use]
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        fs::read(self.path(name)?).ok()
    }

    /// A file's FbrbTool type (`RigidMeshSet`, `ITexture` ...).
    #[must_use]
    pub fn kind(&self, name: &str) -> Option<&str> {
        self.entries.get(&name.to_lowercase()).map(|(_, k)| k.as_str())
    }

    /// Every listed name (lower case).
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.keys().map(String::as_str)
    }

    /// How many files are listed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no file is listed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The folders a prepared level's files are read from, as the editor's level loader orders them: `work\levels\<lv>`'s
/// `pkg`, `dist` and `terrain`, then `work\navkit\common`'s shared bundles.
#[must_use]
pub fn level_folders(opfor: &Path, lv: &str) -> Vec<PathBuf> {
    let base = opfor.join("work").join("levels").join(lv.to_lowercase());
    let mut out = vec![base.join("pkg"), base.join("dist"), base.join("terrain")];
    if let Ok(rd) = fs::read_dir(opfor.join("work").join("navkit").join("common")) {
        let mut common: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        common.sort();
        out.extend(common);
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    #[test]
    fn first_folder_wins_and_names_ignore_case() {
        let dir = std::env::temp_dir().join(format!("bc2render_files_{}", std::process::id()));
        let (a, b) = (dir.join("a"), dir.join("b"));
        for (d, body) in [(&a, "A"), (&b, "B")] {
            fs::create_dir_all(d.join("Sub")).ok();
            fs::write(
                d.join("fbrb.manifest"),
                "# fbrb.manifest\nSub/Thing.res|ITexture|00010000|1\n",
            )
            .ok();
            fs::write(d.join("Sub").join("Thing.res"), body).ok();
        }
        let f = Files::new(&[a, b, dir.join("missing")]);
        assert_eq!(f.len(), 1);
        assert_eq!(f.read("sub/thing.RES").as_deref(), Some(&b"A"[..]));
        assert_eq!(f.kind("Sub/Thing.res"), Some("ITexture"));
        fs::remove_dir_all(&dir).ok();
    }
}
