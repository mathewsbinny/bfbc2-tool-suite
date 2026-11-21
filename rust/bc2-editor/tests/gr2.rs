//! Round GR2's proofs in the running editor, without a window: the Navigation Studio folded into the AI workspace.
//!
//! - `ONS_MP_004` viewed (skipped without the game copy): the navmesh mode reads NavKit's build and DICE's file on its
//!   worker, draws the ground (the view changes with each colouring), the outlines layer adds lines, the path test
//!   between two soldier spawns finds each file's route, a reference AI layer (`level:ons_mp_002`) is drawn and a
//!   marker selected, the Navigation, Islands and spawns and Navigation edits panels show; Dark and Light, saved with
//!   `BC2_EDITOR_SHOTS=<folder>`.
//! - The edit, build, install and restore round trip on a scratch copy (never the game copy): `SP_009` as a project
//!   whose navigation folder is a scratch OPFOR folder holding a copy of DICE's file (a DICE base: additive edits);
//!   edits drawn with the tools by clicks in the view go into the project's log (undo and redo them, saved with the
//!   project), Build with these edits runs `bfbc2.exe navkit apply` on the side file, Install shows its plan and
//!   writes the built file with DICE's kept, Restore puts DICE's back: the scratch game folder is then byte for
//!   byte as it was.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    missing_docs
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bc2_editor::actions::{Action, PanelKind};
use bc2_editor::app::{EditorApp, Tab, Then};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::nav::NavTool;
use bc2_editor::navview::ColourMode;
use bc2_editor::render;
use bc2_editor::theme::ThemeId;
use bc2_editor::workspace::{ViewMode, Workspace};
use eframe::egui::{Color32, Rect};

const SIZE: [u32; 2] = [1600, 1000];

fn shot(name: &str, h: &mut Headless) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        let p = Path::new(&dir).join(name);
        render::write_png(&p, SIZE, &h.image().unwrap()).unwrap();
        println!("saved {}", p.display());
    }
}

/// Pixels within `tol` of `colour` inside `rect` of the last frame.
fn count(h: &mut Headless, rect: Rect, colour: Color32, tol: u8) -> usize {
    let img = h.image().unwrap();
    let mut n = 0;
    for y in rect.top().max(0.0) as u32..(rect.bottom() as u32).min(SIZE[1]) {
        for x in rect.left().max(0.0) as u32..(rect.right() as u32).min(SIZE[0]) {
            let k = ((y * SIZE[0] + x) * 4) as usize;
            let near = |a: u8, b: u8| a.abs_diff(b) <= tol;
            if near(img[k], colour.r()) && near(img[k + 1], colour.g()) && near(img[k + 2], colour.b()) {
                n += 1;
            }
        }
    }
    n
}

/// The view's pixels (the 3D view's rectangle of the last frame).
fn view_pixels(h: &mut Headless, rect: Rect) -> Vec<u8> {
    let img = h.image().unwrap();
    let mut out = Vec::new();
    for y in rect.top() as u32..(rect.bottom() as u32).min(SIZE[1]) {
        for x in rect.left() as u32..(rect.right() as u32).min(SIZE[0]) {
            let k = ((y * SIZE[0] + x) * 4) as usize;
            out.extend_from_slice(&img[k..k + 3]);
        }
    }
    out
}

/// Two soldier spawns far apart (scene indices), on screen.
fn two_spawns(app: &EditorApp) -> (usize, usize) {
    let v = app.view().unwrap();
    let spawns: Vec<usize> = v
        .scene()
        .entities
        .iter()
        .filter(|e| e.category == "soldier spawn" && e.matrix.is_some())
        .map(|e| e.index)
        .filter(|&k| {
            app.project_entity(k)
                .is_some_and(|p| v.view_rect().unwrap().shrink(40.0).contains(p))
        })
        .collect();
    let mut best = (0.0, spawns[0], spawns[0]);
    for &a in &spawns {
        for &b in &spawns {
            let (pa, pb) = (app.project_entity(a).unwrap(), app.project_entity(b).unwrap());
            let d = pa.distance(pb);
            if d > best.0 {
                best = (d, a, b);
            }
        }
    }
    (best.1, best.2)
}

