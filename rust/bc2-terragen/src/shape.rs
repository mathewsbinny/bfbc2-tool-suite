//! TGEN-SHAPE's statistics of a main block, computed as `spec/probes/terrain_shape.py`'s `shape()` computes them
//! (NumPy's `gradient`, `percentile`, `median`, its 2 m mean grid, box means with clamped edges, the plane-removed
//! Hann-windowed radial spectrum), so the generator measures itself on DICE's scale while it shapes a terrain and
//! the CLI can report it. The probe's own numbers on the written files stay the reference (rust/BENCHMARKS.md TG1).

use rayon::prelude::*;

use crate::fft::{C, fft2, fftfreq};

/// The statistics of one terrain (the probe's keys).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Shape {
    /// Samples a side.
    pub n: usize,
    /// Hypsometric integral: (mean - min) / (max - min).
    pub hi: f64,
    /// Slope percentiles 10, 50, 90, 99 in degrees (central differences).
    pub slope: [f64; 4],
    /// Shares of the ground steeper than 15, 30, 45 degrees.
    pub steep: [f64; 3],
    /// RMS relief against a 10 m mean (on the 2 m grid).
    pub rough8: f64,
    /// RMS relief against a 66 m mean (on the 2 m grid).
    pub rough64: f64,
    /// The radial power spectrum's slope (wavelengths 16 - 256 m).
    pub beta: f64,
    /// The border ring's median height minus the centre's.
    pub edge_minus_centre: f64,
    /// The share of samples under the water level.
    pub wet: f64,
    /// Elevation percentiles 0, 1, 10, 50, 90, 99, 100 in metres.
    pub elev: [f64; 7],
}

