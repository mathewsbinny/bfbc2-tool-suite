//! Vegetation, rocks and props on a terrain (round TG4, spec `transforms/terrain_gen.md` TGEN-VEG-1 - 4): the
//! statistics of a level's placed members on its ground ([`measure`]), DICE's per biome ([`BiomeStats::dice`]), and
//! a placement on a new terrain that falls on them ([`place`]), seeded as the terrain is (TGEN-GEN-1).

/// What the vegetation step places.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Kind {
    /// Trees: the level's forest (`ForestEntityData`'s `TreeMemberData`, VEG-CLS-1).
    Forest,
    /// Bushes and small plants: the middlegrowth groups (VEG-CLS-1).
    Middlegrowth,
    /// Rocks, cliffs, stones and boulders of the static groups (TGEN-BIOME-2's names).
    Rock,
    /// Natural ground clutter of the static groups (stumps, logs, dead plants: blueprints under `vegetation/`,
    /// `objects/vegetation/` or a terrain's folder).
    Prop,
}

impl Kind {
    /// Every kind.
    pub const ALL: [Self; 4] = [Self::Forest, Self::Middlegrowth, Self::Rock, Self::Prop];

    /// Its name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Forest => "forest",
            Self::Middlegrowth => "middlegrowth",
            Self::Rock => "rock",
            Self::Prop => "prop",
        }
    }

    /// Its place in [`Kind::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Forest => 0,
            Self::Middlegrowth => 1,
            Self::Rock => 2,
            Self::Prop => 3,
        }
    }
}

/// A transform as the level stores it: right, up, forward, position (DICE's row order).
pub type Rows = [[f32; 3]; 4];

/// The ground a placement is measured on or made over: a square height field of `n` samples `cell` metres apart
/// from (`x0`, `z0`), rows along z, and the water level.
#[derive(Debug, Clone)]
pub struct Ground {
    /// Samples a side.
    pub n: usize,
    /// Metres between samples.
    pub cell: f64,
    /// The first sample's x.
    pub x0: f64,
    /// The first sample's z.
    pub z0: f64,
    /// The heights in metres, `n * n`, rows along z.
    pub heights: Vec<f32>,
    /// The water level (`None`: no water).
    pub water: Option<f64>,
}

impl Ground {
    /// The ground of a generated height field (its heights as the game reads them, its water level).
    #[must_use]
    pub fn of_generated(g: &crate::Generated) -> Self {
        let n = g.size as usize;
        Self {
            n,
            cell: 1.0,
            x0: -(n as f64) / 2.0,
            z0: -(n as f64) / 2.0,
            heights: g.heights(),
            water: g.water_y,
        }
    }

    /// The sample at (`i`, `j`), clamped to the grid.
    fn at(&self, i: i64, j: i64) -> f64 {
        let top = self.n as i64 - 1;
        let (i, j) = (i.clamp(0, top) as usize, j.clamp(0, top) as usize);
        f64::from(self.heights.get(j * self.n + i).copied().unwrap_or(0.0))
    }

    /// The height at (`x`, `z`), bilinear, the edges extended.
    #[must_use]
    pub fn height(&self, x: f64, z: f64) -> f64 {
        let (u, v) = ((x - self.x0) / self.cell, (z - self.z0) / self.cell);
        let (i, j) = (u.floor(), v.floor());
        let (fx, fz) = (u - i, v - j);
        let (i, j) = (i as i64, j as i64);
        let a = self.at(i, j) * (1.0 - fx) + self.at(i + 1, j) * fx;
        let b = self.at(i, j + 1) * (1.0 - fx) + self.at(i + 1, j + 1) * fx;
        a * (1.0 - fz) + b * fz
    }

    /// The ground's unit normal at (`x`, `z`) by central differences over one sample.
    #[must_use]
    pub fn normal(&self, x: f64, z: f64) -> [f64; 3] {
        let c = self.cell;
        let gx = (self.height(x + c, z) - self.height(x - c, z)) / (2.0 * c);
        let gz = (self.height(x, z + c) - self.height(x, z - c)) / (2.0 * c);
        let l = (gx * gx + 1.0 + gz * gz).sqrt();
        [-gx / l, 1.0 / l, -gz / l]
    }

    /// The slope at (`x`, `z`) in degrees.
    #[must_use]
    pub fn slope(&self, x: f64, z: f64) -> f64 {
        self.normal(x, z)[1].clamp(-1.0, 1.0).acos().to_degrees()
    }

    /// The height over the water at (`x`, `z`) (over the lowest sample when there is no water).
    #[must_use]
    pub fn over_water(&self, x: f64, z: f64) -> f64 {
        let base = self
            .water
            .unwrap_or_else(|| f64::from(self.heights.iter().copied().fold(f32::INFINITY, f32::min)));
        self.height(x, z) - base
    }

    /// The side in metres.
    #[must_use]
    pub fn side(&self) -> f64 {
        self.n as f64 * self.cell
    }
}

/// A placed member for the statistics: its kind, its member type (an index of the level's own) and its transform.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Instance {
    /// The kind.
    pub kind: Kind,
    /// The member type, an index the caller gives.
    pub ty: usize,
    /// The transform.
    pub rows: Rows,
}

fn len3(v: [f32; 3]) -> f64 {
    v.iter().map(|&x| f64::from(x) * f64::from(x)).sum::<f64>().sqrt()
}

/// The `q`-quantile (0 - 1) of sorted values (nearest rank), 0 when empty.
#[must_use]
pub fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let k = ((sorted.len() - 1) as f64 * q).round() as usize;
    sorted.get(k).copied().unwrap_or(0.0)
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(f64::total_cmp);
    v
}

