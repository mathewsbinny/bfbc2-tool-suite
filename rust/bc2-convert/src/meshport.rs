//! `bfbc2-bf1943-import/meshport.py`'s layout search (`Porter.pc_layout`, round F2b): which BFBC2 mesh part layout a
//! BF1943 part's vertices are written in - BFBC2's own part, a donor's part of the same material kind, any part with
//! the same elements, else the layout keeping the most of them. BFBC2's part signatures are computed once per index
//! (`signature`: the elements' PC format, usage and stream), each donor's standard parts listed once, and the last
//! resort remembered per (signature, donors), as the Python search keeps them.
//!
//! [`Search::Refused`] where the reference would raise something other than its `PortError` (a layout record
//! shorter than its element count, a part name without a folder).

use std::collections::HashMap;

/// An element as a signature holds it: (PC format, usage, stream).
pub type Elem = (u8, u8, u8);
/// A layout's signature.
pub type Sig = Vec<Elem>;

/// `mesh.PC_FORMAT`: console-only packed formats and their PC format.
fn pc_format(f: u8) -> u8 {
    match f {
        0x2B | 0x0D => 0x08,
        x => x,
    }
}

/// `signature(layout)`, or `None` where the reference raises (a record past the layout's bytes).
#[must_use]
pub fn signature(layout: &[u8]) -> Option<Sig> {
    let n = usize::from(*layout.get(64)?);
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let r = layout.get(4 * i..4 * i + 4)?;
        out.push((pc_format(*r.first()?), *r.get(1)?, *r.get(3)?));
    }
    Some(out)
}

/// `_subset(small, big)`: every element of small in big, as many times.
#[must_use]
pub fn subset(small: &[Elem], big: &[Elem]) -> bool {
    let mut pool: Vec<Elem> = big.to_vec();
    for e in small {
        match pool.iter().position(|x| x == e) {
            Some(i) => {
                pool.remove(i);
            }
            None => return false,
        }
    }
    true
}

/// Python's `str.lower()`.
fn lower(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        s.to_lowercase()
    }
}

/// `k.split('/')[-2]`, or `None` for a name without a folder.
fn folder_of(k: &str) -> Option<&str> {
    let mut it = k.rsplit('/');
    it.next()?;
    it.next()
}

/// `s.rsplit('_', 1)[-1]`.
fn last_word(s: &str) -> &str {
    s.rsplit('_').next().unwrap_or(s)
}

/// The search's answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Found {
    /// BFBC2 has this part (its index).
    Own(usize),
    /// A donor's part: (part index, donor index in the donors given, the part's folder).
    Donor(usize, usize, String),
    /// Any part with the same elements.
    Layout(usize),
    /// The layout keeping the most elements: (part index, elements left out, donor index or `None`, folder).
    Fallback(usize, usize, Option<usize>, Option<String>),
    /// No layout holds a part of its elements (the reference's `PortError`).
    Nothing,
}

/// The last resort's pick: (elements kept, part index, donor index or `None`).
type Best = (usize, usize, Option<usize>);

/// Not answered here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

/// BFBC2's part layouts, indexed once.
#[derive(Debug, Default)]
pub struct Index {
    names: Vec<String>,
    sigs: Vec<Sig>,
    by_name: HashMap<String, usize>,
    by_sig: HashMap<Sig, usize>,
    donors: HashMap<String, Vec<usize>>,
    fallback: HashMap<(Sig, Vec<String>), Option<Best>>,
}

impl Index {
    /// The index of `(name, layout)` pairs, the names sorted (`Porter.part_names`); `None` when a layout's signature
    /// cannot be read.
    #[must_use]
    pub fn new(parts: Vec<(String, Vec<u8>)>) -> Option<Self> {
        let mut ix = Self::default();
        for (n, l) in parts {
            let s = signature(&l)?;
            let i = ix.names.len();
            ix.by_name.insert(n.clone(), i);
            ix.by_sig.entry(s.clone()).or_insert(i);
            ix.names.push(n);
            ix.sigs.push(s);
        }
        Some(ix)
    }

