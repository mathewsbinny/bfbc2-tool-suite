//! Round A1 on synthetic levels (no game data): the site rules (XVAL-PLACE-2 to 4) raised by a layout broken one way
//! each over a synthetic ground - a lake, a rock, a shore - and silent on the clean one; the auto layout moving a
//! layout that stands half in a lake onto the land where the level's art is, with no spawn point in a bad spot, no
//! warning, every statistic inside DICE's ranges, and the same proposal for the same seed.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::assert_is_empty,
    missing_docs
)]

use bc2_formats::terrain::{HeaderWords, Heightfield, Tail};
use bc2_formats::water::WaterMesh;
use bc2_levelwrite::autolayout::{self, Ask};
use bc2_levelwrite::placement::Mode;
use bc2_levelwrite::rules::Severity;
use bc2_levelwrite::site::{Site, Spot};
use bc2_levelwrite::validate;
use bc2_levelwrite::write::LevelWriter;
use bc2_levelwrite::{Op, Profile};

use crate::v1_rules::{Cq, Lb, at, rf, text};

/// A closed box (12 triangles) from `lo` to `hi`.
fn cube(lo: [f64; 3], hi: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
    let v = |i: usize| {
        [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ]
    };
    [
        [0, 1, 3, 2],
        [4, 5, 7, 6],
        [0, 1, 5, 4],
        [2, 3, 7, 6],
        [0, 2, 6, 4],
        [1, 3, 7, 5],
    ]
    .iter()
    .flat_map(|q| [[v(q[0]), v(q[1]), v(q[2])], [v(q[0]), v(q[2]), v(q[3])]])
    .collect()
}

/// A 2,048 m terrain: the west (x below -100) a lake bed 2 m down under water at 3 m, the east land at 4 m rising
/// gently; a town of 400 small buildings (closed boxes) east of the centre; a rock at (60, 0, 0).
fn ground() -> (Heightfield, WaterMesh, Vec<Vec<[[f64; 3]; 3]>>, Vec<([f64; 3], f64)>) {
    let n = 257u32;
    let (lo, hi) = (-1024.0f32, 1024.0f32);
    let cell = f64::from(hi - lo) / f64::from(n);
    let mut raw = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let x = f64::from(lo) + f64::from(i) * cell;
            let z = f64::from(lo) + f64::from(j) * cell;
            let h = if x < -100.0 {
                1.0
            } else {
                4.0 + (x / 400.0) + 0.5 * (z / 300.0).sin()
            };
            raw.push((h / 20.0 * 65535.0).round().clamp(0.0, 65535.0) as u16);
        }
    }
    let hf = Heightfield {
        min: [lo, 0.0, lo],
        max: [hi, 20.0, hi],
        size: n,
        header: HeaderWords::default(),
        raw,
        tail: Tail::Unknown(Vec::new()),
    };
    let water = WaterMesh {
        vertices: vec![
            [-1024.0, 3.0, -1024.0],
            [-100.0, 3.0, -1024.0],
            [-100.0, 3.0, 1024.0],
            [-1024.0, 3.0, 1024.0],
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
    };
    let mut solids = Vec::new();
    let mut objects = Vec::new();
    for a in 0..20 {
        for b in 0..20 {
            let (x, z) = (300.0 + f64::from(a) * 25.0, -250.0 + f64::from(b) * 25.0);
            let g = hf.height(x, z);
            solids.push(cube([x - 3.0, g - 1.0, z - 3.0], [x + 3.0, g + 5.0, z + 3.0]));
            objects.push(([x, g, z], 1.0));
        }
    }
    let g = hf.height(60.0, 0.0);
    solids.push(cube([55.0, g - 2.0, -5.0], [65.0, g + 8.0, 5.0]));
    objects.push(([60.0, g, 0.0], 0.25));
    (hf, water, solids, objects)
}

fn site() -> Site {
    let (hf, water, solids, objects) = ground();
    Site::new(Some(hf), Some(&water), &solids, &objects)
}

/// The clean Conquest layout moved by (dx, dz), its heights on the ground.
fn conquest_at(site: &Site, d: [f32; 2]) -> LevelWriter {
    // DICE-like spawn places around a flag: 30 to 55 m out (DICE's farthest 37-137 m)
    let mut cq = Cq::clean();
    let spawns = vec![
        [40.0, 0.0, 30.0],
        [-45.0, 0.0, 25.0],
        [10.0, 0.0, -50.0],
        [-20.0, 0.0, -35.0],
    ];
    cq.flag_spawns = vec![spawns.clone(), spawns.clone(), spawns];
    let mut w = LevelWriter::parse(&cq.bytes()).unwrap();
    for e in w.d.instances() {
        let Some(mut r) = w.transform_at(e) else { continue };
        r[3][0] += d[0];
        r[3][2] += d[1];
        let g = site.ground(f64::from(r[3][0]), f64::from(r[3][2])).unwrap_or(0.0);
        r[3][1] = g as f32;
        let t = w.d.child(e, b"Transform").unwrap();
        w.d.set_transform(t, &r).unwrap();
    }
    w
}

