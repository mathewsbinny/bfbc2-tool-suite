//! The operations of a map project's log, and what the log says about each object besides its partition data: its
//! layer, folder and group, and how it was last placed on a surface (the sink depth stays with it when it is moved
//! again: `EDITOR_ARCHITECTURE.md` 3.10, item 3).
//!
//! An operation names existing entities by their guid and new ones by the UUID v5 guid they were given when made
//! ([`crate::new_guid`]). One operation is one undo step, however many objects it touches (a gizmo drag of five
//! selected objects is one `move`).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::{Deserialize, Serialize};

/// A transform as the partitions store it: rows right, up, forward and the position, each x y z (singles).
pub type Rows = [[f32; 3]; 4];

/// The unit of a sink depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "kebab-case")]
pub enum SinkUnit {
    /// Metres.
    #[default]
    Metres,
    /// A fraction of the object's height (0.25: a quarter of it below the surface).
    Fraction,
}

/// How an object was placed on a surface (terrain or a mesh): sitting on it, sunk `sink` below it along the surface
/// normal or straight down, optionally turned so its up axis is the surface normal.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Placement {
    /// How far below the surface: metres, or a fraction of the object's height ([`Placement::unit`]).
    pub sink: f64,
    /// The sink's unit.
    #[serde(default)]
    pub unit: SinkUnit,
    /// Sunk along the surface normal (`true`) or straight down.
    #[serde(default)]
    pub along_normal: bool,
    /// The object's up axis turned to the surface normal.
    #[serde(default)]
    pub align_to_normal: bool,
}

impl Placement {
    /// The sink in metres for an object `height` metres tall.
    #[must_use]
    pub fn depth(&self, height: f64) -> f64 {
        match self.unit {
            SinkUnit::Metres => self.sink,
            SinkUnit::Fraction => self.sink * height,
        }
    }

    /// The sink in words: `0.5 m`, `25 % of its height`.
    #[must_use]
    pub fn describe(&self) -> String {
        let how = if self.along_normal {
            "along the surface normal"
        } else {
            "straight down"
        };
        match self.unit {
            SinkUnit::Metres => format!("sunk {:.2} m {how}", self.sink),
            SinkUnit::Fraction => format!("sunk {:.0} % of its height {how}", self.sink * 100.0),
        }
    }
}

/// One object's new transform.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Moved {
    /// The object.
    pub guid: String,
    /// Its transform before.
    pub from: Rows,
    /// Its transform after.
    pub to: Rows,
    /// Placed on a surface: how (kept with the object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
}

/// One copy made by a duplicate.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Duplicated {
    /// The object copied.
    pub source: String,
    /// The copy's guid ([`crate::new_guid`]).
    pub guid: String,
    /// The copy's transform.
    pub transform: Rows,
    /// A deep duplicate: the sub-instances the object owns, copied with it (new guids, the references among them
    /// remapped to the copies, their event connections copied). Empty for a plain duplicate, whose copy shares
    /// them with the original.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned: Vec<SubCopy>,
}

/// A sub-instance copied by a deep duplicate ([`crate::write::LevelWriter::owned`] says which an object owns).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubCopy {
    /// The sub-instance copied.
    pub source: String,
    /// The copy's guid ([`crate::new_guid`]).
    pub guid: String,
    /// The copy's transform (moved with its owner), for sub-instances that have one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transform: Option<Rows>,
}

/// A field and its value (a placed object's fields that differ from its template's).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldIs {
    /// The field: its name, or `Struct.Field`.
    pub field: String,
    /// Its value.
    pub value: FieldValue,
}

/// One object placed from the content browser: a new instance of a blueprint (levelwrite's "add instance").
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    /// The new object's guid ([`crate::new_guid`] over the project, the operation and the blueprint reference).
    pub guid: String,
    /// The instance of the partition the new one is made from: its type and fields (a placement of the same
    /// blueprint when the level has one: [`crate::write::LevelWriter::template_for`]).
    pub template: String,
    /// The blueprint it places: the reference (`objects/props/crate/crate/<guid>`) its `ReferencedObject` (or
    /// [`Placed::ref_field`]) takes; empty: the template's is kept (a spawn point, a flag).
    pub blueprint: String,
    /// The field the blueprint goes to when it is not `ReferencedObject` (a vehicle spawner's `Vehicle`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ref_field: Option<String>,
    /// The sub-instances the template owns, copied with it (a flag's capture area and marker): new guids, the
    /// references among them remapped, only the event connections among them copied (its links to the level's
    /// logic are E4's gameplay wiring).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owned: Vec<SubCopy>,
    /// Its transform.
    pub transform: Rows,
    /// Its fields that differ from the template's: the values the level's placements of the blueprint share
    /// ([`crate::write::LevelWriter::place_defaults`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldIs>,
    /// Placed on a surface: how (kept with the object).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
}

