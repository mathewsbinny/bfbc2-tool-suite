//! gzip as FbrbTool reads it: .NET Framework 4's `GZipStream` (decompression by the managed `Inflater`, not zlib),
//! read to its end with `CopyTo`.
//!
//! What .NET's reader does, and so what this one does:
//!
//! - one member only; whatever follows its trailer is ignored;
//! - the header's magic and method are checked; its flags, time, extra field, name, comment and header checksum are
//!   skipped, never checked;
//! - input that ends early (in the header, the data or the trailer) is no error: the bytes decoded so far are the
//!   result;
//! - the trailer, once all 8 bytes are there, is checked: CRC-32, then the length (modulo 2^32);
//! - the managed inflater's own rules for bad data: an unfilled Huffman table slot decodes as symbol 0 (an error only
//!   when symbol 0 has no code), a distance before the start of the output copies zeros, distance codes 30 and 31
//!   are taken (base 0), length codes 286 and 287 are refused, and so on - each exception with .NET's message.
//!
//! [`gunzip`] decodes a whole stream in memory with a port of the managed inflater ([`gunzip_managed`]) - unless
//! zlib (`flate2`) decodes it first without any complaint, its trailer complete and correct: a stream zlib takes in
//! full is one the managed inflater decodes to the same bytes (zlib refuses everything the managed inflater refuses,
//! and the managed inflater's oddities above only show on data zlib refuses), so the port only ever runs on a damaged
//! stream. [`gunzip_reader`] is the zlib path on a reader (a payload streamed from its bundle), `None` when the port
//! must decide.

use std::io::Read;

use flate2::{Crc, Decompress, FlushDecompress, Status};

/// `InvalidDataException`'s messages (System.dll's resources).
pub mod msg {
    /// `CorruptedGZipHeader`.
    pub const MAGIC: &str =
        "The magic number in GZip header is not correct. Make sure you are passing in a GZip stream.";
    /// `UnknownCompressionMode`.
    pub const METHOD: &str = "The compression mode specified in GZip header is unknown.";
    /// `InvalidCRC`.
    pub const CRC: &str = "The CRC in GZip footer does not match the CRC calculated from the decompressed data.";
    /// `InvalidStreamSize`.
    pub const SIZE: &str = "The stream size in GZip footer does not match the real stream size.";
    /// `InvalidBlockLength`.
    pub const BLOCK_LENGTH: &str = "Block length does not match with its complement.";
    /// `InvalidHuffmanData`.
    pub const HUFFMAN: &str =
        "Failed to construct a huffman tree using the length array. The stream might be corrupted.";
    /// `UnknownBlockType`.
    pub const BLOCK_TYPE: &str = "Unknown block type. Stream might be corrupted.";
    /// `GenericInvalidData` (also `new InvalidDataException()`'s).
    pub const GENERIC: &str = "Found invalid data while decoding.";
}

/// Why .NET's reader stopped: an `InvalidDataException` with its message, or an `IndexOutOfRangeException` (a
/// Huffman tree whose nodes overrun the managed inflater's arrays).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetGzipError {
    /// `InvalidDataException`.
    InvalidData(&'static str),
    /// `IndexOutOfRangeException`.
    IndexOutOfRange,
}

type Res<T> = Result<T, NetGzipError>;

fn invalid(m: &'static str) -> NetGzipError {
    NetGzipError::InvalidData(m)
}

// ------------------------------------------------------------------------------------------- the managed inflater
//
// A port of .NET Framework 4's System.IO.Compression `Inflater`, `InputBuffer`, `OutputWindow`, `HuffmanTree` and
// `GZipDecoder`, kept a resumable state machine as they are: what .NET returns for a stream that ends early depends
// on where `DeflateStream.Read` stops (`Inflate` copies the window out before each decoding step, not after the step
// that runs out of input, and `Stream.CopyTo` ends at the first `Read` that gives nothing - so the bytes of the last
// step are lost when that `Read` copied nothing else).

/// `DeflateStream`'s input buffer: the compressed bytes are fed to the inflater this many at a time.
const INPUT_CHUNK: usize = 8192;
/// `Stream.CopyTo`'s buffer: the `Read` size.
const COPY_CHUNK: usize = 81920;

/// `InputBuffer`: whole bytes feed a bit buffer, least significant bit first.
#[derive(Default)]
struct Input {
    data: Vec<u8>,
    pos: usize,
    bits: u32,
    count: u32,
}

