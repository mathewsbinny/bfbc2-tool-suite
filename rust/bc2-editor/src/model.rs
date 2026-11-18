//! What the panels show of a scene: the outliner's kinds, groups and entries, names as the user would say them, and
//! the selected entity's properties.
//!
//! The outliner lists every entity of the scene, as the Map Inspector does: the level partition's instances, then
//! the static groups' members. Each entity sits in a group by its category (`bc2_scene::scene::category_of`, refined
//! by the blueprint for placed objects) and each group in a kind (gameplay, vehicles, AI, areas, objects, other:
//! `EDITOR_ARCHITECTURE.md` 3.9). [`inspector_layer`] is the Map Inspector's layer of an entity (`layer_name`), kept
//! so the lists can be compared with the inspector's (`tests/inspector_parity.rs`).

use bc2_scene::scene::{Entity, Scene};

/// The outliner's top level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Spawns, flags, objectives.
    Gameplay,
    /// Vehicle spawns.
    Vehicles,
    /// AI orders.
    Ai,
    /// Combat areas and area points.
    Areas,
    /// Placed objects, destructibles, dynamic objects, ladders, effects, static group members.
    Objects,
    /// Everything else (level data, logic, sounds ...).
    Other,
}

impl Kind {
    /// Every kind in outliner order.
    pub const ALL: [Self; 6] = [
        Self::Gameplay,
        Self::Vehicles,
        Self::Ai,
        Self::Areas,
        Self::Objects,
        Self::Other,
    ];

    /// Its name in the outliner.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Gameplay => "Gameplay",
            Self::Vehicles => "Vehicles",
            Self::Ai => "AI",
            Self::Areas => "Areas",
            Self::Objects => "Objects",
            Self::Other => "Other",
        }
    }
}

/// The kind and the group's name (plural) of a category.
#[must_use]
pub fn group_of(category: &str) -> (Kind, &'static str) {
    match category {
        "soldier spawn" => (Kind::Gameplay, "Soldier spawns"),
        "capture point" => (Kind::Gameplay, "Flags"),
        "objective" => (Kind::Gameplay, "Objectives"),
        "vehicle spawn" => (Kind::Vehicles, "Vehicle spawns"),
        "ai order" => (Kind::Ai, "AI orders"),
        "combat area" => (Kind::Areas, "Combat areas"),
        "area point" => (Kind::Areas, "Area points"),
        "placed object" => (Kind::Objects, "Placed objects"),
        "destructible" => (Kind::Objects, "Destructibles"),
        "dynamic object" => (Kind::Objects, "Dynamic objects"),
        "ladder" => (Kind::Objects, "Ladders"),
        "effect" => (Kind::Objects, "Effects"),
        "group member" => (Kind::Objects, "Static group members"),
        _ => (Kind::Other, "Other"),
    }
}

/// The groups hidden in the 3D view when a level opens (the Map Inspector's `HIDDEN_AT_START`).
pub const HIDDEN_AT_START: [&str; 3] = ["area point", "other", "effect"];

/// The Map Inspector's layer of an entity (`inspector.layer_name`): spawns by team, else the category.
#[must_use]
pub fn inspector_layer(e: &Entity) -> String {
    match (e.category.as_str(), e.team) {
        ("soldier spawn", Some(1)) => "soldier spawn, team 1".into(),
        ("soldier spawn", Some(2)) => "soldier spawn, team 2".into(),
        ("soldier spawn", _) => "soldier spawn, no team".into(),
        ("vehicle spawn", Some(1)) => "vehicle spawn, team 1".into(),
        ("vehicle spawn", Some(2)) => "vehicle spawn, team 2".into(),
        ("vehicle spawn", _) => "vehicle spawn, neutral".into(),
        (c, _) => c.to_owned(),
    }
}

/// A team's short label, shown beside its colour (never the colour alone): `T1`, `T2`, `NEU`.
#[must_use]
pub fn team_label(team: u8) -> &'static str {
    match team {
        1 => "T1",
        2 => "T2",
        _ => "NEU",
    }
}

