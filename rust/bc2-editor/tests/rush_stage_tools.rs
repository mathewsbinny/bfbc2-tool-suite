//! Round E4-rest's gameplay tools in the running editor, headless (skipped without a graphics adapter), in both
//! themes: on the demo level (no game copy) a flag renamed from the Gameplay panel in DICE's form, a spawner and a Rush
//! stage refused in words where the level has neither; on Port Valdez Rush (`MP_012GR`, read from the game copy, skipped
//! without it) a fifth stage added by a click on the ground with the Gameplay tool (one operation, the stages listed
//! five), taken out from the panel's "Remove stage" (one operation), undone and redone, and the second stage taken
//! out; then a stage inserted after the first from the panel's "Insert after" and a click (round E4-rest2).
//! `BC2_EDITOR_SHOTS=<folder>` saves the screenshots (`gameplay_rename_<theme>.png`, `gameplay_stage_<theme>.png`,
//! `gameplay_insert_<theme>.png`).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::assert_is_empty,
    missing_docs
)]

use bc2_editor::actions::{Action, PanelKind};
use bc2_editor::app::{EditorApp, PanelRequest, Then};
use bc2_editor::camera::Projection;
use bc2_editor::gameplay::{self, GameplayKind};
use bc2_editor::gizmo::Tool;
use bc2_editor::headless::Headless;
use bc2_editor::theme::ThemeId;
use bc2_editor::workspace::Workspace;
use bc2_editor::{demo, levels, query};
use bc2_levelwrite::Op;
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

fn labels(app: &EditorApp) -> Vec<String> {
    let d = app.view().unwrap().doc.as_ref().unwrap();
    d.project.operations.iter().map(|o| o.label.clone()).collect()
}

fn stages(app: &EditorApp) -> usize {
    gameplay::rush_stages(app.view().unwrap().doc.as_ref().unwrap()).len()
}

/// A screen point on the ground of the 3D view at world (x, z).
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

fn gameplay_workspace(h: &mut Headless, app: &mut EditorApp) {
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    app.run(&gpu, &ctx, Action::SetWorkspace(Workspace::Gameplay));
    app.run(&gpu, &ctx, Action::ShowPanel(PanelKind::Gameplay));
    h.settle(app, 3);
}

