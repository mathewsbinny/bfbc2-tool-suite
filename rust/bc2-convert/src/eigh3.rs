//! The eigenvectors of a symmetric 3 x 3 matrix as `numpy.linalg.eigh` gives them on the machine that recorded the
//! WW2 oracle (round OR14): LAPACK `dsyevd` (`JOBZ = 'V'`, `UPLO = 'L'`) of OpenBLAS 0.3.34 (`scipy-openblas`,
//! `DYNAMIC_ARCH`, the SkylakeX kernels), written out step for step so that every value is the reference's to the
//! bit, on any processor. [`texwrite`](crate::texwrite) takes each 4 x 4 block's principal axis from it.
//!
//! The path for n = 3 (LAPACK 3.12 as OpenBLAS ships it, compiled for baseline x86-64: each operation rounded on
//! its own, in source order):
//!
//! - `dlansy('M')` and the scaling into `[sqrt(sfmin / eps), sqrt(1 / (sfmin / eps))]` (`dlascl`);
//! - `dsytrd` -> `dsytd2`: one Householder reflector (`dlarfg` on `A(2:3, 1)`: `dnrm2` of one value is its
//!   magnitude, `dlapy2`, `dscal`), then `dsymv`, `ddot`, `daxpy`, `dsyr2` on `A(2:3, 2:3)`;
//! - `dstedc('I')` -> `dsteqr('I')` (n is under `SMLSIZ` 25): the implicit QL / QR sweeps (`dlaev2`, the LAPACK
//!   3.10 `dlartg`, `dlasr`, `dlanst`, `dlascl`), the eigenvalues sorted ascending with their vectors;
//! - `dormtr` -> `dormqr` -> `dorm2r` -> `dlarf1f`: the reflector applied to rows 2 and 3 of the vectors (`dgemv`
//!   'T', `daxpy`, `dger`).
//!
//! The BLAS kernels are OpenBLAS's SkylakeX builds (`-march=skylake-avx512`, so the compiler fuses each
//! `y += a * b` into one multiply-add). Measured against NumPy (`rust/parity_texwrite_axes.py`): with every kernel
//! statement fused and every LAPACK statement unfused the axes match bit for bit; leaving any one kernel unfused
//! does not. The kernels' statements for these sizes (n = 2 or 1, unit or strided steps, under the unrolled widths):
//!
//! | BLAS call | kernel statement (OpenBLAS `kernel/x86_64`) | here |
//! |---|---|---|
//! | `dsymv('L', 2)` | `dsymv_L.c` tail: `y[j] += temp1 * a`, `temp2 += a * x`, `y[j] += alpha * temp2` | `mul_add` |
//! | `ddot(2)` | `ddot.c` tail: `dot += y[i] * x[i]` | `mul_add` |
//! | `daxpy` (also `dsyr2`'s and `dger`'s columns) | `daxpy.c`: `y[i] += da * x[i]` | `mul_add` |
//! | `dgemv('T', 1, n)` | `dgemv_t_4.c`, `m3 == 1`: `y[j] += a * (x * alpha)` | `mul_add` |
//! | `dscal` | `dscal.c`: `x[i] *= da` | `*` |
//!
//! `dger`'s factor `alpha * y[j]` (`kernel/generic/ger.c`) is rounded before its `daxpy`; `dsyr2` (the interface's
//! own loop for n < 100) rounds `alpha * x[i]` likewise (exact here: alpha is -1).

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::float_cmp,
    clippy::suboptimal_flops,
    clippy::too_many_lines,
    clippy::needless_range_loop,
    clippy::manual_midpoint,
    clippy::if_not_else,
    clippy::doc_markdown,
    clippy::unreadable_literal,
    reason = "LAPACK's names, 1-based indices into fixed arrays sized for them, its exact compares and branch order, its products (HALF * (SM + RT) is not a midpoint's rounding), each multiply-add kept separate or fused exactly as the reference machine computes it, and NumPy's numbers as it prints them"
)]

/// `dlamch('S')`: the safe minimum.
const SAFMIN: f64 = f64::MIN_POSITIVE;
/// `dlamch('E')`: the relative machine precision (rounding), 2^-53.
const EPS_E: f64 = f64::EPSILON / 2.0;
/// `dlamch('P')`: precision times the base, 2^-52.
const EPS_P: f64 = f64::EPSILON;
/// `dlamch('O')`: the overflow threshold.
const HUGE: f64 = f64::MAX;

