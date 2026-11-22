//! The Gameplay workspace's tools in the running editor, headless, on the demo level (no game copy; skipped without a
//! graphics adapter): the Gameplay tool on the view adds a flag where the pointer meets the ground - a copy of the
//! level's flag with its zone and marker, the picker's team, selected - in one operation; Delete takes it with its
//! unit in one operation and Ctrl+Z brings it back; Carry picks the flag up by its marker and puts it down with its
//! zone and marker; Esc drops what is carried; a spawn point on a level without a spawner is refused in words; the
//! Gameplay panel says the demo level has no mode layer for rules. Both themes; `BC2_EDITOR_SHOTS=<folder>` saves
//! the screenshots (`gameplay_flag_<theme>.png`, `gameplay_panel_<theme>.png`).
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
use bc2_editor::gameplay::GameplayKind;
use bc2_editor::gizmo::Tool;
use bc2_editor::headless::Headless;
use bc2_editor::theme::ThemeId;
use bc2_editor::workspace::Workspace;
use bc2_editor::{demo, query};
use bc2_levelwrite::Op;
use bc2_levelwrite::files::FileEdit;
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
    let d = app.view().unwrap().doc.as_ref().unwrap();
    d.project.operations.iter().map(|o| o.op.clone()).collect()
}

/// A screen point on the terrain of the 3D view at world (x, z).
fn ground(app: &EditorApp, x: f64, z: f64) -> Pos2 {
    let v = app.view().unwrap();
    let vp = &v.viewports[&Projection::Perspective];
    let rect = vp.rect.unwrap();
    let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
    let y = query::surface_below(v.scene(), x, z).map_or(0.0, |h| h.point[1]);
    let s = view.project([x, y, z]).unwrap();
    rect.min + Vec2::new(s[0] as f32, s[1] as f32)
}

fn tool(h: &mut Headless, app: &mut EditorApp, kind: GameplayKind) {
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    app.view_mut().unwrap().tools.gameplay.kind = kind;
    app.run(&gpu, &ctx, Action::UseTool(Tool::Gameplay));
    h.settle(app, 2);
}

