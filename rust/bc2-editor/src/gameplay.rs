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
use bc2_levelwrite::gameplay::{self, Rules, Team};
use bc2_levelwrite::placement::Mode;
use bc2_levelwrite::{LevelWriter, Op};

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

// ---------------------------------------------------------------------------------------------------------------
// Round E4: the gameplay tools (the Gameplay workspace's tool on the views, its panel) - each a levelwrite
// operation made from the level as the log leaves it ([`bc2_levelwrite::gameplay`]).

/// What the gameplay tool does with a click on a view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GameplayKind {
    /// Adds a flag (Conquest): a copy of the level's smallest flag with its unit, owned by the picker's team.
    #[default]
    Flag,
    /// Adds a spawn point to the selected spawner (or the selected flag's, else the nearest flag's or base's).
    SpawnPoint,
    /// Adds an M-COM to Rush stage `k` (play order, from 0).
    Mcom(usize),
    /// Adds a soldier spawner to the selected capture point (else the nearest): a copy of one of its spawners with
    /// its spawn points, linked to it (round E4-rest).
    Spawner,
    /// Adds a Rush stage after the last: a copy of the last stage with its M-COMs, bases, spawns and combat areas,
    /// its M-COMs' middle where the click is (round E4-rest, GAME-RUSH-11).
    Stage,
    /// Carries a flag, base, spawner or M-COM with what moves with it: a click picks it up, the next puts it down.
    Carry,
}

impl GameplayKind {
    /// Its name.
    #[must_use]
    pub fn name(self) -> String {
        match self {
            Self::Flag => "Add flag".into(),
            Self::SpawnPoint => "Add spawn point".into(),
            Self::Mcom(k) => format!("Add M-COM to stage {}", k + 1),
            Self::Spawner => "Add spawner".into(),
            Self::Stage => "Add stage".into(),
            Self::Carry => "Carry".into(),
        }
    }

    /// What a click does, for the status bar.
    #[must_use]
    pub fn hint(self) -> &'static str {
        match self {
            Self::Flag => "Click the ground to add a flag there with its zone, marker and spawns; Esc stops",
            Self::SpawnPoint => {
                "Click the ground to add a spawn point to the selected spawner (or the selected or nearest flag's)"
            }
            Self::Mcom(_) => "Click the ground to add an M-COM to the stage, wired to its counters; Esc stops",
            Self::Spawner => {
                "Click the ground to add a soldier spawner with its spawn points to the selected (or nearest) flag or base"
            }
            Self::Stage => {
                "Click the ground where the new last stage's M-COMs go: the last stage is copied there with its bases,                  spawns and combat areas, and wired after it"
            }
            Self::Carry => {
                "Click a flag, base, spawner or M-COM to pick it up with what moves with it, click again to put it \
                 down; Esc drops it"
            }
        }
    }
}

/// A position as the partitions store it.
fn at32(p: crate::math::V3) -> [f32; 3] {
    #[allow(clippy::cast_possible_truncation, reason = "transforms are singles")]
    p.map(|x| x as f32)
}

/// The team picker's team.
#[must_use]
pub fn team_of_picker(team: u8) -> Team {
    match team {
        1 => Team::Team1,
        2 => Team::Team2,
        _ => Team::TeamNeutral,
    }
}

/// The soldier spawner a new spawn point at `at` goes to: a selected spawner; else a selected capture point's
/// spawner (the picker's team's when it has one per team); else the nearest capture point's.
#[must_use]
pub fn spawner_for(doc: &Doc, selection: &[usize], at: crate::math::V3, team: Team) -> Option<String> {
    let w = doc.writer()?;
    let is = |g: &str, ty: &str| w.find(g).is_some_and(|e| w.type_of(e).ends_with(ty));
    let picked: Vec<String> = selection
        .iter()
        .filter_map(|&k| doc.guid(k).map(str::to_owned))
        .collect();
    if let Some(s) = picked.iter().find(|g| is(g, ".SoldierSpawnEntityData")) {
        return Some(s.clone());
    }
    let of_point = |cp: &str| -> Option<String> {
        let ss: Vec<String> = gameplay::spawners(w, cp)
            .into_iter()
            .filter(|s| is(s, ".SoldierSpawnEntityData"))
            .collect();
        ss.iter()
            .find(|s| gameplay::team_of(w, s) == Some(team))
            .or_else(|| ss.first())
            .cloned()
    };
    if let Some(s) = picked
        .iter()
        .filter(|g| is(g, ".CapturePointEntityData"))
        .find_map(|g| of_point(g))
    {
        return Some(s);
    }
    let d2 = |g: &str| {
        w.transform(g).map_or(f64::INFINITY, |r| {
            let p = r[3];
            (f64::from(p[0]) - at[0]).powi(2) + (f64::from(p[2]) - at[2]).powi(2)
        })
    };
    let mut points: Vec<String> =
        w.d.of_type(b".CapturePointEntityData")
            .into_iter()
            .map(|e| guid(w, e))
            .collect();
    points.sort_by(|a, b| d2(a).total_cmp(&d2(b)));
    points.iter().find_map(|g| of_point(g))
}

