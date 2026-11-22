//! Vehicle spawners on the level's ground (round A2): what a vehicle needs around its spawner to be of use - its room
//! (XVAL-PLACE-3, round A1), a drivable way out for a land vehicle, water access for a boat, an open launch area for a
//! helicopter or a plane - each bounded below the tightest of DICE's own spawners of the class on the stock levels
//! (`rust/BENCHMARKS.md` "A2"), and the spawners of a partition with their teams and the capture points that own
//! them, as the auto layout ([`crate::autolayout`]) and the export's validation ([`crate::validate::site_rules`]) read
//! them.
#![allow(clippy::cast_precision_loss, reason = "counts as f64 for means: far below 2^52")]

use bc2_formats::dbx::edit::NodeId;

use crate::mpconvert::{guid_of, local_ref};
use crate::ops::Rows;
use crate::site::Site;
use crate::write::LevelWriter;

/// How far a land vehicle must be able to drive from its spawner (metres in plan). DICE's 184 land vehicles on the 27
/// stock levels measured all drive 40 m or more (`rust/BENCHMARKS.md` "A2").
pub const WAY_OUT: f64 = 20.0;
/// How far a boat must be able to sail from its spawner (metres in plan), judged where the level's water mesh lies near
/// the ground (as XVAL-PLACE-3's domain): DICE's tightest of 44 judged boats sails 30.5 m (`mp_009gr`).
pub const WATER_REACH: f64 = 24.0;
/// How high the sky over a helicopter's spawner must be open (metres above it, within [`ROTOR`] of it in plan):
/// DICE's 49 helicopters have none of their static collision below 60 m within 3.5 m.
pub const SKY: f64 = 12.0;
/// The radius the open sky is looked for in (metres). One of DICE's helipads (`nam_mp_005r`) has collision 3.1 m above
/// it 7 m out (a main rotor's radius), so the rule looks within half of that.
pub const ROTOR: f64 = 3.5;
/// How long a straight take-off run a plane's spawner must have (metres): DICE's and OPFOR's 15 WW2 planes have
/// 34 m (`guadal_canal`) or more on some bearing.
pub const RUN: f64 = 30.0;
/// A plane's half span for the run (metres).
pub const HALF_SPAN: f64 = 5.0;
/// How far the measures look past a rule's bound (metres): a flood fill's farthest cell is not monotonic in its limit
/// (a cell on the limit's circle may be reached only round a corner), so each rule measures out to its bound plus
/// this and compares.
const LOOK_PAST: f64 = 16.0;

/// What a class of vehicle needs to leave its spawner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Leave {
    /// A drivable way out ([`WAY_OUT`]).
    Drive,
    /// Water to sail ([`WATER_REACH`]).
    Sail,
    /// Open sky ([`SKY`]).
    Lift,
    /// A take-off run ([`RUN`]).
    TakeOff,
}

/// How a vehicle of an L1 class leaves its spawner (`None`: an emplacement, a UAV station or the ambient backdrop,
/// which stand where DICE built them in).
#[must_use]
pub fn leave(class: &str) -> Option<Leave> {
    match class {
        "tank" | "car" | "ifv" | "ifv_aa" | "quad" => Some(Leave::Drive),
        "boat" => Some(Leave::Sail),
        "heli" => Some(Leave::Lift),
        "plane" => Some(Leave::TakeOff),
        _ => None,
    }
}

/// A vehicle spawner of a partition.
#[derive(Debug, Clone, PartialEq)]
pub struct Spawner {
    /// Its guid (upper case).
    pub guid: String,
    /// Its node.
    pub node: NodeId,
    /// L1's classes of the vehicles it makes.
    pub classes: Vec<&'static str>,
    /// Its `Team`.
    pub team: String,
    /// Its transform.
    pub rows: Rows,
    /// The capture point whose `Spawners` list it is in: (guid, a team's base, its `InitialOwnerTeam`).
    pub owner: Option<(String, bool, String)>,
    /// Its `AutoSpawn` (`None`: the field is not there).
    pub auto_spawn: Option<bool>,
    /// Its `Enabled`.
    pub enabled: Option<bool>,
}

