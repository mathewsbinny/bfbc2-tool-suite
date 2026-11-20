//! A game mode's layer as project data (round E3d, `EDITOR_ARCHITECTURE.md` 3.12 "Round E3d").
//!
//! The project's target - its era (the content profile) and its multiplayer mode - is a setting of the project, an
//! operation of its log ([`crate::Op::SetMode`]): choosing it brings a stock level's mode layer INTO the level as
//! ordinary objects (new guids, references re-pointed, placed at first-guess positions) and takes the base's own
//! mode layer out. The user then moves, deletes and edits flags, bases, M-COMs, spawns and combat areas like any
//! object; an undo takes the whole choice back. The export writes the project as it is.
//!
//! **What a mode layer is, from the data.** On the stock levels of every set (BFBC2, BFBC1, Vietnam, WW2), a mode's
//! layer is what its gameplay instances reach, by their references and their events, among the gameplay types:
//!
//! - the seeds: E3's Conquest seeds ([`mpconvert::LAYER_SEEDS`]: teams, player, mission, menus, messages, the
//!   announcer, ticket counters ...) and the other modes' ([`MODE_SEEDS`]: Rush's attacker lives
//!   (`LifeCounterEntityData`), the M-COMs' arm / disarm awards and mission objectives, Squad Deathmatch's kill
//!   counters, every capture point and combat area, the end-of-round spawns of OPFOR's WW2 levels), and the
//!   **objectives** themselves: the placed objects (`ReferenceObjectData`) whose events reach the arm and defuse
//!   interactions (`objects/objects/mpobjective/mpobjective` on BFBC2 and BFBC1, `mp_objective_vietnam_01` on
//!   Vietnam), with the objective props their events destroy (the dish and wall mount);
//! - what they reach (forward references and forward events) among [`mpconvert::LAYER_TYPES`] and [`MODE_TYPES`]
//!   (the event glue, spawns, markers, area geometry, the M-COMs' interactions, sounds and stage sequences).
//!
//! Left out, as E3 left them out: the donor's vehicle spawns (E4 places vehicles), its world objects, effects and
//! cutscenes; references to them are cut and listed. Measured on OPFOR's game copy: `mp_009cq` 238 instances,
//! `mp_005gr` 821 (4 stages of 2 M-COMs), `mp_005sr` 279 (2 stages of 1), `mp_009sdm` 172 (4 squads, 4 kill
//! counters), `guadal_canal` 277, `nam_mp_002r` 507, `bc1_oasis_gr` 1,082.
//!
//! **Rush stages.** An M-COM's `onDestroyed` events trigger `MultipleTriggerEntityData`s that count the stage's
//! objectives down; the M-COMs that trigger the same set of them are one stage ([`stages`]: `mp_005gr`'s
//! `Base_1` .. `Base4Trigger`, two M-COMs each; `mp_005sr`'s two stages of one).
//!
//! **First-guess placement** ([`choose`]): the whole layer is moved (and, when it is larger, scaled down; never
//! turned) so the bounding box of its combat areas lands centred in the base's largest combat area; each instance
//! keeps its height above the donor's ground, over the base's ground. The user then places it properly ("Use
//! selection" moves a flag, base or M-COM with what belongs to it: [`cluster`]); E4's gameplay tools build on this.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bc2_formats::dbx::edit::NodeId;
use serde::{Deserialize, Serialize};

use crate::mpconvert::{self, Converted, Ground, LAYER_SEEDS, LAYER_TYPES, guid_of, local_ref, short, type_of};
use crate::ops::{Moved, Rows};
use crate::project::Profile;
use crate::write::{LevelWriteError, LevelWriter, Result};

/// The other modes' seeds (besides E3's Conquest seeds): Rush's lives, awards and objectives, Squad Deathmatch's
/// kill counters, every capture point and combat area, WW2's end-of-round spawns.
pub const MODE_SEEDS: &[&str] = &[
    "ArmDisarmAwardEntityData",
    "CapturePointEntityData",
    "CombatAreaTriggerEntityData",
    "EndOfRoundSoldierSpawnEntityData",
    "KillCounterEntityData",
    "LifeCounterEntityData",
    "MissionObjectiveEntityData",
];