/// The slope bands of the preferences, in degrees (upper bounds; the last is open).
pub const SLOPE_BANDS: [f64; 6] = [5.0, 10.0, 20.0, 30.0, 45.0, 90.0];
/// The height-over-water bands of the preferences, in metres (upper bounds; the last is open).
pub const HEIGHT_BANDS: [f64; 6] = [2.0, 10.0, 30.0, 60.0, 120.0, f64::INFINITY];
/// The cell of the occupancy grid, in metres.
pub const OCC_CELL: f64 = 64.0;
/// The radii of the neighbour ratio (Ripley's K over the area's density), in metres.
pub const RADII: [f64; 4] = [3.0, 10.0, 30.0, 100.0];
/// The bands of distance outside the playable area, in metres (upper bounds; the first is inside, the last open).
pub const AREA_BANDS: [f64; 6] = [1e-9, 50.0, 150.0, 300.0, 600.0, f64::INFINITY];

/// A band's value where the ground gave no measure (a band of under 1 ha, or under 1 % of the ground): left out of
/// a biome's medians, taken as neutral by a plan.
pub const NO_DATA: f64 = -1.0;

/// The distance of a point outside the playable area (0 inside).
pub type Outside<'a> = &'a (dyn Fn(f64, f64) -> f64 + Sync);

/// The distance outside the central square of half a terrain's `side` (the playable area of a level with no combat
/// area: DICE's lie near the middle, TGEN-PLAY-1).
#[must_use]
pub fn central(side: f64) -> impl Fn(f64, f64) -> f64 + Sync {
    move |x: f64, z: f64| {
        let h = side / 4.0;
        (x.abs() - h).max(0.0).hypot((z.abs() - h).max(0.0))
    }
}

fn band(bands: &[f64], v: f64) -> usize {
    bands.iter().position(|&b| v < b).unwrap_or(bands.len() - 1)
}

/// One kind's statistics on one level (or a biome's medians of them).
#[derive(Debug, Clone, PartialEq)]
pub struct KindStats {
    /// Instances.
    pub count: f64,
    /// Member types with an instance.
    pub types: f64,
    /// The share of the dry 64 m cells holding an instance.
    pub occupancy: f64,
    /// Instances per hectare of dry ground in each of [`AREA_BANDS`] ([`NO_DATA`] for a band of under 1 ha).
    pub band_density: [f64; 6],
    /// Instances per hectare of the occupied cells' dry ground.
    pub density_ha: f64,
    /// Nearest-neighbour distance percentiles 10, 50, 90 in metres.
    pub nn: [f64; 3],
    /// Clark and Evans' ratio over the occupied cells (1 random, under 1 clustered, over 1 regular).
    pub clark_evans: f64,
    /// Neighbours within each of [`RADII`] over the count the occupied ground's density gives (1 random).
    pub k_ratio: [f64; 4],
    /// Per [`SLOPE_BANDS`] band: the share of instances over the share of the occupied cells' dry ground
    /// ([`NO_DATA`] for a band of under 1 % of that ground).
    pub slope_pref: [f64; 6],
    /// Per [`HEIGHT_BANDS`] band: the same for the height over the water.
    pub height_pref: [f64; 6],
    /// Instance slope percentiles 10, 50, 90 in degrees.
    pub slope: [f64; 3],
    /// The angle of the up axis from the vertical, percentiles 50, 90, 99 in degrees.
    pub tilt: [f64; 3],
    /// The angle of the up axis from the ground's normal, percentile 50 in degrees.
    pub tilt_normal: f64,
    /// The mean resultant length of the yaw (0: every direction alike, 1: one direction).
    pub yaw_r: f64,
    /// The up axis's length, percentiles 5, 50, 95.
    pub scale: [f64; 3],
    /// The right axis's length over the up axis's, percentile 50 (1: uniform scale).
    pub aspect: f64,
    /// The position's height minus the ground's, percentiles 10, 50, 90 in metres.
    pub sink: [f64; 3],
}

/// Every kind's statistics of one level.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelStats {
    /// The share of the terrain's 64 m cells that are dry (their centre over the water).
    pub dry_cells: f64,
    /// The dry ground's hectares per [`AREA_BANDS`] band.
    pub areas: [f64; 6],
    /// Per [`Kind::ALL`], `None` when the level places none.
    pub kinds: [Option<KindStats>; 4],
}

/// A biome's statistics: per kind, the median over the biome's terrains that place it of each of [`KindStats`]'s
/// values ([`NO_DATA`] left out), and the share of its terrains that place it.
#[derive(Debug, Clone, PartialEq)]
pub struct BiomeStats {
    /// Terrains measured.
    pub terrains: usize,
    /// Per [`Kind::ALL`]: the share of the terrains placing it and the medians (`None`: no terrain places it).
    pub kinds: [Option<(f64, KindStats)>; 4],
}

impl KindStats {
    /// Every value rounded to 1e-4 (the form the DICE table keeps).
    #[must_use]
    pub fn rounded(&self) -> Self {
        let r = |x: f64| (x * 10_000.0).round() / 10_000.0;
        let a = |v: &[f64]| v.iter().map(|&x| r(x)).collect::<Vec<f64>>();
        let a3 = |v: &[f64; 3]| <[f64; 3]>::try_from(a(v)).unwrap_or(*v);
        let a4 = |v: &[f64; 4]| <[f64; 4]>::try_from(a(v)).unwrap_or(*v);
        let a6 = |v: &[f64; 6]| <[f64; 6]>::try_from(a(v)).unwrap_or(*v);
        Self {
            count: r(self.count),
            types: r(self.types),
            occupancy: r(self.occupancy),
            band_density: a6(&self.band_density),
            density_ha: r(self.density_ha),
            nn: a3(&self.nn),
            clark_evans: r(self.clark_evans),
            k_ratio: a4(&self.k_ratio),
            slope_pref: a6(&self.slope_pref),
            height_pref: a6(&self.height_pref),
            slope: a3(&self.slope),
            tilt: a3(&self.tilt),
            tilt_normal: r(self.tilt_normal),
            yaw_r: r(self.yaw_r),
            scale: a3(&self.scale),
            aspect: r(self.aspect),
            sink: a3(&self.sink),
        }
    }
}

