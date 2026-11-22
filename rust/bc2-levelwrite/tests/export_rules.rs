//! Round V1: the export's rule registry on synthetic broken projects - one per rule of the edit phase and per L1
//! placement rule - built from small synthetic levels (no game data): a clean Conquest, Rush, Squad Rush and Squad
//! Deathmatch layout raises nothing, and each layout broken one way raises its rule (a warning or a note for L1's
//! tendencies, never an error). The stage rules' own pieces (console resources, deleted markers, group members) are
//! tested on synthetic bytes; whole stages are checked by the game tier (`v1_stage_rules`).
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::items_after_statements,
    missing_docs
)]

use std::collections::HashSet;

use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_levelwrite::modelayer::ModeChoice;
use bc2_levelwrite::names::LevelNames;
use bc2_levelwrite::placement::Mode;
use bc2_levelwrite::rules::{self, Finding, Severity};
use bc2_levelwrite::validate::{self, Input};
use bc2_levelwrite::write::LevelWriter;
use bc2_levelwrite::{MapProject, Op, Profile};

// ------------------------------------------------------------------------------------------------ a level builder

/// Instances of a synthetic level partition, made in order.
pub(crate) struct Lb {
    f: DbxFile,
    insts: Vec<Element>,
    objects: Vec<String>,
    events: Vec<(String, String, String, String)>,
    next: u32,
}

pub(crate) type Field = Box<dyn FnOnce(&mut DbxFile) -> Element>;

pub(crate) fn text(n: &'static str, v: &str) -> Field {
    let v = v.to_owned();
    Box::new(move |f| Element::text(f, "field", &[("name", n)], &v))
}

pub(crate) fn boolean(n: &'static str, v: bool) -> Field {
    Box::new(move |f| Element::scalar(f, "field", 6, &[("name", n)], [0, u8::from(v)]))
}

pub(crate) fn float(n: &'static str, v: f32) -> Field {
    Box::new(move |f| Element::numbers(f, "field", &[("name", n)], 4, &v.to_be_bytes()))
}

pub(crate) fn int(n: &'static str, v: i32) -> Field {
    Box::new(move |f| Element::numbers(f, "field", &[("name", n)], 4, &v.to_be_bytes()))
}

pub(crate) fn rf(n: &'static str, g: &str) -> Field {
    let g = g.to_owned();
    Box::new(move |f| Element::text(f, "field", &[("ref", &g), ("name", n)], ""))
}

pub(crate) fn refs(n: &'static str, gs: &[String]) -> Field {
    let gs = gs.to_vec();
    Box::new(move |f| {
        let items = gs.iter().map(|g| Element::text(f, "item", &[("ref", g)], "")).collect();
        Element::container(f, "array", &[("name", n)], items)
    })
}

pub(crate) fn at(p: [f32; 3]) -> Field {
    Box::new(move |f| {
        let mut t = Vec::new();
        for v in [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, p[0], p[1], p[2], 1.0f32,
        ] {
            t.extend(v.to_be_bytes());
        }
        Element::numbers(f, "complex", &[("name", "Transform")], 4, &t)
    })
}

impl Lb {
    pub(crate) fn new() -> Self {
        Self {
            f: DbxFile::default(),
            insts: Vec::new(),
            objects: Vec::new(),
            events: Vec::new(),
            next: 1,
        }
    }

    fn guid(&mut self) -> String {
        let g = format!("AAAAAAAA-0000-0000-0000-{:012}", self.next);
        self.next += 1;
        g
    }

    /// An instance of `ty` with `fields`; listed in LevelData.Objects when `listed`.
    pub(crate) fn add(&mut self, ty: &str, fields: Vec<Field>, listed: bool) -> String {
        let g = self.guid();
        self.add_as(&g, ty, fields, listed);
        g
    }

    pub(crate) fn add_as(&mut self, g: &str, ty: &str, fields: Vec<Field>, listed: bool) {
        let kids: Vec<Element> = fields.into_iter().map(|x| x(&mut self.f)).collect();
        let t = format!("Entity.{ty}");
        let e = Element::container(&mut self.f, "instance", &[("guid", g), ("type", &t)], kids);
        self.insts.push(e);
        if listed {
            self.objects.push(g.to_owned());
        }
    }

    pub(crate) fn event(&mut self, src: &str, sev: &str, dst: &str, tev: &str) {
        self.events.push((src.into(), sev.into(), dst.into(), tev.into()));
    }

    /// A combat area: its outline's points chained by `Next`.
    pub(crate) fn area(&mut self, team: &str, restricted: bool, enabled: bool, pts: &[[f32; 2]]) -> String {
        let gs: Vec<String> = pts.iter().map(|_| self.guid()).collect();
        for (k, p) in pts.iter().enumerate() {
            let next = gs[(k + 1) % gs.len()].clone();
            self.add_as(
                &gs[k],
                "AreaGeometryEntityData",
                vec![at([p[0], 0.0, p[1]]), rf("Next", &next)],
                false,
            );
        }
        self.add(
            "CombatAreaTriggerEntityData",
            vec![
                text("Team", team),
                boolean("UseTeamRestriction", restricted),
                boolean("Enabled", enabled),
                refs("Geometry", &gs[..1]),
            ],
            true,
        )
    }

