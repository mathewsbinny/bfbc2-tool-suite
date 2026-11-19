//! Xbox 360 soot meshes (`SootMesh`: a model's scorch overlay, `<mesh>_lod<N>_Soot.res`) in BFBC2's PC byte order: a
//! port of `bfbc2-console-convert/soot.py` (`soot_mesh`), round F3. The layout is in that module's docstring: the
//! same size on both platforms, every field swapped (32-bit words and floats, a vertex's four 16-bit values and its
//! 32-bit word, 16-bit indices, the 256-byte tail of words).
//!
//! The same bytes as the reference. Where the reference raises (a `SootError`, a read past the end, a stride of 0)
//! this answers [`Refused`] and the Python runs, so the error stays its own. Floats are swapped byte for byte, as
//! NumPy's byte-order cast does (a NaN's bits kept).
//!
//! ```
//! use bc2_convert::soot;
//!
//! let empty = [0, 0, 0, 18, 0, 0, 0, 12, 0, 0, 0, 0, 0, 0, 0, 0];
//! assert_eq!(soot::soot_mesh(&empty)?[..8], [18, 0, 0, 0, 12, 0, 0, 0]);
//! # Ok::<(), soot::Refused>(())
//! ```

/// The empty soot mesh's size.
pub const EMPTY: usize = 16;
/// The tail of 64 words after the indices.
pub const TAIL: usize = 256;

/// The mesh is left to the Python reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("left to the Python reference")
    }
}

impl std::error::Error for Refused {}

type Result<T> = std::result::Result<T, Refused>;

/// `swap(dt, n, at)`: `n` values of `size` bytes at `at` reversed in place in `out` (from `x`).
fn swap(x: &[u8], out: &mut [u8], size: usize, n: usize, at: usize) -> Result<()> {
    let end = n.checked_mul(size).and_then(|b| b.checked_add(at)).ok_or(Refused)?;
    let src = x.get(at..end).ok_or(Refused)?;
    let dst = out.get_mut(at..end).ok_or(Refused)?;
    for (d, s) in dst.chunks_exact_mut(size).zip(src.chunks_exact(size)) {
        d.copy_from_slice(s);
        d.reverse();
    }
    Ok(())
}

fn be32(x: &[u8], at: usize) -> Result<usize> {
    let w = x.get(at..at + 4).ok_or(Refused)?;
    Ok(u32::from_be_bytes(w.try_into().map_err(|_| Refused)?) as usize)
}

/// `soot.soot_mesh(x)`: an Xbox `SootMesh` (BFBC2's or Battlefield 1943's) as BFBC2's PC file.
///
/// # Errors
///
/// [`Refused`] where the reference raises: a field past the end, sizes that do not fit the file, vertices other than
/// 12 bytes.
pub fn soot_mesh(x: &[u8]) -> Result<Vec<u8>> {
    let mut out = vec![0u8; x.len()];
    if x.len() == EMPTY {
        swap(x, &mut out, 4, 4, 0)?;
        return Ok(out);
    }
    let n = be32(x, 0x10)?;
    swap(x, &mut out, 4, 5, 0)?;
    swap(x, &mut out, 4, n.checked_mul(6).ok_or(Refused)?, 0x14)?;
    let h = n.checked_mul(24).and_then(|v| v.checked_add(0x14)).ok_or(Refused)?;
    swap(x, &mut out, 4, 4, h)?;
    let (vb, ib) = (be32(x, h + 4)?, be32(x, h + 8)?);
    let o = h + 16;
    let stride = be32(x, 4)?;
    if stride == 0 || vb % stride != 0 || ib % 2 != 0 || o + vb + ib + TAIL != x.len() {
        return Err(Refused);
    }
    if stride != 12 {
        return Err(Refused);
    }
    for v in 0..vb / 12 {
        let at = o + 12 * v;
        swap(x, &mut out, 2, 4, at)?;
        swap(x, &mut out, 4, 1, at + 8)?;
    }
    swap(x, &mut out, 2, ib / 2, o + vb)?;
    swap(x, &mut out, 4, TAIL / 4, o + vb + ib)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    fn mesh(stride: u32) -> Vec<u8> {
        let mut x = Vec::new();
        for w in [18u32, stride, 0, 0, 1] {
            x.extend_from_slice(&w.to_be_bytes());
        }
        for f in [-1.0f32, -2.0, -3.0, 1.0, 2.0, 3.0] {
            x.extend_from_slice(&f.to_be_bytes());
        }
        for w in [1u32, 12, 6, 0] {
            x.extend_from_slice(&w.to_be_bytes());
        }
        x.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]);
        x.extend_from_slice(&[0, 1, 0, 2, 0, 3]);
        x.extend((0..TAIL).map(|k| k as u8));
        x
    }

    #[test]
    fn swaps_every_field() -> Result<()> {
        let pc = soot_mesh(&mesh(12))?;
        assert_eq!(&pc[..8], &[18, 0, 0, 0, 12, 0, 0, 0]);
        assert_eq!(&pc[0x14..0x18], &(-1.0f32).to_le_bytes());
        let v = 0x14 + 24 + 16;
        assert_eq!(&pc[v..v + 12], &[2, 1, 4, 3, 6, 5, 8, 7, 12, 11, 10, 9]);
        assert_eq!(&pc[v + 12..v + 18], &[1, 0, 2, 0, 3, 0]);
        Ok(())
    }

    #[test]
    fn refusals() {
        assert_eq!(soot_mesh(&mesh(16)), Err(Refused));
        assert_eq!(soot_mesh(&mesh(0)), Err(Refused));
        let m = mesh(12);
        assert_eq!(soot_mesh(&m[..m.len() - 1]), Err(Refused));
        assert_eq!(soot_mesh(&m[..0x12]), Err(Refused));
    }
}
