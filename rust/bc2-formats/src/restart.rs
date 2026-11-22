//! Restart points in a raw deflate stream (round PR1b-1-rest): read any part of a compressed payload without
//! decompressing it from its start, and without keeping the decompressed payload.
//!
//! A deflate stream can only be decoded from a block boundary, and a block's back references reach up to 32 KiB of
//! output behind it. [`build`] decompresses a stream once, start to end, and at the first block boundary after every
//! `span` bytes of output records a [`Point`]: how many compressed bytes were used (and how many bits of the last
//! one), how many bytes were made, and the 32 KiB window behind it. From a point, an inflater primed with the bits
//! left over (`inflatePrime`) and given the window as its dictionary (`inflateSetDictionary`) decodes exactly the
//! bytes the whole stream gives from there: zlib's `zran.c` method. Each point also keeps the CRC-32 of its span (the
//! output up to the next point), and [`decode_span`] checks it, so a span decoded from changed or damaged input is
//! refused, never returned as other bytes.
//!
//! An [`Index`] is a few percent of the payload (a window per point, stored deflated) and is written to disk by its
//! users ([`Index::to_bytes`]): the bundle cache's `.points` files ([`crate::fbrb::cache`]) and the BF1943 source's
//! ([`crate::bf1943`]). [`Spans`] keeps decoded spans in memory, the least recently used dropped first within a byte
//! budget.
//!
//! The inflater is zlib-rs (the one `flate2` already uses) through its C interface (`libz-rs-sys`): `flate2` has no
//! `Z_BLOCK` stops, `data_type` or `inflatePrime`.
//!
//! ```
//! use bc2_formats::restart;
//! use std::io::Write as _;
//!
//! let data: Vec<u8> = (0..400_000u32).map(|i| (i * 31 % 251) as u8).collect();
//! let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
//! z.write_all(&data).unwrap();
//! let z = z.finish().unwrap();
//! let mut out = Vec::new();
//! let built = restart::build(&mut z.as_slice(), 64 << 10, &mut |b: &[u8]| out.extend_from_slice(b))?;
//! assert_eq!(out, data);
//! assert_eq!(built.index.output_len, data.len() as u64);
//! let got = restart::read(&built.index, &mut std::io::Cursor::new(&z), 0, 300_000, 5_000, None)?;
//! assert_eq!(got, &data[300_000..305_000]);
//! # Ok::<(), restart::RestartError>(())
//! ```

use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::{Arc, Mutex, PoisonError};

/// The default output between restart points: 1 MiB (BF1943's source used the same, `bf1943import.SPAN`).
pub const SPAN: u64 = 1 << 20;
/// deflate's window: the most output a back reference reaches.
pub const WINDOW: usize = 32 << 10;
/// The compressed bytes read at a time while an index is built.
const CHUNK: usize = 1 << 16;
/// An index's bytes start with this (the format's version is in it).
const MAGIC: &[u8; 8] = b"BC2RSTP1";

/// Why a stream could not be indexed or read.
#[derive(Debug, thiserror::Error)]
pub enum RestartError {
    /// Reading the compressed bytes failed.
    #[error("{0}")]
    Io(#[from] io::Error),
    /// The deflate data is damaged (zlib's message).
    #[error("invalid deflate data: {0}")]
    Data(String),
    /// The compressed bytes end before the stream does.
    #[error("the deflate stream ends early")]
    Ends,
    /// A span decoded to other bytes than when its index was made (the compressed bytes changed).
    #[error("the compressed bytes changed after the restart points were made")]
    Changed,
    /// An index's bytes are not an index of this format.
    #[error("not a restart index")]
    Format,
}

type Result<T> = std::result::Result<T, RestartError>;

/// zlib's C interface, made safe for the few calls used here.
#[allow(
    unsafe_code,
    reason = "zlib's C interface (libz-rs-sys): the stream is boxed, every pointer is a live slice"
)]
mod z {
    use libz_rs_sys as sys;
    use std::ffi::c_int;

    /// `Z_BLOCK`: stop at a block boundary.
    const Z_BLOCK: c_int = 5;
    const Z_NO_FLUSH: c_int = 0;

