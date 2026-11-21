//! Round DF1's proofs (audit AU4, "the editor's state and gating"), in the running editor without a window on the
//! demo level (skipped without a graphics adapter) and on the parts that need none:
//!
//! - **AU4-SM-1**: a Create that fails leaves no era / mode target for the next project opened.
//! - **AU4-SM-2**: the Export window lets go of a project when the map closes, and never writes into another one.
//! - **AU4-DS-4**: Export waits for the open project's own saved state (not the autosave file's absence).
//! - **AU4-CON-1 / BUG-4 / DS-13**: Export and Install wait for each other; a worker that ends without an answer is
//!   reported; closing the editor while either runs asks first.
//! - **AU4-SM-3**: Install now installs the stage the plan was shown for, and refuses one exported again since.
//! - **AU4-SM-4**: a preview being opened is dropped when the map closes or another project opens.
//! - **AU4-CON-2**: Install, Restore, Build and Close map wait for a running navkit command; a job dropped while it
//!   runs stops its child and removes its half-written file.
//! - **AU4-CQ-2**: closing the map frees its views' textures in egui's renderer.
//! - **AU4-CQ-8**, **BUG-14**, **BUG-15**: the recent list's paths, the export settings written whole, a failed game
//!   pick picking nothing.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    missing_docs
)]

use std::path::{Path, PathBuf};
use std::sync::mpsc::channel;

use bc2_editor::actions::Action;
use bc2_editor::app::{EditorApp, Then, game_pick, same_file};
use bc2_editor::demo;
use bc2_editor::export_ui::{self, NO_PROJECT, WAIT_INSTALL};
use bc2_editor::headless::Headless;
use bc2_editor::render;
use bc2_levelwrite::{MapProject, Profile};

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_df1_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// The editor on the demo level as project `name` in `dir` (its file returned); `None` without an adapter.
fn with_project(name: &str, dir: &Path) -> Option<(Headless, EditorApp, PathBuf)> {
    let mut h = match Headless::new([1280, 800]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return None;
        }
    };
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let scene = demo::level(&dir.join("level")).unwrap();
    let gpu = h.gpu.clone();
    app.open_scene_then(&gpu, "demo", scene, Then::Create(name.into(), dir.to_path_buf()));
    h.settle(&mut app, 3);
    let path = app.view().unwrap().doc.as_ref().unwrap().path.clone();
    Some((h, app, path))
}

fn ops(app: &EditorApp) -> usize {
    app.view().unwrap().doc.as_ref().unwrap().project.operations.len()
}

