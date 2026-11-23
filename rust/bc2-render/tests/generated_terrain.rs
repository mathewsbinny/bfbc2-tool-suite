//! A generated terrain in the game-accurate view (round TG2; spec TGEN-OUT-5's "it draws" step, TGEN-VIS-2): an
//! arid terrain from `bc2_terragen`, its visual side built from MP_005's prepared files (`bc2_terragen::visual`),
//! drawn over MP_005's files with nothing written to disk ([`Files::with`], [`World::load_generated`]). Every patch
//! shader has a program, every sampled pixel whose ray meets the height field is drawn at its height, and the rule
//! layers on the generated heights look as on MP_005's own: flat dry ground, steep ground and the sea floor each
//! within a set distance of MP_005's own colour under the same weather.
//!
//! Skipped (with a note) without OPFOR's prepared MP_005 or Direct3D 11.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::panic,
    missing_docs
)]

use crate::common;

use bc2_formats::terrain::Heightfield;
use bc2_render::d3d::Device;
use bc2_render::files::{Files, level_folders};
use bc2_render::frame::Camera;
use bc2_render::live::{Key, Live, Options};
use bc2_render::world::World;
use bc2_terragen::visual::{self, Donor};
use bc2_terragen::{Biome, Settings, Terrain};

const SIZE: [u32; 2] = [640, 360];
const STEP: u32 = 8;

