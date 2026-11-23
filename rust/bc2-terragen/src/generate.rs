//! The generator's pipeline (TGEN-GEN-1 - 4): a pure function of the seed and the settings.
//!
//! The pipeline closes the prototype's gap (TGEN-SHAPE-2: its 99th-percentile slope 20 - 56 degrees where DICE's
//! biomes have 53 - 75, its 66 m relief half or less of DICE's): steps 2 and 5 are what close it.
//!
//! 1. **Spectral synthesis** (TGEN-SHAPE-3): Gaussian noise from the seed filtered to `P(k) ~ k^-beta`, the biome's
//!    spectrum slope; periodic on the square.
//! 2. **Height remapping and slope matching**, alternated: the heights remapped by a power so their hypsometric
//!    integral is the biome's (TGEN-SHAPE-1: low basins, a long upper tail); then every sample's gradient given the
//!    magnitude its rank has in the biome's slope distribution (TGEN-SHAPE-2: a flat floor and DICE's steep tail) and
//!    the surface rebuilt from the new gradients by a least-squares (Poisson) solve on the periodic square. Each round
//!    measures the slope percentiles as the probe does (central differences) and corrects the distribution it aims
//!    at, so the result lands on DICE's percentiles rather than on the forward differences' ones.
//! 3. **Hydraulic erosion** (TGEN-GEN-3): water droplets from the seed run down the surface, pick up sediment where
//!    they speed up and drop it where they slow down: valleys with flat floors between steep sides.
//! 4. **The border** (TGEN-SHAPE-4): a rise (or a fall, into sea) towards the edge, corrected until the border ring's
//!    median minus the centre's is the biome's (or the settings').
//! 5. **The sea floor** flattened (the ground under the coming water level keeps a share of its relief, found by
//!    bisection) where the 10th slope percentile would otherwise stay over 2 degrees above the biome's: DICE's sea
//!    and lake floors are flat. Then the **hypsometric integral** remapped to the biome's once more (erosion and the
//!    border moved it), the **relief** scaled so the median slope is the biome's, and the border corrected again.
//!
//!    Then **the rough relief** (TGEN-GEN-6; temperate by default): an escarpment raising a share of the ground,
//!    the border corrected again.
//! 6. **Pads** (TGEN-PLAY-3): a disc levelled at each objective site, blended into the ground around it.
//! 7. **Water** (TGEN-SHAPE-4): the level under which the biome's share of the ground lies; the heights from 0 and the
//!    range rounded up to 50 m, at least the biome's (TGEN-REQ-2).
//!
//! Every step runs over rows in parallel with each value computed on its own, and every sum is taken in a fixed
//! order, so the output is the same bytes on any number of threads; the random values are a hash of the seed and
//! the sample (`crate::rng`).

use rayon::prelude::*;

use crate::biome::{Biome, Targets};
use crate::fft::{C, fft2, fftfreq};
use crate::rng::Stream;
use crate::shape::{median, percentile_sorted, slopes};

/// The generator's version: a change to what a seed and settings give is a new version (TGEN-GEN-1); a project
/// records it with the seed. 2 (round TG2): temperate's escarpment (TGEN-GEN-6) and the sub-terrain ring.
pub const GENERATOR_VERSION: u32 = 2;

/// A site to level (TGEN-PLAY-3), in world metres (the terrain is centred on the origin).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pad {
    /// World x.
    pub x: f64,
    /// World z.
    pub z: f64,
    /// The levelled disc's radius in metres; it blends into the ground over as much again.
    pub radius: f64,
}

