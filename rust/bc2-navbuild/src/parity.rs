//! NavKit's parity measures (`navdiff.area_cells`, round E3h): the sample cells inside an area's boundary, as the
//! NumPy reference computes them, value for value.

use crate::trace::P3;

/// `navdiff.area_cells`: the cells (i, j) of the grid of `pitch` whose sample point (nudged off the 45-degree
/// bridges: `+ pitch * 1.37e-3`, `+ pitch * 0.71e-3`) lies inside the boundary `segs` by the even-odd rule on the xz
/// plane, in the reference's order (i, then j), each with the height of the nearest segment end (the first of
/// equals, as `np.argmin`) minus `y_offset`. Empty for no segments, or a boundary whose extent is not finite.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]
pub fn area_cells(segs: &[(P3, P3)], pitch: f64, y_offset: f64) -> Vec<((i64, i64), f64)> {
    if segs.is_empty() {
        return Vec::new();
    }
    let ends = || segs.iter().flat_map(|&(a, b)| [a, b]);
    let fold = |f: fn(f64, f64) -> f64, init: f64, sel: fn(&P3) -> f64| ends().map(|p| sel(&p)).fold(init, f);
    let (xmin, xmax) = (
        fold(f64::min, f64::INFINITY, |p| p.0),
        fold(f64::max, f64::NEG_INFINITY, |p| p.0),
    );
    let (zmin, zmax) = (
        fold(f64::min, f64::INFINITY, |p| p.2),
        fold(f64::max, f64::NEG_INFINITY, |p| p.2),
    );
    if ends().any(|p| p.0.is_nan() || p.2.is_nan()) || ![xmin, xmax, zmin, zmax].iter().all(|v| v.is_finite()) {
        return Vec::new();
    }
    let lim = |v: f64| (v.is_finite() && v.abs() < 1e15).then_some(v as i64);
    let (Some(i0), Some(i1), Some(j0), Some(j1)) = (
        lim((xmin / pitch).floor()),
        lim((xmax / pitch).ceil()),
        lim((zmin / pitch).floor()),
        lim((zmax / pitch).ceil()),
    ) else {
        return Vec::new();
    };
    let (i0, i1, j0, j1) = (i0 - 1, i1 + 1, j0 - 1, j1 + 1);
    let pts: Vec<P3> = segs.iter().map(|s| s.0).chain(segs.iter().map(|s| s.1)).collect();
    let mut out = Vec::new();
    for i in i0..=i1 {
        let px = i as f64 * pitch + pitch * 1.37e-3;
        for j in j0..=j1 {
            let pz = j as f64 * pitch + pitch * 0.71e-3;
            let mut inside = false;
            for &(a, b) in segs {
                let (sax, saz, sbx, sbz) = (a.0, a.2, b.0, b.2);
                if (saz > pz) != (sbz > pz) {
                    let xint = sax + (pz - saz) * (sbx - sax) / (sbz - saz);
                    if px < xint {
                        inside = !inside;
                    }
                }
            }
            if !inside {
                continue;
            }
            let (sx, sz) = (i as f64 * pitch, j as f64 * pitch);
            let mut best = 0usize;
            let mut best_d = f64::NAN;
            for (n, p) in pts.iter().enumerate() {
                let (dx, dz) = (sx - p.0, sz - p.2);
                let d = dx * dx + dz * dz;
                // np.argmin: the first of the least; a NaN wins (the first one)
                if n == 0 || (!best_d.is_nan() && (d < best_d || d.is_nan())) {
                    best = n;
                    best_d = d;
                }
            }
            let y = pts.get(best).map_or(f64::NAN, |p| p.1);
            out.push(((i, j), y - y_offset));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::float_cmp)]
    use super::*;

    #[test]
    fn a_square() {
        // a 2 m square at height 1.95 (ground 1.0 with the offset 0.95): cells 1..2 on each axis at pitch 0.8
        let p = |x: f64, z: f64| (x, 1.95, z);
        let segs = [
            (p(0.5, 0.5), p(2.5, 0.5)),
            (p(2.5, 0.5), p(2.5, 2.5)),
            (p(2.5, 2.5), p(0.5, 2.5)),
            (p(0.5, 2.5), p(0.5, 0.5)),
        ];
        let got = area_cells(&segs, 0.8, 0.95);
        let ij: Vec<(i64, i64)> = got.iter().map(|c| c.0).collect();
        let want: Vec<(i64, i64)> = (1..=3).flat_map(|i| (1..=3).map(move |j| (i, j))).collect();
        assert_eq!(ij, want);
        assert!(got.iter().all(|c| (c.1 - 1.0).abs() < 1e-12));
        assert_eq!(area_cells(&[], 0.8, 0.95), []);
        assert_eq!(area_cells(&[(p(f64::NAN, 0.0), p(1.0, 1.0))], 0.8, 0.95), []);
    }
}
