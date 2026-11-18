//! The vertex work of `bfbc2-model-tools/skinmesh.py` (round F2a): a skinned LOD's parts cut out of a PC MeshData
//! (`SkinnedLod.__init__`), an element decoded (`decode`), the file's winding (`_winding`), new geometry written
//! in a part's layout (`_encode`) and the buffers laid out again (`to_bytes`). The part dicts, the layouts by name
//! and the geometry's tangents (`shapes.Geo.tangents`) stay with the Python class, which calls these.
//!
//! Values are the reference's to the bit: doubles cast to floats and halves rounded to nearest (ties to even,
//! straight from the double as NumPy casts), a normal's length `sqrt((x*x + y*y) + z*z)`, a cross product's
//! components `a1*b2 - a2*b1` (each product rounded, as `np.cross`), the winding sums `((x + y) + z)`. Where the
//! reference raises, these return `None` and the Python runs.

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::chunks_exact_to_as_chunks,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::approx_constant,
    clippy::option_option,
    clippy::type_complexity,
    clippy::similar_names,
    clippy::manual_bit_width,
    clippy::verbose_bit_mask,
    clippy::needless_range_loop,
    clippy::nonminimal_bool,
    clippy::result_unit_err,
    clippy::map_unwrap_or,
    clippy::format_push_string,
    clippy::unnested_or_patterns,
    clippy::unnecessary_wraps,
    clippy::doc_markdown,
    reason = "every index is checked against the data's length first (the walks' need(), the decoders' block counts, the plans' key counts); the reference's names, comparisons, casts and arithmetic as it has them (a float midpoint, an exact float compare and a table constant are its values, not approximations)"
)]

use crate::mesh::{f64_to_f16, size};
use crate::meshconv::Element;

/// Usages (`POS`, `WEIGHTS`, ...).
pub const POS: u8 = 1;
/// Blend weights.
pub const WEIGHTS: u8 = 3;
/// Blend indices.
pub const INDICES: u8 = 2;
/// Normal.
pub const NORMAL: u8 = 4;
/// Tangent.
pub const TANGENT: u8 = 5;
/// Binormal.
pub const BINORMAL: u8 = 6;
/// Texture coordinates.
pub const TEX: u8 = 0;
/// The position's w.
pub const POS_W: u8 = 7;

/// One part cut out of the file: its vertex rows (`rows`, `n` x stride bytes) and its triangles (indices into the
/// rows).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cut {
    /// The rows.
    pub rows: Vec<u8>,
    /// Their count.
    pub n: usize,
    /// The triangles' indices, three per triangle.
    pub idx: Vec<i64>,
}

/// A part's rows and triangles (`SkinnedLod.__init__`'s per-part step): `triangles * 3` 16-bit indices at
/// `ibase + first_index * 2`, the rows from `vbase + vertex_offset + lo * stride` to the highest index; `None` where
/// the reference raises (no triangles, data short).
#[must_use]
pub fn cut(b: &[u8], ibase: usize, vbase: usize, counts: [u32; 4], stride: usize) -> Option<Cut> {
    let [tri, _, first, vo] = counts.map(|v| v as usize);
    let at = ibase.checked_add(first.checked_mul(2)?)?;
    let raw = b.get(at..at.checked_add(tri.checked_mul(6)?)?)?;
    let idx: Vec<i64> = raw
        .chunks_exact(2)
        .map(|c| i64::from(u16::from_le_bytes([c[0], c[1]])))
        .collect();
    let (lo, hi) = (*idx.iter().min()?, *idx.iter().max()?);
    let n = (hi - lo + 1) as usize;
    let from = vbase.checked_add(vo)?.checked_add(lo as usize * stride)?;
    let rows = b.get(from..from.checked_add(n.checked_mul(stride)?)?)?.to_vec();
    if stride == 0 {
        return None;
    }
    Some(Cut {
        rows,
        n,
        idx: idx.iter().map(|v| v - lo).collect(),
    })
}

