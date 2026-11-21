//! The game-accurate view opens OPFOR's WW2 levels (round GW1): Guadalcanal and Iwo Jima (by default; any list with
//! `BC2_GW1_LEVELS`, e.g. `guadal_canal,iwo_jima_s,coral_sea,wake_island_s`) opened as the editor opens them, the
//! view switched to "Game-accurate", the level's own weather state found through its weather entity's references,
//! and the whole terrain drawn (bc2-render's `tests/whole_terrain.rs`, round RV1: every sampled pixel whose ray meets
//! the height field is drawn by the terrain patches, at the height field's height).
//!
//! Until GW1 the view failed on these levels: "no WeatherSettings / WeatherSystem partition under
//! weather/wake_island/ ...". Their weather entity (`weather/wake_island/guadal_canal_e`) names the settings
//! partition after the level (`weather/wake_island/guadal_canal`), not `weathersettings*`, which the view looked for
//! by name in a folder it guessed.
//!
//! The levels live only in the game copy's `game\Package`; the editor prepares them into its `work\editor\levels`.
//! So that nothing is written to OPFOR's `work\`, the test makes a stand-in OPFOR folder in temp whose `game` is a
//! junction to the game copy (removed again at the end, the junction first), or uses `BC2_GW1_OPFOR` (a stand-in
//! kept between runs: the levels stay prepared). `BC2_EDITOR_SHOTS=<folder>` saves each level's view
//! in the dark and the light theme.
//!
//! Skipped (with a note) without the game copy's WW2 levels, a graphics adapter or Direct3D 11.
#![cfg(windows)]
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
    clippy::redundant_closure_for_method_calls,
    clippy::panic,
    missing_docs
)]

use std::path::{Path, PathBuf};

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::theme::ThemeId;
use bc2_formats::terrain::Heightfield;
use bc2_render::d3d::Device;
use bc2_render::frame;
use bc2_render::live::{Key, Live, Options};
use bc2_render::world::World;

const SIZE: [u32; 2] = [640, 360];
/// The coverage cameras' near plane: the maps are 2-4 km across, seen from up to 4 km away; 5 m keeps the depth
/// buffer's step there well under the 2 m the test allows (whole_terrain.rs's 0.5 m does at 1.7 km).
const NEAR: f64 = 5.0;
/// Every `STEP`th pixel's ray is marched.
const STEP: u32 = 10;

/// Each level and the weather state its settings make active (the WW2 levels share `weathersystem_wake`'s five
/// states: sunny_day, iwo_jima, guadal_canal, default, coral_sea).
const STATES: [(&str, &str); 4] = [
    ("guadal_canal", "weather/wake_island/states/guadal_canal"),
    ("iwo_jima_s", "weather/wake_island/states/iwo_jima"),
    // Coral Sea uses Wake's settings (ActiveState 0); its spawn transition to state 4 is only noted (RNDR-WTHR-6)
    ("coral_sea", "weather/wake_island/states/sunny_day"),
    ("wake_island_s", "weather/wake_island/states/sunny_day"),
];

