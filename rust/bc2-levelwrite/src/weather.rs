//! The weather of an exported level (round VW2, `EDITOR_ARCHITECTURE.md` 3.12 "Round VW2"): the project's choice
//! of a weather state ([`crate::Op::SetWeather`]) written into the exported level. Research round WX0 found nothing
//! baked depends on the sun, so a state is data only: no re-bake.
//!
//! **How a level finds its weather** (spec RNDR-WTHR-5, -6): its weather entity (`WeatherSystemEntityData`, an
//! instance of the level partition or the object of a placed blueprint under `weather/`) names a `WeatherSystem`
//! (its `States`) and a `WeatherSystemSettings` (`ActiveState`); the level's first enabled `WeatherStateEntityData`
//! with `StartTransitionOnSpawn` moves to its `State` as the level starts (what players see: in 45 of 70 levels with
//! such a switch it names another state than `ActiveState`). An export sets both to the chosen state (RNDR-WTHR-8).
//!
//! **What the export writes** (every partition new, under `weather/<level>/`, in the level's own bundle: the stock
//! partitions of the base, shared with its other modes, are never changed):
//!
//! - **`VW2a`, one of the level's own states:** the settings copied with `ActiveState` set; the spawn switch's `State`
//!   set; the weather entity re-pointed at the copy (a placed entity blueprint is copied too, and the level's
//!   placement of it re-pointed).
//! - **`VW2b`, another level's state appended:** the system copied with the donor's state (and its component
//!   partitions, renamed under the level's folder: no name of the donor's can collide with the target's) added at
//!   the end of its `States`; the system's 4-byte `WeatherSystem` resource beside the copy (value 72 in all 28 of
//!   DICE's systems, whatever their state count). The target keeps its Setup: its sky panorama and clouds.
//! - **`VW2b`, the donor's whole system swapped in:** the donor's system, every state, its Setup and static data
//!   renamed under the level's folder, its resource beside it, its settings copied as the level's: the donor's sky.
//!   The panorama and cloud textures keep their names (the closure copies them from the donor's bundle).
//! - **`VW2c`, night:** DICE's one night (`r_mp02/ons_mp_002_night`, `MP_002`'s system, in the Onslaught Package) appended
//!   as the template, its values kept, the reflection cube's `ReflectionEnvmapScale` scaled down and the level's own
//!   sky panorama darkened by the state's panoramic layer (its `Trancparency`, DICE's spelling); lamps are placed with the editor's
//!   tool only, never by the export.
//!
//! Night and rain exports are **off by default** and marked "not yet seen in game": the build refuses them unless the
//! choice says `unseen_ok` (in-game tests are paused, 2026-11-20).

use std::collections::{BTreeMap, HashMap, HashSet};

use bc2_formats::dbx::edit::{EditDbx, NodeId};
use serde::{Deserialize, Serialize};

use crate::export::{ExportError, Found, Game, StagedEntry, read_found};
use crate::mpconvert;
use crate::write::LevelWriter;

/// The entity type a level's weather is.
pub const ENTITY_TYPE: &[u8] = b".WeatherSystemEntityData";
/// The state switch's type.
pub const STATE_ENTITY_TYPE: &[u8] = b".WeatherStateEntityData";
/// The `WeatherSystem` resource's type, beside its system partition (`Weather/R_MP005/WeatherSystem_MP005.res`).
pub const SYSTEM_RES_TYPE: &str = "WeatherSystem";
/// The `WeatherSystem` resource's value in all 28 of DICE's systems (4 bytes, little-endian), whatever the system's
/// state count (WX0: 2 to 53 states).
pub const SYSTEM_RES_VALUE: u32 = 72;
/// The night template (`VW2c`, the user's choice 2026-11-21): DICE's one night state, in `MP_002`'s system (the
/// Onslaught Package's `Package\levels\mp_002\level-00.fbrb`). Re-pointed when Onslaught is re-ported.
pub const NIGHT_TEMPLATE: (&str, &str) = ("mp_002", "weather/r_mp02/states/ons_mp_002_night");
/// The reflection cube's scale at night, of the template's (`ReflectionEnvmapScale`): the cube is a capture of the
/// day sky (WX0 B.3), so night shows a tenth of it.
pub const NIGHT_ENVMAP_SCALE: f32 = 0.1;
/// The level's own sky panorama at night: the template's panoramic layer `Trancparency` (DICE's spelling) scaled by
/// this (less of the day panorama over the night sky's colour).
pub const NIGHT_PANORAMA: f32 = 0.25;

fn err(why: impl Into<String>) -> ExportError {
    ExportError(why.into())
}

type Result<T> = std::result::Result<T, ExportError>;

/// Another level's state borrowed (`VW2b`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Borrow {
    /// The donor level (lower case).
    pub donor: String,
    /// The donor state's partition (`weather/r_mp02/states/ons_mp_002_night`).
    pub state: String,
    /// Swap the whole system in (the donor's sky); `false`: append the state (the target keeps its sky).
    pub swap: bool,
}

/// The night look's settings (`VW2c`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Night {
    /// The reflection cube's scale, of the template's.
    pub envmap_scale: f32,
    /// The sky panorama's `Trancparency` scale, of the template's.
    pub panorama: f32,
}

impl Default for Night {
    fn default() -> Self {
        Self {
            envmap_scale: NIGHT_ENVMAP_SCALE,
            panorama: NIGHT_PANORAMA,
        }
    }
}

/// The project's weather (round VW2): the state an export gives the level.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WeatherChoice {
    /// The state: an index into the level's own system's states (`VW2a`); with a borrow, into the donor's system for
    /// a swap (an appended state is the system's last).
    pub state: usize,
    /// The state's name, as picked (`midday`, `ons_mp_002_night`).
    pub name: String,
    /// Another level's state (`VW2b`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub borrow: Option<Borrow>,
    /// The night look (`VW2c`): the template appended, the cube and panorama darkened.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub night: Option<Night>,
    /// The user turned on an export not yet seen in game (night, rain): off by default.
    #[serde(default)]
    pub unseen_ok: bool,
}

