//! `LevelCheck`, `PlacedObject` and `SlotFixer` of `DbxEdit.cs` (FORMAT.md section 7): problems that crash or break
//! a level found without starting the game, and the cull slots of placed objects matched to the level whose static
//! cull trees a file uses.

use std::collections::{HashMap, HashSet};

use crate::dbx::net_g;

use super::fault::{Fault, Result};
use super::file::{EditDbx, NUMBERS, NodeId, Table, be_u32, instances, kids, partition_children, read};
use super::net;

/// A placed object (`ReferenceObjectData`): blueprint, position and cull slot (`Enumeration`).
#[derive(Debug, Clone)]
pub struct PlacedObject {
    /// The instance's index among the partition's children.
    pub index: usize,
    /// The `Enumeration` child's index among the instance's children, if it is a 6-byte numbers element.
    pub slot: Option<usize>,
    /// The cull slot (0 without one).
    pub slot_value: i32,
    /// The instance's guid (None: it has none).
    pub guid: Option<String>,
    /// The blueprint: the second-last part of `ReferencedObject`'s reference, lower case.
    pub blueprint: Option<String>,
    /// The position (the transform's translation).
    pub x: f32,
    /// Y.
    pub y: f32,
    /// Z.
    pub z: f32,
}

fn float_at(p: &[u8], at: usize) -> f32 {
    be_u32(p, at).map_or(0.0, f32::from_bits)
}

impl PlacedObject {
    /// `PlacedObject.All`: every `ReferenceObjectData` instance of the file.
    ///
    /// # Errors
    /// A file without a partition, an instance that is not a container, or a damaged string.
    pub fn all(f: &EditDbx) -> Result<Vec<PlacedObject>> {
        let t = Table(f);
        let mut list = Vec::new();
        for (index, &i) in partition_children(f)?.iter().enumerate() {
            if t.tag(i)? != "instance" {
                continue;
            }
            let ty = t.get(i, "type")?.unwrap_or("");
            if !ty.ends_with(".ReferenceObjectData") {
                continue;
            }
            let mut o = PlacedObject {
                index,
                slot: None,
                slot_value: 0,
                guid: t.get(i, "guid")?.map(str::to_owned),
                blueprint: None,
                x: 0.0,
                y: 0.0,
                z: 0.0,
            };
            for (ci, &c) in f.children(i).ok_or_else(Fault::null)?.iter().enumerate() {
                let name = t.get(c, "name")?;
                let payload = f.payload(c);
                if name == Some("Enumeration") && f.kind(c) == NUMBERS && payload.len() == 6 {
                    o.slot = Some(ci);
                    o.slot_value = be_u32(payload, 2)? as i32;
                } else if name == Some("ReferencedObject") {
                    let r = net::to_lower(t.get(c, "ref")?.unwrap_or(""));
                    let parts: Vec<&str> = r.split('/').collect();
                    o.blueprint = Some(if parts.len() >= 2 {
                        parts.get(parts.len() - 2).copied().unwrap_or("").to_owned()
                    } else {
                        String::new()
                    });
                } else if name == Some("Transform") && f.kind(c) == NUMBERS && payload.len() >= 2 + 16 * 4 {
                    o.x = float_at(payload, 2 + 12 * 4);
                    o.y = float_at(payload, 2 + 13 * 4);
                    o.z = float_at(payload, 2 + 14 * 4);
                }
            }
            list.push(o);
        }
        Ok(list)
    }

    fn blueprint(&self) -> &str {
        self.blueprint.as_deref().unwrap_or("")
    }

    /// `PositionKey`: blueprint and position to 0.1 m (`Math.Round(x, 1)`, banker's rounding, as .NET formats it).
    #[must_use]
    pub fn position_key(&self) -> String {
        let r = |v: f32| {
            let v = f64::from(v);
            net_g(
                if v.abs() < 1e16 {
                    (v * 10.0).round_ties_even() / 10.0
                } else {
                    v
                },
                15,
            )
        };
        format!("{}@{},{},{}", self.blueprint(), r(self.x), r(self.y), r(self.z))
    }

