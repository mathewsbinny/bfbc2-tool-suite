//! Static group members as the editor edits them (round SG1): their identity across edits, their place in the three
//! files that hold a model group, and the group's consistency as the game reads it.
//!
//! **What holds a group** (spec LEVEL-MG-5 - LEVEL-MG-8, decoded on 131 groups of 31 levels, 107,103 members):
//!
//! - the level partition's group entity (`*ModelGroupEntityData`, `ForestEntityData`, named
//!   `Levels/<LV>/<Group>`) lists its **member types** (`MemberDatas`: one `*MemberData` per blueprint and shader set)
//!   in order; each has `InstanceCount`, one `SpawnDatas` record per instance (its `Enumeration`: the instance's leaf
//!   in the client cull tree, and its `MeshShaderSet`), `PhysicsPartCountPerInstance`, `FirstPhysicsPart` and
//!   `PhysicsPartCount` (the type's first part and the **end** of its parts: cumulative over the group's types), and
//!   on static groups `FirstNetworkId` / `NetworkIdCount` / `NetworkIdCountPerInstance` the same way;
//! - the group file `Levels/<LV>/<Group>_HavokAsset.dbx` (`CompositeHavokAsset`) holds the instances in member-type
//!   order: `Assets` (each instance's `<blueprint>_havok` reference), `Transforms` (16 numbers each) and `Enums` (each
//!   instance's leaf, the same numbers as the `SpawnDatas` in the same order);
//! - the group's physics, `<Group>_HavokAsset.res`, holds the collision **per member type** in the type's own space
//!   (one rigid body each: the game places it at each instance's transform), and per physics part of every instance
//!   one word (the owning instance's leaf) and one material byte (the same bytes for every instance of a type).
//!
//! **Identity.** The scene names a member `<group instance guid>#<k>` by its place `k` in the base's file. Deleting a
//! member removes its item, and DICE's layout leaves no gap (every count above is cumulative), so places shift. The
//! editor therefore names members by identity, never by place: a base member keeps `#k` (its place in the base) for
//! the whole project, a member the project adds is `#+n` (its own number in its group instance), and each edited
//! group instance keeps its members' current order ([`Order`]). An edit finds a member's place through that order;
//! the files are always written in DICE's layout (no tombstones, no gaps). New members' leaves are given at export
//! (`Enums` 0 until then).

use std::collections::BTreeMap;

use bc2_formats::dbx::NUMBERS;
use bc2_formats::dbx::edit::{EditDbx, NodeId, numbers_of, varint_bytes};
use serde::{Deserialize, Serialize};

use crate::ops::Rows;
use crate::write::{LevelWriteError, LevelWriter, Result};

/// A member's identity in its group instance (round SG1): stable while other members are deleted and added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Member {
    /// A member of the base level: its place in the base's group file (JSON: the number, as round OP1 wrote it).
    Base(usize),
    /// A member the project added: its number in its group instance (JSON: `{"added": n}`).
    Added {
        /// The number (1, 2 ...).
        added: u32,
    },
}

impl Member {
    /// The part of the scene's guid after `#`: `3`, `+1`.
    #[must_use]
    pub fn suffix(&self) -> String {
        match self {
            Self::Base(k) => k.to_string(),
            Self::Added { added } => format!("+{added}"),
        }
    }

    /// Reads [`Member::suffix`] back.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.strip_prefix('+') {
            Some(n) => n.parse().ok().map(|added| Self::Added { added }),
            None => s.parse().ok().map(Self::Base),
        }
    }
}

impl std::fmt::Display for Member {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.suffix())
    }
}

/// A member's guid as the scene names it: `<group instance guid>#<member>` (upper case; `#+n` for an added one).
#[must_use]
pub fn member_guid(instance: &str, member: Member) -> String {
    format!("{}#{}", instance.to_ascii_uppercase(), member.suffix())
}

/// The group instance and the member of a member's guid (`None`: not a member's guid).
#[must_use]
pub fn parse_member_guid(guid: &str) -> Option<(String, Member)> {
    let (inst, m) = guid.split_once('#')?;
    Some((inst.to_ascii_uppercase(), Member::parse(m)?))
}

/// A member's blueprint name from its `Assets` reference (`objects/props/crate/crate_havok/<guid>` -> `crate`).
#[must_use]
pub fn asset_name(asset: &str) -> String {
    let path = asset.rsplit_once('/').map_or(asset, |x| x.0);
    let last = path.rsplit('/').next().unwrap_or(path);
    let low = last.to_ascii_lowercase();
    low.strip_suffix("_havok")
        .map_or_else(|| last.to_owned(), |s| last.get(..s.len()).unwrap_or(s).to_owned())
}

/// The members of one group instance in their current order: the place of each in the file.
pub type Order = Vec<Member>;

/// The edited group instances' orders, by (group file name, group instance guid upper case).
pub type Orders = BTreeMap<(String, String), Order>;

