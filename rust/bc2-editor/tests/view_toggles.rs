//! Round VW1: the view toggles, on MP_005 and SP_01 headless (the game copy's prepared levels):
//!
//! - each overlay (effects, lights, sounds, combat areas, capture areas, spawn protection) draws its colour's pixels
//!   when on and none when off: the app's look rendered alone (an empty scene, so nothing hides a line);
//! - drawn sizes match the data: a light's radius (SP_01), a sound's minimum distance and a capture radius (MP_005),
//!   each item's circle found where its radius projects (straight down, at 45 degrees) and not 10 % inside or
//!   outside;
//! - MP_005's combat area polygon is the partition's points (the scene loader's own outline of the same field);
//! - the player's-eye camera: the eye height and field of view are the data's (SP_01: 55 x 0.82; MP_005: 55), it
//!   follows the ground walking, Esc returns to the camera before;
//! - collision per destruction state: SP_01's guard tower's collision triangles differ per state, the counts are
//!   its packfile's parts (named by `GuardTower_Havok`'s `PartInfoDatas`) the state shows, and Page Down walks the
//!   states in order;
//! - the navmesh and cull tree view modes render on both levels (their data read, the view changed).
//!
//! `BC2_EDITOR_SHOTS=<folder>` saves the pictures in Dark and Light. Skipped (with a note) without the game copy, the
//! prepared levels or a graphics adapter.
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
    clippy::panic,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::many_single_char_names,
    missing_docs
)]

use std::path::PathBuf;
use std::time::Instant;

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::display::{Destruction, ShowAs};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::overlays::{Item, Overlay};
use bc2_editor::render::{Look, Renderer, SceneGpu};
use bc2_editor::theme::{Palette, ThemeId, Tokens};
use bc2_editor::workspace::ViewMode;
use bc2_scene::scene::Scene;
use eframe::egui::{Color32, Key, Modifiers};

const SIZE: [u32; 2] = [1600, 950];
const SQUARE: [u32; 2] = [600, 600];

fn shots() -> Option<PathBuf> {
    std::env::var_os("BC2_EDITOR_SHOTS").map(PathBuf::from)
}

fn open(h: &mut Headless, lv: &str) -> Option<EditorApp> {
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    if let Err(e) = app.open_level_blocking(&gpu, lv) {
        assert!(
            !crate::level_here(lv),
            "{lv} is in this game copy but does not open: {e}"
        );
        println!("{lv}: skipped, not in this game copy ({e})");
        return None;
    }
    let v = app.view_mut().unwrap();
    v.labels = false;
    assert!(
        v.gpu_scene().visuals().is_some(),
        "{lv}: the render meshes were not read: {:?}",
        v.notes.last()
    );
    h.settle(&mut app, 2);
    Some(app)
}

fn palette(app: &EditorApp) -> Palette {
    Palette::new(Tokens::embedded().unwrap(), app.theme)
}

/// Pixels of an image within `tol` of `c`.
fn count(img: &[u8], c: Color32, tol: u8) -> usize {
    img.as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[0].abs_diff(c.r()) <= tol && p[1].abs_diff(c.g()) <= tol && p[2].abs_diff(c.b()) <= tol)
        .count()
}

/// Whether a pixel within `r` of `at` is within `tol` of `c`.
fn near(img: &[u8], size: [u32; 2], at: [f64; 2], r: i64, c: Color32, tol: u8) -> bool {
    for dy in -r..=r {
        for dx in -r..=r {
            let (x, y) = (at[0] as i64 + dx, at[1] as i64 + dy);
            if x < 0 || y < 0 || x >= i64::from(size[0]) || y >= i64::from(size[1]) {
                continue;
            }
            let k = ((y as u32 * size[0] + x as u32) * 4) as usize;
            if img[k].abs_diff(c.r()) <= tol && img[k + 1].abs_diff(c.g()) <= tol && img[k + 2].abs_diff(c.b()) <= tol {
                return true;
            }
        }
    }
    false
}

/// A look's lines drawn alone (an empty scene: nothing hides them), seen by `cam`.
fn draw_alone(gpu: &bc2_editor::render::Gpu, r: &Renderer, pal: &Palette, look: &Look, cam: &Camera) -> Vec<u8> {
    let empty = Scene::default();
    let mut sg = SceneGpu::new(gpu, &empty, pal);
    sg.look = Look {
        overlay: look.overlay.clone(),
        ..Look::default()
    };
    sg.refresh(gpu, &empty, pal, &|_| false);
    let target = r.target(gpu, SQUARE);
    let view = cam.view([f64::from(SQUARE[0]), f64::from(SQUARE[1])]);
    r.render(gpu, &sg, &target, &view.matrix(), pal);
    r.read_output(gpu, &target).unwrap()
}

