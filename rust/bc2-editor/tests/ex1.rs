//! Round EX1's proofs in the running editor, without a window (skipped without the game copy or a graphics adapter;
//! `BC2_EDITOR_SHOTS=<folder>` saves the screenshots, Dark and Light):
//!
//! - **Names**: the Export window's Names section on an E3d canary (Atacama's Rush as a BFBC2 Conquest level) under
//!   new names - the level id, its folder, the server-browser name, the on-screen name per language - with the rules'
//!   messages (the black loading screen of an id holding `_NAME`, a stock level's id).
//! - **Preview**: the canary's stage opened read only beside the original `MP_005GR` with the same camera; the Diff
//!   panel's comparison (the Rush layer removed, the Conquest layer added); T switches sides; Close preview gives the
//!   project back.
//! - **Diff**: a project of Operation Aurora made a WW2 Conquest level, with a WW2 bunker placed from Guadalcanal's
//!   bundle, compared with its base: the mode layer added, the campaign's spawns and areas removed, the bunker added;
//!   the differences tinted in the 3D view; a click on a row selects and flies to it.
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

use std::path::{Path, PathBuf};

use bc2_editor::actions::Action;
use bc2_editor::app::{EditorApp, Then};
use bc2_editor::compare::{Group, Kind, Side};
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::render;
use bc2_editor::theme::ThemeId;
use bc2_levelwrite::export::{self, ExportSpec, Game, Identity};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::names::LangText;
use bc2_levelwrite::ops::{Op, Placed};
use bc2_levelwrite::{MapProject, Profile};
use eframe::egui::{Color32, Rect};

const SIZE: [u32; 2] = [1600, 1000];

fn shot(name: &str, h: &mut Headless) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        let p = Path::new(&dir).join(name);
        render::write_png(&p, SIZE, &h.image().unwrap()).unwrap();
        println!("saved {}", p.display());
    }
}