    /// The distance between two objects (the differences in single precision, as DbxEdit computes them).
    #[must_use]
    pub fn distance(&self, o: &PlacedObject) -> f64 {
        let dx = f64::from(self.x - o.x);
        let dy = f64::from(self.y - o.y);
        let dz = f64::from(self.z - o.z);
        (dx * dx + dy * dy + dz * dz).sqrt()
    }
}

/// What `fixslots` did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SlotResult {
    /// Matched by guid.
    pub by_guid: usize,
    /// Matched by blueprint and position.
    pub by_position: usize,
    /// Matched by the nearest unused same-blueprint object within 50 m.
    pub by_nearest: usize,
    /// Without a counterpart (slot 0).
    pub no_counterpart: usize,
    /// Slots changed.
    pub changed: usize,
    /// `blueprint guid` of each object without a counterpart.
    pub unmatched: Vec<String>,
}

/// `SlotFixer.Fix`: each object takes the slot of the same object in the level whose static cull trees the file
/// uses - same guid, else same blueprint and position, else the nearest unused same-blueprint object within 50 m -
/// and 0 when there is none.
///
/// # Errors
/// A damaged file, or an object without a guid.
pub fn fix_slots(file: &mut EditDbx, cull_level: &EditDbx) -> Result<SlotResult> {
    let mine: Vec<PlacedObject> = PlacedObject::all(file)?
        .into_iter()
        .filter(|o| o.slot.is_some())
        .collect();
    let theirs: Vec<PlacedObject> = PlacedObject::all(cull_level)?
        .into_iter()
        .filter(|o| o.slot.is_some())
        .collect();
    let mut by_guid: HashMap<String, usize> = HashMap::new();
    for (i, o) in theirs.iter().enumerate() {
        by_guid.insert(net::to_upper(o.guid.as_deref().ok_or_else(Fault::null)?), i);
    }
    let mut by_position: HashMap<String, usize> = HashMap::new();
    for (i, o) in theirs.iter().enumerate() {
        by_position.entry(o.position_key()).or_insert(i);
    }
    let mut r = SlotResult::default();
    let mut assigned: Vec<(usize, i32)> = Vec::new();
    let mut left = Vec::new();
    for (mi, o) in mine.iter().enumerate() {
        let g = net::to_upper(o.guid.as_deref().ok_or_else(Fault::null)?);
        if let Some(&t) = by_guid.get(&g) {
            assigned.push((mi, theirs.get(t).map_or(0, |t| t.slot_value)));
            r.by_guid += 1;
        } else if let Some(&t) = by_position.get(&o.position_key()) {
            assigned.push((mi, theirs.get(t).map_or(0, |t| t.slot_value)));
            r.by_position += 1;
        } else {
            left.push(mi);
        }
    }
    let mut used: HashSet<i32> = assigned.iter().map(|&(_, v)| v).collect();
    for mi in left {
        let Some(o) = mine.get(mi) else { continue };
        let mut best: Option<(&PlacedObject, f64)> = None;
        for t in &theirs {
            if t.blueprint == o.blueprint && t.slot_value != 0 && !used.contains(&t.slot_value) {
                let d = t.distance(o);
                // OrderBy's first smallest; NaN sorts before every number
                if best.is_none_or(|(_, bd)| d < bd || (d.is_nan() && !bd.is_nan())) {
                    best = Some((t, d));
                }
            }
        }
        match best {
            Some((b, d)) if d < 50.0 => {
                assigned.push((mi, b.slot_value));
                used.insert(b.slot_value);
                r.by_nearest += 1;
            }
            _ => {
                assigned.push((mi, 0));
                r.no_counterpart += 1;
                r.unmatched
                    .push(format!("{} {}", o.blueprint(), o.guid.as_deref().unwrap_or("")));
            }
        }
    }
    let p = *file.roots().first().ok_or_else(Fault::list_index)?;
    let parts = file.children(p).ok_or_else(Fault::null)?.to_vec();
    for (mi, v) in assigned {
        let Some(o) = mine.get(mi) else { continue };
        if o.slot_value == v {
            continue;
        }
        let slot = parts
            .get(o.index)
            .and_then(|&i| file.children(i))
            .and_then(|k| o.slot.and_then(|s| k.get(s)))
            .copied();
        if let Some(c) = slot {
            let b = v.to_be_bytes();
            file.set_payload(c, vec![1, 4, b[0], b[1], b[2], b[3]])
                .map_err(|_| Fault::null())?;
        }
        r.changed += 1;
    }
    Ok(r)
}

