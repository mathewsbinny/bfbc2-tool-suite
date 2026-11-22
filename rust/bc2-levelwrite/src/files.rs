//! Operations on any file of a level (round OP1, audit AU1 R18 / D4): an operation names the file it changes - its
//! [`Target`] - and a project holds one writer per file ([`LevelFiles`]): the level partition, the static group files
//! (`Levels/<level>/<Name>_HavokAsset.dbx`, where Wake keeps 2,999 of its objects), the height fields, the
//! navigation side file. The project itself (layers, folders, the export's choices) is a target too: no level file.
//!
//! The operations of rounds E2 - XF1 keep their kinds and their JSON; each has a fixed target ([`Op::files`]).
//! [`Op::Files`] is the typed multi-file operation: its edits ([`FileEdit`]) may change several files, and it applies
//! **atomically** - every edit is made on a copy of the files it names, and the copies replace the files only when
//! all of them applied; an edit that fails leaves every file as it was ([`LevelFiles::apply`]). Undo and redo replay
//! from the editor's checkpoints, which hold every file's writer, so a multi-file operation is undone as one step.
//!
//! A file other than the partition is read the first time an operation names it ([`Sources`]): its base bytes from
//! the level as loaded (the editor's scene, the export's bundles). Its sha1 when first edited is recorded in the
//! project ([`crate::project::Base::files`]), so a changed base file is reported as the partition's is.

use std::collections::BTreeMap;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

pub use crate::members::Member;
use crate::members::{Order, Orders};
use crate::ops::{Op, Operation, Rows};
use crate::schema::Schema;
use crate::write::{Applied, ApplyError, Donors, LevelWriteError, LevelWriter, apply_one_with, apply_partition_edits};

/// A file of a level an operation changes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Target {
    /// The level partition (`Levels/<Level>.dbx`).
    Partition,
    /// A static group file: its name in the level's folder, lower case
    /// (`staticmodelgroupentity_grid00x00_havokasset.dbx`).
    Group {
        /// The file's name.
        file: String,
    },
    /// A height field (`terrains/<a>/<b>.heightfield-000.res`, lower case): brush strokes ([`crate::delta`]).
    Terrain {
        /// The file's name.
        file: String,
    },
    /// A terrain paint mask (round E5's paint brush): a raster of the project's own, one byte per sample, named by
    /// the layer it paints ([`crate::delta`]).
    Paint {
        /// The layer.
        layer: String,
    },
    /// The level's navigation side file (round GR2: the AI workspace's edits, for NavKit).
    Navigation,
    /// The project itself: its organisation (layers, folders, groups, hide, lock) and the export's choices (campaign
    /// content, names, weather). No level file.
    Project,
}

impl Target {
    /// A static group file's target from its path or name (the folder dropped, lower case).
    #[must_use]
    pub fn group(path: &str) -> Self {
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        Self::Group {
            file: name.to_ascii_lowercase(),
        }
    }

    /// The target in words, as the History and the Problems panel name it.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Partition => "the level partition".into(),
            Self::Group { file } => format!("the static group file {file}"),
            Self::Terrain { file } => format!("the height field {file}"),
            Self::Paint { layer } => format!("the paint layer {layer}"),
            Self::Navigation => "the navigation side file".into(),
            Self::Project => "the project".into(),
        }
    }

    /// The key a project's [`crate::project::Base::files`] records the file under (`group:<file>`,
    /// `terrain:<file>`); `None` for targets with no base file of their own (the partition is the base's own, the
    /// paint layers and the navigation side file are the project's, the project is no file).
    #[must_use]
    pub fn base_key(&self) -> Option<String> {
        match self {
            Self::Group { file } => Some(format!("group:{file}")),
            Self::Terrain { file } => Some(format!("terrain:{file}")),
            Self::Partition | Self::Paint { .. } | Self::Navigation | Self::Project => None,
        }
    }
}