/// What the generator takes.
#[derive(Debug, Clone, PartialEq)]
pub struct Settings {
    /// The biome.
    pub biome: Biome,
    /// Samples a side: 1,024 or 2,048 (1 m samples; TGEN-REQ-2).
    pub size: u32,
    /// The seed.
    pub seed: u64,
    /// The border ring's median height minus the centre's in metres: positive raises the edge into hills, negative
    /// sinks it into sea; `None` takes the biome's.
    pub border_m: Option<f64>,
    /// The share of the ground under the water (0 - 1); `None` takes the biome's, 0 puts no ground under water.
    pub water_share: Option<f64>,
    /// Sites levelled for objectives.
    pub pads: Vec<Pad>,
    /// Hydraulic erosion's droplets per 1,000 samples (`None`: the default, 125).
    pub erosion: Option<u32>,
    /// Rough relief (TGEN-GEN-6): terraces and ridges over patches of the ground, 0 (none) to 1; `None` takes the
    /// biome's ([`Biome::rough`]: temperate's rocky terrains 1, the others 0).
    pub rough: Option<f64>,
    /// The water's height in metres (TGEN-VIS-2): the heights raised (or lowered, never under 0) so the water lies
    /// there - a donor's rule layers shade by height, so a terrain drawn with them puts its water at the donor's;
    /// `None` keeps the heights from 0.
    pub water_level_m: Option<f64>,
    /// The sub-terrain ring (TGEN-RING-1): twelve 64-sample blocks around the main block, its edges stitched to it
    /// as DICE stitches them (the default); `false` writes the main block alone (as 14 of DICE's 30 terrains).
    pub ring: bool,
}

impl Settings {
    /// The settings of a biome, size and seed with everything else the biome's.
    #[must_use]
    pub const fn new(biome: Biome, size: u32, seed: u64) -> Self {
        Self {
            biome,
            size,
            seed,
            border_m: None,
            water_share: None,
            pads: Vec::new(),
            erosion: None,
            rough: None,
            water_level_m: None,
            ring: true,
        }
    }
}

/// Why the generator refused its settings.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum GenError {
    /// A size other than 1,024 or 2,048 (TGEN-REQ-2: no other main block is known to load).
    Size(u32),
    /// A water share outside 0 - 1, or a pad outside the square or with a radius of 0 or less.
    Setting(String),
}

impl std::fmt::Display for GenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Size(s) => write!(
                f,
                "size {s}: BFBC2's main terrain blocks are 1024 or 2048 samples a side"
            ),
            Self::Setting(s) => f.write_str(s),
        }
    }
}

impl std::error::Error for GenError {}

/// A generated height field.
#[derive(Debug, Clone, PartialEq)]
pub struct Generated {
    /// Samples a side.
    pub size: u32,
    /// The stored values (TERR-HF-2): `size * size`, rows along z; height = value / 65535 * `range_m`.
    pub raw: Vec<u16>,
    /// The height range in metres (the bounds' max y; min y is 0).
    pub range_m: f64,
    /// The water level in metres, `None` when no ground is under water.
    pub water_y: Option<f64>,
}

impl Generated {
    /// The bounds (TGEN-REQ-2): min x, y, z, max x, y, z; centred on the origin, 1 m samples, heights from 0.
    #[must_use]
    pub fn bounds(&self) -> [f32; 6] {
        let a = (self.size / 2) as f32;
        [-a, 0.0, -a, a, self.range_m as f32, a]
    }

    /// The heights in metres as the game reads them (TERR-HF-2, the reader's f64 arithmetic rounded to f32).
    #[must_use]
    pub fn heights(&self) -> Vec<f32> {
        let s = f64::from(self.range_m as f32) / 65535.0;
        self.raw.iter().map(|&v| (f64::from(v) * s) as f32).collect()
    }
}

