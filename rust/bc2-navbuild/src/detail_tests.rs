//! `detail`'s rules on small synthetic areas (no game data); the whole port is proven against the retired Python on
//! every call of the regression builds (`rust/parity_nk3_detail.py`).

#![allow(clippy::indexing_slicing, clippy::assert_is_empty, clippy::float_cmp)]

use std::collections::BTreeSet;

use super::*;

const P: f64 = 0.8;

fn settings() -> DetailSettings {
    DetailSettings {
        pitch: P,
        ox: 0.0,
        oz: 0.0,
        step: 0.6,
        with_f: true,
        cut_corners: true,
        chain_turns: true,
    }
}

fn run(cells: &[(GridCell, f64)], s: &DetailSettings) -> Features {
    let r: Result<Features, ()> = detail_features(cells, &[], s, |_| Ok(true));
    r.unwrap_or_default()
}

fn grid(f: impl Fn(i64, i64) -> Option<f64>, n: i64, m: i64) -> Vec<(GridCell, f64)> {
    (0..n)
        .flat_map(|i| (0..m).map(move |j| (i, j)))
        .filter_map(|(i, j)| f(i, j).map(|y| ((i, j), y)))
        .collect()
}

fn xz(p: &Point) -> (i64, i64) {
    ((p[0] * 10.0).round() as i64, (p[2] * 10.0).round() as i64)
}

#[test]
fn flat_has_nothing() {
    let f = run(&grid(|_, _| Some(5.0), 4, 4), &settings());
    assert!(f.broken.is_empty() && f.lines.is_empty() && f.cells.is_empty());
}

#[test]
fn ledge_gives_a_facing_pair() {
    let cells = [((0, 0), 0.0), ((1, 0), 0.0), ((0, 1), 0.7), ((1, 1), 0.7)];
    let f = run(&cells, &settings());
    assert_eq!(f.broken.len(), 4);
    assert!(f.cells.is_empty());
    // the run of two cells merged, once per side, the owner on the right
    let mut lines: Vec<Vec<(i64, i64)>> = f.lines.iter().map(|l| l.iter().map(xz).collect()).collect();
    lines.sort();
    assert_eq!(lines, vec![vec![(-4, 4), (12, 4)], vec![(12, 4), (-4, 4)]]);
    let hi = f.lines.iter().find(|l| l.iter().all(|p| p[1] > 1.0));
    assert_eq!(
        hi.map(|l| l.iter().map(|p| p[1]).collect::<Vec<_>>()),
        Some(vec![0.7 + Y_OFFSET, 0.7 + Y_OFFSET])
    );
}

#[test]
fn raised_corner_is_cut() {
    let cells = [((0, 0), 0.0), ((1, 0), 0.0), ((0, 1), 0.0), ((1, 1), 0.7)];
    let f = run(&cells, &settings());
    assert_eq!(f.lines.len(), 2);
    let l = f
        .lines
        .iter()
        .find(|l| l.len() == 3)
        .map(|l| l.iter().map(xz).collect::<Vec<_>>());
    assert_eq!(l, Some(vec![(4, 12), (4, 4), (12, 4)]));
    let d = f
        .lines
        .iter()
        .find(|l| l.len() == 2)
        .map(|l| l.iter().map(xz).collect::<Vec<_>>());
    assert_eq!(d, Some(vec![(12, 4), (4, 12)]));
    let off = run(
        &cells,
        &DetailSettings {
            cut_corners: false,
            ..settings()
        },
    );
    assert!(off.lines.len() == 2 && off.lines.iter().all(|l| l.len() == 3));
}

