//! Which collision asset a level's blueprints use: NavKit's `levelobjects.havok_index` (every `*_Havok.res` under
//! the level's data folders) and `levelobjects.mesh_aliases` (blueprints whose `Mesh` field names another object's
//! mesh), ported with their order and first-wins rules.
//!
//! One walk of each folder finds the packfiles, the converted XML NavKit's prepare step writes and the binary
//! partitions, the folders' subfolders in parallel. The XML files decide which partitions count and in which order,
//! as in the reference; each one's `Mesh` is read from the binary `.dbx` next to it ([`dbx::field_ref`]: the answer
//! `xmlinst.field_ref` gives on its XML, without building the text; `MP_005`'s index reads 87 MB of partitions instead
//! of 205 MB of XML), and from the XML only where the binary cannot answer (an empty *deleted* marker, a text or
//! damaged file). Files are read on all cores.
//!
//! ```no_run
//! use std::path::PathBuf;
//!
//! let bases = [PathBuf::from(r"work\levels\mp_005\pkg"), PathBuf::from(r"work\levels\mp_005\dist")];
//! let index = bc2_scene::collision::index(&bases);
//! let havok = index.resolved()?;                // blueprint -> its packfile, aliases applied as level.load applies them
//! # Ok::<(), std::io::Error>(())
//! ```

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};

use bc2_formats::dbx;
/// `xmlinst.field_ref` on the readable XML (in `bc2-formats` since the aliases are read from the binary files).
pub use bc2_formats::dbx::field_ref_text as field_ref;
use rayon::prelude::*;

use crate::{alias_cache, walk};

/// A map that keeps its insertion order and refuses to replace a key (Python's `dict.setdefault` in a loop).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FirstWins<V> {
    entries: Vec<(String, V)>,
    at: HashMap<String, usize>,
}

impl<V> FirstWins<V> {
    /// `value` under `key` unless the key is there already.
    pub fn set_default(&mut self, key: &str, value: V) {
        if !self.at.contains_key(key) {
            self.at.insert(key.to_owned(), self.entries.len());
            self.entries.push((key.to_owned(), value));
        }
    }

    /// The value under `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.at.get(key).and_then(|&k| self.entries.get(k)).map(|(_, v)| v)
    }

    /// Whether `key` is there.
    #[must_use]
    pub fn contains(&self, key: &str) -> bool {
        self.at.contains_key(key)
    }

    /// The entries in insertion order.
    #[must_use]
    pub fn entries(&self) -> &[(String, V)] {
        &self.entries
    }

    /// The number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// What [`index`] finds.
#[derive(Debug)]
pub struct CollisionIndex {
    /// `levelobjects.havok_index`: lower-case blueprint path -> its packfile.
    pub havok: FirstWins<PathBuf>,
    /// `levelobjects.mesh_aliases`: lower-case blueprint path -> the blueprint path of its `Mesh` (without `_mesh`),
    /// or the error that stopped the reference (an XML file that could not be read).
    pub aliases: io::Result<FirstWins<String>>,
}

impl CollisionIndex {
    /// The packfile of each blueprint as `level.load` resolves it: [`CollisionIndex::havok`], then for each alias in
    /// order a blueprint without a packfile takes its target's (an error reading the aliases is returned).
    ///
    /// # Errors
    ///
    /// The aliases' error.
    pub fn resolved(&self) -> io::Result<FirstWins<PathBuf>> {
        let aliases = self
            .aliases
            .as_ref()
            .map_err(|e| io::Error::new(e.kind(), e.to_string()))?;
        let mut havok = self.havok.clone();
        for (bp, target) in aliases.entries() {
            if !havok.contains(bp)
                && let Some(p) = havok.get(target).cloned()
            {
                havok.set_default(bp, p);
            }
        }
        Ok(havok)
    }
}

const HAVOK: &str = "_havok.res";

/// `levelobjects.havok_index`: every packfile under `bases` by its lower-case blueprint path (the first base that
/// has one wins), without the mesh aliases.
#[must_use]
pub fn havok_index<P: AsRef<Path> + Sync>(bases: &[P]) -> FirstWins<PathBuf> {
    let walks: Vec<Vec<Vec<walk::Found>>> = bases.par_iter().map(|b| walk::find(b.as_ref(), &[HAVOK])).collect();
    packfiles(&walks)
}