/// A field's value, in the field's type (from the schema: [`crate::schema::FieldType`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "kebab-case")]
pub enum FieldValue {
    /// A text (names, enum values).
    Text(String),
    /// A reference (`null`, a local guid, `partition/guid`).
    Ref(String),
    /// A bool (a two-byte value of 0 or 1).
    Bool(bool),
    /// A byte (a two-byte value).
    Byte(u8),
    /// A 4-byte whole number.
    Int(i64),
    /// A 4-byte decimal (single).
    Float(f64),
    /// Several 4-byte numbers (a vector: whole or decimal as the field is).
    Vector(Vec<f64>),
}

impl FieldValue {
    /// The value in words, as the properties panel shows it.
    #[must_use]
    pub fn text(&self) -> String {
        match self {
            Self::Text(s) | Self::Ref(s) => s.clone(),
            Self::Bool(b) => if *b { "true" } else { "false" }.into(),
            Self::Byte(b) => b.to_string(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => format!("{}", *f as f32),
            Self::Vector(v) => v
                .iter()
                .map(|x| format!("{}", *x as f32))
                .collect::<Vec<_>>()
                .join(" / "),
        }
    }
}

/// One field set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldSet {
    /// The object.
    pub guid: String,
    /// The field: its name, or `Struct.Field` inside a structure.
    pub field: String,
    /// The value before (for the history; `None` where it was not read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<FieldValue>,
    /// The value after.
    pub to: FieldValue,
}

/// A layer's state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerState {
    /// Hidden in the views (still in the build).
    #[serde(default)]
    pub hidden: bool,
    /// Locked: its objects cannot be selected in the views or edited.
    #[serde(default)]
    pub locked: bool,
    /// Isolated: only isolated layers are shown while any is.
    #[serde(default)]
    pub isolated: bool,
    /// Its colour (sRGB), drawn beside its name and its objects' rows.
    pub colour: [u8; 3],
}