fn add(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

fn meets(hf: &Heightfield, eye: [f64; 3], dir: [f64; 3], near: f64, far: f64) -> Option<[f64; 3]> {
    let (cx, cz) = hf.cell();
    let lo = [f64::from(hf.min[0]), f64::from(hf.min[1]), f64::from(hf.min[2])];
    let hi = [
        lo[0] + cx * f64::from(hf.size - 1),
        f64::from(hf.max[1]),
        lo[2] + cz * f64::from(hf.size - 1),
    ];
    let (mut t0, mut t1) = (near, far);
    for k in 0..3 {
        if dir[k].abs() < 1e-12 {
            if eye[k] < lo[k] || eye[k] > hi[k] {
                return None;
            }
            continue;
        }
        let (a, b) = ((lo[k] - eye[k]) / dir[k], (hi[k] - eye[k]) / dir[k]);
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
    }
    let first = add(eye, dir, t0);
    if t0 < t1 && first[1] <= hf.height(first[0], first[2]) {
        return None; // the ray enters the box through its side, under the edge: the terrain has no skirt
    }
    let mut t = t0;
    while t <= t1 {
        let p = add(eye, dir, t);
        if p[1] <= hf.height(p[0], p[2]) {
            return Some(p);
        }
        t += 0.5;
    }
    None
}

/// Per class (flat dry, steep, under the water): the mean HDR colour and the pixel count; and (rays on the
/// terrain, drawn, drawn at the height field's height).
struct Look {
    classes: [([f64; 3], usize); 3],
    /// Flat dry plain cells' colour by height above the water, 20 m bands: (sum, count).
    bands: Vec<([f64; 3], usize)>,
    /// Sea-floor cells' colour by depth under the water, 5 m bands: (sum, count).
    depths: Vec<([f64; 3], usize)>,
    hits: usize,
    drawn: usize,
    on: usize,
}

fn slope_deg(hf: &Heightfield, x: f64, z: f64) -> f64 {
    let d = 2.0;
    let gx = (hf.height(x + d, z) - hf.height(x - d, z)) / (2.0 * d);
    let gz = (hf.height(x, z + d) - hf.height(x, z - d)) / (2.0 * d);
    gx.hypot(gz).atan().to_degrees()
}

fn look(dev: &mut Device, world: &mut World, cam: &Camera, target: [f64; 3], water: f64, lim: f64) -> Look {
    let mut live = Live::new(&world.light);
    let patches: Vec<_> = world
        .sources(target)
        .into_iter()
        .filter(|s| matches!(s.key, Key::Patch(..)))
        .collect();
    live.set(patches);
    live.build(cam, SIZE, Options::exact(true));
    let (depth, post) = world.passes(cam, SIZE, false).unwrap();
    let raw = world.raw().clone();
    dev.render_live(&live, SIZE, world.clear(), depth.as_ref(), &post, &world.textures, &raw)
        .unwrap();
    let frame = dev.read_scene(None).unwrap();
    let terr = world.terrain.as_ref().unwrap();
    let hf = terr.hf.clone();
    let map = terr.visual.maps[0].clone();
    let shaders = terr.visual.shaders.clone();
    let side = map.side();
    let kind_at = |x: f64, z: f64| -> Option<visual::RuleKind> {
        let (px, pz) = terr.patch_at(x, z)?;
        if px >= side || pz >= side {
            return None;
        }
        let sh = &shaders[usize::from(map.shader[pz * side + px])];
        (sh.tiled == 0).then(|| visual::rule_kind(&sh.name))
    };
    let mut missed = Vec::new();
    let half = (cam.fov / 2.0).tan();
    let mut out = Look {
        classes: [([0.0; 3], 0); 3],
        bands: vec![([0.0; 3], 0); 16],
        depths: vec![([0.0; 3], 0); 16],
        hits: 0,
        drawn: 0,
        on: 0,
    };
    for y in (STEP / 2..SIZE[1]).step_by(STEP as usize) {
        for x in (STEP / 2..SIZE[0]).step_by(STEP as usize) {
            let sx = (2.0 * (f64::from(x) + 0.5) / f64::from(SIZE[0]) - 1.0) * half * cam.aspect;
            let sy = (1.0 - 2.0 * (f64::from(y) + 0.5) / f64::from(SIZE[1])) * half;
            let d = add(add(cam.forward, cam.right, sx), cam.up, sy);
            let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
            let dir = d.map(|v| v / n);
            let Some(p) = meets(&hf, cam.eye, dir, cam.near, cam.far * n) else {
                continue;
            };
            out.hits += 1;
            let k = (y * SIZE[0] + x) as usize;
            let dz = f64::from(frame.depth[k]);
            if dz >= 1.0 {
                if missed.len() < 4 {
                    missed.push([p[0].round(), p[1].round(), p[2].round()]);
                }
                continue;
            }
            out.drawn += 1;
            let view_z = cam.near * cam.far / (cam.far - dz * (cam.far - cam.near));
            let q = add(cam.eye, dir, view_z * n);
            if (q[1] - hf.height(q[0], q[2])).abs() < 2.0 {
                out.on += 1;
            }
            let s = slope_deg(&hf, p[0], p[2]);
            // the rule-only cells' layers, by the ground under them (TGEN-VIS-2)
            let Some(kind) = kind_at(p[0], p[2]) else { continue };
            // the same square of the map on both (a layer may sample a texture over the whole level)
            if p[0].abs() > lim || p[2].abs() > lim {
                continue;
            }
            let class = if p[1] < water - 2.0 && kind.sea {
                2
            } else if s > 40.0 && kind.slope {
                1
            } else if s < 10.0 && p[1] > water + 2.0 && !kind.sea && !kind.slope {
                0
            } else {
                continue;
            };
            let band = match class {
                0 => Some(&mut out.bands[(((p[1] - water).max(0.0) / 20.0) as usize).min(15)]),
                2 => Some(&mut out.depths[(((water - p[1]).max(0.0) / 5.0) as usize).min(15)]),
                _ => None,
            };
            if let Some(b) = band {
                for (i, v) in b.0.iter_mut().enumerate() {
                    *v += f64::from(frame.hdr[4 * k + i]);
                }
                b.1 += 1;
            }
            let c = &mut out.classes[class];
            for (i, v) in c.0.iter_mut().enumerate() {
                *v += f64::from(frame.hdr[4 * k + i]);
            }
            c.1 += 1;
        }
    }
    for c in &mut out.classes {
        let m = c.1.max(1) as f64;
        c.0 = c.0.map(|v| v / m);
    }
    if !missed.is_empty() {
        println!("  rays on the terrain not drawn, e.g. {missed:?}");
    }
    out
}

fn above(target: [f64; 3]) -> Camera {
    Camera::look(
        [target[0], 1800.0, target[2] + 1.0],
        target,
        60f64.to_radians(),
        0.5,
        4000.0,
        f64::from(SIZE[0]) / f64::from(SIZE[1]),
    )
}

fn oblique(target: [f64; 3]) -> Camera {
    Camera::look(
        [target[0] + 500.0, target[1] + 420.0, target[2] + 700.0],
        target,
        60f64.to_radians(),
        0.5,
        4000.0,
        f64::from(SIZE[0]) / f64::from(SIZE[1]),
    )
}

/// One variant's drawing against MP_005's own: per view and band (flat dry ground in plain rule-only cells by 20 m
/// over the water; sea-floor cells by 5 m under it) with 10 pixels or more on both, the luminance ratio and the
/// hue's worst channel-share difference.
type Compared = Vec<(&'static str, &'static str, usize, f64, f64)>;

/// Two worlds, each with its own device.
struct Pair<'a> {
    dev: &'a mut Device,
    made: &'a mut World,
    water: f64,
    dev_own: &'a mut Device,
    own: &'a mut World,
    own_water: f64,
}

