//! The WW2 weapons' loadout pictures (Project OPFOR's `pictures.py`, round OR4): a weapon's parts seen from its
//! right side (`-x`), muzzle right, z-buffered, textured and lit, supersampled and filtered down with the colour
//! bled into the transparent texels - NumPy's `float64` arithmetic in NumPy's order, so the pictures (and their DXT5
//! files) are the ones the Python drew.
//!
//! The products NumPy hands to OpenBLAS (`(k, 3) @ (3,)`, `dgemv`) are written as the reference machine's kernel
//! computes them: `fma(a2, b2, fma(a1, b1, a0 b0))` for one row and `fma(a2, b2, fma(a0, b0, a1 b1))` for more (the
//! spec's XWW2-PIC rules); the means over the supersampled blocks are NumPy's reduction orders.

use super::{FPic, Pic, clip, rolled, to_u8};

/// One part to draw: positions, optional normals (per vertex), uvs and colour image, triangles.
#[derive(Debug, Clone, Default)]
pub struct Part {
    /// Vertex positions (`float64`).
    pub positions: Vec<[f64; 3]>,
    /// Vertex normals, when the part has them.
    pub normals: Option<Vec<[f64; 3]>>,
    /// Vertex uvs (`float64`), when the part has them.
    pub uvs: Option<Vec<[f64; 2]>>,
    /// The colour image's first three channels as `float64` (`(h, w, 3)`), when the part has one.
    pub image: Option<FPic>,
    /// Triangles (vertex indices; a negative one counts from the end, as NumPy's).
    pub tris: Vec<[i64; 3]>,
}

/// Why a picture could not be drawn (Python's `IndexError` / `ValueError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PictureError(pub String);

impl std::fmt::Display for PictureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for PictureError {}

type Result<T> = std::result::Result<T, PictureError>;

/// `light` and `half` of `render`: `[-0.55, 0.75, 0.35]` normalised, and it plus `[-1, 0, 0]` normalised, as NumPy
/// computed them (their bits).
const LIGHT: [f64; 3] = [
    f64::from_bits(0xbfe1_b606_db14_e5e9),
    f64::from_bits(0x3fe8_26c3_87d6_ade0),
    f64::from_bits(0x3fd6_8a94_5ca6_3be1),
];
const HALF: [f64; 3] = [
    f64::from_bits(0xbfec_33d2_0194_1eeb),
    f64::from_bits(0x3fdb_674c_a2cb_b050),
    f64::from_bits(0x3fc9_939c_dc35_937d),
];

fn idx(i: i64, n: usize) -> Result<usize> {
    let j = if i < 0 { i + n as i64 } else { i };
    if j < 0 || j as usize >= n {
        return Err(PictureError(format!(
            "index {i} is out of bounds for axis 0 with size {n}"
        )));
    }
    Ok(j as usize)
}

/// `(k, 3) @ v` as OpenBLAS's `dgemv` gives it for `k` rows.
fn gemv(n: &[[f64; 3]], v: &[f64; 3]) -> Vec<f64> {
    if n.len() == 1 {
        let a = n[0];
        return vec![a[2].mul_add(v[2], a[1].mul_add(v[1], a[0] * v[0]))];
    }
    n.iter()
        .map(|a| a[2].mul_add(v[2], a[0].mul_add(v[0], a[1] * v[1])))
        .collect()
}

/// NumPy's `x % 1.0` (`np.mod`: the sign of the divisor).
fn mod1(x: f64) -> f64 {
    let m = x % 1.0;
    if m != 0.0 {
        if m < 0.0 { m + 1.0 } else { m }
    } else {
        0.0f64.copysign(1.0)
    }
}

