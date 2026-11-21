//! The export of static group member edits (round SG1): the group files the log edited, their physics resources and
//! the client cull tree, made consistent the way DICE's levels are (spec XLOG-MEM-2, LEVEL-MG-5 - LEVEL-MG-8).
//!
//! - **Leaves.** Every member has one leaf of the client cull tree (`Enums` / `SpawnDatas[].Enumeration`,
//!   CULL-SLOT-1). A moved member keeps its leaf, moved to where it stands now ([`CullTree::reseat`]: nothing that
//!   names the leaf changes); a deleted member's leaf is freed; an added member takes a freed leaf (reseated), else
//!   a new leaf at the client tree's end. New leaves move the exclusive tree's slots up by as many (its `EnumOffset`
//!   is the client tree's leaf count in every stock pair): the export moves `EnumOffset` and every placed object's
//!   slot after the cull tree's own reslot ([`crate::culltree::shift_slots`]). A leaf's sphere is the member's own
//!   leaf in the base (or, for an added member, the leaf of the base member it copies) carried by the member's
//!   transforms.
//! - **Physics.** The group's `.res` is laid out again for the partition's member types ([`GroupPhysics::relaid`]):
//!   one word per part (its instance's leaf) and one material byte per part (its type's). The collision itself is
//!   per member type, placed by the game at each instance's transform: a moved or added member's collision follows
//!   its transform with no change to the packfile.

use std::collections::{BTreeMap, HashMap};

use bc2_formats::dbx::edit::EditDbx;

use crate::culltree::{CullTree, Sphere, apply_rows, to_local};
use crate::files::{Target, member_transform};
use crate::members::{GroupPhysics, Member, Orders, enum_at, group_entity, layout, member_guid, runs, set_enum};
use crate::write::{LevelWriteError, LevelWriter, Result};

fn bad(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

/// The client cull tree's file name in a level's folder.
pub const CLIENT_TREE: &str = "staticculltree_client.dbx";

/// What happened to one member's leaf.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeafRecord {
    /// The member (`<instance>#<k>`, `#+n`).
    pub member: String,
    /// Its leaf.
    pub leaf: u32,
    /// `moved` (its own leaf, moved), `reused` (a deleted member's leaf), `new` (a new leaf), `freed` (deleted).
    pub how: &'static str,
}

/// The files [`stage`] made, by file name in the level's folder (lower case: `staticmodelgroupentity_havokasset.dbx`,
/// `..._havokasset.res`, `staticculltree_client.dbx`), and what it did.
#[derive(Debug, Clone, Default)]
pub struct Staged {
    /// The files.
    pub files: BTreeMap<String, Vec<u8>>,
    /// The client tree's leaf count before (the exclusive tree's `EnumOffset` before).
    pub client_leaves: u32,
    /// The leaves added at the client tree's end.
    pub appended: u32,
    /// Each member's leaf as given.
    pub leaves: Vec<LeafRecord>,
    /// The report's lines.
    pub report: Vec<String>,
}

fn carry(sphere: &Sphere, from: &crate::Rows, to: &crate::Rows) -> Sphere {
    Sphere {
        c: apply_rows(to, to_local(from, sphere.c)),
        r: sphere.r,
    }
}

