//! Round GR1's proofs in the running editor, without a window: the Map Inspector's views folded into the editor.
//!
//! - On the demo level (its partition made up, no game copy needed): the Links panel lists the flag's references
//!   (its capture ring's first point, its map marker), the event it sends (to the marker) and, on the marker, what
//!   references it and the event it receives; clicking a row goes to that object and Back returns; Properties' All
//!   fields lists the flag's fields with their types and opens the Geometry array to its item; the 3D view draws the
//!   links in `view-link`, and K (the action) hides them. The same in a project (the edited partition is read).
//! - On `MP_005` (skipped without the game copy): a flag's links and fields, followed to a linked object and back, in
//!   Dark and Light; `BC2_EDITOR_SHOTS=<folder>` saves the screenshots.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    missing_docs
)]

use bc2_editor::actions::Action;
use bc2_editor::app::{EditorApp, Tab};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::render;
use bc2_editor::theme::ThemeId;
use eframe::egui::{Color32, Rect};

const SIZE: [u32; 2] = [1600, 1000];

fn shot(name: &str, h: &mut Headless) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        let p = std::path::Path::new(&dir).join(name);
        render::write_png(&p, SIZE, &h.image().unwrap()).unwrap();
        println!("saved {}", p.display());
    }
}

/// Pixels of `colour` inside `rect` of the last frame.
fn count(h: &mut Headless, rect: Rect, colour: Color32) -> usize {
    let img = h.image().unwrap();
    let mut n = 0;
    for y in rect.top().max(0.0) as u32..(rect.bottom() as u32).min(SIZE[1]) {
        for x in rect.left().max(0.0) as u32..(rect.right() as u32).min(SIZE[0]) {
            let k = ((y * SIZE[0] + x) * 4) as usize;
            let near = |a: u8, b: u8| a.abs_diff(b) <= 2;
            if near(img[k], colour.r()) && near(img[k + 1], colour.g()) && near(img[k + 2], colour.b()) {
                n += 1;
            }
        }
    }
    n
}

fn rect_of(app: &EditorApp, target: usize) -> Rect {
    app.view()
        .unwrap()
        .link_rects
        .iter()
        .find(|(t, _)| *t == target)
        .unwrap_or_else(|| panic!("no Links row to {target}: {:?}", app.view().unwrap().link_rects))
        .1
}

