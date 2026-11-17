//! Unit and property tests of the walkability rules; the reference's answers on random runs are checked from Python
//! (`bfbc2-auto-navkit/tests/test_navbuild_native.py`).
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::float_cmp)]

use super::*;
use proptest::prelude::*;

/// A grid whose column (i, j) holds the heights `cols[j * nx + i]`; pitch 0.2 from (0, 0).
fn grid(nx: usize, nz: usize, cols: &[&[f64]]) -> Grid {
    let mut start = vec![0];
    let mut y = Vec::new();
    for c in cols {
        y.extend_from_slice(c);
        start.push(y.len());
    }
    Grid::new(nx, nz, start, y, 0.2, 0.0, 0.0).unwrap()
}

/// One surface per column at `h(i, j)`.
fn single(nx: usize, nz: usize, h: impl Fn(usize, usize) -> f64) -> Grid {
    let mut y = Vec::new();
    for j in 0..nz {
        for i in 0..nx {
            y.push(h(i, j));
        }
    }
    Grid::new(nx, nz, (0..=nx * nz).collect(), y, 0.2, 0.0, 0.0).unwrap()
}

#[test]
fn shapes() {
    assert!(Grid::new(2, 1, vec![0, 1], vec![0.0], 0.2, 0.0, 0.0).is_err());
    assert!(Grid::new(1, 1, vec![1, 1], vec![0.0], 0.2, 0.0, 0.0).is_err());
    assert!(Grid::new(1, 1, vec![0, 2], vec![0.0], 0.2, 0.0, 0.0).is_err());
    assert!(Grid::new(2, 1, vec![0, 2, 1], vec![0.0, 1.0], 0.2, 0.0, 0.0).is_err());
    assert!(Grid::new(usize::MAX, 2, vec![0], vec![], 0.2, 0.0, 0.0).is_err());
    let g = grid(1, 1, &[&[0.0, 1.0]]);
    assert_eq!((g.len(), g.is_empty()), (2, false));
    assert!(single(0, 0, |_, _| 0.0).is_empty());
    let e = g.headroom(Some(&[true])).unwrap_err();
    assert_eq!(
        e.to_string(),
        "level arrays do not agree: ignore: 1 values for 2 surfaces"
    );
    assert!(g.stair_slope(&[], 1, 0.7).is_err());
    assert!(
        g.narrow_lips(&[], &[false, false], 0.8, 2, LipTests::default())
            .is_err()
    );
    assert!(
        g.narrow_lips(&[false, false], &[], 0.8, 2, LipTests::default())
            .is_err()
    );
    let ny = Floats::F32(vec![1.0]);
    let t = LipTests {
        rise: None,
        flat: Some((&ny, 0.9)),
    };
    assert!(g.narrow_lips(&[false, false], &[false, false], 0.8, 2, t).is_err());
    assert!(g.landing(&[], &[false, false]).is_err());
    assert!(g.landing(&[false, false], &[]).is_err());
}

#[test]
fn headroom_per_column() {
    // a column of four: 0, 0.03 (within 0.05: not above), 2.0, 1.0; a column of one; an empty column
    let g = grid(3, 1, &[&[0.0, 0.03, 2.0, 1.0], &[5.0], &[]]);
    let h = g.headroom(None).unwrap();
    assert_eq!(h[..4], [1.0, 1.0 - 0.03, f64::INFINITY, 1.0]);
    assert_eq!(h[4], f64::INFINITY);
    // ignored surfaces are not ceilings and keep infinity
    let h = g.headroom(Some(&[false, false, false, true, false])).unwrap();
    assert_eq!(h[..4], [2.0, 2.0 - 0.03, f64::INFINITY, f64::INFINITY]);
    // one left after ignoring: nothing above
    let h = g.headroom(Some(&[true, true, false, true, false])).unwrap();
    assert!(h.iter().all(|v| v.is_infinite()));
}

