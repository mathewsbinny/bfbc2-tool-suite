//! `meshview.Index._build` (`bfbc2-mesh-viewer\meshview.py`, round O3): which bundle holds each entry (the first
//! stock bundle that lists it), and every Dx9 shader database's facts the mesh tools read - mesh part name -> vertex
//! layout record (after its GUID), database entry name -> the texture names its programs' parameter blocks name.
//!
//! The bundles are given in `meshview._bundles`' order. Where the reference would leave something out of its index
//! (a bundle whose header cannot be read, a database it cannot pick or parse) or raise (a program record too short,
//! an index past its section), [`build`] gives an error instead: the caller lets the Python build that index, so its
//! messages stay the reference's.

use std::collections::{HashMap, HashSet};

use crate::fbrb;
use crate::shaderdb::{self, query};

/// The index: each table in the reference's dict order (first insertion).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MeshIndex {
    /// (entry name lower case, index of its bundle in the list given, entry name as the bundle holds it)
    pub where_: Vec<(String, usize, String)>,
    /// (mesh part name lower case, index into [`MeshIndex::layouts`]): parts sharing a layout record share the
    /// object in the reference (the same `bytes` from its `layouts` dict)
    pub parts: Vec<(String, usize)>,
    /// the vertex layout records (each one object in the reference)
    pub layouts: Vec<Vec<u8>>,
    /// (database entry name lower case, its texture names)
    pub entries: Vec<(String, Vec<String>)>,
    /// the shader databases read from the bundles (each an `fbrb.read`: the reference's caches' budget asks then)
    pub databases: usize,
}

/// Latin-1 bytes as a `str`.
fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// `str.lower()` of a latin-1 name.
fn lower(b: &[u8]) -> String {
    latin1(&query::lower(b))
}

/// `re.findall(rb'[A-Za-z0-9_/\.\- ]{6,}', blk)`: the runs of those bytes six or more long.
fn texture_runs(blk: &[u8]) -> Vec<String> {
    let ok = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'/' | b'.' | b'-' | b' ');
    let mut out = Vec::new();
    let mut start = None;
    for (i, &c) in blk.iter().chain(std::iter::once(&0u8)).enumerate() {
        match (ok(c) && i < blk.len(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if i - s >= 6 {
                    out.push(latin1(blk.get(s..i).unwrap_or_default()));
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// `shaderdb.pick_database(bundle, 9, name)`: the name must match exactly one Dx9 database of the bundle (by name, or
/// as a folder above it).
fn picks_one(dbs: &[&str], name: &str) -> bool {
    let want = name.to_lowercase();
    let folder = format!("{}/", want.trim_end_matches('/'));
    dbs.iter()
        .filter(|n| {
            let l = n.to_lowercase();
            l == want || l.starts_with(&folder)
        })
        .count()
        == 1
}

/// The index of the bundles, in order; Err (in words) where the reference's index would differ from a complete one
/// or the reference would raise: the caller hands the build to the Python.
///
/// # Errors
/// A bundle or database the reference would skip, or a record it would fail on.
pub fn build(bundles: &[String]) -> Result<MeshIndex, String> {
    let mut ix = MeshIndex::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut parts_seen: HashSet<String> = HashSet::new();
    let mut entries_seen: HashSet<String> = HashSet::new();
    for (bi, b) in bundles.iter().enumerate() {
        let h = fbrb::entries_path(b).map_err(|e| format!("{b}: {e}"))?;
        for e in &h.entries {
            if e.is_deleted() {
                continue;
            }
            let key = e.name.to_lowercase();
            if seen.insert(key.clone()) {
                ix.where_.push((key, bi, e.name.clone()));
            }
        }
        let live: Vec<&fbrb::Entry> = h.entries.iter().filter(|e| !e.is_deleted()).collect();
        let dx9: Vec<&str> = live
            .iter()
            .filter(|e| e.type_name.ends_with("ShaderDatabase") && e.type_name.to_lowercase() == "dx9shaderdatabase")
            .map(|e| e.name.as_str())
            .collect();
        let names: Vec<&str> = live
            .iter()
            .filter(|e| e.type_name.to_lowercase() == "dx9shaderdatabase")
            .map(|e| e.name.as_str())
            .collect();
        for name in names {
            if !picks_one(&dx9, name) {
                return Err(format!("{b}:{name}: not one database by that name"));
            }
            let data = fbrb::read_path(b, &[name])
                .map_err(|e| format!("{b}:{name}: {e}"))?
                .into_iter()
                .next()
                .ok_or_else(|| format!("{b}:{name}: not read"))?;
            if data.len() < 9 {
                return Err(format!("{b}:{name}: too short"));
            }
            ix.databases += 1;
            let be = shaderdb::is_big_endian(&data);
            let db = shaderdb::parse(&data, 9, be).map_err(|e| format!("{b}:{name}: {e}"))?;
            add_database(&mut ix, &db, be, &mut parts_seen, &mut entries_seen)?;
        }
    }
    Ok(ix)
}

/// `Index._add_database(db)`.
fn add_database(
    ix: &mut MeshIndex,
    db: &shaderdb::ShaderDb<'_>,
    be: bool,
    parts_seen: &mut HashSet<String>,
    entries_seen: &mut HashSet<String>,
) -> Result<(), String> {
    // {r[:16]: r[16:]}: the last record of a GUID wins; each record one object
    let mut layouts: HashMap<&[u8], usize> = HashMap::new();
    for r in &db.layouts {
        let (g, rest) = r.split_at(r.len().min(16));
        let id = ix.layouts.len();
        ix.layouts.push(rest.to_vec());
        layouts.insert(g, id);
    }
    for p in &db.parts {
        if let Some(&id) = layouts.get(p.guid) {
            let key = lower(&p.name);
            if parts_seen.insert(key.clone()) {
                ix.parts.push((key, id));
            }
        }
    }
    let groups: HashMap<&[u8], &Vec<u16>> = db.groups.iter().map(|(g, idx)| (*g, idx)).collect();
    for e in &db.entries {
        let key = lower(&e.name);
        if entries_seen.contains(&key) {
            continue;
        }
        let mut tex: Vec<String> = Vec::new();
        for &pi in groups.get(e.guid).map_or(&[][..], |v| v.as_slice()) {
            let prog = db
                .programs
                .get(usize::from(pi))
                .ok_or("a program index past its section")?;
            let [_, _, a, c] = query::program_refs(prog, be).ok_or("a program record too short")?;
            for blk in [a, c] {
                let blk = db
                    .params
                    .get(blk as usize)
                    .ok_or("a parameter block index past its section")?;
                for s in texture_runs(blk) {
                    if s.contains('/') && !tex.contains(&s) {
                        tex.push(s);
                    }
                }
            }
        }
        entries_seen.insert(key.clone());
        ix.entries.push((key, tex));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runs_as_findall() {
        assert_eq!(
            texture_runs(b"\0abc/def\0ab/cd\0Textures/x_d.dds"),
            vec!["abc/def", "Textures/x_d.dds"]
        );
        assert_eq!(texture_runs(b"123456"), vec!["123456"]);
        assert_eq!(texture_runs(b"12345"), Vec::<String>::new());
    }

    #[test]
    fn database_picked_as_the_reference() {
        let dbs = ["levels/a/shaders/x", "levels/a/shaders/x/y"];
        assert!(!picks_one(&dbs, "levels/a/shaders/x"));
        assert!(picks_one(&dbs, "levels/a/shaders/x/y"));
    }
}