/// A team's name in the properties panel.
#[must_use]
pub fn team_name(team: u8) -> &'static str {
    match team {
        1 => "Team 1",
        2 => "Team 2",
        _ => "Neutral",
    }
}

/// An entity's name as the user would say it: its `Name`, else its id, else its blueprint's last part, else its
/// type.
#[must_use]
pub fn display_name(e: &Entity) -> String {
    let pick = |s: &Option<String>| s.as_deref().filter(|v| !v.is_empty()).map(str::to_owned);
    pick(&e.name)
        .or_else(|| pick(&e.id))
        .or_else(|| {
            e.blueprint
                .as_deref()
                .and_then(|b| b.rsplit('/').next())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
        })
        .unwrap_or_else(|| e.short_type.clone())
}

/// One row of the outliner.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// The entity's index in the scene.
    pub entity: usize,
    /// Its name ([`display_name`]).
    pub name: String,
    /// Its category.
    pub category: String,
    /// Its team.
    pub team: Option<u8>,
    /// Its position.
    pub position: Option<[f64; 3]>,
    /// What the search matches: guid, type, id, name and blueprint, lower case.
    haystack: String,
}

/// A group of the outliner: one category.
#[derive(Debug, Clone, PartialEq)]
pub struct Group {
    /// Its kind.
    pub kind: Kind,
    /// The category.
    pub category: String,
    /// The plural name.
    pub name: &'static str,
    /// Its entries in scene order.
    pub entries: Vec<Entry>,
}

/// The outliner's model of a scene.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Outliner {
    /// Groups in kind order, then by name.
    pub groups: Vec<Group>,
}

impl Outliner {
    /// The outliner of a scene: every entity, grouped.
    #[must_use]
    pub fn new(scene: &Scene) -> Self {
        let mut groups: Vec<Group> = Vec::new();
        for e in &scene.entities {
            let entry = Entry {
                entity: e.index,
                name: display_name(e),
                category: e.category.clone(),
                team: e.team,
                position: e.position(),
                haystack: [
                    e.guid.as_str(),
                    e.type_name.as_str(),
                    e.id.as_deref().unwrap_or(""),
                    e.name.as_deref().unwrap_or(""),
                    e.blueprint.as_deref().unwrap_or(""),
                ]
                .join(" ")
                .to_lowercase(),
            };
            if let Some(g) = groups.iter_mut().find(|g| g.category == e.category) {
                g.entries.push(entry);
            } else {
                let (kind, name) = group_of(&e.category);
                groups.push(Group {
                    kind,
                    category: e.category.clone(),
                    name,
                    entries: vec![entry],
                });
            }
        }
        groups.sort_by(|a, b| (a.kind, a.name).cmp(&(b.kind, b.name)));
        Self { groups }
    }

    /// Every entry, in outliner order.
    pub fn entries(&self) -> impl Iterator<Item = &Entry> {
        self.groups.iter().flat_map(|g| g.entries.iter())
    }

    /// The entries of `group` that the search matches (every one for an empty search; otherwise those whose guid,
    /// type, id, name or blueprint contains it, ignoring case, as the Map Inspector's search does).
    #[must_use]
    pub fn matching<'a>(group: &'a Group, query: &str) -> Vec<&'a Entry> {
        let q = query.trim().to_lowercase();
        group
            .entries
            .iter()
            .filter(|e| q.is_empty() || e.haystack.contains(&q) || e.name.to_lowercase().contains(&q))
            .collect()
    }

    /// The group (index) and row of an entity.
    #[must_use]
    pub fn locate(&self, entity: usize) -> Option<(usize, usize)> {
        self.groups
            .iter()
            .enumerate()
            .find_map(|(g, grp)| grp.entries.iter().position(|e| e.entity == entity).map(|r| (g, r)))
    }
}

