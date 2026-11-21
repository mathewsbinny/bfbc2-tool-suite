//! Round OP1's proofs (the map editor's core rework before E5), on a small synthetic level (a partition and one static
//! group file written to a temporary folder; no game copy, no window):
//!
//! - **Part A** (audit AU1 R18 / D4): an operation that edits the partition and a static group file at once is one
//!   undo step; it applies whole or not at all, in the files and in the scene; undo, redo and History jumps across
//!   the checkpoints bring every file back; the group file's base sha1 is recorded on its first edit.
//! - **Part B** (audit AU1 R24 / R17 / P3 / P16): brush strokes on the height field and a paint layer are stored as
//!   tiles beside the project and undone and redone with the log; the autosave is a journal of the changes that a
//!   reopened editor recovers after a crash; a stroke's commit and an undo on a long log stay within E5's budgets.
//! - **Part C** (audit AU1 R7 / D13, DF1's left items): problems are typed; closing with unsaved edits asks first;
//!   the game-accurate view is read on a worker.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    missing_docs
)]

use std::path::PathBuf;

use bc2_editor::edit::Doc;
use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_levelwrite::delta::{DirStore, Raster};
use bc2_levelwrite::files::{FileEdit, Target, member_guid, member_transform};
use bc2_levelwrite::ops::Moved;
use bc2_levelwrite::{MapProject, Op, Profile, rows_at};

pub(crate) const OBJ: &str = "AAAAAAAA-0000-0000-0000-000000000001";
pub(crate) const GROUP: &str = "GGGGGGGG-0000-0000-0000-000000000001";
pub(crate) const GROUP_FILE: &str = "Grid00x00_StaticModelGroupEntity_HavokAsset.dbx";

pub(crate) fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_op1_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn transform(f: &mut DbxFile, name: &str, xs: &[f32]) -> Element {
    let mut t = Vec::new();
    for x in xs {
        for v in [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, *x, 1.0, 2.0, 1.0f32,
        ] {
            t.extend(v.to_be_bytes());
        }
    }
    Element::numbers(
        f,
        if xs.len() == 1 { "complex" } else { "array" },
        &[("name", name)],
        4,
        &t,
    )
}

/// The level partition: `LevelData` and one object at x = 10.
fn level() -> Vec<u8> {
    let mut f = DbxFile::default();
    let item = Element::text(&mut f, "item", &[("ref", OBJ)], "");
    let objs = Element::container(&mut f, "array", &[("name", "Objects")], vec![item]);
    let lname = Element::text(&mut f, "field", &[("name", "Name")], "Levels/OP1");
    let ld = Element::container(
        &mut f,
        "instance",
        &[
            ("guid", "AAAAAAAA-0000-0000-0000-000000000000"),
            ("type", "Entity.LevelData"),
        ],
        vec![lname, objs],
    );
    let name = Element::text(&mut f, "field", &[("name", "Name")], "Crate");
    let t = transform(&mut f, "Transform", &[10.0]);
    let obj = Element::container(
        &mut f,
        "instance",
        &[("guid", OBJ), ("type", "Entity.ReferenceObjectData")],
        vec![name, t],
    );
    let part = Element::container(&mut f, "partition", &[("guid", "P")], vec![ld, obj]);
    f.roots.push(part);
    dbx::write(&f).unwrap()
}

/// A static group file: one group of two members, at x = 0 and 7.
fn group() -> Vec<u8> {
    let mut f = DbxFile::default();
    let tr = transform(&mut f, "Transforms", &[0.0, 7.0]);
    let items: Vec<Element> = ["objects/props/crate/crate_havok/x", "objects/props/rock/rock_havok/y"]
        .iter()
        .map(|r| Element::text(&mut f, "item", &[("ref", r)], ""))
        .collect();
    let assets = Element::container(&mut f, "array", &[("name", "Assets")], items);
    let inst = Element::container(
        &mut f,
        "instance",
        &[("guid", GROUP), ("type", "Entity.StaticModelGroupEntityData")],
        vec![assets, tr],
    );
    let part = Element::container(&mut f, "partition", &[("guid", "G")], vec![inst]);
    f.roots.push(part);
    dbx::write(&f).unwrap()
}

