//! Round GR4's proofs in the running editor, without a window (skipped without a graphics adapter;
//! `BC2_EDITOR_SHOTS=<folder>` saves the screenshots, Dark and Light):
//!
//! - **The window on a file**: the start screen's "Inspect a DBX file" opens the DBX inspector; a file opened in it
//!   shows its tree; a value typed and applied changes the file as `bfbc2.exe dbxedit edit ... setfield=` does (the
//!   same bytes); a denormal's text typed again changes nothing; Save as writes those bytes; Undo returns the file;
//!   Find selects the first match.
//! - **The project's partition** (game tier, `MP_005`): a field of the project's level partition edited in the
//!   inspector becomes an operation of the project (History), whose partition then holds exactly the bits the same
//!   edit gives the file (a decimal, a whole number, a denormal's bits); Ctrl+Z takes it back.
#![allow(
    clippy::panic,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    clippy::map_unwrap_or,
    clippy::if_not_else,
    missing_docs
)]

use std::path::{Path, PathBuf};

use bc2_editor::app::{EditorApp, Then};
use bc2_editor::dbxinspect::{DbxDoc, Source};
use bc2_editor::headless::Headless;
use bc2_editor::theme::ThemeId;
use bc2_editor::{levels, render};
use bc2_formats::dbxedit::{self as dx, CONTAINER, EditDbx, NUMBERS, Parts, TEXT};
use bc2_levelwrite::{FieldType, Op};

const SIZE: [u32; 2] = [1500, 950];

fn shot(name: &str, h: &mut Headless) {
    if let Some(dir) = std::env::var_os("BC2_EDITOR_SHOTS") {
        let p = Path::new(&dir).join(name);
        render::write_png(&p, SIZE, &h.image().unwrap()).unwrap();
        println!("saved {}", p.display());
    }
}

