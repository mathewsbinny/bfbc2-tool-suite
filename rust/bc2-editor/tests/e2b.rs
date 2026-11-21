//! Round E2b's interface proof, headless, as a user: the demo level with a real partition (made up, written with
//! the suite's DBX writer, so every edit goes through levelwrite) made into a project in a temp folder, then:
//! the context menus' entries per workspace (and a right-click in the view, an entry clicked), Alt+wheel sinking
//! (one history entry per burst), placing from the "Place object here..." picker and by dragging a content browser
//! entry, a deep duplicate of a flag with its capture area, the view modes, polygon selection, a field set on two
//! objects at once ("Mixed" before), the Terrain and Vegetation stubs, a team set from the Gameplay workspace, and
//! "Check edits" replaying the whole log to the same bytes.
//!
//! Skipped (with a note) without any graphics adapter. `BC2_EDITOR_SHOTS=<folder>` saves dark and light screenshots.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::float_cmp,
    clippy::too_many_lines,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::type_complexity,
    clippy::map_unwrap_or,
    clippy::assert_is_empty,
    missing_docs
)]

use bc2_editor::actions::{Action, PanelKind};
use bc2_editor::app::{EditorApp, MenuKind, Then, Workspace, menu_items};
use bc2_editor::camera::Projection;
use bc2_editor::demo;
use bc2_editor::gameplay::BrowserMode;
use bc2_editor::gizmo::Tool;
use bc2_editor::headless::Headless;
use bc2_editor::theme::ThemeId;
use bc2_editor::workspace::ViewMode;
use bc2_levelwrite::{FieldValue, Op};
use eframe::egui::{Key, Modifiers, Pos2, Vec2};

const CTRL_SHIFT: Modifiers = Modifiers {
    alt: false,
    ctrl: true,
    shift: true,
    mac_cmd: false,
    command: true,
};

/// Dark and light screenshots (the theme switched and back).
fn shots(h: &mut Headless, app: &mut EditorApp, name: &str) {
    let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") else {
        return;
    };
    for (theme, tag) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = theme;
        h.settle(app, 4);
        let p = std::path::Path::new(&dir).join(format!("{name}_{tag}.png"));
        h.save(&p).unwrap();
        println!("saved {}", p.display());
    }
    app.theme = ThemeId::Dark;
    h.settle(app, 3);
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

fn index(app: &EditorApp, k: u32) -> usize {
    app.view()
        .unwrap()
        .doc
        .as_ref()
        .unwrap()
        .index(&demo::level_guid(k))
        .unwrap()
}

fn names(items: &[Option<bc2_editor::app::MenuItem>]) -> Vec<String> {
    items.iter().flatten().map(|i| i.name.clone()).collect()
}

/// A screen point on the terrain of the 3D view at world (x, z).
fn ground(app: &EditorApp, x: f64, z: f64) -> Pos2 {
    let v = app.view().unwrap();
    let vp = &v.viewports[&Projection::Perspective];
    let rect = vp.rect.unwrap();
    let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
    let y = bc2_editor::query::surface_below(v.scene(), x, z).map_or(0.0, |h| h.point[1]);
    let s = view.project([x, y, z]).unwrap();
    rect.min + Vec2::new(s[0] as f32, s[1] as f32)
}

fn click_menu(h: &mut Headless, app: &mut EditorApp, label: &str) {
    let (_, r, enabled) = app
        .view()
        .unwrap()
        .menu_rects
        .iter()
        .find(|(n, _, _)| n == label)
        .cloned()
        .unwrap_or_else(|| panic!("{label} not in {:?}", app.view().unwrap().menu_rects));
    assert!(enabled, "{label} is disabled");
    h.click(app, r.center());
}

