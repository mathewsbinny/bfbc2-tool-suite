//! FFmpeg's inverse MDCT as its C code computes it (`libavutil/tx_template.c`: `ff_tx_mdct_inv_float_c` over the
//! split-radix `ff_tx_fft{n}_ns_float_c` codelets, the transforms `av_tx_init(AV_TX_FLOAT_MDCT, inverse)` picks when
//! no SIMD is allowed), operation for operation in single precision, so its output is FFmpeg's to the bit. FFmpeg's
//! SIMD codelets add the same terms in other orders: they differ in the last bits of some samples.

#[derive(Clone, Copy, Default)]
struct C {
    re: f32,
    im: f32,
}

/// `BF(x, y, a, b)`: `x = a - b; y = a + b`.
#[inline]
fn bf(a: f32, b: f32) -> (f32, f32) {
    (a - b, a + b)
}

/// `CMUL(dre, dim, are, aim, bre, bim)`.
#[inline]
fn cmul(are: f32, aim: f32, bre: f32, bim: f32) -> (f32, f32) {
    (are * bre - aim * bim, are * bim + aim * bre)
}

/// The split-radix cosine tables (`ff_tx_tab_{n}`): `cos(i * 2 pi / n)` for `i < n / 4`, then 0.
fn sr_table(n: usize) -> Vec<f32> {
    let freq = 2.0 * std::f64::consts::PI / n as f64;
    let mut t: Vec<f32> = (0..n / 4).map(|i| (i as f64 * freq).cos() as f32).collect();
    t.push(0.0);
    t
}

/// `BUTTERFLIES(a0, a1, a2, a3)` with the temporaries t1, t2, t5, t6 as inputs.
#[inline]
#[allow(clippy::too_many_arguments, reason = "the macro's operands")]
fn butterflies(z: &mut [C], a0: usize, a1: usize, a2: usize, a3: usize, t1: f32, t2: f32, t5: f32, t6: f32) {
    let (r0, i0, r1, i1) = (z[a0].re, z[a0].im, z[a1].re, z[a1].im);
    let (t3, t5) = bf(t5, t1);
    (z[a2].re, z[a0].re) = bf(r0, t5);
    (z[a3].im, z[a1].im) = bf(i1, t3);
    let (t4, t6) = bf(t2, t6);
    (z[a3].re, z[a1].re) = bf(r1, t4);
    (z[a2].im, z[a0].im) = bf(i0, t6);
}

/// `TRANSFORM(a0, a1, a2, a3, wre, wim)`.
#[inline]
fn transform(z: &mut [C], a0: usize, a1: usize, a2: usize, a3: usize, wre: f32, wim: f32) {
    let (t1, t2) = cmul(z[a2].re, z[a2].im, wre, -wim);
    let (t5, t6) = cmul(z[a3].re, z[a3].im, wre, wim);
    butterflies(z, a0, a1, a2, a3, t1, t2, t5, t6);
}

/// The split-radix FFT codelets for one length, in place (`ff_tx_fft{n}_ns`: the input already permuted).
struct Fft {
    /// `tabs[k]`: the table of length `2^k` (k >= 3).
    tabs: Vec<Vec<f32>>,
}

impl Fft {
    fn new(max: usize) -> Self {
        let mut tabs = vec![Vec::new(); 3];
        let mut n = 8;
        while n <= max {
            tabs.push(sr_table(n));
            n *= 2;
        }
        Fft { tabs }
    }

    fn tab(&self, n: usize) -> &[f32] {
        &self.tabs[n.trailing_zeros() as usize]
    }

    fn fft2(z: &mut [C]) {
        let (tre, d0re) = bf(z[0].re, z[1].re);
        let (tim, d0im) = bf(z[0].im, z[1].im);
        z[0] = C { re: d0re, im: d0im };
        z[1] = C { re: tre, im: tim };
    }

    fn fft4(z: &mut [C]) {
        let s = [z[0], z[1], z[2], z[3]];
        let (t3, t1) = bf(s[0].re, s[1].re);
        let (t8, t6) = bf(s[3].re, s[2].re);
        (z[2].re, z[0].re) = bf(t1, t6);
        let (t4, t2) = bf(s[0].im, s[1].im);
        let (t7, t5) = bf(s[2].im, s[3].im);
        (z[3].im, z[1].im) = bf(t4, t8);
        (z[3].re, z[1].re) = bf(t3, t7);
        (z[2].im, z[0].im) = bf(t2, t5);
    }

