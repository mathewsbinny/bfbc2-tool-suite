//! Round OP1's proofs (the map editor's core rework before E5), on a small synthetic level (a partition and one static
//! group file written to a temporary folder; no game copy, no window):
//!
//! - **Part A** (audit AU1 R18 / D4): an operation that edits the partition and a static group file at once is one
//!   undo step; it applies whole or not at all, in the files and in the scene; undo, redo and History jumps across
//!   the checkpoints bring every file back; the group file's base sha1 is recorded on its first edit.
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
        d.problems.last().is_some_and(|p| p.contains("#5 is not in the level")),
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