fn median_of(v: &mut Vec<f64>) -> f64 {
    v.retain(|x| x.is_finite() && *x != NO_DATA);
    if v.is_empty() {
        return NO_DATA;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    let (a, b) = (
        v.get((n - 1) / 2).copied().unwrap_or(0.0),
        v.get(n / 2).copied().unwrap_or(0.0),
    );
    (a + b) / 2.0
}

fn med<const N: usize>(all: &[&KindStats], f: impl Fn(&KindStats) -> [f64; N]) -> [f64; N] {
    std::array::from_fn(|k| median_of(&mut all.iter().map(|s| f(s).get(k).copied().unwrap_or(NO_DATA)).collect()))
}

impl BiomeStats {
    /// Every value rounded to 1e-4.
    #[must_use]
    pub fn rounded(&self) -> Self {
        Self {
            terrains: self.terrains,
            kinds: self
                .kinds
                .clone()
                .map(|k| k.map(|(p, s)| ((p * 10_000.0).round() / 10_000.0, s.rounded()))),
        }
    }

    /// DICE's statistics of a biome (TGEN-VEG-2): the medians over its multiplayer terrains, measured by
    /// `bc2_levelwrite::vegetation::dice_terrain_stats` on the game copy and kept here (`vegetation_dice.rs`; the game
    /// tier measures them again and holds them equal).
    #[must_use]
    pub fn dice(biome: crate::Biome) -> Self {
        crate::vegetation_dice::dice(biome)
    }

    /// The medians of the levels' statistics (one per terrain).
    #[must_use]
    pub fn of(levels: &[LevelStats]) -> Self {
        let kinds = Kind::ALL.map(|k| {
            let all: Vec<&KindStats> = levels.iter().filter_map(|l| l.kinds.get(k.index())?.as_ref()).collect();
            if all.is_empty() {
                return None;
            }
            let one = |f: fn(&KindStats) -> f64| med(&all, |s| [f(s)])[0];
            Some((
                all.len() as f64 / levels.len().max(1) as f64,
                KindStats {
                    count: one(|s| s.count),
                    types: one(|s| s.types),
                    occupancy: one(|s| s.occupancy),
                    band_density: med(&all, |s| s.band_density),
                    density_ha: one(|s| s.density_ha),
                    nn: med(&all, |s| s.nn),
                    clark_evans: one(|s| s.clark_evans),
                    k_ratio: med(&all, |s| s.k_ratio),
                    slope_pref: med(&all, |s| s.slope_pref),
                    height_pref: med(&all, |s| s.height_pref),
                    slope: med(&all, |s| s.slope),
                    tilt: med(&all, |s| s.tilt),
                    tilt_normal: one(|s| s.tilt_normal),
                    yaw_r: one(|s| s.yaw_r),
                    scale: med(&all, |s| s.scale),
                    aspect: one(|s| s.aspect),
                    sink: med(&all, |s| s.sink),
                },
            ))
        });
        Self {
            terrains: levels.len(),
            kinds,
        }
    }
}

/// The cells of the occupancy grid over the ground: (cells a side, dry per cell).
fn dry_cells(g: &Ground) -> (usize, Vec<bool>) {
    let m = (g.side() / OCC_CELL).floor().max(1.0) as usize;
    let mut dry = vec![false; m * m];
    for j in 0..m {
        for i in 0..m {
            let (x, z) = cell_centre(g, i, j);
            if let Some(d) = dry.get_mut(j * m + i) {
                *d = g.water.is_none_or(|w| g.height(x, z) > w);
            }
        }
    }
    (m, dry)
}

fn cell_centre(g: &Ground, i: usize, j: usize) -> (f64, f64) {
    (g.x0 + (i as f64 + 0.5) * OCC_CELL, g.z0 + (j as f64 + 0.5) * OCC_CELL)
}

fn cell_of(g: &Ground, m: usize, x: f64, z: f64) -> Option<usize> {
    let (i, j) = ((x - g.x0) / OCC_CELL, (z - g.z0) / OCC_CELL);
    if i < 0.0 || j < 0.0 {
        return None;
    }
    let (i, j) = (i as usize, j as usize);
    (i < m && j < m).then_some(j * m + i)
}

/// Neighbour lists by a uniform grid of `r` metres.
struct Grid {
    r: f64,
    x0: f64,
    z0: f64,
    m: usize,
    cells: Vec<Vec<usize>>,
}

impl Grid {
    fn new(pts: &[[f64; 2]], r: f64, x0: f64, z0: f64, side: f64) -> Self {
        let m = ((side / r).ceil() as usize).max(1) + 2;
        let mut cells = vec![Vec::new(); m * m];
        let mut g = Self {
            r,
            x0,
            z0,
            m,
            cells: Vec::new(),
        };
        for (k, p) in pts.iter().enumerate() {
            if let Some(c) = cells.get_mut(g.index(p[0], p[1])) {
                c.push(k);
            }
        }
        g.cells = cells;
        g
    }

    fn ij(&self, x: f64, z: f64) -> (usize, usize) {
        let f = |v: f64| ((v / self.r).floor() + 1.0).clamp(0.0, (self.m - 1) as f64) as usize;
        (f(x - self.x0), f(z - self.z0))
    }

    fn index(&self, x: f64, z: f64) -> usize {
        let (i, j) = self.ij(x, z);
        j * self.m + i
    }

    /// Every point within `ring` cells of (`x`, `z`)'s cell.
    fn near(&self, x: f64, z: f64, ring: usize, out: &mut Vec<usize>) {
        out.clear();
        let (i, j) = self.ij(x, z);
        for jj in j.saturating_sub(ring)..=(j + ring).min(self.m - 1) {
            for ii in i.saturating_sub(ring)..=(i + ring).min(self.m - 1) {
                if let Some(c) = self.cells.get(jj * self.m + ii) {
                    out.extend_from_slice(c);
                }
            }
        }
    }
}

/// The nearest-neighbour distance of each point (`f64::INFINITY` alone).
fn nearest(pts: &[[f64; 2]], side: f64, x0: f64, z0: f64) -> Vec<f64> {
    let grid = Grid::new(pts, 8.0, x0, z0, side);
    let mut out = Vec::with_capacity(pts.len());
    let mut near = Vec::new();
    for (k, p) in pts.iter().enumerate() {
        let mut best = f64::INFINITY;
        let mut ring = 1;
        loop {
            grid.near(p[0], p[1], ring, &mut near);
            for &q in &near {
                if q != k
                    && let Some(o) = pts.get(q)
                {
                    best = best.min((o[0] - p[0]).hypot(o[1] - p[1]));
                }
            }
            // the cells searched cover every point closer than (ring) cells
            if best <= ring as f64 * grid.r || ring > grid.m {
                break;
            }
            ring *= 2;
        }
        out.push(best);
    }
    out
}

/// Neighbours within `r` of each point, summed.
fn pairs_within(pts: &[[f64; 2]], r: f64, side: f64, x0: f64, z0: f64) -> f64 {
    let grid = Grid::new(pts, r, x0, z0, side);
    let mut near = Vec::new();
    let mut total = 0usize;
    for (k, p) in pts.iter().enumerate() {
        grid.near(p[0], p[1], 1, &mut near);
        total += near
            .iter()
            .filter(|&&q| q != k && pts.get(q).is_some_and(|o| (o[0] - p[0]).hypot(o[1] - p[1]) <= r))
            .count();
    }
    total as f64
}

/// The dry ground's area in hectares in each of [`AREA_BANDS`], sampled every 8 m.
#[must_use]
pub fn band_areas(g: &Ground, outside: Outside<'_>) -> [f64; 6] {
    let mut a = [0f64; 6];
    let k = (g.side() / 8.0) as usize;
    for j in 0..k {
        for i in 0..k {
            let (x, z) = (g.x0 + 4.0 + 8.0 * i as f64, g.z0 + 4.0 + 8.0 * j as f64);
            if g.water.is_some_and(|w| g.height(x, z) <= w) {
                continue;
            }
            if let Some(v) = a.get_mut(band(&AREA_BANDS, outside(x, z))) {
                *v += 64.0 / 10_000.0;
            }
        }
    }
    a
}

/// One kind's statistics of `instances` (all of one kind) on `g`; `areas` the dry ground's [`band_areas`].
#[must_use]
pub fn kind_stats(g: &Ground, instances: &[Instance], outside: Outside<'_>, areas: &[f64; 6]) -> Option<KindStats> {
    if instances.is_empty() {
        return None;
    }
    let (m, dry) = dry_cells(g);
    let pts: Vec<[f64; 2]> = instances
        .iter()
        .map(|i| [f64::from(i.rows[3][0]), f64::from(i.rows[3][2])])
        .collect();
    let mut occupied = vec![false; m * m];
    for p in &pts {
        if let Some(c) = cell_of(g, m, p[0], p[1])
            && let Some(o) = occupied.get_mut(c)
        {
            *o = true;
        }
    }
    let n_dry = dry.iter().filter(|&&d| d).count().max(1);
    let occ: Vec<usize> = (0..m * m)
        .filter(|&c| occupied.get(c).copied().unwrap_or(false))
        .collect();
    // the occupied cells' ground sampled every 4 m: its dry area, its slope and height bands
    let (mut ground_slope, mut ground_height) = ([0f64; 6], [0f64; 6]);
    let mut area_samples = 0usize;
    for &c in &occ {
        let (cx, cz) = cell_centre(g, c % m, c / m);
        for b in 0..16 {
            for a in 0..16 {
                let (x, z) = (
                    cx - OCC_CELL / 2.0 + 2.0 + 4.0 * f64::from(a),
                    cz - OCC_CELL / 2.0 + 2.0 + 4.0 * f64::from(b),
                );
                if g.water.is_some_and(|w| g.height(x, z) <= w) {
                    continue;
                }
                area_samples += 1;
                if let Some(s) = ground_slope.get_mut(band(&SLOPE_BANDS, g.slope(x, z))) {
                    *s += 1.0;
                }
                if let Some(s) = ground_height.get_mut(band(&HEIGHT_BANDS, g.over_water(x, z))) {
                    *s += 1.0;
                }
            }
        }
    }
    let area_m2 = (area_samples as f64 * 16.0).max(1.0);
    let n = instances.len() as f64;
    let density = n / area_m2;
    let nn = sorted(
        nearest(&pts, g.side(), g.x0, g.z0)
            .into_iter()
            .filter(|d| d.is_finite())
            .collect(),
    );
    let mean_nn = if nn.is_empty() {
        0.0
    } else {
        nn.iter().sum::<f64>() / nn.len() as f64
    };
    let k_ratio = RADII.map(|r| {
        let expected = n * density * std::f64::consts::PI * r * r;
        if expected > 0.0 {
            pairs_within(&pts, r, g.side(), g.x0, g.z0) / expected
        } else {
            0.0
        }
    });
    let mut in_band = [0f64; 6];
    for p in &pts {
        if let Some(v) = in_band.get_mut(band(&AREA_BANDS, outside(p[0], p[1]))) {
            *v += 1.0;
        }
    }
    let band_density = std::array::from_fn(|b| {
        let a = areas.get(b).copied().unwrap_or(0.0);
        if a >= 1.0 {
            in_band.get(b).copied().unwrap_or(0.0) / a
        } else {
            NO_DATA
        }
    });
    let (mut inst_slope, mut inst_height) = ([0f64; 6], [0f64; 6]);
    let (mut slopes, mut tilts, mut tilt_n, mut scales, mut aspects, mut sinks) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let (mut yc, mut ys) = (0.0, 0.0);
    for i in instances {
        let (x, z) = (f64::from(i.rows[3][0]), f64::from(i.rows[3][2]));
        let s = g.slope(x, z);
        slopes.push(s);
        if let Some(v) = inst_slope.get_mut(band(&SLOPE_BANDS, s)) {
            *v += 1.0;
        }
        if let Some(v) = inst_height.get_mut(band(&HEIGHT_BANDS, g.over_water(x, z))) {
            *v += 1.0;
        }
        let up = i.rows[1];
        let lu = len3(up).max(1e-9);
        let upn = [f64::from(up[0]) / lu, f64::from(up[1]) / lu, f64::from(up[2]) / lu];
        tilts.push(upn[1].clamp(-1.0, 1.0).acos().to_degrees());
        let nrm = g.normal(x, z);
        tilt_n.push(
            (upn[0] * nrm[0] + upn[1] * nrm[1] + upn[2] * nrm[2])
                .clamp(-1.0, 1.0)
                .acos()
                .to_degrees(),
        );
        scales.push(lu);
        aspects.push(len3(i.rows[0]) / lu);
        sinks.push(f64::from(i.rows[3][1]) - g.height(x, z));
        let f = i.rows[2];
        let yaw = f64::from(f[0]).atan2(f64::from(f[2]));
        yc += yaw.cos();
        ys += yaw.sin();
    }
    let pref = |inst: [f64; 6], ground: [f64; 6]| -> [f64; 6] {
        let (ti, tg) = (inst.iter().sum::<f64>().max(1.0), ground.iter().sum::<f64>().max(1.0));
        std::array::from_fn(|k| {
            let gshare = ground.get(k).copied().unwrap_or(0.0) / tg;
            if gshare >= 0.01 {
                inst.get(k).copied().unwrap_or(0.0) / ti / gshare
            } else {
                NO_DATA
            }
        })
    };
    let (slopes, tilts, tilt_n, scales, aspects, sinks) = (
        sorted(slopes),
        sorted(tilts),
        sorted(tilt_n),
        sorted(scales),
        sorted(aspects),
        sorted(sinks),
    );
    let mut types: Vec<usize> = instances.iter().map(|i| i.ty).collect();
    types.sort_unstable();
    types.dedup();
    Some(KindStats {
        count: n,
        types: types.len() as f64,
        occupancy: occ.len() as f64 / n_dry as f64,
        band_density,
        density_ha: density * 10_000.0,
        nn: [quantile(&nn, 0.1), quantile(&nn, 0.5), quantile(&nn, 0.9)],
        clark_evans: if density > 0.0 {
            mean_nn / (0.5 / density.sqrt())
        } else {
            0.0
        },
        k_ratio,
        slope_pref: pref(inst_slope, ground_slope),
        height_pref: pref(inst_height, ground_height),
        slope: [quantile(&slopes, 0.1), quantile(&slopes, 0.5), quantile(&slopes, 0.9)],
        tilt: [quantile(&tilts, 0.5), quantile(&tilts, 0.9), quantile(&tilts, 0.99)],
        tilt_normal: quantile(&tilt_n, 0.5),
        yaw_r: (yc * yc + ys * ys).sqrt() / n,
        scale: [quantile(&scales, 0.05), quantile(&scales, 0.5), quantile(&scales, 0.95)],
        aspect: quantile(&aspects, 0.5),
        sink: [quantile(&sinks, 0.1), quantile(&sinks, 0.5), quantile(&sinks, 0.9)],
    })
}

/// Every kind's statistics of a level's placed members on its ground, `outside` the distance outside its playable
/// area.
#[must_use]
pub fn measure(g: &Ground, instances: &[Instance], outside: Outside<'_>) -> LevelStats {
    let (_, dry) = dry_cells(g);
    let areas = band_areas(g, outside);
    let dry_share = dry.iter().filter(|&&d| d).count() as f64 / dry.len().max(1) as f64;
    let kinds = Kind::ALL.map(|k| {
        let of: Vec<Instance> = instances.iter().filter(|i| i.kind == k).copied().collect();
        kind_stats(g, &of, outside, &areas)
    });
    LevelStats {
        dry_cells: dry_share,
        areas,
        kinds,
    }
}

// ------------------------------------------------------------------------------------------------ the placement

/// How one kind is placed: a biome's statistics of it made a plan ([`Plan::of`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// The kind.
    pub kind: Kind,
    /// Instances per hectare of dry ground in each of [`AREA_BANDS`].
    pub band_density: [f64; 6],
    /// The slope preference per [`SLOPE_BANDS`] band (the instances' share over the ground's).
    pub slope_pref: [f64; 6],
    /// The height preference per [`HEIGHT_BANDS`] band.
    pub height_pref: [f64; 6],
    /// Instances per cluster (a Thomas process's mean offspring).
    pub mu: f64,
    /// The clusters' spread in metres (the offspring's standard deviation along each axis).
    pub sigma: f64,
    /// No two instances closer than this, in metres.
    pub min_gap: f64,
}

