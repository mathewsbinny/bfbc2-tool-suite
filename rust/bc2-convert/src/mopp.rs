//! Havok's MOPP byte code (`hkpMoppCode`: the bounding-volume tree of a `hkpMoppBvTreeShape`) decoded and queried
//! (`bfbc2-bf1943-import/mopp.py`, round F2b): the box query, the completeness check and the ray casts built on it,
//! step for step as the reference runs them - the same traversal order, so the same first error.
//!
//! Where the reference would raise something other than its own `MoppError` (an index past the code: Python's
//! `IndexError`; a coordinate too large for its integers to stay exact here), the functions return
//! [`Mopp::Refused`] and the caller runs the Python reference, which raises its own error.

#![allow(clippy::many_single_char_names)] // the reference's names (b, p, o, k ...), kept to read side by side

use std::collections::{BTreeSet, HashMap, HashSet};

/// The shift a query starts at.
pub const START_SHIFT: i64 = 16;

/// The 13 split axes (x, y, z, then the sums and differences), as `mopp.AXES`.
pub const AXES: [[i64; 3]; 13] = [
    [1, 0, 0],
    [0, 1, 0],
    [0, 0, 1],
    [0, 1, 1],
    [0, 1, -1],
    [1, 0, 1],
    [1, 0, -1],
    [1, 1, 0],
    [1, -1, 0],
    [1, 1, 1],
    [1, 1, -1],
    [1, -1, 1],
    [1, -1, -1],
];

/// A query's outcome other than its answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mopp {
    /// The reference's `MoppError` with its message.
    Error(String),
    /// Not answered here (the reference raises another exception, or its integers would leave `i64`).
    Refused,
}

/// A result of this module.
pub type Result<T> = std::result::Result<T, Mopp>;

/// `code[i]` or a refusal (the reference's `IndexError`).
fn at(code: &[u8], i: i64) -> Result<i64> {
    usize::try_from(i)
        .ok()
        .and_then(|i| code.get(i))
        .map(|&b| i64::from(b))
        .ok_or(Mopp::Refused)
}

/// `int.from_bytes(code[a:b], 'big')`: a short slice reads what is there (as Python's slicing does).
fn be(code: &[u8], a: i64, b: i64) -> i64 {
    let n = code.len() as i64;
    let (a, b) = (a.clamp(0, n), b.clamp(0, n));
    let mut v = 0i64;
    if a < b {
        for &x in code.get(a as usize..b as usize).unwrap_or_default() {
            v = (v << 8) | i64::from(x);
        }
    }
    v
}

/// `_value(k, lo, hi)`: the range of split `k`'s value over the box (byte space).
#[allow(clippy::indexing_slicing)] // fixed-size [_; 3] arrays indexed by an axis below 3
fn value(k: usize, lo: &[i64; 3], hi: &[i64; 3]) -> (i64, i64) {
    let w = AXES.get(k).copied().unwrap_or([0; 3]);
    let mut a = 0i64;
    let mut b = 0i64;
    for i in 0..3 {
        let (wi, l, h) = (w[i], lo[i], hi[i]);
        a += if wi > 0 { l * wi } else { h * wi };
        b += if wi > 0 { h * wi } else { l * wi };
    }
    if k < 3 {
        return (a, b);
    }
    let terms = w.iter().filter(|&&x| x != 0).count() as i64;
    let negs = w.iter().filter(|&&x| x < 0).count() as i64;
    ((a + 255 * negs).div_euclid(terms), (b + 255 * negs).div_euclid(terms))
}

/// One traversal state: (code position, origin, shift, terminal offset).
type State = (i64, [i64; 3], i64, i64);

