//! Battlefield 1943's Xbox 360 bundles (`.fbrb`, version 2 of the format, big-endian): the one reader of the suite
//! (round PR1b-1-rest), used by `bf1943import.Source` (through `bc2_native`) and by `bfbc2 bf1943import` / `texqa`.
//!
//! - [`check`]: the BF1943 tool suite's `fbrb1943.Bundle(raw)` verdict on a whole file (the bundles it refuses
//!   left out by `Source`), a port of the version whose SHA-1 is [`READER_SHA1`];
//! - [`gunzip`]: Python's `gzip.decompress` (every member, zero padding between members skipped);
//! - [`Payloads`]: an entry's bytes from a compressed payload through restart points ([`crate::restart`]): the
//!   payload is decompressed once (by any process: the points are kept on disk, by the payload's content), then
//!   any entry costs at most a span (1 MiB) of inflating; the points and the decoded spans of the last reads are
//!   kept in the process within their budgets (`BF1943_RESTARTS_MB`, `BF1943_SOURCE_MEMORY_MB`).
//!
//! A payload is read through points only when it is one plain gzip member as zlib reads it
//! (`zlib.decompressobj(31)`: its trailer checked, nothing after it) decoding to the length the bundle's directory
//! gives; any other is decompressed whole on each read, as `gzip.decompress` reads it (`bf1943import.Source`'s
//! rule). Every span read is checked against the CRC-32 recorded when its points were made
//! ([`crate::restart::decode_span`]): a changed file is refused, never read as other bytes.

#![allow(
    clippy::indexing_slicing,
    reason = "fbrb1943.Bundle's reads: every index after its bound is checked, as the reader checks them"
)]

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use crate::restart::{self, Index, RestartError, Spans};

/// `fbrb1943.py`'s SHA-1 (bf1943-tool-suite e56d878): the reader [`check`] follows.
pub const READER_SHA1: &str = "ad8238401cd380f834b2b1ae30606a8ca4783db3";
/// The output between a compressed payload's restart points (`bf1943import.SPAN`).
pub const SPAN: u64 = restart::SPAN;
/// The environment variable bounding the process's restart points, MiB (`bf1943import.RESTARTS_ENV`).
pub const RESTARTS_ENV: &str = "BF1943_RESTARTS_MB";
/// Its default.
pub const RESTARTS_MB: f64 = 128.0;
/// The environment variable bounding the decoded spans kept, MiB (`bf1943import.MEMORY_ENV`).
pub const MEMORY_ENV: &str = "BF1943_SOURCE_MEMORY_MB";
/// Its default.
pub const MEMORY_MB: f64 = 128.0;
/// A points file's first bytes.
const POINTS_MAGIC: &[u8; 8] = b"BC2BF43P";

/// A directory entry (`fbrb1943.Bundle.entries`' dicts).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The resource's name.
    pub name: String,
    /// Its type.
    pub kind: String,
    /// The entry's flags.
    pub flags: u32,
    /// Where its bytes start in the payload.
    pub offset: u32,
    /// Its length.
    pub size: u32,
}

/// What `fbrb1943.Bundle(raw)` makes of a file it takes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Checked {
    /// The entries, in the directory's order.
    pub entries: Vec<Entry>,
    /// Whether the payload is gzip.
    pub compressed: bool,
    /// Where the payload starts in the file.
    pub payload_at: u64,
    /// The payload's (decompressed) length.
    pub payload_len: u64,
}