/// The neighbour ratio a Thomas process of `density` (per square metre), `mu` offspring per cluster and spread
/// `sigma` gives at radius `r`: K(r) / (pi r^2) = 1 + mu (1 - exp(-r^2 / 4 sigma^2)) / (density pi r^2).
#[must_use]
pub fn thomas_k_ratio(density: f64, mu: f64, sigma: f64, r: f64) -> f64 {
    let a = std::f64::consts::PI * r * r;
    1.0 + mu * (1.0 - (-r * r / (4.0 * sigma * sigma)).exp()) / (density * a).max(1e-12)
}

/// The cluster (`mu`, `sigma`) whose Thomas neighbour ratios at [`RADII`] 3, 10 and 30 m are nearest (in logs) to
/// `k_ratio` at `density_ha`: a search over 1 - 64 instances a cluster and 1 - 64 m.
#[must_use]
pub fn fit_cluster(k_ratio: &[f64; 4], density_ha: f64) -> (f64, f64) {
    let density = (density_ha / 10_000.0).max(1e-9);
    let mut best = (1.0, 8.0, f64::INFINITY);
    for mi in 0..=48 {
        let mu = 2f64.powf(f64::from(mi) / 8.0);
        for si in 0..=48 {
            let sigma = 2f64.powf(f64::from(si) / 8.0);
            let e: f64 = RADII
                .iter()
                .zip(k_ratio)
                .take(3)
                .map(|(&r, &k)| {
                    let m = thomas_k_ratio(density, mu, sigma, r);
                    (m.max(1e-6).ln() - k.max(1e-6).ln()).powi(2)
                })
                .sum();
            if e < best.2 {
                best = (mu, sigma, e);
            }
        }
    }
    (best.0, best.1)
}

