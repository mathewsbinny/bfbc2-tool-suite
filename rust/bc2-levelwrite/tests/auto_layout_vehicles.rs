//! The auto layout's vehicles, pins and proposals, and the export's path-data step, on synthetic levels (round A2; no
//! game data): the vehicle rules (XVAL-PLACE-6 to 8) raised by a tank shut in a courtyard, a boat in a pond, a
//! helicopter under a roof and a plane in a courtyard, and silent on the same vehicles in the open; the auto layout
//! placing a Conquest layout's vehicles by team and class from the level's own spawners (moved, then copies), each
//! base with DICE's count, committed as one operation the log replays; several distinct proposals; pinned objects
//! kept where they stand; the path-data step's sources, refusals, staging and check lines.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::assert_is_empty,
    missing_docs
)]

use std::collections::BTreeSet;

use bc2_formats::hpd::{self, HpdFile, Node, Sector};
use bc2_formats::water::WaterMesh;
use bc2_levelwrite::autolayout::{self, Ask};
use bc2_levelwrite::placement::Mode;
use bc2_levelwrite::site::Site;
use bc2_levelwrite::write::LevelWriter;
use bc2_levelwrite::{Op, Profile, Schema, gameplay, pathdata, validate, vehicles};

use crate::auto_layout::{cube, ground};
use crate::export_rules::{Cq, at, boolean, rf, text};

const TANK: &str = "objects/vehicles/land/m1a2/m1a2/CCCCCCCC-0000-0000-0000-000000000001";
const QUAD: &str = "objects/vehicles/land/quadbike/quadbike/CCCCCCCC-0000-0000-0000-000000000002";
const IFV: &str = "objects/vehicles/land/bmd3/bmd3/CCCCCCCC-0000-0000-0000-000000000003";
const HELI: &str = "objects/vehicles/air/ah64/ah64/CCCCCCCC-0000-0000-0000-000000000004";
const BOAT: &str = "objects/vehicles/sea/pbl/pbl/CCCCCCCC-0000-0000-0000-000000000005";
const PLANE: &str = "objects/vehicles/air/us_corsair/us_corsair/CCCCCCCC-0000-0000-0000-000000000006";

/// Four 1 m walls, 4 m tall, round a 16 m courtyard centred at (x, z) on the ground at height g.
fn courtyard(x: f64, z: f64, g: f64) -> Vec<Vec<[[f64; 3]; 3]>> {
    vec![
        cube([x - 9.0, g - 1.0, z - 9.0], [x + 9.0, g + 4.0, z - 8.0]),
        cube([x - 9.0, g - 1.0, z + 8.0], [x + 9.0, g + 4.0, z + 9.0]),
        cube([x - 9.0, g - 1.0, z - 8.0], [x - 8.0, g + 4.0, z + 8.0]),
        cube([x + 8.0, g - 1.0, z - 8.0], [x + 9.0, g + 4.0, z + 8.0]),
    ]
}

/// The synthetic ground of `auto_layout.rs` with traps north of the town: two courtyards (650, 700) and (650, 800), a
/// roof sheet 8 m up over (750, 700), a 10 m pond 1 m deep at (850, 700).
fn trap_site() -> Site {
    let (hf, water, mut solids, objects) = ground();
    let g = |x: f64, z: f64| hf.height(x, z);
    solids.extend(courtyard(650.0, 700.0, g(650.0, 700.0)));
    solids.extend(courtyard(650.0, 800.0, g(650.0, 800.0)));
    let r = g(750.0, 700.0) + 8.0;
    solids.push(vec![
        [[745.0, r, 695.0], [755.0, r, 695.0], [755.0, r, 705.0]],
        [[745.0, r, 695.0], [755.0, r, 705.0], [745.0, r, 705.0]],
    ]);
    let w = (g(850.0, 700.0) + 1.0) as f32;
    let mut vertices = water.vertices.clone();
    let n = vertices.len() as u32;
    vertices.extend([
        [845.0, w, 695.0],
        [855.0, w, 695.0],
        [855.0, w, 705.0],
        [845.0, w, 705.0],
    ]);
    let mut triangles = water.triangles.clone();
    triangles.extend([[n, n + 1, n + 2], [n, n + 2, n + 3]]);
    let water = WaterMesh { vertices, triangles };
    Site::new(Some(hf), Some(&water), &solids, &objects)
}

