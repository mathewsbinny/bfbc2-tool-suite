//! NavKit's vehicle path data (`bfbc2-auto-navkit/tools/navvehicle.py`, round E4e): the obstacle distance of every
//! candidate surface, the step that took 85 of `ONS_MP_002`'s 90 s.
//!
//! `navvehicle.obstacle_distance` works on bands of rows as fixed layers (every surface of a column a layer), shifts
//! the obstacle layers by each offset of the cylinder's reach in turn (nearest first) and keeps, per candidate
//! surface, the first offset at which a shifted obstacle rises more than `lo` and at most `hi` above the surface's
//! local ground plane. Here each candidate surface walks the offsets itself and stops at its first hit: the same
//! answer, as each surface's answer depends only on its own column's neighbours.
//!
//! **Bit for bit.** The reference computes in `float32` (NumPy 2, NEP 50: a Python float meeting a `float32` array
//! is cast to `float32`): the plane's height `(y + gx * f32(a * pitch)) + gz * f32(b * pitch)`, each product and sum
//! rounded to `float32`; the rise `obstacle - base` in `float32`; `lo` / `hi` compared as `float32`; the distance
//! stored as `f32(d)` of the Python float `d` it was given.

use rayon::prelude::*;

/// What an obstacle-distance run reads: the sample run's column starts and per-surface arrays.
#[derive(Debug, Clone, Copy)]
pub struct Surfaces<'a> {
    /// Columns along x.
    pub nx: usize,
    /// Columns along z.
    pub nz: usize,
    /// Per column (`nx * nz + 1`): the index of its first surface.
    pub start: &'a [usize],
    /// Per surface: its height (`float32`, as the samples hold it).
    pub y: &'a [f32],
    /// Per surface: the ground plane's rise along x per metre (`float32`).
    pub gx: &'a [f32],
    /// Per surface: the ground plane's rise along z per metre (`float32`).
    pub gz: &'a [f32],
}

/// The obstacles: which surfaces are solid (their height an obstacle) and, optionally, an extra obstacle height per
/// surface (NaN: none).
#[derive(Debug, Clone, Copy)]
pub struct Obstacles<'a> {
    /// Per surface: its height is an obstacle.
    pub solid: &'a [bool],
    /// Per surface: a further obstacle height (NaN for none).
    pub extra: Option<&'a [f32]>,
}

/// `navvehicle.obstacle_distance`: per surface, `f32(d)` of the first offset `(d, a, b)` (in the order given: the
/// reference sorts them by distance) at which an obstacle of column `(i + a, j + b)` rises above the surface's plane by
/// more than `lo` and at most `hi`; 99 where none does and for surfaces that are not candidates. `None` when the
/// arrays do not fit together (the caller then runs the reference, which raises).
#[must_use]
#[allow(clippy::many_single_char_names, reason = "the reference's names: s, n, i, j, k")]
pub fn obstacle_distance(
    s: &Surfaces<'_>,
    cand: &[bool],
    obstacles: &Obstacles<'_>,
    offsets: &[(f64, i64, i64)],
    pitch: f64,
    lo: f32,
    hi: f32,
) -> Option<Vec<f32>> {
    let n = s.y.len();
    let cols = s.nx.checked_mul(s.nz)?;
    if s.start.len() != cols + 1
        || s.gx.len() != n
        || s.gz.len() != n
        || cand.len() != n
        || obstacles.solid.len() != n
        || obstacles.extra.is_some_and(|e| e.len() != n)
        || s.start.windows(2).any(|w| w.first() > w.get(1))
        || s.start.last().copied() != Some(n)
    {
        return None;
    }
    // the plane's offsets as the reference's float32 scalars
    let steps: Vec<(f32, i64, i64, f32, f32)> = offsets
        .iter()
        .map(|&(d, a, b)| (d as f32, a, b, (a as f64 * pitch) as f32, (b as f64 * pitch) as f32))
        .collect();
    let (nx, nz) = (s.nx as i64, s.nz as i64);
    let mut out = vec![99.0f32; n];
    // column by column, in parallel: each column's surfaces are a contiguous run of `out`
    let mut runs: Vec<(usize, &mut [f32])> = Vec::with_capacity(cols);
    let mut rest: &mut [f32] = &mut out;
    for c in 0..cols {
        let len = s.start.get(c + 1)? - s.start.get(c)?;
        let (head, tail) = rest.split_at_mut(len);
        runs.push((c, head));
        rest = tail;
    }
    runs.into_par_iter().for_each(|(c, run)| {
        let (i, j) = ((c % s.nx) as i64, (c / s.nx) as i64);
        let first = s.start.get(c).copied().unwrap_or(0);
        for (r, slot) in run.iter_mut().enumerate() {
            let k = first + r;
            if !cand.get(k).copied().unwrap_or(false) {
                continue;
            }
            let (yk, gxk, gzk) = (
                s.y.get(k).copied().unwrap_or(f32::NAN),
                s.gx.get(k).copied().unwrap_or(0.0),
                s.gz.get(k).copied().unwrap_or(0.0),
            );
            for &(d, a, b, sa, sb) in &steps {
                let (ii, jj) = (i + a, j + b);
                if ii < 0 || jj < 0 || ii >= nx || jj >= nz {
                    continue;
                }
                let cc = (jj * nx + ii) as usize;
                let (m0, m1) = (
                    s.start.get(cc).copied().unwrap_or(0),
                    s.start.get(cc + 1).copied().unwrap_or(0),
                );
                let base = (yk + gxk * sa) + gzk * sb;
                let rises = |o: f32| {
                    let diff = o - base;
                    diff > lo && diff <= hi
                };
                let hit = (m0..m1).any(|m| {
                    (obstacles.solid.get(m).copied().unwrap_or(false) && rises(s.y.get(m).copied().unwrap_or(f32::NAN)))
                        || obstacles.extra.and_then(|e| e.get(m)).is_some_and(|&o| rises(o))
                });
                if hit {
                    *slot = d;
                    break;
                }
            }
        }
    });
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nearest_obstacle_in_reach() {
        // 3 x 1 columns, one surface each; the right column has a wall 1 m above the ground
        let start = [0usize, 1, 2, 3];
        let y = [0.0f32, 0.0, 1.0];
        let zero = [0.0f32; 3];
        let s = Surfaces {
            nx: 3,
            nz: 1,
            start: &start,
            y: &y,
            gx: &zero,
            gz: &zero,
        };
        let solid = [false, false, true];
        let offs = [(0.0, 0, 0), (1.0, 1, 0), (1.0, -1, 0), (2.0, 2, 0)];
        let got = obstacle_distance(
            &s,
            &[true, true, false],
            &Obstacles {
                solid: &solid,
                extra: None,
            },
            &offs,
            1.0,
            0.4,
            4.0,
        );
        assert_eq!(got, Some(vec![2.0, 1.0, 99.0]));
        // the extra height counts as an obstacle; a NaN extra does not
        let extra = [f32::NAN, 0.5, f32::NAN];
        let got = obstacle_distance(
            &s,
            &[true, false, false],
            &Obstacles {
                solid: &[false; 3],
                extra: Some(&extra),
            },
            &offs,
            1.0,
            0.4,
            4.0,
        );
        assert_eq!(got, Some(vec![1.0, 99.0, 99.0]));
        // arrays that do not fit: None
        assert!(
            obstacle_distance(
                &s,
                &[true],
                &Obstacles {
                    solid: &solid,
                    extra: None
                },
                &offs,
                1.0,
                0.4,
                4.0
            )
            .is_none()
        );
    }
}
