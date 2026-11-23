//! Unit and property tests of round E3c's pieces (`trace`, `graph`, `route`, `window`); their answers against the
//! Python reference are checked from Python (`bfbc2-auto-navkit/tests/test_navbuild_native.py`, "E3c").
#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::float_cmp,
    clippy::similar_names,
    clippy::assert_is_empty
)]

use proptest::prelude::*;

use crate::graph::{self, RoofCfg, RoofGrid};
use crate::route::{self, Node, RouteCfg};
use crate::trace::{self, EdgeKey, Kind, Ordered, TraceCfg};
use crate::window::WindowSearch;
use crate::{Cell, CellMap, FastMap, FastSet, Floats, Level, LevelData, Rule};

/// `nx` x `nz` columns, one kept terrain surface each at height `h(i, j)` (None: an empty column).
fn level(nx: usize, nz: usize, h: impl Fn(usize, usize) -> Option<f64>) -> Level {
    let mut y = Vec::new();
    let mut start = vec![0];
    for j in 0..nz {
        for i in 0..nx {
            if let Some(v) = h(i, j) {
                y.push(v);
            }
            start.push(y.len());
        }
    }
    let n = y.len();
    Level::new(LevelData {
        nx,
        nz,
        start,
        keep: vec![true; n],
        y,
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

fn tcfg(window: (i64, i64)) -> TraceCfg {
    TraceCfg {
        gi0: 0,
        gj0: 0,
        window,
        y_offset: 0.95,
        rule: Rule::Slope,
    }
}

#[test]
fn ordered_maps_keep_insertion_order() {
    let mut m: Ordered<i32, &str> = Ordered::default();
    assert!(m.is_empty());
    m.insert(3, "a");
    m.insert(1, "b");
    m.insert(3, "c");
    *m.entry(2, || "d") = "e";
    assert_eq!(
        m.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>(),
        vec![(3, "c"), (1, "b"), (2, "e")]
    );
    assert_eq!((m.len(), m.get(&1), m.get(&9)), (3, Some(&"b"), None));
    assert!(Ordered::from_pairs([(1, 1), (1, 2)]).is_none());
    let k = EdgeKey {
        kind: Kind::Outline,
        x_axis: true,
        line2: 7,
        dir: 1,
    };
    assert_eq!(k.line(), 3.5);
}

#[test]
fn two_areas_trace_portals_both_ways() {
    // a 4 x 2 strip split into two areas at x = 2: a portal each way on the line x = 1.5, outline around
    let l = level(4, 2, |_, _| Some(0.0));
    let left: Vec<Cell> = vec![(0, 0, 0), (1, 0, 1), (0, 1, 4), (1, 1, 5)];
    let right: Vec<Cell> = vec![(2, 0, 2), (3, 0, 3), (2, 1, 6), (3, 1, 7)];
    let areas = vec![left, right];
    let aof = trace::area_of(&areas);
    let (per_area, one_way) = trace::trace_edges(&l, &areas, &aof, tcfg((0, 4)), true).unwrap();
    assert!(one_way.is_empty());
    let portal = |a: usize| {
        per_area[a]
            .iter()
            .filter(|(k, _)| matches!(k.kind, Kind::Portal(_)))
            .map(|(k, r)| (k.kind, k.line(), r.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(portal(0), vec![(Kind::Portal(1), 1.5, 2)]);
    assert_eq!(portal(1), vec![(Kind::Portal(0), 1.5, 2)]);
    let em = trace::trace_emit(&l, &areas, per_area, tcfg((0, 4)), 100.0, 200.0, 0.8);
    assert_eq!(em[0].portals.len(), 1);
    let (p0, p1, t) = em[0].portals[0];
    assert_eq!(
        (t, p0.0, p0.2, p1.2),
        (1, 100.0 + 1.5 * 0.8, 200.0 - 0.4, 200.0 + 1.5 * 0.8)
    );
    // the outline closes: 3 runs (bottom, left, top) on each area
    assert_eq!(em[0].outline.len(), 3);
    assert!(em[0].borders.is_empty());
    // a window ending at x = 3: the right area's cells at x = 3 see a border across x = 3.5? no - x = 4 is outside
    // the grid: no neighbour, outline. Cut the window at 3: x = 2's side to x = 3 is a border portal (C)
    let (pa, _) = trace::trace_edges(&l, &areas[..1], &trace::area_of(&areas[..1]), tcfg((0, 2)), true).unwrap();
    assert!(pa[0].iter().any(|(k, _)| k.kind == Kind::Border(1, 0)));
}

#[test]
fn diagonal_cells_make_a_bridge() {
    // two cells touching only at a corner, in one area: two 45-degree segments instead of the corner sides
    let l = level(3, 3, |i, j| ((i, j) == (0, 0) || (i, j) == (1, 1)).then_some(0.0));
    let areas = vec![vec![(0, 0, 0), (1, 1, 1)]];
    let (per_area, _) = trace::trace_edges(&l, &areas, &trace::area_of(&areas), tcfg((-1, 4)), true).unwrap();
    let em = trace::trace_emit(&l, &areas, per_area, tcfg((-1, 4)), 0.0, 0.0, 1.0);
    let diag = em[0].outline.iter().filter(|(a, b)| a.0 != b.0 && a.2 != b.2).count();
    assert_eq!(diag, 2);
    // 8 unit sides, 4 of them under the bridge: 6 straight runs left (2 per cell side pair merged where straight)
    assert!(em[0].outline.len() >= 6);
}

#[test]
fn stacked_layers_give_one_way_candidates() {
    // column 0 holds two surfaces 0.2 m apart (two areas); column 1 one surface joined to both: area 2 sees area 1's
    // side as its interior, so area 0's portal into it has no piece back
    let mut d = LevelData {
        nx: 2,
        nz: 1,
        start: vec![0, 2, 3],
        keep: vec![true; 3],
        y: vec![0.0, 0.2, 0.1],
        gx: Floats::F64(vec![0.0; 3]),
        gz: Floats::F64(vec![0.0; 3]),
        stair: vec![false; 3],
        terrain: vec![true; 3],
        pitch: 0.8,
        step: 0.28,
        link_step: 0.6,
    };
    d.keep[0] = true;
    let l = Level::new(d).unwrap();
    let areas: Vec<Vec<Cell>> = vec![vec![(0, 0, 0)], vec![(0, 0, 1), (1, 0, 2)]];
    let (_, one_way) = trace::trace_edges(&l, &areas, &trace::area_of(&areas), tcfg((-1, 3)), true).unwrap();
    assert_eq!(one_way.len(), 1);
    assert_eq!((one_way[0].src, one_way[0].target), (0, 1));
    // an area_of value naming no area is refused
    let mut bad: FastMap<usize, usize> = trace::area_of(&areas);
    bad.insert(2, 9);
    assert!(trace::trace_edges(&l, &areas, &bad, tcfg((-1, 3)), true).is_err());
}

#[test]
fn roofs_without_ground_nearby_drop() {
    // ground (island 0, 4 surfaces) and a raised deck (island 1) far away: dropped; anchored: kept
    let ys = [0.0, 0.0, 0.0, 0.0, 5.0];
    let start: Vec<usize> = (0..=5).collect();
    let g = RoofGrid {
        nx: 5,
        nz: 1,
        start: &start,
        y: &ys,
        min_x: 0.0,
        min_z: 0.0,
        pitch: 0.8,
    };
    let keep = [true; 5];
    let terrain = [true, true, true, true, false];
    let comp = [0, 0, 0, 0, 1];
    let cfg = RoofCfg {
        raise: 2.0,
        share: 0.5,
        near: 2.0,
        near_dy: 2.5,
    };
    // the deck's column holds only the deck: raised against its own lowest? no - 5.0 - 5.0 = 0: not raised
    let r = graph::roof_drop(&g, &keep, &terrain, &comp, 2, &[], cfg).unwrap();
    assert_eq!(r.stats, Some((2, None)));
    // a deck over ground in one column: raised
    let ys2 = [0.0, 0.0, 0.0, 0.0, 0.0, 5.0];
    let start2 = vec![0, 1, 2, 3, 4, 6];
    let g2 = RoofGrid {
        start: &start2,
        y: &ys2,
        ..g
    };
    let keep2 = [true, true, true, true, false, true];
    let terrain2 = [true, true, true, true, true, false];
    let comp2 = [0, 0, 0, 0, -1, 1];
    let r = graph::roof_drop(&g2, &keep2, &terrain2, &comp2, 2, &[], cfg).unwrap();
    assert_eq!(r.out, vec![false, false, false, false, false, true]);
    assert_eq!(r.stats, Some((2, Some((1, 1)))));
    let r = graph::roof_drop(&g2, &keep2, &terrain2, &comp2, 2, &[5], cfg).unwrap();
    assert!(r.out.iter().all(|&o| !o));
    // too few islands, arrays that do not fit
    assert_eq!(graph::roof_drop(&g2, &keep2, &terrain2, &comp2, 1, &[], cfg), None);
    assert_eq!(graph::roof_drop(&g2, &keep2[..5], &terrain2, &comp2, 2, &[], cfg), None);
    assert_eq!(
        graph::roof_drop(&g2, &keep2, &terrain2, &[0, 0, 0, 0, -1, -1], 1, &[], cfg)
            .unwrap()
            .stats,
        None
    );
}

#[test]
fn cover_nodes_fill_far_cells() {
    // a 10 m strip of cells 1 m apart, no node: the farthest-first fill places nodes until all within 2 m
    let cells: Vec<(f64, f64, f64)> = (0..11).map(|i| (f64::from(i), 0.0, 0.0)).collect();
    let mut nodes = Vec::new();
    let mut placed = Vec::new();
    let mut add = |_a: usize, w: usize| {
        placed.push(w);
        vec![cells[w]]
    };
    graph::cover_nodes(std::slice::from_ref(&cells), &mut nodes, 2.0, &mut add);
    assert_eq!(placed.first(), Some(&0));
    assert_eq!(nodes.len(), placed.len());
    for c in &cells {
        assert!(nodes.iter().any(|n| (n.0 - c.0).abs() <= 2.0));
    }
    // a refused placement (the surface holds a node): marked, the fill goes on
    let mut nodes = vec![(5.0, 0.0, 0.0)];
    let mut calls = 0;
    graph::cover_nodes(std::slice::from_ref(&cells), &mut nodes, 2.0, &mut |_a, _w| {
        calls += 1;
        Vec::new()
    });
    assert!(calls > 0 && nodes.len() == 1);
    // areas under 4 cells are left alone
    graph::cover_nodes(&[cells[..3].to_vec()], &mut nodes, 0.1, &mut |_a, _w| panic!("no call"));
}

#[test]
fn gabriel_keeps_joins() {
    // a, b, c on a line: (a, c) has b inside its circle - dropped, (a, b), (b, c) kept
    let pos = [(0.0, 0.0, 0.0), (1.0, 0.0, 0.0), (2.0, 0.0, 0.0), (50.0, 0.0, 0.0)];
    let pairs = [(0, 1, 1.0), (1, 2, 1.0), (0, 2, 2.0)];
    assert_eq!(graph::gabriel_prune(&pos, &pairs), Some(vec![0, 1]));
    // b on another floor: (a, c) is Gabriel
    let pos2 = [(0.0, 0.0, 0.0), (1.0, 3.0, 0.0), (2.0, 0.0, 0.0)];
    assert_eq!(graph::gabriel_prune(&pos2, &[(0, 2, 2.0)]), Some(vec![0]));
    // a dropped pair that alone joins two groups comes back, shortest first
    let pairs3 = [(0, 2, 2.0), (2, 3, 48.0)];
    assert_eq!(graph::gabriel_prune(&pos, &pairs3), Some(vec![1, 0]));
    assert_eq!(graph::gabriel_prune(&pos, &[(0, 9, 1.0)]), None);
    assert_eq!(graph::gabriel_prune(&pos, &[(0, 1, f64::NAN)]), None);
}

fn rcfg(window: (i64, i64), dist: f64) -> RouteCfg {
    RouteCfg {
        gi0: 0,
        gj0: 0,
        window,
        dist_edge_max: dist,
        rule: Rule::Slope,
    }
}

fn node(l: &Level, k: usize, nx: usize) -> Node {
    let (i, j) = ((k % nx) as i64, (k / nx) as i64);
    Node {
        k,
        cell: (i, j),
        pos: (i as f64 * 0.8, l.y(k) + 0.95, j as f64 * 0.8),
    }
}

#[test]
fn route_pairs_and_joined_groups() {
    // a 30 x 1 corridor: nodes at both ends 23.2 m apart (over 15 m): no pair, then the groups are joined by a
    // string-pulled chain of waypoints
    let l = level(30, 1, |_, _| Some(0.0));
    let nodes = vec![node(&l, 0, 30), node(&l, 29, 30)];
    let blocked = std::collections::HashSet::new();
    let cfg = rcfg((-1, 31), 15.0);
    assert!(route::edge_pairs(&l, &nodes, cfg, &blocked).unwrap().is_empty());
    let close = vec![node(&l, 0, 30), node(&l, 10, 30)];
    let p = route::edge_pairs(&l, &close, cfg, &blocked).unwrap();
    assert_eq!(p.len(), 1);
    assert_eq!((p[0].0, p[0].1), (0, 1));
    let cells: Vec<(i64, i64, usize)> = (0..30).map(|k| (k as i64, 0, k)).collect();
    let j = route::connect_groups(&l, &nodes, &[], &cells, (0.0, 0.0), 0.8, 0.95, cfg, &blocked).unwrap();
    assert!(!j.nodes.is_empty());
    assert_eq!(j.pairs.len(), j.nodes.len() + 1);
    // every leg within 15 m
    assert!(j.pairs.iter().all(|p| p.2 <= 15.0));
    // a wall (an empty column) between them: nothing to join
    let walled = level(30, 1, |i, _| (i != 15).then_some(0.0));
    let nodes2 = vec![node(&walled, 0, 30), node(&walled, 28, 30)];
    let cells2: Vec<(i64, i64, usize)> = (0..29).map(|k| (if k < 15 { k } else { k + 1 } as i64, 0, k)).collect();
    let j2 = route::connect_groups(&walled, &nodes2, &[], &cells2, (0.0, 0.0), 0.8, 0.95, cfg, &blocked).unwrap();
    assert!(j2.nodes.is_empty() && j2.pairs.is_empty());
    // a pair naming no node, a bucket the reference cannot compute
    assert!(route::connect_groups(&l, &nodes, &[(0, 5)], &cells, (0.0, 0.0), 0.8, 0.95, cfg, &blocked).is_err());
    let mut nan = nodes.clone();
    nan[0].pos.0 = f64::NAN;
    assert!(route::edge_pairs(&l, &nan, cfg, &blocked).is_err());
}

/// A random level for the searches: 0-2 surfaces per column on two floors, some steps.
fn random_level(hs: &[u8], nx: usize, nz: usize) -> Level {
    let mut y = Vec::new();
    let mut start = vec![0];
    for (c, &h) in hs.iter().enumerate().take(nx * nz) {
        if h % 5 != 0 {
            y.push(f64::from(h % 4) * 0.15);
        }
        if h % 7 == 0 {
            y.push(3.0 + (c % 3) as f64 * 0.1);
        }
        start.push(y.len());
    }
    let n = y.len();
    Level::new(LevelData {
        nx,
        nz,
        start,
        keep: vec![true; n],
        y,
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

proptest! {
    #![proptest_config(ProptestConfig::with_cases(150))]
    /// The window search answers as the generic search does: the same path, the same cells entered.
    #[test]
    fn window_search_is_the_search(hs in proptest::collection::vec(0u8..40, 64), s in 0usize..64, t in 0usize..64,
                                   w0 in -1i64..2, w1 in 4i64..9) {
        let l = random_level(&hs, 8, 8);
        if l.is_empty() {
            return Ok(());
        }
        let (s, t) = (s % l.len(), t % l.len());
        let col = |k: usize| {
            let c = l.col_of[k];
            ((c % 8) as i64, (c / 8) as i64)
        };
        let targets: FastSet<usize> = [t].into_iter().collect();
        let is_target = |n: usize| targets.contains(&n);
        let mut a: FastMap<usize, (i64, i64)> = [(s, col(s))].into_iter().collect();
        let want = l.search(&[s], &is_target, &mut a, (w0, w1), 0, 0, Rule::Slope);
        let mut w = WindowSearch::new(&l, (0, 0), (w0, w1), Rule::Slope).unwrap();
        w.insert(s, col(s));
        // twice: the second answer comes from the memo
        for _ in 0..2 {
            let got = w.search(&[s], &is_target, None).unwrap();
            match got {
                Ok(p) => prop_assert_eq!(Ok(p), want.clone()),
                Err(()) => prop_assert!(w.local(s).is_none()),
            }
            if w.local(s).is_some() {
                // the same cells entered, no others
                for k in 0..l.len() {
                    prop_assert_eq!(CellMap::get(&w, k), a.get(&k).copied());
                }
            }
        }
    }
}

/// The shortest route between a and b over the kept pairs (Floyd-Warshall on a few nodes).
#[allow(clippy::many_single_char_names)] // the textbook names
fn route_len(n: usize, pairs: &[(usize, usize, f64)], keep: &[usize], a: usize, b: usize) -> f64 {
    let mut d = vec![vec![f64::INFINITY; n]; n];
    for (i, row) in d.iter_mut().enumerate() {
        row[i] = 0.0;
    }
    for &k in keep {
        let (u, v, l) = pairs[k];
        d[u][v] = d[u][v].min(l);
        d[v][u] = d[v][u].min(l);
    }
    for m in 0..n {
        for i in 0..n {
            for j in 0..n {
                let via = d[i][m] + d[m][j];
                if via < d[i][j] {
                    d[i][j] = via;
                }
            }
        }
    }
    d[a][b]
}

#[test]
fn spanner_drops_a_pair_with_a_route_within_1_2() {
    // three nodes on a line: 0 - 1 - 2 joins 0 and 2 in 2 m, within 1.2 x 2 m: the long pair goes
    let pairs = [(0, 1, 1.0), (0, 2, 2.0), (1, 2, 1.0)];
    assert_eq!(graph::spanner_prune(3, &pairs, 1.2), Some(vec![0, 2]));
    // a right angle: the route round the corner (2 m) is longer than 1.2 x sqrt 2: the diagonal stays
    let pairs = [(0, 1, 1.0), (1, 2, 1.0), (0, 2, 2f64.sqrt())];
    assert_eq!(graph::spanner_prune(3, &pairs, 1.2), Some(vec![0, 1, 2]));
    // a pair naming no node, a NaN length
    assert_eq!(graph::spanner_prune(2, &[(0, 2, 1.0)], 1.2), None);
    assert_eq!(graph::spanner_prune(2, &[(0, 1, f64::NAN)], 1.2), None);
}

proptest! {
    /// KYG-EDGE-3: every pair left out has a route over the kept pairs at most 1.2 times its length; every kept pair
    /// had none when it was taken, so the kept pairs join exactly the groups all pairs join.
    #[test]
    fn spanner_keeps_routes_within_the_stretch(
        pts in proptest::collection::vec((0.0f64..20.0, 0.0f64..20.0), 2..12),
        mask in proptest::collection::vec(any::<bool>(), 66),
    ) {
        let n = pts.len();
        let mut pairs = Vec::new();
        let mut m = 0;
        for a in 0..n {
            for b in a + 1..n {
                if mask[m % mask.len()] {
                    pairs.push((a, b, (pts[a].0 - pts[b].0).hypot(pts[a].1 - pts[b].1)));
                }
                m += 1;
            }
        }
        let keep = graph::spanner_prune(n, &pairs, 1.2).unwrap();
        let all: Vec<usize> = (0..pairs.len()).collect();
        for (k, &(a, b, d)) in pairs.iter().enumerate() {
            let r = route_len(n, &pairs, &keep, a, b);
            if !keep.contains(&k) {
                prop_assert!(r <= 1.2 * d + 1e-9, "pair {k} left out with a route of {r} for {d}");
            }
            prop_assert_eq!(r.is_finite(), route_len(n, &pairs, &all, a, b).is_finite());
        }
    }
}
