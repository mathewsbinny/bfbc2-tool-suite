//! Unit and property tests; the reference's answers on whole levels are checked from Python
//! (`bfbc2-auto-navkit/tests/test_navbuild_native.py`).
#![allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic, clippy::float_cmp)]

use super::*;
use proptest::prelude::*;

/// A grid of `nx` x `nz` columns, one kept surface each at height `h(i, j)`, flat gradients; terrain everywhere.
fn flat(nx: usize, nz: usize, h: impl Fn(usize, usize) -> f64) -> LevelData {
    let n = nx * nz;
    let mut y = Vec::with_capacity(n);
    for j in 0..nz {
        for i in 0..nx {
            y.push(h(i, j));
        }
    }
    LevelData {
        nx,
        nz,
        start: (0..=n).collect(),
        keep: vec![true; n],
        y,
        gx: Floats::F64(vec![0.0; n]),
        gz: Floats::F64(vec![0.0; n]),
        stair: vec![false; n],
        terrain: vec![true; n],
        pitch: 0.8,
        step: 0.28,
        link_step: 0.6,
    }
}

#[test]
fn shapes_are_checked() {
    let mut d = flat(2, 2, |_, _| 0.0);
    d.start = vec![0, 1, 2];
    assert!(matches!(Level::new(d), Err(NavError::Shape(_))));
    let mut d = flat(2, 2, |_, _| 0.0);
    d.start = vec![0, 2, 1, 3, 4];
    assert!(Level::new(d).is_err());
    let mut d = flat(2, 2, |_, _| 0.0);
    d.keep.pop();
    assert_eq!(
        Level::new(d).unwrap_err().to_string(),
        "level arrays do not agree: keep: 3 values for 4 surfaces"
    );
    let l = Level::new(flat(2, 2, |_, _| 0.0)).unwrap();
    assert_eq!((l.len(), l.is_empty()), (4, false));
    assert_eq!(l.surface(3), Ok(3));
    assert_eq!(l.surface(4), Err(NavError::Surface(4)));
    assert_eq!(l.surface(-1).unwrap_err().to_string(), "surface -1 is not in the level");
    assert_eq!(NavError::Cell(5).to_string(), "no cell known for surface 5");
    assert_eq!(Rule::from_name("raw"), Some(Rule::Raw));
    assert_eq!(Rule::from_name("slope"), Some(Rule::Slope));
    assert_eq!(Rule::from_name("terrain"), Some(Rule::Terrain));
    assert_eq!(Rule::from_name("x"), None);
}

#[test]
fn numbers_follow_numpy() {
    // float32 stays float32 with Python numbers; float64 wins
    let a = Num::F32(0.1);
    assert_eq!(a.add(Num::F32(0.2)), Num::F32(0.1f32 + 0.2f32));
    assert_eq!(a.add(Num::F64(0.2)), Num::F64(f64::from(0.1f32) + 0.2));
    assert_eq!(a.div_py(2.0).mul_int(3), Num::F32(0.1f32 / 2.0 * 3.0));
    assert_eq!(Num::py_mul(0.8, a), Num::F32(0.8f32 * 0.1f32));
    assert_eq!(Num::py_mul(0.8, Num::F64(0.1)), Num::F64(0.8 * 0.1));
    assert_eq!(Num::F64(0.5).div_py(2.0).mul_int(-1).get(), -0.25);
    assert_eq!(Floats::F32(vec![1.5]).at(0), Num::F32(1.5));
    assert_eq!(Floats::F32(vec![]).len(), 0);
}

