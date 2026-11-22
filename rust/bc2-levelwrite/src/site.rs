//! The ground a level's gameplay stands on (round A1): the terrain's height field, the water, the static collision and
//! where the level's art is, as one set of queries the auto layout ([`crate::autolayout`]) and the placement rules of
//! the export's validation ([`crate::validate`]: XVAL-PLACE-2 to XVAL-PLACE-4) read.
//!
//! - **Ground**: the height field ([`bc2_formats::terrain::Heightfield::height`], the game's diagonal split); a place
//!   outside its bounds has none.
//! - **Water**: the level's water mesh (`<terrain>.Water.Mesh.res`, [`bc2_formats::water`]) rasterised on 4 m cells,
//!   each cell the highest water surface over its centre.
//! - **Collision**: every static object's collision triangles in world space (the scene's
//!   [`bc2_scene::scene::Scene::world_triangles`]: placed objects and static group members), indexed on 8 m cells,
//!   each triangle with its object. A point is **inside** an object when a vertical line through it crosses that
//!   object's triangles an odd number of times above it and an odd number of times below it (a closed solid: a rock,
//!   a wall's thickness; a single-sided roof or a canopy is not a solid), the test DICE's own spawn points pass on
//!   every stock level measured ([`Site::spot`]).
//! - **Art**: how many static objects stand within 48 m of a place, as a share of the level's busiest places (the
//!   95th percentile of the cells with any), a destructible object counting 4 times a fixed one: DICE puts flags,
//!   bases and M-COMs where the level's buildings are, and builds the played part of a level destructible (the
//!   backdrop beyond the combat area is mostly fixed scenery).
//!
//! The site is the base level's: the project's own edits move gameplay, not the world it stands in.
#![allow(
    clippy::cast_precision_loss,
    reason = "cell indices and counts as f64: far below 2^52"
)]

use std::collections::HashSet;

use bc2_formats::terrain::Heightfield;
use bc2_formats::water::WaterMesh;

/// The water raster's cell (metres).
const WATER_CELL: f64 = 4.0;
/// The collision index's cell (metres).
const TRI_CELL: f64 = 8.0;
/// The art raster's cell (metres) and the radius objects count within.
const ART_CELL: f64 = 8.0;
const ART_RADIUS: f64 = 48.0;
/// A triangle wider than this (metres, in plan) is ground, not an object (DICE's large collision sheets): it is
/// left out of the solids.
const SHEET: f64 = 160.0;
/// How deep a place may be inside an object's collision before it is a bad spot (metres up to the object's surface):
/// DICE's own spawn points are up to 4.2 m into a rock's or a boulder's collision on the stock levels measured
/// (`nam_mp_005r`), which the game pushes a soldier out of.
pub const BURIED: f64 = 4.5;
/// How far below a place an object's surface counts as the floor it stands on (an interior, not a solid).
pub const FLOOR: f64 = 0.6;
/// How far below the ground a place may be before it is a bad spot: DICE's deepest spawn point is 4.6 m under the
/// height field (under a cliff on `mp_008`).
pub const UNDERGROUND: f64 = 6.0;

/// What stands at a place, for a soldier or a vehicle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spot {
    /// Solid ground or a surface to stand on.
    Ground,
    /// Outside the terrain's bounds.
    OutsideTerrain,
    /// Over water this deep (metres from the surface to what is under it).
    Water(f64),
    /// Inside a static object's collision (a rock, a wall), this far below its surface (more than [`BURIED`]).
    Solid(f64),
    /// Below the terrain's surface by this much (metres).
    Underground(f64),
}

/// A level's ground, water, collision and art.
#[derive(Debug, Clone)]
pub struct Site {
    hf: Option<Heightfield>,
    /// The terrain's bounds (x0, z0, x1, z1).
    pub bounds: [f64; 4],
    // water: surface per 4 m cell, NaN where none
    wn: [usize; 2],
    water: Vec<f32>,
    // collision: triangles, their objects, and the 8 m index
    tris: Vec<[[f32; 3]; 3]>,
    owner: Vec<u32>,
    tn: [usize; 2],
    index: Vec<Vec<u32>>,
    // art: normalised density per 8 m cell
    an: [usize; 2],
    art: Vec<f32>,
    /// Each object's name (its blueprint, else its guid), when the site was made from a scene.
    names: Vec<String>,
    /// Objects that never bury a place (vegetation: bushes and their collision boxes), by number.
    soft: Vec<bool>,
}

