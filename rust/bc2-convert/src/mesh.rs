//! Xbox 360 -> PC mesh vertices, ported from `bfbc2-console-convert/mesh.py` (`convert`'s `emit` and `element`;
//! the format: that folder's `FORMAT.md` section 3): a run of console vertices re-laid in a PC vertex layout,
//! element by element by the two layouts' formats.
//!
//! | Python (`mesh.py`) | here |
//! |---|---|
//! | `convert`'s `emit(pairs, sc, sp, colour, src, first, count)` | [`convert_vertices`] |
//! | `element(fc, fp, raw, colour)` | [`element`] |
//! | `SIZE`, `SWAP` | [`size`], [`swap_width`] |
//!
//! The work is the reference's to the bit: byte swaps by the format's value width, and the two console-only packed
//! formats decoded to four halves, each value a double (`x / 1023`, `b / 255`) rounded to a half as NumPy rounds a
//! Python float ([`f64_to_f16`]: to nearest, ties to even, straight from the double). Those two formats take only
//! 2,048 + 1,024 + 256 distinct values, so `bfbc2-console-convert/tests/test_mesh_native.py` checks every one
//! against NumPy.
//!
//! Where the retired reference would not convert - a format it does not know (a `KeyError` or its
//! `ConvertError`), a value read past the end of the data (Python's short slice) or written past the PC vertex -
//! [`convert_vertices`] returns `None` and `mesh.py` raises its `ConvertError` saying which (the reference was
//! retired on 2026-11-18, its results recorded in `bfbc2-bf1943-import/tests/ports_golden.json`).
//!
//! ```
//! use bc2_convert::mesh::{self, Pair};
//!
//! // one vertex: a big-endian float3 position at 0 -> a little-endian float3 at 4 of a 16-byte PC vertex
//! let console = [0x3F, 0x80, 0, 0, 0x40, 0, 0, 0, 0xC0, 0, 0, 0];
//! let out = mesh::convert_vertices(&console, 0, 12, 16, 0, 1, &[Pair { fc: 3, oc: 0, fp: 3, op: 4 }], false);
//! let out = out.ok_or("not converted")?;
//! assert_eq!(out.len(), 16);
//! assert_eq!(out.get(4..8), Some(&1f32.to_le_bytes()[..]));
//! # Ok::<(), &str>(())
//! ```

/// A vertex element's size in bytes by its format (`mesh.py`'s `SIZE`); `None`: a format the reference does not
/// know.
#[must_use]
pub const fn size(format: u8) -> Option<usize> {
    match format {
        0x02 | 0x08 => Some(8),
        0x03 => Some(12),
        0x04 => Some(16),
        0x05 => Some(2),
        0x06 | 0x0C | 0x0D | 0x2B => Some(4),
        0x07 => Some(6),
        _ => None,
    }
}

/// The width of each value of a format both platforms share, byte-swapped value by value (`mesh.py`'s `SWAP`);
/// `None`: a format that is not just swapped.
#[must_use]
pub const fn swap_width(format: u8) -> Option<usize> {
    match format {
        0x02..=0x04 => Some(4),
        0x05..=0x08 => Some(2),
        0x0C => Some(1),
        _ => None,
    }
}

/// One element of a vertex: console format `fc` at byte `oc` of the console vertex becomes PC format `fp` at byte
/// `op` of the PC vertex.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pair {
    /// The console element's format.
    pub fc: u8,
    /// Its offset in the console vertex.
    pub oc: usize,
    /// The PC element's format.
    pub fp: u8,
    /// Its offset in the PC vertex.
    pub op: usize,
}

/// A double as an IEEE half's bits, rounded to nearest with ties to even directly from the double (NumPy's
/// `npy_double_to_half`, what `np.array([...], '<f2')` does to Python floats; one rounding, never through a
/// float).
#[must_use]
pub fn f64_to_f16(v: f64) -> u16 {
    let bits = v.to_bits();
    let sign = ((bits >> 48) & 0x8000) as u16;
    let exp = ((bits >> 52) & 0x7FF) as i32;
    let mant = bits & ((1u64 << 52) - 1);
    if exp == 0x7FF {
        // infinity, or a NaN (kept quiet, its top mantissa bits kept as NumPy does)
        return if mant == 0 {
            sign | 0x7C00
        } else {
            sign | 0x7E00 | ((mant >> 42) as u16 & 0x3FF)
        };
    }
    if exp == 0 {
        return sign; // zero or a double subnormal: far below the smallest half
    }
    let e = exp - 1023;
    if e > 15 {
        return sign | 0x7C00;
    }
    // the significand with its leading one, and how far it shifts down to the half's 10 bits
    let full = u128::from(mant | (1u64 << 52));
    let (shift, base) = if e >= -14 {
        (42u32, ((e + 15) as u128) << 10)
    } else {
        ((42 - 14 - e) as u32, 0)
    };
    if shift >= 64 + 52 {
        return sign;
    }
    let mut m = full >> shift;
    let rem = full & ((1u128 << shift) - 1);
    let half = 1u128 << (shift - 1);
    if rem > half || (rem == half && m & 1 == 1) {
        m += 1;
    }
    // for a normal half, m has its leading one at bit 10: adding the exponent field minus that one; a carry out of
    // the mantissa moves into the exponent (up to infinity), as the bit patterns are ordered
    let out = if e >= -14 { base + m - (1 << 10) } else { m };
    sign | (out.min(0x7C00) as u16)
}

