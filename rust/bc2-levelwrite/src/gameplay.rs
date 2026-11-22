//! Typed gameplay operations (round XF1, audit AU1 D18 / R23): edits E4's gameplay tools make, in words the History
//! can show and with the data's rules checked when they apply, instead of `SetField`s with raw references.
//!
//! - [`Team`] set on an object: a capture point's `InitialOwnerTeam`, any other object's `Team` (LEVEL-ENT-1).
//! - A soldier spawner linked to or unlinked from a capture point: an item of the point's `Spawners` added or
//!   removed (GAME-SPAWN-1).
//! - A team's tickets: a ticket counter's `InitialTicketCount` and `TicketLossPerMin` (GAME-TKT-1, GAME-TKT-2).
//! - An outline point inserted after another or removed, the `Next` chain kept closed (LEVEL-ENT-2): a combat area's or
//!   a capture zone's polygon edited one vertex at a time.
//!
//! Round E4's gameplay tools add:
//!
//! - A spawn point linked to or unlinked from a soldier spawner: an item of its `AlternativeSpawnPoints`
//!   (GAME-SPAWN-1, [`link_point`]).
//! - A gameplay object's **unit** ([`unit`]): what moves with it ([`crate::modelayer::cluster`]: a flag's zone, marker,
//!   spawners and spawn points; an M-COM's interactions, marker, alarm and objective props) and what only its events
//!   reach (a flag's team filters, capture messages and voice-over; GAME-CQ-10). A flag, spawner or M-COM is added as
//!   a copy of a template's unit wired as the template is ([`crate::write::LevelWriter::place_unit`]) and deleted with
//!   its unit.
//! - A Rush stage's counters ([`stage_counters`]): each counts its stage's M-COMs (GAME-RUSH-10), so an M-COM added
//!   to or taken from a stage sets every counter's `TriggerAmount` to the M-COMs that reach it.
//! - The round's [`Rules`]: each team's tickets and their loss, the flags' capture time, Rush's lives, Squad
//!   Deathmatch's kills, with DICE's values per mode and era as the default ([`stock`], GAME-RULES-1) and DICE's own
//!   values as the choices ([`choices`]).

use serde::{Deserialize, Serialize};

use crate::files::FileEdit;
use crate::ops::{FieldIs, FieldSet, FieldValue, Moved, Op, Placed, Rows, SubCopy};
use crate::schema::{FieldType, Schema};
use crate::write::{LevelWriteError, LevelWriter, Result, refers_to};

/// A team, as the data names it (LEVEL-ENT-1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Team {
    /// `Team1` (US on BFBC2's levels).
    Team1,
    /// `Team2`.
    Team2,
    /// `TeamNeutral`.
    TeamNeutral,
}

impl Team {
    /// The data's text.
    #[must_use]
    pub fn data(self) -> &'static str {
        match self {
            Self::Team1 => "Team1",
            Self::Team2 => "Team2",
            Self::TeamNeutral => "TeamNeutral",
        }
    }

    /// The data's text read back.
    #[must_use]
    pub fn of_data(t: &str) -> Option<Self> {
        match t {
            "Team1" => Some(Self::Team1),
            "Team2" => Some(Self::Team2),
            "TeamNeutral" => Some(Self::TeamNeutral),
            _ => None,
        }
    }

    /// In words.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Team1 => "team 1",
            Self::Team2 => "team 2",
            Self::TeamNeutral => "neutral",
        }
    }
}

/// One object's team set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSet {
    /// The object.
    pub guid: String,
    /// Its team before (for the History; `None` where it was not read).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Team>,
    /// Its team after.
    pub to: Team,
}

/// A ticket counter's values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Tickets {
    /// `InitialTicketCount`.
    pub initial: i64,
    /// `TicketLossPerMin`.
    pub loss_per_min: f64,
}

/// The type names the operations check (the suffix after the module: `Entity.CapturePointEntityData`).
const CAPTURE_POINT: &str = ".CapturePointEntityData";
const SOLDIER_SPAWN: &str = ".SoldierSpawnEntityData";
const VEHICLE_SPAWN: &str = ".VehicleSpawnEntityData";
const TICKET_COUNTER: &str = ".TicketCounterEntityData";
const AREA_POINT: &str = ".AreaGeometryEntityData";

fn refused(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

/// The instance of `guid` when its type ends with `ty`, else refused in words.
fn of_type(w: &LevelWriter, guid: &str, ty: &str, what: &str) -> Result<bc2_formats::dbx::edit::NodeId> {
    let e = w.find(guid).ok_or_else(|| LevelWriteError::NotFound(guid.to_owned()))?;
    let t = w.type_of(e);
    if !t.ends_with(ty) {
        return Err(refused(format!("{guid} is a {t}, not {what}")));
    }
    Ok(e)
}

/// The field an object's team is in: `InitialOwnerTeam` on a capture point, else `Team`.
///
/// # Errors
/// The object is not there or has no team.
pub fn team_field(w: &LevelWriter, guid: &str) -> Result<&'static str> {
    let e = w.find(guid).ok_or_else(|| LevelWriteError::NotFound(guid.to_owned()))?;
    if w.d.child(e, b"InitialOwnerTeam").is_some() {
        Ok("InitialOwnerTeam")
    } else if w.d.child(e, b"Team").is_some() {
        Ok("Team")
    } else {
        Err(refused(format!("{guid} ({}) has no team", w.type_of(e))))
    }
}

/// An object's team now.
#[must_use]
pub fn team_of(w: &LevelWriter, guid: &str) -> Option<Team> {
    let f = team_field(w, guid).ok()?;
    let e = w.find(guid)?;
    let t = w.d.child(e, f.as_bytes()).and_then(|x| w.d.text(x))?;
    Team::of_data(&String::from_utf8_lossy(t))
}

/// Sets an object's team.
///
/// # Errors
/// The object is not there or has no team, or the field does not take the text.
pub fn set_team(w: &mut LevelWriter, schema: &Schema, guid: &str, team: Team) -> Result<()> {
    let f = team_field(w, guid)?;
    w.set_field(schema, guid, f, &FieldValue::Text(team.data().to_owned()))
}

/// The spawners a capture point lists (upper-case guids).
#[must_use]
pub fn spawners(w: &LevelWriter, point: &str) -> Vec<String> {
    let Some(e) = w.find(point) else { return Vec::new() };
    let Some(a) = w.d.child(e, b"Spawners") else {
        return Vec::new();
    };
    w.d.children(a)
        .unwrap_or_default()
        .iter()
        .filter_map(|&i| w.d.attr(i, b"ref"))
        .map(|r| String::from_utf8_lossy(r.rsplit(|&c| c == b'/').next().unwrap_or(r)).to_ascii_uppercase())
        .collect()
}