/// `render(parts, w, h, box, margin, ss)`: an `(h, w, 4)` picture of the parts seen from `-x` (their right side), z
/// right, y up, the box ((z, y) low, high) framed to fill the width (`margin`: the fraction left free across / up and
/// down), alpha the coverage; supersampled `ss` times.
///
/// # Errors
/// A triangle names a vertex the part does not have.
pub fn render(
    parts: &[Part],
    w: usize,
    h: usize,
    lo: [f64; 2],
    hi: [f64; 2],
    margin: [f64; 2],
    ss: usize,
) -> Result<Pic> {
    let (bw, bh) = (w * ss, h * ss);
    let (wf, hf) = (bw as f64, bh as f64);
    let scale = (wf * (1.0 - 2.0 * margin[0]) / (hi[0] - lo[0]).max(1e-6))
        .min(hf * (1.0 - 2.0 * margin[1]) / (hi[1] - lo[1]).max(1e-6));
    let cx = (lo[0] + hi[0]) / 2.0;
    let cy = (lo[1] + hi[1]) / 2.0;
    let mut col = vec![0.0f64; bw * bh * 3];
    let mut cov = vec![0.0f64; bw * bh];
    let mut zb = vec![f64::INFINITY; bw * bh];
    let mut hits: Vec<(usize, f64, f64, f64)> = Vec::new();
    for p in parts {
        let nv = p.positions.len();
        let sx: Vec<f64> = p.positions.iter().map(|q| (q[2] - cx) * scale + wf / 2.0).collect();
        let sy: Vec<f64> = p.positions.iter().map(|q| hf / 2.0 - (q[1] - cy) * scale).collect();
        for t in &p.tris {
            let t = [idx(t[0], nv)?, idx(t[1], nv)?, idx(t[2], nv)?];
            let (x0, x1, x2) = (sx[t[0]], sx[t[1]], sx[t[2]]);
            let (y0, y1, y2) = (sy[t[0]], sy[t[1]], sy[t[2]]);
            let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
            if area.abs() < 1e-12 {
                continue;
            }
            let xa = x0.min(x1).min(x2).max(0.0) as i64;
            let xb = x0.max(x1).max(x2).min(wf - 1.0) as i64 + 1;
            let ya = y0.min(y1).min(y2).max(0.0) as i64;
            let yb = y0.max(y1).max(y2).min(hf - 1.0) as i64 + 1;
            if xa >= xb || ya >= yb {
                continue;
            }
            let (sz0, sz1, sz2) = (p.positions[t[0]][0], p.positions[t[1]][0], p.positions[t[2]][0]);
            hits.clear();
            for py in ya..yb {
                let gy = py as f64 + 0.5;
                for px in xa..xb {
                    let gx = px as f64 + 0.5;
                    let b0 = ((x1 - gx) * (y2 - gy) - (x2 - gx) * (y1 - gy)) / area;
                    let b1 = ((x2 - gx) * (y0 - gy) - (x0 - gx) * (y2 - gy)) / area;
                    let b2 = 1.0 - b0 - b1;
                    if !(b0 >= 0.0 && b1 >= 0.0 && b2 >= 0.0) {
                        continue;
                    }
                    let z = b0 * sz0 + b1 * sz1 + b2 * sz2;
                    let at = py as usize * bw + px as usize;
                    if z < zb[at] {
                        hits.push((at, b0, b1, b2));
                        zb[at] = z;
                    }
                }
            }
            if hits.is_empty() {
                continue;
            }
            for &(at, ..) in &hits {
                cov[at] = 1.0;
            }
            let mut n: Vec<[f64; 3]> = if let Some(nrm) = &p.normals {
                let (n0, n1, n2) = (nrm[t[0]], nrm[t[1]], nrm[t[2]]);
                hits.iter()
                    .map(|&(_, b0, b1, b2)| std::array::from_fn(|c| b0 * n0[c] + b1 * n1[c] + b2 * n2[c]))
                    .collect()
            } else {
                let (a, b, c) = (p.positions[t[0]], p.positions[t[1]], p.positions[t[2]]);
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let f = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                vec![f; hits.len()]
            };
            for q in &mut n {
                let len = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2]).sqrt();
                let d = if len.is_nan() { len } else { len.max(1e-9) };
                for c in q.iter_mut() {
                    *c /= d;
                }
                if q[0] > 0.0 {
                    for c in q.iter_mut() {
                        *c *= -1.0;
                    }
                }
            }
            let diff = gemv(&n, &LIGHT);
            let spec = gemv(&n, &HALF);
            for (k, &(at, b0, b1, b2)) in hits.iter().enumerate() {
                let diff = clip(diff[k], 0.0, 1.0);
                let spec = clip(spec[k], 0.0, 1.0).powf(24.0);
                let c: [f64; 3] = match (&p.image, &p.uvs) {
                    (Some(tex), Some(uv)) => {
                        let u = b0 * uv[t[0]][0] + b1 * uv[t[1]][0] + b2 * uv[t[2]][0];
                        let v = b0 * uv[t[0]][1] + b1 * uv[t[1]][1] + b2 * uv[t[2]][1];
                        let tx = (mod1(u) * (tex.w as f64 - 1.0)) as i64;
                        let ty = (mod1(v) * (tex.h as f64 - 1.0)) as i64;
                        let tx = idx(tx, tex.w)?;
                        let ty = idx(ty, tex.h)?;
                        let o = (ty * tex.w + tx) * tex.ch;
                        [tex.v[o], tex.v[o + 1], tex.v[o + 2]]
                    }
                    _ => [150.0; 3],
                };
                let shade = 0.72 + 0.78 * diff;
                for j in 0..3 {
                    col[at * 3 + j] = c[j] * shade + 95.0 * spec;
                }
            }
        }
    }
    // brighter, as DICE's: the darks lifted, the middle tones given more contrast
    for v in &mut col {
        let x = clip(*v / 255.0, 0.0, 1.0).powf(0.8);
        *v = 255.0 * clip(0.5 + (x - 0.5) * 1.12, 0.0, 1.0);
    }
    // filter down: premultiplied
    let n9 = (ss * ss) as f64;
    let mut rgb = FPic::zeros(w, h, 3);
    let mut a = vec![0.0f64; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut pre = [0.0f64; 3];
            let mut al = 0.0f64;
            for k in 0..ss {
                let mut row = -0.0f64;
                for l in 0..ss {
                    let at = (y * ss + k) * bw + x * ss + l;
                    for j in 0..3 {
                        pre[j] += col[at * 3 + j] * cov[at];
                    }
                    row += cov[at];
                }
                al += row;
            }
            let al = al / n9;
            a[y * w + x] = al;
            for j in 0..3 {
                rgb.v[(y * w + x) * 3 + j] = if al > 0.0 { (pre[j] / n9) / al.max(1e-6) } else { 0.0 };
            }
        }
    }
    Ok(finish(&bleed(&rgb, &a, 6), &a))
}

