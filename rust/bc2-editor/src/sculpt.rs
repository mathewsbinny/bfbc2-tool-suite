//! Round E5: the Terrain workspace's brushes (spec `transforms/terrain_edit.md`): sculpt (raise, lower, smooth,
//! flatten to a height, noise, a ramp between two points) on the main height field's stored values, and the material
//! paint on the physics material map's cells (0.5 m, or 0.25 m on `MP_001`, `MP_002` and `MP_008`: TEDIT-MM-1), each with
//! a radius, a strength and a falloff.
//!
//! A stroke works on a copy of the raster the log leaves ([`crate::edit::Doc::raster`], tiles shared): each dab
//! changes the samples under it and says the region it touched, which the views redraw at once; letting go commits the
//! whole stroke as one operation ([`crate::edit::Doc::stroke`]: one History step, only the 64 x 64 tiles it changed
//! kept). The height field's derived parts (min / max tree, lower envelope) are made by the export over the edited
//! region (`bc2_levelwrite::terrainedit`), the depth map kept (TEDIT-HF-3). A sculpt that would pass the level's
//! height range is clamped to it (TEDIT-HF-2), and says so.
//!
//! [`TerrainTool`] is the [`ViewTool`] of the Terrain workspace; the brush math is free functions over a [`Raster`]
//! and the terrain's [`Ground`] (testable without a view).

use bc2_formats::terrain::Heightfield;
use bc2_formats::terrain_codec::Region;
use bc2_levelwrite::delta::Raster;
use bc2_levelwrite::files::Target;
use eframe::egui::{Pos2, Stroke};

use crate::actions::Brush;
use crate::camera::Projection;
use crate::gizmo::Tool;
use crate::math::V3;
use crate::tool::{Draw, Layer, Outcome, Pointer, ViewTool};

/// How a brush's effect falls off from its centre to its edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Falloff {
    /// A smooth bell (smoothstep): full at the centre, nothing at the edge.
    #[default]
    Smooth,
    /// Straight from full at the centre to nothing at the edge.
    Linear,
    /// Full over the whole disc.
    Constant,
}

impl Falloff {
    /// Every falloff, in the toolbar's order.
    pub const ALL: [Self; 3] = [Self::Smooth, Self::Linear, Self::Constant];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Smooth => "Smooth",
            Self::Linear => "Linear",
            Self::Constant => "Constant",
        }
    }

    /// The weight at `t` = distance / radius (0 at and past the edge).
    #[must_use]
    pub fn weight(self, t: f64) -> f64 {
        if !(0.0..1.0).contains(&t) {
            return 0.0;
        }
        match self {
            Self::Smooth => {
                let u = 1.0 - t;
                u * u * (3.0 - 2.0 * u)
            }
            Self::Linear => 1.0 - t,
            Self::Constant => 1.0,
        }
    }
}

/// A brush's settings (the Terrain workspace's toolbar).
#[derive(Debug, Clone, PartialEq)]
pub struct BrushSettings {
    /// Radius, metres.
    pub radius: f64,
    /// Strength, 0 - 1: raise and lower move the centre by this many metres a dab, noise by twice it; smooth and
    /// flatten move this share of the way a dab.
    pub strength: f64,
    /// The falloff.
    pub falloff: Falloff,
    /// The noise's seed.
    pub seed: u32,
    /// The material the material brush paints (its index in the material map's names).
    pub material: u8,
}

impl Default for BrushSettings {
    fn default() -> Self {
        Self {
            radius: 12.0,
            strength: 0.5,
            falloff: Falloff::Smooth,
            seed: 1,
            material: 0,
        }
    }
}

/// Where the main height field lies: its first sample's world x and z, its sample spacing, its size and its height
/// scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Ground {
    /// World x of sample column 0.
    pub x0: f64,
    /// World z of sample row 0.
    pub z0: f64,
    /// Metres between samples along x and z.
    pub cell: [f64; 2],
    /// Samples a side.
    pub n: u32,
    /// The height of stored value 0, metres (min y).
    pub lo: f64,
    /// Metres per stored unit ((max y - min y) / 65535).
    pub scale: f64,
}