/// Links a soldier or vehicle spawner to a capture point (an item of its `Spawners` added at the end; DICE's
/// capture points list both: the vehicles of a base or flag, round A2) or unlinks it (its item removed).
///
/// # Errors
/// Either is not there or not of its type, the link is already there (or, unlinking, not there), or the level has
/// no reference item to copy the new item's form from.
pub fn link_spawner(w: &mut LevelWriter, point: &str, spawner: &str, linked: bool) -> Result<()> {
    let p = of_type(w, point, CAPTURE_POINT, "a capture point")?;
    if of_type(w, spawner, VEHICLE_SPAWN, "a vehicle spawner").is_err() {
        of_type(w, spawner, SOLDIER_SPAWN, "a soldier or vehicle spawner")?;
    }
    link_item(w, p, point, b"Spawners", spawner, linked, ("spawner", "spawners"))
}

/// An item naming `item` added to (or removed from) the array `field` of `owner` (node `o`).
fn link_item(
    w: &mut LevelWriter,
    o: bc2_formats::dbx::edit::NodeId,
    owner: &str,
    field: &[u8],
    item: &str,
    linked: bool,
    words: (&str, &str),
) -> Result<()> {
    let a = w.d.child(o, field).ok_or_else(|| LevelWriteError::NoField {
        guid: owner.to_owned(),
        field: String::from_utf8_lossy(field).into_owned(),
    })?;
    let g = item.as_bytes();
    let items: Vec<_> = w.d.children(a).unwrap_or_default().to_vec();
    let at = items
        .iter()
        .position(|&i| w.d.attr(i, b"ref").is_some_and(|r| refers_to(r, g)));
    let (one, many) = words;
    match (linked, at) {
        (true, Some(_)) => Err(refused(format!("{item} is already one of {owner}'s {many}"))),
        (false, None) => Err(refused(format!("{item} is not one of {owner}'s {many}"))),
        (false, Some(i)) => {
            w.d.children_mut(a)?.remove(i);
            Ok(())
        }
        (true, None) => {
            // the new item in the form of a reference item the level has (this list's, else the level's Objects')
            let form = items.first().copied().or_else(|| {
                w.level_data()
                    .and_then(|l| w.d.child(l, b"Objects"))
                    .and_then(|o| w.d.children(o).unwrap_or_default().first().copied())
            });
            let form = form.ok_or_else(|| {
                refused(format!(
                    "the level has no reference item to copy the new {one}'s item from"
                ))
            })?;
            let it = w.d.clone_node(form);
            w.d.set_ref(it, item.to_ascii_lowercase().as_bytes())?;
            if w.d.children(a).is_none() {
                w.d.make_container(a)?;
            }
            w.d.children_mut(a)?.push(it);
            Ok(())
        }
    }
}

/// A ticket counter's values now.
#[must_use]
pub fn tickets_of(w: &LevelWriter, schema: &Schema, counter: &str) -> Option<Tickets> {
    let num = |f: &str| match w.get_field(schema, counter, f).ok()?.1? {
        #[allow(clippy::cast_precision_loss, reason = "ticket counts are far below 2^52")]
        FieldValue::Int(i) => Some(i as f64),
        FieldValue::Float(x) => Some(x),
        _ => None,
    };
    Some(Tickets {
        initial: num("InitialTicketCount")? as i64,
        loss_per_min: num("TicketLossPerMin")?,
    })
}

/// Sets a ticket counter's start and bleed, each in the type the data gives the field.
///
/// # Errors
/// The counter is not there or not a ticket counter, or a value is negative.
pub fn set_tickets(w: &mut LevelWriter, schema: &Schema, counter: &str, to: Tickets) -> Result<()> {
    of_type(w, counter, TICKET_COUNTER, "a ticket counter")?;
    if to.initial < 0 || to.loss_per_min < 0.0 || !to.loss_per_min.is_finite() {
        return Err(refused("tickets and their loss per minute are 0 or more"));
    }
    let typed = |ty: FieldType, whole: i64, v: f64| match ty {
        FieldType::Float => FieldValue::Float(v),
        _ => FieldValue::Int(whole),
    };
    let (ty, _) = w.get_field(schema, counter, "InitialTicketCount")?;
    #[allow(clippy::cast_precision_loss, reason = "ticket counts are far below 2^52")]
    let initial = typed(ty, to.initial, to.initial as f64);
    w.set_field(schema, counter, "InitialTicketCount", &initial)?;
    let (ty, _) = w.get_field(schema, counter, "TicketLossPerMin")?;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "a whole-number field takes the whole part"
    )]
    let loss = typed(ty, to.loss_per_min as i64, to.loss_per_min);
    w.set_field(schema, counter, "TicketLossPerMin", &loss)?;
    Ok(())
}

/// The outline a point is in, from it along `Next` (upper-case guids; it stops at the start again, a `null`, or a
/// point it has seen).
#[must_use]
pub fn outline(w: &LevelWriter, guid: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut at = guid.to_ascii_uppercase();
    while !out.contains(&at) {
        let Some(e) = w.find(&at) else { break };
        out.push(at.clone());
        let Some(n) = w.d.child(e, b"Next").and_then(|x| w.d.attr(x, b"ref")) else {
            break;
        };
        let n = String::from_utf8_lossy(n).to_ascii_uppercase();
        if n == "NULL" || n.is_empty() {
            break;
        }
        n.rsplit('/').next().unwrap_or(&n).clone_into(&mut at);
    }
    out
}

/// A point's `Next` or `Previous` reference (`null` when it has none).
fn link(w: &LevelWriter, guid: &str, field: &[u8]) -> Vec<u8> {
    w.find(guid)
        .and_then(|e| w.d.child(e, field))
        .and_then(|x| w.d.attr(x, b"ref"))
        .map_or_else(|| b"null".to_vec(), <[u8]>::to_vec)
}

/// Sets a point's `Next` or `Previous` (when the reference names a point of the partition).
fn set_link(w: &mut LevelWriter, r: &[u8], field: &[u8], to: &[u8]) -> Result<()> {
    let g = String::from_utf8_lossy(r.rsplit(|&c| c == b'/').next().unwrap_or(r)).into_owned();
    if let Some(e) = w.find(&g)
        && let Some(x) = w.d.child(e, field)
    {
        w.d.set_ref(x, to)?;
    }
    Ok(())
}

