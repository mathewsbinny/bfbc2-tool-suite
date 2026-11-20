//! NavKit's offline sampler's collision assets (`bfbc2-auto-navkit/tools/navsample_offline.py`: `asset_bodies`,
//! `_shape_parts`, `_hull_faces`, `_box_faces`; round F4): the rigid bodies of a `*_Havok.res` packfile as triangles
//! in the asset's space, over [`bc2_formats::havok::Packfile`].
//!
//! **Bit for bit with NumPy.** The reference's small linear algebra runs through OpenBLAS (the wheel's Haswell
//! kernels, measured on the development PC: `rust/BENCHMARKS.md` "F4"): a 3-vector `dot` / `linalg.norm` is
//! `fma(x2, y2, fma(x1, y1, x0 * y0))`; `(k, 3) @ (3,)` is that for one row and `fma(a2, b2, fma(a0, b0, a1 * b1))`
//! for two or more; `(n, 3, 3) @ m[:3, :3]` is the dot's chain per element; each of them gives +0.0 for a zero (the
//! kernels add to a zeroed result). `np.cross`, `mean` over the first axis,
//! `sum` over a row and `np.round(x, 5)` (`rint(x * 1e5) / 1e5`) are plain IEEE steps. `np.unique(.., axis=0)` sorts
//! the rows (an insertion sort up to 17 rows, so stable) and keeps the first of equal ones; `np.argsort` of the
//! angles is NumPy's SIMD sort, not stable: tied angles hand the file to the Python, as do rows that compare equal
//! with other bits (`-0.0`, `0.0`) past 17, any NaN or infinity in a hull's input or in an output, and every error
//! the reference raises outside `_shape_parts`' `except` (that `except`'s errors drop the body, as there).

#![allow(
    clippy::indexing_slicing,
    reason = "vertex indices checked against the vertex count before use; chunks of the sizes taken; on[0] after \
              the count is checked"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::doc_markdown,
    clippy::type_complexity,
    clippy::too_many_lines,
    clippy::chunks_exact_to_as_chunks,
    clippy::float_cmp,
    clippy::items_after_statements,
    reason = "the reference's names (n, d, u, v, c, t, m) and its order of steps; exact float ties are what NumPy's \
              sort sees; the walk is one function as `_shape_parts` is"
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "a row count as the mean's divisor, as NumPy divides"
)]

/// The kinds of `navsample_offline` (`KIND_MESH`, `KIND_CONVEX`).
pub const KIND_MESH: u8 = 1;
/// `KIND_CONVEX`.
pub const KIND_CONVEX: u8 = 2;

use std::collections::HashMap;

use bc2_formats::havok::{HavokError, Packfile};

/// One body's piece: `(system index, body name, kind, triangles, part, root kind)`.
#[derive(Debug, Clone, PartialEq)]
pub struct Piece {
    /// The `hkpPhysicsSystem`'s index.
    pub system: usize,
    /// The body's name (one letter: h / e / c).
    pub name: String,
    /// [`KIND_MESH`] or [`KIND_CONVEX`].
    pub kind: u8,
    /// The triangles in the asset's space (the body's transform applied).
    pub tris: Vec<[[f64; 3]; 3]>,
    /// The mesh piece's index into the asset's `PartInfoDatas`, -1 for convex pieces.
    pub part: i32,
    /// The kind of the body's root shape.
    pub root_kind: u8,
}

/// Why the walk stopped: an error `_shape_parts`' `except` catches (the body is dropped), or one the port hands to
/// the Python.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Skip,
    Python,
}

type R<T> = Result<T, Stop>;

struct Walk<'a> {
    pf: &'a Packfile<'a>,
    d: &'a [u8],
    /// `by_offset`: each object's class (the last of an offset wins).
    by: HashMap<u64, &'a str>,
}

fn fma(a: f64, b: f64, c: f64) -> f64 {
    a.mul_add(b, c)
}