/// One edit of a multi-file operation ([`Op::Files`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "edit", rename_all = "kebab-case")]
pub enum FileEdit {
    /// An edit of the level partition: one of the partition's operations (move, delete, place, set field ...). A
    /// multi-file operation may not hold another one.
    Partition {
        /// The operation.
        op: Box<Op>,
    },
    /// A static group member's transform: member `item` of the group instance `instance` (its place in the base's
    /// `Assets`, or a member the project added: [`Member`]), its 16 numbers in the instance's `Transforms` (each
    /// row's fourth word kept).
    MemberTransform {
        /// The group file ([`Target::group`]'s name).
        file: String,
        /// The group instance's guid.
        instance: String,
        /// The member.
        item: Member,
        /// Its transform before.
        from: Rows,
        /// Its transform after.
        to: Rows,
    },
    /// A static group member deleted (round SG1, [`crate::members::delete`]): its item out of the group file, its
    /// record out of its member type in the partition, the group's counts made again.
    MemberDelete {
        /// The group file.
        file: String,
        /// The group instance's guid.
        instance: String,
        /// The member.
        item: Member,
        /// Its `Assets` reference (what was deleted, for the History and the base-changed report).
        asset: String,
        /// Its transform.
        rows: Rows,
    },
    /// A static group member added (round SG1, [`crate::members::add`]): a copy of member `like` (its member type:
    /// blueprint, shader set, collision) at `rows`, named `item` (always [`Member::Added`]).
    MemberAdd {
        /// The group file.
        file: String,
        /// The group instance's guid.
        instance: String,
        /// The new member.
        item: Member,
        /// The member it copies.
        like: Member,
        /// Its transform.
        rows: Rows,
    },
    /// A typed field of an instance of a static group file (round SG1, for the DBX inspector of round GR4): set as
    /// [`LevelWriter::set_field`] sets a partition's, its type read from the group file's own data. The member
    /// arrays (`Assets`, `Transforms`, `Enums`) are never set so: they are arrays, and members change by the member
    /// edits.
    GroupField {
        /// The group file.
        file: String,
        /// The field set (`guid` an instance of the group file).
        set: crate::ops::FieldSet,
    },
}

impl FileEdit {
    /// The first file it changes.
    #[must_use]
    pub fn target(&self) -> Target {
        self.targets().into_iter().next().unwrap_or(Target::Partition)
    }

    /// The files it changes: a member's delete and add change its group file and the partition (its member type).
    #[must_use]
    pub fn targets(&self) -> Vec<Target> {
        match self {
            Self::Partition { .. } => vec![Target::Partition],
            Self::MemberTransform { file, .. } | Self::GroupField { file, .. } => vec![Target::group(file)],
            Self::MemberDelete { file, .. } | Self::MemberAdd { file, .. } => {
                vec![Target::group(file), Target::Partition]
            }
        }
    }

    /// The edit in words.
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            Self::Partition { op } => op.describe(),
            Self::MemberTransform { instance, item, .. } => {
                format!("Move group member {}", crate::members::member_guid(instance, *item))
            }
            Self::MemberDelete {
                instance, item, asset, ..
            } => format!(
                "Delete group member {} ({})",
                crate::members::member_guid(instance, *item),
                crate::members::asset_name(asset)
            ),
            Self::MemberAdd {
                instance, item, like, ..
            } => format!(
                "Add group member {} (a copy of {})",
                crate::members::member_guid(instance, *item),
                crate::members::member_guid(instance, *like)
            ),
            Self::GroupField { file, set } => format!("Set {} of {} in {file}", set.field, set.guid),
        }
    }

    /// Whether it changes the level partition.
    #[must_use]
    pub fn edits_partition(&self) -> bool {
        !matches!(self, Self::MemberTransform { .. } | Self::GroupField { .. })
    }
}

/// A base static group member's guid as the scene names it: `<group instance guid>#<item>` (upper case; a member
/// the project added: [`crate::members::member_guid`]).
#[must_use]
pub fn member_guid(instance: &str, item: usize) -> String {
    crate::members::member_guid(instance, Member::Base(item))
}

/// The base bytes of the level's files besides its partition, read the first time an operation names one (the
/// editor: from the scene as loaded; the export: from the base level's bundles), and the project's tile files.
pub trait Sources {
    /// The file's bytes as the base level has it; `None`: the level has no such file.
    fn file(&self, target: &Target) -> Option<Vec<u8>>;
    /// A raster's samples as the base level has them (a height field's stored values); `None`: none (a paint layer
    /// starts at 0).
    fn raster(&self, target: &Target) -> Option<crate::delta::Raster> {
        let _ = target;
        None
    }
    /// A stroke's tile file by its sha1 ([`crate::delta::TileStore`]).
    fn tile(&self, sha1: &str) -> Option<Vec<u8>> {
        let _ = sha1;
        None
    }
}

/// No files besides the partition (a project whose log edits the partition only, or a test).
pub struct NoSources;

impl Sources for NoSources {
    fn file(&self, _: &Target) -> Option<Vec<u8>> {
        None
    }
}

impl<F: Fn(&Target) -> Option<Vec<u8>>> Sources for F {
    fn file(&self, target: &Target) -> Option<Vec<u8>> {
        self(target)
    }
}

