//! What a ray through the scene needs to look at (round HG1): the terrain triangles near its path and each collision
//! blueprint's bounding radius, kept with the scene so a ray costs what it touches, not the whole level.
//!
//! The editor's CPU ray (`bc2_editor::query::ray_hit_except`: the walk camera's ground below the feet every frame,
//! placing on a surface, snapping) used to test every terrain triangle - two million on a 1024 x 1024 height field at
//! stride 1 - and to measure each placed object's blueprint radius from all its triangles, for every ray. `SP_01`'s walk
//! test spent minutes on it. Here:
//!
//! - [`TerrainGrid`]: the terrain mesh's triangles bucketed by their plan (x, z) bounds on a uniform grid of about
//!   [`TRIS_PER_CELL`] triangles a cell. [`TerrainGrid::candidates`] lists the triangles of every cell the ray's
//!   plan passes over (a vertical ray: one cell), in mesh order. Each triangle's bounds and the ray's path are padded
//!   by [`PAD`] metres, far more than any rounding in the ray test, so every triangle the ray can meet is listed; the
//!   caller tests them in mesh order exactly as before, so the hit is the same, bit for bit.
//! - Each blueprint's radius (the farthest triangle corner from its origin), computed once per triangle list.
//!
//! Both live in the scene's [`RayCache`], which the scene's clones share (an undo's scene is a clone of the base's:
//! its first ray no longer builds the grid again). The terrain grid is built on the first ray and kept while the
//! scene's terrain mesh is the same `Arc` (the mesh is never edited in place: an edit replaces the `Arc`; the one the
//! grid was built from is held, so its address is never reused); a radius is kept per blueprint triangle list (its
//! `Arc`, held likewise).

use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use bc2_formats::havok::Triangle;

use crate::scene::TerrainMesh;

/// The triangles a grid cell holds on average (a cell is about one height-field square at stride 1).
pub const TRIS_PER_CELL: f64 = 4.0;

/// Metres each triangle's plan bounds and the ray's path are widened by: the ray test's rounding is ~1e-12 m.
pub const PAD: f64 = 1e-3;

/// The most cells along a side.
const MAX_SIDE: usize = 4096;

/// The terrain mesh's triangles bucketed on a uniform plan grid ([`TRIS_PER_CELL`] a cell).
#[derive(Debug, Clone, Default)]
pub struct TerrainGrid {
    x0: f64,
    z0: f64,
    cell: f64,
    nx: usize,
    nz: usize,
    /// Cell `k`'s triangles: `tris[starts[k]..starts[k + 1]]`, ascending.
    starts: Vec<u32>,
    tris: Vec<u32>,
    /// Triangles with a corner that is not finite: listed for every ray (the test answers them as before).
    always: Vec<u32>,
}