/// Pixels of `colour` (within 3 per channel, any shade from 70 %) inside `rect` of the last frame.
fn count(h: &mut Headless, rect: Rect, colour: Color32) -> usize {
    let img = h.image().unwrap();
    let mut n = 0;
    for y in rect.top().max(0.0) as u32..(rect.bottom() as u32).min(SIZE[1]) {
        for x in rect.left().max(0.0) as u32..(rect.right() as u32).min(SIZE[0]) {
            let k = ((y * SIZE[0] + x) * 4) as usize;
            let near = |a: u8, b: u8| a.abs_diff(b) <= 3;
            if near(img[k], colour.r()) && near(img[k + 1], colour.g()) && near(img[k + 2], colour.b()) {
                n += 1;
            }
        }
    }
    n
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_ex1_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

const ID: &str = "MAPEDIT_EX1_PREVIEW";

/// The canary: Atacama's Rush (`MP_005GR`) as a BFBC2 Conquest level, saved in `dir`, its export file under new names.
fn atacama(game: &Game, dir: &Path) -> (PathBuf, ExportSpec) {
    let base = export::level_partition(game, "mp_005gr").unwrap();
    let mut p = MapProject::new("EX1 preview", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let c = export::choose_mode(game, &p, Profile::Bfbc2, "Conquest", None).unwrap();
    let donor = c.donor.clone();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let path = dir.join(p.file_name());
    p.save(&path).unwrap();
    let mut languages = std::collections::BTreeMap::new();
    languages.insert(
        "french".to_owned(),
        LangText {
            name: "D\u{c9}SERT D'ATACAMA - CONQU\u{ca}TE".into(),
            description: String::new(),
        },
    );
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: ID.into(),
        folder: String::new(),
        title: "Atacama Conquest (EX1)".into(),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor,
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Bfbc2,
        identity: Some(Identity {
            name: "ATACAMA CONQUEST".into(),
            description: "Atacama's Rush map as a Conquest level".into(),
            picture: String::new(),
            languages,
        }),
    };
    std::fs::write(
        export::spec_path_of(&path),
        serde_json::to_string_pretty(&spec).unwrap() + "\n",
    )
    .unwrap();
    (path, spec)
}

#[test]
#[ignore = "game tier: a whole export of a real level; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn names_form_and_the_export_preview() {
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
    let dir = temp("preview");
    let (path, spec) = atacama(&game, &dir);
    let p = MapProject::load(&path).unwrap();
    let stage = dir.join("stage");
    let rec = export::build(&game, &p, &path.display().to_string(), &spec, &stage).unwrap();
    assert!(export::check(&game, &stage).unwrap().ok());
    for theme in ThemeId::ALL {
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        if let Err(e) = app.open_level_blocking_then(&gpu, "mp_005gr", Then::Open(path.clone(), None)) {
            println!("skipped mp_005gr: {e}");
            return;
        }
        // the campaign checklist (offered on opening) closed: not this test's
        app.export.strip.open = false;
        // the Names section: the names from the export file, checked against the game copy
        app.export.open_for(&path, Some(&root));
        h.settle(&mut app, 4);
        let s = app.export.spec.clone().unwrap();
        assert_eq!(s.names().id(), ID);
        assert_eq!(s.names().texts("french").0, "D\u{c9}SERT D'ATACAMA - CONQU\u{ca}TE");
        let (taken, langs) = app.export.taken.clone().unwrap();
        let mode = "Conquest";
        let ok = bc2_levelwrite::names::check(&s.names(), Some(&p.id), mode, Some(&taken), Some(&langs));
        assert_eq!(ok, Vec::new());
        shot(&format!("ex1_names_{}.png", theme.key()), &mut h);
        // a broken id: the black loading screen and a stock id, in words
        app.export.spec.as_mut().unwrap().level = "MAPEDIT_NAME_BAD".into();
        h.settle(&mut app, 3);
        let bad = bc2_editor::export_ui::checks(app.export.spec.as_ref().unwrap(), Some(&game), true, None);
        assert!(bad.iter().any(|m| m.contains("black loading screen")), "{bad:?}");
        shot(&format!("ex1_names_problem_{}.png", theme.key()), &mut h);
        app.export.spec.as_mut().unwrap().level = ID.into();
        app.export.open = false;
        h.settle(&mut app, 2);
        // the preview: the stage beside the original, the same camera
        let cam = app.view().unwrap().viewports.values().next().map(|v| v.camera);
        app.open_preview_blocking(&gpu, &stage).unwrap();
        h.settle(&mut app, 6);
        assert_eq!(app.preview_side(), Some(Side::B));
        let v = app.view().unwrap();
        assert_eq!(v.lv, ID.to_ascii_lowercase());
        assert!(v.doc.is_none(), "the preview is read only");
        assert_eq!(
            v.viewports.values().next().map(|x| x.camera),
            cam,
            "the project's camera"
        );
        let d = v.diff.current().unwrap().clone();
        let n = |k: Kind, g: Group| d.rows.iter().filter(|r| r.kind == k && r.group == g).count();
        assert!(
            n(Kind::Added, Group::ModeLayer) > 100,
            "the Conquest layer: {}",
            n(Kind::Added, Group::ModeLayer)
        );
        assert!(
            n(Kind::Removed, Group::ModeLayer) > 100,
            "the Rush layer: {}",
            n(Kind::Removed, Group::ModeLayer)
        );
        let pal = app.palette();
        let view = v.view_rect().unwrap();
        let added_px = count(&mut h, view, pal.view_diff_added);
        shot(&format!("ex1_preview_export_{}.png", theme.key()), &mut h);
        // T: the original, with the same camera; the Rush layer drawn as removed
        let ctx = h.ctx.clone();
        app.run(&gpu, &ctx, Action::SwitchCompare);
        h.settle(&mut app, 6);
        assert_eq!(app.preview_side(), Some(Side::A));
        assert_eq!(app.view().unwrap().lv, "mp_005gr");
        let view = app.view().unwrap().view_rect().unwrap();
        let removed_px = count(&mut h, view, pal.view_diff_removed);
        shot(&format!("ex1_preview_original_{}.png", theme.key()), &mut h);
        // Close preview: the project again
        app.run(&gpu, &ctx, Action::ClosePreview);
        h.settle(&mut app, 2);
        assert!(app.view().unwrap().doc.is_some());
        assert_eq!(app.preview_side(), None);
        println!(
            "ex1 preview {theme:?}: {} rows ({} layer added, {} layer removed), {added_px} added px on the export, \
             {removed_px} removed px on the original",
            d.rows.len(),
            n(Kind::Added, Group::ModeLayer),
            n(Kind::Removed, Group::ModeLayer)
        );
    }
    let _ = rec;
    let _ = std::fs::remove_dir_all(bc2_editor::package::cache_dir(&root, ID));
    let _ = std::fs::remove_dir_all(&dir);
}

