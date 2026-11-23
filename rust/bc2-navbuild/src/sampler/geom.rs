//! The offline sampler's per-triangle arithmetic, one (triangle, column) pair at a time, as NumPy computes it over
//! its arrays (`navsample_offline.py`: `_line_tri_interval`, `_edge_dist2`, `raster_hits`' barycentric test, the
//! triangle normal of `sample_grid`).
//!
//! **Bit for bit.** Every expression keeps the reference's order of operations; NumPy's `minimum` / `maximum` /
//! `clip` propagate a NaN (here [`nmin`], [`nmax`], [`clip`]); `np.cross` is `a1 * b2 - a2 * b1` and so on; a row's
//! `sum` / `linalg.norm` over its three values is `(x0 + x1) + x2` (NumPy 2.5's reduction over a contiguous row of
//! three, measured).

/// A triangle: three corners `(x, y, z)`.
pub type Tri = [[f64; 3]; 3];

/// `np.minimum`: a NaN on either side gives a NaN.
#[inline]
#[must_use]
pub fn nmin(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if b < a {
        b
    } else {
        a
    }
}

/// `np.maximum`: a NaN on either side gives a NaN.
#[inline]
#[must_use]
pub fn nmax(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if b > a {
        b
    } else {
        a
    }
}

/// `np.clip(x, lo, hi)` (a NaN stays a NaN).
#[inline]
#[must_use]
pub fn clip(x: f64, lo: f64, hi: f64) -> f64 {
    nmin(nmax(x, lo), hi)
}

/// `np.where(cond, a, b)` of two computed values.
#[inline]
fn pick(cond: bool, a: f64, b: f64) -> f64 {
    if cond { a } else { b }
}

/// `np.cross(u, v)` of two 3-vectors.
#[inline]
#[must_use]
pub fn cross(u: [f64; 3], v: [f64; 3]) -> [f64; 3] {
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

/// `u - v`.
#[inline]
#[must_use]
pub fn sub(u: [f64; 3], v: [f64; 3]) -> [f64; 3] {
    [u[0] - v[0], u[1] - v[1], u[2] - v[2]]
}

/// A row's `sum` of three values (`(x0 + x1) + x2`).
#[inline]
#[must_use]
pub fn sum3(x: [f64; 3]) -> f64 {
    (x[0] + x[1]) + x[2]
}

/// `np.linalg.norm(v, axis=-1)` of a 3-vector row.
#[inline]
#[must_use]
pub fn norm(v: [f64; 3]) -> f64 {
    sum3([v[0] * v[0], v[1] * v[1], v[2] * v[2]]).sqrt()
}

/// `np.cross(b - a, c - a)`: the triangle's (unnormalised) normal.
#[inline]
#[must_use]
pub fn tri_cross(t: &Tri) -> [f64; 3] {
    cross(sub(t[1], t[0]), sub(t[2], t[0]))
}

/// `sample_grid`'s `tri_n`: the normal over `np.maximum(norm, 1e-12)`.
#[must_use]
pub fn unit_normal(t: &Tri) -> [f64; 3] {
    let n = tri_cross(t);
    let l = nmax(norm(n), 1e-12);
    [n[0] / l, n[1] / l, n[2] / l]
}

/// `raster_hits`' `det` (the triangle's signed doubled area on x / z).
#[inline]
#[must_use]
pub fn det(t: &Tri) -> f64 {
    let [a, b, c] = *t;
    (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2])
}

