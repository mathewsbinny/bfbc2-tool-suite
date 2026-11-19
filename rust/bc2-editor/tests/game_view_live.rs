//! The game-accurate view made interactive (round R5c, `EDITOR_ARCHITECTURE.md` 3.11 phase 3) on MP_005, headless:
//!
//! - the frame reaches egui through the shared texture, and the window's picture is the same as with the copy
//!   fallback (read back and uploaded), pixel for pixel;
//! - a click on the warehouse in the game frame selects the warehouse (picked from the game frame's own draws), and
//!   the selection is outlined on the game frame;
//! - an edit (the selection moved) followed by the view gives the same frame as the level loaded again with the
//!   edit, bit for bit;
//! - frame costs, shared and copied (`--nocapture`).
//!
//! Skipped (with a note) without the game copy, MP_005 prepared, a graphics adapter or Direct3D 11.
//! `BC2_EDITOR_SHOTS=<folder>` saves the selection, the edit and the views.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::chunks_exact_to_as_chunks,
    missing_docs
)]

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::gameview::{GameView, Timings};
use bc2_editor::headless::Headless;
use bc2_editor::levels;

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

fn costs(h: &mut Headless, app: &mut EditorApp) -> Timings {
    let mut all = Vec::new();
    for k in 0..12 {
        app.view_mut()
            .unwrap()
            .viewports
            .get_mut(&Projection::Perspective)
            .unwrap()
            .camera
            .yaw += 0.002 * f64::from(k + 1);
        h.settle(app, 1);
        all.push(app.view().unwrap().game.as_ref().unwrap().timings);
    }
    let n = (all.len() - 2) as f64;
    let mut t = Timings::default();
    for c in all.iter().skip(2) {
        t.build += c.build / n;
        t.submit += c.submit / n;
        t.readback += c.readback / n;
        t.tonemap += c.tonemap / n;
        t.upload += c.upload / n;
        t.draws = c.draws;
        t.listed = c.listed;
        t.shared = c.shared;
    }
    t
}

fn print(what: &str, t: &Timings) {
    println!(
        "{what}: {} of {} draws, frame {:.1} ms = build {:.1} + submit {:.1} + GPU {:.1} + post {:.1} + {} {:.1}",
        t.draws,
        t.listed,
        t.total() * 1e3,
        t.build * 1e3,
        t.submit * 1e3,
        t.readback * 1e3,
        t.tonemap * 1e3,
        if t.shared { "shared" } else { "upload" },
        t.upload * 1e3
    );
}