impl WeatherChoice {
    /// The choice in words (the History panel, the report).
    #[must_use]
    pub fn describe(&self) -> String {
        let what = match (&self.borrow, &self.night) {
            (_, Some(_)) => format!("night (from {})", NIGHT_TEMPLATE.1),
            (Some(b), None) if b.swap => format!("{}'s whole weather system, state {}", b.donor, self.state),
            (Some(b), None) => format!("{} from {}, appended", b.state, b.donor),
            (None, None) => format!("the level's state {}", self.state),
        };
        format!("Weather: {} ({what})", self.name)
    }

    /// Whether the export makes a look not yet seen in game (night, rain): such an export is off by default.
    #[must_use]
    pub fn unseen(&self) -> bool {
        self.night.is_some() || is_unseen_name(&self.name)
    }
}

/// A state's name that makes a look not yet seen in game (night, rain, storm, snow).
#[must_use]
pub fn is_unseen_name(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    ["night", "rain", "storm", "lightning", "snow", "frost"]
        .iter()
        .any(|w| n.contains(w))
}

/// What the export did with the weather (`stage.json`'s `weather`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct WeatherRecord {
    /// The choice in words.
    pub choice: String,
    /// The weather entity's partition (the level partition, or the copied blueprint).
    pub entity: String,
    /// The system the level names now.
    pub system: String,
    /// The settings the level names now.
    pub settings: String,
    /// The system's states (partition paths).
    pub states: Vec<String>,
    /// `ActiveState` set.
    pub active: usize,
    /// The spawn switch's guid and its `State` set (`None`: the level has none; `ActiveState` alone starts it).
    pub spawn: Option<(String, usize)>,
    /// The partitions written (entry names) and why.
    pub written: Vec<(String, String)>,
    /// The other state entities of the level and the states they name now (round-end and scripted switches).
    pub others: Vec<(String, usize)>,
    /// Not yet seen in game (night, rain): the user turned it on.
    pub unseen: bool,
    /// Each copied partition's source: (new path, source path, the level whose bundles hold it) - the check reads
    /// the source's references (a reference DICE's own partition leaves dangling, the static Sun / Sky data a
    /// Setup names, stays so in the copy).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sources: Vec<(String, String, String)>,
}

impl WeatherRecord {
    /// The source of a copied partition (by its entry name or path, any case): (source path, level).
    #[must_use]
    pub fn source_of(&self, entry: &str) -> Option<(&str, &str)> {
        let e = entry.to_ascii_lowercase();
        let e = e.trim_end_matches(".dbx");
        self.sources
            .iter()
            .find(|(n, _, _)| n == e)
            .map(|(_, s, l)| (s.as_str(), l.as_str()))
    }
}

/// A level's weather as its partitions have it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelWeather {
    /// Where the entity is: `None` the level partition (its instance's guid), `Some` a placed blueprint's path.
    pub blueprint: Option<String>,
    /// The entity instance's guid (in the level partition or the blueprint).
    pub entity: String,
    /// The system partition (path, lower case).
    pub system: String,
    /// The settings partition.
    pub settings: String,
    /// The system's states.
    pub states: Vec<String>,
    /// The settings' `ActiveState`.
    pub active: usize,
    /// The spawn switch: its guid and `State`.
    pub spawn: Option<(String, usize)>,
}

/// `path/guid` -> (path lower case, guid upper case).
fn split_ref(r: &[u8]) -> Option<(String, String)> {
    let s = String::from_utf8_lossy(r);
    let (p, g) = s.rsplit_once('/')?;
    Some((p.to_ascii_lowercase(), g.to_ascii_uppercase()))
}

fn ref_field(d: &EditDbx, e: NodeId, name: &[u8]) -> Option<(String, String)> {
    d.child(e, name).and_then(|x| d.attr(x, b"ref")).and_then(split_ref)
}

/// An integer field (a numbers element of one 4-byte big-endian value, or a text).
fn int_field(d: &EditDbx, e: NodeId, name: &[u8]) -> Option<i64> {
    let c = d.child(e, name)?;
    if let Some(t) = d.text(c) {
        return String::from_utf8_lossy(t).trim().parse().ok();
    }
    let (_, vals) = bc2_formats::dbx::edit::numbers_of(d.raw(c))?;
    let v: [u8; 4] = vals.first()?.get(..4)?.try_into().ok()?;
    Some(i64::from(i32::from_be_bytes(v)))
}

fn set_int_field(d: &mut EditDbx, e: NodeId, name: &[u8], v: usize) -> Result<()> {
    let c = d
        .child(e, name)
        .ok_or_else(|| err(format!("no field {}", String::from_utf8_lossy(name))))?;
    let n = i32::try_from(v).map_err(|_| err("a state index past i32"))?;
    if d.text(c).is_some() {
        d.set_text(c, n.to_string().as_bytes()).map_err(|e| err(e.0))
    } else {
        d.set_numbers(c, &[n.to_be_bytes()]).map_err(|e| err(e.0))
    }
}

fn set_float_field(d: &mut EditDbx, e: NodeId, name: &[u8], v: f32) -> Result<bool> {
    let Some(c) = d.child(e, name) else { return Ok(false) };
    if d.text(c).is_some() {
        d.set_text(c, v.to_string().as_bytes()).map_err(|e| err(e.0))?;
    } else {
        d.set_numbers(c, &[v.to_be_bytes()]).map_err(|e| err(e.0))?;
    }
    Ok(true)
}

