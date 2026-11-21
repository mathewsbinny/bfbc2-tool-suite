//! A level partition made a multiplayer level (`EDITOR_ARCHITECTURE.md` 3.12): the campaign's scripting taken out,
//! a game mode's layer grafted from a donor multiplayer level, the layer's bases and flags placed on the map, the
//! level's fields set as a multiplayer level's, and the partition renamed.
//!
//! The lists below come from the data (3.12, "What a campaign level lacks"): DICE's own conversion of a campaign
//! level (`SP_005` -> `MP_SP_005CQ`, Heavy Metal) removed every instance of these types and kept the world (objects,
//! terrain splines, model groups, effects, sound areas, weather states); the multiplayer layer is the set of types
//! every stock Conquest level has and no campaign level wires the same way.
#![allow(
    clippy::implicit_hasher,
    reason = "the guid maps and sets are the standard ones throughout"
)]

use std::collections::{HashMap, HashSet};

use bc2_formats::dbx::edit::{EditDbx, NodeId};
use bc2_formats::terrain::Heightfield;
use serde::{Deserialize, Serialize};

use crate::ops::Rows;
use crate::write::{LevelWriteError, LevelWriter, Result};

/// Campaign scripting with no place in a multiplayer level (type names after the module's dot). The first group
/// is in no stock multiplayer level at all (the 13 campaign levels against 24 multiplayer ones, Onslaught's AI
/// levels left out); the second is in some, but DICE's conversion removed every campaign instance of it (AI orders,
/// cutscenes, mission objectives, the campaign's own squads and static positions).
pub const CAMPAIGN_SCRIPTING: &[&str] = &[
    // in no multiplayer level
    "AimAtOrderEntityData",
    "AmmoCrateEntityData",
    "AnimatedWaypointData",
    "ArtilleryStrikeEntityData",
    "CSITextData",
    "ChangeOrderEntityData",
    "ConsumableTriggerEntityData",
    "ContinueEntityData",
    "CutsceneSoldierEntityData",
    "DebugMenuSubMenuEntityData",
    "DebugTextEntityData",
    "EventIfSwitchEntityData",
    "ExplosionPackEntityData",
    "ExplosiveObjectData",
    "FireAtAreaOrderEntityData",
    "ForbiddenAIAreaData",
    "ForceAreaEntityData",
    "HudCounterEntityData",
    "InterludeOrderEntityData",
    "JoinSquadOrderEntityData",
    "LookAtTriggerEntityData",
    "MissileEntityData",
    "MissionObjectiveHudData",
    "MotionSensorEntityData",
    "PlayVideoEntityData",
    "RandomTriggerEntityData",
    "RandomWalkOrderEntityData",
    "SaveEntityData",
    "ScreenEffectEntityData",
    "ScreenFadeEntityData",
    "ShootAtObjectOrderEntityData",
    "SoldierSpawnGhostEntityData",
    "SpawnPointManagerData",
    "SquadGhostEntityData",
    "StatEventTriggerEntityData",
    "TargetObjectData",
    "TargetObjectDataGroup",
    "TooltipEntityData",
    "UnderFireTriggerEntityData",
    "VehicleStateTriggerEntityData",
    "WaypointAnimationData",
    "WaypointData",
    "WeaponPickupEntityData",
    // in some multiplayer levels, removed from the campaign level by DICE's conversion
    "AnimationEntityData",
    "AreaOrderEntityData",
    "CancelOrderEntityData",
    "CharacterStateTriggerEntityData",
    "CutsceneCameraEntityData",
    "CutsceneEntityCameraData",
    "CutsceneEntityData",
    "CutsceneEntityScreenFadeData",
    "CutsceneObjectEntityData",
    "CutsceneVehicleEntityData",
    "DebugMenuItemEntityData",
    "FollowClosestHumanOrderEntityData",
    "GameInteractionEntityData",
    "MissionObjectiveEntityData",
    "SearchAndDestroyOrderEntityData",
    "SquadEntityData",
    "StaticPositionEntityData",
];

/// The campaign's own instances of what the mode's layer brings (its player, mission, menus, spawns, combat areas,
/// map markers and voice-over), and its vehicle set pieces: replaced, not kept beside the layer's.
pub const REPLACED_BY_LAYER: &[&str] = &[
    "AlternateSpawnEntityData",
    "CombatAreaTriggerEntityData",
    "HumanPlayerEntityData",
    "InputRestrictionEntityData",
    "MapMarkerEntityData",
    "MenuEntityData",
    "MissionEntityData",
    "SoldierSpawnEntityData",
    "VehicleSpawnEntityData",
    "VoiceOverEntityData",
    "VoiceOverLineAsset",
];

/// The campaign's vehicle set pieces (its vehicle spawners): kept or removed by the user's choice (round E3b; DICE's
/// conversion removed them, so removed by default).
pub const CAMPAIGN_VEHICLES: &str = "VehicleSpawnEntityData";

/// A group of the strip checklist: its id, its title, its types, and whether the user may keep them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripGroup {
    /// Its id (kept in no file: the choice keeps type names).
    pub id: &'static str,
    /// Its title in the checklist.
    pub title: &'static str,
    /// What it is, in words.
    pub note: &'static str,
    /// Its types (type names after the module's dot).
    pub types: &'static [&'static str],
    /// The user may keep them (false: the mode's layer replaces them, always removed).
    pub choosable: bool,
}

