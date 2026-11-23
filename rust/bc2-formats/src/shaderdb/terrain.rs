//! A terrain's shader entries cloned under another terrain's name (round TG2; spec TGEN-REQ-4, TGEN-BIOME-4,
//! TEDIT-SH-1 - 3): every entry a donor terrain's visual terrain and visual water name (`Terrains/<D>/<D>/
//! PatchMaskShaders/<layers>`, `.../UgMaskShaders/<layers>`, `Terrains/<D>/Shaders/<cache or water>`) added to a
//! level's database again as `Terrains/<T>/...`, its programs, keys, shaders and vertex layouts the donor's (the
//! suite compiles no shader: TEDIT-SH-1), its parameter blocks with the terrain's own textures renamed (the
//! colour / occlusion map, LIGHT-BAKE-2). The merge is [`super::merge::Merger::clone_entry`] (round `OR3a`, OPFOR's
//! Wake Island patches).
//!
//! DICE did the same for its twin terrains: `SP_008_B`'s patch entries are `SP_008`'s layer combinations under
//! `Terrains/SP_008_B/SP_008_B/...` with the same program keys (23 of 23 shared combinations, Dx9 / 10 / 11) and its
//! own occlusion map (`Terrains/SP_008_B/Textures/sp08_ambientocclusion`, 18 of 18 references).
//!
//! One function for the generated terrain (`bc2-terragen`) and the editor's paint of a new layer combination
//! (E5-rest: the same clone with the level's own terrain as donor and target).

use std::collections::BTreeMap;

use bumpalo::Bump;

use super::merge::{Kind, Merger, Store};
use super::{levels, open};
use crate::visual_terrain;

struct Arena<'a>(&'a Bump);

impl<'a> Store<'a> for Arena<'a> {
    fn keep(&self, b: &[u8]) -> &'a [u8] {
        self.0.alloc_slice_copy(b)
    }
}

/// What [`clone_terrain`] did to one database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cloned {
    /// The database's bytes after the clone.
    pub bytes: Vec<u8>,
    /// (donor entry, new entry, programs) per entry added, in the order asked.
    pub added: Vec<(String, String, usize)>,
    /// Entries asked for that the database had already under the new name (left as they are).
    pub kept: Vec<String>,
    /// Entries asked for that the donor does not have.
    pub missing: Vec<String>,
    /// Each texture rename and how many entries it was made in.
    pub retextured: BTreeMap<String, usize>,
}

/// The donor's entries of terrain `from`'s patch and undergrowth mask shaders (`Terrains/<from>/<from>/
/// PatchMaskShaders/`, `.../UgMaskShaders/`, any case), in file order.
#[must_use]
pub fn mask_entries(db: &super::ShaderDb<'_>, from: &str) -> Vec<String> {
    let pre = format!("terrains/{0}/{0}/", from.to_ascii_lowercase());
    db.entries
        .iter()
        .map(|e| e.name.iter().map(|&b| char::from(b)).collect::<String>())
        .filter(|n| {
            let low = n.to_ascii_lowercase();
            low.starts_with(&pre) && (low.contains("/patchmaskshaders/") || low.contains("/ugmaskshaders/"))
        })
        .collect()
}

fn kind_of(o: &open::Opened<'_>, dx: u32) -> Kind {
    Kind {
        dx,
        big: o.big_endian,
        key_size: o.db.key_size(),
        version: o.db.version,
    }
}

/// Clones `entries` of the `donor` database (`dx`: 9, 10 or 11, as the file's name says) into `base` under terrain
/// `to`'s name ([`visual_terrain::rename`] of terrain `from`'s), with `textures` (`[(old, new)]`, whole names as
/// the parameter blocks spell them) renamed in each entry that samples them. `base` and `donor` may be the same
/// file (a level built on the donor). An entry whose new name the base has is left as it is; an entry the donor
/// lacks, or one not named for terrain `from`, is reported and skipped.
///
/// # Errors
/// A database does not read, or the merge refuses a clone (a texture name longer than its field).
pub fn clone_terrain(
    base: &[u8],
    donor: &[u8],
    dx: u32,
    entries: &[String],
    from: &str,
    to: &str,
    textures: &[(String, String)],
) -> Result<Cloned, String> {
    let mut pairs = Vec::with_capacity(entries.len());
    let mut unnamed = Vec::new();
    for name in entries {
        match visual_terrain::rename(name, from, to) {
            Some(new) => pairs.push((name.clone(), new)),
            None => unnamed.push(name.clone()),
        }
    }
    let mut out = clone_entries(base, donor, dx, &pairs, textures)?;
    out.missing.extend(unnamed);
    Ok(out)
}

/// Clones each (donor entry, new name) of `pairs` from the `donor` database into `base` ([`clone_terrain`] without
/// the terrain's renaming: E5-rest's paint of a new layer combination clones an entry of the level's own terrain
/// under the new combination's name). `textures` as [`clone_terrain`]'s.
///
/// # Errors
/// A database does not read, or the merge refuses a clone (a texture name longer than its field).
pub fn clone_entries(
    base: &[u8],
    donor: &[u8],
    dx: u32,
    pairs: &[(String, String)],
    textures: &[(String, String)],
) -> Result<Cloned, String> {
    let layout = if dx == 11 { 10 } else { dx };
    let bump = Bump::new();
    let store = Arena(&bump);
    let b = open::open(base, Some(layout), Some(false), "the base database")?;
    let d = open::open(donor, Some(layout), Some(false), "the donor database")?;
    let (bk, dk) = (kind_of(&b, dx), kind_of(&d, dx));
    let scan = levels::scan(&d.db).map_err(|e| e.to_string())?;
    let used: BTreeMap<String, Vec<String>> = scan
        .iter()
        .map(|e| {
            (
                e.name.iter().map(|&c| char::from(c)).collect(),
                e.textures
                    .iter()
                    .map(|t| t.2.iter().map(|&c| char::from(c)).collect())
                    .collect(),
            )
        })
        .collect();
    let mut m = Merger::new(b.db, bk, Vec::new(), false).map_err(|e| e.to_string())?;
    let mut out = Cloned::default();
    for (name, new) in pairs {
        let Some(tex) = used.get(name) else {
            out.missing.push(name.clone());
            continue;
        };
        if m.has_entry(
            &new.chars()
                .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
                .collect::<Vec<u8>>(),
        ) {
            out.kept.push(new.clone());
            continue;
        }
        let ren: Vec<(String, String)> = textures.iter().filter(|(o, _)| tex.contains(o)).cloned().collect();
        let n = m
            .clone_entry(&store, &d.db, &dk, name, new, &ren, "the donor")
            .map_err(|e| e.to_string())?;
        for (o, nn) in &ren {
            *out.retextured.entry(format!("{o} -> {nn}")).or_insert(0) += 1;
        }
        out.added.push((name.clone(), new.clone(), n));
    }
    out.bytes = m.finish().map_err(|e| e.to_string())?;
    Ok(out)
}