    /// The part names.
    #[must_use]
    pub fn names(&self) -> &[String] {
        &self.names
    }

    fn donor_parts(&mut self, d: &str) -> Vec<usize> {
        if let Some(v) = self.donors.get(d) {
            return v.clone();
        }
        let pre = lower(&format!("{d}_lod"));
        let v: Vec<usize> = (0..self.names.len())
            .filter(|&i| {
                self.names
                    .get(i)
                    .is_some_and(|k| k.starts_with(&pre) && k.ends_with("/standard"))
            })
            .collect();
        self.donors.insert(d.to_owned(), v.clone());
        v
    }

    /// `pc_layout(name, lod, part, bf_layout, donors)`.
    ///
    /// # Errors
    /// [`Refused`] where the reference raises another exception.
    pub fn search(
        &mut self,
        name: &str,
        lod: i64,
        part: &str,
        bf_layout: &[u8],
        donors: &[String],
    ) -> Result<Found, Refused> {
        let sig = signature(bf_layout).ok_or(Refused)?;
        let own = lower(&format!("{name}_lod{lod}/{part}/standard"));
        if let Some(&i) = self.by_name.get(&own)
            && self.sigs.get(i) == Some(&sig)
        {
            return Ok(Found::Own(i));
        }
        let kind = if part.contains('_') {
            lower(last_word(part))
        } else {
            String::new()
        };
        for (di, d) in donors.iter().enumerate() {
            let hits: Vec<usize> = self
                .donor_parts(d)
                .into_iter()
                .filter(|&i| self.sigs.get(i) == Some(&sig))
                .collect();
            if hits.is_empty() {
                continue;
            }
            let mut pick = None;
            for &i in &hits {
                let f = folder_of(self.names.get(i).ok_or(Refused)?).ok_or(Refused)?;
                if last_word(f) == kind {
                    pick = Some(i);
                    break;
                }
            }
            let k = pick.or_else(|| hits.first().copied()).ok_or(Refused)?;
            let f = folder_of(self.names.get(k).ok_or(Refused)?).ok_or(Refused)?.to_owned();
            return Ok(Found::Donor(k, di, f));
        }
        if let Some(&k) = self.by_sig.get(&sig) {
            return Ok(Found::Layout(k));
        }
        let key = (sig.clone(), donors.to_vec());
        if !self.fallback.contains_key(&key) {
            let mut best: Option<Best> = None;
            for d in std::iter::once(None).chain(donors.iter().enumerate().map(Some)) {
                let pre = d.map(|(_, d)| lower(&format!("{d}_lod")));
                for (k2, name2) in self.names.iter().enumerate() {
                    if let Some(p) = &pre
                        && !name2.starts_with(p.as_str())
                    {
                        continue;
                    }
                    let s2 = self.sigs.get(k2).ok_or(Refused)?;
                    if !(subset(s2, &sig) && s2.iter().any(|e| e.1 == 1)) {
                        continue;
                    }
                    // `d and not best[2]`: a donor (a non-empty name) over the index's own pick
                    let d_truthy = d.is_some_and(|(_, d)| !d.is_empty());
                    let better = match best {
                        None => true,
                        Some((n, _, bd)) => {
                            let bd_falsy = bd.is_none_or(|i| donors.get(i).is_none_or(String::is_empty));
                            s2.len() > n || (s2.len() == n && d_truthy && bd_falsy)
                        }
                    };
                    if better {
                        best = Some((s2.len(), k2, d.map(|(i, _)| i)));
                    }
                }
            }
            self.fallback.insert(key.clone(), best);
        }
        let Some((n, k, d)) = self.fallback.get(&key).copied().flatten() else {
            return Ok(Found::Nothing);
        };
        let donor_named = d.is_some_and(|i| donors.get(i).is_some_and(|x| !x.is_empty()));
        let folder = if donor_named {
            Some(folder_of(self.names.get(k).ok_or(Refused)?).ok_or(Refused)?.to_owned())
        } else {
            None
        };
        Ok(Found::Fallback(k, sig.len() - n, d.filter(|_| donor_named), folder))
    }
}

// ------------------------------------------------------------------------------------------------ the Porter