    /// One `inflate` call's result.
    pub(super) struct Step {
        pub used: usize,
        pub made: usize,
        /// The stream's last block ended.
        pub end: bool,
        /// `data_type`: bit 7 at a block boundary, bit 6 after the last block, bits 0-2 the unused bits of the last
        /// byte used.
        pub data_type: i32,
    }

    /// A raw (headerless) inflater.
    pub(super) struct Inflater {
        strm: Box<sys::z_stream>,
    }

    impl Inflater {
        pub(super) fn raw() -> Option<Self> {
            let mut strm = Box::new(sys::z_stream::default());
            // SAFETY: strm is a valid, boxed (never moved) z_stream with the default allocator; the version string
            // is zlib-rs's own, NUL-terminated.
            let ret = unsafe {
                sys::inflateInit2_(
                    &raw mut *strm,
                    -15,
                    sys::zlibVersion(),
                    size_of::<sys::z_stream>() as c_int,
                )
            };
            (ret == sys::Z_OK).then_some(Self { strm })
        }

        /// `inflatePrime(bits, value)`.
        pub(super) fn prime(&mut self, bits: u8, value: u8) -> bool {
            // SAFETY: the stream was initialised by raw().
            unsafe { sys::inflatePrime(&raw mut *self.strm, c_int::from(bits), c_int::from(value)) == sys::Z_OK }
        }

        /// `inflateSetDictionary(window)`.
        pub(super) fn set_dictionary(&mut self, window: &[u8]) -> bool {
            if window.is_empty() {
                return true;
            }
            // SAFETY: the stream was initialised; the pointer and length are a live slice.
            unsafe {
                sys::inflateSetDictionary(&raw mut *self.strm, window.as_ptr(), window.len() as sys::uInt) == sys::Z_OK
            }
        }

        /// `inflateGetDictionary`: the window (the last 32 KiB of output at most).
        pub(super) fn dictionary(&mut self) -> Vec<u8> {
            let mut out = vec![0u8; super::WINDOW];
            let mut len: sys::uInt = 0;
            // SAFETY: the stream was initialised; out has room for a whole window (zlib never writes more).
            let ret = unsafe { sys::inflateGetDictionary(&raw const *self.strm, out.as_mut_ptr(), &raw mut len) };
            out.truncate(if ret == sys::Z_OK { len as usize } else { 0 });
            out
        }

        /// One `inflate` call from `input` onto `output` (`block`: stop at the next block boundary).
        pub(super) fn inflate(&mut self, input: &[u8], output: &mut [u8], block: bool) -> Result<Step, String> {
            let avail_in = input.len().min(u32::MAX as usize);
            let avail_out = output.len().min(u32::MAX as usize);
            self.strm.next_in = input.as_ptr();
            self.strm.avail_in = avail_in as sys::uInt;
            self.strm.next_out = output.as_mut_ptr();
            self.strm.avail_out = avail_out as sys::uInt;
            // SAFETY: next_in / next_out point into the live slices above, with their lengths.
            let ret = unsafe { sys::inflate(&raw mut *self.strm, if block { Z_BLOCK } else { Z_NO_FLUSH }) };
            let used = avail_in - self.strm.avail_in as usize;
            let made = avail_out - self.strm.avail_out as usize;
            let message = if self.strm.msg.is_null() {
                String::new()
            } else {
                // SAFETY: zlib's messages are static NUL-terminated strings.
                unsafe { std::ffi::CStr::from_ptr(self.strm.msg) }
                    .to_string_lossy()
                    .into_owned()
            };
            self.strm.next_in = std::ptr::null();
            self.strm.next_out = std::ptr::null_mut();
            self.strm.avail_in = 0;
            self.strm.avail_out = 0;
            match ret {
                sys::Z_OK | sys::Z_BUF_ERROR => Ok(Step {
                    used,
                    made,
                    end: false,
                    data_type: self.strm.data_type,
                }),
                sys::Z_STREAM_END => Ok(Step {
                    used,
                    made,
                    end: true,
                    data_type: self.strm.data_type,
                }),
                sys::Z_NEED_DICT => Err("a preset dictionary is needed".to_owned()),
                _ => Err(if message.is_empty() {
                    format!("zlib error {ret}")
                } else {
                    message
                }),
            }
        }
    }