/// `np.dot(x, y)` / the square of `linalg.norm` of 3-vectors (OpenBLAS `ddot`).
fn dot(x: [f64; 3], y: [f64; 3]) -> f64 {
    fma(x[2], y[2], fma(x[1], y[1], x[0] * y[0])) + 0.0
}

/// `(k, 3) @ (3,)`, row `a` of `k` (OpenBLAS `dgemv`).
fn gemv(a: [f64; 3], b: [f64; 3], k: usize) -> f64 {
    if k == 1 {
        dot(a, b)
    } else {
        fma(a[2], b[2], fma(a[0], b[0], a[1] * b[1])) + 0.0
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn finite3(a: [f64; 3]) -> bool {
    a.iter().all(|v| v.is_finite())
}

impl<'a> Walk<'a> {
    fn deref(&self, at: u64) -> Option<u64> {
        self.pf.deref(at)
    }

    /// `struct.unpack_from('<i', d, at)`; `struct.error` past the end.
    fn i32(&self, at: u64) -> R<i32> {
        let a = usize::try_from(at).map_err(|_| Stop::Skip)?;
        let b = self.d.get(a..a.checked_add(4).ok_or(Stop::Skip)?).ok_or(Stop::Skip)?;
        Ok(i32::from_le_bytes(b.try_into().map_err(|_| Stop::Skip)?))
    }

    fn f32s<const N: usize>(&self, at: u64) -> R<[f64; N]> {
        let mut out = [0.0; N];
        for (k, v) in out.iter_mut().enumerate() {
            let a = usize::try_from(at + 4 * k as u64).map_err(|_| Stop::Skip)?;
            let b = self.d.get(a..a + 4).ok_or(Stop::Skip)?;
            *v = f64::from(f32::from_le_bytes(b.try_into().map_err(|_| Stop::Skip)?));
        }
        Ok(out)
    }

    /// `np.frombuffer(d, dtype, count, offset)`'s bytes: NumPy's `ValueError` when the offset is past the buffer or
    /// the bytes run past its end.
    fn buffer(&self, offset: u64, count: i128, size: i128) -> R<&'a [u8]> {
        let o = usize::try_from(offset).map_err(|_| Stop::Skip)?;
        if o > self.d.len() || count < 0 {
            return Err(Stop::Skip);
        }
        let n = usize::try_from(count * size).map_err(|_| Stop::Skip)?;
        self.d.get(o..o.checked_add(n).ok_or(Stop::Skip)?).ok_or(Stop::Skip)
    }

    /// `_arr(pf, at)`: `(pointer, count if the pointer is set else 0)`.
    fn arr(&self, at: u64) -> R<(Option<u64>, i32)> {
        let p = self.deref(at).filter(|&p| p != 0);
        let n = self.i32(at + 4)?;
        Ok((p, if p.is_some() { n } else { 0 }))
    }

    fn class(&self, at: u64) -> Option<&'a str> {
        self.by.get(&at).copied()
    }

    fn shape_parts(
        &self,
        sh: Option<u64>,
        offset: [f64; 3],
        out: &mut Vec<(u8, Vec<[[f64; 3]; 3]>, i32)>,
        depth: u32,
    ) -> R<()> {
        let Some(sh) = sh.filter(|&s| s != 0) else {
            return Ok(());
        };
        if depth > 8 {
            return Ok(());
        }
        match self.class(sh) {
            Some("hkpMoppBvTreeShape") => self.shape_parts(self.deref(sh + 0x34), offset, out, depth + 1)?,
            Some("hkpStorageExtendedMeshShape") => {
                let (arr, n) = self.arr(sh + 0xC0)?;
                let (sp, nsub) = self.arr(sh + 0x50)?;
                for k in 0..i64::from(n.max(0)) {
                    let part = match sp {
                        Some(sp) if k < i64::from(nsub) => self.i32(sp + 0x40 * k as u64 + 0x20)?,
                        _ => -1,
                    };
                    let Some(arr) = arr else { break };
                    let Some(st) = self.deref(arr + 4 * k as u64).filter(|&s| s != 0) else {
                        continue;
                    };
                    let (p, nv) = self.arr(st + 8)?;
                    let (ip, ni) = self.arr(st + 0x14)?;
                    let (Some(p), Some(ip)) = (p, ip) else { continue };
                    if nv <= 0 || ni < 4 {
                        continue;
                    }
                    let vb = self.buffer(p, i128::from(nv) * 4, 4)?;
                    let ib = self.buffer(ip, i128::from(ni - ni % 4), 2)?;
                    let v: Vec<[f64; 3]> = vb
                        .chunks_exact(16)
                        .map(|c| {
                            let f = |o: usize| f64::from(f32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]));
                            [f(0), f(4), f(8)]
                        })
                        .collect();
                    let idx: Vec<[usize; 3]> = ib
                        .chunks_exact(8)
                        .map(|c| {
                            let u = |o: usize| usize::from(u16::from_le_bytes([c[o], c[o + 1]]));
                            [u(0), u(2), u(4)]
                        })
                        .collect();
                    if idx.iter().flatten().any(|&i| i >= v.len()) {
                        continue;
                    }
                    let tris = idx.iter().map(|t| t.map(|i| add(v[i], offset))).collect();
                    out.push((KIND_MESH, tris, part));
                }
            }
            Some("hkpListShape") => {
                let (arr, n) = self.arr(sh + 0x18)?;
                if let Some(arr) = arr {
                    for k in 0..u64::try_from(n).unwrap_or(0) {
                        self.shape_parts(self.deref(arr + 16 * k), offset, out, depth + 1)?;
                    }
                }
            }
            Some("hkpConvexTranslateShape") => {
                let t = self.f32s::<3>(sh + 0x20)?;
                self.shape_parts(self.deref(sh + 0x18), add(offset, t), out, depth + 1)?;
            }
            Some("hkpConvexVerticesShape") => {
                let [radius] = self.f32s::<1>(sh + 0x10)?;
                let (vp, nb) = self.arr(sh + 0x40)?;
                let nverts = self.i32(sh + 0x4C)?;
                let (pp, npl) = self.arr(sh + 0x50)?;
                let (Some(vp), Some(pp)) = (vp, pp) else { return Ok(()) };
                if nb <= 0 || npl <= 0 || nb > 100_000 {
                    return Ok(());
                }
                let fv = self.buffer(vp, i128::from(nb) * 12, 4)?;
                let f = |o: usize| f64::from(f32::from_le_bytes([fv[o], fv[o + 1], fv[o + 2], fv[o + 3]]));
                let nb = nb as usize;
                let mut verts = Vec::with_capacity(nb * 4);
                for i in 0..nb {
                    for c in 0..4 {
                        verts.push([f(48 * i + 4 * c), f(48 * i + 16 + 4 * c), f(48 * i + 32 + 4 * c)]);
                    }
                }
                let keep = (i64::from(nverts).min(verts.len() as i64)).max(1) as usize;
                verts.truncate(keep);
                let pb = self.buffer(pp, i128::from(npl) * 4, 4)?;
                let planes: Vec<[f64; 4]> = pb
                    .chunks_exact(16)
                    .map(|c| {
                        let g = |o: usize| f64::from(f32::from_le_bytes([c[o], c[o + 1], c[o + 2], c[o + 3]]));
                        [g(0), g(4), g(8), g(12)]
                    })
                    .collect();
                if !radius.is_finite()
                    || verts.iter().any(|v| !finite3(*v))
                    || planes.iter().any(|p| p.iter().any(|x| !x.is_finite()))
                {
                    return Err(Stop::Python);
                }
                let t = hull_faces(&verts, &planes, radius)?;
                if !t.is_empty() {
                    out.push((
                        KIND_CONVEX,
                        t.iter().map(|tri| tri.map(|p| add(p, offset))).collect(),
                        -1,
                    ));
                }
            }
            Some("hkpBoxShape") => {
                let [radius] = self.f32s::<1>(sh + 0x10)?;
                let h = self.f32s::<3>(sh + 0x20)?;
                let t = box_faces(h, radius);
                out.push((
                    KIND_CONVEX,
                    t.iter().map(|tri| tri.map(|p| add(p, offset))).collect(),
                    -1,
                ));
            }
            _ => {}
        }
        Ok(())
    }
}

