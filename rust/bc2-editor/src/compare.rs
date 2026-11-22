//! What differs between two levels, for the Diff panel and the 3D view (round EX1: LevelDiff folded into the editor;
//! `EDITOR_ARCHITECTURE.md` 9.5).
//!
//! The comparison is LevelDiff's own ([`bc2_formats::dbxedit::leveldiff`], the code behind `bfbc2.exe leveldiff`):
//! instances matched by GUID, each changed one with its fields (`path: old -> new`) and its reference lists as sets.
//! Here each difference becomes a [`Row`]: **added** (only in B), **removed** (only in A), **moved** (its `Transform`
//! changed) or **changed** (other fields), in a [`Group`] - the mode layer (`bc2_levelwrite::modelayer::layer` of
//! either side), objects placed from another set's bundles (an export's closure), the campaign content the export
//! strips, or other. [`DiffView::marks`] gives what the 3D view draws on either side: the objects of the side it
//! shows tinted by kind, and markers (boxes) where an object of the other side stands.
//!
//! Three comparisons use it: a project against its base level (Compare with base), any open level against another
//! level of the game copy, and an export's stage against the original base level (Preview export: [`stage_level`]
//! opens the stage as a level folder, read only, with the bundles its registration names).

use std::collections::HashSet;
use std::path::Path;
use std::time::Instant;

use bc2_formats::dbxedit::leveldiff::{self, Change, Options};
use bc2_formats::dbxedit::{EditFile, PlacedObject};
use bc2_levelwrite::export::{self, StageRecord};
use bc2_levelwrite::{LevelWriter, modelayer, mpconvert};
use bc2_scene::scene::Scene;

use crate::error::EditorError;
use crate::package::{self, Bundle, PackageLevel};

/// What kind of difference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Kind {
    /// Only in B.
    Added,
    /// Only in A.
    Removed,
    /// In both; its transform differs.
    Moved,
    /// In both; other fields differ.
    Changed,
}

impl Kind {
    /// Every kind, in the panel's order.
    pub const ALL: [Self; 4] = [Self::Added, Self::Removed, Self::Moved, Self::Changed];

    /// Its word.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Added => "Added",
            Self::Removed => "Removed",
            Self::Moved => "Moved",
            Self::Changed => "Changed",
        }
    }

    /// Its index in [`Kind::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::Added => 0,
            Self::Removed => 1,
            Self::Moved => 2,
            Self::Changed => 3,
        }
    }
}

/// What a difference belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Group {
    /// The game mode's layer (flags, bases, spawns, M-COMs, combat areas, the announcer ...).
    ModeLayer,
    /// A placed object whose blueprint the export copied from another set's or level's bundles (the closure).
    OtherSet,
    /// The campaign content an export takes out (scripting, AI, the campaign's spawns and vehicles).
    Campaign,
    /// Everything else (the world).
    Other,
}

impl Group {
    /// Every group, in the panel's order.
    pub const ALL: [Self; 4] = [Self::ModeLayer, Self::OtherSet, Self::Campaign, Self::Other];

    /// Its words.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::ModeLayer => "Mode layer",
            Self::OtherSet => "From other sets",
            Self::Campaign => "Campaign content",
            Self::Other => "World",
        }
    }
}

/// One difference.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Its kind.
    pub kind: Kind,
    /// Its group.
    pub group: Group,
    /// Its type, short.
    pub ty: String,
    /// Its id.
    pub id: Option<String>,
    /// Its GUID in A (`None`: only in B).
    pub guid_a: Option<String>,
    /// Its GUID in B.
    pub guid_b: Option<String>,
    /// A placed object's blueprint (its name).
    pub blueprint: Option<String>,
    /// Where it stands in A.
    pub pos_a: Option<[f32; 3]>,
    /// Where it stands in B.
    pub pos_b: Option<[f32; 3]>,
    /// The fields that differ (LevelDiff's `path: old -> new`).
    pub changes: Vec<Change>,
    /// Reference list entries only in B.
    pub added: Vec<String>,
    /// Reference list entries only in A.
    pub removed: Vec<String>,
}

impl Row {
    /// Its name in the list: type, id or blueprint.
    #[must_use]
    pub fn label(&self) -> String {
        let what = self
            .id
            .clone()
            .filter(|s| !s.is_empty())
            .or_else(|| self.blueprint.clone())
            .unwrap_or_default();
        if what.is_empty() {
            self.ty.clone()
        } else {
            format!("{}  {what}", self.ty)
        }
    }

