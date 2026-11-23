//! A radix-2 complex FFT and its square 2D form, for spectral synthesis, the Poisson solve of the slope matching and
//! the spectrum statistic. Rows are transformed in parallel, each row on its own, so the result is the same on any
//! number of threads.

use rayon::prelude::*;

/// A complex number.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct C {
    /// Real part.
    pub re: f64,
    /// Imaginary part.
    pub im: f64,
}

impl C {
    /// `re + i im`.
    #[must_use]
    pub const fn new(re: f64, im: f64) -> Self {
        Self { re, im }
    }

    const fn mul(self, o: Self) -> Self {
        Self::new(self.re * o.re - self.im * o.im, self.re * o.im + self.im * o.re)
    }

    /// `|z|^2`.
    #[must_use]
    pub fn norm2(self) -> f64 {
        self.re * self.re + self.im * self.im
    }
}

/// The twiddle factors of an `n`-point transform: `exp(-2 pi i k / n)`, k < n / 2.
fn twiddles(n: usize) -> Vec<C> {
    (0..n / 2)
        .map(|k| {
            let a = -std::f64::consts::TAU * k as f64 / n as f64;
            C::new(a.cos(), a.sin())
        })
        .collect()
}

/// In-place FFT of one row (`a.len()` a power of two) with `tw` of its length; `inverse` conjugates the twiddles
/// (no 1 / n scaling).
fn fft_row(a: &mut [C], tw: &[C], inverse: bool) {
    let n = a.len();
    if n < 2 {
        return;
    }
    let bits = n.trailing_zeros();
    for i in 0..n {
        let j = i.reverse_bits() >> (usize::BITS - bits);
        if j > i {
            a.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let step = n / len;
        for start in (0..n).step_by(len) {
            for k in 0..len / 2 {
                let mut w = tw.get(k * step).copied().unwrap_or_default();
                if inverse {
                    w.im = -w.im;
                }
                let (i, j) = (start + k, start + k + len / 2);
                let (Some(&u), Some(&v)) = (a.get(i), a.get(j)) else {
                    continue;
                };
                let t = v.mul(w);
                if let Some(x) = a.get_mut(i) {
                    *x = C::new(u.re + t.re, u.im + t.im);
                }
                if let Some(x) = a.get_mut(j) {
                    *x = C::new(u.re - t.re, u.im - t.im);
                }
            }
        }
        len *= 2;
    }
}

/// The transpose of an `n` x `n` grid.
fn transpose(a: &[C], n: usize) -> Vec<C> {
    let mut out = vec![C::default(); n * n];
    out.par_chunks_mut(n).enumerate().for_each(|(i, row)| {
        for (j, x) in row.iter_mut().enumerate() {
            *x = a.get(j * n + i).copied().unwrap_or_default();
        }
    });
    out
}

/// The 2D FFT of an `n` x `n` grid (row-major; `n` a power of two). `inverse` scales by `1 / n^2`.
#[must_use]
pub fn fft2(a: &[C], n: usize, inverse: bool) -> Vec<C> {
    let tw = twiddles(n);
    let mut rows = a.to_vec();
    rows.par_chunks_mut(n).for_each(|r| fft_row(r, &tw, inverse));
    let mut cols = transpose(&rows, n);
    cols.par_chunks_mut(n).for_each(|r| fft_row(r, &tw, inverse));
    let mut out = transpose(&cols, n);
    if inverse {
        let s = 1.0 / (n * n) as f64;
        out.par_iter_mut().for_each(|x| {
            x.re *= s;
            x.im *= s;
        });
    }
    out
}

/// NumPy's `fftfreq(n, d)`: the frequency of each bin.
#[must_use]
pub fn fftfreq(n: usize, d: f64) -> Vec<f64> {
    (0..n)
        .map(|k| {
            let k = if k < n.div_ceil(2) {
                k as f64
            } else {
                k as f64 - n as f64
            };
            k / (n as f64 * d)
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_definition_and_inverts() {
        let n = 8;
        let a: Vec<C> = (0..n * n)
            .map(|k| C::new(f64::from(k as u32 % 7) - 3.0, f64::from(k as u32 % 3)))
            .collect();
        let f = fft2(&a, n, false);
        // one bin by the definition
        let (u, v) = (3usize, 5usize);
        let mut want = C::default();
        for y in 0..n {
            for x in 0..n {
                let ang = -std::f64::consts::TAU * ((u * y + v * x) as f64) / n as f64;
                let t = a[y * n + x].mul(C::new(ang.cos(), ang.sin()));
                want.re += t.re;
                want.im += t.im;
            }
        }
        let got = f[u * n + v];
        assert!((got.re - want.re).abs() < 1e-9 && (got.im - want.im).abs() < 1e-9);
        let back = fft2(&f, n, true);
        assert!(
            back.iter()
                .zip(&a)
                .all(|(x, y)| (x.re - y.re).abs() < 1e-9 && (x.im - y.im).abs() < 1e-9)
        );
        assert_eq!(fftfreq(4, 0.5), vec![0.0, 0.5, -1.0, -0.5]);
    }
}
