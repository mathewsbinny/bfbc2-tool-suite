//! The sub-terrain ring (TGEN-RING-1, round TG2): twelve blocks of 64 x 64 samples around the main block, each half
//! the main block's side, covering the square twice the main block's side minus the main block - DICE's layout on
//! all 12 of its single-ring terrains (`Heightfield-001` to `-012`, TERR-HF-5, TEDIT-BLK-1).
//!
//! The ring's heights continue the main block's border outwards and blend over a quarter of the main block's side
//! into distant hills from the seed (a smooth spectral field around the border's median height, its spread the
//! main block's own). Then DICE's stitching, measured on its 12 single-ring terrains (`tests/ring.rs` checks each):
//!
//! - a ring block's row or column that touches the main block is the main block's edge samples every `st` samples
//!   (`st` = the main block's samples per ring sample: 8 at 1,024, 16 at 2,048);
//! - the main block's four edges are straight between those knots (within half a stored unit), and each ring
//!   block's last knot along an edge repeats the next block's first (so the main edge is flat over that span);
//! - a ring block's last column and last row are its neighbour's first column and row (the neighbour a ring block or
//!   the main block's knots).

use rayon::prelude::*;

use crate::generate::{Generated, spectral};
use crate::rng::Stream;
use crate::shape::{median, percentile_sorted};

/// Samples a side of a ring block (DICE's on all 12 single-ring terrains).
pub const RING_N: usize = 64;

/// The ring blocks' places, in units of half the main block's side from its min corner (-1, -1), in DICE's order
/// (`-001` .. `-012`; `BC1_Oasis`, `MP_001`, `MP_006`, `MP_007` ...).
pub const PLACES: [(i32, i32); 12] = [
    (-2, -2),
    (-1, -2),
    (-2, -1),
    (0, -2),
    (1, -2),
    (1, -1),
    (-2, 0),
    (-2, 1),
    (-1, 1),
    (1, 0),
    (0, 1),
    (1, 1),
];

/// One ring block.
#[derive(Debug, Clone, PartialEq)]
pub struct RingBlock {
    /// Bounds: min x, y, z, max x, y, z (y: 0 and the main block's range).
    pub bounds: [f32; 6],
    /// [`RING_N`] x [`RING_N`] stored heights, rows along z.
    pub raw: Vec<u16>,
}

/// Where the blend from the border into the distant hills is complete, as a share of the main block's side.
const BLEND: f64 = 0.25;

fn smooth(x: f64) -> f64 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// The ring of `g` (its main block's edges stitched to it in place), from `seed`.
#[must_use]
pub fn ring(g: &mut Generated, seed: u64) -> Vec<RingBlock> {
    let n = g.size as usize;
    let half = n / 2;
    let st = half / RING_N;
    let scale = g.range_m / 65535.0;
    let h: Vec<f64> = g.raw.par_iter().map(|v| f64::from(*v) * scale).collect();
    // the border ring's median and the main block's spread (p10 - p90) for the distant hills
    let ring_w = (n / 16).max(1);
    let mut edge = Vec::new();
    for j in 0..n {
        for i in 0..n {
            if i < ring_w || j < ring_w || i >= n - ring_w || j >= n - ring_w {
                edge.push(h.get(j * n + i).copied().unwrap_or(0.0));
            }
        }
    }
    let base = median(edge);
    let mut sorted = h.clone();
    sorted.par_sort_unstable_by(f64::total_cmp);
    let spread = (percentile_sorted(&sorted, 90.0) - percentile_sorted(&sorted, 10.0)).max(1.0);
    // the distant field: 4 x 64 samples over the outer square (twice the main block's side), smooth (beta 4)
    let m = 4 * RING_N;
    let far = spectral(m, 4.0, Stream::new(seed, 4));
    let side = n as f64; // metres: 1 m samples
    let at_main = |x: f64, z: f64| -> f64 {
        let i = (x + side / 2.0).round().clamp(0.0, side - 1.0) as usize;
        let j = (z + side / 2.0).round().clamp(0.0, side - 1.0) as usize;
        h.get(j * n + i).copied().unwrap_or(base)
    };
    let world = |x: f64, z: f64| -> f64 {
        let d = (x.abs() - side / 2.0).max(z.abs() - side / 2.0).max(0.0);
        let b = smooth(d / (BLEND * side));
        let fi = ((x + side) / (2.0 * side) * m as f64)
            .floor()
            .clamp(0.0, m as f64 - 1.0) as usize;
        let fj = ((z + side) / (2.0 * side) * m as f64)
            .floor()
            .clamp(0.0, m as f64 - 1.0) as usize;
        let hills = base + 0.4 * spread * far.get(fj * m + fi).copied().unwrap_or(0.0);
        let v = at_main(x, z) * (1.0 - b) + hills * b;
        v.clamp(0.0, g.range_m)
    };
    let to_raw = |v: f64| (v / g.range_m * 65535.0).round().clamp(0.0, 65535.0) as u16;
    let bside = side / 2.0;
    let sp = bside / RING_N as f64;
    let mut blocks: Vec<Vec<u16>> = PLACES
        .iter()
        .map(|&(px, pz)| {
            let (x0, z0) = (f64::from(px) * bside, f64::from(pz) * bside);
            (0..RING_N * RING_N)
                .map(|k| world(x0 + (k % RING_N) as f64 * sp, z0 + (k / RING_N) as f64 * sp))
                .map(to_raw)
                .collect()
        })
        .collect();
    stitch(&mut g.raw, n, st, &mut blocks);
    PLACES
        .iter()
        .zip(blocks)
        .map(|(&(px, pz), raw)| {
            let (x0, z0) = (f64::from(px) * bside, f64::from(pz) * bside);
            RingBlock {
                bounds: [
                    x0 as f32,
                    0.0,
                    z0 as f32,
                    (x0 + bside) as f32,
                    g.range_m as f32,
                    (z0 + bside) as f32,
                ],
                raw,
            }
        })
        .collect()
}

