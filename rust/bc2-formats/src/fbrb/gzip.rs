//! gzip as the Python reference reads it, on a raw-deflate inflater ([`flate2::Decompress`]).
//!
//! A bundle has two gzip streams, and the reference reads them with two different standard-library readers whose
//! rules differ; each is reproduced here so that the same files are read and refused:
//!
//! - the header with `gzip.decompress` ([`gunzip_all`]): every member of the data, zero bytes between members
//!   skipped, the reserved flag bits and the header checksum not checked;
//! - the payload with `zlib.decompressobj(16 + MAX_WBITS)` fed 1 MiB at a time ([`Member`]): the first member only
//!   (what follows it is ignored), the reserved flag bits refused, the header checksum checked, and the trailer
//!   checked only once its eight bytes have been fed.

use flate2::{Crc, Decompress, FlushDecompress, Status};

use super::error::{FbrbError, Result};

const FTEXT_RESERVED: u8 = 0xE0;
const FHCRC: u8 = 0x02;
const FEXTRA: u8 = 0x04;
const FNAME: u8 = 0x08;
const FCOMMENT: u8 = 0x10;

fn gzip_err(msg: &str) -> FbrbError {
    FbrbError::Gzip(msg.to_owned())
}

fn deflate_err(e: &flate2::DecompressError) -> FbrbError {
    FbrbError::Gzip(format!("invalid deflate data: {e}"))
}

fn u16_le(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_le(b: &[u8], at: usize) -> Option<u32> {
    let s: [u8; 4] = b.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(s))
}

/// The length of the gzip member header at the start of `data` as Python's `gzip._read_gzip_header` reads it, or
/// `None` for no data at all (the end of `gzip.decompress`'s members).
fn python_header(data: &[u8]) -> Result<Option<usize>> {
    if data.is_empty() {
        return Ok(None);
    }
    if data.get(..2) != Some(&[0x1F, 0x8B][..]) {
        return Err(gzip_err("Not a gzipped file"));
    }
    let fixed = data.get(2..10).ok_or(FbrbError::GzipEnd)?;
    if fixed.first() != Some(&8) {
        return Err(gzip_err("Unknown compression method"));
    }
    let flag = fixed.get(1).copied().unwrap_or(0);
    let mut pos = 10usize;
    if flag & FEXTRA != 0 {
        let n = u16_le(data, pos).ok_or(FbrbError::GzipEnd)?;
        pos += 2 + usize::from(n);
        if pos > data.len() {
            return Err(FbrbError::GzipEnd);
        }
    }
    for bit in [FNAME, FCOMMENT] {
        if flag & bit != 0 {
            // up to and past the zero byte; the end of the data just ends the string
            let rest = data.get(pos..).unwrap_or_default();
            pos += rest.iter().position(|&b| b == 0).map_or(rest.len(), |i| i + 1);
        }
    }
    if flag & FHCRC != 0 {
        pos += 2;
        if pos > data.len() {
            return Err(FbrbError::GzipEnd);
        }
    }
    Ok(Some(pos))
}

/// Inflates one raw deflate stream from `input` onto `out`. Returns whether the stream ended and the input bytes it
/// used.
fn inflate_into(input: &[u8], out: &mut Vec<u8>) -> Result<(bool, usize)> {
    let mut dec = Decompress::new(false);
    loop {
        if out.capacity() - out.len() < 1 << 15 {
            out.reserve(out.len().max(1 << 16));
        }
        let (in_before, out_before) = (dec.total_in(), out.len());
        let used = usize::try_from(in_before).unwrap_or(usize::MAX);
        let status = dec
            .decompress_vec(input.get(used..).unwrap_or_default(), out, FlushDecompress::None)
            .map_err(|e| deflate_err(&e))?;
        if status == Status::StreamEnd {
            return Ok((true, usize::try_from(dec.total_in()).unwrap_or(usize::MAX)));
        }
        if dec.total_in() == in_before && out.len() == out_before {
            return Ok((false, used));
        }
    }
}

/// Python's `gzip.decompress(data)`: every member, in order (the reference reads a bundle's header with it).
pub(crate) fn gunzip_all(data: &[u8]) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = data;
    while let Some(head) = python_header(rest)? {
        let body = rest.get(head..).unwrap_or_default();
        let start = out.len();
        let (ended, used) = inflate_into(body, &mut out)?;
        let unused = body.get(used..).unwrap_or_default();
        let (Some(crc), Some(len), true) = (u32_le(unused, 0), u32_le(unused, 4), ended) else {
            return Err(FbrbError::GzipEnd);
        };
        let member = out.get(start..).unwrap_or_default();
        let mut sum = Crc::new();
        sum.update(member);
        if sum.sum() != crc {
            return Err(gzip_err("CRC check failed"));
        }
        if len != member.len() as u32 {
            return Err(gzip_err("Incorrect length of data produced"));
        }
        let after = unused.get(8..).unwrap_or_default();
        rest = after
            .get(after.iter().take_while(|&&b| b == 0).count()..)
            .unwrap_or_default();
    }
    Ok(out)
}

