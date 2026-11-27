//! The physics materials of OPFOR's WW2 ports (round OR4, "OPFOR in Rust" stage 4: Project OPFOR's `physmat.py`):
//! a destructible part's material (the `HavokAsset` partition's `PartInfoDatas[].MaterialName` and the physics
//! resource's per-part byte, kept in step - the game reads the part material for the part's damage) and a static
//! model group member's face materials (its detail mesh's subpart storages - the hit material of a group member).
//! The bytes are [`crate::havokport`]'s; which slot the game reads: the suite's `spec/` MAT-SLOT rules.

#![allow(
    clippy::indexing_slicing,
    clippy::type_complexity,
    clippy::implicit_hasher,
    clippy::too_many_lines,
    clippy::map_unwrap_or,
    reason = "part and field indexes found by position in the lists they index; the Python's shapes and its one-piece set_parts kept for review"
)]

use std::collections::{BTreeMap, HashMap};

use bc2_formats::dbx::{self, Body, DbxFile, Element};

use crate::havokport::{self, Port};

/// Why a physics-material edit stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhysmatError {
    /// The files are not as the edit wants them (Python's `SystemExit` with this message).
    Exit(String),
    /// A partition without the fields every `HavokAsset` has (Python's `AssertionError`).
    Assert(String),
    /// The partition does not read (Python's `ValueError`).
    Dbx(String),
    /// The physics resource's material slots are not the shape `havokport` reads and writes (`MaterialError`).
    Havok(String),
}

impl std::fmt::Display for PhysmatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Exit(m) | Self::Assert(m) | Self::Dbx(m) | Self::Havok(m) => f.write_str(m),
        }
    }
}

impl std::error::Error for PhysmatError {}

/// A result of this module.
pub type Result<T> = std::result::Result<T, PhysmatError>;

fn hv(e: Port) -> PhysmatError {
    PhysmatError::Havok(match e {
        Port::Error(m) | Port::Material(m) => m,
        Port::Refused => "the physics resource's materials: a layout the port does not take".to_owned(),
    })
}

fn exit<T>(m: String) -> Result<T> {
    Err(PhysmatError::Exit(m))
}

fn parse(data: &[u8]) -> Result<DbxFile> {
    dbx::parse(data).map_err(|e| PhysmatError::Dbx(e.to_string()))
}

/// The index of `e`'s first child named `name`.
fn child_at(f: &DbxFile, e: &Element, name: &str) -> Option<usize> {
    e.children().iter().position(|c| f.attr(c, "name") == Some(name))
}

/// The index of the partition's first instance among its children.
fn first_instance(f: &DbxFile) -> Option<usize> {
    let p = f.partition()?;
    p.children().iter().position(|c| f.tag(c) == "instance")
}

fn children_mut(e: &mut Element) -> Option<&mut Vec<Element>> {
    match &mut e.body {
        Body::Container(c) => Some(c),
        _ => None,
    }
}

/// A part entry's `Name` / `MaterialName` text (every part entry has both).
fn field_text<'a>(f: &'a DbxFile, e: &Element, name: &str) -> Result<&'a str> {
    let c = f
        .child(e, name)
        .ok_or_else(|| PhysmatError::Assert(format!("{} without {name}", f.tag(e))))?;
    Ok(f.text(c))
}

/// `havok_lists(data)`: (the dbx `MaterialMappings` names, `[(part name, material name)]`) of a `HavokAsset`
/// partition (BFBC2's `PartInfoDatas`, BF1943's `PartMaterialDatas`).
///
/// # Errors
/// A partition that does not read, or without those fields.
pub fn havok_lists(data: &[u8]) -> Result<(Vec<String>, Vec<(String, String)>)> {
    let f = parse(data)?;
    let i = f
        .instances()
        .next()
        .ok_or_else(|| PhysmatError::Dbx("a partition without instances".to_owned()))?;
    let mm = f.child(i, "MaterialMappings");
    let pid = f.child(i, "PartInfoDatas").or_else(|| f.child(i, "PartMaterialDatas"));
    let (Some(mm), Some(pid)) = (mm, pid) else {
        return Err(PhysmatError::Assert(
            "a HavokAsset without MaterialMappings / part materials".to_owned(),
        ));
    };
    let maps = mm.children().iter().map(|c| f.text(c).to_owned()).collect();
    let mut parts = Vec::new();
    for c in pid.children() {
        parts.push((
            field_text(&f, c, "Name")?.to_owned(),
            field_text(&f, c, "MaterialName")?.to_owned(),
        ));
    }
    Ok((maps, parts))
}

