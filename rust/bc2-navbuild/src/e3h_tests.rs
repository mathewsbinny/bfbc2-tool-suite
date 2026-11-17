//! Tests of round E3h's pieces: the parallel components behind the floods and islands against their breadth-first
//! references, and the sector graph (`sector`); the sector graph's answers against the Python reference are checked
//! from Python (`bfbc2-auto-navkit/tests/test_navbuild_native.py`, "E3h").
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_cmp,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::HashSet;

use proptest::prelude::*;

use crate::route::Node;
use crate::sector::{self, GraphCfg, GraphJob};
use crate::{Cell, Floats, Level, LevelData, Rule};

/// A random multi-layer level from a seed: 0-3 surfaces per column on rough floors, float32 or float64 gradients,
/// random keep and stair flags.
fn random_level(seed: u64, nx: usize, nz: usize, f32_: bool) -> Level {
    let mut st = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    let mut next = move || {
        st ^= st << 13;
        st ^= st >> 7;
        st ^= st << 17;
        st
    };
    let (mut start, mut y, mut gx, mut gz, mut keep, mut stair) = (vec![0], vec![], vec![], vec![], vec![], vec![]);
    for _ in 0..nx * nz {
        let n = (next() % 4) as usize;
        let mut h: Vec<f64> = (0..n).map(|f| f as f64 * 2.6 + (next() % 9) as f64 * 0.07).collect();
        h.sort_by(f64::total_cmp);
        for v in h {
            y.push(v);
            gx.push(((next() % 7) as f64 - 3.0) * 0.05);
            gz.push(((next() % 7) as f64 - 3.0) * 0.05);
            keep.push(next() % 10 != 0);
            stair.push(next() % 25 == 0);
        }
        start.push(y.len());
    }
    let n = y.len();
    let floats = |v: Vec<f64>| {
        if f32_ {
            Floats::F32(v.iter().map(|&x| x as f32).collect())
        } else {
            Floats::F64(v)
        }
    };
    Level::new(LevelData {
        nx,
        nz,
        start,
        keep,
        y,
        gx: floats(gx),
        gz: floats(gz),
        stair,
        terrain: vec![true; n],
        pitch: 0.8,
        step: 0.28,
        link_step: 0.6,
    })
    .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(160))]
    /// The parallel islands and seeds' flood give the breadth-first answers (seeds walkable or not).
    #[test]
    fn components_as_bfs(seed in any::<u64>(), nx in 1usize..24, nz in 1usize..24, f32_ in any::<bool>(),
                         picks in proptest::collection::vec(any::<u32>(), 0..12)) {
        let l = random_level(seed, nx, nz, f32_);
        prop_assert_eq!(l.islands(), l.islands_bfs());
        if l.is_empty() {
            return Ok(());
        }
        let seeds: Vec<usize> = picks.iter().map(|&p| p as usize % l.len()).collect();
        let mut a = vec![false; l.len()];
        for &k in &seeds {
            a[k] = true;
        }
        let mut b = a.clone();
        let mut added_a = l.flood(&mut a, seeds.clone());
        let mut added_b = l.flood_bfs(&mut b, seeds.clone());
        prop_assert_eq!(&a, &b);
        added_a.sort_unstable();
        added_b.sort_unstable();
        prop_assert_eq!(added_a, added_b);
        // a second flood from one more surface (the ladders'): kept is no longer only the seeds - breadth first
        let more = (seed as usize) % l.len();
        a[more] = true;
        b[more] = true;
        l.flood(&mut a, [more]);
        l.flood_bfs(&mut b, [more]);
        prop_assert_eq!(a, b);
    }
}

#[test]
fn roots_are_the_lowest_surface() {
    let l = random_level(7, 16, 16, true);
    let roots = l.roots();
    let (ids, n) = l.islands_bfs();
    for (k, &r) in roots.iter().enumerate() {
        assert!(r <= k);
        if ids[k] >= 0 {
            assert_eq!(ids[r], ids[k]);
            assert_eq!(ids.iter().position(|&c| c == ids[k]), Some(r));
        } else {
            assert_eq!(r, k);
        }
    }
    assert!(n > 0);
}