impl Plan {
    /// The plan of a kind's statistics: the densities and preferences as measured, the cluster fitted to the
    /// neighbour ratios ([`fit_cluster`]), the gap half the nearest-neighbour distance's 10th percentile.
    #[must_use]
    pub fn of(kind: Kind, s: &KindStats) -> Self {
        let (mu, sigma) = fit_cluster(&s.k_ratio, s.density_ha);
        let neutral = |v: [f64; 6]| v.map(|x| if x < 0.0 { 1.0 } else { x });
        Self {
            kind,
            band_density: s.band_density.map(|x| x.max(0.0)),
            slope_pref: neutral(s.slope_pref),
            height_pref: neutral(s.height_pref),
            mu,
            sigma,
            min_gap: s.nn[0] * 0.5,
        }
    }

    fn weight(&self, g: &Ground, x: f64, z: f64) -> f64 {
        if g.water.is_some_and(|w| g.height(x, z) <= w) {
            return 0.0;
        }
        self.slope_pref
            .get(band(&SLOPE_BANDS, g.slope(x, z)))
            .copied()
            .unwrap_or(0.0)
            * self
                .height_pref
                .get(band(&HEIGHT_BANDS, g.over_water(x, z)))
                .copied()
                .unwrap_or(0.0)
    }
}