    /// Its GUID on a side.
    #[must_use]
    pub fn guid(&self, side: Side) -> Option<&str> {
        match side {
            Side::A => self.guid_a.as_deref(),
            Side::B => self.guid_b.as_deref(),
        }
    }

    /// Where it stands on a side, else on the other.
    #[must_use]
    pub fn position(&self, side: Side) -> Option<[f32; 3]> {
        match side {
            Side::A => self.pos_a.or(self.pos_b),
            Side::B => self.pos_b.or(self.pos_a),
        }
    }
}

/// Which file a view shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Side {
    /// The first (the base, the original).
    A,
    /// The second (the project, the export).
    B,
}

/// The Diff panel's filters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Filters {
    /// The kinds listed and drawn (by [`Kind::index`]).
    pub kinds: [bool; 4],
    /// Only one group (`None`: all).
    pub group: Option<Group>,
    /// Only rows whose type, id, blueprint or GUID hold this (any case).
    pub text: String,
    /// Only placed objects (`ReferenceObjectData`).
    pub placed_only: bool,
}

impl Default for Filters {
    fn default() -> Self {
        Self {
            kinds: [true; 4],
            group: None,
            text: String::new(),
            placed_only: false,
        }
    }
}

impl Filters {
    /// Whether a row passes.
    #[must_use]
    pub fn passes(&self, r: &Row) -> bool {
        let t = self.text.trim().to_lowercase();
        self.kinds.get(r.kind.index()).copied().unwrap_or(true)
            && self.group.is_none_or(|g| g == r.group)
            && (!self.placed_only || r.ty == "ReferenceObjectData")
            && (t.is_empty()
                || [
                    Some(r.ty.as_str()),
                    r.id.as_deref(),
                    r.blueprint.as_deref(),
                    r.guid_a.as_deref(),
                    r.guid_b.as_deref(),
                ]
                .into_iter()
                .flatten()
                .any(|s| s.to_lowercase().contains(&t)))
    }
}

/// What differs between two levels.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffView {
    /// A's name (`SP_01 (base)`).
    pub label_a: String,
    /// B's name.
    pub label_b: String,
    /// The differences: removed, added, then changed and moved, each in its file's order.
    pub rows: Vec<Row>,
    /// Instances in both and the same.
    pub same: usize,
    /// Instances in A and in B.
    pub counts: [usize; 2],
    /// How long the comparison took.
    pub seconds: f64,
}

/// What a 3D view draws for a diff: entities tinted by kind, and markers where the other side's objects stand.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Marks {
    /// Entities of the shown scene, by kind.
    pub tint: Vec<(usize, Kind)>,
    /// Markers: where an object only in the other file stands (and the old place of a moved one), by kind.
    pub markers: Vec<([f32; 3], Kind)>,
    /// A moved object's old and new place.
    pub moves: Vec<([f32; 3], [f32; 3])>,
}

/// A placed object's full blueprint path (lower case, without its guid) by its partition child index.
fn blueprint_path(f: &EditFile, index: usize) -> Option<String> {
    let i = f.partition_children().ok()?.get(index)?;
    for c in i.kids() {
        if f.table.get(c, "name").ok()? == Some("ReferencedObject") {
            let r = f.table.get(c, "ref").ok()??.to_ascii_lowercase();
            return Some(r.rsplit_once('/').map_or(r.clone(), |(head, _)| head.to_owned()));
        }
    }
    None
}

fn is_campaign(ty: &str) -> bool {
    mpconvert::CAMPAIGN_SCRIPTING.contains(&ty)
        || mpconvert::REPLACED_BY_LAYER.contains(&ty)
        || ty == mpconvert::CAMPAIGN_VEHICLES
}