/// The strip checklist (round E3b): [`CAMPAIGN_SCRIPTING`] and [`REPLACED_BY_LAYER`] grouped by what they do. Every
/// type is in exactly one group (`groups_cover_the_lists`).
pub const STRIP_GROUPS: &[StripGroup] = &[
    StripGroup {
        id: "ai-orders",
        title: "AI orders and AI areas",
        note: "what the campaign's soldiers are told to do, and where they may not go",
        types: &[
            "AimAtOrderEntityData",
            "AreaOrderEntityData",
            "CancelOrderEntityData",
            "ChangeOrderEntityData",
            "FireAtAreaOrderEntityData",
            "FollowClosestHumanOrderEntityData",
            "ForbiddenAIAreaData",
            "InterludeOrderEntityData",
            "JoinSquadOrderEntityData",
            "RandomWalkOrderEntityData",
            "SearchAndDestroyOrderEntityData",
            "ShootAtObjectOrderEntityData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "ai-squads",
        title: "Campaign squads and AI spawning",
        note: "the campaign's AI soldiers, their squads, spawn manager and static positions",
        types: &[
            "SoldierSpawnGhostEntityData",
            "SpawnPointManagerData",
            "SquadEntityData",
            "SquadGhostEntityData",
            "StaticPositionEntityData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "waypoints",
        title: "Waypoints and targets",
        note: "AI routes, animated waypoints and the targets orders name",
        types: &[
            "AnimatedWaypointData",
            "TargetObjectData",
            "TargetObjectDataGroup",
            "WaypointAnimationData",
            "WaypointData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "cutscenes",
        title: "Cutscenes and videos",
        note: "scripted scenes, their cameras, actors and vehicles, and full-screen videos",
        types: &[
            "AnimationEntityData",
            "CutsceneCameraEntityData",
            "CutsceneEntityCameraData",
            "CutsceneEntityData",
            "CutsceneEntityScreenFadeData",
            "CutsceneObjectEntityData",
            "CutsceneSoldierEntityData",
            "CutsceneVehicleEntityData",
            "PlayVideoEntityData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "objectives",
        title: "Objectives, checkpoints and saves",
        note: "mission objectives and their HUD, save and continue points, counters and stat events",
        types: &[
            "ContinueEntityData",
            "HudCounterEntityData",
            "MissionObjectiveEntityData",
            "MissionObjectiveHudData",
            "SaveEntityData",
            "StatEventTriggerEntityData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "triggers",
        title: "Scripted triggers",
        note: "triggers on looking, being shot at, vehicle and character states, motion, interactions and forces",
        types: &[
            "CharacterStateTriggerEntityData",
            "ConsumableTriggerEntityData",
            "EventIfSwitchEntityData",
            "ForceAreaEntityData",
            "GameInteractionEntityData",
            "LookAtTriggerEntityData",
            "MotionSensorEntityData",
            "RandomTriggerEntityData",
            "UnderFireTriggerEntityData",
            "VehicleStateTriggerEntityData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "screen",
        title: "Tooltips, debug menus and screen effects",
        note: "on-screen hints and texts, the debug menus, screen fades and effects",
        types: &[
            "CSITextData",
            "DebugMenuItemEntityData",
            "DebugMenuSubMenuEntityData",
            "DebugTextEntityData",
            "ScreenEffectEntityData",
            "ScreenFadeEntityData",
            "TooltipEntityData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "set-pieces",
        title: "Scripted demolitions, strikes and pickups",
        note: "explosion packs, explosive objects, artillery strikes, missiles, weapon pickups and ammo crates",
        types: &[
            "AmmoCrateEntityData",
            "ArtilleryStrikeEntityData",
            "ExplosionPackEntityData",
            "ExplosiveObjectData",
            "MissileEntityData",
            "WeaponPickupEntityData",
        ],
        choosable: true,
    },
    StripGroup {
        id: "vehicles",
        title: "Campaign vehicles",
        note: "the campaign's vehicle spawners (the game mode's own vehicles come in E4)",
        types: &[CAMPAIGN_VEHICLES],
        choosable: true,
    },
    StripGroup {
        id: "replaced",
        title: "Replaced by the game mode's layer",
        note: "the campaign's player, mission, menus, spawns, combat areas, map markers and voice-over: the mode's \
               layer brings its own, so these always go",
        types: &[
            "AlternateSpawnEntityData",
            "CombatAreaTriggerEntityData",
            "HumanPlayerEntityData",
            "InputRestrictionEntityData",
            "MapMarkerEntityData",
            "MenuEntityData",
            "MissionEntityData",
            "SoldierSpawnEntityData",
            "VoiceOverEntityData",
            "VoiceOverLineAsset",
        ],
        choosable: false,
    },
];

/// Which campaign content an export keeps (round E3b): the types (of [`STRIP_GROUPS`]' choosable groups) kept in the
/// new level. Empty is DICE's own choice for `SP_005` -> `MP_SP_005CQ`: every campaign type removed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Strip {
    /// The types kept (type names after the module's dot).
    #[serde(default, skip_serializing_if = "std::collections::BTreeSet::is_empty")]
    pub keep: std::collections::BTreeSet<String>,
}

impl Strip {
    /// Whether an instance of type `t` (after the module's dot) is removed.
    #[must_use]
    pub fn removes(&self, t: &str) -> bool {
        let campaign = CAMPAIGN_SCRIPTING.contains(&t) || REPLACED_BY_LAYER.contains(&t);
        campaign && !(self.keep.contains(t) && choosable(t))
    }

    /// The choice with the campaign vehicles kept or removed.
    #[must_use]
    pub fn with_vehicles(mut self, keep: bool) -> Self {
        if keep {
            self.keep.insert(CAMPAIGN_VEHICLES.into());
        } else {
            self.keep.remove(CAMPAIGN_VEHICLES);
        }
        self
    }

    /// The choice in words (the History panel).
    #[must_use]
    pub fn describe(&self) -> String {
        let kept: Vec<&str> = self.keep.iter().map(String::as_str).filter(|t| choosable(t)).collect();
        if kept.is_empty() {
            return "all removed (DICE's choice)".into();
        }
        let vehicles = kept.contains(&CAMPAIGN_VEHICLES);
        let others = kept.len() - usize::from(vehicles);
        match (vehicles, others) {
            (true, 0) => "campaign vehicles kept".into(),
            (true, n) => format!("campaign vehicles and {n} other kinds kept"),
            (false, n) => format!("{n} kinds kept"),
        }
    }
}

/// Whether the user may keep instances of type `t` (a choosable group's type).
#[must_use]
pub fn choosable(t: &str) -> bool {
    STRIP_GROUPS.iter().any(|g| g.choosable && g.types.contains(&t))
}

/// The mode layer's logic: every instance of these types in the donor is taken (with what it refers to and what
/// its events reach, within [`LAYER_TYPES`]).
pub const LAYER_SEEDS: &[&str] = &[
    "AutoTeamEntityData",
    "HumanPlayerEntityData",
    "InputRestrictionEntityData",
    "MenuEntityData",
    "MessageEntityData",
    "MissionEntityData",
    "PreRoundEntityData",
    "SpectateEntityData",
    "SquadData",
    "TeamEntityData",
    "TeamFilterEntityData",
    "TicketCounterEntityData",
    "VoiceOverEntityData",
];

/// What a graft may take from the donor: the seeds, the chosen capture points and what belongs to them (spawns,
/// alternative spawn points, map markers, area geometry), the event glue between them and the voice-over lines.
/// A reference or event to anything else (a donor vehicle spawn, an object of the donor's map) is not followed and
/// is cut from the copy (listed in the report).
pub const LAYER_TYPES: &[&str] = &[
    "AabbGeometryEntityData",
    "AlternateSpawnEntityData",
    "AreaGeometryEntityData",
    "CapturePointEntityData",
    "DelayTriggerEntityData",
    "EventGateEntityData",
    "EventSplitterEntityData",
    "MapMarkerEntityData",
    "MultipleTriggerEntityData",
    "SoldierSpawnEntityData",
    "SphereGeometryEntityData",
    "VoiceOverLineAsset",
];

/// A base or flag of the layer placed on the map: the donor's capture point of this name, moved (with its spawns,
/// alternative spawn points, map marker and area) so it stands at `at` (x, z; the height follows the ground).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Anchor {
    /// The donor capture point's `Name` (`US Base`, `A: Crash Site`).
    pub from: String,
    /// Where it goes (x, y, z; y is used only where no terrain is given).
    pub at: [f32; 3],
}

/// The game mode's layer: which donor, which of its capture points, where they go.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerSpec {
    /// The donor level (a stock level of the mode: `mp_009cq` for Conquest).
    pub donor: String,
    /// The two teams' bases (headquarters: spawns, not capturable).
    pub bases: Vec<Anchor>,
    /// The flags.
    pub flags: Vec<Anchor>,
    /// The base level's combat area kept as the map's (its guid); the others go.
    pub combat_area: String,
}

/// The two height fields placement reads: the donor's (where the layer stood) and the base's (where it goes).
#[derive(Debug)]
pub struct Ground<'a> {
    /// The donor level's main height field.
    pub donor: &'a Heightfield,
    /// The base level's main height field.
    pub base: &'a Heightfield,
}

/// What [`convert`] did, for the export's report.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Converted {
    /// Campaign instances removed, by type.
    pub removed: Vec<(String, usize)>,
    /// Event connections removed with them.
    pub connections_removed: usize,
    /// References to removed instances cut (array items removed, fields made null).
    pub references_cut: usize,
    /// Donor instances grafted, by type.
    pub grafted: Vec<(String, usize)>,
    /// Donor event connections grafted.
    pub connections_grafted: usize,
    /// References and events of the grafted instances not followed (type, field, target): cut.
    pub not_followed: Vec<String>,
    /// Instances moved to each anchor: (capture point name, instances, metres moved).
    pub placed: Vec<(String, usize, f32)>,
    /// Level fields set.
    pub fields: Vec<String>,
}

pub(crate) fn short(t: &str) -> &str {
    t.rsplit('.').next().unwrap_or(t)
}

pub(crate) fn guid_of(d: &EditDbx, e: NodeId) -> String {
    String::from_utf8_lossy(d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase()
}

pub(crate) fn type_of(d: &EditDbx, e: NodeId) -> String {
    String::from_utf8_lossy(d.attr(e, b"type").unwrap_or_default()).into_owned()
}

/// A local reference (`<guid>`, no partition path), upper case.
pub(crate) fn local_ref(r: &[u8]) -> Option<String> {
    (r.len() == 36 && !r.contains(&b'/')).then(|| String::from_utf8_lossy(r).to_ascii_uppercase())
}

fn refused(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

/// Removes the campaign's scripting ([`CAMPAIGN_SCRIPTING`]) and what the layer replaces ([`REPLACED_BY_LAYER`]),
/// keeping `keep` (the combat area the map keeps) and the types the user's `choice` keeps.
///
/// # Errors
/// The writer refuses (it does not on a level partition).
pub fn strip(w: &mut LevelWriter, keep: &HashSet<String>, choice: &Strip, out: &mut Converted) -> Result<()> {
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut gone = HashSet::new();
    for e in w.d.instances() {
        let t = type_of(&w.d, e);
        let s = short(&t);
        let g = guid_of(&w.d, e);
        if choice.removes(s) && !keep.contains(&g) {
            *by_type.entry(s.to_owned()).or_default() += 1;
            gone.insert(g);
        }
    }
    let d = w.delete_many(&gone)?;
    out.connections_removed += d.connections;
    out.references_cut += d.references;
    let mut v: Vec<(String, usize)> = by_type.into_iter().collect();
    v.sort();
    out.removed = v;
    Ok(())
}

/// The donor's instances a graft takes: every seed, the chosen capture points, and (within [`LAYER_TYPES`]) what
/// they refer to and what their events reach.
fn layer_set(donor: &LevelWriter, capture_points: &HashSet<String>) -> HashSet<String> {
    let mut by_guid: HashMap<String, (NodeId, String)> = HashMap::new();
    for e in donor.d.instances() {
        by_guid.insert(guid_of(&donor.d, e), (e, short(&type_of(&donor.d, e)).to_owned()));
    }
    let allowed = |g: &str| -> bool {
        by_guid.get(g).is_some_and(|(_, t)| {
            if t == "CapturePointEntityData" {
                capture_points.contains(g)
            } else {
                LAYER_SEEDS.contains(&t.as_str()) || LAYER_TYPES.contains(&t.as_str())
            }
        })
    };
    // forward event edges: source -> targets
    let mut events: HashMap<String, Vec<String>> = HashMap::new();
    for a in donor.connection_arrays() {
        for &c in donor.d.children(a).unwrap_or_default() {
            let end = |f: &[u8]| {
                donor
                    .d
                    .child(c, f)
                    .and_then(|x| donor.d.attr(x, b"ref"))
                    .and_then(local_ref)
            };
            if let (Some(s), Some(t)) = (end(b"Source"), end(b"Target")) {
                events.entry(s).or_default().push(t);
            }
        }
    }
    let mut set: HashSet<String> = by_guid
        .iter()
        .filter(|(g, (_, t))| LAYER_SEEDS.contains(&t.as_str()) || capture_points.contains(*g))
        .map(|(g, _)| g.clone())
        .collect();
    let mut work: Vec<String> = set.iter().cloned().collect();
    while let Some(g) = work.pop() {
        let Some(&(e, _)) = by_guid.get(&g) else { continue };
        let mut next: Vec<String> = donor
            .d
            .walk(e)
            .into_iter()
            .filter_map(|x| donor.d.attr(x, b"ref").and_then(local_ref))
            .collect();
        next.extend(events.get(&g).cloned().unwrap_or_default());
        for n in next {
            if allowed(&n) && set.insert(n.clone()) {
                work.push(n);
            }
        }
    }
    set
}

/// A capture point of a level's mode layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CapturePoint {
    /// Its `Name` (`US Base`, `A: Crash Site`): what an [`Anchor`] names.
    pub name: String,
    /// A team's base (`UnableToChangeTeam`: its spawns, never captured).
    pub base: bool,
    /// Its `InitialOwnerTeam` (`Team1`, `Team2`, `TeamNeutral`).
    pub team: String,
    /// Where it stands.
    pub at: [f32; 3],
}

/// The capture points of a level partition, in the partition's order.
#[must_use]
pub fn capture_points(w: &LevelWriter) -> Vec<CapturePoint> {
    w.d.of_type(b".CapturePointEntityData")
        .into_iter()
        .filter_map(|e| {
            let text = |f: &[u8]| {
                w.d.child(e, f)
                    .and_then(|x| w.d.text(x))
                    .map(|t| String::from_utf8_lossy(t).into_owned())
            };
            Some(CapturePoint {
                name: text(b"Name")?,
                // a bool: a two-byte value, the second byte 0 or 1
                base: w
                    .d
                    .child(e, b"UnableToChangeTeam")
                    .is_some_and(|x| w.d.raw(x).get(1).is_some_and(|&b| b != 0)),
                team: text(b"InitialOwnerTeam").unwrap_or_default(),
                at: w.transform(&guid_of(&w.d, e)).map_or([0.0; 3], |r| translation(&r)),
            })
        })
        .collect()
}

/// The partitions (lower case) a level's whole mode layer (every capture point) names outside the level partition.
#[must_use]
pub fn layer_partitions(w: &LevelWriter) -> std::collections::BTreeSet<String> {
    let cps: HashSet<String> =
        w.d.of_type(b".CapturePointEntityData")
            .into_iter()
            .map(|e| guid_of(&w.d, e))
            .collect();
    let set = layer_set(w, &cps);
    let mut out = std::collections::BTreeSet::new();
    for e in w.d.instances() {
        if !set.contains(&guid_of(&w.d, e)) {
            continue;
        }
        for x in w.d.walk(e) {
            if let Some(r) = w.d.attr(x, b"ref")
                && let Some((p, _)) = String::from_utf8_lossy(r).rsplit_once('/')
            {
                out.insert(p.to_ascii_lowercase());
            }
        }
    }
    out
}

/// Re-points the local references under `n` (a copy from the donor) by `map`; a local reference to anything not
/// grafted is cut (an array item removed, a field made null) and named in `cut`.
fn remap(d: &mut EditDbx, n: NodeId, map: &HashMap<String, String>, cut: &mut Vec<String>, owner: &str) -> Result<()> {
    let kids = d.children(n).map(<[NodeId]>::to_vec);
    if let Some(kids) = kids {
        let mut keep = Vec::with_capacity(kids.len());
        for k in kids {
            if let Some(r) = d.attr(k, b"ref").and_then(local_ref) {
                if let Some(new) = map.get(&r) {
                    d.set_ref(k, new.to_ascii_lowercase().as_bytes())?;
                } else {
                    let field = String::from_utf8_lossy(d.attr(k, b"name").unwrap_or(b"item")).into_owned();
                    cut.push(format!("{owner}.{field} -> {r}"));
                    if d.tag(k) == b"item" {
                        continue;
                    }
                    d.set_ref(k, b"null")?;
                }
            }
            remap(d, k, map, cut, owner)?;
            keep.push(k);
        }
        *d.children_mut(n)? = keep;
    }
    Ok(())
}

/// The guid a grafted donor instance gets: UUID v5 over the export, the donor and its guid (a rebuild gives the same).
#[must_use]
pub fn graft_guid(seed: &str, donor: &str, guid: &str) -> String {
    bc2_formats::guid::new_guid(crate::NAMESPACE, &[seed, "graft", donor, &guid.to_ascii_uppercase()])
}

fn named_capture_points(donor: &LevelWriter) -> HashMap<String, String> {
    donor
        .d
        .of_type(b".CapturePointEntityData")
        .into_iter()
        .filter_map(|e| {
            let n = donor.d.child(e, b"Name").and_then(|x| donor.d.text(x))?;
            Some((String::from_utf8_lossy(n).into_owned(), guid_of(&donor.d, e)))
        })
        .collect()
}

/// Grafts the donor's mode layer into `w` (the instances of [`layer_set`], their event connections, their
/// `LevelData.Objects` items, the donor's voice-over arrays); returns donor guid -> new guid.
///
/// # Errors
/// A capture point the spec names is not in the donor, or the writer refuses.
pub fn graft(
    w: &mut LevelWriter,
    donor: &LevelWriter,
    spec: &LayerSpec,
    seed: &str,
    out: &mut Converted,
) -> Result<HashMap<String, String>> {
    let names = named_capture_points(donor);
    let mut cps = HashSet::new();
    for a in spec.bases.iter().chain(&spec.flags) {
        let g = names.get(&a.from).ok_or_else(|| {
            let mut have: Vec<&String> = names.keys().collect();
            have.sort();
            refused(format!(
                "the donor {} has no capture point {:?} (it has {have:?})",
                spec.donor, a.from
            ))
        })?;
        cps.insert(g.clone());
    }
    let set = layer_set(donor, &cps);
    let map: HashMap<String, String> = set
        .iter()
        .map(|g| (g.clone(), graft_guid(seed, &spec.donor, g)))
        .collect();
    graft_map(w, donor, &map, out)?;
    Ok(map)
}

/// Grafts the donor's instances named by `map` (donor guid -> new guid, upper case) into `w`: each copied with its
/// new guid and its local references re-pointed (a reference to an instance not in `map` cut and listed), the
/// donor's event connections between them, their `LevelData.Objects` items, and the donor's voice-over arrays in
/// place of the level's (round E3d: [`crate::modelayer`]'s mode layers use it too).
///
/// # Errors
/// A new guid is already in the level, or the writer refuses.
pub fn graft_map(
    w: &mut LevelWriter,
    donor: &LevelWriter,
    map: &HashMap<String, String>,
    out: &mut Converted,
) -> Result<()> {
    let part = w.d.partition().ok_or_else(|| refused("no partition element"))?;
    let mut by_type: HashMap<String, usize> = HashMap::new();
    for e in donor.d.instances() {
        let g = guid_of(&donor.d, e);
        let Some(new) = map.get(&g) else { continue };
        if w.find(new).is_some() {
            return Err(LevelWriteError::Exists(new.clone()));
        }
        let t = type_of(&donor.d, e);
        *by_type.entry(short(&t).to_owned()).or_default() += 1;
        let c = w.d.copy_from(&donor.d, e);
        w.d.set_attr(c, b"guid", new.as_bytes())?;
        remap(&mut w.d, c, map, &mut out.not_followed, short(&t))?;
        w.d.children_mut(part)?.push(c);
    }
    let level = w.level_data().ok_or_else(|| refused("no LevelData"))?;
    let dl = donor
        .level_data()
        .ok_or_else(|| refused("the donor has no LevelData"))?;
    // the donor's event connections between grafted instances
    let target =
        w.d.child(level, b"EventConnections")
            .ok_or_else(|| refused("no LevelData.EventConnections"))?;
    if let Some(src) = donor.d.child(dl, b"EventConnections") {
        for &c in donor.d.children(src).unwrap_or_default() {
            let end = |f: &[u8]| {
                donor
                    .d
                    .child(c, f)
                    .and_then(|x| donor.d.attr(x, b"ref"))
                    .and_then(local_ref)
            };
            let (Some(s), Some(t)) = (end(b"Source"), end(b"Target")) else {
                continue;
            };
            if !(map.contains_key(&s) && map.contains_key(&t)) {
                continue;
            }
            let copy = w.d.copy_from(&donor.d, c);
            let mut ignore = Vec::new();
            remap(&mut w.d, copy, map, &mut ignore, "EventConnections")?;
            w.d.children_mut(target)?.push(copy);
            out.connections_grafted += 1;
        }
    }
    // LevelData.Objects items of the grafted instances
    let objs =
        w.d.child(level, b"Objects")
            .ok_or_else(|| refused("no LevelData.Objects"))?;
    if let Some(src) = donor.d.child(dl, b"Objects") {
        for &it in donor.d.children(src).unwrap_or_default() {
            if let Some(new) = donor.d.attr(it, b"ref").and_then(local_ref).and_then(|r| map.get(&r)) {
                let copy = w.d.copy_from(&donor.d, it);
                w.d.set_ref(copy, new.to_ascii_lowercase().as_bytes())?;
                w.d.children_mut(objs)?.push(copy);
            }
        }
    }
    // the mode's voice-over (the announcer's lines): the donor's arrays in place of the campaign's
    for f in [&b"VoiceOvers"[..], b"VoiceOverLines"] {
        replace_field(w, donor, level, dl, f, map, out)?;
        out.fields.push(String::from_utf8_lossy(f).into_owned());
    }
    let mut v: Vec<(String, usize)> = by_type.into_iter().collect();
    v.sort();
    out.grafted = v;
    Ok(())
}

/// The field `name` of `LevelData` replaced by the donor's (references re-pointed by `map`, the rest kept as the
/// donor has them when they name another partition, cut when local and not grafted).
fn replace_field(
    w: &mut LevelWriter,
    donor: &LevelWriter,
    level: NodeId,
    dl: NodeId,
    name: &[u8],
    map: &HashMap<String, String>,
    out: &mut Converted,
) -> Result<NodeId> {
    let src = donor.d.child(dl, name).ok_or_else(|| {
        refused(format!(
            "the donor's LevelData has no {}",
            String::from_utf8_lossy(name)
        ))
    })?;
    let copy = w.d.copy_from(&donor.d, src);
    remap(&mut w.d, copy, map, &mut out.not_followed, "LevelData")?;
    let pos =
        w.d.children(level)
            .unwrap_or_default()
            .iter()
            .position(|&k| w.d.attr(k, b"name") == Some(name))
            .ok_or_else(|| refused(format!("LevelData has no {}", String::from_utf8_lossy(name))))?;
    if let Some(slot) = w.d.children_mut(level)?.get_mut(pos) {
        *slot = copy;
    }
    Ok(copy)
}

/// A `LevelData` field replaced by the donor's (a content profile's field: round E3b).
///
/// # Errors
/// Either level lacks the field.
pub fn take_field(
    w: &mut LevelWriter,
    donor: &LevelWriter,
    name: &str,
    map: &HashMap<String, String>,
    out: &mut Converted,
) -> Result<()> {
    let ld = w.level_data().ok_or_else(|| refused("no LevelData"))?;
    let dl = donor
        .level_data()
        .ok_or_else(|| refused("the donor has no LevelData"))?;
    replace_field(w, donor, ld, dl, name.as_bytes(), map, out)?;
    out.fields.push(format!("{name} (the profile's: the donor's)"));
    Ok(())
}

fn translation(rows: &Rows) -> [f32; 3] {
    rows[3]
}

/// Moves each anchor's capture point and what belongs to it (the local references reached from it through spawns,
/// alternative spawn points, map markers and area geometry) so the capture point stands at the anchor; heights
/// follow the ground: each instance keeps its height above the donor's ground, over the base's ground.
///
/// # Errors
/// An anchor's capture point was not grafted.
pub fn place(
    w: &mut LevelWriter,
    donor: &LevelWriter,
    spec: &LayerSpec,
    map: &HashMap<String, String>,
    ground: Option<&Ground<'_>>,
    out: &mut Converted,
) -> Result<()> {
    let names = named_capture_points(donor);
    let follow = [
        "SoldierSpawnEntityData",
        "AlternateSpawnEntityData",
        "MapMarkerEntityData",
        "AreaGeometryEntityData",
        "AabbGeometryEntityData",
        "SphereGeometryEntityData",
    ];
    let mut done = HashSet::new();
    for a in spec.bases.iter().chain(&spec.flags) {
        let dg = names
            .get(&a.from)
            .ok_or_else(|| refused(format!("no capture point {:?}", a.from)))?;
        let cp = map
            .get(dg)
            .ok_or_else(|| refused(format!("{:?} was not grafted", a.from)))?;
        let origin = w
            .transform(cp)
            .map(|r| translation(&r))
            .ok_or_else(|| refused("a capture point without Transform"))?;
        let (dx, dz) = (a.at[0] - origin[0], a.at[2] - origin[2]);
        // the cluster: the capture point and what it reaches through `follow` types
        let mut cluster = vec![cp.clone()];
        let mut seen: HashSet<String> = cluster.iter().cloned().collect();
        let mut i = 0;
        while let Some(g) = cluster.get(i).cloned() {
            i += 1;
            let Some(e) = w.find(&g) else { continue };
            for x in w.d.walk(e) {
                let Some(r) = w.d.attr(x, b"ref").and_then(local_ref) else {
                    continue;
                };
                let Some(t) = w.find(&r).map(|n| type_of(&w.d, n)) else {
                    continue;
                };
                if follow.contains(&short(&t)) && !done.contains(&r) && seen.insert(r.clone()) {
                    cluster.push(r);
                }
            }
        }
        let mut moved = 0;
        for g in &cluster {
            let Some(mut rows) = w.transform(g) else { continue };
            let [x, y, z] = rows[3];
            let (nx, nz) = (x + dx, z + dz);
            let ny = match ground {
                Some(gr) => {
                    let above = f64::from(y) - gr.donor.height(f64::from(x), f64::from(z));
                    (gr.base.height(f64::from(nx), f64::from(nz)) + above) as f32
                }
                None => y + (a.at[1] - origin[1]),
            };
            rows[3] = [nx, ny, nz];
            w.set_transform(g, &rows)?;
            done.insert(g.clone());
            moved += 1;
        }
        out.placed.push((a.from.clone(), moved, dx.hypot(dz)));
    }
    Ok(())
}

fn set_bool_field(w: &mut LevelWriter, inst: NodeId, name: &[u8], v: bool) -> Result<()> {
    let f =
        w.d.child(inst, name)
            .ok_or_else(|| refused(format!("no field {}", String::from_utf8_lossy(name))))?;
    w.d.set_bool(f, u8::from(v))?;
    Ok(())
}

fn set_text_field(w: &mut LevelWriter, inst: NodeId, name: &[u8], v: &str) -> Result<()> {
    let f =
        w.d.child(inst, name)
            .ok_or_else(|| refused(format!("no field {}", String::from_utf8_lossy(name))))?;
    w.d.set_text(f, v.as_bytes())?;
    Ok(())
}

fn set_ref_field(w: &mut LevelWriter, inst: NodeId, name: &[u8], v: &[u8]) -> Result<()> {
    let f =
        w.d.child(inst, name)
            .ok_or_else(|| refused(format!("no field {}", String::from_utf8_lossy(name))))?;
    w.d.set_ref(f, v)?;
    Ok(())
}

/// The level's description fields of a converted level: the base's `LevelDescription` keeps its own map pictures,
/// music and rich presence; the mode's fields (game mode, factions, start, kits, memory) are the donor's.
pub struct DescriptionKeep {
    /// The loc key of the map's name (`SP_01_NAME`).
    pub name: String,
    /// The loc key of its description.
    pub description: String,
    /// The base's `Music`, `MinimapTexture`, `LoadingMovie` and `RichPresence` references.
    pub refs: Vec<(Vec<u8>, Vec<u8>)>,
}

impl DescriptionKeep {
    /// The base level's own fields, read before the conversion.
    #[must_use]
    pub fn of(w: &LevelWriter) -> Option<Self> {
        let ld = w.level_data()?;
        let desc = w.d.child(ld, b"LevelDescription")?;
        let text = |f: &[u8]| {
            w.d.child(desc, f)
                .and_then(|x| w.d.text(x))
                .map(|t| String::from_utf8_lossy(t).into_owned())
        };
        let refs = [&b"Music"[..], b"MinimapTexture", b"LoadingMovie", b"RichPresence"]
            .iter()
            .filter_map(|f| {
                Some((
                    f.to_vec(),
                    w.d.child(desc, f).and_then(|x| w.d.attr(x, b"ref"))?.to_vec(),
                ))
            })
            .collect();
        Some(Self {
            name: text(b"Name")?,
            description: text(b"Description")?,
            refs,
        })
    }

    /// Applies these fields to a `LevelDescription` complex (in a level's `LevelData` or a description asset).
    ///
    /// # Errors
    /// A field missing.
    pub fn apply(&self, w: &mut LevelWriter, desc: NodeId) -> Result<()> {
        set_text_field(w, desc, b"Name", &self.name)?;
        set_text_field(w, desc, b"Description", &self.description)?;
        for (f, r) in &self.refs {
            set_ref_field(w, desc, f, r)?;
        }
        Ok(())
    }
}

/// The level's own fields made a multiplayer level's: no AI, no fade-in, no subtitles or waypoint animations, no
/// pathfinding types; the donor's field of view, friendly damage and `LevelDescription` (then `keep` applied to it).
///
/// # Errors
/// A field missing.
pub fn level_fields(
    w: &mut LevelWriter,
    donor: &LevelWriter,
    keep: &DescriptionKeep,
    map: &HashMap<String, String>,
    out: &mut Converted,
) -> Result<()> {
    let ld = w.level_data().ok_or_else(|| refused("no LevelData"))?;
    let dl = donor
        .level_data()
        .ok_or_else(|| refused("the donor has no LevelData"))?;
    for f in [
        &b"HasAI"[..],
        b"HasFlyingAI",
        b"StartLevelFadedToBlack",
        b"StartLevelWithSoundFaded",
    ] {
        set_bool_field(w, ld, f, false)?;
        out.fields.push(format!("{} = false", String::from_utf8_lossy(f)));
    }
    for f in [&b"SubtitleAsset"[..], b"WaypointAnimations", b"SpawnPointManager"] {
        set_ref_field(w, ld, f, b"null")?;
        out.fields.push(format!("{} = null", String::from_utf8_lossy(f)));
    }
    for f in [
        &b"PathfindingTypes"[..],
        b"InfantryFOVMultiplier",
        b"DefaultFOV",
        b"FriendlyDamageModifier",
    ] {
        replace_field(w, donor, ld, dl, f, map, out)?;
        out.fields.push(format!("{} (the donor's)", String::from_utf8_lossy(f)));
    }
    let desc = replace_field(w, donor, ld, dl, b"LevelDescription", map, out)?;
    keep.apply(w, desc)?;
    out.fields
        .push("LevelDescription (the donor's: game mode, factions, kits; the base's name, map, music)".into());
    Ok(())
}

/// The level's names after renaming: `Levels/SP_01` -> `Levels/<NEW>`, and the partitions copied under the new name
/// (`renamed`: lower-case entry names without extension, `levels/sp_01/staticculltree_client`) re-pointed. The case
/// of the level's name follows the string's (DICE writes `Levels/SP_01/...` in names, `levels/sp_01/...` in refs).
#[must_use]
pub fn renamed_string(s: &[u8], old: &str, new: &str, renamed: &HashSet<String>) -> Option<Vec<u8>> {
    let low = String::from_utf8_lossy(s).to_ascii_lowercase();
    let prefix = format!("levels/{}", old.to_ascii_lowercase());
    if !low.starts_with(&prefix) {
        return None;
    }
    let rest = low.get(prefix.len()..).unwrap_or_default();
    // a reference's partition: the text before "/<guid>"
    let key = match rest.rfind('/') {
        Some(i) if rest.len() - i == 37 => low.get(..prefix.len() + i).unwrap_or_default(),
        _ => low.as_str(),
    };
    if !(key == prefix || renamed.contains(key)) {
        return None;
    }
    let orig = s.get(7..7 + old.len()).unwrap_or_default();
    let name = if orig.iter().any(u8::is_ascii_uppercase) {
        new.to_ascii_uppercase()
    } else {
        new.to_ascii_lowercase()
    };
    let mut out = s.get(..7).unwrap_or_default().to_vec();
    out.extend(name.as_bytes());
    out.extend(s.get(7 + old.len()..).unwrap_or_default());
    Some(out)
}

/// Renames every string of a partition naming the level or one of the `renamed` partitions (references and text
/// fields); returns how many.
///
/// # Errors
/// The writer refuses.
pub fn rename_strings(d: &mut EditDbx, old: &str, new: &str, renamed: &HashSet<String>) -> Result<usize> {
    let mut n = 0;
    let roots = d.roots().to_vec();
    for r in roots {
        for e in d.walk(r) {
            let attrs: Vec<(Vec<u8>, Vec<u8>)> = d.attributes(e).map(|(k, v)| (k.to_vec(), v.to_vec())).collect();
            for (k, v) in attrs {
                if (k == b"ref" || k == b"id")
                    && let Some(x) = renamed_string(&v, old, new, renamed)
                {
                    d.set_attr(e, &k, &x)?;
                    n += 1;
                }
            }
            if d.kind(e) == bc2_formats::dbx::TEXT
                && let Some(x) = d.text(e).and_then(|t| renamed_string(t, old, new, renamed))
            {
                d.set_text(e, &x)?;
                n += 1;
            }
        }
    }
    Ok(n)
}

/// The partition's own guid set (`partguid=`), and its `LevelData`'s `id` attribute when it has one.
///
/// # Errors
/// No partition element.
pub fn set_partition_guid(d: &mut EditDbx, guid: &str) -> Result<()> {
    let p = d.partition().ok_or_else(|| refused("no partition element"))?;
    d.set_attr(p, b"guid", guid.to_ascii_uppercase().as_bytes())?;
    Ok(())
}

/// A partition copied under the new level's name: its guid derived from the new name (as the Onslaught ports'
/// copies), its strings renamed.
#[must_use]
pub fn partition_guid_for(entry: &str) -> String {
    bc2_formats::guid::new_guid(crate::NAMESPACE, &["partition", &entry.to_ascii_lowercase()])
}

/// The id attribute of the level's `LevelData` (`id="SP_01"`).
///
/// # Errors
/// No `LevelData`.
pub fn set_level_id(w: &mut LevelWriter, new: &str) -> Result<()> {
    let ld = w.level_data().ok_or_else(|| refused("no LevelData"))?;
    if w.d.attr(ld, b"id").is_some() {
        w.d.set_attr(ld, b"id", new.to_ascii_uppercase().as_bytes())?;
    }
    Ok(())
}

/// Every instance's guid of a set of types, upper case.
#[must_use]
pub fn guids_of_type(w: &LevelWriter, t: &str) -> Vec<String> {
    w.d.of_type(format!(".{t}").as_bytes())
        .into_iter()
        .map(|e| guid_of(&w.d, e))
        .collect()
}

/// A combat area's `Enabled` set (the kept area is on from the start).
///
/// # Errors
/// No such instance or field.
pub fn enable(w: &mut LevelWriter, guid: &str) -> Result<()> {
    let e = w.find(guid).ok_or_else(|| LevelWriteError::NotFound(guid.to_owned()))?;
    set_bool_field(w, e, b"Enabled", true)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn names_follow_the_level_and_case() {
        let renamed: HashSet<String> = ["levels/sp_01/staticculltree_client".to_owned()].into();
        let r = |s: &str| {
            renamed_string(s.as_bytes(), "SP_01", "Mapedit_X_CQ", &renamed).map(|v| String::from_utf8(v).unwrap())
        };
        assert_eq!(r("Levels/SP_01").as_deref(), Some("Levels/MAPEDIT_X_CQ"));
        assert_eq!(
            r("levels/sp_01/staticculltree_client/3eb17d94-4ffc-11e0-bf3f-cbc4fb3f961e").as_deref(),
            Some("levels/mapedit_x_cq/staticculltree_client/3eb17d94-4ffc-11e0-bf3f-cbc4fb3f961e")
        );
        // not renamed: the material grid, the minimap, another level
        assert_eq!(
            r("levels/sp_01/materials_win32/d1cbe90b-d5b9-4161-b517-8d0259034605"),
            None
        );
        assert_eq!(
            r("levels/sp_01/ui/minimaptexture/33bfcd1e-238e-4a31-a480-e258cf31a6dd"),
            None
        );
        assert_eq!(r("Levels/SP_011"), None);
        assert_eq!(r("levels/mp_005/x"), None);
    }

    #[test]
    fn groups_cover_the_lists() {
        let mut all: Vec<&str> = CAMPAIGN_SCRIPTING.iter().chain(REPLACED_BY_LAYER).copied().collect();
        all.sort_unstable();
        let mut grouped: Vec<&str> = STRIP_GROUPS.iter().flat_map(|g| g.types.iter().copied()).collect();
        grouped.sort_unstable();
        assert_eq!(grouped, all, "every type in exactly one group");
        // the default removes everything; vehicles kept keeps only them; a locked type is never kept
        let d = Strip::default();
        assert!(all.iter().all(|t| d.removes(t)));
        assert!(!d.removes("ReferenceObjectData"));
        let v = d.clone().with_vehicles(true);
        assert!(!v.removes(CAMPAIGN_VEHICLES) && v.removes("CutsceneEntityData"));
        assert_eq!(v.describe(), "campaign vehicles kept");
        assert_eq!(v.with_vehicles(false), d);
        let mut locked = Strip::default();
        locked.keep.insert("HumanPlayerEntityData".into());
        assert!(locked.removes("HumanPlayerEntityData"));
        assert_eq!(d.describe(), "all removed (DICE's choice)");
    }
}