/// An operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum Op {
    /// Objects moved (the gizmo, snapping, align and distribute, placing on a surface, typed positions).
    Move {
        /// The objects.
        items: Vec<Moved>,
    },
    /// Objects turned.
    Rotate {
        /// The objects.
        items: Vec<Moved>,
    },
    /// Objects deleted.
    Delete {
        /// The objects.
        guids: Vec<String>,
    },
    /// Objects copied.
    Duplicate {
        /// The copies.
        items: Vec<Duplicated>,
    },
    /// Fields set.
    SetField {
        /// The fields.
        items: Vec<FieldSet>,
    },
    /// A layer made.
    AddLayer {
        /// Its name.
        layer: String,
        /// Its colour.
        colour: [u8; 3],
    },
    /// A layer's state changed (hide, lock, isolate, colour).
    SetLayer {
        /// Its name.
        layer: String,
        /// Its state before.
        from: LayerState,
        /// Its state after.
        to: LayerState,
    },
    /// A layer removed (its objects go back to no layer).
    RemoveLayer {
        /// Its name.
        layer: String,
    },
    /// Objects put on a layer (`""`: no layer).
    AssignLayer {
        /// The layer.
        layer: String,
        /// The objects.
        guids: Vec<String>,
    },
    /// Objects put in an outliner folder (`Buildings/North`; `""`: no folder).
    SetFolder {
        /// The folder.
        folder: String,
        /// The objects.
        guids: Vec<String>,
    },
    /// Objects placed from the content browser (new instances of blueprints).
    Place {
        /// The new objects.
        items: Vec<Placed>,
    },
    /// Objects hidden in the views (`hidden: false`: shown again); they stay in the build.
    Hide {
        /// The objects.
        guids: Vec<String>,
        /// Hidden or shown.
        hidden: bool,
    },
    /// Objects locked: they cannot be selected in the views or edited (`locked: false`: unlocked).
    Lock {
        /// The objects.
        guids: Vec<String>,
        /// Locked or unlocked.
        locked: bool,
    },
    /// Objects grouped: selecting one selects them all.
    Group {
        /// The group's name.
        group: String,
        /// Its objects.
        guids: Vec<String>,
    },
    /// A group undone (its objects stay).
    Ungroup {
        /// The group's name.
        group: String,
    },
    /// The campaign content an export keeps chosen (round E3b: the export's strip checklist; the partition is not
    /// changed, the export reads the last choice: [`crate::MapProject::strip`]).
    SetStrip {
        /// The choice before.
        from: crate::mpconvert::Strip,
        /// The choice after.
        to: crate::mpconvert::Strip,
    },
    /// Every name of the exported level set (round EX1: the Export window's Names - the id, the folder, the server
    /// browser's name, the on-screen names per language). The partition is not changed; the Export window takes the
    /// log's last names ([`crate::MapProject::names`]).
    SetNames {
        /// The names.
        to: Box<crate::names::LevelNames>,
    },
    /// The exported level's weather chosen (round VW2, [`crate::weather`]): one of the level's states, another
    /// level's state borrowed, or the night look; `None`: the level's own weather as it is. The partition is not
    /// changed in the editor; the export writes the choice ([`crate::MapProject::weather`]).
    SetWeather {
        /// The choice.
        to: Option<Box<crate::weather::WeatherChoice>>,
    },
    /// The project's target chosen (round E3d): its era and multiplayer mode, the mode's layer brought in from a
    /// donor level of them as ordinary objects, the level's old layer taken out ([`crate::modelayer`]).
    SetMode {
        /// The choice.
        to: Box<crate::modelayer::ModeChoice>,
    },
    /// A navigation edit made (round GR2: the AI workspace; [`crate::navedit`]). The partition is not changed: the
    /// editor writes the edits as the level's side file for NavKit.
    NavAdd {
        /// The edit.
        edit: crate::navedit::NavEdit,
    },
    /// A navigation edit taken out of the list.
    NavRemove {
        /// Its place in the list when it was taken out.
        index: usize,
        /// The edit.
        edit: crate::navedit::NavEdit,
    },
    /// Objects' teams set (round XF1, a typed gameplay operation: [`crate::gameplay::set_team`]).
    SetTeam {
        /// The objects.
        items: Vec<crate::gameplay::TeamSet>,
    },
    /// A soldier spawner linked to a capture point or unlinked from it ([`crate::gameplay::link_spawner`]).
    LinkSpawner {
        /// The capture point.
        point: String,
        /// The spawner.
        spawner: String,
        /// Linked (put in the point's `Spawners`) or unlinked (taken out).
        linked: bool,
    },
    /// A team's tickets set ([`crate::gameplay::set_tickets`]).
    SetTickets {
        /// The ticket counter.
        counter: String,
        /// Its values before (for the History; `None` where they were not read).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        from: Option<crate::gameplay::Tickets>,
        /// Its values after.
        to: crate::gameplay::Tickets,
    },
    /// An outline point inserted after another ([`crate::gameplay::insert_point`]).
    InsertPoint {
        /// The point it follows.
        after: String,
        /// The new point's guid ([`crate::new_guid`]).
        guid: String,
        /// Its transform.
        transform: Rows,
    },
    /// An outline point removed, the outline closed over it ([`crate::gameplay::remove_point`]).
    RemovePoint {
        /// The point.
        guid: String,
    },
    /// A side file's edits loaded: they replace the list (reload).
    NavLoad {
        /// The file.
        from: String,
        /// The file it was made on.
        base: crate::navedit::NavBase,
        /// Its edits.
        edits: Vec<crate::navedit::NavEdit>,
    },
}

impl Op {
    /// Whether the operation changes the level partition (the others are the editor's organisation).
    #[must_use]
    pub fn edits_partition(&self) -> bool {
        matches!(
            self,
            Self::Move { .. }
                | Self::Rotate { .. }
                | Self::Delete { .. }
                | Self::Duplicate { .. }
                | Self::SetField { .. }
                | Self::Place { .. }
                | Self::SetMode { .. }
                | Self::SetTeam { .. }
                | Self::LinkSpawner { .. }
                | Self::SetTickets { .. }
                | Self::InsertPoint { .. }
                | Self::RemovePoint { .. }
        )
    }

