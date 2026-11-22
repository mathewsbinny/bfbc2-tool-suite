//! Static group members edited like any object (round SG1): the views' operations (move, turn, delete, duplicate,
//! paste, place into a group) name entities by guid; an operation that names a static group member is turned here
//! into the multi-file operation that edits the member's group file (and, for a delete or an add, its member type
//! in the partition): [`lower`]. An operation that names no member is left exactly as it was, so projects without
//! member edits write the same log and export the same files as before.
//!
//! Members are named by identity ([`bc2_levelwrite::members`]): a base member is `<group instance>#<k>` for the
//! whole project, a member the project adds `<group instance>#+<n>` (`n` counted per group instance over the log).

use bc2_levelwrite::files::{FileEdit, Member};
use bc2_levelwrite::members;
use bc2_levelwrite::ops::{Duplicated, Moved};
use bc2_levelwrite::{LevelWriter, Op};
use bc2_scene::scene::{Entity, Source};

use crate::edit::Doc;

/// A static group member as the edits name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemberRef {
    /// Its group file's name (`StaticModelGroupEntity_HavokAsset.dbx`).
    pub file: String,
    /// The group instance's guid (upper case).
    pub instance: String,
    /// The member.
    pub member: Member,
}

/// The member an entity is, `None` for any other entity.
#[must_use]
pub fn member_of(e: &Entity) -> Option<MemberRef> {
    if !matches!(e.source, Source::GroupMember { .. }) {
        return None;
    }
    let (instance, member) = members::parse_member_guid(&e.guid)?;
    Some(MemberRef {
        file: e.group.clone()?,
        instance,
        member,
    })
}

/// The member a guid names in the document's scene.
fn member_named(doc: &Doc, guid: &str) -> Option<MemberRef> {
    doc.index(guid)
        .and_then(|k| doc.scene.entities.get(k))
        .and_then(member_of)
}

/// The next number for a member added to `instance`: one more than any the log (and its redo) or `taken` gives.
fn next_added(doc: &Doc, instance: &str, taken: &[(String, u32)]) -> u32 {
    let up = instance.to_ascii_uppercase();
    let mut top = 0u32;
    for o in doc.project.operations.iter().chain(&doc.project.redo) {
        if let Op::Files { edits } = &o.op {
            for e in edits {
                if let FileEdit::MemberAdd {
                    instance: i,
                    item: Member::Added { added },
                    ..
                } = e
                    && i.eq_ignore_ascii_case(&up)
                {
                    top = top.max(*added);
                }
            }
        }
    }
    for (i, n) in taken {
        if *i == up {
            top = top.max(*n);
        }
    }
    top + 1
}

/// A member's group file as the log leaves it (else the base's), to read its `Assets`.
fn group_file(doc: &Doc, file: &str) -> Option<std::sync::Arc<LevelWriter>> {
    let t = bc2_levelwrite::files::Target::group(file);
    let bc2_levelwrite::files::Target::Group { file: low } = &t else {
        return None;
    };
    if let Some(w) = doc.files().groups.get(low) {
        return Some(std::sync::Arc::clone(w));
    }
    LevelWriter::parse(&doc.base_file(&t)?).ok().map(std::sync::Arc::new)
}

/// A member's place in its group file now.
fn place(doc: &Doc, m: &MemberRef) -> Option<usize> {
    let bc2_levelwrite::files::Target::Group { file } = bc2_levelwrite::files::Target::group(&m.file) else {
        return None;
    };
    let key = (file, m.instance.clone());
    match doc.files().orders.get(&key) {
        Some(order) => members::place_of(order, m.member),
        None => match m.member {
            Member::Base(k) => Some(k),
            Member::Added { .. } => None,
        },
    }
}

