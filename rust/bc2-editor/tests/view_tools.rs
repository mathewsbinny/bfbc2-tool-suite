//! The tools moved onto `ViewTool` by round E4-rest2 in the running editor, headless (skipped without a graphics
//! adapter), on the demo level (no game copy), in both themes: placing from the content browser is the views' tool
//! while it lasts (the object follows the pointer, Esc drops it, a click places it, the tool before comes back); the
//! AI workspace's navigation edit tools are the views' tool (a seed by a click, a route edge's first point dropped by
//! Esc, another tool chosen turns them off). The frames' times while each is used are measured against a frame's
//! budget (16 ms over the same frames with the Select tool).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::cast_precision_loss,
    clippy::assert_is_empty,
    missing_docs
)]

use bc2_editor::actions::{Action, PanelKind};
use bc2_editor::app::{EditorApp, Then};
use bc2_editor::camera::Projection;
use bc2_editor::gizmo::Tool;
use bc2_editor::headless::Headless;
use bc2_editor::nav::NavTool;
use bc2_editor::theme::ThemeId;
use bc2_editor::workspace::Workspace;
use bc2_editor::{demo, query};
use bc2_levelwrite::Op;
use eframe::egui::{Key, Modifiers, Pos2, Vec2};

fn ops(app: &EditorApp) -> Vec<Op> {
    let d = app.view().unwrap().doc.as_ref().unwrap();
    d.project.operations.iter().map(|o| o.op.clone()).collect()
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

/// The median time of `n` frames with the pointer moving over the ground near `at` (ms).
fn frames(h: &mut Headless, app: &mut EditorApp, at: Pos2, n: usize) -> f64 {
    let mut t: Vec<f64> = (0..n)
        .map(|k| {
            let p = at + Vec2::new((k % 7) as f32 * 3.0, (k % 5) as f32 * 2.0);
            let s = std::time::Instant::now();
            h.hover(app, p);
            s.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    t.sort_by(f64::total_cmp);
    t[t.len() / 2]
}

#[test]
fn placing_and_the_navigation_tools_are_view_tools() {
    let _gate = crate::gate();
    let mut h = match Headless::new([1500, 900]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    for theme in ThemeId::ALL {
        let dir = std::env::temp_dir().join(format!("bc2_editor_view_tools_{}_{}", theme.key(), std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let scene = demo::level(&dir.join("level")).unwrap();
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
        app.open_scene_then(
            &gpu,
            "demo",
            scene,
            Then::Create("View tools".into(), dir.clone(), None),
        );
        h.settle(&mut app, 6);
        app.run(&gpu, &ctx, Action::SetWorkspace(Workspace::Objects));
        app.run(&gpu, &ctx, Action::ShowPanel(PanelKind::Content));
        h.settle(&mut app, 4);
        let at = ground(&app, -30.0, 25.0);
        let select = frames(&mut h, &mut app, at, 40);
        // ------------------------------------------------------------------ placing
        let entry = app
            .view()
            .unwrap()
            .browser
            .content
            .as_ref()
            .and_then(|c| c.by_path(demo::LEVEL_BLUEPRINTS[1].0).cloned())
            .expect("the crate in the content browser");
        let before = app.view().unwrap().tool;
        app.view_mut().unwrap().start_placing(entry.clone());
        assert_eq!(app.view().unwrap().tool, Tool::Place, "placing is the views' tool");
        let placing = frames(&mut h, &mut app, at, 40);
        assert!(
            app.view().unwrap().tools.place.placing.as_ref().unwrap().at.is_some(),
            "it follows the pointer"
        );
        h.key(&mut app, Key::Escape, Modifiers::NONE);
        assert!(app.view().unwrap().tools.place.placing.is_none());
        assert_eq!(app.view().unwrap().tool, before, "the tool before is back");
        assert_eq!(app.view().unwrap().message, "Placing cancelled");
        app.run(&gpu, &ctx, Action::UseTool(Tool::Rotate));
        app.view_mut().unwrap().start_placing(entry);
        h.hover(&mut app, at);
        let s = std::time::Instant::now();
        h.click(&mut app, at);
        let place_click = s.elapsed().as_secs_f64() * 1000.0;
        assert!(matches!(ops(&app).last(), Some(Op::Place { .. })), "{:?}", ops(&app));
        assert_eq!(
            app.view().unwrap().tool,
            Tool::Rotate,
            "the tool before (Rotate) is back"
        );
        // ------------------------------------------------------------------ the navigation tools
        app.run(&gpu, &ctx, Action::SetWorkspace(Workspace::Ai));
        h.settle(&mut app, 3);
        app.run(&gpu, &ctx, Action::NavTool(NavTool::Seed));
        assert_eq!(app.view().unwrap().tool, Tool::Nav);
        assert_eq!(app.view().unwrap().nav_tool(), Some(NavTool::Seed));
        let nav = frames(&mut h, &mut app, at, 40);
        let n = app.view().unwrap().nav_edits().len();
        let s = std::time::Instant::now();
        h.click(&mut app, at);
        let seed_click = s.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(
            app.view().unwrap().nav_edits().len(),
            n + 1,
            "a seed: {}",
            app.view().unwrap().navs.message
        );
        assert_eq!(app.view().unwrap().nav_edits().last().unwrap().op, "seed");
        // a route edge's first point, dropped by Esc
        app.run(&gpu, &ctx, Action::NavTool(NavTool::EdgeAdd));
        h.click(&mut app, at);
        assert_eq!(app.view().unwrap().tools.nav.clicks.len(), 1);
        h.key(&mut app, Key::Escape, Modifiers::NONE);
        assert!(app.view().unwrap().tools.nav.clicks.is_empty());
        assert_eq!(app.view().unwrap().navs.message, "Drawing cancelled");
        assert_eq!(app.view().unwrap().nav_edits().len(), n + 1);
        // the same tool again, or another tool chosen: off
        app.run(&gpu, &ctx, Action::NavTool(NavTool::EdgeAdd));
        assert_eq!(
            (app.view().unwrap().tool, app.view().unwrap().nav_tool()),
            (Tool::Select, None)
        );
        app.run(&gpu, &ctx, Action::NavTool(NavTool::Seed));
        app.run(&gpu, &ctx, Action::UseTool(Tool::Move));
        assert_eq!(app.view().unwrap().nav_tool(), None);
        // with no tool a click selects again
        app.run(&gpu, &ctx, Action::UseTool(Tool::Select));
        h.click(&mut app, at);
        assert_eq!(
            app.view().unwrap().nav_edits().len(),
            n + 1,
            "a click with no navigation tool adds nothing"
        );
        println!(
            "{}: frame median with Select {select:.2} ms, placing {placing:.2} ms, a navigation tool {nav:.2} ms; \
             a placing click {place_click:.2} ms, a seed's click {seed_click:.2} ms",
            theme.key()
        );
        assert!(
            placing < select + 16.0,
            "placing's frame {placing:.2} ms, Select's {select:.2} ms"
        );
        assert!(
            nav < select + 16.0,
            "the navigation tool's frame {nav:.2} ms, Select's {select:.2} ms"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
