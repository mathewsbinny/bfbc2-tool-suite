//! The game-accurate view on Package levels (round R5c): a Vietnam level and the Pacific War's Wake Island, opened
//! as the editor opens them (G3: prepared from their bundles), drawn with their own Dx11 databases read from the
//! bundles in place. What each level is missing is printed (the view's notes), with its frame costs.
//!
//! Runs only with `BC2_R5C_PACKAGE_OPFOR` naming an OPFOR folder (a stand-in whose `game` is the game copy: the
//! editor prepares the levels into its `work\editor\levels`), and `BC2_R5C_PACKAGE_LEVELS` (default
//! `nam_mp_002cq,wake_island_s`). `BC2_EDITOR_SHOTS=<folder>` saves each view.
#![cfg(windows)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    clippy::redundant_closure_for_method_calls,
    clippy::panic,
    missing_docs
)]

use std::path::PathBuf;

use bc2_editor::actions::Action;
use bc2_editor::app::EditorApp;
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::headless::Headless;

#[test]
fn package_levels_in_the_game_accurate_view() {
    let _gate = crate::gate();
    let Some(root) = std::env::var_os("BC2_R5C_PACKAGE_OPFOR").map(PathBuf::from) else {
        println!("skipped: BC2_R5C_PACKAGE_OPFOR not set");
        return;
    };
    let levels = std::env::var("BC2_R5C_PACKAGE_LEVELS").unwrap_or_else(|_| "nam_mp_002cq,wake_island_s".into());
    let shots = std::env::var_os("BC2_EDITOR_SHOTS").map(PathBuf::from);
    let mut h = Headless::new([1600, 950]).unwrap();
    for lv in levels.split(',') {
        let mut app = EditorApp::new(Some(&root), None).unwrap();
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        let t = std::time::Instant::now();
        app.open_level_blocking(&gpu, lv).unwrap();
        println!(
            "{lv}: opened (prepared if needed) in {:.1} s",
            t.elapsed().as_secs_f64()
        );
        // the camera on the first capture point (else the first objective, else the level's first object)
        let target = {
            let s = app.view().unwrap().scene();
            let by = |c: &str| s.entities.iter().find(|e| e.category == c).and_then(|e| e.position());
            by("capture point")
                .or_else(|| by("objective"))
                .or_else(|| s.entities.iter().find_map(|e| e.position()))
                .unwrap()
        };
        {
            let v = app.view_mut().unwrap();
            v.labels = false;
            v.viewports.get_mut(&Projection::Perspective).unwrap().camera = Camera {
                target,
                yaw: 0.7,
                pitch: 0.35,
                distance: 90.0,
                fov: 60.0,
                projection: Projection::Perspective,
            };
        }
        h.settle(&mut app, 2);
        let ctx = h.ctx.clone();
        app.run_action(&gpu, &ctx, Action::ToggleGameView);
        let Some(g) = app.view().unwrap().game.as_ref() else {
            panic!(
                "{lv}: the game-accurate view did not load: {}",
                app.view().unwrap().message
            );
        };
        println!("{lv}: game-accurate view loaded in {:.1} s", g.load_seconds);
        for n in &g.notes {
            println!("  note: {n}");
        }
        h.settle(&mut app, 4);
        let mut all = Vec::new();
        for k in 0..12 {
            app.view_mut()
                .unwrap()
                .viewports
                .get_mut(&Projection::Perspective)
                .unwrap()
                .camera
                .yaw += 0.002 * f64::from(k + 1);
            h.settle(&mut app, 1);
            all.push(app.view().unwrap().game.as_ref().unwrap().timings);
        }
        let g = app.view().unwrap().game.as_ref().unwrap();
        assert!(g.error.is_none(), "{lv}: {:?}", g.error);
        let n = (all.len() - 2) as f64;
        let avg = |f: fn(&bc2_editor::gameview::Timings) -> f64| all.iter().skip(2).map(f).sum::<f64>() / n * 1e3;
        let last = all.last().unwrap();
        println!(
            "{lv}: {} of {} draws, frame {:.1} ms = build {:.1} + submit {:.1} + GPU {:.1} + post {:.1} + {} {:.1}; \
             {:?}",
            last.draws,
            last.listed,
            avg(bc2_editor::gameview::Timings::total),
            avg(|t| t.build),
            avg(|t| t.submit),
            avg(|t| t.readback),
            avg(|t| t.tonemap),
            if last.shared { "shared" } else { "upload" },
            avg(|t| t.upload),
            g.world().counts
        );
        assert!(last.draws > 0, "{lv}: nothing drawn");
        if let Some(d) = &shots {
            h.save(&d.join(format!("r5c_{lv}.png"))).unwrap();
        }
    }
}