    impl Drop for Inflater {
        fn drop(&mut self) {
            // SAFETY: the stream was initialised by raw() and is ended once.
            unsafe {
                sys::inflateEnd(&raw mut *self.strm);
            }
        }
    }
}

/// A place the stream can be decoded from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Point {
    /// Compressed bytes used up to here (from the deflate stream's start).
    pub input: u64,
    /// Bits of the last of those bytes not yet used (0-7): they start the next block.
    pub bits: u8,
    /// Output bytes made up to here.
    pub output: u64,
    /// The length of the output's last 32 KiB before here (less near the start).
    pub window_len: u32,
    /// That window, deflated (kept so in memory too: a window is inflated only when its span is decoded).
    pub packed: Vec<u8>,
    /// CRC-32 of the output from here to the next point (or the end).
    pub crc: u32,
}

/// A stream's restart points: the first at its start, then one at the first block boundary after every
/// [`Index::span`] bytes of output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Index {
    /// The output between points the index was made with.
    pub span: u64,
    /// The points, by output.
    pub points: Vec<Point>,
    /// The deflate stream's compressed length (up to the end of its last block).
    pub input_len: u64,
    /// The stream's whole output.
    pub output_len: u64,
    /// CRC-32 of the whole output.
    pub crc: u32,
}

/// What [`build`] made: the index, and the compressed bytes it read past the stream's end (the start of what
/// follows it: a gzip trailer).
#[derive(Debug, Clone)]
pub struct Built {
    /// The index.
    pub index: Index,
    /// Bytes read after the stream's last block.
    pub after: Vec<u8>,
}

/// Decompresses the raw deflate stream `input` start to end, giving the output to `sink` in order, and makes its
/// restart points `span` bytes of output apart (at least: at the next block boundary).
///
/// # Errors
/// [`RestartError::Data`] for damaged deflate data, [`RestartError::Ends`] when the input ends before the last
/// block, [`RestartError::Io`] when reading fails.
pub fn build(input: &mut impl Read, span: u64, sink: &mut impl FnMut(&[u8])) -> Result<Built> {
    let span = span.max(WINDOW as u64);
    let mut z = z::Inflater::raw().ok_or_else(|| RestartError::Data("inflateInit2 failed".to_owned()))?;
    let mut points = vec![Point {
        input: 0,
        bits: 0,
        output: 0,
        window_len: 0,
        packed: pack(&[]),
        crc: 0,
    }];
    let mut span_crc = flate2::Crc::new();
    let mut all_crc = flate2::Crc::new();
    let mut inbuf = vec![0u8; CHUNK];
    let mut outbuf = vec![0u8; 4 * WINDOW];
    let (mut total_in, mut total_out) = (0u64, 0u64);
    // after the last block, inflate is called without Z_BLOCK (which would stop it again before the stream's end)
    let mut last = false;
    loop {
        let n = read_some(input, &mut inbuf)?;
        let mut chunk = inbuf.get(..n).unwrap_or_default();
        // at the input's end, one call more: the last block may have ended exactly on it
        let mut final_call = n == 0;
        if final_call && !last {
            return Err(RestartError::Ends);
        }
        while !chunk.is_empty() || final_call {
            final_call = false;
            let step = z.inflate(chunk, &mut outbuf, !last).map_err(RestartError::Data)?;
            let made = outbuf.get(..step.made).unwrap_or_default();
            span_crc.update(made);
            all_crc.update(made);
            sink(made);
            total_in += step.used as u64;
            total_out += step.made as u64;
            chunk = chunk.get(step.used..).unwrap_or_default();
            if step.end {
                if let Some(p) = points.last_mut() {
                    p.crc = span_crc.sum();
                }
                return Ok(Built {
                    index: Index {
                        span,
                        points,
                        input_len: total_in,
                        output_len: total_out,
                        crc: all_crc.sum(),
                    },
                    after: chunk.to_vec(),
                });
            }
            if n == 0 {
                return Err(RestartError::Ends);
            }
            // data_type: bit 6 while the last block is decoded (from then on no Z_BLOCK, which would stop it again
            // before the stream's end), bit 7 at a block boundary, bits 0-2 the unused bits of the last byte
            if step.data_type & 64 != 0 {
                last = true;
            } else if step.data_type & 128 != 0 && total_out - points.last().map_or(0, |p| p.output) >= span {
                if let Some(p) = points.last_mut() {
                    p.crc = span_crc.sum();
                }
                span_crc = flate2::Crc::new();
                let window = z.dictionary();
                points.push(Point {
                    input: total_in,
                    bits: (step.data_type & 7) as u8,
                    output: total_out,
                    window_len: window.len() as u32,
                    packed: pack(&window),
                    crc: 0,
                });
            }
            if step.used == 0 && step.made == 0 {
                break; // needs more input than this chunk holds
            }
        }
    }
}

