#![allow(clippy::unwrap_used, clippy::float_cmp, clippy::indexing_slicing)]
use super::*;

/// A run of `nx` x `nz` columns at pitch 0.8 from the origin, one flat surface per column at height `y(i, j)`.
fn flat(nx: usize, nz: usize, y: impl Fn(usize, usize) -> f64) -> Samples {
    let n = nx * nz;
    Samples {
        pitch: 0.8,
        min_x: 0.0,
        min_z: 0.0,
        nx,
        nz,
        header: [0.0; 6],
        start: (0..=n).collect::<Vec<usize>>().into(),
        y: (0..n).map(|c| y(c % nx, c / nx)).collect::<Vec<f64>>().into(),
        y_f32: false,
        normal: Normals::F32(vec![[0.0, 1.0, 0.0]; n]),
        flags: vec![0; n],
        hit: None,
    }
}

fn settings() -> WalkSettings {
    WalkSettings {
        characters_absent: true,
        headroom_band: Some((0.5, 2.0)),
        steep_terrain: false,
        steep_max_cos: 55f64.to_radians().cos(),
        steep_span: 0.8,
        lip_width: Some(2),
        lip_object_rise: Some(0.3),
        lip_object_slope_max: false,
        lip_flat_cos: Some(15f64.to_radians().cos()),
        roof_prune: true,
        roof: RoofCfg {
            raise: 2.0,
            share: 0.5,
            near: 2.0,
            near_dy: 2.5,
        },
        dynamic_absent_headroom: true,
        dynamic_floor: Some(0.3),
        objective_floor: true,
        objective_reach: 1.5,
        sector: 100.0,
        ladder_reach: 1.5,
        ladder_foot: (-1.0, 1.0),
        ladder_top: (-1.5, 0.5),
        terrain_vtable: 1,
        character_vtable: 2,
        slope: 40.0,
        slope_cos: 40f64.to_radians().cos() - 0.01,
        slope_tan: 40f64.to_radians().tan(),
    }
}

#[test]
fn polygon_crossings_as_matplotlib() {
    let sq = [(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)];
    assert!(points_in_polygon(&sq, 1.0, 1.0));
    assert!(!points_in_polygon(&sq, 3.0, 1.0));
    // on the outline, matplotlib's answers (Path.contains_points, matplotlib 3.11): (0, 0) out, (2, 2) in, the
    // low z edge out, the others in
    let on = [(0.0, 0.0), (2.0, 2.0), (1.0, 0.0), (1.0, 2.0), (0.0, 1.0), (2.0, 1.0)];
    let got: Vec<bool> = on.iter().map(|&(x, z)| points_in_polygon(&sq, x, z)).collect();
    assert_eq!(got, vec![false, true, false, true, true, true]);
    assert!(!points_in_polygon(&sq[..2], 0.5, 0.0));
    assert!(!points_in_polygon(&sq, f64::NAN, 1.0));
}

#[test]
fn python_modulo_and_rounding() {
    assert_eq!(py_mod(-1.6, 100.0), Some(98.4));
    assert_eq!(py_mod(201.6, 100.0).map(|v| pymath::round_to(v, 1)), Some(1.6));
    assert_eq!(py_mod(-0.0, 100.0).map(f64::to_bits), Some(0.0f64.to_bits()));
    assert_eq!(py_mod(1.0, 0.0), None);
    assert_eq!(round_int(2.5).unwrap(), 2);
    assert_eq!(round_int(-0.5).unwrap(), 0);
    assert!(round_int(f64::NAN).is_err());
    assert_eq!(weak(0.999_999_99, true), 1.0);
    assert_eq!(weak(0.999_999_99, false), 0.999_999_99);
}

#[test]
fn heights_keep_their_bits() {
    // a damaged file's signalling NaN comes back as it was stored (a plain conversion quiets it)
    for bits in [
        0x7f80_0001u32,
        0xff80_0002,
        0x7fc0_0000,
        0x3f80_0000,
        0x8000_0000,
        0x7f7f_ffff,
    ] {
        let v = f32::from_bits(bits);
        assert_eq!(narrow(widen(v)).to_bits(), bits);
        assert_eq!(widen(v).is_nan(), v.is_nan());
    }
    assert_eq!(widen(1.5), 1.5);
}

#[test]
fn water_and_extent() {
    let s = flat(4, 4, |_, _| 0.0);
    // a water triangle over the low corner, 2 m up
    let v = [[-1.0, 2.0, -1.0], [3.0, 2.0, -1.0], [-1.0, 2.0, 3.0]];
    let top = water_tops(4, 4, 0.8, 0.0, 0.0, &v, &[[0, 1, 2]]).unwrap();
    assert_eq!(top[0], 2.0);
    assert_eq!(top[15], f64::NEG_INFINITY);
    let deep = deep_water(&s, &top, 1.5);
    assert!(deep[0] && !deep[15]);
    assert!(water_tops(4, 4, 0.8, 0.0, 0.0, &v, &[[0, 1, 5]]).is_err());
    // a negative index counts from the end, as NumPy's
    assert_eq!(water_tops(4, 4, 0.8, 0.0, 0.0, &v, &[[0, 1, -1]]).unwrap(), top);
    let out = outside_bounds(&s, [0.8, 0.8, 1.6, 1.6]);
    assert_eq!(out.iter().filter(|&&o| !o).count(), 4);
}

