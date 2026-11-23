//! Bundle packing, byte for byte as FbrbTool's `pack` (FbrbTool.cs's `Pack`, retired in round R4): a folder FbrbTool
//! unpacked (its files and `fbrb.manifest`) back into a bundle.
//!
//! The manifest's first line holds `zipped=1` for a gzip payload and, for DICE's padded uncompressed bundles, the
//! layout tokens FbrbTool's unpack wrote (`layout=align4096` or `layout=gap64`, `hdr=<alignment>`, `plen=size2`,
//! `size2=minus64`: [`Layout`]); every other line not starting with `#` is an entry, `name|type|flags(hex)|size2|size`
//! (older manifests have no size column). The entry's data is the file `<folder>/<name>`; a `*deleted*` marker has
//! no file.
//!
//! What is written, as FbrbTool writes it:
//!
//! - the payload: the entries back to back in manifest order, with DICE's padding before every entry but the first
//!   in a padded layout (*deleted* markers take the position as it is);
//! - each entry's size2: its size, except that the manifest's size2 is kept while the file still has the size it
//!   unpacked to, and in a `size2=minus64` bundle an edited entry of at least 64 bytes gets its size - 64;
//! - the string table: each entry's name, then its type the first time the type is used;
//! - the header's payload length: the payload's, or with `plen=size2` the last live entry's offset + size2;
//! - both gzip streams as .NET Framework's `GZipStream` writes them ([`crate::zlib::gzip_net`]: zlib level 6), the
//!   header region padded with zeros to `hdr`;
//! - one difference from FbrbTool, DICE's bytes winning (round SPV1-fix): a zipped bundle whose payload is empty
//!   (markers only, or no entries) ends with DICE's 20-byte empty gzip member ([`EMPTY_GZIP_MEMBER`],
//!   FBRB-DEL-2), where .NET's `GZipStream` wrote nothing.
//!
//! Round S1: the payload is streamed. The layout (offsets, padding, the header) comes from the files' sizes alone;
//! the files are then read in order on a second thread, a megabyte at a time, and fed through the deflater
//! ([`crate::zlib::gzip_net_stream`]) into the output as it is made. A bundle holds about 10 MB however large its
//! payload (a WW2 level's 1.5 GB was held whole before, then its compressed copy); the bytes are the same.
//!
//! The manifest is read as `File.ReadAllLines` reads it (UTF-8 or a byte-order mark's encoding; lines end at CR, LF
//! or CR LF), numbers as `uint.Parse` and `Convert.ToUInt32(s, 16)` parse them, and names are stored as
//! `Encoding.ASCII` stores them (`?` for anything outside ASCII). An entry name that is empty, rooted or leads
//! out of the folder is refused, as FbrbTool's `Safe` refuses it.

use std::fs;
use std::io::{self, Read as _, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use thiserror::Error;

use super::header::{DELETED, MAGIC};
use crate::zlib::{gzip_net, gzip_net_stream};

/// Why a folder could not be packed (FbrbTool's exceptions: a missing or unreadable file, a malformed manifest, an
/// unsafe entry name, an old manifest whose size2 cannot be kept).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum PackError {
    /// A file (the manifest, an entry's data, the bundle written) could not be read or written.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The error.
        source: io::Error,
    },
    /// The manifest is empty or a line of it is malformed (too few fields, a number that does not parse).
    #[error("fbrb.manifest line {line}: {what}")]
    Manifest {
        /// The line number, from 1.
        line: usize,
        /// What is wrong.
        what: String,
    },
    /// An entry name that is empty, rooted, or leads out of the folder (FbrbTool's `Safe`).
    #[error("entry escapes the output folder: {0}")]
    UnsafeName(String),
    /// An entry of a manifest without the size column whose file no longer has the manifest's size2.
    #[error(
        "fbrb.manifest is from an older unpack (no size column) and entry '{0}' has a differing size2 - unpack the \
         bundle again to rebuild the manifest"
    )]
    OldManifest(String),
    /// The bundle's payload, header or a field would pass 4 GB.
    #[error("the bundle would pass 4 GB")]
    TooLarge,
}

/// The result type of packing.
pub type PackResult<T> = Result<T, PackError>;

/// DICE's padding between the entries of an uncompressed bundle (the manifest's `layout=` token).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Padding {
    /// Each entry after the first starts on a 4096-byte boundary, the gap filled with `0xBF`.
    Align4096,
    /// 64 bytes of `0xBF` between entries.
    Gap64,
}

/// How a bundle is laid out: the manifest's first line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    /// The payload is one gzip stream (`zipped=1`).
    pub zipped: bool,
    /// The padding between entries (`layout=`; never for a zipped bundle, and none for a value FbrbTool does not
    /// know).
    pub padding: Option<Padding>,
    /// The header region (8 + the stored header) is padded with zeros to a multiple of this (`hdr=`; 1 or less:
    /// no padding).
    pub header_align: i32,
    /// The header's payload length is the last live entry's offset + size2 (`plen=size2`).
    pub plen_size2: bool,
    /// An edited entry's size2 is its size - 64 (`size2=minus64`).
    pub size2_minus64: bool,
}

impl Default for Layout {
    /// A zipped bundle without padding, as `fbrb.build` and FbrbTool's unpack of a level bundle write it.
    fn default() -> Self {
        Self {
            zipped: true,
            padding: None,
            header_align: 1,
            plen_size2: false,
            size2_minus64: false,
        }
    }
}

/// One entry to pack, with its final flags and size2 (the manifest's rules applied: [`pack_dir`] does that).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackItem<'a> {
    /// The archive name.
    pub name: &'a str,
    /// The resource type ([`DELETED`] for a marker).
    pub type_name: &'a str,
    /// The flags word.
    pub flags: u32,
    /// The second size field.
    pub size2: u32,
    /// The entry's data (empty for a marker).
    pub data: &'a [u8],
}

/// What a pack wrote: FbrbTool's "packed N entries (X bytes)".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackStats {
    /// The entries, markers included.
    pub entries: usize,
    /// The payload's length before compression.
    pub payload_len: u64,
}

