//! Round GR2's project-format check: the projects the canary examples make (`examples/aurora_export.rs`,
//! `examples/e3d_matrix.rs`: Aurora's first export and E3d's any-base / any-era / any-mode projects) load as version-1
//! files - their `version` set to 1, the format before GR2's navigation operations - with every operation kept, and
//! as the current version; a file from a newer editor and an operation of an unknown kind are refused in words.
//!
//! `BC2_PROJECTS=<folder>`: the projects to read (`*.mapedit.json`, made by the examples outside the repository);
//! without it, skipped with a note.
#![allow(clippy::unwrap_used, clippy::print_stdout, clippy::indexing_slicing, missing_docs)]

use bc2_levelwrite::MapProject;
use bc2_levelwrite::project::{ProjectError, VERSION};

#[test]
fn canary_projects_load_as_version_1() {
    let _gate = crate::gate();
    let Some(dir) = std::env::var_os("BC2_PROJECTS") else {
        println!("skipped: BC2_PROJECTS not set (the canary examples' projects)");
        return;
    };
    let mut n = 0;
    for e in std::fs::read_dir(&dir).unwrap().flatten() {
        let p = e.path();
        if !p.to_string_lossy().ends_with(".mapedit.json") {
            continue;
        }
        let text = std::fs::read_to_string(&p).unwrap();
        let raw: serde_json::Value = serde_json::from_str(&text).unwrap();
        let ops = raw["operations"].as_array().map_or(0, Vec::len);
        let mut v1 = raw.clone();
        v1["version"] = serde_json::Value::from(1);
        let old = MapProject::from_json(&serde_json::to_string(&v1).unwrap()).unwrap();
        assert_eq!(old.version, VERSION, "{}", p.display());
        assert_eq!(old.operations.len(), ops, "{}: every operation kept", p.display());
        let now = MapProject::from_json(&text).unwrap();
        assert_eq!(now.operations, old.operations, "{}", p.display());
        let mut newer = raw.clone();
        newer["version"] = serde_json::Value::from(VERSION + 1);
        newer["min_reader"] = serde_json::Value::from(VERSION + 1);
        let e = MapProject::from_json(&serde_json::to_string(&newer).unwrap()).unwrap_err();
        assert!(matches!(e, ProjectError::Version(_)), "{e}");
        if ops > 0 {
            let mut odd = raw.clone();
            odd["operations"][0]["op"] = serde_json::Value::from("from-the-future");
            let e = MapProject::from_json(&serde_json::to_string(&odd).unwrap()).unwrap_err();
            assert!(
                matches!(&e, ProjectError::UnknownOperation { index: 0, op } if op == "from-the-future"),
                "{e}"
            );
        }
        println!(
            "{}: {ops} operations, loaded as version 1 and {VERSION}",
            p.file_name().unwrap().to_string_lossy()
        );
        n += 1;
    }
    assert!(n > 0, "no projects in {}", dir.to_string_lossy());
    println!("{n} canary projects load as version 1");
}

#[test]
// spec: XLAY-AUTO-3
fn version_7_projects_load_as_8_and_pins_need_8() {
    let _gate = crate::gate();
    // a version-7 project (round SG1's format: no pins) loads as it is, as the current version
    let bytes = crate::export_rules::Cq::clean().bytes();
    let mut p = crate::export_rules::project(&bytes, bc2_levelwrite::Profile::Bfbc2);
    p.push(
        bc2_levelwrite::Op::Lock {
            guids: vec!["AAAAAAAA-0000-0000-0000-000000000002".into()],
            locked: true,
        },
        "",
    );
    let mut v7: serde_json::Value = serde_json::from_str(&p.to_json().unwrap()).unwrap();
    v7["version"] = serde_json::Value::from(7);
    v7["min_reader"] = serde_json::Value::from(1);
    let old = MapProject::from_json(&v7.to_string()).unwrap();
    const { assert!(VERSION >= 8) };
    assert_eq!(old.version, VERSION);
    assert_eq!(old.operations, p.operations);
    assert_eq!(old.reader_needed(), 1);
    // a pin (round A2) needs version 8; it is the project's organisation, never the partition's
    p.push(
        bc2_levelwrite::Op::Pin {
            guids: vec!["aaaaaaaa-0000-0000-0000-000000000002".into()],
            pinned: true,
        },
        "",
    );
    assert_eq!(p.reader_needed(), 8);
    let last = &p.operations.last().unwrap().op;
    assert!(!last.edits_partition());
    assert_eq!(last.describe(), "Pin 1 object for the auto layout");
    let org = bc2_levelwrite::ops::Organisation::of(&p.operations);
    assert!(org.pinned.contains("AAAAAAAA-0000-0000-0000-000000000002"));
    let again = MapProject::from_json(&p.to_json().unwrap()).unwrap();
    assert_eq!(again.operations, p.operations);
    let text = p.to_json().unwrap();
    assert!(text.contains("\"op\": \"pin\""), "{text}");
    // an editor of version 7 is told the file is newer
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["min_reader"].as_u64(), Some(8));
}