/// A triangle's height at (x, z), when the vertical line through it meets the triangle.
fn tri_height(t: &[[f32; 3]; 3], x: f64, z: f64) -> Option<f64> {
    let [a, b, c] = t.map(|v| v.map(f64::from));
    let d = (b[2] - c[2]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[2] - c[2]);
    if d.abs() < 1e-12 {
        return None;
    }
    let l1 = ((b[2] - c[2]) * (x - c[0]) + (c[0] - b[0]) * (z - c[2])) / d;
    let l2 = ((c[2] - a[2]) * (x - c[0]) + (a[0] - c[0]) * (z - c[2])) / d;
    let l3 = 1.0 - l1 - l2;
    let eps = -1e-9;
    (l1 >= eps && l2 >= eps && l3 >= eps).then(|| l1 * a[1] + l2 * b[1] + l3 * c[1])
}

impl Site {
    /// A site from its parts: the height field (`None`: no terrain, every place outside it), the water mesh, the
    /// static objects' collision (triangles per object, world space) and the static objects' positions with their
    /// weights in the art (a destructible object 1, a fixed one 0.25).
    #[must_use]
    pub fn new(
        hf: Option<Heightfield>,
        water: Option<&WaterMesh>,
        solids: &[Vec<[[f64; 3]; 3]>],
        objects: &[([f64; 3], f64)],
    ) -> Self {
        let bounds = hf.as_ref().map_or_else(
            || {
                // no terrain: the box of the collision and the water
                let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
                let wv = water.map_or(&[][..], |w| w.vertices.as_slice());
                for v in solids
                    .iter()
                    .flatten()
                    .flatten()
                    .copied()
                    .chain(wv.iter().map(|v| v.map(f64::from)))
                {
                    b = [b[0].min(v[0]), b[1].min(v[2]), b[2].max(v[0]), b[3].max(v[2])];
                }
                if b[0].is_finite() { b } else { [0.0; 4] }
            },
            |h| {
                [
                    f64::from(h.min[0]),
                    f64::from(h.min[2]),
                    f64::from(h.max[0]),
                    f64::from(h.max[2]),
                ]
            },
        );
        let dims = |cell: f64| {
            [
                (((bounds[2] - bounds[0]) / cell).ceil() as usize).max(1),
                (((bounds[3] - bounds[1]) / cell).ceil() as usize).max(1),
            ]
        };
        let mut s = Self {
            hf,
            bounds,
            wn: dims(WATER_CELL),
            water: Vec::new(),
            tris: Vec::new(),
            owner: Vec::new(),
            tn: dims(TRI_CELL),
            index: Vec::new(),
            an: dims(ART_CELL),
            art: Vec::new(),
            names: Vec::new(),
            soft: Vec::new(),
        };
        s.water = vec![f32::NAN; s.wn[0] * s.wn[1]];
        if let Some(w) = water {
            s.rasterise_water(w);
        }
        s.index = vec![Vec::new(); s.tn[0] * s.tn[1]];
        for (k, obj) in solids.iter().enumerate() {
            for t in obj {
                s.add_triangle(t, k as u32);
            }
        }
        s.splat_art(objects);
        s
    }

    fn cell_of(&self, x: f64, z: f64, cell: f64, n: [usize; 2]) -> Option<usize> {
        let i = ((x - self.bounds[0]) / cell).floor();
        let j = ((z - self.bounds[1]) / cell).floor();
        (i >= 0.0 && j >= 0.0 && (i as usize) < n[0] && (j as usize) < n[1]).then(|| j as usize * n[0] + i as usize)
    }

