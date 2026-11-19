//! The asset browser's catalogue on the game copy (read only): for a BFBC2, a Vietnam and a WW2 level, every
//! placeable object of the content set is listed (the level's placements, its folders', the set's bundles' headers),
//! a blueprint from another level's bundle is resolved (its partition read from the bundle: the primary instance's
//! reference and type), and the Spawners / Objectives tiles of a Conquest and a Rush level are drawn from their
//! data. Nothing is written. Skipped (with a note) without Project OPFOR BFBC2.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::too_many_lines,
    missing_docs
)]

use bc2_editor::content::{Catalogue, GameSource};
use bc2_editor::gameplay::{BrowserMode, Kind, tiles};
use bc2_editor::levels::{self, ContentSet};

#[test]
fn every_placeable_object_of_the_content_set() {
    let root = match levels::find_opfor(None) {
        Ok(r) => r,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let all = levels::list_levels(&root);
    let mut runs = 0;
    for lv in ["mp_005", "mp_005sr", "nam_mp_002cq", "wake_island_s"] {
        let Some(info) = all.iter().find(|l| l.lv == lv) else {
            println!("{lv}: not in this game copy");
            continue;
        };
        let Ok(found) = levels::load_options(&root, lv) else {
            println!("{lv}: not prepared (opening it in the editor prepares it)");
            continue;
        };
        let t = std::time::Instant::now();
        let scene = bc2_scene::scene::load(&found.options).unwrap();
        let game = GameSource {
            opfor: root.clone(),
            set: info.set,
            levels: all.iter().filter(|l| l.set == info.set).map(|l| l.lv.clone()).collect(),
            own: lv.to_owned(),
        };
        let mut cat = Catalogue::start(&scene, found.options.bases.clone(), Some(game));
        cat.wait(std::time::Duration::from_mins(5));
        let level = cat.entries.iter().filter(|e| e.used > 0).count();
        let headers = cat.entries.iter().filter(|e| e.bundle.is_some()).count();
        let elsewhere = cat.entries.iter().filter(|e| !e.in_stack).count();
        println!(
            "{lv} ({}): {} objects - {level} placed in the level, {headers} from the set's bundles' headers ({elsewhere} \
             outside the level's bundles), listed in {:.1} s",
            info.set.word(),
            cat.entries.len(),
            t.elapsed().as_secs_f64()
        );
        assert!(level > 0 && cat.entries.len() > level, "{lv}");
        // a blueprint of another level's bundle, read when placed
        if let Some(path) = cat
            .entries
            .iter()
            .find(|e| !e.in_stack && e.path.starts_with("objects/") && e.reference.is_empty())
            .map(|e| e.path.clone())
        {
            let t = std::time::Instant::now();
            let e = cat.resolved(&path).unwrap();
            assert!(
                e.reference.starts_with(&path) && e.reference.len() == path.len() + 37,
                "{}",
                e.reference
            );
            println!(
                "  resolved {path} from {}: {} ({}) in {:.2} s",
                e.source,
                e.reference,
                e.object_type.as_deref().unwrap_or("?"),
                t.elapsed().as_secs_f64()
            );
        }
        if info.set == ContentSet::Bfbc2 {
            // the Spawners and Objectives tiles from the level's data
            let base = scene.file(0).unwrap().1.to_vec();
            let dir = std::env::temp_dir().join(format!("bc2_asset_browser_{}_{lv}", std::process::id()));
            let doc = bc2_editor::edit::Doc::create("tiles", &dir, lv, scene).unwrap();
            assert_eq!(doc.partition().unwrap().len(), base.len());
            let t = tiles(&doc, Some(&cat));
            let show = |m: BrowserMode| {
                t.iter()
                    .filter(|x| x.mode == m)
                    .map(|x| {
                        format!(
                            "{} x{}{}",
                            x.label,
                            x.count,
                            if x.owned.is_empty() {
                                String::new()
                            } else {
                                format!(" (+{} owned)", x.owned.len())
                            }
                        )
                    })
                    .collect::<Vec<_>>()
            };
            println!(
                "  Spawners: {:?}",
                show(BrowserMode::Spawners).iter().take(12).collect::<Vec<_>>()
            );
            println!("  Objectives: {:?}", show(BrowserMode::Objectives));
            assert!(t.iter().any(|x| x.kind == Kind::SpawnPoint));
            assert!(t.iter().any(|x| matches!(x.kind, Kind::VehicleSpawner(_))));
            if lv == "mp_005" {
                let flag = t.iter().find(|x| x.kind == Kind::CapturePoint).unwrap();
                assert!(!flag.owned.is_empty(), "a Conquest flag owns its capture area");
            } else {
                assert!(t.iter().any(|x| matches!(x.kind, Kind::Mcom(_))), "Rush has M-COMs");
            }
            let _ = std::fs::remove_dir_all(&dir);
        }
        runs += 1;
    }
    println!("{runs} levels' catalogues");
}

/// The asset browser's three modes on `MP_005`, as screenshots (`BC2_EDITOR_SHOTS`, dark and light); skipped without
/// it, the game copy or a graphics adapter.
#[test]
fn asset_browser_screenshots() {
    let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") else {
        println!("skipped: BC2_EDITOR_SHOTS is not set");
        return;
    };
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let Ok(mut h) = bc2_editor::headless::Headless::new([1500, 900]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let tmp = std::env::temp_dir().join(format!("bc2_asset_browser_shots_{}", std::process::id()));
    let mut app = bc2_editor::app::EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    app.open_level_blocking_then(
        &gpu,
        "mp_005",
        bc2_editor::app::Then::Create("Browser".into(), tmp.clone()),
    )
    .unwrap();
    app.run_action(
        &gpu,
        &ctx,
        bc2_editor::actions::Action::ShowPanel(bc2_editor::actions::PanelKind::Content),
    );
    if let Some(c) = app.view_mut().unwrap().content.as_mut() {
        c.wait(std::time::Duration::from_mins(5));
    }
    for (mode, name) in [
        (BrowserMode::PlaceObjects, "place_objects"),
        (BrowserMode::Spawners, "spawners"),
        (BrowserMode::Objectives, "objectives"),
    ] {
        app.view_mut().unwrap().browser_mode = mode;
        for (theme, tag) in [
            (bc2_editor::theme::ThemeId::Dark, "dark"),
            (bc2_editor::theme::ThemeId::Light, "light"),
        ] {
            app.theme = theme;
            h.settle(&mut app, 5);
            let p = std::path::Path::new(&dir).join(format!("e2b_mp005_browser_{name}_{tag}.png"));
            h.save(&p).unwrap();
            println!("saved {}", p.display());
        }
    }
    let _ = std::fs::remove_dir_all(&tmp);
}
