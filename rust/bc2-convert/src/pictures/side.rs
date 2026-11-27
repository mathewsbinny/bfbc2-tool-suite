//! A vehicle mesh seen from the side as a DICE-style HUD icon (Project OPFOR's `type95_icon.silhouette`, round OR14):
//! the Type 95 rowboat's kill-feed picture and the Onslaught boats' (`onslaught/console_icons.py`). The faces are
//! drawn far to near with Pillow's polygon fill ([`super::polygon`]) at `ss` x the size, each lit from the viewer's
//! side, then averaged down, given DICE's gradient, the detail parts' dark gap, the grey rail under the gunwale and
//! the one-pixel outline - NumPy's float work step for step:
//!
//! - the normals: `np.cross` (each product rounded, then subtracted), `np.linalg.norm(axis=1)` (`(x² + y²) + z²`);
//! - the light `n @ light`: OpenBLAS's `dgemv` on this machine - `fma(n_z, l_z, fma(n_x, l_x, n_y * l_y))` - and for
//!   a single face NumPy's dot - `fma(n_z, l_z, fma(n_y, l_y, n_x * l_x))`;
//! - the far-to-near order `np.argsort(-(a_x + b_x + c_x))`: NumPy's default sort (x86-simd-sort on AVX-512) orders
//!   equal keys in no reproducible way; here they keep the faces' order (stable), and [`ties`] tells which pixels
//!   another order could change;
//! - the per-pixel sums `reshape(h, ss, w, ss).sum((1, 3))`: each sub-row's pairwise sum, added row after row.

use super::polygon;
use super::{Pic, clip, pairwise_sum, rolled, to_u8};

/// The fill's brightness at the silhouette's top and bottom (`type95_icon.TOP` / `BOTTOM`).
pub const TOP: f64 = 252.0;
/// See [`TOP`].
pub const BOTTOM: f64 = 198.0;

/// One part of a mesh: its name, positions and triangles (`meshview.load_mesh`'s `name`, `positions`, `indices`).
#[derive(Debug, Clone, Copy)]
pub struct MeshPart<'a> {
    /// The part's name (`skip` / `detail` match its ending).
    pub name: &'a str,
    /// The vertices (x, y, z).
    pub positions: &'a [[f32; 3]],
    /// The triangles: indices into `positions`.
    pub tris: &'a [[u32; 3]],
}

/// What a different order of equal far-to-near keys could change ([`ties`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ties {
    /// Faces whose key equals another face's.
    pub tied_faces: usize,
    /// Supersampled shade pixels whose last face is one of several tied faces covering it with different shades.
    pub shade_pixels: usize,
    /// Output pixels (any channel) that differ between the darkest and the brightest of those choices.
    pub out_pixels: usize,
}

/// The light from the viewer's side, normalised as NumPy does (`[-0.55, 0.75, 0.35] / norm`).
fn light() -> [f64; 3] {
    let l = [-0.55f64, 0.75, 0.35];
    let n = ((l[0] * l[0] + l[1] * l[1]) + l[2] * l[2]).sqrt();
    [l[0] / n, l[1] / n, l[2] / n]
}

/// The drawn faces: the polygons (supersampled pixel coordinates), their shades and detail flags, in far-to-near
/// order with each face's tie group (equal keys).
struct Faces {
    poly: Vec<[(f64, f64); 3]>,
    fill: Vec<u8>,
    detail: Vec<bool>,
    order: Vec<usize>,
    group: Vec<usize>,
}