/// `data` as exactly one plain gzip member - a ten-byte header with no flags, the deflate stream, a trailer whose
/// CRC-32 and length match - followed by nothing, or (`zero_tail`) by zero bytes only; `None` for anything else.
///
/// The strict form every reader agrees on: FbrbTool's unpack (.NET's `GZipStream`) and the reference's readers may
/// differ on extra members, flags or bytes after the stream, so the in-process unpack ([`super::unpack`]) takes
/// only what this accepts and leaves everything else to FbrbTool.exe.
pub(crate) fn gunzip_plain(data: &[u8], zero_tail: bool) -> Option<Vec<u8>> {
    if data.get(..4)? != [0x1F, 0x8B, 8, 0] || data.len() < 10 {
        return None;
    }
    let body = data.get(10..)?;
    let mut out = Vec::new();
    let (ended, used) = inflate_into(body, &mut out).ok()?;
    let trailer = body.get(used..)?;
    let mut sum = Crc::new();
    sum.update(&out);
    if !ended || u32_le(trailer, 0)? != sum.sum() || u32_le(trailer, 4)? != out.len() as u32 {
        return None;
    }
    let rest = trailer.get(8..)?;
    (rest.is_empty() || (zero_tail && rest.iter().all(|&b| b == 0))).then_some(out)
}

/// The length of the gzip member header at the start of `data` as zlib's inflate reads it (`Ok(None)`: not all of
/// it is there yet).
fn zlib_header(data: &[u8]) -> Result<Option<usize>> {
    match data.get(..2) {
        None => return Ok(None),
        Some(m) if m != [0x1F, 0x8B] => return Err(gzip_err("incorrect header check")),
        Some(_) => {}
    }
    match data.get(2) {
        None => return Ok(None),
        Some(&m) if m != 8 => return Err(gzip_err("unknown compression method")),
        Some(_) => {}
    }
    let Some(&flag) = data.get(3) else { return Ok(None) };
    if flag & FTEXT_RESERVED != 0 {
        return Err(gzip_err("unknown header flags set"));
    }
    let mut pos = 10usize;
    if data.len() < pos {
        return Ok(None);
    }
    if flag & FEXTRA != 0 {
        let Some(n) = u16_le(data, pos) else { return Ok(None) };
        pos += 2 + usize::from(n);
        if data.len() < pos {
            return Ok(None);
        }
    }
    for bit in [FNAME, FCOMMENT] {
        if flag & bit != 0 {
            let Some(i) = data.get(pos..).and_then(|r| r.iter().position(|&b| b == 0)) else {
                return Ok(None);
            };
            pos += i + 1;
        }
    }
    if flag & FHCRC != 0 {
        let Some(want) = u16_le(data, pos) else { return Ok(None) };
        let mut sum = Crc::new();
        sum.update(data.get(..pos).unwrap_or_default());
        if sum.sum() & 0xFFFF != u32::from(want) {
            return Err(gzip_err("header crc mismatch"));
        }
        pos += 2;
    }
    Ok(Some(pos))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Header,
    Body,
    Trailer,
    Done,
}

/// One gzip member decompressed as it is fed, as `zlib.decompressobj(16 + zlib.MAX_WBITS)` does: each
/// [`Member::feed`] decompresses everything its input allows; what follows the member is ignored.
pub(crate) struct Member {
    state: State,
    pending: Vec<u8>,
    inflater: Decompress,
    crc: Crc,
    scratch: Vec<u8>,
}

impl Member {
    pub(crate) fn new() -> Self {
        Self {
            state: State::Header,
            pending: Vec::new(),
            inflater: Decompress::new(false),
            crc: Crc::new(),
            scratch: vec![0; 1 << 16],
        }
    }

    /// Whether the member and its checked trailer have been read.
    pub(crate) fn done(&self) -> bool {
        self.state == State::Done
    }