/// The main block's edge: `side` 0 row 0, 1 the last row, 2 column 0, 3 the last column; sample `k` along it.
fn edge_index(n: usize, side: u8, k: usize) -> usize {
    match side {
        0 => k,
        1 => (n - 1) * n + k,
        2 => k * n,
        _ => k * n + n - 1,
    }
}

/// DICE's stitching (the module's list): the main block's edges straightened between the ring's knots, the ring's
/// rows and columns that touch the main block made its knots, each ring block's last column and row its
/// neighbour's first.
fn stitch(main: &mut [u16], n: usize, st: usize, blocks: &mut [Vec<u16>]) {
    let half = n / 2;
    let rn = RING_N;
    // 1. the main block's edges: knots every st samples, each block span's last knot the next span's first, the last
    //    knot the corner; straight between knots, flat after the last
    for side in 0..4u8 {
        let k = n / st;
        let mut knots: Vec<f64> = (0..k)
            .map(|q| f64::from(main.get(edge_index(n, side, q * st)).copied().unwrap_or(0)))
            .collect();
        for q in 0..k {
            if (q + 1) % rn == 0 {
                let next = if q + 1 < k {
                    knots.get(q + 1).copied()
                } else {
                    main.get(edge_index(n, side, n - 1)).map(|v| f64::from(*v))
                };
                if let (Some(v), Some(slot)) = (next, knots.get_mut(q)) {
                    *slot = v;
                }
            }
        }
        for s in 0..n {
            let q = s / st;
            let f = (s % st) as f64 / st as f64;
            let a = knots.get(q).copied().unwrap_or(0.0);
            let b = knots.get(q + 1).copied().unwrap_or(a);
            if let Some(v) = main.get_mut(edge_index(n, side, s)) {
                *v = (a + (b - a) * f).round() as u16;
            }
        }
    }
    let knot = |main: &[u16], side: u8, from: usize, i: usize| -> u16 {
        main.get(edge_index(n, side, from + i * st)).copied().unwrap_or(0)
    };
    // 2. the ring's rows and columns that touch the main block
    for (b, &(px, pz)) in blocks.iter_mut().zip(PLACES.iter()) {
        let off = |p: i32| if p == 0 { half } else { 0 };
        for i in 0..rn {
            let (cell, v) = match (px, pz) {
                (-1 | 0, -2) => ((rn - 1) * rn + i, knot(main, 0, off(px), i)), // below: its last row
                (-1 | 0, 1) => (i, knot(main, 1, off(px), i)),                  // above: its first row
                (-2, -1 | 0) => (i * rn + rn - 1, knot(main, 2, off(pz), i)),   // left: its last column
                (1, -1 | 0) => (i * rn, knot(main, 3, off(pz), i)),             // right: its first column
                _ => continue,
            };
            if let Some(c) = b.get_mut(cell) {
                *c = v;
            }
        }
    }
    // 3. each block's last column, then last row: its neighbour's first (a ring block's, or the main block's knots)
    let find = |px: i32, pz: i32| PLACES.iter().position(|&p| p == (px, pz));
    for pass in 0..2 {
        for k in 0..blocks.len() {
            let Some(&(px, pz)) = PLACES.get(k) else { continue };
            let (nx, nz) = if pass == 0 { (px + 1, pz) } else { (px, pz + 1) };
            let line: Option<Vec<u16>> = if let Some(o) = find(nx, nz) {
                blocks.get(o).map(|ob| {
                    (0..rn)
                        .map(|i| ob.get(if pass == 0 { i * rn } else { i }).copied().unwrap_or(0))
                        .collect()
                })
            } else if (-1..=0).contains(&nx) && (-1..=0).contains(&nz) {
                // the neighbour is the main block: its column 0 (from the left) or row 0 (from below)
                let off = |p: i32| if p == 0 { half } else { 0 };
                Some(if pass == 0 {
                    (0..rn).map(|i| knot(main, 2, off(pz), i)).collect()
                } else {
                    (0..rn).map(|i| knot(main, 0, off(px), i)).collect()
                })
            } else {
                None
            };
            let (Some(line), Some(b)) = (line, blocks.get_mut(k)) else {
                continue;
            };
            for (i, v) in line.into_iter().enumerate() {
                let cell = if pass == 0 { i * rn + rn - 1 } else { (rn - 1) * rn + i };
                if let Some(c) = b.get_mut(cell) {
                    *c = v;
                }
            }
        }
    }
}

