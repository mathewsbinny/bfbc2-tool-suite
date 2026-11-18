//! Vector and transform maths for editing: a transform is the scene's 4 x 4 matrix, row by row (rows right, up,
//! forward, position), as `bc2_scene::scene::Entity::matrix` keeps it.

/// A point or direction.
pub type V3 = [f64; 3];
/// A transform, row by row.
pub type M = [f64; 16];

/// `a - b`.
#[must_use]
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
/// `a + b`.
#[must_use]
pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
/// `a * s`.
#[must_use]
pub fn mul(a: V3, s: f64) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
/// The dot product.
#[must_use]
pub fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// The cross product.
#[must_use]
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
/// The length.
#[must_use]
pub fn len(a: V3) -> f64 {
    dot(a, a).sqrt()
}
/// The unit vector (zero stays zero).
#[must_use]
pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l < 1e-12 { [0.0; 3] } else { mul(a, 1.0 / l) }
}

/// Row `r` (0 right, 1 up, 2 forward, 3 position) of a transform.
#[must_use]
pub fn row(m: &M, r: usize) -> V3 {
    std::array::from_fn(|c| m.get(r * 4 + c).copied().unwrap_or(0.0))
}

/// Sets row `r` of a transform.
pub fn set_row(m: &mut M, r: usize, v: V3) {
    for (c, x) in v.iter().enumerate() {
        if let Some(slot) = m.get_mut(r * 4 + c) {
            *slot = *x;
        }
    }
}

/// The transform moved by `d`.
#[must_use]
pub fn translate(m: &M, d: V3) -> M {
    let mut o = *m;
    set_row(&mut o, 3, add(row(m, 3), d));
    o
}

/// The transform placed at `p`.
#[must_use]
pub fn at(m: &M, p: V3) -> M {
    let mut o = *m;
    set_row(&mut o, 3, p);
    o
}

/// `v` turned by `angle` radians around the unit `axis` (Rodrigues).
#[must_use]
pub fn rotate_vec(v: V3, axis: V3, angle: f64) -> V3 {
    let (s, c) = angle.sin_cos();
    add(
        add(mul(v, c), mul(cross(axis, v), s)),
        mul(axis, dot(axis, v) * (1.0 - c)),
    )
}

/// The transform turned by `angle` radians around the unit `axis` through `pivot`: its axes turn, and its position
/// turns around the pivot.
#[must_use]
pub fn rotate_about(m: &M, pivot: V3, axis: V3, angle: f64) -> M {
    let mut o = *m;
    for r in 0..3 {
        set_row(&mut o, r, rotate_vec(row(m, r), axis, angle));
    }
    let p = row(m, 3);
    set_row(&mut o, 3, add(pivot, rotate_vec(sub(p, pivot), axis, angle)));
    o
}

/// The rotation (unit axis, angle) that turns unit `a` onto unit `b` (any axis for opposite vectors).
#[must_use]
pub fn between(a: V3, b: V3) -> (V3, f64) {
    let c = dot(a, b).clamp(-1.0, 1.0);
    let x = cross(a, b);
    if len(x) < 1e-9 {
        if c > 0.0 {
            return ([0.0, 1.0, 0.0], 0.0);
        }
        // opposite: any axis at right angles to a
        let o = if a[0].abs() < 0.9 {
            [1.0, 0.0, 0.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        return (norm(cross(a, o)), std::f64::consts::PI);
    }
    (norm(x), c.acos())
}

/// Rounds `v` to the nearest multiple of `step` (a step of 0 or less leaves it).
#[must_use]
pub fn snap(v: f64, step: f64) -> f64 {
    if step > 0.0 { (v / step).round() * step } else { v }
}

/// The yaw (around Y, degrees) of a transform's forward row, as the properties show it.
#[must_use]
pub fn yaw_degrees(m: &M) -> f64 {
    let f = row(m, 2);
    f[0].atan2(f[2]).to_degrees()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn rotations() {
        let i: M = [1.0, 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 2., 0., 0., 1.];
        let r = rotate_about(&i, [0.0; 3], [0.0, 1.0, 0.0], std::f64::consts::FRAC_PI_2);
        // the position (2, 0, 0) turns to (0, 0, -2); forward (0, 0, 1) to (1, 0, 0)
        assert!(len(sub(row(&r, 3), [0.0, 0.0, -2.0])) < 1e-12);
        assert!(len(sub(row(&r, 2), [1.0, 0.0, 0.0])) < 1e-12);
        assert!((yaw_degrees(&r) - 90.0).abs() < 1e-9);
        let (ax, a) = between([0.0, 1.0, 0.0], norm([1.0, 1.0, 0.0]));
        assert!(len(sub(rotate_vec([0.0, 1.0, 0.0], ax, a), norm([1.0, 1.0, 0.0]))) < 1e-12);
        let (ax, a) = between([0.0, 1.0, 0.0], [0.0, -1.0, 0.0]);
        assert!(len(sub(rotate_vec([0.0, 1.0, 0.0], ax, a), [0.0, -1.0, 0.0])) < 1e-12);
        assert!((snap(1.26, 0.25) - 1.25).abs() < 1e-12);
        assert!((snap(1.26, 0.0) - 1.26).abs() < 1e-12);
    }
}