impl Input {
    fn needs_input(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn set(&mut self, chunk: &[u8]) {
        self.data.clear();
        self.data.extend_from_slice(chunk);
        self.pos = 0;
    }

    fn available_bytes(&self) -> usize {
        self.data.len() - self.pos + (self.count / 8) as usize
    }

    fn load(&mut self) {
        if let Some(&b) = self.data.get(self.pos) {
            self.bits |= u32::from(b).wrapping_shl(self.count);
            self.count += 8;
            self.pos += 1;
        }
    }

    fn ensure(&mut self, n: u32) -> bool {
        if self.count < n {
            if self.needs_input() {
                return false;
            }
            self.load();
            if self.count < n {
                if self.needs_input() {
                    return false;
                }
                self.load();
            }
        }
        true
    }

    fn try_load16(&mut self) -> u32 {
        if self.count < 8 {
            self.load();
            self.load();
        } else if self.count < 16 {
            self.load();
        }
        self.bits
    }

    /// `GetBits`: `None` when the input has run out.
    fn get(&mut self, n: u32) -> Option<u32> {
        if !self.ensure(n) {
            return None;
        }
        let r = self.bits & 1u32.wrapping_shl(n).wrapping_sub(1);
        self.skip(n);
        Some(r)
    }

    fn skip(&mut self, n: u32) {
        self.bits = self.bits.checked_shr(n).unwrap_or(0);
        self.count = self.count.wrapping_sub(n);
    }

    fn skip_to_byte(&mut self) {
        let r = self.count % 8;
        self.skip(r);
    }

    /// `CopyTo`: whole bytes of the bit buffer first, then the input.
    fn copy_to(&mut self, out: &mut [u8]) -> usize {
        let mut n = 0;
        while self.count > 0 && n < out.len() {
            if let Some(o) = out.get_mut(n) {
                *o = self.bits as u8;
            }
            self.bits >>= 8;
            self.count = self.count.wrapping_sub(8);
            n += 1;
        }
        if n == out.len() {
            return n;
        }
        let take = (out.len() - n).min(self.data.len() - self.pos);
        if let (Some(dst), Some(src)) = (out.get_mut(n..n + take), self.data.get(self.pos..self.pos + take)) {
            dst.copy_from_slice(src);
        }
        self.pos += take;
        n + take
    }
}

const WINDOW: usize = 32768;
const WINDOW_MASK: usize = WINDOW - 1;

/// `OutputWindow`: the last 32 KiB decoded, `used` of them not yet copied out.
struct Window {
    ring: Vec<u8>,
    end: usize,
    used: usize,
}

impl Window {
    fn write(&mut self, b: u8) {
        if let Some(w) = self.ring.get_mut(self.end) {
            *w = b;
        }
        self.end = (self.end + 1) & WINDOW_MASK;
        self.used += 1;
    }

    fn write_length_distance(&mut self, mut length: usize, distance: usize) {
        self.used += length;
        let mut from = self.end.wrapping_sub(distance) & WINDOW_MASK;
        // byte by byte (Array.Copy's case gives the same bytes: it is taken only when the ranges do not overlap)
        while length > 0 {
            let b = self.ring.get(from).copied().unwrap_or(0);
            if let Some(w) = self.ring.get_mut(self.end) {
                *w = b;
            }
            self.end = (self.end + 1) & WINDOW_MASK;
            from = (from + 1) & WINDOW_MASK;
            length -= 1;
        }
    }

    fn copy_from(&mut self, input: &mut Input, length: usize) -> usize {
        let length = length.min(WINDOW - self.used).min(input.available_bytes());
        let tail = WINDOW - self.end;
        let copied = if length > tail {
            let mut c = input.copy_to(self.ring.get_mut(self.end..).unwrap_or_default());
            if c == tail {
                c += input.copy_to(self.ring.get_mut(..length - tail).unwrap_or_default());
            }
            c
        } else {
            input.copy_to(self.ring.get_mut(self.end..self.end + length).unwrap_or_default())
        };
        self.end = (self.end + copied) & WINDOW_MASK;
        self.used += copied;
        copied
    }

    fn free(&self) -> usize {
        WINDOW - self.used
    }

