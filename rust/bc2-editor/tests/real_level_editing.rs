//! Editing a real level (round E2): surface placement with sink depths on MP_005's terrain samples, and an MP_005
//! map project driven headless - select a placed object, drag the gizmo's X arm 10 m on the grid, put it on the
//! ground sunk 25 % of its height - whose edited partition (levelwrite, live in the editor) has the object at the new
//! transform and nothing else changed but that, and whose "Check edits" replays the log to the same bytes.
//!
//! Skipped (with a note) without the game copy or a graphics adapter. The project goes to a temp folder; OPFOR's
//! files are only read.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::float_cmp,
    clippy::panic,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    clippy::too_many_lines,
    missing_docs
)]

use bc2_editor::app::{EditorApp, Then};
use bc2_editor::camera::Projection;
use bc2_editor::gizmo::{self, Handle};
use bc2_editor::headless::Headless;
use bc2_editor::{levels, math, query};
use bc2_formats::dbx::edit::EditDbx;
use bc2_levelwrite::diff::{Change, diff};
use bc2_levelwrite::write::LevelWriter;
use bc2_levelwrite::{Op, Placement, SinkUnit};
use bc2_scene::scene;
use eframe::egui::{Key, Modifiers};

/// Placing on MP_005's terrain: at each sampled triangle's centroid, the surface below is the triangle's plane, and
/// an object placed there sits at that height less its sink (0, 0.5 m, 25 % of its height).
#[test]
#[ignore = "game tier: MP_005's terrain; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn placement_on_real_terrain() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let found = match levels::load_options(&root, "mp_005") {
        Ok(f) => f,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let s = scene::load(&found.options).unwrap();
    let Some((v, tris)) = &s.terrain else {
        println!("skipped: MP_005's terrain is not unpacked");
        return;
    };
    // a placed object with collision to place: its extent and height
    let k = s.collision.as_ref().unwrap().meshes[0].0;
    let m = s.entities[k].matrix.unwrap();
    let ext = query::extent(&s, k);
    let height = query::height(&s, k);
    assert!(height > 0.0);
    let mut checked = 0;
    for t in tris.iter().step_by((tris.len() / 60).max(1)) {
        let [a, b, c] = t.map(|i| v[i as usize].map(f64::from));
        let x = (a[0] + b[0] + c[0]) / 3.0;
        let z = (a[2] + b[2] + c[2]) / 3.0;
        let Some(hit) = query::surface_below_except(&s, x, z, &|_| true) else {
            continue;
        };
        if hit.entity.is_some() {
            continue;
        }
        // the plane of the triangle under the centroid: the terrain sample's height there
        let want = (a[1] + b[1] + c[1]) / 3.0;
        // (a height field's triangles do not overlap: the centroid's surface is its own triangle)
        assert!(
            (hit.point[1] - want).abs() <= 1e-3,
            "terrain at ({x}, {z}): {} vs the sample's {want}",
            hit.point[1]
        );
        for (sink, unit, depth) in [
            (0.0, SinkUnit::Metres, 0.0),
            (0.5, SinkUnit::Metres, 0.5),
            (0.25, SinkUnit::Fraction, 0.25 * height),
        ] {
            let p = Placement {
                sink,
                unit,
                along_normal: false,
                align_to_normal: false,
            };
            let placed = query::place(&math::at(&m, [x, 0.0, z]), ext, &hit, &p);
            let up = math::row(&placed, 1);
            // the object's lowest point is `depth` below the surface
            let bottom = math::row(&placed, 3)[1] + up[1] * ext.0;
            assert!(
                (bottom - (want - depth)).abs() < 1e-6,
                "{sink} {unit:?}: {bottom} vs {}",
                want - depth
            );
        }
        checked += 1;
    }
    assert!(checked >= 30, "{checked} samples");
    println!("placement on MP_005's terrain: {checked} samples x 3 sink depths");
}