/// The colour spread into the transparent texels next to the picture, `steps` times (DXT blocks mix them).
#[must_use]
pub fn bleed(rgb: &FPic, a: &[f64], steps: usize) -> FPic {
    let (w, h) = (rgb.w, rgb.h);
    let mut rgb = rgb.clone();
    let mut known: Vec<bool> = a.iter().map(|&v| v > 0.0).collect();
    for _ in 0..steps {
        let mut acc = vec![0.0f64; w * h * 3];
        let mut cnt = vec![0.0f64; w * h];
        for (dy, dx) in [(0, 1), (0, -1), (1, 0), (-1, 0)] {
            for y in 0..h {
                for x in 0..w {
                    let from = rolled(h, w, y, x, dy, dx);
                    let s = if known[from] { 1.0 } else { 0.0 };
                    let at = y * w + x;
                    for j in 0..3 {
                        acc[at * 3 + j] += rgb.v[from * 3 + j] * s;
                    }
                    cnt[at] += s;
                }
            }
        }
        let mut grow = vec![false; w * h];
        for at in 0..w * h {
            if !known[at] && cnt[at] > 0.0 {
                grow[at] = true;
                for j in 0..3 {
                    rgb.v[at * 3 + j] = acc[at * 3 + j] / cnt[at];
                }
            }
        }
        for at in 0..w * h {
            known[at] |= grow[at];
        }
    }
    rgb
}

/// `np.dstack([rgb, a * 255]).clip(0, 255).astype(np.uint8)`.
fn finish(rgb: &FPic, a: &[f64]) -> Pic {
    let mut out = Pic::new(rgb.w, rgb.h, 4);
    for at in 0..rgb.w * rgb.h {
        for j in 0..3 {
            out.px[at * 4 + j] = to_u8(clip(rgb.v[at * 3 + j], 0.0, 255.0));
        }
        out.px[at * 4 + 3] = to_u8(clip(a[at] * 255.0, 0.0, 255.0));
    }
    out
}

