//! The terrain under a sampling hole (round NKS-deep D, XNAV-BLD-5): a column whose casts stopped inside an object
//! before they reached the ground gets the terrain's surface back from the level's height field.
//!
//! The in-game sampler (and the offline one, which copies it) casts down a column; after a hit the next cast starts
//! just under it, and a cast that starts inside geometry steps down 0.5 m at a time, at most 10 m (`navsample.md`).
//! Under a thick object - a cliff mesh's slab, a tall invisible wall - the column then ends on the object's top and
//! the ground below is never sampled. DICE's generator queries the collision world directly and keeps that ground:
//! on `sp_003_b` a forbidden rock's slab 34 m tall leaves a diagonal strip of 1-4 columns at 0.2 m without terrain, which
//! cuts DICE's one area into two and the route network with it.
//!
//! The rule: a column inside the height field with surfaces but none within [`NEAR`] of the terrain, whose lowest
//! surface lies more than [`ABOVE`] over it, gets a terrain surface at the field's height when, along an axis or a
//! diagonal, the first column on each side that is not such a hole lies within [`REACH`] and holds a terrain surface
//! within [`SIDE`] of its own field height (a narrow strip between sampled ground, never the floor under a building).
//! The new surface's normal is the field's, its hit words the terrain's, and its capsule flag blocked when the column's
//! lowest surface is closer above it than the capsule's top plus its radius.

#![allow(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "the grid's names (c a column, k a surface, h a height, x / z a position); one pass over the columns"
)]

use std::sync::Arc;

use bc2_formats::terrain::Heightfield;

use crate::walk::{Normals, Samples};

/// A surface within this of the field's height is the terrain's (m).
pub const NEAR: f64 = 0.5;
/// The column's lowest surface lies more than this over the field: the casts stopped above the ground (m).
pub const ABOVE: f64 = 1.0;
/// How far the sampled ground on each side may lie (m, plan).
pub const REACH: f64 = 1.2;
/// The sampled ground on each side: a terrain surface within this of its field height (m).
pub const SIDE: f64 = 0.3;

/// The field's slope: a central difference over this (m).
const E: f64 = 0.01;

const DIRS: [(i64, i64); 4] = [(1, 0), (0, 1), (1, 1), (1, -1)];

/// The run with the terrain filled into its sampling holes, and how many surfaces were added; `None` when nothing
/// was added. `vtable`: the terrain's shape class (`WalkSettings::terrain_vtable`).
#[must_use]
pub fn fill(s: &Samples, hf: &Heightfield, vtable: u32) -> Option<(Samples, usize)> {
    let (nx, nz) = (s.nx, s.nz);
    let cols = nx.checked_mul(nz)?;
    if s.start.len() != cols + 1 {
        return None;
    }
    let (x0, x1) = (f64::from(hf.min[0]), f64::from(hf.max[0]));
    let (z0, z1) = (f64::from(hf.min[2]), f64::from(hf.max[2]));
    let pos = |c: usize| (s.min_x + (c % nx) as f64 * s.pitch, s.min_z + (c / nx) as f64 * s.pitch);
    let range = |c: usize| s.start.get(c).copied().unwrap_or(0)..s.start.get(c + 1).copied().unwrap_or(0);
    let is_terrain = |k: usize| s.hit.as_ref().is_none_or(|h| h.get(k).is_some_and(|w| w[0] == vtable));
    let ground_of = |c: usize| {
        let (x, z) = pos(c);
        (x0..=x1).contains(&x) && (z0..=z1).contains(&z)
    };
    // per column: Some(field height) when it is a hole
    let hole: Vec<Option<f64>> = (0..cols)
        .map(|c| {
            let r = range(c);
            if r.is_empty() || !ground_of(c) {
                return None;
            }
            let (x, z) = pos(c);
            let h = hf.height(x, z);
            let low = r
                .clone()
                .map(|k| s.y.get(k).copied().unwrap_or(f64::NAN))
                .fold(f64::INFINITY, f64::min);
            let near = r.clone().any(|k| s.y.get(k).is_some_and(|&y| (y - h).abs() < NEAR));
            (h.is_finite() && !near && low > h + ABOVE).then_some(h)
        })
        .collect();
    let sampled = |c: usize| {
        let (x, z) = pos(c);
        let h = hf.height(x, z);
        range(c).any(|k| is_terrain(k) && s.y.get(k).is_some_and(|&y| (y - h).abs() < SIDE))
    };
    let steps = (REACH / s.pitch).floor().max(1.0) as i64;
    let side = |c: usize, (di, dj): (i64, i64)| -> bool {
        let (i, j) = ((c % nx) as i64, (c / nx) as i64);
        for t in 1..=steps {
            let (a, b) = (i + di * t, j + dj * t);
            if a < 0 || b < 0 || a >= nx as i64 || b >= nz as i64 {
                return false;
            }
            let o = b as usize * nx + a as usize;
            if hole.get(o).copied().flatten().is_some() {
                continue;
            }
            return sampled(o);
        }
        false
    };
    let add: Vec<Option<f64>> = (0..cols)
        .map(|c| {
            let h = hole.get(c).copied().flatten()?;
            DIRS.iter()
                .any(|&(di, dj)| side(c, (di, dj)) && side(c, (-di, -dj)))
                .then_some(h)
        })
        .collect();
    let added = add.iter().filter(|a| a.is_some()).count();
    if added == 0 {
        return None;
    }
    let reach_top = s.header.get(4).copied().unwrap_or(1.9) + s.header.get(2).copied().unwrap_or(0.4);
    let n = s.len() + added;
    let mut start = Vec::with_capacity(cols + 1);
    let mut y = Vec::with_capacity(n);
    let mut flags = Vec::with_capacity(n);
    let mut hit = s.hit.as_ref().map(|_| Vec::with_capacity(n));
    let mut nf32: Vec<[f32; 3]> = Vec::new();
    let mut nf64: Vec<[f64; 3]> = Vec::new();
    let f32n = matches!(s.normal, Normals::F32(_));
    start.push(0);
    for c in 0..cols {
        for k in range(c) {
            y.push(s.y.get(k).copied().unwrap_or(f64::NAN));
            flags.push(s.flags.get(k).copied().unwrap_or(0));
            if let (Some(h), Some(src)) = (hit.as_mut(), s.hit.as_ref()) {
                h.push(src.get(k).copied().unwrap_or([0; 3]));
            }
            match &s.normal {
                Normals::F32(v) => nf32.push(v.get(k).copied().unwrap_or([f32::NAN; 3])),
                Normals::F64(v) => nf64.push(v.get(k).copied().unwrap_or([f64::NAN; 3])),
            }
        }
        if let Some(Some(h)) = add.get(c) {
            let (x, z) = pos(c);
            let low = range(c)
                .map(|k| s.y.get(k).copied().unwrap_or(f64::NAN))
                .fold(f64::INFINITY, f64::min);
            let gx = (hf.height(x + E, z) - hf.height(x - E, z)) / (2.0 * E);
            let gz = (hf.height(x, z + E) - hf.height(x, z - E)) / (2.0 * E);
            let len = (gx * gx + 1.0 + gz * gz).sqrt();
            let nrm = [-gx / len, 1.0 / len, -gz / len];
            y.push(if s.y_f32 { f64::from(*h as f32) } else { *h });
            flags.push(u8::from(low < h + reach_top));
            if let Some(hh) = hit.as_mut() {
                hh.push([vtable, 0, 0]);
            }
            if f32n {
                nf32.push([nrm[0] as f32, nrm[1] as f32, nrm[2] as f32]);
            } else {
                nf64.push(nrm);
            }
        }
        start.push(y.len());
    }
    let normal = if f32n { Normals::F32(nf32) } else { Normals::F64(nf64) };
    Some((
        Samples {
            pitch: s.pitch,
            min_x: s.min_x,
            min_z: s.min_z,
            nx,
            nz,
            header: s.header,
            start: Arc::from(start),
            y: Arc::from(y),
            y_f32: s.y_f32,
            normal,
            flags,
            hit,
        },
        added,
    ))
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::indexing_slicing, reason = "tests")]
mod tests {
    use super::*;