fn vehicle(b: &mut crate::export_rules::Lb, team: &str, r: &str, p: [f32; 3]) -> String {
    b.add(
        "VehicleSpawnEntityData",
        vec![text("Team", team), rf("Vehicle", r), boolean("AutoSpawn", true), at(p)],
        true,
    )
}

#[test]
// spec: XVAL-PLACE-6, XVAL-PLACE-7, XVAL-PLACE-8
fn vehicle_rules_raise_on_shut_in_vehicles_and_not_in_the_open() {
    let _gate = crate::gate();
    let s = trap_site();
    let g = |x: f64, z: f64| s.ground(x, z).unwrap() as f32;
    let mut b = Cq::clean().builder();
    let tank = vehicle(&mut b, "Team1", TANK, [650.0, g(650.0, 700.0), 700.0]);
    let plane = vehicle(&mut b, "Team1", PLANE, [650.0, g(650.0, 800.0) + 1.5, 800.0]);
    let heli = vehicle(&mut b, "Team1", HELI, [750.0, g(750.0, 700.0), 700.0]);
    let boat = vehicle(&mut b, "Team1", BOAT, [850.0, g(850.0, 700.0) + 1.3, 700.0]);
    // the same in the open: the tank, the helicopter and the plane east of the traps, the boat in the lake
    let open: Vec<String> = vec![
        vehicle(&mut b, "Team2", TANK, [700.0, g(700.0, 900.0), 900.0]),
        vehicle(&mut b, "Team2", HELI, [760.0, g(760.0, 900.0), 900.0]),
        vehicle(&mut b, "Team2", PLANE, [850.0, g(850.0, 900.0) + 1.5, 900.0]),
        vehicle(&mut b, "Team2", BOAT, [-400.0, 3.3, 0.0]),
    ];
    b.area("TeamNeutral", false, true, &crate::export_rules::square(1000.0));
    let w = LevelWriter::parse(&b.bytes()).unwrap();
    let f = validate::site_rules(&w, None, &s);
    for x in &f {
        println!("{}", x.line());
    }
    let named = |code: &str, g: &str| {
        f.iter()
            .any(|x| x.code == code && x.objects.first() == Some(&g.to_owned()))
    };
    assert!(named("XVAL-PLACE-6", &tank), "the tank in the courtyard: {f:#?}");
    assert!(named("XVAL-PLACE-7", &boat), "the boat in the pond");
    assert!(named("XVAL-PLACE-8", &heli), "the helicopter under the roof");
    assert!(named("XVAL-PLACE-8", &plane), "the plane in the courtyard");
    for g in &open {
        assert!(!f.iter().any(|x| x.objects.contains(g)), "{g} in the open: {f:#?}");
    }
    assert_eq!(f.len(), 4, "{f:#?}");
    // the measures behind them
    let p = |x: f32, z: f32| [f64::from(x), f64::from(g(f64::from(x), f64::from(z))), f64::from(z)];
    assert!(s.way_out(p(650.0, 700.0), 40.0) < 10.0);
    assert!(s.way_out(p(700.0, 900.0), 40.0) >= 40.0);
    assert!(s.open_sky(p(750.0, 700.0), vehicles::ROTOR, 60.0) < 9.0);
    assert!(s.runway(p(850.0, 900.0), vehicles::HALF_SPAN, 300.0).0 >= 100.0);
    // each rule in the registry with its citations; DICE's own (the base's) are left out
    for code in ["XVAL-PLACE-6", "XVAL-PLACE-7", "XVAL-PLACE-8"] {
        let r = bc2_levelwrite::rules::rule(code).unwrap();
        assert!(!r.spec.is_empty() && r.from == "A2", "{code}");
    }
    assert!(validate::site_rules(&w, Some(&w), &s).is_empty());
}

