//! Round G2's proofs in the running editor, without a window: the 3D view draws with the `view-*` and `scene-*`
//! tokens (each one found in a rendered frame, both themes), the overlay colours read on the terrain as rendered
//! (contrast measured on the frame's own pixels), the Help window opens from F1 and the top bar, and opening a level
//! that needs preparing shows the preparing view, then opens the level or says in plain words why not.
//!
//! The GPU tests are skipped (with a note) on a machine without any graphics adapter, software included; the
//! preparing tests without Python. `BC2_EDITOR_SHOTS=<folder>` saves the images there.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::many_single_char_names,
    clippy::needless_raw_string_hashes,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    missing_docs
)]

use std::path::{Path, PathBuf};

use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::headless::Headless;
use bc2_editor::render::{self, Gpu, Renderer, SceneGpu};
use bc2_editor::theme::{self, Palette, ThemeId, Tokens};
use bc2_editor::{demo, help};
use bc2_scene::scene::{Entity, Scene, Source};
use eframe::egui::{Color32, Event, Key, Modifiers};

fn gpu() -> Option<Gpu> {
    match Gpu::headless() {
        Ok((g, _)) => Some(g),
        Err(e) => {
            println!("skipped: {e}");
            None
        }
    }
}

fn shot(name: &str, size: [u32; 2], rgba: &[u8]) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        let p = Path::new(&dir).join(name);
        render::write_png(&p, size, rgba).unwrap();
        println!("saved {}", p.display());
    }
}

const SIZE: [u32; 2] = [640, 400];

fn pixel(img: &[u8], x: u32, y: u32) -> Color32 {
    let k = ((y * SIZE[0] + x) * 4) as usize;
    Color32::from_rgb(img[k], img[k + 1], img[k + 2])
}