#[test]
fn stair_slopes() {
    // a ramp along x rising 0.1 per sample (0.5 per metre at pitch 0.2): central differences inside, one-sided at the
    // ends; a second layer 3 m up in the middle column is not on the same layer
    let g = grid(3, 1, &[&[0.0], &[0.1, 3.1], &[0.2]]);
    let (deg, gx, gz) = g.stair_slope(&[true, true, true, true], 1, 0.7).unwrap();
    assert_eq!(gx[0], 0.1 / 0.2);
    assert_eq!(gx[1], (0.2 - 0.0) / (2.0 * 0.2));
    assert_eq!(gx[3], (0.2 - 0.1) / 0.2);
    assert_eq!(gz, [0.0; 4]);
    assert_eq!(deg[1], pymath::slope_deg(gx[1], 0.0));
    assert_eq!(deg[2], 90.0, "no neighbour on its layer");
    assert_eq!(gx[2], 0.0);
    // a missing + side takes the - side; outside the mask 90 and 0
    let (deg, gx, _) = g.stair_slope(&[false, false, false, true], 1, 0.7).unwrap();
    assert_eq!((deg[0], gx[0]), (90.0, 0.0));
    assert_eq!(gx[3], (0.2 - 0.1) / 0.2);
    // r = 2 reaches two samples; along z
    let g = single(1, 5, |_, j| j as f64 * 0.05);
    let (_, gx, gz) = g.stair_slope(&[false, false, true, false, false], 2, 0.7).unwrap();
    assert_eq!((gx[2], gz[2]), (0.0, (0.2 - 0.0) / (4.0 * 0.2)));
    // the nearest of a column's surfaces within tol, the first of equals
    let g = grid(2, 1, &[&[0.0], &[0.3, -0.3, 0.2]]);
    let (_, gx, _) = g.stair_slope(&[true, false, false, false], 1, 0.7).unwrap();
    assert_eq!(gx[0], 0.2 / 0.2);
}

#[test]
fn lips() {
    // a steep sample between flat walkable ground: x = 2 is the lip, its neighbours 0.05 lower / higher
    let g = single(5, 1, |i, _| [0.0, 0.0, 0.3, 0.05, 0.05][i]);
    let base = [true, true, false, true, true];
    let cand = [false, false, true, false, false];
    let lim = 40f64.to_radians().tan();
    assert!(g.narrow_lips(&base, &cand, lim, 2, LipTests::default()).unwrap()[2]);
    // too steep between the two sides
    let g2 = single(5, 1, |i, _| [0.0, 0.0, 0.3, 0.6, 0.6][i]);
    assert!(!g2.narrow_lips(&base, &cand, lim, 2, LipTests::default()).unwrap()[2]);
    // the rise test: the lip 0.3 above a side
    let r = LipTests {
        rise: Some(0.2),
        flat: None,
    };
    assert!(!g.narrow_lips(&base, &cand, lim, 2, r).unwrap()[2]);
    let r = LipTests {
        rise: Some(0.3),
        flat: None,
    };
    assert!(g.narrow_lips(&base, &cand, lim, 2, r).unwrap()[2]);
    // the flat test, float32 and float64 normals: the Python float is cast to float32 (equal there, so not below)
    let c = 15f64.to_radians().cos();
    let n32 = Floats::F32(vec![c as f32; 5]);
    let n64 = Floats::F64(vec![f64::from(c as f32); 5]);
    let f32_ = LipTests {
        rise: None,
        flat: Some((&n32, c)),
    };
    let f64_ = LipTests {
        rise: None,
        flat: Some((&n64, c)),
    };
    assert!(g.narrow_lips(&base, &cand, lim, 2, f32_).unwrap()[2]);
    assert!(!g.narrow_lips(&base, &cand, lim, 2, f64_).unwrap()[2]);
    // the + side's normal under the cosine
    let n = Floats::F64(vec![1.0, 1.0, 1.0, 0.5, 1.0]);
    let t = LipTests {
        rise: None,
        flat: Some((&n, 0.9)),
    };
    assert!(!g.narrow_lips(&base, &cand, lim, 2, t).unwrap()[2]);
    // width 1: the walkable ground two samples away on one side is out of reach
    let base1 = [true, false, false, true, true];
    assert!(!g.narrow_lips(&base1, &cand, lim, 1, LipTests::default()).unwrap()[2]);
    assert!(g.narrow_lips(&base1, &cand, lim, 2, LipTests::default()).unwrap()[2]);
    // the grid's edge, a column without a surface near enough, an empty column
    let edge = [true, false, false, false, false];
    assert!(!g.narrow_lips(&base, &edge, lim, 2, LipTests::default()).unwrap()[0]);
    let far = single(3, 1, |i, _| [0.0, 1.5, 0.0][i]);
    assert!(
        !far.narrow_lips(&[true; 3], &[false, true, false], lim, 2, LipTests::default())
            .unwrap()[1]
    );
    let hole = grid(3, 1, &[&[0.0], &[0.0], &[]]);
    assert!(
        !hole
            .narrow_lips(&[true; 2], &[false, true], lim, 2, LipTests::default())
            .unwrap()[1]
    );
    // diagonals: (1, 1) and (1, -1) through a 3 x 3 grid whose axes hold nothing walkable
    let d = single(3, 3, |_, _| 0.0);
    let mut base = [false; 9];
    base[0] = true;
    base[8] = true;
    let mut cand = [false; 9];
    cand[4] = true;
    assert!(d.narrow_lips(&base, &cand, lim, 1, LipTests::default()).unwrap()[4]);
    let mut base = [false; 9];
    base[2] = true;
    base[6] = true;
    assert!(d.narrow_lips(&base, &cand, lim, 1, LipTests::default()).unwrap()[4]);
}