    fn rasterise_water(&mut self, w: &WaterMesh) {
        for t in &w.triangles {
            let v: Vec<[f32; 3]> = t.iter().filter_map(|&i| w.vertices.get(i as usize).copied()).collect();
            let Ok(tri) = <[[f32; 3]; 3]>::try_from(v) else {
                continue;
            };
            let xs = tri.map(|p| f64::from(p[0]));
            let zs = tri.map(|p| f64::from(p[2]));
            let lo = |a: [f64; 3]| a.iter().copied().fold(f64::INFINITY, f64::min);
            let hi = |a: [f64; 3]| a.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            let i0 = (((lo(xs) - self.bounds[0]) / WATER_CELL).floor().max(0.0)) as usize;
            let j0 = (((lo(zs) - self.bounds[1]) / WATER_CELL).floor().max(0.0)) as usize;
            let i1 = (((hi(xs) - self.bounds[0]) / WATER_CELL).ceil().max(0.0) as usize).min(self.wn[0]);
            let j1 = (((hi(zs) - self.bounds[1]) / WATER_CELL).ceil().max(0.0) as usize).min(self.wn[1]);
            for j in j0..j1 {
                for i in i0..i1 {
                    let x = self.bounds[0] + (i as f64 + 0.5) * WATER_CELL;
                    let z = self.bounds[1] + (j as f64 + 0.5) * WATER_CELL;
                    if let Some(h) = tri_height(&tri, x, z)
                        && let Some(c) = self.water.get_mut(j * self.wn[0] + i)
                        && (c.is_nan() || f64::from(*c) < h)
                    {
                        *c = h as f32;
                    }
                }
            }
        }
    }

    fn add_triangle(&mut self, t: &[[f64; 3]; 3], owner: u32) {
        let (xs, zs) = (t.map(|v| v[0]), t.map(|v| v[2]));
        let lo = |a: [f64; 3]| a.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = |a: [f64; 3]| a.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let (x0, x1, z0, z1) = (lo(xs), hi(xs), lo(zs), hi(zs));
        if !(x0.is_finite() && x1.is_finite() && z0.is_finite() && z1.is_finite())
            || x1 - x0 > SHEET
            || z1 - z0 > SHEET
            || x1 < self.bounds[0]
            || z1 < self.bounds[1]
            || x0 > self.bounds[2]
            || z0 > self.bounds[3]
        {
            return;
        }
        let k = self.tris.len() as u32;
        self.tris.push(t.map(|v| v.map(|c| c as f32)));
        self.owner.push(owner);
        let cl = |v: f64, o: f64, n: usize| (((v - o) / TRI_CELL).floor().max(0.0) as usize).min(n - 1);
        for j in cl(z0, self.bounds[1], self.tn[1])..=cl(z1, self.bounds[1], self.tn[1]) {
            for i in cl(x0, self.bounds[0], self.tn[0])..=cl(x1, self.bounds[0], self.tn[0]) {
                if let Some(c) = self.index.get_mut(j * self.tn[0] + i) {
                    c.push(k);
                }
            }
        }
    }

    fn splat_art(&mut self, objects: &[([f64; 3], f64)]) {
        let [nx, nz] = self.an;
        // weights in quarters, summed as whole numbers (the same sums in any order)
        let mut counts = vec![0u32; nx * nz];
        for (p, wt) in objects {
            if let Some(c) = self
                .cell_of(p[0], p[2], ART_CELL, self.an)
                .and_then(|c| counts.get_mut(c))
            {
                *c += (wt * 4.0).round().max(0.0) as u32;
            }
        }
        // a disc of ART_RADIUS: summed-area table, then each cell's square of side 2r (a box, close to the disc)
        let r = (ART_RADIUS / ART_CELL).round() as usize;
        let mut sat = vec![0u64; (nx + 1) * (nz + 1)];
        for j in 0..nz {
            let mut row = 0u64;
            for i in 0..nx {
                row += u64::from(counts.get(j * nx + i).copied().unwrap_or(0));
                let above = sat.get(j * (nx + 1) + i + 1).copied().unwrap_or(0);
                if let Some(s) = sat.get_mut((j + 1) * (nx + 1) + i + 1) {
                    *s = above + row;
                }
            }
        }
        let at = |i: usize, j: usize| sat.get(j * (nx + 1) + i).copied().unwrap_or(0);
        let mut dens = vec![0f32; nx * nz];
        for j in 0..nz {
            for i in 0..nx {
                let (i0, i1) = (i.saturating_sub(r), (i + r + 1).min(nx));
                let (j0, j1) = (j.saturating_sub(r), (j + r + 1).min(nz));
                let v = at(i1, j1) + at(i0, j0) - at(i0, j1) - at(i1, j0);
                if let Some(d) = dens.get_mut(j * nx + i) {
                    *d = v as f32;
                }
            }
        }
        let mut nz_vals: Vec<f32> = dens.iter().copied().filter(|v| *v > 0.0).collect();
        nz_vals.sort_by(f32::total_cmp);
        let p95 = nz_vals.get(nz_vals.len() * 95 / 100).copied().unwrap_or(1.0).max(1.0);
        self.art = dens.into_iter().map(|v| (v / p95).min(1.0)).collect();
    }