/// `_hull_faces(verts, planes, radius)` (the inputs finite).
fn hull_faces(verts: &[[f64; 3]], planes: &[[f64; 4]], radius: f64) -> R<Vec<[[f64; 3]; 3]>> {
    let mut tris = Vec::new();
    let top = verts
        .iter()
        .flatten()
        .fold(f64::NEG_INFINITY, |m, v| if v.abs() > m { v.abs() } else { m });
    let scale = if top > 1.0 { top } else { 1.0 };
    let k = verts.len();
    for n in planes {
        let nn = [n[0], n[1], n[2]];
        let ln = dot(nn, nn).sqrt();
        if ln < 1e-6 {
            continue;
        }
        let nn = [nn[0] / ln, nn[1] / ln, nn[2] / ln];
        let d = n[3] / ln;
        let mut on: Vec<[f64; 3]> = verts
            .iter()
            .filter(|v| (gemv(**v, nn, k) + d).abs() < 2e-3 * scale)
            .map(|v| v.map(|x| (x * 1e5).round_ties_even() / 1e5))
            .collect();
        if on.len() < 3 {
            continue;
        }
        // np.unique(axis=0): rows sorted (stable up to 17), the first of each run of equal rows kept
        let cmp = |a: &[f64; 3], b: &[f64; 3]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal))
                .find(|o| o.is_ne())
                .unwrap_or(std::cmp::Ordering::Equal)
        };
        let big = on.len() > 17;
        on.sort_by(cmp);
        let mut uniq: Vec<[f64; 3]> = Vec::with_capacity(on.len());
        for r in on {
            match uniq.last() {
                Some(last) if cmp(last, &r).is_eq() => {
                    if big && last.map(f64::to_bits) != r.map(f64::to_bits) {
                        return Err(Stop::Python);
                    }
                }
                _ => uniq.push(r),
            }
        }
        let on = uniq;
        if on.len() < 3 {
            continue;
        }
        let mut c = on[0];
        for r in &on[1..] {
            c = add(c, *r);
        }
        let m = on.len() as f64;
        let c = c.map(|x| x / m);
        let e = if nn[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let u = cross(nn, e);
        let lu = dot(u, u).sqrt();
        let u = u.map(|x| x / lu);
        let v = cross(nn, u);
        let km = on.len();
        let ang: Vec<f64> = on
            .iter()
            .map(|r| {
                let q = sub(*r, c);
                gemv(q, v, km).atan2(gemv(q, u, km))
            })
            .collect();
        if ang.iter().any(|a| !a.is_finite()) {
            return Err(Stop::Python);
        }
        let mut order: Vec<usize> = (0..km).collect();
        order.sort_by(|&a, &b| ang[a].partial_cmp(&ang[b]).unwrap_or(std::cmp::Ordering::Equal));
        if order.windows(2).any(|w| ang[w[0]] == ang[w[1]]) {
            return Err(Stop::Python); // NumPy's argsort orders ties its own way
        }
        let on: Vec<[f64; 3]> = order.iter().map(|&i| on[i]).collect();
        let off = nn.map(|x| x * radius);
        for j in 1..km - 1 {
            let mut t = [add(on[0], off), add(on[j], off), add(on[j + 1], off)];
            if dot(cross(sub(t[1], t[0]), sub(t[2], t[0])), nn) < 0.0 {
                t.swap(1, 2);
            }
            tris.push(t);
        }
    }
    Ok(tris)
}