impl TerrainGrid {
    /// The grid of a terrain mesh. Triangles naming a vertex the mesh does not have are left out (the ray test skips
    /// them).
    #[must_use]
    #[allow(
        clippy::cast_precision_loss,
        reason = "cell counts are below 2^24, exact in f64; triangle indices are u32 as the mesh's"
    )]
    pub fn new(mesh: &TerrainMesh) -> Self {
        let (v, tris) = mesh;
        let at = |k: u32| v.get(k as usize).map(|p| p.map(f64::from));
        // a triangle's padded plan bounds: None for one with a missing corner, Some(None) for one not finite
        let bbox = |t: &[u32; 3]| -> Option<Option<[f64; 4]>> {
            let (a, b, c) = (at(t[0])?, at(t[1])?, at(t[2])?);
            if ![a, b, c].iter().flatten().all(|x| x.is_finite()) {
                return Some(None);
            }
            Some(Some([
                a[0].min(b[0]).min(c[0]) - PAD,
                a[2].min(b[2]).min(c[2]) - PAD,
                a[0].max(b[0]).max(c[0]) + PAD,
                a[2].max(b[2]).max(c[2]) + PAD,
            ]))
        };
        let boxes = || {
            tris.iter()
                .enumerate()
                .filter_map(move |(i, t)| bbox(t).flatten().map(|b| (i as u32, b)))
        };
        let always: Vec<u32> = tris
            .iter()
            .enumerate()
            .filter(|(_, t)| bbox(t).is_some_and(|b| b.is_none()))
            .map(|(i, _)| i as u32)
            .collect();
        let mut placed = 0usize;
        let Some(bounds) = boxes()
            .map(|b| {
                placed += 1;
                b.1
            })
            .reduce(|a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])])
        else {
            return Self {
                always,
                ..Self::default()
            };
        };
        let (w, d) = (bounds[2] - bounds[0], bounds[3] - bounds[1]);
        let cells = (placed as f64 / TRIS_PER_CELL).max(1.0);
        let mut cell = (w.max(PAD) * d.max(PAD) / cells).sqrt();
        // no side longer than MAX_SIDE cells
        cell = cell
            .max(w / (MAX_SIDE - 1) as f64)
            .max(d / (MAX_SIDE - 1) as f64)
            .max(PAD);
        let side = |extent: f64| ((extent / cell).floor() as usize + 1).clamp(1, MAX_SIDE);
        let mut g = Self {
            x0: bounds[0],
            z0: bounds[1],
            cell,
            nx: side(w),
            nz: side(d),
            starts: Vec::new(),
            tris: Vec::new(),
            always,
        };
        // counted, then placed: each cell's triangles in mesh order
        let mut count = vec![0u32; g.nx * g.nz + 1];
        for (_, b) in boxes() {
            let (i0, i1, j0, j1) = g.span(&b);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    if let Some(c) = count.get_mut(j * g.nx + i) {
                        *c += 1;
                    }
                }
            }
        }
        let mut starts = Vec::with_capacity(count.len());
        let mut sum = 0u32;
        for c in &count {
            starts.push(sum);
            sum += c;
        }
        let mut fill = starts.clone();
        let mut out = vec![0u32; sum as usize];
        for (t, b) in boxes() {
            let (i0, i1, j0, j1) = g.span(&b);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    if let Some(f) = fill.get_mut(j * g.nx + i) {
                        if let Some(slot) = out.get_mut(*f as usize) {
                            *slot = t;
                        }
                        *f += 1;
                    }
                }
            }
        }
        g.starts = starts;
        g.tris = out;
        g
    }

    /// The cells (first and last column, first and last row) a plan box overlaps, clamped to the grid.
    fn span(&self, b: &[f64; 4]) -> (usize, usize, usize, usize) {
        let col = |x: f64| (((x - self.x0) / self.cell).floor().max(0.0) as usize).min(self.nx - 1);
        let row = |z: f64| (((z - self.z0) / self.cell).floor().max(0.0) as usize).min(self.nz - 1);
        (col(b[0]), col(b[2]), row(b[1]), row(b[3]))
    }

    /// The triangles a ray from `origin` along `dir` can meet, ascending (each once); `None` when the ray is not
    /// finite (then every triangle is to be tested).
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "cell counts are below 2^24, exact in f64")]
    pub fn candidates(&self, origin: [f64; 3], dir: [f64; 3]) -> Option<Vec<u32>> {
        if !origin.iter().chain(&dir).all(|x| x.is_finite()) {
            return None;
        }
        let mut out = self.always.clone();
        if self.nx == 0 || self.nz == 0 {
            return Some(out);
        }
        let (ox, oz, dx, dz) = (origin[0], origin[2], dir[0], dir[2]);
        // the grid's plan rectangle, widened once more
        let (lo_x, lo_z) = (self.x0 - PAD, self.z0 - PAD);
        let (hi_x, hi_z) = (
            self.x0 + self.nx as f64 * self.cell + PAD,
            self.z0 + self.nz as f64 * self.cell + PAD,
        );
        // the ray's parameter range over the rectangle (t >= 0: the test only meets triangles ahead)
        let (mut t0, mut t1) = (0.0f64, f64::INFINITY);
        for (o, d, lo, hi) in [(ox, dx, lo_x, hi_x), (oz, dz, lo_z, hi_z)] {
            if d == 0.0 {
                if o < lo || o > hi {
                    return Some(out);
                }
            } else {
                let (a, b) = ((lo - o) / d, (hi - o) / d);
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
            }
        }
        if t1 < t0 {
            return Some(out);
        }
        let mut add = |b: [f64; 4]| {
            let (i0, i1, j0, j1) = self.span(&b);
            for j in j0..=j1 {
                for i in i0..=i1 {
                    let k = j * self.nx + i;
                    let (Some(&s), Some(&e)) = (self.starts.get(k), self.starts.get(k + 1)) else {
                        continue;
                    };
                    out.extend_from_slice(self.tris.get(s as usize..e as usize).unwrap_or_default());
                }
            }
        };
        let plan = dx.hypot(dz);
        if !t1.is_finite() || plan == 0.0 {
            // straight up or down: the cells around the origin's plan point
            add([ox - PAD, oz - PAD, ox + PAD, oz + PAD]);
        } else {
            // the path over the grid in pieces no longer than a cell, each piece's padded bounds
            let pieces = (((t1 - t0) * plan / self.cell).ceil() as usize).clamp(1, 4 * MAX_SIDE);
            let step = (t1 - t0) / pieces as f64;
            for k in 0..pieces {
                let ta = t0 + step * k as f64;
                let tb = if k + 1 == pieces { t1 } else { ta + step };
                let (ax, az, bx, bz) = (ox + dx * ta, oz + dz * ta, ox + dx * tb, oz + dz * tb);
                add([ax.min(bx) - PAD, az.min(bz) - PAD, ax.max(bx) + PAD, az.max(bz) + PAD]);
            }
        }
        out.sort_unstable();
        out.dedup();
        Some(out)
    }
}