/// The packfile index of walks whose first list is the packfiles.
fn packfiles(walks: &[Vec<Vec<walk::Found>>]) -> FirstWins<PathBuf> {
    let mut havok = FirstWins::default();
    for found in walks.iter().filter_map(|w| w.first()) {
        for f in found {
            let mut rel = f.rel.get(..f.rel.len().saturating_sub(HAVOK.len())).unwrap_or_default();
            havok.set_default(rel, f.path.clone());
            // <name>_Havok_Havok.res (sp_009's animated aircraft door, inv_Wall): the blueprint is <name>
            while let Some(shorter) = rel.strip_suffix("_havok") {
                rel = shorter;
                havok.set_default(rel, f.path.clone());
            }
        }
    }
    havok
}

/// One base folder's walk: the packfiles, the XML files, the binary partitions.
type Walk = Vec<Vec<walk::Found>>;
/// One base folder's cached entries, by the XML file's path.
type Cached = HashMap<String, alias_cache::Entry>;
/// One XML file's `Mesh` reference (or the error reading it) and what the cache keeps of it.
type Answer = (io::Result<Option<String>>, Option<alias_cache::Entry>);

/// The packfiles and mesh aliases under `bases` (in order: the first base that has a blueprint wins), every file
/// read ([`index_cached`] without a cache).
#[must_use]
pub fn index<P: AsRef<Path> + Sync>(bases: &[P]) -> CollisionIndex {
    index_cached(bases, None)
}

/// [`index`], each XML file's `Mesh` reference reused from the cache in `cache` (one file per base,
/// [`crate::alias_cache`]) while the XML file and its binary partition keep the size and modification time they had
/// when it was read; the others are read and the cache updated. The same index as [`index`] whatever the cache holds.
#[must_use]
pub fn index_cached<P: AsRef<Path> + Sync>(bases: &[P], cache: Option<&Path>) -> CollisionIndex {
    let (walks, cached): (Vec<Walk>, Vec<Cached>) = bases
        .par_iter()
        .map(|b| {
            let found = walk::find(b.as_ref(), &[HAVOK, ".xml", ".dbx"]);
            (
                found,
                cache.map_or_else(HashMap::new, |d| alias_cache::load(d, b.as_ref())),
            )
        })
        .unzip();
    let havok = packfiles(&walks);
    // the converted XML files decide which partitions count and in which order (as in the reference); each one's
    // Mesh is read from its binary .dbx next to it when that reads, else from the XML
    let mut binaries: HashMap<(usize, &str), &walk::Found> = HashMap::new();
    let mut xml: Vec<(usize, &walk::Found)> = Vec::new();
    for (b, w) in walks.iter().enumerate() {
        xml.extend(w.get(1).into_iter().flatten().map(|f| (b, f)));
        for f in w.get(2).into_iter().flatten() {
            binaries.insert((b, stem(&f.rel)), f);
        }
    }
    let refs: Vec<Answer> = xml
        .par_iter()
        .map(|&(b, f)| {
            let binary = binaries.get(&(b, stem(&f.rel))).copied();
            let key = f.path.to_string_lossy();
            let stamps = f.stamp.and_then(|x| match binary {
                Some(d) => d.stamp.map(|s| (x, Some(s))),
                None => Some((x, None)),
            });
            if let (Some((x, d)), Some(hit)) = (stamps, cached.get(b).and_then(|c| c.get(key.as_ref())))
                && hit.xml == x
                && hit.dbx == d
            {
                return (Ok(hit.mesh.clone()), Some(hit.clone()));
            }
            let r = mesh_of(binary, f);
            let entry = match (&r, stamps) {
                (Ok(m), Some((x, d))) => Some(alias_cache::Entry {
                    xml: x,
                    dbx: d,
                    mesh: m.clone(),
                }),
                _ => None,
            };
            (r, entry)
        })
        .collect();
    if let Some(dir) = cache {
        store_caches(dir, bases, &xml, &refs, &cached);
    }
    let aliases = (|| {
        let mut out = FirstWins::default();
        for ((_, f), (r, _)) in xml.iter().zip(refs) {
            let Some(r) = r? else { continue };
            if r.is_empty() || r == "null" {
                continue;
            }
            let r = universal_newlines(&r);
            let rel = stem(&f.rel);
            let mesh = r.rsplit_once('/').map_or(r.as_str(), |(p, _)| p).to_lowercase();
            out.set_default(rel, strip_mesh(&mesh));
        }
        Ok(out)
    })();
    CollisionIndex { havok, aliases }
}