fn is_marker(type_name: &str, size: usize) -> bool {
    type_name == DELETED && size == 0
}

fn u32_of(n: usize) -> PackResult<u32> {
    u32::try_from(n).map_err(|_| PackError::TooLarge)
}

/// `Encoding.ASCII.GetBytes`: each UTF-16 unit outside ASCII as `?`.
fn ascii(s: &str, out: &mut Vec<u8>) {
    for c in s.chars() {
        if c.is_ascii() {
            out.push(c as u8);
        } else {
            // one '?' per UTF-16 unit: a character outside the BMP (a surrogate pair) gives two
            out.extend(std::iter::repeat_n(b'?', c.len_utf16()));
        }
    }
}

/// One piece of a payload, in order: DICE's padding, bytes in memory, or a file's bytes (read when the payload is
/// written: round S1 streams them into the deflater instead of holding the payload).
#[derive(Debug, Clone, Copy)]
enum Seg<'a> {
    Pad(usize),
    Bytes(&'a [u8]),
    File(&'a Path, usize),
}

/// A payload being laid out: the entries' offsets and DICE's padding, and the pieces it is made of.
struct Payload<'a> {
    len: usize,
    segs: Vec<Seg<'a>>,
    rows: Vec<[u32; 4]>, // flags, offset, size, size2
    first: bool,
}

impl<'a> Payload<'a> {
    fn new() -> Self {
        Self {
            len: 0,
            segs: Vec::new(),
            rows: Vec::new(),
            first: true,
        }
    }

    /// The padding before an entry (`marker`: a *deleted* marker, which takes the position as it is).
    fn pad(&mut self, layout: Layout, marker: bool) {
        if marker || layout.zipped {
            return;
        }
        if let Some(p) = layout.padding {
            if !self.first {
                let n = match p {
                    Padding::Align4096 => self.len.next_multiple_of(4096) - self.len,
                    Padding::Gap64 => 64,
                };
                self.push(Seg::Pad(n), n);
            }
            self.first = false;
        }
    }

    fn push(&mut self, seg: Seg<'a>, n: usize) {
        if n > 0 {
            self.segs.push(seg);
            self.len += n;
        }
    }

    fn offset(&self) -> PackResult<u32> {
        u32_of(self.len)
    }
}

/// The bundle's first part (magic, length, the gzip'd header padded to `hdr`) for laid-out entries.
fn header_part<'a>(
    names: impl Iterator<Item = (&'a str, &'a str)>,
    p: &Payload<'_>,
    layout: Layout,
) -> PackResult<Vec<u8>> {
    let mut strings = Vec::new();
    let mut types: Vec<(&str, u32)> = Vec::new();
    let mut table = Vec::with_capacity(24 * p.rows.len());
    let mut last_live = None;
    for ((name, type_name), row) in names.zip(&p.rows) {
        let no = u32_of(strings.len())?;
        ascii(name, &mut strings);
        strings.push(0);
        let to = if let Some(&(_, o)) = types.iter().find(|(t, _)| *t == type_name) {
            o
        } else {
            let o = u32_of(strings.len())?;
            types.push((type_name, o));
            ascii(type_name, &mut strings);
            strings.push(0);
            o
        };
        let [flags, offset, size, size2] = *row;
        if !is_marker(type_name, size as usize) {
            last_live = Some(offset.wrapping_add(size2));
        }
        for v in [no, flags, offset, size, size2, to] {
            table.extend(v.to_be_bytes());
        }
    }
    let mut header = Vec::with_capacity(16 + strings.len() + table.len());
    header.extend(2u32.to_be_bytes());
    header.extend(u32_of(strings.len())?.to_be_bytes());
    header.extend(&strings);
    header.extend(u32_of(p.rows.len())?.to_be_bytes());
    header.extend(&table);
    header.push(u8::from(layout.zipped));
    let plen = match last_live {
        Some(end) if layout.plen_size2 => end,
        _ => u32_of(p.len)?,
    };
    header.extend(plen.to_be_bytes());
    let mut part1 = gzip_net(&header);
    if layout.header_align > 1 {
        let a = layout.header_align as usize;
        part1.resize(part1.len() + (a - (8 + part1.len()) % a) % a, 0);
    }
    let mut out = Vec::with_capacity(8 + part1.len());
    out.extend_from_slice(MAGIC);
    out.extend(u32_of(part1.len())?.to_be_bytes());
    out.extend(part1);
    Ok(out)
}

/// Why writing a bundle stopped: the packing's own error (a file that cannot be read or changed size), or the
/// output's.
pub enum StreamError {
    /// The packing's own error.
    Pack(PackError),
    /// The output's.
    Write(io::Error),
}

/// The payload's pieces as one stream of bytes, the files opened one at a time. The first error is kept (`err`) and
/// given to the reader's caller as an `io::Error`.
struct SegReader<'a> {
    segs: &'a [Seg<'a>],
    i: usize,
    done: usize,
    file: Option<fs::File>,
    err: Option<PackError>,
}

impl<'a> SegReader<'a> {
    fn new(segs: &'a [Seg<'a>]) -> Self {
        Self {
            segs,
            i: 0,
            done: 0,
            file: None,
            err: None,
        }
    }

    fn next(&mut self) {
        self.i += 1;
        self.done = 0;
        self.file = None;
    }

    fn fail(&mut self, e: PackError) -> io::Error {
        let msg = e.to_string();
        self.err = Some(e);
        io::Error::other(msg)
    }

    /// Up to `buf.len()` bytes of the current file (`len` long), checked as [`read_exactly`] checks it: a file that
    /// ends early or goes on past its length is an error.
    fn file_bytes(&mut self, src: &Path, len: usize, buf: &mut [u8]) -> io::Result<usize> {
        if self.file.is_none() {
            match fs::File::open(src) {
                Ok(f) => self.file = Some(f),
                Err(e) => return Err(self.fail(io_err(src)(e))),
            }
        }
        let Some(f) = self.file.as_mut() else {
            return Ok(0);
        };
        let want = (len - self.done).min(buf.len());
        if want == 0 {
            // the whole length read: the file must end here
            let mut more = [0u8; 1];
            return match f.read(&mut more) {
                Ok(0) => {
                    self.next();
                    Ok(0)
                }
                Ok(_) => Err(self.fail(PackError::Io {
                    path: src.to_path_buf(),
                    source: io::Error::other("the file grew while it was packed"),
                })),
                Err(e) => Err(self.fail(io_err(src)(e))),
            };
        }
        match f.read(buf.get_mut(..want).unwrap_or_default()) {
            Ok(0) => Err(self.fail(io_err(src)(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "failed to fill whole buffer",
            )))),
            Ok(k) => {
                self.done += k;
                Ok(k)
            }
            Err(e) if e.kind() == io::ErrorKind::Interrupted => Ok(0),
            Err(e) => Err(self.fail(io_err(src)(e))),
        }
    }
}

