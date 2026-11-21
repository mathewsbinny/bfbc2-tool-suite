//! Round R5d in the editor's game-accurate view, on MP_005 headless: the weather's sky dome is drawn behind the
//! level, and a label whose point lies behind what the game frame drew is hidden ([`GameView::hidden`], spec
//! RNDR-VIEW-1): a point under the ground at the view's target is hidden, one above it is not, a gameplay label at
//! the far side of a hill is left out.
//!
//! Skipped (with a note) without the game copy, MP_005 prepared, a graphics adapter or Direct3D 11.
//! `BC2_EDITOR_SHOTS=<folder>` saves the view in both themes.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::many_single_char_names,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    missing_docs
)]

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::theme::ThemeId;

fn harbour_camera() -> Camera {
    Camera {
        target: [159.7, 70.0, 353.9],
        yaw: 3.6,
        pitch: 0.32,
        distance: 75.0,
        fov: 60.0,
        projection: Projection::Perspective,
    }
}

// spec: RNDR-VIEW-1, RNDR-SKY-1
#[test]
#[ignore = "game tier: a whole level in the game view; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn game_view_draws_the_sky_and_hides_labels_behind_the_frame() {
    let _gate = crate::gate();
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
    let cam = harbour_camera();
    {
        let v = app.view_mut().unwrap();
        v.labels = true;
        v.viewports.get_mut(&Projection::Perspective).unwrap().camera = cam;
    }
    h.settle(&mut app, 4);
    let ctx = h.ctx.clone();
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    if app.view().unwrap().game_view.game.is_none() {
        println!("skipped: {}", app.view().unwrap().message);
        return;
    }
    h.settle(&mut app, 4);
    let shots = std::env::var_os("BC2_EDITOR_SHOTS").map(std::path::PathBuf::from);
    if let Some(d) = &shots {
        h.save(&d.join("r5d_game_view_dark.png")).unwrap();
    }
    let v = app.view().unwrap();
    let g = v.game_view.game.as_ref().unwrap();
    assert!(g.error.is_none(), "{:?}", g.error);
    assert!(
        g.world().sky_pass && g.world().sky.is_some(),
        "the sky dome: {:?}",
        g.notes
    );
    let t = cam.target;
    let under = [t[0], t[1] - 40.0, t[2]];
    let above = [t[0], t[1] + 30.0, t[2]];
    assert!(g.hidden(under), "a point 40 m under the target is behind the frame");
    assert!(!g.hidden(above), "a point 30 m over the target is seen");
    // the gameplay labels the view would draw: those behind the frame's depth are left out
    let (mut seen, mut hidden) = (0, 0);
    for e in &v.scene().entities {
        if !matches!(
            e.category.as_str(),
            "soldier spawn" | "vehicle spawn" | "capture point" | "objective"
        ) {
            continue;
        }
        let Some(p) = e.position() else { continue };
        if g.hidden([p[0], p[1] + 2.5, p[2]]) {
            hidden += 1;
        } else {
            seen += 1;
        }
    }
    println!("mp_005 harbour: {seen} gameplay labels seen, {hidden} behind the game frame");
    assert!(
        hidden > 0,
        "some gameplay labels lie behind the harbour's buildings and hills"
    );
    app.theme = ThemeId::Light;
    h.settle(&mut app, 4);
    if let Some(d) = &shots {
        h.save(&d.join("r5d_game_view_light.png")).unwrap();
    }
    app.theme = ThemeId::Dark;
}
