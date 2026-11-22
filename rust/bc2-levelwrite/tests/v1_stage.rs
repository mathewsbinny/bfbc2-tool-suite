//! Round V1 on real data (game tier): the export's rules of the build and the stage on a staged canary and on broken
//! copies of it.
//!
//! - `v1_stage_rules`: E3d's Atacama Rush made a BFBC2 Conquest level (`export_ex1`'s canary) staged and checked:
//!   every check line carries its rule's code, every code is a rule of the registry, the stage passes (no error) and
//!   holds round V1's new lines (XVAL-REG-4's note, XVAL-SND-2, XVAL-TERR-1, XVAL-CONS-1, XVAL-GRP-1, XVAL-SHDR-2).
//!   A copy broken in one place per rule - a recorded sha1 and entry count, a bundleManifest line, a missing target
//!   bundle, the menus' list items, the name keys, the donor, the merged 40 km/h sign entries, streamed wave data, a
//!   deleted sound marker, a console texture, a changed terrain entry - fails exactly those rules' lines.
//! - `v1_build_refusals`: the build refuses, with the rule's code first, a stock level's id (XVAL-REG-3), Onslaught
//!   (XVAL-MODE-1), a paint stroke (XVAL-EDIT-5), an operation naming an object the changed base lacks
//!   (XVAL-EDIT-2), an object moved beyond +-2048 m (XVAL-CULL-4); `validate::project` gives the log's own rules
//!   (XVAL-EDIT-1 to 3) on those projects.
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); writes only into a temporary folder; skipped without a
//! game copy.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::print_stderr,
    clippy::too_many_lines,
    missing_docs
)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use bc2_formats::fbrb;
use bc2_levelwrite::export::{self, ExportSpec, Game, StageRecord, StagedEntry};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::shaders::DatabaseAdds;
use bc2_levelwrite::{MapProject, Op, Profile, rules};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

/// Atacama's Rush with the BFBC2 Conquest target (EX1's canary).
fn canary(game: &Game) -> MapProject {
    let base = export::level_partition(game, "mp_005gr").unwrap();
    let mut p = MapProject::new("V1 rules", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let c = export::choose_mode(game, &p, Profile::Bfbc2, "Conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    p
}

fn spec_of(p: &MapProject, id: &str) -> ExportSpec {
    ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: id.into(),
        folder: String::new(),
        title: "V1 rules canary".into(),
        mode: "conquest".into(),
        layer: LayerSpec {
            donor: p.mode().map_or_else(String::new, |c| c.donor.clone()),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Bfbc2,
        identity: None,
    }
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            copy_dir(&p, &to.join(e.file_name()));
        } else {
            std::fs::copy(&p, to.join(e.file_name())).unwrap();
        }
    }
}

/// A bundle of a stage changed by `f` and packed again, its record's sha1, size and entry count updated.
fn rebundle(stage: &Path, rec: &mut StageRecord, path: &str, f: impl FnOnce(&mut Vec<StagedEntry>)) {
    let (h, data) = fbrb::extract_path_uncached(stage.join(path)).unwrap();
    let mut entries: Vec<StagedEntry> = h
        .entries
        .iter()
        .zip(data)
        .map(|(e, d)| StagedEntry {
            name: e.name.clone(),
            type_name: e.type_name.clone(),
            data: d,
        })
        .collect();
    f(&mut entries);
    let bytes = export::pack_entries(&entries).unwrap();
    std::fs::write(stage.join(path), &bytes).unwrap();
    let r = rec.files.iter_mut().find(|x| x.path == path).unwrap();
    r.sha1 = bc2_levelwrite::project::sha1_hex(&bytes);
    r.size = bytes.len() as u64;
    r.entries = entries.len();
}

fn failed(r: &export::CheckReport) -> BTreeSet<String> {
    r.lines()
        .filter(|(_, ok, _)| !ok)
        .map(|(c, _, _)| c.to_owned())
        .collect()
}