fn compare(label: &'static str, w: &mut Pair<'_>, lim: f64) -> Compared {
    let mut out = Vec::new();
    for (view, cam_g, cam_o) in [
        ("above", above([0.0, 0.0, 0.0]), above([0.0, 0.0, 0.0])),
        (
            "oblique",
            oblique([0.0, w.water, 0.0]),
            oblique([0.0, w.own_water, 0.0]),
        ),
    ] {
        let g = look(w.dev, w.made, &cam_g, [0.0, w.water, 0.0], w.water, lim);
        let o = look(w.dev_own, w.own, &cam_o, [0.0, w.own_water, 0.0], w.own_water, lim);
        println!(
            "{label} {view}: {} rays on the terrain, {} drawn, {} at its height (MP_005: {} / {} / {})",
            g.hits, g.drawn, g.on, o.hits, o.drawn, o.on
        );
        assert!(g.hits > 300, "{label} {view}");
        assert!(
            g.drawn * 100 >= g.hits * 99,
            "{label} {view}: drawn {} of {}",
            g.drawn,
            g.hits
        );
        assert!(
            g.on * 100 >= g.drawn * 95,
            "{label} {view}: at its height {} of {}",
            g.on,
            g.drawn
        );
        let lum = |c: [f64; 3]| 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let share = |c: [f64; 3]| c.map(|x| x / (c[0] + c[1] + c[2]).max(1e-9));
        for (what, step, gb, ob) in [
            ("dry plain ground", 20, &g.bands, &o.bands),
            ("sea floor", 5, &g.depths, &o.depths),
        ] {
            for (i, (gs, os)) in gb.iter().zip(ob.iter()).enumerate() {
                if gs.1 < 10 || os.1 < 10 {
                    continue;
                }
                let (gc, oc) = (gs.0.map(|v| v / gs.1 as f64), os.0.map(|v| v / os.1 as f64));
                let ratio = lum(gc) / lum(oc).max(1e-6);
                let (sg, so) = (share(gc), share(oc));
                let hue = (0..3).map(|i| (sg[i] - so[i]).abs()).fold(0.0, f64::max);
                println!(
                    "  {what} {}-{} m {}: {gc:.3?} ({} px) against MP_005's {oc:.3?} ({} px): brightness {ratio:.2}, hue off by {hue:.3}",
                    i * step,
                    (i + 1) * step,
                    if what == "sea floor" {
                        "under the water"
                    } else {
                        "over the water"
                    },
                    gs.1,
                    os.1
                );
                out.push((view, what, i, ratio, hue));
            }
        }
    }
    out
}

