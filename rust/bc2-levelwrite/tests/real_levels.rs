//! levelwrite on real level partitions (round E2's proof): MP_005 and SP_01 from OPFOR's prepared levels and a WW2
//! level (Guadalcanal) from OPFOR's `work\ww2`, each copied to a temp folder first (OPFOR's files are only read).
//!
//! A project's log of moves, rotates, duplicates (with and without event connections), deletes (with and without)
//! and typed field sets is applied to each base, and the edited partition:
//!   (a) parses (the DBX reader, the editable reader);
//!   (b) differs from the base only where the operations say (field-level diff, exactly the expected set);
//!   (c) round-trips through the Rust DBX writers (`EditDbx` byte for byte; `dbx::write` to the same tree and XML);
//!   (d) loads in bc2-scene with each moved entity at its new transform, the copies at theirs, the deleted gone.
//! Against references: a delete without connections is DbxEdit's `drop` byte for byte (with connections: the same
//! but for the connections, which levelwrite removes and DbxEdit leaves pointing at `null`); text and number field
//! sets are DbxEdit's `setfield` byte for byte; the Python `dbxbin` reference (`tests/dbxbin_reference.py`, the edits
//! `author.py` makes) gives the same bytes for transforms, duplicates and field sets when Python is installed.
//!
//! Skipped (with a note) without the game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::too_many_lines,
    clippy::panic,
    clippy::similar_names,
    clippy::unnested_or_patterns,
    clippy::doc_markdown,
    missing_docs
)]

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use bc2_formats::dbx::edit::EditDbx;
use bc2_formats::dbx::{self, CONTAINER};
use bc2_formats::dbxedit::{EditFile, editor};
use bc2_levelwrite::diff::{Change, diff};
use bc2_levelwrite::ops::{Duplicated, FieldSet, Moved};
use bc2_levelwrite::write::refers_to;
use bc2_levelwrite::{FieldType, FieldValue, LevelWriter, MapProject, Op, Profile, Rows, Schema, apply};