/// `gzip.decompress(data)` (Python 3.13: every member, zero padding between members stripped), or `None` where it
/// raises.
#[must_use]
pub fn gunzip(mut data: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    loop {
        if data.is_empty() {
            return Some(out);
        }
        if data.len() < 2 || data[..2] != [0x1f, 0x8b] {
            return None;
        }
        let head = data.get(..10)?;
        if head[2] != 8 {
            return None;
        }
        let flag = head[3];
        let mut p = 10usize;
        if flag & 4 != 0 {
            let n = usize::from(u16::from_le_bytes(data.get(p..p + 2)?.try_into().ok()?));
            p += 2;
            // _read_exact of the extra field
            data.get(p..p + n)?;
            p += n;
        }
        for bit in [8u8, 16] {
            if flag & bit != 0 {
                let z = data.get(p..)?.iter().position(|&c| c == 0)?;
                p += z + 1;
            }
        }
        if flag & 2 != 0 {
            data.get(p..p + 2)?;
            p += 2;
        }
        let body = data.get(p..)?;
        let mut d = flate2::Decompress::new(false);
        let mut member = Vec::new();
        loop {
            member.reserve(1 << 20);
            let before = d.total_in();
            let st = d
                .decompress_vec(
                    body.get(usize::try_from(before).ok()?..)?,
                    &mut member,
                    flate2::FlushDecompress::None,
                )
                .ok()?;
            if st == flate2::Status::StreamEnd {
                break;
            }
            if d.total_in() == before && member.len() < member.capacity() {
                return None; // the stream ends before its end marker
            }
        }
        let rest = body.get(usize::try_from(d.total_in()).ok()?..)?;
        let tail = rest.get(..8)?;
        let crc = u32::from_le_bytes(tail[..4].try_into().ok()?);
        let len = u32::from_le_bytes(tail[4..8].try_into().ok()?);
        let mut h = flate2::Crc::new();
        h.update(&member);
        if crc != h.sum() || len != (member.len() as u64 & 0xFFFF_FFFF) as u32 {
            return None;
        }
        out.extend_from_slice(&member);
        let after = &rest[8..];
        let skip = after.iter().position(|&c| c != 0).unwrap_or(after.len());
        data = &after[skip..];
    }
}

fn be(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)?.try_into().ok().map(u32::from_be_bytes)
}

/// `fbrb1943.Bundle(raw)`'s verdict on a whole bundle file: `None` where it raises `ValueError` (the bundle is
/// refused). The payload is decompressed to check it and not kept.
#[must_use]
pub fn check(raw: &[u8]) -> Option<Checked> {
    if raw.get(..4) != Some(b"FbRB".as_slice()) || raw.len() < 8 {
        return None;
    }
    let cut = be(raw, 4)? as usize;
    if 8 + cut > raw.len() {
        return None;
    }
    let dir = gunzip(&raw[8..8 + cut])?;
    if dir.len() < 12 || be(&dir, 0)? != 2 {
        return None;
    }
    let strlen = be(&dir, 4)? as usize;
    if 12 + strlen > dir.len() {
        return None;
    }
    let count = be(&dir, 8 + strlen)? as usize;
    let entries_at = 12 + strlen;
    let tail_at = entries_at.checked_add(count.checked_mul(24)?)?;
    if tail_at + 5 != dir.len() {
        return None;
    }
    let compressed = dir[tail_at] != 0;
    let body = &raw[8 + cut..];
    let payload_len = if compressed { gunzip(body)?.len() } else { body.len() };
    if be(&dir, tail_at + 1)? as usize != payload_len {
        return None;
    }
    // cstr: a NUL inside the table (directory.find(0, 8 + offset, entries_at - 4)), ASCII
    let cstr = |offset: u32| -> Option<String> {
        let start = 8 + offset as usize;
        let end = entries_at - 4;
        let z = dir.get(start..end)?.iter().position(|&c| c == 0)?;
        let s = &dir[start..start + z];
        if !s.is_ascii() {
            return None;
        }
        Some(String::from_utf8_lossy(s).into_owned())
    };
    let mut entries = Vec::with_capacity(count);
    let mut pos: u64 = 0;
    for k in 0..count {
        let at = entries_at + 24 * k;
        let w = |i: usize| be(&dir, at + 4 * i);
        let (name, flags, offset, size, size2, kind) = (w(0)?, w(1)?, w(2)?, w(3)?, w(4)?, w(5)?);
        let e = Entry {
            name: cstr(name)?,
            kind: cstr(kind)?,
            flags,
            offset,
            size,
        };
        if u64::from(e.offset) != pos || size != size2 {
            return None;
        }
        pos += u64::from(size);
        entries.push(e);
    }
    if pos != payload_len as u64 {
        return None;
    }
    Some(Checked {
        entries,
        compressed,
        payload_at: 8 + cut as u64,
        payload_len: payload_len as u64,
    })
}

/// Why a read failed (`bf1943import.Bf1943ImportError`'s cases).
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The bundle cannot be read any more.
    #[error("the BF1943 bundle cannot be read any more ({0})")]
    Io(#[from] io::Error),
    /// The payload no longer decompresses (read whole).
    #[error("the BF1943 bundle's payload no longer decompresses")]
    Damaged,
    /// The payload is not the one the source was opened on.
    #[error("the BF1943 bundle's payload changed after the source was opened")]
    Changed,
}