#[test]
fn stairs_give_an_f_pair() {
    let f = run(&grid(|i, _| Some(0.7 * i as f64), 3, 2), &settings());
    assert!(f.lines.is_empty());
    assert_eq!(f.fx, BTreeSet::from([(1, 0), (1, 1)]));
    let dirs: Vec<Point> = f.cells.iter().map(|c| c.direction).collect();
    assert_eq!(dirs, vec![[-1.0, -0.0, -0.0], [1.0, -0.0, -0.0]]);
    assert!(f.cells.iter().all(|c| c.direction[1].is_sign_negative()));
    let pts: Vec<Vec<(i64, i64)>> = f.cells.iter().map(|c| c.points.iter().map(xz).collect()).collect();
    assert_eq!(pts[0], vec![(4, 4), (4, -4), (12, -4), (12, 12), (4, 12), (4, 4)]);
    assert_eq!(pts[1], vec![(4, -4), (12, -4), (12, 12), (4, 12), (4, -4)]);
    let z = run(
        &grid(|_, j| Some(0.7 * j as f64), 2, 3),
        &DetailSettings {
            with_f: false,
            ..settings()
        },
    );
    assert!(z.cells.is_empty() && z.lines.len() == 4);
}

#[test]
fn half_cells_break_with_their_own_height() {
    let cells = [((0, 0), 0.0), ((1, 1), 0.0)];
    let half = [(((1, 0), (-1, 0)), 0.7), (((1, 0), (0, 1)), 0.7)];
    let r: Result<Features, ()> = detail_features(&cells, &half, &settings(), |_| Ok(true));
    assert_eq!(r.map(|f| f.broken.len()), Ok(4));
    let low = [(((1, 0), (-1, 0)), 0.3), (((1, 0), (0, 1)), 0.3)];
    let r: Result<Features, ()> = detail_features(&cells, &low, &settings(), |_| Ok(true));
    assert_eq!(r.map(|f| f.broken.len()), Ok(0));
}

#[test]
fn keep_decides_and_its_error_stops() {
    let cells = [((0, 0), 0.0), ((1, 0), 0.0), ((0, 1), 0.0), ((1, 1), 0.7)];
    let r: Result<Features, ()> = detail_features(&cells, &[], &settings(), |c| {
        Ok(matches!(c, Candidate::Side((_, (_, 0)))))
    });
    assert_eq!(
        r.map(|f| f.lines.iter().map(Vec::len).collect::<Vec<_>>()),
        Ok(vec![2, 2])
    );
    assert_eq!(
        detail_features(&cells, &[], &settings(), |_| Err::<bool, _>("stop")),
        Err("stop")
    );
}

/// The spec's rule where the Python left the choice to its sets: a loop starting at a pinch leaves along the edge
/// whose end is least. Two F groups of cells joined through a hole: the hole's boundary meets itself.
#[test]
fn a_loop_starting_at_a_pinch_takes_the_least_edge() {
    // a ring of cells round (1, 1) and a cell at (3, 3) bridged to (2, 2): one group with a hole
    let fset: BTreeSet<GridCell> = [(0, 0), (1, 0), (2, 0), (0, 1), (2, 1), (0, 2), (1, 2), (2, 2), (3, 3)].into();
    let lps = loops(&quarters(&components(&fset).concat()));
    assert_eq!(lps.len(), 2);
    for lp in &lps {
        let set: BTreeSet<Half> = lp.iter().copied().collect();
        assert_eq!(set.len(), lp.len(), "simple loops");
    }
    // the outer loop is counter-clockwise, the hole clockwise
    let area = |lp: &Vec<Half>| {
        (0..lp.len())
            .map(|k| {
                let (a, b) = (lp[k], lp[(k + 1) % lp.len()]);
                a.0 * b.1 - b.0 * a.1
            })
            .sum::<i64>()
    };
    assert!(area(&lps[0]) > 0 && area(&lps[1]) < 0);
}