    /// The existing objects it names (each must be in the base or made by an earlier operation).
    #[must_use]
    pub fn targets(&self) -> Vec<&str> {
        match self {
            Self::Move { items } | Self::Rotate { items } => items.iter().map(|m| m.guid.as_str()).collect(),
            Self::Delete { guids }
            | Self::AssignLayer { guids, .. }
            | Self::SetFolder { guids, .. }
            | Self::Group { guids, .. }
            | Self::Hide { guids, .. }
            | Self::Lock { guids, .. } => guids.iter().map(String::as_str).collect(),
            Self::Duplicate { items } => items
                .iter()
                .flat_map(|d| std::iter::once(d.source.as_str()).chain(d.owned.iter().map(|o| o.source.as_str())))
                .collect(),
            Self::Place { items } => items
                .iter()
                .flat_map(|p| std::iter::once(p.template.as_str()).chain(p.owned.iter().map(|o| o.source.as_str())))
                .collect(),
            Self::SetField { items } => items.iter().map(|f| f.guid.as_str()).collect(),
            Self::SetMode { to } => to.removed.iter().map(String::as_str).collect(),
            Self::SetTeam { items } => items.iter().map(|t| t.guid.as_str()).collect(),
            Self::LinkSpawner { point, spawner, .. } => vec![point.as_str(), spawner.as_str()],
            Self::SetTickets { counter, .. } => vec![counter.as_str()],
            Self::InsertPoint { after, .. } => vec![after.as_str()],
            Self::RemovePoint { guid } => vec![guid.as_str()],
            Self::AddLayer { .. }
            | Self::SetLayer { .. }
            | Self::RemoveLayer { .. }
            | Self::Ungroup { .. }
            | Self::SetStrip { .. }
            | Self::SetNames { .. }
            | Self::SetWeather { .. }
            | Self::NavAdd { .. }
            | Self::NavRemove { .. }
            | Self::NavLoad { .. } => Vec::new(),
        }
    }

    /// The objects it makes.
    #[must_use]
    pub fn creates(&self) -> Vec<&str> {
        match self {
            Self::Duplicate { items } => items
                .iter()
                .flat_map(|d| std::iter::once(d.guid.as_str()).chain(d.owned.iter().map(|o| o.guid.as_str())))
                .collect(),
            Self::Place { items } => items
                .iter()
                .flat_map(|p| std::iter::once(p.guid.as_str()).chain(p.owned.iter().map(|o| o.guid.as_str())))
                .collect(),
            Self::SetMode { to } => to.items.iter().map(|i| i.guid.as_str()).collect(),
            Self::InsertPoint { guid, .. } => vec![guid.as_str()],
            _ => Vec::new(),
        }
    }

