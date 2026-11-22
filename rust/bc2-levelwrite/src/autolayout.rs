//! The optional auto layout of a mode's gameplay (round A1): a proposal that shapes the project's mode layer - its
//! Conquest flags and bases, its Rush stages with their M-COMs and spawns, Squad Rush's, Squad Deathmatch's squads,
//! the combat areas around them - onto the level's ground, in DICE's style. It is offered, never applied: the
//! editor shows it and commits it as one operation of the log (a [`crate::Op::Move`]) only when the user accepts it,
//! so it can be edited and undone like any move.
//!
//! **DICE's style.** The layer the project's target brought in ([`crate::modelayer`]: a stock level's own mode layer,
//! its event logic and every spawn) is DICE's own layout. The auto layout keeps its shape - the distances between
//! flags, bases, M-COMs and spawns, the stages' order, each team's areas - and fits it to the level:
//!
//! 1. **Search.** The whole layer is turned (24 bearings, the seed's offset), scaled (0.9, 1, 1.1 of the donor's own
//!    distances: [`Ask::layer_scale`] undoes the target's first guess) and placed at every candidate centre (a 24 m
//!    grid over the terrain where the level has art, the seed's jitter). Each candidate is scored on the ground alone
//!    ([`crate::site`]): flags, bases, M-COMs and spawn points over water, outside the terrain, on slopes past 35
//!    degrees or in collision; the combat areas' outlines outside the terrain; how much of the combat areas is land and
//!    how much of it holds the level's art; the art at the objectives; the scale away from DICE's own. The best 48
//!    distinct candidates are refined (bearing, centre and scale stepped down) on every point.
//! 2. **Fit.** The best 8 are applied to the level and fitted exactly: a flag, base or M-COM some of whose points
//!    stand in water, rocks or outside the terrain is moved with all that belongs to it (up to 40 m); a spawn point
//!    still in a bad spot is moved alone (up to 16 m). Heights follow the ground (each object keeps its height above
//!    it). The fit is stricter than the rules: no part of a soldier in collision.
//! 3. **Check.** Each fitted layout is measured with V1's [`crate::placement::measure`] and held against L1's rules
//!    and statistics (`bfbc2-map-editor/placement/l1_a1_params.json`: DICE's ranges per mode) and the site rules of
//!    the export's validation (XVAL-PLACE-2 to 4). The proposal has the fewest warnings and bad spots, then the
//!    fewest statistics outside DICE's ranges, then the fewest notes, then the best ground score plus the statistics'
//!    distance from DICE's medians; ties go to the earlier candidate.
//!
//! Deterministic: the same level, layer, site and seed give the same proposal (no hash order, no clock, no thread
//! order reaches it; the seed is a `SplitMix64` stream).
#![allow(
    clippy::cast_precision_loss,
    reason = "counts of points and candidates as f64: far below 2^52"
)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

use bc2_formats::dbx::edit::NodeId;
use serde::{Deserialize, Serialize};

use crate::mpconvert::{guid_of, local_ref, short, type_of};
use crate::ops::{Moved, Rows};
use crate::placement::{self, Measure, Mode};
use crate::project::Profile;
use crate::rules::{Finding, Severity};
use crate::site::{Routes, Site, Spot};
use crate::write::{LevelWriteError, LevelWriter, Result};

/// What the auto layout is asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ask {
    /// The mode the layer is played as.
    pub mode: Mode,
    /// The era (L1's rules and ranges per era group).
    pub profile: Profile,
    /// The seed (0: the default search).
    pub seed: u64,
    /// How large the layer is now against its donor's own layout (the target's first guess scales a large donor
    /// down to the level: [`crate::modelayer::Fit::scale`]; 1 when it is as DICE made it). The search's scales are
    /// DICE's own (0.9 to 1.1 of the donor's distances), not the first guess's.
    pub layer_scale: f64,
}

impl Ask {
    /// An ask for a layer as large as its donor's.
    #[must_use]
    pub fn new(mode: Mode, profile: Profile, seed: u64) -> Self {
        Self {
            mode,
            profile,
            seed,
            layer_scale: 1.0,
        }
    }

    /// The ask for a project's mode choice: its mode, era and the first guess's scale.
    #[must_use]
    pub fn of_choice(c: &crate::modelayer::ModeChoice, seed: u64) -> Option<Self> {
        let s = f64::from(c.fit.scale);
        Some(Self {
            mode: Mode::of_key(&c.mode)?,
            profile: c.profile,
            seed,
            layer_scale: if s > 0.0 && s.is_finite() { s } else { 1.0 },
        })
    }
}

/// One of DICE's statistics on the proposal: its value(s) against DICE's range.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stat {
    /// L1's metric (`cq.flag_nn_dist`).
    pub metric: String,
    /// L1's statistic it is held against (`flag_nn_m`).
    pub param: String,
    /// The values measured on the proposal.
    pub values: Vec<f64>,
    /// DICE's range (min, max) and median.
    pub dice: [f64; 3],
    /// How many values are inside DICE's range.
    pub inside: usize,
}

/// The proposal: the moves that lay the layer out, how, and what it measures.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Proposal {
    /// The moves (one [`crate::Op::Move`] when accepted), in the partition's order.
    pub moves: Vec<Moved>,
    /// The seed.
    pub seed: u64,
    /// The layer's turn (degrees), scale and centre (x, z).
    pub turn: f64,
    /// The scale of its distances.
    pub scale: f64,
    /// Where its centre stands.
    pub centre: [f64; 2],
    /// Flags, bases or M-COMs moved with what belongs to them to fit the ground.
    pub clusters_fitted: usize,
    /// Spawn points moved alone.
    pub points_fitted: usize,
    /// Points still in a bad spot (0 on every layout proposed for the stock levels).
    pub bad_spots: usize,
    /// The findings of L1's rules and the site rules on the proposal.
    pub findings: Vec<Finding>,
    /// DICE's statistics on it.
    pub stats: Vec<Stat>,
    /// Candidates scored.
    pub candidates: usize,
    /// The ground score of the chosen candidate (lower is better).
    pub score: f64,
    /// The vehicles placed (round A2): the level's own spawners moved, copies of them, the capture points they join.
    #[serde(default)]
    pub vehicles: crate::vehicles::Plan,
    /// The objects the user pinned that the layout kept where they stand (round A2).
    #[serde(default)]
    pub pinned: usize,
}

impl Proposal {
    /// (warnings, notes) among the findings.
    #[must_use]
    pub fn counts(&self) -> (usize, usize) {
        let n = |s: Severity| self.findings.iter().filter(|f| f.severity == s).count();
        (n(Severity::Warning) + n(Severity::Error), n(Severity::Note))
    }

    /// The statistics' values inside DICE's ranges, of all.
    #[must_use]
    pub fn inside(&self) -> (usize, usize) {
        (
            self.stats.iter().map(|s| s.inside).sum(),
            self.stats.iter().map(|s| s.values.len()).sum(),
        )
    }

    /// The proposal in words (the editor's label of the operation, the reports).
    #[must_use]
    pub fn describe(&self) -> String {
        let (w, n) = self.counts();
        let (a, b) = self.inside();
        let v = &self.vehicles;
        let vehicles = if v.vehicles.is_empty() {
            String::new()
        } else {
            format!(
                ", {} vehicles ({} new)",
                v.vehicles.len(),
                v.vehicles.iter().filter(|x| x.copy).count()
            )
        };
        format!(
            "Auto layout (seed {}): turned {:.0} deg, scale {:.2}, {} objects moved{vehicles}; {w} warnings, {n} notes; \
             {a} of {b} statistics inside DICE's ranges",
            self.seed,
            self.turn,
            self.scale,
            self.moves.len() + v.moves.len()
        )
    }

