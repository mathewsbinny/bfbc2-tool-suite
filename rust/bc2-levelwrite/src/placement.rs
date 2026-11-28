//! DICE's placement tendencies as editor warnings (round V1, from round L1's study of the stock maps:
//! `bfo:docs/research/suite/L1_placement_report.md`).
//!
//! [`measure`] reads a level partition's gameplay layout as L1's `l1_measure.py` does - the same metrics, the same
//! names (`cq.flag_nn_dist`, `rush.stage_step` ...), the same definitions - and [`check`] holds each measurement
//! against L1's rules (`l1_rules.json`): outside a rule's warning band a [`Severity::Warning`], inside it but outside
//! the stock band a [`Severity::Note`], each with the stock range and L1's ID. The rules come from DICE's own levels,
//! where none raises a warning (L1's `l1_rules_check.md`; V1 runs the Rust port over every stock level again).
//!
//! What is read (L1's report, "Method"; every point on the XZ ground plane unless a height is named):
//! - capture points: `UnableToChangeTeam` true = a team's base (HQ), else a flag (Conquest) or a Rush spawn base;
//!   their `Spawners` list soldier and vehicle spawners, their `Geometry` the capture zone;
//! - a soldier spawner's places are its `AlternativeSpawnPoints`, its own transform only when it has none;
//! - combat areas: the union of their `Geometry` outlines (`AreaGeometryEntityData` points linked by `Next`), per team
//!   when `UseTeamRestriction`; areas on L1's raster (cells of 2 m or more), margins to the union's boundary sampled
//!   every 2 m;
//! - Rush stages and what is enabled while each is played: [`crate::modelayer::rush_play`] (L1's simulation).
//!
//! Left out: L1-HT-001 (a flag's height above the ground around it) needs the terrain's height field; A1 reads it.
#![allow(
    clippy::cast_precision_loss,
    reason = "counts of points and cells as f64 for means and areas: far below 2^52"
)]

use std::collections::{BTreeMap, HashMap, HashSet};

use bc2_formats::dbx::edit::NodeId;

use crate::mpconvert::{guid_of, local_ref, short, type_of};
use crate::project::Profile;
use crate::rules::{Finding, L1Rule, Severity};
use crate::write::LevelWriter;

/// A multiplayer mode as L1 measures it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    /// Conquest.
    Conquest,
    /// Rush.
    Rush,
    /// Squad Rush.
    SquadRush,
    /// Squad Deathmatch.
    SquadDeathmatch,
}

impl Mode {
    /// L1's word (`CQ`, `Rush`, `SR`, `SDM`).
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Conquest => "CQ",
            Self::Rush => "Rush",
            Self::SquadRush => "SR",
            Self::SquadDeathmatch => "SDM",
        }
    }

    /// The mode of a game mode key (`conquest`, `goldrush`, `squadrush`, `teamdeathmatch`); `None` for the modes L1
    /// gives no rules (Onslaught, Air Superiority).
    #[must_use]
    pub fn of_key(key: &str) -> Option<Self> {
        match crate::modelayer::mode_key(key).as_str() {
            "conquest" => Some(Self::Conquest),
            "goldrush" => Some(Self::Rush),
            "squadrush" => Some(Self::SquadRush),
            "teamdeathmatch" => Some(Self::SquadDeathmatch),
            _ => None,
        }
    }
}

/// L1's era group of a profile.
#[must_use]
pub fn group(p: Profile) -> &'static str {
    match p {
        Profile::Bfbc2 => "BFBC2",
        Profile::Bfbc1 => "BFBC1",
        Profile::Vietnam => "Vietnam",
        Profile::Ww2 => "WW2",
    }
}

/// One measurement (L1's record): a metric's value on a subject.
#[derive(Debug, Clone, PartialEq)]
pub struct Measure {
    /// L1's metric name.
    pub metric: &'static str,
    /// The value (metres, square metres, seconds, a count or a share).
    pub value: f64,
    /// A height difference, where the metric has one (`dy`).
    pub dy: Option<f64>,
    /// What it is about (a flag's or base's name).
    pub subject: String,
    /// The Rush stage (1 is the first played).
    pub stage: Option<usize>,
    /// A flag of the record (L1's `where` clauses): `aabb` (a capture zone of boxes), `attacker` (a life counter).
    pub tag: &'static str,
    /// The objects it is about (upper-case guids).
    pub objects: Vec<String>,
}

// ------------------------------------------------------------------------------------------------ reading

struct Lv<'a> {
    w: &'a LevelWriter,
    by_guid: HashMap<String, (NodeId, String)>,
    order: Vec<String>,
}

impl<'a> Lv<'a> {
    fn new(w: &'a LevelWriter) -> Self {
        let mut by_guid = HashMap::new();
        let mut order = Vec::new();
        for e in w.d.instances() {
            let g = guid_of(&w.d, e);
            order.push(g.clone());
            by_guid.insert(g, (e, short(&type_of(&w.d, e)).to_owned()));
        }
        Self { w, by_guid, order }
    }

    fn ty(&self, g: &str) -> &str {
        self.by_guid.get(g).map_or("", |x| x.1.as_str())
    }

    fn of(&self, t: &str) -> Vec<String> {
        self.order.iter().filter(|g| self.ty(g) == t).cloned().collect()
    }

    fn node(&self, g: &str) -> Option<NodeId> {
        self.by_guid.get(g).map(|x| x.0)
    }

    fn text(&self, g: &str, f: &str) -> Option<String> {
        let e = self.node(g)?;
        let c = self.w.d.child(e, f.as_bytes())?;
        self.w.d.text(c).map(|t| String::from_utf8_lossy(t).into_owned())
    }

    fn boolean(&self, g: &str, f: &str) -> bool {
        self.node(g)
            .and_then(|e| crate::modelayer::bool_field(self.w, e, f.as_bytes()))
            .unwrap_or(false)
    }

    fn raw4(&self, g: &str, f: &str) -> Option<[u8; 4]> {
        let e = self.node(g)?;
        let c = self.w.d.child(e, f.as_bytes())?;
        let (size, vals) = bc2_formats::dbx::edit::numbers_of(self.w.d.raw(c))?;
        if size != 4 || vals.len() != 1 {
            return None;
        }
        vals.first().and_then(|v| <[u8; 4]>::try_from(*v).ok())
    }

    /// A 4-byte float field from its raw bits.
    fn num(&self, g: &str, f: &str) -> Option<f64> {
        self.raw4(g, f).map(|b| f64::from(f32::from_be_bytes(b)))
    }

    /// A field DICE stores as an integer (L1's `INT_FIELDS`), else as a float.
    fn ival(&self, g: &str, f: &str) -> Option<f64> {
        let b = self.raw4(g, f)?;
        let i = i32::from_be_bytes(b);
        if i.unsigned_abs() < (1 << 24) {
            Some(f64::from(i))
        } else {
            Some(f64::from(f32::from_be_bytes(b)))
        }
    }

    fn pos(&self, g: &str) -> Option<[f64; 3]> {
        self.w.transform_at(self.node(g)?).map(|r| r[3].map(f64::from))
    }

    /// Local references of an array field (upper-case guids).
    fn refs(&self, g: &str, f: &str) -> Vec<String> {
        let Some(e) = self.node(g) else { return Vec::new() };
        self.w
            .d
            .child(e, f.as_bytes())
            .and_then(|a| self.w.d.children(a))
            .unwrap_or_default()
            .iter()
            .filter_map(|&i| self.w.d.attr(i, b"ref").and_then(local_ref))
            .collect()
    }