fn faces(
    parts: &[MeshPart<'_>],
    w: usize,
    h: usize,
    ss: usize,
    skip: &[&str],
    detail: &[&str],
) -> Result<Faces, String> {
    if w == 0 || h == 0 || ss == 0 {
        return Err(format!("a {w} x {h} picture at {ss} x: nothing to draw"));
    }
    let mut pos: Vec<[f64; 3]> = Vec::new();
    let mut tris: Vec<[usize; 3]> = Vec::new();
    let mut motor: Vec<bool> = Vec::new();
    let mut kept = 0;
    for p in parts {
        if skip.iter().any(|s| p.name.ends_with(s)) {
            continue;
        }
        kept += 1;
        let base = pos.len();
        let is_detail = detail.iter().any(|s| p.name.ends_with(s));
        for t in p.tris {
            let mut g = [0usize; 3];
            for k in 0..3 {
                let i = t[k] as usize;
                if i >= p.positions.len() {
                    return Err(format!(
                        "index {i} is out of bounds for axis 0 with size {} (part {})",
                        p.positions.len(),
                        p.name
                    ));
                }
                g[k] = base + i;
            }
            tris.push(g);
            motor.push(is_detail);
        }
        pos.extend(
            p.positions
                .iter()
                .map(|v| [f64::from(v[0]), f64::from(v[1]), f64::from(v[2])]),
        );
    }
    if kept == 0 {
        return Err("need at least one array to concatenate".to_owned());
    }
    if pos.is_empty() {
        return Err("zero-size array to reduction operation maximum which has no identity".to_owned());
    }
    if pos.iter().flatten().any(|v| !v.is_finite()) {
        return Err("a vertex position is not finite".to_owned());
    }
    let max = |k: usize| pos.iter().map(|v| v[k]).fold(f64::NEG_INFINITY, f64::max);
    let min = |k: usize| pos.iter().map(|v| v[k]).fold(f64::INFINITY, f64::min);
    let (zmin, zmax, ymin, ymax) = (min(2), max(2), min(1), max(1));
    let span = zmax - zmin;
    if span == 0.0 {
        return Err("the mesh has no length (z): its scale would be infinite".to_owned());
    }
    let scale = ((w as i64 - 2) * ss as i64) as f64 / span;
    let height = (ymax - ymin) * scale;
    let x0 = ss as f64;
    let y0 = ((h * ss) as f64 + height) / 2.0;
    let px: Vec<(f64, f64)> = pos
        .iter()
        .map(|v| (x0 + (v[2] - zmin) * scale, y0 - (v[1] - ymin) * scale))
        .collect();
    let l = light();
    let single = tris.len() == 1;
    let mut fill = Vec::with_capacity(tris.len());
    let mut key = Vec::with_capacity(tris.len());
    for t in &tris {
        let (a, b, c) = (pos[t[0]], pos[t[1]], pos[t[2]]);
        let e = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let f = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let mut n = [
            e[1] * f[2] - e[2] * f[1],
            e[2] * f[0] - e[0] * f[2],
            e[0] * f[1] - e[1] * f[0],
        ];
        let len = ((n[0] * n[0] + n[1] * n[1]) + n[2] * n[2]).sqrt().max(1e-12);
        for v in &mut n {
            *v /= len;
        }
        if n[0] > 0.0 {
            for v in &mut n {
                *v *= -1.0;
            }
        }
        let dot = if single {
            n[2].mul_add(l[2], n[1].mul_add(l[1], n[0] * l[0]))
        } else {
            n[2].mul_add(l[2], n[0].mul_add(l[0], n[1] * l[1]))
        };
        let lit = 0.45 + 0.55 * clip(dot, 0.0, 1.0);
        fill.push((255.0 * lit) as u8);
        key.push(-((a[0] + b[0]) + c[0]));
    }
    let mut order: Vec<usize> = (0..tris.len()).collect();
    // ascending keys, equal ones (±0 too, as NumPy compares them) in the faces' order: `kind='stable'`'s order
    order.sort_by(|&i, &j| {
        key[i]
            .partial_cmp(&key[j])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(i.cmp(&j))
    });
    let mut group = vec![0usize; tris.len()];
    let mut g = 0;
    for k in 1..order.len() {
        if key[order[k]] != key[order[k - 1]] {
            g += 1;
        }
        group[order[k]] = g;
    }
    let poly = tris.iter().map(|t| [px[t[0]], px[t[1]], px[t[2]]]).collect();
    Ok(Faces {
        poly,
        fill,
        detail: motor,
        order,
        group,
    })
}

/// The supersampled mask, shade and detail mask, and each shade pixel's (lowest, highest) tied choice.
type Drawn = (Vec<u8>, Vec<u8>, Vec<u8>, Vec<(u8, u8)>);

/// The three supersampled pictures (mask, shade, detail mask) and, when asked, each shade pixel's (lowest, highest)
/// shade among the tied faces that could be drawn last there.
fn draw(f: &Faces, ww: usize, hh: usize, track: bool) -> Result<Drawn, String> {
    let mut mask = vec![0u8; ww * hh];
    let mut shade = vec![0u8; ww * hh];
    let mut mmask = vec![0u8; ww * hh];
    let mut range: Vec<(u8, u8)> = if track { vec![(0, 0); ww * hh] } else { Vec::new() };
    let mut last: Vec<usize> = if track { vec![usize::MAX; ww * hh] } else { Vec::new() };
    for &k in &f.order {
        let (v, g) = (f.fill[k], f.group[k]);
        polygon::fill(&f.poly[k], ww, hh, |y, x0, x1| {
            let r = y * ww + x0..=y * ww + x1;
            mask[r.clone()].fill(255);
            shade[r.clone()].fill(v);
            if f.detail[k] {
                mmask[r.clone()].fill(255);
            }
            if track {
                for i in r {
                    if last[i] == g {
                        range[i] = (range[i].0.min(v), range[i].1.max(v));
                    } else {
                        last[i] = g;
                        range[i] = (v, v);
                    }
                }
            }
        })?;
    }
    Ok((mask, shade, mmask, range))
}

/// The final picture from the supersampled ones.
fn finish(mask: &[u8], shade: &[u8], mmask: &[u8], w: usize, h: usize, ss: usize) -> Result<Pic, String> {
    let ww = w * ss;
    let n = (ss * ss) as f64;
    let mut cover = vec![0.0f64; w * h];
    let mut lum = vec![0.0f64; w * h];
    let mut mcov = vec![false; w * h];
    let mut sub = vec![0.0f64; ss];
    for y in 0..h {
        for x in 0..w {
            let (mut cnt, mut mcnt, mut s) = (0usize, 0usize, 0.0f64);
            for i in 0..ss {
                let row = (y * ss + i) * ww + x * ss;
                for j in 0..ss {
                    cnt += usize::from(mask[row + j] != 0);
                    mcnt += usize::from(mmask[row + j] != 0);
                    sub[j] = f64::from(shade[row + j]) / 255.0;
                }
                s += pairwise_sum(&sub);
            }
            // the masks' sums are whole numbers: exact in any order
            cover[y * w + x] = cnt as f64 / n;
            lum[y * w + x] = s / (cnt as f64).max(1e-9);
            mcov[y * w + x] = mcnt as f64 / n > 0.5;
        }
    }
    let rows: Vec<usize> = (0..h)
        .filter(|&y| cover[y * w..(y + 1) * w].iter().any(|&c| c > 0.0))
        .collect();
    let (Some(&top), Some(&bot)) = (rows.first(), rows.last()) else {
        return Err("zero-size array to reduction operation minimum which has no identity".to_owned());
    };
    let span = ((bot - top) as i64).max(1) as f64;
    let mut fill = vec![0.0f64; w * h];
    let solid: Vec<bool> = cover.iter().map(|&c| c > 0.35).collect();
    for y in 0..h {
        let grad = TOP + (BOTTOM - TOP) * clip((y as f64 - top as f64) / span, 0.0, 1.0);
        for x in 0..w {
            fill[y * w + x] = grad * (0.84 + 0.16 * clip(lum[y * w + x], 0.0, 1.0));
        }
    }
    let hull: Vec<bool> = (0..w * h).map(|i| solid[i] && !mcov[i]).collect();
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let by = |dy: isize, dx: isize| hull[rolled(h, w, y, x, dy, dx)];
            if mcov[i] && (by(0, 1) || by(0, -1) || by(1, 0) || by(-1, 0)) {
                fill[i] = 95.0;
            }
        }
    }
    let cols: Vec<usize> = (0..w).filter(|&x| (0..h).any(|y| hull[y * w + x])).collect();
    if cols.len() > 6 {
        for &x in &cols[3..cols.len() - 3] {
            let r = (0..h).find(|&y| hull[y * w + x]).unwrap_or(0) + 2;
            if r < h && hull[r * w + x] {
                let i = r * w + x;
                if fill[i] > 165.0 {
                    fill[i] = 165.0;
                }
            }
        }
    }
    let mut out = Pic::new(w, h, 4);
    for y in 0..h {
        for x in 0..w {
            let i = y * w + x;
            let mut near = false;
            for dy in -1..=1 {
                for dx in -1..=1 {
                    near |= solid[rolled(h, w, y, x, dy, dx)];
                }
            }
            let ring = near && !solid[i];
            let edge = !solid[i] && cover[i] > 0.0;
            let mut rgb = if solid[i] { fill[i] } else { 0.0 };
            let mut alpha = if solid[i] {
                255.0
            } else if ring {
                230.0
            } else {
                0.0
            };
            if edge {
                rgb = 0.0;
                alpha = f64::max(alpha, 230.0);
            }
            let c = to_u8(clip(rgb, 0.0, 255.0));
            out.px[i * 4..i * 4 + 3].fill(c);
            out.px[i * 4 + 3] = to_u8(clip(alpha, 0.0, 255.0));
        }
    }
    Ok(out)
}