/// Generates a terrain.
///
/// # Errors
///
/// [`GenError`] for a size other than 1,024 / 2,048 or a setting out of range.
pub fn generate(s: &Settings) -> Result<Generated, GenError> {
    if s.size != 1024 && s.size != 2048 {
        return Err(GenError::Size(s.size));
    }
    if let Some(w) = s.water_share
        && !(0.0..1.0).contains(&w)
    {
        return Err(GenError::Setting(format!(
            "water share {w}: from 0 up to (not including) 1"
        )));
    }
    let n = s.size as usize;
    let half = f64::from(s.size) / 2.0;
    for p in &s.pads {
        if p.x.abs() >= half || p.z.abs() >= half || p.radius.is_nan() || p.radius <= 0.0 {
            return Err(GenError::Setting(format!(
                "pad at ({}, {}) radius {}: inside the square (|x|, |z| < {half}) with a radius over 0",
                p.x, p.z, p.radius
            )));
        }
    }
    let t = s.biome.targets();
    let mut h = spectral(n, t.beta, Stream::new(s.seed, 1));
    shape_slopes(&mut h, n, &t);
    let droplets = u64::from(s.erosion.unwrap_or(125)) * (n * n) as u64 / 1000;
    erode(&mut h, n, droplets, Stream::new(s.seed, 2));
    border(&mut h, n, s.border_m.unwrap_or(t.edge_minus_centre));
    let share = s.water_share.unwrap_or(t.wet);
    let floor = sea_floor_factor(&h, n, share, &t);
    flatten_sea_floor(&mut h, share, floor);
    remap_hypsometric(&mut h, n, t.hi);
    scale_to_median_slope(&mut h, n, t.slope[1]);
    // the remap and the scale moved the border: corrected again (a smooth rise, the slopes barely change)
    border(&mut h, n, s.border_m.unwrap_or(t.edge_minus_centre));
    let rough = s.rough.unwrap_or_else(|| s.biome.rough());
    if rough > 0.0 {
        terrace(&mut h, n, rough, Stream::new(s.seed, 3));
        border(&mut h, n, s.border_m.unwrap_or(t.edge_minus_centre));
    }
    for p in &s.pads {
        pad(&mut h, n, p.x + half, p.z + half, p.radius);
    }
    let lo = h.iter().copied().fold(f64::INFINITY, f64::min);
    h.par_iter_mut().for_each(|v| *v -= lo);
    if let Some(level) = s.water_level_m
        && share > 0.0
    {
        let mut v = h.clone();
        v.par_sort_unstable_by(f64::total_cmp);
        let shift = level - percentile_sorted(&v, share * 100.0);
        h.par_iter_mut().for_each(|x| *x = (*x + shift).max(0.0));
    }
    let top = h.iter().copied().fold(0.0, f64::max);
    let range_m = t.range_m.max((top / 50.0).ceil() * 50.0);
    let raw: Vec<u16> = h
        .par_iter()
        .map(|v| (v / range_m * 65535.0).round().clamp(0.0, 65535.0) as u16)
        .collect();
    let out = Generated {
        size: s.size,
        raw,
        range_m,
        water_y: None,
    };
    let water_y = (share > 0.0).then(|| {
        let mut v: Vec<f64> = out.heights().iter().map(|x| f64::from(*x)).collect();
        v.par_sort_unstable_by(f64::total_cmp);
        percentile_sorted(&v, share * 100.0)
    });
    Ok(Generated { water_y, ..out })
}

// ------------------------------------------------------------------------------------------------- 1. the spectrum

/// Gaussian noise filtered to `P(k) ~ k^-beta`, zero mean, unit standard deviation (periodic on the square).
#[must_use]
pub fn spectral(n: usize, beta: f64, rng: Stream) -> Vec<f64> {
    let noise: Vec<C> = (0..n * n)
        .into_par_iter()
        .map(|k| C::new(rng.normal(k as u64), 0.0))
        .collect();
    let mut f = fft2(&noise, n, false);
    let k = fftfreq(n, 1.0);
    f.par_chunks_mut(n).enumerate().for_each(|(r, row)| {
        let kz = k.get(r).copied().unwrap_or(0.0);
        for (c, x) in row.iter_mut().enumerate() {
            let kr = k.get(c).copied().unwrap_or(0.0).hypot(kz);
            let a = if kr == 0.0 { 0.0 } else { kr.powf(-beta / 2.0) };
            x.re *= a;
            x.im *= a;
        }
    });
    let h: Vec<f64> = fft2(&f, n, true).iter().map(|c| c.re).collect();
    standardise(h)
}

fn mean_rows(h: &[f64], n: usize) -> f64 {
    let rows: Vec<f64> = h.par_chunks(n).map(|r| r.iter().sum::<f64>()).collect();
    rows.iter().sum::<f64>() / h.len() as f64
}

fn standardise(mut h: Vec<f64>) -> Vec<f64> {
    let n = (h.len() as f64).sqrt() as usize;
    let m = mean_rows(&h, n);
    let rows: Vec<f64> = h
        .par_chunks(n)
        .map(|r| r.iter().map(|v| (v - m) * (v - m)).sum::<f64>())
        .collect();
    let sd = (rows.iter().sum::<f64>() / h.len() as f64).sqrt().max(1e-12);
    h.par_iter_mut().for_each(|v| *v = (*v - m) / sd);
    h
}

// --------------------------------------------------------------------------- 2. height remapping and slope matching