#[test]
fn neighbours_and_rules() {
    // a ramp along x rising 0.5 m per column: too steep for the raw rule's 0.6? no - 0.5 <= 0.6 links raw;
    // the slope rule with flat gradients predicts 0, so 0.5 > 0.28 does not link
    let l = Level::new(flat(3, 3, |i, _| i as f64 * 0.5)).unwrap();
    assert_eq!(l.neighbour(0, 0, 0, 1, 0, None, Rule::Raw), Some(1));
    assert_eq!(l.neighbour(0, 0, 0, 1, 0, None, Rule::Slope), None);
    assert_eq!(l.neighbour(0, 0, 0, 1, 0, None, Rule::Terrain), Some(1));
    assert_eq!(l.neighbour(0, 0, 0, -1, 0, None, Rule::Raw), None, "outside the grid");
    assert_eq!(l.neighbour(0, 0, 0, 0, -1, None, Rule::Raw), None);
    // diagonal raw: through a side column within 0.6 of both
    assert_eq!(l.neighbour(0, 0, 0, 1, 1, None, Rule::Raw), Some(4));
    assert!(l.surfaces(5, 0).next().is_none());
    // with gradients the slope rule predicts the rise
    let mut d = flat(3, 1, |i, _| i as f64 * 0.5);
    d.gx = Floats::F64(vec![0.5 / 0.8; 3]);
    let l = Level::new(d).unwrap();
    assert!(l.connected(0, 1, 1, 0, Rule::Slope));
    // non-terrain surfaces take the slope rule under 'terrain'
    let mut d = flat(3, 1, |i, _| i as f64 * 0.5);
    d.terrain = vec![true, false, true];
    let l = Level::new(d).unwrap();
    assert!(!l.connected(0, 1, 1, 0, Rule::Terrain));
    assert!(l.connected(0, 1, 1, 0, Rule::Raw));
}

#[test]
fn stairs_take_their_own_incline() {
    // k is a stair surface with gradient 1.0, n flat: the mean predicts 0.4 (miss 0.4 > 0.28), k's own 0.8 fits
    let mut d = flat(2, 1, |i, _| i as f64 * 0.8);
    d.gx = Floats::F64(vec![1.0, 0.0]);
    let l = Level::new(d.clone()).unwrap();
    assert!(!l.connected(0, 1, 1, 0, Rule::Slope));
    d.stair = vec![true, false];
    let l = Level::new(d.clone()).unwrap();
    assert!(l.connected(0, 1, 1, 0, Rule::Slope));
    // and in float32
    d.gx = Floats::F32(vec![1.0, 0.0]);
    d.gz = Floats::F32(vec![0.0, 0.0]);
    let l = Level::new(d).unwrap();
    assert!(l.connected(0, 1, 1, 0, Rule::Slope));
}

#[test]
fn prefer_and_nearest() {
    // column 1 holds three kept surfaces: 0.1, 0.05 (nearest), 0.2
    let d = LevelData {
        nx: 2,
        nz: 1,
        start: vec![0, 1, 4],
        keep: vec![true, true, true, true],
        y: vec![0.0, 0.1, 0.05, 0.2],
        gx: Floats::F64(vec![0.0; 4]),
        gz: Floats::F64(vec![0.0; 4]),
        stair: vec![false; 4],
        terrain: vec![true; 4],
        pitch: 0.8,
        step: 0.28,
        link_step: 0.6,
    };
    let l = Level::new(d).unwrap();
    assert_eq!(l.neighbour(0, 0, 0, 1, 0, None, Rule::Raw), Some(2));
    let p: FastSet<usize> = [3, 1].into_iter().collect();
    assert_eq!(
        l.neighbour(0, 0, 0, 1, 0, Some(&p), Rule::Raw),
        Some(1),
        "the first preferred in column order"
    );
    let sides = l.side_neighbours(&[(0, 0, 0)], 0, 0, Rule::Raw);
    assert_eq!(sides, vec![Some(2), None, None, None]);
}

#[test]
fn components_split_layers_and_steps() {
    // 4 x 1: heights 0, 0, 2, 2 -> two components under the raw rule
    let l = Level::new(flat(4, 1, |i, _| if i < 2 { 0.0 } else { 2.0 })).unwrap();
    let members: Vec<(usize, i64, i64)> = (0..4).map(|k| (k, k as i64, 0)).collect();
    let c = l.components(&members, 0, 0, Rule::Raw);
    assert_eq!(c, vec![vec![(0, 0, 0), (1, 0, 1)], vec![(2, 0, 2), (3, 0, 3)]]);
    // a duplicate member keeps its first place; a surface outside the members is not followed
    let c = l.components(&[(1, 1, 0), (0, 0, 0), (1, 1, 0)], 0, 0, Rule::Raw);
    assert_eq!(c, vec![vec![(1, 0, 1), (0, 0, 0)]]);
    // a member whose given cell is not its column's neighbour cell is not joined
    let c = l.components(&[(0, 0, 0), (1, 5, 5)], 0, 0, Rule::Raw);
    assert_eq!(c.len(), 2);
}

