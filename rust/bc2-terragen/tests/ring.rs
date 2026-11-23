//! The sub-terrain ring (TGEN-RING-1): a generated terrain's twelve ring blocks read back as DICE's sub-terrain
//! blocks and are stitched to the main block as DICE stitches its own; the same checker on DICE's 12 single-ring
//! terrains (game tier: `cargo test --release -p bc2-terragen --test ring -- --ignored --nocapture`, ignored by a
//! plain `cargo test`; passes with a note without the game copy).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::doc_markdown,
    clippy::cast_precision_loss,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::type_complexity,
    missing_docs
)]

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use bc2_formats::terrain_codec::SubBlock;
use bc2_formats::{fbrb, terrain};
use bc2_terragen::ring::{self, PLACES, RING_N};
use bc2_terragen::{Biome, Settings, Terrain};

fn spread(v: &mut [f64]) -> (f64, f64, f64) {
    v.sort_by(f64::total_cmp);
    let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    (q(0.1), q(0.5), q(0.9))
}

fn border_median(h: &[f64], n: usize) -> f64 {
    let w = n / 16;
    let mut e: Vec<f64> = (0..n * n)
        .filter(|k| {
            let (i, j) = (k % n, k / n);
            i < w || j < w || i >= n - w || j >= n - w
        })
        .map(|k| h[k])
        .collect();
    spread(&mut e).1
}

// spec: TGEN-RING-1
#[test]
fn a_generated_ring_reads_back_stitched_as_dices() {
    for (biome, size) in [(Biome::Arid, 1024u32), (Biome::Snow, 2048)] {
        let t = Terrain::new(&Settings::new(biome, size, 7)).unwrap();
        let n = size as usize;
        assert_eq!(t.ring.len(), 12);
        let res = t.resources("TG_RING").unwrap();
        let half = f64::from(size) / 2.0;
        for (k, (b, &(px, pz))) in t.ring.iter().zip(PLACES.iter()).enumerate() {
            let r = res
                .iter()
                .find(|r| r.name == format!("Terrains/TG_RING/TG_RING.Heightfield-{:03}.res", k + 1))
                .unwrap();
            // TERR-HF-5: DICE's sub-terrain block - the suite's reader, the codec's decode (its quadtree computed)
            let hf = terrain::parse(&r.data).unwrap();
            assert_eq!(hf.size as usize, RING_N);
            assert!(matches!(hf.tail, terrain::Tail::Quadtree(_)));
            let sb = SubBlock::decode(&r.data).unwrap();
            assert_eq!(sb.encode(), r.data);
            assert_eq!(sb.heights, b.raw);
            assert_eq!(
                b.bounds,
                [
                    (f64::from(px) * half) as f32,
                    0.0,
                    (f64::from(pz) * half) as f32,
                    (f64::from(px + 1) * half) as f32,
                    t.height.range_m as f32,
                    (f64::from(pz + 1) * half) as f32,
                ]
            );
        }
        let blocks: Vec<Vec<u16>> = t.ring.iter().map(|b| b.raw.clone()).collect();
        let s = ring::stitching(&t.height.raw, n, &blocks);
        println!("{biome} {size}: {s:?}");
        assert_eq!(s.main_seams.0, s.main_seams.1);
        assert_eq!(s.ring_seams.0, s.ring_seams.1);
        assert!(s.main_seams.1 == 8 * RING_N && s.ring_seams.1 > 0);
        assert!(s.straight <= 0.5, "{s:?}");
        // the ring against DICE's single-ring terrains: its median within 80 m of the border's (DICE -77 .. +79),
        // its 10 - 90 % spread 25 - 432 m (DICE's range)
        let scale = t.height.range_m / 65535.0;
        let h: Vec<f64> = t.height.raw.iter().map(|v| f64::from(*v) * scale).collect();
        let mut rv: Vec<f64> = t
            .ring
            .iter()
            .flat_map(|b| b.raw.iter().map(|v| f64::from(*v) * scale))
            .collect();
        let (p10, p50, p90) = spread(&mut rv);
        let bm = border_median(&h, n);
        println!("{biome} {size}: ring p10 / p50 / p90 {p10:.1} / {p50:.1} / {p90:.1} m, border median {bm:.1} m");
        assert!(
            (p50 - bm).abs() < 80.0 && (25.0..=432.0).contains(&(p90 - p10)),
            "{p10} {p50} {p90} {bm}"
        );
    }
}

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

// spec: TGEN-RING-1, TERR-HF-12
#[test]
#[ignore = "game tier: reads the game copy (run_tests.py game)"]
fn dices_single_ring_terrains_are_stitched_so() {
    let Some(g) = game() else {
        println!("no game copy: skipped");
        return;
    };
    let mut bundles = Vec::new();
    walk(&g, &mut bundles);
    bundles.sort();
    // terrain -> block number -> (bounds, size, heights)
    let mut terrains: BTreeMap<String, BTreeMap<usize, (Vec<u16>, u32, [f32; 3])>> = BTreeMap::new();
    for b in &bundles {
        let h = fbrb::entries_path(b).unwrap();
        let want: Vec<String> = h
            .entries
            .iter()
            .filter(|e| !e.is_deleted() && e.type_name == "TerrainHeightfield")
            .map(|e| e.name.clone())
            .collect();
        if want.is_empty() {
            continue;
        }
        for (name, d) in want.iter().zip(fbrb::read_path_uncached(b, &want).unwrap()) {
            let t = name.split('/').nth(1).unwrap().to_ascii_lowercase();
            let k: usize = name.rsplit('-').next().unwrap()[..3].parse().unwrap();
            let hf = terrain::parse(&d).unwrap();
            terrains
                .entry(t)
                .or_default()
                .entry(k)
                .or_insert((hf.raw, hf.size, hf.min));
        }
    }
    let mut rings = 0;
    for (t, blocks) in &terrains {
        if blocks.len() != 13 {
            continue;
        }
        rings += 1;
        let (main, n, min) = &blocks[&0];
        // DICE's order of the 12 blocks is PLACES' (their bounds' corners, in half main sides)
        let half = f64::from(*n) / 2.0;
        for (k, &(px, pz)) in PLACES.iter().enumerate() {
            let b = &blocks[&(k + 1)];
            assert_eq!(b.1 as usize, RING_N, "{t}");
            assert_eq!(
                (f64::from(b.2[0]), f64::from(b.2[2])),
                (f64::from(px) * half, f64::from(pz) * half),
                "{t} block {}",
                k + 1
            );
            assert!((f64::from(min[0]) + half).abs() < 1e-6);
        }
        let ring: Vec<Vec<u16>> = (1..=12).map(|k| blocks[&k].0.clone()).collect();
        let s = ring::stitching(main, *n as usize, &ring);
        println!("{t}: {s:?}");
        assert_eq!(s.main_seams.0, s.main_seams.1, "{t}");
        assert_eq!(s.ring_seams.0, s.ring_seams.1, "{t}");
        assert!(s.straight <= 1.0, "{t}: {s:?}");
    }
    assert_eq!(rings, 12);
}