/// Inserts an outline point after `after` (LEVEL-ENT-2: points are a chain of `Next` and `Previous`; an area's
/// `Geometry` names one point of each outline): a copy of it as `guid` at `transform` (its `Objects` item and
/// connections copied as a duplicate's), between `after` and its `Next`, both links of all three set.
///
/// # Errors
/// `after` is not there or not an outline point, or the guid is taken.
pub fn insert_point(w: &mut LevelWriter, after: &str, guid: &str, transform: &Rows) -> Result<()> {
    let a = of_type(w, after, AREA_POINT, "an outline point")?;
    if w.d.child(a, b"Next").is_none() {
        return Err(LevelWriteError::NoField {
            guid: after.to_owned(),
            field: "Next".into(),
        });
    }
    let next = link(w, after, b"Next");
    w.duplicate(after, guid, Some(transform))?;
    let new = guid.to_ascii_lowercase();
    let after_low = after.to_ascii_lowercase();
    // the copy's Next is after's (copied); its Previous is after; after's Next and the old next's Previous the copy
    set_link(w, guid.as_bytes(), b"Previous", after_low.as_bytes())?;
    set_link(w, after.as_bytes(), b"Next", new.as_bytes())?;
    if next != b"null" {
        set_link(w, &next, b"Previous", new.as_bytes())?;
    }
    Ok(())
}

/// Removes an outline point: its neighbours linked to each other (`Next`, `Previous`), a list that names it as its
/// outline's point (an area's `Geometry`) made to name its `Next` instead, then it is deleted with its wiring (its
/// `Objects` item and connections).
///
/// # Errors
/// It is not there or not an outline point, or a closed outline would keep fewer than three points.
pub fn remove_point(w: &mut LevelWriter, guid: &str) -> Result<()> {
    of_type(w, guid, AREA_POINT, "an outline point")?;
    let ring = outline(w, guid);
    let next = link(w, guid, b"Next");
    let prev = link(w, guid, b"Previous");
    let closed = next != b"null" && prev != b"null";
    if closed && ring.len() <= 3 {
        return Err(refused(format!(
            "{guid}'s outline has {} points: an outline keeps three at least",
            ring.len()
        )));
    }
    if prev != b"null" {
        set_link(w, &prev, b"Next", &next)?;
    }
    if next != b"null" {
        set_link(w, &next, b"Previous", &prev)?;
    }
    // a list naming it (not Objects): its outline's point is now its Next
    let objects = w.level_data().and_then(|l| w.d.child(l, b"Objects"));
    let g = guid.as_bytes();
    if next != b"null" {
        for inst in w.d.instances() {
            for x in w.d.walk(inst) {
                let in_objects = objects.is_some_and(|o| w.d.children(o).unwrap_or_default().contains(&x));
                if !in_objects && w.d.tag(x) == b"item" && w.d.attr(x, b"ref").is_some_and(|r| refers_to(r, g)) {
                    w.d.set_ref(x, &next)?;
                }
            }
        }
    }
    w.delete(guid)?;
    Ok(())
}

// ---------------------------------------------------------------------------------------------------------------
// Round E4: spawn points, units, Rush stage counters, the round's rules.

const ALTERNATE_SPAWN: &str = ".AlternateSpawnEntityData";
const MULTIPLE_TRIGGER: &str = ".MultipleTriggerEntityData";
const LIFE_COUNTER: &str = ".LifeCounterEntityData";
const KILL_COUNTER: &str = ".KillCounterEntityData";

/// The guid a reference names (its last part), upper case.
fn named(r: &[u8]) -> String {
    String::from_utf8_lossy(r.rsplit(|&c| c == b'/').next().unwrap_or(r)).to_ascii_uppercase()
}

/// The spawn points a soldier spawner lists (`AlternativeSpawnPoints`, upper-case guids).
#[must_use]
pub fn points(w: &LevelWriter, spawner: &str) -> Vec<String> {
    let Some(e) = w.find(spawner) else { return Vec::new() };
    let Some(a) = w.d.child(e, b"AlternativeSpawnPoints") else {
        return Vec::new();
    };
    w.d.children(a)
        .unwrap_or_default()
        .iter()
        .filter_map(|&i| w.d.attr(i, b"ref"))
        .map(named)
        .collect()
}

/// Links a spawn point (`AlternateSpawnEntityData`) to a soldier spawner (an item of its `AlternativeSpawnPoints`
/// added at the end: GAME-SPAWN-1) or unlinks it (its item removed).
///
/// # Errors
/// Either is not there or not of its type, or the link is already there (or, unlinking, not there).
pub fn link_point(w: &mut LevelWriter, spawner: &str, point: &str, linked: bool) -> Result<()> {
    let s = of_type(w, spawner, SOLDIER_SPAWN, "a soldier spawner")?;
    of_type(w, point, ALTERNATE_SPAWN, "a spawn point")?;
    link_item(
        w,
        s,
        spawner,
        b"AlternativeSpawnPoints",
        point,
        linked,
        ("spawn point", "spawn points"),
    )
}

/// A gameplay object's unit (round E4, GAME-CQ-10): the object first, then what moves with it
/// ([`crate::modelayer::cluster`]) and what it owns through references and its events
/// ([`LevelWriter::ownership_with_events`]), in partition order. A flag's unit is its zone, marker, spawners and
/// spawn points, its team filters, capture messages and voice-over; an M-COM's its arm and defuse interactions,
/// marker, alarm, charge trigger and objective props; a soldier spawner's its spawn points. What other objects share
/// (a Rush stage's counters, the announcer's lines several objects start) is not in it.
#[must_use]
pub fn unit(w: &LevelWriter, guid: &str) -> Vec<String> {
    let root = guid.to_ascii_uppercase();
    if w.find(&root).is_none() {
        return Vec::new();
    }
    let cluster = crate::modelayer::cluster(w, &root);
    let owned = w.ownership_with_events().owned_from(&cluster);
    let mut members: std::collections::HashSet<String> = cluster.into_iter().collect();
    members.extend(owned);
    members.remove(&root);
    let mut out = vec![root];
    out.extend(w.guids().into_iter().filter(|g| members.contains(g)));
    out
}

/// One event connection: its source, target (upper-case guids) and source event.
struct Wire {
    src: String,
    dst: String,
    sev: String,
}