impl Spawner {
    /// The first class a vehicle of it leaves by, with how (`None`: none of its vehicles moves).
    #[must_use]
    pub fn moving(&self) -> Option<(&'static str, Leave)> {
        self.classes.iter().find_map(|c| leave(c).map(|l| (*c, l)))
    }
}

fn text(w: &LevelWriter, e: NodeId, f: &[u8]) -> String {
    w.d.child(e, f)
        .and_then(|x| w.d.text(x))
        .map(|t| String::from_utf8_lossy(t).into_owned())
        .unwrap_or_default()
}

/// Every vehicle spawner of a partition with a transform, in the partition's order.
#[must_use]
pub fn spawners(w: &LevelWriter) -> Vec<Spawner> {
    // the capture points' Spawners lists
    let mut owner: std::collections::HashMap<String, (String, bool, String)> = std::collections::HashMap::new();
    for e in w.d.of_type(b".CapturePointEntityData") {
        let cp = guid_of(&w.d, e);
        let base =
            w.d.child(e, b"UnableToChangeTeam")
                .is_some_and(|x| w.d.raw(x).get(1).is_some_and(|&b| b != 0));
        let team = text(w, e, b"InitialOwnerTeam");
        for &i in
            w.d.child(e, b"Spawners")
                .and_then(|a| w.d.children(a))
                .unwrap_or_default()
        {
            if let Some(g) = w.d.attr(i, b"ref").and_then(local_ref) {
                owner.entry(g).or_insert_with(|| (cp.clone(), base, team.clone()));
            }
        }
    }
    crate::placement::vehicle_spawners(w)
        .into_iter()
        .filter_map(|(guid, node, classes)| {
            let rows = w.transform_at(node)?;
            Some(Spawner {
                owner: owner.get(&guid).cloned(),
                auto_spawn: crate::modelayer::bool_field(w, node, b"AutoSpawn"),
                enabled: crate::modelayer::bool_field(w, node, b"Enabled"),
                team: text(w, node, b"Team"),
                guid,
                node,
                classes,
                rows,
            })
        })
        .collect()
}

/// What stands around a vehicle spawner, measured (the calibration's and the rules' numbers).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Around {
    /// The clear radius at a vehicle's height ([`Site::clearance`], up to 8 m).
    pub clearance: f64,
    /// How far a land vehicle drives from it ([`Site::way_out`], up to 40 m).
    pub way_out: f64,
    /// How far a boat sails from it ([`Site::water_reach`], up to 100 m).
    pub water_reach: f64,
    /// The open sky over it ([`Site::open_sky`] within [`ROTOR`], up to 60 m).
    pub sky: f64,
    /// The longest take-off run from it ([`Site::runway`], up to 300 m) and its bearing.
    pub run: (f64, f64),
}

/// Everything [`Around`] measures at `p`.
#[must_use]
pub fn around(site: &Site, p: [f64; 3]) -> Around {
    Around {
        clearance: site.clearance(p, 8.0),
        way_out: site.way_out(p, 40.0),
        water_reach: site.water_reach(p, 100.0),
        sky: site.open_sky(p, ROTOR, 60.0),
        run: site.runway(p, HALF_SPAN, 300.0),
    }
}

/// Whether the level's water mesh is judged at `p`: it lies near the ground there (within 10 m below it), as
/// XVAL-PLACE-3 judges a boat's domain (Vietnam's rivers the mesh does not hold are not seen).
#[must_use]
pub fn water_judged(site: &Site, p: [f64; 3]) -> bool {
    let (surface, ground) = (site.water(p[0], p[2]), site.ground(p[0], p[2]));
    surface.is_some_and(|s| ground.is_none_or(|g| s > g - 10.0))
}

