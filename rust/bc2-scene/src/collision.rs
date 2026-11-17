//! Which collision asset a level's blueprints use: NavKit's `levelobjects.havok_index` (every `*_Havok.res` under
//! the level's data folders) and `levelobjects.mesh_aliases` (blueprints whose `Mesh` field names another object's
//! mesh, read from the converted XML NavKit's prepare step writes), ported with their order and first-wins rules.
//!
//! One walk of each folder finds both kinds of file, the folders' subfolders in parallel; the XML files are read
//! on all cores.
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

use rayon::prelude::*;

use crate::walk;

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

/// The packfiles and mesh aliases under `bases` (in order: the first base that has a blueprint wins).
#[must_use]
pub fn index<P: AsRef<Path> + Sync>(bases: &[P]) -> CollisionIndex {
    let walks: Vec<Vec<Vec<walk::Found>>> = bases
        .par_iter()
        .map(|b| walk::find(b.as_ref(), &[HAVOK, ".xml"]))
        .collect();
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
    let xml: Vec<&walk::Found> = walks.iter().filter_map(|w| w.get(1)).flatten().collect();
    let refs: Vec<io::Result<Option<String>>> = xml
        .par_iter()
        .map(|f| {
            let data = std::fs::read(&f.path)?;
            Ok(field_ref(&String::from_utf8_lossy(&data), "Mesh"))
        })
        .collect();
    let aliases = (|| {
        let mut out = FirstWins::default();
        for (f, r) in xml.iter().zip(refs) {
            let Some(r) = r? else { continue };
            if r.is_empty() || r == "null" {
                continue;
            }
            let r = universal_newlines(&r);
            let rel = f.rel.get(..f.rel.len().saturating_sub(4)).unwrap_or_default();
            let mesh = r.rsplit_once('/').map_or(r.as_str(), |(p, _)| p).to_lowercase();
            out.set_default(rel, strip_mesh(&mesh));
        }
        Ok(out)
    })();
    CollisionIndex { havok, aliases }
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

/// A word character of Python's `re` on `str` patterns.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `xmlinst._ATTR.findall(s)`: every `(\w+)="([^"]*)"`, left to right.
fn attributes(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find(is_word) {
        let run = rest.get(start..).unwrap_or_default();
        let end = run.find(|c: char| !is_word(c)).unwrap_or(run.len());
        let (name, after) = run.split_at(end);
        if let Some(value) = after.strip_prefix("=\"")
            && let Some(close) = value.find('"')
        {
            out.push((name, value.get(..close).unwrap_or_default()));
            rest = value.get(close + 1..).unwrap_or_default();
        } else {
            rest = after;
        }
    }
    out
}

/// `xmlinst.field_ref(text, name)`: the `ref` attribute of the first `<field ...>` whose `name` is `name` and that
/// has a `ref` (attributes in any order; a repeated attribute counts by its last value), or `None`.
#[must_use]
pub fn field_ref(text: &str, name: &str) -> Option<String> {
    let mut rest = text;
    while let Some(at) = rest.find("<field") {
        let after = rest.get(at + "<field".len()..).unwrap_or_default();
        // \b: the next character is not a word character
        if after.chars().next().is_some_and(is_word) {
            rest = after;
            continue;
        }
        // ([^>]*?)/?> : up to the first '>', a '/' just before it left out
        let gt = after.find('>')?;
        let inner = after.get(..gt).unwrap_or_default();
        let inner = inner.strip_suffix('/').unwrap_or(inner);
        let mut found_name = None;
        let mut found_ref = None;
        for (k, v) in attributes(inner) {
            match k {
                "name" => found_name = Some(v),
                "ref" => found_ref = Some(v),
                _ => {}
            }
        }
        if found_name == Some(name)
            && let Some(r) = found_ref
        {
            return Some(r.to_owned());
        }
        rest = after.get(gt + 1..).unwrap_or_default();
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn field_refs() {
        let t = r#"<instance><field name="Other" ref="a/b"/><field ref="Objects/X/X_Mesh/1" name="Mesh"/></instance>"#;
        assert_eq!(field_ref(t, "Mesh").as_deref(), Some("Objects/X/X_Mesh/1"));
        assert_eq!(field_ref(t, "Other").as_deref(), Some("a/b"));
        assert_eq!(field_ref(t, "None"), None);
        // \b, a field without ref, a repeated attribute, attribute names inside other words, no closing '>'
        let t = r#"<fields name="Mesh" ref="no"/><field name="Mesh">x</field><field x-name="Mesh" name="Mesh" ref="1" ref="2">"#;
        assert_eq!(field_ref(t, "Mesh").as_deref(), Some("2"));
        assert_eq!(field_ref(r#"<field name="Mesh" ref="a""#, "Mesh"), None);
        assert_eq!(field_ref(r#"<field name="Mesh ref="a"/>"#, "Mesh"), None);
        assert_eq!(
            field_ref(r#"<field name="Mesh" ref="a/b"/>"#, "Mesh").as_deref(),
            Some("a/b")
        );
        assert_eq!(field_ref(r#"<field name="Mesh" ref=x/>"#, "Mesh"), None);
        assert_eq!(field_ref("<field", "Mesh"), None);
    }

    #[test]
    fn mesh_suffixes() {
        assert_eq!(strip_mesh("objects/x/x_mesh"), "objects/x/x");
        assert_eq!(strip_mesh("objects/x/x_mesh\n"), "objects/x/x\n");
        assert_eq!(strip_mesh("objects/x/mesh"), "objects/x/mesh");
        assert_eq!(universal_newlines("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
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
        // an XML file that cannot be read stops the aliases, as in the reference
        fs::create_dir_all(dist.join("objects/z.xml")).unwrap();
        let broken = super::index(&[&dist]);
        assert!(broken.aliases.is_err());
        assert!(broken.resolved().is_err());
        fs::remove_dir_all(&root).unwrap();
    }
}