fn wires(w: &LevelWriter) -> Vec<Wire> {
    let mut out = Vec::new();
    for a in w.connection_arrays() {
        for &c in w.d.children(a).unwrap_or_default() {
            let end = |f: &[u8]| w.d.child(c, f).and_then(|x| w.d.attr(x, b"ref")).map(named);
            let sev =
                w.d.child(c, b"SourceEvent")
                    .and_then(|x| w.d.child(x, b"Name"))
                    .and_then(|x| w.d.text(x))
                    .map_or_else(String::new, |t| String::from_utf8_lossy(t).into_owned());
            if let (Some(src), Some(dst)) = (end(b"Source"), end(b"Target")) {
                out.push(Wire { src, dst, sev });
            }
        }
    }
    out
}

/// A Rush stage's counters (GAME-RUSH-10): each `MultipleTriggerEntityData` an M-COM's `onDestroyed` reaches
/// directly, with the M-COMs of `mcoms` that reach it (upper-case guids, in the partition's order). DICE's
/// `TriggerAmount` is that number of M-COMs on every stock Rush and Squad Rush level.
#[must_use]
pub fn stage_counters(w: &LevelWriter, mcoms: &[String]) -> Vec<(String, Vec<String>)> {
    let mcoms: Vec<String> = mcoms.iter().map(|m| m.to_ascii_uppercase()).collect();
    let counters: Vec<String> =
        w.d.of_type(MULTIPLE_TRIGGER.as_bytes())
            .into_iter()
            .map(|e| named(w.d.attr(e, b"guid").unwrap_or_default()))
            .collect();
    let ws = wires(w);
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for c in counters {
        let mut by: Vec<String> = Vec::new();
        for wi in ws.iter().filter(|x| x.dst == c && x.sev == "onDestroyed") {
            if mcoms.contains(&wi.src) && !by.contains(&wi.src) {
                by.push(wi.src.clone());
            }
        }
        if !by.is_empty() {
            out.push((c, by));
        }
    }
    out
}

/// A stage counter's `TriggerAmount` as a whole number (`None` where it does not read).
#[must_use]
pub fn trigger_amount(w: &LevelWriter, schema: &Schema, counter: &str) -> Option<i64> {
    number(w, schema, counter, "TriggerAmount").map(whole)
}

/// The round's rules (round E4): what the level's data sets for a round of its mode. `None`: not set (the mode has
/// no such value, or it is left as it is).
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Rules {
    /// Conquest: each team's tickets at the start (every ticket counter's `InitialTicketCount`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tickets: Option<i64>,
    /// Conquest: tickets lost a minute by the team holding fewer flags (`TicketLossPerMin`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub loss_per_min: Option<f64>,
    /// Conquest: seconds to take a flag, and to lose it (every flag's `TimeToGetControl` and `TimeToLoseControl`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capture_s: Option<f64>,
    /// Rush and Squad Rush: the attackers' lives (the attacking life counter's `StartingLifeCount`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attackers: Option<i64>,
    /// Rush and Squad Rush: the defenders' lives (the other life counter's `StartingLifeCount`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub defenders: Option<i64>,
    /// Squad Deathmatch: the kills a squad needs to win (every kill counter's `MaxKillCount`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kills: Option<i64>,
}

/// The rules [`Rules`] holds, in the panel's order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// [`Rules::tickets`].
    Tickets,
    /// [`Rules::loss_per_min`].
    LossPerMin,
    /// [`Rules::capture_s`].
    CaptureS,
    /// [`Rules::attackers`].
    Attackers,
    /// [`Rules::defenders`].
    Defenders,
    /// [`Rules::kills`].
    Kills,
}

#[allow(clippy::cast_precision_loss, reason = "rule values are far below 2^52")]
fn as_f64(x: i64) -> f64 {
    x as f64
}

#[allow(
    clippy::cast_possible_truncation,
    reason = "rule values inside their ranges, rounded"
)]
fn whole(x: f64) -> i64 {
    x.round() as i64
}

impl Rules {
    /// A rule's value.
    #[must_use]
    pub fn get(&self, r: Rule) -> Option<f64> {
        match r {
            Rule::Tickets => self.tickets.map(as_f64),
            Rule::LossPerMin => self.loss_per_min,
            Rule::CaptureS => self.capture_s,
            Rule::Attackers => self.attackers.map(as_f64),
            Rule::Defenders => self.defenders.map(as_f64),
            Rule::Kills => self.kills.map(as_f64),
        }
    }

    /// A rule set (a whole number's rounded; `None`: left as it is).
    pub fn set(&mut self, r: Rule, v: Option<f64>) {
        match r {
            Rule::Tickets => self.tickets = v.map(whole),
            Rule::LossPerMin => self.loss_per_min = v,
            Rule::CaptureS => self.capture_s = v,
            Rule::Attackers => self.attackers = v.map(whole),
            Rule::Defenders => self.defenders = v.map(whole),
            Rule::Kills => self.kills = v.map(whole),
        }
    }

    /// The rules in words (the History).
    #[must_use]
    pub fn describe(&self) -> String {
        let mut out = Vec::new();
        if let Some(t) = self.tickets {
            out.push(format!("{t} tickets"));
        }
        if let Some(l) = self.loss_per_min {
            out.push(format!("losing {} a minute", l as f32));
        }
        if let Some(c) = self.capture_s {
            out.push(format!("a flag taken in {} s", c as f32));
        }
        if let Some(a) = self.attackers {
            out.push(format!("attackers {a} lives"));
        }
        if let Some(d) = self.defenders {
            out.push(format!("defenders {d} lives"));
        }
        if let Some(k) = self.kills {
            out.push(format!("{k} kills to win"));
        }
        if out.is_empty() {
            "unchanged".into()
        } else {
            out.join(", ")
        }
    }
}

/// DICE's rules for a mode and era (GAME-RULES-1, from GAME-TKT-1, GAME-CQ-6, GAME-RUSH-8 and GAME-SDM-1): what the
/// editor sets by default. Conquest 200 tickets losing 10 a minute and 20 s to take a flag (WW2, BF1943's: 130,
/// 4 and 10 s); Rush the attackers 100 lives (BFBC1 and Vietnam 75) against 1,500, Squad Rush 20 against 1,000;
/// Squad Deathmatch 50 kills.
#[must_use]
pub fn stock(mode: crate::placement::Mode, profile: crate::project::Profile) -> Rules {
    use crate::placement::Mode;
    use crate::project::Profile;
    let ww2 = profile == Profile::Ww2;
    match mode {
        Mode::Conquest => Rules {
            tickets: Some(if ww2 { 130 } else { 200 }),
            loss_per_min: Some(if ww2 { 4.0 } else { 10.0 }),
            capture_s: Some(if ww2 { 10.0 } else { 20.0 }),
            ..Rules::default()
        },
        Mode::Rush => Rules {
            attackers: Some(if matches!(profile, Profile::Bfbc1 | Profile::Vietnam) {
                75
            } else {
                100
            }),
            defenders: Some(1500),
            ..Rules::default()
        },
        Mode::SquadRush => Rules {
            attackers: Some(20),
            defenders: Some(1000),
            ..Rules::default()
        },
        Mode::SquadDeathmatch => Rules {
            kills: Some(50),
            ..Rules::default()
        },
    }
}