    /// The operation in words, as the History panel lists it.
    #[must_use]
    #[allow(clippy::too_many_lines, reason = "one arm per operation kind")]
    pub fn describe(&self) -> String {
        let objects = |n: usize| {
            if n == 1 {
                "1 object".to_owned()
            } else {
                format!("{n} objects")
            }
        };
        match self {
            Self::Move { items } => {
                let far = items
                    .iter()
                    .map(|m| {
                        let (a, b) = (m.from[3], m.to[3]);
                        f64::from((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt()
                    })
                    .fold(0.0, f64::max);
                let placed = items.iter().find_map(|m| m.placement);
                match placed {
                    Some(p) => format!("Place {} on the surface, {}", objects(items.len()), p.describe()),
                    None => format!("Move {} {far:.2} m", objects(items.len())),
                }
            }
            Self::Rotate { items } => format!("Rotate {}", objects(items.len())),
            Self::Delete { guids } => format!("Delete {}", objects(guids.len())),
            Self::Duplicate { items } => {
                let owned: usize = items.iter().map(|d| d.owned.len()).sum();
                if owned == 0 {
                    format!("Duplicate {}", objects(items.len()))
                } else {
                    format!(
                        "Deep duplicate {} with {owned} owned sub-object{}",
                        objects(items.len()),
                        if owned == 1 { "" } else { "s" }
                    )
                }
            }
            Self::Place { items } => match items.as_slice() {
                [one] if one.blueprint.is_empty() => format!("Place a copy of {}", one.template),
                [one] => format!("Place {}", blueprint_name(&one.blueprint)),
                many => format!("Place {}", objects(many.len())),
            },
            Self::Hide { guids, hidden } => {
                format!("{} {}", if *hidden { "Hide" } else { "Show" }, objects(guids.len()))
            }
            Self::Lock { guids, locked } => {
                format!("{} {}", if *locked { "Lock" } else { "Unlock" }, objects(guids.len()))
            }
            Self::SetField { items } => match items.as_slice() {
                [one] => format!("Set {} to {}", one.field, one.to.text()),
                many => format!("Set fields of {}", objects(many.len())),
            },
            Self::AddLayer { layer, .. } => format!("Add layer {layer}"),
            Self::SetLayer { layer, from, to } => {
                let what = if from.hidden != to.hidden {
                    if to.hidden { "Hide" } else { "Show" }
                } else if from.locked != to.locked {
                    if to.locked { "Lock" } else { "Unlock" }
                } else if from.isolated != to.isolated {
                    if to.isolated { "Isolate" } else { "Stop isolating" }
                } else {
                    "Recolour"
                };
                format!("{what} layer {layer}")
            }
            Self::RemoveLayer { layer } => format!("Remove layer {layer}"),
            Self::AssignLayer { layer, guids } if layer.is_empty() => {
                format!("Take {} off its layer", objects(guids.len()))
            }
            Self::AssignLayer { layer, guids } => format!("Put {} on layer {layer}", objects(guids.len())),
            Self::SetFolder { folder, guids } if folder.is_empty() => {
                format!("Take {} out of its folder", objects(guids.len()))
            }
            Self::SetFolder { folder, guids } => format!("Put {} in folder {folder}", objects(guids.len())),
            Self::Group { group, guids } => format!("Group {} as {group}", objects(guids.len())),
            Self::Ungroup { group } => format!("Ungroup {group}"),
            Self::SetStrip { to, .. } => format!("Campaign content: {}", to.describe()),
            Self::SetNames { to } => to.describe(),
            Self::SetWeather { to } => to
                .as_ref()
                .map_or_else(|| "Weather: the level's own".to_owned(), |c| c.describe()),
            Self::SetMode { to } => to.describe(),
            Self::NavAdd { edit } => format!("Navigation edit: {}", edit.op),
            Self::NavRemove { edit, .. } => format!("Remove navigation edit: {}", edit.op),
            Self::NavLoad { edits, .. } => format!("Load {} navigation edit(s)", edits.len()),
            Self::SetTeam { items } => match items.as_slice() {
                [one] => format!("Set {} to {}", one.guid, one.to.word()),
                many => format!("Set the team of {}", objects(many.len())),
            },
            Self::LinkSpawner { point, spawner, linked } => {
                if *linked {
                    format!("Link spawner {spawner} to capture point {point}")
                } else {
                    format!("Unlink spawner {spawner} from capture point {point}")
                }
            }
            Self::SetTickets { to, .. } => format!(
                "Set tickets to {}, losing {} a minute",
                to.initial, to.loss_per_min as f32
            ),
            Self::InsertPoint { after, .. } => format!("Insert an outline point after {after}"),
            Self::RemovePoint { guid } => format!("Remove outline point {guid}"),
        }
    }
}

/// A blueprint's name from its reference or path: the last folder (`objects/props/crate/crate/<guid>`: `crate`).
#[must_use]
pub fn blueprint_name(reference: &str) -> &str {
    let parts: Vec<&str> = reference.split('/').filter(|s| !s.is_empty()).collect();
    let looks_guid = |s: &str| s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4;
    match parts.as_slice() {
        [.., name, last] if looks_guid(last) => name,
        [.., last] => last,
        [] => "an object",
    }
}

/// An operation of the log: its number in the project (never reused, also after an undo), who made it and when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Operation {
    /// Its number in the project.
    pub n: u64,
    /// Who made it (the PC's name, as `navtune.author`).
    pub by: String,
    /// When (UTC, ISO 8601).
    pub when: String,
    /// The editor's name for it when it is more than [`Op::describe`] ("Align on X").
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// What it does.
    #[serde(flatten)]
    pub op: Op,
}

impl Operation {
    /// Its name in the History panel.
    #[must_use]
    pub fn name(&self) -> String {
        if self.label.is_empty() {
            self.op.describe()
        } else {
            format!("{} ({})", self.label, self.op.describe())
        }
    }
}

/// A layer as the log leaves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    /// Its name.
    pub name: String,
    /// Its state.
    pub state: LayerState,
}