/// The farthest corner of a blueprint's triangles from its origin, in metres (the ray test's bounding sphere).
#[must_use]
pub fn radius_of(tris: &[Triangle]) -> f64 {
    tris.iter()
        .flat_map(|t| t.iter())
        .map(|p| f64::from(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
        .fold(0.0, f64::max)
}

/// The terrain grid of the mesh it was built from (a weak reference: a mesh no scene holds is freed, and its `Arc`'s
/// block, kept by the weak reference, is never another mesh's).
type Keyed = (Weak<TerrainMesh>, Arc<TerrainGrid>);

/// What an object ray needs of a blueprint, by its triangle list's address (the list held, so the address is not
/// reused).
type ShapeMap = HashMap<usize, Shape>;

/// What an object ray needs of a blueprint's triangles: their bounding radius and their runs' bounds.
#[derive(Debug)]
pub struct Shape {
    /// The triangle list (held: its address is the key).
    _held: Arc<Vec<Triangle>>,
    /// [`radius_of`] the triangles.
    pub radius: f64,
    /// [`runs_of`] the triangles.
    pub runs: Vec<Bounds>,
}

/// The blueprints' [`Shape`]s, locked for a ray's walk over the placed objects (one lock a ray, not one an
/// object: round PR1b-0, audit AU5 ALG-2).
pub struct Shapes<'a>(MutexGuard<'a, ShapeMap>);

impl Shapes<'_> {
    /// The shape of a blueprint's triangles, measured the first time.
    pub fn get(&mut self, tris: &Arc<Vec<Triangle>>) -> &Shape {
        self.0.entry(Arc::as_ptr(tris) as usize).or_insert_with(|| Shape {
            _held: Arc::clone(tris),
            radius: radius_of(tris),
            runs: runs_of(tris),
        })
    }
}

impl std::fmt::Debug for Shapes<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Shapes")
    }
}

/// The triangles of a blueprint a [`Bounds`] covers (consecutive, in the blueprint's order).
pub const RUN: usize = 16;

/// The least and greatest corner of a run of [`RUN`] triangles, in the blueprint's own space.
pub type Bounds = ([f32; 3], [f32; 3]);