#[test]
fn an_mp_005_project_through_the_interface() {
    let _gate = crate::gate();
    let Ok(_root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let mut h = match Headless::new([1400, 860]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let dir = std::env::temp_dir().join(format!("bc2_editor_mp005_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    if let Err(e) = app.open_level_blocking_then(&gpu, "mp_005", Then::Create("MP_005 north".into(), dir.clone(), None))
    {
        println!("skipped: {e}");
        return;
    }
    h.settle(&mut app, 4);
    // a placed object with collision, editable, near the middle of the map
    let k = {
        let v = app.view().unwrap();
        let d = v.doc.as_ref().unwrap();
        let col = d.scene.collision.as_ref().unwrap();
        col.meshes
            .iter()
            .map(|(k, _)| *k)
            .find(|&k| d.why_not(k).is_none() && d.scene.entities[k].type_name.ends_with("ReferenceObjectData"))
            .unwrap()
    };
    let guid = app.view().unwrap().scene().entities[k].guid.clone();
    app.select(&gpu, Some(k));
    app.focus_selected();
    h.settle(&mut app, 4);
    h.key(&mut app, Key::G, Modifiers::NONE);
    {
        let v = app.view_mut().unwrap();
        v.snap.grid_size = 10.0;
    }
    h.settle(&mut app, 2);
    let (o, tip) = app.gizmo_handle(Projection::Perspective, Handle::Axis(0)).unwrap();
    let (l, start) = {
        let v = app.view().unwrap();
        let vp = &v.viewports[&Projection::Perspective];
        let rect = vp.rect.unwrap();
        let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
        (
            gizmo::arm(&view, v.scene().entities[k].position().unwrap()),
            v.scene().entities[k].matrix.unwrap(),
        )
    };
    let grab = o + (tip - o) * 0.6;
    h.drag(&mut app, grab, grab + (tip - o) * (10.5 / l) as f32, 8, Modifiers::NONE);
    // snap to ground, sunk a quarter of its height
    {
        let v = app.view_mut().unwrap();
        v.snap.placement = Placement {
            sink: 0.25,
            unit: SinkUnit::Fraction,
            along_normal: false,
            align_to_normal: false,
        };
    }
    h.key(&mut app, Key::End, Modifiers::NONE);
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        // the edited map with History open, then the same in the Light theme
        let ctx = h.ctx.clone();
        app.run_action(
            &gpu,
            &ctx,
            bc2_editor::actions::Action::ShowPanel(bc2_editor::actions::PanelKind::History),
        );
        h.settle(&mut app, 4);
        h.save(&std::path::Path::new(&dir).join("e2_mp_005_edit.png")).unwrap();
        app.theme = bc2_editor::theme::ThemeId::Light;
        h.settle(&mut app, 4);
        h.save(&std::path::Path::new(&dir).join("e2_mp_005_edit_light.png"))
            .unwrap();
        app.theme = bc2_editor::theme::ThemeId::Dark;
        h.settle(&mut app, 2);
    }
    let v = app.view().unwrap();
    let d = v.doc.as_ref().unwrap();
    let ops: Vec<&Op> = d.project.operations.iter().map(|o| &o.op).collect();
    assert_eq!(ops.len(), 2, "{ops:?}");
    let Op::Move { items } = ops[0] else { panic!() };
    assert_eq!(items[0].guid, guid);
    assert_eq!(f64::from(items[0].to[3][0]) - start[12], 10.0, "10 m on the 10 m grid");
    // the live partition: the object at the scene's transform, and only its Transform changed
    let base = EditDbx::parse(d.scene.file(0).unwrap().1).unwrap();
    let edited = d.partition().unwrap();
    let w = LevelWriter::parse(&edited).unwrap();
    let now = d.scene.entities[k].matrix.unwrap();
    assert_eq!(w.transform(&guid).unwrap(), bc2_levelwrite::rows_of(&now));
    assert_eq!(
        diff(&base, &w.d),
        vec![Change::Field(guid.to_ascii_uppercase(), "Transform".into())]
    );
    let msg = d.check();
    assert!(
        msg.starts_with("2 operation(s) apply cleanly") && !msg.contains("differs"),
        "{msg}"
    );
    println!("MP_005 project: {msg}");
    let _ = std::fs::remove_dir_all(&dir);
}