/// The synthetic level loaded as the editor loads one (the partition, then its group file), and a project on it.
pub(crate) fn doc(name: &str) -> Doc {
    let dir = temp(name);
    let lp = dir.join("OP1.dbx");
    let gp = dir.join(GROUP_FILE);
    std::fs::write(&lp, level()).unwrap();
    std::fs::write(&gp, group()).unwrap();
    let mut o = bc2_scene::scene::LoadOptions::new("OP1", &lp, Vec::new());
    o.groups = vec![gp];
    o.geometry = true;
    let scene = bc2_scene::scene::load(&o).unwrap();
    assert_eq!(scene.entities.len(), 4, "LevelData, the object and two group members");
    let project = MapProject::new(name, "op1", "levels/OP1", &level(), Profile::Bfbc2);
    let path = dir.join(project.file_name());
    project.save(&path).unwrap();
    Doc::with(project, path, scene)
}

pub(crate) fn member_move(x0: f32, x1: f32) -> FileEdit {
    FileEdit::MemberTransform {
        file: GROUP_FILE.into(),
        instance: GROUP.into(),
        item: 1,
        from: rows_at([x0, 1.0, 2.0]),
        to: rows_at([x1, 1.0, 2.0]),
    }
}

fn object_move(x0: f32, x1: f32) -> FileEdit {
    FileEdit::Partition {
        op: Box::new(Op::Move {
            items: vec![Moved {
                guid: OBJ.into(),
                from: rows_at([x0, 1.0, 2.0]),
                to: rows_at([x1, 1.0, 2.0]),
                placement: None,
            }],
        }),
    }
}

fn xs(d: &Doc) -> (f64, f64, f32) {
    let obj = d.index(OBJ).unwrap();
    let member = d.index(&member_guid(GROUP, 1)).unwrap();
    let in_file = d
        .files()
        .groups
        .get(&GROUP_FILE.to_ascii_lowercase())
        .map_or(7.0, |w| member_transform(w, GROUP, 1).unwrap()[3][0]);
    (
        d.scene.entities[obj].position().unwrap()[0],
        d.scene.entities[member].position().unwrap()[0],
        in_file,
    )
}

#[test]
fn a_multi_file_operation_is_one_step_whole_or_not_at_all() {
    let _gate = crate::gate();
    let mut d = doc("multi");
    assert_eq!(xs(&d), (10.0, 7.0, 7.0));
    let n = d.commit(
        Op::Files {
            edits: vec![object_move(10.0, 11.0), member_move(7.0, 9.0)],
        },
        "Move with its group member",
    );
    assert_eq!(xs(&d), (11.0, 9.0, 9.0), "both files and the scene");
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    // the group file's base recorded with its first edit
    let key = Target::group(GROUP_FILE).base_key().unwrap();
    let rec = d.project.base.files.get(&key).expect("recorded");
    assert_eq!(rec.sha1, bc2_levelwrite::project::sha1_hex(&group()));
    // the partition writes the move, the group file the member's
    let w = bc2_levelwrite::LevelWriter::parse(&d.partition().unwrap()).unwrap();
    assert_eq!(w.transform(OBJ).unwrap()[3][0], 11.0);
    assert!(d.check().contains("apply cleanly"), "{}", d.check());

    // one undo step brings both back; redo makes both again
    assert!(d.undo());
    assert_eq!(xs(&d), (10.0, 7.0, 7.0));
    assert!(d.redo());
    assert_eq!(xs(&d), (11.0, 9.0, 9.0));
    assert_eq!(d.last_n(), Some(n));

    // a failing edit (no member 5): nothing changes, in the files or the scene, and the problem names it
    d.commit(
        Op::Files {
            edits: vec![
                object_move(11.0, 50.0),
                FileEdit::MemberTransform {
                    file: GROUP_FILE.into(),
                    instance: GROUP.into(),
                    item: 5,
                    from: rows_at([0.0; 3]),
                    to: rows_at([1.0; 3]),
                },
            ],
        },
        "",
    );
    assert_eq!(xs(&d), (11.0, 9.0, 9.0), "the partition's move was not made either");
    assert!(
        d.problems.last().is_some_and(|p| p.contains("#5 is not in the level")
            && p.code == bc2_editor::error::ProblemCode::NotApplied
            && p.severity == bc2_editor::error::Severity::Error
            && p.operation == d.last_n()),
        "{:?}",
        d.problems
    );
    assert!(d.undo());
    assert!(d.problems.is_empty(), "the failing operation undone: {:?}", d.problems);

    // many operations: undo and History jumps across the checkpoints bring the group file back too
    for k in 0..40u16 {
        let x = 9.0 + f32::from(k);
        d.commit(
            Op::Files {
                edits: vec![member_move(x, x + 1.0)],
            },
            "",
        );
    }
    assert_eq!(xs(&d).2, 49.0);
    d.go_to(20);
    assert_eq!(xs(&d), (11.0, 28.0, 28.0));
    assert_eq!(d.last_replayed, 4, "replayed from the checkpoint after 16 operations");
    d.go_to(1);
    assert_eq!(xs(&d), (11.0, 9.0, 9.0));
    d.go_to(0);
    assert_eq!(xs(&d), (10.0, 7.0, 7.0));
    d.go_to(41);
    assert_eq!(xs(&d), (11.0, 49.0, 49.0));

    // saved and opened again: the same files
    d.save().unwrap();
    let text = std::fs::read_to_string(&d.path).unwrap();
    assert!(text.contains("\"op\": \"files\"") && text.contains("\"edit\": \"member-transform\""));
    assert!(text.contains(&format!("\"version\": {}", bc2_levelwrite::project::VERSION)));
    let p = MapProject::load(&d.path).unwrap();
    let fresh = doc("reopen");
    let again = Doc::with(p, fresh.path.clone(), fresh.scene.clone());
    assert_eq!(xs(&again), (11.0, 49.0, 49.0));
}