/// The Rush stages in play order (their M-COMs), or the stages as the M-COMs group when the order is not known.
#[must_use]
pub fn rush_stages(doc: &Doc) -> Vec<Vec<String>> {
    let Some(w) = doc.writer() else { return Vec::new() };
    let (play, _) = bc2_levelwrite::modelayer::rush_play(w);
    if play.is_empty() {
        bc2_levelwrite::modelayer::stages(w)
    } else {
        play.into_iter().map(|s| s.mcoms).collect()
    }
}

/// The operation a gameplay tool's click at `at` makes (with its History label and the new object's guid), or why
/// not in words.
///
/// # Errors
/// The level has no partition, or levelwrite refuses (no template, no spawner, no such stage).
pub fn place_op(
    doc: &Doc,
    kind: GameplayKind,
    at: crate::math::V3,
    team: Team,
    selection: &[usize],
) -> Result<(Op, String, String), String> {
    let w = doc.writer().ok_or("this level has no partition to edit")?;
    let guid = |s: &str| doc.project.new_guid(s);
    let p = at32(at);
    match kind {
        GameplayKind::Flag => {
            let letter = gameplay::next_letter(w);
            let (op, g) = gameplay::add_flag(w, p, team, &guid).map_err(|e| e.to_string())?;
            Ok((op, format!("Add flag {letter}"), g))
        }
        GameplayKind::SpawnPoint => {
            let s = spawner_for(doc, selection, at, team)
                .ok_or("the level has no soldier spawner to add a spawn point to: select one")?;
            let (op, g) = gameplay::add_spawn_point(w, &s, p, &guid).map_err(|e| e.to_string())?;
            Ok((op, "Add spawn point".into(), g))
        }
        GameplayKind::Mcom(k) => {
            let stages = rush_stages(doc);
            let stage = stages
                .get(k)
                .ok_or_else(|| format!("the level has no Rush stage {}", k + 1))?;
            let (op, g) = gameplay::add_mcom(w, &doc.schema, stage, p, &guid).map_err(|e| e.to_string())?;
            Ok((op, format!("Add M-COM to stage {}", k + 1), g))
        }
        GameplayKind::Spawner => {
            let point = point_for(doc, selection, at).ok_or("the level has no flag or base to add a spawner to")?;
            let (op, g) = gameplay::add_spawner(w, &point, p, team, &guid).map_err(|e| e.to_string())?;
            Ok((op, "Add spawner".into(), g))
        }
        GameplayKind::Stage => {
            let ground = |x: f64, z: f64| -> Option<f64> {
                crate::query::surface_below_except(&doc.scene, x, z, &|_| true)
                    .filter(|h| h.entity.is_none())
                    .map(|h| h.point[1])
            };
            let n = rush_stages(doc).len();
            let s = bc2_levelwrite::rushstage::add_stage(w, &doc.schema, p, &guid, &ground).map_err(|e| e.to_string())?;
            let first = s.mcoms.first().cloned().unwrap_or_default();
            Ok((s.op, format!("Add Rush stage {}", n + 1), first))
        }
        GameplayKind::Carry => Err("the carry tool moves objects; it adds none".into()),
    }
}

/// The capture point a new spawner at `at` goes to: a selected one, else the nearest.
#[must_use]
pub fn point_for(doc: &Doc, selection: &[usize], at: crate::math::V3) -> Option<String> {
    let w = doc.writer()?;
    let is_point = |g: &str| w.find(g).is_some_and(|e| w.type_of(e).ends_with(".CapturePointEntityData"));
    if let Some(g) = selection.iter().filter_map(|&k| doc.guid(k)).find(|g| is_point(g)) {
        return Some(g.to_owned());
    }
    let d2 = |g: &str| {
        w.transform(g).map_or(f64::INFINITY, |r| {
            let p = r[3];
            (f64::from(p[0]) - at[0]).powi(2) + (f64::from(p[2]) - at[2]).powi(2)
        })
    };
    let mut points: Vec<String> =
        w.d.of_type(b".CapturePointEntityData")
            .into_iter()
            .map(|e| guid(w, e))
            .collect();
    points.sort_by(|a, b| d2(a).total_cmp(&d2(b)));
    points.into_iter().next()
}