/// The demo level: the flag renamed from the panel ("Harbor" becomes DICE's "<letter>: Harbor" where the flag has
/// a letter), a spawner and a Rush stage refused in words.
// spec: GAME-CQ-12
#[test]
fn flag_names_and_refusals_on_the_demo_level() {
    let _gate = crate::gate();
    let mut h = match Headless::new([1500, 900]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    for theme in ThemeId::ALL {
        let dir = std::env::temp_dir().join(format!("bc2_editor_e4rest_{}_{}", theme.key(), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let scene = demo::level(&dir.join("level")).unwrap();
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        app.open_scene_then(
            &gpu,
            "demo",
            scene,
            Then::Create("Rename demo".into(), dir.clone(), None),
        );
        h.settle(&mut app, 6);
        gameplay_workspace(&mut h, &mut app);
        let flag = demo::level_guid(5);
        let k = app.view().unwrap().doc.as_ref().unwrap().index(&flag).unwrap();
        app.select(&gpu, Some(k));
        h.settle(&mut app, 2);
        app.view_mut().unwrap().gameplay.request = Some(PanelRequest::Rename(flag.clone(), "Harbor".into()));
        h.settle(&mut app, 3);
        let log = ops(&app);
        assert_eq!(log.len(), 1, "{}", app.view().unwrap().message);
        let Op::SetField { items } = &log[0] else {
            panic!("{log:?}")
        };
        assert!(items.iter().all(|i| i.to.text().ends_with("Harbor")), "{items:?}");
        assert_eq!(labels(&app), ["Rename flag"]);
        shot(&mut h, &mut app, &format!("gameplay_rename_{}.png", theme.key()));
        h.key(&mut app, Key::Z, CTRL);
        assert!(ops(&app).is_empty(), "undone");
        // a spawner and a stage: refused in words
        tool(&mut h, &mut app, GameplayKind::Spawner);
        let at = ground(&app, 0.0, 30.0);
        h.click(&mut app, at);
        assert!(ops(&app).is_empty());
        let msg = app.view().unwrap().message.clone();
        assert!(msg.contains("no soldier spawner"), "{msg}");
        tool(&mut h, &mut app, GameplayKind::Stage);
        h.click(&mut app, at);
        assert!(ops(&app).is_empty());
        let msg = app.view().unwrap().message.clone();
        assert!(msg.contains("no Rush stages"), "{msg}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Port Valdez Rush in the editor: a stage added after the last by a click (the copy of stage 4 where the click is,
/// one operation labelled "Add Rush stage 5"), taken out with the panel's "Remove stage", undone and redone; then the
/// second stage taken out. Both themes.
// spec: GAME-RUSH-11
// spec: XLOG-GAME-3
#[test]
fn a_rush_stage_through_the_interface() {
    let _gate = crate::gate();
    let Ok(_root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let mut h = match Headless::new([1500, 900]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    for theme in ThemeId::ALL {
        let dir = std::env::temp_dir().join(format!("bc2_editor_stage_{}_{}", theme.key(), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        if let Err(e) = app.open_level_blocking_then(
            &gpu,
            "mp_012gr",
            Then::Create("Port Valdez stages".into(), dir.clone(), None),
        ) {
            println!("skipped: {e}");
            return;
        }
        h.settle(&mut app, 4);
        gameplay_workspace(&mut h, &mut app);
        assert_eq!(stages(&app), 4);
        // the last stage's first M-COM in view, the click 15 m beside it
        let (k, p) = {
            let d = app.view().unwrap().doc.as_ref().unwrap();
            let last = gameplay::rush_stages(d).pop().unwrap();
            let k = d.index(&last[0]).unwrap();
            (k, d.scene.entities[k].position().unwrap())
        };
        app.select(&gpu, Some(k));
        app.focus_selected();
        h.settle(&mut app, 40);
        tool(&mut h, &mut app, GameplayKind::Stage);
        // the ground a little beside the framed M-COM (the view's middle)
        let rect = app.view().unwrap().viewports[&Projection::Perspective].rect.unwrap();
        let at = rect.center() + Vec2::new(rect.width() * 0.15, rect.height() * 0.1);
        println!("click at {at:?} in {rect:?}, the M-COM at {p:?}");
        h.hover(&mut app, at);
        h.settle(&mut app, 2);
        h.click(&mut app, at);
        h.settle(&mut app, 3);
        assert_eq!(ops(&app).len(), 1, "{}", app.view().unwrap().message);
        assert_eq!(labels(&app), ["Add Rush stage 5"]);
        assert_eq!(stages(&app), 5);
        shot(&mut h, &mut app, &format!("gameplay_stage_{}.png", theme.key()));
        // taken out from the panel
        app.view_mut().unwrap().gameplay.request = Some(PanelRequest::RemoveStage(4));
        h.settle(&mut app, 3);
        assert_eq!(labels(&app), ["Add Rush stage 5", "Remove Rush stage 5"]);
        assert_eq!(stages(&app), 4);
        h.key(&mut app, Key::Z, CTRL);
        assert_eq!(stages(&app), 5, "undone");
        h.key(&mut app, Key::Y, CTRL);
        assert_eq!(stages(&app), 4, "redone");
        app.view_mut().unwrap().gameplay.request = Some(PanelRequest::RemoveStage(1));
        h.settle(&mut app, 3);
        assert_eq!(stages(&app), 3, "{}", app.view().unwrap().message);
        // round E4-rest2: a stage inserted between stages 1 and 2 by the panel's "Insert after" and a click (a copy
        // of stage 2 played second, one operation), undone
        let before: Vec<Vec<String>> = gameplay::rush_stages(app.view().unwrap().doc.as_ref().unwrap());
        app.view_mut().unwrap().gameplay.request = Some(PanelRequest::InsertAfter(0));
        h.settle(&mut app, 3);
        assert_eq!(app.view().unwrap().tool, Tool::Gameplay);
        assert_eq!(app.view().unwrap().tools.gameplay.kind, GameplayKind::InsertStage(0));
        h.hover(&mut app, at);
        h.settle(&mut app, 2);
        h.click(&mut app, at);
        h.settle(&mut app, 3);
        assert_eq!(
            labels(&app).last().map(String::as_str),
            Some("Insert Rush stage 2"),
            "{}",
            app.view().unwrap().message
        );
        let now: Vec<Vec<String>> = gameplay::rush_stages(app.view().unwrap().doc.as_ref().unwrap());
        assert_eq!(now.len(), 4);
        assert_eq!(now[0], before[0], "stage 1 as before");
        assert_eq!(&now[2..], &before[1..], "the stages after it as before, one later");
        assert!(
            now[1].iter().all(|g| !before.iter().flatten().any(|b| b == g)),
            "the new stage's M-COMs are copies"
        );
        shot(&mut h, &mut app, &format!("gameplay_insert_{}.png", theme.key()));
        h.key(&mut app, Key::Z, CTRL);
        assert_eq!(stages(&app), 3, "undone");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