fn float_field(d: &EditDbx, e: NodeId, name: &[u8]) -> Option<f32> {
    let c = d.child(e, name)?;
    if let Some(t) = d.text(c) {
        return String::from_utf8_lossy(t).trim().parse().ok();
    }
    let (_, vals) = bc2_formats::dbx::edit::numbers_of(d.raw(c))?;
    let v: [u8; 4] = vals.first()?.get(..4)?.try_into().ok()?;
    Some(f32::from_be_bytes(v))
}

fn guid_of(d: &EditDbx, e: NodeId) -> String {
    String::from_utf8_lossy(d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase()
}

/// The `States` of a system partition (paths, lower case, in order).
#[must_use]
pub fn system_states(d: &EditDbx) -> Vec<String> {
    let Some(&sys) = d.of_type(b".WeatherSystemData").first() else {
        return Vec::new();
    };
    let Some(arr) = d.child(sys, b"States") else {
        return Vec::new();
    };
    d.children(arr)
        .unwrap_or_default()
        .iter()
        .filter_map(|&i| d.attr(i, b"ref").and_then(split_ref).map(|x| x.0))
        .collect()
}

/// The level's first enabled spawn switch (`WeatherStateEntityData` with `StartTransitionOnSpawn`): its node.
fn spawn_switch(w: &LevelWriter) -> Option<NodeId> {
    w.d.of_type(STATE_ENTITY_TYPE).into_iter().find(|&e| {
        crate::modelayer::bool_field(w, e, b"Enabled") == Some(true)
            && crate::modelayer::bool_field(w, e, b"StartTransitionOnSpawn") == Some(true)
    })
}

/// A partition by path through a set of live entries: its bytes.
fn read_partition(live: &HashMap<String, Found>, path: &str) -> Result<Vec<u8>> {
    let f = live
        .get(&format!("{}.dbx", path.to_ascii_lowercase()))
        .ok_or_else(|| err(format!("{path}: in no bundle the level loads")))?;
    Ok(read_found(&[f])?.pop().unwrap_or_default())
}

/// The level's weather from its partition (as the project leaves it) and the bundles it loads (`live`): its entity
/// (the level's own, else a placed blueprint under `weather/`), the system's states, `ActiveState`, the spawn switch.
///
/// # Errors
/// No weather entity resolves, or its system or settings is in no bundle.
#[allow(clippy::implicit_hasher, reason = "the export's live entries")]
pub fn level_weather(w: &LevelWriter, live: &HashMap<String, Found>) -> Result<LevelWeather> {
    let spawn = spawn_switch(w).map(|e| {
        (
            guid_of(&w.d, e),
            int_field(&w.d, e, b"State")
                .and_then(|v| usize::try_from(v).ok())
                .unwrap_or(0),
        )
    });
    let mut found: Option<(Option<String>, String, String, String)> = None;
    for e in w.d.of_type(ENTITY_TYPE) {
        if let (Some(s), Some(t)) = (ref_field(&w.d, e, b"WeatherSystem"), ref_field(&w.d, e, b"Settings")) {
            found = Some((None, guid_of(&w.d, e), s.0, t.0));
            break;
        }
    }
    if found.is_none() {
        let mut seen = HashSet::new();
        for e in w.d.of_type(b".ReferenceObjectData") {
            let Some((bp, _)) = ref_field(&w.d, e, b"ReferencedObject") else {
                continue;
            };
            if !bp.starts_with("weather/") || !seen.insert(bp.clone()) {
                continue;
            }
            let Ok(data) = read_partition(live, &bp) else { continue };
            let Ok(d) = EditDbx::parse(&data) else { continue };
            if let Some(&x) = d.of_type(ENTITY_TYPE).first()
                && let (Some(s), Some(t)) = (ref_field(&d, x, b"WeatherSystem"), ref_field(&d, x, b"Settings"))
            {
                found = Some((Some(bp), guid_of(&d, x), s.0, t.0));
                break;
            }
        }
    }
    let (blueprint, entity, system, settings) =
        found.ok_or_else(|| err("the level has no weather entity (WeatherSystemEntityData) that resolves"))?;
    let sd = EditDbx::parse(&read_partition(live, &system)?).map_err(|e| err(format!("{system}: {}", e.0)))?;
    let td = EditDbx::parse(&read_partition(live, &settings)?).map_err(|e| err(format!("{settings}: {}", e.0)))?;
    let active = td
        .of_type(b".WeatherSystemSettings")
        .first()
        .and_then(|&e| int_field(&td, e, b"ActiveState"))
        .and_then(|v| usize::try_from(v).ok())
        .unwrap_or(0);
    Ok(LevelWeather {
        blueprint,
        entity,
        system,
        settings,
        states: system_states(&sd),
        active,
        spawn,
    })
}

/// The references of a partition (paths of `path/guid` references, lower case).
fn ref_paths(d: &EditDbx) -> Vec<String> {
    let mut out = Vec::new();
    for r in d.roots().to_vec() {
        for e in d.walk(r) {
            if let Some(v) = d.attr(e, b"ref")
                && let Some((p, _)) = split_ref(v)
            {
                out.push(p);
            }
        }
    }
    out
}

/// The weather partitions a partition reaches (itself first; only `weather/` partitions are followed: a state's
/// components, a system's states and Setup, a Setup's static data), in a stable order.
fn weather_closure(live: &HashMap<String, Found>, root: &str) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut todo = vec![root.to_ascii_lowercase()];
    while let Some(p) = todo.pop() {
        if !seen.insert(p.clone()) {
            continue;
        }
        let Ok(data) = read_partition(live, &p) else {
            // DICE's static Sun / Sky data the setups name is in no bundle (WX0 A.6): the game runs without it
            continue;
        };
        if let Ok(d) = EditDbx::parse(&data) {
            let mut next: Vec<String> = ref_paths(&d)
                .into_iter()
                .filter(|r| r.starts_with("weather/"))
                .collect();
            next.sort();
            next.dedup();
            todo.extend(next.into_iter().rev());
        }
        out.push((p, data));
    }
    out
}

