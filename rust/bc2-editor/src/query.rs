//! Questions about the scene's surfaces, on the CPU: where a ray first meets the terrain or a placed object's
//! collision (with the surface normal and the triangle there), the point a given sink depth below a surface, and
//! placing an object on a surface (`EDITOR_ARCHITECTURE.md` 3.10, item 3).
//!
//! **Surface placement** ([`place`]): an object dropped on terrain or a mesh sits on it - its lowest point (the
//! bottom of its collision in its own space, or its pivot for an object drawn as a marker) on the surface point -
//! optionally turned so its up axis is the surface normal; then a **sink depth** (metres, or a fraction of the
//! object's height) pushes it into the surface along the surface normal or straight down. The depth is stored with
//! the object's operation and kept when it is moved again (`bc2_levelwrite::ops::Placement`).
//!
//! **Vertex and edge snapping** ([`snap_vertex`], [`snap_edge`]): the corner, or the nearest point on an edge, of the
//! triangle a ray meets. Picking *which* entity is under the cursor stays on the GPU (the id target, `render::pick`).

use bc2_levelwrite::Placement;
use bc2_scene::scene::Scene;

use crate::math::{self, M, V3, add, cross, dot, len, mul, norm, row, sub};

/// What a ray met.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    /// Distance along the (unit) ray.
    pub t: f64,
    /// The point.
    pub point: V3,
    /// The surface's unit normal there, facing the ray's origin.
    pub normal: V3,
    /// The entity whose collision it is; `None` for the terrain.
    pub entity: Option<usize>,
    /// The triangle met (world space).
    pub tri: [V3; 3],
}

impl Hit {
    /// A point met with nothing known of its surface (round E4-rest2: the navigation ground under the pointer, which
    /// the navigation edit tool takes as a click): facing up, no entity, a degenerate triangle.
    #[must_use]
    pub fn at(point: V3) -> Self {
        Self {
            t: 0.0,
            point,
            normal: [0.0, 1.0, 0.0],
            entity: None,
            tri: [point; 3],
        }
    }
}

/// Möller-Trumbore: the distance along the ray to the triangle, if it meets it in front of the origin.
fn ray_triangle(o: V3, d: V3, tri: [V3; 3]) -> Option<(f64, V3)> {
    let [a, b, c] = tri;
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let p = cross(d, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-12 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(o, a);
    let u = dot(s, p) * inv;
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(d, q) * inv;
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t > 1e-9).then(|| (t, cross(e1, e2)))
}

/// A ray in an object's own space (its transform `m` undone: the inverse of its three axes): the same distance `t`
/// reaches the same point. `None` when the axes are degenerate or the numbers are not finite (every run is tested).
fn object_ray(m: &M, origin: V3, dir: V3) -> Option<(V3, V3)> {
    let (a, b, c) = (row(m, 0), row(m, 1), row(m, 2));
    let lens = [len(a), len(b), len(c)];
    let (lo, hi) = (
        lens.iter().copied().fold(f64::INFINITY, f64::min),
        lens.iter().copied().fold(0.0, f64::max),
    );
    // a squashed or mirrored-flat transform: no reliable inverse
    if !(lo > 1e-6 * hi && hi.is_finite()) {
        return None;
    }
    let det = dot(a, cross(b, c));
    if !det.is_finite() || det.abs() < 1e-9 * hi * hi * hi {
        return None;
    }
    // x A = w  =>  x = w A^-1; the rows of A^-1 are the columns of the cofactors
    let (bc, ca, ab) = (cross(b, c), cross(c, a), cross(a, b));
    let solve = |w: V3| -> V3 { [dot(w, bc) / det, dot(w, ca) / det, dot(w, ab) / det] };
    let o = solve(sub(origin, row(m, 3)));
    let d = solve(dir);
    (o.iter().chain(&d).all(|x| x.is_finite())).then_some((o, d))
}