    /// The operation that commits it: one move (A1's), or with vehicles one multi-part operation of the partition -
    /// the moves (the layer's and the vehicle spawners'), the copies placed, the links to their capture points -
    /// applied whole and undone in one step.
    #[must_use]
    pub fn op(&self) -> crate::Op {
        use crate::files::FileEdit;
        let v = &self.vehicles;
        let mut moves = self.moves.clone();
        moves.extend(v.moves.iter().cloned());
        if v.places.is_empty() && v.links.is_empty() {
            return crate::Op::Move { items: moves };
        }
        let mut edits = Vec::new();
        if !moves.is_empty() {
            edits.push(FileEdit::Partition {
                op: Box::new(crate::Op::Move { items: moves }),
            });
        }
        if !v.places.is_empty() {
            edits.push(FileEdit::Partition {
                op: Box::new(crate::Op::Place {
                    items: v.places.clone(),
                }),
            });
        }
        for (point, spawner) in &v.links {
            edits.push(FileEdit::Partition {
                op: Box::new(crate::Op::LinkSpawner {
                    point: point.clone(),
                    spawner: spawner.clone(),
                    linked: true,
                }),
            });
        }
        crate::Op::Files { edits }
    }
}

// ------------------------------------------------------------------------------------------------ DICE's ranges

/// L1's typical layout per mode (`l1_a1_params.json`): statistic -> (median, p25, p75, min, max).
fn params() -> &'static HashMap<String, HashMap<String, [f64; 5]>> {
    static P: OnceLock<HashMap<String, HashMap<String, [f64; 5]>>> = OnceLock::new();
    P.get_or_init(|| {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../../../bfbc2-map-editor/placement/l1_a1_params.json"))
                .unwrap_or_default();
        let mut out = HashMap::new();
        for (group, stats) in v.as_object().into_iter().flatten() {
            let mut m = HashMap::new();
            for (k, s) in stats.as_object().into_iter().flatten() {
                let f = |n: &str| s.get(n).and_then(serde_json::Value::as_f64);
                if let (Some(a), Some(b), Some(c), Some(d), Some(e)) =
                    (f("median"), f("p25"), f("p75"), f("min"), f("max"))
                {
                    m.insert(k.clone(), [a, b, c, d, e]);
                }
            }
            out.insert(group.clone(), m);
        }
        out
    })
}

/// The metrics of [`placement::measure`] held against L1's statistics, per mode: (metric, statistic, a filter).
fn stat_map(mode: Mode) -> &'static [(&'static str, &'static str, &'static str)] {
    match mode {
        Mode::Conquest => &[
            ("cq.flag_count", "flags", ""),
            ("cq.flag_nn_dist", "flag_nn_m", ""),
            ("cq.hq_nearest_flag", "hq_nearest_flag_m", ""),
            ("cq.hq_hq_dist", "hq_hq_m", ""),
            ("cq.flag_axis_t", "flag_axis_t", ""),
            ("cq.flag_axis_lateral", "flag_lateral", ""),
            ("cq.capture_zone", "capture_radius_m", "noaabb"),
            ("cq.flag_spawn_points", "flag_spawn_points", ""),
            ("cq.flag_spawn_dist_max", "flag_spawn_dist_max_m", ""),
            ("cq.hq_spawn_points", "hq_spawn_points", ""),
            ("cq.combat_area_union", "combat_area_m2", ""),
            ("cq.flag_margin", "flag_margin_m", ""),
            ("cq.hq_margin", "hq_margin_m", ""),
            ("cq.hq_vehicle_pool", "hq_vehicle_pool_150m", ""),
            ("cq.flag_vehicle_pool", "flag_vehicle_pool_150m", ""),
        ],
        Mode::Rush | Mode::SquadRush => &[
            ("rush.stage_count", "stages", ""),
            ("rush.mcom_pair_dist", "mcom_pair_m", ""),
            ("rush.stage_step", "stage_step_m", ""),
            (
                "rush.attacker_spawn_to_obj_min",
                "attacker_spawn_to_obj_min_m_stage1",
                "stage1",
            ),
            (
                "rush.attacker_spawn_to_obj_min",
                "attacker_spawn_to_obj_min_m_later",
                "later",
            ),
            ("rush.defender_spawn_to_obj_min", "defender_spawn_to_obj_min_m", ""),
            ("rush.defender_spawn_depth", "defender_spawn_depth_m", ""),
            ("rush.attacker_spawn_depth", "attacker_spawn_depth_m", ""),
            ("rush.obj_margin_attacker_area", "obj_margin_attacker_area_m", ""),
            ("rush.obj_margin_defender_area", "obj_margin_defender_area_m", ""),
            ("rush.lives", "attacker_lives", "attacker"),
            ("rush.attacker_vehicles", "attacker_vehicles_per_stage", ""),
            ("rush.defender_vehicles", "defender_vehicles_per_stage", ""),
        ],
        Mode::SquadDeathmatch => &[
            ("sdm.squad_count", "squads", ""),
            ("sdm.spawn_points", "spawn_points", ""),
            ("sdm.spawn_nn_median", "spawn_nn_median_m", ""),
            ("sdm.combat_area_union", "combat_area_m2", ""),
            ("sdm.vehicles", "vehicles", ""),
        ],
    }
}

/// DICE's statistics on measurements of a layout played as `mode` in `profile`'s era.
#[must_use]
pub fn stats(measures: &[Measure], mode: Mode, profile: Profile) -> Vec<Stat> {
    let group = match (mode, profile) {
        (Mode::Conquest, Profile::Ww2) => "CQ WW2",
        (Mode::Conquest, _) => "CQ DICE",
        (Mode::Rush, _) => "Rush",
        (Mode::SquadRush, _) => "SR",
        (Mode::SquadDeathmatch, _) => "SDM",
    };
    let Some(p) = params().get(group) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for &(metric, param, filter) in stat_map(mode) {
        let Some(&[med, _, _, lo, hi]) = p.get(param) else {
            continue;
        };
        let metric_here = if mode == Mode::SquadRush {
            metric.replacen("rush.", "sr.", 1)
        } else {
            metric.to_owned()
        };
        let values: Vec<f64> = measures
            .iter()
            .filter(|m| m.metric == metric_here)
            .filter(|m| match filter {
                "noaabb" => m.tag != "aabb",
                "stage1" => m.stage == Some(1),
                "later" => m.stage.is_some_and(|k| k > 1),
                "attacker" => m.tag == "attacker",
                _ => true,
            })
            .map(|m| (m.value * 10.0).round() / 10.0)
            .collect();
        if values.is_empty() {
            continue;
        }
        let inside = values.iter().filter(|v| **v >= lo && **v <= hi).count();
        out.push(Stat {
            metric: metric_here,
            param: param.to_owned(),
            values,
            dice: [lo, hi, med],
            inside,
        });
    }
    out
}