    /// A flat field at `height`, 1 m squares from (0, 0).
    fn field(n: u32, height: f32) -> Heightfield {
        Heightfield {
            min: [0.0, height, 0.0],
            max: [n as f32, height + 100.0, n as f32],
            size: n,
            header: bc2_formats::terrain::HeaderWords::default(),
            raw: vec![0; (n * n) as usize],
            tail: bc2_formats::terrain::Tail::Unknown(Vec::new()),
        }
    }

    /// A 9 x 1 run at pitch 0.2 on flat terrain at 10 m: columns 3-4 hold only a slab's top at 40 m (`gap` wide).
    fn run(gap: std::ops::Range<usize>, slab: bool) -> Samples {
        let mut start = vec![0];
        let mut y = Vec::new();
        let mut hit = Vec::new();
        let mut normal = Vec::new();
        for c in 0..9 {
            if gap.contains(&c) {
                if slab {
                    y.push(40.0);
                    hit.push([7, 0, 0]);
                    normal.push([0.0f32, 1.0, 0.0]);
                }
            } else {
                y.push(10.0);
                hit.push([5, 0, 0]);
                normal.push([0.0f32, 1.0, 0.0]);
            }
            start.push(y.len());
        }
        let n = y.len();
        Samples {
            pitch: 0.2,
            min_x: 1.0,
            min_z: 1.0,
            nx: 9,
            nz: 1,
            header: [650.0, -20.0, 0.4, 0.28, 1.9, 0.05],
            start: Arc::from(start),
            y: Arc::from(y),
            y_f32: false,
            normal: Normals::F32(normal),
            flags: vec![0; n],
            hit: Some(hit),
        }
    }

    #[test]
    fn a_narrow_hole_under_a_slab_gets_the_terrain() {
        let hf = field(16, 10.0);
        let (s, added) = fill(&run(3..5, true), &hf, 5).expect("filled");
        assert_eq!(added, 2);
        s.check().expect("a whole run");
        for c in 3..5 {
            let r = s.start[c]..s.start[c + 1];
            assert_eq!(r.len(), 2, "the slab and the terrain");
            assert!((s.y[r.end - 1] - 10.0).abs() < 1e-6);
            assert_eq!(s.hit.as_ref().expect("hits")[r.end - 1][0], 5);
            assert_eq!(s.flags[r.end - 1], 0, "30 m of room above");
        }
    }

    #[test]
    fn wide_holes_and_empty_columns_are_left() {
        let hf = field(16, 10.0);
        // 8 columns of slab at 0.2 m: the sides lie 1.6 m apart, beyond REACH on one side
        assert!(fill(&run(1..9, true), &hf, 5).is_none());
        // no surface at all: nothing says the casts stopped above the ground
        assert!(fill(&run(3..5, false), &hf, 5).is_none());
    }
}
