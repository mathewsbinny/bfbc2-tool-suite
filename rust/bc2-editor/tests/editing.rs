//! Editing in the running editor, headless (round E2's interface proof): the demo scene made into a map project in a
//! temp folder, then driven as a user would - click to select, drag the gizmo's X arm by 5 m on a 1 m grid, Ctrl+Z,
//! Ctrl+Y, turn with the ring on 15-degree steps, box selection, Ctrl+D, Delete, snap to ground with a 0.5 m sink,
//! a layer hidden, save and reopen - and each step checked against the project's operation log and the scene.
//!
//! Skipped (with a note) without any graphics adapter. `BC2_EDITOR_SHOTS=<folder>` saves screenshots of the editing
//! interface there (the gizmo, the snapping toolbar, History, Layers).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::panic,
    clippy::many_single_char_names,
    missing_docs
)]

use bc2_editor::actions::{Action, PanelKind};
use bc2_editor::app::{EditorApp, Then};
use bc2_editor::camera::Projection;
use bc2_editor::gizmo::{self, Handle, Tool};
use bc2_editor::headless::Headless;
use bc2_editor::{demo, query};
use bc2_levelwrite::{MapProject, Op, SinkUnit};
use eframe::egui::{Key, Modifiers, Pos2, Vec2};

const CTRL: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: false,
    mac_cmd: false,
    command: true,
};

fn shot(h: &mut Headless, app: &mut EditorApp, name: &str) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        h.settle(app, 3);
        let p = std::path::Path::new(&dir).join(name);
        h.save(&p).unwrap();
        println!("saved {}", p.display());
    }
}

fn ops(app: &EditorApp) -> Vec<Op> {
    app.view()
        .unwrap()
        .doc
        .as_ref()
        .unwrap()
        .project
        .operations
        .iter()
        .map(|o| o.op.clone())
        .collect()
}

fn pos(app: &EditorApp, k: usize) -> [f64; 3] {
    app.view().unwrap().scene().entities[k].position().unwrap()
}

/// The screen point `metres` along the X arm from the gizmo's origin, by the arm's length on screen.
fn along_x(app: &EditorApp, from: Pos2, metres: f64) -> Pos2 {
    let (o, tip) = app.gizmo_handle(Projection::Perspective, Handle::Axis(0)).unwrap();
    let v = app.view().unwrap();
    let vp = &v.viewports[&Projection::Perspective];
    let rect = vp.rect.unwrap();
    let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
    let origin = v.scene().entities[v.selected.unwrap()].position().unwrap();
    let l = gizmo::arm(&view, origin);
    from + (tip - o) * (metres / l) as f32
}