/// A set of partitions copied under new names: `rename` maps each old path to its new one; every instance gets a
/// new guid (UUID v5 over `seed`, the old path and guid), references into the set are re-pointed (path and guid),
/// the partition guid derives from the new name, the primary instance's `Name` is the new path.
fn copy_renamed(
    set: &[(String, Vec<u8>)],
    rename: &BTreeMap<String, String>,
    seed: &str,
) -> Result<Vec<(String, EditDbx)>> {
    let mut parsed: Vec<(String, EditDbx)> = Vec::new();
    let mut guid_map: HashMap<(String, String), String> = HashMap::new();
    for (p, data) in set {
        let d = EditDbx::parse(data).map_err(|e| err(format!("{p}: {}", e.0)))?;
        for e in d.instances() {
            let g = guid_of(&d, e);
            let n = bc2_stockmp::guid::new_guid(crate::NAMESPACE, &[seed, p, &g]);
            guid_map.insert((p.clone(), g), n);
        }
        parsed.push((p.clone(), d));
    }
    let mut out = Vec::new();
    for (p, mut d) in parsed {
        let new_path = rename.get(&p).cloned().unwrap_or_else(|| p.clone());
        let local: HashMap<String, String> = guid_map
            .iter()
            .filter(|((q, _), _)| *q == p)
            .map(|((_, g), n)| (g.clone(), n.clone()))
            .collect();
        for e in d.instances() {
            let g = guid_of(&d, e);
            if let Some(n) = local.get(&g) {
                d.set_attr(e, b"guid", n.as_bytes()).map_err(|x| err(x.0))?;
            }
        }
        for r in d.roots().to_vec() {
            for e in d.walk(r) {
                if let Some(v) = d.attr(e, b"primaryInstance").map(<[u8]>::to_vec)
                    && let Some(n) = local.get(&String::from_utf8_lossy(&v).to_ascii_uppercase())
                {
                    d.set_attr(e, b"primaryInstance", n.as_bytes()).map_err(|x| err(x.0))?;
                }
                let Some(v) = d.attr(e, b"ref").map(<[u8]>::to_vec) else {
                    continue;
                };
                let s = String::from_utf8_lossy(&v).into_owned();
                let to = match s.rsplit_once('/') {
                    Some((q, g)) => {
                        let ql = q.to_ascii_lowercase();
                        match (rename.get(&ql), guid_map.get(&(ql.clone(), g.to_ascii_uppercase()))) {
                            (Some(np), Some(ng)) => Some(format!("{np}/{}", ng.to_ascii_lowercase())),
                            _ => None,
                        }
                    }
                    None => local.get(&s.to_ascii_uppercase()).map(|n| n.to_ascii_lowercase()),
                };
                if let Some(t) = to {
                    d.set_attr(e, b"ref", t.as_bytes()).map_err(|x| err(x.0))?;
                }
            }
        }
        mpconvert::set_partition_guid(&mut d, &mpconvert::partition_guid_for(&new_path))
            .map_err(|e| err(e.to_string()))?;
        set_name(&mut d, &new_path)?;
        out.push((new_path, d));
    }
    Ok(out)
}

/// The primary instance's `Name` set to a partition's path (DICE's names are the paths in their case:
/// `Weather/R_MP005/WeatherSettings_MP005`; ours `Weather/<LEVEL>/...`, the same path ignoring case).
fn set_name(d: &mut EditDbx, path: &str) -> Result<()> {
    let primary = d
        .partition()
        .and_then(|p| d.attr(p, b"primaryInstance").map(<[u8]>::to_vec));
    let Some(pi) = primary else { return Ok(()) };
    let pi = String::from_utf8_lossy(&pi).to_ascii_uppercase();
    let Some(e) = d.instances().into_iter().find(|&e| guid_of(d, e) == pi) else {
        return Ok(());
    };
    if let Some(n) = d.child(e, b"Name")
        && d.text(n).is_some()
    {
        d.set_text(n, title_path(path).as_bytes()).map_err(|x| err(x.0))?;
    }
    Ok(())
}

/// `weather/mapedit_x/weathersettings` -> `Weather/MAPEDIT_X/WeatherSettings` (the name's case only).
fn title_path(path: &str) -> String {
    let parts: Vec<String> = path
        .split('/')
        .enumerate()
        .map(|(k, s)| match (k, s) {
            (0, "weather") => "Weather".to_owned(),
            (_, "weathersettings") => "WeatherSettings".to_owned(),
            (_, "weathersystem") => "WeatherSystem".to_owned(),
            (_, "weathersystementity") => "WeatherSystemEntity".to_owned(),
            (1, s) => s.to_ascii_uppercase(),
            (_, s) => s.to_owned(),
        })
        .collect();
    parts.join("/")
}

/// The primary instance's guid of a partition (upper case).
fn primary(d: &EditDbx) -> Option<String> {
    d.partition()
        .and_then(|p| d.attr(p, b"primaryInstance"))
        .map(|g| String::from_utf8_lossy(g).to_ascii_uppercase())
}

fn entry(path: &str, d: &EditDbx) -> Result<StagedEntry> {
    Ok(StagedEntry {
        name: format!("{}.dbx", title_path(path)),
        type_name: crate::export::NONRES.into(),
        data: d.write().map_err(|e| err(e.0))?,
    })
}

/// The 4-byte `WeatherSystem` resource of a system written under a new path.
fn system_res(path: &str) -> StagedEntry {
    StagedEntry {
        name: format!("{}.res", title_path(path)),
        type_name: SYSTEM_RES_TYPE.into(),
        data: SYSTEM_RES_VALUE.to_le_bytes().to_vec(),
    }
}

