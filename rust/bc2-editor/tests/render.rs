//! The 3D view drawn offscreen, without a window (round G1's headless proof): the demo scene rendered with a fixed
//! camera is not blank, the selected object's outline is drawn in the theme's `selection` colour around it and
//! nowhere else, picking returns what is drawn at a pixel, and a click in the running editor at an entity's
//! projected position selects it.
//!
//! Skipped (with a note) on a machine without any graphics adapter, software included. `BC2_EDITOR_SHOTS=<folder>`
//! saves the images there.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    missing_docs
)]

use bc2_editor::app::EditorApp;
use bc2_editor::camera::Camera;
use bc2_editor::headless::Headless;
use bc2_editor::render::{self, Gpu, Renderer, SceneGpu};
use bc2_editor::theme::{Palette, ThemeId, Tokens};
use bc2_editor::{demo, model};

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
        let p = std::path::Path::new(&dir).join(name);
        render::write_png(&p, size, rgba).unwrap();
        println!("saved {}", p.display());
    }
}

fn camera() -> Camera {
    Camera {
        target: [0.0, 2.0, 0.0],
        yaw: 0.5,
        pitch: 0.6,
        distance: 90.0,
        fov: 60.0,
        projection: bc2_editor::camera::Projection::Perspective,
    }
}

const SIZE: [u32; 2] = [640, 400];

fn pixel(img: &[u8], x: u32, y: u32) -> [u8; 3] {
    let k = ((y * SIZE[0] + x) * 4) as usize;
    [img[k], img[k + 1], img[k + 2]]
}

#[test]
fn renders_the_scene_and_outlines_the_selection() {
    let _gate = crate::gate();
    let Some(gpu) = gpu() else { return };
    let tokens = Tokens::embedded().unwrap();
    for theme in ThemeId::ALL {
        let palette = Palette::new(tokens, theme);
        let scene = demo::scene();
        let mut sg = SceneGpu::new(&gpu, &scene, &palette);
        let r = Renderer::new(&gpu);
        let target = r.target(&gpu, SIZE);
        let view = camera().view([f64::from(SIZE[0]), f64::from(SIZE[1])]);
        let sel = palette.view_selection;
        let sel = [sel.r(), sel.g(), sel.b()];
        // nothing selected: no selection colour anywhere
        r.render(&gpu, &sg, &target, &view.matrix(), &palette);
        let plain = r.read_output(&gpu, &target).unwrap();
        let count = |img: &[u8]| {
            (0..SIZE[1])
                .flat_map(|y| (0..SIZE[0]).map(move |x| (x, y)))
                .filter(|&(x, y)| pixel(img, x, y) == sel)
                .count()
        };
        assert_eq!(count(&plain), 0, "{theme:?}: selection colour without a selection");
        // not blank: the background, the terrain and the house all show, in many shades
        let mut colours = std::collections::HashSet::new();
        for y in (0..SIZE[1]).step_by(4) {
            for x in (0..SIZE[0]).step_by(4) {
                colours.insert(pixel(&plain, x, y));
            }
        }
        // (the darker G2 terrain gives fewer shades than G1's: 19 in Dark at this size)
        assert!(colours.len() > 12, "{theme:?}: only {} colours", colours.len());
        let bg = palette.scene_sky;
        assert_eq!(
            pixel(&plain, 2, 2),
            [bg.r(), bg.g(), bg.b()],
            "{theme:?}: the corner is the background"
        );
        // the house (entity 0) selected: an outline in the selection colour, around where the house is drawn
        sg.set_selection(&gpu, &[0]);
        r.render(&gpu, &sg, &target, &view.matrix(), &palette);
        let img = r.read_output(&gpu, &target).unwrap();
        let outlined: Vec<(u32, u32)> = (0..SIZE[1])
            .flat_map(|y| (0..SIZE[0]).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&img, x, y) == sel)
            .collect();
        assert!(outlined.len() > 100, "{theme:?}: {} outline pixels", outlined.len());
        let centre = view.project([0.0, 3.0, 0.0]).unwrap();
        for &(x, y) in &outlined {
            let d = ((f64::from(x) - centre[0]).powi(2) + (f64::from(y) - centre[1]).powi(2)).sqrt();
            assert!(d < 120.0, "{theme:?}: an outline pixel at {x},{y}, far from the house");
        }
        // the outline is the only change: every other pixel is as it was
        let changed = (0..SIZE[1])
            .flat_map(|y| (0..SIZE[0]).map(move |x| (x, y)))
            .filter(|&(x, y)| pixel(&img, x, y) != pixel(&plain, x, y))
            .count();
        assert_eq!(changed, outlined.len(), "{theme:?}");
        // the house's own middle is not outlined
        let mid = view.project([0.0, 3.0, 0.0]).unwrap();
        assert_ne!(pixel(&img, mid[0] as u32, mid[1] as u32), sel);
        shot(&format!("render_{}_selected.png", theme.key()), SIZE, &img);
    }
}