#[test]
fn landings() {
    // a stair surface at (2, 2) of a 5 x 5 grid: candidates within two columns and 0.9 m
    let g = single(6, 5, |i, _| if i == 5 { 0.0 } else { i as f64 * 0.3 });
    let mut stair = vec![false; 30];
    stair[2 * 6 + 2] = true;
    let cand = vec![true; 30];
    let l = g.landing(&stair, &cand).unwrap();
    for j in 0..5 {
        for i in 0..6 {
            let want = (i as i64 - 2).abs() <= 2 && (j as i64 - 2).abs() <= 2 && i < 5;
            assert_eq!(l[j * 6 + i], want, "({i}, {j})");
        }
    }
    let none = g.landing(&stair, &[false; 30]).unwrap();
    assert!(none.iter().all(|v| !v));
}

fn tri(p: [[f64; 3]; 3]) -> Tri {
    p
}

#[test]
fn objects() {
    // a 6 x 6 grid at pitch 0.5 from (0, 0), ground at 0 and a deck at 2 in every column
    let mut y = Vec::new();
    for _ in 0..36 {
        y.extend([0.0, 2.0]);
    }
    let g = Grid::new(6, 6, (0..=36).map(|c| 2 * c).collect(), y, 0.5, 0.0, 0.0).unwrap();
    // near: a triangle over x 1..1.5, z 1..1.5 at y 2; grow 0.5 -> columns 1..4 each way; the deck only
    let t = tri([[1.0, 2.0, 1.0], [1.5, 2.0, 1.0], [1.5, 2.0, 1.5]]);
    let near = g.near_objects(&[t], 0.5, 0.5, 0.5).unwrap();
    for j in 0..6 {
        for i in 0..6 {
            let c = j * 6 + i;
            let inside = (1..=4).contains(&i) && (1..=4).contains(&j);
            assert_eq!((near[2 * c], near[2 * c + 1]), (false, inside), "({i}, {j})");
        }
    }
    // outside the grid: nothing; a NaN height: nothing (NumPy's min); a NaN x: the reference fails
    let off = tri([[9.0, 2.0, 9.0], [9.5, 2.0, 9.0], [9.5, 2.0, 9.5]]);
    assert!(g.near_objects(&[off], 0.1, 0.5, 0.5).unwrap().iter().all(|v| !v));
    let nan_y = tri([[1.0, f64::NAN, 1.0], [1.5, 2.0, 1.0], [1.5, 2.0, 1.5]]);
    assert!(g.near_objects(&[nan_y], 0.1, 0.5, 0.5).unwrap().iter().all(|v| !v));
    let ny2 = tri([[1.0, 2.0, 1.0], [1.5, f64::NAN, 1.0], [1.5, 2.0, 1.5]]);
    assert!(g.near_objects(&[ny2], 0.1, 0.5, 0.5).unwrap().iter().all(|v| !v));
    let ny3 = tri([[1.0, 2.0, 1.0], [1.5, 2.0, 1.0], [1.5, f64::NAN, 1.5]]);
    assert!(g.near_objects(&[ny3], 0.1, 0.5, 0.5).unwrap().iter().all(|v| !v));
    let nan_x = tri([[f64::NAN, 2.0, 1.0], [1.5, 2.0, 1.0], [1.5, 2.0, 1.5]]);
    assert!(g.near_objects(&[nan_x], 0.1, 0.5, 0.5).is_err());
    assert!(g.on_objects(&[nan_x], 0.2).is_err());
    let flat = Grid::new(1, 1, vec![0, 1], vec![0.0], 0.0, 0.0, 0.0).unwrap();
    assert!(
        flat.near_objects(&[t], 0.0, 0.5, 0.5).is_err(),
        "a zero pitch: the reference fails"
    );
    // on: a triangle covering x + z <= 2 sloping from 2 at the origin to 2.5 at x = 2; tol 0.2 (columns i <= 1)
    let slope = tri([[0.0, 2.0, 0.0], [2.0, 2.5, 0.0], [0.0, 2.0, 2.0]]);
    let on = g.on_objects(&[slope], 0.2).unwrap();
    for j in 0..6 {
        for i in 0..6 {
            let c = j * 6 + i;
            let want = i + j <= 4 && i <= 1;
            assert_eq!((on[2 * c], on[2 * c + 1]), (false, want), "({i}, {j})");
        }
    }
    // a degenerate triangle (a line) is skipped
    let line = tri([[0.0, 2.0, 0.0], [1.0, 2.0, 1.0], [2.0, 2.0, 2.0]]);
    assert!(g.on_objects(&[line], 0.2).unwrap().iter().all(|v| !v));
    // python's min / max of three: the first unless a later one is less / greater
    assert!(Grid::py_min3(f64::NAN, 1.0, 0.0).is_nan());
    assert_eq!(Grid::py_min3(2.0, f64::NAN, 1.0), 1.0);
    assert_eq!(Grid::py_max3(0.0, f64::NAN, 1.0), 1.0);
    assert_eq!(Grid::py_max3(2.0, 3.0, 1.0), 3.0);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
    /// Random runs: headroom is the least height more than 0.05 m above in the column, or infinity; the lips and
    /// landings are candidates; stair slopes lie in 0..90.
    #[test]
    fn random_runs(cols in proptest::collection::vec(proptest::collection::vec(0u8..40, 0..4), 16),
                   flags in proptest::collection::vec(any::<(bool, bool)>(), 64)) {
        let hs: Vec<Vec<f64>> = cols.iter().map(|c| c.iter().map(|&v| f64::from(v) * 0.1).collect()).collect();
        let refs: Vec<&[f64]> = hs.iter().map(Vec::as_slice).collect();
        let g = grid(4, 4, &refs);
        let n = g.len();
        let base: Vec<bool> = flags.iter().take(n).map(|f| f.0).collect();
        let cand: Vec<bool> = flags.iter().take(n).map(|f| f.1).collect();
        let h = g.headroom(None).unwrap();
        for (c, w) in g.start.windows(2).enumerate() {
            for k in w[0]..w[1] {
                let want = hs[c].iter().copied().filter(|&v| v > g.y[k] + 0.05).fold(f64::INFINITY, f64::min);
                prop_assert_eq!(h[k], if want.is_finite() { want - g.y[k] } else { want });
            }
        }
        let lips = g.narrow_lips(&base, &cand, 0.84, 2, LipTests::default()).unwrap();
        prop_assert!(lips.iter().zip(&cand).all(|(l, c)| !l || *c));
        let land = g.landing(&base, &cand).unwrap();
        prop_assert!(land.iter().zip(&cand).all(|(l, c)| !l || *c));
        let (deg, _, _) = g.stair_slope(&cand, 1, 0.7).unwrap();
        prop_assert!(deg.iter().all(|d| (0.0..=90.0).contains(d)));
    }
}