/// One rule the editor offers: its name, the values DICE used for it (the choices) and the range it takes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Choice {
    /// The rule.
    pub rule: Rule,
    /// Its name in the panel.
    pub name: &'static str,
    /// Its unit.
    pub unit: &'static str,
    /// The values DICE's stock levels of the era use (the panel's choices; any other value is the user's own).
    pub dice: &'static [f64],
    /// The lowest and highest value the editor takes.
    pub range: [f64; 2],
}

/// The rules a mode has, with DICE's values for each as the choices (GAME-RULES-1): the values on the stock levels
/// of the era (L1's measurements: Conquest tickets 200, 250 or 350 on DICE's levels, 130 on BF1943's; a flag taken
/// in 20 s on all but one DICE flag, 25 s on that one, 10 s on BF1943's).
#[must_use]
pub fn choices(mode: crate::placement::Mode, profile: crate::project::Profile) -> Vec<Choice> {
    use crate::placement::Mode;
    use crate::project::Profile;
    let ww2 = profile == Profile::Ww2;
    let old = matches!(profile, Profile::Bfbc1 | Profile::Vietnam);
    let c = |rule, name, unit, dice, range| Choice {
        rule,
        name,
        unit,
        dice,
        range,
    };
    match mode {
        Mode::Conquest => vec![
            c(
                Rule::Tickets,
                "Tickets per team",
                "tickets",
                if ww2 { &[130.0] } else { &[200.0, 250.0, 350.0] },
                [1.0, 5000.0],
            ),
            c(
                Rule::LossPerMin,
                "Ticket loss",
                "a minute",
                if ww2 { &[4.0] } else { &[10.0] },
                [0.0, 200.0],
            ),
            c(
                Rule::CaptureS,
                "Time to take a flag",
                "s",
                if ww2 { &[10.0] } else { &[20.0, 25.0] },
                [1.0, 300.0],
            ),
        ],
        Mode::Rush => vec![
            c(
                Rule::Attackers,
                "Attackers' lives",
                "lives",
                if old { &[75.0] } else { &[100.0] },
                [1.0, 5000.0],
            ),
            c(Rule::Defenders, "Defenders' lives", "lives", &[1500.0], [1.0, 10000.0]),
        ],
        Mode::SquadRush => vec![
            c(Rule::Attackers, "Attackers' lives", "lives", &[20.0], [1.0, 5000.0]),
            c(Rule::Defenders, "Defenders' lives", "lives", &[1000.0], [1.0, 10000.0]),
        ],
        Mode::SquadDeathmatch => vec![c(Rule::Kills, "Kills to win", "kills", &[50.0], [1.0, 1000.0])],
    }
}

/// The instances of a type (by its suffix) a predicate keeps (upper-case guids).
fn all_of(
    w: &LevelWriter,
    ty: &str,
    keep: impl Fn(&LevelWriter, bc2_formats::dbx::edit::NodeId) -> bool,
) -> Vec<String> {
    w.d.of_type(ty.as_bytes())
        .into_iter()
        .filter(|&e| keep(w, e))
        .map(|e| named(w.d.attr(e, b"guid").unwrap_or_default()))
        .collect()
}

/// The capture points a team can take (flags: `UnableToChangeTeam` false), in the partition's order.
#[must_use]
pub fn flags(w: &LevelWriter) -> Vec<String> {
    all_of(w, CAPTURE_POINT, |w, e| {
        crate::modelayer::bool_field(w, e, b"UnableToChangeTeam") == Some(false)
    })
}

/// The bases (capture points no team can take) of a team.
#[must_use]
pub fn bases(w: &LevelWriter, team: Team) -> Vec<String> {
    all_of(w, CAPTURE_POINT, |w, e| {
        crate::modelayer::bool_field(w, e, b"UnableToChangeTeam") == Some(true)
            && w.d
                .child(e, b"InitialOwnerTeam")
                .and_then(|x| w.d.text(x))
                .is_some_and(|t| t == team.data().as_bytes())
    })
}

fn life_counters(w: &LevelWriter, attacker: bool) -> Vec<String> {
    all_of(w, LIFE_COUNTER, |w, e| {
        crate::modelayer::bool_field(w, e, b"Attacker") == Some(attacker)
    })
}

fn number(w: &LevelWriter, schema: &Schema, guid: &str, field: &str) -> Option<f64> {
    match w.get_field(schema, guid, field).ok()?.1? {
        FieldValue::Int(i) => Some(as_f64(i)),
        FieldValue::Float(x) => Some(x),
        _ => None,
    }
}

/// The rules a level holds now: each value read from the first object that holds it (none where the level has no
/// such object).
#[must_use]
pub fn rules_of(w: &LevelWriter, schema: &Schema) -> Rules {
    let first = |gs: &[String], f: &str| gs.first().and_then(|g| number(w, schema, g, f));
    let counters = all_of(w, TICKET_COUNTER, |_, _| true);
    Rules {
        tickets: first(&counters, "InitialTicketCount").map(whole),
        loss_per_min: first(&counters, "TicketLossPerMin"),
        // a flag's capture time is a Conquest rule (Rush's and Squad Deathmatch's capture points are the mode's own)
        capture_s: if counters.is_empty() {
            None
        } else {
            first(&flags(w), "TimeToGetControl")
        },
        attackers: first(&life_counters(w, true), "StartingLifeCount").map(whole),
        defenders: first(&life_counters(w, false), "StartingLifeCount").map(whole),
        kills: first(&all_of(w, KILL_COUNTER, |_, _| true), "MaxKillCount").map(whole),
    }
}

/// A number set in the type the data gives the field (a whole number or a single).
fn set_number(w: &mut LevelWriter, schema: &Schema, guid: &str, field: &str, v: f64) -> Result<()> {
    let (ty, _) = w.get_field(schema, guid, field)?;
    let value = match ty {
        FieldType::Float => FieldValue::Float(v),
        _ => FieldValue::Int(whole(v)),
    };
    w.set_field(schema, guid, field, &value)
}