#[test]
#[allow(clippy::many_single_char_names)]
fn areas_by_tile_and_bisected() {
    // 6 x 2, flat but a wall at x = 4 (row 0 only); tiles of 3 samples from local 0, 2 per axis
    let l = Level::new(flat(6, 2, |i, j| if i == 4 && j == 0 { 5.0 } else { 0.0 })).unwrap();
    let a = l.build_areas(0, 0, (0, 6), (0, 0), 3, 2, None, Rule::Raw).unwrap();
    let tiles: Vec<(i64, i64)> = a.iter().map(|x| x.0).collect();
    assert_eq!(tiles, [(0, 0), (1, 0), (1, 0)]);
    assert_eq!(
        a[0].1,
        [(0, 0, 0), (1, 0, 1), (0, 1, 6), (1, 1, 7), (2, 0, 2), (2, 1, 8)]
    );
    // the wall's surface is 5 m from every neighbour: an area of its own, after the rest of its tile
    assert_eq!(a[1].1.len(), 5);
    assert_eq!(a[2].1, [(4, 0, 4)]);
    // samples before `start` fold into the first tile; the last tile takes the rest
    let b = l.build_areas(0, 0, (0, 6), (2, 5), 1, 3, None, Rule::Raw).unwrap();
    assert_eq!(
        b.iter().map(|x| x.0).collect::<Vec<_>>(),
        [(0, 0), (1, 0), (2, 0), (2, 0)]
    );
    // the window reaches past the grid and the sector offset moves it
    let c = l.build_areas(3, 0, (-1, 4), (0, 0), 100, 1, None, Rule::Raw).unwrap();
    let cells: usize = c.iter().map(|x| x.1.len()).sum();
    assert_eq!(cells, 8, "columns 2 .. 5 of both rows");
    // bisected: at most 2 samples across, the longer axis cut at the middle first
    let f = Level::new(flat(5, 2, |_, _| 0.0)).unwrap();
    let d = f.build_areas(0, 0, (0, 5), (0, 0), 10, 1, Some(2), Rule::Raw).unwrap();
    let pieces: Vec<Vec<usize>> = d.iter().map(|x| x.1.iter().map(|c| c.2).collect()).collect();
    assert_eq!(pieces, [vec![0, 1, 5, 6], vec![2, 7], vec![3, 4, 8, 9]]);
    let e = f.build_areas(0, 0, (0, 2), (0, 0), 10, 1, Some(1), Rule::Raw).unwrap();
    assert_eq!(e.len(), 4, "cut along z too");
    assert!(f.build_areas(0, 0, (0, 5), (0, 0), 0, 1, None, Rule::Raw).is_err());
    assert!(f.build_areas(0, 0, (0, 5), (0, 0), 1, 1, Some(0), Rule::Raw).is_err());
}