/// The other modes' gameplay types a layer may take besides [`mpconvert::LAYER_TYPES`]: the M-COMs' interactions
/// and alarm sounds, area triggers, stage sequences.
pub const MODE_TYPES: &[&str] = &[
    "AreaTriggerEntityData",
    "ClearAreaTriggerEntityData",
    "DamageAreaTriggerEntityData",
    "GameInteractionEntityData",
    "GameSoundEntityData",
    "SequenceData",
];

/// A mode's names: its key in the descriptions' game mode (`goldrush`), the `maps.ini` key and the menus' name.
#[must_use]
pub fn mode_name(mode: &str) -> String {
    match mode.to_ascii_lowercase().replace(' ', "").as_str() {
        "conquest" => "Conquest".into(),
        "goldrush" | "rush" => "Rush".into(),
        "squadrush" => "Squad Rush".into(),
        "teamdeathmatch" | "squaddeathmatch" => "Squad Deathmatch".into(),
        "onslaught" => "Onslaught".into(),
        "airsuperiority" => "Air Superiority".into(),
        other => other.to_owned(),
    }
}

/// The game mode key of a `maps.ini` key or a mode name (`Rush` -> `goldrush`; a key passes through).
#[must_use]
pub fn mode_key(name: &str) -> String {
    match name.to_ascii_lowercase().replace(' ', "").as_str() {
        "rush" | "goldrush" => "goldrush".into(),
        "squaddeathmatch" | "teamdeathmatch" => "teamdeathmatch".into(),
        other => other.to_owned(),
    }
}

/// Why an export does not offer a mode the game copy has (`None`: offered). Onslaught is co-operative play against
/// bots: it needs NavKit path data and the AI layer, which an export does not make yet (3.12, "Bots (later)"). Air
/// Superiority is BF1943's: its planes-only rules are OPFOR's Pacific War work still queued.
#[must_use]
pub fn not_offered(mode: &str) -> Option<&'static str> {
    match mode_key(mode).as_str() {
        "onslaught" => Some("Onslaught needs bots (NavKit path data and the AI layer): a later round"),
        "airsuperiority" => Some("Air Superiority is BF1943's mode, still queued in OPFOR's Pacific War work"),
        _ => None,
    }
}

/// One instance of a donor's layer brought into the project: the donor's guid, the guid it gets, and its place
/// (the first guess; `None`: no transform).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LayerItem {
    /// The donor instance's guid (upper case).
    pub donor: String,
    /// The guid it has in the project ([`mpconvert::graft_guid`] over the project, the operation and the donor's).
    pub guid: String,
    /// Where it stands (x, y, z), its turn kept; `None`: an instance without a transform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<[f32; 3]>,
}

/// How the first guess placed the layer: the donor's combat-area box, the base's, the scale (x0, z0, x1, z1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Fit {
    /// The donor layer's box.
    pub from: [f32; 4],
    /// The base's box it was fitted into.
    pub to: [f32; 4],
    /// The scale (1: as large as on the donor).
    pub scale: f32,
}

/// The project's target (round E3d): its era, its mode and the mode's layer (data only: guids and places; the
/// donor's instances are read from the game copy when the log is applied).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModeChoice {
    /// The era (content profile) the level is made for.
    pub profile: Profile,
    /// The mode's key (`conquest`, `goldrush`, `squadrush`, `teamdeathmatch`).
    pub mode: String,
    /// The donor: a stock level of that era and mode (lower case).
    pub donor: String,
    /// The donor's level partition's sha1 when chosen.
    pub donor_sha1: String,
    /// The donor's layer as brought in.
    pub items: Vec<LayerItem>,
    /// The level's own mode layer (or a campaign base's player, mission, menus, spawns, areas, markers and
    /// announcer) taken out (upper-case guids).
    pub removed: Vec<String>,
    /// How it was placed.
    #[serde(default)]
    pub fit: Fit,
}