/// How far the statistics are from DICE's medians: the mean of |value - median| / (p75 - p25), each value outside
/// DICE's range counting 4 more.
fn stat_cost(stats: &[Stat], mode: Mode, profile: Profile) -> f64 {
    let group = match (mode, profile) {
        (Mode::Conquest, Profile::Ww2) => "CQ WW2",
        (Mode::Conquest, _) => "CQ DICE",
        (Mode::Rush, _) => "Rush",
        (Mode::SquadRush, _) => "SR",
        (Mode::SquadDeathmatch, _) => "SDM",
    };
    let Some(p) = params().get(group) else { return 0.0 };
    let (mut sum, mut n) = (0.0, 0.0);
    for s in stats {
        let Some(&[med, q1, q3, lo, hi]) = p.get(&s.param) else {
            continue;
        };
        let iqr = (q3 - q1).max(med.abs() * 0.1).max(1e-6);
        for v in &s.values {
            sum += ((v - med).abs() / iqr).min(4.0) + if *v < lo || *v > hi { 4.0 } else { 0.0 };
            n += 1.0;
        }
    }
    if n > 0.0 { sum / n } else { 0.0 }
}

// ------------------------------------------------------------------------------------------------ the layer

/// What a point of the layer is, for the ground checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// A flag, base or M-COM.
    Anchor,
    /// A place a soldier spawns.
    Spawn,
    /// A combat area's outline point.
    Outline,
    /// Anything else that moves with the layer.
    Other,
}

#[derive(Debug, Clone)]
struct Item {
    guid: String,
    node: NodeId,
    rows: Rows,
    role: Role,
    /// Its height above the ground (or the surface under it) where it stands now.
    above: f64,
    /// The cluster it moves with (an index into `Layer::clusters`).
    cluster: Option<usize>,
}

#[derive(Debug, Clone)]
struct Layer {
    items: Vec<Item>,
    /// Each flag's, base's or M-COM's items (indices into `items`; the anchor first).
    clusters: Vec<Vec<usize>>,
    /// The combat areas' outlines (indices into `items`).
    outlines: Vec<Vec<usize>>,
    pivot: [f64; 2],
}

fn refs_of(w: &LevelWriter, e: NodeId, field: &[u8]) -> Vec<String> {
    w.d.child(e, field)
        .and_then(|a| w.d.children(a))
        .unwrap_or_default()
        .iter()
        .filter_map(|&i| w.d.attr(i, b"ref").and_then(local_ref))
        .collect()
}

#[allow(
    clippy::too_many_lines,
    reason = "the layer read in one pass: spawns, outlines, anchors, clusters"
)]
fn read_layer(w: &LevelWriter, site: &Site) -> Layer {
    let set = crate::modelayer::layer(w);
    let mut nodes: HashMap<String, (NodeId, String)> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for e in w.d.instances() {
        let g = guid_of(&w.d, e);
        if set.contains(&g) {
            order.push(g.clone());
            nodes.insert(g, (e, short(&type_of(&w.d, e)).to_owned()));
        }
    }
    let ty = |g: &str| nodes.get(g).map_or("", |x| x.1.as_str());
    // the spawn points: soldier spawners' alternatives, a spawner without any itself
    let mut spawn_points: HashSet<String> = HashSet::new();
    for g in &order {
        if ty(g) != "SoldierSpawnEntityData" {
            continue;
        }
        let Some(&(e, _)) = nodes.get(g) else { continue };
        let alts: Vec<String> = refs_of(w, e, b"AlternativeSpawnPoints")
            .into_iter()
            .filter(|a| nodes.contains_key(a))
            .collect();
        if alts.is_empty() {
            spawn_points.insert(g.clone());
        } else {
            spawn_points.extend(alts);
        }
    }
    // the outlines of the combat areas
    let mut outline_guids: Vec<Vec<String>> = Vec::new();
    for g in &order {
        if ty(g) != "CombatAreaTriggerEntityData" {
            continue;
        }
        let Some(&(e, _)) = nodes.get(g) else { continue };
        for f in [&b"Geometry"[..], b"SurroundingGeometry"] {
            for s in refs_of(w, e, f) {
                let chain = crate::gameplay::outline(w, &s);
                if chain.len() >= 3 && !outline_guids.iter().any(|o| o.contains(&s)) {
                    outline_guids.push(chain);
                }
            }
        }
    }
    let on_outline: HashSet<&String> = outline_guids.iter().flatten().collect();
    // the anchors: capture points, then the M-COMs by stage
    let mut anchors: Vec<String> = order
        .iter()
        .filter(|g| ty(g) == "CapturePointEntityData")
        .cloned()
        .collect();
    anchors.extend(crate::modelayer::stages(w).into_iter().flatten());
    let mut items: Vec<Item> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for g in &order {
        let Some(&(e, _)) = nodes.get(g) else { continue };
        let Some(rows) = w.transform_at(e) else { continue };
        let [x, y, z] = rows[3].map(f64::from);
        if x == 0.0 && y == 0.0 && z == 0.0 {
            // the layer's logic at the origin stays there
            continue;
        }
        let role = if anchors.contains(g) {
            Role::Anchor
        } else if spawn_points.contains(g) {
            Role::Spawn
        } else if on_outline.contains(g) {
            Role::Outline
        } else {
            Role::Other
        };
        let base = site.support([x, y, z]).or_else(|| site.ground(x, z)).unwrap_or(y);
        at.insert(g.clone(), items.len());
        items.push(Item {
            guid: g.clone(),
            node: e,
            rows,
            role,
            above: y - base,
            cluster: None,
        });
    }
    let mut clusters: Vec<Vec<usize>> = Vec::new();
    for a in &anchors {
        let Some(&ai) = at.get(a) else { continue };
        if items.get(ai).is_some_and(|i| i.cluster.is_some()) {
            continue;
        }
        let k = clusters.len();
        let mut members = vec![ai];
        for m in crate::modelayer::cluster(w, a) {
            if let Some(&mi) = at.get(&m)
                && mi != ai
                && items
                    .get(mi)
                    .is_some_and(|i| i.cluster.is_none() && i.role != Role::Outline)
            {
                members.push(mi);
            }
        }
        for &mi in &members {
            if let Some(i) = items.get_mut(mi) {
                i.cluster = Some(k);
            }
        }
        clusters.push(members);
    }
    let outlines: Vec<Vec<usize>> = outline_guids
        .iter()
        .map(|o| o.iter().filter_map(|g| at.get(g).copied()).collect())
        .collect();
    // the pivot: the box of the combat areas (else of everything)
    let pts: Vec<[f64; 2]> = {
        let o: Vec<[f64; 2]> = outlines
            .iter()
            .flatten()
            .filter_map(|&i| items.get(i))
            .map(|i| [f64::from(i.rows[3][0]), f64::from(i.rows[3][2])])
            .collect();
        if o.is_empty() {
            items
                .iter()
                .map(|i| [f64::from(i.rows[3][0]), f64::from(i.rows[3][2])])
                .collect()
        } else {
            o
        }
    };
    let b = pts.iter().fold(
        [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
        |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])],
    );
    let pivot = if b[0].is_finite() {
        [f64::midpoint(b[0], b[2]), f64::midpoint(b[1], b[3])]
    } else {
        [0.0, 0.0]
    };
    Layer {
        items,
        clusters,
        outlines,
        pivot,
    }
}

// ------------------------------------------------------------------------------------------------ the search

/// `SplitMix64`: the seed's stream.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in [-1, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

/// A similarity in plan: turn (radians), scale, the centre the pivot goes to.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Fit {
    turn: f64,
    scale: f64,
    centre: [f64; 2],
}

