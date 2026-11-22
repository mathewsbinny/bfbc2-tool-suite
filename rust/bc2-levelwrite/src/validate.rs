//! The export's rules checked on a project as it is edited (round V1): the [`crate::rules`] of [`Phase::Edit`] and
//! L1's placement tendencies ([`crate::placement`]), before anything is written. The editor's Problems panel shows
//! them live (after every change), `mapexport validate` prints them; `mapexport build` refuses the errors again and
//! `mapexport check` proves the staged level.
//!
//! [`Phase::Edit`]: crate::rules::Phase::Edit

use std::collections::{HashMap, HashSet};

use crate::export::Game;
use crate::files::Target;
use crate::mpconvert::guid_of;
use crate::placement::{self, Mode};
use crate::project::{MapProject, Profile};
use crate::rules::Finding;
use crate::write::LevelWriter;

/// What the edit-time rules read.
#[derive(Clone, Copy)]
pub struct Input<'a> {
    /// The project.
    pub project: &'a MapProject,
    /// The level partition as the log leaves it (`None`: it does not parse).
    pub level: Option<&'a LevelWriter>,
    /// The base level's partition (`None`: not read).
    pub base: Option<&'a LevelWriter>,
    /// The mode choice's donor partition as the game copy has it now, and its sha1 (`None`: not read).
    pub donor: Option<(&'a LevelWriter, &'a str)>,
    /// The game copy, for the names' clashes (`None`: only their form is checked).
    pub game: Option<&'a crate::names::Taken>,
    /// The game copy's languages (`None`: plain ASCII).
    pub languages: Option<&'a crate::names::Languages>,
    /// The base level's own DBX check findings' words ([`dbx_check`] of `base`; `None`: made here when `base` is
    /// given). The editor keeps them: the base does not change while a project is open.
    pub base_check: Option<&'a HashSet<String>>,
    /// The mode L1's tendencies are checked for (`None`: the project's, else the level's own: [`mode_of`]).
    pub mode: Option<Mode>,
    /// The base level's ground, water and collision (round A1: the site rules XVAL-PLACE-2 to 4; `None`: not
    /// checked).
    pub site: Option<&'a crate::site::Site>,
    /// The level's route graph (round A1: XVAL-PLACE-5; `None`: no path data, not checked).
    pub routes: Option<&'a crate::site::Routes>,
}

/// The mode a project is played as: its target's, else the base level's own layer's.
#[must_use]
pub fn mode_of(project: &MapProject, level: &LevelWriter) -> Option<Mode> {
    match project.mode() {
        Some(c) => Mode::of_key(&c.mode),
        None => placement::detect_mode(level),
    }
}

/// The blueprint of the 40 km/h sign whose borrowed shader entries crashed the renderer (XVAL-SHDR-2).
pub const SPEED_SIGN: &str = "objects/objects/roadsigns/speedsign_40";