    fn local(&self, g: &str, f: &str) -> Option<String> {
        let e = self.node(g)?;
        self.w
            .d
            .child(e, f.as_bytes())
            .and_then(|x| self.w.d.attr(x, b"ref"))
            .and_then(local_ref)
    }

    fn label(&self, g: &str) -> String {
        self.text(g, "LocationName")
            .filter(|s| !s.is_empty())
            .or_else(|| self.text(g, "Name"))
            .unwrap_or_default()
    }

    /// An outline from its first point, following `Next` (spec LEVEL-ENT-2): (guids, points).
    fn chain(&self, start: &str) -> (Vec<String>, Vec<[f64; 3]>) {
        let (mut gs, mut pts, mut seen) = (Vec::new(), Vec::new(), HashSet::new());
        let mut cur = Some(start.to_owned());
        while let Some(g) = cur.take() {
            if !seen.insert(g.clone()) || self.ty(&g) != "AreaGeometryEntityData" {
                break;
            }
            if let Some(p) = self.pos(&g) {
                pts.push(p);
            }
            cur = self.local(&g, "Next");
            gs.push(g);
        }
        (gs, pts)
    }

    /// The outlines of an area field: one per distinct chain.
    fn polygons(&self, g: &str, f: &str) -> Vec<Vec<[f64; 2]>> {
        let mut out = Vec::new();
        let mut seen: HashSet<Vec<String>> = HashSet::new();
        for s in self.refs(g, f) {
            if self.ty(&s) != "AreaGeometryEntityData" {
                continue;
            }
            let (mut gs, pts) = self.chain(&s);
            gs.sort();
            if pts.len() < 3 || !seen.insert(gs) {
                continue;
            }
            out.push(pts.iter().map(|p| [p[0], p[2]]).collect());
        }
        out
    }

    fn spawn_points(&self, sg: &str) -> Vec<(String, [f64; 3])> {
        let mut out: Vec<(String, [f64; 3])> = self
            .refs(sg, "AlternativeSpawnPoints")
            .into_iter()
            .filter_map(|a| self.pos(&a).map(|p| (a, p)))
            .collect();
        if out.is_empty()
            && let Some(p) = self.pos(sg)
        {
            out.push((sg.to_owned(), p));
        }
        out
    }

    /// A capture point's soldier and vehicle spawners.
    fn cp_spawners(&self, cp: &str) -> (Vec<String>, Vec<String>) {
        let (mut sol, mut veh) = (Vec::new(), Vec::new());
        for s in self.refs(cp, "Spawners") {
            match self.ty(&s) {
                "SoldierSpawnEntityData" => sol.push(s),
                "VehicleSpawnEntityData" => veh.push(s),
                _ => {}
            }
        }
        (sol, veh)
    }

    /// (team, class) of each vehicle a vehicle spawner can make: its `Vehicles` items, else `Vehicle`.
    fn vehicles_of(&self, vg: &str) -> Vec<(String, &'static str)> {
        let Some(e) = self.node(vg) else { return Vec::new() };
        let d = &self.w.d;
        let own_team = self.text(vg, "Team").unwrap_or_default();
        let mut out = Vec::new();
        for &it in d.child(e, b"Vehicles").and_then(|a| d.children(a)).unwrap_or_default() {
            let (mut r, mut team) = (None, None);
            for &c in d.children(it).unwrap_or_default() {
                match d.attr(c, b"name") {
                    Some(b"Vehicle") => r = d.attr(c, b"ref").map(|x| String::from_utf8_lossy(x).into_owned()),
                    Some(b"Team") => team = d.text(c).map(|x| String::from_utf8_lossy(x).into_owned()),
                    _ => {}
                }
            }
            if let Some(r) = r.filter(|r| r != "null") {
                out.push((team.unwrap_or_else(|| own_team.clone()), vehicle_class(&r)));
            }
        }
        if out.is_empty()
            && let Some(c) = d.child(e, b"Vehicle")
        {
            let mut refs: Vec<String> = Vec::new();
            match d.children(c) {
                Some(items) => {
                    for &it in items {
                        refs.extend(d.attr(it, b"ref").map(|x| String::from_utf8_lossy(x).into_owned()));
                        for &cc in d.children(it).unwrap_or_default() {
                            refs.extend(d.attr(cc, b"ref").map(|x| String::from_utf8_lossy(x).into_owned()));
                        }
                    }
                }
                None => refs.extend(d.attr(c, b"ref").map(|x| String::from_utf8_lossy(x).into_owned())),
            }
            for r in refs.into_iter().filter(|r| r != "null") {
                out.push((own_team.clone(), vehicle_class(&r)));
            }
        }
        out
    }

    /// The class of the vehicle a spawner makes for `team` (L1's `spawner_class`): its entry for that team, else its
    /// first entry.
    fn spawner_class(&self, vg: &str, team: &str) -> Option<&'static str> {
        let vs = self.vehicles_of(vg);
        vs.iter().find(|(t, _)| t == team).or_else(|| vs.first()).map(|x| x.1)
    }
}

/// Every vehicle spawner of a partition with L1's classes of the vehicles it makes (round A1's site rules): guid,
/// node, classes (`tank`, `boat`, `heli` ...; `other` for a vehicle L1 does not class).
#[must_use]
pub fn vehicle_spawners(w: &LevelWriter) -> Vec<(String, NodeId, Vec<&'static str>)> {
    let lv = Lv::new(w);
    lv.of("VehicleSpawnEntityData")
        .into_iter()
        .filter_map(|g| {
            let n = lv.node(&g)?;
            let classes = lv.vehicles_of(&g).into_iter().map(|x| x.1).collect();
            Some((g, n, classes))
        })
        .collect()
}

/// L1's vehicle class of a blueprint reference (`objects/vehicles/<domain>/<kind>/...`).
fn vehicle_class(r: &str) -> &'static str {
    const VEH: &[(&str, &str)] = &[
        ("m1a2", "tank"),
        ("t90", "tank"),
        ("m4-sherman", "tank"),
        ("type97-chiha", "tank"),
        ("m48pattontank", "tank"),
        ("t-54", "tank"),
        ("m3a3", "ifv"),
        ("bmd3", "ifv"),
        ("aa-bmd3", "ifv_aa"),
        ("cav", "ifv"),
        ("humvee", "car"),
        ("cobra", "car"),
        ("vodnik", "car"),
        ("willys-mb", "car"),
        ("kurogane", "car"),
        ("m151a1", "car"),
        ("gaz69", "car"),
        ("minitruck", "car"),
        ("truck", "car"),
        ("quadbike", "quad"),
        ("ah60", "heli"),
        ("ah64", "heli"),
        ("mi24", "heli"),
        ("mi28", "heli"),
        ("huey", "heli"),
        ("jap_zero_fighter", "plane"),
        ("us_corsair", "plane"),
        ("uav", "uav"),
        ("pbl", "boat"),
        ("pwc", "boat"),
        ("markv", "boat"),
        ("pbr_mkii", "boat"),
        ("lcvp-landingcraft", "boat"),
        ("a10", "ambient"),
        ("f18", "ambient"),
        ("su39", "ambient"),
        ("cargoplane_backdrop", "ambient"),
        ("phantom", "ambient"),
        ("cruiseplatform", "ambient"),
    ];
    let low = r.to_ascii_lowercase();
    let p: Vec<&str> = low.split('/').collect();
    let kind = p.get(3).copied().unwrap_or(low.as_str());
    let class = |k: &str| VEH.iter().find(|x| x.0 == k).map(|x| x.1);
    if p.contains(&"stationary") || kind.starts_with("stationary_") {
        if class(kind) == Some("ambient") {
            return "ambient";
        }
        if low.contains("uav_station") {
            return "uav";
        }
        return "emplacement";
    }
    if kind == "uav" && low.contains("station") {
        return "uav";
    }
    class(kind).unwrap_or("other")
}