    /// Whether the site has a water mesh (a level without one has no water the rules can see).
    #[must_use]
    pub fn has_water(&self) -> bool {
        self.water.iter().any(|v| !v.is_nan())
    }

    /// Whether the site has a terrain.
    #[must_use]
    pub fn has_terrain(&self) -> bool {
        self.hf.is_some()
    }

    /// The ground's height at (x, z); `None` outside the terrain.
    #[must_use]
    pub fn ground(&self, x: f64, z: f64) -> Option<f64> {
        let h = self.hf.as_ref()?;
        (x >= self.bounds[0] && x <= self.bounds[2] && z >= self.bounds[1] && z <= self.bounds[3])
            .then(|| h.height(x, z))
    }

    /// The ground's slope at (x, z) in degrees (over 2 m each way); `None` outside the terrain.
    #[must_use]
    pub fn slope(&self, x: f64, z: f64) -> Option<f64> {
        let (a, b) = (self.ground(x - 2.0, z)?, self.ground(x + 2.0, z)?);
        let (c, d) = (self.ground(x, z - 2.0)?, self.ground(x, z + 2.0)?);
        let g = ((b - a) / 4.0).hypot((d - c) / 4.0);
        Some(g.atan().to_degrees())
    }

    /// The water's surface at (x, z), where there is water.
    #[must_use]
    pub fn water(&self, x: f64, z: f64) -> Option<f64> {
        let c = self.cell_of(x, z, WATER_CELL, self.wn)?;
        self.water.get(c).copied().filter(|v| !v.is_nan()).map(f64::from)
    }

    /// The art around (x, z): 0 (no static object within 48 m) to 1 (as busy as the level's busiest places).
    #[must_use]
    pub fn art(&self, x: f64, z: f64) -> f64 {
        self.cell_of(x, z, ART_CELL, self.an)
            .and_then(|c| self.art.get(c))
            .map_or(0.0, |v| f64::from(*v))
    }

    /// The collision heights on the vertical line through (x, z), each with its object.
    fn column(&self, x: f64, z: f64) -> Vec<(f64, u32)> {
        let Some(c) = self.cell_of(x, z, TRI_CELL, self.tn) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for &k in self.index.get(c).map_or(&[][..], Vec::as_slice) {
            if let (Some(t), Some(&o)) = (self.tris.get(k as usize), self.owner.get(k as usize))
                && let Some(h) = tri_height(t, x, z)
            {
                out.push((h, o));
            }
        }
        out
    }

    /// Whether `p` is inside a static object's collision (module doc: odd crossings above and below, per object).
    #[must_use]
    pub fn inside_solid(&self, p: [f64; 3]) -> bool {
        self.solid_at(p).is_some()
    }

    /// How deep `p` is inside a static object: the object (its name when the site has names) and the distance up
    /// to the object's surface above `p`, for the deepest such object.
    #[must_use]
    pub fn buried(&self, p: [f64; 3]) -> Option<(String, f64)> {
        let col = self.column(p[0], p[2]);
        let mut owners: Vec<u32> = col.iter().map(|x| x.1).collect();
        owners.sort_unstable();
        owners.dedup();
        let mut best: Option<(u32, f64)> = None;
        for o in owners {
            if self.soft.get(o as usize).copied().unwrap_or(false) {
                continue;
            }
            let mut hs: Vec<f64> = col.iter().filter(|x| x.1 == o).map(|x| x.0).collect();
            hs.sort_by(f64::total_cmp);
            hs.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
            let above: Vec<f64> = hs.iter().copied().filter(|h| *h > p[1]).collect();
            let below = hs.len() - above.len();
            // standing on the object's own floor (a surface of it up to 0.6 m below): an interior - a hangar, a
            // container, a barrier's top - not a solid (a closed shell is crossed as a solid is)
            let floor = hs
                .iter()
                .copied()
                .filter(|h| *h <= p[1])
                .fold(f64::NEG_INFINITY, f64::max);
            if above.len() % 2 == 1 && below % 2 == 1 && p[1] - floor > FLOOR {
                let d = above.first().copied().unwrap_or(p[1]) - p[1];
                if best.is_none_or(|b| d > b.1) {
                    best = Some((o, d));
                }
            }
        }
        best.map(|(o, d)| {
            (
                self.names
                    .get(o as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("object {o}")),
                d,
            )
        })
    }