/// A window deflated (fast: windows are packed as the index is made).
fn pack(window: &[u8]) -> Vec<u8> {
    use std::io::Write as _;
    let mut e = flate2::write::DeflateEncoder::new(Vec::with_capacity(window.len() / 2), flate2::Compression::fast());
    // writing to a Vec cannot fail
    e.write_all(window).and_then(|()| e.finish()).unwrap_or_default()
}

impl Point {
    /// The window, inflated.
    ///
    /// # Errors
    /// [`RestartError::Format`] when the packed bytes do not inflate to the window's length.
    pub fn window(&self) -> Result<Vec<u8>> {
        let mut window = Vec::with_capacity(self.window_len as usize);
        flate2::read::DeflateDecoder::new(self.packed.as_slice())
            .take(u64::from(self.window_len) + 1)
            .read_to_end(&mut window)
            .map_err(|_| RestartError::Format)?;
        if window.len() != self.window_len as usize {
            return Err(RestartError::Format);
        }
        Ok(window)
    }
}

fn read_some(r: &mut impl Read, buf: &mut [u8]) -> io::Result<usize> {
    loop {
        match r.read(buf) {
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            other => return other,
        }
    }
}

impl Index {
    /// The point a read of output `offset` starts from: the last at or before it.
    #[must_use]
    pub fn point_for(&self, offset: u64) -> usize {
        self.points.partition_point(|p| p.output <= offset).saturating_sub(1)
    }

    /// The output range `[start, end)` of span `j`.
    #[must_use]
    pub fn span_range(&self, j: usize) -> (u64, u64) {
        let start = self.points.get(j).map_or(self.output_len, |p| p.output);
        let end = self.points.get(j + 1).map_or(self.output_len, |p| p.output);
        (start, end)
    }

    /// The bytes held in memory (the packed windows and the points), for budgets.
    #[must_use]
    pub fn cost(&self) -> u64 {
        self.points.iter().map(|p| p.packed.len() as u64 + 64).sum::<u64>() + 64
    }

    /// The index as bytes (the windows deflated), for a file.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut b = Vec::with_capacity(64 + self.points.iter().map(|p| p.packed.len() + 32).sum::<usize>());
        b.extend(MAGIC);
        b.extend(self.span.to_le_bytes());
        b.extend(self.input_len.to_le_bytes());
        b.extend(self.output_len.to_le_bytes());
        b.extend(self.crc.to_le_bytes());
        b.extend((self.points.len() as u32).to_le_bytes());
        for p in &self.points {
            b.extend(p.input.to_le_bytes());
            b.push(p.bits);
            b.extend(p.output.to_le_bytes());
            b.extend(p.crc.to_le_bytes());
            b.extend(p.window_len.to_le_bytes());
            b.extend((p.packed.len() as u32).to_le_bytes());
            b.extend(&p.packed);
        }
        b
    }

    /// An index from [`Index::to_bytes`]'s bytes.
    ///
    /// # Errors
    /// [`RestartError::Format`] when the bytes are not a whole, consistent index.
    pub fn from_bytes(b: &[u8]) -> Result<Self> {
        let mut r = Reader { b, at: 0 };
        if r.take(8)? != MAGIC {
            return Err(RestartError::Format);
        }
        let span = r.u64()?;
        let input_len = r.u64()?;
        let output_len = r.u64()?;
        let crc = r.u32()?;
        let count = r.u32()? as usize;
        let mut points = Vec::with_capacity(count.min(1 << 20));
        for _ in 0..count {
            let input = r.u64()?;
            let bits = *r.take(1)?.first().ok_or(RestartError::Format)?;
            let output = r.u64()?;
            let pcrc = r.u32()?;
            let wlen = r.u32()? as usize;
            let plen = r.u32()? as usize;
            let packed = r.take(plen)?;
            if wlen > WINDOW || bits > 7 {
                return Err(RestartError::Format);
            }
            // (the window is checked when its span is decoded: a damaged one inflates wrong and is refused there)
            points.push(Point {
                input,
                bits,
                output,
                window_len: wlen as u32,
                packed: packed.to_vec(),
                crc: pcrc,
            });
        }
        let ordered = points.windows(2).all(|w| match w {
            [a, b] => a.output < b.output && a.input <= b.input,
            _ => true,
        });
        let first_ok = points
            .first()
            .is_some_and(|p| p.output == 0 && p.input == 0 && p.bits == 0);
        let last_ok = points
            .last()
            .is_some_and(|p| p.output <= output_len && p.input <= input_len);
        if r.at != b.len() || !ordered || !first_ok || !last_ok {
            return Err(RestartError::Format);
        }
        Ok(Self {
            span,
            points,
            input_len,
            output_len,
            crc,
        })
    }
}