#[test]
// spec: XVAL-PLACE-2, XVAL-PLACE-3, XVAL-PLACE-4
fn site_rules_raise_on_spawns_in_water_and_rock_vehicles_without_room_and_objects_outside() {
    let _gate = crate::gate();
    let s = site();
    assert!(matches!(s.spot([-500.0, 1.0, 0.0]), Spot::Water(d) if (d - 2.0).abs() < 0.01));
    assert_eq!(s.spot([600.0, s.ground(600.0, 400.0).unwrap(), 400.0]), Spot::Ground);
    assert!(s.inside_solid([60.0, s.ground(60.0, 0.0).unwrap() + 1.0, 0.0]));
    // the clean layout on open land: nothing
    let clean = conquest_at(&s, [0.0, 600.0]);
    let none = validate::site_rules(&clean, None, &s);
    assert!(none.is_empty(), "{none:#?}");
    // a spawn point in the lake, one in the rock, a vehicle spawner in the lake, a tank in a building, a spawn point
    // outside the combat area
    let mut b = Lb::new();
    let g = |x: f64, z: f64| s.ground(x, z).unwrap() as f32;
    let wet = b.spawner("Team1", &[[-500.0, 1.0, 0.0]], 20.0);
    let rock = b.spawner("Team1", &[[60.0, g(60.0, 0.0) + 1.0, 0.0]], 20.0);
    let far = b.spawner("Team1", &[[200.0, g(200.0, 900.0), 900.0]], 20.0);
    let tank_ref = "objects/vehicles/land/m1a2/m1a2/CCCCCCCC-0000-0000-0000-000000000001";
    let boat = b.add(
        "VehicleSpawnEntityData",
        vec![text("Team", "Team1"), rf("Vehicle", tank_ref), at([-500.0, 3.0, 50.0])],
        true,
    );
    let walled = b.add(
        "VehicleSpawnEntityData",
        vec![
            text("Team", "Team1"),
            rf("Vehicle", tank_ref),
            at([300.0, g(300.0, -250.0) + 0.5, -250.0]),
        ],
        true,
    );
    let squeezed = b.add(
        "VehicleSpawnEntityData",
        vec![
            text("Team", "Team1"),
            rf("Vehicle", tank_ref),
            at([304.5, g(304.5, -250.0), -250.0]),
        ],
        true,
    );
    b.capture_point(
        "HQ",
        "Team1",
        true,
        0,
        [0.0, g(0.0, 0.0), 0.0],
        &[
            wet.clone(),
            rock.clone(),
            far.clone(),
            boat.clone(),
            walled.clone(),
            squeezed.clone(),
        ],
        20.0,
        0,
        None,
    );
    b.area("TeamNeutral", false, true, &crate::v1_rules::square(800.0));
    let w = LevelWriter::parse(&b.bytes()).unwrap();
    let f = validate::site_rules(&w, None, &s);
    for x in &f {
        println!("{}", x.line());
    }
    let has = |code: &str, words: &str| f.iter().any(|x| x.code == code && x.message.contains(words));
    assert!(has("XVAL-PLACE-2", "over water 2.0 m deep"), "{f:#?}");
    assert!(has("XVAL-PLACE-2", "m deep inside"));
    assert!(has("XVAL-PLACE-3", "a land vehicle over water"));
    assert!(
        has("XVAL-PLACE-3", "only 1.5 m clear"),
        "a tank beside a building has no room"
    );
    assert!(has("XVAL-PLACE-3", "4.5 m deep inside a static object's collision"));
    assert!(has("XVAL-PLACE-4", "spawn point"));
    assert!(f.iter().all(|x| x.severity == Severity::Warning));
    // each rule is in the registry with its citations
    for code in ["XVAL-PLACE-2", "XVAL-PLACE-3", "XVAL-PLACE-4"] {
        let r = bc2_levelwrite::rules::rule(code).unwrap();
        assert!(!r.spec.is_empty() && r.from == "A1", "{code}");
    }
    // the same objects where the base already has them are DICE's: nothing
    assert!(validate::site_rules(&w, Some(&w), &s).is_empty());
}