/// Every edit-time finding of the project: its log's, its mode's, its names', its objects' and L1's placement
/// tendencies.
#[must_use]
#[allow(clippy::too_many_lines, reason = "the rules in the registry's order")]
pub fn validate(i: Input<'_>) -> Vec<Finding> {
    let p = i.project;
    let mut out = Vec::new();
    // the log's files the export does not write
    for o in &p.operations {
        for t in o.op.files() {
            if matches!(t, Target::Terrain { .. } | Target::Paint { .. }) {
                out.push(Finding::new(
                    "XVAL-EDIT-5",
                    format!(
                        "Operation {} ({}) edits {}: the export does not write terrain yet (round E5).",
                        o.n,
                        o.name(),
                        t.describe()
                    ),
                ));
            }
        }
    }
    // the mode
    if let Some(c) = p.mode() {
        if let Some(why) = crate::modelayer::not_offered(&c.mode) {
            out.push(Finding::new("XVAL-MODE-1", why));
        }
        if let Some((d, sha1)) = i.donor {
            if sha1 != c.donor_sha1 {
                out.push(Finding::new(
                    "XVAL-MODE-4",
                    format!(
                        "{}'s partition changed since the mode was chosen (sha1 {} then, {sha1} now): the layer is \
                         grafted from it as it is now.",
                        c.donor, c.donor_sha1
                    ),
                ));
            }
            if let Some(w) = i.level {
                let miss = crate::modelayer::missing(
                    &c.mode,
                    &crate::modelayer::signature(w),
                    &crate::modelayer::signature(d),
                );
                for m in miss {
                    out.push(Finding::new(
                        "XVAL-MODE-3",
                        format!("The mode layer is incomplete: {m}."),
                    ));
                }
            }
        }
    }
    // the names (the export's own choice of them, in the log)
    if let Some(n) = p.names() {
        let mode = p
            .mode()
            .map_or_else(String::new, |c| crate::export::mode_title(&c.mode));
        for x in crate::names::check(n, Some(&p.id), &mode, i.game, i.languages) {
            out.push(Finding::new(x.code, x.message));
        }
    }
    let Some(w) = i.level else {
        out.push(Finding::new(
            "XVAL-EDIT-4",
            "The level partition does not parse: no edit is written to it.",
        ));
        return out;
    };
    // the partition's own consistency (DbxEditGUI's Level > Check, the DBX inspector's)
    // (what the base level itself already has is DICE's, not an edit's: only what the edits brought is reported)
    let made: HashSet<String>;
    let before = match (i.base_check, i.base) {
        (Some(b), _) => Some(b),
        (None, Some(b)) => {
            made = base_check(b);
            Some(&made)
        }
        (None, None) => None,
    };
    out.extend(
        dbx_check(w)
            .into_iter()
            // shared or other cull slots are the DBX inspector's to report: the export gives moved, copied and
            // placed objects slots of their own (round XF1)
            .filter(|f| !matches!(f.code.as_str(), "XVAL-CULL-5" | "XVAL-CULL-6"))
            .filter(|f| before.is_none_or(|b| !b.contains(&f.message))),
    );
    // objects the edits moved or made: within the cull tree's range, no borrowed 40 km/h sign
    out.extend(out_of_range(w, i.base));
    let base_bp: HashSet<String> = i.base.map_or_default(|b| {
        b.d.of_type(b".ReferenceObjectData")
            .into_iter()
            .filter_map(|e| crate::culltree::blueprint_of(b, e))
            .collect()
    });
    let base_placed = i.base.map(placed);
    for e in w.d.of_type(b".ReferenceObjectData") {
        let g = guid_of(&w.d, e);
        let Some(rows) = w.transform_at(e) else { continue };
        let before = base_placed.as_ref().and_then(|b| b.get(&g));
        if before == Some(&rows) {
            continue;
        }
        if before.is_none()
            && let Some(bp) = crate::culltree::blueprint_of(w, e)
            && bp.starts_with(SPEED_SIGN)
            && i.base.is_some()
            && !base_bp.contains(&bp)
        {
            out.push(
                Finding::new(
                    "XVAL-SHDR-2",
                    format!(
                        "{g} places the 40 km/h sign on a level that does not draw it: its shader entries would come \
                         from another level, which crashed the renderer at 0x98BD6E."
                    ),
                )
                .naming(vec![g]),
            );
        }
    }
    // vehicle spawners of one team on top of each other
    out.extend(stacked_spawners(w, i.base));
    // spawns, vehicle spawners and objectives on the ground, in the combat area (round A1)
    if let Some(site) = i.site {
        out.extend(site_rules(w, i.base, site));
    }
    // spawns and objectives the AI reaches, once the level has path data (round A1)
    if let Some(r) = i.routes {
        out.extend(path_rules(w, i.base, r));
    }
    // L1's placement tendencies
    if let Some(m) = i.mode.or_else(|| mode_of(p, w)) {
        out.extend(placement::check(w, m, p.target_profile()));
    }
    out
}

/// The words of a base level's own DBX check findings: what [`validate`] leaves out as DICE's.
#[must_use]
pub fn base_check(base: &LevelWriter) -> HashSet<String> {
    dbx_check(base).into_iter().map(|f| f.message).collect()
}