struct Reader<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let s = self
            .b
            .get(self.at..self.at.checked_add(n).ok_or(RestartError::Format)?)
            .ok_or(RestartError::Format)?;
        self.at += n;
        Ok(s)
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(
            self.take(4)?.try_into().map_err(|_| RestartError::Format)?,
        ))
    }
    fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(
            self.take(8)?.try_into().map_err(|_| RestartError::Format)?,
        ))
    }
}

/// Span `j` of the stream decoded: the output from point `j` to the next (or the end). `file` holds the deflate
/// stream at `base`. The span's CRC-32 is checked: [`RestartError::Changed`] when the bytes are not the ones the
/// index was made from.
///
/// # Errors
/// [`RestartError::Io`] when reading fails, [`RestartError::Changed`] when the span decodes to other bytes (or not
/// at all).
pub fn decode_span(index: &Index, j: usize, file: &mut (impl Read + Seek), base: u64) -> Result<Vec<u8>> {
    let p = index.points.get(j).ok_or(RestartError::Changed)?;
    let (start, end) = index.span_range(j);
    let in_end = index.points.get(j + 1).map_or(index.input_len, |q| q.input);
    let from = p.input - u64::from(p.bits > 0);
    let mut input = vec![0u8; usize::try_from(in_end.saturating_sub(from)).map_err(|_| RestartError::Format)?];
    file.seek(SeekFrom::Start(base + from))?;
    file.read_exact(&mut input).map_err(|e| {
        if e.kind() == io::ErrorKind::UnexpectedEof {
            RestartError::Changed
        } else {
            RestartError::Io(e)
        }
    })?;
    let mut z = z::Inflater::raw().ok_or_else(|| RestartError::Data("inflateInit2 failed".to_owned()))?;
    let mut feed = input.as_slice();
    if p.bits > 0 {
        let first = *feed.first().ok_or(RestartError::Changed)?;
        if !z.prime(p.bits, first >> (8 - p.bits)) {
            return Err(RestartError::Changed);
        }
        feed = feed.get(1..).unwrap_or_default();
    }
    let window = p.window().map_err(|_| RestartError::Changed)?;
    if !z.set_dictionary(&window) {
        return Err(RestartError::Changed);
    }
    let want = usize::try_from(end - start).map_err(|_| RestartError::Format)?;
    let mut out = vec![0u8; want];
    let mut filled = 0usize;
    while filled < want {
        let step = z
            .inflate(feed, out.get_mut(filled..).unwrap_or_default(), false)
            .map_err(|_| RestartError::Changed)?;
        filled += step.made;
        feed = feed.get(step.used..).unwrap_or_default();
        if step.end || (step.used == 0 && step.made == 0) {
            break;
        }
    }
    let mut crc = flate2::Crc::new();
    crc.update(&out);
    if filled != want || crc.sum() != p.crc {
        return Err(RestartError::Changed);
    }
    Ok(out)
}