/// The synthetic level with a height field of `side` x `side` samples, all 1000.
pub(crate) fn doc_with_terrain(name: &str, side: u32) -> Doc {
    let d = doc(name);
    let mut scene = d.scene.clone();
    scene.heightfield = Some(bc2_formats::terrain::Heightfield {
        min: [0.0, 0.0, 0.0],
        max: [1024.0, 300.0, 1024.0],
        size: side,
        header: bc2_formats::terrain::HeaderWords::default(),
        raw: vec![1000; (side * side) as usize],
        tail: bc2_formats::terrain::Tail::Unknown(Vec::new()),
    });
    let (project, path) = (d.project.clone(), d.path.clone());
    drop(d);
    Doc::with(project, path, scene)
}

/// A round dab of `radius` samples at `(cx, cy)`, raised by `by`.
fn dab(r: &mut Raster, cx: u32, cy: u32, radius: u32, by: u16) {
    for y in cy.saturating_sub(radius)..(cy + radius).min(r.height()) {
        for x in cx.saturating_sub(radius)..(cx + radius).min(r.width()) {
            let (dx, dy) = (f64::from(x) - f64::from(cx), f64::from(y) - f64::from(cy));
            if dx * dx + dy * dy <= f64::from(radius * radius) {
                let v = r.get(x, y).saturating_add(by);
                r.set(x, y, v);
            }
        }
    }
}

#[test]
fn strokes_are_tiles_and_the_autosave_is_a_journal() {
    let _gate = crate::gate();
    let mut d = doc_with_terrain("strokes", 256);
    let terrain = d.terrain_target().unwrap();
    assert_eq!(
        terrain,
        Target::Terrain {
            file: "heightfield".into()
        }
    );
    let flat = d.raster(&terrain).unwrap();
    // a dab across four tiles' corner, and a paint layer's first stroke
    let mut hill = flat.clone();
    dab(&mut hill, 64, 64, 10, 500);
    let n = d.stroke(&terrain, &hill, "Raise").unwrap().unwrap();
    assert_eq!(d.raster(&terrain).unwrap(), hill);
    let paint = Target::Paint { layer: "grass".into() };
    let mut grass = Raster::new(256, 256, 0);
    dab(&mut grass, 200, 200, 4, 255);
    d.stroke(&paint, &grass, "").unwrap().unwrap();
    assert_eq!(d.raster(&paint).unwrap(), grass);
    assert!(
        d.stroke(&paint, &grass, "").unwrap().is_none(),
        "nothing changed: no operation"
    );
    // the log names the tiles; the samples are in the tile folder, not the project
    let names: Vec<String> = d
        .project
        .operations
        .iter()
        .map(bc2_levelwrite::Operation::name)
        .collect();
    assert_eq!(names[0], "Raise (Stroke on the height field heightfield: 4 tiles)");
    let tiles = std::fs::read_dir(DirStore::of(&d.path).dir).unwrap().count();
    // the 4 tiles before are alike (all 1000): kept once, as is the paint tile before (zeros)
    assert_eq!(tiles, 1 + 4 + 1 + 1);
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    // undo, redo
    assert!(d.undo());
    assert!(d.raster(&paint).is_none(), "no paint layer before its first stroke");
    assert!(d.undo());
    assert_eq!(d.raster(&terrain).unwrap(), flat);
    assert!(d.redo() && d.redo());
    assert_eq!(d.raster(&terrain).unwrap(), hill);
    assert_eq!(d.raster(&paint).unwrap(), grass);
    assert_eq!(d.last_n(), Some(n + 1));

    // the autosave is a journal: its first line the project, then one line per change since
    let auto = bc2_levelwrite::project::autosave_path(&d.path);
    let text = std::fs::read_to_string(&auto).unwrap();
    assert_eq!(d.journal().unwrap().lines(), text.lines().count());
    assert!(text.lines().count() >= 5, "{text}");
    assert!(!text.starts_with('{'));
    // the editor stops without a save: the reopened editor offers the journal's project, with its strokes
    let path = d.path.clone();
    let (ops, redo) = (d.project.operations.clone(), d.project.redo.clone());
    let base = doc_with_terrain("strokes_base", 256).scene.clone();
    drop(d);
    let recovered = MapProject::recover(&path).expect("an autosave that differs from the saved project");
    assert_eq!((&recovered.operations, &recovered.redo), (&ops, &redo));
    let mut e = Doc::with(recovered, path.clone(), base);
    assert_eq!(e.raster(&terrain).unwrap(), hill);
    assert_eq!(e.raster(&paint).unwrap(), grass);
    // a change after the recovery starts a journal whole; a save removes it and keeps the tiles
    e.go_to(1);
    assert!(e.journal().is_some_and(|j| j.lines() == 1));
    e.save().unwrap();
    assert!(!auto.exists() && e.journal().is_none());
    assert!(DirStore::of(&path).dir.is_dir());
    assert!(MapProject::recover(&path).is_none());
    let saved = MapProject::load(&path).unwrap();
    assert_eq!(saved.reader_needed(), 6);
    assert!(std::fs::read_to_string(&path).unwrap().contains("\"op\": \"stroke\""));
}

