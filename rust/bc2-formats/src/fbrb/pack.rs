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
//!   header region padded with zeros to `hdr`.
//!
//! The manifest is read as `File.ReadAllLines` reads it (UTF-8 or a byte-order mark's encoding; lines end at CR, LF
//! or CR LF), numbers as `uint.Parse` and `Convert.ToUInt32(s, 16)` parse them, and names are stored as
//! `Encoding.ASCII` stores them (`?` for anything outside ASCII). An entry name that is empty, rooted or leads
//! out of the folder is refused, as FbrbTool's `Safe` refuses it.

use std::fs;
use std::io::{self, Read as _};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use thiserror::Error;

use super::header::{DELETED, MAGIC};
use crate::zlib::gzip_net;

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

/// A payload being laid out: the entries' offsets and DICE's padding.
struct Payload {
    data: Vec<u8>,
    rows: Vec<[u32; 4]>, // flags, offset, size, size2
    first: bool,
}

impl Payload {
    fn new(capacity: usize) -> Self {
        Self {
            data: Vec::with_capacity(capacity),
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
                match p {
                    Padding::Align4096 => {
                        let n = self.data.len().next_multiple_of(4096) - self.data.len();
                        self.data.resize(self.data.len() + n, 0xBF);
                    }
                    Padding::Gap64 => self.data.resize(self.data.len() + 64, 0xBF),
                }
            }
            self.first = false;
        }
    }

    fn offset(&self) -> PackResult<u32> {
        u32_of(self.data.len())
    }
}