/// `list_hashes(res, data)`: `{material name (lower case): hash}` of a physics resource - its wrapper's material list
/// beside its dbx `MaterialMappings` (the same order in DICE's files).
///
/// # Errors
/// As [`havok_lists`] and `havokport`; the two lists of different lengths.
pub fn list_hashes(res: &[u8], data: &[u8]) -> Result<HashMap<String, u32>> {
    let (maps, _) = havok_lists(data)?;
    let (hashes, _) = havokport::part_materials(res).map_err(hv)?;
    if maps.len() != hashes.len() {
        return exit(format!(
            "{} MaterialMappings beside {} wrapper materials",
            maps.len(),
            hashes.len()
        ));
    }
    Ok(maps.iter().map(|m| m.to_lowercase()).zip(hashes).collect())
}

/// A part whose material changed: (part, old material, new material).
pub type PartChange = (String, String, String);

/// `set_parts(res, data, want, hashes)`: the resource and its `HavokAsset` partition with each named part on its new
/// material (`want` `{part: material}`, every part named must exist; a material the file lacks is appended to the
/// dbx `MaterialMappings`, the wrapper's material list and the packfile's `HavokMaterials` - DICE keeps the three
/// equal -, its hash from `hashes` `{lower name: hash}`). Returns (res, data, the changes), the parts in name order.
///
/// # Errors
/// The two slots disagree before the edit, a part is missing, a new material has no hash, or the files do not read.
pub fn set_parts(
    res: &[u8],
    data: &[u8],
    want: &BTreeMap<String, String>,
    hashes: &HashMap<String, u32>,
) -> Result<(Vec<u8>, Vec<u8>, Vec<PartChange>)> {
    let mut f = parse(data)?;
    let ii = first_instance(&f).ok_or_else(|| PhysmatError::Dbx("a partition without instances".to_owned()))?;
    let (mmi, pidi) = {
        let inst = f.partition().and_then(|p| p.children().get(ii));
        let inst = inst.ok_or_else(|| PhysmatError::Dbx("no instance".to_owned()))?;
        match (
            child_at(&f, inst, "MaterialMappings"),
            child_at(&f, inst, "PartInfoDatas"),
        ) {
            (Some(a), Some(b)) => (a, b),
            _ => {
                return Err(PhysmatError::Assert(
                    "a HavokAsset without MaterialMappings / PartInfoDatas".to_owned(),
                ));
            }
        }
    };
    // the read side, on a copy of the two fields (the strings stay the file's)
    let (mm, pid) = {
        let inst = &f.partition().map_or_default(Element::children)[ii];
        (inst.children()[mmi].clone(), inst.children()[pidi].clone())
    };
    let mut maps: Vec<String> = mm.children().iter().map(|c| f.text(c).to_owned()).collect();
    let (old_hashes, idx8) = havokport::part_materials(res).map_err(hv)?;
    if maps.len() != old_hashes.len() {
        return exit(format!(
            "{} MaterialMappings beside {} wrapper materials",
            maps.len(),
            old_hashes.len()
        ));
    }
    let parts = pid.children();
    if parts.len() != idx8.len() {
        return exit(format!(
            "{} PartInfoDatas beside {} wrapper parts",
            parts.len(),
            idx8.len()
        ));
    }
    let mut idx: Vec<i64> = idx8.iter().map(|&i| i64::from(i)).collect();
    for (p, &i) in parts.iter().zip(&idx8) {
        let mat = field_text(&f, p, "MaterialName")?;
        let Some(m) = maps.get(usize::from(i)) else {
            return Err(PhysmatError::Havok(format!(
                "part index {i} past the {} materials",
                maps.len()
            )));
        };
        if mat.to_lowercase() != m.to_lowercase() {
            return exit(format!("part {}: dbx {mat}, wrapper {m}", field_text(&f, p, "Name")?));
        }
    }
    let mut names: HashMap<String, usize> = HashMap::new();
    for (k, p) in parts.iter().enumerate() {
        names.insert(field_text(&f, p, "Name")?.to_owned(), k);
    }
    let mut missing: Vec<&String> = want.keys().filter(|k| !names.contains_key(*k)).collect();
    missing.sort_unstable();
    if !missing.is_empty() {
        let mut have: Vec<&String> = names.keys().collect();
        have.sort_unstable();
        return exit(format!(
            "no such parts: {} (the file has {})",
            py_list(&missing),
            py_list(&have)
        ));
    }
    let mut new_hashes = old_hashes.clone();
    let mut changes = Vec::new();
    let mut new_items: Vec<String> = Vec::new();
    let mut texts: Vec<(usize, String)> = Vec::new();
    for (part, mat) in want {
        let mut low: Vec<String> = maps.iter().map(|m| m.to_lowercase()).collect();
        let ml = mat.to_lowercase();
        if !low.contains(&ml) {
            let Some(&h) = hashes.get(&ml) else {
                return exit(format!("{mat}: not in the file's list and no hash for it"));
            };
            if mm.children().is_empty() {
                return Err(PhysmatError::Assert(
                    "an empty MaterialMappings: no item to copy".to_owned(),
                ));
            }
            new_items.push(ml.clone());
            maps.push(ml.clone());
            new_hashes.push(h);
            low.push(ml.clone());
        }
        let k = names[part];
        let cur = field_text(&f, &parts[k], "MaterialName")?.to_owned();
        if &cur != mat {
            changes.push((part.clone(), cur, mat.clone()));
            texts.push((k, mat.clone()));
        }
        idx[k] = low.iter().position(|m| *m == ml).map_or(-1, |p| p as i64);
    }
    // the edits on the file: new MaterialMappings items (copies of the last, without children), new texts
    let mut new_ids = Vec::new();
    for t in &new_items {
        new_ids.push(f.intern(t));
    }
    let text_ids: Vec<(usize, u32)> = texts.iter().map(|(k, t)| (*k, f.intern(t))).collect();
    let strings = f.strings.clone();
    let is_field = |e: &Element, name: &str| {
        e.attributes
            .iter()
            .find(|(a, _)| strings.get(*a as usize).map(String::as_str) == Some("name"))
            .is_some_and(|(_, v)| strings.get(*v as usize).map(String::as_str) == Some(name))
    };
    let Some(inst) = f.roots.first_mut().and_then(children_mut).and_then(|c| c.get_mut(ii)) else {
        return Err(PhysmatError::Dbx("no instance".to_owned()));
    };
    let Some(fields) = children_mut(inst) else {
        return Err(PhysmatError::Dbx("an instance without fields".to_owned()));
    };
    if let Some(items) = fields.get_mut(mmi).and_then(children_mut) {
        for id in new_ids {
            if let Some(last) = items.last() {
                // Python's copy of the last item without its children, its text set (a text item: what
                // MaterialMappings holds; any other kind keeps its value)
                let mut c = last.clone();
                c.body = match c.body {
                    Body::Text(_) => Body::Text(id),
                    Body::Container(_) => Body::Container(Vec::new()),
                    other => other,
                };
                items.push(c);
            }
        }
    }
    if let Some(ps) = fields.get_mut(pidi).and_then(children_mut) {
        for (k, id) in text_ids {
            if let Some(cs) = ps.get_mut(k).and_then(children_mut)
                && let Some(c) = cs.iter_mut().find(|c| is_field(c, "MaterialName"))
                && let Body::Text(_) = c.body
            {
                c.body = Body::Text(id);
            }
        }
    }
    let mut out = havokport::with_part_materials(res, &idx, &new_hashes).map_err(hv)?;
    if new_hashes.len() > old_hashes.len() {
        let faces = havokport::havok_materials(res).map_err(hv)?;
        let head: Vec<String> = maps.iter().take(old_hashes.len()).map(|m| m.to_lowercase()).collect();
        let fl: Vec<String> = faces.iter().map(|m| m.to_lowercase()).collect();
        if fl != head {
            let shown: Vec<&String> = maps.iter().take(old_hashes.len()).collect();
            let fs: Vec<&String> = faces.iter().collect();
            return exit(format!(
                "HavokMaterials {} is not the material list {}: not grown",
                py_list(&fs),
                py_list(&shown)
            ));
        }
        let mut all = faces;
        all.extend(maps.iter().skip(old_hashes.len()).map(|m| m.to_lowercase()));
        out = havokport::with_havok_materials(&out, &all).map_err(hv)?;
    }
    let written = dbx::write(&f).map_err(|e| PhysmatError::Dbx(e.to_string()))?;
    Ok((out, written, changes))
}