fn bad(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

// ------------------------------------------------------------------------------------------------ numbers

/// The whole numbers of a numbers element (big-endian, signed, of its size); `None` when it is not one.
fn ints(d: &EditDbx, n: NodeId) -> Option<(u32, Vec<i64>)> {
    if d.kind(n) != NUMBERS {
        return None;
    }
    let (size, vals) = numbers_of(d.raw(n))?;
    let out = vals
        .iter()
        .map(|v| {
            let mut x: i64 = if v.first().is_some_and(|b| b & 0x80 != 0) {
                -1
            } else {
                0
            };
            for b in *v {
                x = (x << 8) | i64::from(*b);
            }
            x
        })
        .collect();
    Some((size, out))
}

/// Writes whole numbers into a numbers element in `size` bytes each (big-endian).
fn set_ints(d: &mut EditDbx, n: NodeId, size: u32, values: &[i64]) -> Result<()> {
    let mut raw = varint_bytes(values.len() as u32);
    raw.extend(varint_bytes(size));
    for v in values {
        let b = v.to_be_bytes();
        raw.extend_from_slice(b.get(8 - size.min(8) as usize..).unwrap_or_default());
    }
    d.set_raw(n, raw)?;
    Ok(())
}

/// One whole-number field of an instance.
fn int_field(d: &EditDbx, inst: NodeId, name: &[u8]) -> Option<i64> {
    let f = d.child(inst, name)?;
    ints(d, f).and_then(|(_, v)| v.first().copied())
}

fn set_int_field(d: &mut EditDbx, inst: NodeId, name: &[u8], v: i64) -> Result<()> {
    let f = d.child(inst, name).ok_or_else(|| LevelWriteError::NoField {
        guid: guid_of(d, inst),
        field: String::from_utf8_lossy(name).into_owned(),
    })?;
    let size = ints(d, f).map_or(4, |(s, _)| s);
    set_ints(d, f, size, &[v])
}

fn guid_of(d: &EditDbx, e: NodeId) -> String {
    String::from_utf8_lossy(d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase()
}

// ------------------------------------------------------------------------------------------------ the partition

/// A model group of the level partition: its entity, its member types in order.
#[derive(Debug, Clone)]
pub struct GroupEntity {
    /// The group entity (`StaticModelGroupEntityData` ...).
    pub entity: NodeId,
    /// Its `Name` (`Levels/MP_005/StaticModelGroupEntity`).
    pub name: String,
    /// Its member types (`MemberDatas`, in order).
    pub types: Vec<NodeId>,
}

/// The group file of a group entity's name (`staticmodelgroupentity_havokasset.dbx`, lower case).
#[must_use]
pub fn file_of(name: &str) -> String {
    format!("{}_havokasset.dbx", name.rsplit('/').next().unwrap_or(name)).to_ascii_lowercase()
}

/// Every model group of a partition (static, rigid, middle-growth, forest), in partition order.
#[must_use]
pub fn group_entities(w: &LevelWriter) -> Vec<GroupEntity> {
    let d = &w.d;
    let mut out = Vec::new();
    for e in d.instances() {
        let ty = w.type_of(e);
        if !(ty.ends_with("ModelGroupEntityData") || ty.ends_with(".ForestEntityData")) {
            continue;
        }
        let name = d
            .child(e, b"Name")
            .and_then(|n| d.text(n))
            .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned());
        let types = d
            .child(e, b"MemberDatas")
            .and_then(|a| d.children(a))
            .unwrap_or_default()
            .iter()
            .filter_map(|&i| d.attr(i, b"ref"))
            .filter_map(|r| w.find(&String::from_utf8_lossy(r)))
            .collect();
        out.push(GroupEntity { entity: e, name, types });
    }
    out
}

/// The group entity whose file is `file` (any case, any folder).
#[must_use]
pub fn group_entity(w: &LevelWriter, file: &str) -> Option<GroupEntity> {
    let want = crate::files::Target::group(file);
    group_entities(w)
        .into_iter()
        .find(|g| crate::files::Target::group(&file_of(&g.name)) == want)
}

/// One member type of a group as the partition counts it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRun {
    /// The member type (`StaticModelGroupMemberData` ...).
    pub node: NodeId,
    /// Its first instance's place in the group file.
    pub first: usize,
    /// `InstanceCount`.
    pub count: usize,
    /// `PhysicsPartCountPerInstance`.
    pub parts: u32,
    /// Its instances' leaves (`SpawnDatas[].Enumeration`).
    pub enums: Vec<i64>,
}

fn spawn_items(d: &EditDbx, t: NodeId) -> Vec<NodeId> {
    d.child(t, b"SpawnDatas")
        .and_then(|a| d.children(a))
        .map_or_else(Vec::new, <[NodeId]>::to_vec)
}

/// A group's member types with their places in the group file (cumulative `InstanceCount`).
///
/// # Errors
/// A member type without `InstanceCount` or `PhysicsPartCountPerInstance`.
pub fn runs(w: &LevelWriter, g: &GroupEntity) -> Result<Vec<TypeRun>> {
    let d = &w.d;
    let mut out = Vec::new();
    let mut first = 0usize;
    for &t in &g.types {
        let count = int_field(d, t, b"InstanceCount")
            .and_then(|c| usize::try_from(c).ok())
            .ok_or_else(|| {
                bad(format!(
                    "{}: member type {} has no InstanceCount",
                    g.name,
                    guid_of(d, t)
                ))
            })?;
        let parts = int_field(d, t, b"PhysicsPartCountPerInstance")
            .and_then(|c| u32::try_from(c).ok())
            .ok_or_else(|| {
                bad(format!(
                    "{}: member type {} has no PhysicsPartCountPerInstance",
                    g.name,
                    guid_of(d, t)
                ))
            })?;
        let enums = spawn_items(d, t)
            .iter()
            .filter_map(|&s| int_field(d, s, b"Enumeration"))
            .collect();
        out.push(TypeRun {
            node: t,
            first,
            count,
            parts,
            enums,
        });
        first += count;
    }
    Ok(out)
}

/// The type run holding the member at `place`.
fn run_at(runs: &[TypeRun], place: usize) -> Option<&TypeRun> {
    runs.iter().find(|r| (r.first..r.first + r.count).contains(&place))
}