impl Fit {
    fn apply(&self, pivot: [f64; 2], p: [f64; 2]) -> [f64; 2] {
        let (s, c) = self.turn.sin_cos();
        let (dx, dz) = ((p[0] - pivot[0]) * self.scale, (p[1] - pivot[1]) * self.scale);
        [self.centre[0] + dx * c + dz * s, self.centre[1] - dx * s + dz * c]
    }

    /// The fit of a turn and scale that keeps pinned points where they stand (round A2): the centre that puts them,
    /// on average, on their own places, and how far (metres, mean) they are from them then.
    fn pinned(turn: f64, scale: f64, pivot: [f64; 2], pins: &[[f64; 2]]) -> (Self, f64) {
        let mut f = Self {
            turn,
            scale,
            centre: [0.0, 0.0],
        };
        if pins.is_empty() {
            return (f, 0.0);
        }
        let n = pins.len() as f64;
        let mut c = [0.0, 0.0];
        for p in pins {
            let q = f.apply(pivot, *p);
            c[0] += (p[0] - q[0]) / n;
            c[1] += (p[1] - q[1]) / n;
        }
        f.centre = c;
        let off = pins
            .iter()
            .map(|p| {
                let q = f.apply(pivot, *p);
                (q[0] - p[0]).hypot(q[1] - p[1])
            })
            .sum::<f64>()
            / n;
        (f, off)
    }

    /// A direction (a row of a transform) turned.
    fn turn_row(&self, r: [f32; 3]) -> [f32; 3] {
        let (s, c) = self.turn.sin_cos();
        let (x, z) = (f64::from(r[0]), f64::from(r[2]));
        [(x * c + z * s) as f32, r[1], (-x * s + z * c) as f32]
    }
}

/// The layer's points in plan for scoring: role, (x, z) and, for the combat areas, samples of their inside.
struct Samples {
    anchors: Vec<[f64; 2]>,
    spawns: Vec<[f64; 2]>,
    outline: Vec<[f64; 2]>,
    area: Vec<[f64; 2]>,
}

fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
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

fn samples(l: &Layer, cap: usize) -> Samples {
    let xz = |i: &Item| [f64::from(i.rows[3][0]), f64::from(i.rows[3][2])];
    let of = |r: Role| -> Vec<[f64; 2]> { l.items.iter().filter(|i| i.role == r).map(xz).collect() };
    let polys: Vec<Vec<[f64; 2]>> = l
        .outlines
        .iter()
        .map(|o| o.iter().filter_map(|&i| l.items.get(i)).map(xz).collect())
        .collect();
    let all: Vec<[f64; 2]> = polys.iter().flatten().copied().collect();
    let b = all.iter().fold(
        [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
        |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])],
    );
    let mut area = Vec::new();
    if b[0].is_finite() {
        let step = (((b[2] - b[0]) * (b[3] - b[1]) / cap as f64).sqrt()).max(4.0);
        let mut z = b[1] + step / 2.0;
        while z < b[3] {
            let mut x = b[0] + step / 2.0;
            while x < b[2] {
                if polys.iter().any(|p| inside([x, z], p)) {
                    area.push([x, z]);
                }
                x += step;
            }
            z += step;
        }
    }
    Samples {
        anchors: of(Role::Anchor),
        spawns: of(Role::Spawn),
        outline: all,
        area,
    }
}

/// The ground on a 4 m raster for the search: whether a soldier stands (terrain inside, not over deep water, a slope
/// below 35 degrees, not in collision a little above the ground).
struct Walk {
    x0: f64,
    z0: f64,
    n: [usize; 2],
    ok: Vec<bool>,
    land: Vec<bool>,
}

const WALK_CELL: f64 = 4.0;

impl Walk {
    fn new(site: &Site, routes: Option<&Routes>) -> Self {
        let [x0, z0, x1, z1] = site.bounds;
        let n = [
            (((x1 - x0) / WALK_CELL).ceil() as usize).max(1),
            (((z1 - z0) / WALK_CELL).ceil() as usize).max(1),
        ];
        let mut ok = vec![false; n[0] * n[1]];
        let mut land = vec![false; n[0] * n[1]];
        for j in 0..n[1] {
            for i in 0..n[0] {
                let x = x0 + (i as f64 + 0.5) * WALK_CELL;
                let z = z0 + (j as f64 + 0.5) * WALK_CELL;
                let Some(g) = site.ground(x, z) else { continue };
                let p = [x, g + 0.3, z];
                let wet = site.water(x, z).is_some_and(|w| w - g > 1.2);
                if let Some(l) = land.get_mut(j * n[0] + i) {
                    *l = !wet;
                }
                let good = !wet
                    && site.slope(x, z).is_some_and(|s| s < 35.0)
                    && site.spot(p) == Spot::Ground
                    && site.buried(p).is_none()
                    && routes.is_none_or(|r| matches!(r.reach([x, g, z]), Some((0, _))));
                if let Some(o) = ok.get_mut(j * n[0] + i) {
                    *o = good;
                }
            }
        }
        Self { x0, z0, n, ok, land }
    }

    fn cell(&self, p: [f64; 2]) -> Option<usize> {
        let i = ((p[0] - self.x0) / WALK_CELL).floor();
        let j = ((p[1] - self.z0) / WALK_CELL).floor();
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.n[0] && (j as usize) < self.n[1])
            .then(|| j as usize * self.n[0] + i as usize)
    }

    fn ok(&self, p: [f64; 2]) -> bool {
        self.cell(p).and_then(|c| self.ok.get(c).copied()).unwrap_or(false)
    }

    fn land(&self, p: [f64; 2]) -> bool {
        self.cell(p).and_then(|c| self.land.get(c).copied()).unwrap_or(false)
    }
}

/// A candidate's ground score (lower is better).
fn score(f: &Fit, pivot: [f64; 2], s: &Samples, walk: &Walk, site: &Site, base: f64) -> f64 {
    let frac = |pts: &[[f64; 2]], bad: &dyn Fn([f64; 2]) -> bool| -> f64 {
        if pts.is_empty() {
            0.0
        } else {
            pts.iter().filter(|p| bad(f.apply(pivot, **p))).count() as f64 / pts.len() as f64
        }
    };
    let [x0, z0, x1, z1] = site.bounds;
    let margin = 16.0;
    let outside = |p: [f64; 2]| {
        p[0] < x0 + margin
            || p[0] > x1 - margin
            || p[1] < z0 + margin
            || p[1] > z1 - margin
            || p[0].abs() > 2000.0
            || p[1].abs() > 2000.0
    };
    let bad_anchor = frac(&s.anchors, &|p| !walk.ok(p));
    let bad_spawn = frac(&s.spawns, &|p| !walk.ok(p));
    let out = frac(&s.outline, &outside);
    let wet = frac(&s.area, &|p| !walk.land(p));
    let mean_art = |pts: &[[f64; 2]]| {
        if pts.is_empty() {
            1.0
        } else {
            pts.iter()
                .map(|p| {
                    let q = f.apply(pivot, *p);
                    site.art(q[0], q[1])
                })
                .sum::<f64>()
                / pts.len() as f64
        }
    };
    let art_area = mean_art(&s.area);
    let art_anchor = mean_art(&s.anchors);
    15.0 * bad_anchor
        + 6.0 * bad_spawn
        + 30.0 * out
        + 6.0 * wet
        + 12.0 * (1.0 - art_area)
        + 6.0 * (1.0 - art_anchor)
        + 6.0 * (f.scale * base).ln().abs()
}