/// The bundle of laid-out entries: string table, header, the two parts.
fn assemble<'a>(names: impl Iterator<Item = (&'a str, &'a str)>, p: &Payload, layout: Layout) -> PackResult<Vec<u8>> {
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
        _ => u32_of(p.data.len())?,
    };
    header.extend(plen.to_be_bytes());
    let mut part1 = gzip_net(&header);
    if layout.header_align > 1 {
        let a = layout.header_align as usize;
        part1.resize(part1.len() + (a - (8 + part1.len()) % a) % a, 0);
    }
    let compressed;
    let part2: &[u8] = if layout.zipped {
        compressed = gzip_net(&p.data);
        &compressed
    } else {
        &p.data
    };
    let mut out = Vec::with_capacity(8 + part1.len() + part2.len());
    out.extend_from_slice(MAGIC);
    out.extend(u32_of(part1.len())?.to_be_bytes());
    out.extend(part1);
    out.extend_from_slice(part2);
    Ok(out)
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
    let mut p = Payload::new(items.iter().map(|i| i.data.len()).sum());
    for it in items {
        p.pad(*layout, is_marker(it.type_name, it.data.len()));
        let offset = p.offset()?;
        p.data.extend_from_slice(it.data);
        p.rows.push([it.flags, offset, u32_of(it.data.len())?, it.size2]);
    }
    assemble(items.iter().map(|i| (i.name, i.type_name)), &p, *layout)
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
pub(crate) fn token<'a>(line: &'a str, key: &str) -> Option<&'a str> {
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
pub(crate) fn lines(text: &str) -> Vec<&str> {
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
    let dir = dir.as_ref();
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
    pack_rows(&manifest, &rows)
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
/// [`pack_dir`] or FbrbTool's command line do), its files read on all cores.
///
/// # Errors
/// [`PackError::Io`] for a file that cannot be read or changed length, [`PackError::TooLarge`].
pub fn pack_rows(manifest: &Manifest, rows: &[SourceRow]) -> PackResult<(Vec<u8>, PackStats)> {
    let layout = manifest.layout;
    // the layout from the files' sizes, then every file read into its place on all cores
    let mut p = Payload::new(rows.iter().map(|r| r.len).sum());
    let mut reads: Vec<(&Path, usize, usize)> = Vec::new();
    for (l, r) in manifest.lines.iter().zip(rows) {
        p.pad(layout, is_marker(&l.type_name, r.len));
        let offset = p.offset()?;
        p.rows.push([l.flags, offset, u32_of(r.len)?, r.size2]);
        if let (Some(src), true) = (&r.src, r.len > 0) {
            reads.push((src, p.data.len(), r.len));
            p.data.resize(p.data.len() + r.len, 0);
        }
    }
    let mut jobs = Vec::with_capacity(reads.len());
    let mut rest: &mut [u8] = &mut p.data;
    let mut at = 0;
    for (src, offset, len) in &reads {
        let (_, tail) = std::mem::take(&mut rest).split_at_mut(offset - at);
        let (dst, tail) = tail.split_at_mut(*len);
        jobs.push((*src, dst));
        rest = tail;
        at = offset + len;
    }
    jobs.into_par_iter().try_for_each(|(src, dst)| read_exactly(src, dst))?;
    let names = manifest.lines.iter().map(|l| (l.name.as_str(), l.type_name.as_str()));
    let bytes = assemble(names, &p, layout)?;
    let stats = PackStats {
        entries: manifest.lines.len(),
        payload_len: p.data.len() as u64,
    };
    Ok((bytes, stats))
}

/// The file at `src` into `dst`, which has its size: a file that changed size since is an error.
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

/// [`pack_dir`] written to `out` as FbrbTool writes it: to `<out>.tmp-<process id>` beside it, then moved over
/// `out`, so a pack cut short never leaves a truncated bundle where the old one was.
///
/// # Errors
/// As [`pack_dir`], and [`PackError::Io`] when the bundle cannot be written.
pub fn pack_path(dir: impl AsRef<Path>, out: impl AsRef<Path>) -> PackResult<PackStats> {
    let out = out.as_ref();
    let (bytes, stats) = pack_dir(dir)?;
    let mut tmp = out.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let written = fs::write(&tmp, &bytes).and_then(|()| crate::atomic::replace(&tmp, out));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written.map_err(io_err(out))?;
    Ok(stats)
}

/// [`pack_path`] of each `(folder, bundle)`, in parallel (one bundle per task). Results keep the input order.
#[must_use]
pub fn pack_many<D: AsRef<Path> + Sync, O: AsRef<Path> + Sync>(items: &[(D, O)]) -> Vec<PackResult<PackStats>> {
    items.par_iter().map(|(d, o)| pack_path(d, o)).collect()
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
    fn packs_a_folder() {
        let d = temp("dir");
        fs::create_dir_all(d.join("Levels")).unwrap();
        fs::write(d.join("Levels").join("a.dbx"), b"hello").unwrap();
        fs::write(d.join("b.res"), vec![9u8; 100]).unwrap();
        fs::write(
            d.join("fbrb.manifest"),
            "\u{feff}# fbrb.manifest zipped=1\r\nLevels/a.dbx|<non-resource>|00010000|5|5\r\nGone|*deleted*|00000000|0|0\r\nb.res|R|00010000|7|100\r\n",
        )
        .unwrap();
        let out = d.join("out.fbrb");
        fs::write(&out, b"old").unwrap();
        let stats = pack_path(&d, &out).unwrap();
        assert_eq!(
            stats,
            PackStats {
                entries: 3,
                payload_len: 105
            }
        );
        let (h, data) = extract(&mut Cursor::new(fs::read(&out).unwrap())).unwrap();
        assert_eq!(h.entries[2].size2, 7);
        assert_eq!(data[0], b"hello");
        assert!(h.entries[1].is_deleted());
        let many = pack_many(&[(&d, d.join("again.fbrb"))]);
        assert_eq!(many[0].as_ref().unwrap().entries, 3);
        assert_eq!(fs::read(d.join("again.fbrb")).unwrap(), fs::read(&out).unwrap());

        // errors: a missing file, a missing manifest, an unsafe name, an unwritable bundle
        fs::write(d.join("fbrb.manifest"), "# zipped=1\nmissing|T|0|1|1\n").unwrap();
        assert!(matches!(pack_dir(&d), Err(PackError::Io { .. })));
        fs::write(d.join("fbrb.manifest"), "# zipped=1\n../x|T|0|1|1\n").unwrap();
        assert!(matches!(pack_dir(&d), Err(PackError::UnsafeName(_))));
        fs::write(d.join("fbrb.manifest"), "# zipped=1\nb.res|T|0|1|1\n").unwrap();
        let e = pack_path(&d, d.join("no").join("such").join("x.fbrb")).unwrap_err();
        assert!(matches!(e, PackError::Io { .. }) && e.to_string().contains("x.fbrb"));
        assert!(matches!(pack_dir(d.join("nothing")), Err(PackError::Io { .. })));
        let _ = fs::remove_dir_all(&d);
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