/// `query_int(code, qmin, qmax)`: the terminal keys reached by the quantized box.
///
/// # Errors
/// [`Mopp::Error`] with the reference's message, [`Mopp::Refused`] where it raises another exception.
#[allow(clippy::indexing_slicing)] // fixed-size [_; 3] arrays indexed by an axis below 3
pub fn query_int(code: &[u8], qmin: [i64; 3], qmax: [i64; 3]) -> Result<BTreeSet<i64>> {
    // every value stays far inside i64: the box is checked by the caller (quantize) to stay under 2^40
    let mut out = BTreeSet::new();
    let mut stack: Vec<State> = vec![(0, [0, 0, 0], START_SHIFT, 0)];
    let mut seen: HashSet<State> = HashSet::new();
    let n = code.len() as i64;
    while let Some((mut i, mut o, mut s, mut off)) = stack.pop() {
        while i < n {
            if !seen.insert((i, o, s, off)) {
                break;
            }
            let c = at(code, i)?;
            if (0x30..0x50).contains(&c) {
                out.insert(off + c - 0x30);
                break;
            }
            if (0x50..=0x53).contains(&c) {
                let w = c - 0x4F;
                out.insert(off + be(code, i + 1, i + 1 + w));
                break;
            }
            if c == 0x09 || c == 0x0A {
                let w = c - 0x08;
                off += be(code, i + 1, i + 1 + w);
                i += 1 + w;
                continue;
            }
            if c == 0x0B {
                off = be(code, i + 1, i + 5);
                i += 5;
                continue;
            }
            if (0x05..=0x08).contains(&c) {
                let w = c - 0x04;
                i += 1 + w + be(code, i + 1, i + 1 + w);
                continue;
            }
            if (1..=4).contains(&c) {
                let mut no = [0i64; 3];
                for (a, x) in no.iter_mut().enumerate() {
                    *x = o[a] + (at(code, i + 1 + a as i64)? << s);
                }
                o = no;
                s -= c;
                if s < 0 {
                    return Err(Mopp::Error(format!("rescale below one unit at {i}")));
                }
                i += 4;
                continue;
            }
            let lo = [(qmin[0] - o[0]) >> s, (qmin[1] - o[1]) >> s, (qmin[2] - o[2]) >> s];
            let hi = [(qmax[0] - o[0]) >> s, (qmax[1] - o[1]) >> s, (qmax[2] - o[2]) >> s];
            if (0x26..=0x28).contains(&c) {
                let a = (c - 0x26) as usize;
                if hi[a] < at(code, i + 1)? || lo[a] > at(code, i + 2)? {
                    break;
                }
                i += 3;
                continue;
            }
            if (0x10..=0x1C).contains(&c) {
                let (vlo, vhi) = value((c - 0x10) as usize, &lo, &hi);
                if vhi >= at(code, i + 2)? {
                    stack.push((i + 4 + at(code, i + 3)?, o, s, off));
                }
                if vlo <= at(code, i + 1)? {
                    i += 4;
                    continue;
                }
                break;
            }
            if (0x20..=0x22).contains(&c) {
                let a = (c - 0x20) as usize;
                let sp = at(code, i + 1)?;
                if hi[a] >= sp {
                    stack.push((i + 3 + at(code, i + 2)?, o, s, off));
                }
                if lo[a] <= sp {
                    i += 3;
                    continue;
                }
                break;
            }
            if (0x23..=0x25).contains(&c) {
                let a = (c - 0x23) as usize;
                if hi[a] >= at(code, i + 2)? {
                    stack.push((i + 7 + be(code, i + 5, i + 7), o, s, off));
                }
                if lo[a] <= at(code, i + 1)? {
                    stack.push((i + 7 + be(code, i + 3, i + 5), o, s, off));
                }
                break;
            }
            return Err(Mopp::Error(format!("opcode {c:#04x} at {i}")));
        }
    }
    Ok(out)
}

/// The largest quantized coordinate answered here (the reference's integers are unbounded).
const LIMIT: f64 = 1.0e12;