    /// `CopyTo`: up to `length` of the bytes not yet copied, oldest first, appended to `out`.
    fn copy_to(&mut self, out: &mut Vec<u8>, length: usize) -> usize {
        let length = length.min(self.used);
        let start = (self.end + WINDOW - self.used) & WINDOW_MASK;
        let first = length.min(WINDOW - start);
        out.extend_from_slice(self.ring.get(start..start + first).unwrap_or_default());
        out.extend_from_slice(self.ring.get(..length - first).unwrap_or_default());
        self.used -= length;
        length
    }
}

/// `HuffmanTree`: a 9-bit (literal / length) or 7-bit table, longer codes in `left` / `right`.
struct Tree {
    lengths: Vec<u8>,
    table_bits: u32,
    table: Vec<i16>,
    left: Vec<i16>,
    right: Vec<i16>,
}

const MAX_LITERAL: usize = 288;
const MAX_DIST: usize = 32;
const END_OF_BLOCK: usize = 256;

/// `FastEncoderStatics.BitReverse`: the low `len` bits of `code`, reversed.
fn bit_reverse(mut code: u32, mut len: u32) -> u32 {
    let mut r = 0u32;
    loop {
        r |= code & 1;
        r <<= 1;
        code >>= 1;
        len = len.saturating_sub(1);
        if len == 0 {
            return r >> 1;
        }
    }
}

impl Tree {
    #[expect(
        clippy::indexing_slicing,
        reason = "indices bounded by construction (code lengths <= 15, counts checked against the arrays' sizes)"
    )]
    fn new(lengths: Vec<u8>) -> Res<Self> {
        let table_bits = if lengths.len() == MAX_LITERAL { 9 } else { 7 };
        // CalculateHuffmanCode
        let mut count = [0u32; 17];
        for &l in &lengths {
            if let Some(c) = count.get_mut(usize::from(l)) {
                *c += 1;
            }
        }
        count[0] = 0;
        let mut next = [0u32; 17];
        let mut code = 0u32;
        for bits in 1..=16 {
            code = code.wrapping_add(count[bits - 1]) << 1;
            next[bits] = code;
        }
        let mut codes = vec![0u32; lengths.len()];
        for (c, &l) in codes.iter_mut().zip(&lengths) {
            if l > 0 {
                let l = usize::from(l);
                *c = bit_reverse(next[l], l as u32);
                next[l] = next[l].wrapping_add(1);
            }
        }
        // CreateTable
        let n = lengths.len();
        let mut t = Self {
            table: vec![0; 1 << table_bits],
            left: vec![0; 2 * n],
            right: vec![0; 2 * n],
            lengths,
            table_bits,
        };
        let mut avail = n as i16;
        for (ch, &code) in codes.iter().enumerate() {
            let len = u32::from(t.lengths.get(ch).copied().unwrap_or(0));
            if len == 0 {
                continue;
            }
            let mut start = code as usize;
            if len <= table_bits {
                let increment = 1usize << len;
                if start >= increment {
                    return Err(invalid(msg::HUFFMAN));
                }
                for _ in 0..1usize << (table_bits - len) {
                    *t.table.get_mut(start).ok_or(NetGzipError::IndexOutOfRange)? = ch as i16;
                    start += increment;
                }
            } else {
                let mut overflow = len - table_bits;
                let mut mask = 1usize << table_bits;
                let mut index = start & ((1 << table_bits) - 1);
                let mut which = 0; // 0: table, 1: left, 2: right
                loop {
                    let arr = match which {
                        0 => &mut t.table,
                        1 => &mut t.left,
                        _ => &mut t.right,
                    };
                    let slot = arr.get_mut(index).ok_or(NetGzipError::IndexOutOfRange)?;
                    let mut value = *slot;
                    if value == 0 {
                        *slot = avail.wrapping_neg();
                        value = avail.wrapping_neg();
                        avail = avail.wrapping_add(1);
                    }
                    if value > 0 {
                        return Err(invalid(msg::HUFFMAN));
                    }
                    which = if start & mask == 0 { 1 } else { 2 };
                    index = usize::from(value.unsigned_abs());
                    mask <<= 1;
                    overflow -= 1;
                    if overflow == 0 {
                        break;
                    }
                }
                let arr = if which == 1 { &mut t.left } else { &mut t.right };
                *arr.get_mut(index).ok_or(NetGzipError::IndexOutOfRange)? = ch as i16;
            }
        }
        Ok(t)
    }

    /// `GetNextSymbol`: `Ok(None)` when the input has run out.
    fn next(&self, input: &mut Input) -> Res<Option<usize>> {
        let bits = input.try_load16();
        if input.count == 0 {
            return Ok(None);
        }
        let slot = (bits & ((1 << self.table_bits) - 1)) as usize;
        let mut sym = i32::from(*self.table.get(slot).ok_or(NetGzipError::IndexOutOfRange)?);
        if sym < 0 {
            let mut mask = 1u32 << self.table_bits;
            while sym < 0 {
                let idx = sym.unsigned_abs() as usize;
                let arr = if bits & mask == 0 { &self.left } else { &self.right };
                sym = i32::from(*arr.get(idx).ok_or(NetGzipError::IndexOutOfRange)?);
                mask = mask.wrapping_shl(1);
            }
        }
        let sym = sym as usize;
        let len = u32::from(*self.lengths.get(sym).ok_or(NetGzipError::IndexOutOfRange)?);
        if len == 0 {
            return Err(invalid(msg::HUFFMAN));
        }
        if len > input.count {
            return Ok(None);
        }
        input.skip(len);
        Ok(Some(sym))
    }

    fn fixed_literal() -> Res<Self> {
        let l = (0..MAX_LITERAL)
            .map(|i| match i {
                0..=143 | 280.. => 8,
                144..=255 => 9,
                _ => 7,
            })
            .collect();
        Self::new(l)
    }
}

const EXTRA_LENGTH_BITS: [u32; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
const LENGTH_BASE: [usize; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131, 163, 195, 227, 258,
];
const DISTANCE_BASE: [usize; 32] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537, 2049, 3073, 4097, 6145,
    8193, 12289, 16385, 24577, 0, 0,
];
const CODE_ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];