impl ModeChoice {
    /// The choice in words (the History panel).
    #[must_use]
    pub fn describe(&self) -> String {
        format!(
            "Target: {} {} from {} ({} objects in, {} out)",
            self.profile.word(),
            mode_name(&self.mode),
            self.donor.to_ascii_uppercase(),
            self.items.len(),
            self.removed.len()
        )
    }

    /// Donor guid -> project guid (upper case).
    #[must_use]
    pub fn map(&self) -> HashMap<String, String> {
        self.items
            .iter()
            .map(|i| (i.donor.to_ascii_uppercase(), i.guid.to_ascii_uppercase()))
            .collect()
    }
}

fn refused(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

/// A partition's instances by guid (node, short type) and its forward event edges.
struct Graph {
    by_guid: HashMap<String, (NodeId, String)>,
    order: Vec<String>,
    events: HashMap<String, Vec<String>>,
}

fn graph(w: &LevelWriter) -> Graph {
    let mut by_guid = HashMap::new();
    let mut order = Vec::new();
    for e in w.d.instances() {
        let g = guid_of(&w.d, e);
        order.push(g.clone());
        by_guid.insert(g, (e, short(&type_of(&w.d, e)).to_owned()));
    }
    let mut events: HashMap<String, Vec<String>> = HashMap::new();
    for a in w.connection_arrays() {
        for &c in w.d.children(a).unwrap_or_default() {
            let end = |f: &[u8]| w.d.child(c, f).and_then(|x| w.d.attr(x, b"ref")).and_then(local_ref);
            if let (Some(s), Some(t)) = (end(b"Source"), end(b"Target")) {
                events.entry(s).or_default().push(t);
            }
        }
    }
    Graph { by_guid, order, events }
}

impl Graph {
    fn ty(&self, g: &str) -> &str {
        self.by_guid.get(g).map_or("", |x| x.1.as_str())
    }
}

/// A placed object's blueprint reference (lower case), empty when it is not one.
fn blueprint(w: &LevelWriter, e: NodeId) -> String {
    w.d.child(e, b"ReferencedObject")
        .and_then(|x| w.d.attr(x, b"ref"))
        .map_or_default(|r| String::from_utf8_lossy(r).to_ascii_lowercase())
}

/// The M-COMs: placed objects whose events reach an arm / defuse interaction.
fn mcoms(w: &LevelWriter, g: &Graph) -> Vec<String> {
    g.order
        .iter()
        .filter(|s| {
            g.ty(s) == "ReferenceObjectData"
                && g.events
                    .get(*s)
                    .is_some_and(|ts| ts.iter().any(|t| g.ty(t) == "GameInteractionEntityData"))
        })
        .filter(|s| g.by_guid.get(*s).is_some_and(|(e, _)| !blueprint(w, *e).is_empty()))
        .cloned()
        .collect()
}

/// The objectives: the M-COMs and the objective props their events reach (the dish, the wall mount).
fn objectives(w: &LevelWriter, g: &Graph) -> HashSet<String> {
    let m = mcoms(w, g);
    let mut out: HashSet<String> = m.iter().cloned().collect();
    for o in &m {
        for t in g.events.get(o).map_or(&[][..], Vec::as_slice) {
            if g.ty(t) == "ReferenceObjectData"
                && g.by_guid
                    .get(t)
                    .is_some_and(|(e, _)| blueprint(w, *e).contains("objective"))
            {
                out.insert(t.clone());
            }
        }
    }
    out
}

/// A level's whole mode layer (upper-case guids; module doc): every seed and objective, and what they reach among
/// the layer's types.
#[must_use]
pub fn layer(w: &LevelWriter) -> HashSet<String> {
    let g = graph(w);
    let objs = objectives(w, &g);
    let seed = |t: &str| LAYER_SEEDS.contains(&t) || MODE_SEEDS.contains(&t);
    let allowed = |guid: &str| -> bool {
        let t = g.ty(guid);
        seed(t) || LAYER_TYPES.contains(&t) || MODE_TYPES.contains(&t) || objs.contains(guid)
    };
    let mut set: HashSet<String> = g
        .order
        .iter()
        .filter(|x| seed(g.ty(x)) || objs.contains(*x))
        .cloned()
        .collect();
    let mut work: Vec<String> = set.iter().cloned().collect();
    while let Some(x) = work.pop() {
        let Some(&(e, _)) = g.by_guid.get(&x) else { continue };
        let mut next: Vec<String> =
            w.d.walk(e)
                .into_iter()
                .filter_map(|n| w.d.attr(n, b"ref").and_then(local_ref))
                .collect();
        next.extend(g.events.get(&x).cloned().unwrap_or_default());
        for n in next {
            if allowed(&n) && set.insert(n.clone()) {
                work.push(n);
            }
        }
    }
    set
}

/// What an old layer of the level is, to take out before a new one comes in: a level with a mode layer
/// (`AutoTeamEntityData`) loses that layer and the instances of [`mpconvert::REPLACED_BY_LAYER`]'s types; a campaign
/// level the latter (its player, mission, menus, spawns, areas, markers and announcer: what E3's strip always took).
/// Vehicle spawners stay: the campaign checklist chooses them (their events to removed instances are cut).
#[must_use]
pub fn old_layer(w: &LevelWriter) -> Vec<String> {
    let g = graph(w);
    let mut out: BTreeSet<String> = BTreeSet::new();
    if g.order.iter().any(|x| g.ty(x) == "AutoTeamEntityData") {
        out.extend(layer(w));
    }
    for x in &g.order {
        let t = g.ty(x);
        if mpconvert::REPLACED_BY_LAYER.contains(&t) && t != mpconvert::CAMPAIGN_VEHICLES {
            out.insert(x.clone());
        }
    }
    // in the partition's order
    g.order.iter().filter(|x| out.contains(*x)).cloned().collect()
}

fn translation(rows: &Rows) -> [f32; 3] {
    rows[3]
}

/// The points of the combat areas among `set` (each `CombatAreaTriggerEntityData`'s `Geometry` polygons, followed
/// by `Next`), per area.
fn area_points(w: &LevelWriter, set: &HashSet<String>) -> Vec<Vec<[f32; 3]>> {
    let mut out = Vec::new();
    for e in w.d.of_type(b".CombatAreaTriggerEntityData") {
        if !set.contains(&guid_of(&w.d, e)) {
            continue;
        }
        let mut pts = Vec::new();
        let starts: Vec<String> =
            w.d.child(e, b"Geometry")
                .and_then(|a| w.d.children(a))
                .unwrap_or_default()
                .iter()
                .filter_map(|&i| w.d.attr(i, b"ref").and_then(local_ref))
                .collect();
        for s in starts {
            let mut seen = HashSet::new();
            let mut cur = Some(s);
            while let Some(g) = cur.take() {
                if !seen.insert(g.clone()) {
                    break;
                }
                let Some(n) = w.find(&g) else { break };
                if let Some(r) = w.transform(&g) {
                    pts.push(translation(&r));
                }
                cur =
                    w.d.child(n, b"Next")
                        .and_then(|x| w.d.attr(x, b"ref"))
                        .and_then(local_ref);
            }
        }
        if !pts.is_empty() {
            out.push(pts);
        }
    }
    out
}

fn bbox<'a>(pts: impl IntoIterator<Item = &'a [f32; 3]>) -> Option<[f32; 4]> {
    let mut b: Option<[f32; 4]> = None;
    for p in pts {
        let v = b.get_or_insert([p[0], p[2], p[0], p[2]]);
        v[0] = v[0].min(p[0]);
        v[1] = v[1].min(p[2]);
        v[2] = v[2].max(p[0]);
        v[3] = v[3].max(p[2]);
    }
    b
}

