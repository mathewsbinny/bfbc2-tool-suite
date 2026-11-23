//! Fewer triangles for a mesh: `bfbc2-model-tools/decimate.py`'s quadric edge collapses (Garland-Heckbert onto
//! existing vertices) in Rust (round PR1b-2-rest2). Project OPFOR's third-person weapon LODs are made with it (the
//! Type 38's and the Winchester 1897's), so they are WW2 rebuild oracle outputs: every value here is the NumPy
//! reference's, bit for bit.
//!
//! The reference's matrix products go through NumPy's BLAS, OpenBLAS, whose kernels pick their own order of
//! operations and use fused multiply-adds - and pick them by the processor (round CI1's lesson; spec rule
//! XWW2-MOD-1). The recorded LODs were made on the suite's reference machine (OpenBLAS 0.3.34, its `SkylakeX`
//! kernels: an AMD Zen 4), and this port writes those kernels' operations out, so it gives the recorded LODs on
//! any processor:
//!
//! | NumPy | OpenBLAS kernel | here |
//! |---|---|---|
//! | `np.dot(a, b)`, `np.linalg.norm(a)` of 3- and 4-vectors | `ddot`, under 16 elements: its scalar tail | [`ddot`]: `a0 b0`, then a fused multiply-add per element |
//! | `x @ M` (x a 4-vector, M 4 x 4, C order) | `dgemv` (row-major, transposed) | [`gemv`]: `y_j = fma(x3, M3j, fma(x2, M2j, fma(x0, M0j, x1 M1j)))` |
//! | `np.cross`, `np.outer`, `+`, `/` | (NumPy's own loops) | plain IEEE operations in the reference's order |
//!
//! Found by trial against NumPy on this machine (`rust/BENCHMARKS.md` "PR1b-2-rest2": 100,000 random quadric costs
//! equal) and proved on the meshes in `bfbc2-model-tools/tests/test_decimate.py` against the reference kept there.

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

use std::cmp::Ordering;
use std::collections::{BTreeSet, BinaryHeap, HashMap};

/// Why a mesh was not decimated.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecimateError {
    /// No vertices (NumPy's `key.max()` of an empty array).
    #[error("zero-size array to reduction operation maximum which has no identity")]
    Empty,
    /// A triangle names a vertex that is not there.
    #[error("index {0} is out of bounds for the {1} vertices")]
    Index(i64, usize),
    /// The arrays' lengths do not agree.
    #[error("{0}")]
    Shape(String),
}

type V3 = [f64; 3];

/// OpenBLAS' `ddot` for fewer than 16 elements (its kernel's scalar tail, compiled with fused multiply-adds): the
/// first product, then each next one fused onto the sum; the sum starts at +0, so a zero result is +0 (the `+ 0.0`).
#[must_use]
pub fn ddot(a: &[f64], b: &[f64]) -> f64 {
    let mut it = a.iter().zip(b);
    let Some((x, y)) = it.next() else { return 0.0 };
    let mut s = x * y;
    for (x, y) in it {
        s = x.mul_add(*y, s);
    }
    s + 0.0
}

/// `x @ m` for a 4-vector and a 4 x 4 matrix as OpenBLAS' `dgemv` computes it on the reference machine (added to the
/// zeroed output: a zero is +0).
#[must_use]
pub fn gemv(x: &[f64; 4], m: &[[f64; 4]; 4]) -> [f64; 4] {
    std::array::from_fn(|j| x[3].mul_add(m[3][j], x[2].mul_add(m[2][j], x[0].mul_add(m[0][j], x[1] * m[1][j]))) + 0.0)
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// `np.cross`: each component one product minus the other, both rounded.
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: V3) -> f64 {
    ddot(&a, &a).sqrt()
}

/// `np.outer(p, p) * s` (each product, then times `s`).
fn outer(p: &[f64; 4], s: f64) -> [[f64; 4]; 4] {
    std::array::from_fn(|i| std::array::from_fn(|j| (p[i] * p[j]) * s))
}

fn add_into(q: &mut [[f64; 4]; 4], o: &[[f64; 4]; 4]) {
    for (r, s) in q.iter_mut().zip(o) {
        for (a, b) in r.iter_mut().zip(s) {
            *a += b;
        }
    }
}