impl Ground {
    /// A height field's ground.
    #[must_use]
    pub fn of(hf: &Heightfield) -> Self {
        let (cx, cz) = hf.cell();
        Self {
            x0: f64::from(hf.min[0]),
            z0: f64::from(hf.min[2]),
            cell: [cx, cz],
            n: hf.size,
            lo: f64::from(hf.min[1]),
            scale: (f64::from(hf.max[1]) - f64::from(hf.min[1])) / 65535.0,
        }
    }

    /// A stored value as metres.
    #[must_use]
    pub fn metres(&self, v: u16) -> f64 {
        self.lo + f64::from(v) * self.scale
    }

    /// Metres as a stored value, clamped to the range (and whether it had to be).
    #[must_use]
    pub fn value(&self, h: f64) -> (u16, bool) {
        let v = ((h - self.lo) / self.scale.max(f64::MIN_POSITIVE)).round();
        if v < 0.0 {
            (0, true)
        } else if v > 65535.0 {
            (u16::MAX, true)
        } else {
            (v as u16, false)
        }
    }

    /// The (fractional) sample column and row of a world point.
    #[must_use]
    pub fn grid(&self, x: f64, z: f64) -> (f64, f64) {
        ((x - self.x0) / self.cell[0], (z - self.z0) / self.cell[1])
    }

    /// The world x and z of a sample.
    #[must_use]
    pub fn world(&self, i: u32, j: u32) -> (f64, f64) {
        (
            self.x0 + f64::from(i) * self.cell[0],
            self.z0 + f64::from(j) * self.cell[1],
        )
    }

    /// The height at world (`x`, `z`) on `r` (bilinear between samples; the edge extends), metres.
    #[must_use]
    pub fn height(&self, r: &Raster, x: f64, z: f64) -> f64 {
        let (gi, gj) = self.grid(x, z);
        let top = f64::from(self.n.saturating_sub(1));
        let (gi, gj) = (gi.clamp(0.0, top), gj.clamp(0.0, top));
        let (i0, j0) = (gi.floor() as u32, gj.floor() as u32);
        let (i1, j1) = ((i0 + 1).min(self.n - 1), (j0 + 1).min(self.n - 1));
        let (fx, fz) = (gi - f64::from(i0), gj - f64::from(j0));
        let h = |i: u32, j: u32| self.metres(r.get(i, j));
        let a = h(i0, j0) + (h(i1, j0) - h(i0, j0)) * fx;
        let b = h(i0, j1) + (h(i1, j1) - h(i0, j1)) * fx;
        a + (b - a) * fz
    }