impl io::Read for SegReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.err.is_some() || buf.is_empty() {
            return Ok(0);
        }
        loop {
            let Some(&seg) = self.segs.get(self.i) else {
                return Ok(0);
            };
            let n = match seg {
                Seg::Pad(n) => {
                    let k = (n - self.done).min(buf.len());
                    buf.get_mut(..k).unwrap_or_default().fill(0xBF);
                    self.done += k;
                    if self.done == n {
                        self.next();
                    }
                    k
                }
                Seg::Bytes(b) => {
                    let k = (b.len() - self.done).min(buf.len());
                    buf.get_mut(..k)
                        .unwrap_or_default()
                        .copy_from_slice(b.get(self.done..self.done + k).unwrap_or_default());
                    self.done += k;
                    if self.done == b.len() {
                        self.next();
                    }
                    k
                }
                Seg::File(src, len) => self.file_bytes(src, len, buf)?,
            };
            if n > 0 {
                return Ok(n);
            }
        }
    }
}

/// Bytes read ahead (files read on another thread while the deflater works), as the channel hands them on.
const AHEAD_CHUNK: usize = 1 << 20;
const AHEAD_CHUNKS: usize = 4;

/// The other end of the read-ahead thread: its chunks as one stream, its error kept.
struct Ahead {
    rx: mpsc::Receiver<Result<Vec<u8>, PackError>>,
    chunk: Vec<u8>,
    at: usize,
    err: Option<PackError>,
}

impl io::Read for Ahead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        while self.at == self.chunk.len() {
            match self.rx.recv() {
                Ok(Ok(c)) => {
                    self.chunk = c;
                    self.at = 0;
                }
                Ok(Err(e)) => {
                    let msg = e.to_string();
                    self.err = Some(e);
                    return Err(io::Error::other(msg));
                }
                Err(_) => return Ok(0), // the reader finished
            }
        }
        let k = (self.chunk.len() - self.at).min(buf.len());
        buf.get_mut(..k)
            .unwrap_or_default()
            .copy_from_slice(self.chunk.get(self.at..self.at + k).unwrap_or_default());
        self.at += k;
        Ok(k)
    }
}

/// The gzip member of no data as DICE's packer wrote it after the header of each of its 105 zipped bundles whose
/// payload is empty (FBRB-DEL-2): zlib's gzip header (time 0, extra flags 0, OS 0x0B), the empty final fixed block
/// `03 00`, a zero CRC-32 and a zero length. .NET's `GZipStream` (FbrbTool) writes nothing for no data.
pub const EMPTY_GZIP_MEMBER: [u8; 20] = [
    0x1F, 0x8B, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0x0B, 0x03, 0x00, 0, 0, 0, 0, 0, 0, 0, 0,
];

/// The payload's bytes through `out`: gzip'd for a zipped layout, else as they are. A zipped payload of no bytes
/// is DICE's empty member ([`EMPTY_GZIP_MEMBER`]): every zipped bundle of DICE's has a payload member.
fn copy_payload(input: &mut dyn io::Read, out: &mut dyn Write, zipped: bool) -> io::Result<u64> {
    if zipped {
        let n = gzip_net_stream(input, out)?;
        if n == 0 {
            out.write_all(&EMPTY_GZIP_MEMBER)?;
        }
        Ok(n)
    } else {
        io::copy(input, out)
    }
}

/// The bundle of laid-out entries written to `out`: the header part, then the payload made from its pieces as it is
/// written (files read ahead on a second thread when there are any). Holds about 64 KB of deflate window, a few
/// blocks of output and `AHEAD_CHUNKS` MB read ahead - not the payload.
fn write_bundle<'a>(
    names: impl Iterator<Item = (&'a str, &'a str)>,
    p: &Payload<'_>,
    layout: Layout,
    out: &mut dyn Write,
) -> Result<(), StreamError> {
    let part1 = header_part(names, p, layout).map_err(StreamError::Pack)?;
    out.write_all(&part1).map_err(StreamError::Write)?;
    let files = p.segs.iter().any(|s| matches!(s, Seg::File(..)));
    let (copied, err) = if files {
        std::thread::scope(|scope| {
            let (tx, rx) = mpsc::sync_channel(AHEAD_CHUNKS);
            let segs = &p.segs;
            scope.spawn(move || {
                let mut r = SegReader::new(segs);
                loop {
                    let mut chunk = vec![0u8; AHEAD_CHUNK];
                    let mut n = 0;
                    while n < chunk.len() {
                        match io::Read::read(&mut r, chunk.get_mut(n..).unwrap_or_default()) {
                            Ok(0) | Err(_) => break, // the end, or the error kept in r.err
                            Ok(k) => n += k,
                        }
                    }
                    if let Some(e) = r.err.take() {
                        let _ = tx.send(Err(e));
                        return;
                    }
                    if n == 0 {
                        return; // the end
                    }
                    chunk.truncate(n);
                    if tx.send(Ok(chunk)).is_err() {
                        return; // the writer stopped
                    }
                }
            });
            let mut ahead = Ahead {
                rx,
                chunk: Vec::new(),
                at: 0,
                err: None,
            };
            let copied = copy_payload(&mut ahead, out, layout.zipped);
            // dropping the receiver ends the reader at its next chunk
            (copied, ahead.err)
        })
    } else {
        let mut r = SegReader::new(&p.segs);
        let copied = copy_payload(&mut r, out, layout.zipped);
        (copied, r.err)
    };
    match (copied, err) {
        (_, Some(e)) => Err(StreamError::Pack(e)),
        (Err(e), None) => Err(StreamError::Write(e)),
        (Ok(n), None) if n != p.len as u64 => Err(StreamError::Pack(PackError::Io {
            path: PathBuf::new(),
            source: io::Error::other("the payload changed length while it was packed"),
        })),
        (Ok(_), None) => Ok(()),
    }
}