/// Round OP1's budgets for E5 (audit AU1 P16): a stroke's commit under 16 ms and an undo under 100 ms on a long log,
/// here 400 strokes on a 2048 x 2048 height field (the ignored `stroke_budgets_on_10000_strokes` is the measurement
/// of `rust/BENCHMARKS.md` "OP1").
#[test]
fn stroke_budgets() {
    let _gate = crate::gate();
    budgets("budgets", 400);
}

#[test]
#[ignore = "game tier: a measurement of 10,000 strokes (rust/BENCHMARKS.md OP1)"]
fn stroke_budgets_on_10000_strokes() {
    let _gate = crate::gate();
    budgets("budgets_10000", 10_000);
}

fn budgets(name: &str, strokes: u32) {
    let mut d = doc_with_terrain(name, 2048);
    let terrain = d.terrain_target().unwrap();
    let mut r = d.raster(&terrain).unwrap();
    let mut times = Vec::new();
    let t_all = std::time::Instant::now();
    for k in 0..strokes {
        // dabs of radius 24 walking over the map: each touches 1 - 4 tiles
        let (cx, cy) = (37 + (k * 97) % 1970, 41 + (k * 53) % 1960);
        let t = std::time::Instant::now();
        dab(&mut r, cx, cy, 24, 3);
        d.stroke(&terrain, &r, "").unwrap().unwrap();
        times.push(t.elapsed().as_secs_f64() * 1000.0);
    }
    let total = t_all.elapsed().as_secs_f64();
    times.sort_by(f64::total_cmp);
    let pct = |p: f64| {
        let last = times.len() - 1;
        times[(f64::from(u32::try_from(last).unwrap()) * p) as usize]
    };
    let t = std::time::Instant::now();
    assert!(d.undo());
    let undo = t.elapsed().as_secs_f64() * 1000.0;
    let t = std::time::Instant::now();
    d.go_to(d.project.operations.len() - 40);
    let jump = t.elapsed().as_secs_f64() * 1000.0;
    let t = std::time::Instant::now();
    assert!(d.redo());
    let redo = t.elapsed().as_secs_f64() * 1000.0;
    let project_bytes = d.project.to_json().unwrap().len();
    let journal = d.journal().map_or(0, bc2_levelwrite::journal::Journal::bytes);
    let tile_bytes: u64 = std::fs::read_dir(DirStore::of(&d.path).dir)
        .unwrap()
        .flatten()
        .map(|e| e.metadata().unwrap().len())
        .sum();
    println!(
        "{strokes} strokes on 2048 x 2048 in {total:.1} s: commit median {:.2} ms, p95 {:.2} ms, max {:.2} ms; undo \
         {undo:.1} ms, History jump of 40 {jump:.1} ms, redo {redo:.1} ms; project file {project_bytes} bytes, \
         journal {journal} bytes, tiles {tile_bytes} bytes",
        pct(0.5),
        pct(0.95),
        pct(1.0)
    );
    assert!(pct(0.5) < 16.0, "a stroke's commit: {:.2} ms", pct(0.5));
    assert!(undo < 100.0 && redo < 100.0, "undo {undo:.1} ms, redo {redo:.1} ms");
}