/// The operation with its static group members' parts made group file edits ([`FileEdit`]): moves and turns,
/// deletes, duplicates (an added member of the source's type). The partition's part stays an operation of its own
/// kind (inside the multi-file operation when there are member edits too). An operation naming no member, or of
/// another kind, is returned as it is.
#[must_use]
pub fn lower(doc: &Doc, op: Op) -> Op {
    let mut edits: Vec<FileEdit> = Vec::new();
    let rotate = matches!(op, Op::Rotate { .. });
    let rest: Option<Op> = match op {
        Op::Move { items } | Op::Rotate { items } if items.iter().any(|m| member_named(doc, &m.guid).is_some()) => {
            let mut keep: Vec<Moved> = Vec::new();
            for m in items {
                match member_named(doc, &m.guid) {
                    Some(r) => edits.push(FileEdit::MemberTransform {
                        file: r.file,
                        instance: r.instance,
                        item: r.member,
                        from: m.from,
                        to: m.to,
                    }),
                    None => keep.push(m),
                }
            }
            (!keep.is_empty()).then_some(if rotate {
                Op::Rotate { items: keep }
            } else {
                Op::Move { items: keep }
            })
        }
        Op::Delete { guids } if guids.iter().any(|g| member_named(doc, g).is_some()) => {
            let mut keep = Vec::new();
            for g in guids {
                match member_named(doc, &g) {
                    Some(r) => {
                        let f = group_file(doc, &r.file);
                        let asset = place(doc, &r)
                            .and_then(|p| f.as_ref().and_then(|f| members::asset_at(f, &r.instance, p)))
                            .unwrap_or_default();
                        let rows = doc
                            .index(&g)
                            .and_then(|k| doc.scene.entities.get(k))
                            .and_then(|e| e.matrix)
                            .map_or_else(|| bc2_levelwrite::rows_at([0.0; 3]), |m| bc2_levelwrite::rows_of(&m));
                        edits.push(FileEdit::MemberDelete {
                            file: r.file,
                            instance: r.instance,
                            item: r.member,
                            asset,
                            rows,
                        });
                    }
                    None => keep.push(g),
                }
            }
            (!keep.is_empty()).then_some(Op::Delete { guids: keep })
        }
        Op::Duplicate { items } if items.iter().any(|d| member_named(doc, &d.source).is_some()) => {
            let mut keep: Vec<Duplicated> = Vec::new();
            let mut taken: Vec<(String, u32)> = Vec::new();
            for d in items {
                match member_named(doc, &d.source) {
                    Some(r) => {
                        let n = next_added(doc, &r.instance, &taken);
                        taken.push((r.instance.clone(), n));
                        edits.push(FileEdit::MemberAdd {
                            file: r.file,
                            instance: r.instance,
                            item: Member::Added { added: n },
                            like: r.member,
                            rows: d.transform,
                        });
                    }
                    None => keep.push(d),
                }
            }
            (!keep.is_empty()).then_some(Op::Duplicate { items: keep })
        }
        other => return other,
    };
    let mut all: Vec<FileEdit> = rest
        .map(|op| FileEdit::Partition { op: Box::new(op) })
        .into_iter()
        .collect();
    all.extend(edits);
    Op::Files { edits: all }
}

/// The guids of the entities an operation makes in the scene: a partition operation's new objects and the members
/// it adds (`<instance>#+<n>`).
#[must_use]
pub fn created(op: &Op) -> Vec<String> {
    let mut out: Vec<String> = op.creates().into_iter().map(str::to_owned).collect();
    if let Op::Files { edits } = op {
        for e in edits {
            if let FileEdit::MemberAdd { instance, item, .. } = e {
                out.push(members::member_guid(instance, *item));
            }
        }
    }
    out
}

/// A member of `file` (a group file's name, any case) whose blueprint is `blueprint` (a path, lower case), to add a
/// placed copy of that blueprint to the group as: its kind is in the group (its collision is in the group's
/// physics, per kind).
#[must_use]
pub fn member_like(doc: &Doc, file: &str, blueprint: &str) -> Option<usize> {
    doc.scene.entities.iter().find_map(|e| {
        (doc.shown(e.index)
            && e.blueprint.as_deref() == Some(blueprint)
            && e.group.as_deref().is_some_and(|g| g.eq_ignore_ascii_case(file))
            && member_of(e).is_some())
        .then_some(e.index)
    })
}

/// The model groups of the scene (their files), in the scene's order, each with its member count.
#[must_use]
pub fn groups(doc: &Doc) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for e in &doc.scene.entities {
        if member_of(e).is_none() || !doc.shown(e.index) {
            continue;
        }
        let Some(g) = e.group.as_ref() else { continue };
        match out.iter_mut().find(|(f, _)| f == g) {
            Some((_, n)) => *n += 1,
            None => out.push((g.clone(), 1)),
        }
    }
    out
}

/// A member's description for the properties panel: its group, kind, and place in the group file now.
#[must_use]
pub fn describe(doc: &Doc, k: usize) -> Option<String> {
    let e = doc.scene.entities.get(k)?;
    let r = member_of(e)?;
    let group = r.file.trim_end_matches(".dbx").trim_end_matches("_HavokAsset");
    let place = place(doc, &r).map_or_else(|| "-".to_owned(), |p| p.to_string());
    Some(format!(
        "Static group member #{} of {group}: {}, place {place} in its group file{}",
        r.member,
        e.name.clone().unwrap_or_default(),
        if matches!(r.member, Member::Added { .. }) {
            " (added by this project)"
        } else {
            ""
        }
    ))
}
