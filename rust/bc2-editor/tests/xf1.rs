//! Round XF1 (audit AU1 D1, D2, P1, G-N23): undo on a real project without replaying the whole log, and paste as a
//! deep duplicate.
//!
//! - `undo_on_a_mode_project`: Operation Aurora (`SP_01`) made a WW2 Conquest level (the mode choice, operation 1,
//!   reads Guadalcanal's partition), then 40 moves. Undo, redo and History jumps are timed (printed for BENCHMARKS)
//!   and each result is compared with the whole log applied from the base in a fresh document: the partition byte for
//!   byte, every entity's transform, the deleted set.
//!
//! Skipped (with a note) without the game copy. The project goes to a temp folder; OPFOR's files are only read.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::float_cmp,
    clippy::panic,
    clippy::doc_markdown,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::map_unwrap_or,
    clippy::cast_precision_loss,
    missing_docs
)]

use std::time::Instant;

use bc2_editor::edit::Doc;
use bc2_editor::levels;
use bc2_levelwrite::ops::Moved;
use bc2_levelwrite::{Op, Profile};
use bc2_scene::scene;

/// The state a document shows, to compare two documents: the partition's bytes, every entity's transform, the
/// deleted entities.
fn state(d: &Doc) -> (Vec<u8>, Vec<Option<[f64; 16]>>, Vec<usize>) {
    (
        d.writer().map(|w| w.write().unwrap()).unwrap_or_default(),
        d.scene.entities.iter().map(|e| e.matrix).collect(),
        d.deleted.iter().copied().collect(),
    )
}

/// The same project applied from its base in a fresh document.
fn fresh(d: &Doc, base: &scene::Scene, game: &bc2_levelwrite::export::Game) -> Doc {
    let mut f = Doc::with(d.project.clone(), d.path.clone(), base.clone());
    f.set_game(Some(game.clone()));
    f
}

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