/// A Rush stage taken out with what belongs to it (round E4-rest): one operation, or why not in words.
///
/// # Errors
/// levelwrite refuses (not DICE's stage wiring, the level's only stage).
pub fn remove_stage_op(doc: &Doc, k: usize) -> Result<Op, String> {
    let w = doc.writer().ok_or("this level has no partition to edit")?;
    bc2_levelwrite::rushstage::remove_stage(w, &doc.schema, k).map_err(|e| e.to_string())
}

/// A flag renamed (round E4-rest): one operation, or why not in words.
///
/// # Errors
/// levelwrite refuses (not a capture point, an empty name, the same name).
pub fn rename_op(doc: &Doc, point: &str, name: &str) -> Result<Op, String> {
    let w = doc.writer().ok_or("this level has no partition to edit")?;
    gameplay::rename_flag(w, point, name).map_err(|e| e.to_string())
}

/// Whether an object is one the gameplay tools carry and delete with its unit: a capture point, a soldier spawner,
/// a spawn point, a vehicle spawner or an M-COM.
#[must_use]
pub fn is_unit_root(doc: &Doc, k: usize) -> bool {
    let Some(w) = doc.writer() else { return false };
    let Some(g) = doc.guid(k) else { return false };
    let Some(e) = w.find(g) else { return false };
    let t = w.type_of(e);
    [
        ".CapturePointEntityData",
        ".SoldierSpawnEntityData",
        ".AlternateSpawnEntityData",
        ".VehicleSpawnEntityData",
    ]
    .iter()
    .any(|s| t.ends_with(s))
        || gameplay::stage_of(w, g).is_some()
}

/// The delete of gameplay objects with their units (round E4: one operation; an M-COM's stage counting one fewer),
/// or why not in words. Objects another one's unit holds are taken with it, not twice.
///
/// # Errors
/// levelwrite refuses one of them (a team's last base, the last flag, a stage's last M-COM).
pub fn delete_op(doc: &Doc, ks: &[usize]) -> Result<Op, String> {
    let w = doc.writer().ok_or("this level has no partition to edit")?;
    let mut edits: Vec<bc2_levelwrite::files::FileEdit> = Vec::new();
    let mut taken: Vec<String> = Vec::new();
    for &k in ks {
        let Some(g) = doc.guid(k) else { continue };
        if taken.iter().any(|t| t.eq_ignore_ascii_case(g)) {
            continue;
        }
        let op = gameplay::delete_unit(w, &doc.schema, g).map_err(|e| e.to_string())?;
        for part in op.parts() {
            let part = match part {
                Op::Delete { guids } => {
                    let left: Vec<String> = guids.iter().filter(|x| !taken.contains(x)).cloned().collect();
                    taken.extend(left.iter().cloned());
                    Op::Delete { guids: left }
                }
                other => other.clone(),
            };
            edits.push(bc2_levelwrite::files::FileEdit::Partition { op: Box::new(part) });
        }
    }
    match edits.len() {
        0 => Err("nothing to delete".into()),
        1 => match edits.pop() {
            Some(bc2_levelwrite::files::FileEdit::Partition { op }) => Ok(*op),
            _ => Err("nothing to delete".into()),
        },
        _ => Ok(Op::Files { edits }),
    }
}

/// The carry of a unit: its root to `to`, what moves with it as far, each keeping its height above the ground.
#[must_use]
pub fn carry_op(doc: &Doc, root: &str, to: crate::math::V3) -> Option<Op> {
    let w = doc.writer()?;
    // the terrain under a point (objects left out)
    let ground = |x: f64, z: f64| -> Option<f64> {
        crate::query::surface_below_except(&doc.scene, x, z, &|_| true)
            .filter(|h| h.entity.is_none())
            .map(|h| h.point[1])
    };
    let items = gameplay::carry_unit(w, root, at32(to), &ground);
    (!items.is_empty()).then_some(Op::Move { items })
}

/// The round's rules as the level holds them, the mode they are for and DICE's for it (the panel's rows), when the
/// level has a mode layer.
#[must_use]
pub fn rules(doc: &Doc) -> Option<(Mode, Rules, Rules)> {
    let w = doc.writer()?;
    let mode = bc2_levelwrite::placement::detect_mode(w)?;
    let stock = gameplay::stock(mode, doc.project.target_profile());
    Some((mode, gameplay::rules_of(w, &doc.schema), stock))
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