    /// Where a ray from `o` along `d` first meets the ground of `r` (marched a sample's length at a time, then
    /// halved down to a centimetre), within 8 km; `None` when it does not.
    #[must_use]
    pub fn hit(&self, r: &Raster, o: V3, d: V3) -> Option<V3> {
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len == 0.0 {
            return None;
        }
        let d = [d[0] / len, d[1] / len, d[2] / len];
        let step = self.cell[0].min(self.cell[1]).max(0.25);
        let size = [f64::from(self.n) * self.cell[0], f64::from(self.n) * self.cell[1]];
        let inside =
            |p: V3| p[0] >= self.x0 && p[2] >= self.z0 && p[0] <= self.x0 + size[0] && p[2] <= self.z0 + size[1];
        let at = |t: f64| [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t];
        let above = |t: f64| {
            let p = at(t);
            p[1] - self.height(r, p[0], p[2])
        };
        let mut prev = 0.0;
        let mut t = 0.0;
        while t < 8000.0 {
            t += step;
            let p = at(t);
            if inside(p) && above(t) <= 0.0 {
                let (mut lo, mut hi) = (prev, t);
                while hi - lo > 0.01 {
                    let mid = f64::midpoint(lo, hi);
                    if above(mid) <= 0.0 {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                let q = at(hi);
                return Some([q[0], self.height(r, q[0], q[2]), q[2]]);
            }
            prev = t;
        }
        None
    }

    /// The samples within `radius` metres of world (`x`, `z`), as a region (cut to the grid).
    #[must_use]
    pub fn disc(&self, x: f64, z: f64, radius: f64) -> Region {
        let (gi, gj) = self.grid(x, z);
        let (ri, rj) = (radius / self.cell[0], radius / self.cell[1]);
        let lo = |v: f64| v.floor().max(0.0) as usize;
        let hi = |v: f64| (v.ceil().max(-1.0) + 1.0).max(0.0) as usize;
        Region {
            x0: lo(gi - ri),
            z0: lo(gj - rj),
            x1: hi(gi + ri),
            z1: hi(gj + rj),
        }
        .clip(self.n as usize)
    }
}

/// A smooth value noise in [-1, 1] at world (`x`, `z`) on an 8 m lattice, from `seed` (the noise brush's).
#[must_use]
pub fn noise(x: f64, z: f64, seed: u32) -> f64 {
    let hash = |i: i64, j: i64| -> f64 {
        let mut h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
            ^ (j as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
            ^ u64::from(seed).wrapping_mul(0x1656_67B1_9E37_79F9);
        h ^= h >> 33;
        h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
        h ^= h >> 33;
        f64::from((h >> 40) as u32) / f64::from(1u32 << 24) * 2.0 - 1.0
    };
    let (u, v) = (x / 8.0, z / 8.0);
    let (i, j) = (u.floor() as i64, v.floor() as i64);
    let s = |t: f64| t * t * (3.0 - 2.0 * t);
    let (fx, fz) = (s(u - u.floor()), s(v - v.floor()));
    let a = hash(i, j) + (hash(i + 1, j) - hash(i, j)) * fx;
    let b = hash(i, j + 1) + (hash(i + 1, j + 1) - hash(i, j + 1)) * fx;
    a + (b - a) * fz
}

/// What a dab did: the samples it touched, and whether a height had to be clamped to the level's range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dab {
    /// The samples (or cells) it touched.
    pub region: Region,
    /// A height was clamped to the height range (TEDIT-HF-2).
    pub clamped: bool,
}

/// One dab of a sculpt brush at world (`x`, `z`) on the heights `r`: raise, lower, smooth, flatten (to `target`
/// metres) or noise; the ramp and the paint brushes are not dabs ([`ramp`], [`paint_dab`]).
#[must_use]
pub fn sculpt_dab(r: &mut Raster, g: &Ground, brush: Brush, (x, z): (f64, f64), s: &BrushSettings, target: f64) -> Dab {
    let region = g.disc(x, z, s.radius);
    let mut clamped = false;
    // smoothing reads the heights as they were before this dab
    let before = (brush == Brush::Smooth).then(|| r.clone());
    for j in region.z0..region.z1 {
        for i in region.x0..region.x1 {
            let (i, j) = (i as u32, j as u32);
            let (wx, wz) = g.world(i, j);
            let w = s.falloff.weight((wx - x).hypot(wz - z) / s.radius.max(0.01));
            if w <= 0.0 {
                continue;
            }
            let h = g.metres(r.get(i, j));
            let k = s.strength.clamp(0.0, 1.0);
            let new = match brush {
                Brush::Raise => h + k * w,
                Brush::Lower => h - k * w,
                Brush::Flatten => h + (target - h) * k * w,
                Brush::Noise => h + noise(wx, wz, s.seed) * 2.0 * k * w,
                Brush::Smooth => {
                    let src = before.as_ref().unwrap_or(r);
                    let mut sum = 0.0;
                    let mut count = 0.0;
                    for (di, dj) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (0, 0)] {
                        let (ni, nj) = (i64::from(i) + di, i64::from(j) + dj);
                        if ni >= 0 && nj >= 0 && ni < i64::from(g.n) && nj < i64::from(g.n) {
                            sum += g.metres(src.get(ni as u32, nj as u32));
                            count += 1.0;
                        }
                    }
                    h + (sum / f64::max(count, 1.0) - h) * k * w
                }
                _ => h,
            };
            let (v, c) = g.value(new);
            clamped |= c;
            r.set(i, j, v);
        }
    }
    Dab { region, clamped }
}

/// A ramp from `a` to `b` (world points on the ground): the samples within the brush's radius of the segment pulled
/// to the straight line between the two heights, by the falloff across its width.
#[must_use]
pub fn ramp(r: &mut Raster, g: &Ground, a: V3, b: V3, s: &BrushSettings) -> Dab {
    let region = g.disc(a[0], a[2], s.radius).union(g.disc(b[0], b[2], s.radius)).union({
        let (lx, hx) = (a[0].min(b[0]), a[0].max(b[0]));
        let (lz, hz) = (a[2].min(b[2]), a[2].max(b[2]));
        let (c0, c1) = (g.grid(lx, lz), g.grid(hx, hz));
        Region {
            x0: c0.0.floor().max(0.0) as usize,
            z0: c0.1.floor().max(0.0) as usize,
            x1: (c1.0.ceil() + 1.0).max(0.0) as usize,
            z1: (c1.1.ceil() + 1.0).max(0.0) as usize,
        }
        .clip(g.n as usize)
    });
    let (dx, dz) = (b[0] - a[0], b[2] - a[2]);
    let l2 = (dx * dx + dz * dz).max(1e-9);
    let mut clamped = false;
    for j in region.z0..region.z1 {
        for i in region.x0..region.x1 {
            let (i, j) = (i as u32, j as u32);
            let (wx, wz) = g.world(i, j);
            let t = (((wx - a[0]) * dx + (wz - a[2]) * dz) / l2).clamp(0.0, 1.0);
            let (px, pz) = (a[0] + dx * t, a[2] + dz * t);
            let w = s.falloff.weight((wx - px).hypot(wz - pz) / s.radius.max(0.01));
            if w <= 0.0 {
                continue;
            }
            let h = g.metres(r.get(i, j));
            let line = a[1] + (b[1] - a[1]) * t;
            let (v, c) = g.value(h + (line - h) * w);
            clamped |= c;
            r.set(i, j, v);
        }
    }
    Dab { region, clamped }
}

/// The material map's cells over the ground: its width in cells and the metres a cell spans.
#[must_use]
pub fn cell_size(g: &Ground, width: u32) -> f64 {
    f64::from(g.n) * g.cell[0] / f64::from(width.max(1))
}

/// One dab of the material brush at world (`x`, `z`): every cell whose centre the falloff weighs at a half or more
/// (the whole disc with a constant falloff) set to the brush's material (the cells' rows along z, TERR-MM-3).
#[must_use]
pub fn paint_dab(cells: &mut Raster, g: &Ground, (x, z): (f64, f64), s: &BrushSettings) -> Dab {
    let width = cells.width();
    let size = cell_size(g, width);
    let (ci, cj) = ((x - g.x0) / size, (z - g.z0) / size);
    let rc = s.radius / size;
    let w = width as usize;
    let region = Region {
        x0: (ci - rc).floor().max(0.0) as usize,
        z0: (cj - rc).floor().max(0.0) as usize,
        x1: (ci + rc).ceil().max(0.0) as usize + 1,
        z1: (cj + rc).ceil().max(0.0) as usize + 1,
    }
    .clip(w);
    let threshold = if s.falloff == Falloff::Constant { 0.0 } else { 0.5 };
    for j in region.z0..region.z1 {
        for i in region.x0..region.x1 {
            let (cx, cz) = (i as f64 + 0.5 - ci, j as f64 + 0.5 - cj);
            let wt = s.falloff.weight(cx.hypot(cz) / rc.max(0.01));
            if wt > threshold || (threshold == 0.0 && wt > 0.0) {
                cells.set(i as u32, j as u32, u16::from(s.material));
            }
        }
    }
    Dab { region, clamped: false }
}

/// A stroke being made: the raster it works on (a copy of the log's, tiles shared) and what it touched.
#[derive(Debug, Clone)]
pub struct Working {
    /// The raster's target (the height field, the material map).
    pub target: Target,
    /// The raster as the stroke leaves it so far.
    pub raster: Raster,
    /// What it touched.
    pub region: Region,
    /// A height was clamped to the level's range.
    pub clamped: bool,
    /// Where the last dab was (world x, z).
    last: Option<(f64, f64)>,
    /// The flatten brush's height (the ground under the first dab).
    target_height: f64,
}

/// The Terrain workspace's tool: the chosen brush on the views (see the module).
#[derive(Debug, Default)]
pub struct TerrainTool {
    /// The chosen brush.
    pub brush: Option<Brush>,
    /// Its settings.
    pub settings: BrushSettings,
    /// The stroke being made.
    pub stroke: Option<Working>,
    /// The ramp's first point, once clicked.
    pub ramp_from: Option<V3>,
    /// Where the pointer meets the ground (this frame's), for the brush's circle.
    hover: Option<(Projection, V3)>,
    /// The log's height field (and material map) as last read, at the document's revision: the pointer's ground.
    cache: Option<(u64, Raster)>,
}

impl TerrainTool {
    /// The chosen brush's History label.
    fn label(&self) -> String {
        let b = self.brush.map_or("Terrain", Brush::name);
        format!(
            "{b} (radius {:.0} m, strength {:.2}, {})",
            self.settings.radius,
            self.settings.strength,
            self.settings.falloff.name().to_lowercase()
        )
    }