    /// The object `p` is inside (its name when the site has names, else its number), if any.
    #[must_use]
    pub fn solid_at(&self, p: [f64; 3]) -> Option<String> {
        let col = self.column(p[0], p[2]);
        let mut owners: Vec<u32> = col.iter().map(|x| x.1).collect();
        owners.sort_unstable();
        owners.dedup();
        owners
            .into_iter()
            .filter(|&o| !self.soft.get(o as usize).copied().unwrap_or(false))
            .find(|&o| {
                // a line through an edge meets both triangles at one height: one crossing
                let mut hs: Vec<f64> = col.iter().filter(|x| x.1 == o).map(|x| x.0).collect();
                hs.sort_by(f64::total_cmp);
                hs.dedup_by(|a, b| (*a - *b).abs() < 1e-4);
                let above = hs.iter().filter(|h| **h > p[1]).count();
                let below = hs.len() - above;
                above % 2 == 1 && below % 2 == 1
            })
            .map(|o| {
                self.names
                    .get(o as usize)
                    .cloned()
                    .unwrap_or_else(|| format!("object {o}"))
            })
    }

    /// The collision on the vertical line through `p`, per object: its name and the heights where the line crosses
    /// it, relative to `p` (the evidence of an inside or a support finding).
    #[must_use]
    pub fn explain(&self, p: [f64; 3]) -> Vec<(String, Vec<f64>)> {
        let col = self.column(p[0], p[2]);
        let mut owners: Vec<u32> = col.iter().map(|x| x.1).collect();
        owners.sort_unstable();
        owners.dedup();
        owners
            .into_iter()
            .map(|o| {
                let mut hs: Vec<f64> = col.iter().filter(|x| x.1 == o).map(|x| x.0 - p[1]).collect();
                hs.sort_by(f64::total_cmp);
                (
                    self.names
                        .get(o as usize)
                        .cloned()
                        .unwrap_or_else(|| format!("object {o}")),
                    hs,
                )
            })
            .collect()
    }

    /// The surface under `p`: the highest of the ground and the collision at or below `p` (plus 0.5 m: a place a
    /// little in a floor stands on it).
    #[must_use]
    pub fn support(&self, p: [f64; 3]) -> Option<f64> {
        let top = p[1] + 0.5;
        let g = self.ground(p[0], p[2]).filter(|g| *g <= top + 1.0);
        let c = self
            .column(p[0], p[2])
            .into_iter()
            .map(|x| x.0)
            .filter(|h| *h <= top)
            .fold(None, |m: Option<f64>, h| Some(m.map_or(h, |m| m.max(h))));
        match (g, c) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// What stands at `p` for a soldier: ground, outside the terrain, water deeper than 1.2 m (from its surface to
    /// what is under it, a pier counting), more than [`BURIED`] inside an object, or more than [`UNDERGROUND`] below
    /// the ground.
    #[must_use]
    pub fn spot(&self, p: [f64; 3]) -> Spot {
        let Some(g) = self.ground(p[0], p[2]) else {
            return Spot::OutsideTerrain;
        };
        if p[1] < g - UNDERGROUND && self.support(p).is_none_or(|s| s < g - UNDERGROUND) {
            return Spot::Underground(g - p[1]);
        }
        if let Some((_, d)) = self.buried(p).filter(|b| b.1 > BURIED) {
            return Spot::Solid(d);
        }
        if let Some(w) = self.water(p[0], p[2]) {
            let under = self.support(p).unwrap_or(g);
            if w - under > 1.2 && p[1] < w + 1.0 {
                return Spot::Water(w - under);
            }
        }
        Spot::Ground
    }

    /// The clear radius around `p` at a vehicle's height (0.5 to 3 m above `p`): the distance to the nearest static
    /// collision in that band, up to `limit` (sampled on 16 bearings every 0.5 m).
    #[must_use]
    pub fn clearance(&self, p: [f64; 3], limit: f64) -> f64 {
        let blocked = |x: f64, z: f64| {
            self.column(x, z).iter().any(|c| c.0 > p[1] + 0.5 && c.0 < p[1] + 3.0)
                || self.inside_solid([x, p[1] + 1.0, z])
                || self.inside_solid([x, p[1] + 2.5, z])
        };
        if blocked(p[0], p[2]) {
            return 0.0;
        }
        let mut r = 0.5;
        while r <= limit {
            for k in 0..16 {
                let a = f64::from(k) * std::f64::consts::TAU / 16.0;
                if blocked(p[0] + r * a.cos(), p[2] + r * a.sin()) {
                    return r;
                }
            }
            r += 0.5;
        }
        limit
    }

    /// The number of collision triangles held.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.tris.len()
    }
}

