//! The DBX varint (FORMAT.md 1: 7 bits per byte, low group first, the high bit set on every byte but the last), one
//! implementation for every reader of binary `.dbx` data (round PR1b-2; there were six copies). What differs between
//! the readers is only what each does with a varint the game never writes - one past 64 bits, or past 32 for a
//! string index - so each rule is a function here:
//!
//! | reader | rule |
//! |---|---|
//! | `dbx` (the Python reference's unbounded integer) | [`read_saturating`]: past 64 bits saturates at `u64::MAX` |
//! | the cull tree's element stream, `culltool` | [`read_u64`]: past 64 bits is an error at the byte that overflows |
//! | DbxEdit (C#'s `Varint.Read`) | [`read_i32`]: a 32-bit `int` whose shifts wrap |
//! | `dbx::edit`'s strict reader (dbxbin) | [`read_u64`]'s value, refused past 32 bits by its caller |
//!
//! ```
//! use bc2_formats::varint;
//! let mut out = Vec::new();
//! varint::put(&mut out, 300);
//! assert_eq!(out, [0xAC, 0x02]);
//! assert_eq!(varint::read_u64(&out, 0), Ok((300, 2)));
//! assert_eq!(varint::read_i32(&[0xFF, 0xFF, 0xFF, 0xFF, 0x0F], &mut 0), Some(-1));
//! ```

/// Why [`read_u64`] did not read a varint: where it started.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VarintError {
    /// The data ends inside it.
    Truncated(usize),
    /// Its value does not fit 64 bits.
    TooLong(usize),
}

/// Appends `v` (the shortest form, as every writer of the suite and the game's own files write it).
pub fn put(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

/// The varint at `pos` and the position after it; an error at the byte that takes it past 64 bits.
///
/// # Errors
/// [`VarintError::Truncated`] when the data ends inside it, [`VarintError::TooLong`] past 64 bits.
pub fn read_u64(data: &[u8], mut pos: usize) -> Result<(u64, usize), VarintError> {
    let start = pos;
    let mut v = 0u64;
    let mut shift = 0u32;
    loop {
        let b = *data.get(pos).ok_or(VarintError::Truncated(start))?;
        pos += 1;
        let low = u64::from(b & 0x7f);
        if shift > 63 || (shift > 0 && low >> (64 - shift) != 0) {
            return Err(VarintError::TooLong(start));
        }
        v |= low << shift;
        if b < 0x80 {
            return Ok((v, pos));
        }
        shift += 7;
    }
}

/// The varint at `pos` and the position after it, saturating at `u64::MAX` past 64 bits (the whole varint is read);
/// `None` when the data ends inside it.
#[must_use]
pub fn read_saturating(data: &[u8], mut pos: usize) -> Option<(u64, usize)> {
    let mut value: u64 = 0;
    let mut shift: u32 = 0;
    let mut overflow = false;
    loop {
        let b = *data.get(pos)?;
        pos += 1;
        let part = u64::from(b & 0x7F);
        if shift < 64 {
            let shifted = part << shift;
            if shifted >> shift != part {
                overflow = true;
            }
            value |= shifted;
        } else if part != 0 {
            overflow = true;
        }
        if b < 0x80 {
            return Some((if overflow { u64::MAX } else { value }, pos));
        }
        shift = shift.saturating_add(7);
    }
}

/// C#'s `Varint.Read` (DbxEdit): a 32-bit `int` whose shifts past 31 bits wrap; `None` when the data ends inside it.
/// `pos` moves past it.
#[must_use]
pub fn read_i32(data: &[u8], pos: &mut usize) -> Option<i32> {
    let mut value: i32 = 0;
    let mut shift: u32 = 0;
    loop {
        let b = *data.get(*pos)?;
        *pos += 1;
        value |= i32::from(b & 0x7F).wrapping_shl(shift);
        if b < 0x80 {
            return Some(value);
        }
        shift = shift.wrapping_add(7);
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn the_rules() {
        for v in [0u64, 1, 127, 128, 300, 16_384, u64::from(u32::MAX), u64::MAX] {
            let mut b = Vec::new();
            put(&mut b, v);
            assert_eq!(read_u64(&b, 0), Ok((v, b.len())));
            assert_eq!(read_saturating(&b, 0), Some((v, b.len())));
        }
        let long = [0xFF; 10].iter().copied().chain([0x7F]).collect::<Vec<u8>>();
        assert_eq!(read_u64(&long, 0), Err(VarintError::TooLong(0)));
        assert_eq!(read_saturating(&long, 0), Some((u64::MAX, 11)));
        assert_eq!(read_u64(&[0x80], 0), Err(VarintError::Truncated(0)));
        assert_eq!(read_saturating(&[0x80], 0), None);
        assert_eq!(read_i32(&[0x80, 0x80, 0x80, 0x80, 0x08], &mut 0), Some(i32::MIN));
        assert_eq!(read_i32(&[0x80], &mut 0), None);
    }
}