/// What the log says about the objects besides their partition data: layers, folders, groups, placements; and
/// which objects exist (made by duplicates) or were deleted.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Organisation {
    /// The layers in the order they were made.
    pub layers: Vec<Layer>,
    /// Each object's layer (upper-case guid).
    pub layer_of: HashMap<String, String>,
    /// Each object's folder.
    pub folder_of: HashMap<String, String>,
    /// The groups by name: their objects.
    pub groups: BTreeMap<String, Vec<String>>,
    /// Each object's last surface placement.
    pub placement: HashMap<String, Placement>,
    /// Objects deleted.
    pub deleted: BTreeSet<String>,
    /// Objects hidden one by one (besides their layer's state).
    pub hidden: BTreeSet<String>,
    /// Objects locked one by one.
    pub locked: BTreeSet<String>,
}

/// The layer colours offered to a new layer, in turn (muted, apart from the team, axis and selection colours).
pub const LAYER_COLOURS: [[u8; 3]; 8] = [
    [0x8f, 0xb4, 0x6a],
    [0xc9, 0x8f, 0xd1],
    [0xd6, 0xb0, 0x5c],
    [0x6a, 0xb8, 0xb0],
    [0xd1, 0x8a, 0x8a],
    [0x9a, 0xa4, 0xe0],
    [0xb5, 0xb5, 0x8a],
    [0xa0, 0xa0, 0xa0],
];