/// NumPy's `percentile` (linear) of sorted values.
#[must_use]
pub fn percentile_sorted(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let pos = q / 100.0 * (sorted.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = pos.ceil() as usize;
    let (a, b) = (
        sorted.get(lo).copied().unwrap_or(f64::NAN),
        sorted.get(hi).copied().unwrap_or(f64::NAN),
    );
    a + (b - a) * (pos - lo as f64)
}

/// NumPy's `median`.
#[must_use]
pub fn median(mut v: Vec<f64>) -> f64 {
    v.par_sort_unstable_by(f64::total_cmp);
    percentile_sorted(&v, 50.0)
}

/// NumPy's `gradient(h, cell)` along z and x: central differences inside, one-sided on the edges.
#[must_use]
pub fn gradient(h: &[f64], n: usize, cell: f64) -> (Vec<f64>, Vec<f64>) {
    let at = |i: usize, j: usize| h.get(j * n + i).copied().unwrap_or(0.0);
    let mut gz = vec![0.0; n * n];
    let mut gx = vec![0.0; n * n];
    gz.par_chunks_mut(n)
        .zip(gx.par_chunks_mut(n))
        .enumerate()
        .for_each(|(j, (rz, rx))| {
            for i in 0..n {
                let z = if j == 0 {
                    (at(i, 1) - at(i, 0)) / cell
                } else if j == n - 1 {
                    (at(i, n - 1) - at(i, n - 2)) / cell
                } else {
                    (at(i, j + 1) - at(i, j - 1)) / (2.0 * cell)
                };
                let x = if i == 0 {
                    (at(1, j) - at(0, j)) / cell
                } else if i == n - 1 {
                    (at(n - 1, j) - at(n - 2, j)) / cell
                } else {
                    (at(i + 1, j) - at(i - 1, j)) / (2.0 * cell)
                };
                if let Some(v) = rz.get_mut(i) {
                    *v = z;
                }
                if let Some(v) = rx.get_mut(i) {
                    *v = x;
                }
            }
        });
    (gz, gx)
}

/// Slope in degrees per sample (central differences, `cell` metres).
#[must_use]
pub fn slopes(h: &[f64], n: usize, cell: f64) -> Vec<f64> {
    let (gz, gx) = gradient(h, n, cell);
    gz.par_iter()
        .zip(&gx)
        .map(|(z, x)| z.hypot(*x).atan().to_degrees())
        .collect()
}

/// The 2 x 2 mean grid (the probe's 2 m grid).
#[must_use]
pub fn half(h: &[f64], n: usize) -> Vec<f64> {
    let m = n / 2;
    let at = |i: usize, j: usize| h.get(j * n + i).copied().unwrap_or(0.0);
    (0..m * m)
        .into_par_iter()
        .map(|k| {
            let (j, i) = (k / m, k % m);
            (at(2 * i, 2 * j) + at(2 * i + 1, 2 * j) + at(2 * i, 2 * j + 1) + at(2 * i + 1, 2 * j + 1)) / 4.0
        })
        .collect()
}

/// The mean over a (2r + 1)-sample square, edges clamped (the probe's `box_blur`).
#[must_use]
pub fn box_mean(h: &[f64], m: usize, r: usize) -> Vec<f64> {
    let k = (2 * r + 1) as f64;
    let pass = |src: &[f64], along_x: bool| -> Vec<f64> {
        let mut out = vec![0.0; m * m];
        out.par_chunks_mut(m).enumerate().for_each(|(a, row)| {
            for (b, o) in row.iter_mut().enumerate() {
                let mut s = 0.0;
                for t in 0..=2 * r {
                    let c = (b + t).saturating_sub(r).min(m - 1);
                    s += if along_x {
                        src.get(a * m + c)
                    } else {
                        src.get(c * m + a)
                    }
                    .copied()
                    .unwrap_or(0.0);
                }
                *o = s / k;
            }
        });
        out
    };
    let x = pass(h, true);
    // the second pass reads columns: out[a][b] above is (row a, column b) for x; for z, (column a, row b)
    let z = pass(&x, false);
    let mut out = vec![0.0; m * m];
    out.par_chunks_mut(m).enumerate().for_each(|(j, row)| {
        for (i, o) in row.iter_mut().enumerate() {
            *o = z.get(i * m + j).copied().unwrap_or(0.0);
        }
    });
    out
}

fn std(v: &[f64]) -> f64 {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    (v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / n).sqrt()
}

/// Least squares line through (x, y): its slope (NumPy's `polyfit(x, y, 1)[0]`).
fn line_slope(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
    let sxy: f64 = x.iter().zip(y).map(|(a, b)| (a - mx) * (b - my)).sum();
    let sxx: f64 = x.iter().map(|a| (a - mx) * (a - mx)).sum();
    sxy / sxx
}

/// beta in P(k) ~ k^-beta of the radially averaged power spectrum, wavelengths 16 - 256 m (the probe's
/// `spectrum_slope`: plane removed, Hann window, 12 logarithmic bins).
#[must_use]
pub fn spectrum_slope(h2: &[f64], m: usize, step: f64) -> f64 {
    let (lo, hi) = (16.0f64, 256.0f64);
    // the plane: least squares of h on (x, z, 1) by the normal equations
    let mut s = [[0.0f64; 3]; 3];
    let mut t = [0.0f64; 3];
    for j in 0..m {
        for i in 0..m {
            let v = [i as f64, j as f64, 1.0];
            let h = h2.get(j * m + i).copied().unwrap_or(0.0);
            for (a, va) in v.iter().enumerate() {
                for (b, vb) in v.iter().enumerate() {
                    if let Some(x) = s.get_mut(a).and_then(|r| r.get_mut(b)) {
                        *x += va * vb;
                    }
                }
                if let Some(x) = t.get_mut(a) {
                    *x += va * h;
                }
            }
        }
    }
    let coef = solve3(s, t);
    let w: Vec<f64> = (0..m)
        .map(|k| 0.5 - 0.5 * (std::f64::consts::TAU * k as f64 / (m - 1) as f64).cos())
        .collect();
    let a: Vec<C> = (0..m * m)
        .map(|k| {
            let (j, i) = (k / m, k % m);
            let d = h2.get(k).copied().unwrap_or(0.0) - (coef[0] * i as f64 + coef[1] * j as f64 + coef[2]);
            C::new(
                d * w.get(j).copied().unwrap_or(0.0) * w.get(i).copied().unwrap_or(0.0),
                0.0,
            )
        })
        .collect();
    let p = fft2(&a, m, false);
    let k = fftfreq(m, step);
    let edges: Vec<f64> = (0..13)
        .map(|e| {
            let (l0, l1) = ((1.0 / hi).log10(), (1.0 / lo).log10());
            10f64.powf(l0 + (l1 - l0) * f64::from(e) / 12.0)
        })
        .collect();
    let mut sums = [(0.0f64, 0.0f64, 0usize); 13];
    for (r, kz) in k.iter().enumerate() {
        for (c, kx) in k.iter().enumerate() {
            let kr = kx.hypot(*kz);
            if kr < 1.0 / hi || kr > 1.0 / lo {
                continue;
            }
            // np.digitize: the first edge greater than kr
            let b = edges.iter().position(|e| kr < *e).unwrap_or(13);
            if let Some(x) = sums.get_mut(b) {
                x.0 += kr;
                x.1 += p.get(r * m + c).copied().unwrap_or_default().norm2();
                x.2 += 1;
            }
        }
    }
    let (mut lk, mut lp) = (Vec::new(), Vec::new());
    for &(kr, pw, cnt) in sums.iter().take(13).skip(1) {
        if cnt > 0 {
            lk.push((kr / cnt as f64).log10());
            lp.push((pw / cnt as f64).log10());
        }
    }
    -line_slope(&lk, &lp)
}

#[allow(clippy::indexing_slicing, reason = "fixed 3 x 3 indices, every one in range")]
fn solve3(mut a: [[f64; 3]; 3], mut b: [f64; 3]) -> [f64; 3] {
    for c in 0..3 {
        let p = (c..3)
            .max_by(|&x, &y| a[x][c].abs().total_cmp(&a[y][c].abs()))
            .unwrap_or(c);
        a.swap(c, p);
        b.swap(c, p);
        for r in 0..3 {
            if r != c && a[c][c] != 0.0 {
                let f = a[r][c] / a[c][c];
                for k in 0..3 {
                    a[r][k] -= f * a[c][k];
                }
                b[r] -= f * b[c];
            }
        }
    }
    [b[0] / a[0][0], b[1] / a[1][1], b[2] / a[2][2]]
}

/// The statistics of `n` x `n` heights in metres (rows along z) at `cell` metres, against a water level.
#[must_use]
pub fn shape(h: &[f64], n: usize, cell: f64, water_y: Option<f64>) -> Shape {
    let mut sl = slopes(h, n, cell);
    let total = sl.len() as f64;
    let steep = [15.0, 30.0, 45.0].map(|t| sl.par_iter().filter(|s| **s > t).count() as f64 / total);
    sl.par_sort_unstable_by(f64::total_cmp);
    let slope = [10.0, 50.0, 90.0, 99.0].map(|q| percentile_sorted(&sl, q));
    drop(sl);
    let h2 = half(h, n);
    let m = n / 2;
    let rough = |r: usize| {
        let b = box_mean(&h2, m, r);
        let d: Vec<f64> = h2.iter().zip(&b).map(|(a, c)| a - c).collect();
        std(&d)
    };
    let mut sorted = h.to_vec();
    sorted.par_sort_unstable_by(f64::total_cmp);
    let (lo, top) = (
        sorted.first().copied().unwrap_or(0.0),
        sorted.last().copied().unwrap_or(0.0),
    );
    let mean = h.iter().sum::<f64>() / total;
    let elev = [0.0, 1.0, 10.0, 50.0, 90.0, 99.0, 100.0].map(|q| percentile_sorted(&sorted, q));
    let ring = (n / 16).max(1);
    let mut edge = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let v = h.get(j * n + i).copied().unwrap_or(0.0);
            // the probe concatenates the four strips: a corner sample counts twice
            let k =
                usize::from(j < ring) + usize::from(j >= n - ring) + usize::from(i < ring) + usize::from(i >= n - ring);
            for _ in 0..k {
                edge.push(v);
            }
        }
    }
    let q = n / 4;
    let centre: Vec<f64> = (q..n - q)
        .flat_map(|j| (q..n - q).map(move |i| (i, j)))
        .map(|(i, j)| h.get(j * n + i).copied().unwrap_or(0.0))
        .collect();
    Shape {
        n,
        hi: (mean - lo) / (top - lo).max(1e-6),
        slope,
        steep,
        rough8: rough(2),
        rough64: rough(16),
        beta: spectrum_slope(&h2, m, 2.0 * cell),
        edge_minus_centre: median(edge) - median(centre),
        wet: water_y.map_or(0.0, |w| h.iter().filter(|v| **v < w).count() as f64 / total),
        elev,
    }
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn numpy_semantics() {
        assert_eq!(percentile_sorted(&[1.0, 2.0, 3.0, 4.0], 50.0), 2.5);
        assert_eq!(median(vec![3.0, 1.0, 2.0]), 2.0);
        // a plane 0.5 m per metre along x: 26.57 degrees everywhere, edges included
        let n = 8;
        let h: Vec<f64> = (0..n * n).map(|k| 0.5 * (k % n) as f64).collect();
        assert!(slopes(&h, n, 1.0).iter().all(|s| (s - 26.565_051).abs() < 1e-5));
        // box mean of a constant is the constant; of a ramp, the ramp inside
        let b = box_mean(&h, n, 1);
        assert!((b[3 * n + 3] - 1.5).abs() < 1e-12 && (b[3 * n] - 0.5 / 3.0).abs() < 1e-12);
    }
}