/// A level's route graph for the path rules (round A1, XVAL-PLACE-5): each route node's place (the ground under it)
/// and its island, island 0 being the main network (the AI workspace's islands, round GR2: the largest first).
#[derive(Debug, Clone, Default)]
pub struct Routes {
    nodes: Vec<[f64; 3]>,
    island: Vec<u32>,
    cells: std::collections::HashMap<(i64, i64), Vec<u32>>,
    /// Where the path data came from (`NavKit's build`, `DICE's file`), for the findings' words.
    pub source: String,
}

/// How far a place may be from its route node in plan and in height to reach it (GR2's spawn islands, the Studio's
/// `spawn_islands`: 15 m and 4 m).
pub const JOIN_XZ: f64 = 15.0;
/// See [`JOIN_XZ`].
pub const JOIN_Y: f64 = 4.0;

impl Routes {
    /// The routes from the nodes' places (on the ground) and islands.
    #[must_use]
    pub fn new(nodes: Vec<[f64; 3]>, island: Vec<u32>, source: &str) -> Self {
        let mut cells: std::collections::HashMap<(i64, i64), Vec<u32>> = std::collections::HashMap::new();
        for (k, p) in nodes.iter().enumerate() {
            cells
                .entry(((p[0] / JOIN_XZ).floor() as i64, (p[2] / JOIN_XZ).floor() as i64))
                .or_default()
                .push(k as u32);
        }
        Self {
            nodes,
            island,
            cells,
            source: source.to_owned(),
        }
    }

    /// The island of the route node nearest `p` in plan within [`JOIN_XZ`] and [`JOIN_Y`] in height, with its
    /// distance; `None`: no node near enough. Ties go to the lower node number.
    #[must_use]
    pub fn reach(&self, p: [f64; 3]) -> Option<(u32, f64)> {
        let (ci, cj) = ((p[0] / JOIN_XZ).floor() as i64, (p[2] / JOIN_XZ).floor() as i64);
        let mut best: Option<(f64, u32)> = None;
        for di in -1..=1 {
            for dj in -1..=1 {
                for &k in self.cells.get(&(ci + di, cj + dj)).map_or(&[][..], Vec::as_slice) {
                    let Some(n) = self.nodes.get(k as usize) else { continue };
                    if (n[1] - p[1]).abs() > JOIN_Y {
                        continue;
                    }
                    let d = (n[0] - p[0]).hypot(n[2] - p[2]);
                    if d <= JOIN_XZ && best.is_none_or(|b| d.total_cmp(&b.0).then(k.cmp(&b.1)).is_lt()) {
                        best = Some((d, k));
                    }
                }
            }
        }
        best.map(|(d, k)| (self.island.get(k as usize).copied().unwrap_or(0), d))
    }