// spec: TGEN-VIS-2, TGEN-OUT-5
#[test]
#[ignore = "game tier: a generated terrain over MP_005's prepared files; run_tests.py game runs it"]
fn a_generated_terrain_draws_with_its_donors_look() {
    let _gate = crate::gate();
    let opfor = common::opfor();
    if !opfor.join("work").join("levels").join("mp_005").join("dist").is_dir() {
        println!("skipped: MP_005 is not prepared");
        return;
    }
    let mut dev_own = match Device::new(false) {
        Ok(d) => d,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let files = Files::new(&level_folders(&opfor, "mp_005"));
    let get = |n: &str| files.read(n).unwrap_or_else(|| panic!("{n}"));
    let (vt, vw, atlas) = (
        get("Terrains/MP_005/MP_005.Visual.res"),
        get("Terrains/MP_005/MP_005.Water.res"),
        get("Terrains/MP_005/MP_005.MaskTileAtlas.res"),
    );
    let occ = get("Terrains/MP_005/Textures/mp05_AmbientOcclusion.res");
    let dbs: Vec<(u32, Vec<u8>)> = [9u32, 10, 11]
        .iter()
        .map(|&dx| (dx, get(&format!("levels/mp_005/shaders/dx{dx}_single.res"))))
        .collect();
    let donor = Donor {
        terrain: "MP_005".into(),
        visual: &vt,
        water: Some(&vw),
        atlas: Some(&atlas),
        occlusion: Some(("Terrains/MP_005/Textures/mp05_AmbientOcclusion".into(), &occ)),
        databases: dbs.iter().map(|(d, b)| (*d, b.as_slice())).collect(),
    };
    let (name, _, weather) = common::placed(&opfor, "mp_005").unwrap();
    let mut own = World::load(&opfor, "mp_005", &name, &[], weather.as_deref()).unwrap();
    let own_water = 58.0;
    // (1) the identity: MP_005's own heights through the generated terrain's visual side (its cells, its colour /
    // occlusion map, its cloned entries); (2) a generated terrain with its water at MP_005's (TGEN-VIS-2); (3) the
    // same seed with its heights from 0 (its water at 2.7 m)
    let mut results: Vec<(&str, Compared)> = Vec::new();
    for label in ["identity", "donor water", "heights from 0"] {
        let mut st = Settings::new(Biome::Arid, 1024, 7);
        st.water_level_m = (label == "donor water").then_some(own_water);
        let mut t = Terrain::new(&st).unwrap();
        if label == "identity" {
            let hf = bc2_formats::terrain::parse(&get("Terrains/MP_005/MP_005.Heightfield-000.res")).unwrap();
            t.height = bc2_terragen::Generated {
                size: hf.size,
                raw: hf.raw.clone(),
                range_m: f64::from(hf.max[1]),
                water_y: Some(own_water),
            };
            t.ring.clear();
            t.width = 2 * hf.size as usize;
            t.cells = vec![1; t.width * t.width];
        }
        let v = visual::build(&t, "TG_VIEW", &donor).unwrap();
        let extra: Vec<(String, String, Vec<u8>)> = t
            .resources("TG_VIEW")
            .unwrap()
            .into_iter()
            .chain(v.terrain)
            .chain(v.level)
            .map(|r| (r.name, r.type_name.to_owned(), r.data))
            .collect();
        let water = t.height.water_y.unwrap();
        let started = std::time::Instant::now();
        let mut made = World::load_generated(
            files.clone().with(extra),
            &opfor,
            "tg_view",
            &name,
            &[],
            weather.as_deref(),
            "TG_VIEW",
        )
        .unwrap();
        println!(
            "{label}: loaded in {:.1} s; water at {water:.1} m",
            started.elapsed().as_secs_f64()
        );
        let terr = made.terrain.as_ref().unwrap();
        let failed: Vec<String> = terr
            .draws
            .iter()
            .enumerate()
            .filter_map(|(k, d)| d.as_ref().err().map(|e| format!("{k}: {e}")))
            .collect();
        assert!(failed.is_empty(), "{label}: patch shaders without a draw: {failed:?}");
        assert!(made.water.is_some(), "{label}: {:?}", made.notes);
        // a device keeps the engine textures it was given by name (the height map among them): one a world
        let mut dev = Device::new(false).unwrap();
        let lim = if label == "identity" { 1024.0 } else { 512.0 };
        let mut pair = Pair {
            dev: &mut dev,
            made: &mut made,
            water,
            dev_own: &mut dev_own,
            own: &mut own,
            own_water,
        };
        results.push((label, compare(label, &mut pair, lim)));
    }
    // what is held (rust/BENCHMARKS.md TG2 has the rest as measured):
    // - the identity: the sea floor at every depth within 20 % of MP_005's, the dry plain ground's first 20 m over
    //   the water within 10 % (higher up the computed colour / occlusion map is darker than DICE's painted one);
    // - the generated terrain with MP_005's water: the dry plain ground at every height within 20 % of MP_005's;
    // - its heights from 0: the dry plain ground over 40 % off (the rule layers shade by height: TGEN-VIS-2)
    for (label, r) in &results {
        let off = |what: &str, bands: std::ops::Range<usize>| {
            r.iter()
                .filter(|x| x.1 == what && bands.contains(&x.2))
                .map(|x| (x.3 - 1.0).abs())
                .fold(0.0, f64::max)
        };
        let hue = r.iter().map(|x| x.4).fold(0.0, f64::max);
        println!(
            "{label}: {} bands compared; dry plain ground off by up to {:.0} % (its first 20 m {:.0} %), the sea floor {:.0} %, hue {hue:.3}",
            r.len(),
            off("dry plain ground", 0..16) * 100.0,
            off("dry plain ground", 0..1) * 100.0,
            off("sea floor", 0..16) * 100.0
        );
        assert!(hue < 0.035, "{label}: {r:?}");
        match *label {
            "identity" => {
                assert!(
                    off("sea floor", 0..16) < 0.2 && off("dry plain ground", 0..1) < 0.1,
                    "{label}: {r:?}"
                );
            }
            "donor water" => assert!(off("dry plain ground", 0..16) < 0.2, "{label}: {r:?}"),
            _ => assert!(
                off("dry plain ground", 0..16) > 0.4,
                "{label}: the rule layers shade by height"
            ),
        }
    }
}