// ------------------------------------------------------------------------------------------------ plane geometry

fn d2(a: [f64; 3], b: [f64; 3]) -> f64 {
    (a[0] - b[0]).hypot(a[2] - b[2])
}

fn inside(p: [f64; 2], poly: &[[f64; 2]]) -> bool {
    let mut r = false;
    let n = poly.len();
    for k in 0..n {
        let (Some(a), Some(b)) = (poly.get(k), poly.get((k + 1) % n)) else {
            continue;
        };
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let xc = a[0] + (p[1] - a[1]) * (b[0] - a[0]) / (b[1] - a[1]);
            if p[0] < xc {
                r = !r;
            }
        }
    }
    r
}

fn inside_union(p: [f64; 2], polys: &[&Vec<[f64; 2]>]) -> bool {
    polys.iter().any(|q| inside(p, q))
}

fn shoelace(poly: &[[f64; 2]]) -> f64 {
    let n = poly.len();
    let mut s = 0.0;
    for k in 0..n {
        if let (Some(a), Some(b)) = (poly.get(k), poly.get((k + 1) % n)) {
            s += a[0] * b[1] - b[0] * a[1];
        }
    }
    s.abs() / 2.0
}

fn median(v: &mut [f64]) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let hi = v.get(n / 2).copied()?;
    Some(if n.is_multiple_of(2) {
        f64::midpoint(v.get(n / 2 - 1).copied()?, hi)
    } else {
        hi
    })
}

/// L1's raster over the combat areas' box: cells of 2 m or more (at most 3 million), a cell in an area when its
/// centre is (even-odd per outline, the union over outlines).
struct Grid {
    x0: f64,
    z0: f64,
    cell: f64,
    nx: usize,
    nz: usize,
}

impl Grid {
    fn new(polys: &[&Vec<[f64; 2]>]) -> Option<Self> {
        let mut b: Option<[f64; 4]> = None;
        for p in polys.iter().flat_map(|x| x.iter()) {
            let v = b.get_or_insert([p[0], p[1], p[0], p[1]]);
            v[0] = v[0].min(p[0]);
            v[1] = v[1].min(p[1]);
            v[2] = v[2].max(p[0]);
            v[3] = v[3].max(p[1]);
        }
        let [x0, z0, x1, z1] = b?;
        let (w, h) = ((x1 - x0).max(1.0), (z1 - z0).max(1.0));
        let cell = (w * h / 3e6).sqrt().max(2.0);
        Some(Self {
            x0,
            z0,
            cell,
            nx: (w / cell).ceil() as usize + 1,
            nz: (h / cell).ceil() as usize + 1,
        })
    }

    /// The cells inside the union of `polys` (row by row: each outline's crossings give its runs).
    fn mask(&self, polys: &[&Vec<[f64; 2]>]) -> Vec<bool> {
        let mut m = vec![false; self.nx * self.nz];
        for p in polys {
            for j in 0..self.nz {
                let z = self.z0 + (j as f64 + 0.5) * self.cell;
                let mut xs: Vec<f64> = Vec::new();
                let n = p.len();
                for k in 0..n {
                    let (Some(a), Some(b)) = (p.get(k), p.get((k + 1) % n)) else {
                        continue;
                    };
                    if (a[1] > z) != (b[1] > z) {
                        xs.push(a[0] + (z - a[1]) * (b[0] - a[0]) / (b[1] - a[1]));
                    }
                }
                xs.sort_by(f64::total_cmp);
                for &[lo, hi] in xs.as_chunks::<2>().0 {
                    // cell centres x with lo < x < hi (L1's test: x < crossing, an odd count to the right)
                    let first = ((lo - self.x0) / self.cell - 0.5).floor() as i64 + 1;
                    let last = ((hi - self.x0) / self.cell - 0.5).ceil() as i64 - 1;
                    for i in first.max(0)..=last.min(self.nx as i64 - 1) {
                        if let Some(c) = m.get_mut(j * self.nx + i as usize) {
                            *c = true;
                        }
                    }
                }
            }
        }
        m
    }

    fn area(&self, m: &[bool]) -> f64 {
        m.iter().filter(|&&x| x).count() as f64 * self.cell * self.cell
    }
}

/// The level's combat areas: outlines per area, the raster, boundary samples per set of areas.
struct Areas {
    cas: Vec<String>,
    polys: HashMap<String, Vec<Vec<[f64; 2]>>>,
    team: HashMap<String, String>,
    restricted: HashMap<String, bool>,
    grid: Option<Grid>,
    bnd: std::cell::RefCell<HashMap<Vec<String>, Vec<[f64; 2]>>>,
}

impl Areas {
    fn new(lv: &Lv<'_>) -> Self {
        let cas = lv.of("CombatAreaTriggerEntityData");
        let mut polys = HashMap::new();
        let mut team = HashMap::new();
        let mut restricted = HashMap::new();
        let mut outer = Vec::new();
        for c in &cas {
            polys.insert(c.clone(), lv.polygons(c, "Geometry"));
            outer.extend(lv.polygons(c, "SurroundingGeometry"));
            team.insert(c.clone(), lv.text(c, "Team").unwrap_or_default());
            restricted.insert(c.clone(), lv.boolean(c, "UseTeamRestriction"));
        }
        let all: Vec<&Vec<[f64; 2]>> = polys.values().flatten().chain(outer.iter()).collect();
        let grid = Grid::new(&all);
        Self {
            cas,
            polys,
            team,
            restricted,
            grid,
            bnd: std::cell::RefCell::new(HashMap::new()),
        }
    }

