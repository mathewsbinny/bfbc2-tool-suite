//! Every DICE visual terrain of the game copy reads (round SPG2-rest, VEG-UG-2): the 30 `VisualTerrain` resources of
//! the DICE terrain bundles (`<game>\{Dist\win32,Package}\levels\*\terrain-00.fbrb` dated before 2013, each
//! resource once, the first bundle in path order that holds it, as `spec/probes/terrain.py` reads them), with their
//! patch shaders' undergrowth entries. Until SPG2-rest the reader took an entry's second string as one byte and
//! refused `NAM_MP_003`'s visual terrain, whose 30 entries name the mesh's asset there.
//!
//! No Direct3D. Skipped (with a note) without the game copy.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    clippy::panic,
    missing_docs
)]

use crate::common;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, UNIX_EPOCH};

use bc2_render::files::Files;
use bc2_render::terrain::parse_visual;

/// 2013-01-01: DICE's bundles are older, OPFOR's installs newer.
const DICE_BEFORE: u64 = 1_356_998_400;

fn dice_terrain_bundles(game: &std::path::Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for root in [
        game.join("Dist").join("win32").join("levels"),
        game.join("Package").join("levels"),
    ] {
        let Ok(rd) = std::fs::read_dir(&root) else { continue };
        for level in rd.flatten() {
            let p = level.path().join("terrain-00.fbrb");
            let dice = p
                .metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t < UNIX_EPOCH + Duration::from_secs(DICE_BEFORE));
            if dice {
                out.push(p);
            }
        }
    }
    out.sort_by_key(|p| p.to_string_lossy().to_lowercase());
    out
}

// spec: VEG-UG-1, VEG-UG-2, TERR-VT-3
#[test]
fn every_dice_visual_terrain_reads_with_its_undergrowth() {
    let _gate = crate::gate();
    let game = common::opfor().join("game");
    let bundles = dice_terrain_bundles(&game);
    if bundles.is_empty() {
        println!("skipped: no game copy at {}", game.display());
        return;
    }
    let mut seen = BTreeSet::new();
    let mut terrains = 0;
    let (mut shaders, mut entries, mut shader_named) = (0, 0, 0);
    let mut assets: BTreeMap<String, usize> = BTreeMap::new();
    let mut tails = BTreeSet::new();
    let mut pairs: BTreeMap<(bool, bool), usize> = BTreeMap::new();
    for path in &bundles {
        let files = Files::bundles(std::slice::from_ref(path)).expect("bundle header");
        let mut names: Vec<String> = files
            .names()
            .filter(|n| n.starts_with("terrains/") && n.ends_with(".visual.res"))
            .map(str::to_owned)
            .filter(|n| !seen.contains(n))
            .collect();
        names.sort();
        if names.is_empty() {
            continue;
        }
        for n in &names {
            assert!(files.read(n).is_none());
        }
        files.fetch(&names).expect("read");
        for n in names {
            let data = files.read(&n).expect("fetched");
            let vt = parse_visual(&data).unwrap_or_else(|e| panic!("{n}: {e}"));
            terrains += 1;
            for s in &vt.shaders {
                shaders += 1;
                let ug: Vec<_> = s.meshes.iter().flatten().collect();
                *pairs.entry((!s.undergrowth.is_empty(), !ug.is_empty())).or_default() += 1;
                for e in ug {
                    entries += 1;
                    tails.insert(e.tail);
                    shader_named += usize::from(!e.shader.is_empty());
                    if !e.asset.is_empty() {
                        assert!(e.asset.starts_with(&e.mesh), "{n}: {} / {}", e.mesh, e.asset);
                        *assets
                            .entry(n.split('/').nth(1).unwrap_or_default().to_owned())
                            .or_default() += 1;
                    }
                    assert!(e.values.iter().all(|v| v.is_finite()), "{n}: {:?}", e.values);
                }
            }
            seen.insert(n);
        }
    }
    println!(
        "{terrains} visual terrains, {shaders} patch shaders, {entries} undergrowth entries ({shader_named} with a shader), assets {assets:?}"
    );
    assert_eq!(terrains, 30);
    assert_eq!((shaders, entries, shader_named), (1515, 3069, 1091));
    assert_eq!(tails, BTreeSet::from([[1, 0]]));
    assert_eq!(assets, BTreeMap::from([("nam_mp_003".to_owned(), 30)]));
    assert_eq!(pairs, BTreeMap::from([((true, true), 1068), ((false, false), 447)]));
}