fn area(b: &[f32; 4]) -> f32 {
    (b[2] - b[0]).max(0.0) * (b[3] - b[1]).max(0.0)
}

/// The base's playable box: its largest combat area's (by box), `None` when it has none.
#[must_use]
pub fn playable_box(w: &LevelWriter) -> Option<[f32; 4]> {
    let all: HashSet<String> = w.guids().into_iter().collect();
    area_points(w, &all)
        .iter()
        .filter_map(bbox)
        .max_by(|a, b| area(a).total_cmp(&area(b)))
}

/// The mode layer of `donor` brought into a level (`cur`, the project's partition as its log leaves it): its items
/// with new guids (UUID v5 over `seed`, the donor and the donor's guid) placed by the first guess (module doc: the
/// donor layer's combat-area box into `target`, else the base's largest combat area, heights over `ground`), and the
/// level's old layer to take out ([`old_layer`]).
#[allow(
    clippy::too_many_arguments,
    reason = "the level, the donor and what the choice records"
)]
#[must_use]
pub fn choose(
    cur: &LevelWriter,
    donor: &LevelWriter,
    donor_lv: &str,
    donor_sha1: &str,
    profile: Profile,
    mode: &str,
    seed: &str,
    ground: Option<&Ground<'_>>,
    target: Option<[f32; 4]>,
) -> ModeChoice {
    let donor_lv = donor_lv.to_ascii_lowercase();
    let set = layer(donor);
    let g = graph(donor);
    // the fit: the donor's combat areas (else every placed item) into the base's box
    let pts = area_points(donor, &set);
    let from = bbox(pts.iter().flatten()).or_else(|| {
        let p: Vec<[f32; 3]> = g
            .order
            .iter()
            .filter(|x| set.contains(*x))
            .filter_map(|x| donor.transform(x).map(|r| translation(&r)))
            .collect();
        bbox(&p)
    });
    let to = target.or_else(|| playable_box(cur)).or(from);
    let fit = match (from, to) {
        (Some(f), Some(t)) => {
            let (dw, dh) = ((f[2] - f[0]).max(1.0), (f[3] - f[1]).max(1.0));
            let (tw, th) = ((t[2] - t[0]).max(1.0), (t[3] - t[1]).max(1.0));
            Fit {
                from: f,
                to: t,
                scale: (tw / dw).min(th / dh).min(1.0),
            }
        }
        _ => Fit {
            scale: 1.0,
            ..Fit::default()
        },
    };
    let (fc, tc) = (
        [fit.from[0].midpoint(fit.from[2]), fit.from[1].midpoint(fit.from[3])],
        [fit.to[0].midpoint(fit.to[2]), fit.to[1].midpoint(fit.to[3])],
    );
    let items = g
        .order
        .iter()
        .filter(|x| set.contains(*x))
        .map(|x| {
            let at = donor.transform(x).map(|r| {
                let [px, py, pz] = translation(&r);
                let nx = tc[0] + (px - fc[0]) * fit.scale;
                let nz = tc[1] + (pz - fc[1]) * fit.scale;
                let ny = match ground {
                    Some(gr) => {
                        let above = f64::from(py) - gr.donor.height(f64::from(px), f64::from(pz));
                        (gr.base.height(f64::from(nx), f64::from(nz)) + above) as f32
                    }
                    None => py,
                };
                [nx, ny, nz]
            });
            LayerItem {
                donor: x.clone(),
                guid: mpconvert::graft_guid(seed, &donor_lv, x),
                at,
            }
        })
        .collect();
    ModeChoice {
        profile,
        mode: mode_key(mode),
        donor: donor_lv,
        donor_sha1: donor_sha1.to_owned(),
        items,
        removed: old_layer(cur),
        fit,
    }
}