/// What [`stage`] made: the entries for the level's bundle, the report's lines, the record.
#[derive(Debug, Clone, Default)]
pub struct Staged {
    /// New partitions and resources (the level's own bundle).
    pub entries: Vec<StagedEntry>,
    /// The report's lines.
    pub report: Vec<String>,
    /// The record (`stage.json`).
    pub record: WeatherRecord,
}

/// Writes the project's weather choice into the exported level (module doc): the level partition `w` (as the
/// project and the export left it, before it is renamed) edited in place, the new partitions returned. `base`: the
/// base level; `low`: the new level's id in lower case; `seed`: the export's guid seed.
///
/// # Errors
/// The level's weather does not resolve, the chosen state is not one of the system's, a donor's partition is
/// missing, or a night / rain export was not turned on.
#[allow(clippy::too_many_lines, reason = "the export's weather steps in order")]
pub fn stage(game: &Game, c: &WeatherChoice, w: &mut LevelWriter, base: &str, low: &str, seed: &str) -> Result<Staged> {
    if c.unseen() && !c.unseen_ok {
        return Err(err(format!(
            "{}: a night or rain export is off by default (not yet seen in game); turn it on in the Export window's \
             Weather section to build it anyway",
            c.describe()
        )));
    }
    let base_live = {
        let mut b = game.loaded(base, "level");
        b.extend(game.startup());
        game.live(&b)?
    };
    let lw = level_weather(w, &base_live)?;
    let folder = format!("weather/{low}");
    let seed = format!("{seed}:weather");
    let mut out = Staged::default();
    let mut written: Vec<(String, String)> = Vec::new();
    // (new path, source path, the level whose bundles hold the source)
    let mut sources: Vec<(String, String, String)> = Vec::new();
    let mut appended: Option<String> = None;
    // 1. the system the level names now, and the chosen state's index in it
    let (system, settings_src, settings_live, state) = match (&c.borrow, &c.night) {
        (None, None) => {
            if c.state >= lw.states.len() {
                return Err(err(format!(
                    "state {} of {} - the system has {} states",
                    c.state,
                    lw.system,
                    lw.states.len()
                )));
            }
            (lw.system.clone(), lw.settings.clone(), &base_live, c.state)
        }
        (b, night) => {
            let (donor, state_path, swap) = match (b, night) {
                (_, Some(_)) => (NIGHT_TEMPLATE.0.to_owned(), NIGHT_TEMPLATE.1.to_owned(), false),
                (Some(b), None) => (b.donor.to_ascii_lowercase(), b.state.to_ascii_lowercase(), b.swap),
                (None, None) => unreachable!("matched above"),
            };
            let mut db = game.loaded(&donor, "level");
            db.extend(game.startup());
            let donor_live = game.live(&db)?;
            if swap {
                // the donor's whole system: its own weather, renamed under the level's folder
                let dw = level_weather(
                    &LevelWriter::parse(&crate::export::level_partition(game, &donor)?)?,
                    &donor_live,
                )?;
                let set = weather_closure(&donor_live, &dw.system);
                let rename: BTreeMap<String, String> = set
                    .iter()
                    .map(|(p, _)| (p.clone(), format!("{folder}/{}", p.trim_start_matches("weather/"))))
                    .collect();
                let new_system = rename.get(&dw.system).cloned().unwrap_or_default();
                for (p, d) in copy_renamed(&set, &rename, &format!("{seed}:swap"))? {
                    if let Some((old, _)) = rename.iter().find(|(_, n)| **n == p) {
                        sources.push((p.clone(), old.clone(), donor.clone()));
                    }
                    out.entries.push(entry(&p, &d)?);
                    written.push((p, "the donor's weather system (swapped in)".into()));
                }
                out.entries.push(system_res(&new_system));
                written.push((
                    format!("{new_system}.res"),
                    "the system's WeatherSystem resource".into(),
                ));
                let k = dw.states.iter().position(|s| *s == state_path).unwrap_or(c.state);
                // the donor's settings are the template of the level's (its sky dome, clouds and fog type)
                let mut sd = EditDbx::parse(&read_partition(&donor_live, &dw.settings)?)
                    .map_err(|e| err(format!("{}: {}", dw.settings, e.0)))?;
                let path = format!("{folder}/weathersettings");
                finish_settings(&mut sd, &path, k, &format!("{seed}:settings"))?;
                out.entries.push(entry(&path, &sd)?);
                written.push((path.clone(), format!("{donor}'s settings, ActiveState {k}")));
                sources.push((path.clone(), dw.settings.clone(), donor.clone()));
                if let Some(bp) = &lw.blueprint {
                    sources.push((format!("{folder}/weathersystementity"), bp.clone(), base.to_owned()));
                }
                let ss = primary(&sd).unwrap_or_default();
                let sys_primary = out
                    .entries
                    .iter()
                    .find(|e| e.name.eq_ignore_ascii_case(&format!("{}.dbx", title_path(&new_system))))
                    .and_then(|e| EditDbx::parse(&e.data).ok())
                    .and_then(|d| primary(&d))
                    .unwrap_or_default();
                let states: Vec<String> = dw
                    .states
                    .iter()
                    .map(|s| rename.get(s).cloned().unwrap_or_else(|| s.clone()))
                    .collect();
                point_entity(
                    game,
                    w,
                    &lw,
                    &base_live,
                    &folder,
                    &seed,
                    (&new_system, &sys_primary),
                    (&path, &ss),
                    &mut out.entries,
                    &mut written,
                )?;
                let spawn = set_spawn(w, k)?;
                out.record = record(c, &lw, &new_system, &path, states, k, spawn, w, written);
                out.record.sources = sources;
                out.report.push(format!(
                    "weather: {}'s whole system swapped in under {folder}/ ({} partitions, its sky), state {k} {}; \
                     ActiveState and the spawn switch set",
                    donor,
                    set.len(),
                    out.record.states.get(k).map_or("", String::as_str)
                ));
                return Ok(out);
            }
            // append: the donor's state (and its components) renamed under the level's folder, added to a copy of
            // the level's own system
            let set = weather_closure(&donor_live, &state_path);
            if set.is_empty() {
                return Err(err(format!("{state_path}: not in {donor}'s bundles")));
            }
            let rename: BTreeMap<String, String> = set
                .iter()
                .map(|(p, _)| (p.clone(), format!("{folder}/{}", p.trim_start_matches("weather/"))))
                .collect();
            let new_state = rename.get(&state_path).cloned().unwrap_or_default();
            let mut copies = copy_renamed(&set, &rename, &format!("{seed}:state"))?;
            if let Some(n) = night {
                darken(&mut copies, *n, &mut out.report)?;
            }
            let state_primary = copies
                .iter()
                .find(|(p, _)| *p == new_state)
                .and_then(|(_, d)| primary(d))
                .unwrap_or_default();
            for (p, d) in &copies {
                if let Some((old, _)) = rename.iter().find(|(_, n)| *n == p) {
                    sources.push((p.clone(), old.clone(), donor.clone()));
                }
                out.entries.push(entry(p, d)?);
                written.push((p.clone(), format!("{state_path} from {donor}, appended")));
            }
            let mut sys = EditDbx::parse(&read_partition(&base_live, &lw.system)?)
                .map_err(|e| err(format!("{}: {}", lw.system, e.0)))?;
            let new_system = format!("{folder}/weathersystem");
            append_state(&mut sys, &format!("{new_state}/{}", state_primary.to_ascii_lowercase()))?;
            mpconvert::set_partition_guid(&mut sys, &mpconvert::partition_guid_for(&new_system))
                .map_err(|e| err(e.to_string()))?;
            set_name(&mut sys, &new_system)?;
            out.entries.push(entry(&new_system, &sys)?);
            out.entries.push(system_res(&new_system));
            written.push((
                new_system.clone(),
                format!("{}'s system with the state appended", lw.system),
            ));
            sources.push((new_system.clone(), lw.system.clone(), base.to_owned()));
            appended = Some(new_state.clone());
            written.push((
                format!("{new_system}.res"),
                "the system's WeatherSystem resource".into(),
            ));
            out.report.push(format!(
                "weather: {state_path} from {donor} appended to a copy of {} as state {} ({} partitions renamed under \
                 {folder}/); the level keeps its Setup (its sky panorama {})",
                lw.system,
                lw.states.len(),
                copies.len(),
                if night.is_some() { "darkened" } else { "as it is" }
            ));
            (new_system, lw.settings.clone(), &base_live, lw.states.len())
        }
    };
    // 2. the settings: a copy with ActiveState, the entity re-pointed, the spawn switch set
    let mut sd = EditDbx::parse(&read_partition(settings_live, &settings_src)?)
        .map_err(|e| err(format!("{settings_src}: {}", e.0)))?;
    let path = format!("{folder}/weathersettings");
    finish_settings(&mut sd, &path, state, &format!("{seed}:settings"))?;
    out.entries.push(entry(&path, &sd)?);
    written.push((path.clone(), format!("{settings_src} with ActiveState {state}")));
    sources.push((path.clone(), settings_src.clone(), base.to_owned()));
    if let Some(bp) = &lw.blueprint {
        sources.push((format!("{folder}/weathersystementity"), bp.clone(), base.to_owned()));
    }
    let ss = primary(&sd).unwrap_or_default();
    let sys_primary = if system == lw.system {
        EditDbx::parse(&read_partition(&base_live, &system)?)
            .ok()
            .and_then(|d| primary(&d))
            .unwrap_or_default()
    } else {
        out.entries
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(&format!("{}.dbx", title_path(&system))))
            .and_then(|e| EditDbx::parse(&e.data).ok())
            .and_then(|d| primary(&d))
            .unwrap_or_default()
    };
    point_entity(
        game,
        w,
        &lw,
        &base_live,
        &folder,
        &seed,
        (&system, &sys_primary),
        (&path, &ss),
        &mut out.entries,
        &mut written,
    )?;
    let spawn = set_spawn(w, state)?;
    let mut states = lw.states.clone();
    states.extend(appended);
    out.record = record(c, &lw, &system, &path, states, state, spawn, w, written);
    out.record.sources = sources;
    out.report.push(format!(
        "weather: state {state} {} (ActiveState {} -> {state}; spawn switch {}); {} partitions written under {folder}/",
        out.record.states.get(state).map_or("", String::as_str),
        lw.active,
        match (&lw.spawn, &out.record.spawn) {
            (Some((_, a)), Some((_, b))) => format!("{a} -> {b}"),
            _ => "none (ActiveState alone starts the level)".into(),
        },
        out.record.written.len()
    ));
    if !out.record.others.is_empty() {
        out.report.push(format!(
            "  the level's other state entities (round end, scripted) name: {:?}",
            out.record.others
        ));
    }
    Ok(out)
}