    /// Decompresses `input`, giving the output to `sink` in order.
    pub(crate) fn feed(&mut self, mut input: &[u8], sink: &mut impl FnMut(&[u8])) -> Result<()> {
        if self.state == State::Header {
            // a header split across inputs waits for the rest (zlib keeps its state the same way)
            self.pending.extend_from_slice(input);
            let Some(head) = zlib_header(&self.pending)? else {
                return Ok(());
            };
            let from_input = head - (self.pending.len() - input.len());
            input = input.get(from_input..).unwrap_or_default();
            self.pending.clear();
            self.state = State::Body;
        }
        if self.state == State::Body {
            loop {
                let (in_before, out_before) = (self.inflater.total_in(), self.inflater.total_out());
                let status = self
                    .inflater
                    .decompress(input, &mut self.scratch, FlushDecompress::None)
                    .map_err(|e| deflate_err(&e))?;
                let used = (self.inflater.total_in() - in_before) as usize;
                let made = (self.inflater.total_out() - out_before) as usize;
                let out = self.scratch.get(..made).unwrap_or_default();
                self.crc.update(out);
                sink(out);
                input = input.get(used..).unwrap_or_default();
                if status == Status::StreamEnd {
                    self.state = State::Trailer;
                    break;
                }
                if used == 0 && made == 0 {
                    return Ok(());
                }
            }
        }
        if self.state == State::Trailer {
            let need = 8 - self.pending.len();
            self.pending
                .extend_from_slice(input.get(..need.min(input.len())).unwrap_or_default());
            if let (Some(crc), Some(len)) = (u32_le(&self.pending, 0), u32_le(&self.pending, 4)) {
                if crc != self.crc.sum() {
                    return Err(gzip_err("incorrect data check"));
                }
                if len != self.crc.amount() {
                    return Err(gzip_err("incorrect length check"));
                }
                self.state = State::Done;
            }
        }
        Ok(())
    }
}