/// Applies a choice to the level: the old layer out, the donor's layer in (its event connections, `Objects` items
/// and voice-over arrays: [`mpconvert::graft_map`]), each item at its place.
///
/// # Errors
/// An item's guid is already in the level, or the writer refuses.
pub fn apply(w: &mut LevelWriter, c: &ModeChoice, donor: &LevelWriter) -> Result<Converted> {
    let mut out = Converted::default();
    let gone: HashSet<String> = c.removed.iter().map(|g| g.to_ascii_uppercase()).collect();
    let d = w.delete_many(&gone)?;
    out.connections_removed = d.connections;
    out.references_cut = d.references;
    out.removed = vec![("mode layer".into(), d.instances)];
    let have: HashSet<String> = donor.guids().into_iter().collect();
    let map: HashMap<String, String> = c.map().into_iter().filter(|(k, _)| have.contains(k)).collect();
    if map.is_empty() {
        return Err(refused(format!(
            "the donor {} has none of the layer's instances (it changed since the mode was chosen?)",
            c.donor
        )));
    }
    mpconvert::graft_map(w, donor, &map, &mut out)?;
    for it in &c.items {
        let Some(at) = it.at else { continue };
        if let Some(mut rows) = w.transform(&it.guid) {
            rows[3] = at;
            w.set_transform(&it.guid, &rows)?;
        }
    }
    Ok(out)
}