#[test]
fn ai_workspace_on_ons_mp_004() {
    let _gate = crate::gate();
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
        let ctx = h.ctx.clone();
        if let Err(e) = app.open_level_blocking(&gpu, "ons_mp_004") {
            println!("skipped ons_mp_004: {e}");
            return;
        }
        h.settle(&mut app, 3);
        let rect = app.view().unwrap().view_rect().unwrap();
        let plain = view_pixels(&mut h, rect);
        // the navmesh mode: both files read on the worker, the ground drawn
        app.run_action(&gpu, &ctx, Action::SetViewMode(ViewMode::Navmesh));
        let p = app.palette();
        app.view_mut().unwrap().nav_wait(&gpu, &p);
        h.settle(&mut app, 4);
        let v = app.view().unwrap();
        let d = v.navs.data().expect("read");
        assert_eq!(d.files.iter().map(|f| f.name).collect::<Vec<_>>(), ["NavKit", "DICE"]);
        assert!(d.same.is_some() && d.anomalies.is_some());
        assert!(v.mode_note.contains("NavKit and DICE"), "{}", v.mode_note);
        let island = view_pixels(&mut h, rect);
        assert_ne!(plain, island, "the navmesh drawn over the level");
        let main = count(&mut h, rect, p.view_nav_main, 40);
        assert!(main > 2000, "{main} main-network pixels");
        // each colouring draws differently
        let mut seen = vec![island];
        for mode in [
            ColourMode::Sector,
            ColourMode::Pitch,
            ColourMode::Difference,
            ColourMode::Anomaly,
        ] {
            assert_eq!(app.why_not(Action::NavColour(mode)), None, "{mode:?}");
            app.run_action(&gpu, &ctx, Action::NavColour(mode));
            h.settle(&mut app, 3);
            let img = view_pixels(&mut h, rect);
            assert!(seen.iter().all(|s| *s != img), "{mode:?} draws its own colours");
            seen.push(img);
        }
        app.run_action(&gpu, &ctx, Action::NavColour(ColourMode::Island));
        // a line layer: the outlines add lines
        let lines_before = app.view().unwrap().navs.lines.len();
        app.view_mut().unwrap().navs.hidden.remove("NavKit outlines");
        app.view_mut().unwrap().nav_refresh(&gpu, &p);
        h.settle(&mut app, 3);
        assert!(
            app.view().unwrap().navs.lines.len() > lines_before,
            "the outlines' lines made"
        );
        // the panels
        for k in [PanelKind::Navigation, PanelKind::NavIslands, PanelKind::NavEdits] {
            app.run_action(&gpu, &ctx, Action::ShowPanel(k));
        }
        assert!(app.tabs().contains(&Tab::Navigation) && app.tabs().contains(&Tab::NavIslands));
        // the path test between two spawns far apart: each file's route
        let (a, b) = two_spawns(&app);
        app.run_action(&gpu, &ctx, Action::PathTest);
        assert!(app.view().unwrap().navs.path.active);
        for k in [a, b] {
            let at = app.project_entity(k).unwrap();
            h.click(&mut app, at);
        }
        h.settle(&mut app, 3);
        let v = app.view().unwrap();
        assert_eq!(v.navs.path.points.len(), 2);
        assert_eq!(v.navs.path.routes.len(), 2, "a route (or why not) for each file");
        let found = v.navs.path.routes.iter().filter(|(_, r)| r.found).count();
        let costs: Vec<String> = v
            .navs
            .path
            .routes
            .iter()
            .map(|(n, r)| {
                if r.found {
                    format!("{n} {:.1} m", r.cost)
                } else {
                    format!("{n}: {}", r.reason)
                }
            })
            .collect();
        app.run_action(&gpu, &ctx, Action::PathTest);
        // the compare: a reference level's AI layer, a marker selected
        let root = levels::find_opfor(None).unwrap();
        let c = bc2_editor::nav::open_compare("level:ons_mp_002", &root).unwrap();
        let n_items = c.items().len();
        app.view_mut().unwrap().navs.compare = Some(c);
        app.view_mut().unwrap().nav_refresh(&gpu, &p);
        h.settle(&mut app, 3);
        let ref_px = count(&mut h, rect, p.view_ref_ai, 40);
        assert!(ref_px > 20, "{ref_px} reference AI spawner pixels");
        shot(&format!("gr2_ai_workspace_ons_mp_004_{}.png", theme.key()), &mut h);
        // the islands panel in front, the anomaly colouring
        app.run_action(&gpu, &ctx, Action::NavColour(ColourMode::Anomaly));
        app.run_action(&gpu, &ctx, Action::ShowPanel(PanelKind::NavIslands));
        h.settle(&mut app, 4);
        shot(&format!("gr2_islands_anomaly_ons_mp_004_{}.png", theme.key()), &mut h);
        app.run_action(&gpu, &ctx, Action::NavColour(ColourMode::Difference));
        h.settle(&mut app, 3);
        shot(&format!("gr2_difference_ons_mp_004_{}.png", theme.key()), &mut h);
        println!(
            "gr2 ons_mp_004 {theme:?}: NavKit and DICE read and drawn ({main} main-network pixels), 5 colourings, \
             outlines, path test ({found} of 2 found: {}), {n_items} reference markers ({ref_px} pixels)",
            costs.join(", ")
        );
    }
}

