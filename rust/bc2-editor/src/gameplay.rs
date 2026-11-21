//! The asset browser's Spawners and Objectives modes (round E2b, the user's request): tiles for the gameplay entities
//! a level is made of, drawn from its own data - the entity types its partition holds, each kind's first placed
//! instance the template, the sub-instances that instance owns created with it (a flag's capture area, its marker,
//! its spawn points: levelwrite's `place` with `owned`, the deep duplicate's machinery), the team from the browser's
//! team picker.
//!
//! - **Spawners**: player spawn points (`AlternateSpawnEntityData`), HQ / base spawns (`SoldierSpawnEntityData` of
//!   human players), AI spawners (the AI's), vehicle spawners - one tile per vehicle the level spawns, then every
//!   vehicle and stationary weapon of the content set (from the asset catalogue: a spawner of this level with its
//!   `Vehicle` set to it).
//! - **Objectives**: capture points (Conquest flags; Rush's stage points) with their capture area, M-COMs (the
//!   `mpobjective` objects), combat areas, and the other objective entities the data has (mission objectives,
//!   arm / disarm awards, life and ticket counters: Rush, Onslaught, Squad Rush).
//!
//! **E4's hooks** (the logic wiring is the next round's): [`Tile::wiring`] says what a placed one still needs linked
//! (a spawn to its flag or base, an M-COM to its stage, a stage to the attackers' order, tickets); placements copy
//! only the event connections inside what they copy, never the template's links to the level's logic. Kinds a level
//! does not hold (an M-COM on a Conquest-only level) come from other stock levels in E4 (a graft from their
//! partitions); here a kind is offered where the level has a template.

use std::collections::BTreeMap;

use bc2_formats::dbx::edit::NodeId;
use bc2_levelwrite::LevelWriter;

use crate::content::Catalogue;
use crate::edit::Doc;

/// The asset browser's modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum BrowserMode {
    /// Every placeable map object.
    #[default]
    PlaceObjects,
    /// Spawn points and spawners.
    Spawners,
    /// Flags, M-COMs, combat areas and the other objectives.
    Objectives,
    /// Every mesh of the game copy by name, for the asset preview (round GR3: the mesh viewer's `find`).
    Meshes,
}

impl BrowserMode {
    /// Every mode, in the browser's order.
    pub const ALL: [Self; 4] = [Self::PlaceObjects, Self::Spawners, Self::Objectives, Self::Meshes];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PlaceObjects => "Place Objects",
            Self::Spawners => "Spawners",
            Self::Objectives => "Objectives",
            Self::Meshes => "Meshes",
        }
    }
}

/// What a tile places.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    /// A player spawn point.
    SpawnPoint,
    /// An HQ or base spawn.
    BaseSpawn,
    /// An AI spawner.
    AiSpawner,
    /// A vehicle spawner (its vehicle's path; empty for one without a vehicle).
    VehicleSpawner(String),
    /// A capture point with its capture area.
    CapturePoint,
    /// An M-COM (its blueprint's path).
    Mcom(String),
    /// A combat area.
    CombatArea,
    /// Another objective entity (its type).
    Objective(String),
}

/// One tile of the Spawners or Objectives mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    /// Its mode.
    pub mode: BrowserMode,
    /// What it places.
    pub kind: Kind,
    /// Its label.
    pub label: String,
    /// The instance placed copies (upper-case guid).
    pub template: String,
    /// How many the level has.
    pub count: usize,
    /// The template's team field (`Team`, `InitialOwnerTeam`), set from the team picker.
    pub team_field: Option<&'static str>,
    /// The sub-instances the template owns (created with it).
    pub owned: Vec<String>,
    /// A vehicle spawner's vehicle: the reference its `Vehicle` takes (empty: the template's; a catalogue
    /// vehicle's path to read when placed: [`Tile::vehicle_path`]).
    pub vehicle: String,
    /// A vehicle of the content set not spawned in this level yet: its path in the asset catalogue.
    pub vehicle_path: Option<String>,
    /// Why it cannot be placed (no position: logic set up with the mode in E4), or `None`.
    pub why_not: Option<&'static str>,
    /// What E4 still has to link for a placed one.
    pub wiring: &'static str,
}