    fn of<'s>(&'s self, guids: &[String]) -> Vec<&'s Vec<[f64; 2]>> {
        guids
            .iter()
            .flat_map(|g| self.polys.get(g).into_iter().flatten())
            .collect()
    }

    fn area(&self, guids: &[String]) -> Option<f64> {
        let p = self.of(guids);
        let g = self.grid.as_ref()?;
        (!p.is_empty()).then(|| g.area(&g.mask(&p)))
    }

    /// Distance to the union's boundary: positive inside, negative outside (L1's `signed_margin`: the boundary
    /// sampled every 2 m, samples strictly inside another outline dropped).
    fn margin(&self, p: [f64; 3], guids: &[String]) -> Option<f64> {
        let polys = self.of(guids);
        if polys.is_empty() {
            return None;
        }
        let mut key = guids.to_vec();
        key.sort();
        let mut cache = self.bnd.borrow_mut();
        let b = cache.entry(key).or_insert_with(|| {
            let mut out = Vec::new();
            for (i, q) in polys.iter().enumerate() {
                let others: Vec<&Vec<[f64; 2]>> = polys
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|x| *x.1)
                    .collect();
                let n = q.len();
                for k in 0..n {
                    let (Some(a), Some(c)) = (q.get(k), q.get((k + 1) % n)) else {
                        continue;
                    };
                    let len = (c[0] - a[0]).hypot(c[1] - a[1]);
                    let m = ((len / 2.0).ceil() as usize).max(1);
                    for t in 0..m {
                        let f = t as f64 / m as f64;
                        let s = [a[0] + (c[0] - a[0]) * f, a[1] + (c[1] - a[1]) * f];
                        if others.is_empty() || !inside_union(s, &others) {
                            out.push(s);
                        }
                    }
                }
            }
            out
        });
        let q = [p[0], p[2]];
        let d = b
            .iter()
            .map(|s| (s[0] - q[0]).hypot(s[1] - q[1]))
            .fold(f64::INFINITY, f64::min);
        if !d.is_finite() {
            return None;
        }
        Some(if inside_union(q, &polys) { d } else { -d })
    }

    /// The areas a team plays in: its own, the neutral ones and those without a team restriction.
    fn team_areas(&self, team: &str) -> Vec<String> {
        self.cas
            .iter()
            .filter(|c| {
                let t = self.team.get(*c).map_or("", String::as_str);
                t == team || t == "TeamNeutral" || !self.restricted.get(*c).copied().unwrap_or(false)
            })
            .cloned()
            .collect()
    }
}

// ------------------------------------------------------------------------------------------------ measuring

struct Out(Vec<Measure>);

impl Out {
    fn rec(&mut self, metric: &'static str, value: f64, objects: Vec<String>) -> &mut Measure {
        self.0.push(Measure {
            metric,
            value,
            dy: None,
            subject: String::new(),
            stage: None,
            tag: "",
            objects,
        });
        let n = self.0.len();
        // just pushed
        #[allow(clippy::indexing_slicing, reason = "the element just pushed")]
        &mut self.0[n - 1]
    }
}

impl Measure {
    fn about(&mut self, s: impl Into<String>) -> &mut Self {
        self.subject = s.into();
        self
    }

    fn at_stage(&mut self, k: usize) -> &mut Self {
        self.stage = Some(k);
        self
    }
}

/// The mode a stock level's layer is (an editor project names its own): Squad Deathmatch by its kill counters, Rush
/// by its M-COMs (Squad Rush: one per stage), Conquest by its flags.
#[must_use]
pub fn detect_mode(w: &LevelWriter) -> Option<Mode> {
    let lv = Lv::new(w);
    if !lv.of("KillCounterEntityData").is_empty() {
        return Some(Mode::SquadDeathmatch);
    }
    let stages = crate::modelayer::stages(w);
    if !stages.is_empty() && !lv.of("LifeCounterEntityData").is_empty() {
        return Some(if stages.iter().all(|s| s.len() == 1) {
            Mode::SquadRush
        } else {
            Mode::Rush
        });
    }
    let flags = lv
        .of("CapturePointEntityData")
        .iter()
        .filter(|c| !lv.boolean(c, "UnableToChangeTeam"))
        .count();
    (flags > 0 && !lv.of("TicketCounterEntityData").is_empty()).then_some(Mode::Conquest)
}

/// Every measurement of L1's rules on a level partition played as `mode`.
#[must_use]
pub fn measure(w: &LevelWriter, mode: Mode) -> Vec<Measure> {
    let lv = Lv::new(w);
    let areas = Areas::new(&lv);
    let mut out = Out(Vec::new());
    match mode {
        Mode::Conquest => conquest(&lv, &areas, &mut out),
        Mode::Rush => rush(&lv, &areas, "rush", &mut out),
        Mode::SquadRush => rush(&lv, &areas, "sr", &mut out),
        Mode::SquadDeathmatch => sdm(&lv, &areas, &mut out),
    }
    for s in lv.of("SoldierSpawnEntityData") {
        if let Some(v) = lv.num(&s, "SpawnEnemyRadius") {
            out.rec("all.spawn_enemy_radius", v, vec![s.clone()])
                .about(lv.label(&s));
        }
    }
    out.0
}