/// `_box_faces(h, radius)`.
fn box_faces(h: [f64; 3], radius: f64) -> Vec<[[f64; 3]; 3]> {
    let h = h.map(|x| x + radius);
    let mut c = Vec::with_capacity(8);
    for x in [-h[0], h[0]] {
        for y in [-h[1], h[1]] {
            for z in [-h[2], h[2]] {
                c.push([x, y, z]);
            }
        }
    }
    const FACES: [[usize; 3]; 12] = [
        [0, 1, 3],
        [0, 3, 2],
        [4, 6, 7],
        [4, 7, 5],
        [0, 4, 5],
        [0, 5, 1],
        [2, 3, 7],
        [2, 7, 6],
        [0, 2, 6],
        [0, 6, 4],
        [1, 5, 7],
        [1, 7, 3],
    ];
    FACES
        .iter()
        .map(|f| {
            let t = [c[f[0]], c[f[1]], c[f[2]]];
            let nrm = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let mean = [0, 1, 2].map(|j| ((t[0][j] + t[1][j]) + t[2][j]) / 3.0);
            let s = (nrm[0] * mean[0] + nrm[1] * mean[1]) + nrm[2] * mean[2];
            if s < 0.0 { [t[0], t[2], t[1]] } else { t }
        })
        .collect()
}