/// `quantize(info, lo, hi)`: the world box as the quantized box (floor / ceil); a coordinate the reference's
/// `math.floor` would raise on (NaN, infinity) or one too large for this module is refused.
///
/// # Errors
/// [`Mopp::Refused`].
#[allow(clippy::indexing_slicing)] // fixed-size [_; 3] arrays indexed by an axis below 3
pub fn quantize(info: &[f64; 4], lo: &[f64; 3], hi: &[f64; 3]) -> Result<([i64; 3], [i64; 3])> {
    let mut qmin = [0i64; 3];
    let mut qmax = [0i64; 3];
    for a in 0..3 {
        let l = ((lo[a] - info[a]) * info[3]).floor();
        let h = ((hi[a] - info[a]) * info[3]).ceil();
        if !(l.abs() < LIMIT && h.abs() < LIMIT) {
            return Err(Mopp::Refused);
        }
        qmin[a] = l as i64;
        qmax[a] = h as i64;
    }
    Ok((qmin, qmax))
}

/// `query(code, info, lo, hi)`.
///
/// # Errors
/// As [`query_int`] and [`quantize`].
pub fn query(code: &[u8], info: &[f64; 4], lo: &[f64; 3], hi: &[f64; 3]) -> Result<BTreeSet<i64>> {
    let (qmin, qmax) = quantize(info, lo, hi)?;
    query_int(code, qmin, qmax)
}

/// `complete(code, info, boxes, tolerance)`: the keys whose own (widened) box's query does not reach them, and the
/// mean number of keys a query returns. `boxes` in the reference dict's order.
///
/// # Errors
/// As [`query`].
pub fn complete(
    code: &[u8],
    info: &[f64; 4],
    boxes: &[(i64, [f64; 3], [f64; 3])],
    tolerance: f64,
) -> Result<(Vec<i64>, f64)> {
    let mut missed = Vec::new();
    let mut total = 0usize;
    for (k, lo, hi) in boxes {
        let l = [lo[0] - tolerance, lo[1] - tolerance, lo[2] - tolerance];
        let h = [hi[0] + tolerance, hi[1] + tolerance, hi[2] + tolerance];
        let got = query(code, info, &l, &h)?;
        total += got.len();
        if !got.contains(k) {
            missed.push(*k);
        }
    }
    // both counts are far below 2^53: the quotient is Python's true division
    #[allow(clippy::cast_precision_loss)]
    let mean = total as f64 / boxes.len().max(1) as f64;
    Ok((missed, mean))
}

/// A triangle: three points.
pub type Tri = [[f64; 3]; 3];

/// `sum(x[i] * y[i] for i in range(3))` as Python adds it ([`crate::pyfmt::fsum_py`]).
fn dot(x: &[f64; 3], y: &[f64; 3]) -> f64 {
    crate::pyfmt::fsum_py(&[x[0] * y[0], x[1] * y[1], x[2] * y[2]])
}

/// `_ray_triangle(o, d, tri)`: the distance along the ray to the triangle (Moeller-Trumbore, both sides).
#[must_use]
pub fn ray_triangle(o: &[f64; 3], d: &[f64; 3], tri: &Tri) -> Option<f64> {
    let [a, b, c] = tri;
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let p = [
        d[1] * e2[2] - d[2] * e2[1],
        d[2] * e2[0] - d[0] * e2[2],
        d[0] * e2[1] - d[1] * e2[0],
    ];
    let det = dot(&e1, &p);
    if det.abs() < 1e-12 {
        return None;
    }
    let t0 = [o[0] - a[0], o[1] - a[1], o[2] - a[2]];
    let u = dot(&t0, &p) / det;
    if u < 0.0 || u > 1.0 {
        return None;
    }
    let q = [
        t0[1] * e1[2] - t0[2] * e1[1],
        t0[2] * e1[0] - t0[0] * e1[2],
        t0[0] * e1[1] - t0[1] * e1[0],
    ];
    let v = dot(d, &q) / det;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(&e2, &q) / det;
    (t >= 0.0).then_some(t)
}

/// The ray cast's parameters.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    /// Where it starts.
    pub origin: [f64; 3],
    /// Its direction.
    pub direction: [f64; 3],
    /// How far it goes.
    pub length: f64,
    /// The query steps along it.
    pub step: f64,
    /// Each step's box widened by this.
    pub tolerance: f64,
}