#[test]
fn lines_and_d_sides() {
    let l = Level::new(flat(5, 5, |_, _| 0.0)).unwrap();
    let none = HashSet::new();
    assert!(l.line_walkable(0, (0, 0), (4, 4), 0, 0, 24, &none, Rule::Raw));
    assert!(l.line_walkable(0, (0, 0), (0, 0), 0, 0, 0, &none, Rule::Raw));
    assert!(l.line_walkable(0, (0, 0), (4, 1), 0, 0, 9, &none, Rule::Raw));
    // a wall at x = 2 (height 5)
    let w = Level::new(flat(5, 5, |i, _| if i == 2 { 5.0 } else { 0.0 })).unwrap();
    assert!(!w.line_walkable(0, (0, 0), (4, 0), 0, 0, 4, &none, Rule::Raw));
    // a D side between surfaces 1 and 2 blocks the straight step
    let blocked: HashSet<(usize, usize)> = [(1, 2)].into();
    assert!(!l.line_walkable(0, (0, 0), (4, 0), 0, 0, 4, &blocked, Rule::Raw));
    assert!(
        l.line_walkable(4, (4, 0), (0, 0), 0, 0, 0, &blocked, Rule::Raw),
        "only one way"
    );
    // diagonals: through the corner either way; x first; z first
    let b: HashSet<(usize, usize)> = [(0, 1)].into();
    assert!(!l.line_walkable(0, (0, 0), (1, 1), 0, 0, 6, &b, Rule::Raw));
    assert!(!l.line_walkable(0, (0, 0), (3, 1), 0, 0, 8, &b, Rule::Raw), "x first");
    let b: HashSet<(usize, usize)> = [(0, 5)].into();
    assert!(!l.line_walkable(0, (0, 0), (1, 3), 0, 0, 16, &b, Rule::Raw), "z first");
    let b: HashSet<(usize, usize)> = [(5, 6)].into();
    assert!(
        !l.line_walkable(0, (0, 0), (1, 1), 0, 0, 6, &b, Rule::Raw),
        "the second leg"
    );
    // a diagonal step the line crosses nearer one side column: (0, 0) -> (3, 2) leaves through x first, so only
    // the x side's D sides block it; (0, 0) -> (2, 1) steps straight, then diagonally through z first
    let b: HashSet<(usize, usize)> = [(0, 1)].into();
    assert!(
        !l.line_walkable(0, (0, 0), (3, 2), 0, 0, 13, &b, Rule::Raw),
        "x first: blocked by the x side"
    );
    let b: HashSet<(usize, usize)> = [(0, 5)].into();
    assert!(
        l.line_walkable(0, (0, 0), (3, 2), 0, 0, 13, &b, Rule::Raw),
        "x first: the z side does not count"
    );
    let b: HashSet<(usize, usize)> = [(1, 6)].into();
    assert!(
        !l.line_walkable(0, (0, 0), (2, 1), 0, 0, 7, &b, Rule::Raw),
        "z first: blocked by the z side"
    );
    let b: HashSet<(usize, usize)> = [(1, 2)].into();
    assert!(
        l.line_walkable(0, (0, 0), (2, 1), 0, 0, 7, &b, Rule::Raw),
        "z first: the x side does not count"
    );
    let b: HashSet<(usize, usize)> = [(9, 9)].into();
    assert!(l.line_walkable(0, (0, 0), (1, 1), 0, 0, 6, &b, Rule::Raw));
    // the end: k1 or within 0.6 m
    let h = Level::new(flat(3, 1, |i, _| i as f64 * 0.4)).unwrap();
    assert!(h.line_walkable(0, (0, 0), (1, 0), 0, 0, 2, &none, Rule::Raw));
    assert!(!h.line_walkable(0, (0, 0), (0, 0), 0, 0, 2, &none, Rule::Raw));
}

#[test]
fn searches() {
    // 5 x 1, a wall at x = 2: from 0 nothing is reached beyond it; 4 is reachable from 3
    let l = Level::new(flat(5, 1, |i, _| if i == 2 { 5.0 } else { 0.0 })).unwrap();
    let mut cells: HashMap<usize, (i64, i64)> = [(0, (0, 0)), (3, (3, 0))].into();
    let r = l
        .search(&[0], &|n| n == 4, &mut cells, (-1, 127), 0, 0, Rule::Raw)
        .unwrap();
    assert_eq!(r, None);
    assert_eq!(cells.get(&1), Some(&(1, 0)), "reached surfaces are entered");
    let r = l
        .search(&[3, 3], &|n| n == 4, &mut cells, (-1, 127), 0, 0, Rule::Raw)
        .unwrap();
    assert_eq!(r, Some(vec![3, 4]));
    // a longer path; an entry the map has is kept
    let l = Level::new(flat(5, 1, |_, _| 0.0)).unwrap();
    let mut cells: HashMap<usize, (i64, i64)> = [(0, (0, 0)), (2, (2, 0))].into();
    let r = l
        .search(&[0], &|n| n == 4, &mut cells, (-1, 127), 0, 0, Rule::Raw)
        .unwrap();
    assert_eq!(r, Some(vec![0, 1, 2, 3, 4]));
    // the window stops it
    let mut cells: HashMap<usize, (i64, i64)> = [(0, (0, 0))].into();
    assert_eq!(
        l.search(&[0], &|n| n == 4, &mut cells, (0, 3), 0, 0, Rule::Raw)
            .unwrap(),
        None
    );
    // a start without a cell
    let mut cells: HashMap<usize, (i64, i64)> = HashMap::new();
    assert_eq!(
        l.search(&[0], &|_| false, &mut cells, (0, 3), 0, 0, Rule::Raw),
        Err(NavError::Cell(0))
    );
}