/// A Conquest layout standing half in the lake with the level's own vehicle spawners, unowned, north of the town:
/// each team's tank and quad bike, a neutral IFV.
fn layout_with_vehicles(s: &Site) -> (LevelWriter, Vec<String>) {
    // DICE-like spawn places around a flag (as `auto_layout.rs`'s layout)
    let mut cq = Cq::clean();
    let spawns = vec![
        [40.0, 0.0, 30.0],
        [-45.0, 0.0, 25.0],
        [10.0, 0.0, -50.0],
        [-20.0, 0.0, -35.0],
    ];
    cq.flag_spawns = vec![spawns.clone(), spawns.clone(), spawns];
    let mut b = cq.builder();
    let g = |x: f64, z: f64| s.ground(x, z).unwrap() as f32;
    let own = vec![
        vehicle(&mut b, "Team1", TANK, [600.0, g(600.0, 950.0), 950.0]),
        vehicle(&mut b, "Team1", QUAD, [620.0, g(620.0, 950.0), 950.0]),
        vehicle(&mut b, "Team2", TANK, [640.0, g(640.0, 950.0), 950.0]),
        vehicle(&mut b, "Team2", QUAD, [660.0, g(660.0, 950.0), 950.0]),
        vehicle(&mut b, "TeamNeutral", IFV, [680.0, g(680.0, 950.0), 950.0]),
    ];
    b.area("TeamNeutral", false, true, &crate::export_rules::square(300.0));
    let mut w = LevelWriter::parse(&b.bytes()).unwrap();
    // the layout (not the vehicles) moved to the shore, heights on the ground
    for e in w.d.instances() {
        let guid = String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase();
        if own.contains(&guid) {
            continue;
        }
        let Some(mut r) = w.transform_at(e) else { continue };
        r[3][0] -= 100.0;
        r[3][1] = s.ground(f64::from(r[3][0]), f64::from(r[3][2])).unwrap_or(0.0) as f32;
        let t = w.d.child(e, b"Transform").unwrap();
        w.d.set_transform(t, &r).unwrap();
    }
    (w, own)
}

#[test]
// spec: XLAY-AUTO-2, XLAY-AUTO-3, GAME-VEH-2
fn the_auto_layout_places_vehicles_proposes_several_and_keeps_pins() {
    let _gate = crate::gate();
    let s = crate::auto_layout::site();
    let (w, own) = layout_with_vehicles(&s);
    let ask = Ask::new(Mode::Conquest, Profile::Bfbc2, 0);
    let t = std::time::Instant::now();
    let ps = autolayout::propose_all(&w, &s, None, ask, &BTreeSet::new(), 3).unwrap();
    println!("{} proposals in {:.2} s", ps.len(), t.elapsed().as_secs_f64());
    assert_eq!(ps.len(), 3);
    for (k, p) in ps.iter().enumerate() {
        println!("  {k}: {}", p.describe());
        for n in &p.vehicles.notes {
            println!("     {n}");
        }
    }
    // distinct: another place or turn each
    for (a, b) in [(0, 1), (0, 2), (1, 2)] {
        let (x, y) = (&ps[a], &ps[b]);
        let d = (x.centre[0] - y.centre[0]).hypot(x.centre[1] - y.centre[1]);
        let turn = (x.turn - y.turn).abs().min(360.0 - (x.turn - y.turn).abs());
        assert!(d > 1.0 || turn > 1.0, "{a} and {b} are one proposal");
    }
    let p = &ps[0];
    assert_eq!(p.counts().0, 0, "{:#?}", p.findings);
    let (inside, all) = p.inside();
    assert_eq!(inside, all, "{:#?}", p.stats);
    // DICE's counts: each HQ 4 of its team's vehicles, each flag a neutral IFV pair; the level's own moved first
    let v = &p.vehicles;
    let cps: Vec<(String, bc2_levelwrite::mpconvert::CapturePoint)> =
        w.d.of_type(b".CapturePointEntityData")
            .into_iter()
            .map(|e| String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap()).to_ascii_uppercase())
            .zip(bc2_levelwrite::mpconvert::capture_points(&w))
            .collect();
    let per = |cp: &str| v.vehicles.iter().filter(|x| x.owner == cp).count();
    for (g, c) in &cps {
        assert_eq!(per(g), if c.base { 4 } else { 2 }, "{}: {:#?}", c.name, v.vehicles);
    }
    assert_eq!(
        v.moves.len(),
        own.len(),
        "every one of the level's own spawners used once"
    );
    assert_eq!(v.vehicles.len(), 4 + 4 + 3 * 2);
    assert!(v.vehicles.iter().all(|x| x.distance <= 150.0));
    assert!(
        p.stats
            .iter()
            .any(|x| x.param == "hq_vehicle_pool_150m" && x.values == vec![4.0, 4.0])
    );
    // one operation of the log: the copies placed before their links name them; the log replays
    let mut proj = crate::export_rules::project(&w.write().unwrap(), Profile::Bfbc2);
    proj.push(p.op(), &p.describe());
    let base = w.write().unwrap();
    assert!(proj.base_report(&base).missing.is_empty());
    let applied = bc2_levelwrite::write::apply(&base, &proj.operations, &Schema::default()).unwrap();
    let out = LevelWriter::parse(&applied.bytes).unwrap();
    for x in &v.vehicles {
        assert!(
            gameplay::spawners(&out, &x.owner).contains(&x.guid),
            "{} joins {}",
            x.guid,
            x.owner
        );
    }
    assert!(validate::site_rules(&out, Some(&w), &s).is_empty());
    // deterministic
    assert_eq!(
        autolayout::propose_all(&w, &s, None, ask, &BTreeSet::new(), 3).unwrap(),
        ps
    );
    // pins: flag B and team 1's tank stay where they stand, the rest around them
    let flag_b = cps.iter().find(|(_, c)| c.name == "B").unwrap().0.clone();
    let pins: BTreeSet<String> = [flag_b.clone(), own[0].clone()].into_iter().collect();
    let q = &autolayout::propose_all(&w, &s, None, ask, &pins, 1).unwrap()[0];
    println!("pinned: {}", q.describe());
    assert_eq!(q.pinned, 2);
    assert!(!q.moves.iter().any(|m| m.guid == flag_b), "the pinned flag stays");
    assert!(
        !q.vehicles.moves.iter().any(|m| m.guid == own[0]),
        "the pinned tank stays"
    );
    assert_eq!(q.counts().0, 0, "{:#?}", q.findings);
}

