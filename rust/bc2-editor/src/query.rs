//! Questions about the scene's surfaces, on the CPU: where a ray first meets the terrain or a placed object's
//! collision (with the surface normal there), and the point a given sink depth below a surface.
//!
//! The view uses it for the status bar's surface read-out (the point under a click); it is the base the editing
//! rounds build surface snapping on (`EDITOR_ARCHITECTURE.md` 3.10, item 3: an object dropped on terrain or a mesh
//! sits on it, optionally aligned to the normal, and a sink depth pushes it partly underground along the normal or
//! straight down). Picking *which* entity is under the cursor stays on the GPU (the id target, `render::pick`);
//! this answers *where* on its surface.

use bc2_scene::scene::Scene;

use crate::camera::V3;

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
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
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

/// The first surface (terrain or placed collision) along a ray from `origin` in unit direction `dir`.
#[must_use]
pub fn ray_hit(scene: &Scene, origin: V3, dir: V3) -> Option<Hit> {
    let mut best: Option<(f64, V3, Option<usize>)> = None;
    let mut consider = |tri: [V3; 3], entity: Option<usize>| {
        if let Some((t, n)) = ray_triangle(origin, dir, tri)
            && best.is_none_or(|(bt, _, _)| t < bt)
        {
            best = Some((t, n, entity));
        }
    };
    if let Some((v, tris)) = &scene.terrain {
        let at = |k: u32| v.get(k as usize).map(|p| p.map(f64::from));
        for t in tris {
            if let (Some(a), Some(b), Some(c)) = (at(t[0]), at(t[1]), at(t[2])) {
                consider([a, b, c], None);
            }
        }
    }
    if let Some(col) = &scene.collision {
        for (index, _) in &col.meshes {
            // a bounding-sphere test first: most placed objects are nowhere near the ray
            let Some(e) = scene.entities.get(*index) else { continue };
            let Some(m) = e.matrix else { continue };
            let Some(Some(local)) = e.blueprint.as_ref().and_then(|b| col.blueprints.get(b)) else {
                continue;
            };
            let radius = local
                .iter()
                .flat_map(|t| t.iter())
                .map(|p| f64::from(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
                .fold(0.0, f64::max);
            let scale = (0..3)
                .map(|r| {
                    (0..3)
                        .map(|c| m.get(r * 4 + c).copied().unwrap_or(0.0).powi(2))
                        .sum::<f64>()
                        .sqrt()
                })
                .fold(0.0, f64::max);
            let centre = [m[12], m[13], m[14]];
            let oc = sub(centre, origin);
            let along = dot(oc, dir);
            let miss2 = dot(oc, oc) - along * along;
            let r = radius * scale;
            if along < -r || miss2 > r * r {
                continue;
            }
            if let Some(world) = scene.world_triangles(*index) {
                for t in world {
                    consider(t, Some(*index));
                }
            }
        }
    }
    let (t, n, entity) = best?;
    let len = dot(n, n).sqrt();
    let mut normal = if len > 0.0 { n.map(|c| c / len) } else { [0.0, 1.0, 0.0] };
    if dot(normal, dir) > 0.0 {
        normal = normal.map(|c| -c);
    }
    Some(Hit {
        t,
        point: [origin[0] + dir[0] * t, origin[1] + dir[1] * t, origin[2] + dir[2] * t],
        normal,
        entity,
    })
}

/// The surface straight below (or above) a point: the first hit of a vertical ray from high above it.
#[must_use]
pub fn surface_below(scene: &Scene, x: f64, z: f64) -> Option<Hit> {
    ray_hit(scene, [x, 100_000.0, z], [0.0, -1.0, 0.0])
}

/// Where an object placed on a surface sits when sunk `depth` metres into it: along the surface normal into the
/// surface (`along_normal`), or straight down.
#[must_use]
pub fn sunk(hit: &Hit, depth: f64, along_normal: bool) -> V3 {
    let d = if along_normal { hit.normal } else { [0.0, 1.0, 0.0] };
    [
        hit.point[0] - d[0] * depth,
        hit.point[1] - d[1] * depth,
        hit.point[2] - d[2] * depth,
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

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
        let want = ((x * 0.05).sin() + (z * 0.07).cos()) * 0.8 - 1.6;
        assert!((g.point[1] - want).abs() < 0.05, "{} vs {want}", g.point[1]);
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
    }
}