/// Remaps `h` (any scale) by `lo + (top - lo) * u^g`, u its position in its range, with `g` such that the
/// hypsometric integral is `hi`; keeps the range.
fn remap_hypsometric(h: &mut [f64], n: usize, hi: f64) {
    let lo = h.iter().copied().fold(f64::INFINITY, f64::min);
    let top = h.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (top - lo).max(1e-12);
    let u: Vec<f64> = h.par_iter().map(|v| (v - lo) / span).collect();
    let mean_at = |g: f64| {
        // every 4th row: the integral to 3 decimals at a quarter of the cost
        let rows: Vec<f64> = u
            .par_chunks(4 * n)
            .map(|r| r.iter().take(n).map(|x| x.powf(g)).sum::<f64>())
            .collect();
        rows.iter().sum::<f64>() / (rows.len() * n) as f64
    };
    let (mut a, mut b) = (0.05f64, 20.0f64);
    for _ in 0..50 {
        let g = (a * b).sqrt();
        if mean_at(g) > hi {
            a = g;
        } else {
            b = g;
        }
    }
    let g = (a * b).sqrt();
    h.par_iter_mut().zip(&u).for_each(|(v, x)| *v = lo + span * x.powf(g));
}

/// Rounds of remapping and matching.
const ROUNDS: usize = 5;

/// The hypsometric integral: (mean - min) / (max - min).
fn hypsometric(h: &[f64], n: usize) -> f64 {
    let lo = h.iter().copied().fold(f64::INFINITY, f64::min);
    let top = h.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    (mean_rows(h, n) - lo) / (top - lo).max(1e-12)
}

/// The target slope distribution: degrees at quantiles 0, 0.1, 0.5, 0.9, 0.99, 1 (the biome's percentiles, a top
/// a half of the p90 - p99 step above p99, at most 85 degrees).
fn quantile_table(t: &Targets) -> [(f64, f64); 6] {
    let [p10, p50, p90, p99] = t.slope;
    [
        (0.0, 0.0),
        (0.10, p10),
        (0.50, p50),
        (0.90, p90),
        (0.99, p99),
        (1.0, (p99 + 0.5 * (p99 - p90)).min(85.0).max(p99)),
    ]
}

fn table_at(table: &[(f64, f64)], q: f64) -> f64 {
    for w in table.windows(2) {
        if let [(q0, v0), (q1, v1)] = *w
            && q <= q1
        {
            return if q1 > q0 {
                v0 + (v1 - v0) * (q - q0) / (q1 - q0)
            } else {
                v1
            };
        }
    }
    table.last().map_or(0.0, |x| x.1)
}

/// The alternation of step 2: hypsometric remap, then gradients given their rank's magnitude in the (corrected)
/// target distribution and the surface rebuilt by a periodic Poisson solve; four rounds.
fn shape_slopes(h: &mut Vec<f64>, n: usize, t: &Targets) {
    let goal = quantile_table(t);
    // the distribution aimed at for the forward differences, in tan space; corrected each round
    let mut aim: Vec<(f64, f64)> = goal.iter().map(|&(q, d)| (q, d.to_radians().tan())).collect();
    // a start at about DICE's relief: the remap needs a scale, the matching sets the final one
    h.par_iter_mut().for_each(|v| *v *= 50.0);
    let mut hi_aim = t.hi;
    for _ in 0..ROUNDS {
        remap_hypsometric(h, n, hi_aim);
        *h = match_slopes(h, n, &aim);
        hi_aim = (hi_aim + t.hi - hypsometric(h, n)).clamp(0.05, 0.95);
        // measure as the probe measures and move the aim by the miss (tan space, per quantile)
        let mut sl = slopes(h, n, 1.0);
        sl.par_sort_unstable_by(f64::total_cmp);
        for (k, (q, want)) in goal.iter().enumerate() {
            if *q <= 0.0 || *q >= 1.0 {
                continue;
            }
            let got = percentile_sorted(&sl, q * 100.0).to_radians().tan();
            let want = want.to_radians().tan();
            if let Some(a) = aim.get_mut(k)
                && got > 1e-6
                && want > 1e-6
            {
                a.1 *= (want / got).clamp(0.2, 4.0);
            }
        }
        // keep the aim increasing
        for k in 1..aim.len() {
            let prev = aim.get(k - 1).map_or(0.0, |x| x.1);
            if let Some(a) = aim.get_mut(k) {
                a.1 = a.1.max(prev);
            }
        }
    }
}