/// Output bytes `[offset, offset + size)` of the stream in `file` at `base`, decoded from the restart points
/// (through `spans` when given: decoded spans kept, under `key`).
///
/// # Errors
/// As [`decode_span`]; [`RestartError::Changed`] when the range lies past the stream's end.
pub fn read(
    index: &Index,
    file: &mut (impl Read + Seek),
    base: u64,
    offset: u64,
    size: u64,
    spans: Option<(&Spans, u64)>,
) -> Result<Vec<u8>> {
    let end = offset.checked_add(size).ok_or(RestartError::Changed)?;
    if end > index.output_len {
        return Err(RestartError::Changed);
    }
    let mut out = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
    let mut at = offset;
    let mut j = index.point_for(offset);
    while at < end {
        let (s0, s1) = index.span_range(j);
        let span = match spans {
            Some((cache, key)) => cache.get_or(key, j, || decode_span(index, j, file, base))?,
            None => Arc::new(decode_span(index, j, file, base)?),
        };
        let lo = usize::try_from(at - s0).map_err(|_| RestartError::Format)?;
        let hi = usize::try_from(end.min(s1) - s0).map_err(|_| RestartError::Format)?;
        out.extend_from_slice(span.get(lo..hi).ok_or(RestartError::Changed)?);
        at = end.min(s1);
        j += 1;
    }
    Ok(out)
}

/// The most spans one [`read_ranges`] decodes side by side (256 MiB at the default spacing); a read needing more
/// decodes them one after another.
const PARALLEL_SPANS: usize = 256;

/// Many output ranges `(offset, size)` of the stream in the file at `path` (its deflate data at `base`): the spans
/// they need that `spans` does not hold are decoded side by side on the worker threads (each with its own handle
/// on the file), then each range is copied out. The same bytes as [`read`] range by range.
///
/// # Errors
/// As [`read`], for the first range that fails.
pub fn read_ranges(
    index: &Index,
    path: &std::path::Path,
    base: u64,
    ranges: &[(u64, u64)],
    spans: &Spans,
    key: u64,
) -> Result<Vec<Vec<u8>>> {
    use rayon::prelude::*;
    let mut needed = std::collections::BTreeSet::new();
    for &(offset, size) in ranges {
        if size == 0 {
            continue;
        }
        let end = offset.checked_add(size).ok_or(RestartError::Changed)?;
        if end > index.output_len {
            return Err(RestartError::Changed);
        }
        let last = index.point_for(end - 1);
        needed.extend(index.point_for(offset)..=last);
    }
    let mut local: HashMap<usize, Arc<Vec<u8>>> = HashMap::new();
    if needed.len() <= PARALLEL_SPANS {
        let missing: Vec<usize> = needed
            .iter()
            .copied()
            .filter(|&j| spans.peek(key, j).is_none())
            .collect();
        let decoded: Vec<(usize, Result<Vec<u8>>)> = missing
            .par_iter()
            .map(|&j| {
                let got = std::fs::File::open(path)
                    .map_err(RestartError::Io)
                    .and_then(|mut f| decode_span(index, j, &mut f, base));
                (j, got)
            })
            .collect();
        for (j, got) in decoded {
            let data = Arc::new(got?);
            spans.put(key, j, &data);
            local.insert(j, data);
        }
    }
    let mut file: Option<std::fs::File> = None;
    let mut out = Vec::with_capacity(ranges.len());
    for &(offset, size) in ranges {
        let mut got = Vec::with_capacity(usize::try_from(size).unwrap_or(0));
        let end = offset + size;
        let mut at = offset;
        let mut j = index.point_for(offset);
        while at < end {
            let (s0, s1) = index.span_range(j);
            let span = if let Some(d) = local.get(&j) {
                Arc::clone(d)
            } else {
                if file.is_none() {
                    file = Some(std::fs::File::open(path)?);
                }
                let f = file.as_mut().ok_or(RestartError::Changed)?;
                spans.get_or(key, j, || decode_span(index, j, f, base))?
            };
            let lo = usize::try_from(at - s0).map_err(|_| RestartError::Format)?;
            let hi = usize::try_from(end.min(s1) - s0).map_err(|_| RestartError::Format)?;
            got.extend_from_slice(span.get(lo..hi).ok_or(RestartError::Changed)?);
            at = end.min(s1);
            j += 1;
        }
        out.push(got);
    }
    Ok(out)
}

/// Decoded spans kept in memory, the least recently used dropped first beyond a byte budget. Keyed by the caller's
/// stream key and the span's number. Thread-safe.
pub struct Spans {
    inner: Mutex<SpanState>,
}