/// Sets the round's rules (round E4): each value `to` sets, on every object of the level that holds it (both ticket
/// counters, every flag, the attacking or the defending life counter, every kill counter); the number of fields set.
///
/// # Errors
/// A value is negative or not finite, a value has no object to hold it (a Conquest rule on a level without ticket
/// counters), or a field does not take it.
pub fn set_rules(w: &mut LevelWriter, schema: &Schema, to: &Rules) -> Result<usize> {
    let all = [
        Rule::Tickets,
        Rule::LossPerMin,
        Rule::CaptureS,
        Rule::Attackers,
        Rule::Defenders,
        Rule::Kills,
    ];
    if all.iter().filter_map(|r| to.get(*r)).any(|v| v < 0.0 || !v.is_finite()) {
        return Err(refused("the round's rules are numbers of 0 or more"));
    }
    let mut n = 0;
    for r in all {
        let Some(v) = to.get(r) else { continue };
        let (guids, fields, what): (Vec<String>, &[&str], &str) = match r {
            Rule::Tickets => (
                all_of(w, TICKET_COUNTER, |_, _| true),
                &["InitialTicketCount"],
                "ticket counter",
            ),
            Rule::LossPerMin => (
                all_of(w, TICKET_COUNTER, |_, _| true),
                &["TicketLossPerMin"],
                "ticket counter",
            ),
            Rule::CaptureS => (
                if w.d.of_type(TICKET_COUNTER.as_bytes()).is_empty() {
                    Vec::new()
                } else {
                    flags(w)
                },
                &["TimeToGetControl", "TimeToLoseControl"],
                "Conquest flag",
            ),
            Rule::Attackers => (life_counters(w, true), &["StartingLifeCount"], "attacking life counter"),
            Rule::Defenders => (
                life_counters(w, false),
                &["StartingLifeCount"],
                "defending life counter",
            ),
            Rule::Kills => (all_of(w, KILL_COUNTER, |_, _| true), &["MaxKillCount"], "kill counter"),
        };
        if guids.is_empty() {
            return Err(refused(format!("the level has no {what} to hold that rule")));
        }
        for g in &guids {
            for f in fields {
                set_number(w, schema, g, f, v)?;
                n += 1;
            }
        }
    }
    Ok(n)
}

/// The capture points' letters (the `Sid` of the map marker each names: what the minimap and the HUD show; `A` -
/// `D` on DICE's levels), in the partition's order.
#[must_use]
pub fn letters(w: &LevelWriter) -> Vec<String> {
    let mut out = Vec::new();
    for e in w.d.of_type(CAPTURE_POINT.as_bytes()) {
        let Some(m) = w.d.child(e, b"MapMarker").and_then(|x| w.d.attr(x, b"ref")) else {
            continue;
        };
        let Some(me) = w.find(&named(m)) else { continue };
        if let Some(t) = w.d.child(me, b"Sid").and_then(|x| w.d.text(x))
            && !t.is_empty()
        {
            out.push(String::from_utf8_lossy(t).into_owned());
        }
    }
    out
}

/// The next flag letter: the one after the level's last (`E` after `A` - `D`; `A` on a level without letters).
#[must_use]
pub fn next_letter(w: &LevelWriter) -> char {
    let used: Vec<char> = letters(w)
        .iter()
        .filter_map(|l| l.chars().next())
        .filter(char::is_ascii_uppercase)
        .collect();
    let last = used.iter().max().copied();
    ('A'..='Z')
        .find(|c| !used.contains(c) && last.is_none_or(|l| *c > l))
        .or_else(|| ('A'..='Z').find(|c| !used.contains(c)))
        .unwrap_or('Z')
}

/// The map marker a capture point names (upper-case guid).
#[must_use]
pub fn marker_of(w: &LevelWriter, point: &str) -> Option<String> {
    let e = w.find(point)?;
    let m = w.d.child(e, b"MapMarker").and_then(|x| w.d.attr(x, b"ref"))?;
    let g = named(m);
    (g != "NULL" && w.find(&g).is_some()).then_some(g)
}

// ---------------------------------------------------------------------------------------------------------------
// Round E4: the gameplay tools' operations, made from the level as it is (one operation of the log each).

/// A transform carried with its unit's root: `sub` keeps where it stands against the root when the root goes from
/// `old` to `new` (rows: right, up, forward, position; `sub * old^-1 * new`).
#[must_use]
pub fn carry(old: &Rows, new: &Rows, sub: &Rows) -> Rows {
    let m = |r: &Rows| -> [[f64; 3]; 4] { r.map(|row| row.map(f64::from)) };
    let (o, n, s) = (m(old), m(new), m(sub));
    // the inverse of old's 3 x 3 part (rows right, up, forward)
    let [a, b, c] = [o[0], o[1], o[2]];
    let det =
        a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0]) + a[2] * (b[0] * c[1] - b[1] * c[0]);
    if det.abs() < 1e-12 {
        return *sub;
    }
    let inv = [
        [
            (b[1] * c[2] - b[2] * c[1]) / det,
            (a[2] * c[1] - a[1] * c[2]) / det,
            (a[1] * b[2] - a[2] * b[1]) / det,
        ],
        [
            (b[2] * c[0] - b[0] * c[2]) / det,
            (a[0] * c[2] - a[2] * c[0]) / det,
            (a[2] * b[0] - a[0] * b[2]) / det,
        ],
        [
            (b[0] * c[1] - b[1] * c[0]) / det,
            (a[1] * c[0] - a[0] * c[1]) / det,
            (a[0] * b[1] - a[1] * b[0]) / det,
        ],
    ];
    let mul3 = |v: [f64; 3], m: &[[f64; 3]; 3]| -> [f64; 3] {
        let [x, y, z] = v;
        let [r0, r1, r2] = m;
        [
            x * r0[0] + y * r1[0] + z * r2[0],
            x * r0[1] + y * r1[1] + z * r2[1],
            x * r0[2] + y * r1[2] + z * r2[2],
        ]
    };
    let n3 = [n[0], n[1], n[2]];
    // a direction row: into old's frame, then out of new's
    let dir = |v: [f64; 3]| mul3(mul3(v, &inv), &n3);
    let p = {
        let rel = [s[3][0] - o[3][0], s[3][1] - o[3][1], s[3][2] - o[3][2]];
        let q = dir(rel);
        [q[0] + n[3][0], q[1] + n[3][1], q[2] + n[3][2]]
    };
    #[allow(clippy::cast_possible_truncation, reason = "transforms are singles")]
    let f = |v: [f64; 3]| v.map(|x| x as f32);
    [f(dir(s[0])), f(dir(s[1])), f(dir(s[2])), f(p)]
}