/// A path-data file: sector 0x0101 with four nodes in a line (the main network), sector 0x0102 with two of their own
/// (an island), its nodes in `order`.
fn hpd_file(order: [u32; 2]) -> Vec<u8> {
    let node = |id: u32, x: f32| Node {
        id,
        pos: [x, 5.0, 0.0],
        flags: 0x8000_0000,
        value: 0.0,
        a: 0,
        b: 0,
        c: 0,
    };
    let edge = |s: u32, d: u32| hpd::Edge {
        tag: 0x8000_0000 | 0xA0,
        src: s,
        dst: d,
    };
    let mesh = b"KynogonRuntimeMesh\0\0";
    let file = HpdFile {
        header: [0; 36],
        sectors: vec![
            Sector {
                id: 0x0101,
                bounds: [0.0; 6],
                reserved: 0,
                date: 100_007,
                time: 0,
                nodes: (1..=4).map(|i| node(i, i as f32 * 10.0)).collect(),
                edges: vec![edge(0, 1), edge(1, 2), edge(2, 3)],
                mesh,
            },
            Sector {
                id: 0x0102,
                bounds: [0.0; 6],
                reserved: 0,
                date: 100_007,
                time: 0,
                nodes: order.iter().map(|&i| node(i, 500.0 + i as f32)).collect(),
                edges: vec![edge(0, 1)],
                mesh,
            },
        ],
    };
    hpd::write(&file).unwrap()
}