/// A top view over some points (their box, a margin).
fn top_over(pts: impl Iterator<Item = [f32; 3]>) -> Camera {
    let (mut lo, mut hi) = ([f64::MAX; 3], [f64::MIN; 3]);
    for p in pts {
        for a in 0..3 {
            lo[a] = lo[a].min(f64::from(p[a]));
            hi[a] = hi[a].max(f64::from(p[a]));
        }
    }
    let span = (hi[0] - lo[0]).max(hi[2] - lo[2]).max(2.0);
    Camera {
        target: [f64::midpoint(lo[0], hi[0]), hi[1] + 10.0, f64::midpoint(lo[2], hi[2])],
        yaw: 0.0,
        pitch: 0.0,
        distance: span * 1.1,
        fov: 60.0,
        projection: Projection::Top,
    }
}

/// An item's circle is drawn at its radius: its centre's top view, the circle's points at 45 and 225 degrees found
/// (within 2 px), and none 10 % inside or outside them.
fn radius_drawn(gpu: &bc2_editor::render::Gpu, r: &Renderer, pal: &Palette, item: &Item, what: &str) -> f64 {
    let radius = item.radius.unwrap();
    let c = item.paint.colour(pal);
    let look = Look {
        overlay: item.lines.iter().map(|s| (*s, c)).collect(),
        ..Look::default()
    };
    // a circle of the radius around the item's anchor in its lines: the centre is the anchor
    let centre = item.at;
    let cam = Camera {
        target: centre,
        yaw: 0.0,
        pitch: 0.0,
        distance: radius * 3.0,
        fov: 60.0,
        projection: Projection::Top,
    };
    let img = draw_alone(gpu, r, pal, &look, &cam);
    let view = cam.view([f64::from(SQUARE[0]), f64::from(SQUARE[1])]);
    let d = std::f64::consts::FRAC_1_SQRT_2;
    let mut found = 0.0;
    for (sx, sz) in [(d, d), (-d, -d)] {
        let at = |k: f64| [centre[0] + sx * radius * k, centre[1], centre[2] + sz * radius * k];
        let on = view.project(at(1.0)).unwrap();
        let inside = view.project(at(0.9)).unwrap();
        let outside = view.project(at(1.1)).unwrap();
        assert!(
            near(&img, SQUARE, on, 2, c, 10),
            "{what}: the circle at its radius {radius} m"
        );
        assert!(!near(&img, SQUARE, inside, 1, c, 10), "{what}: nothing 10 % inside");
        assert!(!near(&img, SQUARE, outside, 1, c, 10), "{what}: nothing 10 % outside");
        // the drawn radius in metres: the pixel distance from the centre's pixel, in the view's scale
        let mid = view.project(centre).unwrap();
        let px = ((on[0] - mid[0]).powi(2) + (on[1] - mid[1]).powi(2)).sqrt();
        let per_metre = f64::from(SQUARE[1]) / (2.0 * view.ortho.unwrap());
        found = px / per_metre;
    }
    println!("{what}: radius {radius} m in the data, drawn {found:.3} m (projected), the circle found there");
    found
}

/// Turns every overlay off, then `only` on (if any): the look's overlay lines.
fn only(app: &mut EditorApp, h: &Headless, only: Option<Overlay>) -> Look {
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    for o in Overlay::ALL {
        let on = app.view().unwrap().display().overlays.on(o);
        if on != (Some(o) == only) {
            app.run_action(&gpu, &ctx, Action::ToggleOverlay(o));
        }
    }
    app.view().unwrap().gpu_scene().look.clone()
}