/// An XML file's `Mesh` reference: from its binary partition when that answers (not when it cannot be read, is empty
/// - a deleted marker -, is text or is damaged), else from the XML.
fn mesh_of(binary: Option<&walk::Found>, xml: &walk::Found) -> io::Result<Option<String>> {
    let answer = binary
        .and_then(|f| std::fs::read(&f.path).ok())
        .filter(|d| dbx::is_binary(d))
        .and_then(|d| dbx::field_ref(&d, "Mesh").ok());
    if let Some(r) = answer {
        return Ok(r);
    }
    let data = std::fs::read(&xml.path)?;
    Ok(field_ref(&String::from_utf8_lossy(&data), "Mesh"))
}

/// Rewrites the cache file of each base whose entries changed: every XML file read without an error, with its
/// stamps. A base with a file that could not be read keeps its old file; an error writing is ignored (the next index
/// reads those files again).
fn store_caches<P: AsRef<Path>>(
    dir: &Path,
    bases: &[P],
    xml: &[(usize, &walk::Found)],
    refs: &[Answer],
    cached: &[Cached],
) {
    for (b, base) in bases.iter().enumerate() {
        let mine: Vec<(String, &Answer)> = xml
            .iter()
            .zip(refs)
            .filter(|((k, _), _)| *k == b)
            .map(|((_, f), r)| (f.path.to_string_lossy().into_owned(), r))
            .collect();
        if mine.iter().any(|(_, (r, _))| r.is_err()) {
            continue;
        }
        let entries: Vec<(&str, &alias_cache::Entry)> = mine
            .iter()
            .filter_map(|(k, (_, e))| e.as_ref().map(|e| (k.as_str(), e)))
            .collect();
        let old = cached.get(b);
        let unchanged =
            old.is_some_and(|old| old.len() == entries.len() && entries.iter().all(|(k, e)| old.get(*k) == Some(*e)));
        if unchanged {
            alias_cache::touch(dir, base.as_ref()); // used: its time is the cache's least-recently-used clock
        } else {
            let _ = alias_cache::store(dir, base.as_ref(), &entries);
        }
    }
}

/// Python's text mode: `\r\n` and `\r` read as `\n`.
fn universal_newlines(s: &str) -> String {
    s.replace("\r\n", "\n").replace('\r', "\n")
}

/// `re.sub(r'_mesh$', '', s)`: `_mesh` at the end, or just before a final newline, removed.
fn strip_mesh(s: &str) -> String {
    if let Some(p) = s.strip_suffix("_mesh") {
        p.to_owned()
    } else if let Some(p) = s.strip_suffix("_mesh\n") {
        format!("{p}\n")
    } else {
        s.to_owned()
    }
}