impl DiffView {
    /// Compares two level partitions. `closure`: the partitions an export copied from other sets (lower case, as
    /// `stage.json`'s `closure` names them): placed objects of those blueprints are grouped as from other sets.
    ///
    /// # Errors
    /// A partition that is not a binary `.dbx`, or a damaged one.
    pub fn compute(
        a: &[u8],
        b: &[u8],
        label_a: &str,
        label_b: &str,
        closure: &HashSet<String>,
    ) -> Result<Self, EditorError> {
        let t = Instant::now();
        let fa = EditFile::parse(a.to_vec()).map_err(|e| EditorError::Level(e.to_string()))?;
        let fb = EditFile::parse(b.to_vec()).map_err(|e| EditorError::Level(e.to_string()))?;
        if !fa.is_binary || !fb.is_binary {
            return Err(EditorError::Level("both partitions must be binary .dbx".into()));
        }
        let d = leveldiff::diff(&fa, &fb, &Options::default()).map_err(|e| EditorError::Level(e.to_string()))?;
        let layer = |bytes: &[u8]| LevelWriter::parse(bytes).map_or_else(|_| HashSet::new(), |w| modelayer::layer(&w));
        let (la, lb) = (layer(a), layer(b));
        let in_layer = |g: Option<&String>| {
            g.is_some_and(|g| la.contains(&g.to_ascii_uppercase()) || lb.contains(&g.to_ascii_uppercase()))
        };
        let placed = |f: &EditFile| -> std::collections::HashMap<usize, PlacedObject> {
            PlacedObject::all(f)
                .unwrap_or_default()
                .into_iter()
                .map(|p| (p.index, p))
                .collect()
        };
        let (pa, pb) = (placed(&fa), placed(&fb));
        let pos = |f: &EditFile, p: &std::collections::HashMap<usize, PlacedObject>, k: usize| {
            p.get(&k)
                .map(|o| [o.x, o.y, o.z])
                .or_else(|| leveldiff::position_of(f, k))
        };
        let bp =
            |p: &std::collections::HashMap<usize, PlacedObject>, k: usize| p.get(&k).and_then(|o| o.blueprint.clone());
        let mut rows = Vec::new();
        for l in &d.only_a {
            rows.push(Row {
                kind: Kind::Removed,
                group: if in_layer(l.guid.as_ref()) {
                    Group::ModeLayer
                } else if is_campaign(&l.ty) {
                    Group::Campaign
                } else {
                    Group::Other
                },
                ty: l.ty.clone(),
                id: l.id.clone(),
                guid_a: l.guid.clone(),
                guid_b: None,
                blueprint: bp(&pa, l.index),
                pos_a: pos(&fa, &pa, l.index),
                pos_b: None,
                changes: Vec::new(),
                added: Vec::new(),
                removed: Vec::new(),
            });
        }
        for l in &d.only_b {
            let other_set = l.ty == "ReferenceObjectData"
                && blueprint_path(&fb, l.index).is_some_and(|p| closure.contains(&format!("{p}.dbx")));
            rows.push(Row {
                kind: Kind::Added,
                group: if in_layer(l.guid.as_ref()) {
                    Group::ModeLayer
                } else if other_set {
                    Group::OtherSet
                } else {
                    Group::Other
                },
                ty: l.ty.clone(),
                id: l.id.clone(),
                guid_a: None,
                guid_b: l.guid.clone(),
                blueprint: bp(&pb, l.index),
                pos_a: None,
                pos_b: pos(&fb, &pb, l.index),
                changes: Vec::new(),
                added: Vec::new(),
                removed: Vec::new(),
            });
        }
        for c in &d.changed {
            rows.push(Row {
                kind: if c.moved() { Kind::Moved } else { Kind::Changed },
                group: if in_layer(c.guid.as_ref()) || in_layer(c.guid_b.as_ref()) {
                    Group::ModeLayer
                } else {
                    Group::Other
                },
                ty: c.ty.clone(),
                id: c.id.clone(),
                guid_a: c.guid.clone(),
                guid_b: c.guid_b.clone(),
                blueprint: bp(&pb, c.index_b).or_else(|| bp(&pa, c.index_a)),
                pos_a: pos(&fa, &pa, c.index_a),
                pos_b: pos(&fb, &pb, c.index_b),
                changes: c.changes.clone(),
                added: c.added.clone(),
                removed: c.removed.clone(),
            });
        }
        Ok(Self {
            label_a: label_a.to_owned(),
            label_b: label_b.to_owned(),
            rows,
            same: d.same - d.changed.len(),
            counts: [d.count_a, d.count_b],
            seconds: t.elapsed().as_secs_f64(),
        })
    }

    /// The rows of each kind (by [`Kind::index`]).
    #[must_use]
    pub fn counts_by_kind(&self) -> [usize; 4] {
        let mut n = [0; 4];
        for r in &self.rows {
            if let Some(x) = n.get_mut(r.kind.index()) {
                *x += 1;
            }
        }
        n
    }