#[allow(clippy::too_many_lines, reason = "the steps in order: leaves freed and moved, leaves given, the files")]
/// The edited group files made whole for the export: each added member given a leaf (in its group file and its
/// partition record), each moved one its leaf moved, the client tree written when it changed, each edited group's
/// physics laid out again. `read` gives a base file of the level's folder by its lower-case name; `edited` holds the
/// log's group files ([`crate::files::AppliedFiles::files`]); `w` is the edited partition.
///
/// # Errors
/// A base file missing or damaged, a leaf the tree cannot hold, or a group whose files do not count alike.
pub fn stage(
    base: &LevelWriter,
    w: &mut LevelWriter,
    edited: &BTreeMap<Target, Vec<u8>>,
    orders: &Orders,
    added: &BTreeMap<String, Member>,
    read: &dyn Fn(&str) -> Option<Vec<u8>>,
) -> Result<Staged> {
    let mut out = Staged::default();
    let groups: Vec<(String, Vec<u8>)> = edited
        .iter()
        .filter_map(|(t, b)| match t {
            Target::Group { file } => Some((file.clone(), b.clone())),
            _ => None,
        })
        .collect();
    if groups.is_empty() {
        return Ok(out);
    }
    let tree_bytes = read(CLIENT_TREE).ok_or_else(|| bad(format!("the base level has no {CLIENT_TREE}")))?;
    let mut tree_d = EditDbx::parse(&tree_bytes).map_err(|e| bad(format!("{CLIENT_TREE}: {}", e.0)))?;
    let base_tree = CullTree::read(&tree_d)?;
    let mut tree = base_tree.clone();
    out.client_leaves = base_tree.leaf_count();
    let leaf_of = |leaf: i64| -> Result<(Sphere, f32)> {
        u32::try_from(leaf)
            .ok()
            .and_then(|l| base_tree.leaf(l))
            .ok_or_else(|| bad(format!("leaf {leaf} is not in the base's client cull tree")))
    };
    let mut bases: HashMap<String, LevelWriter> = HashMap::new();
    let mut finals: HashMap<String, LevelWriter> = HashMap::new();
    for (file, bytes) in &groups {
        let b = read(file).ok_or_else(|| bad(format!("the base level has no {file}")))?;
        bases.insert(file.clone(), LevelWriter::parse(&b)?);
        finals.insert(file.clone(), LevelWriter::parse(bytes)?);
    }
    // 1. deleted members free their leaves; moved ones keep theirs, moved where they stand
    let mut freed: Vec<u32> = Vec::new();
    let mut adds: Vec<(String, String, usize, Member, Member)> = Vec::new();
    let (mut moved, mut kept) = (0usize, 0usize);
    for ((file, inst), order) in orders {
        let (Some(f0), Some(f1)) = (bases.get(file), finals.get(file)) else {
            continue;
        };
        let count = crate::members::member_count(f0, inst)?;
        for k in 0..count {
            if !order.contains(&Member::Base(k)) {
                let leaf = enum_at(f0, inst, k).unwrap_or(0);
                if let Ok(l) = u32::try_from(leaf)
                    && l > 0
                {
                    freed.push(l);
                    out.leaves.push(LeafRecord {
                        member: member_guid(inst, Member::Base(k)),
                        leaf: l,
                        how: "freed",
                    });
                }
            }
        }
        for (p, m) in order.iter().enumerate() {
            match *m {
                Member::Base(k) => {
                    let (Some(r0), Some(r1)) = (member_transform(f0, inst, k), member_transform(f1, inst, p)) else {
                        continue;
                    };
                    if r0 == r1 {
                        kept += 1;
                        continue;
                    }
                    let leaf = enum_at(f0, inst, k).unwrap_or(0);
                    let (s, _) = leaf_of(leaf)?;
                    tree.reseat(leaf as u32, &carry(&s, &r0, &r1))?;
                    moved += 1;
                    out.leaves.push(LeafRecord {
                        member: member_guid(inst, *m),
                        leaf: leaf as u32,
                        how: "moved",
                    });
                }
                Member::Added { .. } => {
                    let root = added
                        .get(&member_guid(inst, *m))
                        .copied()
                        .ok_or_else(|| bad(format!("{}: the member it copies is not known", member_guid(inst, *m))))?;
                    adds.push((file.clone(), inst.clone(), p, *m, root));
                }
            }
        }
    }
    freed.sort_unstable();
    freed.reverse();
    // 2. added members: a freed leaf, else a new one at the client tree's end
    for (file, inst, p, m, root) in &adds {
        let Member::Base(kr) = *root else {
            return Err(bad(format!("{}: it copies no base member", member_guid(inst, *m))));
        };
        let (Some(f0), Some(f1)) = (bases.get(file), finals.get_mut(file)) else {
            continue;
        };
        let r0 = member_transform(f0, inst, kr).ok_or_else(|| bad(format!("{}: no transform", member_guid(inst, *root))))?;
        let r1 = member_transform(f1, inst, *p).ok_or_else(|| bad(format!("{}: no transform", member_guid(inst, *m))))?;
        let (s, mx) = leaf_of(enum_at(f0, inst, kr).unwrap_or(0))?;
        let placed = carry(&s, &r0, &r1);
        let (leaf, how) = if let Some(l) = freed.pop() {
            tree.reseat(l, &placed)?;
            (l, "reused")
        } else {
            (tree.add(&[(placed, mx)])?, "new")
        };
        set_enum(f1, w, file, inst, *p, i64::from(leaf))?;
        out.leaves.push(LeafRecord {
            member: member_guid(inst, *m),
            leaf,
            how,
        });
    }
    out.appended = tree.leaf_count() - out.client_leaves;
    // 3. the files: the group files, their physics laid out again, the client tree when it changed
    for (file, _) in &groups {
        let f1 = finals.get(file).ok_or_else(|| bad("internal: a group file lost"))?;
        out.files.insert(file.clone(), f1.write()?);
        let g1 = group_entity(w, file).ok_or_else(|| bad(format!("the edited level has no group for {file}")))?;
        let g0 = group_entity(base, file).ok_or_else(|| bad(format!("the base level has no group for {file}")))?;
        let res_name = format!("{}.res", file.strip_suffix(".dbx").unwrap_or(file));
        let Some(res) = read(&res_name) else {
            out.report.push(format!("  {file}: no physics resource in the base (none written)"));
            continue;
        };
        let counts: Vec<usize> = runs(base, &g0)?.iter().map(|r| r.count).collect();
        let p = GroupPhysics::read(&res).map_err(|e| bad(format!("{res_name}: {e}")))?;
        let lay = layout(w, &g1)?;
        let again = p.relaid(&counts, &lay).map_err(|e| bad(format!("{res_name}: {e}")))?.write();
        let members: usize = lay.iter().map(Vec::len).sum();
        let before: usize = counts.iter().sum();
        out.report.push(format!(
            "  {file}: {before} -> {members} members; {res_name} {} -> {} parts{}",
            p.part_words.len(),
            GroupPhysics::read(&again).map_or(0, |x| x.part_words.len()),
            if again == res { " (the same bytes)" } else { "" }
        ));
        if again != res {
            out.files.insert(res_name, again);
        }
    }
    if moved > 0 || !adds.is_empty() {
        tree.write_into(&mut tree_d)?;
        out.files.insert(CLIENT_TREE.into(), tree_d.write()?);
    }
    out.report.insert(
        0,
        format!(
            "static groups: {} group files edited; {moved} members moved (their leaves moved with them), {kept} \
             kept, {} deleted (leaves freed), {} added ({} on freed leaves, {} new leaves: client tree {} -> {})",
            groups.len(),
            out.leaves.iter().filter(|l| l.how == "freed").count(),
            adds.len(),
            out.leaves.iter().filter(|l| l.how == "reused").count(),
            out.appended,
            out.client_leaves,
            tree.leaf_count()
        ),
    );
    Ok(out)
}