#[test]
// spec: XLOG-GAME-1, XLOG-GAME-2
fn gameplay_tools_through_the_interface() {
    let _gate = crate::gate();
    let mut h = match Headless::new([1500, 900]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    for theme in ThemeId::ALL {
        let dir = std::env::temp_dir().join(format!("bc2_editor_gameplay_{}_{}", theme.key(), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let scene = demo::level(&dir.join("level")).unwrap();
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        let ctx = h.ctx.clone();
        app.open_scene_then(
            &gpu,
            "demo",
            scene,
            Then::Create("Gameplay demo".into(), dir.clone(), None),
        );
        h.settle(&mut app, 6);
        app.run(&gpu, &ctx, Action::SetWorkspace(Workspace::Gameplay));
        app.run(&gpu, &ctx, Action::ShowPanel(PanelKind::Gameplay));
        h.settle(&mut app, 3);
        let flag = app
            .view()
            .unwrap()
            .doc
            .as_ref()
            .unwrap()
            .index(&demo::level_guid(5))
            .unwrap();
        let (entities, shapes) = {
            let s = app.view().unwrap().scene();
            (s.entities.len(), s.shapes.len())
        };

        // ------------------------------------------------------------------ a flag added by a click on the ground
        app.view_mut().unwrap().browser.place_team = 2;
        tool(&mut h, &mut app, GameplayKind::Flag);
        let at = ground(&app, -30.0, -5.0);
        h.hover(&mut app, at);
        h.click(&mut app, at);
        let log = ops(&app);
        assert_eq!(log.len(), 1, "{} {log:?}", app.view().unwrap().message);
        let Op::Files { edits } = &log[0] else {
            panic!("{log:?}")
        };
        let FileEdit::Partition { op } = &edits[0] else {
            panic!("{edits:?}")
        };
        let Op::PlaceUnit { item } = &**op else {
            panic!("{op:?}")
        };
        assert_eq!(item.template, demo::level_guid(5));
        assert_eq!(item.owned.len(), 5, "its ring and its marker");
        let v = app.view().unwrap();
        let new = v.selected.unwrap();
        let e = &v.scene().entities[new];
        assert_eq!(e.guid, item.guid);
        assert_eq!(e.team, Some(2), "the picker's team");
        let p = e.position().unwrap();
        assert!((p[0] + 30.0).abs() < 0.5 && (p[2] + 5.0).abs() < 0.5, "{p:?}");
        assert_eq!(v.scene().entities.len(), entities + 6);
        assert_eq!(v.scene().shapes.len(), shapes + 1, "its zone");
        assert_eq!(
            app.view().unwrap().doc.as_ref().unwrap().project.operations[0].label,
            "Add flag A"
        );
        shot(&mut h, &mut app, &format!("gameplay_flag_{}.png", theme.key()));

        // ------------------------------------------------------------------ deleted with its unit, and undone
        app.run(&gpu, &ctx, Action::Delete);
        h.settle(&mut app, 2);
        let log = ops(&app);
        assert_eq!(log.len(), 2, "{}", app.view().unwrap().message);
        let Op::Delete { guids } = &log[1] else {
            panic!("{log:?}")
        };
        assert_eq!(guids.len(), 6, "the flag, its ring and its marker");
        assert!(!app.view().unwrap().doc.as_ref().unwrap().shown(new));
        h.key(&mut app, Key::Z, CTRL);
        assert_eq!(ops(&app).len(), 1, "undone");
        assert!(app.view().unwrap().doc.as_ref().unwrap().shown(new));

        // ------------------------------------------------------------------ the old flag carried by its marker
        tool(&mut h, &mut app, GameplayKind::Carry);
        let at = app.project_entity(flag).unwrap();
        h.click(&mut app, at);
        assert!(
            app.view().unwrap().tools.gameplay.carrying.is_some(),
            "{}",
            app.view().unwrap().message
        );
        assert_eq!(app.view().unwrap().selected, Some(flag));
        let to = ground(&app, 40.0, 30.0);
        h.hover(&mut app, to);
        h.click(&mut app, to);
        let log = ops(&app);
        assert_eq!(log.len(), 2, "{}", app.view().unwrap().message);
        let Op::Move { items } = &log[1] else { panic!("{log:?}") };
        assert_eq!(items.len(), 6, "the flag with its ring and marker");
        let p = app.view().unwrap().scene().entities[flag].position().unwrap();
        assert!((p[0] - 40.0).abs() < 0.5 && (p[2] - 30.0).abs() < 0.5, "{p:?}");
        assert!(app.view().unwrap().tools.gameplay.carrying.is_none());
        // Esc drops what is carried
        let at = app.project_entity(flag).unwrap();
        h.click(&mut app, at);
        assert!(app.view().unwrap().tools.gameplay.carrying.is_some());
        h.key(&mut app, Key::Escape, Modifiers::NONE);
        assert!(app.view().unwrap().tools.gameplay.carrying.is_none());
        assert_eq!(ops(&app).len(), 2);

        // ------------------------------------------------------------------ a spawn point: refused in words
        tool(&mut h, &mut app, GameplayKind::SpawnPoint);
        let at = ground(&app, 0.0, 30.0);
        h.click(&mut app, at);
        assert_eq!(ops(&app).len(), 2);
        let msg = app.view().unwrap().message.clone();
        assert!(msg.contains("no soldier spawner"), "{msg}");

        // the panel: no mode layer on the demo level for rules
        h.settle(&mut app, 2);
        shot(&mut h, &mut app, &format!("gameplay_panel_{}.png", theme.key()));
        assert!(bc2_editor::gameplay::rules(app.view().unwrap().doc.as_ref().unwrap()).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