/// Operation Aurora made a WW2 Conquest level, with Guadalcanal's radar bunker placed.
fn aurora_ww2(game: &Game, dir: &Path) -> (PathBuf, String) {
    let base = export::level_partition(game, "sp_01").unwrap();
    let mut p = MapProject::new("EX1 Aurora 1943", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let c = export::choose_mode(game, &p, Profile::Ww2, "Conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let (cur, _) = export::current(game, &p).unwrap();
    let gc = game.level_bundles("guadal_canal", "level");
    let bunker =
        export::blueprint_ref(game, &gc, "objects/architecture/beach/radarbunker_t01/radarbunker_t01").unwrap();
    let template = cur.template_for(&bunker).unwrap();
    let n = p.next_n();
    let guid = bc2_levelwrite::new_guid(&p.id, n, &bunker);
    p.push(
        Op::Place {
            items: vec![Placed {
                guid: guid.clone(),
                template,
                blueprint: bunker,
                ref_field: None,
                owned: Vec::new(),
                transform: bc2_levelwrite::rows_at([-640.0, 6.0, 330.0]),
                fields: Vec::new(),
                placement: None,
            }],
        },
        "a WW2 bunker",
    );
    let path = dir.join(p.file_name());
    p.save(&path).unwrap();
    (path, guid)
}

#[test]
#[ignore = "game tier: two whole levels diffed; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn diff_panel_on_aurora_made_ww2_conquest() {
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
    let dir = temp("diff");
    let (path, bunker) = aurora_ww2(&game, &dir);
    for theme in ThemeId::ALL {
        let mut app = EditorApp::new(None, None).unwrap();
        app.theme = theme;
        h.settle(&mut app, 2);
        let gpu = h.gpu.clone();
        if let Err(e) = app.open_level_blocking_then(&gpu, "sp_01", Then::Open(path.clone(), None)) {
            println!("skipped sp_01: {e}");
            return;
        }
        // the campaign checklist opens by itself on a campaign base: not this test's
        app.export.strip.open = false;
        let ctx = h.ctx.clone();
        app.run(&gpu, &ctx, Action::CompareWithBase);
        let pal = app.palette();
        app.view_mut().unwrap().diff_settle(&gpu, &pal);
        h.settle(&mut app, 6);
        let v = app.view().unwrap();
        let d = v.diff.current().unwrap().clone();
        let n = |k: Kind, g: Group| d.rows.iter().filter(|r| r.kind == k && r.group == g).count();
        assert!(
            n(Kind::Added, Group::ModeLayer) > 100,
            "the WW2 Conquest layer: {}",
            n(Kind::Added, Group::ModeLayer)
        );
        assert!(
            n(Kind::Removed, Group::Campaign) > 10,
            "the campaign's spawns and areas: {}",
            n(Kind::Removed, Group::Campaign)
        );
        let b = d
            .rows
            .iter()
            .position(|r| r.guid_b.as_deref() == Some(bunker.as_str()))
            .expect("the bunker listed");
        assert_eq!(d.rows[b].kind, Kind::Added);
        assert_eq!(d.rows[b].blueprint.as_deref(), Some("radarbunker_t01"));
        // drawn: the scene's own differing objects tinted, markers where the removed ones stood
        let look = &v.gpu_scene().look;
        assert!(look.tint.len() > 100, "{} tinted", look.tint.len());
        assert!(look.tint.values().any(|c| *c == pal.view_diff_added));
        assert!(
            look.overlay.iter().any(|(_, c)| *c == pal.view_diff_removed),
            "markers of removed objects"
        );
        let view = v.view_rect().unwrap();
        let px = count(&mut h, view, pal.view_diff_removed) + count(&mut h, view, pal.view_diff_added);
        shot(&format!("ex1_diff_{}.png", theme.key()), &mut h);
        // the filters: only the removed ones, then a text
        let v = app.view_mut().unwrap();
        v.diff.filters.kinds = [false, true, false, false];
        let shown = d.visible(&v.diff.filters).len();
        assert_eq!(shown, d.counts_by_kind()[1]);
        v.diff.filters.kinds = [true; 4];
        v.diff.filters.text = "radarbunker".into();
        assert_eq!(d.visible(&v.diff.filters), [b]);
        h.settle(&mut app, 3);
        // a click on the row: the bunker selected, the camera at it
        let rect = app
            .view()
            .unwrap()
            .diff
            .row_rects
            .iter()
            .find(|(k, _)| *k == b)
            .map(|x| x.1);
        let Some(rect) = rect else {
            panic!(
                "the Diff panel shows no row for the bunker: {:?}",
                app.view().unwrap().diff.row_rects
            )
        };
        h.click(&mut app, rect.center());
        h.settle(&mut app, 4);
        let v = app.view().unwrap();
        let sel = v
            .selected
            .and_then(|k| v.scene().entities.get(k))
            .map(|e| e.guid.clone());
        assert_eq!(sel.as_deref(), Some(bunker.as_str()), "the row selects the bunker");
        shot(&format!("ex1_diff_picked_{}.png", theme.key()), &mut h);
        println!(
            "ex1 diff {theme:?}: {} rows ({} layer added, {} campaign removed) in {:.2} s, {} tinted, {px} diff px",
            d.rows.len(),
            n(Kind::Added, Group::ModeLayer),
            n(Kind::Removed, Group::Campaign),
            d.seconds,
            v.gpu_scene().look.tint.len()
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
