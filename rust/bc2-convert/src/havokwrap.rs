//! DICE's wrapper before the Havok packfile in a console `HavokPhysicsData` resource, in the PC's byte order: a port
//! of `bfbc2-console-convert/havok.py`'s `convert_wrapper` (FORMAT.md 1.1), round F3. [`crate::havok`] converts the
//! packfile after it.
//!
//! The wrapper: two words, as many part-name hashes as the second says, a 16-bit count of vectors (3 floats each), a
//! 16-bit count of boxes (6 floats each), a run of bytes kept as they are, then the word 1, a count `k` and `k`
//! hashes ending at the packfile. The byte run's length is where that last pattern holds; it must hold at exactly
//! one place.
//!
//! The same bytes as the reference. Where it raises (a `ConvertError`, a read past the end) this answers
//! [`Refused`] and the Python runs, so the error stays its own; so does a NaN float (`struct` converts the floats
//! through Python's own, and a NaN's bits through that are the interpreter's to define).
//!
//! ```
//! use bc2_convert::havokwrap;
//!
//! assert!(havokwrap::convert_wrapper(&[0; 8], 8).is_err());
//! ```

/// The wrapper is left to the Python reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("left to the Python reference")
    }
}

impl std::error::Error for Refused {}

type Result<T> = std::result::Result<T, Refused>;

struct Walk<'a> {
    b: &'a [u8],
    p: usize,
    out: Vec<u8>,
}

impl Walk<'_> {
    fn be(&self, size: usize, at: usize) -> Result<u32> {
        let s = self.b.get(at..at.checked_add(size).ok_or(Refused)?).ok_or(Refused)?;
        Ok(s.iter().fold(0u32, |v, &x| v << 8 | u32::from(x)))
    }

    /// `num(fmt, n)`: `n` values of `size` bytes copied reversed; floats refused when a NaN.
    fn num(&mut self, size: usize, n: usize, float: bool) -> Result<()> {
        let end = n.checked_mul(size).and_then(|v| v.checked_add(self.p)).ok_or(Refused)?;
        let src = self.b.get(self.p..end).ok_or(Refused)?;
        for v in src.chunks_exact(size) {
            if float && f32::from_be_bytes(v.try_into().map_err(|_| Refused)?).is_nan() {
                return Err(Refused);
            }
            self.out.extend(v.iter().rev());
        }
        self.p = end;
        Ok(())
    }
}

/// `havok.convert_wrapper(b, o)`: the wrapper before the packfile at `o`, console to PC.
///
/// # Errors
///
/// [`Refused`] where the reference raises (a read past the end, a byte run whose end is not unique), for an `o`
/// past the end of `b`, or a NaN float.
pub fn convert_wrapper(b: &[u8], o: usize) -> Result<Vec<u8>> {
    if o > b.len() {
        return Err(Refused);
    }
    let mut w = Walk {
        b,
        p: 0,
        out: Vec::new(),
    };
    w.be(4, 0)?;
    w.num(4, 2, false)?;
    let hashes = w.be(4, 4)? as usize;
    w.num(4, hashes, false)?;
    let vectors = w.be(2, w.p)? as usize;
    w.num(2, 1, false)?;
    w.num(4, 3 * vectors, true)?;
    let boxes = w.be(2, w.p)? as usize;
    w.num(2, 1, false)?;
    w.num(4, 6 * boxes, true)?;
    let mut tail = None;
    for at in w.p..o.saturating_sub(7) {
        if w.be(4, at)? == 1 && at as u64 + 8 + 4 * u64::from(w.be(4, at + 4)?) == o as u64 {
            if tail.is_some() {
                return Err(Refused);
            }
            tail = Some(at);
        }
    }
    let start = tail.ok_or(Refused)?;
    w.out.extend_from_slice(b.get(w.p..start).ok_or(Refused)?);
    w.p = start;
    let count = w.be(4, start + 4)? as usize;
    w.num(4, 2 + count, false)?;
    Ok(w.out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)]
    use super::*;

    fn wrapper() -> Vec<u8> {
        let mut b = Vec::new();
        for v in [3u32, 1, 0xAABB_CCDD] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b.extend_from_slice(&1u16.to_be_bytes());
        for f in [1.0f32, 2.0, 3.0] {
            b.extend_from_slice(&f.to_be_bytes());
        }
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&[9, 8, 7]);
        for v in [1u32, 2, 0x11, 0x22] {
            b.extend_from_slice(&v.to_be_bytes());
        }
        b
    }

    #[test]
    fn swaps_the_wrapper() -> Result<()> {
        let b = wrapper();
        let pc = convert_wrapper(&b, b.len())?;
        assert_eq!(pc.len(), b.len());
        assert_eq!(&pc[..4], &3u32.to_le_bytes());
        assert_eq!(&pc[14..18], &1.0f32.to_le_bytes());
        assert_eq!(&pc[28..31], &[9, 8, 7]);
        assert_eq!(&pc[pc.len() - 4..], &0x22u32.to_le_bytes());
        Ok(())
    }

    #[test]
    fn refusals() {
        let b = wrapper();
        assert_eq!(convert_wrapper(&b, b.len() - 1), Err(Refused));
        assert_eq!(convert_wrapper(&b, b.len() + 1), Err(Refused));
        let mut nan = b.clone();
        nan[14..18].copy_from_slice(&f32::NAN.to_be_bytes());
        assert_eq!(convert_wrapper(&nan, nan.len()), Err(Refused));
    }
}