#[test]
#[ignore = "game tier: a whole staged export and broken copies; run_tests.py game runs it (tests/test_rust.py run --game)"]
// spec: XVAL-FILE-1, XVAL-FILE-2, XVAL-REG-1, XVAL-REG-2, XVAL-REG-4, XVAL-NAME-8, XVAL-NAME-9, XVAL-MODE-2, XVAL-SHDR-2, XVAL-SND-1, XVAL-SND-2, XVAL-CONS-1, XVAL-TERR-1, XVAL-GRP-1
fn v1_stage_rules() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let p = canary(&game);
    let id = "MAPEDIT_V1_RULES";
    let tmp = std::env::temp_dir().join(format!("bc2_v1_stage_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    let stage = tmp.join("stage");
    let t = std::time::Instant::now();
    export::build(&game, &p, "test", &spec_of(&p, id), &stage).unwrap();
    println!("built in {:.1} s", t.elapsed().as_secs_f64());
    let r = export::check(&game, &stage).unwrap();
    assert!(r.ok(), "{:#?}", r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>());
    assert_eq!(r.codes.len(), r.checks.len());
    for (code, _, what) in r.lines() {
        assert!(rules::rule(code).is_some(), "{code:?}: {what}");
    }
    let codes: BTreeSet<&str> = r.lines().map(|x| x.0).collect();
    for c in [
        "XVAL-REG-4",
        "XVAL-SND-2",
        "XVAL-TERR-1",
        "XVAL-CONS-1",
        "XVAL-GRP-1",
        "XVAL-SHDR-2",
    ] {
        assert!(codes.contains(c), "no {c} line");
    }
    assert!(
        r.findings().iter().any(|f| f.code == "XVAL-REG-4"),
        "the server and client note"
    );
    println!("canary: {} lines, {} rules, ok", r.checks.len(), codes.len());
    // the broken copy: one break per rule
    let broken = tmp.join("broken");
    copy_dir(&stage, &broken);
    let mut rec = export::read_record(&broken).unwrap();
    let low = id.to_ascii_lowercase();
    rebundle(&broken, &mut rec, "level-00.fbrb", |e| {
        e.push(StagedEntry {
            name: format!("levels/{low}/sound/x_waveDataSns_0"),
            type_name: "<non-resource>".into(),
            data: vec![0; 16],
        });
        e.push(StagedEntry {
            name: "Sound/Weapons/V1/v1_fire.dbx".into(),
            type_name: fbrb::DELETED.into(),
            data: Vec::new(),
        });
        let mut tex = vec![0, 0, 0, 3];
        tex.extend([0u8; 88]);
        e.push(StagedEntry {
            name: format!("levels/{low}/v1_console.res"),
            type_name: "ITexture".into(),
            data: tex,
        });
    });
    rebundle(&broken, &mut rec, "terrain-00.fbrb", |e| {
        let x = e.iter_mut().find(|x| !x.data.is_empty()).unwrap();
        x.data[0] ^= 0xFF;
    });
    // the record: a sha1, an entry count, a manifest line, a target, the menus' list, a name key, the donor, the sign
    let loader = rec.files.iter().position(|f| f.path == "loader-00.fbrb").unwrap();
    rec.files[loader].sha1 = "0".repeat(40);
    let ingame = rec.files.iter().position(|f| f.path == "ingame-00.fbrb").unwrap();
    rec.files[ingame].entries += 1;
    rec.bundle_manifest.remove(1);
    rec.mft_blocks[3].push("BundleTarget package:levels/v1_nowhere/level-00.fbrb".into());
    rec.level_list.reverse();
    rec.strings = vec![(format!("{id}_NAME"), "V1".into())];
    rec.donor = "sp_01".into();
    rec.shaders.push(DatabaseAdds {
        database: format!("levels/{low}/shaders/dx10_single.res"),
        missing: 1,
        added: vec![(
            "objects/objects/roadsigns/speedsign_40/speedsign_40_mesh".into(),
            "mp_012cq".into(),
            3,
        )],
        nowhere: Vec::new(),
    });
    std::fs::write(broken.join("stage.json"), serde_json::to_string_pretty(&rec).unwrap()).unwrap();
    let t = std::time::Instant::now();
    let rb = export::check(&game, &broken).unwrap();
    let got = failed(&rb);
    println!("broken copy checked in {:.1} s: {got:?}", t.elapsed().as_secs_f64());
    for c in [
        "XVAL-FILE-1",
        "XVAL-FILE-2",
        "XVAL-REG-1",
        "XVAL-REG-2",
        "XVAL-NAME-8",
        "XVAL-NAME-9",
        "XVAL-MODE-2",
        "XVAL-SHDR-2",
        "XVAL-SND-1",
        "XVAL-SND-2",
        "XVAL-CONS-1",
        "XVAL-TERR-1",
    ] {
        assert!(got.contains(c), "{c} did not fail: {got:?}");
    }
    assert!(!rb.ok());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
#[ignore = "game tier: export refusals on real levels; run_tests.py game runs it (tests/test_rust.py run --game)"]
// spec: XVAL-REG-3, XVAL-MODE-1, XVAL-EDIT-1, XVAL-EDIT-2, XVAL-EDIT-3, XVAL-EDIT-5, XVAL-CULL-4
fn v1_build_refusals() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        eprintln!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    let tmp = std::env::temp_dir().join(format!("bc2_v1_refused_{}", std::process::id()));
    let refused = |p: &MapProject, spec: &ExportSpec, code: &str| {
        let e = export::build(&game, p, "test", spec, &tmp.join("stage")).unwrap_err();
        assert!(e.0.starts_with(&format!("{code}: ")), "{code}: {}", e.0);
        println!("refused: {}", e.0.chars().take(160).collect::<String>());
    };
    let p = canary(&game);
    refused(&p, &spec_of(&p, "MP_005"), "XVAL-REG-3");
    // Onslaught without a project mode: the export file's
    let base = export::level_partition(&game, "mp_005gr").unwrap();
    let plain = MapProject::new("V1 plain", "mp_005gr", "levels/MP_005GR", &base, Profile::Bfbc2);
    let mut ons = spec_of(&plain, "MAPEDIT_V1_ONS");
    ons.mode = "onslaught".into();
    ons.layer.donor = "ons_mp_005gr".into();
    refused(&plain, &ons, "XVAL-MODE-1");
    // a paint stroke
    let mut paint = canary(&game);
    paint.push(
        Op::Stroke {
            target: bc2_levelwrite::files::Target::Paint { layer: "grass".into() },
            size: [64, 64],
            tiles: Vec::new(),
        },
        "Paint",
    );
    refused(&paint, &spec_of(&paint, "MAPEDIT_V1_PAINT"), "XVAL-EDIT-5");
    // an object of the base moved beyond the cull tree's range
    let w = bc2_levelwrite::LevelWriter::parse(&base).unwrap();
    let obj = w
        .guids()
        .into_iter()
        .find(|g| w.find(g).is_some_and(|e| w.type_of(e).ends_with("ReferenceObjectData")) && w.transform(g).is_some())
        .unwrap();
    let mut far = canary(&game);
    let mut rows = w.transform(&obj).unwrap();
    rows[3][0] = 3000.0;
    far.push(
        Op::Move {
            items: vec![bc2_levelwrite::ops::Moved {
                guid: obj.clone(),
                from: w.transform(&obj).unwrap(),
                to: rows,
                placement: None,
            }],
        },
        "Move",
    );
    refused(&far, &spec_of(&far, "MAPEDIT_V1_FAR"), "XVAL-CULL-4");
    // the base changed, an operation naming an object it lacks
    let mut gone = canary(&game);
    gone.base.sha1 = "0".repeat(40);
    gone.push(
        Op::Delete {
            guids: vec!["DDDDDDDD-0000-0000-0000-00000000F00D".into()],
        },
        "Delete",
    );
    refused(&gone, &spec_of(&gone, "MAPEDIT_V1_GONE"), "XVAL-EDIT-2");
    let f = bc2_levelwrite::validate::project(&game, &gone).unwrap();
    let codes: BTreeSet<&str> = f.iter().map(|x| x.code.as_str()).collect();
    for c in ["XVAL-EDIT-1", "XVAL-EDIT-2", "XVAL-EDIT-3"] {
        assert!(codes.contains(c), "{c}: {codes:?}");
    }
    let ok = bc2_levelwrite::validate::project(&game, &canary(&game)).unwrap();
    assert!(
        ok.iter().all(|x| x.severity != rules::Severity::Error),
        "{:#?}",
        ok.iter()
            .filter(|x| x.severity == rules::Severity::Error)
            .collect::<Vec<_>>()
    );
    println!("the canary project: {} findings, no error", ok.len());
    let _ = std::fs::remove_dir_all(&tmp);
}