/// `_line_tri_interval(t, px, pz, r)` for one pair: the height interval `(lo, hi)` of the vertical line through
/// `(px, pz)` inside the triangle grown by a ball of radius `r` (`lo > hi`: none).
#[must_use]
#[allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    reason = "the reference's names"
)]
pub fn line_tri_interval(t: &Tri, px: f64, pz: f64, r: f64) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    let [a, b, c] = *t;
    let rr = r * r;
    // vertex spheres
    for v in [a, b, c] {
        let (dx, dz) = (px - v[0], pz - v[2]);
        let q = rr - dx * dx - dz * dz;
        let ok = q >= 0.0;
        let s = pick(ok, q, 0.0).sqrt();
        lo = pick(ok, nmin(lo, v[1] - s), lo);
        hi = pick(ok, nmax(hi, v[1] + s), hi);
    }
    // edge cylinders, capped by the edge's ends
    for (u, v) in [(a, b), (b, c), (c, a)] {
        let e = sub(v, u);
        let ee0 = sum3([e[0] * e[0], e[1] * e[1], e[2] * e[2]]);
        let ee = pick(ee0 > 1e-12, ee0, 1e-12);
        let w0 = [px - u[0], -u[1], pz - u[2]];
        let we0 = sum3([w0[0] * e[0], w0[1] * e[1], w0[2] * e[2]]);
        let ey = e[1];
        let aa = 1.0 - ey * ey / ee;
        let bb = 2.0 * (w0[1] - we0 * ey / ee);
        let cc = sum3([w0[0] * w0[0], w0[1] * w0[1], w0[2] * w0[2]]) - we0 * we0 / ee - rr;
        let disc = bb * bb - 4.0 * aa * cc;
        let mut okq = disc >= 0.0 && aa > 1e-9;
        let sq = pick(okq, disc, 0.0).sqrt();
        let a2 = pick(aa > 1e-9, 2.0 * aa, 1.0);
        let (mut h1, mut h2) = ((-bb - sq) / a2, (-bb + sq) / a2);
        let vert = aa <= 1e-9 && cc <= 0.0;
        okq |= vert;
        h1 = pick(vert, f64::NEG_INFINITY, h1);
        h2 = pick(vert, f64::INFINITY, h2);
        let steep = ey.abs() > 1e-12;
        let s1 = pick(steep, (0.0 * ee - we0) / ey, f64::NEG_INFINITY);
        let s2 = pick(steep, (ee - we0) / ey, f64::INFINITY);
        let (mut sl, mut sh) = (nmin(s1, s2), nmax(s1, s2));
        let flat = ey.abs() <= 1e-12;
        let inrange = we0 >= 0.0 && we0 <= ee;
        sl = pick(flat, pick(inrange, f64::NEG_INFINITY, f64::INFINITY), sl);
        sh = pick(flat, pick(inrange, f64::INFINITY, f64::NEG_INFINITY), sh);
        let (l2, h2b) = (nmax(h1, sl), nmin(h2, sh));
        let ok = okq && l2 <= h2b;
        lo = pick(ok, nmin(lo, l2), lo);
        hi = pick(ok, nmax(hi, h2b), hi);
    }
    // the face slab: |n . (P - a)| <= r, the projection inside the triangle
    let n0 = tri_cross(t);
    let ln = norm(n0);
    let good = ln > 1e-12;
    let d = pick(good, ln, 1.0);
    let nrm = [n0[0] / d, n0[1] / d, n0[2] / d];
    let ny = nrm[1];
    let okf = good && ny.abs() > 1e-6;
    let f0 = nrm[0] * (px - a[0]) + nrm[2] * (pz - a[2]) - ny * a[1];
    let nys = pick(okf, ny, 1.0);
    let (ha, hb) = ((-r - f0) / nys, (r - f0) / nys);
    let (mut fl, mut fh) = (nmin(ha, hb), nmax(ha, hb));
    let wall = good && !okf;
    fl = pick(wall, pick(f0.abs() <= r, f64::NEG_INFINITY, f64::INFINITY), fl);
    fh = pick(wall, f64::INFINITY, fh);
    for (u, v) in [(a, b), (b, c), (c, a)] {
        let e = sub(v, u);
        let m = cross(nrm, e);
        let g0 = m[0] * (px - u[0]) + m[2] * (pz - u[2]) - m[1] * u[1];
        let gy = m[1];
        let hb = -g0 / gy;
        let pos = gy > 1e-12;
        let neg = gy < -1e-12;
        fl = pick(pos, nmax(fl, hb), fl);
        fh = pick(neg, nmin(fh, hb), fh);
        if !pos && !neg && g0 < 0.0 {
            fl = f64::INFINITY;
        }
    }
    let ok = good && fl <= fh;
    lo = pick(ok, nmin(lo, fl), lo);
    hi = pick(ok, nmax(hi, fh), hi);
    (lo, hi)
}