/// `ray_cast(code, info, tris, origin, direction, length, step, tolerance)`: (distance, key) of the nearest hit.
///
/// # Errors
/// As [`query`].
#[allow(clippy::indexing_slicing)] // fixed-size [_; 3] arrays indexed by an axis below 3
pub fn ray_cast<S: std::hash::BuildHasher>(
    code: &[u8],
    info: &[f64; 4],
    tris: &HashMap<i64, Tri, S>,
    ray: &Ray,
) -> Result<Option<(f64, i64)>> {
    let (o, d, length) = (&ray.origin, &ray.direction, ray.length);
    let mut best: Option<(f64, i64)> = None;
    let mut t = 0.0f64;
    while t < length {
        let u = py_min(t + ray.step, length);
        let mut lo = [0.0; 3];
        let mut hi = [0.0; 3];
        for i in 0..3 {
            let a = o[i] + d[i] * t;
            let b = o[i] + d[i] * u;
            lo[i] = py_min(a, b) - ray.tolerance;
            hi[i] = py_max(a, b) + ray.tolerance;
        }
        for k in query(code, info, &lo, &hi)? {
            if let Some(tri) = tris.get(&k)
                && let Some(dist) = ray_triangle(o, d, tri)
                && dist <= length
                && best.is_none_or(|b| dist < b.0)
            {
                best = Some((dist, k));
            }
        }
        if let Some(b) = best
            && b.0 <= u
        {
            return Ok(best);
        }
        t = u;
    }
    Ok(best)
}

/// Python's `max(a, b)`: the first unless the second is larger (`f64::max` differs on NaN and signed zeros).
fn py_max(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// Python's `min(a, b)`: the first unless the second is smaller.
fn py_min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// `ray_cast_brute(tris, origin, direction, length)`: the same over every triangle, in the dict's order.
#[must_use]
pub fn ray_cast_brute(tris: &[(i64, Tri)], origin: &[f64; 3], direction: &[f64; 3], length: f64) -> Option<(f64, i64)> {
    let mut best: Option<(f64, i64)> = None;
    for (k, tri) in tris {
        if let Some(d) = ray_triangle(origin, direction, tri)
            && d <= length
            && best.is_none_or(|b| d < b.0)
        {
            best = Some((d, *k));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminals_and_splits() {
        // a split on x at 128: left terminal 1, right terminal 2
        let code = [0x20, 128, 1, 0x31, 0x32];
        assert_eq!(query_int(&code, [0, 0, 0], [10 << 16, 0, 0]), Ok([1].into()));
        assert_eq!(query_int(&code, [200 << 16, 0, 0], [201 << 16, 0, 0]), Ok([2].into()));
        assert_eq!(query_int(&code, [0, 0, 0], [255 << 16, 0, 0]), Ok([1, 2].into()));
    }

    #[test]
    fn errors_and_refusals() {
        assert_eq!(
            query_int(&[0x7F], [0; 3], [0; 3]),
            Err(Mopp::Error("opcode 0x7f at 0".into()))
        );
        assert_eq!(query_int(&[0x26, 0], [0; 3], [0; 3]), Err(Mopp::Refused));
        let deep = [
            0x04, 0, 0, 0, 0x04, 0, 0, 0, 0x04, 0, 0, 0, 0x04, 0, 0, 0, 0x04, 0, 0, 0,
        ];
        assert_eq!(
            query_int(&deep, [0; 3], [0; 3]),
            Err(Mopp::Error("rescale below one unit at 16".into()))
        );
        assert_eq!(
            quantize(&[0.0, 0.0, 0.0, 1.0], &[f64::NAN; 3], &[0.0; 3]),
            Err(Mopp::Refused)
        );
    }

    #[test]
    fn ray_hits() {
        let tri: Tri = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let d = ray_triangle(&[0.2, 0.2, 1.0], &[0.0, 0.0, -1.0], &tri);
        assert_eq!(d, Some(1.0));
        assert_eq!(
            ray_cast_brute(&[(7, tri)], &[0.2, 0.2, 1.0], &[0.0, 0.0, -1.0], 2.0),
            Some((1.0, 7))
        );
    }
}