    /// The rows of each group passing the kind filters.
    #[must_use]
    pub fn counts_by_group(&self, f: &Filters) -> Vec<(Group, usize)> {
        Group::ALL
            .iter()
            .map(|&g| {
                (
                    g,
                    self.rows
                        .iter()
                        .filter(|r| r.group == g && f.kinds.get(r.kind.index()).copied().unwrap_or(true))
                        .count(),
                )
            })
            .collect()
    }

    /// The rows that pass the filters (their indices).
    #[must_use]
    pub fn visible(&self, f: &Filters) -> Vec<usize> {
        (0..self.rows.len())
            .filter(|&k| self.rows.get(k).is_some_and(|r| f.passes(r)))
            .collect()
    }

    /// What a view of `side` draws for the rows passing `f`: the scene's own differing objects tinted (found by
    /// GUID), markers where the other side's objects stand, and each move's old and new place.
    #[must_use]
    pub fn marks(&self, scene: &Scene, side: Side, f: &Filters) -> Marks {
        let by_guid: std::collections::HashMap<String, usize> = scene
            .entities
            .iter()
            .map(|e| (e.guid.to_ascii_uppercase(), e.index))
            .collect();
        let mut m = Marks::default();
        for r in self.rows.iter().filter(|r| f.passes(r)) {
            match r.guid(side).and_then(|g| by_guid.get(&g.to_ascii_uppercase())) {
                Some(&k) => m.tint.push((k, r.kind)),
                None => {
                    if let Some(p) = r.position(side) {
                        m.markers.push((p, r.kind));
                    }
                }
            }
            if r.kind == Kind::Moved
                && let (Some(a), Some(b)) = (r.pos_a, r.pos_b)
            {
                m.moves.push((a, b));
            }
        }
        m
    }
}