#[test]
fn game_view_shares_picks_outlines_and_follows_edits() {
    let Ok(opfor) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
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
    let ctx = h.ctx.clone();
    app.run_action(&gpu, &ctx, Action::ToggleGameView);
    if app.view().unwrap().game.is_none() {
        println!("skipped: {}", app.view().unwrap().message);
        return;
    }
    h.settle(&mut app, 4);
    let shots = std::env::var_os("BC2_EDITOR_SHOTS").map(std::path::PathBuf::from);
    let save = |h: &mut Headless, name: &str| {
        if let Some(d) = &shots {
            h.save(&d.join(name)).unwrap();
        }
    };
    // shared against copied: the same window, pixel for pixel
    let shared_on = gpu
        .gpu
        .device
        .features()
        .contains(eframe::egui_wgpu::wgpu::Features::VULKAN_EXTERNAL_MEMORY_WIN32);
    let t_shared = costs(&mut h, &mut app);
    print("shared", &t_shared);
    {
        let g = app.view().unwrap().game.as_ref().unwrap();
        assert!(g.error.is_none(), "{:?}", g.error);
        assert_eq!(
            g.timings.shared, shared_on,
            "the frame is shared when egui's device can open it"
        );
    }
    let shared_img = h.image().unwrap();
    app.view_mut().unwrap().game.as_mut().unwrap().copy = true;
    app.view_mut().unwrap().game.as_mut().unwrap().redraw();
    h.settle(&mut app, 2);
    let copied_img = h.image().unwrap();
    // the costs line at the view's top (its numbers change every frame) left out
    let r = app
        .view()
        .unwrap()
        .viewports
        .get(&Projection::Perspective)
        .unwrap()
        .rect
        .unwrap();
    let text = (r.top() as usize)..(r.top() as usize + 24);
    let width = 1600;
    let same = shared_img
        .chunks_exact(4)
        .zip(copied_img.chunks_exact(4))
        .enumerate()
        .filter(|(k, (a, b))| a == b || text.contains(&(k / width)))
        .count();
    println!(
        "shared texture ({}) against the copy fallback: {same} of {} window pixels identical",
        if shared_on { "on" } else { "not available" },
        shared_img.len() / 4
    );
    assert_eq!(same, shared_img.len() / 4);
    let t_copy = costs(&mut h, &mut app);
    print("copy fallback", &t_copy);
    app.view_mut().unwrap().game.as_mut().unwrap().copy = false;
    {
        let v = app.view_mut().unwrap();
        v.viewports.get_mut(&Projection::Perspective).unwrap().camera = warehouse_camera();
    }
    h.settle(&mut app, 3);
    save(&mut h, "r5c_game_view.png");
    let before = h.image().unwrap();
    // a click at the view's centre (the camera's target: the warehouse) picks from the game frame
    let rect = app
        .view()
        .unwrap()
        .viewports
        .get(&Projection::Perspective)
        .unwrap()
        .rect
        .unwrap();
    h.click(&mut app, rect.center());
    h.settle(&mut app, 3);
    let v = app.view().unwrap();
    let picked = v.selected.expect("the click picked something");
    let bp = v.scene().entities[picked].blueprint.clone().unwrap_or_default();
    println!("picked entity {picked}: {bp}");
    assert!(bp.to_lowercase().contains("warehouse"), "{bp}");
    save(&mut h, "r5c_selection.png");
    let after = h.image().unwrap();
    // the outline: the selection's edge in orange on the game frame
    let orange = before
        .chunks_exact(4)
        .zip(after.chunks_exact(4))
        .filter(|(a, b)| a != b && b[0] > 200 && b[1] > 80 && b[1] < 160 && b[2] < 60)
        .count();
    println!("outline: {orange} pixels turned orange");
    assert!(orange > 200, "{orange}");
    // an edit: the warehouse moved 6 m east and 1.5 m up; the view follows it, as the level loaded with it
    let size = [rect.width() as u32, rect.height() as u32];
    let mut scene = app.view().unwrap().scene().clone();
    {
        let m = scene.entities[picked].matrix.as_mut().unwrap();
        m[12] += 6.0;
        m[13] += 1.5;
    }
    let cam = warehouse_camera();
    let t = std::time::Instant::now();
    let g = app.view_mut().unwrap().game.as_mut().unwrap();
    assert!(g.sync(&scene));
    let replayed = g.read_back(&cam, size, &[]).unwrap();
    let replay = t.elapsed().as_secs_f64();
    let mut fresh = GameView::load(&opfor, "mp_005", &scene).unwrap();
    let scratch = fresh.read_back(&cam, size, &[]).unwrap();
    let n = scratch.depth.len();
    let same = (0..n)
        .filter(|&k| {
            replayed.hdr[4 * k..4 * k + 4]
                .iter()
                .zip(&scratch.hdr[4 * k..4 * k + 4])
                .all(|(a, b)| a.to_bits() == b.to_bits())
                && replayed.classes[k] == scratch.classes[k]
                && replayed.depth[k].to_bits() == scratch.depth[k].to_bits()
        })
        .count();
    println!(
        "edit replayed and drawn in {:.0} ms: {same} of {n} pixels identical to the level loaded again with the edit",
        replay * 1e3
    );
    assert_eq!(same, n);
    assert_eq!(replayed.ldr, scratch.ldr);
    if let Some(d) = &shots {
        let g = app.view_mut().unwrap().game.as_mut().unwrap();
        let shown = g.read_back(&cam, size, &[picked]).unwrap();
        let file = std::fs::File::create(d.join("r5c_edit_moved.png")).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size[0], size[1]);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&shown.ldr).unwrap();
    }
}