/// Why a vehicle of `class` at `p` cannot leave its spawner (XVAL-PLACE-6 to 8), in words with the rule's code;
/// `None`: it can (or its class stands where it was built in, or a boat where the water mesh is not judged).
#[must_use]
pub fn cannot_leave(site: &Site, class: &str, p: [f64; 3]) -> Option<(&'static str, String)> {
    match leave(class)? {
        Leave::Drive => {
            let d = site.way_out(p, WAY_OUT + LOOK_PAST);
            (d < WAY_OUT).then(|| {
                (
                    "XVAL-PLACE-6",
                    format!("it can drive only {d:.0} m from its spawner ({WAY_OUT:.0} m needed)"),
                )
            })
        }
        Leave::Sail => {
            if !water_judged(site, p) {
                return None;
            }
            let d = site.water_reach(p, WATER_REACH + LOOK_PAST);
            (d < WATER_REACH).then(|| {
                (
                    "XVAL-PLACE-7",
                    format!("it can sail only {d:.0} m from its spawner ({WATER_REACH:.0} m of open water needed)"),
                )
            })
        }
        Leave::Lift => {
            let s = site.open_sky(p, ROTOR, SKY);
            (s < SKY).then(|| {
                (
                    "XVAL-PLACE-8",
                    format!(
                        "static collision {s:.1} m over its spawner within {ROTOR:.0} m ({SKY:.0} m of open sky needed)"
                    ),
                )
            })
        }
        Leave::TakeOff => {
            let (r, _) = site.runway(p, HALF_SPAN, RUN + LOOK_PAST);
            (r < RUN).then(|| {
                (
                    "XVAL-PLACE-8",
                    format!("its longest straight take-off run is {r:.0} m ({RUN:.0} m needed)"),
                )
            })
        }
    }
}

// ------------------------------------------------------------------------------------------------ the auto layout's vehicles

/// DICE's vehicle counts per mode (L1's statistics, `l1_a1_params.json`, the medians): a Conquest base's pool (4 on
/// BFBC2, BFBC1 and Vietnam, 8 on WW2) and a flag's (2: one per team, or one a neutral spawner makes for its owner);
/// Rush's attackers' and defenders' per stage (3 and 2); Squad Rush's none; Squad Deathmatch's one (DICE's an IFV
/// every time).
#[must_use]
pub fn dice_counts(mode: crate::placement::Mode, profile: crate::project::Profile) -> [usize; 2] {
    use crate::placement::Mode;
    match (mode, profile) {
        (Mode::Conquest, crate::project::Profile::Ww2) => [8, 2],
        (Mode::Conquest, _) => [4, 2],
        (Mode::Rush, _) => [3, 2],
        (Mode::SquadRush, _) => [0, 0],
        (Mode::SquadDeathmatch, _) => [1, 0],
    }
}

/// The classes a base's, a flag's and Squad Deathmatch's vehicles are taken in, DICE's most common first (L1's class
/// totals: a Conquest base's tanks 27, quads 24, IFVs 19, cars 12, helicopters 12, boats 4; a flag's IFVs 18, tanks
/// 16, cars 13, boats 12, quads 9, helicopters 9; Squad Deathmatch's IFV).
const BASE_ORDER: &[&str] = &["tank", "quad", "ifv", "car", "heli", "boat", "ifv_aa", "plane"];
const FLAG_ORDER: &[&str] = &["ifv", "tank", "car", "boat", "quad", "heli", "ifv_aa", "plane"];
const SQUAD_ORDER: &[&str] = &["ifv", "tank", "car", "quad"];

/// The nearest a vehicle spawner the auto layout places stands to another vehicle spawner (metres; more for a large
/// room): DICE's tanks stand 4.8 m apart side by side.
const SPACING: f64 = 6.0;
/// How near a vehicle spawner may stand to a soldier spawn place (metres).
const OFF_SPAWNS: f64 = 4.0;

/// A vehicle the auto layout placed.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Vehicle {
    /// The spawner: one of the level's own, moved, or a new copy of one.
    pub guid: String,
    /// Its class.
    pub class: String,
    /// The capture point (base, flag, squad) whose `Spawners` it joins.
    pub owner: String,
    /// Its team.
    pub team: String,
    /// A new copy (else the level's own spawner, moved).
    pub copy: bool,
    /// Metres from its capture point.
    pub distance: f64,
}