/// The files of a level as a project's log leaves them: a writer per file (audit AU1 R18). The partition's is made
/// from the base; the others are read from [`Sources`] when an operation first names them.
///
/// Each writer is shared (round PR1b-0, audit AU5 ALG-3 / ALG-4): a clone of the files (an editor checkpoint, a
/// refusal's trial) shares every writer, and an operation copies a writer only when it edits one that is shared
/// (`Arc::make_mut`), so checkpoints hold one copy of each file per change, not one per checkpoint.
#[derive(Debug, Clone, Default)]
pub struct LevelFiles {
    /// The level partition (`None`: the scene has none, as the demo's).
    pub partition: Option<Arc<LevelWriter>>,
    /// The static group files edited, by [`Target::group`]'s name.
    pub groups: BTreeMap<String, Arc<LevelWriter>>,
    /// The rasters strokes changed (height fields, paint layers), by target: their tiles shared with the
    /// checkpoints that hold them ([`crate::delta::Raster`]).
    pub rasters: BTreeMap<Target, crate::delta::Raster>,
    /// The members of each group instance a member's edit touched, in their current order (round SG1: an edit
    /// finds a member's place through it; an instance not here has its base's order).
    pub orders: Orders,
    /// Each member the project added (its guid, [`crate::members::member_guid`]) and the base member whose leaf
    /// shape it takes at export (the member it copies, or the one that one copies ...).
    pub added: BTreeMap<String, Member>,
}

impl LevelFiles {
    /// The files of a level partition (`None`: no partition).
    #[must_use]
    pub fn new(partition: Option<LevelWriter>) -> Self {
        Self {
            partition: partition.map(Arc::new),
            groups: BTreeMap::new(),
            rasters: BTreeMap::new(),
            orders: Orders::new(),
            added: BTreeMap::new(),
        }
    }