/// What [`Porter`] reads of BF1943's source (`bf1943import.Source`: its entries by lower-case name).
pub trait Source {
    /// The entry's (type, name as the bundle holds it), or `None`.
    fn entry(&self, key: &str) -> Option<(&str, &str)>;
    /// The entry's bytes (`Source.data(key)`), or `None` where it cannot be read.
    fn data(&self, key: &str) -> Option<Vec<u8>>;
    /// Every entry name (lower case), in any order.
    fn keys(&self) -> Vec<&str>;
}

/// A port's failure: the reference's exception (its type's name and `str()`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortError {
    /// The exception type's name (`PortError`, `ConvertError`, `Refused`, `KeyError`, ...).
    pub kind: String,
    /// Its message.
    pub msg: String,
}

impl std::fmt::Display for PortError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.kind, self.msg)
    }
}

impl std::error::Error for PortError {}

fn perr<T>(kind: &str, msg: impl Into<String>) -> Result<T, PortError> {
    Err(PortError {
        kind: kind.to_owned(),
        msg: msg.into(),
    })
}

/// `Porter.port`'s result, as far as the parity tools read it: the mesh set's kind, each LOD's PC `MeshData`, the
/// PC mesh set, the converted textures (by name, in the source's name order) and each LOD's PC layouts per part.
#[derive(Debug, Clone, Default)]
pub struct Ported {
    /// The mesh set's type (`RigidMeshSet`, ...).
    pub kind: String,
    /// Each LOD's PC `MeshData`.
    pub lods: Vec<Vec<u8>>,
    /// The PC mesh set.
    pub meshset: Vec<u8>,
    /// The converted textures: (the source's entry name, PC bytes), in the source's name order.
    pub textures: Vec<(String, Vec<u8>)>,
    /// Per LOD, each part's PC layout record body.
    pub pc_layouts: Vec<Vec<Vec<u8>>>,
    /// What was not converted.
    pub notes: Vec<String>,
    /// Whether the mesh is skinned to the soldier skeleton (`plan['skinned_to_soldier']`).
    pub soldier: bool,
    /// Each part's layout choice, by part name in the order first met (`plan['parts']`): (part, how, donor).
    pub plan_parts: Vec<(String, String, Donor)>,
    /// The mesh set's material names, sorted (`plan['materials']`).
    pub materials: Vec<String>,
}

/// A layout's donor: (the donor mesh, the part's folder), or none.
pub type Donor = Option<(String, String)>;

/// The soldier skeleton.
pub const SOLDIER: &str = "characters/skeletons/ske01";

/// `meshport.Porter` (round PR1b-1d: `bfbc2 portparity`): a BF1943 model in BFBC2's PC formats - every LOD's
/// `MeshData` (each part's vertices in the PC layout the search picks: BFBC2's own part, a donor's, any with the same
/// elements), the mesh set, the textures of its folder. A soldier's palettes and mesh-set bones are renumbered onto
/// BFBC2's skeleton. The skeleton poses and the shader-database plan are not made here (no caller reads them).
pub struct Porter<'a> {
    src: &'a dyn Source,
    ix: &'a crate::meshview::Index,
    layouts: Option<HashMap<String, Vec<u8>>>,
    bones: Option<HashMap<u32, u32>>,
    entry_keys: Option<Vec<String>>,
    search: Option<Index>,
}

/// Python's `repr` of a signature (a tuple of 3-tuples).
fn sig_repr(s: &Sig) -> String {
    let items: Vec<String> = s.iter().map(|(a, b, c)| format!("({a}, {b}, {c})")).collect();
    match items.as_slice() {
        [one] => format!("({one},)"),
        _ => format!("({})", items.join(", ")),
    }
}

impl<'a> Porter<'a> {
    /// `Porter(source, ix)`.
    #[must_use]
    pub fn new(src: &'a dyn Source, ix: &'a crate::meshview::Index) -> Self {
        Self {
            src,
            ix,
            layouts: None,
            bones: None,
            entry_keys: None,
            search: None,
        }
    }