/// The copies of a unit's members for a placement of its root at `at`: new guids from `guid`, each member with a
/// transform carried with the root.
fn sub_copies(w: &LevelWriter, unit: &[String], at: &Rows, guid: &dyn Fn(&str) -> String) -> Vec<SubCopy> {
    let Some(old) = unit.first().and_then(|r| w.transform(r)) else {
        return Vec::new();
    };
    unit.iter()
        .skip(1)
        .map(|s| SubCopy {
            source: s.clone(),
            guid: guid(s),
            transform: w.transform(s).map(|t| carry(&old, at, &t)),
        })
        .collect()
}

/// The flag a new flag copies: the flag with the smallest unit (the fewest vehicles and spawns), the first of
/// equals.
#[must_use]
pub fn flag_template(w: &LevelWriter) -> Option<String> {
    flags(w).into_iter().min_by_key(|f| unit(w, f).len())
}

fn text_field(field: &str, guid: &str, to: &str) -> FieldSet {
    FieldSet {
        guid: guid.to_owned(),
        field: field.to_owned(),
        from: None,
        to: FieldValue::Text(to.to_owned()),
    }
}

fn partition(op: Op) -> FileEdit {
    FileEdit::Partition { op: Box::new(op) }
}

/// A flag added (round E4): a copy of [`flag_template`]'s unit at `at` (its zone, marker, spawners and spawn points
/// carried with it; its team filters, capture messages and voice-over its own; wired to the level's shared logic as
/// the template is), owned by `team`, lettered with [`next_letter`] (its name, its marker's `Sid`), its `Index` the
/// level's highest plus one. One operation: the placement and the marker's letter. Returns it with the new flag's
/// guid.
///
/// # Errors
/// The level has no flag to copy.
pub fn add_flag(w: &LevelWriter, at: &Rows, team: Team, guid: &dyn Fn(&str) -> String) -> Result<(Op, String)> {
    let template = flag_template(w).ok_or_else(|| refused("the level has no flag to copy a new one from"))?;
    let members = unit(w, &template);
    let owned = sub_copies(w, &members, at, guid);
    let letter = next_letter(w);
    let name = format!("{letter}: Flag {letter}");
    let index =
        w.d.of_type(CAPTURE_POINT.as_bytes())
            .into_iter()
            .filter_map(|e| w.d.child(e, b"Index"))
            .filter_map(|x| {
                let (size, vals) = bc2_formats::dbx::edit::numbers_of(w.d.raw(x))?;
                (size == 4 && vals.len() == 1)
                    .then(|| {
                        vals.first()
                            .and_then(|v| <[u8; 4]>::try_from(*v).ok())
                            .map(i32::from_be_bytes)
                    })
                    .flatten()
            })
            .max()
            .map_or(0, |m| i64::from(m) + 1);
    let new = guid(&template);
    let mut fields: Vec<FieldIs> = ["Name", "LocationName", "ControlPointName"]
        .into_iter()
        .filter(|f| w.find(&template).is_some_and(|e| w.d.child(e, f.as_bytes()).is_some()))
        .map(|f| FieldIs {
            field: f.to_owned(),
            value: FieldValue::Text(name.clone()),
        })
        .collect();
    fields.push(FieldIs {
        field: "InitialOwnerTeam".into(),
        value: FieldValue::Text(team.data().to_owned()),
    });
    if w.find(&template).is_some_and(|e| w.d.child(e, b"Index").is_some()) {
        fields.push(FieldIs {
            field: "Index".into(),
            value: FieldValue::Int(index),
        });
    }
    let mut edits = vec![partition(Op::PlaceUnit {
        item: Placed {
            guid: new.clone(),
            template: template.clone(),
            blueprint: String::new(),
            ref_field: None,
            owned: owned.clone(),
            transform: *at,
            fields,
            placement: None,
        },
    })];
    // the marker's letter (the minimap's and the HUD's) and name
    if let Some(m) = marker_of(w, &template)
        && let Some(copy) = owned.iter().find(|o| o.source == m)
    {
        let mut items = vec![text_field("Sid", &copy.guid, &letter.to_string())];
        if w.find(&m).is_some_and(|e| w.d.child(e, b"Name").is_some()) {
            items.push(text_field("Name", &copy.guid, &name));
        }
        edits.push(partition(Op::SetField { items }));
    }
    Ok((Op::Files { edits }, new))
}

/// A spawn point added to a soldier spawner (round E4): a copy of one of its points (else of any spawn point of the
/// level) at `at`, linked into the spawner's `AlternativeSpawnPoints` (GAME-SPAWN-1). One operation; returns it with
/// the new point's guid.
///
/// # Errors
/// The spawner is not a soldier spawner, or the level has no spawn point to copy.
pub fn add_spawn_point(
    w: &LevelWriter,
    spawner: &str,
    at: &Rows,
    guid: &dyn Fn(&str) -> String,
) -> Result<(Op, String)> {
    of_type(w, spawner, SOLDIER_SPAWN, "a soldier spawner")?;
    let template = points(w, spawner)
        .into_iter()
        .find(|p| w.transform(p).is_some())
        .or_else(|| {
            all_of(w, ALTERNATE_SPAWN, |w, e| w.d.child(e, b"Transform").is_some())
                .into_iter()
                .next()
        })
        .ok_or_else(|| refused("the level has no spawn point to copy a new one from"))?;
    let new = guid(&template);
    let place = Op::Place {
        items: vec![Placed {
            guid: new.clone(),
            template,
            blueprint: String::new(),
            ref_field: None,
            owned: Vec::new(),
            transform: *at,
            fields: Vec::new(),
            placement: None,
        }],
    };
    let link = Op::LinkPoint {
        spawner: spawner.to_ascii_uppercase(),
        point: new.clone(),
        linked: true,
    };
    Ok((
        Op::Files {
            edits: vec![partition(place), partition(link)],
        },
        new,
    ))
}

/// The stage counters' `TriggerAmount`s set to `count(m-coms reaching it)` (GAME-RUSH-10).
fn recount(w: &LevelWriter, schema: &Schema, counters: &[(String, usize)]) -> Result<Op> {
    let mut items = Vec::new();
    for (c, n) in counters {
        let (ty, from) = w.get_field(schema, c, "TriggerAmount")?;
        #[allow(clippy::cast_precision_loss, reason = "a handful of M-COMs")]
        let to = match ty {
            FieldType::Float => FieldValue::Float(*n as f64),
            _ => FieldValue::Int(i64::try_from(*n).unwrap_or(i64::MAX)),
        };
        items.push(FieldSet {
            guid: c.clone(),
            field: "TriggerAmount".into(),
            from,
            to,
        });
    }
    Ok(Op::SetField { items })
}

