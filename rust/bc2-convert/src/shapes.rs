//! `bfbc2-model-tools/shapes.py`'s NumPy work in Rust (round PR1b-2-rest2): the matrix products of `Geo.moved`, the
//! per-vertex tangent frames of `Geo.tangents` and the triangle winding tests of `_facing`, `outward` and
//! `fix_winding`. Project OPFOR's WW2 weapon tools build their parts with them, so the values are the NumPy
//! reference's, bit for bit: the products as OpenBLAS computes them on the machine that recorded the WW2 oracle
//! (spec rule XWW2-MOD-1, [`crate::decimate`]), the rest NumPy's own elementwise operations in its order.
//!
//! | NumPy | here |
//! |---|---|
//! | `(n, 3) @ (3, 3)` for n >= 2 (`dgemm`) | each element `fma(a2, b2, fma(a1, b1, a0 b0))` |
//! | `(1, 3) @ (3, 3)` (`dgemv`) | `fma(a2, b2, fma(a0, b0, a1 b1))` |
//! | `a @ b` of two 3-vectors (`ddot`) | [`crate::decimate::ddot`] |
//! | `np.linalg.norm(x, axis=1)`, `np.sum(x, axis=1)` of 3 columns | `((0 + x0) + x1) + x2` (NumPy's pairwise sum under 8 elements) |
//!
//! The solids' builders (`lathe`, `extrude`, `_triangulate`) stay Python: plain Python floats, no BLAS.

#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::doc_markdown,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::needless_range_loop,
    clippy::single_match_else,
    reason = "the NumPy reference's names (a, b, c, n, p, q, t, u, v, pos / ppos) kept for review against it; rows \n              indexed by their own ranges, vertex indexes checked on entry, bones as NumPy's float rows"
)]

use crate::decimate::ddot;

type V3 = [f64; 3];