#[allow(clippy::too_many_lines, reason = "L1's Conquest measurements in its order")]
fn conquest(lv: &Lv<'_>, a: &Areas, out: &mut Out) {
    let cps = lv.of("CapturePointEntityData");
    let mut flags: Vec<String> = cps
        .iter()
        .filter(|c| !lv.boolean(c, "UnableToChangeTeam") && lv.pos(c).is_some())
        .cloned()
        .collect();
    flags.sort_by(|x, y| {
        lv.ival(x, "Index")
            .unwrap_or(0.0)
            .total_cmp(&lv.ival(y, "Index").unwrap_or(0.0))
    });
    let team_of = |c: &str| lv.text(c, "InitialOwnerTeam").unwrap_or_default();
    let hqs: Vec<String> = cps
        .iter()
        .filter(|c| {
            lv.boolean(c, "UnableToChangeTeam")
                && matches!(team_of(c).as_str(), "Team1" | "Team2")
                && lv.pos(c).is_some()
        })
        .cloned()
        .collect();
    let pos = |g: &str| lv.pos(g).unwrap_or([0.0; 3]);
    out.rec("cq.flag_count", flags.len() as f64, flags.clone());
    for (i, fa) in flags.iter().enumerate() {
        for fb in flags.iter().skip(i + 1) {
            let (pa, pb) = (pos(fa), pos(fb));
            let m = out
                .rec("cq.flag_pair_dist", d2(pa, pb), vec![fa.clone(), fb.clone()])
                .about(format!("{} - {}", lv.label(fa), lv.label(fb)));
            m.dy = Some((pa[1] - pb[1]).abs());
        }
    }
    for f in &flags {
        if let Some(o) = flags
            .iter()
            .filter(|o| *o != f)
            .min_by(|x, y| d2(pos(f), pos(x)).total_cmp(&d2(pos(f), pos(y))))
        {
            out.rec("cq.flag_nn_dist", d2(pos(f), pos(o)), vec![f.clone(), o.clone()])
                .about(lv.label(f));
        }
    }
    for h in &hqs {
        if let Some(f) = flags
            .iter()
            .min_by(|x, y| d2(pos(h), pos(x)).total_cmp(&d2(pos(h), pos(y))))
        {
            out.rec("cq.hq_nearest_flag", d2(pos(h), pos(f)), vec![h.clone(), f.clone()])
                .about(lv.label(h));
        }
    }
    let t1 = hqs.iter().find(|h| team_of(h) == "Team1");
    let t2 = hqs.iter().find(|h| team_of(h) == "Team2");
    if let (Some(h1), Some(h2)) = (t1, t2) {
        let (p1, p2) = (pos(h1), pos(h2));
        let hh = d2(p1, p2);
        out.rec("cq.hq_hq_dist", hh, vec![h1.clone(), h2.clone()]);
        if !flags.is_empty() {
            let n = |p: [f64; 3]| flags.iter().map(|f| d2(p, pos(f))).fold(f64::INFINITY, f64::min);
            out.rec(
                "cq.hq_nearest_flag_asymmetry",
                (n(p1) - n(p2)).abs(),
                vec![h1.clone(), h2.clone()],
            );
        }
        if hh > 1.0 {
            let u = [(p2[0] - p1[0]) / hh, (p2[2] - p1[2]) / hh];
            for f in &flags {
                let pf = pos(f);
                let v = [pf[0] - p1[0], pf[2] - p1[2]];
                out.rec("cq.flag_axis_t", (v[0] * u[0] + v[1] * u[1]) / hh, vec![f.clone()])
                    .about(lv.label(f));
                out.rec(
                    "cq.flag_axis_lateral",
                    (v[0] * u[1] - v[1] * u[0]).abs() / hh,
                    vec![f.clone()],
                )
                .about(lv.label(f));
            }
        }
    }
    for f in &flags {
        if let Some((r, aabb, zone)) = capture_zone(lv, f) {
            let m = out
                .rec("cq.capture_zone", r, [vec![f.clone()], zone].concat())
                .about(lv.label(f));
            if aabb {
                m.tag = "aabb";
            }
        }
        if let Some(t) = lv.ival(f, "TimeToGetControl") {
            out.rec("cq.time_to_capture", t, vec![f.clone()]).about(lv.label(f));
        }
    }
    for c in flags.iter().chain(&hqs) {
        let (sol, _) = lv.cp_spawners(c);
        let pts: Vec<(String, [f64; 3])> = sol.iter().flat_map(|s| lv.spawn_points(s)).collect();
        let is_hq = hqs.contains(c);
        let metric = if is_hq {
            "cq.hq_spawn_points"
        } else {
            "cq.flag_spawn_points"
        };
        out.rec(metric, pts.len() as f64, vec![c.clone()]).about(lv.label(c));
        if !is_hq
            && let Some((g, d)) = pts
                .iter()
                .map(|(g, p)| (g, d2(pos(c), *p)))
                .max_by(|x, y| x.1.total_cmp(&y.1))
        {
            out.rec("cq.flag_spawn_dist_max", d, vec![g.clone(), c.clone()])
                .about(lv.label(c));
        }
    }
    // round A2: the vehicle pools (L1's `cq.vehicle_spawner` and `l1_a1.py`'s pools): every spawner of a vehicle
    // other than the ambient backdrop by its nearest flag or base (flags first among equals); the spawners within
    // 150 m of it are its pool, one per spawner
    let anchors: Vec<&String> = flags.iter().chain(&hqs).collect();
    let mut pools: BTreeMap<usize, Vec<String>> = BTreeMap::new();
    for v in lv.of("VehicleSpawnEntityData") {
        let Some(p) = lv.pos(&v) else { continue };
        if !lv.vehicles_of(&v).iter().any(|x| x.1 != "ambient") {
            continue;
        }
        if let Some((k, d)) = anchors
            .iter()
            .enumerate()
            .map(|(k, a)| (k, d2(pos(a), p)))
            .min_by(|x, y| x.1.total_cmp(&y.1))
            && d <= 150.0
        {
            pools.entry(k).or_default().push(v);
        }
    }
    for (k, vs) in pools {
        let Some(a) = anchors.get(k) else { continue };
        let metric = if hqs.contains(a) {
            "cq.hq_vehicle_pool"
        } else {
            "cq.flag_vehicle_pool"
        };
        out.rec(metric, vs.len() as f64, vs).about(lv.label(a));
    }
    for t in lv.of("TicketCounterEntityData") {
        if let Some(v) = lv.ival(&t, "InitialTicketCount") {
            out.rec("cq.tickets", v, vec![t.clone()])
                .about(lv.text(&t, "TeamId").unwrap_or_default());
        }
    }
    // combat areas
    if a.cas.is_empty() {
        return;
    }
    if let Some(v) = a.area(&a.cas) {
        out.rec("cq.combat_area_union", v, a.cas.clone());
    }
    let per_team: [(&str, Vec<String>); 2] = [("Team1", a.team_areas("Team1")), ("Team2", a.team_areas("Team2"))];
    let margin_of = |p: [f64; 3], t: &str| per_team.iter().find(|x| x.0 == t).and_then(|x| a.margin(p, &x.1));
    for f in &flags {
        let ms: Vec<f64> = per_team.iter().filter_map(|(t, _)| margin_of(pos(f), t)).collect();
        if let Some(m) = ms.iter().copied().reduce(f64::min) {
            out.rec("cq.flag_margin", m, vec![f.clone()]).about(lv.label(f));
        }
    }
    for h in &hqs {
        let own = team_of(h);
        if let Some(m) = margin_of(pos(h), &own) {
            out.rec("cq.hq_margin", m, vec![h.clone()]).about(lv.label(h));
        }
        let enemy = if own == "Team1" { "Team2" } else { "Team1" };
        let per_team_areas = a
            .cas
            .iter()
            .any(|c| a.restricted.get(c).copied().unwrap_or(false) && a.team.get(c).map(String::as_str) == Some(enemy));
        if per_team_areas && let Some(m) = margin_of(pos(h), enemy) {
            out.rec("cq.hq_vs_enemy_area", m, vec![h.clone()]).about(lv.label(h));
        }
    }
    // every capture point's spawn points against their team's area
    for c in &cps {
        let (sol, _) = lv.cp_spawners(c);
        for s in sol {
            let team = lv.text(&s, "Team").unwrap_or_default();
            let gs = per_team
                .iter()
                .find(|x| x.0 == team)
                .map_or_else(|| a.cas.clone(), |x| x.1.clone());
            for (g, p) in lv.spawn_points(&s) {
                if let Some(m) = a.margin(p, &gs) {
                    out.rec("cq.spawn_margin", m, vec![g, s.clone(), c.clone()])
                        .about(lv.label(c));
                }
            }
        }
    }
}

/// A capture zone's equivalent radius (from its `Geometry`: spheres, boxes read as half sizes, outlines; else
/// `CaptureRadius`), whether it holds a box, and its geometry's objects.
fn capture_zone(lv: &Lv<'_>, cp: &str) -> Option<(f64, bool, Vec<String>)> {
    let mut area = 0.0;
    let mut objects = Vec::new();
    let mut aabb = false;
    let mut any = false;
    let mut seen: HashSet<Vec<String>> = HashSet::new();
    let mut radius = None;
    for s in lv.refs(cp, "Geometry") {
        match lv.ty(&s) {
            "AreaGeometryEntityData" => {
                let (mut gs, pts) = lv.chain(&s);
                gs.sort();
                if pts.len() < 3 || !seen.insert(gs.clone()) {
                    continue;
                }
                let p: Vec<[f64; 2]> = pts.iter().map(|q| [q[0], q[2]]).collect();
                area += shoelace(&p);
                objects.extend(gs);
                any = true;
            }
            "SphereGeometryEntityData" => {
                let r = lv.num(&s, "Radius").unwrap_or(0.0);
                area += std::f64::consts::PI * r * r;
                radius = Some(r);
                objects.push(s);
                any = true;
            }
            "AabbGeometryEntityData" => {
                let e = lv.node(&s).and_then(|n| lv.w.d.child(n, b"Extents")).and_then(|c| {
                    let (size, vals) = bc2_formats::dbx::edit::numbers_of(lv.w.d.raw(c))?;
                    (size == 4).then(|| {
                        vals.iter()
                            .take(3)
                            .filter_map(|v| <[u8; 4]>::try_from(*v).ok())
                            .map(|b| f64::from(f32::from_be_bytes(b)))
                            .collect::<Vec<f64>>()
                    })
                });
                let e = e.unwrap_or_default();
                area += 4.0 * e.first().copied().unwrap_or(0.0) * e.get(2).copied().unwrap_or(0.0);
                aabb = true;
                objects.push(s);
                any = true;
            }
            _ => {}
        }
    }
    if !any {
        let r = lv.num(cp, "CaptureRadius").unwrap_or(0.0);
        return (r > 0.0).then_some((r, false, objects));
    }
    let eq = if area > 0.0 {
        Some((area / std::f64::consts::PI).sqrt())
    } else {
        radius
    };
    eq.map(|r| (r, aabb, objects))
}