/// Fortran `SIGN(a, b)`: `|a|` with `b`'s sign.
fn sign(a: f64, b: f64) -> f64 {
    a.abs().copysign(b)
}

/// `dlapy2`: `sqrt(x^2 + y^2)` without needless overflow (a NaN given back, `y`'s first).
fn dlapy2(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        return y;
    }
    if x.is_nan() {
        return x;
    }
    let (xa, ya) = (x.abs(), y.abs());
    let (w, z) = (xa.max(ya), xa.min(ya));
    if z == 0.0 || w > HUGE {
        return w;
    }
    let q = z / w;
    w * (1.0 + q * q).sqrt()
}

/// `dlascl`'s multipliers for `cto / cfrom`, applied one after the other (each a plain product).
fn dlascl_muls(cfrom: f64, cto: f64) -> Vec<f64> {
    let smlnum = SAFMIN;
    let bignum = 1.0 / smlnum;
    let (mut cfromc, mut ctoc) = (cfrom, cto);
    let mut muls = Vec::new();
    loop {
        let cfrom1 = cfromc * smlnum;
        if cfrom1 == cfromc {
            muls.push(ctoc / cfromc);
            return muls;
        }
        let cto1 = ctoc / bignum;
        if cto1 == ctoc {
            muls.push(ctoc);
            return muls;
        }
        if cfrom1.abs() > ctoc.abs() && ctoc != 0.0 {
            muls.push(smlnum);
            cfromc = cfrom1;
        } else if cto1.abs() > cfromc.abs() {
            muls.push(bignum);
            ctoc = cto1;
        } else {
            let mul = ctoc / cfromc;
            if mul != 1.0 {
                muls.push(mul);
            }
            return muls;
        }
    }
}

/// `dlaev2(a, b, c)`: the eigen decomposition of `[[a, b], [b, c]]`: `(rt1, rt2, cs1, sn1)`.
fn dlaev2(a: f64, b: f64, c: f64) -> (f64, f64, f64, f64) {
    let sm = a + c;
    let df = a - c;
    let adf = df.abs();
    let tb = b + b;
    let ab = tb.abs();
    let (acmx, acmn) = if a.abs() > c.abs() { (a, c) } else { (c, a) };
    let rt = if adf > ab {
        let q = ab / adf;
        adf * (1.0 + q * q).sqrt()
    } else if adf < ab {
        let q = adf / ab;
        ab * (1.0 + q * q).sqrt()
    } else {
        ab * 2f64.sqrt()
    };
    let (rt1, rt2, sgn1) = if sm < 0.0 {
        let rt1 = 0.5 * (sm - rt);
        (rt1, (acmx / rt1) * acmn - (b / rt1) * b, -1)
    } else if sm > 0.0 {
        let rt1 = 0.5 * (sm + rt);
        (rt1, (acmx / rt1) * acmn - (b / rt1) * b, 1)
    } else {
        (0.5 * rt, -0.5 * rt, 1)
    };
    let (cs, sgn2) = if df >= 0.0 { (df + rt, 1) } else { (df - rt, -1) };
    let (mut cs1, mut sn1);
    if cs.abs() > ab {
        let ct = -tb / cs;
        sn1 = 1.0 / (1.0 + ct * ct).sqrt();
        cs1 = ct * sn1;
    } else if ab == 0.0 {
        cs1 = 1.0;
        sn1 = 0.0;
    } else {
        let tn = -cs / tb;
        cs1 = 1.0 / (1.0 + tn * tn).sqrt();
        sn1 = tn * cs1;
    }
    if sgn1 == sgn2 {
        let tn = cs1;
        cs1 = -sn1;
        sn1 = tn;
    }
    (rt1, rt2, cs1, sn1)
}