/// The 12 edges of a box of `half` metres about `p`, as line segments.
#[must_use]
pub fn box_lines(p: [f32; 3], half: f32) -> Vec<[[f32; 3]; 2]> {
    let c = |i: u8| {
        [
            p[0] + if i & 1 == 0 { -half } else { half },
            p[1] + if i & 2 == 0 { -half } else { half },
            p[2] + if i & 4 == 0 { -half } else { half },
        ]
    };
    let mut out = Vec::with_capacity(12);
    for i in 0..8u8 {
        for bit in [1u8, 2, 4] {
            if i & bit == 0 {
                out.push([c(i), c(i | bit)]);
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ the panel's state

/// What the open level is compared with.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Against {
    /// Nothing yet.
    #[default]
    Nothing,
    /// The project's base level (A) against the project (B).
    Base,
    /// Another level of the game copy (A) against the open one (B).
    Level(String),
    /// The original base level (A) against an export's stage (B): Preview export.
    Stage,
}

/// A view's diff: the comparison, the side it shows, the panel's filters and choice.
#[derive(Debug, Default)]
pub struct DiffState {
    /// The comparison (or why it failed).
    pub view: Option<Result<std::sync::Arc<DiffView>, EditorError>>,
    /// The side the view shows.
    pub side: SideOf,
    /// The panel's filters (also what the 3D view draws).
    pub filters: Filters,
    /// The differences are drawn in the 3D view.
    pub show: bool,
    /// The row chosen in the panel.
    pub picked: Option<usize>,
    /// A comparison under way.
    pub job: Option<crate::task::Task<DiffView>>,
    /// What B is compared with.
    pub against: Against,
    /// The project's revision the comparison was made at.
    pub revision: u64,
    /// The panel's level name field (Compare with level).
    pub other_level: String,
    /// A line about the comparison (the preview: the export's era, donor and what its closure copied).
    pub note: String,
    /// The panel's rows on screen: (row, rectangle) - the tests click them.
    pub row_rects: Vec<(usize, eframe::egui::Rect)>,
}

/// The side a view shows ([`Side::B`] unless it is the preview's original).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SideOf(pub Side);

impl Default for SideOf {
    fn default() -> Self {
        Self(Side::B)
    }
}

impl DiffState {
    /// The comparison, when it is there.
    #[must_use]
    pub fn current(&self) -> Option<&DiffView> {
        self.view.as_ref().and_then(|r| r.as_ref().ok()).map(|v| &**v)
    }

    /// Starts a comparison of `a` and `b` on a worker.
    pub fn start(&mut self, a: Vec<u8>, b: Vec<u8>, labels: [String; 2], closure: HashSet<String>) {
        self.job = Some(crate::task::Task::spawn("The comparison", move || {
            DiffView::compute(&a, &b, &labels[0], &labels[1], &closure)
        }));
    }

    /// The worker's answer, when it came (`true`: the view changed).
    pub fn poll(&mut self) -> bool {
        let Some(r) = self.job.as_ref().and_then(crate::task::Task::poll) else {
            return false;
        };
        self.job = None;
        self.view = Some(r.map(std::sync::Arc::new));
        self.show = true;
        self.picked = None;
        true
    }

    /// Waits for the worker (headless runs).
    pub fn settle(&mut self) {
        if let Some(job) = self.job.take() {
            let r = job.wait();
            self.view = Some(r.map(std::sync::Arc::new));
            self.show = true;
            self.picked = None;
        }
    }
}

// ------------------------------------------------------------------------------------------------ the stage

/// An export's stage as a level the editor opens read only (Preview export): the level folder `stage` (its
/// `level-00.fbrb` ...) in front of the bundles its registration names in the game copy at `game` - the base's,
/// the commons - in the game's order, as a [`PackageLevel`] (`package::prepare` reads it like any Package level).
///
/// # Errors
/// The stage's record does not read, or names no level block.
pub fn stage_level(game: &Path, stage: &Path) -> Result<(PackageLevel, StageRecord), EditorError> {
    let rec = export::read_record(stage).map_err(|e| EditorError::Level(format!("{}: {e}", stage.display())))?;
    let low = rec.level.to_ascii_lowercase();
    let own = format!("levels/{low}/");
    let stack = |kind: &str| -> Vec<Bundle> {
        let source = format!("BundleSource game:levels/{low}/{kind}-00.fbrb");
        let Some(block) = rec
            .mft_blocks
            .iter()
            .find(|b| b.first().is_some_and(|s| s.eq_ignore_ascii_case(&source)))
        else {
            return Vec::new();
        };
        block
            .iter()
            .filter_map(|l| l.strip_prefix("BundleTarget "))
            .filter_map(|t| {
                let lt = t.to_ascii_lowercase();
                let path = match lt.split_once(&own) {
                    Some((_, file)) => Some(stage.join(file)),
                    None => package::bundle_path(game, t),
                }?;
                path.is_file().then(|| Bundle {
                    reference: t.to_owned(),
                    path,
                })
            })
            .collect()
    };
    let level = stack("level");
    if level.is_empty() {
        return Err(EditorError::Level(format!(
            "{}: its stage names no level bundle",
            rec.level
        )));
    }
    let terrain = stack("terrain");
    Ok((
        PackageLevel {
            profile: package::profile_of_stack(&low, &level),
            lv: low,
            level,
            terrain,
        },
        rec,
    ))
}

/// The partitions an export's closure copied (lower case entry names), for [`DiffView::compute`].
#[must_use]
pub fn closure_of(rec: &StageRecord) -> HashSet<String> {
    rec.closure.iter().map(|c| c.name.to_ascii_lowercase()).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn row(kind: Kind, ty: &str) -> Row {
        Row {
            kind,
            group: Group::Other,
            ty: ty.into(),
            id: None,
            guid_a: Some("A".into()),
            guid_b: None,
            blueprint: Some("crate".into()),
            pos_a: Some([1.0, 2.0, 3.0]),
            pos_b: None,
            changes: Vec::new(),
            added: Vec::new(),
            removed: Vec::new(),
        }
    }

    #[test]
    fn filters_and_boxes() {
        let r = row(Kind::Removed, "ReferenceObjectData");
        let mut f = Filters::default();
        assert!(f.passes(&r));
        f.text = "CRA".into();
        assert!(f.passes(&r));
        f.text = "flag".into();
        assert!(!f.passes(&r));
        f.text.clear();
        f.kinds[Kind::Removed.index()] = false;
        assert!(!f.passes(&r));
        f.kinds = [true; 4];
        f.group = Some(Group::ModeLayer);
        assert!(!f.passes(&r));
        f.group = None;
        f.placed_only = true;
        assert!(!f.passes(&row(Kind::Added, "SoldierSpawnEntityData")));
        assert_eq!(box_lines([0.0; 3], 1.0).len(), 12);
        assert_eq!(r.position(Side::B), Some([1.0, 2.0, 3.0]));
        assert_eq!(r.label(), "ReferenceObjectData  crate");
    }
}