    /// The heights the log leaves (read once per document revision).
    fn heights(&mut self, doc: &crate::edit::Doc) -> Option<Raster> {
        let t = doc.terrain_target()?;
        if self.cache.as_ref().is_none_or(|(rev, _)| *rev != doc.revision) {
            self.cache = Some((doc.revision, doc.raster(&t)?));
        }
        self.cache.as_ref().map(|(_, r)| r.clone())
    }
}

impl ViewTool for TerrainTool {
    fn id(&self) -> Tool {
        Tool::Terrain
    }

    fn boxes(&self) -> bool {
        false
    }

    #[allow(clippy::too_many_lines, reason = "the brushes' input in one place")]
    fn input(&mut self, pointer: &Pointer, v: &crate::app::ViewState) -> Option<Outcome> {
        if !pointer.terrain {
            return None;
        }
        let brush = self.brush?;
        let doc = v.doc.as_ref()?;
        let g = Ground::of(doc.base_scene().heightfield.as_deref()?);
        let heights = match &self.stroke {
            Some(w) if matches!(w.target, Target::Terrain { .. }) => w.raster.clone(),
            _ => self.heights(doc)?,
        };
        let local = |p: Pos2| [f64::from(p.x - pointer.rect.min.x), f64::from(p.y - pointer.rect.min.y)];
        let under = |p: Pos2| {
            let (o, d) = pointer.view.ray(local(p));
            g.hit(&heights, o, d)
        };
        self.hover = pointer.hover.and_then(under).map(|p| (pointer.projection, p));
        // the ramp: two clicks
        if brush == Brush::Ramp {
            if !pointer.clicked {
                return None;
            }
            let p = under(pointer.at?)?;
            let Some(a) = self.ramp_from.take() else {
                self.ramp_from = Some(p);
                return Some(Outcome::Message("Ramp: click where it ends (Esc drops it)".into()));
            };
            let target = doc.terrain_target()?;
            let mut r = heights;
            let dab = ramp(&mut r, &g, a, p, &self.settings);
            return Some(Outcome::Stroke(
                target,
                Box::new(r),
                format!(
                    "Ramp {:.0} m (radius {:.0} m)",
                    (p[0] - a[0]).hypot(p[2] - a[2]),
                    self.settings.radius
                ),
                dab.clamped,
            ));
        }
        if !pointer.down {
            // letting go: the stroke committed as one operation
            let w = self.stroke.take()?;
            self.cache = None;
            return Some(Outcome::Stroke(w.target, Box::new(w.raster), self.label(), w.clamped));
        }
        let p = under(pointer.hover.or(pointer.at)?)?;
        let material = brush == Brush::Material;
        if self.stroke.is_none() {
            let (target, raster) = if material {
                let t = doc.material_target()?;
                let r = doc.raster(&t)?;
                (t, r)
            } else {
                (doc.terrain_target()?, heights)
            };
            self.stroke = Some(Working {
                target,
                raster,
                region: Region {
                    x0: 0,
                    z0: 0,
                    x1: 0,
                    z1: 0,
                },
                clamped: false,
                last: None,
                target_height: p[1],
            });
        }
        let s = self.settings.clone();
        let w = self.stroke.as_mut()?;
        // dabs a quarter of the radius apart (at least half a metre)
        let spacing = (s.radius * 0.25).max(0.5);
        if w.last.is_some_and(|(x, z)| (p[0] - x).hypot(p[2] - z) < spacing) {
            return Some(Outcome::Done);
        }
        w.last = Some((p[0], p[2]));
        let dab = if material {
            paint_dab(&mut w.raster, &g, (p[0], p[2]), &s)
        } else {
            sculpt_dab(&mut w.raster, &g, brush, (p[0], p[2]), &s, w.target_height)
        };
        w.region = w.region.union(dab.region);
        w.clamped |= dab.clamped;
        Some(Outcome::TerrainPreview(w.target.clone(), dab.region))
    }