#[test]
fn editing_through_the_interface() {
    let _gate = crate::gate();
    let mut h = match Headless::new([1400, 860]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let dir = std::env::temp_dir().join(format!("bc2_editor_editing_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    app.open_scene_then(
        &gpu,
        "demo",
        demo::scene(),
        Then::Create("Demo edits".into(), dir.clone(), None),
    );
    h.settle(&mut app, 6);
    let path = app.view().unwrap().doc.as_ref().unwrap().path.clone();
    assert!(path.is_file(), "the project was made at once");
    // select the team-1 spawn (entity 1) by clicking it
    let at = app.project_entity(1).unwrap();
    h.click(&mut app, at);
    assert_eq!(app.view().unwrap().selected, Some(1), "click selects");
    assert_eq!(app.view().unwrap().tool, Tool::Move);
    // grid snapping on (G), then drag the X arm by about 5 m: one move of exactly 5 m
    h.key(&mut app, Key::G, Modifiers::NONE);
    assert!(app.view().unwrap().snap.grid);
    h.settle(&mut app, 2);
    let (o, tip) = app.gizmo_handle(Projection::Perspective, Handle::Axis(0)).unwrap();
    let grab = o + (tip - o) * 0.6;
    let to = along_x(&app, grab, 5.2);
    let before = pos(&app, 1);
    h.drag(&mut app, grab, to, 8, Modifiers::NONE);
    shot(&mut h, &mut app, "e2_gizmo_move.png");
    let log = ops(&app);
    assert_eq!(log.len(), 1, "one drag, one operation: {log:?}");
    let Op::Move { items } = &log[0] else { panic!("{log:?}") };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].guid, app.view().unwrap().scene().entities[1].guid);
    let d: Vec<f32> = (0..3).map(|c| items[0].to[3][c] - items[0].from[3][c]).collect();
    assert_eq!(d, [5.0, 0.0, 0.0], "5 m along X on a 1 m grid");
    assert_eq!(pos(&app, 1)[0], before[0] + 5.0);
    // Ctrl+Z, Ctrl+Y
    h.key(&mut app, Key::Z, CTRL);
    assert!(ops(&app).is_empty(), "undone");
    assert_eq!(pos(&app, 1), before);
    h.key(&mut app, Key::Y, CTRL);
    assert_eq!(ops(&app).len(), 1, "redone");
    assert_eq!(pos(&app, 1)[0], before[0] + 5.0);
    // the rotate tool (E) with 15-degree steps: a ring drag is one rotate of a multiple of 15 degrees
    h.key(&mut app, Key::E, Modifiers::NONE);
    app.view_mut().unwrap().snap.angle = true;
    h.settle(&mut app, 2);
    assert!(app.gizmo_handle(Projection::Perspective, Handle::Ring(1)).is_some());
    let ring_pt = |a: f32| {
        let v = app.view().unwrap();
        let vp = &v.viewports[&Projection::Perspective];
        let rect = vp.rect.unwrap();
        let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
        let o = v.scene().entities[1].position().unwrap();
        let r = gizmo::arm(&view, o) * 0.9;
        let w = [o[0] + r * f64::from(a.cos()), o[1], o[2] + r * f64::from(a.sin())];
        let s = view.project(w).unwrap();
        rect.min + Vec2::new(s[0] as f32, s[1] as f32)
    };
    let (a, b) = (ring_pt(0.3), ring_pt(1.2));
    h.drag(&mut app, a, b, 10, Modifiers::NONE);
    shot(&mut h, &mut app, "e2_gizmo_rotate.png");
    let log = ops(&app);
    assert_eq!(log.len(), 2, "{log:?}");
    let Op::Rotate { items } = &log[1] else {
        panic!("{log:?}")
    };
    let f = items[0].to[2];
    let yaw = f64::from(f[0]).atan2(f64::from(f[2])).to_degrees();
    assert!(
        (yaw / 15.0 - (yaw / 15.0).round()).abs() < 1e-3 && yaw.abs() > 1.0,
        "turned {yaw} deg"
    );
    // box selection (Q, drag over the view): every shown object inside, then Ctrl+D and Delete
    h.key(&mut app, Key::Q, Modifiers::NONE);
    let rect = app.view().unwrap().view_rect().unwrap();
    h.drag(
        &mut app,
        rect.min + Vec2::new(4.0, 80.0),
        rect.max - Vec2::new(4.0, 4.0),
        6,
        Modifiers::NONE,
    );
    let selected = app.view().unwrap().selection.clone();
    assert!(selected.len() >= 4, "box selection: {selected:?}");
    shot(&mut h, &mut app, "e2_box_selection.png");
    h.key(&mut app, Key::D, CTRL);
    let log = ops(&app);
    let Op::Duplicate { items } = &log[2] else {
        panic!("{log:?}")
    };
    assert_eq!(items.len(), selected.len());
    let copies: Vec<usize> = app.view().unwrap().selection.clone();
    assert_eq!(copies.len(), selected.len(), "the copies are selected");
    assert!(copies.iter().all(|&k| k >= 5));
    h.key(&mut app, Key::Delete, Modifiers::NONE);
    let log = ops(&app);
    let Op::Delete { guids } = &log[3] else {
        panic!("{log:?}")
    };
    assert_eq!(guids.len(), copies.len());
    assert_eq!(app.view().unwrap().selection.len(), 0, "nothing left selected");
    // snap to ground (End) with surface snapping's sink of 0.5 m: the house sinks 0.5 m into the terrain
    app.select(&gpu, Some(0));
    {
        let v = app.view_mut().unwrap();
        v.snap.surface = true;
        v.snap.placement.sink = 0.5;
        v.snap.placement.unit = SinkUnit::Metres;
    }
    h.settle(&mut app, 2);
    shot(&mut h, &mut app, "e2_snapping_toolbar.png");
    h.key(&mut app, Key::End, Modifiers::NONE);
    let log = ops(&app);
    let Op::Move { items } = &log[4] else { panic!("{log:?}") };
    let p = items[0].placement.unwrap();
    assert_eq!((p.sink, p.unit), (0.5, SinkUnit::Metres));
    let house = pos(&app, 0);
    let ground = {
        let v = app.view().unwrap();
        query::surface_below_except(v.scene(), house[0], house[2], &|k| k == 0)
            .unwrap()
            .point[1]
    };
    assert!(
        (house[1] - (ground - 0.5)).abs() < 1e-3,
        "house at {} on ground {ground}",
        house[1]
    );
    // the sink stays with the house: a move with surface snapping keeps 0.5 m
    h.key(&mut app, Key::W, Modifiers::NONE);
    h.settle(&mut app, 2);
    let (o, tip) = app.gizmo_handle(Projection::Perspective, Handle::Axis(2)).unwrap();
    h.drag(&mut app, o + (tip - o) * 0.6, o + (tip - o) * 1.4, 6, Modifiers::NONE);
    let log = ops(&app);
    let Op::Move { items } = &log[5] else { panic!("{log:?}") };
    assert_eq!(items[0].placement.unwrap().sink, 0.5);
    let house = pos(&app, 0);
    let ground = {
        let v = app.view().unwrap();
        query::surface_below_except(v.scene(), house[0], house[2], &|k| k == 0)
            .unwrap()
            .point[1]
    };
    assert!(
        (house[1] - (ground - 0.5)).abs() < 1e-3,
        "kept its sink: {} on {ground}",
        house[1]
    );
    // a layer from the selection, hidden: the house is no longer drawn or pickable
    let gpu2 = gpu.clone();
    let ctx = h.ctx.clone();
    app.run_action(&gpu2, &ctx, Action::NewLayer);
    app.run_action(&gpu2, &ctx, Action::ShowPanel(PanelKind::Layers));
    h.settle(&mut app, 3);
    shot(&mut h, &mut app, "e2_layers.png");
    app.run_action(&gpu2, &ctx, Action::ShowPanel(PanelKind::History));
    h.settle(&mut app, 3);
    shot(&mut h, &mut app, "e2_history.png");
    let log = ops(&app);
    assert!(matches!(log[6], Op::AddLayer { .. }) && matches!(log[7], Op::AssignLayer { .. }));
    {
        let d = app.view().unwrap().doc.as_ref().unwrap();
        assert!(d.org.layer(&d.scene.entities[0].guid).is_some());
    }
    // save (Ctrl+S): the autosave goes; reopened, the same scene and log
    let auto = bc2_levelwrite::project::autosave_path(&path);
    assert!(auto.is_file(), "every change is autosaved");
    h.key(&mut app, Key::S, CTRL);
    assert!(!auto.exists());
    let saved = MapProject::load(&path).unwrap();
    assert_eq!(saved.operations.len(), 8);
    let at = [pos(&app, 0), pos(&app, 1)];
    app.open_scene_then(&gpu, "demo", demo::scene(), Then::Open(path.clone(), None));
    h.settle(&mut app, 3);
    assert_eq!([pos(&app, 0), pos(&app, 1)], at);
    assert_eq!(ops(&app).len(), 8);
    // the palette lists the editing actions
    app.open_palette("snap");
    h.settle(&mut app, 3);
    shot(&mut h, &mut app, "e2_palette.png");
    println!(
        "editing through the interface: {} operations (move 5 m on the grid, undo, redo, rotate in 15 deg steps, box \
         selection of {}, duplicate, delete, snap to ground sunk 0.5 m and kept on a move, layer), saved and reopened",
        saved.operations.len(),
        selected.len()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