/// The auto layout's vehicles: the spawners moved, the copies placed, the capture points they join.
#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Plan {
    /// The level's own spawners moved.
    pub moves: Vec<crate::ops::Moved>,
    /// New copies of them.
    pub places: Vec<crate::ops::Placed>,
    /// (capture point, spawner) links made.
    pub links: Vec<(String, String)>,
    /// Every vehicle placed.
    pub vehicles: Vec<Vehicle>,
    /// What could not be placed, in words.
    pub notes: Vec<String>,
}

/// What a capture point of the layer asks for.
struct Want {
    cp: String,
    at: [f64; 3],
    team: String,
    n: usize,
    order: &'static [&'static str],
    /// A flag: a neutral spawner, else one per team.
    flag: bool,
}

fn xz(p: [f64; 3]) -> [f64; 2] {
    [p[0], p[2]]
}

fn d2(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

fn rows_xz(r: &Rows) -> [f64; 2] {
    [f64::from(r[3][0]), f64::from(r[3][2])]
}

/// The capture points of the layer and what each asks for, per mode.
fn wants(
    w: &LevelWriter,
    layer: &std::collections::HashSet<String>,
    mode: crate::placement::Mode,
    counts: [usize; 2],
) -> Vec<Want> {
    use crate::placement::Mode;
    type Cp = (String, [f64; 3], bool, String, Option<bool>);
    let cps: Vec<Cp> =
        w.d.of_type(b".CapturePointEntityData")
            .into_iter()
            .filter_map(|e| {
                let g = guid_of(&w.d, e);
                if !layer.contains(&g) {
                    return None;
                }
                let base =
                    w.d.child(e, b"UnableToChangeTeam")
                        .is_some_and(|x| w.d.raw(x).get(1).is_some_and(|&b| b != 0));
                let p = w.transform_at(e)?[3].map(f64::from);
                Some((
                    g,
                    p,
                    base,
                    text(w, e, b"InitialOwnerTeam"),
                    crate::modelayer::bool_field(w, e, b"Enabled"),
                ))
            })
            .collect();
    let want = |c: &Cp, team: &str, n: usize, order: &'static [&'static str], flag: bool| Want {
        cp: c.0.clone(),
        at: c.1,
        team: team.to_owned(),
        n,
        order,
        flag,
    };
    let mut out = Vec::new();
    match mode {
        Mode::Conquest => {
            for c in &cps {
                if c.2 && matches!(c.3.as_str(), "Team1" | "Team2") {
                    out.push(want(c, &c.3, counts[0], BASE_ORDER, false));
                } else if !c.2 {
                    out.push(want(c, "TeamNeutral", counts[1], FLAG_ORDER, true));
                }
            }
        }
        Mode::Rush => {
            // the bases played from the start (stage 1's): a later stage's vehicles are enabled by the stage's events,
            // which the layout does not make
            let (stages, _) = crate::modelayer::rush_play(w);
            let attacker =
                w.d.of_type(b".LifeCounterEntityData")
                    .into_iter()
                    .find(|&e| crate::modelayer::bool_field(w, e, b"Attacker") == Some(true))
                    .map_or_else(|| "Team1".to_owned(), |e| text(w, e, b"TeamId"));
            let first = stages.first();
            for c in &cps {
                let on = first.and_then(|s| s.enabled.get(&c.0).copied()).or(c.4).unwrap_or(true);
                if !on || !matches!(c.3.as_str(), "Team1" | "Team2") {
                    continue;
                }
                let n = if c.3 == attacker { counts[0] } else { counts[1] };
                out.push(want(c, &c.3, n, BASE_ORDER, false));
            }
        }
        Mode::SquadRush => {}
        Mode::SquadDeathmatch => {
            // DICE's one IFV belongs to the layer's neutral capture point on every stock level, else the first squad's
            if let Some(c) = cps.iter().find(|c| c.3 == "TeamNeutral").or_else(|| cps.first()) {
                out.push(want(c, "TeamNeutral", counts[0], SQUAD_ORDER, false));
            }
        }
    }
    out
}