/// The cumulative fields made again from each type's counts (DICE's layout: `FirstPhysicsPart` and
/// `PhysicsPartCount` the first and the end of the type's parts, the same for network ids where the group has them).
fn renumber(w: &mut LevelWriter, g: &GroupEntity) -> Result<()> {
    let (mut part, mut net) = (0i64, 0i64);
    for &t in &g.types {
        let count = int_field(&w.d, t, b"InstanceCount").unwrap_or(0);
        let per = int_field(&w.d, t, b"PhysicsPartCountPerInstance").unwrap_or(0);
        set_int_field(&mut w.d, t, b"FirstPhysicsPart", part)?;
        part += per * count;
        set_int_field(&mut w.d, t, b"PhysicsPartCount", part)?;
        if w.d.child(t, b"FirstNetworkId").is_some() {
            let per_net = int_field(&w.d, t, b"NetworkIdCountPerInstance").unwrap_or(0);
            set_int_field(&mut w.d, t, b"FirstNetworkId", net)?;
            net += per_net * count;
            set_int_field(&mut w.d, t, b"NetworkIdCount", net)?;
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ the group file

/// The arrays of a group instance in its file.
struct Arrays {
    assets: NodeId,
    transforms: NodeId,
    enums: Option<NodeId>,
}

fn arrays(f: &LevelWriter, instance: &str) -> Result<Arrays> {
    let e = f
        .find(instance)
        .ok_or_else(|| LevelWriteError::NotFound(instance.to_owned()))?;
    let need = |name: &[u8]| {
        f.d.child(e, name).ok_or_else(|| LevelWriteError::NoField {
            guid: instance.to_owned(),
            field: String::from_utf8_lossy(name).into_owned(),
        })
    };
    Ok(Arrays {
        assets: need(b"Assets")?,
        transforms: need(b"Transforms")?,
        enums: f.d.child(e, b"Enums"),
    })
}

/// The number of members of a group instance in its file (its `Assets`).
///
/// # Errors
/// No such instance, or no `Assets`.
pub fn member_count(f: &LevelWriter, instance: &str) -> Result<usize> {
    let a = arrays(f, instance)?;
    Ok(f.d.children(a.assets).map_or(0, <[NodeId]>::len))
}

/// The order of a group instance not edited yet: its base members in place order.
///
/// # Errors
/// As [`member_count`].
pub fn base_order(f: &LevelWriter, instance: &str) -> Result<Order> {
    Ok((0..member_count(f, instance)?).map(Member::Base).collect())
}

/// A member's place in its group file now.
#[must_use]
pub fn place_of(order: &[Member], member: Member) -> Option<usize> {
    order.iter().position(|m| *m == member)
}

/// The `Assets` reference of the member at `place`.
#[must_use]
pub fn asset_at(f: &LevelWriter, instance: &str, place: usize) -> Option<String> {
    let a = arrays(f, instance).ok()?;
    let item = *f.d.children(a.assets)?.get(place)?;
    f.d.attr(item, b"ref").map(|r| String::from_utf8_lossy(r).into_owned())
}

/// The leaf (`Enums`) of the member at `place`.
#[must_use]
pub fn enum_at(f: &LevelWriter, instance: &str, place: usize) -> Option<i64> {
    let a = arrays(f, instance).ok()?;
    ints(&f.d, a.enums?)?.1.get(place).copied()
}

fn remove_numbers(d: &mut EditDbx, n: NodeId, at: usize, width: usize) -> Result<()> {
    let (size, vals) = numbers_of(d.raw(n)).ok_or_else(|| bad("damaged numbers"))?;
    let mut v: Vec<Vec<u8>> = vals.iter().map(|x| x.to_vec()).collect();
    if at + width > v.len() {
        return Err(bad("a member past the end of its group's numbers"));
    }
    v.drain(at..at + width);
    write_values(d, n, size, &v)
}

fn insert_numbers(d: &mut EditDbx, n: NodeId, at: usize, new: &[Vec<u8>]) -> Result<()> {
    let (size, vals) = numbers_of(d.raw(n)).ok_or_else(|| bad("damaged numbers"))?;
    let mut v: Vec<Vec<u8>> = vals.iter().map(|x| x.to_vec()).collect();
    if at > v.len() {
        return Err(bad("a member past the end of its group's numbers"));
    }
    v.splice(at..at, new.iter().cloned());
    write_values(d, n, size, &v)
}

fn write_values(d: &mut EditDbx, n: NodeId, size: u32, v: &[Vec<u8>]) -> Result<()> {
    let mut raw = varint_bytes(v.len() as u32);
    raw.extend(varint_bytes(size));
    for x in v {
        raw.extend_from_slice(x);
    }
    d.set_raw(n, raw)?;
    Ok(())
}

// ------------------------------------------------------------------------------------------------ the edits

/// Deletes a member from its group (round SG1): its `Assets` item, its 16 transform numbers and its leaf in the
/// group file, and in the partition its type's `SpawnDatas` record, `InstanceCount` one less and the group's
/// cumulative part and network numbers made again. The last member of a type is refused (DICE ships no member type
/// without instances, and its physics stays in the group's `.res`).
///
/// # Errors
/// The member is not in the group, its group is not in the partition, or it is the last of its type.
pub fn delete(
    f: &mut LevelWriter,
    w: &mut LevelWriter,
    file: &str,
    instance: &str,
    order: &mut Order,
    member: Member,
) -> Result<()> {
    let name = member_guid(instance, member);
    let place = place_of(order, member).ok_or_else(|| LevelWriteError::NotFound(name.clone()))?;
    let g = group_entity(w, file).ok_or_else(|| bad(format!("the level partition has no group for {file}")))?;
    let rs = runs(w, &g)?;
    let r = run_at(&rs, place)
        .ok_or_else(|| {
            bad(format!(
                "{name}: its group's member types count fewer members than its file"
            ))
        })?
        .clone();
    if r.count <= 1 {
        let what =
            w.d.child(r.node, b"Name")
                .and_then(|n| w.d.text(n))
                .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned());
        return Err(bad(format!(
            "{name} is the last {} of its group: a group keeps at least one of each kind it holds (DICE's groups \
             never have a kind without one; its collision stays in the group's physics). Move it instead",
            what.rsplit('/').next().unwrap_or(&what)
        )));
    }
    let a = arrays(f, instance)?;
    f.d.children_mut(a.assets)?.remove(place);
    remove_numbers(&mut f.d, a.transforms, 16 * place, 16)?;
    if let Some(e) = a.enums {
        remove_numbers(&mut f.d, e, place, 1)?;
    }
    let spawns =
        w.d.child(r.node, b"SpawnDatas")
            .ok_or_else(|| bad(format!("{name}: its member type has no SpawnDatas")))?;
    let k = place - r.first;
    let items = w.d.children_mut(spawns)?;
    if k >= items.len() {
        return Err(bad(format!(
            "{name}: its member type has fewer SpawnDatas than instances"
        )));
    }
    items.remove(k);
    set_int_field(&mut w.d, r.node, b"InstanceCount", (r.count - 1) as i64)?;
    renumber(w, &g)?;
    order.remove(place);
    Ok(())
}

/// Adds a member to a group (round SG1): a copy of member `like` (its type: the same blueprint, shader set and
/// collision) at `rows`, at the end of its type's run in the group file, with `Enums` 0 (its leaf is given at
/// export); in the partition its type's `SpawnDatas` gets a copy of `like`'s record (its `Enumeration` 0),
/// `InstanceCount` one more, and the group's cumulative numbers are made again.
///
/// # Errors
/// `like` is not in the group, `id` is already, or the group is not in the partition.
#[allow(
    clippy::too_many_arguments,
    reason = "the two files, the member's group, its order and the new member"
)]
pub fn add(
    f: &mut LevelWriter,
    w: &mut LevelWriter,
    file: &str,
    instance: &str,
    order: &mut Order,
    like: Member,
    id: Member,
    rows: &Rows,
) -> Result<()> {
    if place_of(order, id).is_some() {
        return Err(LevelWriteError::Exists(member_guid(instance, id)));
    }
    let src_name = member_guid(instance, like);
    let src = place_of(order, like).ok_or_else(|| LevelWriteError::NotFound(src_name.clone()))?;
    let g = group_entity(w, file).ok_or_else(|| bad(format!("the level partition has no group for {file}")))?;
    let rs = runs(w, &g)?;
    let r = run_at(&rs, src)
        .ok_or_else(|| {
            bad(format!(
                "{src_name}: its group's member types count fewer members than its file"
            ))
        })?
        .clone();
    let at = r.first + r.count;
    let a = arrays(f, instance)?;
    let item =
        *f.d.children(a.assets)
            .and_then(|c| c.get(src))
            .ok_or_else(|| LevelWriteError::NotFound(src_name.clone()))?;
    let copy = f.d.clone_node(item);
    f.d.children_mut(a.assets)?.insert(at, copy);
    let mut t: Vec<Vec<u8>> = Vec::with_capacity(16);
    let (size, old) = numbers_of(f.d.raw(a.transforms)).ok_or_else(|| bad("damaged Transforms"))?;
    if size != 4 {
        return Err(bad("Transforms are not 4-byte numbers"));
    }
    for r4 in 0..4 {
        for c in 0..4 {
            let v = match rows.get(r4).and_then(|row| row.get(c)) {
                Some(x) => x.to_be_bytes().to_vec(),
                // each row's fourth word as `like`'s
                None => old
                    .get(16 * src + 4 * r4 + c)
                    .map_or_else(|| vec![0; 4], |x| x.to_vec()),
            };
            t.push(v);
        }
    }
    insert_numbers(&mut f.d, a.transforms, 16 * at, &t)?;
    if let Some(e) = a.enums {
        let size = numbers_of(f.d.raw(e)).map_or(4, |(s, _)| s) as usize;
        insert_numbers(&mut f.d, e, at, &[vec![0; size]])?;
    }
    let spawns =
        w.d.child(r.node, b"SpawnDatas")
            .ok_or_else(|| bad(format!("{src_name}: its member type has no SpawnDatas")))?;
    let src_spawn = *w.d.children(spawns).and_then(|c| c.get(src - r.first)).ok_or_else(|| {
        bad(format!(
            "{src_name}: its member type has fewer SpawnDatas than instances"
        ))
    })?;
    let sc = w.d.clone_node(src_spawn);
    set_int_field(&mut w.d, sc, b"Enumeration", 0)?;
    w.d.children_mut(spawns)?.push(sc);
    set_int_field(&mut w.d, r.node, b"InstanceCount", (r.count + 1) as i64)?;
    renumber(w, &g)?;
    order.insert(at, id);
    Ok(())
}