/// The Rush stage (its M-COMs) an M-COM is in.
#[must_use]
pub fn stage_of(w: &LevelWriter, mcom: &str) -> Option<Vec<String>> {
    let m = mcom.to_ascii_uppercase();
    crate::modelayer::stages(w).into_iter().find(|s| s.contains(&m))
}

/// An M-COM added to a Rush stage (round E4): a copy of the stage's first M-COM with its unit (interactions,
/// marker, alarm, charge trigger, objective props) at `at`, wired to the stage's counters and the announcer as it
/// is, and every counter of the stage counting one more (GAME-RUSH-10). One operation; returns it with the new
/// M-COM's guid.
///
/// # Errors
/// The stage has no M-COM, or a counter's `TriggerAmount` does not read.
pub fn add_mcom(
    w: &LevelWriter,
    schema: &Schema,
    stage: &[String],
    at: &Rows,
    guid: &dyn Fn(&str) -> String,
) -> Result<(Op, String)> {
    let template = stage
        .first()
        .ok_or_else(|| refused("the stage has no M-COM to copy a new one from"))?
        .to_ascii_uppercase();
    let members = unit(w, &template);
    let owned = sub_copies(w, &members, at, guid);
    let new = guid(&template);
    let place = Op::PlaceUnit {
        item: Placed {
            guid: new.clone(),
            template: template.clone(),
            blueprint: String::new(),
            ref_field: None,
            owned,
            transform: *at,
            fields: Vec::new(),
            placement: None,
        },
    };
    // the new M-COM reaches the counters its template reaches: they count one more
    let counters: Vec<(String, usize)> = stage_counters(w, stage)
        .into_iter()
        .map(|(c, by)| {
            let n = by.len() + usize::from(by.contains(&template));
            (c, n)
        })
        .collect();
    let mut edits = vec![partition(place)];
    if !counters.is_empty() {
        edits.push(partition(recount(w, schema, &counters)?));
    }
    Ok((Op::Files { edits }, new))
}

/// The members of a unit no other gameplay object's unit holds (what a delete takes: a marker or interaction two
/// M-COMs share stays).
fn exclusive(w: &LevelWriter, members: &[String]) -> Vec<String> {
    let Some(root) = members.first() else { return Vec::new() };
    let mut others: std::collections::HashSet<String> = std::collections::HashSet::new();
    let roots = all_of(w, CAPTURE_POINT, |_, _| true)
        .into_iter()
        .chain(all_of(w, SOLDIER_SPAWN, |_, _| true))
        .chain(crate::modelayer::stages(w).into_iter().flatten());
    for r in roots {
        if r != *root && !members.contains(&r) {
            others.extend(crate::modelayer::cluster(w, &r));
        }
    }
    members.iter().filter(|m| !others.contains(*m)).cloned().collect()
}

/// A gameplay object deleted with its unit (round E4): a flag with its zone, marker, spawners and spawn points and
/// its announcement logic; a soldier spawner with its spawn points; a spawn point alone; an M-COM with its
/// interactions, marker and alarm, its stage's counters counting one fewer (GAME-RUSH-10). What another object's unit
/// holds stays. One operation.
///
/// # Errors
/// The object is not there, it is a team's last base or the level's last flag (Conquest keeps both: GAME-LAYER-4),
/// or the last M-COM of its stage (taking a stage out is not one of the tools yet).
pub fn delete_unit(w: &LevelWriter, schema: &Schema, guid: &str) -> Result<Op> {
    let g = guid.to_ascii_uppercase();
    let e = w.find(&g).ok_or_else(|| LevelWriteError::NotFound(g.clone()))?;
    let ty = w.type_of(e);
    if ty.ends_with(CAPTURE_POINT) {
        let team = team_of(w, &g);
        if let Some(t) = team.filter(|t| *t != Team::TeamNeutral)
            && bases(w, t) == [g.clone()]
        {
            return Err(refused(format!("{g} is {}'s last base: a team keeps one", t.word())));
        }
        let ticketed = !w.d.of_type(TICKET_COUNTER.as_bytes()).is_empty();
        if ticketed && flags(w) == [g.clone()] {
            return Err(refused(format!("{g} is the level's last flag: Conquest keeps one")));
        }
    }
    if let Some(stage) = stage_of(w, &g) {
        if stage.len() <= 1 {
            return Err(refused(format!(
                "{g} is its stage's last M-COM: a stage keeps one (taking a whole stage out is not a tool yet)"
            )));
        }
        let counters: Vec<(String, usize)> = stage_counters(w, &stage)
            .into_iter()
            .filter(|(_, by)| by.contains(&g))
            .map(|(c, by)| (c, by.len() - 1))
            .collect();
        let delete = Op::Delete {
            guids: exclusive(w, &unit(w, &g)),
        };
        let mut edits = vec![partition(recount(w, schema, &counters)?), partition(delete)];
        if counters.is_empty() {
            edits.remove(0);
        }
        return Ok(Op::Files { edits });
    }
    let members = if ty.ends_with(ALTERNATE_SPAWN) {
        vec![g]
    } else {
        exclusive(w, &unit(w, &g))
    };
    Ok(Op::Delete { guids: members })
}

/// A unit carried by the cursor (round E4): its root to `to`, every member with a transform moved as far, each
/// keeping its height above the ground (`ground(x, z)`; a member without ground under it keeps its height step).
#[must_use]
pub fn carry_unit(w: &LevelWriter, guid: &str, to: [f32; 3], ground: &dyn Fn(f64, f64) -> Option<f64>) -> Vec<Moved> {
    let members = crate::modelayer::cluster(w, guid);
    let Some(origin) = w.transform(guid).map(|r| r[3]) else {
        return Vec::new();
    };
    let d = [to[0] - origin[0], to[1] - origin[1], to[2] - origin[2]];
    members
        .iter()
        .filter_map(|m| {
            let from = w.transform(m)?;
            let [x, y, z] = from[3];
            let (nx, nz) = (x + d[0], z + d[2]);
            let ny = if m.eq_ignore_ascii_case(guid) {
                to[1]
            } else {
                match (ground(f64::from(x), f64::from(z)), ground(f64::from(nx), f64::from(nz))) {
                    #[allow(clippy::cast_possible_truncation, reason = "transforms are singles")]
                    (Some(a), Some(b)) => (f64::from(y) - a + b) as f32,
                    _ => y + d[1],
                }
            };
            let mut rows = from;
            rows[3] = [nx, ny, nz];
            Some(Moved {
                guid: m.clone(),
                from,
                to: rows,
                placement: None,
            })
        })
        .collect()
}