/// A kept span: its bytes and when it was last used.
type KeptSpan = (Arc<Vec<u8>>, u64);

struct SpanState {
    budget: u64,
    held: u64,
    tick: u64,
    map: HashMap<(u64, usize), KeptSpan>,
    hits: u64,
    decoded: u64,
    decoded_bytes: u64,
}

impl Spans {
    /// An empty cache of at most `budget` bytes (0: nothing kept).
    #[must_use]
    pub fn new(budget: u64) -> Self {
        Self {
            inner: Mutex::new(SpanState {
                budget,
                held: 0,
                tick: 0,
                map: HashMap::new(),
                hits: 0,
                decoded: 0,
                decoded_bytes: 0,
            }),
        }
    }

    fn state(&self) -> std::sync::MutexGuard<'_, SpanState> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Span `j` of stream `key`, decoded by `make` when not kept.
    ///
    /// # Errors
    /// `make`'s.
    pub fn get_or(&self, key: u64, j: usize, make: impl FnOnce() -> Result<Vec<u8>>) -> Result<Arc<Vec<u8>>> {
        if let Some(data) = self.peek(key, j) {
            return Ok(data);
        }
        let data = Arc::new(make()?);
        self.put(key, j, &data);
        Ok(data)
    }

    /// Span `j` of stream `key` when kept (marked used).
    #[must_use]
    pub fn peek(&self, key: u64, j: usize) -> Option<Arc<Vec<u8>>> {
        let mut s = self.state();
        s.tick += 1;
        let tick = s.tick;
        let (data, used) = s.map.get_mut(&(key, j))?;
        *used = tick;
        let data = Arc::clone(data);
        s.hits += 1;
        Some(data)
    }

    /// Keeps a decoded span (counted as decoded; dropped at once when it alone is over the budget).
    pub fn put(&self, key: u64, j: usize, data: &Arc<Vec<u8>>) {
        let mut s = self.state();
        s.decoded += 1;
        s.decoded_bytes += data.len() as u64;
        let size = data.len() as u64;
        if size <= s.budget && !s.map.contains_key(&(key, j)) {
            s.tick += 1;
            let tick = s.tick;
            s.map.insert((key, j), (Arc::clone(data), tick));
            s.held += size;
            while s.held > s.budget {
                let Some(old) = s.map.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else {
                    break;
                };
                if let Some((d, _)) = s.map.remove(&old) {
                    s.held -= d.len() as u64;
                }
            }
        }
    }