#[test]
fn edits_later_operations_win() {
    let s = flat(4, 1, |i, _| i as f64);
    let ops = vec![
        (true, Region::Box([-1.0, -1.0, -1.0, 9.0, 9.0, 9.0])),
        (false, Region::Box([1.0, 0.0, -1.0, 1.7, 9.0, 1.0])),
    ];
    let (fw, fu) = edit_masks(&s, &ops).unwrap();
    assert_eq!(fw, vec![true, true, false, true]);
    assert_eq!(fu, vec![false, false, true, false]);
    let poly = Region::Polygon {
        y: (-1.0, 9.0),
        poly: vec![(-0.4, -0.4), (1.2, -0.4), (1.2, 0.4), (-0.4, 0.4)],
    };
    assert_eq!(region_mask(&s, &poly).unwrap(), vec![true, true, false, false]);
    assert!(
        region_mask(
            &s,
            &Region::Polygon {
                y: (0.0, 1.0),
                poly: vec![]
            }
        )
        .is_err()
    );
}

#[test]
fn coarse_keeps_every_fourth_column() {
    let mut s = flat(9, 5, |i, j| (i + 10 * j) as f64);
    s.pitch = 0.2;
    let c = s.coarse(0.8).unwrap();
    assert_eq!((c.nx, c.nz, c.pitch), (3, 2, 0.8));
    assert_eq!(&c.y[..], &[0.0, 4.0, 8.0, 40.0, 44.0, 48.0]);
}

#[test]
fn a_flat_field_is_kept_from_its_edge_seeds() {
    // 130 columns along x reach the sector line at x = 1.6 (column 2) and x = 101.6 (column 127)
    let s = flat(130, 3, |_, _| 0.0);
    let inp = WalkInputs {
        step: 0.28,
        stairs: true,
        forbid_tol: 0.08,
        ..WalkInputs::default()
    };
    let w = walkable(&s, &inp, &settings()).unwrap();
    assert!(w.keep.iter().all(|&k| k));
    // a wall cuts the far side off: no seed there
    let wall = flat(130, 3, |i, _| if i < 5 { 0.0 } else { 3.0 });
    let mut s2 = wall;
    s2.flags = (0..390).map(|c| u8::from(c % 130 == 4)).collect();
    let w2 = walkable(&s2, &inp, &settings()).unwrap();
    assert!(w2.keep[0] && !w2.keep[4] && w2.keep[129]);
    let (gx, _, stair) = w2.gradients();
    assert!(matches!(gx, Floats::F64(_)) && stair.iter().all(|&v| !v));
    let unseed = WalkInputs {
        unseeds: vec![(0.0, 0.0, 0.0)],
        ..inp
    };
    let w3 = walkable(&s2, &unseed, &settings()).unwrap();
    assert!(!w3.keep[0] && w3.keep[129]);
}

#[test]
fn a_file_read_back() {
    let h = navsample::Header {
        pitch: 0.8,
        min_x: -1.6,
        min_z: 0.0,
        nx: 2,
        nz: 1,
        y_top: 10.0,
        y_bottom: -10.0,
        radius: 0.4,
        cap_bottom: 0.28,
        cap_top: 1.9,
        ray_radius: 0.05,
    };
    let start: Vec<u8> = [0i64, 1, 3].iter().flat_map(|x| x.to_le_bytes()).collect();
    let y: Vec<u8> = [1.5f32, 0.0, 4.0].iter().flat_map(|x| x.to_le_bytes()).collect();
    let normal: Vec<u8> = [0.0f32, 1.0, 0.0]
        .repeat(3)
        .iter()
        .flat_map(|x| x.to_le_bytes())
        .collect();
    let b = navsample::write_v3(
        &h,
        &navsample::Columns {
            start: &start,
            y: &y,
            normal: &normal,
            flags: &[0, 1, 2],
            hit: None,
        },
    )
    .unwrap();
    let (s, read) = Samples::parse(&b).unwrap();
    assert_eq!((read, s.nx, s.nz, s.len(), s.min_x), (2, 2, 1, 3, f64::from(-1.6f32)));
    assert_eq!(&s.start[..], &[0, 1, 3]);
    assert_eq!(s.hit.as_ref().map(Vec::len), Some(3));
    assert!(Samples::parse(&b[..b.len() - 2]).is_err());
}