/// The path without its 4-character extension (`.xml`, `.dbx`).
fn stem(rel: &str) -> &str {
    rel.get(..rel.len().saturating_sub(4)).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn mesh_suffixes() {
        assert_eq!(strip_mesh("objects/x/x_mesh"), "objects/x/x");
        assert_eq!(strip_mesh("objects/x/x_mesh\n"), "objects/x/x\n");
        assert_eq!(strip_mesh("objects/x/mesh"), "objects/x/mesh");
        assert_eq!(universal_newlines("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn index_and_aliases() {
        let root = std::env::temp_dir().join(format!("bc2_scene_collision_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let (pkg, dist) = (root.join("pkg"), root.join("dist"));
        for d in ["objects/a", "objects/door"] {
            fs::create_dir_all(dist.join(d)).unwrap();
        }
        fs::create_dir_all(pkg.join("objects/a")).unwrap();
        fs::write(dist.join("objects/a/a_Havok.res"), b"dist").unwrap();
        fs::write(pkg.join("objects/a/a_Havok.res"), b"pkg").unwrap();
        fs::write(dist.join("objects/door/door_Havok_Havok.res"), b"").unwrap();
        fs::write(
            dist.join("objects/a/b.xml"),
            br#"<partition><instance><field ref="Objects/A/A_Mesh/9" name="Mesh"/></instance></partition>"#,
        )
        .unwrap();
        fs::write(dist.join("objects/a/c.xml"), br#"<field name="Mesh" ref="null"/>"#).unwrap();
        fs::write(
            dist.join("objects/a/d.xml"),
            br#"<field name="Mesh" ref="Objects/Gone_Mesh/1"/>"#,
        )
        .unwrap();
        let index = index(&[&pkg, &dist]);
        assert_eq!(
            index.havok.get("objects/a/a").unwrap(),
            &pkg.join("objects").join("a").join("a_Havok.res")
        );
        assert!(index.havok.contains("objects/door/door_havok"));
        assert!(index.havok.contains("objects/door/door"));
        let aliases = index.aliases.as_ref().unwrap();
        assert_eq!(aliases.len(), 2);
        assert_eq!(aliases.get("objects/a/b").unwrap(), "objects/a/a");
        assert!(!aliases.is_empty());
        let resolved = index.resolved().unwrap();
        assert_eq!(resolved.get("objects/a/b"), resolved.get("objects/a/a"));
        assert!(!resolved.contains("objects/a/d"));
        assert_eq!(resolved.entries().len(), 4);
        // havok_index alone: the same packfiles
        assert_eq!(havok_index(&[&pkg, &dist]), index.havok);
        // the Mesh read from the binary partition next to each XML file, the XML where the binary cannot answer
        let mut f = dbx::DbxFile::default();
        let mesh = dbx::Element::text(&mut f, "field", &[("name", "Mesh"), ("ref", "Objects/A/A_Mesh/2")], "");
        let inst = dbx::Element::container(&mut f, "instance", &[], vec![mesh]);
        f.roots.push(inst);
        let binary = dbx::write(&f).unwrap();
        let mut damaged = binary.clone();
        damaged[20..24].copy_from_slice(&[0xFF; 4]); // a string table past the end
        let stale = br#"<field name="Mesh" ref="Objects/Stale/Stale_Mesh/1"/>"#;
        let other = br#"<field name="Mesh" ref="Objects/X/X_Mesh/1"/>"#;
        for (name, dbx_data, xml) in [
            ("e", Some(binary.as_slice()), Some(stale.as_slice())), // binary wins
            ("f", Some(b"".as_slice()), Some(other.as_slice())),    // a deleted marker: its XML
            ("g", Some(binary.as_slice()), None),                   // no XML: not counted, as in the reference
            ("h", Some(damaged.as_slice()), Some(other.as_slice())), // damaged: its XML
            ("i", Some(b"<partition/>".as_slice()), Some(other.as_slice())), // a text partition: its XML
        ] {
            if let Some(d) = dbx_data {
                fs::write(pkg.join(format!("objects/a/{name}.DBX")), d).unwrap();
            }
            if let Some(x) = xml {
                fs::write(pkg.join(format!("objects/a/{name}.xml")), x).unwrap();
            }
        }
        let index = super::index(&[&pkg, &dist]);
        let aliases = index.aliases.as_ref().unwrap();
        assert_eq!(aliases.get("objects/a/e").unwrap(), "objects/a/a");
        assert_eq!(aliases.get("objects/a/f").unwrap(), "objects/x/x");
        assert!(!aliases.contains("objects/a/g"));
        assert_eq!(aliases.get("objects/a/h").unwrap(), "objects/x/x");
        assert_eq!(aliases.get("objects/a/i").unwrap(), "objects/x/x");
        assert_eq!(
            aliases.entries()[..4]
                .iter()
                .map(|(k, _)| k.as_str())
                .collect::<Vec<_>>(),
            ["objects/a/e", "objects/a/f", "objects/a/h", "objects/a/i"]
        );
        // a .dbx that cannot be read (a folder): its XML
        fs::create_dir_all(pkg.join("objects/a/j.dbx")).unwrap();
        fs::write(pkg.join("objects/a/j.xml"), other).unwrap();
        let index = super::index(&[&pkg]);
        assert_eq!(
            index.aliases.as_ref().unwrap().get("objects/a/j").unwrap(),
            "objects/x/x"
        );
        // the cache: the same index cold and warm; a hit reads nothing; a change of size or time is read again
        let cache = root.join("cache");
        let plain = entries(&super::index(&[&pkg, &dist]));
        assert_eq!(entries(&index_cached(&[&pkg, &dist], Some(&cache))), plain);
        assert_eq!(fs::read_dir(&cache).unwrap().count(), 2);
        assert_eq!(entries(&index_cached(&[&pkg, &dist], Some(&cache))), plain);
        // the same size and time, other content: the cached answer (proof that the file was not read)
        let e = pkg.join("objects/a/e.xml");
        let when = fs::metadata(&e).unwrap().modified().unwrap();
        let mut changed = stale.to_vec();
        changed[0] = b' ';
        fs::remove_file(pkg.join("objects/a/e.DBX")).unwrap();
        fs::write(pkg.join("objects/a/e.DBX"), &binary).unwrap();
        let swap = |data: &[u8]| {
            fs::write(&e, data).unwrap();
            fs::File::options()
                .write(true)
                .open(&e)
                .unwrap()
                .set_modified(when)
                .unwrap();
        };
        swap(&changed);
        // the .dbx was rewritten (new time): read again, still the binary's answer
        let warm = index_cached(&[&pkg, &dist], Some(&cache));
        assert_eq!(
            warm.aliases.as_ref().unwrap().get("objects/a/e").unwrap(),
            "objects/a/a"
        );
        // now both unchanged: a hit even with the XML's content changed under the same stamp
        fs::write(
            pkg.join("objects/a/f.xml"),
            br#"<field name="Mesh" ref="Objects/Y/Y_Mesh/1"/>"#,
        )
        .unwrap();
        let fresh = index_cached(&[&pkg, &dist], Some(&cache));
        assert_eq!(
            fresh.aliases.as_ref().unwrap().get("objects/a/f").unwrap(),
            "objects/y/y"
        );
        assert_eq!(entries(&fresh), entries(&super::index(&[&pkg, &dist])));
        let f = pkg.join("objects/a/f.xml");
        let when_f = fs::metadata(&f).unwrap().modified().unwrap();
        fs::write(&f, br#"<field name="Mesh" ref="Objects/Z/Z_Mesh/1"/>"#).unwrap();
        fs::File::options()
            .write(true)
            .open(&f)
            .unwrap()
            .set_modified(when_f)
            .unwrap();
        let hit = index_cached(&[&pkg, &dist], Some(&cache));
        assert_eq!(hit.aliases.as_ref().unwrap().get("objects/a/f").unwrap(), "objects/y/y");
        // a damaged cache file: everything read again, the right answer, the file replaced
        for c in fs::read_dir(&cache).unwrap() {
            fs::write(c.unwrap().path(), b"damaged").unwrap();
        }
        let again = index_cached(&[&pkg, &dist], Some(&cache));
        assert_eq!(
            again.aliases.as_ref().unwrap().get("objects/a/f").unwrap(),
            "objects/z/z"
        );
        assert_eq!(entries(&again), entries(&super::index(&[&pkg, &dist])));
        // an XML file that cannot be read stops the aliases, as in the reference (cached or not)
        fs::create_dir_all(dist.join("objects/z.xml")).unwrap();
        let broken = super::index(&[&dist]);
        assert!(broken.aliases.is_err());
        assert!(broken.resolved().is_err());
        assert!(index_cached(&[&dist], Some(&cache)).aliases.is_err());
        // a cache folder that cannot be written: the index all the same
        let blocked = root.join("blocked");
        fs::write(&blocked, b"").unwrap();
        assert_eq!(
            entries(&index_cached(&[&pkg], Some(&blocked))),
            entries(&super::index(&[&pkg]))
        );
        fs::remove_dir_all(&root).unwrap();
    }

    type Lists = (Vec<(String, PathBuf)>, Vec<(String, String)>);

    /// An index as comparable lists.
    fn entries(i: &CollisionIndex) -> Lists {
        (
            i.havok.entries().to_vec(),
            i.aliases.as_ref().unwrap().entries().to_vec(),
        )
    }
}