/// One slope matching: forward-difference gradients (periodic), each scaled to the magnitude its rank takes in
/// `aim` (quantile, tan), the heights rebuilt by least squares (the periodic Poisson equation solved by FFT).
fn match_slopes(h: &[f64], n: usize, aim: &[(f64, f64)]) -> Vec<f64> {
    let at = |i: usize, j: usize| h.get((j % n) * n + (i % n)).copied().unwrap_or(0.0);
    let gx: Vec<f64> = (0..n * n)
        .into_par_iter()
        .map(|k| at(k % n + 1, k / n) - at(k % n, k / n))
        .collect();
    let gz: Vec<f64> = (0..n * n)
        .into_par_iter()
        .map(|k| at(k % n, k / n + 1) - at(k % n, k / n))
        .collect();
    let mag: Vec<f64> = gx.par_iter().zip(&gz).map(|(x, z)| x.hypot(*z)).collect();
    let mut order: Vec<u32> = (0..(n * n) as u32).collect();
    order.par_sort_unstable_by(|a, b| {
        let (x, y) = (
            mag.get(*a as usize).copied().unwrap_or(0.0),
            mag.get(*b as usize).copied().unwrap_or(0.0),
        );
        x.total_cmp(&y).then(a.cmp(b))
    });
    let total = (n * n) as f64;
    let mut scale = vec![0.0f64; n * n];
    for (rank, &k) in order.iter().enumerate() {
        let q = (rank as f64 + 0.5) / total;
        let want = table_at(aim, q);
        let m = mag.get(k as usize).copied().unwrap_or(0.0);
        if let Some(s) = scale.get_mut(k as usize) {
            *s = if m > 1e-12 { want / m } else { 0.0 };
        }
    }
    drop(order);
    // divergence of the scaled field (backward differences of the forward ones)
    let fx = |i: usize, j: usize| {
        let k = (j % n) * n + (i % n);
        gx.get(k).copied().unwrap_or(0.0) * scale.get(k).copied().unwrap_or(0.0)
    };
    let fz = |i: usize, j: usize| {
        let k = (j % n) * n + (i % n);
        gz.get(k).copied().unwrap_or(0.0) * scale.get(k).copied().unwrap_or(0.0)
    };
    let div: Vec<C> = (0..n * n)
        .into_par_iter()
        .map(|k| {
            let (i, j) = (k % n, k / n);
            C::new(fx(i, j) - fx(i + n - 1, j) + fz(i, j) - fz(i, j + n - 1), 0.0)
        })
        .collect();
    let mut f = fft2(&div, n, false);
    let lam: Vec<f64> = (0..n)
        .map(|u| 2.0 * (std::f64::consts::TAU * u as f64 / n as f64).cos() - 2.0)
        .collect();
    f.par_chunks_mut(n).enumerate().for_each(|(v, row)| {
        let lz = lam.get(v).copied().unwrap_or(0.0);
        for (u, x) in row.iter_mut().enumerate() {
            let l = lam.get(u).copied().unwrap_or(0.0) + lz;
            if l.abs() < 1e-12 {
                *x = C::default();
            } else {
                x.re /= l;
                x.im /= l;
            }
        }
    });
    fft2(&f, n, true).iter().map(|c| c.re).collect()
}

// -------------------------------------------------------------------------------------------- 3. hydraulic erosion