/// `InflaterState` (the order matters: dynamic header states come before `DecodeTop`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum State {
    ReadingHeader,
    ReadingBFinal,
    ReadingBType,
    ReadingNumLitCodes,
    ReadingNumDistCodes,
    ReadingNumCodeLengthCodes,
    ReadingCodeLengthCodes,
    ReadingTreeCodesBefore,
    ReadingTreeCodesAfter,
    DecodeTop,
    HaveInitialLength,
    HaveFullLength,
    HaveDistCode,
    UncompressedAligning,
    Uncompressed(u8), // the length's four bytes, 0..=3
    DecodingUncompressed,
    StartReadingFooter,
    VerifyingFooter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Block {
    Stored,
    Fixed,
    Dynamic,
}

/// `GzipHeaderState` of `GZipDecoder.ReadHeader`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Head {
    Id1,
    Id2,
    Cm,
    Flg,
    Fixed(u8), // time (4), extra flags, OS: six bytes
    XLen1,
    XLen2,
    XLenData,
    FileName,
    Comment,
    Crc16(u8),
    Done,
}

/// `GZipDecoder`: the header skipped, the trailer read, the output's CRC-32 and length kept.
struct Gzip {
    head: Head,
    flag: u32,
    xlen: u32,
    counter: u32,
    footer_counter: u32,
    footer_crc_done: bool,
    expected_crc: u32,
    expected_size: u32,
    crc: Crc,
    size: u32,
}

impl Gzip {
    fn new() -> Self {
        Self {
            head: Head::Id1,
            flag: 0,
            xlen: 0,
            counter: 0,
            footer_counter: 0,
            footer_crc_done: false,
            expected_crc: 0,
            expected_size: 0,
            crc: Crc::new(),
            size: 0,
        }
    }

    fn read_header(&mut self, input: &mut Input) -> Res<bool> {
        loop {
            match self.head {
                Head::Id1 | Head::Id2 | Head::Cm => {
                    let Some(b) = input.get(8) else { return Ok(false) };
                    self.head = match self.head {
                        Head::Id1 if b == 0x1F => Head::Id2,
                        Head::Id2 if b == 0x8B => Head::Cm,
                        Head::Cm if b == 8 => Head::Flg,
                        Head::Cm => return Err(invalid(msg::METHOD)),
                        _ => return Err(invalid(msg::MAGIC)),
                    };
                }
                Head::Flg => {
                    let Some(b) = input.get(8) else { return Ok(false) };
                    self.flag = b;
                    self.head = Head::Fixed(0);
                }
                Head::Fixed(i) => {
                    if input.get(8).is_none() {
                        return Ok(false);
                    }
                    self.head = if i < 5 { Head::Fixed(i + 1) } else { Head::XLen1 };
                }
                Head::XLen1 => {
                    if self.flag & 4 == 0 {
                        self.head = Head::FileName;
                        continue;
                    }
                    let Some(b) = input.get(8) else { return Ok(false) };
                    self.xlen = b;
                    self.head = Head::XLen2;
                }
                Head::XLen2 => {
                    let Some(b) = input.get(8) else { return Ok(false) };
                    self.xlen |= b << 8;
                    self.counter = 0;
                    self.head = Head::XLenData;
                }
                Head::XLenData => {
                    while self.counter < self.xlen {
                        if input.get(8).is_none() {
                            return Ok(false);
                        }
                        self.counter += 1;
                    }
                    self.head = Head::FileName;
                }
                Head::FileName | Head::Comment => {
                    let bit = if self.head == Head::FileName { 8 } else { 16 };
                    if self.flag & bit != 0 {
                        loop {
                            match input.get(8) {
                                None => return Ok(false),
                                Some(0) => break,
                                Some(_) => {}
                            }
                        }
                    }
                    self.head = if self.head == Head::FileName {
                        Head::Comment
                    } else {
                        Head::Crc16(0)
                    };
                }
                Head::Crc16(i) => {
                    if self.flag & 2 == 0 {
                        self.head = Head::Done;
                        continue;
                    }
                    if input.get(8).is_none() {
                        return Ok(false);
                    }
                    self.head = if i == 0 { Head::Crc16(1) } else { Head::Done };
                }
                Head::Done => return Ok(true),
            }
        }
    }

    fn read_footer(&mut self, input: &mut Input) -> bool {
        input.skip_to_byte();
        if !self.footer_crc_done {
            while self.footer_counter < 4 {
                let Some(b) = input.get(8) else { return false };
                self.expected_crc |= b << (8 * self.footer_counter);
                self.footer_counter += 1;
            }
            self.footer_crc_done = true;
            self.footer_counter = 0;
        }
        if self.footer_counter == 0 {
            self.expected_size = 0;
        }
        while self.footer_counter < 4 {
            let Some(b) = input.get(8) else { return false };
            self.expected_size |= b << (8 * self.footer_counter);
            self.footer_counter += 1;
        }
        true
    }

    fn validate(&self) -> Res<()> {
        if self.expected_crc != self.crc.sum() {
            return Err(invalid(msg::CRC));
        }
        if self.size != self.expected_size {
            return Err(invalid(msg::SIZE));
        }
        Ok(())
    }
}