/// One finding of `check`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    /// An error (else a warning).
    pub error: bool,
    /// Where.
    pub place: String,
    /// What.
    pub message: String,
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}{}: {}",
            if self.error { "ERROR  " } else { "WARN   " },
            self.place,
            self.message
        )
    }
}

const WORLD_TYPES: [&str; 4] = [
    "ModelGroupEntityData",
    "ModelGroupMemberData",
    "ForestEntityData",
    "TreeMemberData",
];
const LISTED_WORLD_TYPES: [&str; 4] = [
    "TerrainEntityData",
    "ModelGroupEntityData",
    "ForestEntityData",
    "StaticEnvmapEntityData",
];
const WORLD_PARTS: [&str; 2] = ["PhysicsData", "MeshEntityType"];

fn short(ty: &str) -> &str {
    ty.rfind('.').map_or(ty, |i| ty.get(i + 1..).unwrap_or(""))
}

fn walk(file: &EditDbx, element: NodeId, path: &str, visit: &mut dyn FnMut(NodeId, &str) -> Result<()>) -> Result<()> {
    let table = Table(file);
    let Some(kids) = file.children(element) else {
        return Ok(());
    };
    let mut seen: HashMap<String, usize> = HashMap::new();
    for &child in kids {
        let name = match table.get(child, "name")? {
            Some(name) => name.to_owned(),
            None => table.tag(child)?.to_owned(),
        };
        let nth = seen.get(&name).copied().unwrap_or(0);
        seen.insert(name.clone(), nth + 1);
        let child_path = if path.is_empty() {
            name
        } else if table.tag(child)? == "item" {
            format!("{path}[{nth}]")
        } else {
            format!("{path}.{name}")
        };
        visit(child, &child_path)?;
        walk(file, child, &child_path, visit)?;
    }
    Ok(())
}

/// `LevelCheck.Run`: `slots_from` the level whose cull trees the file uses (or none); `bundles` unpacked folders to
/// resolve references into other files.
///
/// # Errors
/// A damaged file, or an object without a guid where one is compared.
pub fn level_check(f: &EditDbx, slots_from: Option<&EditDbx>, bundles: &[String]) -> Result<Vec<Finding>> {
    let t = Table(f);
    let mut found = Vec::new();
    let mut guids = HashSet::new();
    for i in instances(f)? {
        guids.insert(net::to_upper(t.get(i?, "guid")?.unwrap_or("")));
    }
    // external references: "partition|guid" -> up to 3 places, in the order first met
    let mut external: Vec<(String, Vec<String>)> = Vec::new();
    let mut external_index: HashMap<String, usize> = HashMap::new();
    for inst in instances(f)? {
        let inst = inst?;
        let ty = t.get(inst, "type")?.unwrap_or("");
        let place = format!("{} {}", short(ty), t.get(inst, "guid")?.unwrap_or(""));
        let level_data = ty.ends_with(".LevelData");
        let world = WORLD_TYPES.iter().any(|w| ty.ends_with(w));
        walk(f, inst, "", &mut |e, path| {
            let Some(r) = t.get(e, "ref")? else { return Ok(()) };
            let in_objects = level_data && path.starts_with("Objects");
            let is_local = r.encode_utf16().count() == 36 && !r.contains('/');
            if r == "null" {
                if in_objects && t.tag(e)? == "item" {
                    found.push(Finding {
                        error: true,
                        place: format!("{place} {path}"),
                        message:
                            "empty entry in the level's object list (the game reads it without a check and crashes)"
                                .into(),
                    });
                } else if world && let Some(n) = t.get(e, "name")?.filter(|n| WORLD_PARTS.contains(n)) {
                    found.push(Finding {
                        error: true,
                        place: format!("{place} {path}"),
                        message: format!("world object without {n} (the model-group set-up crashes on it)"),
                    });
                }
            } else if is_local && !guids.contains(&net::to_upper(r)) {
                found.push(Finding {
                    error: true,
                    place: format!("{place} {path}"),
                    message: format!(
                        "refers to instance {r}, which is not in this file{}",
                        if in_objects {
                            " (an empty entry in the level's object list: crash)"
                        } else {
                            ""
                        }
                    ),
                });
            } else if r.find('/').is_some_and(|i| i > 0) {
                let cut = r.rfind('/').unwrap_or(0);
                let key = format!("{}|{}", r.get(..cut).unwrap_or(""), r.get(cut + 1..).unwrap_or(""));
                let ki = net::to_upper(&key);
                let idx = *external_index.entry(ki).or_insert_with(|| {
                    external.push((key, Vec::new()));
                    external.len() - 1
                });
                if let Some((_, users)) = external.get_mut(idx)
                    && users.len() < 3
                {
                    users.push(format!("{place} {path}"));
                }
            }
            Ok(())
        })?;
    }
    check_world_listed(f, &mut found)?;
    check_slots(f, slots_from, &mut found)?;
    if !bundles.is_empty() {
        check_external(&external, bundles, &mut found)?;
    }
    Ok(found)
}