/// Sets the leaf of the member at `place`: its `Enums` number in the group file and its `SpawnDatas` record's
/// `Enumeration` in the partition (the export's last step for new members).
///
/// # Errors
/// No such member, or the group is not in the partition.
pub fn set_enum(
    f: &mut LevelWriter,
    w: &mut LevelWriter,
    file: &str,
    instance: &str,
    place: usize,
    leaf: i64,
) -> Result<()> {
    let a = arrays(f, instance)?;
    if let Some(e) = a.enums {
        let (size, mut v) = ints(&f.d, e).ok_or_else(|| bad("damaged Enums"))?;
        *v.get_mut(place).ok_or_else(|| bad("a member past the end of Enums"))? = leaf;
        set_ints(&mut f.d, e, size, &v)?;
    }
    let g = group_entity(w, file).ok_or_else(|| bad(format!("the level partition has no group for {file}")))?;
    let rs = runs(w, &g)?;
    let r = run_at(&rs, place).ok_or_else(|| bad("a member past its group's member types"))?;
    let s = *spawn_items(&w.d, r.node)
        .get(place - r.first)
        .ok_or_else(|| bad("a member type with fewer SpawnDatas than instances"))?;
    set_int_field(&mut w.d, s, b"Enumeration", leaf)
}

// ------------------------------------------------------------------------------------------------ the physics

/// A model group's physics resource (`<Group>_HavokAsset.res`) as round SG1 reads it (spec HAVOK-RUN-2): DICE's
/// wrapper in the material-table form, then the packfile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupPhysics {
    /// One word per physics part of every instance: the owning instance's leaf (`Enums`).
    pub part_words: Vec<u32>,
    /// The bytes from the vector count up to the material table's end (vectors, boxes, the table): per member type,
    /// kept as they are.
    pub types: Vec<u8>,
    /// The number of member types (`m`) and the type parts' end positions (cumulative per-instance part counts).
    pub ends: Vec<u16>,
    /// The part materials: `d` (distinct), `t` (highest), and one byte per part when `d` is not 1.
    pub distinct: u8,
    /// The highest material index.
    pub highest: u8,
    /// One material byte per part (empty when `distinct` is 1: every part takes `highest`).
    pub materials: Vec<u8>,
    /// The material list and the packfile, kept as they are.
    pub tail: Vec<u8>,
}

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .and_then(|x| <[u8; 4]>::try_from(x).ok())
        .map(u32::from_le_bytes)
}

fn u16_le(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2)
        .and_then(|x| <[u8; 2]>::try_from(x).ok())
        .map(u16::from_le_bytes)
}