    fn data(&self, key: &str) -> Result<Vec<u8>, PortError> {
        match self.src.data(key) {
            Some(d) => Ok(d),
            None if self.src.entry(key).is_none() => perr("KeyError", format!("'{key}'")),
            None => perr(
                "Bf1943ImportError",
                format!("{key}: the BF1943 bundle does not read as when opened"),
            ),
        }
    }

    /// `layouts()`: BF1943's part layouts, from every level's shader database (the first one's wins).
    fn layouts(&mut self) -> Result<&HashMap<String, Vec<u8>>, PortError> {
        if self.layouts.is_none() {
            let mut keys: Vec<&str> = self.src.keys();
            keys.sort_unstable();
            let mut out: HashMap<String, Vec<u8>> = HashMap::new();
            for k in keys {
                if k.starts_with("levels/") && self.src.entry(k).is_some_and(|(t, _)| t == "Dx9ShaderDatabase") {
                    for (n, rec) in crate::meshconv::console_layouts(&self.data(k)?) {
                        let n: String = n.iter().map(|&c| char::from(c)).collect();
                        out.entry(n).or_insert(rec);
                    }
                }
            }
            self.layouts = Some(out);
        }
        Ok(self.layouts.get_or_insert_with(HashMap::new))
    }

    fn bones(&mut self) -> Result<HashMap<u32, u32>, PortError> {
        if let Some(b) = &self.bones {
            return Ok(b.clone());
        }
        let key = format!("{SOLDIER}.res");
        let src = self.data(&key)?;
        let Some(dst) = self.ix.read(&key) else {
            return perr("granny.GrannyError", "target skeleton: no data");
        };
        let b = crate::granny::bone_map(&src, &dst).map_err(|e| PortError {
            kind: "GrannyError".into(),
            msg: e.0,
        })?;
        self.bones = Some(b.clone());
        Ok(b)
    }

    fn search(&mut self) -> Result<&mut Index, PortError> {
        if self.search.is_none() {
            let parts: Vec<(String, Vec<u8>)> = self
                .ix
                .part_names()
                .into_iter()
                .map(|k| (k.to_owned(), self.ix.layout(k).unwrap_or_default().to_vec()))
                .collect();
            let Some(ix) = Index::new(parts) else {
                return perr("IndexError", "a BFBC2 part layout shorter than its element count");
            };
            self.search = Some(ix);
        }
        Ok(self.search.get_or_insert_with(Index::default))
    }

    /// `pc_layout(name, lod, part, bf_layout, donors)`: the PC layout body.
    fn pc_layout(
        &mut self,
        name: &str,
        lod: i64,
        part: &str,
        bf: &[u8],
        donors: &[String],
    ) -> Result<(Vec<u8>, String, Donor), PortError> {
        let ix = self.ix;
        let s = self.search()?;
        let found = s.search(name, lod, part, bf, donors).map_err(|_| PortError {
            kind: "IndexError".into(),
            msg: "tuple index out of range".into(),
        })?;
        let name_of = |k: usize| s.names().get(k).cloned().unwrap_or_default();
        let (k, how, donor) = match found {
            Found::Own(k) => (k, "BFBC2 has this part".to_owned(), None),
            Found::Donor(k, d, folder) => (
                k,
                "donor".to_owned(),
                Some((donors.get(d).cloned().unwrap_or_default(), folder)),
            ),
            Found::Layout(k) => (k, format!("layout of {}", name_of(k)), None),
            Found::Fallback(k, left, d, folder) => (
                k,
                format!("layout of {} ({left} element(s) left out)", name_of(k)),
                d.map(|d| (donors.get(d).cloned().unwrap_or_default(), folder.unwrap_or_default())),
            ),
            Found::Nothing => {
                let sig = signature(bf).unwrap_or_default();
                return perr(
                    "PortError",
                    format!(
                        "{name} LOD {lod} {part}: no BFBC2 layout holds a part of its elements {}",
                        sig_repr(&sig)
                    ),
                );
            }
        };
        let n = s.names().get(k).cloned().unwrap_or_default();
        Ok((ix.layout(&n).unwrap_or_default().to_vec(), how, donor))
    }