#[test]
fn a_failed_create_leaves_no_target_for_the_next_project() {
    let _gate = crate::gate();
    let dir = temp("sm1");
    let Some((mut h, mut app, x)) = with_project("DF1 X", &dir) else {
        return;
    };
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    let saved = std::fs::read(&x).unwrap();
    let before = ops(&app);
    app.run_action(&gpu, &ctx, Action::CloseMap);
    // the new-project form's target, then a Create that fails (no name)
    app.set_pending_target(Profile::Ww2, "conquest");
    let scene = demo::level(&dir.join("level")).unwrap();
    app.open_scene_then(&gpu, "demo", scene.clone(), Then::Create("  ".into(), dir.clone()));
    assert!(app.view().is_none(), "the Create failed");
    assert!(app.error().is_some_and(|e| e.contains("name")), "{:?}", app.error());
    // the next project opened keeps its era and mode: no operation, nothing written
    app.open_scene_then(&gpu, "demo", scene, Then::Open(x.clone(), None));
    h.settle(&mut app, 2);
    assert_eq!(
        app.error(),
        None,
        "no target was tried on X (the failed Create's error went with it)"
    );
    assert_eq!(ops(&app), before, "X's log is as it was");
    assert!(app.view().unwrap().doc.as_ref().unwrap().project.mode().is_none());
    assert_eq!(std::fs::read(&x).unwrap(), saved, "X's file is as it was");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_export_window_lets_go_of_a_closed_project() {
    let _gate = crate::gate();
    let dir = temp("sm2");
    let Some((mut h, mut app, a)) = with_project("DF1 A", &dir) else {
        return;
    };
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    app.export.strip.open = false;
    app.run_action(&gpu, &ctx, Action::Export);
    h.settle(&mut app, 2);
    assert!(app.export.open);
    assert_eq!(app.export.project.as_deref(), Some(a.as_path()));
    assert_eq!(app.export.bound, Some(app.generation()));
    // AU4-DS-4: the project's own saved state decides (no autosave file is written by this flag)
    app.view_mut().unwrap().doc.as_mut().unwrap().saved = false;
    h.settle(&mut app, 1);
    assert!(
        app.export
            .why_not
            .as_deref()
            .is_some_and(|w| w.starts_with("Save the project first")),
        "{:?}",
        app.export.why_not
    );
    app.view_mut().unwrap().doc.as_mut().unwrap().saved = true;
    // the map closed: the window, its form and its target go
    let generation = app.generation();
    app.run_action(&gpu, &ctx, Action::CloseMap);
    assert!(app.generation() > generation);
    assert!(!app.export.open);
    assert!(app.export.project.is_none() && app.export.spec.is_none() && app.export.target.is_none());
    // project B: nothing of A's reaches it
    let scene = demo::level(&dir.join("level")).unwrap();
    app.open_scene_then(&gpu, "demo", scene, Then::Create("DF1 B".into(), dir.clone()));
    h.settle(&mut app, 3);
    let b_ops = ops(&app);
    assert!(!app.export.open);
    // a window opened for A while B is shown (the old bug's state) acts on nothing and says so
    app.export.open_for(&a, None);
    h.settle(&mut app, 3);
    assert_eq!(app.export.why_not.as_deref(), Some(NO_PROJECT));
    assert_eq!(app.export.bound, None, "never bound to B");
    assert_eq!(ops(&app), b_ops, "no operation of A's form committed into B");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_and_install_wait_for_each_other_and_a_dead_worker_is_reported() {
    let _gate = crate::gate();
    let dir = temp("con1");
    let Some((mut h, mut app, _)) = with_project("DF1 C", &dir) else {
        return;
    };
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    app.export.strip.open = false;
    app.run_action(&gpu, &ctx, Action::Export);
    h.settle(&mut app, 2);
    assert_eq!(app.busy_work(), None);
    assert!(app.close_requested(), "nothing runs: the window may close");
    // an install runs: Export waits, in words; closing asks first
    let (tx, rx) = channel::<(bool, String)>();
    app.export.install.running_from(rx);
    h.settle(&mut app, 1);
    assert_eq!(app.export.why_not.as_deref(), Some(WAIT_INSTALL));
    assert!(app.busy_work().is_some_and(|w| w.contains("install")));
    assert!(!app.close_requested());
    assert!(app.close_asked.is_some());
    // the install's worker ends without an answer (it panicked): a failed run, said, never polled forever
    drop(tx);
    h.settle(&mut app, 2);
    assert!(!app.export.install.busy());
    assert!(!app.export.install.ok);
    assert!(
        app.export
            .install
            .text
            .iter()
            .any(|l| l.contains("stopped unexpectedly")),
        "{:?}",
        app.export.install.text
    );
    assert_ne!(app.export.why_not.as_deref(), Some(WAIT_INSTALL));
    assert!(
        app.close_asked.is_none(),
        "the close question goes once the work has ended"
    );
    // an export runs: closing asks; its worker ending without an answer ends 'Exporting...'
    let (tx, rx) = channel::<Result<Vec<String>, String>>();
    app.export.running_from(rx);
    h.settle(&mut app, 1);
    assert_eq!(app.export.why_not.as_deref(), Some("Exporting..."));
    assert!(app.busy_work().is_some_and(|w| w.contains("export")));
    drop(tx);
    h.settle(&mut app, 1);
    assert!(!app.export.exporting());
    assert!(
        app.export
            .log
            .iter()
            .any(|l| l.starts_with("Not exported") && l.contains("stopped")),
        "{:?}",
        app.export.log
    );
    // a worker's panic is an error line with its text
    let r: Result<(), String> = export_ui::guarded("The export", || panic!("a damaged bundle"));
    assert!(r.unwrap_err().contains("a damaged bundle"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn install_now_takes_the_planned_stage_only() {
    let _gate = crate::gate();
    let dir = temp("sm3");
    let stage = dir.join("work").join("mapedit").join("mapedit_x_cq").join("stage");
    std::fs::create_dir_all(&stage).unwrap();
    std::fs::write(stage.join("stage.json"), b"{\"first\": 1}").unwrap();
    let mut install = export_ui::Install::default();
    assert!(install.stale_plan().is_some(), "no plan shown");
    install.planned = Some((stage.clone(), std::fs::read(stage.join("stage.json")).unwrap()));
    assert_eq!(install.stale_plan(), None);
    // exported again after the plan: refused
    std::fs::write(stage.join("stage.json"), b"{\"second\": 2}").unwrap();
    assert!(install.stale_plan().is_some_and(|w| w.contains("exported again")));
    // one stage path, from the id as the export makes it (a name typed with spaces is the same level)
    let p = MapProject::new("X", "sp_01", "levels/SP_01", b"x", Profile::Bfbc2);
    let mut spec = export_ui::default_spec(&p, None);
    spec.level = "  mapedit_x_cq ".into();
    assert_eq!(export_ui::stage_of(&dir, &spec), Some(stage));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_preview_being_opened_goes_with_its_project() {
    let _gate = crate::gate();
    let dir = temp("sm4");
    let Some((mut h, mut app, _)) = with_project("DF1 P", &dir) else {
        return;
    };
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    let stage = dir.join("no-stage");
    app.start_preview(&stage);
    if app.preview_opening().is_none() {
        println!("skipped: no game copy (no preview starts)");
        return;
    }
    app.run_action(&gpu, &ctx, Action::CloseMap);
    assert_eq!(app.preview_opening(), None, "dropped with the map");
    // asked on one project, answered on another: not shown there
    let scene = demo::level(&dir.join("level")).unwrap();
    app.open_scene_then(&gpu, "demo", scene.clone(), Then::View);
    app.start_preview(&stage);
    app.open_scene_then(&gpu, "demo", scene, Then::View);
    h.settle(&mut app, 4);
    assert_eq!(app.preview_opening(), None);
    assert!(
        !app.view().unwrap().message.contains("preview"),
        "{}",
        app.view().unwrap().message
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn navkit_commands_wait_for_the_one_running() {
    let _gate = crate::gate();
    let dir = temp("con2");
    let Some((mut h, mut app, _)) = with_project("DF1 N", &dir) else {
        return;
    };
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    app.run_action(&gpu, &ctx, Action::SetWorkspace(bc2_editor::app::Workspace::Ai));
    // a stand-in for a long navkit build, writing its output
    let partial = dir.join("pathdata_dice_edited.hpd");
    std::fs::write(&partial, b"half written").unwrap();
    let child = std::process::Command::new("python")
        .args(["-c", "import time; time.sleep(60)"])
        .spawn();
    let Ok(child) = child else {
        println!("skipped: no python");
        return;
    };
    let mut job = bc2_editor::nav::Job::from_child(child, "apply");
    job.partial = Some(partial.clone());
    app.view_mut().unwrap().navs.job = Some(job);
    h.settle(&mut app, 1);
    let running = bc2_editor::app::NAV_RUNNING;
    for a in [Action::NavInstall, Action::NavRestore, Action::NavBuild] {
        let why = app.view().unwrap().nav_why_not(a);
        assert_eq!(why, Some(running), "{a:?}");
    }
    assert!(app.why_not(Action::CloseMap).is_some_and(|w| w.contains("navkit")));
    assert!(app.busy_work().is_some_and(|w| w.contains("navkit")));
    // the job dropped while it runs (the editor closed anyway): its child stopped, its half-written file gone
    app.view_mut().unwrap().navs.job = None;
    assert!(!partial.exists(), "the half-written build was removed");
    assert_eq!(app.why_not(Action::CloseMap), None);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn closing_the_map_frees_its_textures() {
    let _gate = crate::gate();
    let dir = temp("cq2");
    let Some((h, mut app, _)) = with_project("DF1 T", &dir) else {
        return;
    };
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    let id = app
        .view()
        .unwrap()
        .viewports
        .values()
        .find_map(bc2_editor::app::Viewport::texture_id)
        .expect("the 3D view's texture");
    assert!(h.gpu.egui.read().texture(&id).is_some());
    app.run_action(&gpu, &ctx, Action::CloseMap);
    assert!(h.gpu.egui.read().texture(&id).is_none(), "freed with its view");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn paths_settings_and_picks() {
    let _gate = crate::gate();
    // AU4-CQ-8: one file, however it is spelled on Windows
    if cfg!(windows) {
        assert!(same_file(
            Path::new(r"C:\Maps\a.mapedit.json"),
            Path::new("c:/maps/A.mapedit.json")
        ));
    }
    assert!(!same_file(Path::new("a.mapedit.json"), Path::new("b.mapedit.json")));
    // AU4-BUG-14: the settings written whole (over an older file), nothing left beside them
    let dir = temp("bug14");
    let f = dir.join("p.mapedit.json.export.json");
    std::fs::write(&f, b"old").unwrap();
    export_ui::write_whole(&f, b"{\"new\": true}\n").unwrap();
    assert_eq!(std::fs::read(&f).unwrap(), b"{\"new\": true}\n");
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1, "no temporary file left");
    let _ = std::fs::remove_dir_all(&dir);
    // AU4-BUG-15: a failed game pick picks nothing and says why
    let (id, why) = game_pick(Err("no game-accurate frame drawn yet".into()));
    assert_eq!(id, render::ID_NONE);
    assert!(why.is_some_and(|w| w.contains("no game-accurate frame")));
    assert_eq!(game_pick(Ok(Some(3))), (render::entity_id(3), None));
    assert_eq!(game_pick(Ok(None)), (render::ID_NONE, None));
}