impl GroupPhysics {
    /// Reads a PC model group's physics resource.
    ///
    /// # Errors
    /// Not a model group's wrapper (the part counts differ, the table is not ascending, the run does not end at
    /// the material list).
    pub fn read(b: &[u8]) -> Result<Self> {
        let damaged = |why: &str| bad(format!("the group's physics resource: {why}"));
        let n = u32_le(b, 0).ok_or_else(|| damaged("too short"))? as usize;
        if u32_le(b, 4) != Some(n as u32) {
            return Err(damaged("its two part counts differ"));
        }
        let part_words = (0..n)
            .map(|i| u32_le(b, 8 + 4 * i))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| damaged("too short for its parts"))?;
        let types_at = 8 + 4 * n;
        let mut o = types_at;
        let a = u16_le(b, o).ok_or_else(|| damaged("no vector count"))? as usize;
        o += 2 + 12 * a;
        let c = u16_le(b, o).ok_or_else(|| damaged("no box count"))? as usize;
        o += 2 + 24 * c;
        let m = u16_le(b, o).ok_or_else(|| damaged("no material table"))? as usize;
        let k2 = u16_le(b, o + 2).ok_or_else(|| damaged("no material table"))? as usize;
        if m == 0 {
            return Err(damaged(
                "not the material-table form (a blueprint's physics, not a group's)",
            ));
        }
        let ends: Vec<u16> = (0..m)
            .map(|i| u16_le(b, o + 4 + 4 * k2 + 2 * i))
            .collect::<Option<_>>()
            .ok_or_else(|| damaged("too short for its table"))?;
        if ends.windows(2).any(|w| w.first() > w.get(1)) || ends.last().map(|e| *e as usize) != Some(k2) {
            return Err(damaged("its table's end positions do not ascend to its type parts"));
        }
        let run = o + 4 + 4 * k2 + 2 * m;
        let types = b.get(types_at..run).ok_or_else(|| damaged("too short"))?.to_vec();
        let distinct = *b.get(run).ok_or_else(|| damaged("no part run"))?;
        let highest = *b.get(run + 1).ok_or_else(|| damaged("no part run"))?;
        let mat_len = if distinct == 1 { 0 } else { n };
        let materials = b
            .get(run + 2..run + 2 + mat_len)
            .ok_or_else(|| damaged("too short for its part materials"))?
            .to_vec();
        let tail = b.get(run + 2 + mat_len..).unwrap_or_default().to_vec();
        // the tail opens with 1, k, k material hashes, then the packfile
        let k = u32_le(&tail, 4).unwrap_or(u32::MAX) as usize;
        let magic_at = 8 + 4 * k;
        if u32_le(&tail, 0) != Some(1) || tail.get(magic_at..magic_at + 8) != Some(&bc2_formats::havok::MAGIC[..]) {
            return Err(damaged("its part run does not end at the material list"));
        }
        Ok(Self {
            part_words,
            types,
            ends,
            distinct,
            highest,
            materials,
            tail,
        })
    }

    /// The resource's bytes.
    #[must_use]
    pub fn write(&self) -> Vec<u8> {
        let n = self.part_words.len() as u32;
        let mut out = Vec::with_capacity(16 + 5 * self.part_words.len() + self.types.len() + self.tail.len());
        out.extend(n.to_le_bytes());
        out.extend(n.to_le_bytes());
        for w in &self.part_words {
            out.extend(w.to_le_bytes());
        }
        out.extend_from_slice(&self.types);
        out.push(self.distinct);
        out.push(self.highest);
        out.extend_from_slice(&self.materials);
        out.extend_from_slice(&self.tail);
        out
    }

    /// The physics parts of one instance of each member type (`ends` as counts).
    #[must_use]
    pub fn parts_per_type(&self) -> Vec<usize> {
        let mut prev = 0usize;
        self.ends
            .iter()
            .map(|&e| {
                let n = (e as usize).saturating_sub(prev);
                prev = e as usize;
                n
            })
            .collect()
    }

    /// The material bytes of one instance of each member type: the first instance's (every instance of a type has
    /// the same, spec HAVOK-RUN-3); `None` per type with no instance, or when every part takes [`Self::highest`].
    ///
    /// # Errors
    /// `instances` does not count the resource's parts.
    pub fn type_materials(&self, instances: &[usize]) -> Result<Vec<Option<Vec<u8>>>> {
        let per = self.parts_per_type();
        if per.len() != instances.len()
            || per.iter().zip(instances).map(|(p, c)| p * c).sum::<usize>() != self.part_words.len()
        {
            return Err(bad(
                "the group's physics resource does not count its member types' parts as the partition does",
            ));
        }
        let mut out = Vec::with_capacity(per.len());
        let mut at = 0usize;
        for (p, c) in per.iter().zip(instances) {
            out.push(if self.materials.is_empty() || *c == 0 {
                None
            } else {
                self.materials.get(at..at + p).map(<[u8]>::to_vec)
            });
            at += p * c;
        }
        Ok(out)
    }

    /// The resource for a group laid out as `layout` (per member type in order: its instances' leaves), the member
    /// types', the material list's and the packfile's bytes kept: one word (the instance's leaf) and one material
    /// byte (its type's) per part. With `layout` the resource's own, its bytes again.
    ///
    /// # Errors
    /// `base` (each type's instance count in this resource) does not count its parts, or `layout` names another
    /// number of member types.
    pub fn relaid(&self, base: &[usize], layout: &[Vec<i64>]) -> Result<Self> {
        let per = self.parts_per_type();
        let mats = self.type_materials(base)?;
        if layout.len() != per.len() {
            return Err(bad(format!(
                "the group has {} member types and its physics {}",
                layout.len(),
                per.len()
            )));
        }
        let mut out = self.clone();
        out.part_words.clear();
        out.materials.clear();
        let short = self.materials.is_empty();
        for ((p, leaves), m) in per.iter().zip(layout).zip(&mats) {
            for &leaf in leaves {
                out.part_words.extend(std::iter::repeat_n(leaf as u32, *p));
                if !short {
                    match m {
                        Some(m) => out.materials.extend_from_slice(m),
                        None => {
                            return Err(bad(
                                "a member type with no instance in the base gets one: its part materials are not known",
                            ));
                        }
                    }
                }
            }
        }
        if !short && !out.materials.is_empty() {
            // `d` the distinct materials, `t` the highest; a run down to one material takes the short form (with
            // d = 1 the reader takes no bytes: every part uses `t`, HAVOK-RUN-1)
            let mut s = out.materials.clone();
            s.sort_unstable();
            s.dedup();
            out.distinct = s.len() as u8;
            out.highest = s.last().copied().unwrap_or(self.highest);
            if out.distinct == 1 {
                out.materials.clear();
            }
        }
        Ok(out)
    }
}

