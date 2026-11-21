//! Game files by name, in the unpacked bundles a level is prepared from (`renderqa.Files`' staged folders): each
//! folder's `fbrb.manifest` lists its entries (`name|type|flags|size...`); the first folder naming a file wins, as the
//! editor's own level loader orders them (Package, Dist, the level's terrain, the shared bundles).

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use crate::error::RenderError;
use bc2_formats::fbrb;

/// The environment variable that reads Package bundles without the bundle cache, one after another ([`Files::fetch`]
/// as rounds R5c to GW1 read them).
pub const UNCACHED_ENV: &str = "BC2_RENDER_UNCACHED";

/// The files of a set of unpacked bundle folders.
#[derive(Debug, Clone, Default)]
pub struct Files {
    /// Lower-case name -> (path, FbrbTool type).
    entries: HashMap<String, (PathBuf, String)>,
    /// Or the files of a stack of bundles, read from the bundles themselves ([`Files::bundles`]).
    bundled: Option<Arc<Bundled>>,
}

/// A stack of bundles read in place (round R5c: the Package levels, nothing unpacked to disk).
#[derive(Debug, Default)]
struct Bundled {
    /// The bundles, in the game's order.
    paths: Vec<PathBuf>,
    /// Lower-case name -> (bundle, the name as stored, its type): the first bundle that lists a name decides it.
    index: HashMap<String, (usize, String, String)>,
    /// What was read.
    data: Mutex<HashMap<String, Arc<Vec<u8>>>>,
    /// Names asked for and not read yet.
    missed: Mutex<BTreeSet<String>>,
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
        Self { entries, bundled: None }
    }

    /// The files of a stack of bundles, in the game's order (a level's own, its common bundles, `mp_common`): the
    /// first bundle that lists a name decides it - its data, or for a *deleted* marker nothing. Only the headers
    /// are read here; a file is read by [`Files::fetch`] after [`Files::read`] has asked for it (and answered
    /// `None`): one streamed pass over each bundle for every name asked, nothing written to disk.
    ///
    /// # Errors
    /// A bundle's header cannot be read.
    pub fn bundles(paths: &[PathBuf]) -> Result<Self, RenderError> {
        let mut index = HashMap::new();
        for (k, p) in paths.iter().enumerate() {
            let h = fbrb::entries_path(p).map_err(|e| RenderError::Data(format!("{}: {e}", p.display())))?;
            for e in &h.entries {
                index
                    .entry(e.name.to_ascii_lowercase())
                    .or_insert_with(|| (k, e.name.clone(), e.type_name.clone()));
            }
        }
        // the deleted markers hide the name in every later bundle
        index.retain(|_, (_, _, t)| t != fbrb::DELETED);
        Ok(Self {
            entries: HashMap::new(),
            bundled: Some(Arc::new(Bundled {
                paths: paths.to_vec(),
                index,
                ..Bundled::default()
            })),
        })
    }

    /// The names [`Files::read`] was asked for that the bundles hold and that are not read yet (taken).
    #[must_use]
    pub fn missed(&self) -> Vec<String> {
        self.bundled.as_ref().map_or_else(Vec::new, |b| {
            std::mem::take(&mut *b.missed.lock().unwrap_or_else(PoisonError::into_inner))
                .into_iter()
                .collect()
        })
    }

    /// Reads names from the bundles; returns how many were read. Round R5d: the bundles of a round are read at once
    /// (one task a bundle: a payload is one gzip stream) and through the suite's bundle cache
    /// ([`bc2_formats::fbrb::read_many`]: the payload decompressed so far kept in the process, and a large bundle read
    /// a second time written whole to the shared disk cache, `BFBC2_FBRB_CACHE`, inside its one budget), so a later
    /// round seeks instead of decompressing the payload from its start again. [`UNCACHED_ENV`] (or the cache turned
    /// off) reads as before: one streamed pass a bundle, one bundle after another.
    ///
    /// # Errors
    /// A bundle cannot be read.
    pub fn fetch(&self, names: &[String]) -> Result<usize, RenderError> {
        let Some(b) = &self.bundled else { return Ok(0) };
        let mut by_bundle: HashMap<usize, Vec<(String, String)>> = HashMap::new();
        for n in names {
            if let Some((k, stored, _)) = b.index.get(&n.to_lowercase()) {
                by_bundle
                    .entry(*k)
                    .or_default()
                    .push((n.to_lowercase(), stored.clone()));
            }
        }
        let mut keys: Vec<usize> = by_bundle.keys().copied().collect();
        keys.sort_unstable();
        let lists: Vec<(usize, Vec<(String, String)>)> = keys
            .into_iter()
            .map(|k| (k, by_bundle.remove(&k).unwrap_or_default()))
            .collect();
        let items: Vec<(&PathBuf, Vec<&str>)> = lists
            .iter()
            .filter_map(|(k, list)| Some((b.paths.get(*k)?, list.iter().map(|(_, s)| s.as_str()).collect())))
            .collect();
        let got: Vec<Result<Vec<Vec<u8>>, RenderError>> = if std::env::var_os(UNCACHED_ENV).is_some() {
            items
                .iter()
                .map(|(p, n)| {
                    fbrb::read_path_uncached(p, n).map_err(|e| RenderError::Data(format!("{}: {e}", p.display())))
                })
                .collect()
        } else {
            fbrb::read_many(&items)
                .into_iter()
                .zip(&items)
                .map(|(r, (p, _))| r.map_err(|e| RenderError::Data(format!("{}: {e}", p.display()))))
                .collect()
        };
        let mut read = 0;
        let mut data = b.data.lock().unwrap_or_else(PoisonError::into_inner);
        for ((_, list), got) in lists.into_iter().zip(got) {
            for ((low, _), bytes) in list.into_iter().zip(got?) {
                data.insert(low, Arc::new(bytes));
                read += 1;
            }
        }
        Ok(read)
    }

    /// The bytes of the files read from bundles, in all.
    #[must_use]
    pub fn bundled_bytes(&self) -> usize {
        self.bundled.as_ref().map_or(0, |b| {
            b.data
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .values()
                .map(|d| d.len())
                .sum()
        })
    }

    /// Where a file is.
    #[must_use]
    pub fn path(&self, name: &str) -> Option<&Path> {
        self.entries.get(&name.to_lowercase()).map(|(p, _)| p.as_path())
    }

    /// A file's bytes, or `None` when it is not listed or cannot be read.
    #[must_use]
    pub fn read(&self, name: &str) -> Option<Vec<u8>> {
        if let Some(b) = &self.bundled {
            let low = name.to_lowercase();
            if let Some(d) = b.data.lock().unwrap_or_else(PoisonError::into_inner).get(&low) {
                return Some(d.as_ref().clone());
            }
            if b.index.contains_key(&low) {
                b.missed.lock().unwrap_or_else(PoisonError::into_inner).insert(low);
            }
            return None;
        }
        fs::read(self.path(name)?).ok()
    }

    /// A file's FbrbTool type (`RigidMeshSet`, `ITexture` ...).
    #[must_use]
    pub fn kind(&self, name: &str) -> Option<&str> {
        if let Some(b) = &self.bundled {
            return b.index.get(&name.to_lowercase()).map(|(_, _, t)| t.as_str());
        }
        self.entries.get(&name.to_lowercase()).map(|(_, k)| k.as_str())
    }

    /// Every listed name (lower case).
    pub fn names(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        match &self.bundled {
            Some(b) => Box::new(b.index.keys().map(String::as_str)),
            None => Box::new(self.entries.keys().map(String::as_str)),
        }
    }

    /// How many files are listed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bundled.as_ref().map_or(self.entries.len(), |b| b.index.len())
    }

    /// Whether no file is listed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// The bundles a Package level loads, from the game's registry (`game\Dist\win32\package.mft`): its