/// A vertex with nothing beside it on the record's side and no F cell touching it: the mean of the nearest F cells
/// furthest toward that side.
#[test]
fn nearest_f_cells_toward_the_side() {
    // the centre of the empty cell (1, 1), with F cells left, below and above it: 2 half units away each
    let fset: BTreeSet<GridCell> = [(0, 1), (1, 0), (1, 2)].into();
    let at: FastMap<GridCell, f64> = [((0, 1), 1.0), ((1, 0), 2.0), ((1, 2), 4.0)].into_iter().collect();
    // +x record: (1, 0) and (1, 2) are further toward +x than (0, 1)
    assert_eq!(nearest_f_cells(&fset, (2, 2), Axis::X, 1), vec![(1, 0), (1, 2)]);
    assert_eq!(f_point((2, 2), Axis::X, 1, &at, &fset, &settings())[1], 3.0 + Y_OFFSET);
    // -x record: (0, 1) alone; +z: (1, 2); -z: (1, 0)
    assert_eq!(f_point((2, 2), Axis::X, -1, &at, &fset, &settings())[1], 1.0 + Y_OFFSET);
    assert_eq!(f_point((2, 2), Axis::Z, 1, &at, &fset, &settings())[1], 4.0 + Y_OFFSET);
    assert_eq!(f_point((2, 2), Axis::Z, -1, &at, &fset, &settings())[1], 2.0 + Y_OFFSET);
}

#[test]
fn a_side_test_decides_where_it_answers() {
    // a flat 3 x 1 strip: no step breaks; the test breaks (0,0)|(1,0) whatever the heights (DICE's terrain rule,
    // KYG-DF-1), and leaves (1,0)|(2,0) to the step
    let cells = grid(|i, _| Some(if i == 2 { 1.0 } else { 0.0 }), 3, 1);
    let test = |((i, _), (di, _)): Side| -> Option<bool> { (i + i + di == 1).then_some(true) };
    let got = candidates_by(&cells, &[], 0.6, test);
    let sides: BTreeSet<Side> = got
        .iter()
        .map(|c| match *c {
            Candidate::Side(s) | Candidate::Leg(s) => s,
        })
        .collect();
    let want: BTreeSet<Side> = [((0, 0), (1, 0)), ((1, 0), (-1, 0)), ((1, 0), (1, 0)), ((2, 0), (-1, 0))]
        .into_iter()
        .collect();
    assert_eq!(sides, want, "both ways, the test's side and the step's");
    // a test answering false keeps a side the step would break
    let none = candidates_by(&cells, &[], 0.6, |_| Some(false));
    assert!(none.is_empty());
    // no test: the step rule (candidates)
    assert_eq!(candidates(&cells, &[], 0.6).len(), 2);
}

#[test]
fn split_rule_f_by_the_step_d_by_the_test() {
    // a 5 x 1 strip: cell 1 a pit (stepped into and out of along x), flat elsewhere; the test breaks only (3,0)|(4,0)
    let cells = grid(|i, _| Some(if i == 1 { -1.0 } else { 0.0 }), 5, 1);
    let test = |((i, _), (di, _)): Side| -> Option<bool> { Some(i + i + di == 7) };
    let r: Result<Features, ()> = detail_features_by(&cells, &[], &settings(), test, |_| Ok(true));
    let f = r.unwrap_or_default();
    assert_eq!(
        f.fx,
        [(1, 0)].into_iter().collect::<BTreeSet<_>>(),
        "the pit is an x-F cell by its steps"
    );
    let d: BTreeSet<Side> = [((3, 0), (1, 0)), ((4, 0), (-1, 0))].into_iter().collect();
    assert_eq!(f.d_sides, d, "the D sides are the test's");
    assert!(f.broken.contains(&((1, 0), (1, 0))) && f.broken.contains(&((0, 0), (1, 0))));
    // a test that never answers: the step rule alone, as detail_features
    let plain: Result<Features, ()> = detail_features_by(&cells, &[], &settings(), |_| None, |_| Ok(true));
    assert_eq!(plain.unwrap_or_default(), run(&cells, &settings()));
}