#[allow(clippy::too_many_lines, reason = "L1's Rush measurements in its order")]
fn rush(lv: &Lv<'_>, a: &Areas, prefix: &'static str, out: &mut Out) {
    let m = |name: &str| -> &'static str {
        // the metric's name under the prefix (rush.* / sr.*)
        const NAMES: &[&str] = &[
            "stage_count",
            "mcoms_per_stage",
            "mcom_pair_dist",
            "stage_step",
            "attacker_spawn_to_obj_min",
            "defender_spawn_to_obj_min",
            "spawn_ratio",
            "attacker_spawn_depth",
            "defender_spawn_depth",
            "attacker_spawns_in_enemy_area",
            "defender_spawns_in_enemy_area",
            "obj_margin_attacker_area",
            "obj_margin_defender_area",
            "attacker_area_kept",
            "defender_area_kept",
            "lives",
            "attacker_vehicles",
            "defender_vehicles",
        ];
        const RUSH: &[&str] = &[
            "rush.stage_count",
            "rush.mcoms_per_stage",
            "rush.mcom_pair_dist",
            "rush.stage_step",
            "rush.attacker_spawn_to_obj_min",
            "rush.defender_spawn_to_obj_min",
            "rush.spawn_ratio",
            "rush.attacker_spawn_depth",
            "rush.defender_spawn_depth",
            "rush.attacker_spawns_in_enemy_area",
            "rush.defender_spawns_in_enemy_area",
            "rush.obj_margin_attacker_area",
            "rush.obj_margin_defender_area",
            "rush.attacker_area_kept",
            "rush.defender_area_kept",
            "rush.lives",
            "rush.attacker_vehicles",
            "rush.defender_vehicles",
        ];
        const SR: &[&str] = &[
            "sr.stage_count",
            "sr.mcoms_per_stage",
            "sr.mcom_pair_dist",
            "sr.stage_step",
            "sr.attacker_spawn_to_obj_min",
            "sr.defender_spawn_to_obj_min",
            "sr.spawn_ratio",
            "sr.attacker_spawn_depth",
            "sr.defender_spawn_depth",
            "sr.attacker_spawns_in_enemy_area",
            "sr.defender_spawns_in_enemy_area",
            "sr.obj_margin_attacker_area",
            "sr.obj_margin_defender_area",
            "sr.attacker_area_kept",
            "sr.defender_area_kept",
            "sr.lives",
            "sr.attacker_vehicles",
            "sr.defender_vehicles",
        ];
        let i = NAMES.iter().position(|n| *n == name).unwrap_or(0);
        let t = if prefix == "rush" { RUSH } else { SR };
        t.get(i).copied().unwrap_or("rush.stage_count")
    };
    let lcs = lv.of("LifeCounterEntityData");
    let att = lcs
        .iter()
        .find(|c| lv.boolean(c, "Attacker"))
        .and_then(|c| lv.text(c, "TeamId"))
        .unwrap_or_else(|| "Team1".into());
    let dfn = if att == "Team1" { "Team2" } else { "Team1" }.to_owned();
    for c in &lcs {
        if let Some(v) = lv.ival(c, "StartingLifeCount") {
            let r = out.rec(m("lives"), v, vec![c.clone()]);
            if lv.boolean(c, "Attacker") {
                r.tag = "attacker";
            }
        }
    }
    let (stages, _) = crate::modelayer::rush_play(lv.w);
    out.rec(
        m("stage_count"),
        stages.len() as f64,
        stages.iter().flat_map(|s| s.mcoms.clone()).collect(),
    );
    for (k, s) in stages.iter().enumerate() {
        let r = out.rec(m("mcoms_per_stage"), s.mcoms.len() as f64, s.mcoms.clone());
        r.stage = Some(k + 1);
    }
    let cps = lv.of("CapturePointEntityData");
    let mut prev_cent: Option<[f64; 3]> = None;
    let mut prev_area: HashMap<&str, Option<Vec<bool>>> = HashMap::new();
    for (k0, s) in stages.iter().enumerate() {
        let k = k0 + 1;
        let on = |g: &str| s.enabled.get(g).copied().unwrap_or(false);
        let mp: Vec<(String, [f64; 3])> = s
            .mcoms
            .iter()
            .filter_map(|g| lv.pos(g).map(|p| (g.clone(), p)))
            .collect();
        if mp.is_empty() {
            continue;
        }
        let n = mp.len() as f64;
        let cent = [0, 1, 2].map(|i| mp.iter().map(|x| x.1.get(i).copied().unwrap_or(0.0)).sum::<f64>() / n);
        if let [(ga, pa), (gb, pb)] = mp.as_slice() {
            let r = out.rec(m("mcom_pair_dist"), d2(*pa, *pb), vec![ga.clone(), gb.clone()]);
            r.stage = Some(k);
            r.dy = Some((pa[1] - pb[1]).abs());
        }
        if let Some(pc) = prev_cent {
            out.rec(m("stage_step"), d2(pc, cent), s.mcoms.clone()).at_stage(k);
        }
        let came_from = prev_cent;
        prev_cent = Some(cent);
        let active: Vec<&String> = cps.iter().filter(|c| on(c)).collect();
        let mut nearest: HashMap<&str, f64> = HashMap::new();
        let mut side_pts: HashMap<&str, Vec<[f64; 3]>> = HashMap::new();
        for (side, team) in [("attacker", att.as_str()), ("defender", dfn.as_str())] {
            let mine: Vec<&String> = active
                .iter()
                .copied()
                .filter(|c| lv.text(c, "InitialOwnerTeam").as_deref() == Some(team))
                .collect();
            let pts: Vec<(String, [f64; 3])> = mine
                .iter()
                .flat_map(|c| lv.cp_spawners(c).0)
                .flat_map(|sg| lv.spawn_points(&sg))
                .collect();
            if let Some((d, g)) = pts
                .iter()
                .map(|(g, p)| (mp.iter().map(|x| d2(*p, x.1)).fold(f64::INFINITY, f64::min), g))
                .min_by(|x, y| x.0.total_cmp(&y.0))
            {
                let metric = if side == "attacker" {
                    "attacker_spawn_to_obj_min"
                } else {
                    "defender_spawn_to_obj_min"
                };
                out.rec(m(metric), d, vec![g.clone()]).at_stage(k);
                nearest.insert(side, d);
            }
            side_pts.insert(side, pts.iter().map(|x| x.1).collect());
            // round A2: the side's vehicles while the stage is played (L1's `<side>_vehicles`): the vehicle spawners
            // of its active bases, one per spawner whose vehicle for the team is not the ambient backdrop
            let veh: Vec<String> = mine
                .iter()
                .flat_map(|c| lv.cp_spawners(c).1)
                .filter(|v| lv.spawner_class(v, team).is_some_and(|c| c != "ambient"))
                .collect();
            let metric = if side == "attacker" {
                "attacker_vehicles"
            } else {
                "defender_vehicles"
            };
            out.rec(m(metric), veh.len() as f64, veh).at_stage(k);
            // the side's combat area while the stage is played
            let cas: Vec<String> = a
                .cas
                .iter()
                .filter(|c| {
                    on(c) && matches!(a.team.get(*c).map(String::as_str), Some(t) if t == team || t == "TeamNeutral")
                })
                .cloned()
                .collect();
            if cas.is_empty() {
                prev_area.insert(side, None);
                continue;
            }
            let cm = a.grid.as_ref().map(|g| g.mask(&a.of(&cas)));
            if let (Some(g), Some(Some(pm)), Some(cm)) = (a.grid.as_ref(), prev_area.get(side), cm.as_ref()) {
                let inter: Vec<bool> = pm.iter().zip(cm).map(|(x, y)| *x && *y).collect();
                let kept = g.area(&inter) / g.area(pm).max(1.0) * 100.0;
                let metric = if side == "attacker" {
                    "attacker_area_kept"
                } else {
                    "defender_area_kept"
                };
                out.rec(m(metric), kept, cas.clone()).at_stage(k);
            }
            prev_area.insert(side, cm);
            for (g, p) in &mp {
                if let Some(v) = a.margin(*p, &cas) {
                    let metric = if side == "attacker" {
                        "obj_margin_attacker_area"
                    } else {
                        "obj_margin_defender_area"
                    };
                    out.rec(m(metric), v, [vec![g.clone()], cas.clone()].concat())
                        .at_stage(k);
                }
            }
            // the other side's spawn points against this side's area
            let other = if team == att { dfn.as_str() } else { att.as_str() };
            let opts: Vec<(String, [f64; 3])> = active
                .iter()
                .filter(|c| lv.text(c, "InitialOwnerTeam").as_deref() == Some(other))
                .flat_map(|c| lv.cp_spawners(c).0)
                .flat_map(|sg| lv.spawn_points(&sg))
                .collect();
            if let Some((v, g)) = opts
                .iter()
                .filter_map(|(g, p)| a.margin(*p, &cas).map(|v| (v, g)))
                .max_by(|x, y| x.0.total_cmp(&y.0))
            {
                let metric = if side == "attacker" {
                    "defender_spawns_in_enemy_area"
                } else {
                    "attacker_spawns_in_enemy_area"
                };
                out.rec(m(metric), v, [vec![g.clone()], cas.clone()].concat())
                    .at_stage(k);
            }
        }
        // the spawns against the attack direction: from the previous stage's objectives (stage 1: the attackers'
        // spawn points) to this stage's
        let start = came_from.or_else(|| {
            side_pts.get("attacker").filter(|v| !v.is_empty()).map(|v| {
                let n = v.len() as f64;
                [0, 1, 2].map(|i| v.iter().map(|p| p.get(i).copied().unwrap_or(0.0)).sum::<f64>() / n)
            })
        });
        if let Some(st) = start {
            let u = [cent[0] - st[0], cent[2] - st[2]];
            let len = u[0].hypot(u[1]);
            if len > 1.0 {
                let u = [u[0] / len, u[1] / len];
                for side in ["attacker", "defender"] {
                    if let Some(v) = side_pts.get(side).filter(|v| !v.is_empty()) {
                        let n = v.len() as f64;
                        let sc = [
                            v.iter().map(|p| p[0]).sum::<f64>() / n,
                            v.iter().map(|p| p[2]).sum::<f64>() / n,
                        ];
                        let depth = (sc[0] - cent[0]) * u[0] + (sc[1] - cent[2]) * u[1];
                        let metric = if side == "attacker" {
                            "attacker_spawn_depth"
                        } else {
                            "defender_spawn_depth"
                        };
                        out.rec(m(metric), depth, s.mcoms.clone()).at_stage(k);
                    }
                }
            }
        }
        if let (Some(&at), Some(&de)) = (nearest.get("attacker"), nearest.get("defender"))
            && at != 0.0
            && de != 0.0
        {
            out.rec(m("spawn_ratio"), at / de.max(1.0), s.mcoms.clone()).at_stage(k);
        }
    }
}