/// `level-00.fbrb` stack, then its `terrain-00.fbrb` stack's other bundles - each in the game's look-up order
/// (`package:` is `game\Package`, `game:` is `game\Dist\win32`). `None` when the registry does not name the level
/// or a bundle is missing.
#[must_use]
pub fn package_stack(game: &Path, lv: &str) -> Option<Vec<PathBuf>> {
    let text = fs::read(game.join("Dist").join("win32").join("package.mft")).ok()?;
    let text = String::from_utf8_lossy(&text);
    let mut stacks: HashMap<String, Vec<String>> = HashMap::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(src) = line.strip_prefix("BundleSource ") {
            let key = src.trim().to_lowercase();
            stacks.entry(key.clone()).or_default();
            current = Some(key);
        } else if let Some(t) = line.strip_prefix("BundleTarget ") {
            if let Some(list) = current.as_ref().and_then(|k| stacks.get_mut(k)) {
                list.push(t.trim().to_owned());
            }
        } else if line.is_empty() {
            current = None;
        }
    }
    let lv = lv.to_lowercase();
    let mut out: Vec<PathBuf> = Vec::new();
    for kind in ["level", "terrain"] {
        let Some(list) = stacks.get(&format!("game:levels/{lv}/{kind}-00.fbrb")) else {
            if kind == "level" {
                return None;
            }
            continue;
        };
        for r in list {
            let (root, rest) = if let Some(r) = r.strip_prefix("game:") {
                (game.join("Dist").join("win32"), r)
            } else if let Some(r) = r.strip_prefix("package:") {
                (game.join("Package"), r)
            } else {
                continue;
            };
            let p = rest.split('/').fold(root, |p, s| p.join(s));
            if !p.is_file() {
                return None;
            }
            if !out.contains(&p) {
                out.push(p);
            }
        }
    }
    Some(out)
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
