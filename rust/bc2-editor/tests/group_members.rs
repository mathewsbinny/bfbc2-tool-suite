//! Round SG1: static group members edited like any object.
//!
//! - `members_are_edited_like_objects` (a small synthetic level: a partition with a static group of two crates and
//!   a rock, and its group file; no game copy, no window): a member moved, deleted and duplicated through the same
//!   operations the views make; each becomes the group file's edit (and the member type's in the partition), one
//!   History step each, named by member identity; the rock, the last of its kind, is refused before it enters the
//!   log; undo, redo and History jumps give what a fresh replay gives; the project saves as version 7 and reopens to
//!   the same scene.
//! - `wake_members_through_the_interface` (game tier: OPFOR's Wake Island, headless): a static group member clicked
//!   in the 3D view, dragged 6 m along the gizmo's X arm on a 1 m grid (one move: the member's group file edit),
//!   deleted, duplicated, placed into its group from the asset browser, undone; `BC2_EDITOR_SHOTS=<folder>` saves
//!   the moved member, selected, in the dark and the light theme.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::float_cmp,
    clippy::many_single_char_names,
    missing_docs
)]

use bc2_editor::edit::Doc;
use bc2_formats::dbx::{self, DbxFile, Element};
use bc2_levelwrite::files::{FileEdit, Member};
use bc2_levelwrite::ops::{Duplicated, Moved};
use bc2_levelwrite::{MapProject, Op, Profile, rows_at};

const OBJ: &str = "AAAAAAAA-0000-0000-0000-000000000001";
const GROUP: &str = "GGGGGGGG-0000-0000-0000-000000000001";
const GROUP_FILE: &str = "StaticModelGroupEntity_HavokAsset.dbx";

fn int(f: &mut DbxFile, name: &str, v: i32) -> Element {
    Element::numbers(f, "field", &[("name", name)], 4, &v.to_be_bytes())
}