fn check_world_listed(f: &EditDbx, found: &mut Vec<Finding>) -> Result<()> {
    let t = Table(f);
    let mut level = None;
    for i in instances(f)? {
        let i = i?;
        if t.get(i, "type")?.unwrap_or("").ends_with(".LevelData") {
            level = Some(i);
            break;
        }
    }
    let Some(level) = level else { return Ok(()) };
    let mut objects = None;
    for &c in f.children(level).ok_or_else(Fault::null)? {
        if t.get(c, "name")? == Some("Objects") && f.is_container(c) {
            objects = Some(c);
            break;
        }
    }
    let mut listed = HashSet::new();
    if let Some(o) = objects {
        for &c in kids(f, o) {
            listed.insert(net::to_upper(t.get(c, "ref")?.unwrap_or("")));
        }
    }
    for i in instances(f)? {
        let i = i?;
        let ty = t.get(i, "type")?.unwrap_or("");
        let guid = t.get(i, "guid")?;
        if LISTED_WORLD_TYPES.iter().any(|w| ty.ends_with(w)) {
            // a null guid is never in a set
            let in_list = guid.is_some_and(|g| listed.contains(&net::to_upper(g)));
            if !in_list {
                found.push(Finding {
                    error: false,
                    place: format!("{} {}", short(ty), guid.unwrap_or("")),
                    message:
                        "not in the level's object list, so it is not loaded (missing terrain or buildings in game)"
                            .into(),
                });
            }
        }
    }
    Ok(())
}