/// A transform at `p` facing `forward` (in plan): the rows right, up, forward, translation.
fn facing(p: [f64; 3], forward: [f64; 2]) -> Rows {
    let l = forward[0].hypot(forward[1]);
    let (fx, fz) = if l > 1e-9 {
        (forward[0] / l, forward[1] / l)
    } else {
        (0.0, 1.0)
    };
    [
        [fz as f32, 0.0, (-fx) as f32],
        [0.0, 1.0, 0.0],
        [fx as f32, 0.0, fz as f32],
        [p[0] as f32, p[1] as f32, p[2] as f32],
    ]
}

/// What a spot must keep clear of: the combat areas it must be in, the capture points (its own must be the nearest),
/// the vehicle spawners already standing and the soldiers' spawn places.
struct Keep<'a> {
    polys: &'a [Vec<[f64; 2]>],
    anchors: &'a [[f64; 2]],
    taken: &'a [[f64; 2]],
    spawns: &'a [[f64; 2]],
}

/// Where a vehicle of `class` can stand for the capture point at `at`: the first good spot of rings 12 to 88 m out
/// (boats 8 to 148 m, aircraft from 20 m), each ring walked from the bearing to `toward`: on its domain (land: a
/// slope up to 15 degrees, no water over 0.3 m; a boat: water 0.4 to 30 m deep where the mesh is judged) with its
/// room, able to leave ([`cannot_leave`]), inside a combat area, its capture point the nearest of all within 150 m
/// (its pool, L1's), clear of the other vehicles and of the soldiers' spawn places. The spawner stands `above` the
/// surface (its template's own height over its surface).
fn spot(site: &Site, class: &str, at: [f64; 3], toward: [f64; 2], keep: &Keep<'_>, above: f64) -> Option<Rows> {
    let how = leave(class)?;
    let room = crate::validate::vehicle_room(class).map_or(0.5, |r| r.0);
    let (first, last) = match how {
        Leave::Sail => (8.0, 148.0),
        Leave::Lift | Leave::TakeOff => (20.0, 88.0),
        Leave::Drive => (12.0, 88.0),
    };
    let base = (toward[1] - at[2]).atan2(toward[0] - at[0]);
    let home = xz(at);
    let mut r: f64 = first;
    while r <= last {
        let n = ((std::f64::consts::TAU * r / 6.0).ceil() as usize).clamp(12, 48);
        let mut order: Vec<(f64, f64)> = (0..n)
            .map(|k| {
                let a = (k as f64) * std::f64::consts::TAU / n as f64;
                let off = (a + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU) - std::f64::consts::PI;
                (off.abs(), base + a)
            })
            .collect();
        order.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.total_cmp(&y.1)));
        for (_, a) in order {
            let q = [at[0] + r * a.cos(), at[2] + r * a.sin()];
            // the cheap checks first
            if crate::validate::outside_by(q, keep.polys) > 0.0
                || keep.taken.iter().any(|t| d2(*t, q) < SPACING.max(2.0 * room + 1.0))
                || keep.spawns.iter().any(|s| d2(*s, q) < OFF_SPAWNS)
                || keep.anchors.iter().any(|o| d2(*o, q) + 1e-6 < d2(home, q))
                || d2(home, q) > 150.0
            {
                continue;
            }
            let Some(g) = site.ground(q[0], q[1]) else { continue };
            let p = if how == Leave::Sail {
                let Some(wl) = site.water(q[0], q[1]) else { continue };
                let under = site.support([q[0], wl, q[1]]).unwrap_or(g);
                // over the terrain too (XVAL-PLACE-3 calls a boat whose ground stands above the water "on dry land")
                if !(0.4..=30.0).contains(&(wl - under)) || wl <= g || !water_judged(site, [q[0], wl, q[1]]) {
                    continue;
                }
                [q[0], wl + above.clamp(0.0, 1.0), q[1]]
            } else {
                if site.slope(q[0], q[1]).is_none_or(|s| s > 15.0)
                    || site.water(q[0], q[1]).is_some_and(|wl| wl - g > 0.3)
                {
                    continue;
                }
                let y = site.stand([q[0], g + 0.2, q[1]]);
                let p = [q[0], y + above.clamp(0.0, 3.0), q[1]];
                if site.spot(p) != crate::site::Spot::Ground {
                    continue;
                }
                p
            };
            if site.buried([p[0], p[1] + 1.0, p[2]]).is_some()
                || site.clearance(p, room + 0.5) < room + 0.5
                || cannot_leave(site, class, p).is_some()
            {
                continue;
            }
            let forward = if how == Leave::TakeOff {
                let (_, b) = site.runway(p, HALF_SPAN, RUN + LOOK_PAST);
                [b.cos(), b.sin()]
            } else {
                [toward[0] - p[0], toward[1] - p[2]]
            };
            return Some(facing(p, forward));
        }
        r += 4.0;
    }
    None
}