/// The Rush stages of a level: its M-COMs grouped by the stage counters (`MultipleTriggerEntityData`) their events
/// trigger, in the partition's order (upper-case guids).
#[must_use]
pub fn stages(w: &LevelWriter) -> Vec<Vec<String>> {
    let g = graph(w);
    let mut groups: Vec<(BTreeSet<String>, Vec<String>)> = Vec::new();
    for m in mcoms(w, &g) {
        let key: BTreeSet<String> = g
            .events
            .get(&m)
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter(|t| g.ty(t) == "MultipleTriggerEntityData")
            .cloned()
            .collect();
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, v)) => v.push(m),
            None => groups.push((key, vec![m])),
        }
    }
    groups.into_iter().map(|x| x.1).collect()
}

/// What a level's mode layer has: its pieces counted (round E3d's completeness check compares them with the donor's).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LayerSig {
    /// Player, mission and auto-team present.
    pub core: Vec<String>,
    /// The bases (capture points no team can take) of team 1 and team 2.
    pub bases: [usize; 2],
    /// The flags (capture points that can be taken).
    pub flags: usize,
    /// Combat areas.
    pub combat_areas: usize,
    /// Ticket counters (Conquest), life counters (Rush's attacker tickets), kill counters (Squad Deathmatch).
    pub counters: [usize; 3],
    /// Teams (`TeamEntityData`: Squad Deathmatch has its squads).
    pub teams: usize,
    /// Soldier spawns.
    pub spawns: usize,
    /// M-COMs per Rush stage.
    pub stages: Vec<usize>,
}

/// A level's mode layer counted.
#[must_use]
pub fn signature(w: &LevelWriter) -> LayerSig {
    let count = |t: &str| w.d.of_type(format!(".{t}").as_bytes()).len();
    let mut sig = LayerSig {
        core: ["HumanPlayerEntityData", "MissionEntityData", "AutoTeamEntityData"]
            .iter()
            .filter(|t| count(t) > 0)
            .map(|t| (*t).to_owned())
            .collect(),
        combat_areas: count("CombatAreaTriggerEntityData"),
        counters: [
            count("TicketCounterEntityData"),
            count("LifeCounterEntityData"),
            count("KillCounterEntityData"),
        ],
        teams: count("TeamEntityData"),
        spawns: count("SoldierSpawnEntityData"),
        stages: stages(w).iter().map(Vec::len).collect(),
        ..LayerSig::default()
    };
    for c in mpconvert::capture_points(w) {
        if c.base {
            match c.team.as_str() {
                "Team1" => sig.bases[0] += 1,
                "Team2" => sig.bases[1] += 1,
                _ => sig.flags += 1,
            }
        } else {
            sig.flags += 1;
        }
    }
    sig
}

fn n_of(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("one {one}")
    } else {
        format!("{n} {many}")
    }
}