fn add(a: [f64; 3], b: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] + b[0] * s, a[1] + b[1] * s, a[2] + b[2] * s]
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// Where the ray from `eye` along `dir` first meets the height field (inside its square, before `far`).
fn meets(hf: &Heightfield, eye: [f64; 3], dir: [f64; 3], near: f64, far: f64) -> Option<[f64; 3]> {
    let (lo, hi) = bounds(hf);
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
    // a ray that enters the box through a side below the terrain's edge meets the edge's wall, which no patch draws
    // (the corner camera stands outside the map)
    let first = add(eye, dir, t0);
    if t0 <= t1 && first[1] <= hf.height(first[0], first[2]) {
        return None;
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

/// The height field's box.
fn bounds(hf: &Heightfield) -> ([f64; 3], [f64; 3]) {
    let (cx, cz) = hf.cell();
    // a metre below the lowest height: a sea floor at the minimum is met inside the box
    let lo = [f64::from(hf.min[0]), f64::from(hf.min[1]) - 1.0, f64::from(hf.min[2])];
    let hi = [
        lo[0] + cx * f64::from(hf.size - 1),
        f64::from(hf.max[1]),
        lo[2] + cz * f64::from(hf.size - 1),
    ];
    (lo, hi)
}

/// Of the sampled pixels whose ray meets the terrain: how many, how many the terrain patches drew, and how many of
/// those lie within 2 m of the height field's surface (whole_terrain.rs's measure).
fn coverage(dev: &mut Device, world: &mut World, cam: &frame::Camera, target: [f64; 3]) -> (usize, usize, usize) {
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
    let hf = &world.terrain.as_ref().unwrap().hf;
    let half = (cam.fov / 2.0).tan();
    let (mut hits, mut drawn, mut on) = (0, 0, 0);
    for y in (STEP / 2..SIZE[1]).step_by(STEP as usize) {
        for x in (STEP / 2..SIZE[0]).step_by(STEP as usize) {
            let sx = (2.0 * (f64::from(x) + 0.5) / f64::from(SIZE[0]) - 1.0) * half * cam.aspect;
            let sy = (1.0 - 2.0 * (f64::from(y) + 0.5) / f64::from(SIZE[1])) * half;
            let d = add(add(cam.forward, cam.right, sx), cam.up, sy);
            let n = dist(d, [0.0; 3]);
            let dir = d.map(|v| v / n);
            if meets(hf, cam.eye, dir, cam.near, cam.far * n).is_some() {
                hits += 1;
                let dz = f64::from(frame.depth[(y * SIZE[0] + x) as usize]);
                if dz < 1.0 {
                    drawn += 1;
                    let view_z = cam.near * cam.far / (cam.far - dz * (cam.far - cam.near));
                    let p = add(cam.eye, dir, view_z * n);
                    if (p[1] - hf.height(p[0], p[2])).abs() < 2.0 {
                        on += 1;
                    }
                }
            }
        }
    }
    (hits, drawn, on)
}

/// A stand-in OPFOR folder: `game` a junction to the game copy, an empty `work`.
struct StandIn {
    root: PathBuf,
    made: bool,
}

impl StandIn {
    fn new(game: &Path) -> Option<Self> {
        if let Some(r) = std::env::var_os("BC2_GW1_OPFOR").map(PathBuf::from) {
            return Some(Self { root: r, made: false });
        }
        let root = std::env::temp_dir().join(format!("bc2_gw1_{}", std::process::id()));
        std::fs::create_dir_all(root.join("work")).ok()?;
        let ok = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(root.join("game"))
            .arg(game)
            .output()
            .ok()?
            .status
            .success();
        ok.then_some(Self { root, made: true })
    }
}

impl Drop for StandIn {
    fn drop(&mut self) {
        if self.made {
            // the junction first (never through it), then the prepared copies
            let _ = std::process::Command::new("cmd")
                .args(["/c", "rmdir"])
                .arg(self.root.join("game"))
                .output();
            if !self.root.join("game").exists() {
                let _ = std::fs::remove_dir_all(&self.root);
            }
        }
    }
}

#[test]
#[ignore = "game tier: prepares and draws WW2 levels; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn ww2_levels_open_in_the_game_accurate_view() {
    let _gate = crate::gate();
    let Ok(opfor) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let game = opfor.join("game");
    let wanted = std::env::var("BC2_GW1_LEVELS").unwrap_or_else(|_| "guadal_canal,iwo_jima_s".into());
    let lvs: Vec<&str> = wanted
        .split(',')
        .filter(|lv| {
            let there = game
                .join("Package")
                .join("levels")
                .join(lv)
                .join("level-00.fbrb")
                .is_file();
            if !there {
                println!("{lv}: skipped, not in the game copy's Package");
            }
            there
        })
        .collect();
    if lvs.is_empty() {
        return;
    }
    let mut h = match Headless::new([1600, 950]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    if let Err(e) = Device::new(false) {
        println!("skipped: {e}");
        return;
    }
    let stand_in = StandIn::new(&game).expect("a stand-in OPFOR folder with a junction to the game copy");
    // the editor falls back to OPFOR itself when the folder it is given has no `game`: the levels would then be
    // prepared into OPFOR's own work\editor\levels (it happened once, with a stand-in that had gone)
    let used = levels::find_opfor(Some(&stand_in.root)).unwrap();
    assert_eq!(
        used,
        std::path::absolute(&stand_in.root).unwrap(),
        "the stand-in {} has no game folder: refusing to prepare into {}",
        stand_in.root.display(),
        used.display()
    );
    let shots = std::env::var_os("BC2_EDITOR_SHOTS").map(PathBuf::from);
    for lv in lvs {
        let mut app = EditorApp::new(Some(&stand_in.root), None).unwrap();
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        let t = std::time::Instant::now();
        app.open_level_blocking(&gpu, lv).unwrap();
        println!(
            "{lv}: opened (prepared if needed) in {:.1} s",
            t.elapsed().as_secs_f64()
        );
        // the map from its south-west, high enough to see most of it
        let target = {
            let s = app.view().unwrap().scene();
            let by = |c: &str| s.entities.iter().find(|e| e.category == c).and_then(|e| e.position());
            by("capture point")
                .or_else(|| by("objective"))
                .or_else(|| s.entities.iter().find_map(|e| e.position()))
                .unwrap()
        };
        {
            let v = app.view_mut().unwrap();
            v.labels = false;
            v.viewports.get_mut(&Projection::Perspective).unwrap().camera = Camera {
                target,
                yaw: 0.7,
                pitch: 0.45,
                distance: 600.0,
                fov: 60.0,
                projection: Projection::Perspective,
            };
        }
        h.settle(&mut app, 2);
        let ctx = h.ctx.clone();
        app.run_action(&gpu, &ctx, Action::ToggleGameView);
        let message = app.view().unwrap().message.clone();
        let Some(g) = app.view().unwrap().game_view.game.as_ref() else {
            panic!("{lv}: the game-accurate view did not load: {message}");
        };
        println!("{lv}: {message}");
        for n in &g.notes {
            println!("  note: {n}");
        }
        // the level's own weather state, through its weather entity's references
        let w = g.world();
        assert!(!w.default_weather, "{lv}: no weather found: {:?}", g.notes);
        assert!(!message.contains("No weather"), "{lv}: {message}");
        if let Some((_, state)) = STATES.iter().find(|(l, _)| *l == lv) {
            assert_eq!(
                g.notes.first().map(String::as_str),
                Some(format!("weather: {state}").as_str()),
                "{lv}: the active state of its settings"
            );
        }
        assert!(
            w.light
                .get("outdoorLightKeyColor")
                .is_some_and(|c| c.iter().take(3).any(|v| *v > 0.5)),
            "{lv}: the state's key light"
        );
        assert!(w.terrain.is_some(), "{lv}: terrain loaded: {:?}", g.notes);
        h.settle(&mut app, 4);
        {
            let g = app.view().unwrap().game_view.game.as_ref().unwrap();
            assert!(g.error.is_none(), "{lv}: {:?}", g.error);
            assert!(g.timings.draws > 0, "{lv}: nothing drawn");
            println!("{lv}: {} draws, {:?}", g.timings.draws, g.world().counts);
        }
        if let Some(d) = &shots {
            h.save(&d.join(format!("gw1_{lv}.png"))).unwrap();
            // and in the light theme (round R5e: the open sea in both)
            app.theme = ThemeId::Light;
            h.settle(&mut app, 4);
            h.save(&d.join(format!("gw1_{lv}_light.png"))).unwrap();
            app.theme = ThemeId::Dark;
            h.settle(&mut app, 2);
        }
        // the whole terrain drawn, from above its middle and from its south-west corner (whole_terrain.rs; a device
        // of its own: a device keeps the engine textures it was given by name, the height map among them)
        let mut dev = Device::new(false).unwrap();
        let world = app.view_mut().unwrap().game_view.game.as_mut().unwrap().world_mut();
        let (lo, hi) = bounds(&world.terrain.as_ref().unwrap().hf);
        let mid = [0, 1, 2].map(|k| f64::midpoint(lo[k], hi[k]));
        let span = (hi[0] - lo[0]).max(hi[2] - lo[2]);
        println!("{lv}: the height field spans {lo:?} to {hi:?}");
        let aspect = f64::from(SIZE[0]) / f64::from(SIZE[1]);
        let far = span * 3.0;
        let above = frame::Camera::look(
            [mid[0], hi[1] + span * 0.9, mid[2] + 1.0],
            mid,
            60f64.to_radians(),
            NEAR,
            far,
            aspect,
        );
        let corner = frame::Camera::look(
            [lo[0] - span * 0.2, hi[1] + span * 0.3, lo[2] - span * 0.2],
            mid,
            60f64.to_radians(),
            NEAR,
            far,
            aspect,
        );
        for (view, cam) in [("above", above), ("corner", corner)] {
            let (hits, drawn, on) = coverage(&mut dev, world, &cam, mid);
            println!(
                "{lv} {view}: of {hits} sampled pixels on the terrain, {drawn} drawn ({:.1}%), {on} of them on the \
                 height field",
                drawn as f64 * 100.0 / hits.max(1) as f64
            );
            assert!(
                hits > 300,
                "{lv} {view}: the view sees the terrain ({hits} sampled pixels)"
            );
            assert!(
                drawn * 100 >= hits * 99,
                "{lv} {view}: the terrain is drawn wherever the view meets it: {drawn} of {hits} sampled pixels"
            );
            assert!(
                on * 100 >= drawn * 98,
                "{lv} {view}: the terrain is drawn at the height field's height: {on} of {drawn} sampled pixels"
            );
        }
    }
}