/// Part C (audit AU1 R7 / G-N20, DF1's left items): closing the map or the window with unsaved edits asks first -
/// Save, Don't save, Cancel - as one close state, after the busy question.
#[test]
fn closing_with_unsaved_edits_asks_first() {
    use bc2_editor::actions::Action;
    use bc2_editor::app::{CloseWhat, Closing, EditorApp, Then, UnsavedAnswer};
    let _gate = crate::gate();
    let dir = temp("close");
    let mut h = match bc2_editor::headless::Headless::new([1280, 800]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let (gpu, ctx) = (h.gpu.clone(), h.ctx.clone());
    let open = |app: &mut EditorApp, h: &mut bc2_editor::headless::Headless, name: &str| {
        let scene = bc2_editor::demo::level(&dir.join("level")).unwrap();
        app.open_scene_then(
            &h.gpu.clone(),
            "demo",
            scene,
            Then::Create(name.into(), dir.clone(), None),
        );
        h.settle(app, 2);
        app.view().unwrap().doc.as_ref().unwrap().path.clone()
    };
    let edit = |app: &mut EditorApp| {
        let d = app.view_mut().unwrap().doc.as_mut().unwrap();
        let g = d.guid(1).unwrap().to_owned();
        d.commit(
            Op::Hide {
                guids: vec![g],
                hidden: true,
            },
            "",
        );
        assert!(!d.saved);
    };
    // nothing unsaved: the map closes at once, the window may close
    open(&mut app, &mut h, "OP1 close A");
    assert!(app.close_requested());
    app.answer_close(false);
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let path = open(&mut app, &mut h, "OP1 close B");
    edit(&mut app);
    // Close map: asked; Cancel keeps the map open
    app.run_action(&gpu, &ctx, Action::CloseMap);
    assert!(matches!(app.closing(), Closing::Unsaved { then: CloseWhat::Map, project } if project == "OP1 close B"));
    assert!(app.view().is_some());
    app.answer_unsaved(UnsavedAnswer::Cancel);
    assert_eq!(app.closing(), &Closing::No);
    assert!(app.view().is_some());
    // Save: saved, then closed
    app.run_action(&gpu, &ctx, Action::CloseMap);
    app.answer_unsaved(UnsavedAnswer::Save);
    assert!(app.view().is_none(), "closed after the save");
    assert_eq!(MapProject::load(&path).unwrap().operations.len(), 1);
    assert!(MapProject::recover(&path).is_none());
    // the window: asked; Don't save lets the edits and their autosave go, and the window closes
    let path = open(&mut app, &mut h, "OP1 close C");
    edit(&mut app);
    assert!(!app.close_requested());
    assert!(matches!(
        app.closing(),
        Closing::Unsaved {
            then: CloseWhat::Window,
            ..
        }
    ));
    app.answer_unsaved(UnsavedAnswer::Discard);
    assert!(app.closing_anyway());
    assert!(
        !bc2_levelwrite::project::autosave_path(&path).exists(),
        "the autosave let go"
    );
    assert_eq!(MapProject::load(&path).unwrap().operations.len(), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Part C: the game-accurate view's world is read on a worker; a level that does not read is an error from the
/// worker, typed, never a hang.
#[test]
fn the_game_view_loads_on_a_worker() {
    let _gate = crate::gate();
    let scene = bc2_editor::demo::scene();
    let missing = temp("no_opfor");
    let load = bc2_editor::gameview::GameLoad::start(&missing, "demo", &scene);
    let t = std::time::Instant::now();
    let r = loop {
        if let Some(r) = load.poll() {
            break r;
        }
        assert!(t.elapsed().as_secs() < 60, "the worker answers");
        std::thread::sleep(std::time::Duration::from_millis(10));
    };
    let Err(e) = r else { panic!("no level there") };
    assert!(
        matches!(
            e,
            bc2_editor::error::EditorError::Render(_) | bc2_editor::error::EditorError::Worker(_)
        ),
        "{e:?}"
    );
    assert!(
        bc2_editor::gameview::GameLoad::start(&missing, "demo", &scene)
            .wait()
            .is_err()
    );
}