/// Droplet erosion (Beyer's particle model): `droplets` drops, each from a seeded start, run down the bilinear
/// gradient with inertia; where a drop carries less than its capacity it erodes the ground around it (radius 3),
/// where more, it deposits under itself. One after the other, so the result does not depend on threads.
fn erode(h: &mut [f64], n: usize, droplets: u64, rng: Stream) {
    const INERTIA: f64 = 0.05;
    const CAPACITY: f64 = 4.0;
    const MIN_SLOPE: f64 = 0.01;
    const ERODE: f64 = 0.3;
    const DEPOSIT: f64 = 0.3;
    const EVAPORATE: f64 = 0.01;
    const GRAVITY: f64 = 4.0;
    const STEPS: usize = 64;
    const RADIUS: i64 = 3;
    // the erosion brush: weights over the disc, summing to 1
    let mut brush: Vec<(i64, i64, f64)> = Vec::new();
    for dz in -RADIUS..=RADIUS {
        for dx in -RADIUS..=RADIUS {
            let d = ((dx * dx + dz * dz) as f64).sqrt();
            if d < RADIUS as f64 {
                brush.push((dx, dz, 1.0 - d / RADIUS as f64));
            }
        }
    }
    let wsum: f64 = brush.iter().map(|b| b.2).sum();
    for b in &mut brush {
        b.2 /= wsum;
    }
    let ni = n as i64;
    let get = |h: &[f64], i: i64, j: i64| h.get((j * ni + i) as usize).copied().unwrap_or(0.0);
    let sample = |h: &[f64], x: f64, z: f64| -> (f64, f64, f64) {
        let (i, j) = (x.floor() as i64, z.floor() as i64);
        let (u, v) = (x - i as f64, z - j as f64);
        let (a, b, c, d) = (get(h, i, j), get(h, i + 1, j), get(h, i, j + 1), get(h, i + 1, j + 1));
        let gx = (b - a) * (1.0 - v) + (d - c) * v;
        let gz = (c - a) * (1.0 - u) + (d - b) * u;
        let height = a * (1.0 - u) * (1.0 - v) + b * u * (1.0 - v) + c * (1.0 - u) * v + d * u * v;
        (height, gx, gz)
    };
    let span = (n - 2) as f64;
    for k in 0..droplets {
        let mut x = rng.unit(2 * k) * span;
        let mut z = rng.unit(2 * k + 1) * span;
        let (mut dx, mut dz) = (0.0f64, 0.0f64);
        let (mut speed, mut water, mut sediment) = (1.0f64, 1.0f64, 0.0f64);
        for _ in 0..STEPS {
            let (i, j) = (x.floor() as i64, z.floor() as i64);
            let (u, v) = (x - i as f64, z - j as f64);
            let (h0, gx, gz) = sample(h, x, z);
            dx = dx * INERTIA - gx * (1.0 - INERTIA);
            dz = dz * INERTIA - gz * (1.0 - INERTIA);
            let len = dx.hypot(dz);
            if len < 1e-12 {
                break;
            }
            dx /= len;
            dz /= len;
            x += dx;
            z += dz;
            if x < 0.0 || z < 0.0 || x >= span || z >= span {
                break;
            }
            let (h1, _, _) = sample(h, x, z);
            let dh = h1 - h0;
            let cap = (-dh).max(MIN_SLOPE) * speed * water * CAPACITY;
            if sediment > cap || dh > 0.0 {
                // deposit under the old position (bilinear), filling a pit up to its rim at most
                let amount = if dh > 0.0 {
                    dh.min(sediment)
                } else {
                    (sediment - cap) * DEPOSIT
                };
                sediment -= amount;
                for (di, dj, w) in [
                    (0, 0, (1.0 - u) * (1.0 - v)),
                    (1, 0, u * (1.0 - v)),
                    (0, 1, (1.0 - u) * v),
                    (1, 1, u * v),
                ] {
                    if let Some(c) = h.get_mut(((j + dj) * ni + i + di) as usize) {
                        *c += amount * w;
                    }
                }
            } else {
                // erode around the old position, never deeper than the drop's fall
                let amount = ((cap - sediment) * ERODE).min(-dh);
                for &(bx, bz, w) in &brush {
                    let (ci, cj) = (i + bx, j + bz);
                    if ci < 0 || cj < 0 || ci >= ni || cj >= ni {
                        continue;
                    }
                    if let Some(c) = h.get_mut((cj * ni + ci) as usize) {
                        let take = amount * w;
                        *c -= take;
                        sediment += take;
                    }
                }
            }
            speed = (speed * speed - dh * GRAVITY).max(0.0).sqrt();
            water *= 1.0 - EVAPORATE;
        }
    }
}

// ----------------------------------------------------------------------------------------------------- 4. the border

/// The bowl: 0 in the middle, 1 at the middle of the border, `(2 r)^3` of the distance from the centre in sides.
fn bowl(n: usize) -> Vec<f64> {
    (0..n * n)
        .into_par_iter()
        .map(|k| {
            let (i, j) = (k % n, k / n);
            let x = i as f64 / (n - 1) as f64 - 0.5;
            let z = j as f64 / (n - 1) as f64 - 0.5;
            (x.hypot(z) * 2.0).clamp(0.0, 1.0).powi(3)
        })
        .collect()
}