    fn draw(&self, d: &Draw<'_>, layer: Layer) {
        if layer != Layer::Tools || self.brush.is_none() {
            return;
        }
        let Some((_, c)) = self.hover.filter(|(p, _)| *p == d.projection) else {
            return;
        };
        // the brush's circle on the ground (flat at the pointer's height), its inner ring where the falloff halves
        let ring = |r: f64| -> Vec<Pos2> {
            (0..=48)
                .filter_map(|k| {
                    let a = f64::from(k) / 48.0 * std::f64::consts::TAU;
                    d.to_screen([c[0] + r * a.cos(), c[1], c[2] + r * a.sin()])
                })
                .collect()
        };
        let p = d.palette;
        d.painter.add(eframe::egui::Shape::line(
            ring(self.settings.radius),
            Stroke::new(1.5, p.view_selection),
        ));
        if self.settings.falloff != Falloff::Constant {
            d.painter.add(eframe::egui::Shape::line(
                ring(self.settings.radius * 0.5),
                Stroke::new(1.0, p.view_selection.gamma_multiply(0.6)),
            ));
        }
        if let Some(a) = self.ramp_from
            && let (Some(sa), Some(sb)) = (d.to_screen(a), d.to_screen(c))
        {
            d.painter.line_segment([sa, sb], Stroke::new(2.0, p.view_selection));
            d.painter.circle_filled(sa, 4.0, p.view_selection);
        }
    }