/// `_edge_dist2(t, px, pz)` for one pair: the horizontal squared distance to the triangle's nearest edge, the height
/// there and that point's x, z.
#[must_use]
#[allow(clippy::float_cmp, reason = "the reference's exact comparisons")]
pub fn edge_dist2(t: &Tri, px: f64, pz: f64) -> (f64, f64, f64, f64) {
    let (mut best, mut yb, mut xb, mut zb) = (f64::INFINITY, 0.0f64, 0.0f64, 0.0f64);
    for (u, v) in [(0usize, 1usize), (1, 2), (2, 0)] {
        let (tu, tv) = (t[u], t[v]);
        let (ex, ez) = (tv[0] - tu[0], tv[2] - tu[2]);
        let ll = ex * ex + ez * ez;
        let s = clip(
            ((px - tu[0]) * ex + (pz - tu[2]) * ez) / pick(ll > 0.0, ll, 1.0),
            0.0,
            1.0,
        );
        let (qx, qz) = (tu[0] + s * ex, tu[2] + s * ez);
        let (dx, dz) = (qx - px, qz - pz);
        let d2 = dx * dx + dz * dz;
        let qy = pick(ll > 0.0, tu[1] + s * (tv[1] - tu[1]), nmax(tu[1], tv[1]));
        let better = d2 < best - 1e-12;
        let tie = (d2 - best).abs() <= 1e-12;
        yb = pick(better, qy, pick(tie, nmax(yb, qy), yb));
        best = pick(better, d2, best);
        xb = pick(better, qx, xb);
        zb = pick(better, qz, zb);
    }
    (best, yb, xb, zb)
}

/// One `raster_hits` pair's answer: where the cast meets the triangle in the column, `None` when it does not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// The hit's height.
    pub y: f64,
    /// The sphere's contact normal (an edge or vertex touched), `None`: the face itself (`NaN` row in the reference).
    pub edge: Option<[f64; 3]>,
}