/// The overlays: each on draws its colour, each off none.
fn toggles_draw(app: &mut EditorApp, h: &Headless, lv: &str) {
    let gpu = h.gpu.clone();
    let pal = palette(app);
    let r = Renderer::new(&gpu.gpu);
    let o = app.view().unwrap().overlays.clone().unwrap();
    println!(
        "{lv}: overlays read in {:.3} s: {}; notes {:?}",
        o.seconds,
        Overlay::ALL
            .iter()
            .map(|x| format!("{} {}", x.name(), o.count(*x)))
            .collect::<Vec<_>>()
            .join(", "),
        o.notes
    );
    for ov in Overlay::ALL {
        let items: Vec<&Item> = o.items.iter().filter(|i| i.overlay == ov).collect();
        if items.is_empty() {
            println!("{lv}: {} - none in this level", ov.name());
            continue;
        }
        let cam = top_over(items.iter().flat_map(|i| i.lines.iter().flatten().copied()));
        let colours: Vec<Color32> = {
            let mut c: Vec<Color32> = items.iter().map(|i| i.paint.colour(&pal)).collect();
            c.dedup();
            c
        };
        let on = only(app, h, Some(ov));
        let off = only(app, h, None);
        let lit = draw_alone(&gpu.gpu, &r, &pal, &on, &cam);
        let dark = draw_alone(&gpu.gpu, &r, &pal, &off, &cam);
        let n_on: usize = colours.iter().map(|c| count(&lit, *c, 6)).sum();
        let n_off: usize = colours.iter().map(|c| count(&dark, *c, 6)).sum();
        println!(
            "{lv}: {} on: {} items, {} lines, {n_on} pixels of its colour; off: {n_off}",
            ov.name(),
            items.len(),
            on.overlay.len()
        );
        assert!(n_on > 0, "{lv}: {} draws when on", ov.name());
        assert_eq!(n_off, 0, "{lv}: {} draws nothing when off", ov.name());
        assert!(off.overlay.is_empty(), "{lv}: no overlay lines with every toggle off");
    }
    // back to the defaults (combat and capture areas on)
    only(app, h, None);
    for ov in [Overlay::CombatAreas, Overlay::CaptureAreas] {
        app.run_action(&gpu, &h.ctx.clone(), Action::ToggleOverlay(ov));
    }
}

