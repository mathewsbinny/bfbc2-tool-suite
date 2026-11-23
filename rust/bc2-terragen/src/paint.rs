//! Material painting (TGEN-MAT-2): each material-map cell's index from the height field's slope and height and the
//! biome's names (`Biome::materials`: 0 `defaultmaterial`, 1 the ground, 2 the steep material, 3 under water, 4 the
//! slope material). The rules split ground as the donor's rule-only patch shaders split it when they draw it
//! (TGEN-VIS-1): sea bed under the water, rock over 35 degrees, gravel or dirt from 25 to 35, else the biome's
//! ground; so what a soldier walks on and what is drawn agree.

use rayon::prelude::*;

use crate::shape::slopes;

/// The index of the biome's ground.
pub const GROUND: u8 = 1;
/// The index of the steep material (over [`STEEP_DEG`]).
pub const STEEP: u8 = 2;
/// The index of what lies under the water.
pub const UNDER_WATER: u8 = 3;
/// The index of the slope material ([`SLOPE_DEG`] to [`STEEP_DEG`]).
pub const SLOPE: u8 = 4;
/// Steeper than this (degrees): the steep material.
pub const STEEP_DEG: f64 = 35.0;
/// Steeper than this (degrees): the slope material.
pub const SLOPE_DEG: f64 = 25.0;

/// The material cells of `n` x `n` heights in metres (rows along z, `cell` metres apart) on a map of `width` cells a
/// side (TERR-MM-3: the map covers the block, `width` a multiple of `n` or `n` a multiple of `width`), rows along z.
#[must_use]
pub fn paint(heights: &[f32], n: usize, cell: f64, water_y: Option<f64>, width: usize) -> Vec<u8> {
    let h: Vec<f64> = heights.iter().map(|v| f64::from(*v)).collect();
    let s = slopes(&h, n, cell);
    let idx: Vec<u8> = s
        .par_iter()
        .zip(&h)
        .map(|(deg, y)| {
            if water_y.is_some_and(|w| *y < w) {
                UNDER_WATER
            } else if *deg > STEEP_DEG {
                STEEP
            } else if *deg > SLOPE_DEG {
                SLOPE
            } else {
                GROUND
            }
        })
        .collect();
    let mut out = vec![0u8; width * width];
    out.par_chunks_mut(width).enumerate().for_each(|(r, row)| {
        let j = r * n / width;
        for (c, x) in row.iter_mut().enumerate() {
            *x = idx.get(j * n + c * n / width).copied().unwrap_or(GROUND);
        }
    });
    out
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn rules_by_slope_and_water() {
        // 4 x 4: a flat left half at 0 m, a ramp of 1 m per metre on the right (45 degrees)
        let n = 4;
        let h: Vec<f32> = (0..16)
            .map(|k| if k % 4 < 2 { 0.0 } else { (k % 4) as f32 * 1.0 })
            .collect();
        let cells = paint(&h, n, 1.0, Some(-1.0), 8);
        assert_eq!(cells.len(), 64);
        assert_eq!(cells[0], GROUND);
        assert_eq!(cells[7], STEEP);
        assert_eq!(cells[1], cells[0]);
        let wet = paint(&h, n, 1.0, Some(0.5), 4);
        assert_eq!(wet[0], UNDER_WATER);
    }
}