/// What `decode(p, usage)` gives: doubles (n x k) for float / half formats, or the element's bytes (n x size).
#[derive(Debug, Clone, PartialEq)]
pub enum Decoded {
    /// `astype(float)` of float3 or half values.
    Floats(usize, Vec<f64>),
    /// The raw bytes of each row's element.
    Bytes(usize, Vec<u8>),
}

/// `decode(p, usage)`: the first stream-0 element of the usage, or `Ok(None)` when there is none; `Err(())` where
/// the reference raises.
///
/// # Errors
/// Where the reference raises (an element past the row, an unknown format).
pub fn decode(rows: &[u8], n: usize, stride: usize, els: &[Element], usage: u8) -> Result<Option<Decoded>, ()> {
    let Some(&(f, _, off, _)) = els.iter().find(|e| e.1 == usage && e.3 == 0) else {
        return Ok(None);
    };
    let z = size(f).ok_or(())?;
    let off = usize::from(off);
    if off + z > stride || rows.len() != n * stride {
        return Err(());
    }
    let col = |r: usize| &rows[r * stride + off..r * stride + off + z];
    Ok(Some(match f {
        0x03 => Decoded::Floats(
            3,
            (0..n)
                .flat_map(|r| {
                    col(r)
                        .chunks_exact(4)
                        .map(|c| f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]])))
                })
                .collect(),
        ),
        0x05..=0x08 => Decoded::Floats(
            z / 2,
            (0..n)
                .flat_map(|r| {
                    col(r)
                        .chunks_exact(2)
                        .map(|c| crate::mesh::f16_to_f64(u16::from_le_bytes([c[0], c[1]])))
                })
                .collect(),
        ),
        _ => Decoded::Bytes(z, (0..n).flat_map(|r| col(r).to_vec()).collect()),
    }))
}

/// A part as `_winding` reads it: its rows, layout and triangles.
pub struct WindingPart<'a> {
    /// Rows.
    pub rows: &'a [u8],
    /// Their count.
    pub n: usize,
    /// Stride.
    pub stride: usize,
    /// The layout's elements.
    pub els: &'a [Element],
    /// Triangles' indices.
    pub idx: &'a [i64],
}

fn floats3(d: Option<Decoded>) -> Result<Option<(usize, Vec<f64>)>, ()> {
    match d {
        None => Ok(None),
        Some(Decoded::Floats(k, v)) if k >= 3 => Ok(Some((k, v))),
        // positions or normals of fewer than three values, or bytes: NumPy's arithmetic differs (left to it)
        Some(_) => Err(()),
    }
}

