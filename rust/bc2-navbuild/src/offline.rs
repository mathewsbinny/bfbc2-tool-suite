//! NavKit's offline sampler (`bfbc2-auto-navkit/tools/navsample_offline.py`, round E4e): the capsule's terrain test,
//! `terrain_blocks` - half of a 0.2 m run's time (`sp_009`'s detailed sectors: `terrain_at` 18.5 of 39 s, profiled).
//!
//! **Bit for bit.** `terrain_at`'s arithmetic as NumPy does it: `u = (x - min_x) / cell_x` in `float64`, the square
//! `i = clip(floor(u), 0, n - 2)`, `fu = u - i`; the corner heights `float32`, their differences `(b - a)` in
//! `float32`, then promoted (exactly) to `float64` for `a + (b - a) * fu + (d - b) * fw`; outside the field a NaN.
//! The test `nanmax(h - (y + lift)) > 1e-4` over the ring points is "any ring point's `h - (y + lift)` is above
//! `1e-4`" (NaNs left out, as the reference's `-inf` leaves them).

use rayon::prelude::*;

/// A terrain height field as `terrain.Heightfield` holds it.
#[derive(Debug, Clone, Copy)]
pub struct Field<'a> {
    /// `(size, size)` heights, row `j` (z) then column `i` (x), `float32`.
    pub heights: &'a [f32],
    /// Samples per side.
    pub size: usize,
    /// `min[0]`, `min[2]`.
    pub min: (f64, f64),
    /// `cell`: metres per sample along x and z.
    pub cell: (f64, f64),
}

impl Field<'_> {
    /// `navsample_offline.terrain_at(hf, x, z, normals=False)`'s height (NaN outside the field).
    #[must_use]
    #[allow(clippy::many_single_char_names, reason = "the reference's names")]
    pub fn height(&self, x: f64, z: f64) -> f64 {
        let n = self.size as i64;
        let u = (x - self.min.0) / self.cell.0;
        let w = (z - self.min.1) / self.cell.1;
        let last = (n - 1) as f64;
        if u < 0.0 || w < 0.0 || u > last || w > last || u.is_nan() || w.is_nan() {
            return f64::NAN;
        }
        let i = (u.floor() as i64).clamp(0, n - 2);
        let j = (w.floor() as i64).clamp(0, n - 2);
        let (fu, fw) = (u - i as f64, w - j as f64);
        let at = |jj: i64, ii: i64| self.heights.get((jj * n + ii) as usize).copied().unwrap_or(f32::NAN);
        let (a, b, c, d) = (at(j, i), at(j, i + 1), at(j + 1, i), at(j + 1, i + 1));
        if fu > fw {
            f64::from(a) + f64::from(b - a) * fu + f64::from(d - b) * fw
        } else {
            f64::from(a) + f64::from(c - a) * fw + f64::from(d - c) * fu
        }
    }
}

/// `navsample_offline.terrain_blocks`' test for every surface `(x, z, y)`: does the terrain under any ring point
/// `(x + ring_x[k], z + ring_z[k])` rise more than `1e-4` above `y + lift[k]`? One answer per surface, on all cores.
/// `None` when the arrays do not fit together.
#[must_use]
pub fn terrain_blocks(
    field: &Field<'_>,
    pts: &[(f64, f64, f64)],
    ring_x: &[f64],
    ring_z: &[f64],
    lift: &[f64],
) -> Option<Vec<bool>> {
    if field.size < 2
        || field.heights.len() != field.size.checked_mul(field.size)?
        || ring_x.len() != lift.len()
        || ring_z.len() != lift.len()
    {
        return None;
    }
    Some(
        pts.par_iter()
            .map(|&(x, z, y)| {
                ring_x.iter().zip(ring_z).zip(lift).any(|((&rx, &rz), &l)| {
                    let h = field.height(x + rx, z + rz);
                    !h.is_nan() && h - (y + l) > 1e-4
                })
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_slope_and_its_edge() {
        // 3 x 3 samples, 1 m apart from (0, 0): heights rise 1 m per sample along x
        let h = [0.0f32, 1.0, 2.0, 0.0, 1.0, 2.0, 0.0, 1.0, 2.0];
        let f = Field {
            heights: &h,
            size: 3,
            min: (0.0, 0.0),
            cell: (1.0, 1.0),
        };
        assert!((f.height(0.5, 0.25) - 0.5).abs() < 1e-12);
        assert!(f.height(-0.1, 0.0).is_nan());
        assert!((f.height(2.0, 2.0) - 2.0).abs() < 1e-12);
        let got = terrain_blocks(
            &f,
            &[(0.5, 1.0, 0.0), (0.5, 1.0, 5.0), (-9.0, 1.0, 0.0)],
            &[0.0, 1.0],
            &[0.0, 0.0],
            &[0.0, 0.0],
        );
        assert_eq!(got, Some(vec![true, false, false]));
        assert!(terrain_blocks(&f, &[], &[0.0], &[], &[0.0]).is_none());
    }
}
