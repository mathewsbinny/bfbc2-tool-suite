//! Round A1's proof in the running editor, without a window (game tier; skipped without the game copy or a graphics
//! adapter; `BC2_EDITOR_SHOTS=<folder>` saves the screenshots, Dark and Light): the Gameplay workspace's auto layout
//! proposes a layout of the project's mode layer on a stock level's ground, the views draw it, nothing changes until
//! it is accepted, and the accepted layout passes the export's live validation.
//!
//! Laguna Alta (`MP_003`) as a project without its own gameplay: the export's target is BFBC2 Conquest from Panama
//! Canal's layer (`MP_001`, the donor `examples/a1_layout.rs` picks for it), which takes the level's own layer out. The
//! proposal is made on the editor's worker as the panel's Propose makes it, drawn over the views (screenshot
//! `a1_proposal_<theme>.png`), accepted as one move operation, then the Problems panel lists no error and no warning
//! (`a1_validation_<theme>.png`); an undo takes the whole layout back.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    missing_docs
)]

use std::path::{Path, PathBuf};

use bc2_editor::actions::{Action, PanelKind};
use bc2_editor::app::{EditorApp, Then};
use bc2_editor::camera::{Camera, Projection};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::render;
use bc2_editor::theme::ThemeId;
use bc2_editor::validation;
use bc2_editor::workspace::Workspace;
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::ops::Op;
use bc2_levelwrite::{MapProject, Profile};

const SIZE: [u32; 2] = [1600, 1000];

fn shot(name: &str, h: &mut Headless) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        let p = Path::new(&dir).join(name);
        render::write_png(&p, SIZE, &h.image().unwrap()).unwrap();
        println!("saved {}", p.display());
    }
}

/// Laguna Alta with Panama Canal's Conquest layer, saved in `dir`.
fn laguna(game: &Game, dir: &Path) -> PathBuf {
    let base = export::level_partition(game, "mp_003").unwrap();
    let mut p = MapProject::new("A1 auto layout", "mp_003", "levels/MP_003", &base, Profile::Bfbc2);
    let c = export::choose_mode(game, &p, Profile::Bfbc2, "Conquest", Some("mp_001")).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let path = dir.join(p.file_name());
    p.save(&path).unwrap();
    path
}

#[test]
#[ignore = "game tier: a real level in the editor; run_tests.py game runs it (tests/test_rust.py run --game)"]
// spec: XVAL-PLACE-2, XVAL-PLACE-3, XVAL-PLACE-4, XLAY-AUTO-1
fn the_auto_layout_is_proposed_drawn_accepted_and_validated() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let Ok(mut h) = Headless::new(SIZE) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let game = Game::open(&root.join("game")).unwrap();
    let dir = std::env::temp_dir().join(format!("bc2_editor_a1_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = laguna(&game, &dir);
    let mut first: Option<bc2_levelwrite::autolayout::Proposal> = None;
    for theme in ThemeId::ALL {
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        if let Err(e) = app.open_level_blocking_then(&gpu, "mp_003", Then::Open(path.clone(), None)) {
            println!("skipped mp_003: {e}");
            return;
        }
        app.export.strip.open = false;
        let ctx = h.ctx.clone();
        app.run(&gpu, &ctx, Action::SetWorkspace(Workspace::Gameplay));
        app.run(&gpu, &ctx, Action::ShowPanel(PanelKind::Problems));
        app.run(&gpu, &ctx, Action::ShowPanel(PanelKind::AutoLayout));
        h.settle(&mut app, 3);
        let (revision, operations) = {
            let d = app.view().unwrap().doc.as_ref().unwrap();
            (d.revision, d.project.operations.len())
        };
        // Propose, as the panel does it: the site of the Problems panel, the worker
        let t = std::time::Instant::now();
        {
            let v = app.view_mut().unwrap();
            let d = v.doc.as_ref().unwrap();
            v.live.refresh(d);
            let site = v.live.site().expect("the base level's site");
            assert!(
                site.has_terrain() && site.triangles() > 100_000,
                "ground and collision read"
            );
            v.autolayout.seed = 0;
            v.autolayout.start(d, site, None);
        }
        while app.view().unwrap().autolayout.busy() {
            h.settle(&mut app, 1);
            assert!(t.elapsed().as_secs() < 120, "the proposal takes too long");
        }
        h.settle(&mut app, 3);
        let prop = {
            let v = app.view().unwrap();
            let d = v.doc.as_ref().unwrap();
            assert_eq!(d.revision, revision, "a proposal changes nothing");
            assert_eq!(d.project.operations.len(), operations);
            v.autolayout.current(d).cloned().expect("a proposal")
        };
        println!(
            "a1 {theme:?}: {} in {:.1} s; {} findings",
            prop.describe(),
            t.elapsed().as_secs_f64(),
            prop.findings.len()
        );
        assert_eq!(prop.bad_spots, 0);
        assert_eq!(prop.counts().0, 0, "no warning: {:#?}", prop.findings);
        let (inside, all) = prop.inside();
        assert_eq!(inside, all, "every statistic inside DICE's ranges: {:#?}", prop.stats);
        // the same seed on the same project: the same proposal in both themes
        match &first {
            Some(p) => assert_eq!(p, &prop, "deterministic"),
            None => first = Some(prop.clone()),
        }
        // the views on the proposal: its centre, from 700 m
        {
            let v = app.view_mut().unwrap();
            let y = v
                .live
                .site()
                .and_then(|s| s.ground(prop.centre[0], prop.centre[1]))
                .unwrap_or(0.0);
            v.labels = false;
            v.viewports.get_mut(&Projection::Perspective).unwrap().camera = Camera {
                target: [prop.centre[0], y, prop.centre[1]],
                yaw: 0.6,
                pitch: 0.75,
                distance: 650.0,
                fov: 60.0,
                projection: Projection::Perspective,
            };
            v.redraw();
        }
        h.settle(&mut app, 3);
        shot(&format!("a1_proposal_{}.png", theme.key()), &mut h);
        // Accept: one move operation; then the Problems panel lists no error and no warning
        let palette = app.palette();
        app.view_mut().unwrap().accept_layout(&gpu, &palette);
        assert!(!app.view().unwrap().autolayout.holds());
        app.run(&gpu, &ctx, Action::ShowPanel(PanelKind::Problems));
        h.settle(&mut app, 4);
        let rows = {
            let v = app.view_mut().unwrap();
            let d = v.doc.as_ref().unwrap();
            assert_eq!(d.project.operations.len(), operations + 1);
            v.live.refresh(d);
            validation::rows(d, &v.live)
        };
        let (e, w, n) = validation::counts(&rows);
        println!("a1 {theme:?}: after accepting, {e} errors, {w} warnings, {n} notes");
        assert_eq!((e, w), (0, 0), "{rows:#?}");
        assert!(!rows.iter().any(|r| r.code.starts_with("XVAL-PLACE")), "{rows:#?}");
        shot(&format!("a1_validation_{}.png", theme.key()), &mut h);
        // an undo takes the whole layout back
        app.view_mut().unwrap().doc.as_mut().unwrap().undo();
        h.settle(&mut app, 2);
        let d = app.view().unwrap().doc.as_ref().unwrap();
        assert_eq!(d.project.operations.len(), operations);
        let w = d.writer().unwrap();
        let m = &prop.moves[0];
        assert_eq!(w.transform(&m.guid), Some(m.from), "back where it stood");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