/// The player's-eye camera: the data's eye height and field of view, the ground followed, Esc back.
fn walk(app: &mut EditorApp, h: &mut Headless, lv: &str, fov: f64) {
    // start on open terrain: the first soldier spawn whose ground is the height field, 20 m around it too
    {
        let v = app.view().unwrap();
        let hf = v.scene().heightfield.clone().unwrap();
        let on_terrain = |x: f64, z: f64| {
            let t = hf.height(x, z);
            v.ground_below([x, t + 3.0, z], 10.0)
                .is_some_and(|g| (g - t).abs() < 0.05)
        };
        // the most uneven of those (the walk climbs and descends): the spawns ranked by the height field's rise
        // first (cheap), then the first that passes the ray test (21 rays through the level's collision each,
        // slow in a debug build: testing every spawn took ~16 min on SP_01). The same spawn as max_by over the
        // passing ones: reversed, then a stable sort, so ties pick the last as max_by does.
        let rise = |p: &[f64; 3]| {
            let hs: Vec<f64> = (0..=20)
                .map(|k| hf.height(p[0] + f64::from(k), p[2] + 0.3 * f64::from(k)))
                .collect();
            hs.iter().copied().fold(f64::MIN, f64::max) - hs.iter().copied().fold(f64::MAX, f64::min)
        };
        let mut spawns: Vec<([f64; 3], f64)> = v
            .scene()
            .entities
            .iter()
            .filter(|e| e.category == "soldier spawn")
            .filter_map(bc2_scene::scene::Entity::position)
            .map(|p| (p, rise(&p)))
            .collect();
        spawns.reverse();
        spawns.sort_by(|a, b| b.1.total_cmp(&a.1));
        let start = spawns
            .into_iter()
            .map(|(p, _)| p)
            .find(|p| (0..=20).all(|k| on_terrain(p[0] + f64::from(k), p[2] + 0.3 * f64::from(k))))
            .expect("a spawn on open terrain");
        let cam = Camera {
            target: start,
            yaw: -std::f64::consts::FRAC_PI_2,
            pitch: 0.3,
            distance: 30.0,
            fov: 60.0,
            projection: Projection::Perspective,
        };
        app.view_mut()
            .unwrap()
            .viewports
            .get_mut(&Projection::Perspective)
            .unwrap()
            .camera = cam;
        h.settle(app, 2);
    }
    let before = app.view().unwrap().viewports[&Projection::Perspective].camera;
    h.key(app, Key::W, Modifiers::ALT);
    let w = app.view().unwrap().walk.clone().expect("Alt+W starts the walk");
    println!("{lv}: player's eye: {}", w.soldier.source);
    assert!(w.soldier.source.contains("DefaultSoldierPhysics") && w.soldier.source.contains("LevelData"));
    assert_eq!(w.soldier.stand, 1.6, "EyePosition of the standing pose");
    assert_eq!(w.soldier.crouch, 1.0, "EyePosition of the crouching pose");
    assert!(
        (w.soldier.fov() - fov).abs() < 1e-4,
        "{lv}: field of view {} against {fov}",
        w.soldier.fov()
    );
    let cam = app.view().unwrap().viewports[&Projection::Perspective].camera;
    assert!((cam.fov - fov).abs() < 1e-4);
    // walking: the feet on the surface (the terrain's height field where it is the terrain), the eye 1.6 m above
    let hf = app.view().unwrap().scene().heightfield.clone().unwrap();
    let mut w = w;
    let (mut on_terrain, mut worst, mut moved) = (0usize, 0.0f64, 0.0);
    let (mut lo, mut hi) = (f64::MAX, f64::MIN);
    for _ in 0..120 {
        let from = w.feet;
        let v = app.view().unwrap();
        w.step(1.0, 0.3, 1.0 / 30.0, false, |p, d| v.ground_below(p, d));
        moved += ((w.feet[0] - from[0]).powi(2) + (w.feet[2] - from[2]).powi(2)).sqrt();
        let terrain = hf.height(w.feet[0], w.feet[2]);
        let e = w.eye();
        assert!((e[1] - w.feet[1] - 1.6).abs() < 1e-9);
        if (w.feet[1] - terrain).abs() < 0.05 {
            on_terrain += 1;
            worst = worst.max((e[1] - terrain - 1.6).abs());
        }
        lo = lo.min(w.feet[1]);
        hi = hi.max(w.feet[1]);
    }
    println!(
        "{lv}: walked {moved:.2} m in 4 s (4 m/s from the data); on the terrain {on_terrain} of 120 steps, the eye 1.6 m \
         above the height field (worst {worst:.4} m); the feet from {lo:.2} to {hi:.2} m"
    );
    assert!((moved - 16.0).abs() < 0.2, "{lv}: {moved} m");
    assert!(on_terrain > 30, "{lv}: the walk stands on the terrain ({on_terrain})");
    assert!(hi - lo > 0.5, "{lv}: the walk climbs or descends ({lo} - {hi})");
    assert!(worst < 0.05, "{lv}: the eye follows the ground ({worst})");
    // the app's own walk: W held for a few frames moves it; Esc returns
    let start = app.view().unwrap().walk.clone().unwrap().feet;
    for _ in 0..10 {
        h.frame(
            app,
            vec![eframe::egui::Event::Key {
                key: Key::W,
                physical_key: None,
                pressed: true,
                repeat: true,
                modifiers: Modifiers::NONE,
            }],
        );
    }
    h.frame(
        app,
        vec![eframe::egui::Event::Key {
            key: Key::W,
            physical_key: None,
            pressed: false,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
    let after = app.view().unwrap().walk.clone().unwrap().feet;
    println!("{lv}: W held 10 frames in the app: {start:?} -> {after:?}");
    assert_ne!(start, after, "{lv}: W walks in the app");
    if let Some(dir) = shots() {
        for theme in ThemeId::ALL {
            app.theme = theme;
            h.settle(app, 3);
            let t = if theme == ThemeId::Dark { "dark" } else { "light" };
            h.save(&dir.join(format!("vw1_{lv}_players_eye_{t}.png"))).unwrap();
        }
        app.theme = ThemeId::Dark;
        h.settle(app, 2);
    }
    h.key(app, Key::Escape, Modifiers::NONE);
    assert!(app.view().unwrap().walk.is_none(), "Esc ends the walk");
    assert_eq!(
        app.view().unwrap().viewports[&Projection::Perspective].camera,
        before,
        "Esc returns to the camera before"
    );
}

/// The navmesh and cull tree view modes render: their data read, the view changed.
fn modes_render(app: &mut EditorApp, h: &mut Headless, lv: &str, nav: bool) {
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    let rect = app.view().unwrap().view_rect().unwrap();
    let pixels = |h: &mut Headless| -> Vec<u8> {
        let img = h.image().unwrap();
        let mut out = Vec::new();
        for y in rect.top() as u32..(rect.bottom() as u32).min(SIZE[1]) {
            for x in rect.left() as u32..(rect.right() as u32).min(SIZE[0]) {
                let k = ((y * SIZE[0] + x) * 4) as usize;
                out.extend_from_slice(&img[k..k + 3]);
            }
        }
        out
    };
    h.settle(app, 3);
    let lit = pixels(h);
    app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Navmesh));
    let p = app.palette();
    app.view_mut().unwrap().nav_wait(&gpu, &p);
    h.settle(app, 4);
    let v = app.view().unwrap();
    let note = v.mode_note.clone();
    let files = v
        .navs
        .data()
        .map(|d| d.files.iter().map(|f| f.name).collect::<Vec<_>>());
    let drawn = pixels(h);
    let changed = lit.chunks(3).zip(drawn.chunks(3)).filter(|(a, b)| a != b).count();
    println!("{lv}: navmesh mode: {note} - files {files:?}, {changed} pixels changed");
    if nav {
        assert!(files.is_some_and(|f| !f.is_empty()), "{lv}: the path data read: {note}");
        assert!(changed > 1000, "{lv}: the navmesh drawn");
    } else {
        // a multiplayer level without path data (no AI in DICE's game; NavKit has not built it): said plainly
        assert!(note.contains("no navigation data"), "{lv}: {note}");
    }
    app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::CullTree));
    h.settle(app, 4);
    let v = app.view().unwrap();
    let note = v.mode_note.clone();
    let cull = pixels(h);
    let changed = lit.chunks(3).zip(cull.chunks(3)).filter(|(a, b)| a != b).count();
    println!("{lv}: cull tree mode: {note}, {changed} pixels changed");
    assert!(
        note.starts_with("Cull tree:") && note.contains("leaves"),
        "{lv}: {note}"
    );
    assert!(!note.contains(": 0 leaves"), "{lv}: the tree's leaves read: {note}");
    assert!(changed > 1000, "{lv}: the cull tree drawn");
    if let Some(dir) = shots() {
        for (m, name) in [(ViewMode::Navmesh, "navmesh"), (ViewMode::CullTree, "culltree")] {
            app.run_action(&gpu, &ctx, Action::SetViewMode(m));
            app.view_mut().unwrap().nav_wait(&gpu, &p);
            h.settle(app, 3);
            h.save(&dir.join(format!("vw1_{lv}_{name}_dark.png"))).unwrap();
        }
    }
    app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Lit));
    h.settle(app, 2);
}