/// Every file under a folder: (relative path, SHA-1).
fn snapshot(dir: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![dir.to_owned()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else {
                let rel = p.strip_prefix(dir).unwrap().display().to_string();
                out.insert(
                    rel,
                    sha1_smol::Sha1::from(std::fs::read(&p).unwrap()).digest().to_string(),
                );
            }
        }
    }
    out
}

/// A scratch OPFOR folder for `lv`: the game's executable and the level's bundle as empty stand-ins (NavKit checks
/// they are there), and a copy of DICE's path data.
fn scratch_opfor(dir: &Path, root: &Path, lv: &str) -> PathBuf {
    let _ = std::fs::remove_dir_all(dir);
    let game = dir.join("game");
    std::fs::create_dir_all(game.join("Dist").join("win32").join("levels").join(lv)).unwrap();
    std::fs::write(game.join("BFBC2Game.exe"), b"").unwrap();
    std::fs::write(
        game.join("Dist")
            .join("win32")
            .join("levels")
            .join(lv)
            .join("level-00.fbrb"),
        b"",
    )
    .unwrap();
    let dice = bc2_editor::navdata::original_file(&bc2_editor::navdata::installed_pathdata(root, lv)).unwrap();
    let to = bc2_editor::navdata::installed_pathdata(dir, lv);
    std::fs::create_dir_all(to.parent().unwrap()).unwrap();
    std::fs::copy(&dice, &to).unwrap();
    std::fs::create_dir_all(dir.join("work")).unwrap();
    to
}