/// What a level's layer lacks for its mode, against the donor's own layer (empty: complete). Conquest may lose
/// flags (one is enough) but keeps a base per team; Rush and Squad Rush keep every stage with the donor's M-COMs,
/// its combat areas and attacker lives; Squad Deathmatch its squads and kill counters.
#[must_use]
pub fn missing(mode: &str, have: &LayerSig, donor: &LayerSig) -> Vec<String> {
    let name = mode_name(mode);
    let mut out = Vec::new();
    for c in &donor.core {
        if !have.core.contains(c) {
            out.push(format!("the layer has no {c}"));
        }
    }
    if have.spawns == 0 {
        out.push(format!("{name} has no soldier spawn"));
    }
    if have.combat_areas == 0 {
        out.push(format!("{name} has no combat area"));
    }
    let teams = |out: &mut Vec<String>| {
        for (k, (h, d)) in have.bases.iter().zip(donor.bases.iter()).enumerate() {
            if *d > 0 && *h == 0 {
                out.push(format!("{name}: team {} has no base", k + 1));
            }
        }
    };
    match mode_key(mode).as_str() {
        "conquest" => {
            teams(&mut out);
            if have.flags == 0 {
                out.push("Conquest needs at least one flag".into());
            }
            if have.counters[0] < donor.counters[0] {
                out.push(format!(
                    "Conquest has {} (the donor's {})",
                    n_of(have.counters[0], "ticket counter", "ticket counters"),
                    donor.counters[0]
                ));
            }
        }
        "goldrush" | "squadrush" => {
            teams(&mut out);
            if have.stages.len() < donor.stages.len() {
                out.push(format!(
                    "{name} has {} (the donor's {})",
                    n_of(have.stages.len(), "stage", "stages"),
                    donor.stages.len()
                ));
            }
            for (k, d) in donor.stages.iter().enumerate() {
                let h = have.stages.get(k).copied().unwrap_or(0);
                if h != *d {
                    out.push(format!(
                        "{name} stage {} has {} (the donor's has {d})",
                        k + 1,
                        n_of(h, "M-COM", "M-COMs")
                    ));
                }
            }
            if have.combat_areas < donor.combat_areas {
                out.push(format!(
                    "{name} has {} (the donor's {}: one a stage)",
                    n_of(have.combat_areas, "combat area", "combat areas"),
                    donor.combat_areas
                ));
            }
            if have.counters[1] < donor.counters[1] {
                out.push(format!(
                    "{name} has {} (the attackers' tickets; the donor's {})",
                    n_of(have.counters[1], "life counter", "life counters"),
                    donor.counters[1]
                ));
            }
        }
        "teamdeathmatch" => {
            if have.teams < donor.teams {
                out.push(format!(
                    "{name} has {} (the donor's {}: the squads)",
                    n_of(have.teams, "team", "teams"),
                    donor.teams
                ));
            }
            if have.counters[2] < donor.counters[2] {
                out.push(format!(
                    "{name} has {} (the donor's {})",
                    n_of(have.counters[2], "kill counter", "kill counters"),
                    donor.counters[2]
                ));
            }
        }
        _ => {}
    }
    out
}

/// What belongs to a flag, base or M-COM and moves with it: a capture point's spawns, alternative spawn points, map
/// markers and area geometry (E3's `place`), an M-COM's interactions, markers, alarm sound and objective props.
#[must_use]
pub fn cluster(w: &LevelWriter, guid: &str) -> Vec<String> {
    let g = graph(w);
    let root = guid.to_ascii_uppercase();
    let follow_refs = [
        "SoldierSpawnEntityData",
        "AlternateSpawnEntityData",
        "MapMarkerEntityData",
        "AreaGeometryEntityData",
        "AabbGeometryEntityData",
        "SphereGeometryEntityData",
    ];
    let follow_events = [
        "GameInteractionEntityData",
        "MapMarkerEntityData",
        "GameSoundEntityData",
        "ReferenceObjectData",
    ];
    let objs = objectives(w, &g);
    let mut out = vec![root.clone()];
    let mut seen: HashSet<String> = out.iter().cloned().collect();
    let mut i = 0;
    while let Some(x) = out.get(i).cloned() {
        i += 1;
        let Some(&(e, _)) = g.by_guid.get(&x) else { continue };
        let mut next: Vec<String> = Vec::new();
        if g.ty(&x) != "ReferenceObjectData" {
            next.extend(
                w.d.walk(e)
                    .into_iter()
                    .filter_map(|n| w.d.attr(n, b"ref").and_then(local_ref))
                    .filter(|r| follow_refs.contains(&g.ty(r))),
            );
        }
        if x == root && objs.contains(&x) {
            next.extend(
                g.events
                    .get(&x)
                    .map_or(&[][..], Vec::as_slice)
                    .iter()
                    .filter(|t| {
                        follow_events.contains(&g.ty(t)) && (g.ty(t) != "ReferenceObjectData" || objs.contains(*t))
                    })
                    .cloned(),
            );
        }
        for n in next {
            if w.transform(&n).is_some() && seen.insert(n.clone()) {
                out.push(n);
            }
        }
    }
    out
}