fn temp(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bc2_editor_gr4_{name}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A small binary `.dbx`: a partition, an instance with a name, two decimals, a denormal, a bool, a reference and a
/// list, and a second instance.
fn sample() -> Vec<u8> {
    let mut f = dx::empty();
    let mut t = |s: &str| dx::intern(&mut f, s) as i32;
    let [partition, instance, field, array, item, guid, ty, id, name, r] = [
        "partition",
        "instance",
        "field",
        "array",
        "item",
        "guid",
        "type",
        "id",
        "name",
        "ref",
    ]
    .map(&mut t);
    let [g1, g2, gp, t1, t2, text] = [
        "11111111-1111-1111-1111-111111111111",
        "22222222-2222-2222-2222-222222222222",
        "AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA",
        "Entity.CrateEntityData",
        "Entity.SpawnEntityData",
        "objects/props/crate",
    ]
    .map(&mut t);
    let [n_name, n_scale, n_tiny, n_on, n_link, n_list] =
        ["Name", "Scale", "Tiny", "Enabled", "Link", "Objects"].map(&mut t);
    let numbers = |vals: &[u32]| {
        let mut p = dx::varint_bytes(vals.len() as i32);
        p.extend(dx::varint_bytes(4));
        for v in vals {
            p.extend(v.to_be_bytes());
        }
        p
    };
    let ids = |a: Vec<(i32, i32)>| a.into_iter().map(|(k, v)| (k as u32, v as u32)).collect();
    let value = |f: &mut EditDbx, tag: i32, attributes, kind, payload| {
        f.push(Parts {
            tag: tag as u32,
            kind,
            attrs: ids(attributes),
            payload,
            ..Parts::default()
        })
    };
    let cont = |f: &mut EditDbx, tag: i32, attributes, children| {
        f.push(Parts {
            tag: tag as u32,
            kind: CONTAINER,
            attrs: ids(attributes),
            children: Some(children),
            ..Parts::default()
        })
    };
    let fs = &mut f;
    let mut kids = vec![
        value(fs, field, vec![(name, n_name)], TEXT, dx::varint_bytes(text)),
        value(
            fs,
            field,
            vec![(name, n_scale)],
            NUMBERS,
            numbers(&[1.5f32.to_bits(), 2.0f32.to_bits()]),
        ),
        value(fs, field, vec![(name, n_tiny)], NUMBERS, numbers(&[0x0008_0000])),
        value(fs, field, vec![(name, n_on)], 6, vec![1, 0]),
        value(fs, field, vec![(name, n_link), (r, g2)], TEXT, dx::varint_bytes(0)),
    ];
    let it = value(fs, item, vec![(r, g2)], TEXT, dx::varint_bytes(0));
    kids.push(cont(fs, array, vec![(name, n_list)], vec![it]));
    let i1 = cont(fs, instance, vec![(guid, g1), (ty, t1), (id, text)], kids);
    let i2 = cont(fs, instance, vec![(guid, g2), (ty, t2)], vec![]);
    let p = cont(fs, partition, vec![(guid, gp)], vec![i1, i2]);
    fs.roots_mut().push(p);
    dx::serialize(&f).unwrap()
}

fn click(h: &mut Headless, app: &mut EditorApp, label: &str) {
    let r = app
        .dbx
        .rects
        .iter()
        .find(|(l, _)| l == label)
        .map(|(_, r)| *r)
        .unwrap_or_else(|| {
            panic!(
                "no {label} on screen: {:?}",
                app.dbx.rects.iter().map(|x| &x.0).collect::<Vec<_>>()
            )
        });
    h.click(app, r.center());
}

#[test]
fn the_inspector_window_edits_a_file_as_dbxedit_does() {
    let _gate = crate::gate();
    let Ok(mut h) = Headless::new(SIZE) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let dir = temp("window");
    let file = dir.join("Sample.dbx");
    std::fs::write(&file, sample()).unwrap();
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 3);
    // the start screen's link opens the window
    let link = app.start_button("Inspect a DBX file").expect("the start screen's link");
    h.click(&mut app, link.center());
    assert!(app.dbx.open, "the start screen's link opens the DBX inspector");
    app.dbx.open_file(&file);
    h.settle(&mut app, 3);
    assert!(app.dbx.rects.iter().any(|(l, _)| l == "row 0,0"), "the instance's row");
    // a row clicked selects it; a value typed and applied
    app.dbx.select(Some(vec![0, 0]));
    h.settle(&mut app, 2);
    click(&mut h, &mut app, "row 0,0");
    assert_eq!(app.dbx.selected, Some(vec![0, 0]));
    app.dbx.select(Some(vec![0, 0, 1]));
    h.settle(&mut app, 2);
    assert_eq!(app.dbx.value_edit, "1.5/2");
    app.dbx.value_edit = "0.25/3.14159265358979".into();
    click(&mut h, &mut app, "Apply");
    let mut cli = dx::read(&sample()).unwrap();
    dx::editor::set_field(
        &mut cli,
        "11111111-1111-1111-1111-111111111111",
        "Scale",
        "0.25/3.14159265358979",
    )
    .unwrap();
    let want = dx::serialize(&cli).unwrap();
    let d = app.dbx.doc.as_ref().unwrap();
    assert_eq!(d.bytes().unwrap(), want, "Apply gives dbxedit setfield's bytes");
    assert!(
        d.log
            .iter()
            .any(|l| l.starts_with("Rounded to the nearest 4-byte value")),
        "{:?}",
        d.log
    );
    // the denormal's text typed again: nothing changes (its bits kept, not 524288.0)
    app.dbx.select(Some(vec![0, 0, 2]));
    h.settle(&mut app, 2);
    assert_eq!(app.dbx.value_edit, "524288");
    click(&mut h, &mut app, "Apply");
    assert_eq!(app.dbx.doc.as_ref().unwrap().bytes().unwrap(), want);
    for (theme, tag) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = theme;
        h.settle(&mut app, 4);
        shot(&format!("gr4_inspector_{tag}.png"), &mut h);
    }
    // Save as through its question
    click(&mut h, &mut app, "Save as\u{2026}");
    let out = dir.join("Saved.dbx");
    app.dbx.prompt.as_mut().unwrap().fields[0] = out.display().to_string();
    h.settle(&mut app, 1);
    click(&mut h, &mut app, "OK");
    assert_eq!(std::fs::read(&out).unwrap(), want, "Save as writes the edited bytes");
    assert!(app.dbx.prompt.is_none());
    // Undo, Find
    click(&mut h, &mut app, "Undo");
    assert_eq!(app.dbx.doc.as_ref().unwrap().bytes().unwrap(), sample());
    app.dbx.find = "crate".into();
    h.settle(&mut app, 1);
    click(&mut h, &mut app, "Find");
    assert_eq!(
        app.dbx.selected,
        Some(vec![0, 0]),
        "the first match: the instance's type"
    );
    // a question shown, for the record
    app.dbx.ask(bc2_editor::dbxpanel::Ask::ReplacePrefix);
    for (theme, tag) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = theme;
        h.settle(&mut app, 4);
        shot(&format!("gr4_prompt_{tag}.png"), &mut h);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// The path of instance `guid`'s field `field` in the inspector's file.