/// The border ring's median minus the centre's (the probe's `edge_minus_centre`).
fn edge_minus_centre(h: &[f64], n: usize) -> f64 {
    let ring = (n / 16).max(1);
    let q = n / 4;
    let mut edge = Vec::new();
    let mut centre = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let v = h.get(j * n + i).copied().unwrap_or(0.0);
            let k =
                usize::from(j < ring) + usize::from(j >= n - ring) + usize::from(i < ring) + usize::from(i >= n - ring);
            for _ in 0..k {
                edge.push(v);
            }
            if (q..n - q).contains(&i) && (q..n - q).contains(&j) {
                centre.push(v);
            }
        }
    }
    median(edge) - median(centre)
}

/// Adds the bowl until the border minus centre is `target` metres (three corrections).
fn border(h: &mut [f64], n: usize, target: f64) {
    let b = bowl(n);
    let unit = edge_minus_centre(&b, n).max(1e-6);
    for _ in 0..3 {
        let miss = target - edge_minus_centre(h, n);
        if miss.abs() < 0.5 {
            break;
        }
        let a = miss / unit;
        h.par_iter_mut().zip(&b).for_each(|(v, w)| *v += a * w);
    }
}

// ------------------------------------------------------------------------------------------ 4b. the rough relief

/// The escarpment's height in metres at full strength (TGEN-GEN-6).
const ESCARPMENT_M: f64 = 120.0;
/// The share of the ground on the raised side.
const TERRACE_SHARE: f64 = 0.3;
/// The share of the ground the escarpment's face spans (a band of the mask's quantiles).
const FACE_SHARE: f64 = 0.002;
/// The mask's spectrum slope: smooth (a mask of beta 3 is rough at the sample scale, and its face frays over a tenth
/// of the ground).
const MASK_BETA: f64 = 6.0;

/// The rough relief of DICE's rocky temperate terrains (TGEN-GEN-6): a terrace - the ground over a share of the
/// square, where a smooth mask from the seed is high, raised by an escarpment of `k` x [`ESCARPMENT_M`]. DICE's rough
/// relief is that kind of ground: on `MP_004`, `SP_005` and `SP_005_B` 1.5 - 2.3 % of the 2 m samples lie more than
/// 5 m off their 10 m mean, by up to 138 - 201 m (cliffs and escarpments over smooth slopes), not a ground rough
/// everywhere; terraces of many low steps or a riser on every contour raised the 90th slope percentile by 4 - 30
/// degrees for less relief (rust/BENCHMARKS.md TG2). Runs after the relief is scaled (the face is steep, the slopes
/// elsewhere are kept), then the border is corrected again.
fn terrace(h: &mut [f64], n: usize, k: f64, rng: Stream) {
    let mask = spectral(n, MASK_BETA, rng);
    let mut sorted = mask.clone();
    sorted.par_sort_unstable_by(f64::total_cmp);
    let total = sorted.len() as f64;
    let jump = k.clamp(0.0, 1.0) * ESCARPMENT_M;
    let edge = 1.0 - TERRACE_SHARE;
    h.par_iter_mut().zip(&mask).for_each(|(v, m)| {
        let q = sorted.partition_point(|x| x < m) as f64 / total;
        let x = ((q - edge) / FACE_SHARE + 0.5).clamp(0.0, 1.0);
        *v += jump * x * x * (3.0 - 2.0 * x);
    });
}

// --------------------------------------------------------------------------------------------- 5. relief, 6. pads

/// The 10th slope percentile after the steps that follow the border (sea floor, hypsometric remap, median slope).
fn p10_after(h: &[f64], n: usize, share: f64, floor: f64, t: &Targets) -> f64 {
    let mut g = h.to_vec();
    flatten_sea_floor(&mut g, share, floor);
    remap_hypsometric(&mut g, n, t.hi);
    scale_to_median_slope(&mut g, n, t.slope[1]);
    let mut sl = slopes(&g, n, 1.0);
    sl.par_sort_unstable_by(f64::total_cmp);
    percentile_sorted(&sl, 10.0)
}

/// A floor within this many degrees of the biome's 10th slope percentile is left as it is: flattening the floor
/// steepens the rest (the median is held), and a near miss at the bottom costs less than a far one at the top.
const FLOOR_SLACK_DEG: f64 = 2.0;