/// DICE's stitching rules checked on a main block and its 12 ring blocks (in [`PLACES`] order), each as (holds,
/// checked): the ring's rows and columns that touch the main block equal its knots; each ring block's last column
/// and row equal its neighbour's first; and the main block's edges' worst distance (stored units) from the straight
/// lines between the knots (flat after the last).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stitching {
    /// Ring samples that touch the main block and equal its knot.
    pub main_seams: (usize, usize),
    /// Ring blocks' last-column and last-row samples equal to their neighbour's first.
    pub ring_seams: (usize, usize),
    /// The main edges' worst distance from the knots' lines, in stored units (DICE's: 0.5 or less, rounding).
    pub straight: f64,
}

/// Checks DICE's stitching ([`Stitching`]) on a main block of `n` samples and its ring.
#[must_use]
pub fn stitching(main: &[u16], n: usize, blocks: &[Vec<u16>]) -> Stitching {
    let rn = RING_N;
    let half = n / 2;
    let st = (half / rn).max(1);
    let mut out = Stitching::default();
    let knot = |side: u8, from: usize, i: usize| main.get(edge_index(n, side, from + i * st)).copied();
    let off = |p: i32| if p == 0 { half } else { 0 };
    for (b, &(px, pz)) in blocks.iter().zip(PLACES.iter()) {
        for i in 0..rn {
            let (cell, want) = match (px, pz) {
                (-1 | 0, -2) => ((rn - 1) * rn + i, knot(0, off(px), i)),
                (-1 | 0, 1) => (i, knot(1, off(px), i)),
                (-2, -1 | 0) => (i * rn + rn - 1, knot(2, off(pz), i)),
                (1, -1 | 0) => (i * rn, knot(3, off(pz), i)),
                _ => continue,
            };
            out.main_seams.1 += 1;
            out.main_seams.0 += usize::from(b.get(cell).copied() == want);
        }
        for (dx, dz) in [(1, 0), (0, 1)] {
            let Some(o) = PLACES.iter().position(|&p| p == (px + dx, pz + dz)) else {
                continue;
            };
            let Some(ob) = blocks.get(o) else { continue };
            for i in 0..rn {
                let (mine, theirs) = if dx == 1 {
                    (i * rn + rn - 1, i * rn)
                } else {
                    ((rn - 1) * rn + i, i)
                };
                out.ring_seams.1 += 1;
                out.ring_seams.0 += usize::from(b.get(mine) == ob.get(theirs));
            }
        }
    }
    for side in 0..4u8 {
        for s in 0..n {
            let (q, f) = (s / st, (s % st) as f64 / st as f64);
            let a = main.get(edge_index(n, side, q * st)).map_or(0.0, |v| f64::from(*v));
            let b = if (q + 1) * st < n {
                main.get(edge_index(n, side, (q + 1) * st)).map_or(a, |v| f64::from(*v))
            } else {
                a
            };
            let v = main.get(edge_index(n, side, s)).map_or(0.0, |v| f64::from(*v));
            out.straight = out.straight.max((v - (a + (b - a) * f)).abs());
        }
    }
    out
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_places_tile_the_ring() {
        let mut seen = std::collections::BTreeSet::new();
        for &(x, z) in &PLACES {
            assert!((-2..=1).contains(&x) && (-2..=1).contains(&z));
            assert!(
                !((-1..=0).contains(&x) && (-1..=0).contains(&z)),
                "inside the main block"
            );
            assert!(seen.insert((x, z)));
        }
        assert_eq!(seen.len(), 12);
    }
}