/// The auto layout's vehicles for the layer of `w` (the layout applied): each base, flag or squad gets DICE's count of
/// vehicles ([`dice_counts`]), taken from the level's own vehicle spawners that no capture point of the layer owns
/// (DICE's for this level: the base's own layer has gone) and that spawn by themselves (`AutoSpawn`: events start the
/// others), by team (a flag: a neutral spawner, else one of each team) and class in DICE's order - each spawner moved
/// once, then copies of it - each on a spot of its own ([`spot`]) and joined to its capture point's `Spawners`.
/// `pinned` spawners stay where they stand and count for the capture point nearest them; `tag` names the copies'
/// guids (the layout's seed and the partition's size).
#[must_use]
#[allow(clippy::too_many_lines, reason = "the wants, the palette and the spots in one pass")]
pub fn plan(
    w: &LevelWriter,
    site: &Site,
    mode: crate::placement::Mode,
    profile: crate::project::Profile,
    pinned: &std::collections::BTreeSet<String>,
    tag: &str,
) -> Plan {
    let mut out = Plan::default();
    let layer = crate::modelayer::layer(w);
    let mut wants = wants(w, &layer, mode, dice_counts(mode, profile));
    if wants.is_empty() {
        return out;
    }
    let all = spawners(w);
    let polys = crate::validate::combat_outlines(w);
    let anchors: Vec<[f64; 2]> = wants.iter().map(|x| xz(x.at)).collect();
    let centre = {
        let n = anchors.len() as f64;
        [
            anchors.iter().map(|a| a[0]).sum::<f64>() / n,
            anchors.iter().map(|a| a[1]).sum::<f64>() / n,
        ]
    };
    let spawns: Vec<[f64; 2]> =
        w.d.of_type(b".AlternateSpawnEntityData")
            .into_iter()
            .filter_map(|e| w.transform_at(e))
            .map(|r| rows_xz(&r))
            .collect();
    // the palette: the level's own spawners of vehicles that move, no capture point of the layer's, spawning by
    // themselves, not pinned
    let palette: Vec<&Spawner> = all
        .iter()
        .filter(|s| {
            s.moving().is_some()
                && !layer.contains(&s.guid)
                && s.owner.as_ref().is_none_or(|o| !layer.contains(&o.0))
                && s.auto_spawn != Some(false)
                && !pinned.contains(&s.guid)
        })
        .collect();
    // the spawners that stay keep their room; a pinned one counts for the capture point nearest it
    let mut taken: Vec<[f64; 2]> = all
        .iter()
        .filter(|s| !palette.iter().any(|p| p.guid == s.guid))
        .map(|s| rows_xz(&s.rows))
        .collect();
    for g in pinned {
        let Some(s) = all.iter().find(|s| &s.guid == g && s.moving().is_some()) else {
            continue;
        };
        let p = rows_xz(&s.rows);
        let near = anchors
            .iter()
            .enumerate()
            .min_by(|a, b| d2(*a.1, p).total_cmp(&d2(*b.1, p)))
            .map(|x| x.0);
        if let Some(x) = near.and_then(|k| wants.get_mut(k)) {
            x.n = x.n.saturating_sub(1);
        }
    }
    let mut used: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut copies = 0u64;
    for want in &wants {
        // the teams of its templates: a flag's neutral, else one of each team
        let teams: Vec<&str> = if want.flag {
            vec!["TeamNeutral", "Team1", "Team2"]
        } else {
            vec![want.team.as_str()]
        };
        let classes: Vec<&str> = want
            .order
            .iter()
            .copied()
            .filter(|c| {
                palette
                    .iter()
                    .any(|p| teams.contains(&p.team.as_str()) && p.moving().is_some_and(|m| m.0 == *c))
            })
            .collect();
        if classes.is_empty() {
            if want.n > 0 {
                out.notes.push(format!(
                    "{}: the level has no vehicle of {} to place",
                    want.cp,
                    if want.flag { "either team" } else { want.team.as_str() }
                ));
            }
            continue;
        }
        let mut made = 0usize;
        let mut tries = 0usize;
        let mut dead: std::collections::BTreeSet<&str> = std::collections::BTreeSet::new();
        while made < want.n && tries < want.n * classes.len() * 2 + 4 && dead.len() < classes.len() {
            let Some(&class) = classes
                .iter()
                .cycle()
                .skip(tries % classes.len())
                .take(classes.len())
                .find(|c| !dead.contains(*c))
            else {
                break;
            };
            tries += 1;
            // a flag takes a neutral spawner, else a pair (one per team)
            let picks: Vec<&str> = if want.flag {
                if palette
                    .iter()
                    .any(|p| p.team == "TeamNeutral" && p.moving().is_some_and(|m| m.0 == class))
                {
                    vec!["TeamNeutral"]
                } else if want.n - made >= 2 {
                    vec!["Team1", "Team2"]
                } else {
                    // one more is wanted: a pair would pass DICE's count
                    dead.insert(class);
                    continue;
                }
            } else {
                vec![want.team.as_str()]
            };
            let mut here: Vec<(&Spawner, Rows, bool)> = Vec::new();
            for team in picks {
                let of_class: Vec<&&Spawner> = palette
                    .iter()
                    .filter(|p| p.team == team && p.moving().is_some_and(|m| m.0 == class))
                    .collect();
                let fresh = of_class
                    .iter()
                    .find(|p| !used.contains(&p.guid) && !here.iter().any(|h| h.0.guid == p.guid));
                let Some(template) = fresh.or_else(|| of_class.first()) else {
                    here.clear();
                    break;
                };
                let tp = template.rows[3].map(f64::from);
                let above = if leave(class) == Some(Leave::Sail) {
                    site.water(tp[0], tp[2]).map_or(0.3, |wl| tp[1] - wl)
                } else {
                    tp[1] - site.stand(tp)
                };
                let mut busy = taken.clone();
                busy.extend(here.iter().map(|x| rows_xz(&x.1)));
                let keep = Keep {
                    polys: &polys,
                    anchors: &anchors,
                    taken: &busy,
                    spawns: &spawns,
                };
                let Some(rows) = spot(site, class, want.at, centre, &keep, above) else {
                    here.clear();
                    break;
                };
                here.push((template, rows, fresh.is_none()));
            }
            if here.is_empty() {
                dead.insert(class);
                continue;
            }
            for (template, rows, copy) in here {
                let guid = if copy {
                    copies += 1;
                    let g =
                        crate::new_guid(tag, copies, &format!("{}/{}", template.guid, want.cp)).to_ascii_uppercase();
                    out.places.push(crate::ops::Placed {
                        guid: g.clone(),
                        template: template.guid.clone(),
                        blueprint: String::new(),
                        ref_field: None,
                        owned: Vec::new(),
                        transform: rows,
                        fields: Vec::new(),
                        placement: None,
                    });
                    g
                } else {
                    used.insert(template.guid.clone());
                    out.moves.push(crate::ops::Moved {
                        guid: template.guid.clone(),
                        from: template.rows,
                        to: rows,
                        placement: None,
                    });
                    template.guid.clone()
                };
                taken.push(rows_xz(&rows));
                out.links.push((want.cp.clone(), guid.clone()));
                out.vehicles.push(Vehicle {
                    guid,
                    class: class.to_owned(),
                    owner: want.cp.clone(),
                    team: template.team.clone(),
                    copy,
                    distance: d2(xz(want.at), rows_xz(&rows)),
                });
                made += 1;
            }
        }
        if made < want.n {
            out.notes.push(format!(
                "{}: {made} of DICE's {} vehicles placed (no spot with the vehicle's room, a way out and its capture \
                 point's pool)",
                want.cp, want.n
            ));
        }
    }
    out
}