/// `data` as one gzip member (no file name, modification time 0), for the test bundles [`super::build`] writes.
pub(crate) fn gzip(data: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut enc = flate2::GzBuilder::new().write(Vec::new(), flate2::Compression::default());
    // writing to a Vec cannot fail
    if enc.write_all(data).is_err() {
        return Vec::new();
    }
    enc.finish().unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn member_of(data: &[u8], chunk: usize) -> Result<(Vec<u8>, bool)> {
        let mut m = Member::new();
        let mut out = Vec::new();
        for c in data.chunks(chunk.max(1)) {
            m.feed(c, &mut |b: &[u8]| out.extend_from_slice(b))?;
        }
        Ok((out, m.done()))
    }

    #[test]
    fn round_trip_any_split() {
        let text: Vec<u8> = (0..70_000u32).map(|i| (i * 7 % 251) as u8).collect();
        let z = gzip(&text);
        assert_eq!(gunzip_all(&z).unwrap(), text);
        for chunk in [1, 2, 7, 4096, z.len()] {
            assert_eq!(member_of(&z, chunk).unwrap(), (text.clone(), true));
        }
    }

    #[test]
    fn python_reader_rules() {
        let a = gzip(b"abc");
        // members joined, zero bytes between and after them skipped, no data is no members
        let mut two = a.clone();
        two.extend([0, 0, 0]);
        two.extend(&a);
        two.extend([0, 0]);
        assert_eq!(gunzip_all(&two).unwrap(), b"abcabc");
        assert_eq!(gunzip_all(b"").unwrap(), b"");
        // anything else after a member is refused
        let mut junk = a.clone();
        junk.extend(b"xx");
        assert!(matches!(gunzip_all(&junk), Err(FbrbError::Gzip(_))));
        assert!(matches!(gunzip_all(&a[..1]), Err(FbrbError::Gzip(_))));
        assert!(matches!(gunzip_all(&a[..5]), Err(FbrbError::GzipEnd)));
        assert!(matches!(gunzip_all(&a[..a.len() - 1]), Err(FbrbError::GzipEnd)));
        let mut bad_crc = a.clone();
        let n = bad_crc.len();
        bad_crc[n - 8] ^= 1;
        assert!(matches!(gunzip_all(&bad_crc), Err(FbrbError::Gzip(m)) if m.contains("CRC")));
        let mut bad_len = a.clone();
        bad_len[n - 4] ^= 1;
        assert!(matches!(gunzip_all(&bad_len), Err(FbrbError::Gzip(m)) if m.contains("length")));
        let mut method = a.clone();
        method[2] = 7;
        assert!(matches!(gunzip_all(&method), Err(FbrbError::Gzip(m)) if m.contains("method")));
        let mut deflate = a.clone();
        deflate[10] = 0xFF;
        assert!(matches!(gunzip_all(&deflate), Err(FbrbError::Gzip(_))));
    }

    /// A member with every optional header field: extra, name, comment and header checksum.
    fn full_header(body: &[u8], flags_extra: u8, good_hcrc: bool) -> Vec<u8> {
        let plain = gzip(body);
        let mut h = vec![
            0x1F,
            0x8B,
            8,
            FEXTRA | FNAME | FCOMMENT | FHCRC | flags_extra,
            0,
            0,
            0,
            0,
            0,
            0,
        ];
        h.extend([3, 0, b'x', b'y', b'z']);
        h.extend(b"name\0comment\0");
        let mut c = Crc::new();
        c.update(&h);
        let hcrc = (c.sum() & 0xFFFF) as u16 ^ u16::from(!good_hcrc);
        h.extend(hcrc.to_le_bytes());
        h.extend(&plain[10..]);
        h
    }

    #[test]
    fn optional_header_fields() {
        let z = full_header(b"hello", 0, true);
        assert_eq!(gunzip_all(&z).unwrap(), b"hello");
        for chunk in [1, 3, z.len()] {
            assert_eq!(member_of(&z, chunk).unwrap(), (b"hello".to_vec(), true));
        }
        // zlib checks the header checksum and the reserved flags; Python's gzip does not
        let bad = full_header(b"hello", 0, false);
        assert_eq!(gunzip_all(&bad).unwrap(), b"hello");
        assert!(matches!(member_of(&bad, 64), Err(FbrbError::Gzip(m)) if m.contains("header crc")));
        let reserved = full_header(b"hello", 0x20, true);
        assert_eq!(gunzip_all(&reserved).unwrap(), b"hello");
        assert!(matches!(member_of(&reserved, 64), Err(FbrbError::Gzip(m)) if m.contains("flags")));
        // Python's gzip: a name that runs to the end ends there (and then the deflate data is missing)
        assert!(matches!(gunzip_all(&z[..18]), Err(FbrbError::GzipEnd)));
        assert!(matches!(gunzip_all(&z[..11]), Err(FbrbError::GzipEnd)));
        let mut only_hcrc = vec![0x1F, 0x8B, 8, FHCRC, 0, 0, 0, 0, 0, 0, 0];
        assert!(matches!(gunzip_all(&only_hcrc), Err(FbrbError::GzipEnd)));
        only_hcrc.truncate(3);
        assert!(matches!(gunzip_all(&only_hcrc), Err(FbrbError::GzipEnd)));
    }

    #[test]
    fn member_rules() {
        let a = gzip(b"abcdef");
        // what follows the member is ignored
        let mut more = a.clone();
        more.extend(b"anything at all");
        assert_eq!(member_of(&more, 5).unwrap(), (b"abcdef".to_vec(), true));
        // a trailer not fed yet is not checked; a wrong one is refused once it is there
        let (out, done) = member_of(&a[..a.len() - 3], 64).unwrap();
        assert_eq!((out.as_slice(), done), (&b"abcdef"[..], false));
        let n = a.len();
        let mut crc = a.clone();
        crc[n - 8] ^= 1;
        assert!(matches!(member_of(&crc, 64), Err(FbrbError::Gzip(m)) if m.contains("data check")));
        let mut len = a.clone();
        len[n - 1] ^= 1;
        assert!(matches!(member_of(&len, 64), Err(FbrbError::Gzip(m)) if m.contains("length check")));
        for (i, v, what) in [(0, 0, "header check"), (2, 9, "method")] {
            let mut b = a.clone();
            b[i] = v;
            assert!(matches!(member_of(&b, 64), Err(FbrbError::Gzip(m)) if m.contains(what)));
        }
        // a header cut short waits for more
        assert_eq!(member_of(&a[..1], 64).unwrap(), (vec![], false));
        assert_eq!(member_of(&a[..2], 64).unwrap(), (vec![], false));
        assert_eq!(member_of(&a[..3], 64).unwrap(), (vec![], false));
        assert_eq!(member_of(&a[..9], 64).unwrap(), (vec![], false));
        let z = full_header(b"x", 0, true);
        for cut in [11, 13, 17, 26, 29] {
            assert_eq!(member_of(&z[..cut], 64).unwrap(), (vec![], false), "cut at {cut}");
        }
        let mut deflate = a.clone();
        deflate[10] = 0xFF;
        assert!(matches!(member_of(&deflate, 64), Err(FbrbError::Gzip(_))));
    }

    /// zlib refuses a back-reference before the start of the output ("invalid distance too far back"); so must the
    /// inflater (`miniz_oxide` reads it from a zeroed window instead: the reason for the zlib-rs backend).
    #[test]
    fn distance_too_far_back_is_refused() {
        // a fixed-Huffman block: length 3 at distance 1 before any literal, then the end of the block
        let raw = [0x03, 0x02, 0x00, 0x00];
        assert!(matches!(inflate_into(&raw, &mut Vec::new()), Err(FbrbError::Gzip(_))));
        let mut z = vec![0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 0];
        z.extend(raw);
        z.extend([0; 8]);
        assert!(gunzip_all(&z).is_err());
        assert!(member_of(&z, 64).is_err());
    }

    #[test]
    fn little_endian_helpers() {
        assert_eq!(u16_le(&[1, 2], 0), Some(0x0201));
        assert_eq!(u16_le(&[1], 0), None);
        assert_eq!(u16_le(&[1, 2], usize::MAX), None);
        assert_eq!(u32_le(&[1, 2, 3], 0), None);
    }
}