/// The bounds of each run of [`RUN`] triangles of a blueprint (the last run may be shorter). An object ray (round
/// PR1b-0, audit AU5 ALG-2) places and tests only the runs whose bounds, padded, its path in the object's space
/// meets: every triangle it could meet is still tested, in the same order, so the hit is the same.
#[must_use]
pub fn runs_of(tris: &[Triangle]) -> Vec<Bounds> {
    tris.chunks(RUN)
        .map(|run| {
            let mut lo = [f32::INFINITY; 3];
            let mut hi = [f32::NEG_INFINITY; 3];
            for p in run.iter().flatten() {
                for ((l, h), v) in lo.iter_mut().zip(hi.iter_mut()).zip(p) {
                    *l = l.min(*v);
                    *h = h.max(*v);
                }
            }
            (lo, hi)
        })
        .collect()
}

/// The most terrain grids kept at once (a scene and its clones may hold different meshes once the terrain is edited).
const GRIDS: usize = 4;

/// The structures themselves, shared by every clone of a [`RayCache`].
#[derive(Default)]
struct Shared {
    terrain: Mutex<Vec<Keyed>>,
    shapes: Mutex<ShapeMap>,
}

/// A scene's ray structures, built when first asked for (see the module). A clone shares them: each is kept with
/// what it was built from, so a scene and its clones (the base and an undo's scene) build each grid once.
#[derive(Default, Clone)]
pub struct RayCache {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for RayCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RayCache")
    }
}

impl RayCache {
    /// The grid of `mesh`, built on the first call for that mesh (that `Arc`).
    pub fn terrain(&self, mesh: &Arc<TerrainMesh>) -> Arc<TerrainGrid> {
        let mut grids = self.shared.terrain.lock().unwrap_or_else(PoisonError::into_inner);
        grids.retain(|(m, _)| m.strong_count() > 0);
        if let Some((_, g)) = grids.iter().find(|(m, _)| std::ptr::eq(m.as_ptr(), Arc::as_ptr(mesh))) {
            return Arc::clone(g);
        }
        let g = Arc::new(TerrainGrid::new(mesh));
        if grids.len() >= GRIDS {
            grids.remove(0);
        }
        grids.push((Arc::downgrade(mesh), Arc::clone(&g)));
        g
    }

    /// [`radius_of`] a blueprint's triangles, computed once per triangle list.
    #[must_use]
    pub fn radius(&self, tris: &Arc<Vec<Triangle>>) -> f64 {
        self.shapes().get(tris).radius
    }

    /// The blueprints' shapes, locked until the answer is dropped.
    pub fn shapes(&self) -> Shapes<'_> {
        Shapes(self.shared.shapes.lock().unwrap_or_else(PoisonError::into_inner))
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::many_single_char_names
)]
mod tests {
    use super::*;

    /// A bumpy `n` x `n` grid mesh of 1 m squares, its triangles as a height field's.
    fn mesh(n: u32) -> TerrainMesh {
        let mut v = Vec::new();
        for j in 0..n {
            for i in 0..n {
                let h = ((i * 7 + j * 13) % 5) as f32 * 0.3;
                v.push([i as f32 - 3.5, h, j as f32 + 0.25]);
            }
        }
        let mut t = Vec::new();
        for j in 0..n - 1 {
            for i in 0..n - 1 {
                let a = j * n + i;
                t.push([a, a + n, a + n + 1]);
                t.push([a, a + n + 1, a + 1]);
            }
        }
        (v, t)
    }