/// `_plane_quadric(a, b, c)`.
fn plane_quadric(a: V3, b: V3, c: V3) -> [[f64; 4]; 4] {
    let n = cross(sub(b, a), sub(c, a));
    let area = norm(n);
    if area < 1e-14 {
        return [[0.0; 4]; 4];
    }
    let n = [n[0] / area, n[1] / area, n[2] / area];
    let p = [n[0], n[1], n[2], -ddot(&n, &a)];
    outer(&p, area)
}

/// `np.round(x, 6)`: times 10^6, rounded half to even, divided.
fn round6(x: f64) -> f64 {
    (x * 1e6).round_ties_even() / 1e6
}

/// A heap entry: `(cost, u, v, version u, version v)`, Python's tuple order (the smallest first).
#[derive(Debug, Clone, Copy)]
struct Entry(f64, usize, usize, i64, i64);

impl PartialEq for Entry {
    fn eq(&self, o: &Self) -> bool {
        self.cmp(o) == Ordering::Equal
    }
}

impl Eq for Entry {}

impl PartialOrd for Entry {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Entry {
    /// Reversed (BinaryHeap is a max-heap): Python compares the costs as floats (-0 and 0 equal), then the ints.
    fn cmp(&self, o: &Self) -> Ordering {
        let c = self.0.partial_cmp(&o.0).unwrap_or_else(|| self.0.total_cmp(&o.0));
        c.then(self.1.cmp(&o.1))
            .then(self.2.cmp(&o.2))
            .then(self.3.cmp(&o.3))
            .then(self.4.cmp(&o.4))
            .reverse()
    }
}

/// The decimated mesh: the input vertices kept (their indices, ascending) and the triangles over them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decimated {
    /// The vertices kept: `np.unique` of the triangles left.
    pub used: Vec<usize>,
    /// The triangles left, in their order, over [`Decimated::used`]'s positions.
    pub tris: Vec<[usize; 3]>,
}