    /// The number of route nodes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// A site from a scene loaded with geometry (the editor's level, or a test's): its height field, water, every
/// static object's collision (entities with collision whose guid is not in `skip`, the gameplay layer) and the
/// positions of those objects (the art).
#[must_use]
pub fn from_scene<S: std::hash::BuildHasher>(scene: &bc2_scene::scene::Scene, skip: &HashSet<String, S>) -> Site {
    with_ground(
        scene,
        skip,
        scene.heightfield.as_deref().cloned(),
        scene.water.as_ref().map(|w| &w.mesh),
    )
}

/// [`from_scene`] with the height field and water given (read from the game copy where the scene has none:
/// [`crate::export::ground_of`]).
#[must_use]
pub fn with_ground<S: std::hash::BuildHasher>(
    scene: &bc2_scene::scene::Scene,
    skip: &HashSet<String, S>,
    hf: Option<Heightfield>,
    water: Option<&WaterMesh>,
) -> Site {
    let mut solids = Vec::new();
    let mut objects = Vec::new();
    let mut names = Vec::new();
    for (k, e) in scene.entities.iter().enumerate() {
        if skip.contains(&e.guid.to_ascii_uppercase()) {
            continue;
        }
        if let Some(t) = scene.world_triangles(k).filter(|t| !t.is_empty()) {
            if let Some(p) = e.position() {
                objects.push((p, if e.destructible { 1.0 } else { 0.25 }));
            }
            solids.push(t);
            names.push(e.blueprint.clone().unwrap_or_else(|| e.guid.clone()));
        }
    }
    let mut s = Site::new(hf, water, &solids, &objects);
    s.soft = names
        .iter()
        .map(|n| n.to_ascii_lowercase().contains("vegetation"))
        .collect();
    s.names = names;
    s
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// A closed box (12 triangles) from `lo` to `hi`.
    pub(crate) fn cube(lo: [f64; 3], hi: [f64; 3]) -> Vec<[[f64; 3]; 3]> {
        let v = |i: usize| {
            [
                if i & 1 == 0 { lo[0] } else { hi[0] },
                if i & 2 == 0 { lo[1] } else { hi[1] },
                if i & 4 == 0 { lo[2] } else { hi[2] },
            ]
        };
        let quads = [
            [0, 1, 3, 2],
            [4, 5, 7, 6],
            [0, 1, 5, 4],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 3, 7, 5],
        ];
        quads
            .iter()
            .flat_map(|q| [[v(q[0]), v(q[1]), v(q[2])], [v(q[0]), v(q[2]), v(q[3])]])
            .collect()
    }

    #[test]
    fn a_closed_box_is_solid_a_sheet_is_not_and_water_is_deep() {
        let rock = cube([10.0, -2.0, 10.0], [20.0, 6.0, 20.0]);
        let roof = vec![[[30.0, 4.0, 30.0], [40.0, 4.0, 30.0], [40.0, 4.0, 40.0]]];
        let water = WaterMesh {
            vertices: vec![
                [50.0, 3.0, 50.0],
                [90.0, 3.0, 50.0],
                [90.0, 3.0, 90.0],
                [50.0, 3.0, 90.0],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
        };
        let s = Site::new(None, Some(&water), &[rock, roof], &[([15.0, 0.0, 15.0], 1.0)]);
        assert!(s.inside_solid([15.0, 1.0, 15.0]));
        // buried 5 m below the box's top; standing on its floor (0.1 m above it) is an interior, not buried
        assert!(s.buried([15.0, 1.0, 15.0]).is_some_and(|b| (b.1 - 5.0).abs() < 1e-6));
        assert!(s.buried([15.0, -1.9, 15.0]).is_none());
        assert!(!s.inside_solid([15.0, 7.0, 15.0]));
        assert!(
            !s.inside_solid([38.0, 1.0, 32.0]),
            "a single sheet over a place is no solid"
        );
        assert!((s.water(70.0, 70.0).unwrap() - 3.0).abs() < 1e-6);
        assert!(s.water(5.0, 5.0).is_none());
        // no terrain: every place is outside it
        assert_eq!(s.spot([70.0, 0.0, 70.0]), Spot::OutsideTerrain);
        assert!(s.art(15.0, 15.0) > 0.9 && s.art(150.0, 150.0) < 1e-9);
        let c = s.clearance([15.0, -1.0, 25.0], 10.0);
        assert!((c - 5.0).abs() < 0.6, "{c}");
        assert!((s.clearance([15.0, -1.0, 60.0], 10.0) - 10.0).abs() < 1e-9);
    }
}