/// A thinned copy of a sample set (every k-th point) for the coarse pass.
fn thin(s: &Samples, n: usize) -> Samples {
    let t = |v: &[[f64; 2]]| -> Vec<[f64; 2]> {
        let k = v.len().div_ceil(n).max(1);
        v.iter().step_by(k).copied().collect()
    };
    Samples {
        anchors: s.anchors.clone(),
        spawns: t(&s.spawns),
        outline: t(&s.outline),
        area: t(&s.area),
    }
}

/// The best distinct candidates (centres 48 m or bearings 20 degrees apart), best first.
#[allow(clippy::too_many_lines, reason = "the coarse pass and the refinement in one piece")]
fn search(
    l: &Layer,
    site: &Site,
    walk: &Walk,
    seed: u64,
    base: f64,
    keep: usize,
    pins: &[[f64; 2]],
) -> (Vec<(f64, Fit)>, usize) {
    // pinned points (round A2): each turn and scale has its one centre; a pin left off its place costs 0.2 a metre
    let pin_cost = |f: &Fit| {
        if pins.is_empty() {
            (*f, 0.0)
        } else {
            let (g, off) = Fit::pinned(f.turn, f.scale, l.pivot, pins);
            (g, 0.2 * off)
        }
    };
    let score = |f: &Fit, s: &Samples| -> (f64, Fit) {
        let (g, extra) = pin_cost(f);
        (score(&g, l.pivot, s, walk, site, base) + extra, g)
    };
    let full = samples(l, 400);
    let coarse = thin(&full, 60);
    let mut rng = Rng(seed);
    let offset = if seed == 0 { 0.0 } else { rng.unit() * 7.5 };
    let turns: Vec<f64> = (0..24).map(|k| (f64::from(k) * 15.0 + offset).to_radians()).collect();
    // DICE's own distances (the donor's), 0.9 to 1.1 of them: the layer's scale now is `base` of the donor's
    let scales = [1.0 / base, 0.9 / base, 1.1 / base];
    let [x0, z0, x1, z1] = site.bounds;
    let step = 24.0;
    let mut centres: Vec<[f64; 2]> = Vec::new();
    let mut z = z0 + step / 2.0;
    while z < z1 {
        let mut x = x0 + step / 2.0;
        while x < x1 {
            let (jx, jz) = if seed == 0 {
                (0.0, 0.0)
            } else {
                (rng.unit() * step / 2.0, rng.unit() * step / 2.0)
            };
            let c = [x + jx, z + jz];
            if site.art(c[0], c[1]) > 0.05 && walk.land(c) {
                centres.push(c);
            }
            x += step;
        }
        z += step;
    }
    if !pins.is_empty() {
        // the centre follows from the turn and the scale
        centres = vec![l.pivot];
    }
    if centres.is_empty() {
        let mut z = z0 + step / 2.0;
        while z < z1 {
            let mut x = x0 + step / 2.0;
            while x < x1 {
                centres.push([x, z]);
                x += step;
            }
            z += step;
        }
    }
    // the coarse pass
    let mut scored: Vec<(f64, Fit)> = Vec::new();
    for &scale in &scales {
        for &turn in &turns {
            for &centre in &centres {
                scored.push(score(&Fit { turn, scale, centre }, &coarse));
            }
        }
    }
    let n = scored.len();
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    let distinct = |picked: &[(f64, Fit)], f: &Fit| {
        picked.iter().all(|(_, g)| {
            let d = (g.centre[0] - f.centre[0]).hypot(g.centre[1] - f.centre[1]);
            let mut a = (g.turn - f.turn).abs() % std::f64::consts::TAU;
            if a > std::f64::consts::PI {
                a = std::f64::consts::TAU - a;
            }
            d > 48.0 || a > 20f64.to_radians()
        })
    };
    let mut top: Vec<(f64, Fit)> = Vec::new();
    for (sc, f) in scored {
        if top.len() >= 48 {
            break;
        }
        if distinct(&top, &f) {
            top.push((sc, f));
        }
    }
    // refine each on every point: bearing, centre and scale stepped down
    let mut refined: Vec<(f64, Fit)> = top
        .into_iter()
        .map(|(_, f0)| {
            let (mut best, mut f) = score(&f0, &full);
            let (mut dt, mut dc, mut ds) = (5f64.to_radians(), if pins.is_empty() { 8.0 } else { 0.0 }, 0.04);
            for _ in 0..4 {
                let mut moved = true;
                while moved {
                    moved = false;
                    let tries = [
                        Fit { turn: f.turn + dt, ..f },
                        Fit { turn: f.turn - dt, ..f },
                        Fit {
                            centre: [f.centre[0] + dc, f.centre[1]],
                            ..f
                        },
                        Fit {
                            centre: [f.centre[0] - dc, f.centre[1]],
                            ..f
                        },
                        Fit {
                            centre: [f.centre[0], f.centre[1] + dc],
                            ..f
                        },
                        Fit {
                            centre: [f.centre[0], f.centre[1] - dc],
                            ..f
                        },
                        Fit {
                            scale: (f.scale + ds / base).min(1.15 / base),
                            ..f
                        },
                        Fit {
                            scale: (f.scale - ds / base).max(0.85 / base),
                            ..f
                        },
                    ];
                    for t in tries {
                        let (s, t) = score(&t, &full);
                        if s + 1e-9 < best {
                            best = s;
                            f = t;
                            moved = true;
                        }
                    }
                }
                dt /= 2.0;
                dc /= 2.0;
                ds /= 2.0;
            }
            (best, f)
        })
        .collect();
    refined.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut out: Vec<(f64, Fit)> = Vec::new();
    for (sc, f) in refined {
        if out.len() >= keep {
            break;
        }
        if distinct(&out, &f) {
            out.push((sc, f));
        }
    }
    (out, n)
}

// ------------------------------------------------------------------------------------------------ the fit

/// An item's rows under a fit, its height following the ground.
fn placed(it: &Item, f: &Fit, pivot: [f64; 2], site: &Site) -> Rows {
    let p = [f64::from(it.rows[3][0]), f64::from(it.rows[3][2])];
    let q = f.apply(pivot, p);
    let mut r = it.rows;
    r[0] = f.turn_row(r[0]);
    r[1] = f.turn_row(r[1]);
    r[2] = f.turn_row(r[2]);
    let y = site
        .ground(q[0], q[1])
        .map_or(f64::from(it.rows[3][1]), |g| g + it.above);
    r[3] = [q[0] as f32, y as f32, q[1] as f32];
    r
}

/// The rows moved in plan by (dx, dz), the height following the ground.
fn shifted(r: &Rows, above: f64, d: [f64; 2], site: &Site) -> Rows {
    let mut out = *r;
    let (x, z) = (f64::from(r[3][0]) + d[0], f64::from(r[3][2]) + d[1]);
    let y = site.ground(x, z).map_or(f64::from(r[3][1]), |g| g + above);
    out[3] = [x as f32, y as f32, z as f32];
    out
}

