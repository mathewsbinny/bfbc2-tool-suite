//! Round V1's proof in the running editor, without a window (game tier; skipped without the game copy or a graphics
//! adapter; `BC2_EDITOR_SHOTS=<folder>` saves the screenshots, Dark and Light): the Problems panel shows the export's
//! rules live as the project is edited.
//!
//! E3d's canary (Atacama's Rush, `MP_005GR`, made a BFBC2 Conquest level) opened as a project; two edits through the
//! document as the tools make them: flag B moved 20 m from flag A (L1-CQ-002, a warning quoting DICE's 58-450 m) and
//! a prop moved 3 km out (XVAL-CULL-4, an error: the cull tree holds +-2048 m). After each change the panel's rows are
//! made again (the document's revision moved on); each names its objects, which the scene has (its "Go to" link). An
//! undo takes the error away.
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
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::render;
use bc2_editor::theme::ThemeId;
use bc2_editor::validation;
use bc2_levelwrite::export::{self, Game};
use bc2_levelwrite::ops::{Moved, Op};
use bc2_levelwrite::rules::Severity;
use bc2_levelwrite::{MapProject, Profile};

const SIZE: [u32; 2] = [1600, 1000];

fn shot(name: &str, h: &mut Headless) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        let p = Path::new(&dir).join(name);
        render::write_png(&p, SIZE, &h.image().unwrap()).unwrap();
        println!("saved {}", p.display());
    }
}

/// The canary project, saved in `dir`.
fn atacama(game: &Game, dir: &Path) -> PathBuf {
    let base = export::level_partition(game, "mp_005gr").unwrap();
    let mut p = MapProject::new("V1 problems", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let c = export::choose_mode(game, &p, Profile::Bfbc2, "Conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let path = dir.join(p.file_name());
    p.save(&path).unwrap();
    path
}

#[test]
#[ignore = "game tier: a real level in the editor; run_tests.py game runs it (tests/test_rust.py run --game)"]
// spec: XVAL-CULL-4
fn problems_panel_shows_the_rules_live() {
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
    let dir = std::env::temp_dir().join(format!("bc2_editor_v1_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path = atacama(&game, &dir);
    for theme in ThemeId::ALL {
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        if let Err(e) = app.open_level_blocking_then(&gpu, "mp_005gr", Then::Open(path.clone(), None)) {
            println!("skipped mp_005gr: {e}");
            return;
        }
        app.export.strip.open = false;
        let ctx = h.ctx.clone();
        app.run(&gpu, &ctx, Action::ShowPanel(PanelKind::Problems));
        h.settle(&mut app, 3);
        // the canary as chosen: no error
        let rows = {
            let v = app.view_mut().unwrap();
            let d = v.doc.as_ref().unwrap();
            v.live.refresh(d);
            validation::rows(d, &v.live)
        };
        assert!(rows.iter().all(|r| r.severity != Severity::Error), "{rows:#?}");
        let before = rows.len();
        // the edits: flag B 20 m from flag A, a prop 3 km out
        let (a, b, prop) = {
            let d = app.view().unwrap().doc.as_ref().unwrap();
            let w = d.writer().unwrap();
            let flags: Vec<String> = w
                .guids()
                .into_iter()
                .filter(|g| {
                    let e = w.find(g).unwrap();
                    w.type_of(e).ends_with("CapturePointEntityData")
                        && bc2_levelwrite::modelayer::bool_field(w, e, b"UnableToChangeTeam") == Some(false)
                        && w.transform(g).is_some()
                })
                .collect();
            let prop = w
                .guids()
                .into_iter()
                .find(|g| {
                    let e = w.find(g).unwrap();
                    w.type_of(e).ends_with("ReferenceObjectData") && w.transform(g).is_some() && d.index(g).is_some()
                })
                .unwrap();
            (flags[0].clone(), flags[1].clone(), prop)
        };
        {
            let d = app.view_mut().unwrap().doc.as_mut().unwrap();
            let w = d.writer().unwrap();
            let (ra, rb, rp) = (
                w.transform(&a).unwrap(),
                w.transform(&b).unwrap(),
                w.transform(&prop).unwrap(),
            );
            let mut to = rb;
            to[3] = [ra[3][0] + 20.0, ra[3][1], ra[3][2]];
            d.commit(
                Op::Move {
                    items: vec![Moved {
                        guid: b.clone(),
                        from: rb,
                        to,
                        placement: None,
                    }],
                },
                "Move flag",
            );
            let mut far = rp;
            far[3][0] = 3000.0;
            d.commit(
                Op::Move {
                    items: vec![Moved {
                        guid: prop.clone(),
                        from: rp,
                        to: far,
                        placement: None,
                    }],
                },
                "Move prop",
            );
        }
        h.settle(&mut app, 4);
        let (rows, took) = {
            let v = app.view_mut().unwrap();
            let d = v.doc.as_ref().unwrap();
            v.live.refresh(d);
            (validation::rows(d, &v.live), v.live.took_ms)
        };
        let doc = app.view().unwrap().doc.as_ref().unwrap();
        let cq = rows
            .iter()
            .find(|r| r.code == "L1-CQ-002" && r.objects.contains(&b))
            .expect("the flag warning");
        assert_eq!(cq.severity, Severity::Warning);
        assert!(cq.text.contains("[L1-CQ-002: stock 58 to 450.4"), "{}", cq.text);
        let far = rows.iter().find(|r| r.code == "XVAL-CULL-4").expect("the far prop");
        assert_eq!(far.severity, Severity::Error);
        assert_eq!(rows[0].severity, Severity::Error, "errors first");
        for r in [cq, far] {
            assert!(
                r.objects.iter().any(|g| doc.index(g).is_some()),
                "{}: no object to go to",
                r.code
            );
        }
        shot(&format!("v1_problems_{}.png", theme.key()), &mut h);
        println!(
            "v1 {theme:?}: {} rows before, {} after the edits ({:?}); validation {took:.0} ms",
            before,
            rows.len(),
            validation::counts(&rows)
        );
        // an undo takes the error away
        app.view_mut().unwrap().doc.as_mut().unwrap().undo();
        h.settle(&mut app, 2);
        let rows = {
            let v = app.view_mut().unwrap();
            let d = v.doc.as_ref().unwrap();
            v.live.refresh(d);
            validation::rows(d, &v.live)
        };
        assert!(!rows.iter().any(|r| r.code == "XVAL-CULL-4"), "{rows:#?}");
        assert!(rows.iter().any(|r| r.code == "L1-CQ-002"));
    }
    let _ = std::fs::remove_dir_all(&dir);
}