/// A bundle of these entries in this order, laid out as FbrbTool lays out a manifest's entries (each item's
/// flags and size2 as given).
///
/// # Errors
/// [`PackError::TooLarge`] when the payload, the string table or a field passes 4 GB.
///
/// ```
/// use bc2_formats::fbrb::{self, Layout, PackItem};
///
/// let items = [PackItem { name: "Levels/X.dbx", type_name: "<non-resource>", flags: 0x10000, size2: 3, data: b"abc" }];
/// let bundle = fbrb::pack(&items, &Layout::default())?;
/// let (header, data) = fbrb::extract(&mut std::io::Cursor::new(&bundle)).expect("a bundle");
/// assert_eq!((header.entries[0].name.as_str(), data[0].as_slice()), ("Levels/X.dbx", &b"abc"[..]));
/// # Ok::<(), fbrb::PackError>(())
/// ```
pub fn pack(items: &[PackItem<'_>], layout: &Layout) -> PackResult<Vec<u8>> {
    let mut p = Payload::new();
    for it in items {
        p.pad(*layout, is_marker(it.type_name, it.data.len()));
        let offset = p.offset()?;
        p.push(Seg::Bytes(it.data), it.data.len());
        p.rows.push([it.flags, offset, u32_of(it.data.len())?, it.size2]);
    }
    let mut out = Vec::new();
    match write_bundle(items.iter().map(|i| (i.name, i.type_name)), &p, *layout, &mut out) {
        Ok(()) => Ok(out),
        Err(StreamError::Pack(e)) => Err(e),
        Err(StreamError::Write(e)) => Err(PackError::Io {
            path: PathBuf::new(),
            source: e,
        }),
    }
}

/// One entry line of a manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManifestLine {
    /// The archive name, as written (also the file's path under the folder).
    pub name: String,
    /// The resource type.
    pub type_name: String,
    /// The flags word.
    pub flags: u32,
    /// The size2 the bundle had.
    pub size2: u32,
    /// The size the entry unpacked to (`None` in a manifest from before the size column).
    pub size: Option<u32>,
}

impl ManifestLine {
    /// The size2 the entry is packed with when its data is `len` bytes long (FbrbTool's rule: the manifest's size2
    /// while the data has its unpacked size, size - 64 for an edited entry of a `size2=minus64` bundle, else the
    /// size).
    ///
    /// # Errors
    /// [`PackError::OldManifest`] for a manifest without the size column whose size2 differs from `len`.
    pub fn size2_for(&self, len: usize, layout: &Layout) -> PackResult<u32> {
        let size = u32_of(len)?;
        let unpacked = match self.size {
            Some(s) => s,
            None if self.size2 == size => size,
            None => return Err(PackError::OldManifest(self.name.clone())),
        };
        let unchanged = size == unpacked;
        if layout.size2_minus64 && !is_marker(&self.type_name, len) && !unchanged && size >= 64 {
            return Ok(size - 64);
        }
        Ok(if self.size2 != size && unchanged {
            self.size2
        } else {
            size
        })
    }
}

/// A parsed `fbrb.manifest`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The first line's layout.
    pub layout: Layout,
    /// The entries in archive order.
    pub lines: Vec<ManifestLine>,
}

/// .NET's white space for `uint.Parse` (`NumberStyles.Integer`).
fn net_trim(s: &str) -> &str {
    s.trim_matches(|c: char| matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' '))
}

/// `int.Parse` / `uint.Parse` (`NumberStyles.Integer`): white space around, a leading sign, decimal digits.
fn net_int(s: &str) -> Option<i64> {
    let t = net_trim(s);
    let (neg, digits) = match t.as_bytes().first() {
        Some(b'-') => (true, t.get(1..)?),
        Some(b'+') => (false, t.get(1..)?),
        _ => (false, t),
    };
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let mut v: i64 = 0;
    for b in digits.bytes() {
        v = v.checked_mul(10)?.checked_add(i64::from(b - b'0'))?;
    }
    Some(if neg { -v } else { v })
}

fn net_uint(s: &str) -> Option<u32> {
    net_int(s).and_then(|v| u32::try_from(v).ok())
}

/// `Convert.ToUInt32(s, 16)`: hex digits, an optional `0x` prefix, at most 32 bits.
fn net_hex(s: &str) -> Option<u32> {
    let d = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
    if d.is_empty() || !d.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    match d.trim_start_matches('0') {
        "" => Some(0),
        t => u32::from_str_radix(t, 16).ok(),
    }
}

/// The value of `key=` in a line of space-separated tokens (FbrbTool's `Token`).
#[must_use]
pub fn token<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    line.split(' ')
        .find_map(|t| t.strip_prefix(key).and_then(|r| r.strip_prefix('=')))
}

