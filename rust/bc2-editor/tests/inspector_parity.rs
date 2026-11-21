//! The editor's outliner against the Python Map Inspector (round G1's scene-model proof): for `SP_01`, `MP_005`,
//! `MP_009GR` and `ONS_MP_004`, the editor finds and loads the level itself (`levels::load_options`, `bc2_scene`) and
//! its outliner lists the same entities, in the same order, with the same categories (the inspector's layers),
//! teams and positions as the inspector listed them (recorded once by `record_inspector.py` into `tests/data`).
//! Then each level is drawn with a flag selected and the outline checked, as in the window.
//!
//! Skipped (with a note) when the game copy is not there or a level is not prepared; `BC2_EDITOR_SHOTS=<folder>`
//! saves a window screenshot of each level.
#![allow(
    clippy::many_single_char_names,
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    missing_docs
)]

use std::collections::BTreeMap;
use std::io::Read;

use bc2_editor::app::EditorApp;
use bc2_editor::headless::Headless;
use bc2_editor::levels;
use bc2_editor::model::{self, Outliner};
use bc2_scene::scene;
use serde_json::Value;

const LEVELS: [&str; 4] = ["sp_01", "mp_005", "mp_009gr", "ons_mp_004"];

fn recorded(lv: &str) -> Value {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("data")
        .join(format!("inspector_{lv}.json.gz"));
    let mut s = String::new();
    flate2::read::GzDecoder::new(std::fs::File::open(&path).unwrap())
        .read_to_string(&mut s)
        .unwrap();
    serde_json::from_str(&s).unwrap()
}

#[test]
fn outliner_lists_what_the_map_inspector_lists() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let mut checked = 0;
    for lv in LEVELS {
        let found = match levels::load_options(&root, lv) {
            Ok(f) => f,
            Err(e) => {
                println!("skipped {lv}: {e}");
                continue;
            }
        };
        let s = scene::load(&found.options).unwrap();
        let o = Outliner::new(&s);
        let want = recorded(lv);
        assert_eq!(want["level"].as_str().unwrap(), s.name, "{lv}");
        let rows = want["entities"].as_array().unwrap();
        // every entity once, in the inspector's order
        let mut entries: Vec<&model::Entry> = o.entries().collect();
        assert_eq!(entries.len(), rows.len(), "{lv}: entity count");
        entries.sort_by_key(|e| e.entity);
        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for (e, row) in entries.iter().zip(rows) {
            let r = row.as_array().unwrap();
            let ent = &s.entities[e.entity];
            assert_eq!(e.entity as u64, r[0].as_u64().unwrap(), "{lv}: order");
            assert_eq!(ent.guid, r[1].as_str().unwrap(), "{lv}: guid of {}", e.entity);
            assert_eq!(e.category, r[2].as_str().unwrap(), "{lv}: category of {}", ent.guid);
            let layer = model::inspector_layer(ent);
            assert_eq!(layer, r[3].as_str().unwrap(), "{lv}: layer of {}", ent.guid);
            assert_eq!(e.team.map(u64::from), r[4].as_u64(), "{lv}: team of {}", ent.guid);
            match (e.position, r[5].as_array()) {
                (None, None) => {}
                (Some(p), Some(q)) => {
                    for (a, b) in p.iter().zip(q) {
                        // both are the stored single-precision transform, widened
                        let b = b.as_f64().unwrap();
                        assert_eq!(*a as f32, b as f32, "{lv}: position of {}", ent.guid);
                        assert_eq!(
                            f64::from(*a as f32),
                            *a,
                            "{lv}: position of {} is a stored single",
                            ent.guid
                        );
                    }
                }
                (a, b) => panic!("{lv}: position of {}: {a:?} vs {b:?}", ent.guid),
            }
            *counts.entry(layer).or_default() += 1;
        }
        // the inspector's layer counts, and the outliner's groups add up to them
        let want_counts: BTreeMap<String, u64> = want["counts"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.as_u64().unwrap()))
            .collect();
        assert_eq!(counts, want_counts, "{lv}: counts per layer");
        let group_total: usize = o.groups.iter().map(|g| g.entries.len()).sum();
        assert_eq!(group_total, rows.len());
        println!(
            "{lv}: {} entities, {} groups, as the Map Inspector lists them",
            rows.len(),
            o.groups.len()
        );
        checked += 1;
    }
    println!("{checked} of {} levels compared", LEVELS.len());
}

#[test]
#[ignore = "game tier: four whole levels drawn; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn each_level_draws_with_its_selection_outlined() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let Ok(mut h) = Headless::new([1600, 950]) else {
        println!("skipped: no graphics adapter");
        return;
    };
    for lv in LEVELS {
        let mut app = EditorApp::new(None, None).unwrap();
        h.settle(&mut app, 2);
        if let Err(e) = app.open_level_blocking(&h.gpu, lv) {
            println!("skipped {lv}: {e}");
            continue;
        }
        // select the first flag (else the first spawn) and focus it, as F does
        let pick = {
            let v = app.view().unwrap();
            let by = |c: &str| {
                v.scene()
                    .entities
                    .iter()
                    .find(|e| e.category == c && e.position().is_some())
                    .map(|e| e.index)
            };
            by("capture point").or_else(|| by("soldier spawn")).unwrap()
        };
        let gpu = h.gpu.clone();
        app.select(&gpu, Some(pick));
        app.focus_selected();
        h.settle(&mut app, 12);
        // the selected entity's projected position is inside the view; a click there keeps it selected
        let at = app.project_entity(pick).expect("the selection is in view");
        h.click(&mut app, at);
        assert_eq!(app.view().unwrap().selected, Some(pick), "{lv}: clicking the selection");
        let img = h.image().unwrap();
        let sel = app.palette().selection;
        let rect = app.view().unwrap().view_rect().unwrap();
        let outline = (rect.top() as u32..rect.bottom() as u32)
            .flat_map(|y| (rect.left() as u32..rect.right() as u32).map(move |x| (x, y)))
            .filter(|&(x, y)| {
                let k = ((y * 1600 + x) * 4) as usize;
                img[k..k + 3] == [sel.r(), sel.g(), sel.b()]
            })
            .count();
        assert!(outline > 30, "{lv}: {outline} outline pixels");
        if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
            let p = std::path::Path::new(&dir).join(format!("window_{lv}_dark.png"));
            bc2_editor::render::write_png(&p, [1600, 950], &img).unwrap();
            println!("saved {}", p.display());
        }
        println!("{lv}: drawn, {outline} outline pixels around the selection");
    }
}