/// A compressed payload's place: its file and where its payload lies in it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PayloadRef {
    /// The bundle file.
    pub path: PathBuf,
    /// The file's size when the source was opened.
    pub size: u64,
    /// Its modification time (ns) when the source was opened.
    pub mtime_ns: i128,
    /// Where the payload starts.
    pub payload_at: u64,
    /// The payload's decompressed length.
    pub payload_len: u64,
}

/// How a payload is read: through restart points (the deflate data's offset in the file), or whole each time.
#[derive(Debug)]
enum How {
    Points { base: u64, index: Index },
    Whole,
}

/// What one read did (added to `Source.stats` by the Python).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the counts of Source.stats, each 0 or 1 for one read"
)]
pub struct ReadInfo {
    /// The points were made by this read (the payload decompressed start to end).
    pub indexed: bool,
    /// The points were found in the process (made by an earlier read, of any `Source`).
    pub shared: bool,
    /// The points were read from disk.
    pub from_disk: bool,
    /// The payload was decompressed whole (not one plain member).
    pub whole: bool,
    /// Spans decoded.
    pub spans: u64,
    /// Spans found decoded.
    pub hits: u64,
    /// Bytes decompressed.
    pub decompressed_bytes: u64,
}

/// The budgets the process keeps within, in bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budgets {
    /// Restart points (`BF1943_RESTARTS_MB`).
    pub restarts: u64,
    /// Decoded spans (`BF1943_SOURCE_MEMORY_MB`).
    pub spans: u64,
}

fn mb(name: &str, default: f64) -> u64 {
    let v = std::env::var(name)
        .ok()
        .filter(|v| !v.is_empty())
        .map_or(Some(default), |v| v.trim().parse::<f64>().ok())
        .unwrap_or(default);
    (v.max(0.0) * f64::from(1u32 << 20)) as u64
}

impl Budgets {
    /// From the environment ([`RESTARTS_ENV`], [`MEMORY_ENV`]), as `bf1943import` reads them (a value that is not
    /// a number is the default).
    #[must_use]
    pub fn from_env() -> Self {
        Self {
            restarts: mb(RESTARTS_ENV, RESTARTS_MB),
            spans: mb(MEMORY_ENV, MEMORY_MB),
        }
    }
}

struct Kept {
    how: Arc<How>,
    cost: u64,
    used: u64,
    key: u64,
}

#[derive(Default)]
struct State {
    kept: HashMap<PayloadRef, Kept>,
    held: u64,
    tick: u64,
    next_key: u64,
    hits: u64,
    misses: u64,
    dropped: u64,
}

/// The process's restart points and decoded spans (`bf1943import.RESTARTS`). Thread-safe: the points are made
/// outside the lock (two threads may both make the same payload's once).
pub struct Payloads {
    state: Mutex<State>,
    spans: Spans,
}

static GLOBAL: OnceLock<Payloads> = OnceLock::new();

/// The process's [`Payloads`].
pub fn global() -> &'static Payloads {
    GLOBAL.get_or_init(|| Payloads {
        state: Mutex::new(State::default()),
        spans: Spans::new(0),
    })
}

/// A payload's identity by content, for its points file: the compressed length, the decompressed length, the gzip
/// trailer (CRC-32 and length, the file's last 8 bytes) and the first 4 KiB of the compressed bytes. Points made from
/// one copy serve every copy of the same bytes, wherever it lies; each span's CRC-32 still checks the bytes read.
fn content_key(f: &mut File, r: &PayloadRef) -> io::Result<Vec<u8>> {
    let len = f.metadata()?.len();
    let body = len.saturating_sub(r.payload_at);
    let mut key = Vec::with_capacity(4096 + 40);
    key.extend(b"bf1943 payload v1");
    key.extend(body.to_le_bytes());
    key.extend(r.payload_len.to_le_bytes());
    key.extend(SPAN.to_le_bytes());
    let mut tail = [0u8; 8];
    if body >= 8 {
        f.seek(SeekFrom::Start(len - 8))?;
        f.read_exact(&mut tail)?;
    }
    key.extend(tail);
    let mut head = vec![0u8; body.min(4096) as usize];
    f.seek(SeekFrom::Start(r.payload_at))?;
    f.read_exact(&mut head)?;
    key.extend(head);
    Ok(key)
}

fn points_file(folder: &Path, key: &[u8]) -> PathBuf {
    let mut h = sha1_smol::Sha1::new();
    h.update(key);
    let hex = h.digest().to_string();
    folder.join(format!("{}.points", &hex[..24]))
}