fn opfor() -> Option<PathBuf> {
    if let Some(r) = std::env::var_os("OPFOR_ROOT").map(PathBuf::from).filter(|p| p.is_dir()) {
        return Some(r);
    }
    let here = Path::new(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2"))
        .find(|p| p.join("work").is_dir())
}

/// The bases: (name, level, the partition in OPFOR's work folder).
fn bases(root: &Path) -> Vec<(&'static str, &'static str, PathBuf)> {
    let lv = |lv: &str, file: &str| -> PathBuf {
        let b = root.join("work").join("levels").join(lv);
        let pkg = b.join("pkg").join("levels").join(file);
        if pkg.is_file() {
            pkg
        } else {
            b.join("dist").join("levels").join(file)
        }
    };
    vec![
        ("MP_005", "mp_005", lv("mp_005", "MP_005.dbx")),
        ("SP_01", "sp_01", lv("sp_01", "SP_01.dbx")),
        (
            "guadal_canal",
            "guadal_canal",
            root.join("work")
                .join("ww2")
                .join("guadal_canal")
                .join("levels")
                .join("guadal_canal.dbx"),
        ),
    ]
}

/// The base copied to a temp folder (OPFOR's file is only read).
fn copy_to_temp(src: &Path, tag: &str) -> (PathBuf, Vec<u8>) {
    let dir = std::env::temp_dir().join(format!("bc2_levelwrite_{}_{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let dst = dir.join(src.file_name().unwrap());
    std::fs::copy(src, &dst).unwrap();
    let bytes = std::fs::read(&dst).unwrap();
    (dir, bytes)
}

fn up(b: &[u8]) -> String {
    String::from_utf8_lossy(b).to_ascii_uppercase()
}

/// Which instances (and their top fields) refer to a guid anywhere below them.
fn referrers(d: &EditDbx, guid: &str) -> BTreeSet<(String, String)> {
    let mut out = BTreeSet::new();
    for inst in d.instances() {
        let g = up(d.attr(inst, b"guid").unwrap_or_default());
        for &c in d.children(inst).unwrap_or_default() {
            if d.walk(c)
                .iter()
                .any(|&x| d.attr(x, b"ref").is_some_and(|r| refers_to(r, guid.as_bytes())))
            {
                let name = String::from_utf8_lossy(d.attr(c, b"name").unwrap_or_default()).into_owned();
                out.insert((g.clone(), name));
            }
        }
    }
    out
}

struct Picks {
    level_data: String,
    in_objects: Vec<String>,
    connected: HashSet<String>,
}

fn picks(w: &LevelWriter) -> Picks {
    let d = &w.d;
    let ld = w.level_data().expect("a LevelData");
    let level_data = up(d.attr(ld, b"guid").unwrap());
    let in_objects = d
        .child(ld, b"Objects")
        .and_then(|o| d.children(o))
        .unwrap_or_default()
        .iter()
        .filter_map(|&i| d.attr(i, b"ref"))
        .map(up)
        .collect();
    let mut connected = HashSet::new();
    for inst in d.instances() {
        let Some(a) = d.child(inst, b"EventConnections").filter(|&a| d.kind(a) == CONTAINER) else {
            continue;
        };
        for &c in d.children(a).unwrap_or_default() {
            for f in [&b"Source"[..], b"Target"] {
                if let Some(r) = d.child(c, f).and_then(|x| d.attr(x, b"ref")) {
                    connected.insert(up(r));
                }
            }
        }
    }
    Picks {
        level_data,
        in_objects,
        connected,
    }
}

/// Instances with a Transform, in Objects, of a type, with or without connections; not in `used`.
fn pick(w: &LevelWriter, p: &Picks, suffix: &str, connected: bool, used: &mut HashSet<String>) -> Option<String> {
    for inst in w.d.instances() {
        let g = up(w.d.attr(inst, b"guid").unwrap_or_default());
        if used.contains(&g)
            || !w.type_of(inst).ends_with(suffix)
            || w.transform(&g).is_none()
            || !p.in_objects.contains(&g)
            || p.connected.contains(&g) != connected
        {
            continue;
        }
        used.insert(g.clone());
        return Some(g);
    }
    None
}

/// A field of this type on some instance not in `used`: (guid, field, value now).
fn field_of(
    w: &LevelWriter,
    s: &Schema,
    want: &dyn Fn(&FieldType) -> bool,
    used: &HashSet<String>,
) -> Option<(String, String, FieldValue)> {
    for inst in w.d.instances() {
        let g = up(w.d.attr(inst, b"guid").unwrap_or_default());
        if used.contains(&g) || w.type_of(inst).ends_with(".LevelData") {
            continue;
        }
        for &c in w.d.children(inst).unwrap_or_default() {
            let Some(name) = w.d.attr(c, b"name") else { continue };
            let name = String::from_utf8_lossy(name).into_owned();
            let Ok((ty, v)) = w.get_field(s, &g, &name) else {
                continue;
            };
            // a number that is 0 reads either way: DbxEdit's setfield would type it as whole
            let nonzero = !matches!(v, Some(FieldValue::Int(0)) | Some(FieldValue::Float(0.0)));
            if want(&ty)
                && nonzero
                && let Some(v) = v
            {
                return Some((g, name, v));
            }
        }
    }
    None
}

fn turned(r: &Rows, deg: f32) -> Rows {
    let (s, c) = deg.to_radians().sin_cos();
    let yaw = |v: [f32; 3]| [v[0] * c + v[2] * s, v[1], -v[0] * s + v[2] * c];
    [yaw(r[0]), yaw(r[1]), yaw(r[2]), r[3]]
}

fn moved(r: &Rows, by: [f32; 3]) -> Rows {
    let mut o = *r;
    for k in 0..3 {
        o[3][k] += by[k];
    }
    o
}

#[test]
fn edits_on_real_levels() {
    let Some(root) = opfor() else {
        println!("skipped: no game copy (project-opfor-bfbc2)");
        return;
    };
    let mut done = 0;
    let python = python_ok();
    for (name, lv, src) in bases(&root) {
        if !src.is_file() {
            println!("skipped {name}: {} not there", src.display());
            continue;
        }
        let (dir, base) = copy_to_temp(&src, lv);
        let w = LevelWriter::parse(&base).unwrap();
        assert_eq!(
            w.write().unwrap(),
            base,
            "{name}: an untouched partition writes back byte for byte"
        );
        let schema = Schema::scan([&w.d]);
        let p = picks(&w);
        let mut used = HashSet::new();
        // the objects: placed objects and gameplay entities with and without event connections
        let mv = pick(&w, &p, "ReferenceObjectData", false, &mut used).expect("a placed object");
        let rot = pick(&w, &p, "ReferenceObjectData", false, &mut used).expect("a second placed object");
        let dup_plain = pick(&w, &p, "ReferenceObjectData", false, &mut used).expect("a third placed object");
        let del_plain = pick(&w, &p, "ReferenceObjectData", false, &mut used).expect("a fourth placed object");
        let dup_conn = pick(&w, &p, "", true, &mut used);
        let del_conn = pick(&w, &p, "", true, &mut used);
        let text = field_of(&w, &schema, &|t| *t == FieldType::Text, &used);
        let int = field_of(&w, &schema, &|t| *t == FieldType::Int, &used);
        let float = field_of(&w, &schema, &|t| *t == FieldType::Float, &used);
        let boolean = field_of(&w, &schema, &|t| *t == FieldType::Bool, &used);
        // the log, as the editor writes it
        let mut proj = MapProject::new(
            &format!("{name} proof"),
            lv,
            &format!("levels/{name}"),
            &base,
            Profile::of_level(lv),
        );
        let tr = |g: &str| w.transform(g).unwrap();
        let (mv_to, rot_to) = (
            moved(&tr(&mv), [12.5, -0.75, 3.0]),
            turned(&moved(&tr(&rot), [0.0, 1.0, 0.0]), 37.0),
        );
        proj.push(
            Op::Move {
                items: vec![Moved {
                    guid: mv.clone(),
                    from: tr(&mv),
                    to: mv_to,
                    placement: None,
                }],
            },
            "",
        );
        proj.push(
            Op::Rotate {
                items: vec![Moved {
                    guid: rot.clone(),
                    from: tr(&rot),
                    to: rot_to,
                    placement: None,
                }],
            },
            "",
        );
        let mut dups: Vec<(String, String, Rows)> = Vec::new();
        for s in std::iter::once(&dup_plain).chain(dup_conn.as_ref()) {
            let g = proj.new_guid(s);
            let t = moved(&tr(s), [5.0, 0.0, 5.0]);
            proj.push(
                Op::Duplicate {
                    items: vec![Duplicated {
                        source: s.clone(),
                        guid: g.clone(),
                        transform: t,
                    }],
                },
                "",
            );
            dups.push((s.clone(), g, t));
        }
        let deletes: Vec<String> = std::iter::once(del_plain.clone()).chain(del_conn.clone()).collect();
        proj.push(Op::Delete { guids: deletes.clone() }, "");
        let mut sets: Vec<(String, String, FieldValue)> = Vec::new();
        if let Some((g, f, _)) = &text {
            sets.push((g.clone(), f.clone(), FieldValue::Text("OPFOR map editor".into())));
        }
        if let Some((g, f, FieldValue::Int(v))) = &int {
            sets.push((g.clone(), f.clone(), FieldValue::Int(v + 1)));
        }
        if let Some((g, f, FieldValue::Float(v))) = &float {
            sets.push((g.clone(), f.clone(), FieldValue::Float(v + 0.5)));
        }
        if let Some((g, f, FieldValue::Bool(v))) = &boolean {
            sets.push((g.clone(), f.clone(), FieldValue::Bool(!v)));
        }
        for (g, f, v) in &sets {
            proj.push(
                Op::SetField {
                    items: vec![FieldSet {
                        guid: g.clone(),
                        field: f.clone(),
                        from: None,
                        to: v.clone(),
                    }],
                },
                "",
            );
        }
        // the log survives its file
        let path = dir.join(proj.file_name());
        proj.save(&path).unwrap();
        let proj = MapProject::load(&path).unwrap();
        let t0 = std::time::Instant::now();
        let out = apply(&base, &proj.operations, &schema).unwrap();
        let secs = t0.elapsed().as_secs_f64();
        // (a) it parses, both ways
        let tree = dbx::parse(&out.bytes).unwrap();
        let edited = EditDbx::parse(&out.bytes).unwrap();
        // (b) only what the operations say changed
        let a = EditDbx::parse(&base).unwrap();
        let got: BTreeSet<Change> = diff(&a, &edited).into_iter().collect();
        let mut want: BTreeSet<Change> = BTreeSet::new();
        want.insert(Change::Field(mv.clone(), "Transform".into()));
        want.insert(Change::Field(rot.clone(), "Transform".into()));
        for (_, g, _) in &dups {
            want.insert(Change::Added(g.clone()));
        }
        want.insert(Change::Field(p.level_data.clone(), "Objects".into()));
        if dup_conn.is_some() || del_conn.is_some() {
            want.insert(Change::Field(p.level_data.clone(), "EventConnections".into()));
        }
        for g in &deletes {
            want.insert(Change::Removed(g.clone()));
            for (inst, field) in referrers(&a, g) {
                if !deletes.contains(&inst) {
                    want.insert(Change::Field(inst, field));
                }
            }
        }
        for (g, f, _) in &sets {
            want.insert(Change::Field(g.clone(), f.clone()));
        }
        assert_eq!(
            got, want,
            "{name}: the edited partition differs from its base exactly where the operations say"
        );
        // (c) round trips: the editable reader and writer byte for byte; dbx::write to the same tree and XML
        assert_eq!(edited.write().unwrap(), out.bytes, "{name}");
        // dbx::write makes a fresh string table: the same elements and values (the XML), and a fixed point
        let rewritten = dbx::write(&tree).unwrap();
        assert!(
            dbx::to_xml(&rewritten).unwrap() == dbx::to_xml(&out.bytes).unwrap(),
            "{name}: dbx::write gives the same elements and values"
        );
        assert!(
            dbx::write(&dbx::parse(&rewritten).unwrap()).unwrap() == rewritten,
            "{name}: dbx::write round trip"
        );
        // (d) the scene loads it: moved, turned and copied entities where the log put them, deleted gone
        let part = dir.join(format!("{name}.edited.dbx"));
        std::fs::write(&part, &out.bytes).unwrap();
        let scene = bc2_scene::scene::load(&bc2_scene::scene::LoadOptions::new(name, &part, Vec::new())).unwrap();
        let at = |g: &str| scene.entities.iter().find(|e| e.guid.eq_ignore_ascii_case(g));
        let close = |g: &str, rows: &Rows| {
            let m = at(g)
                .unwrap_or_else(|| panic!("{name}: {g} not in the scene"))
                .matrix
                .unwrap();
            assert_eq!(
                bc2_levelwrite::rows_of(&m),
                *rows,
                "{name}: {g}'s transform in the scene"
            );
        };
        close(&mv, &mv_to);
        close(&rot, &rot_to);
        for (_, g, t) in &dups {
            close(g, t);
        }
        for g in &deletes {
            assert!(at(g).is_none(), "{name}: {g} deleted");
        }
        assert_eq!(scene.entities.len(), a.instances().len() + dups.len() - deletes.len());
        // references: DbxEdit's drop and setfield, byte for byte
        let mut refs = Vec::new();
        {
            let mut ours = LevelWriter::parse(&base).unwrap();
            ours.delete(&del_plain).unwrap();
            let mut de = EditFile::parse(base.clone()).unwrap();
            let (n, _) = editor::drop(&mut de, &del_plain).unwrap();
            assert_eq!(n, 1);
            assert_eq!(
                ours.write().unwrap(),
                de.serialize().unwrap(),
                "{name}: delete = DbxEdit drop"
            );
            refs.push("delete (no connections) = DbxEdit drop");
        }
        if let Some(g) = &del_conn {
            let mut ours = LevelWriter::parse(&base).unwrap();
            let r = ours.delete(g).unwrap();
            let mut de = EditFile::parse(base.clone()).unwrap();
            editor::drop(&mut de, g).unwrap();
            let theirs = EditDbx::parse(&de.serialize().unwrap()).unwrap();
            let mine = EditDbx::parse(&ours.write().unwrap()).unwrap();
            assert_eq!(
                diff(&theirs, &mine),
                vec![Change::Field(p.level_data.clone(), "EventConnections".into())],
                "{name}: delete with connections = DbxEdit drop but for the connections"
            );
            assert!(r.connections > 0);
            refs.push("delete (connections) = DbxEdit drop + connections removed");
        }
        for (g, f, v) in &sets {
            let dbxedit_text = match v {
                FieldValue::Text(s) => s.clone(),
                FieldValue::Int(i) => i.to_string(),
                FieldValue::Float(x) => format!("{}", *x as f32),
                _ => continue,
            };
            let mut ours = LevelWriter::parse(&base).unwrap();
            ours.set_field(&schema, g, f, v).unwrap();
            let mut de = EditFile::parse(base.clone()).unwrap();
            assert!(editor::set_field(&mut de, g, f, &dbxedit_text).unwrap());
            assert_eq!(
                ours.write().unwrap(),
                de.serialize().unwrap(),
                "{name}: set {f} = DbxEdit setfield"
            );
            refs.push(match v {
                FieldValue::Text(_) => "set text = DbxEdit setfield",
                FieldValue::Int(_) => "set whole number = DbxEdit setfield",
                _ => "set decimal = DbxEdit setfield",
            });
        }
        // the Python reference (dbxbin, the edits author.py makes)
        if python {
            let mut edits = Vec::new();
            for o in &proj.operations {
                match &o.op {
                    Op::Move { items } | Op::Rotate { items } => {
                        for m in items {
                            edits.push(serde_json::json!({"kind": "transform", "guid": m.guid, "rows": bits(&m.to)}));
                        }
                    }
                    Op::Duplicate { items } => {
                        for d in items {
                            edits.push(serde_json::json!({"kind": "duplicate", "source": d.source, "guid": d.guid, "rows": bits(&d.transform)}));
                        }
                    }
                    Op::SetField { items } => {
                        for f in items {
                            edits.push(
                                serde_json::json!({"kind": "field", "guid": f.guid, "field": f.field, "value": f.to}),
                            );
                        }
                    }
                    _ => {}
                }
            }
            // the same log without its deletes (dbxbin has no delete; DbxEdit is that reference)
            let keep: Vec<_> = proj
                .operations
                .iter()
                .filter(|o| !matches!(o.op, Op::Delete { .. }))
                .collect();
            let ours = apply(&base, keep, &schema).unwrap();
            let theirs = python_reference(&dir, &dir.join(src.file_name().unwrap()), &edits);
            assert_eq!(ours.bytes, theirs, "{name}: the Python dbxbin reference");
            refs.push("transforms, duplicates, field sets = Python dbxbin");
        }
        println!(
            "{name}: {} instances, {} operations applied in {:.3} s ({} transforms, {} duplicates, {} deleted, {} \
             fields); {} changes, exactly the expected; parse / round trip / scene ok; references: {}",
            a.instances().len(),
            out.operations,
            secs,
            out.transforms,
            out.duplicated,
            out.deleted,
            out.fields,
            got.len(),
            refs.join("; ")
        );
        let _ = std::fs::remove_dir_all(&dir);
        done += 1;
    }
    println!("levels edited and proven: {done}");
}

fn bits(r: &Rows) -> Vec<Vec<u32>> {
    r.iter().map(|row| row.iter().map(|v| v.to_bits()).collect()).collect()
}

fn python_ok() -> bool {
    let ok = std::process::Command::new("python")
        .args(["-c", "import struct"])
        .output()
        .is_ok_and(|o| o.status.success());
    if !ok {
        println!("the Python reference is skipped: no python on PATH");
    }
    ok
}

fn python_reference(dir: &Path, base: &Path, edits: &[serde_json::Value]) -> Vec<u8> {
    let script = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("dbxbin_reference.py");
    let job = dir.join("edits.json");
    let out = dir.join("python.dbx");
    std::fs::write(&job, serde_json::to_string(edits).unwrap()).unwrap();
    let r = std::process::Command::new("python")
        .arg(&script)
        .arg(base)
        .arg(&job)
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        r.status.success(),
        "the Python reference failed: {}",
        String::from_utf8_lossy(&r.stderr)
    );
    std::fs::read(&out).unwrap()
}
