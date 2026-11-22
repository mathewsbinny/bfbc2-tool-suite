//! Typed gameplay operations (round XF1, audit AU1 D18 / R23): edits E4's gameplay tools make, in words the History
//! can show and with the data's rules checked when they apply, instead of `SetField`s with raw references.
//!
//! - [`Team`] set on an object: a capture point's `InitialOwnerTeam`, any other object's `Team` (LEVEL-ENT-1).
//! - A soldier spawner linked to or unlinked from a capture point: an item of the point's `Spawners` added or
//!   removed (GAME-SPAWN-1).
//! - A team's tickets: a ticket counter's `InitialTicketCount` and `TicketLossPerMin` (GAME-TKT-1, GAME-TKT-2).
//! - An outline point inserted after another or removed, the `Next` chain kept closed (LEVEL-ENT-2): a combat area's or
//!   a capture zone's polygon edited one vertex at a time.

use serde::{Deserialize, Serialize};

use crate::ops::{FieldValue, Rows};
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
    let a = w.d.child(p, b"Spawners").ok_or_else(|| LevelWriteError::NoField {
        guid: point.to_owned(),
        field: "Spawners".into(),
    })?;
    let g = spawner.as_bytes();
    let items: Vec<_> = w.d.children(a).unwrap_or_default().to_vec();
    let at = items
        .iter()
        .position(|&i| w.d.attr(i, b"ref").is_some_and(|r| refers_to(r, g)));
    match (linked, at) {
        (true, Some(_)) => Err(refused(format!("{spawner} is already one of {point}'s spawners"))),
        (false, None) => Err(refused(format!("{spawner} is not one of {point}'s spawners"))),
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
            let form =
                form.ok_or_else(|| refused("the level has no reference item to copy the new item's form from"))?;
            let it = w.d.clone_node(form);
            w.d.set_ref(it, spawner.to_ascii_lowercase().as_bytes())?;
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