/// `_winding`: +1 when the file winds triangles counter-clockwise around their normals, else -1 (the majority over
/// the largest part with normals); `None` where the reference raises or computes on other types.
#[must_use]
pub fn winding(parts: &[WindingPart<'_>]) -> Option<i32> {
    let mut best: Option<(usize, Vec<f64>, usize, Vec<f64>, usize)> = None;
    for (i, p) in parts.iter().enumerate() {
        let nrm = floats3(decode(p.rows, p.n, p.stride, p.els, NORMAL).ok()?).ok()?;
        let pos = floats3(decode(p.rows, p.n, p.stride, p.els, POS).ok()?).ok()?;
        let (Some((kn, nv)), Some((kp, pv))) = (nrm, pos) else {
            continue;
        };
        let len = p.idx.len() / 3;
        if best.as_ref().is_none_or(|b| len > parts[b.0].idx.len() / 3) {
            best = Some((i, pv, kp, nv, kn));
        }
    }
    let Some((i, pos, kp, nrm, kn)) = best else {
        return Some(1);
    };
    let p = &parts[i];
    if !p.idx.len().is_multiple_of(3) {
        return None;
    }
    let (mut plus, mut minus) = (0usize, 0usize);
    let get = |v: &[f64], k: usize, at: i64, c: usize| -> Option<f64> {
        let at = usize::try_from(at).ok()?;
        v.get(at * k + c).copied().filter(|_| at < p.n)
    };
    for t in p.idx.chunks_exact(3) {
        let pt = |j: usize, c: usize| get(&pos, kp, t[j], c);
        let nm = |j: usize, c: usize| get(&nrm, kn, t[j], c);
        let ab: [f64; 3] = [pt(1, 0)? - pt(0, 0)?, pt(1, 1)? - pt(0, 1)?, pt(1, 2)? - pt(0, 2)?];
        let ac: [f64; 3] = [pt(2, 0)? - pt(0, 0)?, pt(2, 1)? - pt(0, 1)?, pt(2, 2)? - pt(0, 2)?];
        let cr = [
            ab[1] * ac[2] - ab[2] * ac[1],
            ab[2] * ac[0] - ab[0] * ac[2],
            ab[0] * ac[1] - ab[1] * ac[0],
        ];
        let ns: [f64; 3] = [
            (nm(0, 0)? + nm(1, 0)?) + nm(2, 0)?,
            (nm(0, 1)? + nm(1, 1)?) + nm(2, 1)?,
            (nm(0, 2)? + nm(1, 2)?) + nm(2, 2)?,
        ];
        let s = (cr[0] * ns[0] + cr[1] * ns[1]) + cr[2] * ns[2];
        plus += usize::from(s > 0.0);
        minus += usize::from(s < 0.0);
    }
    Some(if plus >= minus { 1 } else { -1 })
}

/// The geometry `_encode` writes (each n x 3 doubles but `uv`, n x 2), with each vertex's blend index (into the
/// part's palette) and the tangent frame `shapes.Geo.tangents` gives.
pub struct Geometry<'a> {
    /// Positions.
    pub pos: &'a [f64],
    /// Normals (normalised here).
    pub nrm: &'a [f64],
    /// Texture coordinates.
    pub uv: &'a [f64],
    /// Blend indices.
    pub bi: &'a [u8],
    /// Tangents.
    pub tan: &'a [f64],
    /// Binormals.
    pub bin: &'a [f64],
}

fn halves(dst: &mut [u8], v: &[f64]) {
    for (d, x) in dst.chunks_exact_mut(2).zip(v) {
        d.copy_from_slice(&f64_to_f16(*x).to_le_bytes());
    }
}

/// `_encode(p, geo, palette)`: rows for `n` vertices in a part's layout; `None` where the reference raises (an
/// element it does not write, an element past the row, arrays of other shapes).
#[must_use]
pub fn encode(els: &[Element], stride: usize, n: usize, g: &Geometry<'_>) -> Option<Vec<u8>> {
    if g.pos.len() != 3 * n
        || g.nrm.len() != 3 * n
        || g.uv.len() != 2 * n
        || g.bi.len() != n
        || g.tan.len() != 3 * n
        || g.bin.len() != 3 * n
    {
        return None;
    }
    let mut rows = vec![0u8; n * stride];
    for &(f, u, off, stream) in els {
        if stream != 0 {
            continue;
        }
        let off = usize::from(off);
        let width = match (u, f) {
            (POS, 0x03) => 12,
            (POS, 0x07) => 6,
            (POS, 0x08) | (NORMAL | TANGENT | BINORMAL, 0x08) => 8,
            (POS_W, 0x05) => 2,
            (INDICES | WEIGHTS, 0x0C) | (TEX, 0x06) => 4,
            _ => return None,
        };
        if off + width > stride {
            return None;
        }
        for r in 0..n {
            let dst = &mut rows[r * stride + off..r * stride + off + width];
            let three = |v: &[f64]| [v[3 * r], v[3 * r + 1], v[3 * r + 2]];
            match (u, f) {
                (POS, 0x03) => {
                    for (d, x) in dst.chunks_exact_mut(4).zip(three(g.pos)) {
                        d.copy_from_slice(&(x as f32).to_le_bytes());
                    }
                }
                (POS, 0x07) => halves(dst, &three(g.pos)),
                (POS, 0x08) => {
                    let p = three(g.pos);
                    halves(dst, &[p[0], p[1], p[2], 1.0]);
                }
                (POS_W, _) => halves(dst, &[1.0]),
                (INDICES, _) => dst.fill(g.bi[r]),
                (WEIGHTS, _) => dst[0] = 255,
                (NORMAL, _) => {
                    let v = three(g.nrm);
                    let len = ((v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]).sqrt();
                    let d = if len.is_nan() { len } else { len.max(1e-12) };
                    halves(dst, &[v[0] / d, v[1] / d, v[2] / d, 1.0]);
                }
                (TANGENT | BINORMAL, _) => {
                    let v = three(if u == TANGENT { g.tan } else { g.bin });
                    halves(dst, &[v[0], v[1], v[2], 1.0]);
                }
                _ => halves(dst, &[g.uv[2 * r], g.uv[2 * r + 1]]),
            }
        }
    }
    Some(rows)
}

