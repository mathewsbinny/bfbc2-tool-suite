//! A bounds-checked reading position in a byte slice: the format's varints, bytes and slices.

use super::error::{DbxError, Result};

/// A position in `data`. It may move past the end (as the reference does after a container's flag byte); every
/// read checks, so a read there is an error, never a panic.
#[derive(Debug, Clone)]
pub(crate) struct Cursor<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(data: &'a [u8], pos: usize) -> Self {
        Self { data, pos }
    }

    /// The current offset.
    pub(crate) fn pos(&self) -> usize {
        self.pos
    }

    /// Whether the position is at or past the end (the reference's `while pos < n` loop ends there).
    pub(crate) fn at_end(&self) -> bool {
        self.pos >= self.data.len()
    }

    /// Moves `n` bytes on without reading them.
    pub(crate) fn skip(&mut self, n: usize) {
        self.pos = self.pos.saturating_add(n);
    }

    /// One byte.
    pub(crate) fn byte(&mut self) -> Result<u8> {
        let b = *self
            .data
            .get(self.pos)
            .ok_or(DbxError::UnexpectedEnd { offset: self.pos })?;
        self.pos += 1;
        Ok(b)
    }

    /// The byte `offset` bytes after the current position, without moving.
    pub(crate) fn peek_at(&self, offset: usize) -> Result<u8> {
        let at = self.pos.saturating_add(offset);
        self.data.get(at).copied().ok_or(DbxError::UnexpectedEnd { offset: at })
    }

    /// An unsigned LEB128 varint (FORMAT.md 1). A value too large for 64 bits saturates at `u64::MAX`: the
    /// reference's unbounded integer is then only ever an index or a count that fails its own check.
    pub(crate) fn varint(&mut self) -> Result<u64> {
        let mut value: u64 = 0;
        let mut shift: u32 = 0;
        let mut overflow = false;
        loop {
            let b = self.byte()?;
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
                return Ok(if overflow { u64::MAX } else { value });
            }
            shift = shift.saturating_add(7);
        }
    }

    /// The bytes from `start` up to `start + len`, clamped to the data as a Python slice is (possibly empty).
    pub(crate) fn clamped(&self, start: usize, len: usize) -> &'a [u8] {
        let n = self.data.len();
        let s = start.min(n);
        let e = start.saturating_add(len).min(n);
        self.data.get(s..e).unwrap_or_default()
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn varints() {
        let mut c = Cursor::new(&[0x05, 0x80, 0x01, 0xFF, 0x7F], 0);
        assert_eq!(c.varint().unwrap(), 5);
        assert_eq!(c.varint().unwrap(), 128);
        assert_eq!(c.varint().unwrap(), 0x3FFF);
        assert!(c.at_end());
        assert_eq!(c.varint(), Err(DbxError::UnexpectedEnd { offset: 5 }));
    }

    #[test]
    fn varint_overflow_saturates() {
        let mut data = vec![0xFF; 10];
        data.push(0x01);
        assert_eq!(Cursor::new(&data, 0).varint().unwrap(), u64::MAX);
        // 64 bits exactly fit
        let mut max = vec![0xFF; 9];
        max.push(0x01);
        assert_eq!(Cursor::new(&max, 0).varint().unwrap(), u64::MAX);
        let mut fits = vec![0xFF; 9];
        fits.push(0x00);
        assert_eq!(Cursor::new(&fits, 0).varint().unwrap(), (1u64 << 63) - 1);
        // zero-valued continuation bytes past 64 bits do not overflow
        let mut zeros = vec![0x81];
        zeros.extend([0x80; 12]);
        zeros.push(0x00);
        assert_eq!(Cursor::new(&zeros, 0).varint().unwrap(), 1);
    }

    #[test]
    fn clamped_slices() {
        let c = Cursor::new(&[1, 2, 3], 0);
        assert_eq!(c.clamped(1, 4), &[2, 3]);
        assert_eq!(c.clamped(5, 2), &[] as &[u8]);
        assert_eq!(c.clamped(usize::MAX, 2), &[] as &[u8]);
    }

    #[test]
    fn skip_past_end_then_read_fails() {
        let mut c = Cursor::new(&[1], 0);
        c.skip(usize::MAX);
        assert!(c.at_end());
        assert!(c.byte().is_err());
        assert!(c.peek_at(1).is_err());
    }
}