#[test]
fn matched_flags_by_height() {
    // two columns: [0.0, 3.0] and [1.0]; the other run: [0.04, 2.0] (flags 2, 3) and [1.06] (flag 3)
    let got = matched_flags(
        2,
        &[0, 2, 3],
        &[0.0, 3.0, 1.0],
        &[0, 2, 3],
        &[0.04, 2.0, 1.06],
        &[2, 3, 3],
    )
    .unwrap();
    assert_eq!(got, [2, 1, 1]);
    // the first match in the column wins
    assert_eq!(
        matched_flags(1, &[0, 1], &[0.0], &[0, 2], &[0.01, -0.01], &[5, 6]).unwrap(),
        [5]
    );
    // arrays that do not cover the columns
    for (s, o, f) in [
        (&[0usize][..], &[0usize, 1][..], &[1u8][..]),
        (&[0, 1], &[0], &[1]),
        (&[0, 9], &[0, 1], &[1]),
        (&[0, 1], &[0, 9], &[1]),
        (&[0, 1], &[0, 1], &[]),
    ] {
        assert!(matches!(
            matched_flags(1, s, &[0.0], o, &[0.0], f),
            Err(NavError::Shape(_))
        ));
    }
}

#[test]
fn floods_and_islands() {
    // heights along x: 0, 0.1, 2 (a step the slope rule refuses), 2.1; z rows the same
    let mut d = flat(4, 2, |i, _| [0.0, 0.1, 2.0, 2.1][i]);
    d.keep[1] = false; // not walkable: the flood does not enter it, row 1 joins the two
    let l = Level::new(d).unwrap();
    let mut kept = vec![false; 8];
    kept[0] = true;
    l.flood(&mut kept, [0]);
    assert_eq!(kept, [true, false, false, false, true, true, false, false]);
    let (ids, n) = l.islands();
    assert_eq!(n, 2);
    assert_eq!(ids, [0, -1, 1, 1, 0, 0, 1, 1]);
    // an empty level
    let e = Level::new(flat(0, 0, |_, _| 0.0)).unwrap();
    assert!(e.is_empty());
    assert_eq!(e.islands(), (vec![], 0));
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]
    /// Random heights: the neighbour is kept, in the asked column, connected; components cover every member once.
    #[test]
    fn random_grids(hs in proptest::collection::vec(0u8..8, 16), gi in -1i64..5, gj in -1i64..5) {
        let l = Level::new(flat(4, 4, |i, j| f64::from(hs[j * 4 + i]) * 0.25)).unwrap();
        for (di, dj) in DIRS8 {
            for rule in [Rule::Raw, Rule::Slope, Rule::Terrain] {
                if let (Ok(k), true) = (usize::try_from(gj * 4 + gi), (0..4).contains(&gi) && (0..4).contains(&gj))
                    && let Some(n) = l.neighbour(k, gi, gj, di, dj, None, rule)
                {
                    prop_assert!(l.connected(k, n, di, dj, rule));
                    prop_assert_eq!(n as i64, (gj + dj) * 4 + gi + di);
                }
            }
        }
        let members: Vec<(usize, i64, i64)> = (0..16).map(|k| (k, (k % 4) as i64, (k / 4) as i64)).collect();
        let comps = l.components(&members, 0, 0, Rule::Terrain);
        let mut all: Vec<usize> = comps.iter().flatten().map(|c| c.2).collect();
        all.sort_unstable();
        prop_assert_eq!(all, (0..16).collect::<Vec<_>>());
    }
}