/// `type95_icon.silhouette(ix, (w, h), ss, mesh, skip, detail)` of the mesh's parts: the RGBA `(h, w, 4)` picture.
/// `skip`: part name endings left out; `detail`: part name endings outlined where they meet the rest.
///
/// # Errors
/// A zero size or `ss`, every part skipped, a triangle index out of its part, a non-finite position, a mesh of no
/// length along z, or nothing drawn inside the picture (the Python's NumPy errors, but the last three: decided).
pub fn silhouette(
    parts: &[MeshPart<'_>],
    w: usize,
    h: usize,
    ss: usize,
    skip: &[&str],
    detail: &[&str],
) -> Result<Pic, String> {
    let f = faces(parts, w, h, ss, skip, detail)?;
    let (mask, shade, mmask, _) = draw(&f, w * ss, h * ss, false)?;
    finish(&mask, &shade, &mmask, w, h, ss)
}

/// What NumPy's unreproducible order of equal far-to-near keys could change in [`silhouette`]'s picture: the shade
/// pixels whose last-drawn face is one of several tied faces of different shades, and the output pixels that differ
/// between the picture made with each such pixel's darkest choice and with its brightest (the output is monotonic in
/// each shade: a pixel equal in both is the same for every order).
///
/// # Errors
/// As [`silhouette`].
pub fn ties(
    parts: &[MeshPart<'_>],
    w: usize,
    h: usize,
    ss: usize,
    skip: &[&str],
    detail: &[&str],
) -> Result<Ties, String> {
    let f = faces(parts, w, h, ss, skip, detail)?;
    let mut sizes = std::collections::HashMap::<usize, usize>::new();
    for &g in &f.group {
        *sizes.entry(g).or_default() += 1;
    }
    let tied_faces = f
        .group
        .iter()
        .filter(|g| sizes.get(g).copied().unwrap_or(0) > 1)
        .count();
    let (mask, _, mmask, range) = draw(&f, w * ss, h * ss, true)?;
    let shade_pixels = range.iter().filter(|r| r.0 != r.1).count();
    if shade_pixels == 0 {
        return Ok(Ties {
            tied_faces,
            ..Ties::default()
        });
    }
    let lo: Vec<u8> = range.iter().map(|r| r.0).collect();
    let hi: Vec<u8> = range.iter().map(|r| r.1).collect();
    let a = finish(&mask, &lo, &mmask, w, h, ss)?;
    let b = finish(&mask, &hi, &mmask, w, h, ss)?;
    let out_pixels =
        a.px.chunks_exact(4)
            .zip(b.px.chunks_exact(4))
            .filter(|(p, q)| p != q)
            .count();
    Ok(Ties {
        tied_faces,
        shade_pixels,
        out_pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_light_is_numpys() {
        let l = light();
        assert_eq!(
            [l[0].to_bits(), l[1].to_bits(), l[2].to_bits()],
            [0xbfe1_b606_db14_e5e9, 0x3fe8_26c3_87d6_ade0, 0x3fd6_8a94_5ca6_3be1]
        );
    }

    /// xorshift64: the synthetic meshes' numbers (the same generator in Python made the recorded digests).
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: u64) -> usize {
            (self.next() % n) as usize
        }
    }

    type Case = (Vec<(String, Vec<[f32; 3]>, Vec<[u32; 3]>)>, usize, usize, usize);

    /// Case `k`: 1 - 3 parts (a hull, a `_Metal` detail part, a skipped `_Rope`), random or half-unit grid positions
    /// (coincident vertices, equal depth keys, flat x), random triangles (degenerate ones too), a random size and ss.
    fn case(k: u64) -> Case {
        let mut r = Rng(0x9E37_79B9_7F4A_7C15 ^ k);
        let mut parts = Vec::new();
        let np = 1 + r.below(3);
        for p in 0..np {
            let nv = 3 + r.below(40);
            let mut pos = Vec::with_capacity(nv);
            for _ in 0..nv {
                let v: [f32; 3] = std::array::from_fn(|_| {
                    if k % 3 == 1 {
                        r.below(9) as f32 / 2.0 - 2.0
                    } else {
                        ((r.next() >> 40) as i64 - (1 << 23)) as f32 / (1 << 20) as f32
                    }
                });
                pos.push(v);
            }
            if k % 3 == 2 {
                for v in &mut pos {
                    v[0] = 0.25;
                }
            }
            let nt = 1 + r.below(60);
            let tris = (0..nt)
                .map(|_| std::array::from_fn(|_| r.below(nv as u64) as u32))
                .collect();
            let name = ["Hull", "Deck_Metal", "Coil_Rope"][p % 3].to_owned();
            parts.push((name, pos, tris));
        }
        let w = 20 + r.below(60);
        let h = 10 + r.below(30);
        let ss = [1, 2, 4, 8][r.below(4)];
        (parts, w, h, ss)
    }

    fn digest(k: u64) -> String {
        let (parts, w, h, ss) = case(k);
        let ps: Vec<MeshPart<'_>> = parts
            .iter()
            .map(|(name, positions, tris)| MeshPart { name, positions, tris })
            .collect();
        match silhouette(&ps, w, h, ss, &["_Rope"], &["_Metal"]) {
            Ok(p) => sha1_smol::Sha1::from(&p.px).digest().to_string(),
            Err(e) => e,
        }
    }

    /// SHA-1s of `type95_icon.silhouette`'s pictures of the synthetic cases (Pillow 12.3, NumPy 2.5.4 on AVX-512),
    /// its argsort made `kind='stable'` - the order this port draws equal depth keys in. Cases 1, 4, 7 and 10 are
    /// pictures NumPy's default argsort draws otherwise (its order of equal keys); the rest are the default's too.
    #[test]
    fn synthetic_meshes_match_the_python() {
        let want = [
            (0, "7039ecaf73523c2d270f849049fa26d54b1d11d0"),
            (1, "fb509ed3025c1b7bf4782d75e2ffa68ff55fc7c4"),
            (2, "a040a1dd351164db5750e3497bc63eb8bf9bed4e"),
            (3, "d68d49c67ffe46b65e9a2d9858fb1db6629eed9e"),
            (4, "01f7b5584553caacee8f8424c211ee42b6e89b08"),
            (5, "e0e1b83c56712cb77284b4980e2ab7a0268dfecc"),
            (6, "e873390c5c1ed5a8f1b3aeb54459375b78328700"),
            (7, "3c9e1c22e9e427e93a31a9fadc3b693257c3a04b"),
            (8, "5131ba564723d5e2c435bf627e12b66c3fafb3c7"),
            (9, "c80dc4c432136918461bec5abbe978057ec756cd"),
            (10, "d58d67ec92503cda78bff6bfb92e735742757cf5"),
            (11, "19bbf9249e155dd69496aa6b2d60464fe2a11aad"),
        ];
        for (k, d) in want {
            assert_eq!(digest(k), d, "case {k}");
        }
    }
}