/// A part as `to_bytes` lays it out: changed parts bring their rows (stride) and triangles.
pub struct OutPart<'a> {
    /// Changed: (rows, stride, triangles' indices).
    pub changed: Option<(&'a [u8], usize, &'a [i64])>,
}

/// `to_bytes`' buffers: the file's vertex and index buffers as they were, each changed part's vertices (from a
/// whole vertex of its stride) and 16-bit indices after them; per changed part (vertex offset, first index,
/// vertices, triangles). `None` where the reference raises (more than 65,536 vertices in a part).
#[must_use]
#[expect(clippy::type_complexity, reason = "the buffers and each part's four counts")]
pub fn lay_out(vb: &[u8], ib: &[u8], parts: &[OutPart<'_>]) -> Option<(Vec<u8>, Vec<u8>, Vec<Option<[usize; 4]>>)> {
    let (mut v, mut i) = (vb.to_vec(), ib.to_vec());
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        let Some((rows, stride, idx)) = p.changed else {
            out.push(None);
            continue;
        };
        if stride == 0 || !rows.len().is_multiple_of(stride) {
            return None;
        }
        let pad = (stride - v.len() % stride) % stride;
        v.resize(v.len() + pad, 0);
        let n = rows.len() / stride;
        if n as i64 - 1 > 0xFFFF {
            return None;
        }
        out.push(Some([v.len(), i.len() / 2, n, idx.len() / 3]));
        v.extend_from_slice(rows);
        for &x in idx {
            i.extend_from_slice(&(x as u16).to_le_bytes());
        }
    }
    Some((v, i, out))
}

/// `bounds()`: the minimum and maximum of every part's positions' x, y, z; `None` where the reference raises or a
/// position is a NaN (NumPy's result is then one NaN of its choosing).
#[must_use]
pub fn bounds(parts: &[(&[u8], usize, usize, &[Element])]) -> Option<[f64; 6]> {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    let mut any = false;
    for &(rows, n, stride, els) in parts {
        let Ok(Some(Decoded::Floats(k, v))) = decode(rows, n, stride, els, POS) else {
            return None;
        };
        if k < 3 {
            return None;
        }
        for r in 0..n {
            any = true;
            for c in 0..3 {
                let x = v[r * k + c];
                if x.is_nan() {
                    return None; // NumPy carries one NaN of its choosing (its payload): left to the reference
                }
                lo[c] = lo[c].min(x);
                hi[c] = hi[c].max(x);
            }
        }
    }
    any.then_some([lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_pads_to_whole_vertices() {
        let rows = [1u8; 8];
        let idx = [0i64, 1, 0];
        let (v, i, at) = lay_out(
            &[9; 5],
            &[],
            &[OutPart {
                changed: Some((&rows, 4, &idx)),
            }],
        )
        .unwrap_or_default();
        assert_eq!(v.len(), 16);
        assert_eq!(i, vec![0, 0, 1, 0, 0, 0]);
        assert_eq!(at, vec![Some([8, 0, 2, 1])]);
    }
}
