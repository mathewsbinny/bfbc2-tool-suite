//! Round XF1 (audit AU1 G-N1, G-N2): an object placed from another level gets its shader entries and its own cull
//! shape in the export, as DICE's levels register the meshes they draw.
//!
//! - `stock_levels_carry_their_meshes_entries`: the ground truth. A level's own shader databases hold the entries of
//!   the meshes it reaches, the commons' included (no `_common` bundle has a database).
//! - `a_cross_set_object_gets_its_entries_and_shape`: Operation Aurora (`SP_01`) as a WW2 Conquest level with
//!   Guadalcanal's radar bunker placed from a template of another blueprint: the bunker's entries and mesh-part names
//!   in each of the level's databases are Guadalcanal's, its programs the same by content (the merge's own
//!   verification and the database checks); its cull leaves are the bunker's shape in Guadalcanal (two leaves), not
//!   the template's (one); `mapexport check` passes every line, the new shader and cull lines among them.
//!
//! Reads OPFOR's game copy (beside the suite, or `OPFOR_GAME`); writes only into a temporary folder (or
//! `BC2_XF1_STAGE`, kept for the install round trip); skipped without a game copy.
#![allow(
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::indexing_slicing,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::similar_names,
    missing_docs
)]

use std::collections::BTreeSet;
use std::path::PathBuf;

use bc2_levelwrite::export::{self, ExportSpec, Game};
use bc2_levelwrite::mpconvert::LayerSpec;
use bc2_levelwrite::ops::Placed;
use bc2_levelwrite::{MapProject, Op, Profile, shaders};

fn game_dir() -> Option<PathBuf> {
    if let Some(g) = std::env::var_os("OPFOR_GAME") {
        return Some(PathBuf::from(g));
    }
    let here = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    here.ancestors()
        .map(|a| a.join("project-opfor-bfbc2").join("game"))
        .find(|g| g.join("Dist").join("win32").join("package.mft").is_file())
}

const BUNKER: &str = "objects/architecture/beach/radarbunker_t01/radarbunker_t01";
const BUNKER_MESH: &str = "objects/architecture/beach/radarbunker_t01/radarbunker_t01_mesh";

/// A database's entry and mesh-part names of a mesh (lower case).
fn names_of(data: &[u8], file: &str, mesh: &str) -> (BTreeSet<String>, BTreeSet<String>) {
    let dx = bc2_formats::shaderdb::open::dx_in_name(file).unwrap();
    let o = bc2_formats::shaderdb::open::open(data, Some(if dx == 11 { 10 } else { dx }), Some(false), file).unwrap();
    let low = |n: &[u8]| String::from_utf8_lossy(&bc2_formats::shaderdb::query::lower(n)).into_owned();
    let entries =
        o.db.entries
            .iter()
            .map(|e| low(&e.name))
            .filter(|n| n.starts_with(&format!("{mesh}/")))
            .collect();
    let parts =
        o.db.parts
            .iter()
            .filter(|p| bc2_formats::shaderdb::query::part_mesh(&p.name).as_deref() == Some(mesh.as_bytes()))
            .map(|p| low(&p.name))
            .collect();
    (entries, parts)
}

fn database(game: &Game, lv: &str, file: &str) -> Vec<u8> {
    let live = game.live(&game.level_bundles(lv, "level")).unwrap();
    let f = &live[&format!("levels/{lv}/shaders/{file}")];
    export::read_found(&[f]).unwrap().pop().unwrap()
}

#[test]
#[ignore = "game tier: whole exports of stock levels; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn stock_levels_carry_their_meshes_entries() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    for lv in ["mp_005", "sp_01"] {
        let reached = shaders::stock_reach(&game, lv).unwrap();
        let lacking = shaders::stock_uncovered(&game, lv, "dx11_single.res", &reached).unwrap();
        println!(
            "{lv}: {} meshes reached, {} without an entry in its own database",
            reached.len(),
            lacking.len()
        );
        assert!(reached.len() > 300);
        // most meshes the level reaches, the commons' with them, have their entries in the level's own file (the
        // rest: variants and pieces it reaches but never draws, and OPFOR's own additions to its Package copy; by
        // bundle, round XF1's probe: MP_005 384 of mp_common's 431, SP_01 157 of sp_common's 163)
        assert!(
            lacking.len() * 6 < reached.len(),
            "{lv}: {} of {}",
            lacking.len(),
            reached.len()
        );
    }
    // no shared bundle holds a shader database
    for b in [
        "game:levels/mp_common/level-00.fbrb",
        "game:levels/sp_common/level-00.fbrb",
    ] {
        let live = game.live(&[game.bundle(b)]).unwrap();
        assert!(
            !live
                .values()
                .any(|f| f.1.type_name.to_ascii_lowercase().ends_with("shaderdatabase")),
            "{b}"
        );
    }
}