/// The layout of a group as its partition counts it: per member type, its instances' leaves.
///
/// # Errors
/// As [`runs`].
pub fn layout(w: &LevelWriter, g: &GroupEntity) -> Result<Vec<Vec<i64>>> {
    Ok(runs(w, g)?.into_iter().map(|r| r.enums).collect())
}

// ------------------------------------------------------------------------------------------------ the checks

/// One finding of [`check_group`]: whether it holds, and the rule in words.
pub type Finding = (bool, String);

#[allow(clippy::too_many_lines, reason = "one rule after another, read top to bottom")]
/// A group's files held to the rules every DICE group keeps (spec LEVEL-MG-5 - LEVEL-MG-8: 131 of 131 groups of 31
/// levels): the group file's members as the partition counts them (assets, transforms, leaves, in type order), the
/// cumulative part and network numbers, every type with an instance, the leaves unique and in the client tree, and
/// the physics resource's part words and materials as the partition lays the group out.
#[must_use]
pub fn check_group(
    w: &LevelWriter,
    g: &GroupEntity,
    file: &LevelWriter,
    res: Option<&[u8]>,
    client_leaves: Option<u32>,
) -> Vec<Finding> {
    let mut out = Vec::new();
    let name = g.name.rsplit('/').next().unwrap_or(&g.name).to_owned();
    let mut say = |ok: bool, what: String| out.push((ok, format!("{name}: {what}")));
    let rs = match runs(w, g) {
        Ok(r) => r,
        Err(e) => {
            say(false, e.to_string());
            return out;
        }
    };
    let total: usize = rs.iter().map(|r| r.count).sum();
    let inst = file
        .d
        .instances()
        .into_iter()
        .find(|&e| file.d.child(e, b"Assets").is_some() && file.d.child(e, b"Transforms").is_some());
    let Some(inst) = inst else {
        say(false, "the group file has no Assets and Transforms".into());
        return out;
    };
    let guid = guid_of(&file.d, inst);
    let assets = member_count(file, &guid).unwrap_or(0);
    let transforms = file
        .d
        .child(inst, b"Transforms")
        .and_then(|t| numbers_of(file.d.raw(t)).map(|(_, v)| v.len()))
        .unwrap_or(0);
    let enums = file
        .d
        .child(inst, b"Enums")
        .and_then(|e| ints(&file.d, e))
        .map_or_else(Vec::new, |(_, v)| v);
    let spawn: Vec<i64> = rs.iter().flat_map(|r| r.enums.iter().copied()).collect();
    say(
        assets == total && transforms == 16 * assets,
        format!(
            "{assets} members in its file ({transforms} transform numbers); its {} member types count {total}",
            rs.len()
        ),
    );
    say(
        rs.iter().all(|r| r.count > 0 && r.enums.len() == r.count),
        "every member type has instances and one SpawnDatas record per instance".into(),
    );
    say(
        enums == spawn,
        "the file's Enums are the SpawnDatas' Enumerations, in order".into(),
    );
    let mut sorted = spawn.clone();
    sorted.sort_unstable();
    sorted.dedup();
    let in_tree = client_leaves.is_none_or(|l| spawn.iter().all(|&e| e >= 1 && e <= i64::from(l)));
    say(
        sorted.len() == spawn.len() && in_tree,
        match client_leaves {
            Some(l) => format!("every member's leaf unique and a leaf of the client tree (1 - {l})"),
            None => "every member's leaf unique".into(),
        },
    );
    let (mut part, mut net, mut ok_p, mut ok_n) = (0i64, 0i64, true, true);
    for r in &rs {
        let d = &w.d;
        let per = i64::from(r.parts);
        ok_p &= int_field(d, r.node, b"FirstPhysicsPart") == Some(part)
            && int_field(d, r.node, b"PhysicsPartCount") == Some(part + per * r.count as i64);
        part += per * r.count as i64;
        if d.child(r.node, b"FirstNetworkId").is_some() {
            let pn = int_field(d, r.node, b"NetworkIdCountPerInstance").unwrap_or(0);
            ok_n &= int_field(d, r.node, b"FirstNetworkId") == Some(net)
                && int_field(d, r.node, b"NetworkIdCount") == Some(net + pn * r.count as i64);
            net += pn * r.count as i64;
        }
    }
    say(
        ok_p && ok_n,
        format!("physics parts (0 - {part}) and network ids (0 - {net}) cumulative over its member types"),
    );
    // the assets in type order: each member's Assets reference under its type's blueprint folder
    if let Some(res) = res {
        match GroupPhysics::read(res) {
            Ok(p) => {
                let per = p.parts_per_type();
                say(
                    per.len() == rs.len() && per.iter().zip(&rs).all(|(a, r)| *a == r.parts as usize),
                    format!(
                        "its physics has {} member types with the partition's parts per instance",
                        per.len()
                    ),
                );
                let words: Vec<u32> = rs
                    .iter()
                    .flat_map(|r| {
                        r.enums
                            .iter()
                            .flat_map(move |&e| std::iter::repeat_n(e as u32, r.parts as usize))
                    })
                    .collect();
                say(
                    p.part_words == words,
                    format!(
                        "its physics' {} part words are each part's instance leaf",
                        p.part_words.len()
                    ),
                );
                let counts: Vec<usize> = rs.iter().map(|r| r.count).collect();
                let mats_ok = p.materials.is_empty() && p.distinct == 1
                    || p.materials.len() == p.part_words.len() && {
                        let mut at = 0usize;
                        let mut same = true;
                        for (pp, c) in per.iter().zip(&counts) {
                            let first = p.materials.get(at..at + pp);
                            for i in 0..*c {
                                same &= p.materials.get(at + i * pp..at + (i + 1) * pp) == first;
                            }
                            at += pp * c;
                        }
                        let mut s = p.materials.clone();
                        s.sort_unstable();
                        s.dedup();
                        same && s.len() == p.distinct as usize && s.last() == Some(&p.highest)
                    };
                say(
                    mats_ok,
                    "its physics' part materials: one byte per part, the same for each instance of a type".into(),
                );
            }
            Err(e) => say(false, e.to_string()),
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
pub(crate) mod tests {
    use super::*;
    use bc2_formats::dbx::{DbxFile, Element};

    pub(crate) const G: &str = "GGGGGGGG-0000-0000-0000-000000000001";
    pub(crate) const FILE: &str = "staticmodelgroupentity_havokasset.dbx";

    /// A group file: three members (crate, crate, rock), leaves 1 - 3, crates at x = 0 and 7, the rock at x = 20.
    pub(crate) fn group_file() -> Vec<u8> {
        let mut f = DbxFile::default();
        let mut t = Vec::new();
        for x in [0.0f32, 7.0, 20.0] {
            for v in [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, 1.0, 2.0, 1.0f32,
            ] {
                t.extend(v.to_be_bytes());
            }
        }
        let tr = Element::numbers(&mut f, "array", &[("name", "Transforms")], 4, &t);
        let e: Vec<u8> = [1u32, 2, 3].iter().flat_map(|v| v.to_be_bytes()).collect();
        let en = Element::numbers(&mut f, "array", &[("name", "Enums")], 4, &e);
        let items: Vec<Element> = [
            "objects/props/crate/crate_havok/x",
            "objects/props/crate/crate_havok/x",
            "objects/props/rock/rock_havok/y",
        ]
        .iter()
        .map(|r| Element::text(&mut f, "item", &[("ref", r)], ""))
        .collect();
        let assets = Element::container(&mut f, "array", &[("name", "Assets")], items);
        let inst = Element::container(
            &mut f,
            "instance",
            &[("guid", G), ("type", "Physics.CompositeHavokAsset")],
            vec![assets, tr, en],
        );
        let part = Element::container(&mut f, "partition", &[("guid", G)], vec![inst]);
        f.roots.push(part);
        bc2_formats::dbx::write(&f).unwrap()
    }

    fn int(f: &mut DbxFile, name: &str, v: i32) -> Element {
        Element::numbers(f, "field", &[("name", name)], 4, &v.to_be_bytes())
    }

    /// A level partition with the group: crates (2 instances, 2 parts and 1 network id each) and the rock (1, 1
    /// part, no network id).
    pub(crate) fn partition() -> Vec<u8> {
        let mut f = DbxFile::default();
        let spawn = |f: &mut DbxFile, e: i32| {
            let en = int(f, "Enumeration", e);
            let ss = Element::text(f, "field", &[("ref", "null"), ("name", "MeshShaderSet")], "");
            Element::container(f, "complex", &[], vec![en, ss])
        };
        let member = |f: &mut DbxFile, guid: &str, name: &str, enums: &[i32], per: i32, net: i32| {
            let spawns: Vec<Element> = enums.iter().map(|&e| spawn(f, e)).collect();
            let sd = Element::container(f, "array", &[("name", "SpawnDatas")], spawns);
            let nm = Element::text(f, "field", &[("name", "Name")], name);
            let fields = vec![
                sd,
                nm,
                int(f, "FirstPhysicsPart", 0),
                int(f, "PhysicsPartCount", 0),
                int(f, "PhysicsPartCountPerInstance", per),
                int(f, "InstanceCount", enums.len() as i32),
                int(f, "FirstNetworkId", 0),
                int(f, "NetworkIdCount", 0),
                int(f, "NetworkIdCountPerInstance", net),
            ];
            Element::container(
                f,
                "instance",
                &[
                    ("guid", guid),
                    ("type", "GameSharedResources.StaticModelGroupMemberData"),
                ],
                fields,
            )
        };
        let m1 = member(
            &mut f,
            "MMMMMMMM-0000-0000-0000-000000000001",
            "Objects/Props/Crate",
            &[1, 2],
            2,
            1,
        );
        let m2 = member(
            &mut f,
            "MMMMMMMM-0000-0000-0000-000000000002",
            "Objects/Props/Rock",
            &[3],
            1,
            0,
        );
        let items: Vec<Element> = [
            "mmmmmmmm-0000-0000-0000-000000000001",
            "mmmmmmmm-0000-0000-0000-000000000002",
        ]
        .iter()
        .map(|r| Element::text(&mut f, "item", &[("ref", r)], ""))
        .collect();
        let md = Element::container(&mut f, "array", &[("name", "MemberDatas")], items);
        let nm = Element::text(
            &mut f,
            "field",
            &[("name", "Name")],
            "Levels/Test/StaticModelGroupEntity",
        );
        let ge = Element::container(
            &mut f,
            "instance",
            &[
                ("guid", "EEEEEEEE-0000-0000-0000-000000000001"),
                ("type", "GameSharedResources.StaticModelGroupEntityData"),
            ],
            vec![nm, md],
        );
        let ld = Element::container(
            &mut f,
            "instance",
            &[
                ("guid", "LLLLLLLL-0000-0000-0000-000000000001"),
                ("type", "Entity.LevelData"),
            ],
            vec![],
        );
        let part = Element::container(&mut f, "partition", &[("guid", "P")], vec![ld, ge, m1, m2]);
        f.roots.push(part);
        let mut w = LevelWriter::parse(&bc2_formats::dbx::write(&f).unwrap()).unwrap();
        let g = group_entity(&w, FILE).unwrap();
        renumber(&mut w, &g).unwrap();
        w.write().unwrap()
    }

    /// The group's physics: crates 2 parts per instance (materials 0, 2), the rock 1 (material 1); leaves 1 - 3.
    pub(crate) fn physics() -> Vec<u8> {
        let mut b = Vec::new();
        let words = [1u32, 1, 2, 2, 3];
        b.extend(5u32.to_le_bytes());
        b.extend(5u32.to_le_bytes());
        for w in words {
            b.extend(w.to_le_bytes());
        }
        b.extend(0u16.to_le_bytes()); // vectors
        b.extend(3u16.to_le_bytes()); // boxes, one per type part
        b.extend(std::iter::repeat_n(0u8, 3 * 24));
        b.extend(2u16.to_le_bytes()); // m
        b.extend(3u16.to_le_bytes()); // k2
        for h in [11u32, 12, 13] {
            b.extend(h.to_le_bytes());
        }
        for e in [2u16, 3] {
            b.extend(e.to_le_bytes());
        }
        b.extend([3u8, 2]); // d, t
        b.extend([0u8, 2, 0, 2, 1]);
        b.extend(1u32.to_le_bytes());
        b.extend(1u32.to_le_bytes());
        b.extend(77u32.to_le_bytes());
        b.extend(bc2_formats::havok::MAGIC);
        b.extend([0u8; 8]);
        b
    }

    #[test]
    fn identities_read_back() {
        assert_eq!(member_guid("abc", Member::Base(3)), "ABC#3");
        assert_eq!(member_guid("abc", Member::Added { added: 2 }), "ABC#+2");
        assert_eq!(
            parse_member_guid("ABC#+2"),
            Some(("ABC".into(), Member::Added { added: 2 }))
        );
        assert_eq!(parse_member_guid("ABC#12"), Some(("ABC".into(), Member::Base(12))));
        assert_eq!(parse_member_guid("ABC"), None);
        // JSON: a base member is the number round OP1 wrote, an added one a small object
        assert_eq!(serde_json::to_string(&Member::Base(4)).unwrap(), "4");
        assert_eq!(
            serde_json::to_string(&Member::Added { added: 1 }).unwrap(),
            "{\"added\":1}"
        );
        assert_eq!(serde_json::from_str::<Member>("4").unwrap(), Member::Base(4));
    }

    #[test]
    fn the_physics_reads_and_relays() {
        let b = physics();
        let p = GroupPhysics::read(&b).unwrap();
        assert_eq!(p.write(), b, "read and written again: the same bytes");
        assert_eq!(p.parts_per_type(), [2, 1]);
        // its own layout: the same bytes
        assert_eq!(p.relaid(&[2, 1], &[vec![1, 2], vec![3]]).unwrap().write(), b);
        // a crate deleted, two crates added
        let q = p.relaid(&[2, 1], &[vec![2, 4, 5], vec![3]]).unwrap();
        assert_eq!(q.part_words, [2, 2, 4, 4, 5, 5, 3]);
        assert_eq!(q.materials, [0, 2, 0, 2, 0, 2, 1]);
        assert_eq!((q.distinct, q.highest), (3, 2));
        // the rock gone from the layout's count: refused
        assert!(p.relaid(&[2, 1], &[vec![1, 2]]).is_err());
    }

    #[test]
    fn members_are_deleted_and_added_in_dices_layout() {
        let mut f = LevelWriter::parse(&group_file()).unwrap();
        let mut w = LevelWriter::parse(&partition()).unwrap();
        let mut order = base_order(&f, G).unwrap();
        let g = group_entity(&w, FILE).unwrap();
        let res = physics();
        let check = |w: &LevelWriter, f: &LevelWriter, res: &[u8]| {
            let g = group_entity(w, FILE).unwrap();
            for (ok, what) in check_group(w, &g, f, Some(res), Some(9)) {
                assert!(ok, "{what}");
            }
        };
        check(&w, &f, &res);
        // the rock is the last of its kind: refused, nothing changed
        let (f0, w0) = (f.write().unwrap(), w.write().unwrap());
        let e = delete(&mut f, &mut w, FILE, G, &mut order, Member::Base(2)).unwrap_err();
        assert!(e.to_string().contains("is the last Rock of its group"), "{e}");
        assert_eq!((f.write().unwrap(), w.write().unwrap()), (f0, w0));
        // delete crate #0, add a crate like #1 at x = 30
        delete(&mut f, &mut w, FILE, G, &mut order, Member::Base(0)).unwrap();
        assert_eq!(order, [Member::Base(1), Member::Base(2)]);
        let new = Member::Added { added: 1 };
        add(
            &mut f,
            &mut w,
            FILE,
            G,
            &mut order,
            Member::Base(1),
            new,
            &crate::rows_at([30.0, 1.0, 2.0]),
        )
        .unwrap();
        assert_eq!(order, [Member::Base(1), new, Member::Base(2)], "after its type's run");
        assert!(
            add(
                &mut f,
                &mut w,
                FILE,
                G,
                &mut order,
                Member::Base(1),
                new,
                &crate::rows_at([0.0; 3])
            )
            .is_err()
        );
        let rs = runs(&w, &g).unwrap();
        assert_eq!((rs[0].count, rs[1].first), (2, 2));
        assert_eq!(rs[0].enums, [2, 0], "the new member's leaf is given at export");
        assert_eq!(asset_at(&f, G, 1).unwrap(), "objects/props/crate/crate_havok/x");
        assert_eq!(crate::files::member_transform(&f, G, 1).unwrap()[3], [30.0, 1.0, 2.0]);
        assert_eq!(crate::files::member_transform(&f, G, 2).unwrap()[3], [20.0, 1.0, 2.0]);
        // the cumulative numbers again: crates parts 0 - 4, network 0 - 2; the rock parts 4 - 5
        assert_eq!(int_field(&w.d, rs[1].node, b"FirstPhysicsPart"), Some(4));
        assert_eq!(int_field(&w.d, rs[1].node, b"PhysicsPartCount"), Some(5));
        assert_eq!(int_field(&w.d, rs[0].node, b"NetworkIdCount"), Some(2));
        // its leaf given, the physics laid out again: every rule holds
        set_enum(&mut f, &mut w, FILE, G, 1, 1).unwrap();
        let lay = layout(&w, &g).unwrap();
        assert_eq!(lay, [vec![2, 1], vec![3]]);
        let p = GroupPhysics::read(&res).unwrap().relaid(&[2, 1], &lay).unwrap();
        check(&w, &f, &p.write());
        // a leaf left at 0 is caught
        set_enum(&mut f, &mut w, FILE, G, 1, 0).unwrap();
        let g2 = group_entity(&w, FILE).unwrap();
        assert!(check_group(&w, &g2, &f, None, Some(9)).iter().any(|(ok, _)| !ok));
    }
}