/// `dlartg(f, g)` (LAPACK 3.10's, `la_constants`' safe minimum and maximum): `(c, s, r)`.
fn dlartg(f: f64, g: f64) -> (f64, f64, f64) {
    let safmax = 1.0 / SAFMIN;
    let rtmin = SAFMIN.sqrt();
    let rtmax = (safmax / 2.0).sqrt();
    let (f1, g1) = (f.abs(), g.abs());
    if g == 0.0 {
        return (1.0, 0.0, f);
    }
    if f == 0.0 {
        return (0.0, sign(1.0, g), g1);
    }
    if f1 > rtmin && f1 < rtmax && g1 > rtmin && g1 < rtmax {
        let d = (f * f + g * g).sqrt();
        let r = sign(d, f);
        return (f1 / d, g / r, r);
    }
    let u = safmax.min(SAFMIN.max(f1).max(g1));
    let (fs, gs) = (f / u, g / u);
    let d = (fs * fs + gs * gs).sqrt();
    let r = sign(d, f);
    (fs.abs() / d, gs / r, r * u)
}

/// 1-based 3 x 3 storage: `z[i][j]` is Fortran's `Z(i, j)`.
type M4 = [[f64; 4]; 4];

/// `dlasr('R', 'V', 'B' or 'F', 3, n, c, s, Z(1, col0))`: `c` and `s` 1-based (`n - 1` rotations).
fn dlasr(backward: bool, n: usize, c: &[f64; 4], s: &[f64; 4], z: &mut M4, col0: usize) {
    let mut rot = |j: usize| {
        let (ct, st) = (c[j], s[j]);
        if ct != 1.0 || st != 0.0 {
            for i in 1..=3 {
                let temp = z[i][col0 + j];
                z[i][col0 + j] = ct * temp - st * z[i][col0 + j - 1];
                z[i][col0 + j - 1] = st * temp + ct * z[i][col0 + j - 1];
            }
        }
    };
    if backward {
        for j in (1..n).rev() {
            rot(j);
        }
    } else {
        for j in 1..n {
            rot(j);
        }
    }
}

/// Scales `d(lo..=hi)` and `e(lo..hi)` by `dlascl`'s multipliers for `cto / cfrom`.
fn scale_de(d: &mut [f64; 4], e: &mut [f64; 3], lo: usize, hi: usize, cfrom: f64, cto: f64) {
    for mul in dlascl_muls(cfrom, cto) {
        for v in &mut d[lo..=hi] {
            *v *= mul;
        }
        for v in &mut e[lo..hi] {
            *v *= mul;
        }
    }
}