#[test]
#[ignore = "game tier: Aurora as WW2 Conquest, 40 moves undone; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn undo_on_a_mode_project() {
    let _gate = crate::gate();
    let Ok(root) = levels::find_opfor(None) else {
        println!("skipped: no game copy");
        return;
    };
    let found = match levels::load_options(&root, "sp_01") {
        Ok(f) => f,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let Ok(game) = bc2_levelwrite::export::Game::open(&root.join("game")) else {
        println!("skipped: no game copy");
        return;
    };
    let base = scene::load(&found.options).unwrap();
    let dir = std::env::temp_dir().join(format!("bc2_editor_xf1_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut d = Doc::create("XF1 undo", &dir, "sp_01", base.clone()).unwrap();
    d.set_game(Some(game.clone()));
    let c = bc2_levelwrite::export::choose_mode(&game, &d.project, Profile::Ww2, "conquest", None).unwrap();
    let t = Instant::now();
    d.commit(Op::SetMode { to: Box::new(c) }, "XF1: era and mode");
    println!("mode choice applied in {:.0} ms", ms(t));
    // 40 moves of placed objects with a transform, one at a time
    let movable: Vec<(String, bc2_levelwrite::Rows)> = {
        let w = d.writer().unwrap();
        w.d.of_type(b".ReferenceObjectData")
            .into_iter()
            .filter_map(|e| {
                let g = String::from_utf8_lossy(w.d.attr(e, b"guid")?).to_ascii_uppercase();
                w.transform(&g).map(|r| (g, r))
            })
            .take(40)
            .collect()
    };
    assert_eq!(movable.len(), 40);
    for (i, (g, from)) in movable.iter().enumerate() {
        let mut to = *from;
        to[3][0] += 3.0 + i as f32;
        d.commit(
            Op::Move {
                items: vec![Moved {
                    guid: g.clone(),
                    from: *from,
                    to,
                    placement: None,
                }],
            },
            "XF1: a move",
        );
    }
    let all = d.project.operations.len();
    assert_eq!(all, 41);
    // undo x3, redo x3, then History to after the mode choice, to the start and to the end
    let mut times = Vec::new();
    for _ in 0..3 {
        let t = Instant::now();
        assert!(d.undo());
        times.push(("undo", ms(t)));
        assert!(
            d.last_replayed < bc2_editor::edit::CHECKPOINT_EVERY,
            "undo replays from a checkpoint, not the whole log ({} operations)",
            d.last_replayed
        );
    }
    let undone = state(&d);
    assert_eq!(
        undone,
        state(&fresh(&d, &base, &game)),
        "undo x3 = the log applied afresh"
    );
    for _ in 0..3 {
        let t = Instant::now();
        assert!(d.redo());
        times.push(("redo", ms(t)));
    }
    assert_eq!(
        state(&d),
        state(&fresh(&d, &base, &game)),
        "redo x3 = the log applied afresh"
    );
    for (to, what) in [
        (1, "History: after the mode choice"),
        (20, "History: op 20"),
        (0, "History: the base"),
        (all, "History: the end"),
    ] {
        let t = Instant::now();
        d.go_to(to);
        times.push((what, ms(t)));
        assert_eq!(d.project.operations.len(), to);
        assert_eq!(
            state(&d),
            state(&fresh(&d, &base, &game)),
            "{what} = the log applied afresh"
        );
    }
    // a new operation after an undo drops the redo branch: undo again gives what a fresh replay gives
    assert!(d.undo());
    let (g, from) = &movable[0];
    let mut to = *from;
    to[3][2] += 7.0;
    d.commit(
        Op::Move {
            items: vec![Moved {
                guid: g.clone(),
                from: *from,
                to,
                placement: None,
            }],
        },
        "XF1: a new branch",
    );
    assert!(d.undo());
    assert_eq!(state(&d), state(&fresh(&d, &base, &game)), "undo after a new branch");
    for (what, t) in &times {
        println!("{what}: {t:.0} ms");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

fn ops(app: &bc2_editor::app::EditorApp) -> Vec<Op> {
    let d = app.view().unwrap().doc.as_ref().unwrap();
    d.project.operations.iter().map(|o| o.op.clone()).collect()
}

/// Paste is a deep duplicate (audit AU1 G-N23): a pasted flag has its own capture area and marker. An edit of the
/// copy's capture area (a point moved, a field set) leaves the original's instances byte for byte as they were, and
/// the original flag still names its own marker.
#[test]
fn paste_is_a_deep_duplicate() {
    use bc2_editor::actions::Action;
    use bc2_editor::app::{EditorApp, Then};
    use bc2_editor::demo;
    use bc2_editor::headless::Headless;
    use bc2_levelwrite::FieldValue;
    use bc2_levelwrite::ops::FieldSet;

    let _gate = crate::gate();
    let mut h = match Headless::new([1200, 800]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let dir = std::env::temp_dir().join(format!("bc2_editor_xf1_paste_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let scene = demo::level(&dir.join("level")).unwrap();
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    app.open_scene_then(&gpu, "demo", scene, Then::Create("XF1 paste".into(), dir.clone()));
    h.settle(&mut app, 6);
    let flag = app
        .view()
        .unwrap()
        .doc
        .as_ref()
        .unwrap()
        .index(&demo::level_guid(5))
        .unwrap();
    app.select(&gpu, Some(flag));
    h.settle(&mut app, 2);
    app.run_action(&gpu, &ctx, Action::Copy);
    let n = ops(&app).len();
    app.run_action(&gpu, &ctx, Action::Paste);
    let log = ops(&app);
    assert_eq!(log.len(), n + 1, "one paste, one operation");
    let Some(Op::Duplicate { items }) = log.last() else {
        panic!("{log:?}")
    };
    assert_eq!(items.len(), 1, "the flag alone: its ring and marker come as its own");
    let sources: Vec<String> = items[0].owned.iter().map(|o| o.source.clone()).collect();
    assert_eq!(
        sources,
        [6, 7, 8, 9, 10].map(demo::level_guid),
        "the ring and the marker are copied"
    );
    let copy = items[0].clone();
    let instance = |app: &EditorApp, g: &str| -> Vec<u8> {
        let d = app.view().unwrap().doc.as_ref().unwrap();
        let w = d.writer().unwrap();
        let e = w.find(g).unwrap();
        // every element of the instance: its reference and its value bytes
        let mut out = Vec::new();
        for x in w.d.walk(e) {
            out.extend(w.d.attr(x, b"ref").unwrap_or_default());
            out.push(0);
            out.extend(w.d.raw(x));
            out.push(0);
        }
        out
    };
    let originals: Vec<String> = std::iter::once(demo::level_guid(5))
        .chain(sources.iter().cloned())
        .collect();
    let before: Vec<Vec<u8>> = originals.iter().map(|g| instance(&app, g)).collect();
    // the copy's names: its own marker and ring
    {
        let d = app.view().unwrap().doc.as_ref().unwrap();
        let w = d.writer().unwrap();
        let get = |g: &str, f: &str| match w.get_field(&d.schema, g, f).unwrap().1 {
            Some(FieldValue::Ref(r)) => r,
            v => panic!("{v:?}"),
        };
        assert_eq!(get(&copy.guid, "MapMarker"), copy.owned[4].guid.to_ascii_lowercase());
        assert_eq!(
            get(&copy.owned[3].guid, "Next"),
            copy.owned[0].guid.to_ascii_lowercase()
        );
        assert_eq!(
            get(&demo::level_guid(5), "MapMarker"),
            demo::level_guid(10).to_ascii_lowercase()
        );
    }
    // edit the copy: a point of its ring moved, its marker renamed
    {
        let doc = app.view_mut().unwrap().doc.as_mut().unwrap();
        let p = copy.owned[0].guid.clone();
        let from = doc.writer().unwrap().transform(&p).unwrap();
        let mut to = from;
        to[3][0] += 9.0;
        doc.commit(
            Op::Move {
                items: vec![Moved {
                    guid: p,
                    from,
                    to,
                    placement: None,
                }],
            },
            "XF1: the copy's ring",
        );
        let schema = doc.schema.clone();
        let marker = copy.owned[4].guid.clone();
        let named = doc
            .writer()
            .unwrap()
            .fields(&schema, &marker)
            .into_iter()
            .find(|(f, t, _)| *t == bc2_levelwrite::FieldType::Text && !f.contains('.'))
            .map(|(f, _, v)| (f, v));
        if let Some((field, from)) = named {
            doc.commit(
                Op::SetField {
                    items: vec![FieldSet {
                        guid: marker,
                        field,
                        from,
                        to: FieldValue::Text("XF1 copy".into()),
                    }],
                },
                "XF1: the copy's marker",
            );
        }
    }
    h.settle(&mut app, 2);
    let after: Vec<Vec<u8>> = originals.iter().map(|g| instance(&app, g)).collect();
    for ((g, b), a) in originals.iter().zip(&before).zip(&after) {
        assert_eq!(b, a, "the original {g} is unchanged by an edit of the copy");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