#[allow(clippy::too_many_arguments, reason = "the record's parts")]
fn record(
    c: &WeatherChoice,
    lw: &LevelWeather,
    system: &str,
    settings: &str,
    states: Vec<String>,
    active: usize,
    spawn: Option<(String, usize)>,
    w: &LevelWriter,
    written: Vec<(String, String)>,
) -> WeatherRecord {
    let others =
        w.d.of_type(STATE_ENTITY_TYPE)
            .into_iter()
            .map(|e| {
                (
                    guid_of(&w.d, e),
                    int_field(&w.d, e, b"State")
                        .and_then(|v| usize::try_from(v).ok())
                        .unwrap_or(0),
                )
            })
            .filter(|(g, _)| spawn.as_ref().is_none_or(|(s, _)| s != g))
            .collect();
    WeatherRecord {
        choice: c.describe(),
        entity: lw.blueprint.clone().unwrap_or_else(|| "the level partition".into()),
        system: system.to_owned(),
        settings: settings.to_owned(),
        states,
        active,
        spawn,
        written,
        others,
        unseen: c.unseen(),
        sources: Vec::new(),
    }
}

/// The settings copy: new guids, its name and partition guid, `ActiveState`.
fn finish_settings(sd: &mut EditDbx, path: &str, state: usize, seed: &str) -> Result<()> {
    let set: Vec<(String, Vec<u8>)> = vec![(String::from("settings"), sd.write().map_err(|e| err(e.0))?)];
    let rename: BTreeMap<String, String> = [(String::from("settings"), path.to_owned())].into();
    let (_, mut d) = copy_renamed(&set, &rename, seed)?
        .pop()
        .ok_or_else(|| err("settings"))?;
    let e = d
        .of_type(b".WeatherSystemSettings")
        .first()
        .copied()
        .ok_or_else(|| err("the settings partition has no WeatherSystemSettings"))?;
    set_int_field(&mut d, e, b"ActiveState", state)?;
    *sd = d;
    Ok(())
}