impl Tile {
    /// Its key (drag and drop, tests).
    #[must_use]
    pub fn key(&self) -> String {
        format!("{:?}", self.kind)
    }
}

fn text(w: &LevelWriter, e: NodeId, field: &[u8]) -> Option<String> {
    let c = w.d.child(e, field)?;
    w.d.text(c).map(|t| String::from_utf8_lossy(t).into_owned())
}

fn reference(w: &LevelWriter, e: NodeId, field: &[u8]) -> Option<String> {
    let c = w.d.child(e, field)?;
    w.d.attr(c, b"ref").map(|r| String::from_utf8_lossy(r).to_lowercase())
}

fn guid(w: &LevelWriter, e: NodeId) -> String {
    String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase()
}

/// Whether a catalogue path is a vehicle or a stationary weapon (what a vehicle spawner spawns).
#[must_use]
pub fn is_vehicle_path(path: &str) -> bool {
    path.starts_with("objects/vehicles/")
        || path.starts_with("objects/weapons/stationary/")
        || path.starts_with("vehicles/")
}

/// The Spawners and Objectives tiles of a project's level (and the content set's vehicles from the catalogue).
#[must_use]
pub fn tiles(doc: &Doc, catalogue: Option<&Catalogue>) -> Vec<Tile> {
    let Some(w) = doc.writer() else { return Vec::new() };
    let graph = w.ownership();
    let mut groups: BTreeMap<Kind, (BrowserMode, String, Vec<NodeId>)> = BTreeMap::new();
    for e in w.d.instances() {
        let t = w.type_of(e);
        let short = t.rsplit('.').next().unwrap_or(&t).to_owned();
        let (mode, kind, label) = match short.as_str() {
            "AlternateSpawnEntityData" => (BrowserMode::Spawners, Kind::SpawnPoint, "Player spawn point".to_owned()),
            "SoldierSpawnEntityData" => {
                let human = text(w, e, b"PlayerType").is_none_or(|p| p.is_empty() || p == "SSHumanPlayer");
                if human {
                    (BrowserMode::Spawners, Kind::BaseSpawn, "HQ / base spawn".to_owned())
                } else {
                    (BrowserMode::Spawners, Kind::AiSpawner, "AI spawner".to_owned())
                }
            }
            "VehicleSpawnEntityData" => {
                let v = reference(w, e, b"Vehicle").filter(|r| r != "null").unwrap_or_default();
                let path = crate::edit::blueprint_path(&v);
                let name = if v.is_empty() {
                    "(none)".to_owned()
                } else {
                    bc2_levelwrite::ops::blueprint_name(&path).to_owned()
                };
                (
                    BrowserMode::Spawners,
                    Kind::VehicleSpawner(v),
                    format!("Vehicle spawner: {name}"),
                )
            }
            "CapturePointEntityData" => (
                BrowserMode::Objectives,
                Kind::CapturePoint,
                "Capture point (flag)".to_owned(),
            ),
            "CombatAreaTriggerEntityData" => (BrowserMode::Objectives, Kind::CombatArea, "Combat area".to_owned()),
            "ReferenceObjectData" => {
                let Some(r) = reference(w, e, b"ReferencedObject").filter(|r| r.contains("mpobjective")) else {
                    continue;
                };
                let path = crate::edit::blueprint_path(&r);
                let name = bc2_levelwrite::ops::blueprint_name(&path).to_owned();
                (BrowserMode::Objectives, Kind::Mcom(path), format!("M-COM: {name}"))
            }
            s if [
                "MissionObjectiveEntityData",
                "ArmDisarmAwardEntityData",
                "LifeCounterEntityData",
                "TicketCounterEntityData",
            ]
            .contains(&s)
                || s.contains("Objective") =>
            {
                let label = match s {
                    "MissionObjectiveEntityData" => "Mission objective".to_owned(),
                    "ArmDisarmAwardEntityData" => "Arm / disarm award (M-COM)".to_owned(),
                    "LifeCounterEntityData" => "Life counter (Rush tickets)".to_owned(),
                    "TicketCounterEntityData" => "Ticket counter".to_owned(),
                    other => other.trim_end_matches("EntityData").to_owned(),
                };
                (BrowserMode::Objectives, Kind::Objective(s.to_owned()), label)
            }
            _ => continue,
        };
        groups
            .entry(kind)
            .or_insert_with(|| (mode, label, Vec::new()))
            .2
            .push(e);
    }
    let mut out = Vec::new();
    let mut spawner_template = None;
    for (kind, (mode, label, insts)) in groups {
        // the template: the instance with a transform that owns the least, the first of equals
        let Some(&first) = insts.first() else { continue };
        let template = insts
            .iter()
            .copied()
            .filter(|&e| w.d.child(e, b"Transform").is_some())
            .min_by_key(|&e| graph.owned(&guid(w, e)).len())
            .unwrap_or(first);
        let g = guid(w, template);
        let positioned = w.d.child(template, b"Transform").is_some();
        let team_field = ["Team", "InitialOwnerTeam"]
            .into_iter()
            .find(|f| w.d.child(template, f.as_bytes()).is_some());
        if matches!(kind, Kind::VehicleSpawner(_)) && positioned {
            spawner_template.get_or_insert_with(|| (g.clone(), team_field));
        }
        let wiring = match &kind {
            Kind::SpawnPoint => "E4: link it to its flag's or base's spawner (AlternativeSpawnPoints)",
            Kind::BaseSpawn | Kind::AiSpawner => "E4: give it its spawn points and its team's base",
            Kind::VehicleSpawner(_) => "E4: link it to the flag or base that owns it",
            Kind::CapturePoint => "E4: tickets, the mode's flag order and its spawns' links",
            Kind::Mcom(_) => "E4: its Rush stage (attackers' order, M-COM pairs per stage)",
            Kind::CombatArea => "E4: which team it bounds and when (Rush stages)",
            Kind::Objective(_) => "E4: the mode's logic (stages, tickets, events)",
        };
        out.push(Tile {
            mode,
            vehicle: match &kind {
                Kind::VehicleSpawner(v) => v.clone(),
                _ => String::new(),
            },
            kind,
            label,
            owned: graph.owned(&g),
            template: g,
            count: insts.len(),
            team_field,
            vehicle_path: None,
            why_not: (!positioned)
                .then_some("It has no position: the mode's logic objects are set up with the mode (E4)"),
            wiring,
        });
    }
    // every other vehicle of the content set: a spawner of this level with its Vehicle set to it
    if let (Some((template, team_field)), Some(cat)) = (spawner_template, catalogue) {
        let have: Vec<String> = out
            .iter()
            .filter(|t| matches!(t.kind, Kind::VehicleSpawner(_)))
            .map(|t| crate::edit::blueprint_path(&t.vehicle))
            .collect();
        for e in cat
            .entries
            .iter()
            .filter(|e| is_vehicle_path(&e.path) && !have.contains(&e.path))
        {
            out.push(Tile {
                mode: BrowserMode::Spawners,
                kind: Kind::VehicleSpawner(e.path.clone()),
                label: format!("Vehicle spawner: {}", e.name),
                template: template.clone(),
                count: 0,
                team_field,
                owned: Vec::new(),
                vehicle: e.reference.clone(),
                vehicle_path: Some(e.path.clone()),
                why_not: None,
                wiring: "E4: link it to the flag or base that owns it",
            });
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_demo_levels_tiles() {
        let dir = std::env::temp_dir().join(format!("bc2_editor_gameplay_{}", std::process::id()));
        let s = crate::demo::level(&dir.join("level")).unwrap();
        let d = Doc::create("Gameplay tiles", &dir, "demo", s).unwrap();
        let t = tiles(&d, None);
        let labels: Vec<&str> = t.iter().map(|t| t.label.as_str()).collect();
        assert!(labels.contains(&"Player spawn point"), "{labels:?}");
        assert!(labels.contains(&"Capture point (flag)"));
        let flag = t.iter().find(|t| t.kind == Kind::CapturePoint).unwrap();
        assert_eq!(flag.owned.len(), 5, "its ring and its marker");
        assert_eq!(flag.team_field, Some("InitialOwnerTeam"));
        assert!(is_vehicle_path("objects/vehicles/land/cobra/cobra"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