/// `dsteqr('I', 3, d, e, z)`: `d(1..=3)`, `e(1..=2)` 1-based; `z` set to the identity, then rotated. The
/// eigenvalues come back ascending in `d`, their vectors in `z`'s columns; false: no convergence (LAPACK's INFO > 0,
/// left unsorted).
fn dsteqr(d: &mut [f64; 4], e: &mut [f64; 3], z: &mut M4) -> bool {
    const N: usize = 3;
    let eps = EPS_E;
    let eps2 = eps * eps;
    let safmin = SAFMIN;
    let safmax = 1.0 / safmin;
    let ssfmax = safmax.sqrt() / 3.0;
    let ssfmin = safmin.sqrt() / eps2;
    *z = [[0.0; 4]; 4];
    for i in 1..=N {
        z[i][i] = 1.0;
    }
    // work(1..=n-1): the cosines, work(n..=2n-2): the sines
    let mut wc = [0f64; 4];
    let mut ws = [0f64; 4];
    let nmaxit = N * 30;
    let mut jtot = 0;
    let mut l1 = 1;
    'outer: loop {
        // label 10
        if l1 > N {
            break;
        }
        if l1 > 1 {
            e[l1 - 1] = 0.0;
        }
        let mut m = N;
        for mm in l1..N {
            let tst = e[mm].abs();
            if tst == 0.0 {
                m = mm;
                break;
            }
            if tst <= (d[mm].abs().sqrt() * d[mm + 1].abs().sqrt()) * eps {
                e[mm] = 0.0;
                m = mm;
                break;
            }
        }
        let mut l = l1;
        let lsv = l;
        let mut lend = m;
        let lendsv = lend;
        l1 = m + 1;
        if lend == l {
            continue;
        }
        // dlanst('M')
        let mut anorm = d[lend].abs();
        for i in l..lend {
            for v in [d[i].abs(), e[i].abs()] {
                if anorm < v || v.is_nan() {
                    anorm = v;
                }
            }
        }
        let mut iscale = 0;
        if anorm == 0.0 {
            continue;
        }
        if anorm > ssfmax {
            iscale = 1;
            scale_de(d, e, l, lend, anorm, ssfmax);
        } else if anorm < ssfmin {
            iscale = 2;
            scale_de(d, e, l, lend, anorm, ssfmin);
        }
        if d[lend].abs() < d[l].abs() {
            lend = lsv;
            l = lendsv;
        }
        if lend > l {
            // QL iteration (label 40)
            loop {
                let mut m = lend;
                for mm in l..lend {
                    let tst = e[mm].abs().powi(2);
                    if tst <= (eps2 * d[mm].abs()) * d[mm + 1].abs() + safmin {
                        m = mm;
                        break;
                    }
                }
                if m < lend {
                    e[m] = 0.0;
                }
                let mut p = d[l];
                if m == l {
                    // label 80
                    d[l] = p;
                    l += 1;
                    if l <= lend {
                        continue;
                    }
                    break;
                }
                if m == l + 1 {
                    let (rt1, rt2, c, s) = dlaev2(d[l], e[l], d[l + 1]);
                    dlasr(true, 2, &[0.0, c, 0.0, 0.0], &[0.0, s, 0.0, 0.0], z, l);
                    d[l] = rt1;
                    d[l + 1] = rt2;
                    e[l] = 0.0;
                    l += 2;
                    if l <= lend {
                        continue;
                    }
                    break;
                }
                if jtot == nmaxit {
                    break;
                }
                jtot += 1;
                let mut g = (d[l + 1] - p) / (2.0 * e[l]);
                let mut r = dlapy2(g, 1.0);
                g = (d[m] - p) + (e[l] / (g + sign(r, g)));
                let (mut s, mut c) = (1.0, 1.0);
                p = 0.0;
                for i in (l..m).rev() {
                    let f = s * e[i];
                    let b = c * e[i];
                    (c, s, r) = dlartg(g, f);
                    if i != m - 1 {
                        e[i + 1] = r;
                    }
                    g = d[i + 1] - p;
                    r = (d[i] - g) * s + (2.0 * c) * b;
                    p = s * r;
                    d[i + 1] = g + p;
                    g = c * r - b;
                    wc[i] = c;
                    ws[i] = -s;
                }
                let mm = m - l + 1;
                let (mut cc, mut ss) = ([0f64; 4], [0f64; 4]);
                for j in 1..mm {
                    cc[j] = wc[l + j - 1];
                    ss[j] = ws[l + j - 1];
                }
                dlasr(true, mm, &cc, &ss, z, l);
                d[l] -= p;
                e[l] = g;
            }
        } else {
            // QR iteration (label 90)
            loop {
                let mut m = lend;
                for mm in (lend + 1..=l).rev() {
                    let tst = e[mm - 1].abs().powi(2);
                    if tst <= (eps2 * d[mm].abs()) * d[mm - 1].abs() + safmin {
                        m = mm;
                        break;
                    }
                }
                if m > lend {
                    e[m - 1] = 0.0;
                }
                let mut p = d[l];
                if m == l {
                    // label 130
                    d[l] = p;
                    if l == lend {
                        break;
                    }
                    l -= 1;
                    if l >= lend {
                        continue;
                    }
                    break;
                }
                if m + 1 == l {
                    let (rt1, rt2, c, s) = dlaev2(d[l - 1], e[l - 1], d[l]);
                    dlasr(false, 2, &[0.0, c, 0.0, 0.0], &[0.0, s, 0.0, 0.0], z, l - 1);
                    d[l - 1] = rt1;
                    d[l] = rt2;
                    e[l - 1] = 0.0;
                    if l < lend + 2 {
                        break;
                    }
                    l -= 2;
                    continue;
                }
                if jtot == nmaxit {
                    break;
                }
                jtot += 1;
                let mut g = (d[l - 1] - p) / (2.0 * e[l - 1]);
                let mut r = dlapy2(g, 1.0);
                g = (d[m] - p) + (e[l - 1] / (g + sign(r, g)));
                let (mut s, mut c) = (1.0, 1.0);
                p = 0.0;
                for i in m..l {
                    let f = s * e[i];
                    let b = c * e[i];
                    (c, s, r) = dlartg(g, f);
                    if i != m {
                        e[i - 1] = r;
                    }
                    g = d[i] - p;
                    r = (d[i + 1] - g) * s + (2.0 * c) * b;
                    p = s * r;
                    d[i] = g + p;
                    g = c * r - b;
                    wc[i] = c;
                    ws[i] = s;
                }
                let mm = l - m + 1;
                let (mut cc, mut ss) = ([0f64; 4], [0f64; 4]);
                for j in 1..mm {
                    cc[j] = wc[m + j - 1];
                    ss[j] = ws[m + j - 1];
                }
                dlasr(false, mm, &cc, &ss, z, m);
                d[l] -= p;
                e[l - 1] = g;
            }
        }
        // label 140
        if iscale == 1 {
            scale_de(d, e, lsv, lendsv, ssfmax, anorm);
        } else if iscale == 2 {
            scale_de(d, e, lsv, lendsv, ssfmin, anorm);
        }
        if jtot < nmaxit {
            continue 'outer;
        }
        // not converged: LAPACK returns unsorted (INFO > 0; NumPy then fills NaN)
        return false;
    }
    // label 160: selection sort, ascending
    for ii in 2..=N {
        let i = ii - 1;
        let mut k = i;
        let mut p = d[i];
        for j in ii..=N {
            if d[j] < p {
                k = j;
                p = d[j];
            }
        }
        if k != i {
            d[k] = d[i];
            d[i] = p;
            for row in z.iter_mut().skip(1) {
                row.swap(i, k);
            }
        }
    }
    true
}