/// A face material that changed: (member, subpart, old, new).
pub type FaceChange = (String, usize, String, String);

/// `group_faces(res, member_types, remap)`: a static model group's physics with its members' detail faces remapped.
/// `member_types`: the group's member partitions in order of first use (the `CompositeHavokAsset`'s assets: one
/// detail body each, in that order); `remap`: `{member partition's last name: {old face: new face}}` (names as the
/// packfile's `HavokMaterials` spell them). Returns (res, the changes).
///
/// # Errors
/// The detail bodies are not one per member, a new face is not in the group's `HavokMaterials`, or `havokport`'s.
pub fn group_faces(
    res: &[u8],
    member_types: &[String],
    remap: &HashMap<String, HashMap<String, String>>,
) -> Result<(Vec<u8>, Vec<FaceChange>)> {
    let mats = havokport::havok_materials(res).map_err(hv)?;
    let details: Vec<(usize, Vec<Vec<u32>>)> = havokport::bodies(res)
        .map_err(hv)?
        .into_iter()
        .enumerate()
        .filter_map(|(k, (_, m))| m.map(|m| (k, m)))
        .collect();
    if details.len() != member_types.len() {
        return exit(format!(
            "{} detail bodies for {} member types",
            details.len(),
            member_types.len()
        ));
    }
    let mut changes = Vec::new();
    let mut edits: Vec<(i64, Vec<Vec<i64>>)> = Vec::new();
    for ((bi, subparts), member) in details.iter().zip(member_types) {
        let last = member.rsplit('/').next().unwrap_or(member);
        let Some(rule) = remap.get(last).filter(|r| !r.is_empty()) else {
            continue;
        };
        let mut new: Vec<Vec<i64>> = Vec::new();
        for (s, lst) in subparts.iter().enumerate() {
            let mut out = Vec::new();
            for &x in lst {
                let mut x = i64::from(x);
                let name = usize::try_from(x).ok().and_then(|i| mats.get(i));
                let Some(name) = name else {
                    return Err(PhysmatError::Havok(format!(
                        "face material {x} past the {} HavokMaterials",
                        mats.len()
                    )));
                };
                if let Some(to) = rule.get(name) {
                    let Some(p) = mats.iter().position(|m| m == to) else {
                        let ms: Vec<&String> = mats.iter().collect();
                        return exit(format!("{to}: not in the group's HavokMaterials {}", py_list(&ms)));
                    };
                    changes.push((member.clone(), s, name.clone(), to.clone()));
                    x = p as i64;
                }
                out.push(x);
            }
            new.push(out);
        }
        let same = new.len() == subparts.len()
            && new
                .iter()
                .zip(subparts)
                .all(|(a, b)| a.len() == b.len() && a.iter().zip(b).all(|(x, y)| *x == i64::from(*y)));
        if !same {
            edits.push((*bi as i64, new));
        }
    }
    let out = havokport::with_face_materials(res, &edits).map_err(hv)?;
    Ok((out, changes))
}

/// A list of strings as Python's `repr` of a list of str shows it (`['a', 'b']`; the names here hold no quote).
fn py_list(items: &[&String]) -> String {
    let inner: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", inner.join(", "))
}