#[test]
#[ignore = "game tier: a whole cross-set export; run_tests.py game runs it (tests/test_rust.py run --game)"]
fn a_cross_set_object_gets_its_entries_and_shape() {
    let _gate = crate::gate();
    let Some(dir) = game_dir() else {
        println!("skipped: no game copy");
        return;
    };
    let game = Game::open(&dir).unwrap();
    if game.level_bundles("guadal_canal", "level").is_empty() {
        println!("skipped: OPFOR's Guadalcanal is not installed");
        return;
    }
    let base = export::level_partition(&game, "sp_01").unwrap();
    let mut p = MapProject::new("XF1 cross-set", "sp_01", "levels/SP_01", &base, Profile::Bfbc2);
    let c = export::choose_mode(&game, &p, Profile::Ww2, "conquest", None).unwrap();
    p.push(Op::SetMode { to: Box::new(c) }, "target");
    let gc = game.level_bundles("guadal_canal", "level");
    let bunker = export::blueprint_ref(&game, &gc, BUNKER).unwrap();
    let (cur, _) = export::current(&game, &p).unwrap();
    // SP_01 places no bunker: the template is another blueprint's placement
    let template = cur.template_for(&bunker).unwrap();
    let template_bp = {
        let e = cur.find(&template).unwrap();
        bc2_levelwrite::culltree::blueprint_of(&cur, e).unwrap()
    };
    assert_ne!(template_bp, BUNKER);
    let n = p.next_n();
    let guid = bc2_levelwrite::new_guid(&p.id, n, &bunker);
    p.push(
        Op::Place {
            items: vec![Placed {
                guid: guid.clone(),
                template,
                blueprint: bunker.clone(),
                ref_field: None,
                owned: Vec::new(),
                transform: bc2_levelwrite::rows_at([-640.0, 6.0, 330.0]),
                fields: Vec::new(),
                placement: None,
            }],
        },
        "XF1: Guadalcanal's radar bunker",
    );
    let spec = ExportSpec {
        format: export::FORMAT.into(),
        version: export::VERSION,
        level: "MAPEDIT_XF1_AURORA_BUNKER".into(),
        folder: String::new(),
        title: "XF1 Aurora bunker".into(),
        mode: "Conquest".into(),
        layer: LayerSpec {
            donor: p.mode().unwrap().donor.clone(),
            bases: Vec::new(),
            flags: Vec::new(),
            combat_area: String::new(),
        },
        profile: Profile::Ww2,
        identity: None,
        path_data: false,
    };
    let keep = std::env::var_os("BC2_XF1_STAGE").map(PathBuf::from);
    let tmp = std::env::temp_dir().join(format!("bc2_levelwrite_xf1_{}", std::process::id()));
    let stage = keep.clone().unwrap_or_else(|| tmp.join("stage"));
    let rec = export::build(&game, &p, "test", &spec, &stage).unwrap();
    let r = export::check(&game, &stage).unwrap();
    for (ok, l) in &r.checks {
        if l.starts_with("shaders") || l.starts_with("cull") {
            println!("{} {l}", if *ok { "ok    " } else { "FAILED" });
        }
    }
    assert!(r.ok(), "{:#?}", r.checks.iter().filter(|c| !c.0).collect::<Vec<_>>());
    // its own cull shape: Guadalcanal's bunker's, two leaves, not the template's
    let slotted = rec.cull.iter().find(|s| s.guid == guid).unwrap();
    assert_eq!(slotted.blueprint, BUNKER);
    assert!(slotted.source.starts_with("guadal_canal "), "{}", slotted.source);
    assert!(
        rec.report
            .iter()
            .any(|l| l.contains(&guid) && l.contains("(2 leaves, ")),
        "{:#?}",
        rec.report.iter().filter(|l| l.contains("slot")).collect::<Vec<_>>()
    );
    // its shader entries: in each of the level's databases, Guadalcanal's names for the bunker's mesh
    let (h, data) = bc2_formats::fbrb::extract_path_uncached(stage.join("level-00.fbrb")).unwrap();
    let mut seen = 0;
    for (e, d) in h.entries.iter().zip(&data) {
        let Some(file) = shaders::database_file(&e.name, &e.type_name) else {
            continue;
        };
        let ours = names_of(d, &file, BUNKER_MESH);
        let dice = names_of(&database(&game, "guadal_canal", &file), &file, BUNKER_MESH);
        let sp01 = names_of(&database(&game, "sp_01", &file), &file, BUNKER_MESH);
        println!(
            "{}: {} entries, {} mesh-part names for the bunker (Guadalcanal's: {}, {}; SP_01's: {}, {})",
            e.name,
            ours.0.len(),
            ours.1.len(),
            dice.0.len(),
            dice.1.len(),
            sp01.0.len(),
            sp01.1.len()
        );
        assert!(sp01.0.is_empty(), "SP_01 never drew the bunker");
        assert!(!dice.0.is_empty());
        assert_eq!(ours, dice, "the bunker registered as Guadalcanal registers it");
        let added = rec.shaders.iter().find(|s| s.database == e.name).unwrap();
        assert!(added.added.iter().any(|a| a.0 == BUNKER_MESH && a.1 == "guadal_canal"));
        seen += 1;
    }
    assert_eq!(seen, 3, "Dx9, Dx10 and Dx11");
    if keep.is_none() {
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