    fn fft8(&self, z: &mut [C]) {
        let cos = self.tab(8)[1];
        let s = [z[4], z[5], z[6], z[7]];
        Self::fft4(z);
        let (t1, d5re) = bf(s[0].re, -s[1].re);
        let (t2, d5im) = bf(s[0].im, -s[1].im);
        let (t5, d7re) = bf(s[2].re, -s[3].re);
        let (t6, d7im) = bf(s[2].im, -s[3].im);
        z[5] = C { re: d5re, im: d5im };
        z[7] = C { re: d7re, im: d7im };
        butterflies(z, 0, 2, 4, 6, t1, t2, t5, t6);
        transform(z, 1, 3, 5, 7, cos, cos);
    }

    fn fft16(&self, z: &mut [C]) {
        let cos = self.tab(16);
        let (c1, c2, c3) = (cos[1], cos[2], cos[3]);
        self.fft8(&mut z[0..8]);
        Self::fft4(&mut z[8..12]);
        Self::fft4(&mut z[12..16]);
        let (t1, t2, t5, t6) = (z[8].re, z[8].im, z[12].re, z[12].im);
        butterflies(z, 0, 4, 8, 12, t1, t2, t5, t6);
        transform(z, 2, 6, 10, 14, c2, c2);
        transform(z, 1, 5, 9, 13, c1, c3);
        transform(z, 3, 7, 11, 15, c3, c1);
    }

    /// `ff_tx_fft_sr_combine(z, cos, len)`.
    fn combine(z: &mut [C], cos: &[f32], len: usize) {
        let (o1, o2, o3) = (2 * len, 4 * len, 6 * len);
        let mut base = 0;
        let mut c = 0;
        // wim = cos + o1 - 7, moving back 8 a step
        let mut w = o1 as isize - 7;
        let mut i = 0;
        while i < len {
            let wim = |k: isize| cos[(w + k) as usize];
            for (k, wk) in [(0, 7), (2, 5), (4, 3), (6, 1), (1, 6), (3, 4), (5, 2), (7, 0)] {
                let (wre, wi) = (cos[c + k], wim(wk));
                transform(z, base + k, base + o1 + k, base + o2 + k, base + o3 + k, wre, wi);
            }
            base += 8;
            c += 8;
            w -= 8;
            i += 4;
        }
    }

    /// `ff_tx_fft{n}_ns` in place.
    fn run(&self, z: &mut [C], n: usize) {
        match n {
            2 => Self::fft2(z),
            4 => Self::fft4(z),
            8 => self.fft8(z),
            16 => self.fft16(z),
            _ => {
                let (n2, n4) = (n / 2, n / 4);
                self.run(&mut z[..n2], n2);
                self.run(&mut z[n4 * 2..n4 * 3], n4);
                self.run(&mut z[n4 * 3..n4 * 4], n4);
                Self::combine(z, self.tab(n), n4 >> 1);
            }
        }
    }
}

/// `split_radix_permutation(i, len, inv)` (libavutil/tx.c).
fn split_radix_permutation(i: i32, len: i32, inv: i32) -> i32 {
    let len = len >> 1;
    if len <= 1 {
        return i & 1;
    }
    if i & len == 0 {
        return split_radix_permutation(i, len, inv) * 2;
    }
    let len = len >> 1;
    split_radix_permutation(i, len, inv) * 4 + 1 - 2 * (i32::from(i & len == 0) ^ inv)
}

/// An inverse MDCT of `len` coefficients giving `len` samples (FFmpeg's half IMDCT), with its scale.
pub(crate) struct Imdct {
    len: usize,
    /// The sub-FFT's gather map, doubled (`s->map[i] <<= 1`).
    map: Vec<usize>,
    /// `s->exp`: the pre-rotation (permuted) then the post-rotation table, each `len / 2` long.
    exp: Vec<C>,
    fft: Fft,
    z: Vec<C>,
}

