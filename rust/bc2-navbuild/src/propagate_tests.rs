//! DICE's placement (round NKS-deep B) on synthetic ground: the probes' directions and step, the density limits, the
//! simplification's bounds and the over-connection pass.
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::float_cmp)]

use std::collections::HashSet;

use super::{Ground, PNode, Params, place};
use crate::{Cell, Floats, Level, LevelData, Rule};

/// A flat level of nx x nz columns at pitch 0.8, with the columns `holes` left out (no surface).
fn level(nx: usize, nz: usize, holes: &dyn Fn(usize, usize) -> bool) -> (Level, Vec<Cell>) {
    let mut start = vec![0];
    let mut cells = Vec::new();
    let mut n = 0;
    for j in 0..nz {
        for i in 0..nx {
            if !holes(i, j) {
                cells.push((i as i64, j as i64, n));
                n += 1;
            }
            start.push(n);
        }
    }
    // the columns run x fastest: start is per column in that order
    let lv = Level::new(LevelData {
        nx,
        nz,
        start,
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
    .unwrap();
    (lv, cells)
}

fn run(lv: &Level, cells: &[Cell], seed: (i64, i64)) -> (super::Placed, Params) {
    let blocked: HashSet<(usize, usize)> = HashSet::new();
    let areas = vec![cells.to_vec()];
    let g = Ground::of(lv, &areas, 0, 0, (0.0, 0.0), 0.8, 0.95, Rule::Slope, &blocked);
    let p = Params::at_pitch(0.8, 15.0);
    let k = cells.iter().find(|c| (c.0, c.1) == seed).unwrap().2;
    let s = PNode {
        k,
        cell: seed,
        pos: (seed.0 as f64 * 0.8, 0.95, seed.1 as f64 * 0.8),
        locked: true,
    };
    (place(&g, &p, &[s], &areas), p)
}

/// Union-find's root.
fn find(p: &mut [usize], mut x: usize) -> usize {
    while p[x] != x {
        x = p[x];
    }
    x
}

#[test]
fn parameters_are_dices() {
    let p = Params::at_pitch(0.8, 15.0);
    assert!((p.dist - 3.35).abs() < 1e-12 && p.sectors == 32);
    assert!((p.density_max - 2.1775).abs() < 1e-12 && (p.density_min - 0.35).abs() < 1e-12);
    assert!((p.connection - 4.02).abs() < 1e-12 && (p.back_off - 1.6).abs() < 1e-12);
    let d = Params::at_pitch(0.2, 15.0);
    assert!((d.dist - 0.85).abs() < 1e-12 && d.sectors == 128 && (d.back_off - 0.4).abs() < 1e-12);
}

#[test]
fn propagation_steps_dist_propagation_on_the_probe_directions() {
    // open ground: every propagated node one step from its nearest node, the first ring on the 32 directions
    let (lv, cells) = level(40, 40, &|_, _| false);
    let blocked: HashSet<(usize, usize)> = HashSet::new();
    let areas = vec![cells.clone()];
    let g = Ground::of(&lv, &areas, 0, 0, (0.0, 0.0), 0.8, 0.95, Rule::Slope, &blocked);
    let p = Params::at_pitch(0.8, 15.0);
    let k = cells.iter().find(|c| (c.0, c.1) == (20, 20)).unwrap().2;
    let seed = PNode {
        k,
        cell: (20, 20),
        pos: (16.0, 0.95, 16.0),
        locked: true,
    };
    let nodes = super::propagate(&g, &p, &[seed], &areas);
    assert!(nodes.len() > 20);
    let first: Vec<&PNode> = nodes
        .iter()
        .skip(1)
        .filter(|n| (super::dist2(n.pos, seed.pos) - 3.35).abs() < 1e-9)
        .collect();
    assert!(first.len() >= 6, "{} nodes on the first ring", first.len());
    for n in &first {
        let a = (n.pos.2 - seed.pos.2)
            .atan2(n.pos.0 - seed.pos.0)
            .to_degrees()
            .rem_euclid(11.25);
        assert!(a < 1e-6 || 11.25 - a < 1e-6, "off the 32 directions: {a}");
    }
    // no two nodes closer than DistDensityMax (all see each other on open ground)
    for (i, a) in nodes.iter().enumerate() {
        for b in &nodes[i + 1..] {
            assert!(super::dist3(a.pos, b.pos) > p.density_max);
        }
    }
}

#[test]
fn open_ground_simplifies_within_dist_edge_max() {
    let (lv, cells) = level(60, 60, &|_, _| false);
    let (placed, p) = run(&lv, &cells, (30, 30));
    let kept = placed.nodes.iter().flatten().count();
    // the seed stays where it is; the rest is sparse, every edge within DistEdgeMax, one connected graph
    assert_eq!(placed.nodes[0].map(|n| n.cell), Some((30, 30)));
    assert!(kept > 4 && kept < 100, "{kept} nodes");
    let mut parent: Vec<usize> = (0..placed.nodes.len()).collect();
    for &(a, b, d) in &placed.edges {
        assert!(placed.nodes[a].is_some() && placed.nodes[b].is_some());
        assert!(super::dist2(placed.nodes[a].unwrap().pos, placed.nodes[b].unwrap().pos) <= p.dist_edge_max + 1e-9);
        assert!(d > 0.0);
        let (ra, rb) = (find(&mut parent, a), find(&mut parent, b));
        parent[ra] = rb;
    }
    let roots: HashSet<usize> = (0..placed.nodes.len())
        .filter(|&n| placed.nodes[n].is_some())
        .map(|n| find(&mut parent, n))
        .collect();
    assert_eq!(roots.len(), 1, "one connected graph");
}

#[test]
fn over_connection_leaves_no_route_within_1_2() {
    let (lv, cells) = level(60, 60, &|_, _| false);
    let (placed, p) = run(&lv, &cells, (30, 30));
    let pos = |n: usize| placed.nodes[n].unwrap().pos;
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); placed.nodes.len()];
    for &(a, b, _) in &placed.edges {
        adj[a].push(b);
        adj[b].push(a);
    }
    // an edge with a route of at most 1.2 x through other edges would have been removed (removals only lengthen
    // the routes of the edges tested before them, so the last edge tested has none at all)
    let &(a, b, len) = placed.edges.last().unwrap();
    let mut best = vec![f64::INFINITY; placed.nodes.len()];
    best[a] = 0.0;
    let mut todo = vec![a];
    while let Some(u) = todo.pop() {
        for &v in &adj[u] {
            if (u, v) == (a, b) || (u, v) == (b, a) {
                continue;
            }
            let d = best[u] + super::dist3(pos(u), pos(v));
            if d < best[v] && d <= p.over_connection * len {
                best[v] = d;
                todo.push(v);
            }
        }
    }
    assert!(best[b] > p.over_connection * len);
}

#[test]
fn a_wall_is_passed_through_its_gap_only() {
    // a wall of missing columns along i = 30 with a gap at j 0..5
    let (lv, cells) = level(60, 40, &|i, j| i == 30 && j >= 5);
    let (placed, _) = run(&lv, &cells, (10, 20));
    let nodes: Vec<PNode> = placed.nodes.iter().flatten().copied().collect();
    assert!(
        nodes.iter().any(|n| n.cell.0 > 30),
        "the ground beyond the wall is reached through the gap"
    );
    for &(a, b, _) in &placed.edges {
        let (pa, pb) = (placed.nodes[a].unwrap(), placed.nodes[b].unwrap());
        if (pa.cell.0 < 30) != (pb.cell.0 < 30) && pa.cell.0 != 30 && pb.cell.0 != 30 {
            // an edge across the wall's line crosses it in the gap
            let t = (30.0 * 0.8 - pa.pos.0) / (pb.pos.0 - pa.pos.0);
            let z = pa.pos.2 + t * (pb.pos.2 - pa.pos.2);
            assert!(z < 5.0 * 0.8, "edge across the wall at z {z}");
        }
    }
}