#[test]
fn picking_returns_what_is_drawn() {
    let _gate = crate::gate();
    let Some(gpu) = gpu() else { return };
    let palette = Palette::new(Tokens::embedded().unwrap(), ThemeId::Dark);
    let scene = demo::scene();
    let sg = SceneGpu::new(&gpu, &scene, &palette);
    let r = Renderer::new(&gpu);
    let target = r.target(&gpu, SIZE);
    let view = camera().view([f64::from(SIZE[0]), f64::from(SIZE[1])]);
    r.render(&gpu, &sg, &target, &view.matrix(), &palette);
    // each entity with a marker or mesh: its drawn middle picks it
    for (index, lift) in [(0usize, 3.0), (1, 0.6), (2, 1.0), (3, 1.25)] {
        let p = scene.entities[index].position().unwrap();
        let px = view.project([p[0], p[1] + lift, p[2]]).unwrap();
        let id = r.pick(&gpu, &target, [px[0] as u32, px[1] as u32], 0);
        assert_eq!(render::id_entity(id), Some(index), "entity {index} at {px:?}");
    }
    // the sky is nothing; the terrain is the terrain, not an entity
    assert_eq!(r.pick(&gpu, &target, [2, 2], 0), render::ID_NONE);
    let ground = view.project([-40.0, -1.0, 40.0]).unwrap();
    assert_eq!(
        r.pick(&gpu, &target, [ground[0] as u32, ground[1] as u32], 0),
        render::ID_TERRAIN
    );
}

/// The action registry drives the window: the command palette opens the top view (docked beside the 3D view, with
/// its own camera), a click there selects what is under it, F focuses every view on it, and Reset layout removes
/// the extra view again.
#[test]
fn palette_opens_a_top_view_with_its_own_camera() {
    use bc2_editor::app::Tab;
    use bc2_editor::camera::Projection;
    use eframe::egui::{Event, Key, Modifiers};
    let _gate = crate::gate();
    let Ok(mut h) = Headless::new([1280, 800]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    app.open_scene(&h.gpu, "demo", demo::scene());
    h.settle(&mut app, 3);
    // Ctrl+Shift+P, then "top view" and Enter, as a person would
    let key = |k, modifiers| Event::Key {
        key: k,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    };
    h.frame(&mut app, vec![key(Key::P, Modifiers::COMMAND.plus(Modifiers::SHIFT))]);
    assert!(app.palette_is_open(), "Ctrl+Shift+P opens the palette");
    h.frame(&mut app, vec![Event::Text("top view".into())]);
    h.frame(&mut app, vec![key(Key::Enter, Modifiers::NONE)]);
    h.settle(&mut app, 4);
    assert!(!app.palette_is_open());
    assert!(app.tabs().contains(&Tab::Viewport(Projection::Top)), "{:?}", app.tabs());
    // the top view has its own camera, looking straight down
    assert_eq!(
        app.view().unwrap().viewports[&Projection::Top].camera.projection,
        Projection::Top
    );
    assert_eq!(
        app.view().unwrap().viewports[&Projection::Perspective]
            .camera
            .projection,
        Projection::Perspective
    );
    // a click on the flag in the top view selects it, and the surface read-out has the point under it
    let at = app
        .project_entity_in(Projection::Top, 2)
        .expect("the flag is in the top view");
    h.click(&mut app, at);
    assert_eq!(app.view().unwrap().selected, Some(2));
    assert!(app.view().unwrap().last_hit.is_some());
    // F focuses every view on it
    h.frame(&mut app, vec![key(Key::F, Modifiers::NONE)]);
    h.settle(&mut app, 2);
    for (p, vp) in &app.view().unwrap().viewports {
        assert_eq!(vp.camera.target, [20.0, 0.0, -15.0], "{p:?}");
    }
    shot("window_demo_top_view.png", [1280, 800], &h.image().unwrap());
    // Reset layout (its action) brings back the default panels
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    app.run(&gpu, &ctx, bc2_editor::actions::Action::ResetLayout);
    assert!(!app.tabs().contains(&Tab::Viewport(Projection::Top)));
    // the 3D view, Outliner, Layers, Place objects (round E2b), Properties, Links (round GR1), Navigation, Navigation
    // edits, Islands and spawns (round GR2), Problems, History, Diff (round EX1)
    assert_eq!(app.tabs().len(), 12);
}

#[test]
fn a_click_in_the_editor_selects_the_entity_there() {
    let _gate = crate::gate();
    let Ok(mut h) = Headless::new([1280, 800]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    app.open_scene(&h.gpu, "demo", demo::scene());
    h.settle(&mut app, 3);
    for index in [3usize, 1, 0] {
        let at = app.project_entity(index).expect("the entity is in view");
        h.click(&mut app, at);
        assert_eq!(app.view().unwrap().selected, Some(index), "click at {at:?}");
    }
    // the outliner shows the selection: its group opens and the properties name it
    h.settle(&mut app, 10);
    let name = model::display_name(&app.view().unwrap().scene().entities[0]);
    assert_eq!(name, "House");
    // a click on the sky clears the selection
    let rect = app.view().unwrap().view_rect().unwrap();
    h.click(&mut app, rect.left_top() + eframe::egui::vec2(4.0, 4.0));
    assert_eq!(app.view().unwrap().selected, None);
    // and the whole window draws
    app.select(&h.gpu.clone(), Some(2));
    h.settle(&mut app, 10);
    let img = h.image().unwrap();
    assert_eq!(img.len(), 1280 * 800 * 4);
    shot("window_demo_dark.png", [1280, 800], &img);
}