impl Organisation {
    /// The organisation after `ops`, in order.
    #[must_use]
    pub fn of<'a>(ops: impl IntoIterator<Item = &'a Operation>) -> Self {
        let mut o = Self::default();
        for op in ops {
            o.apply(&op.op);
        }
        o
    }

    fn delete(&mut self, guids: &[String]) {
        for g in guids {
            let g = g.to_ascii_uppercase();
            self.layer_of.remove(&g);
            self.folder_of.remove(&g);
            self.hidden.remove(&g);
            self.locked.remove(&g);
            for members in self.groups.values_mut() {
                members.retain(|m| *m != g);
            }
            self.deleted.insert(g);
        }
        self.groups.retain(|_, m| !m.is_empty());
    }

    #[allow(clippy::too_many_lines, reason = "one arm per operation, read top to bottom")]
    fn apply(&mut self, op: &Op) {
        let up = |g: &str| g.to_ascii_uppercase();
        match op {
            Op::Move { items } | Op::Rotate { items } => {
                for m in items {
                    if let Some(p) = m.placement {
                        self.placement.insert(up(&m.guid), p);
                    }
                }
            }
            Op::Delete { guids } => self.delete(guids),
            // the old layer taken out: as a delete
            Op::SetMode { to } => self.delete(&to.removed),
            Op::Duplicate { items } => {
                // a copy is on its source's layer and in its folder, not in its group; its placement goes with it
                let pairs = items.iter().flat_map(|d| {
                    std::iter::once((d.source.as_str(), d.guid.as_str()))
                        .chain(d.owned.iter().map(|o| (o.source.as_str(), o.guid.as_str())))
                });
                for (source, guid) in pairs {
                    let (s, g) = (up(source), up(guid));
                    if let Some(l) = self.layer_of.get(&s).cloned() {
                        self.layer_of.insert(g.clone(), l);
                    }
                    if let Some(f) = self.folder_of.get(&s).cloned() {
                        self.folder_of.insert(g.clone(), f);
                    }
                    if let Some(p) = self.placement.get(&s).copied() {
                        self.placement.insert(g, p);
                    }
                }
            }
            Op::SetField { .. }
            | Op::SetTeam { .. }
            | Op::LinkSpawner { .. }
            | Op::SetTickets { .. }
            | Op::InsertPoint { .. }
            | Op::RemovePoint { .. }
            | Op::SetStrip { .. }
            | Op::SetNames { .. }
            | Op::SetWeather { .. }
            | Op::NavAdd { .. }
            | Op::NavRemove { .. }
            | Op::NavLoad { .. } => {}
            Op::Place { items } => {
                for p in items {
                    if let Some(pl) = p.placement {
                        self.placement.insert(up(&p.guid), pl);
                    }
                }
            }
            Op::Hide { guids, hidden } => {
                for g in guids {
                    if *hidden {
                        self.hidden.insert(up(g));
                    } else {
                        self.hidden.remove(&up(g));
                    }
                }
            }
            Op::Lock { guids, locked } => {
                for g in guids {
                    if *locked {
                        self.locked.insert(up(g));
                    } else {
                        self.locked.remove(&up(g));
                    }
                }
            }
            Op::AddLayer { layer, colour } => {
                if !self.layers.iter().any(|l| l.name == *layer) {
                    self.layers.push(Layer {
                        name: layer.clone(),
                        state: LayerState {
                            hidden: false,
                            locked: false,
                            isolated: false,
                            colour: *colour,
                        },
                    });
                }
            }
            Op::SetLayer { layer, to, .. } => {
                if let Some(l) = self.layers.iter_mut().find(|l| l.name == *layer) {
                    l.state = to.clone();
                }
            }
            Op::RemoveLayer { layer } => {
                self.layers.retain(|l| l.name != *layer);
                self.layer_of.retain(|_, l| l != layer);
            }
            Op::AssignLayer { layer, guids } => {
                for g in guids {
                    if layer.is_empty() {
                        self.layer_of.remove(&up(g));
                    } else {
                        self.layer_of.insert(up(g), layer.clone());
                    }
                }
            }
            Op::SetFolder { folder, guids } => {
                for g in guids {
                    if folder.is_empty() {
                        self.folder_of.remove(&up(g));
                    } else {
                        self.folder_of.insert(up(g), folder.clone());
                    }
                }
            }
            Op::Group { group, guids } => {
                // an object is in one group: grouping takes it out of any other
                let members: Vec<String> = guids.iter().map(|g| up(g)).collect();
                for m in self.groups.values_mut() {
                    m.retain(|g| !members.contains(g));
                }
                self.groups.insert(group.clone(), members);
                self.groups.retain(|_, m| !m.is_empty());
            }
            Op::Ungroup { group } => {
                self.groups.remove(group);
            }
        }
    }

    /// The layer an object is on, with its state.
    #[must_use]
    pub fn layer(&self, guid: &str) -> Option<&Layer> {
        let name = self.layer_of.get(&guid.to_ascii_uppercase())?;
        self.layers.iter().find(|l| l.name == *name)
    }

    /// Whether an object is shown in the views: its layer is not hidden, and when any layer is isolated, its layer
    /// is one of them.
    #[must_use]
    pub fn shown(&self, guid: &str) -> bool {
        if self.hidden.contains(&guid.to_ascii_uppercase()) {
            return false;
        }
        let layer = self.layer(guid);
        if layer.is_some_and(|l| l.state.hidden) {
            return false;
        }
        let isolating = self.layers.iter().any(|l| l.state.isolated);
        !isolating || layer.is_some_and(|l| l.state.isolated)
    }

    /// Whether an object can be selected in the views and edited (its layer is not locked).
    #[must_use]
    pub fn editable(&self, guid: &str) -> bool {
        !self.locked.contains(&guid.to_ascii_uppercase()) && !self.layer(guid).is_some_and(|l| l.state.locked)
    }

    /// The group an object is in.
    #[must_use]
    pub fn group_of(&self, guid: &str) -> Option<(&str, &[String])> {
        let g = guid.to_ascii_uppercase();
        self.groups
            .iter()
            .find(|(_, m)| m.contains(&g))
            .map(|(n, m)| (n.as_str(), m.as_slice()))
    }

    /// A layer name not used yet (`Layer 1`, `Layer 2` ...).
    #[must_use]
    pub fn free_layer_name(&self) -> String {
        // one more name than there are layers is always enough
        (1..=self.layers.len() + 1)
            .map(|k| format!("Layer {k}"))
            .find(|n| !self.layers.iter().any(|l| l.name == *n))
            .unwrap_or_default()
    }

    /// A group name not used yet.
    #[must_use]
    pub fn free_group_name(&self) -> String {
        (1..=self.groups.len() + 1)
            .map(|k| format!("Group {k}"))
            .find(|n| !self.groups.contains_key(n))
            .unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assert_is_empty,
    clippy::doc_markdown
)]
mod tests {
    use super::*;