/// Whether a ray (object space, `t` from just behind its origin on) meets a run's bounds, padded far beyond any
/// rounding of the placing and of the inverse (1e-4 m and a millionth of the bounds' size).
#[allow(
    clippy::indexing_slicing,
    reason = "[f64; 3] and [f32; 3] vectors indexed by their axes (0..3)"
)]
fn meets(o: V3, d: V3, b: &bc2_scene::raygrid::Bounds) -> bool {
    let (lo, hi) = b;
    let size = (0..3)
        .map(|a| f64::from(lo[a]).abs().max(f64::from(hi[a]).abs()))
        .fold(0.0, f64::max);
    let pad = 1e-4 + 1e-6 * size;
    let (mut t0, mut t1) = (-1.0f64, f64::INFINITY);
    for a in 0..3 {
        let (l, h) = (f64::from(lo[a]) - pad, f64::from(hi[a]) + pad);
        if d[a] == 0.0 {
            if o[a] < l || o[a] > h {
                return false;
            }
            continue;
        }
        let (p, q) = ((l - o[a]) / d[a], (h - o[a]) / d[a]);
        t0 = t0.max(p.min(q));
        t1 = t1.min(p.max(q));
        if t0 > t1 {
            return false;
        }
    }
    // bounds that are not numbers (an empty run): tested as before
    true
}

/// The first surface (terrain or placed collision) along a ray from `origin` in unit direction `dir`.
#[must_use]
pub fn ray_hit(scene: &Scene, origin: V3, dir: V3) -> Option<Hit> {
    ray_hit_except(scene, origin, dir, &|_| false)
}

/// [`ray_hit`] passing through the entities `skip` says yes to (the objects being dragged, deleted ones).
#[must_use]
pub fn ray_hit_except(scene: &Scene, origin: V3, dir: V3, skip: &dyn Fn(usize) -> bool) -> Option<Hit> {
    let mut best: Option<(f64, V3, Option<usize>, [V3; 3])> = None;
    let mut consider = |tri: [V3; 3], entity: Option<usize>| {
        if let Some((t, n)) = ray_triangle(origin, dir, tri)
            && best.is_none_or(|(bt, _, _, _)| t < bt)
        {
            best = Some((t, n, entity, tri));
        }
    };
    if let Some((v, tris)) = scene.terrain.as_deref() {
        let at = |k: u32| v.get(k as usize).map(|p| p.map(f64::from));
        let mut test = |t: &[u32; 3]| {
            if let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
                consider([a, b, c], None);
            }
        };
        // only the triangles near the ray's path (round HG1: the scene's terrain grid), in the mesh's order: the
        // same first hit as testing all of them
        match scene.terrain_candidates(origin, dir) {
            Some(near) => near.iter().filter_map(|&k| tris.get(k as usize)).for_each(&mut test),
            None => tris.iter().for_each(&mut test),
        }
    }
    if let Some(col) = &scene.collision {
        // the blueprints' radii and run bounds, locked once for the whole walk
        let mut shapes = scene.collision_shapes();
        for (index, _) in &col.meshes {
            if skip(*index) {
                continue;
            }
            // a bounding-sphere test first: most placed objects are nowhere near the ray
            let Some(e) = scene.entities.get(*index) else { continue };
            let Some(m) = e.matrix else { continue };
            let Some(Some(local)) = e.blueprint.as_ref().and_then(|b| col.blueprints.get(b)) else {
                continue;
            };
            // the blueprint's radius, measured once (round HG1: it was measured from every triangle for every ray)
            let shape = shapes.get(local);
            let radius = shape.radius;
            let scale = (0..3).map(|r| len(row(&m, r))).fold(0.0, f64::max);
            let centre = row(&m, 3);
            let oc = sub(centre, origin);
            let along = dot(oc, dir);
            let miss2 = dot(oc, oc) - along * along;
            let r = radius * scale;
            if along < -r || miss2 > r * r {
                continue;
            }
            // the blueprint's triangles in runs (round PR1b-0, audit AU5 ALG-2): only the runs whose bounds the ray
            // meets in the object's own space are placed and tested - in order, placed as world_triangles places
            // them, without a list of every placed triangle
            let in_object = object_ray(&m, origin, dir);
            for (k, b) in shape.runs.iter().enumerate() {
                if let Some((o, d)) = in_object
                    && !meets(o, d, b)
                {
                    continue;
                }
                let at = k * bc2_scene::raygrid::RUN;
                for tri in local
                    .get(at..(at + bc2_scene::raygrid::RUN).min(local.len()))
                    .unwrap_or_default()
                {
                    consider(tri.map(|v| bc2_scene::scene::place(&m, v)), Some(*index));
                }
            }
        }
    }
    let (t, n, entity, tri) = best?;
    let mut normal = norm(n);
    if len(normal) == 0.0 {
        normal = [0.0, 1.0, 0.0];
    }
    if dot(normal, dir) > 0.0 {
        normal = mul(normal, -1.0);
    }
    Some(Hit {
        t,
        point: add(origin, mul(dir, t)),
        normal,
        entity,
        tri,
    })
}