/// `Inflater` with its `GZipDecoder`.
struct Inflater {
    output: Window,
    input: Input,
    state: State,
    bfinal: bool,
    block: Block,
    length_buf: [u8; 4],
    block_length: usize,
    length: usize,
    distance_code: usize,
    extra_bits: u32,
    counter: usize,
    lit_count: usize,
    dist_count: usize,
    cl_count: usize,
    code_array_size: usize,
    length_code: usize,
    code_list: Vec<u8>,
    cl_lengths: [u8; 19],
    cl_tree: Option<Tree>,
    lit_tree: Option<std::rc::Rc<Tree>>,
    dist_tree: Option<std::rc::Rc<Tree>>,
    fixed: std::rc::Rc<Tree>,
    gzip: Gzip,
}

impl Inflater {
    fn new() -> Res<Self> {
        Ok(Self {
            output: Window {
                ring: vec![0; WINDOW],
                end: 0,
                used: 0,
            },
            input: Input::default(),
            state: State::ReadingHeader,
            bfinal: false,
            block: Block::Stored,
            length_buf: [0; 4],
            block_length: 0,
            length: 0,
            distance_code: 0,
            extra_bits: 0,
            counter: 0,
            lit_count: 0,
            dist_count: 0,
            cl_count: 0,
            code_array_size: 0,
            length_code: 0,
            code_list: vec![0; MAX_LITERAL + MAX_DIST],
            cl_lengths: [0; 19],
            cl_tree: None,
            lit_tree: None,
            dist_tree: None,
            fixed: std::rc::Rc::new(Tree::fixed_literal()?),
            gzip: Gzip::new(),
        })
    }

    fn finished(&self) -> bool {
        self.state == State::VerifyingFooter
    }

    /// `Inflate`: up to `length` bytes appended to `out`; the count.
    fn inflate(&mut self, out: &mut Vec<u8>, mut length: usize) -> Res<usize> {
        let mut count = 0;
        loop {
            let at = out.len();
            let copied = self.output.copy_to(out, length);
            if copied > 0 {
                let bytes = out.get(at..).unwrap_or_default();
                self.gzip.crc.update(bytes);
                self.gzip.size = self.gzip.size.wrapping_add(copied as u32);
                count += copied;
                length -= copied;
            }
            if length == 0 {
                break;
            }
            if self.finished() || !self.decode()? {
                break;
            }
        }
        if self.state == State::VerifyingFooter && self.output.used == 0 {
            self.gzip.validate()?;
        }
        Ok(count)
    }

    fn decode(&mut self) -> Res<bool> {
        if self.finished() {
            return Ok(true);
        }
        if self.state == State::ReadingHeader {
            if !self.gzip.read_header(&mut self.input)? {
                return Ok(false);
            }
            self.state = State::ReadingBFinal;
        } else if self.state == State::StartReadingFooter {
            if !self.gzip.read_footer(&mut self.input) {
                return Ok(false);
            }
            self.state = State::VerifyingFooter;
            return Ok(true);
        }
        if self.state == State::ReadingBFinal {
            let Some(b) = self.input.get(1) else { return Ok(false) };
            self.bfinal = b != 0;
            self.state = State::ReadingBType;
        }
        if self.state == State::ReadingBType {
            let Some(t) = self.input.get(2) else { return Ok(false) };
            match t {
                2 => {
                    self.block = Block::Dynamic;
                    self.state = State::ReadingNumLitCodes;
                }
                1 => {
                    self.block = Block::Fixed;
                    self.lit_tree = Some(self.fixed.clone());
                    self.dist_tree = None;
                    self.state = State::DecodeTop;
                }
                0 => {
                    self.block = Block::Stored;
                    self.state = State::UncompressedAligning;
                }
                _ => return Err(invalid(msg::BLOCK_TYPE)),
            }
        }
        let mut eob = false;
        let result = match self.block {
            Block::Dynamic if self.state < State::DecodeTop => self.dynamic_header()?,
            Block::Stored => self.stored(&mut eob)?,
            _ => self.block(&mut eob)?,
        };
        if eob && self.bfinal {
            self.state = State::StartReadingFooter;
        }
        Ok(result)
    }

    fn stored(&mut self, eob: &mut bool) -> Res<bool> {
        loop {
            match self.state {
                State::UncompressedAligning => {
                    self.input.skip_to_byte();
                    self.state = State::Uncompressed(0);
                }
                State::Uncompressed(at) => {
                    let Some(byte) = self.input.get(8) else {
                        return Ok(false);
                    };
                    if let Some(slot) = self.length_buf.get_mut(usize::from(at)) {
                        *slot = byte as u8;
                    }
                    if at == 3 {
                        let [lo, hi, nlo, nhi] = self.length_buf;
                        let len = u16::from_le_bytes([lo, hi]);
                        if len != !u16::from_le_bytes([nlo, nhi]) {
                            return Err(invalid(msg::BLOCK_LENGTH));
                        }
                        self.block_length = usize::from(len);
                        self.state = State::DecodingUncompressed;
                    } else {
                        self.state = State::Uncompressed(at + 1);
                    }
                }
                State::DecodingUncompressed => {
                    let copied = self.output.copy_from(&mut self.input, self.block_length);
                    self.block_length -= copied;
                    if self.block_length == 0 {
                        self.state = State::ReadingBFinal;
                        *eob = true;
                        return Ok(true);
                    }
                    return Ok(self.output.free() == 0);
                }
                _ => {
                    return Err(invalid(
                        "Decoder is in some unknown state. This might be caused by corrupted data.",
                    ));
                }
            }
        }
    }