    fn op(n: u64, op: Op) -> Operation {
        Operation {
            n,
            by: "PC".into(),
            when: "2026-11-18T00:00:00Z".into(),
            label: String::new(),
            op,
        }
    }

    #[test]
    fn json_shape_and_names() {
        let o = op(
            1,
            Op::Move {
                items: vec![Moved {
                    guid: "AB".into(),
                    from: crate::rows_at([0.0, 0.0, 0.0]),
                    to: crate::rows_at([3.0, 4.0, 0.0]),
                    placement: Some(Placement {
                        sink: 0.25,
                        unit: SinkUnit::Fraction,
                        along_normal: false,
                        align_to_normal: false,
                    }),
                }],
            },
        );
        let j = serde_json::to_value(&o).unwrap();
        assert_eq!(j["op"], "move");
        assert_eq!(j["n"], 1);
        assert_eq!(j["items"][0]["placement"]["unit"], "fraction");
        let back: Operation = serde_json::from_value(j).unwrap();
        assert_eq!(back, o);
        assert_eq!(
            back.name(),
            "Place 1 object on the surface, sunk 25 % of its height straight down"
        );
        let f = op(
            2,
            Op::SetField {
                items: vec![FieldSet {
                    guid: "AB".into(),
                    field: "Name".into(),
                    from: None,
                    to: FieldValue::Text("Bunker".into()),
                }],
            },
        );
        let j = serde_json::to_value(&f).unwrap();
        assert_eq!(
            j["items"][0]["to"],
            serde_json::json!({"type": "text", "value": "Bunker"})
        );
        assert_eq!(f.name(), "Set Name to Bunker");
        assert!(f.op.edits_partition());
        assert!(!op(3, Op::Ungroup { group: "G".into() }).op.edits_partition());
    }

    #[test]
    fn organisation_follows_the_log() {
        let ops = vec![
            op(
                1,
                Op::AddLayer {
                    layer: "Rocks".into(),
                    colour: LAYER_COLOURS[0],
                },
            ),
            op(
                2,
                Op::AssignLayer {
                    layer: "Rocks".into(),
                    guids: vec!["a".into(), "b".into()],
                },
            ),
            op(
                3,
                Op::Group {
                    group: "G".into(),
                    guids: vec!["a".into(), "c".into()],
                },
            ),
            op(
                4,
                Op::Duplicate {
                    items: vec![Duplicated {
                        source: "a".into(),
                        guid: "d".into(),
                        transform: crate::rows_at([0.0; 3]),
                        owned: Vec::new(),
                    }],
                },
            ),
            op(
                5,
                Op::SetLayer {
                    layer: "Rocks".into(),
                    from: LayerState {
                        hidden: false,
                        locked: false,
                        isolated: false,
                        colour: LAYER_COLOURS[0],
                    },
                    to: LayerState {
                        hidden: true,
                        locked: true,
                        isolated: false,
                        colour: LAYER_COLOURS[0],
                    },
                },
            ),
            op(
                6,
                Op::Delete {
                    guids: vec!["c".into()],
                },
            ),
        ];
        let o = Organisation::of(&ops);
        assert_eq!(o.layer("A").unwrap().name, "Rocks");
        assert_eq!(o.layer("D").unwrap().name, "Rocks");
        assert!(!o.shown("a") && !o.editable("b") && o.shown("c") && o.editable("c"));
        assert_eq!(o.group_of("a").unwrap().1, ["A"]);
        assert!(o.deleted.contains("C"));
        assert_eq!(o.free_layer_name(), "Layer 1");
        assert_eq!(o.free_group_name(), "Group 1");
        // isolating a layer hides what is on no layer
        let mut iso = ops.clone();
        iso.push(op(
            7,
            Op::SetLayer {
                layer: "Rocks".into(),
                from: o.layers[0].state.clone(),
                to: LayerState {
                    hidden: false,
                    locked: false,
                    isolated: true,
                    colour: LAYER_COLOURS[0],
                },
            },
        ));
        let o = Organisation::of(&iso);
        assert!(o.shown("a") && !o.shown("x"));
    }
}
