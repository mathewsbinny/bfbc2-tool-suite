//! FFmpeg's bit reader (`get_bits.h`, the checked "safe" reader every build uses) and writer (`put_bits.h`) as the
//! WMA Pro decoder uses them: big-endian, most significant bit first; every advance is clamped at the end plus 8
//! bits, and a read past the end sees whatever bytes follow (the frame buffer's earlier contents), as FFmpeg's does.

/// A reader over `buf`, `size` bits long (`init_get_bits`); bytes past `buf`'s end read as zero.
pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    index: usize,
    size: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(buf: &'a [u8], size_bits: usize) -> Self {
        Reader {
            buf,
            index: 0,
            size: size_bits,
        }
    }

    fn byte(&self, i: usize) -> u8 {
        self.buf.get(i).copied().unwrap_or(0)
    }

    /// `AV_RB32(buffer + (index >> 3)) << (index & 7)`: the reader's cache.
    fn cache(&self) -> u32 {
        let b = self.index >> 3;
        let v = u32::from_be_bytes([self.byte(b), self.byte(b + 1), self.byte(b + 2), self.byte(b + 3)]);
        v << (self.index & 7)
    }

    fn advance(&mut self, n: usize) {
        self.index = (self.index + n).min(self.size + 8);
    }

    /// `get_bits_count`.
    pub(crate) fn count(&self) -> usize {
        self.index
    }

    /// `get_bits(n)`, n at most 25.
    pub(crate) fn bits(&mut self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        let v = self.cache() >> (32 - n);
        self.advance(n as usize);
        v
    }

    /// `get_bitsz`: 0 bits read as 0.
    pub(crate) fn bitsz(&mut self, n: u32) -> u32 {
        self.bits(n)
    }

    /// `get_bits_long(n)`, n at most 32.
    pub(crate) fn bits_long(&mut self, n: u32) -> u32 {
        if n <= 25 {
            self.bits(n)
        } else {
            let hi = self.bits(16) << (n - 16);
            hi | self.bits(n - 16)
        }
    }

    /// `get_sbits(n)`.
    pub(crate) fn sbits(&mut self, n: u32) -> i32 {
        let v = (self.cache() as i32) >> (32 - n);
        self.advance(n as usize);
        v
    }

    /// `get_bits1`.
    pub(crate) fn bit(&mut self) -> u32 {
        let v = (u32::from(self.byte(self.index >> 3)) << (self.index & 7) >> 7) & 1;
        if self.index < self.size + 8 {
            self.index += 1;
        }
        v
    }

    /// `show_bits(n)`, n at most 25.
    pub(crate) fn show(&self, n: u32) -> u32 {
        if n == 0 {
            return 0;
        }
        self.cache() >> (32 - n)
    }

    /// The next 32 bits, left-aligned, for a code lookup.
    pub(crate) fn peek32(&self) -> u32 {
        let b = self.index >> 3;
        let v = u64::from_be_bytes([
            self.byte(b),
            self.byte(b + 1),
            self.byte(b + 2),
            self.byte(b + 3),
            self.byte(b + 4),
            0,
            0,
            0,
        ]);
        ((v << (self.index & 7)) >> 32) as u32
    }

    /// `skip_bits` (and the VLC reader's skips): clamped at the end plus 8.
    pub(crate) fn skip(&mut self, n: usize) {
        self.advance(n);
    }

    /// `skip_bits_long(n)`: `index += av_clip(n, -index, size_plus8 - index)`.
    pub(crate) fn skip_long(&mut self, n: i64) {
        let idx = self.index as i64;
        let hi = (self.size + 8) as i64 - idx;
        self.index = (idx + n.clamp(-idx, hi.max(-idx))) as usize;
    }

    /// The bytes from the reader's current byte on (`gb->buffer + (get_bits_count(gb) >> 3)`).
    pub(crate) fn tail(&self) -> &'a [u8] {
        self.buf.get(self.index >> 3..).unwrap_or(&[])
    }
}

/// The frame buffer (`frame_data`, `MAX_FRAMESIZE` + padding, zeroed once) and its writer (`PutBitContext`). Bits are
/// stored as they are put; a flush writes the last partial byte with zeros after the bits, as `flush_put_bits`
/// does; bytes past it keep what earlier frames left.
pub(crate) struct FrameBuf {
    pub(crate) data: Vec<u8>,
    bits: usize,
}

pub(crate) const MAX_FRAMESIZE: usize = 32768;
const PADDING: usize = 64;

impl FrameBuf {
    pub(crate) fn new() -> Self {
        FrameBuf {
            data: vec![0; MAX_FRAMESIZE + PADDING],
            bits: 0,
        }
    }

    /// `init_put_bits`.
    pub(crate) fn reset(&mut self) {
        self.bits = 0;
    }

    /// `put_bits_count`.
    pub(crate) fn count(&self) -> usize {
        self.bits
    }

    /// `put_bits_left`.
    pub(crate) fn left(&self) -> usize {
        MAX_FRAMESIZE * 8 - self.bits
    }