/// Frame times of the view with every overlay off and on (the view drawn and waited for).
fn frame_times(app: &mut EditorApp, h: &Headless, lv: &str) {
    let gpu = h.gpu.clone();
    let pal = palette(app);
    let r = Renderer::new(&gpu.gpu);
    let big = r.target(&gpu.gpu, [1600, 950]);
    // the whole level from above (the overlays' extent)
    let o = app.view().unwrap().overlays.clone().unwrap();
    let top = top_over(o.items.iter().flat_map(|i| i.lines.iter().flatten().copied()));
    let cam = Camera {
        pitch: 0.9,
        yaw: 0.6,
        projection: Projection::Perspective,
        ..top
    };
    let time = |app: &EditorApp| {
        let mut t = Vec::new();
        for f in 0..14 {
            let c = Camera {
                yaw: cam.yaw + 0.01 * f64::from(f),
                ..cam
            };
            let view = c.view([1600.0, 950.0]);
            let s = Instant::now();
            r.render(&gpu.gpu, app.view().unwrap().gpu_scene(), &big, &view.matrix(), &pal);
            let _ = gpu.gpu.device.poll(eframe::egui_wgpu::wgpu::PollType::Wait {
                submission_index: None,
                timeout: None,
            });
            t.push(s.elapsed().as_secs_f64() * 1e3);
        }
        t.drain(..2);
        t.sort_by(f64::total_cmp);
        t[t.len() / 2]
    };
    // alternated three times, the best of each (the first frames warm the GPU up)
    let (mut off, mut on) = (f64::MAX, f64::MAX);
    let (mut off_look, mut on_look) = (Look::default(), Look::default());
    for _ in 0..3 {
        off_look = only(app, h, None);
        off = off.min(time(app));
        for o in Overlay::ALL {
            app.run_action(&gpu, &h.ctx.clone(), Action::ToggleOverlay(o));
        }
        on_look = app.view().unwrap().gpu_scene().look.clone();
        on = on.min(time(app));
    }
    println!(
        "{lv} frame, overview: every overlay off {off:.2} ms ({} lines), every overlay on {on:.2} ms ({} lines)",
        off_look.overlay.len(),
        on_look.overlay.len()
    );
    only(app, h, None);
    for ov in [Overlay::CombatAreas, Overlay::CaptureAreas] {
        app.run_action(&gpu, &h.ctx.clone(), Action::ToggleOverlay(ov));
    }
}