/// A system's `States` with one more item (a reference `path/guid`).
fn append_state(sys: &mut EditDbx, reference: &str) -> Result<()> {
    let s = sys
        .of_type(b".WeatherSystemData")
        .first()
        .copied()
        .ok_or_else(|| err("the system partition has no WeatherSystemData"))?;
    let arr = sys.child(s, b"States").ok_or_else(|| err("the system has no States"))?;
    let last = sys
        .children(arr)
        .and_then(|c| c.last().copied())
        .ok_or_else(|| err("the system has no state to copy an item from"))?;
    let item = sys.clone_node(last);
    sys.set_attr(item, b"ref", reference.as_bytes()).map_err(|e| err(e.0))?;
    sys.children_mut(arr).map_err(|e| err(e.0))?.push(item);
    Ok(())
}

/// The night look on the template's copies (`VW2c`): the reflection cube scaled down, the panorama darkened.
fn darken(copies: &mut [(String, EditDbx)], n: Night, report: &mut Vec<String>) -> Result<()> {
    let mut done = Vec::new();
    for (p, d) in copies.iter_mut() {
        for e in d.of_type(b".DynamicOutdoorLightData") {
            let old = float_field(d, e, b"ReflectionEnvmapScale").unwrap_or(1.0);
            if set_float_field(d, e, b"ReflectionEnvmapScale", old * n.envmap_scale)? {
                done.push(format!("{p}: ReflectionEnvmapScale {old} -> {}", old * n.envmap_scale));
            }
        }
        for e in d.of_type(b".DynamicCloudLayerData") {
            // the panoramic layer is a struct of the cloud layer; DICE spells its field `Trancparency`
            let Some(pl) = d.child(e, b"PanoramicCloudLayer") else {
                continue;
            };
            let Some(old) = float_field(d, pl, b"Trancparency") else {
                continue;
            };
            if set_float_field(d, pl, b"Trancparency", old * n.panorama)? {
                done.push(format!(
                    "{p}: PanoramicCloudLayer.Trancparency {old} -> {}",
                    old * n.panorama
                ));
            }
        }
    }
    report.push(format!("  night: {}", done.join("; ")));
    Ok(())
}

/// The level's weather entity re-pointed at the system and settings (path, primary guid): in the level partition
/// itself, or a copy of the placed blueprint (the level's placements of it re-pointed).
#[allow(clippy::too_many_arguments, reason = "the export's state")]
fn point_entity(
    _game: &Game,
    w: &mut LevelWriter,
    lw: &LevelWeather,
    base_live: &HashMap<String, Found>,
    folder: &str,
    seed: &str,
    system: (&str, &str),
    settings: (&str, &str),
    entries: &mut Vec<StagedEntry>,
    written: &mut Vec<(String, String)>,
) -> Result<()> {
    let sys_ref = format!("{}/{}", system.0, system.1.to_ascii_lowercase());
    let set_ref = format!("{}/{}", settings.0, settings.1.to_ascii_lowercase());
    let point = |d: &mut EditDbx, e: NodeId| -> Result<()> {
        for (f, v) in [(&b"WeatherSystem"[..], &sys_ref), (b"Settings", &set_ref)] {
            let c = d
                .child(e, f)
                .ok_or_else(|| err(format!("the weather entity has no {}", String::from_utf8_lossy(f))))?;
            d.set_ref(c, v.as_bytes()).map_err(|x| err(x.0))?;
        }
        Ok(())
    };
    match &lw.blueprint {
        None => {
            let e = w
                .find(&lw.entity)
                .ok_or_else(|| err("the level's weather entity is gone"))?;
            point(&mut w.d, e)?;
            written.push(("the level partition".into(), "its weather entity re-pointed".into()));
        }
        Some(bp) => {
            let data = read_partition(base_live, bp)?;
            let path = format!("{folder}/weathersystementity");
            let set = vec![(bp.clone(), data)];
            let rename: BTreeMap<String, String> = [(bp.clone(), path.clone())].into();
            let (_, mut d) = copy_renamed(&set, &rename, &format!("{seed}:entity"))?
                .pop()
                .ok_or_else(|| err("entity"))?;
            let e = d
                .of_type(ENTITY_TYPE)
                .first()
                .copied()
                .ok_or_else(|| err("the entity blueprint has no WeatherSystemEntityData"))?;
            point(&mut d, e)?;
            let bp_primary = primary(&d).unwrap_or_default();
            entries.push(entry(&path, &d)?);
            written.push((path.clone(), format!("{bp} re-pointed")));
            // the level's placements of the blueprint
            let new_ref = format!("{path}/{}", bp_primary.to_ascii_lowercase());
            let mut n = 0;
            for r in w.d.of_type(b".ReferenceObjectData") {
                if let Some(c) = w.d.child(r, b"ReferencedObject")
                    && w.d.attr(c, b"ref").and_then(split_ref).is_some_and(|(p, _)| p == *bp)
                {
                    w.d.set_ref(c, new_ref.as_bytes()).map_err(|x| err(x.0))?;
                    n += 1;
                }
            }
            written.push((
                "the level partition".into(),
                format!("{n} placement(s) of {bp} re-pointed"),
            ));
        }
    }
    Ok(())
}