    /// `put_bits(n, v)`.
    pub(crate) fn put(&mut self, n: u32, v: u32) {
        for k in (0..n).rev() {
            let bit = (v >> k) & 1;
            let (byte, sh) = (self.bits >> 3, 7 - (self.bits & 7));
            if let Some(b) = self.data.get_mut(byte) {
                *b = (*b & !(1 << sh)) | ((bit as u8) << sh);
            }
            self.bits += 1;
        }
    }

    /// `ff_copy_bits(pb, src, length)`: the first `length` bits of `src` (bytes past `src` read as zero).
    pub(crate) fn copy_bits(&mut self, src: &[u8], length: usize) {
        let whole = length / 8;
        if self.bits.is_multiple_of(8) {
            let at = self.bits >> 3;
            for i in 0..whole {
                if let Some(d) = self.data.get_mut(at + i) {
                    *d = src.get(i).copied().unwrap_or(0);
                }
            }
            self.bits += whole * 8;
        } else {
            for i in 0..whole {
                self.put(8, u32::from(src.get(i).copied().unwrap_or(0)));
            }
        }
        let rest = (length & 7) as u32;
        if rest > 0 {
            let b = u32::from(src.get(whole).copied().unwrap_or(0));
            self.put(rest, b >> (8 - rest));
        }
    }

    /// `flush_put_bits` on a copy: the last partial byte's unused bits zeroed.
    pub(crate) fn flush(&mut self) {
        if self.bits & 7 != 0 {
            let used = self.bits & 7;
            if let Some(b) = self.data.get_mut(self.bits >> 3) {
                *b &= 0xFFu8 << (8 - used);
            }
        }
    }
}

/// A prefix code built as `ff_vlc_init_from_lengths` builds it: the codes given in order, each the previous plus one
/// at its length (left-aligned in 32 bits), the symbol plus `offset`. Every table the decoder uses is complete, so a
/// lookup always finds its code.
pub(crate) struct Vlc {
    starts: Vec<u64>,
    lens: Vec<u8>,
    syms: Vec<i32>,
}

impl Vlc {
    pub(crate) fn new(lens: &[u8], syms: &[i32], offset: i32) -> Self {
        let mut code: u64 = 0;
        let mut v = Vlc {
            starts: Vec::with_capacity(lens.len()),
            lens: Vec::with_capacity(lens.len()),
            syms: Vec::with_capacity(lens.len()),
        };
        for (&l, &s) in lens.iter().zip(syms) {
            if l == 0 {
                continue;
            }
            v.starts.push(code);
            v.lens.push(l);
            v.syms.push(s + offset);
            code += 1u64 << (32 - u32::from(l));
        }
        debug_assert_eq!(code, 1u64 << 32, "a complete code");
        v
    }

    /// `get_vlc2`: the symbol, its code's bits skipped.
    pub(crate) fn read(&self, r: &mut Reader<'_>) -> i32 {
        let v = u64::from(r.peek32());
        let i = self.starts.partition_point(|&s| s <= v).saturating_sub(1);
        let (len, sym) = (
            self.lens.get(i).copied().unwrap_or(0),
            self.syms.get(i).copied().unwrap_or(-1),
        );
        r.skip(usize::from(len));
        sym
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn reader_clamps_and_reads_msb_first() {
        let b = [0b1010_1100u8, 0xFF, 0x00, 0x80];
        let mut r = Reader::new(&b, 12);
        assert_eq!(r.bit(), 1);
        assert_eq!(r.bits(3), 0b010);
        assert_eq!(r.show(4), 0b1100);
        assert_eq!(r.bits(8), 0b1100_1111);
        assert_eq!(r.sbits(4), -1);
        assert_eq!(r.count(), 16);
        r.skip(100);
        assert_eq!(r.count(), 20, "clamped at the size plus 8");
        r.skip_long(-30);
        assert_eq!(r.count(), 0);
        let mut r = Reader::new(&b, 32);
        assert_eq!(r.bits_long(32), 0xACFF_0080);
    }

    #[test]
    fn writer_copies_bits_at_any_offset() {
        let mut f = FrameBuf::new();
        f.put(3, 0b101);
        f.copy_bits(&[0xAB, 0xCD, 0xEF], 20);
        f.flush();
        let mut r = Reader::new(&f.data, f.count());
        assert_eq!(r.bits(3), 0b101);
        assert_eq!(r.bits(20), 0xABCDE);
        assert_eq!(r.bits(1), 0, "flushed with zeros");
        f.reset();
        f.copy_bits(&[0x12, 0x34], 12);
        f.flush();
        assert_eq!(
            &f.data[..3],
            &[0x12, 0x30, 0xBC],
            "the partial byte flushed with zeros; the byte after kept"
        );
    }

    #[test]
    fn vlc_from_lengths() {
        // codes 0, 10, 110, 111 for lengths 1, 2, 3, 3
        let v = Vlc::new(&[1, 2, 3, 3], &[7, 8, 9, 10], -1);
        let b = [0b0101_1011u8, 0b1000_0000];
        let mut r = Reader::new(&b, 16);
        assert_eq!(v.read(&mut r), 6);
        assert_eq!(v.read(&mut r), 7);
        assert_eq!(v.read(&mut r), 8);
        assert_eq!(v.read(&mut r), 9);
        assert_eq!(r.count(), 9);
    }
}