/// `mesh.py`'s `_hend3n`: a signed normalized 11:11:10 value as three doubles.
fn hend3n(v: u32) -> [f64; 3] {
    let wrap = |x: u32, half: u32| -> f64 {
        let x = i64::from(x);
        let h = i64::from(half);
        f64::from(if x >= h { x - 2 * h } else { x } as i32)
    };
    let x = wrap(v & 0x7FF, 1024);
    let y = wrap((v >> 11) & 0x7FF, 1024);
    let z = wrap((v >> 22) & 0x3FF, 512);
    [(x / 1023.0).max(-1.0), (y / 1023.0).max(-1.0), (z / 511.0).max(-1.0)]
}

fn halves(out: &mut [u8], values: [f64; 4]) {
    let (pairs, _) = out.as_chunks_mut::<2>();
    for (dst, v) in pairs.iter_mut().zip(values) {
        *dst = f64_to_f16(v).to_le_bytes();
    }
}

/// One console element value `raw` (format `fc`, its [`size`]) as the PC's (format `fp`) written into `dst` (the
/// PC format's size): `mesh.py`'s `element`. `false`: a conversion the reference does not make (its
/// `ConvertError`), or sizes that are not the formats'.
#[must_use]
pub fn element(fc: u8, fp: u8, raw: &[u8], colour: bool, dst: &mut [u8]) -> bool {
    if size(fc) != Some(raw.len()) || size(fp) != Some(dst.len()) {
        return false;
    }
    if fc == fp
        && let Some(w) = swap_width(fc)
    {
        if fc == 0x0C && colour {
            for (d, s) in dst.iter_mut().zip(raw.iter().rev()) {
                *d = *s;
            }
        } else {
            for (d, s) in dst.chunks_exact_mut(w).zip(raw.chunks_exact(w)) {
                for (a, b) in d.iter_mut().zip(s.iter().rev()) {
                    *a = *b;
                }
            }
        }
        return true;
    }
    match (fc, fp, raw) {
        (0x2B, 0x08, &[b0, b1, b2, b3]) => {
            let [nx, ny, nz] = hend3n(u32::from_be_bytes([b0, b1, b2, b3]));
            halves(dst, [nx, ny, nz, 1.0]);
            true
        }
        (0x0D, 0x08, &[b0, b1, b2, b3]) => {
            let unit = |v: u8| f64::from(v) / 255.0;
            halves(dst, [unit(b3), unit(b2), unit(b1), unit(b0)]);
            true
        }
        _ => false,
    }
}

/// Console vertices `first .. first + count` of the region at byte `src` of `data` (stride `sc`), each converted
/// into a zeroed PC vertex of `sp` bytes by `pairs`: `mesh.py`'s `emit`, the bytes it appends. `None` where the
/// reference would not convert or would read short (the module's docs): the caller runs the reference then.
#[must_use]
#[expect(clippy::too_many_arguments, reason = "the reference's emit arguments")]
pub fn convert_vertices(
    data: &[u8],
    src: usize,
    sc: usize,
    sp: usize,
    first: usize,
    count: usize,
    pairs: &[Pair],
    colour: bool,
) -> Option<Vec<u8>> {
    if count == 0 {
        return Some(Vec::new());
    }
    // every element in bounds of its vertex and every conversion one the reference makes, checked once
    let mut plan = Vec::with_capacity(pairs.len());
    for p in pairs {
        let (zc, zp) = (size(p.fc)?, size(p.fp)?);
        if p.op.checked_add(zp)? > sp {
            return None;
        }
        let mut probe = vec![0u8; zp];
        if !element(p.fc, p.fp, &vec![0u8; zc], colour, &mut probe) {
            return None;
        }
        plan.push((*p, zc, zp));
    }
    let last = src.checked_add(first.checked_add(count)?.checked_mul(sc)?)?;
    let reach = plan.iter().map(|(p, zc, _)| p.oc + zc).max().unwrap_or(0);
    if last.checked_sub(sc)?.checked_add(reach)? > data.len() {
        return None;
    }
    let mut out = vec![0u8; count.checked_mul(sp)?];
    for (v, row) in out.chunks_exact_mut(sp).enumerate() {
        let base = src + (first + v) * sc;
        for (p, zc, zp) in &plan {
            let raw = data.get(base + p.oc..base + p.oc + zc)?;
            let dst = row.get_mut(p.op..p.op + zp)?;
            if !element(p.fc, p.fp, raw, colour, dst) {
                return None;
            }
        }
    }
    Some(out)
}

#[cfg(test)]
#[path = "mesh_tests.rs"]
mod tests;