/// A field of the properties panel: label, value, and whether the value is data (shown in the mono `value` style).
pub type Field = (&'static str, String, bool);

/// The selected entity's fields, read-only: type, guid, transform, team, source partition ...
#[must_use]
pub fn properties(e: &Entity) -> Vec<Field> {
    let mut f: Vec<Field> = vec![
        ("Name", display_name(e), false),
        ("Type", e.short_type.clone(), true),
        ("Full type", e.type_name.clone(), true),
        ("Category", e.category.clone(), false),
        ("GUID", e.guid.clone(), true),
    ];
    if let Some(id) = e.id.as_deref().filter(|v| !v.is_empty()) {
        f.push(("Id", id.to_owned(), true));
    }
    if let Some(t) = e.team {
        f.push(("Team", format!("{} ({})", team_name(t), team_label(t)), false));
    }
    if let Some(ai) = e.ai {
        f.push((
            "Player type",
            if ai { "AI soldiers" } else { "Human players" }.to_owned(),
            false,
        ));
    }
    if let Some(bp) = &e.blueprint {
        f.push(("Blueprint", bp.clone(), true));
    }
    if let Some(bt) = &e.blueprint_type {
        f.push(("Blueprint object", bt.clone(), true));
    }
    if e.destructible {
        f.push(("Destructible", "Yes (health states or breakable parts)".into(), false));
    }
    let source = match &e.group {
        Some(g) => format!("{} ({g})", e.partition),
        None => e.partition.clone(),
    };
    f.push(("Source partition", source, true));
    let where_ = match e.source {
        bc2_scene::scene::Source::Instance { instance } => format!("instance {instance}"),
        bc2_scene::scene::Source::GroupMember { file, instance, item } => {
            format!("group file {file}, instance {instance}, item {item}")
        }
    };
    f.push(("Source element", where_, true));
    f
}

/// The transform's rows for the properties panel: position and the right / up / forward axes, in metres.
#[must_use]
pub fn transform_rows(e: &Entity) -> Vec<(&'static str, [f64; 3])> {
    let Some(m) = e.matrix else {
        return Vec::new();
    };
    let row = |r: usize| -> [f64; 3] { std::array::from_fn(|c| m.get(r * 4 + c).copied().unwrap_or(0.0)) };
    vec![
        ("Position", row(3)),
        ("Right", row(0)),
        ("Up", row(1)),
        ("Forward", row(2)),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn groups_and_search() {
        let s = crate::demo::scene();
        let o = Outliner::new(&s);
        let names: Vec<_> = o.groups.iter().map(|g| (g.kind, g.name)).collect();
        assert_eq!(
            names,
            [
                (Kind::Gameplay, "Flags"),
                (Kind::Gameplay, "Soldier spawns"),
                (Kind::Vehicles, "Vehicle spawns"),
                (Kind::Areas, "Area points"),
                (Kind::Objects, "Placed objects"),
            ]
        );
        assert_eq!(o.entries().count(), 5);
        assert_eq!(o.groups[4].entries[0].name, "House");
        assert_eq!(o.groups[3].entries[0].name, "AreaGeometryEntityData");
        assert_eq!(Outliner::matching(&o.groups[1], "spawn 1").len(), 1);
        assert_eq!(Outliner::matching(&o.groups[1], "0000-000000000001").len(), 1);
        assert_eq!(Outliner::matching(&o.groups[1], "nothing").len(), 0);
        assert_eq!(o.locate(0), Some((4, 0)));
        assert_eq!(inspector_layer(&s.entities[1]), "soldier spawn, team 1");
        assert_eq!(inspector_layer(&s.entities[3]), "vehicle spawn, team 2");
        let p = properties(&s.entities[1]);
        assert!(p.iter().any(|(k, v, _)| *k == "Team" && v == "Team 1 (T1)"));
        assert_eq!(transform_rows(&s.entities[2])[0].1, [20.0, 0.0, -15.0]);
    }
}