fn load_points(folder: &Path, key: &[u8]) -> Option<(u64, Index)> {
    let data = std::fs::read(points_file(folder, key)).ok()?;
    let rest = data.strip_prefix(POINTS_MAGIC.as_slice())?;
    let klen = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    if rest.get(4..4 + klen)? != key {
        return None;
    }
    let base = u64::from_le_bytes(rest.get(4 + klen..12 + klen)?.try_into().ok()?);
    let index = Index::from_bytes(rest.get(12 + klen..)?).ok()?;
    Some((base, index))
}

fn save_points(folder: &Path, key: &[u8], base: u64, index: &Index) {
    let mut b = Vec::new();
    b.extend(POINTS_MAGIC);
    b.extend((key.len() as u32).to_le_bytes());
    b.extend(key);
    b.extend(base.to_le_bytes());
    b.extend(index.to_bytes());
    // only a cache: a failed write leaves the next process to make the points again
    if std::fs::create_dir_all(folder).is_ok() {
        let _ = crate::atomic::write_rebuildable(&points_file(folder, key), &[&b]);
    }
}

/// The points of a compressed payload made: `How::Points` when it is one plain gzip member (zlib's rules, nothing
/// after its trailer) of the expected length, else `How::Whole`. The bytes decompressed are returned too.
fn make(f: &mut File, r: &PayloadRef) -> Result<(How, u64), ReadError> {
    let len = f.metadata()?.len();
    f.seek(SeekFrom::Start(r.payload_at))?;
    let mut head = Vec::new();
    (&mut *f).take(1 << 16).read_to_end(&mut head)?;
    let Ok(Some(hlen)) = crate::fbrb::gzip_member_header(&head) else {
        return Ok((How::Whole, 0));
    };
    let base = r.payload_at + hlen as u64;
    f.seek(SeekFrom::Start(base))?;
    let built = match restart::build(
        &mut BufReader::with_capacity(1 << 20, &mut *f),
        SPAN,
        &mut |_: &[u8]| {},
    ) {
        Ok(b) => b,
        Err(RestartError::Io(e)) => return Err(ReadError::Io(e)),
        Err(_) => return Ok((How::Whole, 0)),
    };
    let ix = built.index;
    let made = ix.output_len;
    let trailer_at = base + ix.input_len;
    if trailer_at + 8 != len || ix.output_len != r.payload_len {
        return Ok((How::Whole, made));
    }
    let mut tail = [0u8; 8];
    f.seek(SeekFrom::Start(trailer_at))?;
    f.read_exact(&mut tail)?;
    let crc = u32::from_le_bytes([tail[0], tail[1], tail[2], tail[3]]);
    let isize = u32::from_le_bytes([tail[4], tail[5], tail[6], tail[7]]);
    if crc != ix.crc || isize != (ix.output_len & 0xFFFF_FFFF) as u32 {
        return Ok((How::Whole, made));
    }
    Ok((How::Points { base, index: ix }, made))
}