/// Saves the view in Dark and Light with some overlays on, looking at `target` from `distance`.
fn overlay_shots(app: &mut EditorApp, h: &mut Headless, lv: &str, name: &str, on: &[Overlay], cam: Camera) {
    let Some(dir) = shots() else { return };
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    only(app, h, None);
    for o in on {
        app.run_action(&gpu, &ctx, Action::ToggleOverlay(*o));
    }
    app.view_mut()
        .unwrap()
        .viewports
        .get_mut(&Projection::Perspective)
        .unwrap()
        .camera = cam;
    for theme in ThemeId::ALL {
        app.theme = theme;
        h.settle(app, 3);
        let t = if theme == ThemeId::Dark { "dark" } else { "light" };
        h.save(&dir.join(format!("vw1_{lv}_{name}_{t}.png"))).unwrap();
    }
    app.theme = ThemeId::Dark;
    only(app, h, None);
    for ov in [Overlay::CombatAreas, Overlay::CaptureAreas] {
        app.run_action(&gpu, &ctx, Action::ToggleOverlay(ov));
    }
    h.settle(app, 2);
}

fn look_from(at: [f64; 3], distance: f64, yaw: f64, pitch: f64) -> Camera {
    Camera {
        target: at,
        yaw,
        pitch,
        distance,
        fov: 60.0,
        projection: Projection::Perspective,
    }
}

#[test]
#[ignore = "game tier: opens MP_005 and walks it; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn mp_005_overlays_walk_and_modes() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new(SIZE) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let Some(mut app) = open(&mut h, "mp_005") else { return };
    let gpu = h.gpu.clone();
    let pal = palette(&app);
    let r = Renderer::new(&gpu.gpu);
    toggles_draw(&mut app, &h, "mp_005");
    let o = app.view().unwrap().overlays.clone().unwrap();
    // ---- the drawn radii: a sound's minimum distance, a capture radius, a spawn check
    let sound = o
        .items
        .iter()
        .find(|i| i.overlay == Overlay::Sounds && i.radius.is_some_and(|r| r > 0.0))
        .expect("a sound with a minimum distance");
    assert!(
        (radius_drawn(&gpu.gpu, &r, &pal, sound, "mp_005 sound") - sound.radius.unwrap()).abs()
            < 0.05 * sound.radius.unwrap()
    );
    let capture = o
        .items
        .iter()
        .find(|i| i.overlay == Overlay::CaptureAreas && i.radius.is_some())
        .expect("a capture radius (RUS_Base: CaptureRadius 1)");
    assert!(
        (radius_drawn(&gpu.gpu, &r, &pal, capture, "mp_005 capture radius") - capture.radius.unwrap()).abs()
            < 0.05 * capture.radius.unwrap()
    );
    let spawn = o.items.iter().find(|i| i.overlay == Overlay::SpawnProtection).unwrap();
    assert_eq!(spawn.radius, Some(20.0), "SpawnEnemyRadius 20 on MP_005");
    // ---- the combat area polygon is the partition's points (the scene's outline of its Geometry)
    let scene = app.view().unwrap().scene().clone();
    let combat = o.items.iter().find(|i| i.overlay == Overlay::CombatAreas).unwrap();
    let k = combat.entity.unwrap();
    let outline = scene
        .shapes
        .iter()
        .find(|s| s.entity == k && s.field == "Geometry")
        .expect("the scene's outline of the combat area");
    for p in &outline.points {
        let hit = combat
            .lines
            .iter()
            .flatten()
            .any(|q| (f64::from(q[0]) - p[0]).abs() < 1e-3 && (f64::from(q[2]) - p[2]).abs() < 1e-3);
        assert!(hit, "the combat area's point {p:?} drawn");
    }
    println!(
        "mp_005: combat area {}: {} points, each drawn (a {}-line wall); {}",
        scene.entities[k].name.clone().unwrap_or_default(),
        outline.points.len(),
        combat.lines.len(),
        combat.label
    );
    // the scene no longer draws the outline the overlay covers
    assert!(app.view().unwrap().gpu_scene().look.skip_shapes.contains(&k));
    // ---- the capture areas: MP_005's five flags, the bases HQs
    let flags: Vec<&Item> = o.items.iter().filter(|i| i.overlay == Overlay::CaptureAreas).collect();
    println!(
        "mp_005: capture areas {:?}",
        flags.iter().map(|i| i.label.as_str()).collect::<Vec<_>>()
    );
    assert_eq!(flags.len(), 5);
    assert_eq!(flags.iter().filter(|i| i.label.starts_with("HQ")).count(), 2);
    assert_eq!(
        flags.iter().filter(|i| i.label.contains("sphere")).count(),
        2,
        "the islands: sphere geometry"
    );
    // ---- the pictures: the flags' capture areas, the combat area, sounds and effects
    if shots().is_some() {
        let b = flags
            .iter()
            .find(|i| i.label.contains(" B"))
            .map_or(flags[0].at, |i| i.at);
        overlay_shots(
            &mut app,
            &mut h,
            "mp_005",
            "flag_b_capture_spawns",
            &[
                Overlay::CaptureAreas,
                Overlay::SpawnProtection,
                Overlay::Sounds,
                Overlay::Effects,
            ],
            look_from(b, 140.0, 0.9, 0.55),
        );
        overlay_shots(
            &mut app,
            &mut h,
            "mp_005",
            "combat_area",
            &[Overlay::CombatAreas, Overlay::CaptureAreas],
            look_from(combat.at, 1600.0, 0.6, 0.75),
        );
        let fx = o.items.iter().find(|i| i.overlay == Overlay::Effects).unwrap();
        overlay_shots(
            &mut app,
            &mut h,
            "mp_005",
            "effects_sounds_lights",
            &[Overlay::Effects, Overlay::Sounds, Overlay::Lights],
            look_from(fx.at, 90.0, 0.4, 0.45),
        );
    }
    // ---- the player's eye: MP_005's DefaultFOV 55 x InfantryFOVMultiplier 1
    walk(&mut app, &mut h, "mp_005", 55.0);
    modes_render(&mut app, &mut h, "mp_005", false);
    frame_times(&mut app, &h, "mp_005");
}