    /// One operation applied, its counts added to `out` (whose `bytes` it leaves alone).
    ///
    /// The operations of rounds E2 - XF1 apply to the partition as [`apply_one_with`] applies them (an edit made
    /// before a failing one of the same operation stays made, as it always did: old logs replay to the same files).
    /// [`Op::Files`] applies atomically: all of its edits, or none.
    ///
    /// # Errors
    /// The operation cannot be applied (named, with why); for [`Op::Files`] nothing was changed.
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per kind of edit, applied on the staged copies in order"
    )]
    pub fn apply(
        &mut self,
        o: &Operation,
        schema: &Schema,
        out: &mut Applied,
        donors: &Donors<'_>,
        sources: &dyn Sources,
    ) -> Result<(), ApplyError> {
        if let Op::Stroke { target, size, tiles } = &o.op {
            return self.stroke(o, target, *size, tiles, out, sources);
        }
        let Op::Files { edits } = &o.op else {
            if let Some(w) = self.partition.as_mut() {
                apply_one_with(Arc::make_mut(w), o, schema, out, donors)?;
            }
            return Ok(());
        };
        let fail = |error: LevelWriteError| ApplyError {
            n: o.n,
            name: o.name(),
            error,
        };
        // the copies the edits are made on: the partition only when an edit names it, each group file once (shared
        // until the first edit of each: Arc::make_mut copies it then)
        let mut stage = Self::default();
        let mut counts = Applied::default();
        if edits.iter().any(FileEdit::edits_partition) {
            stage.partition.clone_from(&self.partition);
        }
        if let Some(w) = stage.partition.as_mut() {
            apply_partition_edits(Arc::make_mut(w), o, schema, &mut counts, donors)?;
        } else if o.op.parts().iter().any(|p| matches!(p, Op::Files { .. })) {
            return Err(fail(LevelWriteError::Refused(
                "a multi-file operation holds another one".into(),
            )));
        }
        let mut orders = Orders::new();
        let mut added = self.added.clone();
        for e in edits {
            let (file, instance) = match e {
                FileEdit::Partition { .. } => continue,
                FileEdit::GroupField { file, set } => {
                    let Target::Group { file } = Target::group(file) else {
                        continue;
                    };
                    let mut f = match stage.groups.remove(&file) {
                        Some(w) => w,
                        None => {
                            if let Some(w) = self.groups.get(&file) {
                                Arc::clone(w)
                            } else {
                                let t = Target::Group { file: file.clone() };
                                let bytes = sources
                                    .file(&t)
                                    .ok_or_else(|| fail(LevelWriteError::NoFile(t.describe())))?;
                                Arc::new(LevelWriter::parse(&bytes).map_err(fail)?)
                            }
                        }
                    };
                    let field_schema = Schema::scan([&f.d]);
                    Arc::make_mut(&mut f)
                        .set_field(&field_schema, &set.guid, &set.field, &set.to)
                        .map_err(fail)?;
                    stage.groups.insert(file, f);
                    counts.fields += 1;
                    counts.files += 1;
                    continue;
                }
                FileEdit::MemberTransform { file, instance, .. }
                | FileEdit::MemberDelete { file, instance, .. }
                | FileEdit::MemberAdd { file, instance, .. } => (file, instance),
            };
            let Target::Group { file } = Target::group(file) else {
                continue;
            };
            if !stage.groups.contains_key(&file) {
                let w = if let Some(w) = self.groups.get(&file) {
                    Arc::clone(w)
                } else {
                    let t = Target::Group { file: file.clone() };
                    let bytes = sources
                        .file(&t)
                        .ok_or_else(|| fail(LevelWriteError::NoFile(t.describe())))?;
                    Arc::new(LevelWriter::parse(&bytes).map_err(fail)?)
                };
                stage.groups.insert(file.clone(), w);
            }
            let f = Arc::make_mut(
                stage
                    .groups
                    .get_mut(&file)
                    .ok_or_else(|| fail(LevelWriteError::NoFile(file.clone())))?,
            );
            let key = (file.clone(), instance.to_ascii_uppercase());
            let order: &mut Order = match orders.entry(key) {
                std::collections::btree_map::Entry::Occupied(x) => x.into_mut(),
                std::collections::btree_map::Entry::Vacant(x) => {
                    let o = match self.orders.get(x.key()) {
                        Some(o) => o.clone(),
                        None => crate::members::base_order(f, instance).map_err(fail)?,
                    };
                    x.insert(o)
                }
            };
            let name = |m: Member| crate::members::member_guid(instance, m);
            match e {
                FileEdit::MemberTransform { item, to, .. } => {
                    let place = crate::members::place_of(order, *item)
                        .ok_or_else(|| fail(LevelWriteError::NotFound(name(*item))))?;
                    set_member_transform(f, instance, place, to).map_err(fail)?;
                    counts.transforms += 1;
                }
                FileEdit::MemberDelete { item, .. } => {
                    let w = stage
                        .partition
                        .as_mut()
                        .ok_or_else(|| fail(LevelWriteError::NoFile(Target::Partition.describe())))?;
                    crate::members::delete(f, Arc::make_mut(w), &file, instance, order, *item).map_err(fail)?;
                    counts.deleted += 1;
                }
                FileEdit::MemberAdd { item, like, rows, .. } => {
                    if !matches!(item, Member::Added { .. }) {
                        return Err(fail(LevelWriteError::Refused(format!(
                            "{}: an added member is numbered as one (#+n)",
                            name(*item)
                        ))));
                    }
                    let w = stage
                        .partition
                        .as_mut()
                        .ok_or_else(|| fail(LevelWriteError::NoFile(Target::Partition.describe())))?;
                    crate::members::add(f, Arc::make_mut(w), &file, instance, order, *like, *item, rows)
                        .map_err(fail)?;
                    // the base member whose leaf shape it takes at export
                    let root = match like {
                        Member::Base(_) => *like,
                        Member::Added { .. } => added.get(&name(*like)).copied().unwrap_or(*like),
                    };
                    added.insert(name(*item), root);
                    counts.duplicated += 1;
                }
                FileEdit::Partition { .. } | FileEdit::GroupField { .. } => {}
            }
            counts.files += 1;
        }
        // every edit applied: the copies replace the files
        if let Some(p) = stage.partition {
            self.partition = Some(p);
        }
        self.groups.extend(stage.groups);
        self.orders.extend(orders);
        self.added = added;
        counts.operations = 1;
        out.add(&counts);
        Ok(())
    }

    /// A stroke applied to its raster (taken from the sources, or made at `size` for a paint layer, the first time):
    /// every tile checked first, nothing written unless all are the ones it was made on.
    fn stroke(
        &mut self,
        o: &Operation,
        target: &Target,
        size: [u32; 2],
        tiles: &[crate::delta::TileDelta],
        out: &mut Applied,
        sources: &dyn Sources,
    ) -> Result<(), ApplyError> {
        let fail = |error: LevelWriteError| ApplyError {
            n: o.n,
            name: o.name(),
            error,
        };
        let raster = match self.rasters.entry(target.clone()) {
            std::collections::btree_map::Entry::Occupied(e) => e.into_mut(),
            std::collections::btree_map::Entry::Vacant(e) => {
                let r = match (sources.raster(target), target) {
                    (Some(r), _) => r,
                    (None, Target::Paint { .. }) => crate::delta::Raster::new(size[0], size[1], 0),
                    (None, other) => return Err(fail(LevelWriteError::NoFile(other.describe()))),
                };
                e.insert(r)
            }
        };
        if [raster.width(), raster.height()] != size {
            return Err(fail(LevelWriteError::Refused(format!(
                "the stroke was made on {} x {} samples; {} has {} x {}",
                size[0],
                size[1],
                target.describe(),
                raster.width(),
                raster.height()
            ))));
        }
        crate::delta::apply_stroke(raster, tiles, &|s| sources.tile(s), true).map_err(fail)?;
        out.operations += 1;
        out.files += tiles.len();
        Ok(())
    }

    /// The files besides the partition as the log leaves them: their bytes by target (a raster's samples as
    /// little-endian `u16`s, row by row).
    ///
    /// # Errors
    /// A file that does not write.
    pub fn edited(&self) -> Result<BTreeMap<Target, Vec<u8>>, LevelWriteError> {
        let mut out: BTreeMap<Target, Vec<u8>> = self
            .groups
            .iter()
            .map(|(f, w)| Ok((Target::Group { file: f.clone() }, w.write()?)))
            .collect::<Result<_, LevelWriteError>>()?;
        for (t, r) in &self.rasters {
            out.insert(t.clone(), r.samples().iter().flat_map(|s| s.to_le_bytes()).collect());
        }
        Ok(out)
    }
}