fn flat_level(nx: usize, nz: usize) -> Level {
    let n = nx * nz;
    Level::new(LevelData {
        nx,
        nz,
        start: (0..=n).collect(),
        keep: vec![true; n],
        y: vec![0.0; n],
        gx: Floats::F64(vec![0.0; n]),
        gz: Floats::F64(vec![0.0; n]),
        stair: vec![false; n],
        terrain: vec![true; n],
        pitch: 0.8,
        step: 0.28,
        link_step: 0.6,
    })
    .unwrap()
}

fn cfg() -> GraphCfg {
    GraphCfg {
        pitch: 0.8,
        y_offset: 0.95,
        window: (-1, 12),
        dist_edge_max: 15.0,
        rule: Rule::Slope,
        cover: None,
        gabriel: false,
        order_nodes: true,
        order_node_min: 3.0,
        grid_min: (0.0, 0.0),
    }
}

#[test]
fn sector_graph_basics() {
    let l = flat_level(10, 10);
    let cells: Vec<Cell> = (0..100).map(|k| ((k % 10) as i64, (k / 10) as i64, k)).collect();
    let areas = vec![cells];
    // a centre node, a border node on its own surface, a ladder to a new node, an order node far from the rest
    let job = GraphJob {
        gi0: 0,
        gj0: 0,
        origin: (0.0, 0.0),
        mids: vec![vec![]],
        borders: vec![((0.0, 0.95, 0.0), 0)],
        orders: vec![(7.2, 0.0, 7.2), (7.2, 0.0, 0.8)], // the first: on the ladder node's surface (skipped)
        ladders: vec![(0, 99)],
        blocked: HashSet::new(),
    };
    let g = sector::build_graph(&l, &areas, &job, &cfg()).unwrap();
    let ks: Vec<usize> = g.nodes.iter().map(|n| n.node.k).collect();
    assert_eq!(g.nodes[0].node.k, 44); // the cell nearest the mean (4.5, 4.5): the first of the four
    assert_eq!(g.nodes[1].border, Some(0));
    assert!(ks.contains(&99));
    assert_eq!(
        g.nodes.iter().filter(|n| n.order).map(|n| n.node.k).collect::<Vec<_>>(),
        [19]
    );
    assert!(g.pairs.iter().any(|p| (p.0, p.1) == (1, 2) || (p.0, p.1) == (2, 1)));
    // every pair names nodes; no two nodes on one surface but a ladder end on a border's
    assert!(g.pairs.iter().all(|p| p.0 < g.nodes.len() && p.1 < g.nodes.len()));
    // a ladder end with no cell is refused (the reference raises KeyError)
    let bad = GraphJob {
        ladders: vec![(0, 5000)],
        ..job.clone()
    };
    assert!(sector::build_graph(&l, &areas, &bad, &cfg()).is_err());
    // positions round(v, 3) cannot take are refused
    let nan = GraphJob {
        borders: vec![((f64::NAN, 0.95, 0.0), 0)],
        ..job
    };
    assert!(sector::build_graph(&l, &areas, &nan, &cfg()).is_err());
    let _ = Node {
        k: 0,
        cell: (0, 0),
        pos: (0.0, 0.0, 0.0),
    };
}

#[test]
fn sector_graph_dedups_and_covers() {
    let l = flat_level(10, 10);
    let cells: Vec<Cell> = (0..100).map(|k| ((k % 10) as i64, (k / 10) as i64, k)).collect();
    // the border node stands where the centre node would: the centre is not added
    let job = GraphJob {
        origin: (0.0, 0.0),
        borders: vec![((3.2, 0.95, 3.2), 7)],
        mids: vec![vec![(0.1, 0.1), (0.1, 0.1)]],
        ..GraphJob::default()
    };
    let mut c = cfg();
    c.cover = Some(2.0);
    c.gabriel = true;
    let g = sector::build_graph(&l, &[cells], &job, &c).unwrap();
    assert_eq!(g.nodes.iter().filter(|n| n.node.k == 0).count(), 1); // two portal middles, one node
    assert!(
        !g.nodes
            .iter()
            .any(|n| n.border.is_none() && n.node.pos == (3.2, 0.95, 3.2))
    );
    assert!(g.nodes.len() > 10); // cover nodes every 2 m
    let mut seen = HashSet::new();
    assert!(g.nodes.iter().all(|n| seen.insert(n.node.k)));
}