impl Payloads {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// `[offset, offset + size)` of the compressed payload `r`, through the process's points (made, or read from
    /// `folder`'s points files, when not kept; `None`: no files) within `budgets`.
    ///
    /// # Errors
    /// [`ReadError`]: the file cannot be read, its payload no longer decompresses or is not the one opened.
    pub fn read(
        &self,
        r: &PayloadRef,
        offset: u64,
        size: u64,
        folder: Option<&Path>,
        budgets: Budgets,
    ) -> Result<(Vec<u8>, ReadInfo), ReadError> {
        let mut info = ReadInfo::default();
        self.spans.set_budget(budgets.spans);
        let mut f = File::open(&r.path)?;
        let found = {
            let mut s = self.state();
            s.tick += 1;
            let tick = s.tick;
            if let Some(k) = s.kept.get_mut(r) {
                k.used = tick;
                let got = (Arc::clone(&k.how), k.key);
                s.hits += 1;
                Some(got)
            } else {
                s.misses += 1;
                None
            }
        };
        let (how, key) = if let Some(got) = found {
            info.shared = true;
            got
        } else {
            let content = content_key(&mut f, r)?;
            let disk = folder.and_then(|d| load_points(d, &content));
            let how = if let Some((base, index)) = disk {
                info.from_disk = true;
                How::Points { base, index }
            } else {
                let (how, made) = make(&mut f, r)?;
                info.indexed = true;
                info.decompressed_bytes += made;
                if let (Some(d), How::Points { base, index }) = (folder, &how) {
                    save_points(d, &content, *base, index);
                }
                how
            };
            self.keep(r, how, budgets.restarts)
        };
        let data = match &*how {
            How::Whole => {
                info.whole = true;
                f.seek(SeekFrom::Start(r.payload_at))?;
                let mut body = Vec::new();
                f.read_to_end(&mut body)?;
                let payload = gunzip(&body).ok_or(ReadError::Damaged)?;
                info.decompressed_bytes += payload.len() as u64;
                if payload.len() as u64 != r.payload_len {
                    return Err(ReadError::Changed);
                }
                let lo = usize::try_from(offset).map_err(|_| ReadError::Changed)?;
                let hi = usize::try_from(offset + size).map_err(|_| ReadError::Changed)?;
                payload.get(lo..hi).ok_or(ReadError::Changed)?.to_vec()
            }
            How::Points { base, index } => {
                if size == 0 {
                    return Ok((Vec::new(), info));
                }
                let (_, _, hits0, dec0, bytes0) = self.spans.stats();
                let got = restart::read(index, &mut f, *base, offset, size, Some((&self.spans, key)));
                let (_, _, hits1, dec1, bytes1) = self.spans.stats();
                // (other threads' reads may fall between: the counts are for the statistics only)
                info.hits += hits1.saturating_sub(hits0);
                info.spans += dec1.saturating_sub(dec0);
                info.decompressed_bytes += bytes1.saturating_sub(bytes0);
                got.map_err(|e| match e {
                    RestartError::Io(e) => ReadError::Io(e),
                    _ => ReadError::Changed,
                })?
            }
        };
        Ok((data, info))
    }

    /// Keeps `how` for `r` (the least recently used dropped beyond `budget`; nothing kept when it alone is more).
    fn keep(&self, r: &PayloadRef, how: How, budget: u64) -> (Arc<How>, u64) {
        let cost = match &how {
            How::Points { index, .. } => 1024 + index.cost(),
            How::Whole => 1024,
        };
        let how = Arc::new(how);
        let mut s = self.state();
        s.next_key += 1;
        let key = s.next_key;
        if let Some(old) = s.kept.remove(r) {
            s.held -= old.cost;
            self.spans.forget(old.key);
        }
        if cost <= budget {
            s.tick += 1;
            let used = s.tick;
            s.kept.insert(
                r.clone(),
                Kept {
                    how: Arc::clone(&how),
                    cost,
                    used,
                    key,
                },
            );
            s.held += cost;
            while s.held > budget && s.kept.len() > 1 {
                let Some(victim) = s.kept.iter().min_by_key(|(_, k)| k.used).map(|(p, _)| p.clone()) else {
                    break;
                };
                if let Some(old) = s.kept.remove(&victim) {
                    s.held -= old.cost;
                    s.dropped += 1;
                    self.spans.forget(old.key);
                }
            }
        }
        (how, key)
    }

    /// (payloads kept, their bytes as costed).
    #[must_use]
    pub fn held(&self) -> (usize, u64) {
        let s = self.state();
        (s.kept.len(), s.held)
    }

    /// (hits, misses, dropped) of the process's points.
    #[must_use]
    pub fn counts(&self) -> (u64, u64, u64) {
        let s = self.state();
        (s.hits, s.misses, s.dropped)
    }

    /// Forgets every payload's points and every decoded span.
    pub fn clear(&self) {
        let mut s = self.state();
        s.kept.clear();
        s.held = 0;
        drop(s);
        self.spans.clear();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::many_single_char_names)]
mod tests {
    use super::*;
    use std::io::Write as _;

    fn gz(data: &[u8]) -> Vec<u8> {
        let mut e = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        e.write_all(data).unwrap();
        e.finish().unwrap()
    }

    #[test]
    fn gzip_as_python() {
        assert_eq!(gunzip(b"").unwrap(), b"");
        let mut two = gz(b"hello ");
        two.extend([0, 0, 0]);
        two.extend(gz(b"world"));
        assert_eq!(gunzip(&two).unwrap(), b"hello world");
        let one = gz(b"abc");
        assert!(gunzip(&one[..one.len() - 3]).is_none());
        let mut bad = one.clone();
        bad.push(1);
        assert!(gunzip(&bad).is_none());
    }