/// The surface straight below (or above) a point: the first hit of a vertical ray from high above it.
#[must_use]
pub fn surface_below(scene: &Scene, x: f64, z: f64) -> Option<Hit> {
    surface_below_except(scene, x, z, &|_| false)
}

/// [`surface_below`] passing through the entities `skip` says yes to.
#[must_use]
pub fn surface_below_except(scene: &Scene, x: f64, z: f64, skip: &dyn Fn(usize) -> bool) -> Option<Hit> {
    ray_hit_except(scene, [x, 100_000.0, z], [0.0, -1.0, 0.0], skip)
}

/// Where an object placed on a surface sits when sunk `depth` metres into it: along the surface normal into the
/// surface (`along_normal`), or straight down.
#[must_use]
pub fn sunk(hit: &Hit, depth: f64, along_normal: bool) -> V3 {
    let d = if along_normal { hit.normal } else { [0.0, 1.0, 0.0] };
    sub(hit.point, mul(d, depth))
}

/// Vertex snapping: the corner of the hit triangle nearest the hit point.
#[must_use]
pub fn snap_vertex(hit: &Hit) -> V3 {
    let mut best = hit.tri[0];
    for v in hit.tri {
        if len(sub(v, hit.point)) < len(sub(best, hit.point)) {
            best = v;
        }
    }
    best
}

/// Edge snapping: the point on the hit triangle's edges nearest the hit point.
#[must_use]
pub fn snap_edge(hit: &Hit) -> V3 {
    let mut best = hit.tri[0];
    let mut bd = f64::INFINITY;
    let [p, q, r] = hit.tri;
    for (a, b) in [(p, q), (q, r), (r, p)] {
        let ab = sub(b, a);
        let t = (dot(sub(hit.point, a), ab) / dot(ab, ab).max(1e-12)).clamp(0.0, 1.0);
        let q = add(a, mul(ab, t));
        let d = len(sub(q, hit.point));
        if d < bd {
            bd = d;
            best = q;
        }
    }
    best
}

/// An object's extent along its own up axis (local units, before its transform's scale): the lowest and highest
/// points of its collision, or of its marker (pivot at the bottom) when it has none.
#[must_use]
pub fn extent(scene: &Scene, entity: usize) -> (f64, f64) {
    let Some(e) = scene.entities.get(entity) else {
        return (0.0, 0.0);
    };
    let placed = scene
        .collision
        .as_ref()
        .filter(|c| c.meshes.iter().any(|(k, _)| *k == entity));
    let tris = placed.and_then(|c| c.blueprints.get(e.blueprint.as_deref()?).cloned().flatten());
    if let Some(t) = tris
        && !t.is_empty()
    {
        let ys = t.iter().flat_map(|tri| tri.iter().map(|p| f64::from(p[1])));
        return ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), y| (lo.min(y), hi.max(y)));
    }
    (0.0, crate::render::marker_size(&e.category).map_or(1.0, f64::from))
}

