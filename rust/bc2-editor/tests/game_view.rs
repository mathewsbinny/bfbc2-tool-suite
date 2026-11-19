//! The 3D view's game-accurate mode (round R5, `EDITOR_ARCHITECTURE.md` 3.11) on MP_005: the level opened headless,
//! the camera on the harbour's warehouses, the view switched to "Game-accurate" - the placed objects and the terrain
//! patches drawn with DICE's own Direct3D 11 shaders off screen - and back. The frame is drawn, it is not one flat
//! colour, the editor's view differs from it, and switching back shows the editor's view again. Per-frame costs are
//! printed (`--nocapture`).
//!
//! Skipped (with a note) without the game copy, MP_005 prepared, a graphics adapter or Direct3D 11.
//! `BC2_EDITOR_SHOTS=<folder>` saves the editor's and the game-accurate view in both themes.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::chunks_exact_to_as_chunks,
    missing_docs
)]

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::theme::ThemeId;

/// MP_005's first warehouse (`objects/buildings/ind_warehouse`), looked at from its south-west, a little above.
fn warehouse_camera() -> Camera {
    Camera {
        target: [159.7, 70.0, 353.9],
        yaw: 3.6,
        pitch: 0.32,
        distance: 75.0,
        fov: 60.0,
        projection: Projection::Perspective,
    }
}

/// MP_005's west shore: the sea, the shore and the camp above it, looked at from the east.
fn harbour_camera() -> Camera {
    Camera {
        target: [-90.0, 60.0, 300.0],
        yaw: std::f64::consts::FRAC_PI_2,
        pitch: 0.35,
        distance: 120.0,
        fov: 60.0,
        projection: Projection::Perspective,
    }
}

fn spread(img: &[u8]) -> f64 {
    let lum: Vec<f64> = img
        .chunks_exact(4)
        .map(|p| 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]))
        .collect();
    let mean = lum.iter().sum::<f64>() / lum.len() as f64;
    (lum.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / lum.len() as f64).sqrt()
}

#[test]
fn game_accurate_view_of_mp_005() {
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new([1600, 950]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    if let Err(e) = app.open_level_blocking(&gpu, "mp_005") {
        println!("skipped: {e}");
        return;
    }
    {
        let v = app.view_mut().unwrap();
        v.labels = false;
        v.viewports.get_mut(&Projection::Perspective).unwrap().camera = warehouse_camera();
    }
    h.settle(&mut app, 4);
    let shots = std::env::var_os("BC2_EDITOR_SHOTS").map(std::path::PathBuf::from);
    let save = |h: &mut Headless, name: &str| {
        if let Some(d) = &shots {
            h.save(&d.join(name)).unwrap();
        }
    };
    save(&mut h, "r5_editor_view_dark.png");
    let editor = h.image().unwrap();
    let ctx = h.ctx.clone();
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    let loaded = app.view().unwrap();
    if loaded.game.is_none() {
        println!("skipped: {}", loaded.message);
        return;
    }
    let g = loaded.game.as_ref().unwrap();
    println!("loaded in {:.2} s: {}", g.load_seconds, g.notes.last().unwrap());
    assert!(
        g.object_count() > 100,
        "MP_005's placed objects are drawn: {}",
        g.object_count()
    );
    assert!(loaded.game_accurate);
    h.settle(&mut app, 4);
    {
        let g = app.view().unwrap().game.as_ref().unwrap();
        assert!(g.error.is_none(), "{:?}", g.error);
        assert!(
            g.timings.draws > 1000,
            "objects and terrain patches: {} draws",
            g.timings.draws
        );
    }
    save(&mut h, "r5_game_view_dark.png");
    let game = h.image().unwrap();
    assert!(spread(&game) > 10.0, "the game-accurate frame is not one colour");
    let changed = editor
        .chunks_exact(4)
        .zip(game.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    assert!(
        changed > editor.len() / 4 / 8,
        "the view shows the game-accurate frame: {changed} pixels changed"
    );
    // per-frame costs: the camera turned a little each frame, so every frame is drawn again
    let mut costs = Vec::new();
    for k in 0..12 {
        app.view_mut()
            .unwrap()
            .viewports
            .get_mut(&Projection::Perspective)
            .unwrap()
            .camera
            .yaw += 0.002 * f64::from(k + 1);
        h.settle(&mut app, 1);
        costs.push(app.view().unwrap().game.as_ref().unwrap().timings);
    }
    let n = costs.len() as f64;
    let avg = |f: fn(&bc2_editor::gameview::Timings) -> f64| costs.iter().skip(2).map(f).sum::<f64>() / (n - 2.0) * 1e3;
    println!(
        "{} draws at 1600 x 950 (less the panels): frame {:.1} ms = build {:.1} + submit {:.1} + GPU and read back {:.1} \
         + tonemap {:.1} + upload {:.1}",
        costs.last().unwrap().draws,
        avg(bc2_editor::gameview::Timings::total),
        avg(|t| t.build),
        avg(|t| t.submit),
        avg(|t| t.readback),
        avg(|t| t.tonemap),
        avg(|t| t.upload),
    );
    // round R5b: the harbour - the water, the shore, the group members - in both themes
    let set_camera = |app: &mut EditorApp, c: Camera| {
        app.view_mut()
            .unwrap()
            .viewports
            .get_mut(&Projection::Perspective)
            .unwrap()
            .camera = c;
    };
    set_camera(&mut app, harbour_camera());
    h.settle(&mut app, 4);
    save(&mut h, "r5b_mp_005_harbour_dark.png");
    let harbour = h.image().unwrap();
    assert!(spread(&harbour) > 10.0, "the harbour frame is not one colour");
    {
        let g = app.view().unwrap().game.as_ref().unwrap();
        assert!(g.error.is_none(), "{:?}", g.error);
        let c = g.world().counts;
        assert_eq!(c.water, 2, "the harbour's water and wakes: {c:?}");
        assert!(c.members > 2000 && c.overridden > 200, "{c:?}");
    }
    app.theme = ThemeId::Light;
    h.settle(&mut app, 4);
    save(&mut h, "r5b_mp_005_harbour_light.png");
    set_camera(&mut app, warehouse_camera());
    h.settle(&mut app, 4);
    save(&mut h, "r5_game_view_light.png");
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    h.settle(&mut app, 4);
    save(&mut h, "r5_editor_view_light.png");
    assert!(!app.view().unwrap().game_accurate);
    app.theme = ThemeId::Dark;
    h.settle(&mut app, 4);
    let back = h.image().unwrap();
    let same = editor
        .chunks_exact(4)
        .zip(back.chunks_exact(4))
        .filter(|(a, b)| a == b)
        .count();
    assert!(
        same > editor.len() / 4 * 9 / 10,
        "the editor's view again: {same} pixels as before"
    );
}