fn bad(site: &Site, routes: Option<&Routes>, it: &Item, r: &Rows) -> bool {
    let p = r[3].map(f64::from);
    match it.role {
        // stricter than the rules: no part of a soldier in collision, no slope past 35 degrees; with path data, on
        // the main route network (XVAL-PLACE-5)
        Role::Anchor | Role::Spawn => {
            site.spot(p) != Spot::Ground
                || site.buried([p[0], p[1] + 0.3, p[2]]).is_some()
                || site.slope(p[0], p[2]).is_none_or(|s| s > 35.0)
                || routes.is_some_and(|rt| !matches!(rt.reach(p), Some((0, _))))
        }
        Role::Outline | Role::Other => false,
    }
}

/// A fitted layout: every item's rows, and how many clusters and points were moved to fit.
struct Fitted {
    rows: Vec<Rows>,
    clusters: usize,
    points: usize,
    bad: usize,
}

#[allow(
    clippy::too_many_lines,
    reason = "the pins, the clusters and the spawn points fitted in turn"
)]
fn fit(l: &Layer, f: &Fit, site: &Site, routes: Option<&Routes>, pinned: &[bool]) -> Fitted {
    let is_pinned = |i: usize| pinned.get(i).copied().unwrap_or(false);
    let mut rows: Vec<Rows> = l.items.iter().map(|it| placed(it, f, l.pivot, site)).collect();
    // pinned objects stay where they stand (round A2); a pinned flag's, base's or M-COM's own spawns and markers
    // follow it
    for members in &l.clusters {
        let Some(&a) = members.first() else { continue };
        let (Some(it), Some(now)) = (l.items.get(a), rows.get(a).copied()) else {
            continue;
        };
        if !is_pinned(a) {
            continue;
        }
        let d = [
            f64::from(it.rows[3][0]) - f64::from(now[3][0]),
            f64::from(it.rows[3][2]) - f64::from(now[3][2]),
        ];
        for &i in members.iter().skip(1) {
            if let (Some(m), Some(x)) = (l.items.get(i), rows.get_mut(i)) {
                *x = shifted(x, m.above, d, site);
            }
        }
    }
    for (i, it) in l.items.iter().enumerate() {
        if is_pinned(i)
            && let Some(x) = rows.get_mut(i)
        {
            *x = it.rows;
        }
    }
    let mut fitted = (0, 0);
    let count_bad = |rows: &[Rows], members: &[usize]| {
        members
            .iter()
            .filter(|&&i| match (l.items.get(i), rows.get(i)) {
                (Some(it), Some(r)) => bad(site, routes, it, r),
                _ => false,
            })
            .count()
    };
    // a cluster with points in bad spots moves whole (rings of 4 to 40 m, 12 bearings)
    for members in &l.clusters {
        let now = count_bad(&rows, members);
        if now == 0 {
            continue;
        }
        let mut best: (usize, [f64; 2]) = (now, [0.0, 0.0]);
        'rings: for ring in 1..=10 {
            let r = f64::from(ring) * 4.0;
            for k in 0..12 {
                let a = f64::from(k) * std::f64::consts::TAU / 12.0;
                let d = [r * a.cos(), r * a.sin()];
                let trial: Vec<Rows> = rows
                    .iter()
                    .enumerate()
                    .map(|(i, x)| match l.items.get(i) {
                        Some(it) if members.contains(&i) && !is_pinned(i) => shifted(x, it.above, d, site),
                        _ => *x,
                    })
                    .collect();
                let b = count_bad(&trial, members);
                if b < best.0 {
                    best = (b, d);
                    if b == 0 {
                        break 'rings;
                    }
                }
            }
        }
        if best.1 != [0.0, 0.0] {
            for &i in members {
                if is_pinned(i) {
                    continue;
                }
                if let (Some(it), Some(x)) = (l.items.get(i), rows.get_mut(i)) {
                    *x = shifted(x, it.above, best.1, site);
                }
            }
            fitted.0 += 1;
        }
    }
    // a spawn point still in a bad spot moves alone (up to 16 m)
    let mut left = 0;
    for (i, it) in l.items.iter().enumerate() {
        let Some(r) = rows.get(i).copied() else { continue };
        if !bad(site, routes, it, &r) {
            continue;
        }
        let mut done = false;
        if it.role == Role::Spawn && !is_pinned(i) {
            'ring: for ring in 1..=8 {
                let rad = f64::from(ring) * 2.0;
                for k in 0..16 {
                    let a = f64::from(k) * std::f64::consts::TAU / 16.0;
                    let t = shifted(&r, it.above, [rad * a.cos(), rad * a.sin()], site);
                    if !bad(site, routes, it, &t) {
                        if let Some(x) = rows.get_mut(i) {
                            *x = t;
                        }
                        fitted.1 += 1;
                        done = true;
                        break 'ring;
                    }
                }
            }
        }
        if !done {
            left += 1;
        }
    }
    Fitted {
        rows,
        clusters: fitted.0,
        points: fitted.1,
        bad: left,
    }
}

// ------------------------------------------------------------------------------------------------ the proposal

/// Why no layout could be proposed.
fn refused(why: &str) -> LevelWriteError {
    LevelWriteError::Refused(why.to_owned())
}

/// The auto layout of the project's mode layer on `w` (the project's partition as its log leaves it, the layer in
/// it) over `site` (the base level's ground): the proposal, not applied.
///
/// # Errors
/// The site has no terrain, or the partition has no mode layer to lay out.
pub fn propose(w: &LevelWriter, site: &Site, ask: Ask) -> Result<Proposal> {
    propose_with(w, site, None, ask)
}

/// A fitted candidate: (warnings and bad spots, statistics outside, notes, score), the proposal, the layout applied.
type Ranked = ((usize, usize, usize, f64), Proposal, LevelWriter);

/// [`propose`] with the level's path data: every flag, base, M-COM and spawn point also on the main route network
/// (the path rule XVAL-PLACE-5 held as the site rules are).
///
/// # Errors
/// As [`propose`].
pub fn propose_with(w: &LevelWriter, site: &Site, routes: Option<&Routes>, ask: Ask) -> Result<Proposal> {
    propose_all(w, site, routes, ask, &BTreeSet::new(), 1)?
        .into_iter()
        .next()
        .ok_or_else(|| refused("no place on the level holds the layer"))
}