/// The spawn switch's `State` set (`None`: the level has none).
fn set_spawn(w: &mut LevelWriter, state: usize) -> Result<Option<(String, usize)>> {
    let Some(e) = spawn_switch(w) else { return Ok(None) };
    set_int_field(&mut w.d, e, b"State", state)?;
    Ok(Some((guid_of(&w.d, e), state)))
}

/// The check's weather lines (`mapexport check`, round VW2) on the staged level partition and the bundles the level
/// loads: the entity resolves, its system and settings are in the bundles, every state's partition is too,
/// `ActiveState` and the spawn switch name the recorded state, a system written by the export has its 4-byte
/// resource, and a night or rain export says it is not yet seen in game.
#[must_use]
#[allow(clippy::implicit_hasher, reason = "the check's live entries")]
pub fn check(rec: &WeatherRecord, level: &LevelWriter, live: &HashMap<String, Found>) -> Vec<(bool, String)> {
    let mut out = Vec::new();
    match level_weather(level, live) {
        Ok(lw) => {
            out.push((
                lw.system == rec.system && lw.settings == rec.settings,
                format!(
                    "weather: the level's entity names {} and {} (the export's: {} and {})",
                    lw.system, lw.settings, rec.system, rec.settings
                ),
            ));
            let missing: Vec<&String> = lw
                .states
                .iter()
                .filter(|s| !live.contains_key(&format!("{s}.dbx")))
                .collect();
            out.push((
                missing.is_empty() && lw.states == rec.states,
                format!(
                    "weather: the system's {} states are in the bundles the level loads {missing:?}",
                    lw.states.len()
                ),
            ));
            let spawn_ok = match (&lw.spawn, &rec.spawn) {
                (Some((g, s)), Some((rg, rs))) => g == rg && s == rs && *s == rec.active,
                (None, None) => true,
                _ => false,
            };
            out.push((
                lw.active == rec.active && lw.active < lw.states.len() && spawn_ok,
                format!(
                    "weather: ActiveState {} and the spawn switch {:?} name state {} {} (spec RNDR-WTHR-8)",
                    lw.active,
                    lw.spawn.as_ref().map(|x| x.1),
                    rec.active,
                    lw.states.get(rec.active).map_or("(none)", String::as_str)
                ),
            ));
            if rec
                .written
                .iter()
                .any(|(p, _)| p.eq_ignore_ascii_case(&format!("{}.res", rec.system)))
            {
                let res = live.get(&format!("{}.res", rec.system));
                let ok = res.is_some_and(|f| f.1.type_name == SYSTEM_RES_TYPE)
                    && res
                        .and_then(|f| read_found(&[f]).ok())
                        .and_then(|mut v| v.pop())
                        .is_some_and(|b| b == SYSTEM_RES_VALUE.to_le_bytes());
                out.push((
                    ok,
                    format!(
                        "weather: the new system's WeatherSystem resource {}.res is there, 4 bytes, {SYSTEM_RES_VALUE}",
                        rec.system
                    ),
                ));
            }
            // every partition a state reaches resolves (its components)
            let mut unresolved = Vec::new();
            for s in &lw.states {
                {
                    for (_, data) in weather_closure(live, s) {
                        if let Ok(d) = EditDbx::parse(&data) {
                            for r in ref_paths(&d).into_iter().filter(|r| r.starts_with("weather/")) {
                                if !live.contains_key(&format!("{r}.dbx")) && !unresolved.contains(&r) {
                                    unresolved.push(r);
                                }
                            }
                        }
                    }
                }
            }
            // DICE's static Sun / Sky data are in no bundle (WX0 A.6): the game runs without them
            unresolved.retain(|r| !r.contains("/staticdata_") || !(r.ends_with("_sun") || r.ends_with("_sky")));
            out.push((
                unresolved.is_empty(),
                format!("weather: every state's components resolve {unresolved:?}"),
            ));
        }
        Err(e) => out.push((false, format!("weather: {}", e.0))),
    }
    out.push((
        true,
        if rec.unseen {
            format!(
                "weather: {} - NOT YET SEEN IN GAME (night / rain; turned on by the user)",
                rec.choice
            )
        } else {
            format!("weather: {}", rec.choice)
        },
    ));
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn names_choices_and_paths() {
        assert_eq!(
            title_path("weather/mapedit_x/weathersettings"),
            "Weather/MAPEDIT_X/WeatherSettings"
        );
        assert_eq!(
            title_path("weather/mapedit_x/r_mp02/states/ons_mp_002_night"),
            "Weather/MAPEDIT_X/r_mp02/states/ons_mp_002_night"
        );
        assert!(is_unseen_name("ons_mp_002_night") && is_unseen_name("prison_rain") && !is_unseen_name("midday"));
        let c = WeatherChoice {
            state: 4,
            name: "coral_sea".into(),
            borrow: None,
            night: None,
            unseen_ok: false,
        };
        assert!(!c.unseen());
        assert_eq!(c.describe(), "Weather: coral_sea (the level's state 4)");
        let n = WeatherChoice {
            night: Some(Night::default()),
            ..c
        };
        assert!(n.unseen());
        assert_eq!(
            split_ref(b"weather/a/B/0aB"),
            Some(("weather/a/b".into(), "0AB".into()))
        );
    }
}