/// Every placed object's transform by guid (one pass: [`LevelWriter::find`] walks every instance).
fn placed(w: &LevelWriter) -> HashMap<String, crate::ops::Rows> {
    w.d.of_type(b".ReferenceObjectData")
        .into_iter()
        .filter_map(|e| w.transform_at(e).map(|r| (guid_of(&w.d, e), r)))
        .collect()
}

/// Placed objects the edits moved or made beyond the cull tree's +-2048 m (XVAL-CULL-4): the export cannot slot
/// them (`culltree::encode`).
#[must_use]
pub fn out_of_range(w: &LevelWriter, base: Option<&LevelWriter>) -> Vec<Finding> {
    let mut out = Vec::new();
    let before = base.map(placed);
    for e in w.d.of_type(b".ReferenceObjectData") {
        let g = guid_of(&w.d, e);
        let Some(rows) = w.transform_at(e) else { continue };
        if before.as_ref().and_then(|b| b.get(&g)) == Some(&rows) {
            continue;
        }
        let [x, y, z] = rows[3];
        if [x, y, z].iter().any(|v| v.abs() > 2047.9) {
            out.push(
                Finding::new(
                    "XVAL-CULL-4",
                    format!(
                        "{g} stands at ({x:.0}, {y:.0}, {z:.0}): beyond +-2048 m the export cannot give it a cull slot."
                    ),
                )
                .naming(vec![g]),
            );
        }
    }
    out
}

/// `DbxEditGUI`'s Level > Check (the DBX inspector's, round GR4) on a partition, each finding under its rule.
#[must_use]
pub fn dbx_check(w: &LevelWriter) -> Vec<Finding> {
    let Ok(bytes) = w.write() else { return Vec::new() };
    let Ok(f) = bc2_formats::dbxedit::EditFile::parse(bytes) else {
        return Vec::new();
    };
    let Ok(found) = bc2_formats::dbxedit::level_check(&f, None, &[]) else {
        return Vec::new();
    };
    found
        .into_iter()
        .map(|x| {
            let guid = x
                .place
                .split_whitespace()
                .nth(1)
                .filter(|g| g.len() == 36)
                .map(str::to_ascii_uppercase);
            Finding::new(
                crate::rules::dbx_check_code(&x.message),
                format!("{}: {}", x.place, x.message),
            )
            .naming(guid.into_iter().collect())
        })
        .collect()
}

/// The same place, to the bit (a spawner the edits left where the base has it).
fn same(a: [f32; 3], b: [f32; 3]) -> bool {
    a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits())
}

/// A vehicle spawner: its guid, its team, where it stands.
type Spawner = (String, String, [f32; 3]);