fn entity(index: usize, category: &str, team: Option<u8>, at: [f64; 3]) -> Entity {
    let mut m = [0.0; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m[12] = at[0];
    m[13] = at[1];
    m[14] = at[2];
    Entity {
        index,
        guid: format!("00000000-0000-0000-0000-{index:012}"),
        type_name: "Entity.Test".into(),
        short_type: "Test".into(),
        category: category.into(),
        id: None,
        name: Some(format!("{category} {index}")),
        partition: "levels/TEST".into(),
        matrix: Some(m),
        blueprint: None,
        group: None,
        blueprint_type: None,
        destructible: false,
        team,
        ai: None,
        source: Source::Instance { instance: index },
    }
}

/// One marker of each colour on the demo's terrain, 16 m apart along x: (category, team, the token it is drawn in).
const MARKERS: [(&str, Option<u8>, &str); 11] = [
    ("soldier spawn", Some(1), "view-team-1"),
    ("soldier spawn", Some(2), "view-team-2"),
    ("capture point", Some(0), "view-team-neutral"),
    ("objective", None, "view-unowned"),
    ("placed object", None, "scene-object"),
    ("destructible", None, "scene-object-destructible"),
    ("dynamic object", None, "scene-object-dynamic"),
    ("ladder", None, "scene-ladder"),
    ("effect", None, "scene-effect"),
    ("area point", None, "scene-area-point"),
    ("other", None, "scene-other"),
];

fn x_of(k: usize) -> f64 {
    (k as f64 - 5.0) * 8.0
}

/// A static group member: drawn by its collision only (it has no marker), the demo's 10 x 6 x 8 m box.
const GROUP_MEMBER: usize = MARKERS.len();
const GROUP_AT: [f64; 3] = [0.0, 0.0, 16.0];

fn scene() -> Scene {
    let mut s = demo::scene();
    s.shapes.clear();
    s.entities = MARKERS
        .iter()
        .enumerate()
        .map(|(k, (c, t, _))| entity(k, c, *t, [x_of(k), 0.0, 0.0]))
        .collect();
    s.entities.push(entity(GROUP_MEMBER, "group member", None, GROUP_AT));
    if let Some(c) = s.collision.as_mut() {
        c.meshes = vec![(GROUP_MEMBER, "objects/props/demo_crate".to_owned())];
    }
    s
}

fn top_camera() -> Camera {
    Camera {
        target: [0.0, 0.0, 0.0],
        yaw: 0.0,
        pitch: 1.5,
        distance: 120.0,
        fov: 60.0,
        projection: Projection::Top,
    }
}

/// Every view token reaches the frame: a top view of one marker per colour, the selection outlined, both themes.
/// Overlays are lit evenly (their top face is the token itself); scene objects and the terrain with the view's
/// light (their top face is the token at the light's 0.93).
#[test]
fn view_tokens_reach_the_frame() {
    let Some(gpu) = gpu() else { return };
    let tokens = Tokens::embedded().unwrap();
    let r = Renderer::new(&gpu);
    let target = r.target(&gpu, SIZE);
    for theme in ThemeId::ALL {
        let p = Palette::new(tokens, theme);
        let s = scene();
        let mut sg = SceneGpu::new(&gpu, &s, &p);
        sg.set_hidden(&gpu, "water", true);
        sg.set_selection(&gpu, Some(3));
        let view = top_camera().view([f64::from(SIZE[0]), f64::from(SIZE[1])]);
        r.render(&gpu, &sg, &target, &view.matrix(), &p);
        let img = r.read_output(&gpu, &target).unwrap();
        shot(&format!("g2_tokens_top_{}.png", theme.key()), SIZE, &img);
        let near = |a: Color32, b: Color32| {
            a.r().abs_diff(b.r()) <= 3 && a.g().abs_diff(b.g()) <= 3 && a.b().abs_diff(b.b()) <= 3
        };
        let found = |want: Color32| (0..SIZE[1]).any(|y| (0..SIZE[0]).any(|x| near(pixel(&img, x, y), want)));
        let names: std::collections::HashMap<&str, Color32> = p.view_entries().into_iter().collect();
        for (k, (_, _, token)) in MARKERS.iter().enumerate() {
            let c = names[token];
            let want = if token.starts_with("view-") {
                c
            } else {
                theme::shaded(c, 0.9316)
            };
            // at the marker's own place
            let at = view.project([x_of(k), 1.5, 0.0]).unwrap();
            let got = pixel(&img, at[0] as u32, at[1] as u32);
            assert!(near(got, want), "{theme:?}: {token} drawn {got:?}, wanted {want:?}");
        }
        let at = view.project([GROUP_AT[0], 6.0, GROUP_AT[2]]).unwrap();
        let got = pixel(&img, at[0] as u32, at[1] as u32);
        let want = theme::shaded(p.scene_object_group, 0.9316);
        assert!(
            near(got, want),
            "{theme:?}: scene-object-group drawn {got:?}, wanted {want:?}"
        );
        assert!(
            found(theme::shaded(p.scene_terrain, 0.9316)),
            "{theme:?}: scene-terrain"
        );
        assert!(found(p.view_selection), "{theme:?}: view-selection");
        // the sky: the view's corner, past the terrain's edge in a perspective view
        let persp = Camera {
            target: [0.0, 0.0, 0.0],
            yaw: 0.3,
            pitch: 0.15,
            distance: 90.0,
            fov: 60.0,
            projection: Projection::Perspective,
        }
        .view([f64::from(SIZE[0]), f64::from(SIZE[1])]);
        sg.set_hidden(&gpu, "water", false);
        r.render(&gpu, &sg, &target, &persp.matrix(), &p);
        let img = r.read_output(&gpu, &target).unwrap();
        assert_eq!(pixel(&img, 2, 2), p.scene_sky, "{theme:?}: scene-sky");
        let water = theme::shaded(p.scene_water, 0.9316);
        let has_water = (0..SIZE[1]).any(|y| {
            (0..SIZE[0]).any(|x| {
                let c = pixel(&img, x, y);
                c.r().abs_diff(water.r()) <= 3 && c.g().abs_diff(water.g()) <= 3 && c.b().abs_diff(water.b()) <= 3
            })
        });
        // the demo scene has no water: the token is checked where a level has it (the screenshots); here, that
        // nothing else is drawn in its colour
        assert!(!has_water || s.water.is_some(), "{theme:?}: scene-water without water");
    }
}

/// The overlay colours read on the terrain as the view draws them: in a perspective frame (side faces in view),
/// each overlay marker's pixels against the terrain around it, at least 3.5:1 median to median and 3:1 between
/// the marker's darkest tenth and the terrain's brightest tenth, both themes.
#[test]
fn overlays_read_on_the_rendered_terrain() {
    let Some(gpu) = gpu() else { return };
    let tokens = Tokens::embedded().unwrap();
    let r = Renderer::new(&gpu);
    let target = r.target(&gpu, SIZE);
    for theme in ThemeId::ALL {
        let p = Palette::new(tokens, theme);
        let s = scene();
        let sg = SceneGpu::new(&gpu, &s, &p);
        for (k, (cat, _, token)) in MARKERS.iter().enumerate().take(4) {
            let size = f64::from(render::marker_size(cat).unwrap());
            let cam = Camera {
                target: [x_of(k), size / 2.0, 0.0],
                yaw: 0.7,
                pitch: 0.45,
                distance: 14.0,
                fov: 60.0,
                projection: Projection::Perspective,
            };
            let view = cam.view([f64::from(SIZE[0]), f64::from(SIZE[1])]);
            r.render(&gpu, &sg, &target, &view.matrix(), &p);
            let img = r.read_output(&gpu, &target).unwrap();
            shot(&format!("g2_contrast_{}_{token}.png", theme.key()), SIZE, &img);
            // the marker's pixels: picked by id; the terrain's: picked as terrain, in a ring around it
            let ids = r.read_ids(&gpu, &target).unwrap();
            let c = view.project([x_of(k), size / 2.0, 0.0]).unwrap();
            let (cx, cy) = (c[0] as i64, c[1] as i64);
            let mut mark = Vec::new();
            let mut ground = Vec::new();
            for dy in -90i64..=90 {
                for dx in -90i64..=90 {
                    let (x, y) = (cx + dx, cy + dy);
                    if x < 0 || y < 0 || x >= i64::from(SIZE[0]) || y >= i64::from(SIZE[1]) {
                        continue;
                    }
                    let id = ids[(y as u32 * SIZE[0] + x as u32) as usize];
                    let px = pixel(&img, x as u32, y as u32);
                    if render::id_entity(id) == Some(k) {
                        mark.push(theme::luminance(px));
                    } else if id == render::ID_TERRAIN {
                        ground.push(theme::luminance(px));
                    }
                }
            }
            assert!(
                mark.len() > 20 && ground.len() > 100,
                "{theme:?} {token}: {} / {}",
                mark.len(),
                ground.len()
            );
            mark.sort_by(f64::total_cmp);
            ground.sort_by(f64::total_cmp);
            let at = |v: &[f64], f: f64| v[((v.len() - 1) as f64 * f).round() as usize];
            let ratio = |a: f64, b: f64| (a.max(b) + 0.05) / (a.min(b) + 0.05);
            let median = ratio(at(&mark, 0.5), at(&ground, 0.5));
            let worst = ratio(at(&mark, 0.1), at(&ground, 0.9));
            println!("{theme:?} {token}: median {median:.2}, worst {worst:.2}");
            assert!(median >= 3.5, "{theme:?} {token}: median {median:.2}");
            assert!(worst >= 3.0, "{theme:?} {token}: worst {worst:.2}");
        }
    }
}

fn key(k: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// F1 opens Help (from the registry: the same action as the top bar's Help menu), its search finds a control, and
/// the faction labels show on the demo level's labels and properties.
#[test]
fn help_opens_and_searches() {
    let Ok(mut h) = Headless::new([1280, 800]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    app.open_scene(&h.gpu, "demo", demo::scene());
    h.settle(&mut app, 3);
    assert!(!app.help.open);
    h.frame(&mut app, vec![key(Key::F1, Modifiers::NONE)]);
    assert!(app.help.open, "F1 opens Help");
    app.help.page = help::Page::Controls;
    h.settle(&mut app, 6);
    shot("g2_help_controls.png", [1280, 800], &h.image().unwrap());
    app.help.query = "bookmark".into();
    h.settle(&mut app, 4);
    let t = help::topics();
    assert!(help::search(&t, &app.help.query).len() >= 2);
    app.help.query.clear();
    app.help.page = help::Page::GettingStarted;
    h.settle(&mut app, 4);
    shot("g2_help_getting_started.png", [1280, 800], &h.image().unwrap());
    // the demo's teams are US / RU
    let v = app.view().unwrap();
    assert_eq!(
        (v.teams.label(1), v.teams.label(2), v.teams.label(0)),
        ("US", "RU", "NEU")
    );
}

// ------------------------------------------------------------------------------------------------ preparing

fn python_ok() -> bool {
    std::process::Command::new("python")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_g2_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A stand-in game copy: one level, its bundle a placeholder (the stand-in script never reads it).
fn stand_in(root: &Path, lv: &str) {
    let d = root
        .join("game")
        .join("Dist")
        .join("win32")
        .join("levels")
        .join(lv.to_uppercase());
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("level-00.fbrb"), b"stand-in").unwrap();
}

/// A stand-in for prepare_level.py: the same output protocol, three steps, then done (it makes the level's dist
/// folder, as NavKit's prepare would) or an error.
fn script(dir: &Path, fail: bool) -> PathBuf {
    let p = dir.join(if fail { "prepare_fail.py" } else { "prepare_ok.py" });
    let body = format!(
        r#"import os, sys, time
lv = sys.argv[1]
root = sys.argv[sys.argv.index('--opfor') + 1]
for n, what in enumerate(['Unpacking the shared bundles (sp_common, mp_common)', 'Unpacking the level',
                          'Extracting objects, collision and spawn points'], 1):
    print(f'@step {{n}} 3 {{what}}', flush=True)
    print(f'@log  working on step {{n}}', flush=True)
    print('a plain print from a tool', flush=True)
    time.sleep(0.4)
if {fail}:
    print('@error ' + lv + ': the level bundle is damaged (a stand-in error)', flush=True)
    sys.exit(1)
os.makedirs(os.path.join(root, 'work', 'levels', lv, 'dist'), exist_ok=True)
print('@done', flush=True)
"#,
        fail = if fail { "True" } else { "False" }
    );
    std::fs::write(&p, body).unwrap();
    p
}

/// Opening a level that needs preparing: the preparing view with its step and log, then the level loads (here it
/// has no partition, so the load says so on the start screen); a failing preparation says why in plain words; the
/// start screen lists the level as prepared afterwards.
#[test]
fn preparing_then_opening() {
    if !python_ok() {
        println!("skipped: no python");
        return;
    }
    let Ok(mut h) = Headless::new([1280, 800]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let root = temp("prepare");
    stand_in(&root, "mp_901");
    let mut app = EditorApp::new(Some(&root), None).unwrap();
    assert_eq!(app.levels().len(), 1);
    assert!(!app.levels()[0].prepared);
    h.settle(&mut app, 2);
    app.open_level_preparing_with(&script(&root, false), "mp_901");
    // the preparing view, step by step
    let mut seen_steps = Vec::new();
    let mut shot_taken = false;
    let start = std::time::Instant::now();
    while app.preparing().is_some() && start.elapsed().as_secs() < 30 {
        h.frame(&mut app, Vec::new());
        if let Some(p) = app.preparing()
            && p.step > 0
        {
            if !seen_steps.contains(&p.step) {
                seen_steps.push(p.step);
            }
            if p.step == 2 && !p.last.is_empty() && !shot_taken {
                h.settle(&mut app, 2);
                shot("g2_preparing.png", [1280, 800], &h.image().unwrap());
                shot_taken = true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    assert_eq!(seen_steps, [1, 2, 3], "every step shows");
    // then it loads: this stand-in has no partition, which the start screen says in words
    let start = std::time::Instant::now();
    while app.is_loading() && start.elapsed().as_secs() < 30 {
        h.frame(&mut app, Vec::new());
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    h.settle(&mut app, 2);
    let err = app.error().unwrap_or_default().to_owned();
    assert!(err.contains("level partition not found"), "{err}");
    assert!(app.levels()[0].prepared, "listed as prepared now");
    // a failing preparation: the reason, in the script's words, on the start screen
    let root2 = temp("prepare_fail");
    stand_in(&root2, "mp_902");
    let mut app = EditorApp::new(Some(&root2), None).unwrap();
    h.settle(&mut app, 2);
    app.open_level_preparing_with(&script(&root2, true), "mp_902");
    let start = std::time::Instant::now();
    while app.preparing().is_some() && start.elapsed().as_secs() < 30 {
        h.frame(&mut app, Vec::new());
        std::thread::sleep(std::time::Duration::from_millis(30));
    }
    h.settle(&mut app, 3);
    assert_eq!(
        app.error(),
        Some("mp_902: the level bundle is damaged (a stand-in error)")
    );
    shot("g2_prepare_failed.png", [1280, 800], &h.image().unwrap());
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_dir_all(&root2);
}

/// The real thing, on a stand-in Project OPFOR BFBC2 folder in temp (the game copy linked read-only by a junction,
/// its own work folder): opening a level that was never prepared there runs the suite's prepare_level.py (NavKit's
/// prepare and the terrain unpack) with its progress, then shows the map with its terrain. Minutes, so only with
/// `BC2_EDITOR_PREPARE=<level>` (rust/BENCHMARKS.md, G2, has a run); the junction is removed before the folder.
#[test]
fn prepare_a_level_on_a_stand_in_opfor() {
    let Some(lv) = std::env::var_os("BC2_EDITOR_PREPARE").map(|v| v.to_string_lossy().to_lowercase()) else {
        println!("skipped: set BC2_EDITOR_PREPARE=<level> to prepare a level on a stand-in OPFOR folder");
        return;
    };
    let Ok(real) = bc2_editor::levels::find_opfor(None) else {
        println!("skipped: no Project OPFOR BFBC2");
        return;
    };
    let Ok(mut h) = Headless::new([1280, 800]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let root = temp("stand_in_opfor");
    let game = root.join("game");
    let made = std::process::Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&game)
        .arg(real.join("game"))
        .output()
        .unwrap();
    assert!(made.status.success(), "{}", String::from_utf8_lossy(&made.stderr));
    let cleanup = |root: &Path| {
        // the junction first (its target is the real game copy), then the stand-in's own files
        std::fs::remove_dir(root.join("game")).unwrap();
        assert!(real.join("game").is_dir());
        let _ = std::fs::remove_dir_all(root);
    };
    let mut app = EditorApp::new(Some(&root), None).unwrap();
    h.settle(&mut app, 2);
    assert!(
        app.levels().iter().any(|l| l.lv == lv && !l.prepared),
        "{lv} is listed, not prepared"
    );
    let t0 = std::time::Instant::now();
    app.open_level(&lv);
    let mut steps = Vec::new();
    let mut shot_taken = false;
    while (app.preparing().is_some() || app.is_loading()) && t0.elapsed().as_secs() < 1200 {
        h.frame(&mut app, Vec::new());
        if let Some(p) = app.preparing() {
            if p.step > 0 && !steps.contains(&p.step) {
                println!(
                    "{:>5.1} s  step {} of {}: {}",
                    t0.elapsed().as_secs_f64(),
                    p.step,
                    p.steps,
                    p.what
                );
                steps.push(p.step);
            }
            if p.step == 2 && !shot_taken {
                h.settle(&mut app, 2);
                shot(&format!("g2_preparing_{lv}.png"), [1280, 800], &h.image().unwrap());
                shot_taken = true;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    let secs = t0.elapsed().as_secs_f64();
    let ok = app.view().is_some();
    let err = app.error().map(str::to_owned);
    if ok {
        h.settle(&mut app, 20);
        shot(&format!("g2_prepared_{lv}.png"), [1280, 800], &h.image().unwrap());
    }
    let v = app
        .view()
        .map(|v| (v.scene.entities.len(), v.scene.terrain.is_some(), v.notes.clone()));
    cleanup(&root);
    assert!(ok, "{lv} did not open: {err:?}");
    let (n, terrain, notes) = v.unwrap();
    println!("{lv}: prepared and opened in {secs:.1} s - {n} entities, terrain {terrain}, notes {notes:?}");
    assert_eq!(steps, [1, 2, 3, 4]);
    assert!(n > 100 && terrain, "{notes:?}");
}