    #[expect(
        clippy::indexing_slicing,
        reason = "indices bounded by construction (code lengths <= 15, counts checked against the arrays' sizes)"
    )]
    fn dynamic_header(&mut self) -> Res<bool> {
        loop {
            match self.state {
                State::ReadingNumLitCodes => {
                    let Some(b) = self.input.get(5) else { return Ok(false) };
                    self.lit_count = b as usize + 257;
                    self.state = State::ReadingNumDistCodes;
                }
                State::ReadingNumDistCodes => {
                    let Some(b) = self.input.get(5) else { return Ok(false) };
                    self.dist_count = b as usize + 1;
                    self.state = State::ReadingNumCodeLengthCodes;
                }
                State::ReadingNumCodeLengthCodes => {
                    let Some(b) = self.input.get(4) else { return Ok(false) };
                    self.cl_count = b as usize + 4;
                    self.counter = 0;
                    self.state = State::ReadingCodeLengthCodes;
                }
                State::ReadingCodeLengthCodes => {
                    while self.counter < self.cl_count {
                        let Some(b) = self.input.get(3) else { return Ok(false) };
                        if let Some(&o) = CODE_ORDER.get(self.counter) {
                            self.cl_lengths[o] = b as u8;
                        }
                        self.counter += 1;
                    }
                    for &o in CODE_ORDER.iter().skip(self.cl_count) {
                        self.cl_lengths[o] = 0;
                    }
                    self.cl_tree = Some(Tree::new(self.cl_lengths.to_vec())?);
                    self.code_array_size = self.lit_count + self.dist_count;
                    self.counter = 0;
                    self.state = State::ReadingTreeCodesBefore;
                }
                State::ReadingTreeCodesBefore | State::ReadingTreeCodesAfter => {
                    while self.counter < self.code_array_size {
                        if self.state == State::ReadingTreeCodesBefore {
                            let Some(t) = &self.cl_tree else {
                                return Err(invalid(msg::GENERIC));
                            };
                            let Some(c) = t.next(&mut self.input)? else {
                                return Ok(false);
                            };
                            self.length_code = c;
                        }
                        if self.length_code <= 15 {
                            self.code_list[self.counter] = self.length_code as u8;
                            self.counter += 1;
                        } else {
                            if !self.input.ensure(7) {
                                self.state = State::ReadingTreeCodesAfter;
                                return Ok(false);
                            }
                            let (repeat, value) = match self.length_code {
                                16 => {
                                    if self.counter == 0 {
                                        return Err(invalid(msg::GENERIC));
                                    }
                                    let prev = self.code_list[self.counter - 1];
                                    (self.input.get(2).unwrap_or(0) as usize + 3, prev)
                                }
                                17 => (self.input.get(3).unwrap_or(0) as usize + 3, 0),
                                _ => (self.input.get(7).unwrap_or(0) as usize + 11, 0),
                            };
                            if self.counter + repeat > self.code_array_size {
                                return Err(invalid(msg::GENERIC));
                            }
                            for _ in 0..repeat {
                                self.code_list[self.counter] = value;
                                self.counter += 1;
                            }
                        }
                        self.state = State::ReadingTreeCodesBefore;
                    }
                    break;
                }
                _ => {
                    return Err(invalid(
                        "Decoder is in some unknown state. This might be caused by corrupted data.",
                    ));
                }
            }
        }
        let mut lit = vec![0u8; MAX_LITERAL];
        lit[..self.lit_count].copy_from_slice(&self.code_list[..self.lit_count]);
        let mut dist = vec![0u8; MAX_DIST];
        dist[..self.dist_count].copy_from_slice(&self.code_list[self.lit_count..self.code_array_size]);
        if lit[END_OF_BLOCK] == 0 {
            return Err(invalid(msg::GENERIC));
        }
        self.lit_tree = Some(std::rc::Rc::new(Tree::new(lit)?));
        self.dist_tree = Some(std::rc::Rc::new(Tree::new(dist)?));
        self.state = State::DecodeTop;
        Ok(true)
    }

    fn block(&mut self, eob: &mut bool) -> Res<bool> {
        let (Some(lit), dist) = (self.lit_tree.clone(), self.dist_tree.clone()) else {
            return Err(invalid(msg::GENERIC));
        };
        let mut free = self.output.free();
        while free > 258 {
            match self.state {
                State::DecodeTop => {
                    let Some(sym) = lit.next(&mut self.input)? else {
                        return Ok(false);
                    };
                    if sym < 256 {
                        self.output.write(sym as u8);
                        free -= 1;
                        continue;
                    }
                    if sym == 256 {
                        *eob = true;
                        self.state = State::ReadingBFinal;
                        return Ok(true);
                    }
                    let s = sym - 257;
                    if s < 8 {
                        self.length = s + 3;
                        self.extra_bits = 0;
                    } else if s == 28 {
                        self.length = 258;
                        self.extra_bits = 0;
                    } else {
                        self.extra_bits = *EXTRA_LENGTH_BITS.get(s).ok_or(invalid(msg::GENERIC))?;
                        self.length = s;
                    }
                    self.state = State::HaveInitialLength;
                }
                State::HaveInitialLength => {
                    if self.extra_bits > 0 {
                        let Some(bits) = self.input.get(self.extra_bits) else {
                            return Ok(false);
                        };
                        let base = *LENGTH_BASE.get(self.length).ok_or(invalid(msg::GENERIC))?;
                        self.length = base + bits as usize;
                    }
                    self.state = State::HaveFullLength;
                }
                State::HaveFullLength => {
                    let code = match &dist {
                        Some(t) => t.next(&mut self.input)?,
                        None => self.input.get(5).map(|b| bit_reverse(b, 5) as usize),
                    };
                    let Some(code) = code else { return Ok(false) };
                    self.distance_code = code;
                    self.state = State::HaveDistCode;
                }
                State::HaveDistCode => {
                    let offset = if self.distance_code > 3 {
                        let extra = ((self.distance_code - 2) >> 1) as u32;
                        let Some(bits) = self.input.get(extra) else {
                            return Ok(false);
                        };
                        DISTANCE_BASE.get(self.distance_code).copied().unwrap_or(0) + bits as usize
                    } else {
                        self.distance_code + 1
                    };
                    self.output.write_length_distance(self.length, offset);
                    free = free.saturating_sub(self.length);
                    self.state = State::DecodeTop;
                }
                _ => {
                    return Err(invalid(
                        "Decoder is in some unknown state. This might be caused by corrupted data.",
                    ));
                }
            }
        }
        Ok(true)
    }
}