    /// A soldier spawner of `team` with alternative points at `pts`.
    pub(crate) fn spawner(&mut self, team: &str, pts: &[[f32; 3]], radius: f32) -> String {
        let alts: Vec<String> = pts
            .iter()
            .map(|p| self.add("AlternateSpawnEntityData", vec![at(*p)], false))
            .collect();
        self.add(
            "SoldierSpawnEntityData",
            vec![
                text("Team", team),
                float("SpawnEnemyRadius", radius),
                refs("AlternativeSpawnPoints", &alts),
                at(pts.first().copied().unwrap_or_default()),
            ],
            true,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn capture_point(
        &mut self,
        name: &str,
        team: &str,
        base: bool,
        index: i32,
        pos: [f32; 3],
        spawners: &[String],
        radius: f32,
        time: i32,
        enabled: Option<bool>,
    ) -> String {
        let mut f = vec![
            text("Name", name),
            text("LocationName", name),
            text("InitialOwnerTeam", team),
            boolean("UnableToChangeTeam", base),
            int("Index", index),
            int("TimeToGetControl", time),
            float("CaptureRadius", radius),
            refs("Spawners", spawners),
            at(pos),
        ];
        if let Some(e) = enabled {
            f.push(boolean("Enabled", e));
        }
        self.add("CapturePointEntityData", f, true)
    }

    pub(crate) fn bytes(mut self) -> Vec<u8> {
        let f = &mut self.f;
        let items: Vec<Element> = self
            .objects
            .iter()
            .map(|g| Element::text(f, "item", &[("ref", g)], ""))
            .collect();
        let objs = Element::container(f, "array", &[("name", "Objects")], items);
        let conns: Vec<Element> = self
            .events
            .iter()
            .map(|(s, se, t, te)| {
                let src = Element::text(f, "field", &[("ref", s), ("name", "Source")], "");
                let tgt = Element::text(f, "field", &[("ref", t), ("name", "Target")], "");
                let sn = Element::text(f, "field", &[("name", "Name")], se);
                let sev = Element::container(f, "complex", &[("name", "SourceEvent")], vec![sn]);
                let tn = Element::text(f, "field", &[("name", "Name")], te);
                let tev = Element::container(f, "complex", &[("name", "TargetEvent")], vec![tn]);
                Element::container(f, "complex", &[], vec![src, tgt, sev, tev])
            })
            .collect();
        let ec = Element::container(f, "array", &[("name", "EventConnections")], conns);
        let name = Element::text(f, "field", &[("name", "Name")], "Levels/V1TEST");
        let ld = Element::container(
            f,
            "instance",
            &[
                ("guid", "AAAAAAAA-0000-0000-0000-000000000000"),
                ("type", "Entity.LevelData"),
                ("id", "V1TEST"),
            ],
            vec![name, objs, ec],
        );
        let mut insts = vec![ld];
        insts.append(&mut self.insts);
        let part = Element::container(f, "partition", &[("guid", "P")], insts);
        f.roots.push(part);
        dbx::write(f).unwrap()
    }
}

// ------------------------------------------------------------------------------------------------ the layouts

/// A Conquest layout: two HQs, flags, one shared or two per-team combat areas.
#[derive(Clone)]
pub(crate) struct Cq {
    pub(crate) hq: [[f32; 3]; 2],
    pub(crate) flags: Vec<[f32; 3]>,
    /// Per flag: its spawn points' offsets.
    pub(crate) flag_spawns: Vec<Vec<[f32; 3]>>,
    pub(crate) hq_spawns: usize,
    pub(crate) area: Vec<[f32; 2]>,
    /// Per team areas (team 1's, team 2's) instead of the shared one.
    pub(crate) team_areas: Option<[Vec<[f32; 2]>; 2]>,
    pub(crate) radius: f32,
    pub(crate) time: i32,
    pub(crate) tickets: i32,
    pub(crate) enemy_radius: f32,
}

pub(crate) fn square(h: f32) -> Vec<[f32; 2]> {
    vec![[-h, -h], [h, -h], [h, h], [-h, h]]
}

pub(crate) fn rect(x0: f32, z0: f32, x1: f32, z1: f32) -> Vec<[f32; 2]> {
    vec![[x0, z0], [x1, z0], [x1, z1], [x0, z1]]
}

impl Cq {
    pub(crate) fn clean() -> Self {
        let spawns = vec![[10.0, 0.0, 10.0], [-10.0, 0.0, 10.0], [0.0, 0.0, -15.0]];
        Self {
            hq: [[0.0, 0.0, -250.0], [0.0, 0.0, 250.0]],
            flags: vec![[0.0, 0.0, -120.0], [50.0, 0.0, 0.0], [0.0, 0.0, 120.0]],
            flag_spawns: vec![spawns.clone(), spawns.clone(), spawns],
            hq_spawns: 6,
            area: square(300.0),
            team_areas: None,
            radius: 15.0,
            time: 20,
            tickets: 250,
            enemy_radius: 20.0,
        }
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        self.builder().bytes()
    }

    /// The layout's instances, before the partition is made (more can be added).
    pub(crate) fn builder(&self) -> Lb {
        let mut b = Lb::new();
        for (k, (team, p)) in [("Team1", self.hq[0]), ("Team2", self.hq[1])].into_iter().enumerate() {
            let pts: Vec<[f32; 3]> = (0..self.hq_spawns)
                .map(|i| {
                    [
                        p[0] + 8.0 * i as f32 - 20.0,
                        p[1],
                        p[2] + if k == 0 { -10.0 } else { 10.0 },
                    ]
                })
                .collect();
            let s = b.spawner(team, &pts, self.enemy_radius);
            b.capture_point(&format!("HQ {team}"), team, true, 0, p, &[s], 20.0, 0, None);
        }
        for (i, f) in self.flags.iter().enumerate() {
            let mut sp = Vec::new();
            if let Some(off) = self.flag_spawns.get(i).filter(|o| !o.is_empty()) {
                for team in ["Team1", "Team2"] {
                    let pts: Vec<[f32; 3]> = off.iter().map(|o| [f[0] + o[0], f[1] + o[1], f[2] + o[2]]).collect();
                    sp.push(b.spawner(team, &pts, self.enemy_radius));
                }
            }
            let name = ["A", "B", "C", "D", "E"][i % 5];
            b.capture_point(
                name,
                "TeamNeutral",
                false,
                i as i32,
                *f,
                &sp,
                self.radius,
                self.time,
                None,
            );
        }
        for t in ["Team1", "Team2"] {
            b.add(
                "TicketCounterEntityData",
                vec![text("TeamId", t), int("InitialTicketCount", self.tickets)],
                true,
            );
        }
        match &self.team_areas {
            Some([a1, a2]) => {
                b.area("Team1", true, true, a1);
                b.area("Team2", true, true, a2);
            }
            None => {
                b.area("TeamNeutral", false, true, &self.area);
            }
        }
        b
    }
}

/// A Rush (or Squad Rush) layout along +z: stage k's M-COMs at z = 200 k, its attackers' base `att` behind them, its
/// defenders' base `def` in front; attackers' areas growing, defenders' per stage.
#[derive(Clone)]
pub(crate) struct Rush {
    stages: usize,
    /// M-COM x offsets of a stage (2 for Rush, 1 for Squad Rush).
    mcoms: Vec<f32>,
    step: f32,
    mcom_dy: f32,
    att: f32,
    def: f32,
    lives: i32,
    /// The attackers' area ends this far past a stage's M-COMs.
    att_front: f32,
    /// The defenders' area starts this far before a stage's M-COMs.
    def_back: f32,
    /// The attackers' area of a stage starts this far behind its M-COMs (None: from the map's start, growing).
    att_area_from: Option<f32>,
    /// One stage with an extra M-COM.
    third_mcom: bool,
}

impl Rush {
    pub(crate) fn clean() -> Self {
        Self {
            stages: 3,
            mcoms: vec![-20.0, 20.0],
            step: 200.0,
            mcom_dy: 0.0,
            att: -200.0,
            def: 40.0,
            lives: 100,
            att_front: 30.0,
            def_back: 60.0,
            att_area_from: None,
            third_mcom: false,
        }
    }

    pub(crate) fn squad() -> Self {
        Self {
            mcoms: vec![0.0],
            step: 120.0,
            att: -150.0,
            ..Self::clean()
        }
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        let mut b = Lb::new();
        let interact = b.add("GameInteractionEntityData", vec![], true);
        b.add(
            "LifeCounterEntityData",
            vec![
                text("TeamId", "Team1"),
                boolean("Attacker", true),
                int("StartingLifeCount", self.lives),
            ],
            true,
        );
        let mut counters = Vec::new();
        let mut per_stage: Vec<Vec<String>> = Vec::new(); // the stage's bases and areas
        for k in 0..self.stages {
            let z = self.step * k as f32;
            let on = k == 0;
            let counter = b.add("MultipleTriggerEntityData", vec![boolean("Enabled", on)], true);
            let mut xs = self.mcoms.clone();
            if self.third_mcom && k == 1 {
                xs.push(0.0);
            }
            for (i, x) in xs.iter().enumerate() {
                let y = if i == 1 { self.mcom_dy } else { 0.0 };
                let m = b.add(
                    "ReferenceObjectData",
                    vec![
                        rf(
                            "ReferencedObject",
                            "objects/objects/mpobjective/mpobjective/BBBBBBBB-0000-0000-0000-000000000001",
                        ),
                        at([*x, y, z]),
                    ],
                    true,
                );
                b.event(&m, "OnArm", &interact, "Arm");
                b.event(&m, "onDestroyed", &counter, "Trigger");
            }
            let pts = |dz: f32| vec![[-10.0, 0.0, z + dz], [10.0, 0.0, z + dz], [0.0, 0.0, z + dz - 5.0]];
            let sa = b.spawner("Team1", &pts(self.att), 20.0);
            let sd = b.spawner("Team2", &pts(self.def), 20.0);
            let ca = b.capture_point(
                "Attackers",
                "Team1",
                true,
                0,
                [0.0, 0.0, z + self.att],
                &[sa],
                20.0,
                0,
                Some(on),
            );
            let cd = b.capture_point(
                "Defenders",
                "Team2",
                true,
                0,
                [0.0, 0.0, z + self.def],
                &[sd],
                20.0,
                0,
                Some(on),
            );
            let a_from = self.att_area_from.map_or(-400.0, |d| z - d);
            let aa = b.area("Team1", true, on, &rect(-150.0, a_from, 150.0, z + self.att_front));
            let ad = b.area("Team2", true, on, &rect(-150.0, z - self.def_back, 150.0, z + 100.0));
            counters.push(counter);
            per_stage.push(vec![ca, cd, aa, ad]);
        }
        for k in 0..self.stages {
            for x in &per_stage[k] {
                b.event(&counters[k], "OnTriggerOut", x, "Disable");
            }
            if k + 1 < self.stages {
                b.event(&counters[k], "OnTriggerOut", &counters[k + 1], "Enable");
                for x in &per_stage[k + 1] {
                    b.event(&counters[k], "OnTriggerOut", x, "Enable");
                }
            }
        }
        b.bytes()
    }
}

/// A Squad Deathmatch layout: four squads sharing a grid of spawn points, a vehicle.
#[derive(Clone)]
pub(crate) struct Sdm {
    squads: usize,
    cols: usize,
    rows: usize,
    spacing: f32,
    area: f32,
    vehicles: usize,
    /// One spawn point moved this far outside the area.
    outside: Option<f32>,
}

impl Sdm {
    pub(crate) fn clean() -> Self {
        Self {
            squads: 4,
            cols: 5,
            rows: 4,
            spacing: 25.0,
            area: 75.0,
            vehicles: 1,
            outside: None,
        }
    }

    pub(crate) fn bytes(&self) -> Vec<u8> {
        let mut b = Lb::new();
        let mut pts = Vec::new();
        for i in 0..self.cols {
            for j in 0..self.rows {
                let x = (i as f32 - (self.cols as f32 - 1.0) / 2.0) * self.spacing;
                let z = (j as f32 - (self.rows as f32 - 1.0) / 2.0) * self.spacing;
                pts.push([x, 0.0, z]);
            }
        }
        if let Some(d) = self.outside {
            pts[0] = [self.area + d, 0.0, 0.0];
        }
        for s in 0..self.squads {
            let team = format!("Team{}", s + 1);
            let sp = b.spawner(&team, &pts, 20.0);
            let mut spawners = vec![sp];
            if s == 0 {
                for _ in 0..self.vehicles {
                    spawners.push(b.add(
                        "VehicleSpawnEntityData",
                        vec![
                            text("Team", "TeamNeutral"),
                            rf(
                                "Vehicle",
                                "objects/vehicles/land/m3a3/m3a3/CCCCCCCC-0000-0000-0000-000000000001",
                            ),
                            at([0.0, 0.0, 0.0]),
                        ],
                        true,
                    ));
                }
            }
            b.capture_point(
                &format!("Squad {s}"),
                &team,
                true,
                0,
                [0.0; 3],
                &spawners,
                10.0,
                0,
                None,
            );
            b.add("KillCounterEntityData", vec![text("TeamId", &team)], true);
        }
        b.area("TeamNeutral", false, true, &square(self.area));
        b.bytes()
    }
}

// ------------------------------------------------------------------------------------------------ running the rules

pub(crate) fn project(bytes: &[u8], profile: Profile) -> MapProject {
    MapProject::new("V1 test", "v1test", "Levels/V1TEST", bytes, profile)
}

fn findings(bytes: &[u8], profile: Profile, mode: Mode) -> Vec<Finding> {
    let p = project(bytes, profile);
    let w = LevelWriter::parse(bytes).unwrap();
    validate::validate(Input {
        project: &p,
        level: Some(&w),
        base: None,
        donor: None,
        game: None,
        languages: None,
        base_check: None,
        mode: Some(mode),
        site: None,
        routes: None,
    })
}

fn codes(f: &[Finding]) -> Vec<String> {
    f.iter().map(|x| format!("{} {}", x.code, x.severity.word())).collect()
}

#[test]
fn clean_layouts_raise_nothing() {
    let _gate = crate::gate();
    for (name, bytes, mode, profile) in [
        ("Conquest", Cq::clean().bytes(), Mode::Conquest, Profile::Bfbc2),
        ("Rush", Rush::clean().bytes(), Mode::Rush, Profile::Bfbc2),
        ("Squad Rush", Rush::squad().bytes(), Mode::SquadRush, Profile::Vietnam),
        (
            "Squad Deathmatch",
            Sdm::clean().bytes(),
            Mode::SquadDeathmatch,
            Profile::Bfbc1,
        ),
    ] {
        let f = findings(&bytes, profile, mode);
        assert!(f.is_empty(), "{name}: {f:#?}");
        // the mode is recognised from the layer as the stock levels' are
        let w = LevelWriter::parse(&bytes).unwrap();
        assert_eq!(bc2_levelwrite::placement::detect_mode(&w), Some(mode), "{name}");
    }
}

/// Each L1 rule, its synthetic broken layout and the severity it raises.
fn broken() -> Vec<(&'static str, Vec<u8>, Mode, Profile, Severity)> {
    use Mode::{Conquest as C, Rush as R, SquadDeathmatch as D, SquadRush as S};
    use Profile::{Bfbc2 as B, Ww2 as W};
    use Severity::{Note as N, Warning as Wn};
    let cq = |f: &dyn Fn(&mut Cq)| {
        let mut c = Cq::clean();
        f(&mut c);
        c.bytes()
    };
    let ww2 = |f: &dyn Fn(&mut Cq)| {
        let mut c = Cq::clean();
        // the WW2 tendencies: five flags 150 m apart, HQs 300 m from them, ten seconds to capture
        c.hq = [[0.0, 0.0, -600.0], [0.0, 0.0, 600.0]];
        c.flags = (0..5).map(|i| [0.0, 0.0, -300.0 + 150.0 * i as f32]).collect();
        c.flag_spawns = vec![c.flag_spawns[0].clone(); 5];
        c.area = square(700.0);
        c.time = 10;
        f(&mut c);
        c.bytes()
    };
    let rush = |f: &dyn Fn(&mut Rush)| {
        let mut r = Rush::clean();
        f(&mut r);
        r.bytes()
    };
    let sr = |f: &dyn Fn(&mut Rush)| {
        let mut r = Rush::squad();
        f(&mut r);
        r.bytes()
    };
    let sdm = |f: &dyn Fn(&mut Sdm)| {
        let mut s = Sdm::clean();
        f(&mut s);
        s.bytes()
    };
    vec![
        (
            "L1-CQ-001",
            cq(&|c| {
                c.flags.truncate(2);
            }),
            C,
            B,
            N,
        ),
        (
            "L1-CQ-001w",
            ww2(&|c| {
                c.flags.truncate(4);
            }),
            C,
            W,
            N,
        ),
        ("L1-CQ-002", cq(&|c| c.flags[1] = [0.0, 0.0, -90.0]), C, B, Wn),
        ("L1-CQ-002w", ww2(&|c| c.flags[1] = [0.0, 0.0, -240.0]), C, W, Wn),
        ("L1-CQ-003", cq(&|c| c.hq[0] = [0.0, 0.0, -180.0]), C, B, Wn),
        ("L1-CQ-003w", ww2(&|c| c.hq[0] = [0.0, 0.0, -400.0]), C, W, Wn),
        (
            "L1-CQ-004",
            cq(&|c| {
                c.hq = [[0.0, 0.0, -110.0], [0.0, 0.0, 110.0]];
                c.flags = vec![[-120.0, 0.0, -10.0], [0.0, 0.0, 0.0], [120.0, 0.0, 10.0]];
            }),
            C,
            B,
            Wn,
        ),
        (
            "L1-CQ-005",
            cq(&|c| c.hq = [[0.0, 0.0, -225.0], [0.0, 0.0, 290.0]]),
            C,
            B,
            N,
        ),
        ("L1-CQ-006", cq(&|c| c.flags[2] = [0.0, 0.0, 290.0]), C, B, Wn),
        ("L1-CQ-007", cq(&|c| c.flags[1] = [280.0, 0.0, 0.0]), C, B, N),
        ("L1-CQ-008", cq(&|c| c.radius = 3.0), C, B, Wn),
        ("L1-CQ-009", cq(&|c| c.time = 30), C, B, N),
        ("L1-CQ-009w", ww2(&|c| c.time = 20), C, W, N),
        ("L1-CQ-010", cq(&|c| c.flag_spawns[1].clear()), C, B, Wn),
        (
            "L1-CQ-011",
            cq(&|c| c.flag_spawns[0] = vec![[10.0, 0.0, 10.0], [0.0, 0.0, 250.0]]),
            C,
            B,
            Wn,
        ),
        ("L1-CQ-012", cq(&|c| c.hq_spawns = 2), C, B, N),
        ("L1-CQ-013", cq(&|c| c.flags[1] = [295.0, 0.0, 0.0]), C, B, N),
        (
            "L1-CQ-014",
            cq(&|c| c.hq = [[0.0, 0.0, -320.0], [0.0, 0.0, 250.0]]),
            C,
            B,
            Wn,
        ),
        (
            "L1-CQ-015",
            cq(&|c| {
                c.team_areas = Some([rect(-300.0, -300.0, 300.0, 200.0), rect(-300.0, -280.0, 300.0, 300.0)]);
            }),
            C,
            B,
            Wn,
        ),
        ("L1-CQ-016", cq(&|c| c.tickets = 500), C, B, N),
        ("L1-CQ-017", cq(&|c| c.area = square(600.0)), C, B, N),
        ("L1-AL-001", cq(&|c| c.enemy_radius = 35.0), C, B, N),
        (
            "L1-AL-002",
            cq(&|c| c.flag_spawns[2] = vec![[0.0, 0.0, 10.0], [0.0, 0.0, 200.0]]),
            C,
            B,
            Wn,
        ),
        ("L1-HT-002", cq(&|c| c.flags[2][1] = 40.0), C, B, N),
        ("L1-RU-001", rush(&|r| r.stages = 2), R, B, N),
        ("L1-RU-002", rush(&|r| r.third_mcom = true), R, B, Wn),
        ("L1-RU-003", rush(&|r| r.mcoms = vec![-3.0, 3.0]), R, B, Wn),
        ("L1-RU-004", rush(&|r| r.step = 60.0), R, B, Wn),
        ("L1-RU-004s", sr(&|r| r.step = 30.0), S, B, Wn),
        ("L1-RU-005", rush(&|r| r.def = 150.0), R, B, Wn),
        ("L1-RU-005s", sr(&|r| r.def = 150.0), S, B, Wn),
        ("L1-RU-006", rush(&|r| r.att = -30.0), R, B, Wn),
        ("L1-RU-006s", sr(&|r| r.att = -30.0), S, B, Wn),
        (
            "L1-RU-007",
            rush(&|r| {
                r.att = -60.0;
                r.def = 70.0;
            }),
            R,
            B,
            Wn,
        ),
        (
            "L1-RU-007s",
            sr(&|r| {
                r.att = -55.0;
                r.def = 90.0;
            }),
            S,
            B,
            Wn,
        ),
        ("L1-RU-008", rush(&|r| r.def = -45.0), R, B, Wn),
        ("L1-RU-009", rush(&|r| r.att = 45.0), R, B, Wn),
        (
            "L1-RU-010",
            rush(&|r| {
                r.att = -50.0;
                r.def_back = 80.0;
            }),
            R,
            B,
            Wn,
        ),
        ("L1-RU-011", rush(&|r| r.att_front = -5.0), R, B, Wn),
        ("L1-RU-012", rush(&|r| r.def_back = -5.0), R, B, Wn),
        ("L1-RU-013", rush(&|r| r.att_area_from = Some(260.0)), R, B, N),
        ("L1-RU-014", rush(&|r| r.mcom_dy = 15.0), R, B, N),
        ("L1-RU-015", rush(&|r| r.lives = 30), R, B, N),
        ("L1-SD-001", sdm(&|s| s.squads = 3), D, B, N),
        (
            "L1-SD-002",
            sdm(&|s| {
                s.cols = 3;
                s.rows = 2;
            }),
            D,
            B,
            Wn,
        ),
        ("L1-SD-003", sdm(&|s| s.spacing = 5.0), D, B, N),
        ("L1-SD-004", sdm(&|s| s.area = 250.0), D, B, N),
        ("L1-SD-005", sdm(&|s| s.outside = Some(10.0)), D, B, Wn),
        ("L1-SD-006", sdm(&|s| s.vehicles = 0), D, B, N),
    ]
}

#[test]
fn each_placement_rule_has_a_broken_layout_that_raises_it() {
    let _gate = crate::gate();
    let cases = broken();
    let mut covered = HashSet::new();
    for (id, bytes, mode, profile, severity) in &cases {
        let f = findings(bytes, *profile, *mode);
        assert!(
            f.iter().any(|x| x.code == *id && x.severity == *severity),
            "{id}: wanted a {} of it, got {:?}",
            severity.word(),
            codes(&f)
        );
        assert!(f.iter().all(|x| x.severity != Severity::Error), "{id}: {:?}", codes(&f));
        let x = f.iter().find(|x| x.code == *id).unwrap();
        // each cites its statistic: the stock range, the levels, L1's ID
        assert!(x.message.contains(&format!("[{id}: stock")), "{}", x.message);
        assert!(
            !x.objects.is_empty() || id.ends_with("001") || id.contains("SD-"),
            "{id}: names no object"
        );
        covered.insert(*id);
    }
    for r in rules::all().iter().filter(|r| r.code.starts_with("L1-")) {
        assert!(
            covered.contains(r.code.as_str()),
            "{}: no synthetic broken layout",
            r.code
        );
    }
    println!("{} placement rules each raised by its broken layout", covered.len());
}

// ------------------------------------------------------------------------------------------------ edit-phase rules

fn edit_findings(p: &MapProject, level: Option<&LevelWriter>, base: Option<&LevelWriter>) -> Vec<Finding> {
    validate::validate(Input {
        project: p,
        level,
        base,
        donor: None,
        game: None,
        languages: None,
        base_check: None,
        mode: Some(Mode::Conquest),
        site: None,
        routes: None,
    })
}

fn has(f: &[Finding], code: &str) -> bool {
    f.iter().any(|x| x.code == code)
}

#[test]
fn each_edit_rule_has_a_broken_project_that_raises_it() {
    let _gate = crate::gate();
    let clean = Cq::clean().bytes();
    let base = LevelWriter::parse(&clean).unwrap();
    let ok = |p: &MapProject, w: &LevelWriter| edit_findings(p, Some(w), Some(&base));
    let p0 = project(&clean, Profile::Bfbc2);
    assert!(ok(&p0, &base).is_empty(), "{:?}", codes(&ok(&p0, &base)));
    let mut covered: Vec<&str> = Vec::new();
    let mut expect = |code: &'static str, f: &[Finding]| {
        assert!(has(f, code), "{code}: {:?}", codes(f));
        covered.push(code);
    };
    // XVAL-EDIT-4: the partition does not parse
    expect("XVAL-EDIT-4", &edit_findings(&p0, None, Some(&base)));
    // XVAL-EDIT-5: a paint stroke
    let mut p = p0.clone();
    p.push(
        Op::Stroke {
            target: bc2_levelwrite::files::Target::Paint { layer: "grass".into() },
            size: [64, 64],
            tiles: Vec::new(),
        },
        "Paint",
    );
    expect("XVAL-EDIT-5", &ok(&p, &base));
    // XVAL-MODE-1, -3, -4: Onslaught chosen; a layer missing its flags; the donor changed
    let mut p = p0.clone();
    let choice = |mode: &str, sha1: &str| ModeChoice {
        profile: Profile::Bfbc2,
        mode: mode.into(),
        donor: "mp_donor".into(),
        donor_sha1: sha1.into(),
        items: Vec::new(),
        removed: Vec::new(),
        fit: bc2_levelwrite::modelayer::Fit::default(),
    };
    p.push(
        Op::SetMode {
            to: Box::new(choice("onslaught", "x")),
        },
        "Mode",
    );
    expect("XVAL-MODE-1", &ok(&p, &base));
    let mut p = p0.clone();
    p.push(
        Op::SetMode {
            to: Box::new(choice("conquest", "then")),
        },
        "Mode",
    );
    let mut few = Cq::clean();
    few.flags.clear();
    few.flag_spawns.clear();
    let few_w = LevelWriter::parse(&few.bytes()).unwrap();
    let f = validate::validate(Input {
        project: &p,
        level: Some(&few_w),
        base: Some(&base),
        donor: Some((&base, "now")),
        game: None,
        languages: None,
        base_check: None,
        mode: None,
        site: None,
        routes: None,
    });
    expect("XVAL-MODE-3", &f);
    expect("XVAL-MODE-4", &f);
    // the names: each rule of form
    for (code, n) in [
        (
            "XVAL-NAME-1",
            LevelNames {
                level: "1BAD".into(),
                browser: "Fine".into(),
                ..LevelNames::default()
            },
        ),
        (
            "XVAL-NAME-2",
            LevelNames {
                level: "MY_NAME_MAP".into(),
                browser: "Fine".into(),
                ..LevelNames::default()
            },
        ),
        (
            "XVAL-NAME-4",
            LevelNames {
                level: "GOOD_ID".into(),
                folder: "other".into(),
                browser: "Fine".into(),
                ..LevelNames::default()
            },
        ),
        (
            "XVAL-NAME-5",
            LevelNames {
                level: "GOOD_ID".into(),
                browser: " padded".into(),
                ..LevelNames::default()
            },
        ),
        (
            "XVAL-NAME-7",
            LevelNames {
                level: "GOOD_ID".into(),
                browser: "Fine".into(),
                own: true,
                name: "x".repeat(80),
                ..LevelNames::default()
            },
        ),
    ] {
        let mut p = p0.clone();
        p.push(Op::SetNames { to: Box::new(n) }, "Names");
        expect(code, &ok(&p, &base));
    }
    // the clash rules need the game's names: XVAL-NAME-3 and -6 with a stand-in of what the game has
    let mut taken = bc2_levelwrite::names::Taken::default();
    taken.levels.insert("mp_005".into(), "a level folder".into());
    taken.browser.insert("atacama desert".into(), "MP_005".into());
    for (code, n) in [
        (
            "XVAL-NAME-3",
            LevelNames {
                level: "MP_005".into(),
                browser: "Fine".into(),
                ..LevelNames::default()
            },
        ),
        (
            "XVAL-NAME-6",
            LevelNames {
                level: "GOOD_ID".into(),
                browser: "Atacama Desert".into(),
                ..LevelNames::default()
            },
        ),
    ] {
        let mut p = p0.clone();
        p.push(Op::SetNames { to: Box::new(n) }, "Names");
        let f = validate::validate(Input {
            project: &p,
            level: Some(&base),
            base: Some(&base),
            donor: None,
            game: Some(&taken),
            languages: None,
            base_check: None,
            mode: None,
            site: None,
            routes: None,
        });
        expect(code, &f);
    }
    // objects: beyond +-2048 m; the 40 km/h sign on a level without it; two vehicle spawners on one spot; the DBX
    // check's empty list entry, world object without its physics, object not listed, dangling local reference
    let level_with = |extra: &dyn Fn(&mut Lb)| -> LevelWriter {
        // the clean layout's instances again, then the extra ones: rebuilt from scratch with the same guids
        let mut b = Lb::new();
        let c = Cq::clean();
        for (k, (team, pos)) in [("Team1", c.hq[0]), ("Team2", c.hq[1])].into_iter().enumerate() {
            let pts: Vec<[f32; 3]> = (0..c.hq_spawns)
                .map(|i| {
                    [
                        pos[0] + 8.0 * i as f32 - 20.0,
                        pos[1],
                        pos[2] + if k == 0 { -10.0 } else { 10.0 },
                    ]
                })
                .collect();
            let s = b.spawner(team, &pts, c.enemy_radius);
            b.capture_point(&format!("HQ {team}"), team, true, 0, pos, &[s], 20.0, 0, None);
        }
        extra(&mut b);
        LevelWriter::parse(&b.bytes()).unwrap()
    };
    let plain = level_with(&|_| {});
    let rel = |p: &MapProject, w: &LevelWriter| {
        validate::validate(Input {
            project: p,
            level: Some(w),
            base: Some(&plain),
            donor: None,
            game: None,
            languages: None,
            base_check: None,
            mode: None,
            site: None,
            routes: None,
        })
    };
    let p1 = project(&plain.write().unwrap(), Profile::Bfbc2);
    assert!(rel(&p1, &plain).is_empty(), "{:?}", codes(&rel(&p1, &plain)));
    let prop = "objects/props/crate/crate/BBBBBBBB-0000-0000-0000-000000000002";
    expect(
        "XVAL-CULL-4",
        &rel(
            &p1,
            &level_with(&|b| {
                b.add(
                    "ReferenceObjectData",
                    vec![rf("ReferencedObject", prop), at([3000.0, 0.0, 0.0])],
                    true,
                );
            }),
        ),
    );
    expect(
        "XVAL-SHDR-2",
        &rel(
            &p1,
            &level_with(&|b| {
                b.add(
                    "ReferenceObjectData",
                    vec![
                        rf(
                            "ReferencedObject",
                            "objects/objects/roadsigns/speedsign_40/speedsign_40/BBBBBBBB-0000-0000-0000-000000000003",
                        ),
                        at([10.0, 0.0, 0.0]),
                    ],
                    true,
                );
            }),
        ),
    );
    expect(
        "XVAL-PLACE-1",
        &rel(
            &p1,
            &level_with(&|b| {
                for _ in 0..2 {
                    b.add(
                        "VehicleSpawnEntityData",
                        vec![text("Team", "Team1"), rf("Vehicle", prop), at([5.0, 0.0, 5.0])],
                        true,
                    );
                }
            }),
        ),
    );
    expect("XVAL-OBJ-1", &rel(&p1, &level_with(&|b| b.objects.push("null".into()))));
    expect(
        "XVAL-OBJ-2",
        &rel(
            &p1,
            &level_with(&|b| {
                b.add("ModelGroupEntityData", vec![rf("PhysicsData", "null")], true);
            }),
        ),
    );
    expect(
        "XVAL-OBJ-3",
        &rel(
            &p1,
            &level_with(&|b| {
                b.add("TerrainEntityData", vec![], false);
            }),
        ),
    );
    expect(
        "XVAL-REF-1",
        &rel(
            &p1,
            &level_with(&|b| {
                b.add(
                    "ReferenceObjectData",
                    vec![rf("Blueprint", "DDDDDDDD-0000-0000-0000-000000000009")],
                    true,
                );
            }),
        ),
    );
    // every rule checked live (edit phase) has its broken project here, but the log's own three, which the
    // editor's replay raises (OP1's typed problems; the game tier's v1_stage_rules runs them through `validate::project`)
    for r in rules::all().iter().filter(|r| r.phases.contains(&rules::Phase::Edit)) {
        if ["XVAL-EDIT-1", "XVAL-EDIT-2", "XVAL-EDIT-3"].contains(&r.code.as_str()) {
            continue;
        }
        // the site, vehicle and path rules (rounds A1, A2) read a site and a route graph: tests/auto_layout.rs breaks
        // a layout for each
        if r.code.starts_with("XVAL-PLACE-") && r.code != "XVAL-PLACE-1" {
            continue;
        }
        assert!(
            covered.contains(&r.code.as_str()),
            "{}: no synthetic broken project",
            r.code
        );
    }
    println!("{} edit-phase rules each raised by a broken project", covered.len());
}

// ------------------------------------------------------------------------------------------------ stage rules' pieces

#[test]
fn console_resources_deleted_sound_markers_and_build_refusals_are_named() {
    let _gate = crate::gate();
    use bc2_levelwrite::export::{hides_sound, is_console_resource};
    // XVAL-CONS-1: a big-endian texture header, a Havok packfile with the console's layout rules
    assert!(is_console_resource("ITexture", &[0, 0, 0, 3, 0, 0, 0, 0]));
    assert!(!is_console_resource("ITexture", &[3, 0, 0, 0, 0, 0, 0, 0]));
    let mut hk = vec![0u8; 8];
    hk.extend([0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10]);
    hk.extend([0; 8]);
    let mut console = hk.clone();
    console.extend([4, 0, 0, 1]);
    let mut pc = hk;
    pc.extend([4, 1, 0, 1]);
    assert!(is_console_resource("HavokPhysicsData", &console));
    assert!(!is_console_resource("HavokPhysicsData", &pc));
    // XVAL-SND-2: markers naming sounds or wave data
    assert!(hides_sound("Sound/Weapons/M16/m16_fire.dbx"));
    assert!(hides_sound("levels/x/sound_wavedata_1"));
    assert!(!hides_sound("Objects/Props/crate.dbx"));
    // build refusals carry their codes
    assert!(
        rules::refusal("XVAL-MODE-1", "Onslaught")
            .0
            .starts_with("XVAL-MODE-1: ")
    );
    // every rule's spec citations and message are there; the counts per severity
    let [n, w, e] = rules::counts();
    assert!(e >= 40 && w >= 40 && n >= 2, "{n} {w} {e}");
    println!("registry: {} rules - {e} errors, {w} warnings, {n} notes", n + w + e);
}

#[test]
fn the_registry_and_the_spec_agree() {
    let _gate = crate::gate();
    // every XVAL rule is a rule of spec/transforms/validation.md with its title, and the other way round
    let spec = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../spec/transforms/validation.md"),
    )
    .unwrap();
    let headings: Vec<(String, String)> = spec
        .lines()
        .filter_map(|l| l.strip_prefix("### "))
        .filter_map(|l| l.split_once(": "))
        .filter(|(id, _)| id.starts_with("XVAL-"))
        .map(|(a, b)| (a.to_owned(), b.to_owned()))
        .collect();
    let ours: Vec<(String, String)> = rules::all()
        .iter()
        .filter(|r| r.code.starts_with("XVAL-"))
        .map(|r| (r.code.clone(), r.title.clone()))
        .collect();
    for r in &ours {
        assert!(headings.contains(r), "{r:?} is not a heading of validation.md");
    }
    for h in &headings {
        assert!(ours.contains(h), "{h:?} of validation.md is not in the registry");
    }
}