#[test]
// spec: XVAL-PLACE-2
fn the_auto_layout_moves_a_layout_out_of_the_lake_onto_the_town() {
    let _gate = crate::gate();
    let s = site();
    // the clean layout centred at the shore: its west half in the lake
    let w = conquest_at(&s, [-100.0, 0.0]);
    let before = validate::site_rules(&w, None, &s);
    assert!(
        before.iter().any(|f| f.code == "XVAL-PLACE-2"),
        "the start stands in the lake"
    );
    let ask = Ask::new(Mode::Conquest, Profile::Bfbc2, 0);
    let t = std::time::Instant::now();
    let p = autolayout::propose(&w, &s, ask).unwrap();
    println!("{} in {:.2} s", p.describe(), t.elapsed().as_secs_f64());
    for f in &p.findings {
        println!("  {}", f.line());
    }
    assert_eq!(p.bad_spots, 0);
    assert_eq!(p.counts().0, 0, "no warning");
    let (inside, all) = p.inside();
    assert_eq!(inside, all, "{:#?}", p.stats);
    assert!(all >= 20);
    // applied: every moved spawn point on land, the flags near the town's art
    let mut out = w.clone();
    let mut proj = crate::v1_rules::project(&w.write().unwrap(), Profile::Bfbc2);
    proj.push(Op::Move { items: p.moves.clone() }, &p.describe());
    for m in &p.moves {
        let e = out.find(&m.guid).unwrap();
        let t = out.d.child(e, b"Transform").unwrap();
        out.d.set_transform(t, &m.to).unwrap();
    }
    assert!(validate::site_rules(&out, None, &s).is_empty());
    let flags: Vec<[f32; 3]> = bc2_levelwrite::mpconvert::capture_points(&out)
        .into_iter()
        .filter(|c| !c.base)
        .map(|c| c.at)
        .collect();
    assert!(flags.iter().all(|f| f[0] > -60.0), "{flags:?}");
    // deterministic: the same seed, the same proposal; the log takes it as one move
    assert_eq!(autolayout::propose(&w, &s, ask).unwrap(), p);
    assert_eq!(proj.operations.len(), 1);
    // against itself, a layout compares at 0 m and an IoU of 1
    let c = autolayout::compare(&out, &out, Mode::Conquest);
    assert!(c.mean_distance < 1e-9 && (c.iou - 1.0).abs() < 1e-9, "{c:?}");
}

#[test]
// spec: XVAL-PLACE-5
fn path_rules_want_spawns_and_objectives_on_the_main_network() {
    let _gate = crate::gate();
    let s = site();
    let w = conquest_at(&s, [0.0, 600.0]);
    // route nodes every 10 m on the ground over the layout: the main network, but an island around flag A
    let flag_a = bc2_levelwrite::mpconvert::capture_points(&w)
        .into_iter()
        .find(|c| c.name == "A")
        .unwrap()
        .at;
    let (mut nodes, mut island) = (Vec::new(), Vec::new());
    for i in -40..=40 {
        for j in -40..=40 {
            let (x, z) = (f64::from(i) * 10.0, 600.0 + f64::from(j) * 10.0);
            let near_a = (x - f64::from(flag_a[0])).hypot(z - f64::from(flag_a[2])) < 70.0;
            nodes.push([x, s.ground(x, z).unwrap(), z]);
            island.push(u32::from(near_a));
        }
    }
    let routes = bc2_levelwrite::site::Routes::new(nodes, island, "a test build");
    let f = validate::path_rules(&w, None, &routes);
    for x in &f {
        println!("{}", x.line());
    }
    assert!(
        f.iter()
            .all(|x| x.code == "XVAL-PLACE-5" && x.severity == Severity::Warning)
    );
    assert!(
        f.iter()
            .any(|x| x.message.contains("The flag") && x.message.contains("island 1")),
        "{f:#?}"
    );
    assert!(
        f.iter()
            .any(|x| x.message.contains("The spawn point") && x.message.contains("island 1"))
    );
    // only what stands by flag A: the HQs, the other flags and their spawns are on the main network
    for x in &f {
        let p = w.transform(&x.objects[0]).unwrap()[3];
        let d = f64::from(p[0] - flag_a[0]).hypot(f64::from(p[2] - flag_a[2]));
        assert!(d < 70.0, "{} is {d:.0} m from flag A", x.objects[0]);
    }
    // no routes near: every spawn and objective is reported
    let far = bc2_levelwrite::site::Routes::new(vec![[5000.0, 0.0, 5000.0]], vec![0], "far");
    let all = validate::path_rules(&w, None, &far);
    assert!(all.iter().all(|x| x.message.contains("no route node within 15 m")));
    assert!(all.len() >= 3 * 4 + 3, "{}", all.len());
    // what the base already has where it has it is DICE's
    assert!(validate::path_rules(&w, Some(&w), &far).is_empty());
}