/// .NET's whole run of `GZipStream.CopyTo` over `data` (the port of the managed inflater, `DeflateStream.Read` and
/// `Stream.CopyTo`; [`gunzip`] uses it only when zlib complains).
///
/// # Errors
/// What .NET throws for the stream.
pub fn gunzip_managed(data: &[u8]) -> Result<Vec<u8>, NetGzipError> {
    let mut inf = Inflater::new()?;
    let mut fed = 0usize;
    let mut out = Vec::new();
    loop {
        // DeflateStream.Read(buffer, 0, 81920)
        let mut remaining = COPY_CHUNK;
        loop {
            let n = inf.inflate(&mut out, remaining)?;
            remaining -= n;
            if remaining == 0 || inf.finished() {
                break;
            }
            let chunk = data.get(fed..data.len().min(fed + INPUT_CHUNK)).unwrap_or_default();
            if chunk.is_empty() {
                break;
            }
            fed += chunk.len();
            inf.input.set(chunk);
        }
        if remaining == COPY_CHUNK {
            return Ok(out); // Stream.CopyTo stops at a Read of nothing
        }
    }
}

/// The length of the gzip header at the start of `data`, when all of it is there and .NET accepts it.
fn header_len(data: &[u8]) -> Option<usize> {
    let mut input = Input::default();
    input.set(data.get(..data.len().min(1 << 16)).unwrap_or_default());
    match Gzip::new().read_header(&mut input) {
        Ok(true) => Some(input.pos),
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------- the zlib path

/// Inflates from `read` (which gives the bytes after the header first, then more) with zlib. `Some(output)` when
/// the deflate stream ends with an 8-byte trailer whose CRC-32 and length match: .NET's result; `None` for anything
/// else (the managed port decides).
fn zlib_member(
    mut pending: Vec<u8>,
    read: &mut dyn FnMut(&mut Vec<u8>) -> std::io::Result<usize>,
    hint: usize,
) -> Option<Vec<u8>> {
    let mut dec = Decompress::new(false);
    let mut out: Vec<u8> = Vec::with_capacity(hint);
    let mut at = 0usize;
    let mut eof = false;
    loop {
        if pending.len() - at < 1 << 16 && !eof {
            pending.drain(..at);
            at = 0;
            match read(&mut pending) {
                Ok(0) => eof = true,
                Ok(_) => {}
                Err(_) => return None,
            }
        }
        if out.capacity() - out.len() < 1 << 16 {
            out.reserve(out.len().max(1 << 20));
        }
        let before = (dec.total_in(), out.len());
        let status = dec
            .decompress_vec(pending.get(at..).unwrap_or_default(), &mut out, FlushDecompress::None)
            .ok()?;
        at += (dec.total_in() - before.0) as usize;
        if status == Status::StreamEnd {
            break;
        }
        if eof && dec.total_in() == before.0 && out.len() == before.1 {
            return None;
        }
    }
    // the trailer
    while pending.len() - at < 8 && !eof {
        match read(&mut pending) {
            Ok(0) => eof = true,
            Ok(_) => {}
            Err(_) => return None,
        }
    }
    let t: [u8; 8] = pending.get(at..at + 8)?.try_into().ok()?;
    let [c0, c1, c2, c3, s0, s1, s2, s3] = t;
    let mut sum = Crc::new();
    sum.update(&out);
    (sum.sum() == u32::from_le_bytes([c0, c1, c2, c3]) && u32::from_le_bytes([s0, s1, s2, s3]) == out.len() as u32)
        .then_some(out)
}

/// FbrbTool's `Gunzip` of `data`: .NET's result, the bytes or the exception.
///
/// # Errors
/// What .NET throws for the stream.
pub fn gunzip(data: &[u8]) -> Result<Vec<u8>, NetGzipError> {
    if let Some(h) = header_len(data) {
        let mut done = false;
        let mut feed = |buf: &mut Vec<u8>| {
            if done {
                return Ok(0);
            }
            done = true;
            buf.extend_from_slice(data.get(h..).unwrap_or_default());
            Ok(data.len() - h)
        };
        if let Some(out) = zlib_member(Vec::new(), &mut feed, size_hint(data)) {
            return Ok(out);
        }
    }
    gunzip_managed(data)
}

/// The trailer's length field as a capacity, when it is plausible for `data`.
fn size_hint(data: &[u8]) -> usize {
    let Some(t) = data
        .len()
        .checked_sub(4)
        .and_then(|i| data.get(i..))
        .and_then(|t| <[u8; 4]>::try_from(t).ok())
    else {
        return 0;
    };
    let n = u32::from_le_bytes(t) as usize;
    if n <= data.len().saturating_mul(1100) { n } else { 0 }
}

/// [`gunzip`] of a stream read from `r` with zlib, without holding the compressed bytes: `Some` when zlib took it
/// all (then it is .NET's result), `None` when the managed port must decide (the caller reads the stream into memory
/// and calls [`gunzip`]). `size_hint`: the expected output length, for the first allocation.
pub fn gunzip_reader<R: Read>(r: &mut R, size_hint: usize) -> Option<Vec<u8>> {
    let mut chunk = vec![0u8; 1 << 20];
    let mut read = |buf: &mut Vec<u8>| -> std::io::Result<usize> {
        let n = r.read(&mut chunk)?;
        buf.extend_from_slice(chunk.get(..n).unwrap_or_default());
        Ok(n)
    };
    // the header: up to 64 KiB is plenty for any header .NET reads without a long extra field or name
    let mut head = Vec::new();
    let h = loop {
        if let Some(h) = header_len(&head) {
            break h;
        }
        if head.len() > 1 << 16 || read(&mut head).ok()? == 0 {
            return None;
        }
    };
    let rest = head.split_off(h);
    zlib_member(rest, &mut read, size_hint)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::zlib::gzip_net;

    #[test]
    fn valid_streams_both_ways() {
        let data: Vec<u8> = (0..200_000u32)
            .map(|i| (i * 7 % 251) as u8 ^ (i / 1000) as u8)
            .collect();
        let gz = gzip_net(&data);
        assert_eq!(gunzip(&gz).unwrap(), data);
        assert_eq!(gunzip_managed(&gz).unwrap(), data);
        assert_eq!(gunzip_reader(&mut gz.as_slice(), 0).unwrap(), data);
        // trailing bytes are ignored
        let mut more = gz.clone();
        more.extend_from_slice(b"junk");
        assert_eq!(gunzip(&more).unwrap(), data);
    }

    #[test]
    fn damage() {
        let data = b"hello hello hello hello world".repeat(50);
        let gz = gzip_net(&data);
        // cut short: what was decoded, no error
        let cut = gunzip(&gz[..gz.len() - 3]).unwrap();
        assert_eq!(cut, data);
        let early = gunzip(&gz[..20]).unwrap();
        assert!(data.starts_with(&early));
        assert_eq!(gunzip(&gz[..5]).unwrap(), b"");
        assert_eq!(gunzip(&[]).unwrap(), b"");
        // the trailer checked
        let mut bad = gz.clone();
        let n = bad.len();
        bad[n - 8] ^= 1;
        assert_eq!(gunzip(&bad), Err(NetGzipError::InvalidData(msg::CRC)));
        let mut bad = gz.clone();
        bad[n - 1] ^= 1;
        assert_eq!(gunzip(&bad), Err(NetGzipError::InvalidData(msg::SIZE)));
        let mut bad = gz.clone();
        bad[0] = 0;
        assert_eq!(gunzip(&bad), Err(NetGzipError::InvalidData(msg::MAGIC)));
        let mut bad = gz;
        bad[2] = 9;
        assert_eq!(gunzip(&bad), Err(NetGzipError::InvalidData(msg::METHOD)));
        // block type 3
        assert_eq!(
            gunzip(&[0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 0, 0x07]),
            Err(NetGzipError::InvalidData(msg::BLOCK_TYPE))
        );
        // a stored block whose length and complement disagree
        assert_eq!(
            gunzip(&[0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 0, 0, 0x01, 1, 0, 0, 0]),
            Err(NetGzipError::InvalidData(msg::BLOCK_LENGTH))
        );
    }
}