/// `raster_hits` for one (triangle, column) pair, `grow > 0` (both of the sampler's calls): the line through the
/// face, else the cast's sphere touching the triangle within `grow`.
#[must_use]
#[allow(clippy::many_single_char_names, reason = "the reference's names")]
pub fn raster_pair(t: &Tri, det: f64, px: f64, pz: f64, grow: f64) -> Option<Hit> {
    let [a, b, c] = *t;
    let fl = det.abs() > 1e-9;
    let dt = pick(fl, det, 1.0);
    let l1 = ((b[2] - c[2]) * (px - c[0]) + (c[0] - b[0]) * (pz - c[2])) / dt;
    let l2 = ((c[2] - a[2]) * (px - c[0]) + (a[0] - c[0]) * (pz - c[2])) / dt;
    let l3 = 1.0 - l1 - l2;
    let inside = fl && l1 >= -1e-7 && l2 >= -1e-7 && l3 >= -1e-7;
    if inside {
        return Some(Hit {
            y: l1 * a[1] + l2 * b[1] + l3 * c[1],
            edge: None,
        });
    }
    let (lo, hi) = line_tri_interval(t, px, pz, grow);
    if !(lo <= hi) {
        return None;
    }
    let (d2, _, ex, ez) = edge_dist2(t, px, pz);
    let reach = nmax(grow * grow - d2, 0.0).sqrt();
    Some(Hit {
        y: hi - reach,
        edge: Some([(px - ex) / grow, reach / grow, (pz - ez) / grow]),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "tests")]
mod tests {
    use super::*;

    #[test]
    fn nan_propagates_as_numpy() {
        assert!(nmin(f64::NAN, 1.0).is_nan() && nmax(1.0, f64::NAN).is_nan() && clip(f64::NAN, 0.0, 1.0).is_nan());
        assert_eq!(clip(2.0, 0.0, 1.0), 1.0);
        assert_eq!(nmin(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn a_flat_triangle_interval_and_hit() {
        let t: Tri = [[0.0, 1.0, 0.0], [2.0, 1.0, 0.0], [0.0, 1.0, 2.0]];
        let (lo, hi) = line_tri_interval(&t, 0.5, 0.5, 0.4);
        assert!((lo - 0.6).abs() < 1e-12 && (hi - 1.4).abs() < 1e-12, "{lo} {hi}");
        let h = raster_pair(&t, det(&t), 0.5, 0.5, 0.05).unwrap();
        assert!((h.y - 1.0).abs() < 1e-12 && h.edge.is_none());
        // 0.03 m outside the hypotenuse: the sphere touches the edge
        let h = raster_pair(&t, det(&t), 1.0 + 0.03 / 2f64.sqrt(), 1.0 + 0.03 / 2f64.sqrt(), 0.05).unwrap();
        assert!(
            h.edge.is_some_and(|n| (n[1] - 0.8).abs() < 1e-9) && (h.y - 1.0).abs() < 1e-9,
            "{h:?}"
        );
        assert!(raster_pair(&t, det(&t), 3.0, 3.0, 0.05).is_none());
    }

    /// The distance from `p` to triangle `t` (brute force: the plane inside, else the nearest edge).
    fn point_tri_dist(p: [f64; 3], t: &Tri) -> f64 {
        let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
        let [a, b, c] = *t;
        let n = tri_cross(t);
        let ln = norm(n);
        let mut best = f64::INFINITY;
        if ln > 1e-12 {
            let n = [n[0] / ln, n[1] / ln, n[2] / ln];
            let d = dot(sub(p, a), n);
            let q = [p[0] - d * n[0], p[1] - d * n[1], p[2] - d * n[2]];
            if [(a, b), (b, c), (c, a)]
                .iter()
                .all(|&(u, v)| dot(cross(sub(v, u), sub(q, u)), n) >= -1e-12)
            {
                best = d.abs();
            }
        }
        for (u, v) in [(a, b), (b, c), (c, a)] {
            let e = sub(v, u);
            let s = (dot(sub(p, u), e) / dot(e, e).max(1e-12)).clamp(0.0, 1.0);
            best = best.min(norm(sub(p, [u[0] + s * e[0], u[1] + s * e[1], u[2] + s * e[2]])));
        }
        best
    }

    /// `_line_tri_interval` against brute force (the reference's test 8): 300 random triangles - a third flat, a
    /// sixth vertical - and random vertical lines, the line sampled every 0.02 m.
    #[test]
    fn the_interval_against_brute_force() {
        let mut seed = 0x9e37_79b9_7f4a_7c15u64;
        let mut rnd = |lo: f64, hi: f64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            lo + (hi - lo) * ((seed >> 11) as f64 / (1u64 << 53) as f64)
        };
        let mut bad = 0;
        for k in 0..300 {
            let mut t: Tri = [[0.0; 3]; 3];
            for v in &mut t {
                *v = [rnd(-1.0, 1.0), rnd(-1.0, 1.0), rnd(-1.0, 1.0)];
            }
            if k < 100 {
                for v in &mut t {
                    v[1] = 0.2;
                }
            } else if k < 150 {
                let x = t[0][0];
                for v in &mut t {
                    v[0] = x;
                }
            }
            let (px, pz) = (rnd(-1.2, 1.2), rnd(-1.2, 1.2));
            let (lo, hi) = line_tri_interval(&t, px, pz, 0.4);
            let inside: Vec<f64> = (0..=200)
                .map(|i| -2.0 + 0.02 * f64::from(i))
                .filter(|&h| point_tri_dist([px, h, pz], &t) <= 0.4)
                .collect();
            match (inside.first(), inside.last()) {
                (Some(&a), Some(&b)) => bad += usize::from((lo - a).abs() > 0.021 || (hi - b).abs() > 0.021),
                _ => bad += usize::from(lo <= hi && hi - lo > 0.03),
            }
        }
        assert_eq!(bad, 0);
    }
}