/// A static group member's transform in its group file, by its place there ([`FileEdit::MemberTransform`]).
///
/// # Errors
/// No such group instance, no 16 numbers per item in its `Transforms`, or no such item.
pub fn set_member_transform(w: &mut LevelWriter, instance: &str, item: usize, rows: &Rows) -> crate::write::Result<()> {
    let member = member_guid(instance, item);
    let e = w
        .find(instance)
        .ok_or_else(|| LevelWriteError::NotFound(member.clone()))?;
    let t = w.d.child(e, b"Transforms").ok_or_else(|| LevelWriteError::NoField {
        guid: instance.to_owned(),
        field: "Transforms".into(),
    })?;
    let (size, old) = bc2_formats::dbx::edit::numbers_of(w.d.raw(t))
        .ok_or_else(|| LevelWriteError::Dbx(format!("{instance}: damaged Transforms")))?;
    if size != 4 || old.len() % 16 != 0 {
        return Err(LevelWriteError::Value {
            guid: instance.to_owned(),
            field: "Transforms".into(),
            why: "not 16 numbers per member".into(),
        });
    }
    if item >= old.len() / 16 {
        return Err(LevelWriteError::NotFound(member));
    }
    let mut values: Vec<[u8; 4]> = old
        .iter()
        .map(|v| <[u8; 4]>::try_from(*v).unwrap_or_default())
        .collect();
    for (r, row) in rows.iter().enumerate() {
        for (c, v) in row.iter().enumerate() {
            if let Some(slot) = values.get_mut(16 * item + 4 * r + c) {
                *slot = v.to_be_bytes();
            }
        }
    }
    w.d.set_numbers(t, &values)?;
    Ok(())
}

/// A static group member's transform as its group file holds it.
#[must_use]
pub fn member_transform(w: &LevelWriter, instance: &str, item: usize) -> Option<Rows> {
    let e = w.find(instance)?;
    let t = w.d.child(e, b"Transforms")?;
    let (size, vals) = bc2_formats::dbx::edit::numbers_of(w.d.raw(t))?;
    if size != 4 || vals.len() < 16 * (item + 1) {
        return None;
    }
    let f = |k: usize| -> f32 {
        vals.get(16 * item + k)
            .and_then(|v| <[u8; 4]>::try_from(*v).ok())
            .map_or(0.0, f32::from_be_bytes)
    };
    Some(std::array::from_fn(|r| std::array::from_fn(|c| f(r * 4 + c))))
}

/// What [`apply_files`] made: the partition (as [`crate::write::apply`] makes it) and every other file the log
/// changed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AppliedFiles {
    /// The partition and the counts.
    pub partition: Applied,
    /// The other files changed, by target.
    pub files: BTreeMap<Target, Vec<u8>>,
    /// The group instances whose members the log edited, their members in order (round SG1).
    pub orders: Orders,
    /// The members the log added and the base member whose leaf shape each takes (round SG1).
    pub added: BTreeMap<String, Member>,
}