    /// The source's entry names starting with `folder`, in name order.
    fn entries_under(&mut self, folder: &str) -> Vec<String> {
        let keys = self.entry_keys.get_or_insert_with(|| {
            let mut k: Vec<String> = self.src.keys().into_iter().map(str::to_owned).collect();
            k.sort_unstable();
            k
        });
        let i = keys.partition_point(|k| k.as_str() < folder);
        keys.get(i..)
            .unwrap_or_default()
            .iter()
            .take_while(|k| k.starts_with(folder))
            .cloned()
            .collect()
    }

    /// `port(name)` (no donors, no renaming).
    ///
    /// # Errors
    /// The reference's exception where it raises.
    pub fn port(&mut self, name: &str) -> Result<Ported, PortError> {
        self.port_with(name, false)
    }

    /// `port(name)`; `bare`: a mesh skinned to no soldier whatever its partition says (Project OPFOR's
    /// `port_mesh(..., bare=True)`: the partition is not read).
    ///
    /// # Errors
    /// The reference's exception where it raises.
    #[allow(
        clippy::too_many_lines,
        clippy::many_single_char_names,
        reason = "Porter.port, step for step (its names)"
    )]
    pub fn port_with(&mut self, name: &str, bare: bool) -> Result<Ported, PortError> {
        use bc2_formats::meshdata::{self, Kind};
        let low = name.to_lowercase();
        let Some((kind, _)) = self.src.entry(&format!("{low}.res")) else {
            return perr("PortError", format!("{name}: no mesh set in the BF1943 source"));
        };
        let kind = kind.to_owned();
        let soldier = if bare {
            false
        } else {
            let xml = bc2_formats::dbx::to_xml(&self.data(&format!("{low}.dbx"))?).map_err(|e| PortError {
                kind: "ValueError".into(),
                msg: e.to_string(),
            })?;
            xml.to_lowercase().contains(&format!("{SOLDIER}/"))
        };
        let bones = if soldier { Some(self.bones()?) } else { None };
        let mut out = Ported {
            kind: kind.clone(),
            soldier,
            ..Ported::default()
        };
        for lod in 0..16i64 {
            let key = format!("{low}_lod{lod}_data.res");
            if self.src.entry(&key).is_none() {
                break;
            }
            let b = self.data(&key)?;
            let Ok(m) = meshdata::parse_mesh(&b, true) else {
                return perr(
                    "Refused",
                    format!(
                        "mesh data: {} bytes the header reader does not read (cut short or damaged)",
                        b.len()
                    ),
                );
            };
            let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(m.parts.len());
            for p in &m.parts {
                let pn: String = p.name.iter().map(|&c| char::from(c)).collect();
                let lkey = format!("{name}_lod{lod}/{pn}/standard").to_lowercase();
                let Some(bf) = self.layouts()?.get(&lkey).cloned() else {
                    return perr(
                        "PortError",
                        format!("{name} LOD {lod} {pn}: no layout in BF1943's shader databases"),
                    );
                };
                let (pc, how, donor) = self.pc_layout(name, lod, &pn, &bf, &[])?;
                if !out.plan_parts.iter().any(|(p, _, _)| *p == pn) {
                    out.plan_parts.push((pn.clone(), how, donor));
                }
                pairs.push((bf, pc));
            }
            let refs: Vec<(&[u8], &[u8])> = pairs.iter().map(|(a, c)| (a.as_slice(), c.as_slice())).collect();
            let pc = crate::meshconv::convert(&b, &refs, bones.as_ref()).map_err(|e| match e {
                crate::meshconv::Refusal::Error(m) => PortError {
                    kind: "ConvertError".into(),
                    msg: m,
                },
                crate::meshconv::Refusal::Reference => PortError {
                    kind: "Refused".into(),
                    msg: "mesh data: a file or layouts the converter does not read (cut short or damaged)".into(),
                },
            })?;
            out.lods.push(pc);
            out.pc_layouts.push(pairs.into_iter().map(|(_, c)| c).collect());
        }
        if out.lods.is_empty() {
            return perr("PortError", format!("{name}: no LODs in the BF1943 source"));
        }
        let Some(k) = Kind::of(&kind) else {
            return perr(
                "ConvertError",
                format!("{kind}: only RigidMeshSet, CompositeMeshSet, SkinnedMeshSet, TreeMeshSet are converted"),
            );
        };
        let raw = self.data(&format!("{low}.res"))?;
        let flag = (k == Kind::Rigid).then_some(false);
        let Ok(mut s) = meshdata::parse_set(&raw, k, true, flag) else {
            return perr(
                "Refused",
                format!("{kind}: a mesh set the reader does not read (cut short or damaged)"),
            );
        };
        let mut materials: Vec<String> = s
            .lods
            .iter()
            .flat_map(|l| l.sets.iter().flat_map(|(_, shs)| shs.iter()))
            .map(|sh| sh.iter().map(|&c| char::from(c)).collect::<String>())
            .collect();
        materials.sort();
        materials.dedup();
        out.materials = materials;
        for (l, pc) in s.lods.iter_mut().zip(&out.lods) {
            let Ok(h) = meshdata::parse_mesh(pc, false) else {
                return perr("Refused", "mesh data: a converted LOD the header reader does not read");
            };
            l.size = h.sizes[0].wrapping_add(h.sizes[1]).wrapping_add(12);
            if k == Kind::Skinned
                && let (Some(map), Some((ids, _))) = (&bones, l.bones.as_mut())
            {
                for x in ids.iter_mut() {
                    *x = *map.get(x).ok_or_else(|| PortError {
                        kind: "KeyError".into(),
                        msg: x.to_string(),
                    })?;
                }
            }
        }
        let folder = format!("{}/", low.rsplit_once('/').map_or(low.as_str(), |(f, _)| f));
        let mut textures: Vec<(String, Vec<u8>)> = Vec::new();
        for key in self.entries_under(&folder) {
            let Some((t, n)) = self.src.entry(&key).map(|(t, n)| (t.to_owned(), n.to_owned())) else {
                continue;
            };
            if t != "XenonTexture" {
                continue;
            }
            match crate::texture::convert(&self.data(&key)?) {
                Ok(pc) => match textures.iter_mut().find(|(x, _)| *x == n) {
                    Some(slot) => slot.1 = pc,
                    None => textures.push((n, pc)),
                },
                Err(e) => out.notes.push(format!("{n}: not converted ({e})")),
            }
        }
        out.textures = textures;
        out.meshset = meshdata::write_set(&s, k).map_err(|_| PortError {
            kind: "Refused".into(),
            msg: format!("{kind}: a mesh set the writer does not write"),
        })?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[allow(clippy::indexing_slicing)]
    fn layout(els: &[(u8, u8, u8)]) -> Vec<u8> {
        let mut l = vec![0u8; 0x49];
        for (i, (f, u, s)) in els.iter().enumerate() {
            l[4 * i..4 * i + 4].copy_from_slice(&[*f, *u, 0, *s]);
        }
        l[64] = els.len() as u8;
        l
    }

    #[test]
    #[allow(clippy::indexing_slicing, clippy::unwrap_used)]
    fn searches() {
        let a = layout(&[(2, 1, 0), (8, 3, 0)]);
        let b = layout(&[(2, 1, 0)]);
        let mut ix = Index::new(vec![
            ("donor_lod0/body_metal/standard".into(), a.clone()),
            ("x_lod0/p/standard".into(), b.clone()),
        ])
        .unwrap();
        assert_eq!(
            ix.search("m", 0, "Body_Metal", &a, &["donor".into()]),
            Ok(Found::Donor(0, 0, "body_metal".into()))
        );
        assert_eq!(ix.search("x", 0, "p", &b, &[]), Ok(Found::Own(1)));
        let c = layout(&[(2, 1, 0), (9, 5, 1)]);
        assert_eq!(ix.search("m", 0, "q", &c, &[]), Ok(Found::Fallback(1, 1, None, None)));
        assert!(!subset(&[(1, 1, 1), (1, 1, 1)], &[(1, 1, 1)]));
    }
}