/// The text of a manifest file as `File.ReadAllLines` decodes it: a UTF-8, UTF-16 or UTF-32 byte-order mark's
/// encoding, else UTF-8 (invalid bytes as U+FFFD).
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    let utf16 = |b: &[u8], le: bool| {
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|&pair| {
                if le {
                    u16::from_le_bytes(pair)
                } else {
                    u16::from_be_bytes(pair)
                }
            })
            .collect();
        String::from_utf16_lossy(&units)
    };
    let utf32 = |b: &[u8], le: bool| {
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|&w| {
                let v = if le {
                    u32::from_le_bytes(w)
                } else {
                    u32::from_be_bytes(w)
                };
                char::from_u32(v).unwrap_or('\u{FFFD}')
            })
            .collect()
    };
    match bytes {
        [0xEF, 0xBB, 0xBF, rest @ ..] => String::from_utf8_lossy(rest).into_owned(),
        [0xFF, 0xFE, 0, 0, rest @ ..] => utf32(rest, true),
        [0, 0, 0xFE, 0xFF, rest @ ..] => utf32(rest, false),
        [0xFF, 0xFE, rest @ ..] => utf16(rest, true),
        [0xFE, 0xFF, rest @ ..] => utf16(rest, false),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

/// Lines as `StreamReader.ReadLine` splits them: at CR LF, LF or CR; no empty line after a final line end.
#[must_use]
pub fn lines(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut rest = text;
    while !rest.is_empty() {
        let Some(i) = rest.find(['\r', '\n']) else {
            out.push(rest);
            break;
        };
        out.push(rest.get(..i).unwrap_or(""));
        let skip = if rest.get(i..i + 2) == Some("\r\n") { 2 } else { 1 };
        rest = rest.get(i + skip..).unwrap_or("");
    }
    out
}

/// A manifest's text as FbrbTool's `Pack` reads it.
///
/// # Errors
/// [`PackError::Manifest`] for an empty manifest, an entry line with fewer than four fields, or a flags, size2,
/// size or `hdr=` value that does not parse.
///
/// ```
/// let m = bc2_formats::fbrb::parse_manifest("# fbrb.manifest  zipped=1\nA.dbx|<non-resource>|00010000|3|3\n")?;
/// assert!(m.layout.zipped);
/// assert_eq!((m.lines[0].name.as_str(), m.lines[0].flags, m.lines[0].size), ("A.dbx", 0x10000, Some(3)));
/// # Ok::<(), bc2_formats::fbrb::PackError>(())
/// ```
pub fn parse_manifest(text: &str) -> PackResult<Manifest> {
    let all = lines(text);
    let first = *all.first().ok_or_else(|| PackError::Manifest {
        line: 1,
        what: "the manifest is empty".into(),
    })?;
    let zipped = first.contains("zipped=1");
    let bad = |line: usize, what: String| PackError::Manifest { line, what };
    let header_align = match token(first, "hdr") {
        Some(v) => net_int(v)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| bad(1, format!("hdr={v} is not a number")))?,
        None => 1,
    };
    let padding = if zipped {
        None
    } else {
        match token(first, "layout") {
            Some("align4096") => Some(Padding::Align4096),
            Some("gap64") => Some(Padding::Gap64),
            _ => None,
        }
    };
    let layout = Layout {
        zipped,
        padding,
        header_align,
        plen_size2: token(first, "plen") == Some("size2"),
        size2_minus64: token(first, "size2") == Some("minus64"),
    };
    let mut out = Vec::new();
    for (i, line) in all.iter().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let n = i + 1;
        let p: Vec<&str> = line.split('|').collect();
        let [name, type_name, flags, size2, rest @ ..] = p.as_slice() else {
            return Err(bad(
                n,
                format!("{} field(s), name|type|flags|size2[|size] expected", p.len()),
            ));
        };
        out.push(ManifestLine {
            name: (*name).to_owned(),
            type_name: (*type_name).to_owned(),
            flags: net_hex(flags).ok_or_else(|| bad(n, format!("flags {flags} is not a hex number")))?,
            size2: net_uint(size2).ok_or_else(|| bad(n, format!("size2 {size2} is not a number")))?,
            size: match rest.first() {
                Some(s) => Some(net_uint(s).ok_or_else(|| bad(n, format!("size {s} is not a number")))?),
                None => None,
            },
        });
    }
    Ok(Manifest { layout, lines: out })
}

/// The file of entry `name` under `dir` (FbrbTool's `Safe`): either slash separates folders; an empty name, a
/// rooted one (`\x`, `/x`, `C:x`) or one whose `..` leads out of `dir` is refused.
///
/// # Errors
/// [`PackError::UnsafeName`].
pub fn entry_path(dir: &Path, name: &str) -> PackResult<PathBuf> {
    let unsafe_name = || PackError::UnsafeName(name.to_owned());
    if name.is_empty() || name.starts_with(['/', '\\']) || name.as_bytes().get(1) == Some(&b':') {
        return Err(unsafe_name());
    }
    let mut parts: Vec<&str> = Vec::new();
    for c in name.split(['/', '\\']) {
        match c {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or_else(unsafe_name)?;
            }
            c => parts.push(c),
        }
    }
    Ok(parts.iter().fold(dir.to_path_buf(), |p, c| p.join(c)))
}

fn io_err(path: &Path) -> impl FnOnce(io::Error) -> PackError + '_ {
    move |source| PackError::Io {
        path: path.to_path_buf(),
        source,
    }
}

/// The bundle FbrbTool's `pack` makes of `dir` (its `fbrb.manifest` and files), in memory.
///
/// # Errors
/// [`PackError::Io`] for a manifest or entry file that cannot be read (a marker's file may be missing), the
/// manifest's errors ([`parse_manifest`], [`ManifestLine::size2_for`]), [`PackError::UnsafeName`],
/// [`PackError::TooLarge`].
pub fn pack_dir(dir: impl AsRef<Path>) -> PackResult<(Vec<u8>, PackStats)> {
    let (manifest, rows) = dir_rows(dir.as_ref())?;
    pack_rows(&manifest, &rows)
}

/// `dir`'s manifest and the rows [`pack_rows`] takes for it (each file checked, its size2 decided).
///
/// # Errors
/// The manifest or a listed file cannot be read, or the manifest is not valid.
pub fn dir_rows(dir: &Path) -> PackResult<(Manifest, Vec<SourceRow>)> {
    let mpath = dir.join("fbrb.manifest");
    let manifest = parse_manifest(&decode_text(&fs::read(&mpath).map_err(io_err(&mpath))?))?;
    let layout = manifest.layout;
    let mut rows = Vec::with_capacity(manifest.lines.len());
    for l in &manifest.lines {
        let src = entry_path(dir, &l.name)?;
        // a marker has no file since FbrbTool's unpack stopped writing them (an empty one from an older unpack is
        // read as before)
        let (src, len) = if l.type_name == DELETED && !src.is_file() {
            (None, 0)
        } else {
            let m = fs::metadata(&src).map_err(io_err(&src))?;
            if !m.is_file() {
                return Err(io_err(&src)(io::Error::other("not a file")));
            }
            (Some(src), usize::try_from(m.len()).map_err(|_| PackError::TooLarge)?)
        };
        let size2 = l.size2_for(len, &layout)?;
        rows.push(SourceRow { src, len, size2 });
    }
    Ok((manifest, rows))
}