/// The base partition and the level's other files with `ops` applied in order ([`LevelFiles::apply`]): what an
/// export writes. A log of the operations of rounds E2 - XF1 gives the partition [`crate::write::apply_with`] gives,
/// byte for byte, and no other file.
///
/// # Errors
/// The base does not read, or an operation cannot be applied (named, with why).
pub fn apply_files<'a>(
    base: &[u8],
    ops: impl IntoIterator<Item = &'a Operation>,
    schema: &Schema,
    donors: &Donors<'_>,
    sources: &dyn Sources,
) -> Result<AppliedFiles, ApplyError> {
    let w = LevelWriter::parse(base).map_err(|error| ApplyError {
        n: 0,
        name: "read the base".into(),
        error,
    })?;
    let mut files = LevelFiles::new(Some(w));
    let mut out = Applied::default();
    for o in ops {
        files.apply(o, schema, &mut out, donors, sources)?;
    }
    let write = |error| ApplyError {
        n: 0,
        name: "write the files".into(),
        error,
    };
    out.bytes = files
        .partition
        .as_deref()
        .map_or_else(|| Ok(Vec::new()), LevelWriter::write)
        .map_err(write)?;
    let edited = files.edited().map_err(write)?;
    Ok(AppliedFiles {
        partition: out,
        files: edited,
        orders: files.orders,
        added: files.added,
    })
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::doc_markdown,
    clippy::too_many_lines
)]
pub(crate) mod tests {
    use super::*;
    use bc2_formats::dbx::{DbxFile, Element};

    /// A small static group file: one group instance `GG...` with two members (their `Assets` items and 32
    /// `Transforms` numbers), member 1 at x = 7.
    pub(crate) fn group_file() -> Vec<u8> {
        let mut f = DbxFile::default();
        let mut t = Vec::new();
        for x in [0.0f32, 7.0] {
            for v in [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, 1.0, 2.0, 1.0f32,
            ] {
                t.extend(v.to_be_bytes());
            }
        }
        let tr = Element::numbers(&mut f, "array", &[("name", "Transforms")], 4, &t);
        let items: Vec<Element> = ["objects/props/crate/crate_havok/x", "objects/props/rock/rock_havok/y"]
            .iter()
            .map(|r| Element::text(&mut f, "item", &[("ref", r)], ""))
            .collect();
        let assets = Element::container(&mut f, "array", &[("name", "Assets")], items);
        let inst = Element::container(
            &mut f,
            "instance",
            &[
                ("guid", "gggggggg-0000-0000-0000-000000000001"),
                ("type", "Entity.StaticModelGroupEntityData"),
            ],
            vec![assets, tr],
        );
        let part = Element::container(&mut f, "partition", &[("guid", "G")], vec![inst]);
        f.roots.push(part);
        bc2_formats::dbx::write(&f).unwrap()
    }

    fn op(n: u64, op: Op) -> Operation {
        Operation {
            n,
            by: "PC".into(),
            when: "2026-11-21T00:00:00Z".into(),
            label: String::new(),
            op,
        }
    }

    const G: &str = "GGGGGGGG-0000-0000-0000-000000000001";
    const FILE: &str = "Levels/Test/StaticModelGroupEntity_Grid00x00_HavokAsset.dbx";