fn check_slots(f: &EditDbx, slots_from: Option<&EditDbx>, found: &mut Vec<Finding>) -> Result<()> {
    let all = PlacedObject::all(f)?;
    let objects: Vec<&PlacedObject> = all.iter().filter(|o| o.slot.is_some() && o.slot_value != 0).collect();
    let mut groups: Vec<(i32, Vec<&PlacedObject>)> = Vec::new();
    let mut group_index: HashMap<i32, usize> = HashMap::new();
    for o in &objects {
        let gi = *group_index.entry(o.slot_value).or_insert_with(|| {
            groups.push((o.slot_value, Vec::new()));
            groups.len() - 1
        });
        if let Some((_, g)) = groups.get_mut(gi) {
            g.push(o);
        }
    }
    for (slot, g) in groups.iter().filter(|(_, g)| g.len() > 1).take(20) {
        let names: Vec<&str> = g.iter().take(3).map(|o| o.blueprint()).collect();
        found.push(Finding {
            error: false,
            place: format!("cull slot {slot}"),
            message: format!(
                "shared by {} objects ({}): they are shown and hidden together - see-through objects",
                g.len(),
                names.join(", ")
            ),
        });
    }
    let Some(from) = slots_from else { return Ok(()) };
    let mut theirs: HashMap<String, i32> = HashMap::new();
    for o in PlacedObject::all(from)?.into_iter().filter(|o| o.slot.is_some()) {
        let g = net::to_upper(o.guid.as_deref().ok_or_else(Fault::null)?);
        if theirs.insert(g, o.slot_value).is_some() {
            return Err(Fault::new(
                super::fault::FaultKind::Argument,
                "An item with the same key has already been added.",
            ));
        }
    }
    let mut wrong: Vec<&PlacedObject> = Vec::new();
    for o in all.iter().filter(|o| o.slot.is_some()) {
        let g = net::to_upper(o.guid.as_deref().ok_or_else(Fault::null)?);
        if theirs.get(&g).is_some_and(|&v| v != o.slot_value) {
            wrong.push(o);
        }
    }
    if !wrong.is_empty() {
        let e: Vec<String> = wrong
            .iter()
            .take(3)
            .map(|o| format!("{} {}", o.blueprint(), o.slot_value))
            .collect();
        found.push(Finding {
            error: false,
            place: "cull slots".into(),
            message: format!(
                "{} objects have another slot than the same object in the cull trees' level (e.g. {}) - see-through objects; fix with: DbxEdit fixslots",
                wrong.len(),
                e.join(", ")
            ),
        });
    }
    Ok(())
}

fn check_external(external: &[(String, Vec<String>)], bundles: &[String], found: &mut Vec<Finding>) -> Result<()> {
    let mut index: HashMap<String, String> = HashMap::new();
    for b in bundles {
        for p in net::dbx_files_under(b)? {
            let rel = p
                .get(b.len()..)
                .unwrap_or("")
                .trim_start_matches(['\\', '/'])
                .replace('\\', "/");
            let mut cut = rel.len().saturating_sub(4);
            while !rel.is_char_boundary(cut) {
                cut -= 1;
            }
            let rel = net::to_upper(rel.get(..cut).unwrap_or(""));
            index.entry(rel).or_insert(p);
        }
    }
    let mut loaded: HashMap<String, Option<HashSet<String>>> = HashMap::new();
    let mut missing_files = 0;
    for (key, users) in external {
        let mut parts = key.split('|');
        let (partition, guid) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let Some(path) = index.get(&net::to_upper(partition)) else {
            missing_files += 1;
            if missing_files <= 30 {
                found.push(Finding {
                    error: false,
                    place: users.first().cloned().unwrap_or_default(),
                    message: format!("refers to {partition}, which is not in the given folders"),
                });
            }
            continue;
        };
        let lp = net::to_upper(path);
        if !loaded.contains_key(&lp) {
            let d = read(&net::read_bytes(path)?)?;
            let g = if d.is_binary() {
                let mut set = HashSet::new();
                for i in instances(&d)? {
                    set.insert(net::to_upper(Table(&d).get(i?, "guid")?.unwrap_or("")));
                }
                Some(set)
            } else {
                None
            };
            loaded.insert(lp.clone(), g);
        }
        if let Some(Some(g)) = loaded.get(&lp)
            && !g.contains(&net::to_upper(guid))
        {
            found.push(Finding {
                error: true,
                place: users.first().cloned().unwrap_or_default(),
                message: format!("refers to instance {guid} in {partition}, which that file does not have"),
            });
        }
    }
    if missing_files > 30 {
        found.push(Finding {
            error: false,
            place: "references to other files".into(),
            message: format!("{} more files not in the given folders", missing_files - 30),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(short("a.b.C"), "C");
        assert_eq!(short("C"), "C");
        let f = Finding {
            error: true,
            place: "x".into(),
            message: "y".into(),
        };
        assert_eq!(f.to_string(), "ERROR  x: y");
        let o = PlacedObject {
            index: 0,
            slot: None,
            slot_value: 0,
            guid: None,
            blueprint: Some("crate".into()),
            x: 1.25,
            y: 0.35,
            z: -0.05,
        };
        assert_eq!(o.position_key(), "crate@1.2,0.3,-0.1");
        assert!((o.distance(&o)).abs() < 1e-9);
    }
}