#[test]
// spec: XVAL-AI-1, XLAY-PATH-1
fn the_path_data_step_finds_refuses_stages_and_checks() {
    let _gate = crate::gate();
    let tmp = std::env::temp_dir().join(format!("bc2_a2_pathdata_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let (opfor, stage) = (tmp.join("opfor"), tmp.join("stage"));
    // the route graph: 6 nodes, the line the main network, the pair an island
    let sorted = hpd_file([10, 11]);
    let routes = pathdata::routes_of(&hpd::parse(&sorted).unwrap(), "a test file");
    assert_eq!(routes.len(), 6);
    assert_eq!(routes.reach([20.0, 4.05, 0.0]).map(|x| x.0), Some(0));
    assert_eq!(routes.reach([510.0, 4.05, 0.0]).map(|x| x.0), Some(1));
    // the sources: NavKit's in-game build before the offline sampler's
    assert!(pathdata::sources(&opfor, "v1test").is_empty());
    for d in ["navkit_offline", "navkit"] {
        let f = opfor.join("work").join(d).join("v1test").join("pathdata.hpd");
        std::fs::create_dir_all(f.parent().unwrap()).unwrap();
        std::fs::write(&f, &sorted).unwrap();
    }
    let found = pathdata::sources(&opfor, "V1TEST");
    assert_eq!(found.len(), 2);
    assert!(found[0].kind.contains("in-game"), "{found:?}");
    // a project that moves only gameplay is staged; one that moves a static object is refused in words
    let mut b = Cq::clean().builder();
    let crate_obj = b.add(
        "ReferenceObjectData",
        vec![
            rf(
                "ReferencedObject",
                "objects/props/crate/crate/CCCCCCCC-0000-0000-0000-000000000009",
            ),
            at([30.0, 0.0, 30.0]),
        ],
        true,
    );
    let base_bytes = b.bytes();
    let base = LevelWriter::parse(&base_bytes).unwrap();
    let flag = bc2_levelwrite::mpconvert::capture_points(&base)
        .iter()
        .position(|c| c.name == "A")
        .unwrap();
    let flag_guid: String = base
        .d
        .of_type(b".CapturePointEntityData")
        .into_iter()
        .map(|e| String::from_utf8_lossy(base.d.attr(e, b"guid").unwrap()).to_ascii_uppercase())
        .nth(flag)
        .unwrap();
    let mv = |g: &str| {
        let from = base.transform(g).unwrap();
        let mut to = from;
        to[3][0] += 5.0;
        Op::Move {
            items: vec![bc2_levelwrite::ops::Moved {
                guid: g.to_owned(),
                from,
                to,
                placement: None,
            }],
        }
    };
    let mut ok = crate::export_rules::project(&base_bytes, Profile::Bfbc2);
    ok.push(mv(&flag_guid), "");
    assert!(pathdata::blockers(&ok, &base, &base).is_empty());
    let mut bad = crate::export_rules::project(&base_bytes, Profile::Bfbc2);
    bad.push(mv(&crate_obj), "");
    let why = pathdata::blockers(&bad, &base, &base);
    assert!(why.len() == 1 && why[0].contains("collision changed"), "{why:?}");
    let refused = pathdata::stage(&opfor, &bad, &base, &base, &stage, "V1TEST_A2").unwrap_err();
    assert!(refused[0].contains("static object"), "{refused:?}");
    // staged: the file, its sha1 and install path; the check passes on it and names an unsorted sector
    let rec = pathdata::stage(&opfor, &ok, &base, &base, &stage, "V1TEST_A2").unwrap();
    assert_eq!(rec.files.len(), 1);
    assert_eq!(
        rec.files[0].install,
        "Output/win32/ai/levels/v1test_a2/infantry/pathdata.hpd"
    );
    assert!(rec.source.contains("in-game"));
    let lines = pathdata::check(&stage, &rec, None);
    assert!(lines.iter().all(|l| l.1), "{lines:?}");
    // the staged level's objectives far from every route node: XVAL-PLACE-5 on the stage fails
    let lines = pathdata::check(&stage, &rec, Some(&base));
    assert!(lines.iter().any(|l| l.0 == "XVAL-PLACE-5" && !l.1), "{lines:?}");
    std::fs::write(stage.join(&rec.files[0].path), hpd_file([11, 10])).unwrap();
    let mut unsorted = rec.clone();
    unsorted.files[0].sha1 = bc2_levelwrite::project::sha1_hex(&hpd_file([11, 10]));
    let lines = pathdata::check(&stage, &unsorted, None);
    assert!(
        lines.iter().any(|l| l.0 == "XVAL-AI-1" && !l.1 && l.2.contains("0102")),
        "{lines:?}"
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