/// `asset_bodies(path)` of the file's bytes: `Some(pieces)` (empty where the reference gives `[]`: no packfile,
/// or one `hkpack.load` refuses with a `ValueError`), `None` where the Python must answer (a `struct.error` or
/// `IndexError` while loading, an error outside `_shape_parts`' `except`, or a case the port does not take; see
/// the module's text).
#[must_use]
pub fn asset_bodies(data: &[u8]) -> Option<Vec<Piece>> {
    let pf = match Packfile::parse(data) {
        Ok(pf) => pf,
        Err(HavokError::NotPackfile | HavokError::Refused(_)) => return Some(Vec::new()),
        Err(_) => return None,
    };
    let by = pf.objects().iter().map(|(o, c)| (*o, c.as_str())).collect();
    let w = Walk { pf: &pf, d: data, by };
    let mut out = Vec::new();
    let systems: Vec<u64> = pf.objects_of("hkpPhysicsSystem").collect();
    for (si, sy) in systems.into_iter().enumerate() {
        let (arr, n) = w.arr(sy + 8).ok()?;
        let Some(arr) = arr else { continue };
        for k in 0..u64::try_from(n).unwrap_or(0) {
            let Some(b) = w.deref(arr + 4 * k).filter(|&b| b != 0) else {
                continue;
            };
            if w.class(b) != Some("hkpRigidBody") {
                continue;
            }
            let name = match w.deref(b + 0x70).filter(|&p| p != 0) {
                Some(p) => bc2_formats::havok::reflect::cstr(data, p)?,
                None => String::new(),
            };
            let m = w.buffer(b + 0xE0, 16, 4).ok()?;
            let m: Vec<f64> = m
                .chunks_exact(4)
                .map(|c| f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]])))
                .collect();
            let mut parts = Vec::new();
            match w.shape_parts(w.deref(b + 0x10), [0.0; 3], &mut parts, 0) {
                Ok(()) => {}
                Err(Stop::Skip) => continue,
                Err(Stop::Python) => return None,
            }
            let root = w.deref(b + 0x10).and_then(|r| w.class(r));
            let rk = if matches!(root, Some("hkpMoppBvTreeShape" | "hkpStorageExtendedMeshShape")) {
                KIND_MESH
            } else {
                KIND_CONVEX
            };
            for (kind, t, part) in parts {
                let tris: Vec<[[f64; 3]; 3]> = t
                    .iter()
                    .map(|tri| {
                        tri.map(|p| {
                            [0, 1, 2].map(|j| (fma(p[2], m[8 + j], fma(p[1], m[4 + j], p[0] * m[j])) + 0.0) + m[12 + j])
                        })
                    })
                    .collect();
                if tris.iter().flatten().any(|p| !finite3(*p)) {
                    return None;
                }
                out.push(Piece {
                    system: si,
                    name: name.clone(),
                    kind,
                    tris,
                    part,
                    root_kind: rk,
                });
            }
        }
    }
    Some(out)
}