/// How much of its relief the ground under the water keeps (1: all; at least 0.05): DICE's sea and lake floors are
/// flat (the jungle and vietnam terrains' 10th slope percentile is 0 - 1 degree where a fractal floor has 5), so
/// the floor is flattened until the 10th slope percentile comes down to the biome's (bisection, six steps).
fn sea_floor_factor(h: &[f64], n: usize, share: f64, t: &Targets) -> f64 {
    if share <= 0.0 || p10_after(h, n, share, 1.0, t) <= t.slope[0] + FLOOR_SLACK_DEG {
        return 1.0;
    }
    let (mut a, mut b) = (0.05f64, 1.0f64);
    for _ in 0..6 {
        let c = f64::midpoint(a, b);
        if p10_after(h, n, share, c, t) > t.slope[0] {
            b = c;
        } else {
            a = c;
        }
    }
    f64::midpoint(a, b)
}

/// Compresses the ground under the water level of `share` towards that level, keeping `floor` of its relief: a
/// monotone remap, so the same share stays under the water.
fn flatten_sea_floor(h: &mut [f64], share: f64, floor: f64) {
    if share <= 0.0 || floor >= 1.0 {
        return;
    }
    let mut v = h.to_vec();
    v.par_sort_unstable_by(f64::total_cmp);
    let w = percentile_sorted(&v, share * 100.0);
    h.par_iter_mut().for_each(|x| {
        if *x < w {
            *x = w - (w - *x) * floor;
        }
    });
}

/// Scales the heights about their mean until the median slope is `deg` (at most six times).
fn scale_to_median_slope(h: &mut [f64], n: usize, deg: f64) {
    if deg <= 0.0 {
        return;
    }
    for _ in 0..6 {
        let mut sl = slopes(h, n, 1.0);
        sl.par_sort_unstable_by(f64::total_cmp);
        let got = percentile_sorted(&sl, 50.0);
        if got <= 0.0 || (got - deg).abs() < 0.05 {
            break;
        }
        let f = deg.to_radians().tan() / got.to_radians().tan();
        let m = mean_rows(h, n);
        h.par_iter_mut().for_each(|v| *v = m + (*v - m) * f);
    }
}

/// Levels a disc of `r` metres at sample (`ci`, `cj`) to the median height under it, blended linearly into the
/// ground over the next `r` (TGEN-PLAY-3).
fn pad(h: &mut [f64], n: usize, ci: f64, cj: f64, r: f64) {
    let reach = (2.0 * r).ceil() as i64 + 1;
    let (i0, j0) = (ci.round() as i64, cj.round() as i64);
    let ni = n as i64;
    let mut core = Vec::new();
    let mut cells = Vec::new();
    for j in (j0 - reach).max(0)..=(j0 + reach).min(ni - 1) {
        for i in (i0 - reach).max(0)..=(i0 + reach).min(ni - 1) {
            let d = (i as f64 - ci).hypot(j as f64 - cj);
            let k = (j * ni + i) as usize;
            if d <= r {
                core.push(h.get(k).copied().unwrap_or(0.0));
            }
            if d <= 2.0 * r {
                cells.push((k, ((d - r) / r).clamp(0.0, 1.0)));
            }
        }
    }
    if core.is_empty() {
        return;
    }
    let level = median(core);
    for (k, blend) in cells {
        if let Some(v) = h.get_mut(k) {
            *v = level + (*v - level) * blend;
        }
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn refuses_what_the_game_does_not_take() {
        assert_eq!(generate(&Settings::new(Biome::Arid, 512, 1)), Err(GenError::Size(512)));
        let mut s = Settings::new(Biome::Arid, 1024, 1);
        s.water_share = Some(1.0);
        assert!(matches!(generate(&s), Err(GenError::Setting(_))));
        let mut s = Settings::new(Biome::Arid, 1024, 1);
        s.pads.push(Pad {
            x: 600.0,
            z: 0.0,
            radius: 24.0,
        });
        assert!(matches!(generate(&s), Err(GenError::Setting(_))));
    }

    #[test]
    fn the_table_interpolates() {
        let t = quantile_table(&Biome::Jungle.targets());
        assert!((table_at(&t, 0.5) - 22.2).abs() < 1e-9);
        assert!((table_at(&t, 0.3) - (0.6 + 21.6 * 0.5)).abs() < 1e-9);
        assert!(table_at(&t, 1.0) <= 85.0);
    }
}