    /// The triangles a plan segment passes over, by brute force (their padded bounds meet the segment's).
    fn listed_everywhere(g: &TerrainGrid, origin: [f64; 3], dir: [f64; 3], m: &TerrainMesh) {
        let c = g.candidates(origin, dir).unwrap();
        assert!(c.windows(2).all(|w| w[0] < w[1]), "ascending, each once");
        // every triangle under any point of the ray ahead (sampled finely) is listed
        for k in 0..4000 {
            let t = f64::from(k) * 0.01;
            let p = [origin[0] + dir[0] * t, origin[2] + dir[2] * t];
            for (i, tri) in m.1.iter().enumerate() {
                let cs: Vec<[f64; 3]> = tri.iter().map(|&x| m.0[x as usize].map(f64::from)).collect();
                let (lx, hx) = (
                    cs.iter().map(|c| c[0]).fold(f64::MAX, f64::min),
                    cs.iter().map(|c| c[0]).fold(f64::MIN, f64::max),
                );
                let (lz, hz) = (
                    cs.iter().map(|c| c[2]).fold(f64::MAX, f64::min),
                    cs.iter().map(|c| c[2]).fold(f64::MIN, f64::max),
                );
                if p[0] >= lx && p[0] <= hx && p[1] >= lz && p[1] <= hz {
                    assert!(c.binary_search(&(i as u32)).is_ok(), "triangle {i} under {p:?}");
                }
            }
        }
    }

    #[test]
    fn every_triangle_under_the_ray_is_listed() {
        let m = mesh(24);
        let g = TerrainGrid::new(&m);
        assert!(g.nx > 1 && g.nz > 1);
        // vertical, on a vertex, on a cell edge, slanted, along an axis, from outside the grid
        listed_everywhere(&g, [2.0, 10.0, 5.25], [0.0, -1.0, 0.0], &m);
        listed_everywhere(&g, [0.5, 10.0, 3.75], [0.0, -1.0, 0.0], &m);
        listed_everywhere(&g, [-3.0, 10.0, 1.0], [0.6, -0.48, 0.64], &m);
        listed_everywhere(&g, [-3.5, 2.0, 7.25], [1.0, 0.0, 0.0], &m);
        listed_everywhere(&g, [-10.0, 5.0, -4.0], [0.5, -0.1, 0.86], &m);
        // a ray away from the grid lists nothing
        assert_eq!(g.candidates([-10.0, 5.0, -4.0], [-1.0, 0.0, 0.0]), Some(Vec::new()));
        // a ray that is not finite: every triangle
        assert_eq!(g.candidates([f64::NAN, 0.0, 0.0], [0.0, -1.0, 0.0]), None);
    }

    #[test]
    fn missing_and_unfinite_corners() {
        let mut m = mesh(4);
        m.1.push([0, 1, 999]);
        m.0[5][1] = f32::NAN;
        let g = TerrainGrid::new(&m);
        let c = g.candidates([100.0, 1.0, 100.0], [0.0, -1.0, 0.0]).unwrap();
        // the triangles touching vertex 5 always, the one naming vertex 999 never
        assert_ne!(c.len(), 0);
        assert!(c.iter().all(|&t| m.1[t as usize].contains(&5)));
        assert!(!c.contains(&((m.1.len() - 1) as u32)));
    }

    #[test]
    fn the_cache_follows_the_mesh() {
        let c = RayCache::default();
        let a = Arc::new(mesh(6));
        let g1 = c.terrain(&a);
        assert!(Arc::ptr_eq(&g1, &c.terrain(&a)));
        let b = Arc::new(mesh(8));
        let g2 = c.terrain(&b);
        assert!(!Arc::ptr_eq(&g1, &g2));
        // a clone shares the grids: the first mesh's is still there (two meshes kept at once)
        let c2 = c.clone();
        assert!(Arc::ptr_eq(&g1, &c2.terrain(&a)));
        assert!(Arc::ptr_eq(&g2, &c2.terrain(&b)));
        // the same vertices in another Arc: another mesh (an edit makes a new Arc)
        let a2 = Arc::new(mesh(6));
        assert!(!Arc::ptr_eq(&g1, &c.terrain(&a2)));
        // a mesh no longer held: its grid goes at the next look-up
        drop(a);
        let _ = c.terrain(&b);
        assert_eq!(c.shared.terrain.lock().unwrap().len(), 2);
        let tris = Arc::new(vec![[[3.0f32, 4.0, 0.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]]]);
        assert!((c.radius(&tris) - 5.0).abs() < 1e-12);
        assert!((c.radius(&tris) - 5.0).abs() < 1e-12);
    }
}