    fn transient(&self) -> bool {
        self.ramp_from.is_some()
    }

    fn cancel(&mut self) {
        self.ramp_from = None;
    }

    fn left(&mut self) {
        self.ramp_from = None;
        self.hover = None;
        self.stroke = None;
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn flat(n: u32, v: u16) -> (Raster, Ground) {
        (
            Raster::new(n, n, v),
            Ground {
                x0: -(f64::from(n) / 2.0),
                z0: -(f64::from(n) / 2.0),
                cell: [1.0, 1.0],
                n,
                lo: 0.0,
                scale: 500.0 / 65535.0,
            },
        )
    }

    #[test]
    fn falloffs() {
        for f in Falloff::ALL {
            assert!((f.weight(0.0) - 1.0).abs() < 1e-12);
            assert!(f.weight(1.0).abs() < 1e-12);
            assert!(f.weight(-0.1).abs() < 1e-12);
        }
        assert!((Falloff::Linear.weight(0.25) - 0.75).abs() < 1e-12);
        assert!((Falloff::Smooth.weight(0.5) - 0.5).abs() < 1e-12);
        assert!((Falloff::Constant.weight(0.99) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn sculpt_brushes_move_the_ground() {
        let (mut r, g) = flat(128, 13_107); // 100 m
        let s = BrushSettings {
            radius: 8.0,
            strength: 1.0,
            ..BrushSettings::default()
        };
        let d = sculpt_dab(&mut r, &g, Brush::Raise, (0.0, 0.0), &s, 0.0);
        assert!(!d.clamped);
        assert_eq!((d.region.x0, d.region.x1), (56, 73));
        let centre = g.metres(r.get(64, 64));
        assert!((centre - 101.0).abs() < 0.01, "{centre}");
        assert_eq!(r.get(64 + 9, 64), 13_107, "outside the radius");
        // the ground under a ray from above
        let hit = g.hit(&r, [0.0, 500.0, 0.0], [0.0, -1.0, 0.0]).unwrap();
        assert!((hit[1] - centre).abs() < 0.02, "{hit:?}");
        assert!(g.hit(&r, [0.0, 500.0, 0.0], [0.0, 1.0, 0.0]).is_none());
        // lower twice, flatten back to 100 m, smooth: never past the start
        let _ = sculpt_dab(&mut r, &g, Brush::Lower, (0.0, 0.0), &s, 0.0);
        let _ = sculpt_dab(&mut r, &g, Brush::Lower, (0.0, 0.0), &s, 0.0);
        assert!(g.metres(r.get(64, 64)) < 99.5);
        for _ in 0..4 {
            let _ = sculpt_dab(&mut r, &g, Brush::Flatten, (0.0, 0.0), &s, 100.0);
        }
        assert!((g.metres(r.get(64, 64)) - 100.0).abs() < 0.02);
        let _ = sculpt_dab(&mut r, &g, Brush::Smooth, (0.0, 0.0), &s, 0.0);
        // noise is deterministic from its seed
        let (mut a, _) = flat(128, 13_107);
        let (mut b, _) = flat(128, 13_107);
        let _ = sculpt_dab(&mut a, &g, Brush::Noise, (3.0, 3.0), &s, 0.0);
        let _ = sculpt_dab(&mut b, &g, Brush::Noise, (3.0, 3.0), &s, 0.0);
        assert_eq!(a, b);
        // a raise past the range is clamped
        let (mut top, _) = flat(32, 65_530);
        assert!(sculpt_dab(&mut top, &g, Brush::Raise, (-50.0, -50.0), &s, 0.0).clamped);
    }

    #[test]
    fn ramps_and_material_paint() {
        let (mut r, g) = flat(128, 0);
        let s = BrushSettings {
            radius: 4.0,
            falloff: Falloff::Constant,
            ..BrushSettings::default()
        };
        let d = ramp(&mut r, &g, [-20.0, 0.0, 0.0], [20.0, 40.0, 0.0], &s);
        assert!(!d.clamped);
        // halfway along: 20 m up
        assert!((g.metres(r.get(64, 64)) - 20.0).abs() < 0.05);
        assert!((g.metres(r.get(64 + 10, 64)) - 30.0).abs() < 0.05);
        assert_eq!(r.get(64, 64 + 6), 0, "beyond its width");
        // material: 0.5 m cells over the 128 m ground
        let mut cells = Raster::new(256, 256, 0);
        assert!((cell_size(&g, 256) - 0.5).abs() < 1e-12);
        let s = BrushSettings {
            radius: 2.0,
            material: 3,
            ..BrushSettings::default()
        };
        let d = paint_dab(&mut cells, &g, (0.0, 0.0), &s);
        assert_eq!(cells.get(128, 128), 3);
        assert_eq!(cells.get(128 + 4, 128), 0, "outside the half-weight ring");
        assert!(d.region.x0 <= 124 && d.region.x1 >= 132);
    }
}