/// An object's height in metres (its extent along its up axis, scaled by its transform).
#[must_use]
pub fn height(scene: &Scene, entity: usize) -> f64 {
    let (lo, hi) = extent(scene, entity);
    let scale = scene
        .entities
        .get(entity)
        .and_then(|e| e.matrix)
        .map_or(1.0, |m| len(row(&m, 1)));
    (hi - lo) * scale
}

/// An object (transform `m`, extent `ext` from [`extent`]) placed on the surface at `hit`: sitting on it, turned so
/// its up axis is the normal when `p.align_to_normal`, then sunk `p.depth(height)` along the normal or straight
/// down.
#[must_use]
pub fn place(m: &M, ext: (f64, f64), hit: &Hit, p: &Placement) -> M {
    let mut out = *m;
    if p.align_to_normal {
        let up = norm(row(m, 1));
        let (axis, angle) = math::between(up, hit.normal);
        for r in 0..3 {
            math::set_row(&mut out, r, math::rotate_vec(row(m, r), axis, angle));
        }
    }
    let up = row(&out, 1);
    let height = (ext.1 - ext.0) * len(up);
    // the lowest point on the surface: the pivot sits ext.0 up-units below the bottom
    let base = sub(hit.point, mul(up, ext.0));
    let dir = if p.along_normal { hit.normal } else { [0.0, 1.0, 0.0] };
    let pos = sub(base, mul(dir, p.depth(height)));
    math::set_row(&mut out, 3, pos);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use bc2_levelwrite::SinkUnit;

    fn demo_height(x: f64, z: f64) -> f64 {
        ((x * 0.05).sin() + (z * 0.07).cos()) * 0.8 - 1.6
    }

    #[test]
    fn rays_meet_the_terrain_and_the_house() {
        let s = crate::demo::scene();
        // the house (entity 0) is a 10 x 6 x 8 m box on the origin: its roof is at 6 m
        let h = surface_below(&s, 1.0, 1.0).unwrap();
        assert_eq!(h.entity, Some(0));
        assert!((h.point[1] - 6.0).abs() < 1e-9);
        assert!((h.normal[1] - 1.0).abs() < 1e-9);
        // beside it, the terrain (the demo's height formula)
        let (x, z) = (-40.0f64, 30.0f64);
        let g = surface_below(&s, x, z).unwrap();
        assert_eq!(g.entity, None);
        assert!((g.point[1] - demo_height(x, z)).abs() < 0.05, "{}", g.point[1]);
        // a ray from the side meets the house's wall facing it
        let w = ray_hit(&s, [-100.0, 3.0, 0.5], [1.0, 0.0, 0.0]).unwrap();
        assert_eq!(w.entity, Some(0));
        assert!((w.point[0] + 5.0).abs() < 1e-9);
        assert!((w.normal[0] + 1.0).abs() < 1e-9);
        // sinking: 0.5 m into the roof, straight down or along the normal (the same on a flat roof)
        assert!((sunk(&h, 0.5, false)[1] - 5.5).abs() < 1e-9);
        assert!((sunk(&w, 0.5, true)[0] + 4.5).abs() < 1e-9);
        // nothing up in the sky
        assert!(ray_hit(&s, [0.0, 50.0, 0.0], [0.0, 1.0, 0.0]).is_none());
        // passing through the house reaches the terrain under it
        let under = surface_below_except(&s, 1.0, 1.0, &|k| k == 0).unwrap();
        assert_eq!(under.entity, None);
        assert!((under.point[1] - demo_height(1.0, 1.0)).abs() < 0.05);
    }

    #[test]
    fn snapping_to_corners_and_edges() {
        let s = crate::demo::scene();
        let h = surface_below(&s, 4.6, 3.7).unwrap();
        let v = snap_vertex(&h);
        assert!((v[0] - 5.0).abs() < 1e-9 && (v[1] - 6.0).abs() < 1e-9 && (v[2] - 4.0).abs() < 1e-9);
        let e = snap_edge(&surface_below(&s, 4.9, 0.5).unwrap());
        assert!((e[0] - 5.0).abs() < 1e-9 && (e[2] - 0.5).abs() < 1e-9, "{e:?}");
    }

    /// Placing on terrain and on a mesh, sunk 0, 0.5 m and 25 % of the object's height, straight down and along the
    /// normal: the heights expected from the terrain samples.
    #[test]
    fn placing_with_a_sink_depth() {
        let mut s = crate::demo::scene();
        let p = |sink: f64, unit: SinkUnit, along_normal: bool, align: bool| Placement {
            sink,
            unit,
            along_normal,
            align_to_normal: align,
        };
        // the house (6 m tall, its bottom at its pivot) on the terrain at (-40, 30)
        let (x, z) = (-40.0, 30.0);
        let hit = surface_below_except(&s, x, z, &|k| k == 0).unwrap();
        let ground = hit.point[1];
        assert!((ground - demo_height(x, z)).abs() < 0.05);
        let m = s.entities[0].matrix.unwrap();
        let ext = extent(&s, 0);
        assert_eq!(ext, (0.0, 6.0));
        assert!((height(&s, 0) - 6.0).abs() < 1e-12);
        for (sink, unit, want) in [
            (0.0, SinkUnit::Metres, 0.0),
            (0.5, SinkUnit::Metres, 0.5),
            (0.25, SinkUnit::Fraction, 1.5),
        ] {
            let placed = place(&m, ext, &hit, &p(sink, unit, false, false));
            let at = row(&placed, 3);
            assert!(
                (at[1] - (ground - want)).abs() < 1e-9,
                "sink {sink} {unit:?}: {}",
                at[1]
            );
            assert!((at[0] - x).abs() < 1e-9 && (at[2] - z).abs() < 1e-9);
            // along the normal: the same depth, measured along it
            let along = row(&place(&m, ext, &hit, &p(sink, unit, true, false)), 3);
            assert!((len(sub(along, hit.point)) - want).abs() < 1e-9);
            assert!((along[1] - (ground - want * hit.normal[1])).abs() < 1e-9);
        }
        // turned to the slope: the up axis is the normal
        let aligned = place(&m, ext, &hit, &p(0.0, SinkUnit::Metres, false, true));
        assert!(len(sub(norm(row(&aligned, 1)), hit.normal)) < 1e-9);
        // a spawn (a 1.2 m marker) on the house's roof (a mesh): 6 m, 5.5 m, 6 - 0.3 m
        let roof = surface_below_except(&s, 1.0, 1.0, &|k| k == 1).unwrap();
        assert_eq!(roof.entity, Some(0));
        let spawn = s.entities[1].matrix.unwrap();
        let ext = extent(&s, 1);
        for (sink, unit, want) in [
            (0.0, SinkUnit::Metres, 6.0),
            (0.5, SinkUnit::Metres, 5.5),
            (0.25, SinkUnit::Fraction, 5.7),
        ] {
            let at = row(&place(&spawn, ext, &roof, &p(sink, unit, false, false)), 3);
            // the marker's size is a single (1.2 m to 7 digits)
            assert!((at[1] - want).abs() < 1e-6, "{sink} {unit:?}: {}", at[1]);
        }
        // a scaled object: the fraction is of its scaled height
        s.entities[0].matrix = Some([2.0, 0., 0., 0., 0., 2., 0., 0., 0., 0., 2., 0., 0., 0., 0., 1.]);
        assert!((height(&s, 0) - 12.0).abs() < 1e-12);
    }
}