/// A member's pose relative to the ground, its yaw taken out: its three axes in the ground's frame at its foot (the
/// vertical frame, or the normal's when `normal` is set) and its height over the ground. A member type's poses are
/// its DICE instances' ([`Pose::of`]): a new instance takes one of them, turned to a new yaw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// Right, up and forward in the frame, yaw 0.
    pub axes: [[f64; 3]; 3],
    /// The position's height over the ground in metres.
    pub sink: f64,
    /// Whether the frame's up is the ground's normal (else the vertical).
    pub normal: bool,
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt().max(1e-12);
    [a[0] / l, a[1] / l, a[2] / l]
}

/// The ground frame at (`x`, `z`): right, up, forward (up the normal or the vertical, forward world +z made
/// perpendicular to it).
fn frame(g: &Ground, x: f64, z: f64, normal: bool) -> [[f64; 3]; 3] {
    let up = if normal { g.normal(x, z) } else { [0.0, 1.0, 0.0] };
    let f = unit([-up[2] * up[0], -up[2] * up[1], 1.0 - up[2] * up[2]]);
    let r = cross(up, f);
    [r, up, f]
}

impl Pose {
    /// The pose of a placed member on its ground.
    #[must_use]
    pub fn of(rows: &Rows, g: &Ground, normal: bool) -> Self {
        let (x, z) = (f64::from(rows[3][0]), f64::from(rows[3][2]));
        let b = frame(g, x, z, normal);
        let local = |k: usize| {
            let v = rows.get(k).copied().unwrap_or_default().map(f64::from);
            [dot(v, b[0]), dot(v, b[1]), dot(v, b[2])]
        };
        let l = [local(0), local(1), local(2)];
        // the yaw: the forward axis's heading in the frame's plane
        let yaw = l[2][0].atan2(l[2][2]);
        let (c, s) = (yaw.cos(), yaw.sin());
        let turn = |v: [f64; 3]| [c * v[0] - s * v[2], v[1], s * v[0] + c * v[2]];
        Self {
            axes: l.map(turn),
            sink: f64::from(rows[3][1]) - g.height(x, z),
            normal,
        }
    }

    /// The transform of this pose at (`x`, `z`) turned by `yaw` (radians).
    #[must_use]
    pub fn at(&self, g: &Ground, x: f64, z: f64, yaw: f64) -> Rows {
        let b = frame(g, x, z, self.normal);
        let (c, s) = (yaw.cos(), yaw.sin());
        // the inverse of Pose::of's turn
        let turn = |v: [f64; 3]| [c * v[0] + s * v[2], v[1], -s * v[0] + c * v[2]];
        let world = |v: [f64; 3]| {
            let t = turn(v);
            std::array::from_fn(|k| (t[0] * b[0][k] + t[1] * b[1][k] + t[2] * b[2][k]) as f32)
        };
        [
            world(self.axes[0]),
            world(self.axes[1]),
            world(self.axes[2]),
            [x as f32, (g.height(x, z) + self.sink) as f32, z as f32],
        ]
    }
}

/// A member type a placement may use: its kind, the caller's index for it, its share of its kind's instances and the
/// poses its DICE instances have.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeChoice {
    /// The kind.
    pub kind: Kind,
    /// The caller's index.
    pub ty: usize,
    /// Its share among its kind's types (any positive scale).
    pub weight: f64,
    /// Its poses (at least one).
    pub poses: Vec<Pose>,
}

/// A placed member.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    /// The kind.
    pub kind: Kind,
    /// The member type (the caller's index of [`TypeChoice::ty`]).
    pub ty: usize,
    /// The pose it took: its index in the type's [`TypeChoice::poses`] (a writer copies that DICE instance's record).
    pub pose: usize,
    /// The transform.
    pub rows: Rows,
}