/// One manifest entry's data as [`pack_rows`] takes it: its file (none for a marker without one), the file's length
/// and the size2 it is packed with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRow {
    /// The entry's file.
    pub src: Option<PathBuf>,
    /// The file's length (0 without a file).
    pub len: usize,
    /// The entry's size2 in the bundle.
    pub size2: u32,
}

/// The bundle of `manifest`'s entries, each `rows[i]` (the files checked and size2 decided by the caller, as
/// [`pack_dir`] or FbrbTool's command line do), in memory: [`pack_rows_into`] into a `Vec` (the bundle's bytes are
/// held, not its payload).
///
/// # Errors
/// [`PackError::Io`] for a file that cannot be read or changed length, [`PackError::TooLarge`].
pub fn pack_rows(manifest: &Manifest, rows: &[SourceRow]) -> PackResult<(Vec<u8>, PackStats)> {
    let mut out = Vec::new();
    let stats = pack_rows_into(manifest, rows, &mut out, Path::new(""))?;
    Ok((out, stats))
}

/// [`pack_rows`] written to `out` as it is made (round S1): each file is read when the payload reaches it and
/// streamed into the deflater, so a bundle of any size holds about 10 MB (the deflate window, a few blocks of
/// output, the files read ahead) instead of its whole payload (1.5 GB for a WW2 level). The bytes are
/// [`pack_rows`]'s. `out_path` names the output in a write error.
///
/// # Errors
/// As [`pack_rows`], and [`PackError::Io`] (with `out_path`) when `out` fails.
pub fn pack_rows_into(
    manifest: &Manifest,
    rows: &[SourceRow],
    out: &mut dyn Write,
    out_path: &Path,
) -> PackResult<PackStats> {
    pack_rows_stream(manifest, rows, out).map_err(|e| match e {
        StreamError::Pack(e) => e,
        StreamError::Write(source) => PackError::Io {
            path: out_path.to_path_buf(),
            source,
        },
    })
}

/// [`pack_rows_into`], a write error told apart from the packing's (FbrbTool's command line reports them apart).
///
/// # Errors
/// [`StreamError::Pack`] for the packing's own error, [`StreamError::Write`] for the output's.
pub fn pack_rows_stream(
    manifest: &Manifest,
    rows: &[SourceRow],
    out: &mut dyn Write,
) -> Result<PackStats, StreamError> {
    let layout = manifest.layout;
    let mut p = Payload::new();
    for (l, r) in manifest.lines.iter().zip(rows) {
        p.pad(layout, is_marker(&l.type_name, r.len));
        let offset = p.offset().map_err(StreamError::Pack)?;
        p.rows
            .push([l.flags, offset, u32_of(r.len).map_err(StreamError::Pack)?, r.size2]);
        if let (Some(src), true) = (&r.src, r.len > 0) {
            p.push(Seg::File(src, r.len), r.len);
        }
    }
    let names = manifest.lines.iter().map(|l| (l.name.as_str(), l.type_name.as_str()));
    write_bundle(names, &p, layout, out)?;
    Ok(PackStats {
        entries: manifest.lines.len(),
        payload_len: p.len as u64,
    })
}

