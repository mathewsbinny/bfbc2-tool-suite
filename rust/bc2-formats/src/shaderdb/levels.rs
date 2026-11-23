//! What a level's shader database is asked for as a whole (round OR3b): the entry-name prefixes of the level cover
//! index (`shaderdb.cover_prefixes`) and the level scan (`shaderdb.level_scan`: per entry the template signature,
//! textures, vertex layouts and main-pass skinning kinds; the mesh parts) - Project OPFOR's `wake_shaders --clone`
//! asked these of every stock level's database, three renderers each, through Python queries.
//!
//! | Python (`shaderdb.py`) | here |
//! |---|---|
//! | `cover_prefixes(db)` | [`cover_prefixes`] |
//! | `entry_signatures(db)`, `entry_textures(db)`, `entry_layouts(db)`, `{n: {k[0x33] for main-pass keys}}` | [`scan`] |
//!
//! Entries are given in file order, a name listed twice once per listing (the caller's dictionary keeps the last
//! one's answer at the first one's place, as the Python's dictionaries did).

use std::collections::{BTreeSet, HashMap};

use super::ShaderDb;
use super::query::{block_constants, block_textures, lower, program_refs};

/// `cover_prefixes(db)`: for every entry, each `/`-joined leading part of its lower-case name (the whole name too)
/// counted once, in the order first met.
#[must_use]
pub fn cover_prefixes<'n>(names: impl IntoIterator<Item = &'n [u8]>) -> Vec<(Vec<u8>, u32)> {
    let mut at: HashMap<Vec<u8>, usize> = HashMap::new();
    let mut out: Vec<(Vec<u8>, u32)> = Vec::new();
    for name in names {
        let low = lower(name);
        let cuts = low
            .iter()
            .enumerate()
            .filter(|&(_, &c)| c == b'/')
            .map(|(i, _)| i)
            .chain([low.len()]);
        for end in cuts {
            let k = low.get(..end).unwrap_or_default();
            if let Some(&i) = at.get(k) {
                if let Some(row) = out.get_mut(i) {
                    row.1 += 1;
                }
            } else {
                at.insert(k.to_vec(), out.len());
                out.push((k.to_vec(), 1));
            }
        }
    }
    out
}

/// One entry of the level scan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryScan<'a> {
    /// The entry's name (latin-1).
    pub name: &'a [u8],
    /// Its parameter blocks' constant names, sorted, each once (`entry_signatures`' first part).
    pub constants: Vec<&'a [u8]>,
    /// The number of samplers its blocks bind textures to (`entry_signatures`' second part).
    pub samplers: usize,
    /// `entry_textures`: each (sampler, cube, name) once, sorted.
    pub textures: Vec<(u8, u8, &'a [u8])>,
    /// `entry_layouts`: the vertex layout GUIDs (key bytes 32-48), each once, sorted.
    pub layouts: Vec<&'a [u8]>,
    /// The skinning kinds (key byte 0x33) of its main-pass programs (key byte 0x30 zero), each once, sorted.
    pub kinds: Vec<u8>,
    /// The bodies (bytes 16-) of the database's layout records its layouts name, each once, sorted - the last
    /// record of a GUID (Project OPFOR's `wake_install.index_templates`, round `OR3c`).
    pub layout_records: Vec<&'a [u8]>,
}

/// Why a database cannot be scanned: an index past its section or a record too short (the Python queries raised
/// `IndexError` / `struct.error` there).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Malformed;

impl std::fmt::Display for Malformed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("a shader database index points outside its section or a record is short")
    }
}

impl std::error::Error for Malformed {}

/// A parameter block's constant names and texture records, read once.
struct Block<'a> {
    constants: Vec<&'a [u8]>,
    textures: Vec<(u8, u8, &'a [u8])>,
}

/// The level scan of `db`: every entry in file order ([`EntryScan`]).
///
/// # Errors
/// [`Malformed`]: a program, parameter block or key an entry's group names is not there, or a record is too short to
/// hold what is read of it.
pub fn scan<'a>(db: &'a ShaderDb<'_>) -> Result<Vec<EntryScan<'a>>, Malformed> {
    let be = db.big_endian;
    let groups: HashMap<&[u8], &[u16]> = db.groups.iter().map(|(g, idx)| (*g, idx.as_slice())).collect();
    let mut blocks: HashMap<usize, Block<'a>> = HashMap::new();
    let records: HashMap<&[u8], &[u8]> = db
        .layouts
        .iter()
        .map(|r| (r.get(..16).unwrap_or(r), r.get(16..).unwrap_or_default()))
        .collect();
    let mut out = Vec::with_capacity(db.entries.len());
    for e in &db.entries {
        let progs: &[u16] = groups.get(e.guid).copied().unwrap_or_default();
        let mut constants = BTreeSet::new();
        let mut samplers = BTreeSet::new();
        let mut textures = BTreeSet::new();
        let mut layouts = BTreeSet::new();
        let mut kinds = BTreeSet::new();
        for &i in progs {
            let i = usize::from(i);
            let refs = program_refs(db.programs.get(i).ok_or(Malformed)?, be).ok_or(Malformed)?;
            for k in [refs[2] as usize, refs[3] as usize] {
                let b = match blocks.entry(k) {
                    std::collections::hash_map::Entry::Occupied(o) => o.into_mut(),
                    std::collections::hash_map::Entry::Vacant(v) => {
                        let b: &'a [u8] = db.params.get(k).ok_or(Malformed)?;
                        v.insert(Block {
                            constants: block_constants(b, be).into_iter().map(|c| c.name).collect(),
                            textures: block_textures(b, be)
                                .into_iter()
                                .map(|t| (t.sampler, t.cube, t.name))
                                .collect(),
                        })
                    }
                };
                constants.extend(b.constants.iter().copied());
                samplers.extend(b.textures.iter().map(|t| t.0));
                textures.extend(b.textures.iter().copied());
            }
            let key: &'a [u8] = db.keys.get(i).ok_or(Malformed)?;
            layouts.insert(key.get(32.min(key.len())..48.min(key.len())).unwrap_or_default());
            if *key.get(0x30).ok_or(Malformed)? == 0 {
                kinds.insert(*key.get(0x33).ok_or(Malformed)?);
            }
        }
        let layout_records: BTreeSet<&[u8]> = layouts.iter().filter_map(|g| records.get(g).copied()).collect();
        out.push(EntryScan {
            name: &e.name,
            constants: constants.into_iter().collect(),
            samplers: samplers.len(),
            textures: textures.into_iter().collect(),
            layouts: layouts.into_iter().collect(),
            kinds: kinds.into_iter().collect(),
            layout_records: layout_records.into_iter().collect(),
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefixes_in_order_counted_once_per_entry() {
        let got = cover_prefixes([b"Objects/A/Mesh".as_slice(), b"objects/a", b"Objects/B", b"\xc9/x"]);
        let want: Vec<(Vec<u8>, u32)> = vec![
            (b"objects".to_vec(), 3),
            (b"objects/a".to_vec(), 2),
            (b"objects/a/mesh".to_vec(), 1),
            (b"objects/b".to_vec(), 1),
            (b"\xe9".to_vec(), 1),
            (b"\xe9/x".to_vec(), 1),
        ];
        assert_eq!(got, want);
    }

    #[test]
    fn empty_name_and_trailing_slash() {
        let got = cover_prefixes([b"".as_slice(), b"a/"]);
        assert_eq!(got, vec![(b"".to_vec(), 1), (b"a".to_vec(), 1), (b"a/".to_vec(), 1)]);
    }
}