/// `numpy.linalg.eigh(m)` of a symmetric 3 x 3 matrix (row-major; only the lower triangle is read, as `UPLO='L'`):
/// the eigenvalues ascending and the eigenvectors, `v[r][c]` the `r`-th component of the `c`-th vector (NumPy's
/// layout). `None` where LAPACK reports no convergence (NumPy gives NaN there; never for finite matrices met so
/// far).
#[must_use]
pub fn eigh(m: &[f64; 9]) -> Option<([f64; 3], [[f64; 3]; 3])> {
    let mut a: M4 = [[0.0; 4]; 4];
    for i in 0..3 {
        for j in 0..=i {
            a[i + 1][j + 1] = m[i * 3 + j];
        }
    }
    // dlansy('M', 'L') and the scaling
    let mut anrm = 0f64;
    for j in 1..=3 {
        for i in j..=3 {
            let v = a[i][j].abs();
            if anrm < v || v.is_nan() {
                anrm = v;
            }
        }
    }
    let smlnum = SAFMIN / EPS_P;
    let bignum = 1.0 / smlnum;
    let (rmin, rmax) = (smlnum.sqrt(), bignum.sqrt());
    let sigma = if anrm > 0.0 && anrm < rmin {
        Some(rmin / anrm)
    } else if anrm > rmax {
        Some(rmax / anrm)
    } else {
        None
    };
    if let Some(sg) = sigma {
        for mul in dlascl_muls(1.0, sg) {
            for j in 1..=3 {
                for i in j..=3 {
                    a[i][j] *= mul;
                }
            }
        }
    }
    // dsytd2('L'): i = 1 (dlarfg on A(2:3, 1)); i = 2's dlarfg of one value gives tau 0
    let (mut alpha, mut x) = (a[2][1], a[3][1]);
    let mut tau = 0.0;
    if x != 0.0 {
        // dnrm2 of one value: its magnitude
        let mut beta = -sign(dlapy2(alpha, x.abs()), alpha);
        let sfmin = SAFMIN / EPS_E;
        let mut knt = 0;
        if beta.abs() < sfmin {
            let rsafmn = 1.0 / sfmin;
            loop {
                knt += 1;
                x *= rsafmn;
                beta *= rsafmn;
                alpha *= rsafmn;
                if !(beta.abs() < sfmin && knt < 20) {
                    break;
                }
            }
            beta = -sign(dlapy2(alpha, x.abs()), alpha);
        }
        tau = (beta - alpha) / beta;
        x *= 1.0 / (alpha - beta);
        for _ in 0..knt {
            beta *= sfmin;
        }
        alpha = beta;
    }
    let (e1, v2) = (alpha, x);
    if tau != 0.0 {
        let (mut a00, mut a10, mut a11) = (a[2][2], a[3][2], a[3][3]);
        // dsymv('L', 2, tau, A(2:3, 2:3), v = (1, v2), 0, w): dsymv_L.c with x[0] = 1
        let mut y0 = tau.mul_add(a00, 0.0);
        let mut y1 = tau.mul_add(a10, 0.0);
        let t2 = a10.mul_add(v2, 0.0);
        y0 = tau.mul_add(t2, y0);
        let t1 = tau * v2;
        y1 = t1.mul_add(a11, y1);
        y1 = tau.mul_add(0.0, y1);
        // ddot(2, w, v): dot += v[i] * w[i]
        let mut dot = 1f64.mul_add(y0, 0.0);
        dot = v2.mul_add(y1, dot);
        let alph = -((0.5 * tau) * dot);
        // daxpy(2, alph, v, w)
        y0 = alph.mul_add(1.0, y0);
        y1 = alph.mul_add(v2, y1);
        // dsyr2('L', 2, -1, v, w, A(2:3, 2:3)): per column, daxpy(-v[i], w) then daxpy(-w[i], v)
        a00 = (-1f64).mul_add(y0, a00);
        a10 = (-1f64).mul_add(y1, a10);
        a00 = (-y0).mul_add(1.0, a00);
        a10 = (-y0).mul_add(v2, a10);
        a11 = (-v2).mul_add(y1, a11);
        a11 = (-y1).mul_add(v2, a11);
        (a[2][2], a[3][2], a[3][3]) = (a00, a10, a11);
    }
    let mut d = [0.0, a[1][1], a[2][2], a[3][3]];
    let mut e = [0.0, e1, a[3][2]];
    let mut z: M4 = [[0.0; 4]; 4];
    if !dsteqr(&mut d, &mut e, &mut z) {
        return None;
    }
    // dormtr('L', 'L', 'N') -> dorm2r on C = Z(2:3, 1:3): reflector 2 has tau 0; reflector 1 by dlarf1f
    if tau != 0.0 {
        let lastv = if v2 != 0.0 { 2 } else { 1 };
        // iladlc(lastv, 3, C): the last column with a nonzero in C(1..=lastv, :)
        let lastc = if z[2][3] != 0.0 || z[1 + lastv][3] != 0.0 {
            3
        } else {
            (1..=3)
                .rev()
                .find(|&j| (1..=lastv).any(|r| z[1 + r][j] != 0.0))
                .unwrap_or(0)
        };
        if lastv == 1 {
            let f = 1.0 - tau;
            for j in 1..=lastc {
                z[2][j] *= f;
            }
        } else {
            for j in 1..=lastc {
                // dgemv('T', 1, lastc, 1, C(2, :), v2, 0, work), then daxpy(lastc, 1, C(1, :), work)
                let mut w = z[3][j].mul_add(v2, 0.0);
                w = 1f64.mul_add(z[2][j], w);
                // daxpy(lastc, -tau, work, C(1, :))
                z[2][j] = (-tau).mul_add(w, z[2][j]);
                // dger(1, lastc, -tau, v2, work, C(2, :)): (-tau * work[j]) rounded, then daxpy
                let y = -tau * w;
                z[3][j] = y.mul_add(v2, z[3][j]);
            }
        }
    }
    // dscal(3, 1 / sigma, w)
    let mut w = [d[1], d[2], d[3]];
    if let Some(sg) = sigma {
        let r = 1.0 / sg;
        for v in &mut w {
            *v *= r;
        }
    }
    let v = std::array::from_fn(|r| std::array::from_fn(|c| z[r + 1][c + 1]));
    Some((w, v))
}