/// `framed(parts, w, h)`: the picture framed on what shows - a first pass (512 x 512, no margin, no supersampling)
/// over the box of every vertex a triangle uses finds where the weapon covers pixels (slivers too thin to draw do not
/// count), and the picture is made on that.
///
/// # Errors
/// As [`render`]; no part draws a pixel.
pub fn framed(parts: &[Part], w: usize, h: usize) -> Result<Pic> {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    let mut any = false;
    for p in parts {
        let n = p.positions.len();
        for t in &p.tris {
            for &i in t {
                let q = p.positions[idx(i, n)?];
                for (j, c) in [2, 1].into_iter().enumerate() {
                    lo[j] = lo[j].min(q[c]);
                    hi[j] = hi[j].max(q[c]);
                }
                any = true;
            }
        }
    }
    if !any {
        return Err(PictureError(
            "zero-size array to reduction operation minimum which has no identity".into(),
        ));
    }
    let test = render(parts, 512, 512, lo, hi, [0.0, 0.0], 1)?;
    let (mut xmin, mut xmax, mut ymin, mut ymax) = (usize::MAX, 0usize, usize::MAX, 0usize);
    for y in 0..512 {
        for x in 0..512 {
            if test.px[(y * 512 + x) * 4 + 3] > 0 {
                xmin = xmin.min(x);
                xmax = xmax.max(x);
                ymin = ymin.min(y);
                ymax = ymax.max(y);
            }
        }
    }
    if xmin == usize::MAX {
        return Err(PictureError(
            "zero-size array to reduction operation minimum which has no identity".into(),
        ));
    }
    let span = [hi[0] - lo[0], hi[1] - lo[1]];
    let s = 512.0 / span[0].max(span[1]);
    let c = [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0];
    let zl = c[0] + (xmin as f64 - 256.0) / s;
    let zh = c[0] + ((xmax + 1) as f64 - 256.0) / s;
    let yl = c[1] + (256.0 - ymax as f64 - 1.0) / s;
    let yh = c[1] + (256.0 - ymin as f64) / s;
    render(parts, w, h, [zl, yl], [zh, yh], [0.03, 0.08], 3)
}

/// `down(img, w, h)`: a smaller picture of an RGBA one (premultiplied box filter), its colour bled as [`render`]'s.
///
/// # Errors
/// The picture is not 4 channels or its sides are not whole multiples of `w` / `h`.
pub fn down(img: &Pic, w: usize, h: usize) -> Result<Pic> {
    if img.ch != 4 || w == 0 || h == 0 || img.w % w != 0 || img.h % h != 0 {
        return Err(PictureError(format!(
            "cannot reshape a {} x {} picture of {} channels into {w} x {h}",
            img.w, img.h, img.ch
        )));
    }
    let (k0, k1) = (img.h / h, img.w / w);
    let n = (k0 * k1) as f64;
    let mut rgb = FPic::zeros(w, h, 3);
    let mut a = vec![0.0f64; w * h];
    for y in 0..h {
        for x in 0..w {
            let mut pre = [0.0f64; 3];
            let mut al = 0.0f64;
            for k in 0..k0 {
                let mut row = -0.0f64;
                for l in 0..k1 {
                    let at = ((y * k0 + k) * img.w + x * k1 + l) * 4;
                    let av = f64::from(img.px[at + 3]) / 255.0;
                    for j in 0..3 {
                        pre[j] += f64::from(img.px[at + j]) * av;
                    }
                    row += av;
                }
                al += row;
            }
            let al = al / n;
            a[y * w + x] = al;
            for j in 0..3 {
                rgb.v[(y * w + x) * 3 + j] = if al > 0.0 { (pre[j] / n) / al.max(1e-6) } else { 0.0 };
            }
        }
    }
    Ok(finish(&bleed(&rgb, &a, 6), &a))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_is_normalised() {
        for v in [LIGHT, HALF] {
            let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
            assert!((n - 1.0).abs() < 1e-15);
        }
    }

    #[test]
    fn a_square_fills_its_frame() {
        let p = Part {
            positions: vec![[0.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 1.0, 4.0], [0.0, 0.0, 4.0]],
            tris: vec![[0, 1, 2], [0, 2, 3]],
            ..Part::default()
        };
        let pic = framed(&[p], 64, 16).ok();
        let pic = pic.as_ref();
        assert_eq!(pic.map(|p| (p.w, p.h, p.ch)), Some((64, 16, 4)));
        // the middle is opaque, the corners keep the margin clear
        assert_eq!(pic.map(|p| p.px[(8 * 64 + 32) * 4 + 3]), Some(255));
        assert_eq!(pic.map(|p| p.px[3]), Some(0));
        let small = pic.and_then(|p| down(p, 32, 8).ok());
        assert_eq!(small.map(|p| p.px[(4 * 32 + 16) * 4 + 3]), Some(255));
    }

    #[test]
    fn a_bad_index_is_an_error() {
        let p = Part {
            positions: vec![[0.0; 3]; 2],
            tris: vec![[0, 1, 2]],
            ..Part::default()
        };
        assert!(render(&[p], 4, 4, [0.0; 2], [1.0; 2], [0.0; 2], 1).is_err());
    }
}