fn transforms(f: &mut DbxFile, name: &str, xs: &[f32]) -> Element {
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

/// The partition: `LevelData`, a crate object at x = 10, and the static group (crates: 2 instances, 2 parts each;
/// the rock: 1 instance, 1 part), its cumulative numbers as DICE writes them.
fn level() -> Vec<u8> {
    let mut f = DbxFile::default();
    let spawn = |f: &mut DbxFile, e: i32| {
        let en = int(f, "Enumeration", e);
        Element::container(f, "complex", &[], vec![en])
    };
    let member = |f: &mut DbxFile, guid: &str, name: &str, enums: &[i32], per: i32, first: i32| {
        let spawns: Vec<Element> = enums.iter().map(|&e| spawn(f, e)).collect();
        let sd = Element::container(f, "array", &[("name", "SpawnDatas")], spawns);
        let nm = Element::text(f, "field", &[("name", "Name")], name);
        let n = enums.len() as i32;
        let fields = vec![
            sd,
            nm,
            int(f, "FirstPhysicsPart", first),
            int(f, "PhysicsPartCount", first + per * n),
            int(f, "PhysicsPartCountPerInstance", per),
            int(f, "InstanceCount", n),
        ];
        Element::container(
            f,
            "instance",
            &[
                ("guid", guid),
                ("type", "GameSharedResources.StaticModelGroupMemberData"),
            ],
            fields,
        )
    };
    let m1 = member(
        &mut f,
        "MMMMMMMM-0000-0000-0000-000000000001",
        "Objects/Props/Crate",
        &[1, 2],
        2,
        0,
    );
    let m2 = member(
        &mut f,
        "MMMMMMMM-0000-0000-0000-000000000002",
        "Objects/Props/Rock",
        &[3],
        1,
        4,
    );
    let items: Vec<Element> = [
        "MMMMMMMM-0000-0000-0000-000000000001",
        "MMMMMMMM-0000-0000-0000-000000000002",
    ]
    .iter()
    .map(|r| Element::text(&mut f, "item", &[("ref", r)], ""))
    .collect();
    let md = Element::container(&mut f, "array", &[("name", "MemberDatas")], items);
    let gname = Element::text(
        &mut f,
        "field",
        &[("name", "Name")],
        "Levels/SG1/StaticModelGroupEntity",
    );
    let ge = Element::container(
        &mut f,
        "instance",
        &[
            ("guid", "EEEEEEEE-0000-0000-0000-000000000001"),
            ("type", "GameSharedResources.StaticModelGroupEntityData"),
        ],
        vec![gname, md],
    );
    let i1 = Element::text(&mut f, "item", &[("ref", OBJ)], "");
    let i2 = Element::text(&mut f, "item", &[("ref", "EEEEEEEE-0000-0000-0000-000000000001")], "");
    let objs = Element::container(&mut f, "array", &[("name", "Objects")], vec![i1, i2]);
    let lname = Element::text(&mut f, "field", &[("name", "Name")], "Levels/SG1");
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
    let t = transforms(&mut f, "Transform", &[10.0]);
    let obj = Element::container(
        &mut f,
        "instance",
        &[("guid", OBJ), ("type", "Entity.ReferenceObjectData")],
        vec![name, t],
    );
    let part = Element::container(&mut f, "partition", &[("guid", "P")], vec![ld, obj, ge, m1, m2]);
    f.roots.push(part);
    dbx::write(&f).unwrap()
}

/// The group file: crates at x = 0 and 7, the rock at x = 20, leaves 1 - 3.
fn group() -> Vec<u8> {
    let mut f = DbxFile::default();
    let tr = transforms(&mut f, "Transforms", &[0.0, 7.0, 20.0]);
    let e: Vec<u8> = [1u32, 2, 3].iter().flat_map(|v| v.to_be_bytes()).collect();
    let en = Element::numbers(&mut f, "array", &[("name", "Enums")], 4, &e);
    let items: Vec<Element> = [
        "objects/props/crate/crate_havok/x",
        "objects/props/crate/crate_havok/x",
        "objects/props/rock/rock_havok/y",
    ]
    .iter()
    .map(|r| Element::text(&mut f, "item", &[("ref", r)], ""))
    .collect();
    let assets = Element::container(&mut f, "array", &[("name", "Assets")], items);
    let inst = Element::container(
        &mut f,
        "instance",
        &[("guid", GROUP), ("type", "Physics.CompositeHavokAsset")],
        vec![assets, tr, en],
    );
    let part = Element::container(&mut f, "partition", &[("guid", GROUP)], vec![inst]);
    f.roots.push(part);
    dbx::write(&f).unwrap()
}

fn temp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_sg1_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn scene(dir: &std::path::Path) -> bc2_scene::scene::Scene {
    let lp = dir.join("SG1.dbx");
    let gp = dir.join(GROUP_FILE);
    std::fs::write(&lp, level()).unwrap();
    std::fs::write(&gp, group()).unwrap();
    let mut o = bc2_scene::scene::LoadOptions::new("SG1", &lp, Vec::new());
    o.groups = vec![gp];
    o.geometry = true;
    bc2_scene::scene::load(&o).unwrap()
}

fn member(n: usize) -> String {
    format!("{GROUP}#{n}")
}

fn x_of(d: &Doc, guid: &str) -> f64 {
    d.scene.entities[d.index(guid).unwrap()].position().unwrap()[0]
}

#[test]
fn members_are_edited_like_objects() {
    let _gate = crate::gate();
    let dir = temp("doc");
    let sc = scene(&dir);
    assert_eq!(sc.entities.iter().filter(|e| e.category == "group member").count(), 3);
    let project = MapProject::new("SG1 members", "sg1", "levels/SG1", &level(), Profile::Bfbc2);
    let path = dir.join(project.file_name());
    project.save(&path).unwrap();
    let mut d = Doc::with(project, path.clone(), sc.clone());
    for k in 0..d.scene.entities.len() {
        if d.scene.entities[k].category == "group member" {
            assert_eq!(d.why_not(k), None, "a member is editable");
        }
    }
    // a move of the object and a member: one operation, the partition's move and the member's edit
    let mv = |guid: &str, a: f32, b: f32| Moved {
        guid: guid.into(),
        from: rows_at([a, 1.0, 2.0]),
        to: rows_at([b, 1.0, 2.0]),
        placement: None,
    };
    d.commit(
        Op::Move {
            items: vec![mv(OBJ, 10.0, 12.0), mv(&member(1), 7.0, 9.0)],
        },
        "",
    );
    let Op::Files { edits } = &d.project.operations[0].op else {
        panic!("{:?}", d.project.operations[0].op)
    };
    assert!(matches!(&edits[0], FileEdit::Partition { op } if matches!(**op, Op::Move { .. })));
    assert!(matches!(
        &edits[1],
        FileEdit::MemberTransform {
            item: Member::Base(1),
            ..
        }
    ));
    assert_eq!((x_of(&d, OBJ), x_of(&d, &member(1))), (12.0, 9.0));
    // a move of the object alone is the operation it always was
    d.commit(
        Op::Move {
            items: vec![mv(OBJ, 12.0, 13.0)],
        },
        "",
    );
    assert!(matches!(d.project.operations[1].op, Op::Move { .. }));
    // the rock is the last of its kind in the group: refused before it enters the log
    let rock = Op::Delete { guids: vec![member(2)] };
    let why = d.refusal(&rock).unwrap();
    assert!(why.contains("the last Rock of its group"), "{why}");
    // crate #0 deleted: the group file's edit and its member type's
    assert_eq!(d.refusal(&Op::Delete { guids: vec![member(0)] }), None);
    d.commit(Op::Delete { guids: vec![member(0)] }, "");
    assert_eq!(
        d.project.operations[2].name(),
        "Delete group member GGGGGGGG-0000-0000-0000-000000000001#0 (crate) (2 files)"
    );
    let k0 = sc.entities.iter().position(|e| e.guid == member(0)).unwrap();
    assert!(!d.shown(k0) && d.index(&member(0)).is_none());
    // now crate #1 is the last crate: refused too
    assert!(d.refusal(&Op::Delete { guids: vec![member(1)] }).is_some());
    // crate #1 duplicated: an added member `#+1`, selected by the views through created_by_last
    d.commit(
        Op::Duplicate {
            items: vec![Duplicated {
                source: member(1),
                guid: d.project.new_guid(&member(1)),
                transform: rows_at([30.0, 1.0, 2.0]),
                owned: Vec::new(),
            }],
        },
        "",
    );
    let added = format!("{GROUP}#+1");
    assert_eq!(d.created_by_last(), std::slice::from_ref(&added));
    assert_eq!(x_of(&d, &added), 30.0);
    assert_eq!(d.why_not(d.index(&added).unwrap()), None);
    // the added member moved, then duplicated: `#+2`, numbered after it
    d.commit(
        Op::Move {
            items: vec![mv(&added, 30.0, 31.0)],
        },
        "",
    );
    d.commit(
        Op::Duplicate {
            items: vec![Duplicated {
                source: added.clone(),
                guid: String::new(),
                transform: rows_at([40.0, 1.0, 2.0]),
                owned: Vec::new(),
            }],
        },
        "",
    );
    assert_eq!(d.created_by_last(), [format!("{GROUP}#+2")]);
    // the files: crates #1, #+1, #+2 then the rock, in DICE's layout
    let f = d.files().groups.values().next().unwrap();
    let w = d.files().partition.as_ref().unwrap();
    let g = bc2_levelwrite::members::group_entity(w, GROUP_FILE).unwrap();
    let runs = bc2_levelwrite::members::runs(w, &g).unwrap();
    assert_eq!(runs.iter().map(|r| r.count).collect::<Vec<_>>(), [3, 1]);
    let xs: Vec<f32> = (0..4)
        .map(|p| bc2_levelwrite::files::member_transform(f, GROUP, p).unwrap()[3][0])
        .collect();
    assert_eq!(xs, [9.0, 31.0, 40.0, 20.0]);
    assert!(d.problems.is_empty(), "{:?}", d.problems);
    let check = d.check();
    assert!(!check.contains("differs") && !check.contains("do not apply"), "{check}");
    // undo to before the delete, redo, History jumps: what a fresh replay gives
    let all = d.project.operations.len();
    for to in [2, 4, 0, all] {
        d.go_to(to);
        let fresh = {
            let mut p = d.project.clone();
            p.redo.clear();
            Doc::with(p, dir.join("fresh.mapedit.json"), sc.clone())
        };
        let bytes = |x: &Doc| {
            x.files()
                .groups
                .iter()
                .map(|(k, w)| (k.clone(), w.write().unwrap()))
                .collect::<Vec<_>>()
        };
        assert_eq!(bytes(&d), bytes(&fresh), "History at {to}");
        assert_eq!(d.files().orders, fresh.files().orders, "History at {to}");
        for e in &fresh.scene.entities {
            if fresh.shown(e.index) {
                assert_eq!(d.scene.entities[e.index].matrix, e.matrix, "{} at {to}", e.guid);
            }
            assert_eq!(d.shown(e.index), fresh.shown(e.index), "{} at {to}", e.guid);
        }
    }
    assert!(d.undo() && d.undo());
    assert!(d.index(&format!("{GROUP}#+2")).is_none());
    assert!(d.redo() && d.redo());
    assert_eq!(x_of(&d, &format!("{GROUP}#+2")), 40.0);
    // saved as version 7 (a member delete and add need it), reopened: the same scene
    d.save().unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        (v["version"].as_u64(), v["min_reader"].as_u64()),
        (Some(u64::from(bc2_levelwrite::project::VERSION)), Some(7))
    );
    let again = Doc::with(MapProject::load(&path).unwrap(), path.clone(), sc);
    for e in &d.scene.entities {
        assert_eq!(again.scene.entities[e.index].matrix, e.matrix);
        assert_eq!(again.shown(e.index), d.shown(e.index));
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
#[ignore = "game tier: OPFOR's Wake Island opened in the headless editor; run_tests.py game runs it"]
fn wake_members_through_the_interface() {
    use bc2_editor::actions::Action;
    use bc2_editor::app::{EditorApp, Then};
    use bc2_editor::camera::Projection;
    use bc2_editor::gizmo::{self, Handle};
    use bc2_editor::headless::Headless;
    use eframe::egui::{Key, Modifiers};

    let _gate = crate::gate();
    if bc2_editor::levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let mut h = match Headless::new([1600, 950]) {
        Ok(h) => h,
        Err(e) => {
            println!("skipped: {e}");
            return;
        }
    };
    let tmp = temp("wake");
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    let ctx = h.ctx.clone();
    if let Err(e) = app.open_level_blocking_then(
        &gpu,
        "wake_island_s",
        Then::Create("SG1 Wake members".into(), tmp.clone(), None),
    ) {
        println!("skipped: Wake Island does not open: {e}");
        return;
    }
    h.settle(&mut app, 4);
    // a member of the static group whose kind has others (so it can be deleted), well inside the map
    let k = {
        let v = app.view().unwrap();
        let d = v.doc.as_ref().unwrap();
        let members: Vec<&bc2_scene::scene::Entity> = v
            .scene()
            .entities
            .iter()
            .filter(|e| {
                e.category == "group member"
                    && e.group
                        .as_deref()
                        .is_some_and(|g| g.to_ascii_lowercase().starts_with("staticmodelgroupentity"))
            })
            .collect();
        println!("{} static group members", members.len());
        assert!(members.len() > 1000);
        members
            .iter()
            .find(|e| {
                d.refusal(&Op::Delete {
                    guids: vec![e.guid.clone()],
                })
                .is_none()
                    && members.iter().filter(|o| o.blueprint == e.blueprint).count() > 3
            })
            .unwrap()
            .index
    };
    let guid = app.view().unwrap().scene().entities[k].guid.clone();
    app.select(&gpu, Some(k));
    app.run_action(&gpu, &ctx, Action::FocusSelection);
    h.settle(&mut app, 6);
    // clicked in the 3D view: the member is what is selected
    app.select(&gpu, None);
    h.settle(&mut app, 2);
    let at = app.project_entity(k).unwrap();
    h.click(&mut app, at);
    h.settle(&mut app, 2);
    let picked = app.view().unwrap().selected;
    println!("clicked {at:?}: selected {picked:?} (the member is {k})");
    if picked != Some(k) {
        // another object in front of it: selected as the outliner selects it
        app.select(&gpu, Some(k));
        h.settle(&mut app, 2);
    }
    // the gizmo on a member; a 6 m drag along X on a 1 m grid
    h.key(&mut app, Key::G, Modifiers::NONE);
    h.settle(&mut app, 2);
    let (o, tip) = app
        .gizmo_handle(Projection::Perspective, Handle::Axis(0))
        .expect("the gizmo on a member");
    let grab = o + (tip - o) * 0.6;
    let to = {
        let v = app.view().unwrap();
        let vp = &v.viewports[&Projection::Perspective];
        let rect = vp.rect.unwrap();
        let view = vp.camera.view([f64::from(rect.width()), f64::from(rect.height())]);
        let origin = v.scene().entities[k].position().unwrap();
        let l = gizmo::arm(&view, origin);
        grab + (tip - o) * (6.2 / l) as f32
    };
    let before = app.view().unwrap().scene().entities[k].position().unwrap();
    h.drag(&mut app, grab, to, 8, Modifiers::NONE);
    h.settle(&mut app, 3);
    let after = app.view().unwrap().scene().entities[k].position().unwrap();
    let log: Vec<Op> = {
        let d = app.view().unwrap().doc.as_ref().unwrap();
        d.project.operations.iter().map(|o| o.op.clone()).collect()
    };
    assert_eq!(log.len(), 1, "{log:?}");
    let Op::Files { edits } = &log[0] else {
        panic!("{log:?}")
    };
    assert!(matches!(&edits[0], FileEdit::MemberTransform { .. }), "{edits:?}");
    // whole metres on the grid (the drag's length on screen is the arm's measure, so about 6 m)
    let dx = after[0] - before[0];
    assert!(
        dx.fract() == 0.0 && (4.0..=7.0).contains(&dx),
        "{before:?} -> {after:?}"
    );
    assert_eq!((after[1], after[2]), (before[1], before[2]));
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        // the campaign checklist Wake's new project offers, closed: the views in full
        app.export.strip.open = false;
        app.run_action(&gpu, &ctx, Action::ShowPanel(bc2_editor::actions::PanelKind::History));
        for (theme, tag) in [
            (bc2_editor::theme::ThemeId::Dark, "dark"),
            (bc2_editor::theme::ThemeId::Light, "light"),
        ] {
            app.theme = theme;
            h.settle(&mut app, 6);
            let p = std::path::Path::new(&dir).join(format!("sg1_wake_member_moved_{tag}.png"));
            h.save(&p).unwrap();
            println!("saved {}", p.display());
        }
    }
    // Delete, then Ctrl+D on another member of its kind; Undo
    h.key(&mut app, Key::Delete, Modifiers::NONE);
    h.settle(&mut app, 2);
    {
        let d = app.view().unwrap().doc.as_ref().unwrap();
        assert!(d.index(&guid).is_none(), "deleted");
        assert!(
            d.project
                .operations
                .last()
                .unwrap()
                .name()
                .starts_with("Delete group member")
        );
    }
    let other = {
        let v = app.view().unwrap();
        let d = v.doc.as_ref().unwrap();
        let bp = v.scene().entities[k].blueprint.clone();
        v.scene()
            .entities
            .iter()
            .find(|e| e.category == "group member" && e.blueprint == bp && d.shown(e.index))
            .unwrap()
            .index
    };
    app.select(&gpu, Some(other));
    app.run_action(&gpu, &ctx, Action::Duplicate);
    h.settle(&mut app, 2);
    {
        let v = app.view().unwrap();
        let d = v.doc.as_ref().unwrap();
        let made = d.created_by_last();
        assert_eq!(made.len(), 1);
        assert!(made[0].contains("#+1"), "{made:?}");
        assert_eq!(v.selected, d.index(&made[0]), "the copy selected");
        println!(
            "{}",
            d.project
                .operations
                .iter()
                .map(bc2_levelwrite::Operation::name)
                .collect::<Vec<_>>()
                .join(" | ")
        );
    }
    app.run_action(&gpu, &ctx, Action::Undo);
    app.run_action(&gpu, &ctx, Action::Undo);
    h.settle(&mut app, 2);
    let d = app.view().unwrap().doc.as_ref().unwrap();
    assert_eq!(d.project.operations.len(), 1);
    assert!(d.index(&guid).is_some(), "the delete undone");
    let _ = std::fs::remove_dir_all(&tmp);
}