/// The file at `src` into `dst`, which has its size: a file that changed size since is an error.
#[cfg(test)]
fn read_exactly(src: &Path, dst: &mut [u8]) -> PackResult<()> {
    let mut f = fs::File::open(src).map_err(io_err(src))?;
    f.read_exact(dst).map_err(io_err(src))?;
    let mut more = [0u8; 1];
    if f.read(&mut more).map_err(io_err(src))? != 0 {
        return Err(PackError::Io {
            path: src.to_path_buf(),
            source: io::Error::other("the file grew while it was packed"),
        });
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::fbrb::{extract, read_header};

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("bc2-pack-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn numbers_as_dotnet_parses_them() {
        assert_eq!(net_uint(" +12\t"), Some(12));
        assert_eq!(net_uint("-0"), Some(0));
        assert_eq!(net_uint("-1"), None);
        assert_eq!(net_uint("4294967296"), None);
        assert_eq!(net_uint("1 2"), None);
        assert_eq!(net_uint(""), None);
        assert_eq!(net_uint("+"), None);
        assert_eq!(net_int("99999999999999999999"), None);
        assert_eq!(net_hex("00010000"), Some(0x10000));
        assert_eq!(net_hex("0xFF"), Some(255));
        assert_eq!(net_hex("000000000000000A"), Some(10));
        assert_eq!(net_hex("100000000"), None);
        assert_eq!(net_hex("0x"), None);
        assert_eq!(net_hex("g"), None);
    }

    #[test]
    fn lines_and_encodings() {
        assert_eq!(lines("a\r\nb\rc\n\nd"), ["a", "b", "c", "", "d"]);
        assert_eq!(lines("a\n"), ["a"]);
        assert_eq!(lines(""), Vec::<&str>::new());
        assert_eq!(decode_text(b"\xEF\xBB\xBFx"), "x");
        assert_eq!(decode_text(b"\xFF\xFEx\0"), "x");
        assert_eq!(decode_text(b"\xFE\xFF\0x"), "x");
        assert_eq!(decode_text(b"\xFF\xFE\0\0x\0\0\0"), "x");
        assert_eq!(decode_text(b"\0\0\xFE\xFF\0\0\0x"), "x");
        assert_eq!(decode_text(b"a\xFFb"), "a\u{FFFD}b");
    }

    #[test]
    fn manifest_tokens() {
        let m = parse_manifest(
            "# x zipped=0 layout=align4096 hdr=4096 plen=size2 size2=minus64\n# comment\n\nA|T|1|2\nB|T|0x2|3|4",
        )
        .unwrap();
        assert_eq!(
            m.layout,
            Layout {
                zipped: false,
                padding: Some(Padding::Align4096),
                header_align: 4096,
                plen_size2: true,
                size2_minus64: true
            }
        );
        assert_eq!(m.lines.len(), 2);
        assert_eq!((m.lines[0].size, m.lines[1].flags, m.lines[1].size), (None, 2, Some(4)));
        // a zipped bundle has no padding; an unknown layout is none; hdr is read whatever the payload
        let z = parse_manifest("# zipped=1 layout=gap64 hdr=4").unwrap().layout;
        assert_eq!((z.padding, z.header_align), (None, 4));
        let u = parse_manifest("# zipped=0 layout=other").unwrap().layout;
        assert_eq!(u.padding, None);
        assert_eq!(
            parse_manifest("# zipped=0 layout=gap64").unwrap().layout.padding,
            Some(Padding::Gap64)
        );
        // a first line that is not a comment is an entry too
        assert_eq!(parse_manifest("A|T|1|1").unwrap().lines.len(), 1);
        for bad in [
            "",
            "# h\nA|T|1",
            "# h\nA|T|x|1",
            "# h\nA|T|1|-1",
            "# h\nA|T|1|1|z",
            "# hdr=x",
        ] {
            assert!(
                matches!(parse_manifest(bad), Err(PackError::Manifest { .. })),
                "{bad:?}"
            );
        }
        assert!(
            PackError::Manifest {
                line: 2,
                what: "x".into()
            }
            .to_string()
            .contains("line 2")
        );
    }

    #[test]
    fn size2_rules() {
        let l = |t: &str, size2, size| ManifestLine {
            name: "n".into(),
            type_name: t.into(),
            flags: 0,
            size2,
            size,
        };
        let plain = Layout::default();
        let m64 = Layout {
            size2_minus64: true,
            ..plain
        };
        assert_eq!(l("T", 90, Some(100)).size2_for(100, &plain).unwrap(), 90); // unchanged: kept
        assert_eq!(l("T", 90, Some(100)).size2_for(120, &plain).unwrap(), 120); // edited: the size
        assert_eq!(l("T", 36, Some(100)).size2_for(100, &m64).unwrap(), 36);
        assert_eq!(l("T", 36, Some(100)).size2_for(200, &m64).unwrap(), 136);
        assert_eq!(l("T", 36, Some(100)).size2_for(50, &m64).unwrap(), 50);
        assert_eq!(l(DELETED, 0, Some(1)).size2_for(0, &m64).unwrap(), 0);
        assert_eq!(l("T", 5, None).size2_for(5, &plain).unwrap(), 5);
        let old = l("T", 5, None).size2_for(6, &plain).unwrap_err();
        assert!(matches!(old, PackError::OldManifest(_)) && old.to_string().contains("older unpack"));
    }

    #[test]
    fn safe_names() {
        let d = Path::new("root");
        assert_eq!(
            entry_path(d, "a/b\\c.dbx").unwrap(),
            d.join("a").join("b").join("c.dbx")
        );
        assert_eq!(entry_path(d, "a/./x/../b").unwrap(), d.join("a").join("b"));
        for bad in ["", "/a", "\\a", "C:a", "..", "a/../../b"] {
            assert!(matches!(entry_path(d, bad), Err(PackError::UnsafeName(_))), "{bad:?}");
        }
    }

    #[test]
    fn padded_layouts() {
        let data = [vec![1u8; 10], vec![], vec![2u8; 5000], vec![3u8; 70]];
        let ty = ["T", DELETED, "T", "T"];
        let items: Vec<PackItem<'_>> = data
            .iter()
            .zip(ty)
            .enumerate()
            .map(|(i, (d, t))| PackItem {
                name: ["a", "gone", "b", "c"][i],
                type_name: t,
                flags: 0x10000,
                size2: d.len().saturating_sub(64) as u32,
                data: d,
            })
            .collect();
        for (padding, offsets) in [
            (Padding::Align4096, [0u32, 10, 4096, 12288]),
            (Padding::Gap64, [0, 10, 74, 5138]),
        ] {
            let layout = Layout {
                zipped: false,
                padding: Some(padding),
                header_align: 4096,
                plen_size2: true,
                size2_minus64: true,
            };
            let b = pack(&items, &layout).unwrap();
            let h = read_header(&mut b.as_slice()).unwrap();
            assert_eq!(h.entries.iter().map(|e| e.offset).collect::<Vec<_>>(), offsets);
            assert_eq!(h.payload_start % 4096, 0);
            assert_eq!(h.payload_len, Some(offsets[3] + 6));
            let (_, got) = extract(&mut Cursor::new(&b)).unwrap();
            assert_eq!(got, data);
            assert_eq!(b[h.payload_start as usize + 10], 0xBF);
        }
        // a padding layout of a zipped bundle is not applied, and a bundle of markers has no live entry for plen
        let z = Layout {
            padding: Some(Padding::Gap64),
            plen_size2: true,
            ..Layout::default()
        };
        let gone = [PackItem {
            name: "x",
            type_name: DELETED,
            flags: 0,
            size2: 0,
            data: b"",
        }];
        let h = read_header(&mut pack(&gone, &z).unwrap().as_slice()).unwrap();
        assert_eq!(h.payload_len, Some(0));
        assert!(pack(&items, &z).unwrap().len() < 1000);
    }

    /// An output that always fails.
    struct Full;

    impl Write for Full {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("disk full"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    /// Round S1: files streamed through the read-ahead thread (several chunks, entries across chunk boundaries)
    /// give the bytes the same entries give from memory, in every layout; a file that changed size since its row
    /// was made is an error naming it, and `pack_path` leaves no temporary file.
    #[test]
    fn streamed_files_as_from_memory() {
        let d = temp("stream");
        let sizes = [0usize, 1, 3 << 20, 70, 5000, 1 << 20, 0, 123_457];
        let data: Vec<Vec<u8>> = sizes
            .iter()
            .enumerate()
            .map(|(k, &n)| (0..n).map(|i| ((i * (k + 3)) ^ (i >> 7)) as u8).collect())
            .collect();
        let names: Vec<String> = (0..sizes.len()).map(|k| format!("e{k}.res")).collect();
        for (n, b) in names.iter().zip(&data) {
            fs::write(d.join(n), b).unwrap();
        }
        for first in [
            "# zipped=1",
            "# zipped=0 layout=align4096 hdr=4096 plen=size2",
            "# zipped=0 layout=gap64",
        ] {
            let mut text = format!("{first}\n");
            for (n, b) in names.iter().zip(&data) {
                std::fmt::Write::write_fmt(&mut text, format_args!("{n}|T|00010000|{0}|{0}\n", b.len())).unwrap();
            }
            fs::write(d.join("fbrb.manifest"), &text).unwrap();
            let layout = parse_manifest(&text).unwrap().layout;
            let items: Vec<PackItem<'_>> = names
                .iter()
                .zip(&data)
                .map(|(n, b)| PackItem {
                    name: n,
                    type_name: "T",
                    flags: 0x10000,
                    size2: b.len() as u32,
                    data: b,
                })
                .collect();
            let want = pack(&items, &layout).unwrap();
            let (got, stats) = pack_dir(&d).unwrap();
            assert!(got == want, "{first}");
            assert_eq!(stats.entries, sizes.len());
            if layout.zipped {
                assert_eq!(stats.payload_len as usize, sizes.iter().sum::<usize>());
            }
            let (m, r) = dir_rows(&d).unwrap();
            let mut streamed = Vec::new();
            pack_rows_into(&m, &r, &mut streamed, Path::new("out.fbrb")).unwrap();
            assert!(streamed == want, "{first}");
        }
        // rows made, then the file changed: shorter, longer
        let (manifest, mut rows) = dir_rows(&d).unwrap();
        rows[2].len += 1;
        let e = pack_rows(&manifest, &rows).unwrap_err();
        assert!(
            matches!(&e, PackError::Io { path, .. } if path.ends_with("e2.res")),
            "{e}"
        );
        rows[2].len -= 2;
        let e = pack_rows(&manifest, &rows).unwrap_err();
        assert!(
            e.to_string().contains("grew") && e.to_string().contains("e2.res"),
            "{e}"
        );
        // an output that fails: the error names it
        rows[2].len += 1;
        let e = pack_rows_into(&manifest, &rows, &mut Full, Path::new("x.fbrb")).unwrap_err();
        assert!(
            e.to_string().contains("x.fbrb") && e.to_string().contains("disk full"),
            "{e}"
        );
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn strings_types_once_and_ascii() {
        let items = [
            PackItem {
                name: "é",
                type_name: "T",
                flags: 1,
                size2: 1,
                data: b"x",
            },
            PackItem {
                name: "b",
                type_name: "T",
                flags: 2,
                size2: 1,
                data: b"y",
            },
        ];
        let b = pack(&items, &Layout::default()).unwrap();
        let h = read_header(&mut b.as_slice()).unwrap();
        assert_eq!(h.entries[0].name, "?");
        assert_eq!(h.entries[1].type_name, "T");
        // the gzip header is .NET's
        assert_eq!(&b[8..18], &[0x1F, 0x8B, 8, 0, 0, 0, 0, 0, 4, 0]);
    }

    #[test]
    fn empty_zipped_payload_is_dices_member() {
        let marker = PackItem {
            name: "Levels/MP_005/Terrain",
            type_name: DELETED,
            flags: 0,
            size2: 0,
            data: b"",
        };
        let stored = Layout {
            zipped: false,
            ..Layout::default()
        };
        for (items, layout, member) in [
            (&[marker][..], Layout::default(), true),
            (&[][..], Layout::default(), true),
            (&[marker][..], stored, false),
            (&[][..], stored, false),
        ] {
            let b = pack(items, &layout).unwrap();
            let h = read_header(&mut b.as_slice()).unwrap();
            let end = usize::try_from(h.payload_start).unwrap();
            assert_eq!(h.payload_len, Some(0));
            // DICE's 20 bytes after a zipped header (FBRB-DEL-2), nothing after a stored one
            assert_eq!(&b[end..], if member { &EMPTY_GZIP_MEMBER[..] } else { &[][..] });
            // the member is a valid gzip stream of no data, and the bundle reads back
            if member {
                let mut out = Vec::new();
                io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&b[end..]), &mut out).unwrap();
                assert_eq!(out, Vec::<u8>::new());
            }
            let (_, data) = extract(&mut Cursor::new(&b)).unwrap();
            assert!(data.iter().all(Vec::is_empty));
        }
        // a payload of bytes is .NET's member as before (no empty member after it)
        let live = PackItem {
            data: b"x",
            type_name: "T",
            ..marker
        };
        let b = pack(&[marker, live], &Layout::default()).unwrap();
        assert!(!b.ends_with(&EMPTY_GZIP_MEMBER));
    }

    #[test]
    fn files_that_change_size() {
        let d = temp("sizes");
        let f = d.join("x");
        fs::write(&f, b"abcd").unwrap();
        let mut short = [0u8; 3];
        assert!(read_exactly(&f, &mut short).unwrap_err().to_string().contains("grew"));
        let mut long = [0u8; 5];
        assert!(matches!(read_exactly(&f, &mut long), Err(PackError::Io { .. })));
        let mut exact = [0u8; 4];
        read_exactly(&f, &mut exact).unwrap();
        assert_eq!(&exact, b"abcd");
        // a folder where an entry's file should be
        fs::create_dir_all(d.join("dir.res")).unwrap();
        fs::write(d.join("fbrb.manifest"), "# zipped=1\ndir.res|T|0|0|0\n").unwrap();
        assert!(pack_dir(&d).unwrap_err().to_string().contains("not a file"));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn too_large() {
        assert!(matches!(u32_of(usize::MAX), Err(PackError::TooLarge)));
        assert!(PackError::TooLarge.to_string().contains("4 GB"));
    }
}