/// `decimate.decimate(pos, nrm, uv, tris, bone, target, boundary_weight, max_cost)`'s choice of vertices and
/// triangles (the attributes follow the vertices: `pos[used]`, `nrm[used]` ...).
///
/// # Errors
/// No vertices, a triangle naming a vertex that is not there, or `bone` not one per vertex.
#[allow(
    clippy::too_many_lines,
    clippy::indexing_slicing,
    reason = "decimate.py's one function, kept as it reads; indexes checked on entry (triangles) or made here"
)]
pub fn decimate(
    pos: &[V3],
    tris_in: &[[i64; 3]],
    bone: &[i64],
    target: i64,
    boundary_weight: f64,
    max_cost: Option<f64>,
) -> Result<Decimated, DecimateError> {
    let n = pos.len();
    if bone.len() != n {
        return Err(DecimateError::Shape(format!("{} bones for {n} vertices", bone.len())));
    }
    if n == 0 {
        return Err(DecimateError::Empty);
    }
    let mut tris: Vec<[usize; 3]> = Vec::with_capacity(tris_in.len());
    for t in tris_in {
        let mut out = [0usize; 3];
        for (o, &w) in out.iter_mut().zip(t) {
            *o = usize::try_from(w)
                .ok()
                .filter(|&w| w < n)
                .ok_or(DecimateError::Index(w, n))?;
        }
        tris.push(out);
    }
    // points: wedges at one position with one bone (np.unique of the rounded rows: lexicographic, as floats)
    let rows: Vec<[f64; 4]> = pos
        .iter()
        .zip(bone)
        .map(|(p, &b)| [round6(p[0]), round6(p[1]), round6(p[2]), b as f64])
        .collect();
    let cmp_rows = |a: &[f64; 4], b: &[f64; 4]| -> Ordering {
        for (x, y) in a.iter().zip(b) {
            match x.partial_cmp(y) {
                Some(Ordering::Equal) | None => {}
                Some(o) => return o,
            }
        }
        Ordering::Equal
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| cmp_rows(&rows[a], &rows[b]));
    let mut key = vec![0usize; n];
    let mut npts = 0usize;
    for (i, &w) in order.iter().enumerate() {
        if i > 0 && cmp_rows(&rows[order[i - 1]], &rows[w]) != Ordering::Equal {
            npts += 1;
        }
        key[w] = npts;
    }
    npts += 1;
    let mut ppos = vec![[0.0; 3]; npts];
    for (w, &k) in key.iter().enumerate() {
        ppos[k] = pos[w]; // the last wedge of a point wins, as NumPy's assignment
    }
    let mut wedges: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); npts];
    for (w, &k) in key.iter().enumerate() {
        wedges[k].insert(w);
    }
    let mut alive = vec![true; tris.len()];
    let mut vt: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    for (t, tri) in tris.iter().enumerate() {
        for &w in tri {
            vt[w].insert(t);
        }
    }
    // quadrics: triangle planes, and planes across open edges (the edges in the order Python's dict holds them)
    let mut q = vec![[[0.0f64; 4]; 4]; npts];
    let mut edge_at: HashMap<(usize, usize), usize> = HashMap::new();
    let mut edges: Vec<((usize, usize), Vec<usize>)> = Vec::new();
    for (t, &[a, b, c]) in tris.iter().enumerate() {
        let pq = plane_quadric(pos[a], pos[b], pos[c]);
        for w in [a, b, c] {
            add_into(&mut q[key[w]], &pq);
        }
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let e = (key[u].min(key[v]), key[u].max(key[v]));
            let i = *edge_at.entry(e).or_insert_with(|| {
                edges.push((e, Vec::new()));
                edges.len() - 1
            });
            edges[i].1.push(t);
        }
    }
    for &((p0, p1), ref ts) in &edges {
        if ts.len() == 1 && p0 != p1 {
            let [a, b, c] = tris[ts[0]];
            let f = cross(sub(pos[b], pos[a]), sub(pos[c], pos[a]));
            let d = sub(ppos[p1], ppos[p0]);
            let side = cross(d, f);
            let n2 = norm(side);
            if n2 < 1e-14 {
                continue;
            }
            let side = [side[0] / n2, side[1] / n2, side[2] / n2];
            let pl = [side[0], side[1], side[2], -ddot(&side, &ppos[p0])];
            // np.outer(pl, pl) * boundary_weight * np.dot(d, d): left to right
            let mut bq = outer(&pl, boundary_weight);
            let dd = ddot(&d, &d);
            for x in bq.iter_mut().flatten() {
                *x *= dd;
            }
            add_into(&mut q[p0], &bq);
            add_into(&mut q[p1], &bq);
        }
    }
    let mut nbr: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); npts];
    for &((p0, p1), _) in &edges {
        if p0 != p1 {
            nbr[p0].insert(p1);
            nbr[p1].insert(p0);
        }
    }
    let mut version = vec![0i64; npts];
    // move point u onto point v
    let cost = |q: &[[[f64; 4]; 4]], ppos: &[V3], u: usize, v: usize| -> f64 {
        let x = [ppos[v][0], ppos[v][1], ppos[v][2], 1.0];
        let s: [[f64; 4]; 4] = std::array::from_fn(|i| std::array::from_fn(|j| q[u][i][j] + q[v][i][j]));
        ddot(&gemv(&x, &s), &x)
    };
    let mut heap: BinaryHeap<Entry> = BinaryHeap::new();
    for u in 0..npts {
        for &v in &nbr[u] {
            if u < v {
                heap.push(Entry(cost(&q, &ppos, u, v), u, v, 0, 0));
                heap.push(Entry(cost(&q, &ppos, v, u), v, u, 0, 0));
            }
        }
    }
    let mut count = tris.len() as i64;
    let tri_normal = |tris: &[[usize; 3]], key: &[usize], t: usize, moved: Option<(usize, usize)>| -> V3 {
        let at = |w: usize| match moved {
            Some((from, to)) if key[w] == from => ppos[to],
            _ => ppos[key[w]],
        };
        let [a, b, c] = tris[t].map(at);
        cross(sub(b, a), sub(c, a))
    };
    while count > target {
        let Some(Entry(c, u, v, vu, vv)) = heap.pop() else {
            break;
        };
        if vu != version[u] || vv != version[v] || wedges[u].is_empty() || wedges[v].is_empty() || !nbr[u].contains(&v)
        {
            continue;
        }
        if max_cost.is_some_and(|m| c > m) {
            break; // every collapse left would cost more: stop short of the target
        }
        let tu: BTreeSet<usize> = wedges[u].iter().flat_map(|&w| vt[w].iter().copied()).collect();
        let shared: BTreeSet<usize> = tu
            .iter()
            .copied()
            .filter(|&t| tris[t].iter().any(|&w| key[w] == v))
            .collect();
        // each of u's wedges -> v's wedge it shares a triangle with
        let mut remap: Vec<(usize, usize)> = Vec::new();
        let mut ok = true;
        for &w in &wedges[u] {
            if vt[w].is_empty() {
                continue; // a wedge no triangle uses any more
            }
            let cand: BTreeSet<usize> = vt[w]
                .intersection(&shared)
                .flat_map(|&t| tris[t])
                .filter(|&x| key[x] == v)
                .collect();
            let mut it = cand.iter();
            match (it.next(), it.next()) {
                (Some(&x), None) => remap.push((w, x)),
                _ => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }
        // no triangle turned over or squashed
        for &t in tu.difference(&shared) {
            let n0 = tri_normal(&tris, &key, t, None);
            let n1 = tri_normal(&tris, &key, t, Some((u, v)));
            let (l0, l1) = (norm(n0), norm(n1));
            if l1 < 1e-12 || (l0 > 1e-12 && ddot(&n0, &n1) < 0.17 * l0 * l1) {
                ok = false;
                break;
            }
        }
        if !ok {
            continue;
        }
        for &t in &shared {
            alive[t] = false;
            count -= 1;
            for w in tris[t] {
                vt[w].remove(&t);
            }
        }
        for &(w, x) in &remap {
            for t in std::mem::take(&mut vt[w]) {
                for c in &mut tris[t] {
                    if *c == w {
                        *c = x;
                    }
                }
                vt[x].insert(t);
            }
            key[w] = v;
        }
        wedges[u].clear();
        let qu = q[u];
        add_into(&mut q[v], &qu);
        for x in std::mem::take(&mut nbr[u]) {
            nbr[x].remove(&u);
            if x != v {
                nbr[x].insert(v);
                nbr[v].insert(x);
            }
        }
        nbr[v].remove(&u);
        nbr[v].remove(&v);
        version[v] += 1;
        for &x in &nbr[v] {
            heap.push(Entry(cost(&q, &ppos, v, x), v, x, version[v], version[x]));
            heap.push(Entry(cost(&q, &ppos, x, v), x, v, version[x], version[v]));
        }
    }
    let kept: Vec<[usize; 3]> = tris.iter().zip(&alive).filter(|(_, a)| **a).map(|(t, _)| *t).collect();
    let used: Vec<usize> = kept
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<usize>>()
        .into_iter()
        .collect();
    let mut ren = vec![usize::MAX; n];
    for (i, &w) in used.iter().enumerate() {
        ren[w] = i;
    }
    Ok(Decimated {
        tris: kept.iter().map(|t| t.map(|w| ren[w])).collect(),
        used,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn blas_forms() {
        assert_eq!(ddot(&[1.0, 2.0, 3.0], &[4.0, 5.0, 6.0]), 32.0);
        let m = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [0.0, 0.0, 3.0, 0.0],
            [0.0, 0.0, 0.0, 4.0],
        ];
        assert_eq!(gemv(&[1.0, 1.0, 1.0, 1.0], &m), [1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn a_square_of_four_triangles() {
        // a flat square in four triangles around its middle: collapsing the middle leaves two
        let pos = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.5, 0.5, 0.0],
        ];
        let tris = [[0, 1, 4], [1, 2, 4], [2, 3, 4], [3, 0, 4]];
        let d = decimate(&pos, &tris, &[0; 5], 2, 10.0, None).unwrap();
        assert_eq!(d.tris.len(), 2);
        assert_eq!(d.used, vec![0, 1, 2, 3]);
        assert_eq!(decimate(&[], &[], &[], 1, 10.0, None), Err(DecimateError::Empty));
        assert!(matches!(
            decimate(&pos, &[[0, 1, 9]], &[0; 5], 1, 10.0, None),
            Err(DecimateError::Index(9, 5))
        ));
    }
}