/// The auto layout's best `k` proposals (round A2: the editor shows several side by side), best first, each a
/// distinct place or turn of the layer, each with its vehicles ([`crate::vehicles::plan`]) and measured with them.
/// The objects in `pins` (upper-case guids) stay where they stand: a pinned flag, base or M-COM keeps its place (the
/// layer is turned and scaled about the pins, its centre where they stand best) with its own spawns and markers; a
/// pinned spawn point or vehicle spawner is not moved; the rest is laid out around them.
///
/// # Errors
/// As [`propose`].
#[allow(
    clippy::too_many_lines,
    reason = "the candidates fitted, checked, ranked, then given their vehicles"
)]
pub fn propose_all(
    w: &LevelWriter,
    site: &Site,
    routes: Option<&Routes>,
    ask: Ask,
    pins: &BTreeSet<String>,
    k: usize,
) -> Result<Vec<Proposal>> {
    if !site.has_terrain() {
        return Err(refused(
            "the level has no terrain height field: nothing to lay the gameplay out on",
        ));
    }
    let layer = read_layer(w, site);
    if layer.clusters.is_empty() {
        return Err(refused(
            "the project has no mode layer (no flag, base or M-COM): choose the export's target mode first",
        ));
    }
    let pinned: Vec<bool> = layer.items.iter().map(|it| pins.contains(&it.guid)).collect();
    let pin_points: Vec<[f64; 2]> = layer
        .items
        .iter()
        .zip(&pinned)
        .filter(|(it, p)| **p && it.role != Role::Outline)
        .map(|(it, _)| [f64::from(it.rows[3][0]), f64::from(it.rows[3][2])])
        .collect();
    let walk = Walk::new(site, routes);
    let (cands, n) = search(&layer, site, &walk, ask.seed, ask.layer_scale, 8, &pin_points);
    let check = |work: &LevelWriter| {
        let measures = placement::measure(work, ask.mode);
        let mut findings = placement::check_measures(&measures, ask.mode, placement::group(ask.profile));
        findings.extend(crate::validate::site_rules(work, Some(w), site));
        if let Some(r) = routes {
            findings.extend(crate::validate::path_rules(work, Some(w), r));
        }
        let st = stats(&measures, ask.mode, ask.profile);
        (findings, st)
    };
    let mut ranked: Vec<Ranked> = Vec::new();
    for (sc, f) in cands {
        let fitted = fit(&layer, &f, site, routes, &pinned);
        let mut work = w.clone();
        for (it, r) in layer.items.iter().zip(&fitted.rows) {
            if let Some(t) = work.d.child(it.node, b"Transform") {
                work.d.set_transform(t, r)?;
            }
        }
        let (findings, st) = check(&work);
        let warn = findings.iter().filter(|x| x.severity >= Severity::Warning).count();
        let notes = findings.len() - warn;
        let outside: usize = st.iter().map(|s| s.values.len() - s.inside).sum();
        let total = sc + stat_cost(&st, ask.mode, ask.profile);
        let key = (warn + fitted.bad, outside, notes, total);
        let moves = layer
            .items
            .iter()
            .zip(&fitted.rows)
            .filter(|(it, r)| it.rows != **r)
            .map(|(it, r)| Moved {
                guid: it.guid.clone(),
                from: it.rows,
                to: *r,
                placement: None,
            })
            .collect();
        ranked.push((
            key,
            Proposal {
                moves,
                seed: ask.seed,
                turn: f.turn.to_degrees().rem_euclid(360.0),
                scale: f.scale * ask.layer_scale,
                centre: f.centre,
                clusters_fitted: fitted.clusters,
                points_fitted: fitted.points,
                bad_spots: fitted.bad,
                findings,
                stats: st,
                candidates: n,
                score: total,
                vehicles: crate::vehicles::Plan::default(),
                pinned: pinned.iter().filter(|p| **p).count(),
            },
            work,
        ));
    }
    // the fewest warnings and bad spots, then statistics outside DICE's ranges, then notes, then the score; ties keep
    // the earlier candidate
    ranked.sort_by(|a, b| {
        (a.0.0, a.0.1, a.0.2)
            .cmp(&(b.0.0, b.0.1, b.0.2))
            .then(a.0.3.total_cmp(&b.0.3))
    });
    let others = pins
        .iter()
        .filter(|g| !layer.items.iter().any(|it| &it.guid == *g) && w.find(g).is_some())
        .count();
    let mut out = Vec::new();
    for (_, mut prop, work) in ranked.into_iter().take(k.max(1)) {
        prop.pinned += others;
        // the vehicles (round A2) on the laid-out layer, then the proposal measured again with them
        let tag = format!("auto-layout/{}/{}", ask.seed, w.d.instances().len());
        prop.vehicles = crate::vehicles::plan(&work, site, ask.mode, ask.profile, pins, &tag);
        if !prop.vehicles.vehicles.is_empty() {
            let mut with = work.clone();
            let op = crate::Operation {
                n: 0,
                by: String::new(),
                when: String::new(),
                label: String::new(),
                op: Proposal {
                    moves: Vec::new(),
                    ..prop.clone()
                }
                .op(),
            };
            let mut applied = crate::write::Applied::default();
            match crate::write::apply_one(&mut with, &op, &crate::Schema::default(), &mut applied) {
                Ok(()) => {
                    let (findings, st) = check(&with);
                    prop.findings = findings;
                    prop.stats = st;
                }
                Err(e) => {
                    let v = &mut prop.vehicles;
                    v.notes.push(format!("the vehicles could not be applied: {e}"));
                    v.moves.clear();
                    v.places.clear();
                    v.links.clear();
                    v.vehicles.clear();
                }
            }
        }
        out.push(prop);
    }
    Ok(out)
}

/// The objects of a proposal by guid, for the editor's preview (where each would stand).
#[must_use]
pub fn preview(p: &Proposal) -> BTreeMap<String, [f32; 3]> {
    p.moves.iter().map(|m| (m.guid.clone(), m.to[3])).collect()
}

/// The guids a proposal moves (a set, for a view's highlight).
#[must_use]
pub fn moved(p: &Proposal) -> BTreeSet<String> {
    p.moves.iter().map(|m| m.guid.clone()).collect()
}

// ------------------------------------------------------------------------------------------------ against DICE's

/// A proposed layout against DICE's own layout of the same level and mode (round A1's proof).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Comparison {
    /// Each of DICE's anchors matched to the proposal's (what, the distance in plan, metres): Conquest flags by the
    /// matching with the least total distance, bases by team; Rush stages by order (their M-COMs' centres); Squad
    /// Deathmatch the spawn points' centre.
    pub anchors: Vec<(String, f64)>,
    /// The mean of those distances.
    pub mean_distance: f64,
    /// DICE's combat area (union, m2), the proposal's, and their intersection over their union.
    pub dice_area: f64,
    /// The proposal's combat area (m2).
    pub our_area: f64,
    /// Intersection over union of the two (0 to 1).
    pub iou: f64,
    /// The share of the proposal's gameplay points (flags, bases, M-COMs, spawn places) inside DICE's combat area.
    pub inside_dice: f64,
}

fn centre(pts: &[[f64; 2]]) -> Option<[f64; 2]> {
    (!pts.is_empty()).then(|| {
        let n = pts.len() as f64;
        [
            pts.iter().map(|p| p[0]).sum::<f64>() / n,
            pts.iter().map(|p| p[1]).sum::<f64>() / n,
        ]
    })
}

/// One step of [`best_match`]'s search: the `k`-th point of `small` given each free point of `large`.
fn go(
    k: usize,
    small: &[[f64; 2]],
    large: &[[f64; 2]],
    used: &mut Vec<bool>,
    cur: &mut Vec<f64>,
    best: &mut Option<(f64, Vec<f64>)>,
) {
    let Some(p) = small.get(k) else {
        let t: f64 = cur.iter().sum();
        if best.as_ref().is_none_or(|b| t < b.0) {
            *best = Some((t, cur.clone()));
        }
        return;
    };
    for j in 0..large.len() {
        if used.get(j).copied().unwrap_or(true) {
            continue;
        }
        let Some(q) = large.get(j) else { continue };
        if let Some(u) = used.get_mut(j) {
            *u = true;
        }
        cur.push((p[0] - q[0]).hypot(p[1] - q[1]));
        go(k + 1, small, large, used, cur, best);
        cur.pop();
        if let Some(u) = used.get_mut(j) {
            *u = false;
        }
    }
}