#[test]
fn edit_build_install_restore_on_a_scratch_copy() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let Ok(mut h) = Headless::new(SIZE) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let lv = "sp_009";
    let tmp = std::env::temp_dir().join(format!("bc2_editor_gr2_{}", std::process::id()));
    let scratch = tmp.join("opfor");
    let installed = scratch_opfor(&scratch, &root, lv);
    let original = std::fs::read(&installed).unwrap();
    let before = snapshot(&scratch.join("game"));
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    if let Err(e) = app.open_level_blocking_then(&gpu, lv, Then::Create("GR2 nav".into(), tmp.join("projects"))) {
        println!("skipped {lv}: {e}");
        return;
    }
    h.settle(&mut app, 3);
    // the navigation of the scratch copy: DICE's file only, a DICE base
    app.view_mut().unwrap().navs.root = Some(scratch.clone());
    app.run_action(&gpu, &ctx, Action::SetWorkspace(Workspace::Ai));
    let p = app.palette();
    app.view_mut().unwrap().nav_wait(&gpu, &p);
    h.settle(&mut app, 3);
    let v = app.view().unwrap();
    assert_eq!(
        v.navs.data().unwrap().files.iter().map(|f| f.name).collect::<Vec<_>>(),
        ["DICE"]
    );
    assert_eq!(v.nav_kind(), "dice");
    // region edits need a NavKit base; the additive ones are offered
    assert!(app.why_not(Action::NavTool(NavTool::Walkable)).is_some());
    assert_eq!(app.why_not(Action::NavTool(NavTool::EdgeAdd)), None);
    // two points a few metres apart around a soldier spawn, the camera close: one sector, two route nodes (a link or
    // route edge joins each point to its nearest node, both in one sector)
    // two route nodes of DICE's file in one sector, 15 - 40 m apart, on screen with the camera looking at them
    let (na, nb) = {
        let f = &app.view().unwrap().navs.data().unwrap().files[0].data;
        let n = f.node_pos.len();
        let mut found = None;
        'outer: for i in (0..n).step_by(7) {
            for j in 0..n {
                let (p, q) = (f.node_pos[i], f.node_pos[j]);
                let d = (p[0] - q[0]).hypot(p[2] - q[2]);
                if f.node_sector[i] == f.node_sector[j] && (15.0..40.0).contains(&d) && (p[1] - q[1]).abs() < 1.0 {
                    let g = |v: [f64; 3]| [v[0], v[1] - bc2_editor::navdata::Y_OFFSET, v[2]];
                    found = Some((g(p), g(q)));
                    break 'outer;
                }
            }
        }
        found.expect("two nodes of one sector")
    };
    let mid = [
        f64::midpoint(na[0], nb[0]),
        f64::midpoint(na[1], nb[1]),
        f64::midpoint(na[2], nb[2]),
    ];
    for vp in app.view_mut().unwrap().viewports.values_mut() {
        vp.camera.focus(mid, Some(60.0));
    }
    h.settle(&mut app, 4);
    let screen = |app: &EditorApp, p: [f64; 3]| {
        let v = app.view().unwrap();
        let rect = v.view_rect().unwrap();
        let cam = v.viewports[&bc2_editor::camera::Projection::Perspective].camera;
        let s = cam
            .view([f64::from(rect.width()), f64::from(rect.height())])
            .project(p)
            .unwrap();
        rect.min + eframe::egui::Vec2::new(s[0] as f32, s[1] as f32)
    };
    let (pa, pb) = (screen(&app, na), screen(&app, nb));
    // a route edge both ways, a ladder traversal, a link, a note: each two clicks (the note one)
    let mut made = 0;
    for tool in [NavTool::EdgeAdd, NavTool::Traversal, NavTool::LinkAreas] {
        app.run_action(&gpu, &ctx, Action::NavTool(tool));
        h.click(&mut app, pa);
        h.settle(&mut app, 30);
        h.click(&mut app, pb);
        made += 1;
        let n = app.view().unwrap().nav_edits().len();
        assert_eq!(n, made, "{tool:?}: {}", app.view().unwrap().navs.message);
    }
    app.run_action(&gpu, &ctx, Action::NavTool(NavTool::Note));
    app.view_mut().unwrap().navs.note = "GR2 round trip".into();
    h.click(&mut app, pa);
    made += 1;
    app.run_action(&gpu, &ctx, Action::NavTool(NavTool::Note));
    h.settle(&mut app, 2);
    let edits = app.view().unwrap().nav_edits();
    assert_eq!(edits.len(), made);
    assert_eq!(
        edits.iter().map(|e| e.op.as_str()).collect::<Vec<_>>(),
        ["route-edge", "traversal", "link-areas", "note"]
    );
    shot("gr2_edits_sp_009_dark.png", &mut h);
    // undo and redo: the project's log
    app.run_action(&gpu, &ctx, Action::Undo);
    assert_eq!(app.view().unwrap().nav_edits().len(), made - 1);
    app.run_action(&gpu, &ctx, Action::Redo);
    assert_eq!(app.view().unwrap().nav_edits().len(), made);
    app.run_action(&gpu, &ctx, Action::Save);
    let project = app.view().unwrap().doc.as_ref().unwrap().path.clone();
    let text = std::fs::read_to_string(&project).unwrap();
    assert!(
        text.contains("nav-add") && text.contains("GR2 round trip"),
        "saved with the project"
    );
    // the side file, in navtune's form
    let side = app.view_mut().unwrap().nav_save_side().unwrap();
    let (base, back) = bc2_editor::nav::read_side(&side).unwrap();
    assert_eq!(base.kind, "dice");
    assert_eq!(back, edits);
    // build: navkit apply on a copy of DICE's file
    assert_eq!(app.why_not(Action::NavInstall), Some("Build with these edits first"));
    app.run_action(&gpu, &ctx, Action::NavBuild);
    let ok = app.view_mut().unwrap().navs.job.as_mut().unwrap().wait();
    let out = app.view().unwrap().navs.job.as_ref().unwrap().output();
    assert!(ok, "navkit apply failed:\n{}", out.join("\n"));
    let built = app.view().unwrap().nav_out().unwrap();
    assert!(built.ends_with("pathdata_dice_edited.hpd") && built.starts_with(&scratch));
    let built_bytes = std::fs::read(&built).unwrap();
    assert_ne!(built_bytes, original, "the edits are in the built file");
    assert_eq!(
        snapshot(&scratch.join("game")),
        before,
        "the build wrote nothing in the game folder"
    );
    // install: the plan first, then confirmed
    app.run_action(&gpu, &ctx, Action::NavInstall);
    let plan = app.view().unwrap().navs.confirm.clone().unwrap();
    assert!(plan.blocked.is_none(), "{plan:?}");
    assert!(plan.lines[0].starts_with("Keep DICE's file"), "{:?}", plan.lines);
    assert!(
        plan.touches.iter().all(|t| t.starts_with(scratch.join("game"))),
        "{:?}",
        plan.touches
    );
    h.settle(&mut app, 2);
    shot("gr2_install_plan_sp_009_dark.png", &mut h);
    let confirm = app
        .view()
        .unwrap()
        .navs
        .rects
        .iter()
        .find(|(n, _)| n == "confirm")
        .map(|(_, r)| *r);
    match confirm {
        Some(r) => h.click(&mut app, r.center()),
        None => app.view_mut().unwrap().nav_confirm(true),
    }
    h.settle(&mut app, 2);
    assert!(app.view().unwrap().navs.confirm.is_none());
    assert_eq!(std::fs::read(&installed).unwrap(), built_bytes, "installed");
    // DICE's file kept once in the game copy's install ledger (round DF2)
    let kept = bc2_editor::navdata::original_file(&installed).expect("DICE's kept in the ledger");
    assert!(
        kept.starts_with(scratch.join("game").join("opfor").join("ledger")),
        "{kept:?}"
    );
    assert_eq!(std::fs::read(&kept).unwrap(), original, "DICE's kept");
    let trav = installed.parent().unwrap().join(format!("{lv}.traversal.json"));
    assert!(trav.is_file(), "the traversal annotations installed beside it");
    // restore: the plan, confirmed: the game folder as it was
    app.run_action(&gpu, &ctx, Action::NavRestore);
    let plan = app.view().unwrap().navs.confirm.clone().unwrap();
    assert!(
        plan.lines[0].starts_with("Put the file from before NavKit back"),
        "{:?}",
        plan.lines
    );
    app.view_mut().unwrap().nav_confirm(true);
    h.settle(&mut app, 2);
    let after = snapshot(&scratch.join("game"));
    assert_eq!(after, before, "the scratch game folder byte for byte as it was");
    assert_eq!(
        app.why_not(Action::NavRestore),
        Some("Nothing to restore: no NavKit file is installed")
    );
    println!(
        "gr2 {lv}: {made} edits drawn (undo, redo, saved), the side file read back, built ({} bytes), installed with \
         DICE's kept and the traversals beside it, restored: {} files byte for byte as before",
        built_bytes.len(),
        after.len()
    );
    let _ = std::fs::remove_dir_all(&tmp);
}