impl Imdct {
    /// `av_tx_init(AV_TX_FLOAT_MDCT, inv = 1, len, &scale)` (scale a float, as FFmpeg's float transforms read it).
    pub(crate) fn new(len: usize, scale: f32) -> Self {
        let n = len >> 1; // the FFT's length
        let map0: Vec<usize> = (0..n)
            .map(|i| (-split_radix_permutation(i as i32, n as i32, 1) & (n as i32 - 1)) as usize)
            .collect();
        // ff_tx_mdct_gen_exp(s, map): theta 1/8 for a positive scale, the scale's square root
        let scale_d = f64::from(scale);
        let theta = (if scale_d < 0.0 { n as f64 } else { 0.0 }) + 1.0 / 8.0;
        let sc = scale_d.abs().sqrt();
        let mut exp = vec![C::default(); 2 * n];
        for i in 0..n {
            let alpha = std::f64::consts::FRAC_PI_2 * (i as f64 + theta) / n as f64;
            exp[n + i] = C {
                re: (alpha.cos() * sc) as f32,
                im: (alpha.sin() * sc) as f32,
            };
        }
        for i in 0..n {
            exp[i] = exp[n + map0[i]];
        }
        Imdct {
            len,
            map: map0.iter().map(|&m| m << 1).collect(),
            exp,
            fft: Fft::new(n.max(8)),
            z: vec![C::default(); n],
        }
    }

    /// `ff_tx_mdct_inv(dst, src)`: `len` samples into `dst` from `len` coefficients in `src`.
    pub(crate) fn run(&mut self, dst: &mut [f32], src: &[f32]) {
        let len2 = self.len >> 1;
        let len4 = self.len >> 2;
        let z = &mut self.z;
        for i in 0..len2 {
            let k = self.map[i];
            let (are, aim) = (src[2 * len2 - 1 - k], src[k]);
            let e = self.exp[i];
            let (re, im) = cmul(are, aim, e.re, e.im);
            z[i] = C { re, im };
        }
        self.fft.run(z, len2);
        let exp = &self.exp[len2..];
        for i in 0..len4 {
            let (i0, i1) = (len4 + i, len4 - i - 1);
            let src1 = C {
                re: z[i1].im,
                im: z[i1].re,
            };
            let src0 = C {
                re: z[i0].im,
                im: z[i0].re,
            };
            let (a, b) = cmul(src1.re, src1.im, exp[i1].im, exp[i1].re);
            z[i1].re = a;
            z[i0].im = b;
            let (a, b) = cmul(src0.re, src0.im, exp[i0].im, exp[i0].re);
            z[i0].re = a;
            z[i1].im = b;
        }
        for (i, c) in z.iter().enumerate() {
            dst[2 * i] = c.re;
            dst[2 * i + 1] = c.im;
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// The half IMDCT against its definition in double precision:
    /// `y[n] = scale * sum_k x[k] cos(pi / N * (n + 1/2 + N/2) * (k + 1/2))` over the middle half of the full output.
    #[test]
    fn imdct_matches_the_definition() {
        for bits in 6..=9 {
            let len = 1usize << bits;
            let scale = 1.0f32 / (len as f32);
            let mut t = Imdct::new(len, scale);
            let mut seed = 12345u32;
            let src: Vec<f32> = (0..len)
                .map(|_| {
                    seed = seed.wrapping_mul(1_103_515_245).wrapping_add(12345);
                    ((seed >> 8) as f32 / (1u32 << 24) as f32) - 0.5
                })
                .collect();
            let mut dst = vec![0f32; len];
            t.run(&mut dst, &src);
            let n2 = 2 * len;
            for (n, &got) in dst.iter().enumerate() {
                // the half IMDCT gives the full output's samples len/2 .. 3len/2
                let m = n + len / 2;
                let mut acc = 0f64;
                for (k, &x) in src.iter().enumerate() {
                    let a = std::f64::consts::PI / len as f64 * (m as f64 + 0.5 + len as f64 / 2.0) * (k as f64 + 0.5);
                    acc += f64::from(x) * a.cos();
                }
                let want = -acc * f64::from(scale);
                assert!(
                    (f64::from(got) - want).abs() < 1e-4,
                    "len {len} n {n}: {got} vs {want} ({n2})"
                );
            }
        }
    }
}