/// Vehicle spawners of one team within 1 m of each other (XVAL-PLACE-1); pairs the base level already has are
/// DICE's own and left out.
#[must_use]
pub fn stacked_spawners(w: &LevelWriter, base: Option<&LevelWriter>) -> Vec<Finding> {
    let spawners: Vec<(String, String, [f32; 3])> =
        w.d.of_type(b".VehicleSpawnEntityData")
            .into_iter()
            .filter_map(|e| {
                let g = guid_of(&w.d, e);
                let team =
                    w.d.child(e, b"Team")
                        .and_then(|x| w.d.text(x))
                        .map_or_default(|t| String::from_utf8_lossy(t).into_owned());
                w.transform_at(e).map(|r| (g, team, r[3]))
            })
            .collect();
    let base_at: Option<HashMap<String, [f32; 3]>> = base.map(|bw| {
        bw.d.of_type(b".VehicleSpawnEntityData")
            .into_iter()
            .filter_map(|e| bw.transform_at(e).map(|r| (guid_of(&bw.d, e), r[3])))
            .collect()
    });
    let mut by_team: HashMap<&str, Vec<&Spawner>> = HashMap::new();
    for s in &spawners {
        by_team.entry(s.1.as_str()).or_default().push(s);
    }
    let mut out = Vec::new();
    for list in by_team.values() {
        for (k, a) in list.iter().enumerate() {
            for b in list.iter().skip(k + 1) {
                let d = (0..3)
                    .map(|j| (a.2.get(j).copied().unwrap_or(0.0) - b.2.get(j).copied().unwrap_or(0.0)).powi(2))
                    .sum::<f32>()
                    .sqrt();
                if d > 1.0 {
                    continue;
                }
                let dice = base_at.as_ref().is_some_and(|m| {
                    m.get(&a.0).is_some_and(|p| same(*p, a.2)) && m.get(&b.0).is_some_and(|p| same(*p, b.2))
                });
                if dice {
                    continue;
                }
                out.push(
                    Finding::new(
                        "XVAL-PLACE-1",
                        format!(
                            "The vehicle spawners {} and {} ({}) stand {d:.1} m apart: their vehicles spawn into one \
                             another.",
                            a.0,
                            b.0,
                            if a.1.is_empty() { "no team" } else { a.1.as_str() }
                        ),
                    )
                    .naming(vec![a.0.clone(), b.0.clone()]),
                );
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ the site (A1)

/// The room a vehicle of an L1 class needs around its spawner (metres clear of static collision 0.5 to 3 m above
/// it, sampled every 0.5 m) and its domain. Each room is below the tightest of DICE's own spawners of the class on
/// the 27 stock levels measured (`rust/BENCHMARKS.md` "A1": tanks 2.5 m, cars 1 m, helicopters 4 m, boats 1 m, IFVs
/// and quads 0.5 m); emplacements, UAV stations and the ambient aircraft stand where DICE built them in, so they are
/// not held to a room.
#[must_use]
pub fn vehicle_room(class: &str) -> Option<(f64, Domain)> {
    match class {
        "tank" => Some((2.0, Domain::Land)),
        "car" | "ifv" | "ifv_aa" | "quad" => Some((0.5, Domain::Land)),
        "heli" => Some((3.5, Domain::Air)),
        "boat" => Some((0.5, Domain::Water)),
        _ => None,
    }
}

/// Where a vehicle can be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Domain {
    /// On land (not over water deeper than a soldier wades).
    Land,
    /// On water (DICE's boats spawn in 0.4 to 1.9 m of it, by the shore).
    Water,
    /// Anywhere outside collision, also beyond the terrain (an aircraft).
    Air,
}

/// The combat areas' outlines in plan (every `CombatAreaTriggerEntityData`'s `Geometry`, LEVEL-ENT-2).
#[must_use]
pub fn combat_outlines(w: &LevelWriter) -> Vec<Vec<[f64; 2]>> {
    let mut out: Vec<Vec<[f64; 2]>> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for e in w.d.of_type(b".CombatAreaTriggerEntityData") {
        let starts: Vec<String> =
            w.d.child(e, b"Geometry")
                .and_then(|a| w.d.children(a))
                .unwrap_or_default()
                .iter()
                .filter_map(|&i| w.d.attr(i, b"ref").and_then(crate::mpconvert::local_ref))
                .collect();
        for s in starts {
            if seen.contains(&s) {
                continue;
            }
            let chain = crate::gameplay::outline(w, &s);
            seen.extend(chain.iter().cloned());
            let pts: Vec<[f64; 2]> = chain
                .iter()
                .filter_map(|g| w.transform(g))
                .map(|r| [f64::from(r[3][0]), f64::from(r[3][2])])
                .collect();
            if pts.len() >= 3 {
                out.push(pts);
            }
        }
    }
    out
}

fn in_polygon(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut r = false;
    let n = poly.len();
    for k in 0..n {
        let (Some(a), Some(b)) = (poly.get(k), poly.get((k + 1) % n)) else {
            continue;
        };
        if (a[1] > p[1]) != (b[1] > p[1]) && p[0] < a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]) {
            r = !r;
        }
    }
    r
}

/// How far `p` is outside the union of `polys` (0 inside): the distance to the nearest edge.
#[must_use]
pub fn outside_by(p: [f64; 2], polys: &[Vec<[f64; 2]>]) -> f64 {
    if polys.iter().any(|q| in_polygon(p, q)) {
        return 0.0;
    }
    let mut best = f64::INFINITY;
    for q in polys {
        let n = q.len();
        for k in 0..n {
            let (Some(a), Some(b)) = (q.get(k), q.get((k + 1) % n)) else {
                continue;
            };
            let (dx, dz) = (b[0] - a[0], b[1] - a[1]);
            let len2 = dx * dx + dz * dz;
            let t = if len2 > 0.0 {
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dz) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            best = best.min((p[0] - a[0] - t * dx).hypot(p[1] - a[1] - t * dz));
        }
    }
    best
}

/// Every instance of the given types with its transform, by guid.
fn transforms_of(w: &LevelWriter, types: &[&str]) -> Vec<(String, crate::ops::Rows)> {
    let mut out = Vec::new();
    for t in types {
        for e in w.d.of_type(format!(".{t}").as_bytes()) {
            if let Some(r) = w.transform_at(e) {
                out.push((guid_of(&w.d, e), r));
            }
        }
    }
    out
}

/// A spot in words, with the object a buried place is in.
fn spot_words(site: &crate::site::Site, p: [f64; 3]) -> Option<String> {
    use crate::site::Spot;
    match site.spot(p) {
        Spot::Ground => None,
        Spot::OutsideTerrain => Some("outside the terrain".into()),
        Spot::Water(d) => Some(format!("over water {d:.1} m deep")),
        Spot::Solid(d) => Some(match site.buried(p) {
            Some((name, _)) => format!("{d:.1} m deep inside the collision of {name}"),
            None => format!("{d:.1} m deep inside a static object's collision"),
        }),
        Spot::Underground(d) => Some(format!("{d:.1} m under the ground")),
    }
}

/// How far a land or sea vehicle spawner may stand outside every combat area: DICE's own stand up to 142 m outside
/// on the stock levels measured (`mp_006cq`'s tanks, `mp_007`'s IFVs: vehicles that drive in); one farther is
/// reported.
pub const VEHICLE_OUTSIDE: f64 = 150.0;
/// How far a soldier spawn place may stand outside every combat area: DICE's one such place of 1,475 is less than
/// a metre out (`mp_008`).
pub const SPAWN_OUTSIDE: f64 = 1.0;

/// The site rules (round A1) on the gameplay objects the edits moved or made (what the base level already has, at
/// the same place, is DICE's and left out; `base` `None`: every object). Each threshold lies beyond what DICE's own
/// objects do on the 27 stock levels measured, where the rules find nothing (`rust/BENCHMARKS.md` "A1"):
///
/// - XVAL-PLACE-2: a soldier spawn place (an alternative spawn point, a spawner without any) over water deeper than a
///   soldier wades (1.2 m), buried deeper than 4.5 m inside a static object's collision (DICE's own: up to 4.2 m into
///   a rock's; standing on an object's floor is an interior, not buried), more than 6 m under the ground or outside
///   the terrain;
/// - XVAL-PLACE-3: a vehicle spawner without its vehicle's room ([`vehicle_room`]) or out of its domain (a land
///   vehicle over deep water, a boat on dry land where the water mesh lies near the ground, any buried in collision,
///   a land or sea vehicle outside the terrain);
/// - XVAL-PLACE-4: an M-COM outside every combat area, a soldier spawn place more than [`SPAWN_OUTSIDE`] outside, or
///   a land or sea vehicle spawner more than [`VEHICLE_OUTSIDE`] outside (aircraft and the ambient backdrop are not
///   held to it; capture points are L1's: L1-CQ-013 and L1-CQ-014 hold a flag and an HQ to its team's area, and
///   DICE's Rush, Squad Rush and Squad Deathmatch bases stand outside theirs).
#[must_use]
#[allow(clippy::too_many_lines, reason = "the three rules in order")]
pub fn site_rules(w: &LevelWriter, base: Option<&LevelWriter>, site: &crate::site::Site) -> Vec<Finding> {
    use crate::site::Spot;
    let mut out = Vec::new();
    let types = [
        "AlternateSpawnEntityData",
        "SoldierSpawnEntityData",
        "VehicleSpawnEntityData",
        "CapturePointEntityData",
        "ReferenceObjectData",
    ];
    let before: Option<HashMap<String, crate::ops::Rows>> =
        base.map(|b| transforms_of(b, &types).into_iter().collect());
    let edited = |g: &str, r: &crate::ops::Rows| before.as_ref().is_none_or(|b| b.get(g) != Some(r));
    // soldier spawn places: the alternatives, and spawners that have none
    let mut places: Vec<(String, crate::ops::Rows)> = transforms_of(w, &["AlternateSpawnEntityData"]);
    for e in w.d.of_type(b".SoldierSpawnEntityData") {
        let has_alts =
            w.d.child(e, b"AlternativeSpawnPoints")
                .and_then(|a| w.d.children(a))
                .is_some_and(|c| !c.is_empty());
        if !has_alts && let Some(r) = w.transform_at(e) {
            places.push((guid_of(&w.d, e), r));
        }
    }
    for (g, r) in &places {
        if !edited(g, r) {
            continue;
        }
        if let Some(words) = spot_words(site, r[3].map(f64::from)) {
            out.push(
                Finding::new(
                    "XVAL-PLACE-2",
                    format!("The spawn point {g} stands {words}: a soldier spawning there drowns or is stuck."),
                )
                .naming(vec![g.clone()]),
            );
        }
    }
    // vehicle spawners
    let mut vehicles: Vec<(String, crate::ops::Rows, Vec<&'static str>)> = Vec::new();
    for (g, e, classes) in placement::vehicle_spawners(w) {
        let Some(r) = w.transform_at(e) else { continue };
        vehicles.push((g.clone(), r, classes.clone()));
        if !edited(&g, &r) {
            continue;
        }
        let p = r[3].map(f64::from);
        let spot = site.spot(p);
        for c in classes {
            let Some((room, domain)) = vehicle_room(c) else {
                continue;
            };
            let (surface, ground) = (site.water(p[0], p[2]), site.ground(p[0], p[2]));
            let wet = surface.is_some_and(|s| ground.is_none_or(|g| s > g));
            // a boat is judged only where the level's water mesh lies near the ground (within 10 m below it): water the
            // mesh does not hold (Vietnam's rivers, `nam_mp_005r`: the mesh 103 m below them) is not seen
            let judged = surface.is_some_and(|s| ground.is_none_or(|g| s > g - 10.0));
            let wrong = match (domain, spot) {
                (Domain::Land | Domain::Water, Spot::OutsideTerrain) => Some("outside the terrain".to_owned()),
                (_, Spot::Solid(d)) => Some(format!("{d:.1} m deep inside a static object's collision")),
                (Domain::Land, Spot::Water(d)) => Some(format!("a land vehicle over water {d:.1} m deep")),
                (Domain::Water, _) if judged && !wet => Some("a boat on dry land".to_owned()),
                _ => None,
            };
            let clear = if wrong.is_none() { site.clearance(p, room) } else { room };
            let why = wrong.or_else(|| {
                (clear < room).then(|| format!("only {clear:.1} m clear of static collision ({room:.1} m needed)"))
            });
            if let Some(why) = why {
                out.push(
                    Finding::new(
                        "XVAL-PLACE-3",
                        format!(
                            "The vehicle spawner {g} ({c}): {why}: its vehicle spawns into the world or cannot move."
                        ),
                    )
                    .naming(vec![g.clone()]),
                );
                break;
            }
        }
    }
    // inside a combat area: (guid, rows, what, how far outside is allowed)
    let polys = combat_outlines(w);
    if !polys.is_empty() {
        let mut gameplay: Vec<(String, crate::ops::Rows, String, f64)> = Vec::new();
        for (g, _, p) in crate::modelayer::anchors(w).iter().filter(|a| a.1.contains("M-COM")) {
            gameplay.push((
                g.clone(),
                w.transform(g).unwrap_or_else(|| crate::rows_at(*p)),
                "M-COM".to_owned(),
                0.0,
            ));
        }
        gameplay.extend(
            places
                .iter()
                .map(|(g, r)| (g.clone(), *r, "spawn point".to_owned(), SPAWN_OUTSIDE)),
        );
        for (g, r, classes) in &vehicles {
            if classes
                .iter()
                .any(|c| matches!(vehicle_room(c), Some((_, Domain::Land | Domain::Water))))
            {
                gameplay.push((
                    g.clone(),
                    *r,
                    format!("vehicle spawner ({})", classes.join(", ")),
                    VEHICLE_OUTSIDE,
                ));
            }
        }
        for (g, r, what, allowed) in gameplay {
            if !edited(&g, &r) {
                continue;
            }
            let d = outside_by([f64::from(r[3][0]), f64::from(r[3][2])], &polys);
            if d > allowed {
                out.push(
                    Finding::new(
                        "XVAL-PLACE-4",
                        format!(
                            "The {what} {g} stands {d:.0} m outside every combat area: a player there is told to \
                             return or dies as a deserter."
                        ),
                    )
                    .naming(vec![g]),
                );
            }
        }
    }
    out
}

/// XVAL-PLACE-5 (round A1): the gameplay objects the edits moved or made that the AI cannot reach from the main route
/// network of the level's path data (`routes`: NavKit's build or DICE's file, the AI workspace's): a soldier spawn
/// place, a flag or an M-COM with no route node within 15 m in plan and 4 m in height, or whose nearest node is on
/// an island cut off from the main network (GR2's islands). A bot spawned there cannot leave; an objective there
/// cannot be attacked or defended by bots.
#[must_use]
pub fn path_rules(w: &LevelWriter, base: Option<&LevelWriter>, routes: &crate::site::Routes) -> Vec<Finding> {
    let mut out = Vec::new();
    if routes.is_empty() {
        return out;
    }
    let types = [
        "AlternateSpawnEntityData",
        "CapturePointEntityData",
        "ReferenceObjectData",
    ];
    let before: Option<HashMap<String, crate::ops::Rows>> =
        base.map(|b| transforms_of(b, &types).into_iter().collect());
    let edited = |g: &str, r: &crate::ops::Rows| before.as_ref().is_none_or(|b| b.get(g) != Some(r));
    let mut objects: Vec<(String, crate::ops::Rows, &str)> = transforms_of(w, &["AlternateSpawnEntityData"])
        .into_iter()
        .map(|(g, r)| (g, r, "spawn point"))
        .collect();
    for e in w.d.of_type(b".CapturePointEntityData") {
        let base_point =
            w.d.child(e, b"UnableToChangeTeam")
                .is_some_and(|x| w.d.raw(x).get(1).is_some_and(|&b| b != 0));
        if !base_point && let Some(r) = w.transform_at(e) {
            objects.push((guid_of(&w.d, e), r, "flag"));
        }
    }
    for (g, _, p) in crate::modelayer::anchors(w).iter().filter(|a| a.1.contains("M-COM")) {
        objects.push((g.clone(), w.transform(g).unwrap_or_else(|| crate::rows_at(*p)), "M-COM"));
    }
    for (g, r, what) in objects {
        if !edited(&g, &r) {
            continue;
        }
        let why = match routes.reach(r[3].map(f64::from)) {
            None => Some(format!(
                "no route node within {:.0} m ({})",
                crate::site::JOIN_XZ,
                routes.source
            )),
            Some((0, _)) => None,
            Some((k, d)) => Some(format!(
                "its nearest route node ({d:.0} m) is on island {k}, cut off from the main network ({})",
                routes.source
            )),
        };
        if let Some(why) = why {
            out.push(
                Finding::new("XVAL-PLACE-5", format!("The {what} {g} has no path for the AI: {why}.")).naming(vec![g]),
            );
        }
    }
    out
}

/// A project validated against the game copy as `mapexport validate` does it: the log applied to the base (its
/// failures XVAL-EDIT-1 to 3), then [`validate`] with the donor and the names' clashes read from the game, and the
/// site rules on the base's height field and water (round A1; static collision is the editor's: its scene).
///
/// # Errors
/// The base level's partition is not in the game copy.
pub fn project(game: &Game, p: &MapProject) -> crate::export::Result<Vec<Finding>> {
    let base_bytes = crate::export::level_partition(game, &p.base.level)?;
    let mut out = Vec::new();
    let r = p.base_report(&base_bytes);
    if r.changed {
        out.push(Finding::new(
            "XVAL-EDIT-1",
            format!(
                "Levels/{} is not the partition the project was made on; {} operation(s) name objects it lacks.",
                p.base.level,
                r.missing.len()
            ),
        ));
    }
    for (n, name, guid) in &r.missing {
        out.push(
            Finding::new(
                "XVAL-EDIT-2",
                format!("Operation {n} ({name}) names {guid}, which the level does not have."),
            )
            .naming(vec![guid.clone()]),
        );
    }
    let base = LevelWriter::parse(&base_bytes).ok();
    let schema = crate::Schema::scan(base.iter().map(|b| &b.d));
    let donors = |lv: &str| crate::export::level_partition(game, lv).ok();
    let base_lv = p.base.level.to_ascii_lowercase();
    let base_bundles = game.loaded(&base_lv, "level");
    // the log's other files (static group files) as the export reads them: the base's own
    let sources = |t: &Target| crate::export::base_level_file(game, &base_bundles, &base_lv, t);
    let level = match crate::files::apply_files(&base_bytes, &p.operations, &schema, &donors, &sources) {
        Ok(a) => LevelWriter::parse(&a.partition.bytes).ok(),
        Err(e) => {
            let m = e.to_string();
            out.push(Finding::new(crate::rules::apply_code(&m), m));
            None
        }
    };
    let donor = p
        .mode()
        .and_then(|c| crate::export::level_partition(game, &c.donor).ok())
        .map(|b| (crate::project::sha1_hex(&b), LevelWriter::parse(&b).ok()));
    let donor_ref = donor.as_ref().and_then(|(s, w)| w.as_ref().map(|w| (w, s.as_str())));
    let taken = crate::names::Taken::read(game);
    let langs = crate::names::Languages::read(game);
    // round A1: the base's ground and water from the game copy (no collision: the editor reads that from its scene)
    let site = base
        .as_ref()
        .and_then(|b| crate::export::ground_of(game, &base_lv, b).ok())
        .filter(|(hf, _)| hf.is_some())
        .map(|(hf, water)| crate::site::Site::new(hf, water.as_ref(), &[], &[]));
    out.extend(validate(Input {
        project: p,
        level: level.as_ref(),
        base: base.as_ref(),
        donor: donor_ref,
        game: Some(&taken),
        languages: Some(&langs),
        base_check: None,
        mode: None,
        site: site.as_ref(),
        routes: None,
    }));
    Ok(out)
}

/// The findings of a stock level as it is (V1's proof: no stock level raises an error): its own mode and era.
///
/// # Errors
/// The level's partition is not in the game copy.
pub fn stock(
    game: &Game,
    lv: &str,
    profile: Profile,
    mode: Option<Mode>,
) -> crate::export::Result<(Option<Mode>, Vec<Finding>)> {
    let bytes = crate::export::level_partition(game, lv)?;
    let w = LevelWriter::parse(&bytes)?;
    let p = MapProject::new(lv, lv, &format!("Levels/{}", lv.to_ascii_uppercase()), &bytes, profile);
    let mode = mode.or_else(|| placement::detect_mode(&w));
    let f = validate(Input {
        project: &p,
        level: Some(&w),
        base: Some(&w),
        donor: None,
        game: None,
        languages: None,
        base_check: None,
        mode,
        site: None,
        routes: None,
    });
    Ok((mode, f))
}