/// The move of a cluster ([`cluster`]) so its root stands at `to` (x, z): each member moved as much, its height
/// following the ground (`ground`: the level's height at its new place plus its height above the old one).
#[must_use]
pub fn move_cluster(
    w: &LevelWriter,
    guid: &str,
    to: [f32; 2],
    ground: Option<&bc2_formats::terrain::Heightfield>,
) -> Vec<Moved> {
    let members = cluster(w, guid);
    let Some(origin) = w.transform(guid).map(|r| translation(&r)) else {
        return Vec::new();
    };
    let (dx, dz) = (to[0] - origin[0], to[1] - origin[2]);
    members
        .iter()
        .filter_map(|m| {
            let from = w.transform(m)?;
            let [x, y, z] = from[3];
            let (nx, nz) = (x + dx, z + dz);
            let ny = ground.map_or(y, |h| {
                (f64::from(y) - h.height(f64::from(x), f64::from(z)) + h.height(f64::from(nx), f64::from(nz))) as f32
            });
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

/// The anchors of a level's layer, for the form: (guid, what, position): its capture points (`US Base`, `A: ...`)
/// and its M-COMs (`Stage 2 M-COM 1`), in the partition's order.
#[must_use]
pub fn anchors(w: &LevelWriter) -> Vec<(String, String, [f32; 3])> {
    let mut out = Vec::new();
    for e in w.d.of_type(b".CapturePointEntityData") {
        let g = guid_of(&w.d, e);
        let name = w.d.child(e, b"Name").and_then(|x| w.d.text(x)).map_or_else(
            || "capture point".to_owned(),
            |t| String::from_utf8_lossy(t).into_owned(),
        );
        if let Some(r) = w.transform(&g) {
            out.push((g, name, translation(&r)));
        }
    }
    for (k, s) in stages(w).iter().enumerate() {
        for (j, m) in s.iter().enumerate() {
            if let Some(r) = w.transform(m) {
                out.push((m.clone(), format!("Stage {} M-COM {}", k + 1, j + 1), translation(&r)));
            }
        }
    }
    out
}

/// Counts of a set of guids by type (the reports).
#[must_use]
pub fn by_type(w: &LevelWriter, guids: &[String]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for g in guids {
        if let Some(e) = w.find(g) {
            *out.entry(short(&type_of(&w.d, e)).to_owned()).or_default() += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_names_and_keys() {
        assert_eq!(mode_name("goldrush"), "Rush");
        assert_eq!(mode_name("teamdeathmatch"), "Squad Deathmatch");
        assert_eq!(mode_name("squadrush"), "Squad Rush");
        assert_eq!(mode_key("Rush"), "goldrush");
        assert_eq!(mode_key("Squad Deathmatch"), "teamdeathmatch");
        assert_eq!(mode_key("Squad Rush"), "squadrush");
        assert_eq!(mode_key("Conquest"), "conquest");
        assert!(not_offered("onslaught").is_some() && not_offered("goldrush").is_none());
        let d = LayerSig {
            core: vec!["HumanPlayerEntityData".into()],
            bases: [1, 1],
            flags: 0,
            combat_areas: 4,
            counters: [0, 2, 0],
            teams: 3,
            spawns: 16,
            stages: vec![2, 2, 2, 2],
        };
        assert_eq!(missing("goldrush", &d, &d), Vec::<String>::new());
        let mut h = d.clone();
        h.stages = vec![2, 1, 2, 2];
        assert_eq!(
            missing("goldrush", &h, &d),
            ["Rush stage 2 has one M-COM (the donor's has 2)"]
        );
        h.stages = vec![2, 2];
        h.combat_areas = 1;
        let m = missing("goldrush", &h, &d);
        assert!(m.iter().any(|x| x == "Rush has 2 stages (the donor's 4)"), "{m:?}");
        assert!(m.iter().any(|x| x.starts_with("Rush has one combat area")), "{m:?}");
    }
}