/// A Poisson count of mean `mean` from uniform values (Knuth's product method; a normal approximation past 30).
fn poisson(mean: f64, u: &mut dyn FnMut() -> f64) -> usize {
    if mean > 30.0 {
        let (a, b) = (u(), u());
        let z = (-2.0 * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos();
        return (mean + mean.sqrt() * z).round().max(0.0) as usize;
    }
    let l = (-mean).exp();
    let (mut k, mut p) = (0usize, 1.0);
    loop {
        p *= u();
        if p <= l {
            return k;
        }
        k += 1;
    }
}

/// The instances of the kinds of `plans` on `g` (spec TGEN-VEG-2): per kind and band of distance outside the
/// playable area, as many as the band's density gives on the band's dry ground; each from a cluster (a parent
/// uniform on the band's dry ground, offspring around it by the plan's spread), kept with the ground's preference
/// (slope and height over the water, over its largest) and when no other instance of the kind is nearer than the
/// plan's gap; its member type by the types' weights, its pose one of the type's turned to a uniform yaw. Every type
/// gets at least one instance. The same arguments give the same instances (seeded streams, drawn one after another).
#[must_use]
pub fn place(g: &Ground, plans: &[Plan], types: &[TypeChoice], outside: Outside<'_>, seed: u64) -> Vec<Placed> {
    let mut out = Vec::new();
    let areas = band_areas(g, outside);
    for (pi, plan) in plans.iter().enumerate() {
        let choices: Vec<&TypeChoice> = types
            .iter()
            .filter(|t| t.kind == plan.kind && !t.poses.is_empty())
            .collect();
        if choices.is_empty() {
            continue;
        }
        let stream = crate::rng::Stream::new(seed, 0x7E6_0000 + pi as u64);
        let mut idx = 0u64;
        let mut u = || {
            idx += 1;
            stream.unit(idx)
        };
        let mut placed_here = place_kind(g, plan, &choices, outside, &areas, &mut u);
        every_type_once(g, plan.kind, &choices, &mut placed_here, &mut u);
        out.extend(placed_here);
    }
    out
}

/// A plan's cluster made to give DICE's clustering on this ground (TGEN-VEG-2): the plan placed, measured as DICE's
/// levels are ([`kind_stats`]) and its cluster count scaled by the neighbour ratio's excess at 10 m and its spread by
/// the nearest-neighbour median, `rounds` times; the plan whose measure lay nearest `target`'s (in logs) is kept.
/// The thinning by slope and height and the playable area's bands make a placement's clustering other than the
/// Thomas process's own, so the fit of [`Plan::of`] is only the start.
#[must_use]
pub fn calibrate(
    g: &Ground,
    plan: &Plan,
    target: &KindStats,
    types: &[TypeChoice],
    outside: Outside<'_>,
    seed: u64,
    rounds: usize,
) -> Plan {
    let choices: Vec<&TypeChoice> = types
        .iter()
        .filter(|t| t.kind == plan.kind && !t.poses.is_empty())
        .collect();
    if choices.is_empty() || target.k_ratio[1] <= 0.0 || target.nn[1] <= 0.0 {
        return plan.clone();
    }
    let areas = band_areas(g, outside);
    let mut cur = plan.clone();
    let mut best = (plan.clone(), f64::INFINITY);
    for round in 0..=rounds {
        let stream = crate::rng::Stream::new(seed, 0x7E6_1000 + round as u64);
        let mut idx = 0u64;
        let mut u = || {
            idx += 1;
            stream.unit(idx)
        };
        let placed = place_kind(g, &cur, &choices, outside, &areas, &mut u);
        let inst: Vec<Instance> = placed
            .iter()
            .map(|p| Instance {
                kind: p.kind,
                ty: p.ty,
                rows: p.rows,
            })
            .collect();
        let Some(m) = kind_stats(g, &inst, outside, &areas) else {
            break;
        };
        let (k_got, k_want) = ((m.k_ratio[1] - 1.0).max(0.02), (target.k_ratio[1] - 1.0).max(0.02));
        let (nn_got, nn_want) = (m.nn[1].max(0.05), target.nn[1].max(0.05));
        let e = (k_got / k_want).ln().powi(2) + (nn_got / nn_want).ln().powi(2);
        if e < best.1 {
            best = (cur.clone(), e);
        }
        cur.mu = (cur.mu * (k_want / k_got).clamp(0.25, 4.0)).clamp(1.0, 256.0);
        cur.sigma = (cur.sigma * (nn_want / nn_got).clamp(0.5, 2.0)).clamp(0.5, 128.0);
    }
    best.0
}

/// A biome's placement on `g` (TGEN-VEG-2): each kind DICE's terrains of the biome place ([`BiomeStats::dice`]) and
/// `types` hold planned from DICE's medians ([`Plan::of`]), every band's density scaled by `density`, its cluster
/// calibrated on this ground ([`calibrate`], three rounds, at DICE's density only), then placed ([`place`]).
#[must_use]
pub fn place_biome(
    g: &Ground,
    biome: crate::Biome,
    types: &[TypeChoice],
    outside: Outside<'_>,
    seed: u64,
    density: f64,
) -> (Vec<Plan>, Vec<Placed>) {
    let stats = BiomeStats::dice(biome);
    let mut plans = Vec::new();
    for k in Kind::ALL {
        if !types.iter().any(|t| t.kind == k) {
            continue;
        }
        let Some(Some((_, s))) = stats.kinds.get(k.index()) else {
            continue;
        };
        let mut p = Plan::of(k, s);
        if (density - 1.0).abs() < 1e-9 {
            p = calibrate(g, &p, s, types, outside, seed, 6);
        } else {
            p.band_density = p.band_density.map(|d| d * density.max(0.0));
        }
        plans.push(p);
    }
    let placed = place(g, &plans, types, outside, seed);
    (plans, placed)
}

fn place_kind(
    g: &Ground,
    plan: &Plan,
    choices: &[&TypeChoice],
    outside: Outside<'_>,
    areas: &[f64; 6],
    u: &mut dyn FnMut() -> f64,
) -> Vec<Placed> {
    let total_w: f64 = choices.iter().map(|t| t.weight.max(0.0)).sum::<f64>().max(1e-12);
    // the largest weight, for the thinning
    let w_max =
        plan.slope_pref.iter().copied().fold(0.0, f64::max) * plan.height_pref.iter().copied().fold(0.0, f64::max);
    // the share of offspring the thinning keeps on this ground (its dry ground sampled every 8 m): a cluster gets as
    // many more, so the kept clusters hold the plan's count
    let (mut sum, mut n) = (0.0, 0usize);
    let k = (g.side() / 8.0) as usize;
    for j in 0..k {
        for i in 0..k {
            let (x, z) = (g.x0 + 4.0 + 8.0 * i as f64, g.z0 + 4.0 + 8.0 * j as f64);
            if g.water.is_some_and(|w| g.height(x, z) <= w) {
                continue;
            }
            sum += plan.weight(g, x, z);
            n += 1;
        }
    }
    let keep = if n > 0 && w_max > 0.0 {
        (sum / n as f64 / w_max).clamp(0.05, 1.0)
    } else {
        1.0
    };
    let mut kept: Vec<[f64; 2]> = Vec::new();
    let mut grid = KeepGrid::new(g, plan.min_gap.max(0.5));
    let mut placed: Vec<Placed> = Vec::new();
    let tau = std::f64::consts::TAU;
    for b in 0..AREA_BANDS.len() {
        let want = (plan.band_density.get(b).copied().unwrap_or(0.0) * areas.get(b).copied().unwrap_or(0.0)).round();
        let want = want as usize;
        if want == 0 || w_max <= 0.0 {
            continue;
        }
        let mut got = 0usize;
        let mut tries = 0usize;
        while got < want && tries < 400 * want + 1000 {
            tries += 1;
            // a parent uniform on the band's dry ground
            let (px, pz) = (g.x0 + u() * g.side(), g.z0 + u() * g.side());
            if band(&AREA_BANDS, outside(px, pz)) != b || g.water.is_some_and(|w| g.height(px, pz) <= w) {
                continue;
            }
            let n = poisson(plan.mu / keep, u).max(1);
            for _ in 0..n {
                if got >= want {
                    break;
                }
                let (a, c) = (u(), u());
                let r = plan.sigma * (-2.0 * a.ln()).sqrt();
                let (x, z) = (px + r * (tau * c).cos(), pz + r * (tau * c).sin());
                let edge = g.side() - 1.0;
                if x <= g.x0 + 1.0 || z <= g.z0 + 1.0 || x >= g.x0 + edge || z >= g.z0 + edge {
                    continue;
                }
                if u() * w_max >= plan.weight(g, x, z) || grid.near(&kept, x, z, plan.min_gap) {
                    continue;
                }
                // the type by weight, a pose of it, a yaw
                let mut pick = u() * total_w;
                let Some(t) = choices
                    .iter()
                    .find(|t| {
                        pick -= t.weight.max(0.0);
                        pick < 0.0
                    })
                    .or_else(|| choices.last())
                    .copied()
                else {
                    continue;
                };
                let k = ((u() * t.poses.len() as f64) as usize).min(t.poses.len().saturating_sub(1));
                let Some(pose) = t.poses.get(k) else { continue };
                let yaw = u() * tau;
                grid.insert(kept.len(), x, z);
                kept.push([x, z]);
                placed.push(Placed {
                    kind: plan.kind,
                    ty: t.ty,
                    pose: k,
                    rows: pose.at(g, x, z, yaw),
                });
                got += 1;
            }
        }
    }
    placed
}

/// Every type at least once (a group keeps one of each of its kinds, LEVEL-MG-7): a type no instance took takes the
/// last instance of its kind's most used type; with none to take, one instance on dry ground.
fn every_type_once(
    g: &Ground,
    kind: Kind,
    choices: &[&TypeChoice],
    placed: &mut Vec<Placed>,
    u: &mut dyn FnMut() -> f64,
) {
    let tau = std::f64::consts::TAU;
    for t in choices {
        if placed.iter().any(|p| p.ty == t.ty) {
            continue;
        }
        let Some(pose) = t.poses.first() else { continue };
        let mut counts: std::collections::BTreeMap<usize, usize> = std::collections::BTreeMap::new();
        for p in placed.iter() {
            *counts.entry(p.ty).or_default() += 1;
        }
        let donor = counts
            .iter()
            .filter(|(_, c)| **c > 1)
            .max_by_key(|(ty, c)| (**c, std::cmp::Reverse(**ty)))
            .map(|(ty, _)| *ty);
        if let Some(dty) = donor
            && let Some(p) = placed.iter_mut().rev().find(|p| p.ty == dty)
        {
            let (x, z) = (f64::from(p.rows[3][0]), f64::from(p.rows[3][2]));
            *p = Placed {
                kind,
                ty: t.ty,
                pose: 0,
                rows: pose.at(g, x, z, u() * tau),
            };
            continue;
        }
        for _ in 0..10_000 {
            let (x, z) = (g.x0 + u() * g.side(), g.z0 + u() * g.side());
            if g.water.is_some_and(|w| g.height(x, z) <= w) {
                continue;
            }
            placed.push(Placed {
                kind,
                ty: t.ty,
                pose: 0,
                rows: pose.at(g, x, z, u() * tau),
            });
            break;
        }
    }
}

/// Kept instances by a grid of the gap's size, for the gap test.
struct KeepGrid {
    cell: f64,
    x0: f64,
    z0: f64,
    m: usize,
    cells: std::collections::HashMap<usize, Vec<usize>>,
}

impl KeepGrid {
    fn new(g: &Ground, cell: f64) -> Self {
        Self {
            cell,
            x0: g.x0,
            z0: g.z0,
            m: (g.side() / cell).ceil() as usize + 1,
            cells: std::collections::HashMap::new(),
        }
    }

    fn ij(&self, x: f64, z: f64) -> (usize, usize) {
        (
            ((x - self.x0) / self.cell).max(0.0) as usize,
            ((z - self.z0) / self.cell).max(0.0) as usize,
        )
    }

    fn insert(&mut self, k: usize, x: f64, z: f64) {
        let (i, j) = self.ij(x, z);
        self.cells.entry(j * self.m + i).or_default().push(k);
    }

    fn near(&self, kept: &[[f64; 2]], x: f64, z: f64, gap: f64) -> bool {
        if gap <= 0.0 {
            return false;
        }
        let (i, j) = self.ij(x, z);
        for jj in j.saturating_sub(1)..=j + 1 {
            for ii in i.saturating_sub(1)..=i + 1 {
                if let Some(c) = self.cells.get(&(jj * self.m + ii))
                    && c.iter()
                        .any(|&k| kept.get(k).is_some_and(|p| (p[0] - x).hypot(p[1] - z) < gap))
                {
                    return true;
                }
            }
        }
        false
    }
}