/// The demo level's links, fields and drawn links, viewed (`project: false`) or in a project.
fn demo_level(h: &mut Headless, project: bool) {
    let dir = std::env::temp_dir().join(format!("bc2_editor_gr1_{}_{project}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let scene = bc2_editor::demo::level(&dir.join("level")).unwrap();
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    if project {
        let then = bc2_editor::app::Then::Create("GR1 links".into(), dir.join("projects"));
        app.open_scene_then(&gpu, "demo", scene, then);
    } else {
        app.open_scene(&gpu, "demo", scene);
    }
    h.settle(&mut app, 3);
    assert!(app.tabs().contains(&Tab::Links), "{:?}", app.tabs());
    assert_eq!(app.view().unwrap().doc.is_some(), project);
    // the flag: two references (its ring's first point, its marker), one event sent
    app.select(&gpu, Some(5));
    app.focus_selected();
    h.settle(&mut app, 6);
    let rows: Vec<usize> = app.view().unwrap().link_rects.iter().map(|(t, _)| *t).collect();
    assert_eq!(rows, [6, 10, 10], "references 6 and 10, the event to 10");
    // the links drawn in the 3D view, in view-link
    let p = app.palette();
    let view = app.view().unwrap().view_rect().unwrap();
    let drawn = count(h, view, p.view_link);
    assert!(drawn > 40, "{drawn} view-link pixels");
    // All fields: the flag's fields with their types; Geometry opens to its item
    let headers: Vec<String> = app.view().unwrap().field_rects.iter().map(|(n, _)| n.clone()).collect();
    assert_eq!(headers, ["Geometry"], "the flag's one container");
    let at = app.view().unwrap().field_rects[0].1.center();
    h.click(&mut app, at);
    h.settle(&mut app, 2);
    let tree = app.view_mut().unwrap().property_tree(5).unwrap().to_vec();
    let rows: Vec<(&str, &str)> = tree.iter().map(|n| (n.name.as_str(), n.ty.as_str())).collect();
    assert_eq!(
        rows,
        [
            ("Name", "text"),
            ("Transform", "float32 x16"),
            ("Geometry", "array [1]"),
            ("MapMarker", "reference"),
            ("InitialOwnerTeam", "text")
        ]
    );
    // a click on the marker's row goes there; it is referenced by the flag and receives its event
    let at = rect_of(&app, 10).center();
    h.click(&mut app, at);
    h.settle(&mut app, 3);
    assert_eq!(app.view().unwrap().selected, Some(10));
    assert_eq!(app.view().unwrap().link_back, [5]);
    let rows: Vec<usize> = app.view().unwrap().link_rects.iter().map(|(t, _)| *t).collect();
    assert_eq!(rows, [5, 5], "referenced by the flag, the event from it");
    // Back returns to the flag
    app.view_mut().unwrap().go_back(&gpu);
    h.settle(&mut app, 3);
    assert_eq!(app.view().unwrap().selected, Some(5));
    assert_eq!(app.view().unwrap().link_back, Vec::<usize>::new());
    // K hides the drawn links
    let ctx = h.ctx.clone();
    app.run(&gpu, &ctx, Action::ToggleLinks);
    h.settle(&mut app, 3);
    let view = app.view().unwrap().view_rect().unwrap();
    assert_eq!(count(h, view, p.view_link), 0, "no links drawn after K");
    // Measure (M): two clicks on the ground add two points, the selection stays; M again ends it
    app.run(&gpu, &ctx, Action::ToggleMeasure);
    h.settle(&mut app, 2);
    let centre = app.view().unwrap().view_rect().unwrap().center();
    for dx in [-150.0, 150.0] {
        h.click(&mut app, centre + eframe::egui::Vec2::new(dx, 80.0));
    }
    let pts = app.view().unwrap().measure.clone();
    assert_eq!(pts.len(), 2, "{pts:?}");
    assert_eq!(app.view().unwrap().selected, Some(5), "measuring does not select");
    let f = bc2_editor::inspect::measure(&pts).unwrap();
    assert!(f.segment > 1.0 && (f.total - f.segment).abs() < 1e-9, "{f:?}");
    app.run(&gpu, &ctx, Action::ToggleMeasure);
    h.settle(&mut app, 2);
    assert!(app.view().unwrap().measure.is_empty() && !app.view().unwrap().measuring);
    println!(
        "gr1 demo ({}): links listed, followed and back, fields typed, {drawn} link pixels, hidden by K; measured {:.2} m",
        if project { "project" } else { "viewed" },
        f.segment
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn links_and_fields_on_the_demo_level() {
    let Ok(mut h) = Headless::new(SIZE) else {
        println!("skipped: no graphics adapter");
        return;
    };
    demo_level(&mut h, false);
    demo_level(&mut h, true);
}

#[test]
fn links_and_fields_on_mp_005() {
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let Ok(mut h) = Headless::new(SIZE) else {
        println!("skipped: no graphics adapter");
        return;
    };
    for theme in ThemeId::ALL {
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        if let Err(e) = app.open_level_blocking(&gpu, "mp_005") {
            println!("skipped mp_005: {e}");
            return;
        }
        // the flag with the most links
        let flag = {
            let v = app.view_mut().unwrap();
            let flags: Vec<usize> = v
                .scene()
                .entities
                .iter()
                .filter(|e| e.category == "capture point")
                .map(|e| e.index)
                .collect();
            let links = v.links();
            flags.into_iter().max_by_key(|&k| links.count(k)).unwrap()
        };
        app.select(&gpu, Some(flag));
        app.focus_selected();
        h.settle(&mut app, 10);
        let v = app.view().unwrap();
        let n_rows = v.link_rects.len();
        assert!(n_rows >= 3, "{n_rows} link rows");
        let p = app.palette();
        let drawn = count(&mut h, v.view_rect().unwrap(), p.view_link);
        assert!(drawn > 40, "{theme:?}: {drawn} view-link pixels");
        // Properties scrolled down to All fields, its first container opened
        let props = v.view_rect().unwrap().right_top() + eframe::egui::Vec2::new(150.0, 200.0);
        h.wheel(&mut app, props, -5);
        h.settle(&mut app, 3);
        let first = app.view().unwrap().field_rects.first().cloned();
        if let Some((name, r)) = first {
            h.click(&mut app, r.center());
            h.settle(&mut app, 3);
            println!("gr1 mp_005: opened {name}");
        }
        shot(&format!("gr1_links_fields_mp_005_{}.png", theme.key()), &mut h);
        // follow the first link and come back
        let (target, r) = app.view().unwrap().link_rects[0];
        h.click(&mut app, r.center());
        h.settle(&mut app, 6);
        assert_eq!(app.view().unwrap().selected, Some(target));
        shot(&format!("gr1_followed_mp_005_{}.png", theme.key()), &mut h);
        app.view_mut().unwrap().go_back(&gpu);
        h.settle(&mut app, 3);
        assert_eq!(app.view().unwrap().selected, Some(flag));
        println!("gr1 mp_005 {theme:?}: flag {flag}, {n_rows} link rows, {drawn} link pixels, followed and back");
    }
}