    /// A version-2 bundle of `entries`, its payload gzip (`split`: as two members) or stored.
    fn bundle(entries: &[(&str, &[u8])], compressed: bool, split: bool) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut payload = Vec::new();
        let mut rows = Vec::new();
        let kind_at = {
            let at = strings.len() as u32;
            strings.extend(b"res\0");
            at
        };
        for (name, data) in entries {
            let at = strings.len() as u32;
            strings.extend(name.as_bytes());
            strings.push(0);
            rows.push((at, payload.len() as u32, data.len() as u32));
            payload.extend_from_slice(data);
        }
        let mut dir = Vec::new();
        dir.extend(2u32.to_be_bytes());
        dir.extend((strings.len() as u32 + 4).to_be_bytes());
        dir.extend(&strings);
        dir.extend([0u8; 4]);
        dir.extend((rows.len() as u32).to_be_bytes());
        for (name, offset, size) in &rows {
            for v in [*name, 0, *offset, *size, *size, kind_at] {
                dir.extend(v.to_be_bytes());
            }
        }
        dir.push(u8::from(compressed));
        dir.extend((payload.len() as u32).to_be_bytes());
        let zdir = gz(&dir);
        let mut raw = b"FbRB".to_vec();
        raw.extend((zdir.len() as u32).to_be_bytes());
        raw.extend(zdir);
        if !compressed {
            raw.extend(payload);
        } else if split {
            let half = payload.len() / 2;
            raw.extend(gz(&payload[..half]));
            raw.extend(gz(&payload[half..]));
        } else {
            raw.extend(gz(&payload));
        }
        raw
    }

    fn noise(n: usize, seed: u32) -> Vec<u8> {
        let mut x = seed.max(1);
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                b'a' + (x % 19) as u8
            })
            .collect()
    }

    #[test]
    fn reads_through_points_and_whole() {
        let dir = std::env::temp_dir().join(format!("bc2-bf1943-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let data: Vec<Vec<u8>> = (0..12).map(|i| noise(300_000 + i * 7_000, i as u32 + 1)).collect();
        let entries: Vec<(String, &[u8])> = data
            .iter()
            .enumerate()
            .map(|(i, d)| (format!("e{i}"), d.as_slice()))
            .collect();
        let named: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (n.as_str(), *d)).collect();
        for (name, split) in [("one.fbrb", false), ("two.fbrb", true)] {
            let raw = bundle(&named, true, split);
            let c = check(&raw).unwrap();
            assert_eq!(c.entries.len(), 12);
            let path = dir.join(name);
            std::fs::write(&path, &raw).unwrap();
            let r = PayloadRef {
                path: path.clone(),
                size: raw.len() as u64,
                mtime_ns: 0,
                payload_at: c.payload_at,
                payload_len: c.payload_len,
            };
            let p = Payloads {
                state: Mutex::new(State::default()),
                spans: Spans::new(0),
            };
            let budgets = Budgets {
                restarts: 64 << 20,
                spans: 4 << 20,
            };
            for (k, e) in c.entries.iter().enumerate().rev() {
                let (got, info) = p
                    .read(&r, u64::from(e.offset), u64::from(e.size), Some(&dir), budgets)
                    .unwrap();
                assert_eq!(got, data[k]);
                assert_eq!(info.whole, split);
            }
            // a second process (a new Payloads) reads the points from disk
            let q = Payloads {
                state: Mutex::new(State::default()),
                spans: Spans::new(0),
            };
            let e = &c.entries[5];
            let (got, info) = q.read(&r, u64::from(e.offset), 10, Some(&dir), budgets).unwrap();
            assert_eq!(got, &data[5][..10]);
            assert_eq!(info.from_disk, !split);
        }
        // a changed byte in the payload: refused
        let raw = bundle(&named, true, false);
        let c = check(&raw).unwrap();
        let mut bad = raw.clone();
        let n = bad.len();
        bad[n - 5000] ^= 0x10;
        let path = dir.join("one.fbrb");
        std::fs::write(&path, &bad).unwrap();
        let r = PayloadRef {
            path,
            size: bad.len() as u64,
            mtime_ns: 1,
            payload_at: c.payload_at,
            payload_len: c.payload_len,
        };
        let p = Payloads {
            state: Mutex::new(State::default()),
            spans: Spans::new(0),
        };
        let e = c.entries.last().unwrap();
        // the points on disk are found by content (the trailer and the first bytes are the same) and the span's
        // CRC-32 refuses the changed bytes
        let got = p.read(
            &r,
            u64::from(e.offset),
            u64::from(e.size),
            Some(&dir),
            Budgets::from_env(),
        );
        assert!(got.is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