    #[test]
    fn multi_file_operations_apply_whole_or_not_at_all() {
        let base = crate::write::tests::level();
        let group = group_file();
        let sources = |t: &Target| (*t == Target::group(FILE)).then(|| group.clone());
        let schema = Schema::scan([&LevelWriter::parse(&base).unwrap().d]);
        let mv = |x: f32| FileEdit::MemberTransform {
            file: FILE.into(),
            instance: G.into(),
            item: Member::Base(1),
            from: crate::rows_at([7.0, 1.0, 2.0]),
            to: crate::rows_at([x, 1.0, 2.0]),
        };
        let a = "AAAAAAAA-0000-0000-0000-000000000001";
        let both = op(
            1,
            Op::Files {
                edits: vec![
                    FileEdit::Partition {
                        op: Box::new(Op::Delete { guids: vec![a.into()] }),
                    },
                    mv(9.0),
                ],
            },
        );
        assert_eq!(
            both.op.files(),
            [Target::Partition, Target::group(FILE)],
            "the files it names, in order"
        );
        assert_eq!(
            both.name(),
            "Delete 1 object; Move group member GGGGGGGG-0000-0000-0000-000000000001#1 (2 files)"
        );
        let r = apply_files(&base, [&both], &schema, &|_| None, &sources).unwrap();
        assert_eq!(r.partition.operations, 1);
        assert_eq!(r.partition.deleted, 1);
        assert_eq!(r.partition.files, 1);
        let g = LevelWriter::parse(&r.files[&Target::group(FILE)]).unwrap();
        assert_eq!(member_transform(&g, G, 1).unwrap()[3], [9.0, 1.0, 2.0]);
        assert_eq!(
            member_transform(&g, G, 0).unwrap()[3],
            [0.0, 1.0, 2.0],
            "the other member kept"
        );
        // each row's fourth word kept (the last row's 1.0 here)
        let t = g.d.child(g.find(G).unwrap(), b"Transforms").unwrap();
        let (_, vals) = bc2_formats::dbx::edit::numbers_of(g.d.raw(t)).unwrap();
        assert_eq!(vals[31], 1.0f32.to_be_bytes());

        // atomic: a failing second edit leaves the partition's delete unmade too
        let mut files = LevelFiles::new(Some(LevelWriter::parse(&base).unwrap()));
        let bad = op(
            2,
            Op::Files {
                edits: vec![
                    FileEdit::Partition {
                        op: Box::new(Op::Delete { guids: vec![a.into()] }),
                    },
                    FileEdit::MemberTransform {
                        file: FILE.into(),
                        instance: G.into(),
                        item: Member::Base(5),
                        from: crate::rows_at([0.0; 3]),
                        to: crate::rows_at([1.0; 3]),
                    },
                ],
            },
        );
        let mut out = Applied::default();
        let e = files.apply(&bad, &schema, &mut out, &|_| None, &sources).unwrap_err();
        assert!(e.to_string().contains("#5 is not in the level"), "{e}");
        assert!(
            files.partition.as_ref().unwrap().find(a).is_some(),
            "the delete was not made"
        );
        assert!(files.groups.is_empty(), "no group file taken");
        assert_eq!(out, Applied::default());
        // a file the level does not have, and a multi-file operation inside another, are refused
        let missing = op(
            3,
            Op::Files {
                edits: vec![FileEdit::MemberTransform {
                    file: "other_havokasset.dbx".into(),
                    instance: G.into(),
                    item: Member::Base(0),
                    from: crate::rows_at([0.0; 3]),
                    to: crate::rows_at([1.0; 3]),
                }],
            },
        );
        let e = files
            .apply(&missing, &schema, &mut out, &|_| None, &sources)
            .unwrap_err();
        assert!(
            e.to_string()
                .contains("the static group file other_havokasset.dbx is not among the level's files"),
            "{e}"
        );
        let nested = op(
            4,
            Op::Files {
                edits: vec![FileEdit::Partition {
                    op: Box::new(Op::Files { edits: Vec::new() }),
                }],
            },
        );
        assert!(files.apply(&nested, &schema, &mut out, &|_| None, &sources).is_err());
        // the operations of earlier rounds give apply_with's partition byte for byte, and no other file
        let old = [op(
            5,
            Op::Move {
                items: vec![crate::ops::Moved {
                    guid: a.into(),
                    from: crate::rows_at([10.0, 2.0, 3.0]),
                    to: crate::rows_at([11.0, 2.0, 3.0]),
                    placement: None,
                }],
            },
        )];
        let x = apply_files(&base, &old, &schema, &|_| None, &NoSources).unwrap();
        assert_eq!(x.partition, crate::write::apply(&base, &old, &schema).unwrap());
        assert!(x.files.is_empty());
    }

    /// Round SG1: a typed field of a group file's instance (the DBX inspector's edit through the project), typed by
    /// the group file's own data; the member arrays are refused, nothing changed.
    #[test]
    fn group_fields_are_typed_by_the_group_file() {
        let mut f = DbxFile::default();
        let name = Element::text(
            &mut f,
            "field",
            &[("name", "Name")],
            "Levels/Test/StaticModelGroupEntity_HavokAsset",
        );
        let tr = Element::numbers(&mut f, "array", &[("name", "Transforms")], 4, &[0u8; 64]);
        let item = Element::text(&mut f, "item", &[("ref", "objects/x/x_havok/y")], "");
        let assets = Element::container(&mut f, "array", &[("name", "Assets")], vec![item]);
        let inst = Element::container(
            &mut f,
            "instance",
            &[("guid", G), ("type", "Physics.CompositeHavokAsset")],
            vec![name, assets, tr],
        );
        let part = Element::container(&mut f, "partition", &[("guid", "G")], vec![inst]);
        f.roots.push(part);
        let group = bc2_formats::dbx::write(&f).unwrap();
        let sources = |_: &Target| Some(group.clone());
        let set = |field: &str, to: crate::ops::FieldValue| {
            op(
                1,
                Op::Files {
                    edits: vec![FileEdit::GroupField {
                        file: FILE.into(),
                        set: crate::ops::FieldSet {
                            guid: G.into(),
                            field: field.into(),
                            from: None,
                            to,
                        },
                    }],
                },
            )
        };
        let schema = Schema::default();
        let ok = set("Name", crate::ops::FieldValue::Text("Levels/Test/Renamed".into()));
        assert_eq!(ok.op.files(), [Target::group(FILE)]);
        let mut files = LevelFiles::default();
        let mut out = Applied::default();
        files.apply(&ok, &schema, &mut out, &|_| None, &sources).unwrap();
        let w = &files.groups[&file_key()];
        let e = w.find(G).unwrap();
        assert_eq!(
            w.d.text(w.d.child(e, b"Name").unwrap()),
            Some(&b"Levels/Test/Renamed"[..])
        );
        assert_eq!(out.fields, 1);
        let before = files.groups[&file_key()].write().unwrap();
        let bad = set("Transforms", crate::ops::FieldValue::Int(0));
        assert!(files.apply(&bad, &schema, &mut out, &|_| None, &sources).is_err());
        assert_eq!(files.groups[&file_key()].write().unwrap(), before);
    }

