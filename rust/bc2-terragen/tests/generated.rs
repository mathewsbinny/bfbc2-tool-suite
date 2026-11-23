//! A generated terrain end to end (spec/transforms/terrain_gen.md): its resources read back in the suite's readers
//! (TGEN-OUT-1 - 3), the same seed giving the same bytes on any number of threads (TGEN-GEN-1), objective pads flat
//! (TGEN-PLAY-3), and its shape against the biome's DICE medians (TGEN-GEN-4, TGEN-SHAPE-1 / -2 / -4).
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    missing_docs
)]

use bc2_formats::terrain::{self, Tail};
use bc2_formats::terrain_codec::{MainBlock, MaterialMap};
use bc2_formats::water;
use bc2_terragen::{Biome, Pad, Settings, Terrain, paint};

fn pool(threads: usize) -> rayon::ThreadPool {
    rayon::ThreadPoolBuilder::new().num_threads(threads).build().unwrap()
}

fn settings() -> Settings {
    let mut s = Settings::new(Biome::Arid, 1024, 7);
    s.pads = vec![
        Pad {
            x: 0.0,
            z: 0.0,
            radius: 24.0,
        },
        Pad {
            x: -170.0,
            z: 0.0,
            radius: 24.0,
        },
        Pad {
            x: 170.0,
            z: 0.0,
            radius: 24.0,
        },
    ];
    s
}

// spec: TGEN-OUT-1, TGEN-OUT-2, TGEN-OUT-3, TGEN-GEN-1, TGEN-PLAY-3, TGEN-MAT-2
#[test]
fn a_generated_terrain_reads_back_and_is_its_seeds() {
    let s = settings();
    let t = pool(4).install(|| Terrain::new(&s)).unwrap();
    let res = t.resources("TG_TEST").unwrap();
    let names: Vec<&str> = res.iter().map(|r| r.name.as_str()).collect();
    assert_eq!(
        names,
        [
            "Terrains/TG_TEST/TG_TEST.Heightfield-000.res",
            "Terrains/TG_TEST/TG_TEST-MaterialMap.res",
            "Terrains/TG_TEST/TG_TEST.Water.Mesh.res",
            "Terrains/TG_TEST/TG_TEST.res",
        ]
    );
    // TGEN-OUT-1: the suite's reader, every height within one step; the tail a main block's; the codec's decode
    let hf = terrain::parse(&res[0].data).unwrap();
    assert_eq!(
        (hf.size, hf.min, hf.max),
        (1024, [-512.0, 0.0, -512.0], [512.0, t.height.range_m as f32, 512.0])
    );
    assert_eq!(hf.raw, t.height.raw);
    assert!(matches!(hf.tail, Tail::Sections { .. }));
    assert_eq!((hf.header.log_a, hf.header.log_b, hf.header.w26), (7, 8, 4));
    let block = MainBlock::decode(&res[0].data).unwrap();
    assert_eq!(block.encode(), res[0].data);
    assert!(block.depth.iter().all(|d| *d == 255));
    assert!((block.under_share() - 1.0).abs() < f64::EPSILON);
    // TGEN-REQ-2: DICE's 1,024 block size (the heights, the tail of a depth map of 255)
    assert_eq!(
        res[0].data.len(),
        0x31 + 2 * 1024 * 1024 + 4 * 1024 + 4 * 1024 + 2 * (10_922 + 22_363)
    );
    // TGEN-OUT-2: the material map decodes to the painted cells and names, re-encodes byte for byte
    let mm = MaterialMap::decode(&res[1].data).unwrap();
    assert_eq!((mm.width, mm.names.as_slice()), (2048, t.names.as_slice()));
    assert_eq!(mm.cells, t.cells);
    assert_eq!(mm.encode(), res[1].data);
    let used: std::collections::BTreeSet<u8> = t.cells.iter().copied().collect();
    assert!(
        [paint::GROUND, paint::STEEP, paint::UNDER_WATER, paint::SLOPE]
            .iter()
            .all(|i| used.contains(i)),
        "{used:?}"
    );
    // TGEN-OUT-3: the water mesh reads
    let w = water::parse_mesh(&res[2].data).unwrap();
    assert_eq!((w.vertices.len(), w.triangles.len()), (4, 2));
    assert_eq!(res[3].data, Vec::<u8>::new());
    // TGEN-GEN-1: the same seed, the same bytes, on 1 thread and on 4; another seed, others
    let again = pool(1)
        .install(|| Terrain::new(&s))
        .unwrap()
        .resources("TG_TEST")
        .unwrap();
    assert!(again.iter().zip(&res).all(|(a, b)| a.data == b.data));
    let mut other = s.clone();
    other.seed += 1;
    let other = Terrain::new(&other).unwrap();
    assert_ne!(other.height.raw, t.height.raw);
    // TGEN-PLAY-3: each pad's middle (half its radius) varies under 0.5 m
    let h = t.height.heights();
    for p in &s.pads {
        let (ci, cj) = ((p.x + 512.0) as usize, (p.z + 512.0) as usize);
        let r = (p.radius / 2.0) as usize;
        let mut lo = f32::MAX;
        let mut hi = f32::MIN;
        for j in cj - r..cj + r {
            for i in ci - r..ci + r {
                lo = lo.min(h[j * 1024 + i]);
                hi = hi.max(h[j * 1024 + i]);
            }
        }
        assert!(hi - lo < 0.5, "pad at ({}, {}): {lo} - {hi}", p.x, p.z);
    }
}

// spec: TGEN-GEN-4, TGEN-SHAPE-2
#[test]
fn every_biome_lands_on_its_dice_medians() {
    for b in Biome::ALL {
        let t = Terrain::new(&Settings::new(b, 1024, 7)).unwrap();
        let s = t.shape();
        let g = b.targets();
        let what = format!("{b}: {s:?} against {g:?}");
        // the median slope, the 90th and 99th percentiles (DICE's steep tail), the hypsometric integral, the
        // spectrum, the border and the water: each near the biome's DICE median
        assert!((s.slope[1] - g.slope[1]).abs() < 1.0, "{what}");
        assert!((s.slope[2] - g.slope[2]).abs() < 6.0, "{what}");
        assert!((s.slope[3] - g.slope[3]).abs() < 5.0, "{what}");
        assert!(s.slope[0] < g.slope[0] + 4.0, "{what}");
        assert!((s.hi - g.hi).abs() < 0.03, "{what}");
        assert!((s.beta - g.beta).abs() < 0.3, "{what}");
        assert!((s.edge_minus_centre - g.edge_minus_centre).abs() < 3.0, "{what}");
        assert!((s.wet - g.wet).abs() < 0.01, "{what}");
        assert!(
            t.height.range_m >= g.range_m && t.height.range_m % 50.0 == 0.0,
            "{what}"
        );
    }
}