/// The eigenvector of the largest eigenvalue, `numpy.linalg.eigh(m)[1][:, -1]` to the bit (row-major input, lower
/// triangle read); NaNs where NumPy gives them (no convergence).
#[must_use]
pub fn principal_axis(m: &[f64; 9]) -> [f64; 3] {
    eigh(m).map_or([f64::NAN; 3], |(_, v)| [v[0][2], v[1][2], v[2][2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `numpy.linalg.eigh(cov)[1][:, -1]`'s bit patterns, recorded with NumPy 2.5.4 / scipy-openblas 0.3.34 on the
    /// WW2 oracle's machine (SkylakeX kernels): block covariances of texwrite's kinds (noise, a small spread, two
    /// colours, a line, transparent texels filled with the opaque mean, channels on and off), then the zero matrix,
    /// diagonals, a rotated pair, a repeated eigenvalue and the two scalings (tiny, huge).
    const RECORDED: [([f64; 9], [u64; 3]); 12] = [
        (
            [
                72708.4375, 4432.3125, -3669.4375, 4432.3125, 50241.9375, 9077.6875, -3669.4375, 9077.6875, 90610.4375,
            ],
            [0x3fc1_2d76_e131_a209, 0xbfc8_ae5e_2a40_059b, 0xbfef_1abc_23d9_d870],
        ),
        (
            [65.9375, 4.75, -7.9375, 4.75, 55.0, -9.75, -7.9375, -9.75, 45.9375],
            [0xbfe9_af54_2961_42d9, 0xbfdc_3ad0_50d7_bb79, 0x3fd9_b1e7_867c_a6cd],
        ),
        (
            [
                13053.75, 4867.5, 14381.25, 4867.5, 1815.0, 5362.5, 14381.25, 5362.5, 15843.75,
            ],
            [0x3fe4_dcb8_eee0_6e20, 0x3fcf_1dd2_bf68_c6f1, 0x3fe6_fbd8_c1be_d8c6],
        ),
        (
            [
                280.0, -1204.0, -1666.0, -1204.0, 5188.0, 7183.0, -1666.0, 7183.0, 9948.0,
            ],
            [0xbfc1_3a0c_11da_d48b, 0x3fe2_9053_e11e_af1b, 0x3fe9_b4ef_84c5_e1bd],
        ),
        (
            [
                39361.33333333333,
                2047.9999999999998,
                2203.333333333334,
                2047.9999999999998,
                33045.5,
                7077.0,
                2203.333333333334,
                7077.0,
                34115.33333333333,
            ],
            [0xbfe4_106d_9b29_613a, 0xbfe0_f3f7_c2f6_0714, 0xbfe2_46be_914f_cc55],
        ),
        (
            [
                137500.0, 22500.0, -37500.0, 22500.0, 137500.0, -2500.0, -37500.0, -2500.0, 97500.0,
            ],
            [0xbfe8_464c_5e21_a54f, 0xbfe0_89be_a399_26aa, 0x3fd9_6550_07f3_a95f],
        ),
        ([0.0; 9], [0, 0, 0x3ff0_0000_0000_0000]),
        (
            [1.0, 0.0, 0.0, 0.0, 5.0, 0.0, 0.0, 0.0, 2.0],
            [0, 0x3ff0_0000_0000_0000, 0],
        ),
        (
            [2.0, 1.0, 0.0, 1.0, 2.0, 0.0, 0.0, 0.0, 1.0],
            [0x3fe6_a09e_667f_3bcc, 0x3fe6_a09e_667f_3bcc, 0],
        ),
        (
            [4.0, 0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 4.0],
            [0, 0, 0x3ff0_0000_0000_0000],
        ),
        (
            [1e-300, 2e-300, 0.0, 2e-300, 1e-300, 3e-301, 0.0, 3e-301, 5e-300],
            [0x3fa8_f372_b913_60c2, 0x3fb9_22c5_418e_677d, 0x3fef_cea2_c4a1_c9e4],
        ),
        (
            [1e300, 2e299, 1e299, 2e299, 3e300, 0.0, 1e299, 0.0, 2e300],
            [0xbfb9_5857_d34d_5168, 0xbfef_d75d_6a07_0947, 0xbf83_e165_ce46_9ce0],
        ),
    ];

    #[test]
    fn numpy_axes_to_the_bit() {
        for (m, want) in RECORDED {
            assert_eq!(principal_axis(&m).map(f64::to_bits), want, "{m:?}");
        }
    }

    #[test]
    fn eigenvalues_ascending_and_vectors_orthonormal() {
        let m = RECORDED[0].0;
        let (w, v) = eigh(&m).unwrap_or_else(|| unreachable!());
        assert!(w[0] <= w[1] && w[1] <= w[2]);
        for a in 0..3 {
            for b in 0..3 {
                let dot: f64 = (0..3).map(|r| v[r][a] * v[r][b]).sum();
                assert!((dot - if a == b { 1.0 } else { 0.0 }).abs() < 1e-12);
            }
            // m v = w v
            for r in 0..3 {
                let mv: f64 = (0..3).map(|k| m[r * 3 + k] * v[k][a]).sum();
                assert!((mv - w[a] * v[r][a]).abs() < 1e-7 * w[2]);
            }
        }
    }
}