    fn file_key() -> String {
        let Target::Group { file } = Target::group(FILE) else {
            unreachable!()
        };
        file
    }

    /// Round OP1 part B: strokes on a height field (from the sources) and a paint layer (made by its first stroke),
    /// their tiles in a store; a stroke on another raster or one applied twice is refused, nothing written.
    #[test]
    fn strokes_apply_from_their_tiles() {
        use crate::delta::{MemStore, Raster, TileStore as _, stroke};
        struct Src {
            store: MemStore,
            terrain: Raster,
        }
        impl Sources for Src {
            fn file(&self, _: &Target) -> Option<Vec<u8>> {
                None
            }
            fn raster(&self, t: &Target) -> Option<Raster> {
                matches!(t, Target::Terrain { .. }).then(|| self.terrain.clone())
            }
            fn tile(&self, sha1: &str) -> Option<Vec<u8>> {
                self.store.get(sha1)
            }
        }
        let src = Src {
            store: MemStore::default(),
            terrain: Raster::new(200, 200, 1000),
        };
        let terrain = Target::Terrain {
            file: "terrains/t/t.heightfield-000.res".into(),
        };
        let paint = Target::Paint { layer: "grass".into() };
        let mut hill = src.terrain.clone();
        for x in 60..70 {
            hill.set(x, 10, 2000);
        }
        let mut grass = Raster::new(200, 200, 0);
        grass.set(199, 199, 255);
        let ops = [
            op(
                1,
                Op::Stroke {
                    target: terrain.clone(),
                    size: [200, 200],
                    tiles: stroke(&src.terrain, &hill, &src.store).unwrap(),
                },
            ),
            op(
                2,
                Op::Stroke {
                    target: paint.clone(),
                    size: [200, 200],
                    tiles: stroke(&Raster::new(200, 200, 0), &grass, &src.store).unwrap(),
                },
            ),
        ];
        assert_eq!(
            ops[0].name(),
            "Stroke on the height field terrains/t/t.heightfield-000.res: 2 tiles"
        );
        assert_eq!(ops[0].op.files(), std::slice::from_ref(&terrain));
        let schema = Schema::default();
        let mut files = LevelFiles::default();
        let mut out = Applied::default();
        for o in &ops {
            files.apply(o, &schema, &mut out, &|_| None, &src).unwrap();
        }
        assert_eq!((out.operations, out.files), (2, 3));
        assert_eq!(files.rasters[&terrain], hill);
        assert_eq!(files.rasters[&paint], grass);
        // again: the tiles are no longer the ones the strokes were made on
        let before = files.clone();
        let e = files.apply(&ops[0], &schema, &mut out, &|_| None, &src).unwrap_err();
        assert!(e.to_string().contains("is not the one the stroke was made on"), "{e}");
        assert_eq!(files.rasters, before.rasters);
        // a stroke made for another size
        let mut wrong = ops[1].clone();
        if let Op::Stroke { size, .. } = &mut wrong.op {
            *size = [256, 256];
        }
        assert!(files.apply(&wrong, &schema, &mut out, &|_| None, &src).is_err());
        // a height field the level does not have
        let mut none = LevelFiles::default();
        let e = none
            .apply(&ops[0], &schema, &mut out, &|_| None, &NoSources)
            .unwrap_err();
        assert!(e.to_string().contains("is not among the level's files"), "{e}");
    }
}