#[test]
fn e2b_through_the_interface() {
    let _gate = crate::gate();
    let mut h = match Headless::new([1500, 900]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let dir = std::env::temp_dir().join(format!("bc2_editor_e2b_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let scene = demo::level(&dir.join("level")).unwrap();
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    app.open_scene_then(&gpu, "demo", scene, Then::Create("E2b demo".into(), dir.clone()));
    h.settle(&mut app, 6);
    assert!(app.view().unwrap().doc.as_ref().unwrap().has_partition());
    let (house, crate_a, crate_b, flag) = (index(&app, 1), index(&app, 2), index(&app, 3), index(&app, 5));
    // ---------------------------------------------------------------- the context menus' entries per workspace
    app.select(&gpu, Some(crate_a));
    {
        let v = app.view().unwrap();
        let obj = names(&menu_items(v, MenuKind::Object, Some(crate_a)));
        for want in [
            "Move",
            "Rotate",
            "Snap to ground",
            "Sink below surface\u{2026}",
            "Duplicate",
            "Deep duplicate",
            "Delete",
            "Group",
            "Ungroup",
            "Hide",
            "Lock",
            "Focus",
            "Properties",
            "Copy GUID",
            "Select all of this type",
        ] {
            assert!(obj.iter().any(|n| n == want), "{want} in the Objects menu: {obj:?}");
        }
        let ground_menu = names(&menu_items(v, MenuKind::Ground, None));
        assert_eq!(
            ground_menu,
            [
                "Place object here\u{2026}",
                "Paste",
                "Select all of this type",
                "Focus here"
            ]
        );
        // disabled entries say why
        let items = menu_items(v, MenuKind::Ground, None);
        let paste = items.iter().flatten().find(|i| i.name == "Paste").unwrap();
        assert_eq!(paste.why, Some("Copy objects first"));
        let ungroup = menu_items(v, MenuKind::Object, Some(crate_a));
        let ungroup = ungroup.iter().flatten().find(|i| i.name == "Ungroup").unwrap();
        assert_eq!(ungroup.why, Some("Select a grouped object first"));
    }
    for (w, has, lacks) in [
        (
            Workspace::Gameplay,
            vec!["Set team 1", "Set team 2", "Set neutral", "Deep duplicate"],
            vec!["Group"],
        ),
        (
            Workspace::Ai,
            vec!["Show the navmesh", "Path test from here"],
            vec!["Group", "Set team 1"],
        ),
        (Workspace::View, vec!["Focus", "Copy GUID"], vec!["Delete", "Move"]),
    ] {
        app.run_action(&gpu, &ctx, Action::SetWorkspace(w));
        let v = app.view().unwrap();
        let obj = names(&menu_items(v, MenuKind::Object, Some(flag)));
        for n in has {
            assert!(obj.iter().any(|x| x == n), "{n} in the {} menu: {obj:?}", w.name());
        }
        for n in lacks {
            assert!(!obj.iter().any(|x| x == n), "{n} not in the {} menu", w.name());
        }
    }
    app.run_action(&gpu, &ctx, Action::SetWorkspace(Workspace::Ai));
    let path_test = menu_items(app.view().unwrap(), MenuKind::Ground, None);
    let path_test = path_test
        .iter()
        .flatten()
        .find(|i| i.name == "Path test from here")
        .unwrap();
    // the demo level has no path data (round GR2: the path test needs a file read)
    assert_eq!(
        path_test.why,
        Some("The level has no navigation data read (show the navmesh first)")
    );
    for (w, brushes) in [(Workspace::Terrain, 5usize), (Workspace::Vegetation, 2)] {
        app.run_action(&gpu, &ctx, Action::SetWorkspace(w));
        let items = menu_items(app.view().unwrap(), MenuKind::Ground, None);
        let greyed: Vec<_> = items.iter().flatten().filter(|i| i.why.is_some()).collect();
        assert_eq!(greyed.len(), brushes, "{}", w.name());
        assert!(greyed.iter().all(|i| i.why.unwrap().starts_with("Coming in the")));
        h.settle(&mut app, 3);
        shots(&mut h, &mut app, &format!("e2b_{}_stub", w.name().to_lowercase()));
    }
    app.run_action(&gpu, &ctx, Action::SetWorkspace(Workspace::Objects));
    h.settle(&mut app, 3);
    // a right-click on the crate in the view: the object menu; Hide clicked is one operation
    app.select(&gpu, None);
    h.settle(&mut app, 2);
    let at = app.project_entity(crate_a).unwrap();
    h.right_click(&mut app, at);
    h.settle(&mut app, 2);
    assert_eq!(app.view().unwrap().selected, Some(crate_a), "a right-click selects");
    assert_eq!(app.view().unwrap().menu_kind, Some(MenuKind::Object));
    shots(&mut h, &mut app, "e2b_context_menu_object");
    click_menu(&mut h, &mut app, "Hide");
    assert!(matches!(ops(&app).last(), Some(Op::Hide { hidden: true, .. })));
    assert!(!app.view().unwrap().doc.as_ref().unwrap().shown(crate_a));
    app.run_action(&gpu, &ctx, Action::Undo);
    assert!(ops(&app).is_empty());
    // a right-click on open ground: the ground menu
    let g = ground(&app, -35.0, 30.0);
    h.right_click(&mut app, g);
    h.settle(&mut app, 2);
    assert_eq!(app.view().unwrap().menu_kind, Some(MenuKind::Ground));
    shots(&mut h, &mut app, "e2b_context_menu_ground");
    // ---------------------------------------------------------------- placing from the picker
    click_menu(&mut h, &mut app, "Place object here\u{2026}");
    assert!(app.view().unwrap().picker.is_some(), "the picker opened");
    app.view_mut().unwrap().picker.as_mut().unwrap().query = "crate".into();
    h.settle(&mut app, 3);
    shots(&mut h, &mut app, "e2b_place_picker");
    h.key(&mut app, Key::Enter, Modifiers::NONE);
    assert!(app.view().unwrap().placing.is_some(), "Enter takes the first match");
    // the object follows the pointer; Shift+wheel turns it 15 degrees a notch, Alt+wheel sinks it
    let g = ground(&app, -30.0, 25.0);
    h.hover(&mut app, g);
    h.modifiers = Modifiers::SHIFT;
    h.wheel(&mut app, g, 2);
    h.modifiers = Modifiers::ALT;
    h.wheel(&mut app, g, -3);
    h.modifiers = Modifiers::NONE;
    {
        let p = app.view().unwrap().placing.as_ref().unwrap();
        assert_eq!(p.yaw, 30.0);
        assert!((p.placement.sink - 0.3).abs() < 1e-9, "{}", p.placement.sink);
        assert!(p.at.is_some(), "it is on the ground under the pointer");
    }
    shots(&mut h, &mut app, "e2b_placing");
    assert!(ops(&app).is_empty(), "nothing is committed while placing");
    h.click(&mut app, g);
    let log = ops(&app);
    let Some(Op::Place { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert_eq!(
        items[0].blueprint,
        format!("{}/{}", demo::LEVEL_BLUEPRINTS[1].0, demo::LEVEL_BLUEPRINTS[1].1)
    );
    assert_eq!(
        items[0].template,
        demo::level_guid(2),
        "a plain placement of the crate is its template"
    );
    assert!((items[0].placement.unwrap().sink - 0.3).abs() < 1e-9);
    let placed = app.view().unwrap().selected.unwrap();
    assert_eq!(
        app.view().unwrap().scene().entities[placed].guid,
        items[0].guid,
        "the new object is selected"
    );
    // ---------------------------------------------------------------- placing by dragging a content browser entry
    app.run_action(&gpu, &ctx, Action::ShowPanel(PanelKind::Content));
    h.settle(&mut app, 4);
    shots(&mut h, &mut app, "e2b_content_browser");
    let row = app
        .view()
        .unwrap()
        .content_rects
        .iter()
        .find(|(p, _)| p == demo::LEVEL_BLUEPRINTS[0].0)
        .map(|(_, r)| *r)
        .expect("the house's row");
    let n = ops(&app).len();
    // the 3D view is behind the content tab's neighbour: find a ground point in it
    let target = ground(&app, -25.0, -25.0);
    h.drag(&mut app, row.center(), target, 12, Modifiers::NONE);
    let log = ops(&app);
    assert_eq!(log.len(), n + 1, "one drag, one placement: {log:?}");
    let Some(Op::Place { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert_eq!(items[0].template, demo::level_guid(1));
    assert!(app.view().unwrap().placing.is_none());
    // ---------------------------------------------------------------- Objectives: a flag with its capture area
    app.view_mut().unwrap().browser_mode = BrowserMode::Objectives;
    app.view_mut().unwrap().place_team = 2;
    h.settle(&mut app, 4);
    shots(&mut h, &mut app, "e2b_browser_objectives");
    let tile_at = |app: &EditorApp, key: &str| {
        app.view()
            .unwrap()
            .content_rects
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, r)| r.center())
            .unwrap_or_else(|| panic!("{key} in {:?}", app.view().unwrap().content_rects))
    };
    let n = ops(&app).len();
    let flags_before = app.view().unwrap().scene().shapes.len();
    let from = tile_at(&app, "CapturePoint");
    let target = ground(&app, -30.0, -5.0);
    h.drag(&mut app, from, target, 12, Modifiers::NONE);
    let log = ops(&app);
    assert_eq!(log.len(), n + 1, "{:?} {}", &log[n..], app.view().unwrap().message);
    let Some(Op::Place { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert_eq!(items[0].template, demo::level_guid(5));
    assert_eq!(items[0].owned.len(), 5, "the flag's ring and marker come with it");
    assert_eq!(items[0].fields[0].value, FieldValue::Text("Team2".into()));
    let new_flag = app.view().unwrap().selected.unwrap();
    assert_eq!(app.view().unwrap().scene().entities[new_flag].team, Some(2));
    assert_eq!(
        app.view().unwrap().scene().shapes.len(),
        flags_before + 1,
        "its capture area outline"
    );
    {
        // the copy's references point at its own ring and marker; only internal connections were copied
        let d = app.view().unwrap().doc.as_ref().unwrap();
        let w = bc2_levelwrite::LevelWriter::parse(&d.partition().unwrap()).unwrap();
        let get = |g: &str, f: &str| match w.get_field(&d.schema, g, f).unwrap().1 {
            Some(FieldValue::Ref(r)) => r,
            v => panic!("{v:?}"),
        };
        assert_eq!(
            get(&items[0].guid, "MapMarker"),
            items[0].owned[4].guid.to_ascii_lowercase()
        );
        // the outliner lists it as a flag; the Gameplay workspace selects it and offers its menu
        assert!(
            app.view()
                .unwrap()
                .outliner
                .groups
                .iter()
                .any(|g| g.entries.iter().any(|e| e.entity == new_flag))
        );
    }
    app.run_action(&gpu, &ctx, Action::SetWorkspace(Workspace::Gameplay));
    assert!(app.view().unwrap().selectable(new_flag));
    let menu = names(&menu_items(app.view().unwrap(), MenuKind::Object, Some(new_flag)));
    assert!(menu.iter().any(|n| n == "Set team 1"), "{menu:?}");
    app.run_action(&gpu, &ctx, Action::SetWorkspace(Workspace::Objects));
    // ---------------------------------------------------------------- Spawners: a player spawn point
    app.view_mut().unwrap().browser_mode = BrowserMode::Spawners;
    app.view_mut().unwrap().place_team = 1;
    h.settle(&mut app, 4);
    shots(&mut h, &mut app, "e2b_browser_spawners");
    let n = ops(&app).len();
    let from = tile_at(&app, "SpawnPoint");
    let target = ground(&app, -35.0, 20.0);
    h.drag(&mut app, from, target, 12, Modifiers::NONE);
    let log = ops(&app);
    assert_eq!(log.len(), n + 1);
    let Some(Op::Place { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert_eq!(items[0].template, demo::level_guid(4));
    assert_eq!(items[0].fields[0].value, FieldValue::Text("Team1".into()));
    app.view_mut().unwrap().browser_mode = BrowserMode::PlaceObjects;
    h.settle(&mut app, 3);
    // ---------------------------------------------------------------- the sink popup: Alt+wheel, one entry per burst
    app.select(&gpu, Some(crate_b));
    h.settle(&mut app, 2);
    let at = app.project_entity(crate_b).unwrap();
    let n = ops(&app).len();
    h.modifiers = Modifiers::ALT;
    h.wheel(&mut app, at, -3);
    h.modifiers = Modifiers::NONE;
    let log = ops(&app);
    assert_eq!(
        log.len(),
        n + 1,
        "a burst of three notches is one operation: {:?}",
        &log[n..]
    );
    let Some(Op::Move { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert!((items[0].placement.unwrap().sink - 0.3).abs() < 1e-9);
    assert!(app.view().unwrap().sink.is_some(), "the popup is open");
    shots(&mut h, &mut app, "e2b_sink_popup");
    // a pause, then Shift+Alt: 0.01 m a notch, a second operation
    h.settle(&mut app, 50);
    h.modifiers = Modifiers::ALT | Modifiers::SHIFT;
    h.wheel(&mut app, at, -2);
    h.modifiers = Modifiers::NONE;
    let log = ops(&app);
    assert_eq!(log.len(), n + 2, "after a pause, another operation");
    let Some(Op::Move { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert!((items[0].placement.unwrap().sink - 0.32).abs() < 1e-9);
    app.run_action(&gpu, &ctx, Action::Undo);
    assert_eq!(ops(&app).len(), n + 1, "one undo takes back the whole burst");
    h.key(&mut app, Key::Escape, Modifiers::NONE);
    assert!(app.view().unwrap().sink.is_none(), "Esc closes the popup");
    // ---------------------------------------------------------------- deep duplicate: the flag with its capture area
    app.select(&gpu, Some(flag));
    h.settle(&mut app, 2);
    let n = ops(&app).len();
    h.key(&mut app, Key::D, CTRL_SHIFT);
    let log = ops(&app);
    assert_eq!(log.len(), n + 1);
    let Some(Op::Duplicate { items }) = log.last() else {
        panic!("{log:?}")
    };
    let owned: Vec<String> = items[0].owned.iter().map(|o| o.source.clone()).collect();
    assert_eq!(
        owned,
        [6, 7, 8, 9, 10].map(demo::level_guid),
        "the ring and the marker are the flag's"
    );
    {
        let d = app.view().unwrap().doc.as_ref().unwrap();
        let w = bc2_levelwrite::LevelWriter::parse(&d.partition().unwrap()).unwrap();
        let get = |g: &str, f: &str| match w.get_field(&d.schema, g, f).unwrap().1 {
            Some(FieldValue::Ref(r)) => r,
            v => panic!("{v:?}"),
        };
        assert_eq!(
            get(&items[0].guid, "MapMarker"),
            items[0].owned[4].guid.to_ascii_lowercase()
        );
        assert_eq!(
            get(&items[0].owned[3].guid, "Next"),
            items[0].owned[0].guid.to_ascii_lowercase()
        );
        // a plain duplicate shares them
        let shapes = app.view().unwrap().scene().shapes.len();
        assert_eq!(
            shapes, 3,
            "the copy has its own capture area outline (beside the placed flag's)"
        );
    }
    // ---------------------------------------------------------------- view modes
    let mut images = Vec::new();
    for m in ViewMode::ALL {
        app.run_action(&gpu, &ctx, Action::SetViewMode(m));
        h.settle(&mut app, 3);
        assert_eq!(app.view().unwrap().view_mode, m);
        images.push(h.image().unwrap());
        shots(
            &mut h,
            &mut app,
            &format!("e2b_view_{}", m.name().to_lowercase().replace(' ', "_")),
        );
    }
    for (k, a) in images.iter().enumerate() {
        for b in images.iter().skip(k + 1).take(1) {
            assert_ne!(a, b, "view mode {k} draws differently from the next");
        }
    }
    let note = app.view().unwrap().mode_note.clone();
    assert!(note.starts_with("Cull tree:"), "{note}");
    app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Navmesh));
    assert!(app.view().unwrap().mode_note.contains("no navigation data"));
    app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Lit));
    h.settle(&mut app, 3);
    // ---------------------------------------------------------------- polygon selection around the two crates
    app.select(&gpu, None);
    h.key(&mut app, Key::P, Modifiers::NONE);
    assert_eq!(app.view().unwrap().tool, Tool::Polygon);
    let (a, b) = (
        app.project_entity(crate_a).unwrap(),
        app.project_entity(crate_b).unwrap(),
    );
    let (lo, hi) = (a.min(b) - Vec2::splat(25.0), a.max(b) + Vec2::splat(25.0));
    for p in [lo, Pos2::new(hi.x, lo.y), hi, Pos2::new(lo.x, hi.y)] {
        h.click(&mut app, p);
        h.settle(&mut app, 20);
    }
    shots(&mut h, &mut app, "e2b_polygon");
    h.key(&mut app, Key::Enter, Modifiers::NONE);
    let mut sel = app.view().unwrap().selection.clone();
    sel.sort_unstable();
    assert!(
        sel.contains(&crate_a) && sel.contains(&crate_b) && !sel.contains(&house),
        "{sel:?}"
    );
    // ---------------------------------------------------------------- two objects' fields: Mixed, then one set
    app.select_many(&gpu, &[crate_a, crate_b]);
    h.key(&mut app, Key::W, Modifiers::NONE);
    h.settle(&mut app, 3);
    let common = app.view().unwrap().common_fields();
    let health = common.iter().find(|(p, _, _)| p == "Health").unwrap();
    assert_eq!(health.2, None, "25 and 30: Mixed");
    shots(&mut h, &mut app, "e2b_mixed_fields");
    let n = ops(&app).len();
    let palette = app.palette();
    app.view_mut()
        .unwrap()
        .set_common_field(&gpu, &palette, "Health", "40")
        .unwrap();
    let log = ops(&app);
    assert_eq!(log.len(), n + 1);
    let Some(Op::SetField { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert_eq!(items.len(), 2);
    let common = app.view().unwrap().common_fields();
    assert_eq!(
        common.iter().find(|(p, _, _)| p == "Health").unwrap().2,
        Some(FieldValue::Float(40.0))
    );
    // ---------------------------------------------------------------- the Gameplay workspace: only gameplay selects
    app.run_action(&gpu, &ctx, Action::SetWorkspace(Workspace::Gameplay));
    h.settle(&mut app, 2);
    let at = app.project_entity(house).unwrap();
    h.click(&mut app, at);
    assert!(
        !app.view().unwrap().selection.contains(&house),
        "a house is not gameplay"
    );
    app.select(&gpu, Some(flag));
    app.run_action(&gpu, &ctx, Action::SetTeam(1));
    let log = ops(&app);
    let Some(Op::SetField { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert_eq!(
        (items[0].field.as_str(), &items[0].to),
        ("InitialOwnerTeam", &FieldValue::Text("Team1".into()))
    );
    assert_eq!(app.view().unwrap().scene().entities[flag].team, Some(1));
    // ---------------------------------------------------------------- the whole log replays to the same bytes
    let check = app.view().unwrap().doc.as_ref().unwrap().check();
    assert!(check.contains("apply cleanly") && !check.contains("differs"), "{check}");
    println!("{check}");
    h.key(&mut app, Key::S, Modifiers::COMMAND);
    let path = app.view().unwrap().doc.as_ref().unwrap().path.clone();
    let saved = bc2_levelwrite::MapProject::load(&path).unwrap();
    let scene = demo::level(&dir.join("level2")).unwrap();
    app.open_scene_then(&gpu, "demo", scene, Then::Open(path, None));
    h.settle(&mut app, 3);
    assert_eq!(ops(&app).len(), saved.operations.len());
    let again = app.view().unwrap().doc.as_ref().unwrap().check();
    assert!(again.contains("apply cleanly") && !again.contains("differs"), "{again}");
    println!(
        "e2b through the interface: {} operations, saved and reopened",
        saved.operations.len()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