/// `points @ m` (rows of 3 by a 3 x 3 matrix) as NumPy's OpenBLAS computes it on the reference machine (added to
/// the zeroed output: a zero is +0).
#[must_use]
pub fn matmul(points: &[V3], m: &[V3; 3]) -> Vec<V3> {
    if points.len() == 1 {
        let a = points.first().copied().unwrap_or_default();
        return vec![std::array::from_fn(|j| {
            a[2].mul_add(m[2][j], a[0].mul_add(m[0][j], a[1] * m[1][j])) + 0.0
        })];
    }
    points
        .iter()
        .map(|a| std::array::from_fn(|j| a[2].mul_add(m[2][j], a[1].mul_add(m[1][j], a[0] * m[0][j])) + 0.0))
        .collect()
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `np.cross`.
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// `np.sum(x * y, axis=1)` of one row: NumPy's sum of three, in order, from +0 (so three -0 products sum to +0).
fn sum3(a: V3, b: V3) -> f64 {
    ((0.0 + a[0] * b[0]) + a[1] * b[1]) + a[2] * b[2]
}

/// `np.linalg.norm(x, axis=1)` of one row.
fn norm_row(a: V3) -> f64 {
    sum3(a, a).sqrt()
}

/// Why a mesh was refused.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("index {0} is out of bounds for the {1} vertices")]
pub struct IndexError(pub i64, pub usize);

fn check(tris: &[[i64; 3]], n: usize) -> Result<Vec<[usize; 3]>, IndexError> {
    tris.iter()
        .map(|t| {
            let mut out = [0usize; 3];
            for (o, &w) in out.iter_mut().zip(t) {
                *o = usize::try_from(w).ok().filter(|&w| w < n).ok_or(IndexError(w, n))?;
            }
            Ok(out)
        })
        .collect()
}

/// `Geo.tangents()`: (tangent, binormal) per vertex.
///
/// # Errors
/// A triangle naming a vertex that is not there.
#[allow(
    clippy::indexing_slicing,
    reason = "vertex indexes checked on entry; rows of 3 indexed by 0..3"
)]
pub fn tangents(pos: &[V3], nrm: &[V3], uv: &[[f64; 2]], tris: &[[i64; 3]]) -> Result<(Vec<V3>, Vec<V3>), IndexError> {
    let n = pos.len();
    let tris = check(tris, n.min(nrm.len()).min(uv.len()))?;
    let mut t = vec![[0.0; 3]; n];
    let mut bv = vec![[0.0; 3]; n];
    for &[a, b, c] in &tris {
        let (e1, e2) = (sub(pos[b], pos[a]), sub(pos[c], pos[a]));
        let d1 = [uv[b][0] - uv[a][0], uv[b][1] - uv[a][1]];
        let d2 = [uv[c][0] - uv[a][0], uv[c][1] - uv[a][1]];
        let r = d1[0] * d2[1] - d2[0] * d1[1];
        if r.abs() < 1e-12 {
            continue;
        }
        let dt: V3 = std::array::from_fn(|k| (e1[k] * d2[1] - e2[k] * d1[1]) / r);
        let db: V3 = std::array::from_fn(|k| (e2[k] * d1[0] - e1[k] * d2[0]) / r);
        // t[[a, b, c]] += x: each vertex once, a repeated one too
        let mut seen: Vec<usize> = Vec::with_capacity(3);
        for w in [a, b, c] {
            if seen.contains(&w) {
                continue;
            }
            seen.push(w);
            for k in 0..3 {
                t[w][k] += dt[k];
                bv[w][k] += db[k];
            }
        }
    }
    let nn: Vec<V3> = nrm
        .iter()
        .take(n)
        .map(|v| {
            let l = norm_row(*v).max(1e-12);
            [v[0] / l, v[1] / l, v[2] / l]
        })
        .collect();
    let mut b = Vec::with_capacity(n);
    for (i, ti) in t.iter_mut().enumerate() {
        let ni = nn[i];
        let s = sum3(*ti, ni);
        *ti = std::array::from_fn(|k| ti[k] - ni[k] * s);
        if norm_row(*ti) < 1e-9 {
            // no usable texture gradient: any direction across the normal
            let alt = if ni[0].abs() < 0.9 {
                [1.0, 0.0, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };
            *ti = cross(ni, alt);
        }
        let l = norm_row(*ti);
        *ti = [ti[0] / l, ti[1] / l, ti[2] / l];
        let c = cross(ni, *ti);
        let mut bi = [-c[0], -c[1], -c[2]];
        if sum3(bi, bv[i]) < 0.0 {
            bi = [-bi[0], -bi[1], -bi[2]]; // mirrored texture: +v runs the other way
        }
        b.push(bi);
    }
    Ok((t, b))
}

/// What a triangle's geometric normal is compared with to decide its winding.
#[derive(Debug, Clone, Copy)]
pub enum Facing {
    /// `_facing(g, normal)`: one direction.
    Normal(V3),
    /// `outward(g, inside_point)`: away from a point (the triangle's middle minus it).
    Outward(V3),
    /// `fix_winding(g)`: the sum of its corners' normals.
    Normals,
}

/// The triangles to turn over (`t[::-1]`): those whose `np.cross(b - a, c - a) @ v < 0`.
///
/// # Errors
/// A triangle naming a vertex that is not there.
#[allow(clippy::indexing_slicing, reason = "vertex indexes checked on entry")]
pub fn turn_over(pos: &[V3], nrm: &[V3], tris: &[[i64; 3]], facing: Facing) -> Result<Vec<bool>, IndexError> {
    let n = match facing {
        Facing::Normals => pos.len().min(nrm.len()),
        _ => pos.len(),
    };
    let tris = check(tris, n)?;
    Ok(tris
        .iter()
        .map(|&[a, b, c]| {
            let (pa, pb, pc) = (pos[a], pos[b], pos[c]);
            let v: V3 = match facing {
                Facing::Normal(v) => v,
                Facing::Outward(p) => std::array::from_fn(|k| ((pa[k] + pb[k]) + pc[k]) / 3.0 - p[k]),
                Facing::Normals => std::array::from_fn(|k| (nrm[a][k] + nrm[b][k]) + nrm[c][k]),
            };
            ddot(&cross(sub(pb, pa), sub(pc, pa)), &v) < 0.0
        })
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn products_and_windings() {
        let id = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        assert_eq!(matmul(&[[1.0, 2.0, 3.0]], &id), vec![[1.0, 2.0, 3.0]]);
        assert_eq!(matmul(&[[1.0, 2.0, 3.0], [4.0, 5.0, 6.0]], &id).len(), 2);
        let pos = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let up = turn_over(&pos, &[], &[[0, 1, 2]], Facing::Normal([0.0, 0.0, 1.0])).unwrap();
        let down = turn_over(&pos, &[], &[[0, 1, 2]], Facing::Normal([0.0, 0.0, -1.0])).unwrap();
        assert_eq!((up, down), (vec![false], vec![true]));
        assert!(turn_over(&pos, &[], &[[0, 1, 3]], Facing::Normals).is_err());
    }
}