fn field_path(d: &DbxDoc, guid: &str, field: &str) -> Option<Vec<usize>> {
    let i = d.instance_path(guid)?;
    let e = d.element(&i)?;
    let k = d
        .kids(e)
        .iter()
        .position(|&c| d.get(c, "name").as_deref() == Some(field))?;
    Some([i, vec![k]].concat())
}

#[test]
#[ignore = "game tier: MP_005's project partition (run_tests.py game)"]
fn project_partition_edits_are_project_operations() {
    let _gate = crate::gate();
    if levels::find_opfor(None).is_err() {
        println!("skipped: no game copy");
        return;
    }
    let Ok(mut h) = Headless::new(SIZE) else {
        println!("skipped: no graphics adapter");
        return;
    };
    let dir = temp("project");
    let mut app = EditorApp::new(None, None).unwrap();
    h.settle(&mut app, 2);
    let gpu = h.gpu.clone();
    app.open_level_blocking_then(&gpu, "mp_005", Then::Create("GR4 inspector".into(), dir.clone(), None))
        .unwrap();
    // a decimal field, a whole-number field of the level's objects
    let (float, int) = {
        let doc = app.view().unwrap().doc.as_ref().unwrap();
        let mut float = None;
        let mut int = None;
        for e in &doc.scene.entities {
            for r in doc.fields(e.index) {
                if r.path.contains('.') {
                    continue;
                }
                match r.ty {
                    FieldType::Float if float.is_none() => float = Some((e.guid.clone(), r.path.clone())),
                    FieldType::Int if int.is_none() => int = Some((e.guid.clone(), r.path.clone())),
                    _ => {}
                }
            }
            if float.is_some() && int.is_some() {
                break;
            }
        }
        (float.unwrap(), int.unwrap())
    };
    let mut panel = std::mem::take(&mut app.dbx);
    panel.open_project(app.view().unwrap().doc.as_ref().unwrap());
    app.dbx = panel;
    let base = app.dbx.doc.as_ref().unwrap().clone();
    assert!(matches!(base.source, Source::Project { .. }));
    for ((guid, field), text) in [(&float, "0.30000000000000004"), (&float, "524289"), (&int, "7")] {
        let d = app.dbx.doc.as_ref().unwrap().clone();
        let p = field_path(&d, guid, field).unwrap_or_else(|| panic!("{guid} {field}"));
        // what the same edit gives the file
        let mut alone = d.clone();
        alone.source = Source::File(PathBuf::new());
        let attrs: Vec<String> = d.attributes(d.element(&p).unwrap()).into_iter().map(|a| a.1).collect();
        alone.apply_edit(&p, &attrs, text).unwrap();
        let want = alone.file.payload(alone.element(&p).unwrap()).to_vec();
        let ops = app.view().unwrap().doc.as_ref().unwrap().project.operations.len();
        app.dbx.select(Some(p.clone()));
        h.settle(&mut app, 2);
        app.dbx.value_edit = text.into();
        click(&mut h, &mut app, "Apply");
        h.settle(&mut app, 2);
        let doc = app.view().unwrap().doc.as_ref().unwrap();
        assert_eq!(
            doc.project.operations.len(),
            ops + 1,
            "{field} = {text}: one operation, {:?}",
            app.dbx.error
        );
        assert!(matches!(doc.project.operations.last().unwrap().op, Op::SetField { .. }));
        // the inspector follows the project; the partition holds the file's bits
        let now = app.dbx.doc.as_ref().unwrap();
        assert_eq!(
            now.file.payload(now.element(&p).unwrap()),
            want,
            "{field} = {text}: the bits the file takes"
        );
    }
    for (theme, tag) in [(ThemeId::Dark, "dark"), (ThemeId::Light, "light")] {
        app.theme = theme;
        h.settle(&mut app, 4);
        shot(&format!("gr4_project_{tag}.png"), &mut h);
    }
    // Ctrl+Z takes the edits back: the partition as it was
    let ctx = h.ctx.clone();
    for _ in 0..3 {
        app.run_action(&gpu, &ctx, bc2_editor::actions::Action::Undo);
    }
    h.settle(&mut app, 3);
    assert_eq!(
        app.dbx.doc.as_ref().unwrap().bytes().unwrap(),
        base.bytes().unwrap(),
        "undone in the project, undone in the inspector"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