fn sdm(lv: &Lv<'_>, a: &Areas, out: &mut Out) {
    let cps = lv.of("CapturePointEntityData");
    let squads: Vec<&String> = cps
        .iter()
        .filter(|c| {
            let t = lv.text(c, "InitialOwnerTeam").unwrap_or_default();
            t.starts_with("Team") && t != "TeamNeutral"
        })
        .collect();
    out.rec(
        "sdm.squad_count",
        squads.len() as f64,
        squads.iter().map(|x| (*x).clone()).collect(),
    );
    let mut uniq: BTreeMap<(i64, i64), (String, [f64; 3])> = BTreeMap::new();
    for c in &squads {
        for s in lv.cp_spawners(c).0 {
            for (g, p) in lv.spawn_points(&s) {
                uniq.insert(((p[0] * 10.0).round() as i64, (p[2] * 10.0).round() as i64), (g, p));
            }
        }
    }
    let pts: Vec<(String, [f64; 3])> = uniq.into_values().collect();
    out.rec(
        "sdm.spawn_points",
        pts.len() as f64,
        pts.iter().take(40).map(|x| x.0.clone()).collect(),
    );
    if pts.len() >= 2 {
        let mut nn: Vec<f64> = pts
            .iter()
            .enumerate()
            .map(|(i, (_, p))| {
                pts.iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, (_, q))| d2(*p, *q))
                    .fold(f64::INFINITY, f64::min)
            })
            .collect();
        if let Some(v) = median(&mut nn) {
            out.rec("sdm.spawn_nn_median", v, Vec::new());
        }
    }
    let veh: Vec<String> = cps.iter().flat_map(|c| lv.cp_spawners(c).1).collect();
    let n = veh
        .iter()
        .flat_map(|v| lv.vehicles_of(v))
        .filter(|(_, c)| *c != "ambient")
        .count();
    out.rec("sdm.vehicles", n as f64, veh);
    if a.cas.is_empty() {
        return;
    }
    if let Some(v) = a.area(&a.cas) {
        out.rec("sdm.combat_area_union", v, a.cas.clone());
    }
    if let Some((m, g)) = pts
        .iter()
        .filter_map(|(g, p)| a.margin(*p, &a.cas).map(|m| (m, g)))
        .min_by(|x, y| x.0.total_cmp(&y.0))
    {
        out.rec("sdm.spawn_margin_min", m, vec![g.clone()]);
    }
}

// ------------------------------------------------------------------------------------------------ the rules

/// The L1 rules V1 checks: all but L1-HT-001 (it needs the terrain's height field).
#[must_use]
pub fn implemented(id: &str) -> bool {
    id != "L1-HT-001"
}

