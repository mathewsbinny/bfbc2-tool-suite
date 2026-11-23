//! The terrain codec (`bc2_formats::terrain_codec`, round E5) on DICE's terrains in Project OPFOR BFBC2's game copy
//! (run by `run_tests.py game`; ignored by a plain `cargo test`):
//!
//!   cargo test --release -p bc2-formats --test terrain_corpus -- --ignored --nocapture
//!
//! Every `TerrainHeightfield` and `TerrainMaterialMap` of the bundles dated before 2013, each resource once: a main
//! block decodes and encodes byte for byte and its min / max tree is the computed one (TERR-HF-9 - 11); a sub-terrain
//! block's quadtree is the computed one (TERR-HF-5); a material map decodes and encodes byte for byte (TERR-MM-1 - 4).
//! The game copy is `OPFOR_ROOT`'s `game`, else `project-opfor-bfbc2\game` beside the suite; the test passes with a
//! note when it is missing.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::print_stdout,
    clippy::doc_markdown,
    missing_docs
)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bc2_formats::terrain_codec::{MainBlock, MaterialMap, sub_quadtree};
use bc2_formats::{fbrb, terrain};

const DICE_BEFORE: u64 = 1_356_998_400;

fn game() -> Option<PathBuf> {
    let suite = Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
    let root = std::env::var_os("OPFOR_ROOT")
        .filter(|v| !v.is_empty())
        .map_or_else(|| suite.join("..").join("project-opfor-bfbc2"), PathBuf::from);
    let g = root.join("game");
    g.join("Dist").join("win32").join("levels").is_dir().then_some(g)
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten() {
        let p = e.path();
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk(&p, out);
        } else if p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("terrain-00.fbrb"))
            && p.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .is_some_and(|d| d.as_secs() < DICE_BEFORE)
        {
            out.push(p);
        }
    }
}

#[test]
#[ignore = "game tier: reads the game copy (run_tests.py game)"]
fn dice_terrains_round_trip() {
    let Some(g) = game() else {
        println!("no game copy: skipped");
        return;
    };
    let mut bundles = Vec::new();
    walk(&g, &mut bundles);
    bundles.sort();
    let mut seen = HashSet::new();
    let (mut mains, mut subs, mut maps) = (0, 0, 0);
    let mut bad = Vec::new();
    for b in &bundles {
        let h = fbrb::entries_path(b).unwrap();
        let want: Vec<String> = h
            .entries
            .iter()
            .filter(|e| !e.is_deleted() && matches!(e.type_name.as_str(), "TerrainHeightfield" | "TerrainMaterialMap"))
            .filter(|e| seen.insert(e.name.to_ascii_lowercase()))
            .map(|e| e.name.clone())
            .collect();
        if want.is_empty() {
            continue;
        }
        let datas = fbrb::read_path_uncached(b, &want).unwrap();
        for (name, d) in want.iter().zip(datas) {
            let ty = &h.entries.iter().find(|e| &e.name == name).unwrap().type_name;
            if ty == "TerrainMaterialMap" {
                maps += 1;
                match MaterialMap::decode(&d) {
                    Ok(m) if m.encode() == d => {}
                    Ok(_) => bad.push(format!("{name}: material map re-encodes to other bytes")),
                    Err(e) => bad.push(format!("{name}: {e}")),
                }
                continue;
            }
            let hf = terrain::parse(&d).unwrap();
            if hf.header.log_b == 0 {
                subs += 1;
                if let terrain::Tail::Quadtree(q) = &hf.tail
                    && *q != sub_quadtree(&hf)
                {
                    bad.push(format!("{name}: sub-terrain quadtree differs from the computed one"));
                }
                continue;
            }
            mains += 1;
            match MainBlock::decode(&d) {
                Ok(m) => {
                    if m.encode() != d {
                        bad.push(format!("{name}: main block re-encodes to other bytes"));
                    }
                    if m.tree != bc2_formats::terrain_codec::minmax_tree(&m.heights, m.size as usize) {
                        bad.push(format!("{name}: min / max tree differs from the computed one"));
                    }
                    println!("{name}: n {}, envelope under {:.4}", m.size, m.under_share());
                }
                Err(e) => bad.push(format!("{name}: {e}")),
            }
        }
    }
    println!("{mains} main blocks, {subs} sub-terrain blocks, {maps} material maps");
    assert!(bad.is_empty(), "{bad:#?}");
    assert_eq!((mains, maps), (30, 30));
}