#[test]
#[ignore = "game tier: opens SP_01 and walks it; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn sp_01_lights_collision_states_walk_and_modes() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new(SIZE) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let Some(mut app) = open(&mut h, "sp_01") else { return };
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    let pal = palette(&app);
    let r = Renderer::new(&gpu.gpu);
    toggles_draw(&mut app, &h, "sp_01");
    let o = app.view().unwrap().overlays.clone().unwrap();
    // ---- a light's radius as drawn: the data's PointLightRadius
    let lights: Vec<&Item> = o.items.iter().filter(|i| i.overlay == Overlay::Lights).collect();
    println!(
        "sp_01: {} lights: {:?}",
        lights.len(),
        lights
            .iter()
            .map(|i| (&i.label, i.radius, i.swatch))
            .take(6)
            .collect::<Vec<_>>()
    );
    assert_ne!(lights.len(), 0);
    let big = lights
        .iter()
        .max_by(|a, b| a.radius.unwrap_or(0.0).total_cmp(&b.radius.unwrap_or(0.0)))
        .unwrap();
    assert!(
        (radius_drawn(&gpu.gpu, &r, &pal, big, "sp_01 light") - big.radius.unwrap()).abs() < 0.05 * big.radius.unwrap()
    );
    // ---- collision per destruction state: the guard tower
    let scene = app.view().unwrap().scene().clone();
    let tower = scene
        .entities
        .iter()
        .find(|e| {
            e.blueprint
                .as_deref()
                .is_some_and(|b| b.ends_with("guardtower/guardtower"))
                && e.matrix.is_some()
        })
        .unwrap()
        .index;
    let bp = scene.entities[tower].blueprint.clone().unwrap();
    let col = scene.collision.as_ref().unwrap();
    let parts = col.parts.get(&bp).expect("the guard tower's packfile parts").clone();
    assert!(parts.split(), "its collision is split per part");
    // the part names: GuardTower_Havok's PartInfoDatas, in order, each hash the header's
    let names = ["part_1", "part_1_dest", "part_2", "part_2_dest", "ruin", "ruin_downed"];
    let hashes: Vec<u32> = names.iter().map(|n| bc2_formats::havok::part_hash(n)).collect();
    assert_eq!(parts.hashes, hashes, "the header's parts are PartInfoDatas' names");
    let per_part: Vec<usize> = (0..names.len())
        .map(|k| parts.part_of.iter().filter(|&&p| usize::from(p) == k).count())
        .collect();
    println!(
        "sp_01 guard tower: {} collision triangles, per part {:?}",
        parts.part_of.len(),
        names.iter().zip(&per_part).collect::<Vec<_>>()
    );
    let comps = app
        .view()
        .unwrap()
        .gpu_scene()
        .components(&scene.entities[tower])
        .to_vec();
    let steps = Destruction::steps(bc2_render::simple::stage_count(&comps));
    assert_eq!(
        steps,
        [
            Destruction::Intact,
            Destruction::Stage(1),
            Destruction::Destroyed,
            Destruction::AllParts
        ]
    );
    // the tower shown as its collision, the rest as meshes
    let g = scene.entities[tower].guid.to_lowercase();
    app.view_mut().unwrap().set_display(&gpu, &pal, |d| {
        d.objects.insert(g, ShowAs::Collision);
    });
    app.select(&gpu, Some(tower));
    let drawn = |app: &EditorApp| -> u32 {
        app.view()
            .unwrap()
            .gpu_scene()
            .drawn(tower)
            .iter()
            .filter(|d| !d.render_mesh)
            .map(|d| d.count / 3)
            .sum()
    };
    // the expected triangles per state: the parts the state shows (by name), from the per-part counts
    let shown = |s: Destruction| -> u32 {
        let st = s.state();
        names
            .iter()
            .zip(&per_part)
            .filter(|(n, _)| bc2_render::simple::collision_part_shown(&comps, bc2_formats::havok::part_hash(n), st))
            .map(|(_, c)| *c as u32)
            .sum()
    };
    let mut seen = Vec::new();
    for (k, s) in steps.iter().enumerate() {
        let now = app.view().unwrap().display().state_of(&scene.entities[tower]);
        assert_eq!(now, *s, "Page Down walks the states in order (step {k})");
        let n = drawn(&app);
        println!("sp_01 guard tower, {}: {n} collision triangles drawn", s.name());
        assert_eq!(n, shown(*s), "{}: its parts' triangles", s.name());
        seen.push(n);
        if let Some(dir) = shots() {
            let at = scene.entities[tower].position().unwrap();
            app.view_mut()
                .unwrap()
                .viewports
                .get_mut(&Projection::Perspective)
                .unwrap()
                .camera = look_from([at[0], at[1] + 6.0, at[2]], 20.0, 2.4, 0.35);
            for theme in ThemeId::ALL {
                app.theme = theme;
                h.settle(&mut app, 3);
                let t = if theme == ThemeId::Dark { "dark" } else { "light" };
                let n = s.name().to_lowercase().replace(' ', "_");
                h.save(&dir.join(format!("vw1_sp_01_guardtower_collision_{k}_{n}_{t}.png")))
                    .unwrap();
            }
            app.theme = ThemeId::Dark;
            h.settle(&mut app, 2);
        }
        h.key(&mut app, Key::PageDown, Modifiers::NONE);
    }
    assert_eq!(seen[3] as usize, parts.part_of.len(), "every part: every triangle");
    for a in 0..3 {
        for b in a + 1..3 {
            assert_ne!(seen[a], seen[b], "the states' collision differs ({a}, {b})");
        }
    }
    // Page Up back to intact
    for _ in 0..4 {
        h.key(&mut app, Key::PageUp, Modifiers::NONE);
    }
    assert_eq!(
        app.view().unwrap().display().state_of(&scene.entities[tower]),
        Destruction::Intact
    );
    app.run_action(&gpu, &ctx, Action::ResetViewOverrides);
    app.select(&gpu, None);
    // ---- a light in the picture
    if shots().is_some() {
        overlay_shots(
            &mut app,
            &mut h,
            "sp_01",
            "lights_sounds",
            &[Overlay::Lights, Overlay::Sounds, Overlay::Effects],
            look_from(big.at, big.radius.unwrap() * 4.0, 0.7, 0.5),
        );
    }
    // ---- the player's eye: SP_01's DefaultFOV 55 x InfantryFOVMultiplier 0.82
    walk(&mut app, &mut h, "sp_01", 55.0 * 0.82);
    modes_render(&mut app, &mut h, "sp_01", true);
    frame_times(&mut app, &h, "sp_01");
}

/// The navmesh and cull tree modes on a second level with path data (Onslaught's ONS_MP_004: NavKit's build and
/// DICE's file).
#[test]
#[ignore = "game tier: opens ONS_MP_004; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn ons_mp_004_navmesh_and_cull_tree() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new(SIZE) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let Some(mut app) = open(&mut h, "ons_mp_004") else {
        return;
    };
    modes_render(&mut app, &mut h, "ons_mp_004", true);
}