/// The matching of `a` to `b` (each of the smaller set to a distinct one of the other) with the least total distance
/// (every assignment tried: at most 6 flags).
fn best_match(a: &[[f64; 2]], b: &[[f64; 2]]) -> Vec<f64> {
    let d = |p: [f64; 2], q: [f64; 2]| (p[0] - q[0]).hypot(p[1] - q[1]);
    let (small, large) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if small.is_empty() || large.len() > 8 {
        return small
            .iter()
            .map(|p| large.iter().map(|q| d(*p, *q)).fold(f64::INFINITY, f64::min))
            .collect();
    }
    let mut best: Option<(f64, Vec<f64>)> = None;
    let mut used = vec![false; large.len()];
    let mut cur: Vec<f64> = Vec::new();
    go(0, small, large, &mut used, &mut cur, &mut best);
    best.map_or_default(|b| b.1)
}

fn union_mask(polys: &[Vec<[f64; 2]>], b: [f64; 4], cell: f64) -> Vec<bool> {
    let nx = ((b[2] - b[0]) / cell).ceil() as usize;
    let nz = ((b[3] - b[1]) / cell).ceil() as usize;
    let mut m = vec![false; nx * nz];
    for j in 0..nz {
        for i in 0..nx {
            let p = [b[0] + (i as f64 + 0.5) * cell, b[1] + (j as f64 + 0.5) * cell];
            if polys.iter().any(|q| inside(p, q))
                && let Some(c) = m.get_mut(j * nx + i)
            {
                *c = true;
            }
        }
    }
    m
}

/// The gameplay points of a layout in plan: (flags, bases by team, stage centres, spawn places).
type Points = (Vec<[f64; 2]>, Vec<(String, [f64; 2])>, Vec<[f64; 2]>, Vec<[f64; 2]>);

fn points(w: &LevelWriter) -> Points {
    let xz = |p: [f32; 3]| [f64::from(p[0]), f64::from(p[2])];
    let cps = crate::mpconvert::capture_points(w);
    let flags = cps.iter().filter(|c| !c.base).map(|c| xz(c.at)).collect();
    let bases = cps
        .iter()
        .filter(|c| c.base)
        .map(|c| (c.team.clone(), xz(c.at)))
        .collect();
    let stages = crate::modelayer::stages(w)
        .iter()
        .filter_map(|s| {
            let pts: Vec<[f64; 2]> = s.iter().filter_map(|m| w.transform(m)).map(|r| xz(r[3])).collect();
            centre(&pts)
        })
        .collect();
    let spawns =
        w.d.of_type(b".AlternateSpawnEntityData")
            .into_iter()
            .filter_map(|e| w.transform_at(e))
            .map(|r| xz(r[3]))
            .collect();
    (flags, bases, stages, spawns)
}

/// A proposed layout (`ours`: the project's partition with the proposal applied) against DICE's own layout of the
/// level (`dice`: the stock partition) played as `mode`.
#[must_use]
pub fn compare(dice: &LevelWriter, ours: &LevelWriter, mode: Mode) -> Comparison {
    let (df, db, ds, dsp) = points(dice);
    let (of, ob, os, osp) = points(ours);
    let mut anchors: Vec<(String, f64)> = Vec::new();
    match mode {
        Mode::Conquest => {
            for (k, d) in best_match(&df, &of).into_iter().enumerate() {
                anchors.push((format!("flag {}", k + 1), d));
            }
            for (team, p) in &db {
                if let Some((_, q)) = ob.iter().find(|x| x.0 == *team) {
                    anchors.push((format!("{team} base"), (p[0] - q[0]).hypot(p[1] - q[1])));
                }
            }
        }
        Mode::Rush | Mode::SquadRush => {
            for (k, (p, q)) in ds.iter().zip(&os).enumerate() {
                anchors.push((format!("stage {}", k + 1), (p[0] - q[0]).hypot(p[1] - q[1])));
            }
        }
        Mode::SquadDeathmatch => {
            if let (Some(p), Some(q)) = (centre(&dsp), centre(&osp)) {
                anchors.push(("spawns' centre".into(), (p[0] - q[0]).hypot(p[1] - q[1])));
            }
        }
    }
    let mean_distance = if anchors.is_empty() {
        0.0
    } else {
        anchors.iter().map(|a| a.1).sum::<f64>() / anchors.len() as f64
    };
    let (dp, op) = (
        crate::validate::combat_outlines(dice),
        crate::validate::combat_outlines(ours),
    );
    let all: Vec<[f64; 2]> = dp.iter().chain(&op).flatten().copied().collect();
    let b = all.iter().fold(
        [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY],
        |b, p| [b[0].min(p[0]), b[1].min(p[1]), b[2].max(p[0]), b[3].max(p[1])],
    );
    let (mut dice_area, mut our_area, mut iou) = (0.0, 0.0, 0.0);
    if b[0].is_finite() {
        let cell = (((b[2] - b[0]) * (b[3] - b[1]) / 1.0e6).sqrt()).max(2.0);
        let (md, mo) = (union_mask(&dp, b, cell), union_mask(&op, b, cell));
        let a = cell * cell;
        let n = |m: &[bool]| m.iter().filter(|x| **x).count() as f64;
        let inter = md.iter().zip(&mo).filter(|(x, y)| **x && **y).count() as f64;
        let uni = md.iter().zip(&mo).filter(|(x, y)| **x || **y).count() as f64;
        dice_area = n(&md) * a;
        our_area = n(&mo) * a;
        iou = if uni > 0.0 { inter / uni } else { 0.0 };
    }
    let mine: Vec<[f64; 2]> = of
        .iter()
        .copied()
        .chain(ob.iter().map(|x| x.1))
        .chain(osp.iter().copied())
        .chain(
            crate::modelayer::stages(ours)
                .iter()
                .flatten()
                .filter_map(|m| ours.transform(m))
                .map(|r| [f64::from(r[3][0]), f64::from(r[3][2])]),
        )
        .collect();
    let inside_dice = if mine.is_empty() || dp.is_empty() {
        0.0
    } else {
        mine.iter().filter(|p| dp.iter().any(|q| inside(**p, q))).count() as f64 / mine.len() as f64
    };
    Comparison {
        anchors,
        mean_distance,
        dice_area,
        our_area,
        iou,
        inside_dice,
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn a_fit_turns_and_scales_about_the_pivot() {
        let f = Fit {
            turn: std::f64::consts::FRAC_PI_2,
            scale: 2.0,
            centre: [100.0, 50.0],
        };
        let q = f.apply([10.0, 10.0], [11.0, 10.0]);
        assert!((q[0] - 100.0).abs() < 1e-9 && (q[1] - 48.0).abs() < 1e-9, "{q:?}");
        let r = f.turn_row([1.0, 0.0, 0.0]);
        assert!(r[0].abs() < 1e-6 && (r[2] + 1.0).abs() < 1e-6, "{r:?}");
        let mut a = Rng(7);
        let mut b = Rng(7);
        assert_eq!(a.next(), b.next());
        assert!((-1.0..1.0).contains(&a.unit()));
    }

    #[test]
    fn dice_ranges_are_read() {
        let p = params();
        assert_eq!(p["CQ DICE"]["flag_nn_m"][3], 58.0);
        assert_eq!(p["Rush"]["stage_step_m"][4], 315.6);
        let m = Measure {
            metric: "cq.flag_nn_dist",
            value: 120.04,
            dy: None,
            subject: String::new(),
            stage: None,
            tag: "",
            objects: Vec::new(),
        };
        let s = stats(&[m], Mode::Conquest, Profile::Bfbc2);
        assert_eq!(s.len(), 1);
        assert_eq!((s[0].inside, s[0].values[0]), (1, 120.0));
    }
}