    /// Changes the budget (dropping the least recently used beyond it).
    pub fn set_budget(&self, budget: u64) {
        let mut s = self.state();
        s.budget = budget;
        while s.held > s.budget {
            let Some(old) = s.map.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| *k) else {
                break;
            };
            if let Some((d, _)) = s.map.remove(&old) {
                s.held -= d.len() as u64;
            }
        }
    }

    /// Forgets every span of stream `key`.
    pub fn forget(&self, key: u64) {
        let mut s = self.state();
        let gone: u64 = s
            .map
            .iter()
            .filter(|(k, _)| k.0 == key)
            .map(|(_, v)| v.0.len() as u64)
            .sum();
        s.map.retain(|k, _| k.0 != key);
        s.held -= gone;
    }

    /// Forgets every span.
    pub fn clear(&self) {
        let mut s = self.state();
        s.map.clear();
        s.held = 0;
    }

    /// (spans held, their bytes, hits, spans decoded, bytes decoded).
    #[must_use]
    pub fn stats(&self) -> (usize, u64, u64, u64, u64) {
        let s = self.state();
        (s.map.len(), s.held, s.hits, s.decoded, s.decoded_bytes)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write as _};

    fn deflate(data: &[u8], level: u32) -> Vec<u8> {
        let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(level));
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    fn sample(n: usize, seed: u32) -> Vec<u8> {
        // noise over a small alphabet (Huffman-coded literals, few matches), phrases and runs: many blocks of every
        // kind, most ending inside a byte
        let mut x = seed.wrapping_mul(2_654_435_761).max(1);
        let mut next = move || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x
        };
        let mut v = Vec::with_capacity(n);
        while v.len() < n {
            let r = next();
            match r % 8 {
                0 => v.extend_from_slice(b"Battlefield: Bad Company 2 "),
                1 => v.extend(std::iter::repeat_n((r >> 8) as u8, (r % 300) as usize)),
                _ => {
                    for _ in 0..(r % 2000) {
                        v.push(b'a' + (next() % 23) as u8);
                    }
                }
            }
        }
        v.truncate(n);
        v
    }

    #[test]
    fn every_range_as_the_whole() {
        for (n, level, span) in [
            (0usize, 6, 32_768u64),
            (1, 6, 32_768),
            (900_000, 6, 40_000),
            (700_000, 0, 50_000),
            (600_000, 1, 32_768),
            (500_000, 9, 100_000),
        ] {
            let data = sample(n, n as u32 + level);
            let z = deflate(&data, level);
            let mut out = Vec::new();
            let built = build(&mut z.as_slice(), span, &mut |b: &[u8]| out.extend_from_slice(b)).unwrap();
            assert_eq!(out, data);
            assert_eq!(built.index.input_len, z.len() as u64);
            assert_eq!(built.after, b"");
            let ix = Index::from_bytes(&built.index.to_bytes()).unwrap();
            assert_eq!(ix, built.index);
            if n > 400_000 && level != 1 {
                // (zlib-rs's level 1 writes one long block: one point)
                assert!(ix.points.len() > 3, "{} points, level {level}", ix.points.len());
                assert!(level == 0 || ix.points.iter().any(|p| p.bits > 0));
            }
            let mut f = Cursor::new(&z);
            for j in 0..ix.points.len() {
                let (a, b) = ix.span_range(j);
                assert_eq!(decode_span(&ix, j, &mut f, 0).unwrap(), &data[a as usize..b as usize]);
            }
            let cache = Spans::new(1 << 20);
            for (off, len) in [(0u64, 10u64), (n as u64 / 3, 70_000), (n as u64 / 2, 1), (0, n as u64)] {
                if off + len > n as u64 {
                    continue;
                }
                let got = read(&ix, &mut f, 0, off, len, Some((&cache, 1))).unwrap();
                assert_eq!(got, &data[off as usize..(off + len) as usize]);
            }
            assert!(read(&ix, &mut f, 0, n as u64, 1, None).is_err());
        }
    }

    #[test]
    fn after_the_stream_and_damage() {
        let data = sample(300_000, 7);
        let mut z = deflate(&data, 6);
        let len = z.len();
        z.extend(b"TRAILER!");
        let mut out = Vec::new();
        let built = build(&mut z.as_slice(), 32_768, &mut |b: &[u8]| out.extend_from_slice(b)).unwrap();
        assert_eq!(built.index.input_len, len as u64);
        assert_eq!(built.after, b"TRAILER!");
        // an early end
        assert!(matches!(
            build(&mut &z[..len / 2], 32_768, &mut |_: &[u8]| {}),
            Err(RestartError::Ends)
        ));
        // a changed byte inside a span: refused, never other bytes
        let ix = built.index;
        let mut bad = z.clone();
        let j = ix.points.len() / 2;
        bad[(ix.points[j].input + 50) as usize] ^= 0x55;
        let got = decode_span(&ix, j, &mut Cursor::new(&bad), 0);
        assert!(matches!(got, Err(RestartError::Changed)), "{got:?}");
        assert!(Index::from_bytes(b"BC2RSTP1").is_err());
        let mut cut = ix.to_bytes();
        cut.pop();
        assert!(Index::from_bytes(&cut).is_err());
    }

    #[test]
    fn spans_budget() {
        let c = Spans::new(10);
        let a = c.get_or(1, 0, || Ok(vec![1; 6])).unwrap();
        assert_eq!(*a, vec![1; 6]);
        c.get_or(1, 1, || Ok(vec![2; 6])).unwrap();
        assert_eq!(c.stats().0, 1);
        assert_eq!(*c.get_or(1, 1, || Ok(vec![9])).unwrap(), vec![2; 6]);
        c.get_or(1, 2, || Ok(vec![3; 11])).unwrap();
        assert_eq!(c.stats().0, 1);
        c.forget(1);
        assert_eq!(c.stats().1, 0);
    }
}