/// The specification rules an L1 rule's tendency is written down in (`spec/engine/gameplay.md`).
#[must_use]
pub fn spec_of(id: &str) -> &'static [&'static str] {
    match id.trim_end_matches(['w', 's']) {
        "L1-CQ-001" => &["GAME-CQ-2"],
        "L1-CQ-002" | "L1-CQ-003" | "L1-CQ-004" | "L1-CQ-005" => &["GAME-CQ-3"],
        "L1-CQ-006" | "L1-CQ-007" => &["GAME-CQ-4"],
        "L1-CQ-008" => &["GAME-CQ-5"],
        "L1-CQ-009" => &["GAME-CQ-6"],
        "L1-CQ-010" | "L1-CQ-011" | "L1-CQ-012" => &["GAME-CQ-7", "GAME-SPAWN-1"],
        "L1-CQ-013" | "L1-CQ-014" | "L1-CQ-015" | "L1-CQ-017" | "L1-AL-002" => &["GAME-AREA-1", "GAME-AREA-2"],
        "L1-CQ-016" => &["GAME-TKT-1"],
        "L1-RU-001" | "L1-RU-002" => &["GAME-RUSH-3"],
        "L1-RU-003" | "L1-RU-004" | "L1-RU-014" => &["GAME-RUSH-4"],
        "L1-RU-005" | "L1-RU-006" | "L1-RU-007" | "L1-RU-008" | "L1-RU-009" => &["GAME-RUSH-5"],
        "L1-RU-010" => &["GAME-RUSH-6"],
        "L1-RU-011" | "L1-RU-012" | "L1-RU-013" => &["GAME-RUSH-7"],
        "L1-RU-015" => &["GAME-RUSH-8"],
        "L1-SD-001" | "L1-SD-002" | "L1-SD-003" | "L1-SD-006" => &["GAME-SDM-1"],
        "L1-SD-004" | "L1-SD-005" => &["GAME-SDM-2"],
        "L1-AL-001" => &["GAME-SPAWN-2"],
        "L1-HT-002" => &["GAME-CQ-9"],
        _ => &["GAME-LAYER-4"],
    }
}

/// An L1 rule's title: its metric in words.
#[must_use]
pub(crate) fn title_of(r: &L1Rule) -> String {
    let scope = r
        .modes
        .as_ref()
        .map_or_else(|| "every mode".to_owned(), |m| m.join(" / "));
    format!("{} ({scope}; {})", r.metric, r.groups.join(", "))
}

/// The rules whose value is the record's height difference (L1's `DY_RULES`).
const DY_RULES: &[&str] = &["L1-RU-014", "L1-HT-002"];

/// L1's message with a measurement's value, subject and stage (`{v:.0f}`, `{v:.2f}`, `{s}`, `{st}`, `{pst}`).
fn fill(template: &str, v: &str, m: &Measure) -> String {
    let mut s = template.to_owned();
    for (k, prec) in [("{v:.0f}", 0usize), ("{v:.1f}", 1), ("{v:.2f}", 2)] {
        if s.contains(k) {
            let val: f64 = v.parse().unwrap_or(0.0);
            s = s.replace(k, &format!("{val:.prec$}"));
        }
    }
    s = s.replace("{v}", v);
    s = s.replace("{s}", &m.subject);
    if let Some(k) = m.stage {
        s = s.replace("{st}", &k.to_string());
        s = s.replace("{pst}", &(k.saturating_sub(1)).to_string());
    }
    s
}

fn band_holds(b: Option<[Option<f64>; 2]>, v: f64) -> bool {
    b.is_none_or(|[lo, hi]| lo.is_none_or(|l| v >= l) && hi.is_none_or(|h| v <= h))
}

/// The findings of L1's rules on measurements of a level of `group` (L1's era group: [`group`]) played as `mode`.
#[must_use]
pub fn check_measures(measures: &[Measure], mode: Mode, group: &str) -> Vec<Finding> {
    let mut out = Vec::new();
    for r in crate::rules::l1_rules() {
        if !implemented(&r.id)
            || !r.groups.iter().any(|g| g == group)
            || r.modes.as_ref().is_some_and(|m| !m.iter().any(|x| x == mode.word()))
        {
            continue;
        }
        // rush.* metrics are sr.* on Squad Rush
        let metric = if mode == Mode::SquadRush {
            r.metric.replacen("rush.", "sr.", 1)
        } else {
            r.metric.clone()
        };
        let stock = r.range.map_or_else(
            || "stock: two M-COMs every stage".to_owned(),
            |[lo, hi]| format!("stock {} to {}", trim(lo), trim(hi)),
        );
        let cite = format!(" [{}: {stock}, {} levels]", r.id, r.levels);
        if r.id == "L1-RU-002" {
            let bad: Vec<&Measure> = measures
                .iter()
                .filter(|m| m.metric == metric && (m.value - 2.0).abs() > 0.01)
                .collect();
            for m in bad {
                out.push(
                    Finding::new(&r.id, format!("{}{cite}", fill(&r.message, &trim(m.value), m)))
                        .naming(m.objects.clone()),
                );
            }
            continue;
        }
        for m in measures.iter().filter(|m| m.metric == metric) {
            if (r.id == "L1-CQ-008" && m.tag == "aabb")
                || (r.id == "L1-RU-010" && m.stage != Some(1))
                || (r.id == "L1-RU-015" && m.tag != "attacker")
            {
                continue;
            }
            let v = if DY_RULES.contains(&r.id.as_str()) {
                match m.dy {
                    Some(d) => d,
                    None => continue,
                }
            } else {
                m.value
            };
            // L1 rounds every measurement to 0.1 before its rules read it (`r1`)
            let v = (v * 10.0).round() / 10.0;
            let sev = if !band_holds(r.warn, v) {
                Severity::Warning
            } else if !band_holds(r.info, v) {
                Severity::Note
            } else {
                continue;
            };
            out.push(
                Finding::new(&r.id, format!("{}{cite}", fill(&r.message, &v.to_string(), m)))
                    .at(sev)
                    .naming(m.objects.clone()),
            );
        }
    }
    out
}

fn trim(v: f64) -> String {
    let s = format!("{v:.1}");
    s.strip_suffix(".0").map_or(s.clone(), str::to_owned)
}

/// The placement findings of a level partition played as `mode` in `profile`'s era.
#[must_use]
pub fn check(w: &LevelWriter, mode: Mode, profile: Profile) -> Vec<Finding> {
    check_measures(&measure(w, mode), mode, group(profile))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_raster_area_of_a_square_and_its_margin() {
        let sq = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
        let g = Grid::new(&[&sq]).unwrap();
        let a = g.area(&g.mask(&[&sq]));
        assert!((a - 10_000.0).abs() < 900.0, "{a}");
        assert!(inside([50.0, 50.0], &sq) && !inside([150.0, 50.0], &sq));
        assert!((shoelace(&sq) - 10_000.0).abs() < 1e-9);
    }

    #[test]
    fn messages_take_the_value_subject_and_stage() {
        let m = Measure {
            metric: "rush.stage_step",
            value: 40.0,
            dy: None,
            subject: "A".into(),
            stage: Some(3),
            tag: "",
            objects: Vec::new(),
        };
        assert_eq!(
            fill("Stage {st} is {v:.0f} m from stage {pst}; {s}", "40.2", &m),
            "Stage 3 is 40 m from stage 2; A"
        );
        assert!(band_holds(Some([Some(75.0), None]), 80.0) && !band_holds(Some([Some(75.0), None]), 40.0));
        assert_eq!(vehicle_class("objects/vehicles/land/m1a2/m1a2/x"), "tank");
        assert_eq!(vehicle_class("objects/vehicles/air/a10/a10/x"), "ambient");
    }
}
