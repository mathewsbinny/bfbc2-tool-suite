//! Vegetation, rocks and props placed on a generated terrain (round TG4, spec/transforms/terrain_gen.md TGEN-VEG-2 /
//! -3): the placement falls on DICE's statistics of its biome (`BiomeStats::dice`, measured on DICE's multiplayer
//! terrains) when measured as DICE's levels are, and the same seed gives the same instances.
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::print_stdout,
    clippy::cast_precision_loss,
    clippy::doc_markdown,
    missing_docs
)]

use bc2_terragen::vegetation::{
    BiomeStats, Ground, Instance, Kind, Placed, Pose, TypeChoice, central, measure, place_biome,
};
use bc2_terragen::{Biome, Settings, generate};

/// Three types of each kind, upright, standing on the ground.
fn types() -> Vec<TypeChoice> {
    let up: [[f32; 3]; 4] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]];
    let flat = Ground {
        n: 4,
        cell: 1.0,
        x0: -2.0,
        z0: -2.0,
        heights: vec![0.0; 16],
        water: None,
    };
    let pose = Pose::of(&up, &flat, false);
    let mut out = Vec::new();
    for k in Kind::ALL {
        for t in 0..3 {
            out.push(TypeChoice {
                kind: k,
                ty: out.len(),
                weight: f64::from(t + 1),
                poses: vec![pose],
            });
        }
    }
    out
}

fn placed(biome: Biome, seed: u64) -> (Ground, Vec<Placed>) {
    let g = Ground::of_generated(&generate(&Settings::new(biome, 1024, 7)).unwrap());
    let (_, out) = place_biome(&g, biome, &types(), &central(g.side()), seed, 1.0);
    (g, out)
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs().max(1e-9)
}

fn log_ratio(a: f64, b: f64) -> f64 {
    (a.max(1e-6) / b.max(1e-6)).ln().abs()
}

// spec: TGEN-VEG-3
#[test]
fn a_placement_is_its_seeds() {
    let (_, a) = placed(Biome::Jungle, 3);
    let (_, b) = placed(Biome::Jungle, 3);
    let (_, c) = placed(Biome::Jungle, 4);
    assert_eq!(a, b);
    assert_ne!(a, c);
    // every type at least once (a group keeps one of each of its types)
    for t in types() {
        if BiomeStats::dice(Biome::Jungle).kinds[t.kind.index()].is_some() {
            assert!(a.iter().any(|p| p.ty == t.ty), "type {} of {:?}", t.ty, t.kind);
        }
    }
}

// spec: TGEN-VEG-2, TGEN-VEG-3
#[test]
fn every_biome_falls_on_dices_statistics() {
    for biome in Biome::ALL {
        let t0 = std::time::Instant::now();
        let (g, out) = placed(biome, 7);
        let secs = t0.elapsed().as_secs_f64();
        let inst: Vec<Instance> = out
            .iter()
            .map(|p| Instance {
                kind: p.kind,
                ty: p.ty,
                rows: p.rows,
            })
            .collect();
        let outside = central(g.side());
        let m = measure(&g, &inst, &outside);
        let dice = BiomeStats::dice(biome);
        println!("== {} ({} instances in {secs:.2} s)", biome.name(), out.len());
        for k in Kind::ALL {
            let (Some((_, d)), Some(s)) = (&dice.kinds[k.index()], &m.kinds[k.index()]) else {
                continue;
            };
            println!(
                "  {:12} inside {:.1} / {:.1} per ha, nn p50 {:.1} / {:.1} m, k3 {:.2} / {:.2}, k10 {:.2} / {:.2}, k30 {:.2} / {:.2}, \
                 slope p50 {:.1} / {:.1} deg, tilt p50 {:.1} / {:.1}",
                k.name(),
                s.band_density[0],
                d.band_density[0],
                s.nn[1],
                d.nn[1],
                s.k_ratio[0],
                d.k_ratio[0],
                s.k_ratio[1],
                d.k_ratio[1],
                s.k_ratio[2],
                d.k_ratio[2],
                s.slope[1],
                d.slope[1],
                s.tilt[0],
                d.tilt[0],
            );
            // the playable area's density is DICE's (within a tenth; a fifth for a kind of under 100 instances, whose
            // last clusters fall on the band's edge); a kind of enough instances clusters as DICE's
            let tol = if s.count >= 100.0 { 0.1 } else { 0.2 };
            // (temperate's rock: one instance on one of its four terrains, none inside a playable area)
            if d.count >= 10.0 {
                assert!(rel(s.band_density[0], d.band_density[0]) < tol, "{biome} {k:?} density");
            }
            if s.count >= 50.0 && d.count >= 50.0 {
                assert!(
                    log_ratio(s.nn[1], d.nn[1]) < 1.6f64.ln(),
                    "{biome} {k:?} nearest neighbour"
                );
                assert!(
                    log_ratio(s.k_ratio[1] - 1.0, d.k_ratio[1] - 1.0) < 3f64.ln(),
                    "{biome} {k:?} neighbour ratio at 10 m"
                );
                assert!((s.slope[1] - d.slope[1]).abs() < 12.0, "{biome} {k:?} slope");
            }
        }
    }
}
