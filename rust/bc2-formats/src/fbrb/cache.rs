//! Repeated reads of the same bundles made cheap (round E3e): [`super::read_path`], [`super::read_many`],
//! [`super::extract_path`] and [`super::extract_many`] go through here.
//!
//! A zipped payload is one gzip stream, so the bytes of an entry cost decompressing the payload from its start up to
//! the entry; a tool that reads one entry at a time from the same large bundle decompressed it again for every read
//! (OPFOR's WW2 level build: 726 reads, 132 s, mostly of the same 23 stock bundles, most reads ending past half of
//! the payload). Three layers keep the answers the same and the work once:
//!
//! - **the header**: kept per bundle (the last [`HEADERS`]), so a read does not decompress and parse it again;
//! - **in this process**: the payload decompressed so far, with the inflater's state ([`Session`]), so a later
//!   read inside it is a copy and one past it continues where the last stopped; up to [`CacheConfig::memory`]
//!   bytes of them, the least recently used dropped first;
//! - **on disk, shared by every process**: the restart points of a large bundle's payload (from
//!   [`CacheConfig::disk_min`] compressed bytes) that has been read before (by any process: a marker file is left at
//!   the first read), a `.points` file ([`crate::restart`], round PR1b-1-rest): a read seeks into the bundle's own
//!   compressed bytes and inflates at most a span (1 MiB) per entry, the decoded spans kept within
//!   [`CacheConfig::memory`]. A points file is about 3 % of the payload it serves (the whole decompressed payloads it
//!   replaced, `.payload` files, filled the 16 GiB cap). The files live in [`CacheConfig::folder`],
//!   `%LOCALAPPDATA%\bfbc2-tool-suite\cache\fbrb` by default, up to [`CacheConfig::disk_cap`] bytes, the least
//!   recently used deleted first.
//!
//! The memory layers are keyed by the bundle's [`Identity`]: its canonical path, length, modification and creation
//! times, taken from the open file the read uses, so a bundle replaced or rewritten is a different bundle. A points
//! file is keyed by the payload's **content** (`ContentKey`: the bundle's length and header bytes, the payload's
//! first bytes and the gzip trailer's CRC-32 and length), so every copy of the same bundle (a level's `Dist` and
//! `Package` copies) shares one; each span read is checked against the CRC-32 recorded when the points were made, so
//! bytes that changed are refused (the read falls back to decompressing), never answered as other bytes. Cache files
//! are written beside their final name and moved over it ([`crate::atomic`]), so a reader in another process sees a
//! whole file or none.
//!
//! **Superseded points** (round QW1, audit AU2 S1): a bundle rewritten at the same path (an install, a scratch copy)
//! with other bytes has other points, and the old ones serve no read made through that path again; nor do a deleted
//! bundle's (a temporary folder's). Writing a bundle's points deletes the other points files made from the same
//! canonical path ([`drop_superseded`]); [`superseded`] lists every points file of a folder whose bundle was
//! rewritten or deleted, and every `.payload` file of the retired format, for the caches tool's `--prune`
//! (`bfbc2core.caches`, `bfbc2 caches`).
//!
//! **The same results.** The reference reads the payload 1 MiB of compressed bytes at a time, from the start, until
//! the decompressed bytes reach the end of the last entry asked for; an error in any chunk it reads is raised, and
//! nothing after that chunk is looked at. A [`Session`] feeds the same chunks to the same inflater, so its state
//! after `k` chunks is the reference's; it keeps only what whole chunks gave, and remembers a gzip error with the
//! output before the failing chunk, so a read that would have reached that chunk gets the same error and one that
//! would not gets its bytes. A payload goes to disk only once its whole stream has been read with its checksum and
//! length checked: every chunk is then valid, so any read of it is a copy (or [`FbrbError::PayloadEnds`] past its
//! end), as the reference's would be. A bundle whose stream fails is never written; its reads keep the session's
//! rules. Stored (unzipped) payloads need none of this: [`super::read_payload`] seeks to each entry. The points are
//! made by a pass over the whole stream with zlib-rs's `Z_BLOCK` stops (the same inflater, the same header and
//! trailer checks), so a stream either the reference or that pass refuses is never written.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::error::{FbrbError, Result};
use super::gzip::Member;
use super::header::{Entry, Header, read_header};
use super::read::{CHUNK, Collector, read_full, read_payload};
use crate::restart::{self, Index, Spans};

/// How many bundle headers a process keeps.
pub const HEADERS: usize = 64;

/// The environment variable naming the disk cache's folder; `0` (or `off`) turns the disk cache off.
pub const FOLDER_ENV: &str = "BFBC2_FBRB_CACHE";
/// The environment variable giving the disk cache's size limit in GiB (default [`DEFAULT_DISK_CAP_GB`]).
pub const DISK_CAP_ENV: &str = "BFBC2_FBRB_CACHE_GB";
/// The environment variable giving the least compressed payload, in MiB, that goes to disk (default
/// [`DEFAULT_DISK_MIN_MB`]).
pub const DISK_MIN_ENV: &str = "BFBC2_FBRB_CACHE_MIN_MB";
/// The environment variable giving the in-process budget in MiB (default [`DEFAULT_MEMORY_MB`]; `0` keeps nothing).
pub const MEMORY_ENV: &str = "BFBC2_FBRB_MEMORY_MB";

/// The disk cache's default size limit, GiB.
pub const DEFAULT_DISK_CAP_GB: u64 = 16;
/// The default least compressed payload, MiB, that goes to disk.
pub const DEFAULT_DISK_MIN_MB: u64 = 8;
/// The default in-process budget, MiB.
pub const DEFAULT_MEMORY_MB: u64 = 128;

/// A points file's first bytes (the format's version is in them).
const MAGIC: &[u8; 8] = b"BC2FBPT1";
/// The points files' extension.
const POINTS: &str = "points";
/// The retired whole-payload files' extension (format `BC2FBPL1`, before round PR1b-1-rest): never read, listed by
/// [`superseded`], counted and trimmed with the points files.
const LEGACY: &str = "payload";
/// Points files a process keeps parsed.
const INDEXES: usize = 256;
/// Temporary files older than this are a writer that died: deleted when the cache is trimmed.
const STALE_TMP: Duration = Duration::from_secs(3600);
/// Markers older than this are forgotten when the cache is trimmed.
const STALE_MARKER: Duration = Duration::from_hours(30 * 24);

/// Where and how much the cache keeps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheConfig {
    /// Bytes of decompressed payload a process keeps in memory (0: none).
    pub memory: u64,
    /// The disk cache's folder (`None`: no disk cache).
    pub folder: Option<PathBuf>,
    /// The disk cache's size limit in bytes: after a payload is written, the least recently used are deleted
    /// beyond it.
    pub disk_cap: u64,
    /// The least compressed payload (bytes) that goes to disk: smaller bundles decompress in milliseconds.
    pub disk_min: u64,
}

impl CacheConfig {
    /// No cache at all: every read decompresses as the reference does (stored payloads still seek).
    #[must_use]
    pub fn off() -> Self {
        Self {
            memory: 0,
            folder: None,
            disk_cap: 0,
            disk_min: u64::MAX,
        }
    }

    /// The configuration the environment gives ([`FOLDER_ENV`], [`DISK_CAP_ENV`], [`DISK_MIN_ENV`],
    /// [`MEMORY_ENV`]), else the defaults: the disk cache in `%LOCALAPPDATA%\bfbc2-tool-suite\cache\fbrb`.
    #[must_use]
    pub fn from_env() -> Self {
        // a number of MiB / GiB (fractions allowed) as bytes; `default` when unset or not a number
        #[expect(clippy::cast_precision_loss, reason = "the units are powers of two, exact in f64")]
        let bytes = |name: &str, default: u64, unit: u64| {
            std::env::var(name)
                .ok()
                .and_then(|v| v.trim().parse::<f64>().ok())
                .filter(|v| v.is_finite() && *v >= 0.0)
                .map_or(default * unit, |v| (v * unit as f64) as u64)
        };
        let folder = match std::env::var_os(FOLDER_ENV) {
            Some(v) if v == "0" || v.eq_ignore_ascii_case("off") => None,
            Some(v) if !v.is_empty() => Some(PathBuf::from(v)),
            _ => std::env::var_os("LOCALAPPDATA")
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
                .map(|base| base.join("bfbc2-tool-suite").join("cache").join("fbrb")),
        };
        Self {
            memory: bytes(MEMORY_ENV, DEFAULT_MEMORY_MB, 1 << 20),
            folder,
            disk_cap: bytes(DISK_CAP_ENV, DEFAULT_DISK_CAP_GB, 1 << 30),
            disk_min: bytes(DISK_MIN_ENV, DEFAULT_DISK_MIN_MB, 1 << 20),
        }
    }
}

/// What the cache did in this process (since the last [`reset_stats`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    /// Reads of zipped payloads through the cache.
    pub reads: u64,
    /// Reads answered through a points file on disk.
    pub disk_hits: u64,
    /// Reads answered from a session's bytes without decompressing anything.
    pub memory_hits: u64,
    /// Reads that decompressed (from the start, or continuing a session).
    pub decompressed_reads: u64,
    /// Payload bytes decompressed.
    pub decompressed_bytes: u64,
    /// Points files written to disk.
    pub written: u64,
    /// The bytes of the points files written.
    pub written_bytes: u64,
    /// Headers answered from memory.
    pub header_hits: u64,
}

/// Which bundle a file is: its canonical path, length, and modification and creation times (nanoseconds since
/// 1970; 0 where the system does not keep one). Taken from the open file a read uses.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Identity {
    /// The canonical path (the path as given when it cannot be resolved).
    pub path: PathBuf,
    /// The file's length.
    pub len: u64,
    /// The last modification time.
    pub modified: u128,
    /// The creation time.
    pub created: u128,
}

fn nanos(t: io::Result<SystemTime>) -> u128 {
    t.ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_nanos())
}

impl Identity {
    /// The identity of `file`, opened from `path`.
    ///
    /// # Errors
    /// When the file's metadata cannot be read.
    pub fn of(path: &Path, file: &File) -> io::Result<Self> {
        let m = file.metadata()?;
        Ok(Self {
            path: fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()),
            len: m.len(),
            modified: nanos(m.modified()),
            created: nanos(m.created()),
        })
    }

    /// The bytes a cache file repeats in its header.
    fn bytes(&self) -> Vec<u8> {
        let p = self.path.to_string_lossy();
        let mut b = Vec::with_capacity(p.len() + 44);
        b.extend(self.len.to_le_bytes());
        b.extend(self.modified.to_le_bytes());
        b.extend(self.created.to_le_bytes());
        b.extend((p.len() as u32).to_le_bytes());
        b.extend(p.as_bytes());
        b
    }

    /// The cache's file stem: the bundle's folder and name, readable, then a hash of the identity.
    fn stem(&self) -> String {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in self.bytes() {
            h = (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        let parts: Vec<String> = self
            .path
            .iter()
            .rev()
            .take(2)
            .map(|c| {
                c.to_string_lossy()
                    .chars()
                    .map(|ch| {
                        if ch.is_ascii_alphanumeric() || ch == '-' {
                            ch
                        } else {
                            '_'
                        }
                    })
                    .collect()
            })
            .collect();
        let name: Vec<&str> = parts.iter().rev().map(String::as_str).collect();
        format!("{}-{h:016x}", name.join("_").to_ascii_lowercase())
    }
}

/// A payload's identity by content, for its points file: the bundle's length, where its payload starts, the
/// SHA-1 of its first 64 KiB (the header and the payload's start) and its last 8 bytes (the gzip trailer's CRC-32 and
/// length when the member ends the file). Every copy of the same bundle has the same key; the CRC-32 of each span,
/// checked on every read, catches bytes that differ anyway.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ContentKey {
    bytes: Vec<u8>,
    /// FNV-1a of the bytes: the points file's name and the decoded spans' key.
    hash: u64,
}

impl ContentKey {
    fn of(f: &mut File, len: u64, payload_start: u64) -> io::Result<Self> {
        let mut head = vec![0u8; len.min(1 << 16) as usize];
        f.seek(SeekFrom::Start(0))?;
        f.read_exact(&mut head)?;
        let mut tail = [0u8; 8];
        if len >= 8 {
            f.seek(SeekFrom::Start(len - 8))?;
            f.read_exact(&mut tail)?;
        }
        let mut sha = sha1_smol::Sha1::new();
        sha.update(&head);
        let mut bytes = b"fbrb points v1".to_vec();
        bytes.extend(len.to_le_bytes());
        bytes.extend(payload_start.to_le_bytes());
        bytes.extend(restart::SPAN.to_le_bytes());
        bytes.extend(sha.digest().bytes());
        bytes.extend(tail);
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for b in &bytes {
            h = (h ^ u64::from(*b)).wrapping_mul(0x0100_0000_01b3);
        }
        Ok(Self { bytes, hash: h })
    }
}

/// A points file read: where the deflate data starts after the payload's start (the gzip header's length), and the
/// points.
struct Points {
    header_len: u64,
    index: Index,
}

/// How a session's stream stands.
#[derive(Debug)]
enum Flow {
    /// More chunks may be read.
    Open,
    /// The member ended (its trailer checked), or the file did: no more bytes will come.
    Ended,
    /// A chunk failed: the error the reference raises for any read that reaches it.
    Failed(Failure),
}

/// A gzip error, kept to be raised again.
#[derive(Debug, Clone)]
enum Failure {
    Gzip(String),
    GzipEnd,
}

impl Failure {
    fn of(e: &FbrbError) -> Option<Self> {
        match e {
            FbrbError::Gzip(m) => Some(Self::Gzip(m.clone())),
            FbrbError::GzipEnd => Some(Self::GzipEnd),
            _ => None,
        }
    }

    fn error(&self) -> FbrbError {
        match self {
            Self::Gzip(m) => FbrbError::Gzip(m.clone()),
            Self::GzipEnd => FbrbError::GzipEnd,
        }
    }
}

/// A zipped payload decompressed as far as the reads so far needed: its bytes after whole chunks and the inflater
/// that made them.
struct Session {
    data: Vec<u8>,
    member: Member,
    /// The file offset of the next compressed chunk.
    next_in: u64,
    flow: Flow,
}

impl Session {
    fn new(payload_start: u64) -> Self {
        Self {
            data: Vec::new(),
            member: Member::new(),
            next_in: payload_start,
            flow: Flow::Open,
        }
    }

    /// Decompresses whole chunks until the bytes reach `end` or the stream stops. `f` is the bundle.
    fn fill(&mut self, f: &mut File, end: u64, stats: &mut u64) -> Result<()> {
        if (self.data.len() as u64) >= end || !matches!(self.flow, Flow::Open) {
            return Ok(());
        }
        f.seek(SeekFrom::Start(self.next_in))?;
        let mut buf = vec![0; CHUNK];
        while (self.data.len() as u64) < end {
            let n = read_full(f, &mut buf)?;
            if n == 0 {
                self.flow = Flow::Ended;
                break;
            }
            let before = self.data.len();
            let data = &mut self.data;
            let fed = self.member.feed(buf.get(..n).unwrap_or_default(), &mut |d: &[u8]| {
                data.extend_from_slice(d);
            });
            if let Err(e) = fed {
                self.data.truncate(before);
                if let Some(fail) = Failure::of(&e) {
                    self.flow = Flow::Failed(fail);
                }
                return Err(e);
            }
            *stats += (self.data.len() - before) as u64;
            self.next_in += n as u64;
            if self.member.done() {
                self.flow = Flow::Ended;
                break;
            }
        }
        Ok(())
    }

    /// The entries' bytes as the reference returns them after reading up to `end`.
    fn answer(&self, entries: &[&Entry], end: u64) -> Result<Vec<Vec<u8>>> {
        if let Flow::Failed(fail) = &self.flow
            && (self.data.len() as u64) < end
        {
            return Err(fail.error());
        }
        slices(&self.data, entries)
    }
}

/// The entries' bytes out of a payload's first bytes `data`: [`FbrbError::PayloadEnds`] when one does not fit (an
/// empty entry always does, as in the reference).
fn slices(data: &[u8], entries: &[&Entry]) -> Result<Vec<Vec<u8>>> {
    entries
        .iter()
        .map(|e| {
            if e.size == 0 {
                return Ok(Vec::new());
            }
            usize::try_from(e.offset)
                .ok()
                .zip(usize::try_from(e.end()).ok())
                .and_then(|(a, b)| data.get(a..b))
                .map(<[u8]>::to_vec)
                .ok_or(FbrbError::PayloadEnds)
        })
        .collect()
}

/// A header, and its entries by lower-case name (index into `header.entries`).
pub(crate) struct Indexed {
    pub(crate) header: Header,
    pub(crate) names: HashMap<String, usize>,
}

impl Indexed {
    fn new(header: Header) -> Self {
        let names = header
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_deleted())
            .map(|(i, e)| (e.name.to_ascii_lowercase(), i))
            .collect();
        Self { header, names }
    }

    /// The entries named (lower case expected), or [`FbrbError::MissingEntry`] for the first that is not one.
    pub(crate) fn lookup<S: AsRef<str>>(&self, names: &[S]) -> Result<Vec<&Entry>> {
        names
            .iter()
            .enumerate()
            .map(|(index, n)| {
                self.names
                    .get(&n.as_ref().to_ascii_lowercase())
                    .and_then(|&i| self.header.entries.get(i))
                    .ok_or(FbrbError::MissingEntry { index })
            })
            .collect()
    }
}

/// A session in a cache's table, with the bytes it was charged at its last use.
struct Slot {
    session: Arc<Mutex<Session>>,
    charged: u64,
    used: u64,
}

struct State {
    config: CacheConfig,
    sessions: HashMap<Identity, Slot>,
    charged: u64,
    headers: VecDeque<(Identity, Arc<Indexed>)>,
    /// Bundles whose stream failed on the way to disk: read through their session only.
    no_disk: HashSet<Identity>,
    /// Cache files whose time this cache has refreshed (once each).
    touched: HashSet<PathBuf>,
    /// The content keys of the bundles read (by identity).
    keys: HashMap<Identity, ContentKey>,
    /// Points files parsed, the most recently used last (at most [`INDEXES`]).
    indexes: VecDeque<(u64, Arc<Points>)>,
    /// The content keys whose points are known to be in the disk cache's folder (written or checked).
    on_disk: HashSet<u64>,
    stats: CacheStats,
}

static GLOBAL: OnceLock<Cache> = OnceLock::new();
static TICK: AtomicU64 = AtomicU64::new(0);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The process's cache, which the path functions use (configured from the environment at its first use).
pub fn global() -> &'static Cache {
    GLOBAL.get_or_init(|| Cache::new(CacheConfig::from_env()))
}

/// The configuration of the process's cache.
#[must_use]
pub fn config() -> CacheConfig {
    global().config()
}

/// [`Cache::configure`] of the process's cache.
pub fn configure(config: CacheConfig) {
    global().configure(config);
}

/// [`Cache::clear_memory`] of the process's cache.
pub fn clear_memory() {
    global().clear_memory();
}

/// [`Cache::stats`] of the process's cache.
#[must_use]
pub fn stats() -> CacheStats {
    global().stats()
}

/// [`Cache::reset_stats`] of the process's cache.
pub fn reset_stats() {
    global().reset_stats();
}

/// The bytes of the cache files (points, and any retired payloads) in the disk cache's `folder`, and their count.
#[must_use]
pub fn disk_usage(folder: &Path) -> (u64, usize) {
    let files = cache_files(folder);
    (files.iter().map(|f| f.1).sum(), files.len())
}

/// A points file's header: (the content key's bytes, the identity of the bundle it was made from, where its points
/// start in the file). `None`: not a points file of this format, or damaged.
fn points_head(data: &[u8]) -> Option<(Vec<u8>, Identity, usize)> {
    let rest = data.strip_prefix(MAGIC.as_slice())?;
    let key_len = u32::from_le_bytes(rest.get(..4)?.try_into().ok()?) as usize;
    let key = rest.get(4..4 + key_len)?.to_vec();
    let at = 4 + key_len;
    let id_len = u32::from_le_bytes(rest.get(at..at + 4)?.try_into().ok()?) as usize;
    let id = rest.get(at + 4..at + 4 + id_len)?;
    if id_len < 44 {
        return None;
    }
    let len = u64::from_le_bytes(id.get(..8)?.try_into().ok()?);
    let modified = u128::from_le_bytes(id.get(8..24)?.try_into().ok()?);
    let created = u128::from_le_bytes(id.get(24..40)?.try_into().ok()?);
    let path_len = u32::from_le_bytes(id.get(40..44)?.try_into().ok()?) as usize;
    let path = String::from_utf8(id.get(44..44 + path_len)?.to_vec()).ok()?;
    if 44 + path_len != id_len {
        return None;
    }
    let identity = Identity {
        path: PathBuf::from(path),
        len,
        modified,
        created,
    };
    Some((key, identity, 8 + at + 4 + id_len))
}

/// The key and source identity a points file's header names (only the header is read).
fn points_identity(file: &Path) -> Option<(Vec<u8>, Identity)> {
    let mut f = File::open(file).ok()?;
    let mut head = vec![0u8; 1 << 16];
    let n = read_full(&mut f, &mut head).ok()?;
    head.truncate(n);
    let (key, id, _) = points_head(&head)?;
    Some((key, id))
}

/// The cache files of `folder` no read can use again (round QW1): every retired `.payload` file, and the points
/// files whose bundle was rewritten with other bytes (the file at their header's path has another content key now)
/// or deleted (nothing is at that path). A bundle that is there but cannot be opened (held by another program) keeps
/// its points, and a file that is not a readable points file of this format is never listed.
#[must_use]
pub fn superseded(folder: &Path) -> Vec<PathBuf> {
    let mut by_path: HashMap<PathBuf, Vec<(PathBuf, Vec<u8>)>> = HashMap::new();
    let mut out = Vec::new();
    for (file, _, _) in cache_files(folder) {
        if file.extension().is_some_and(|x| x == LEGACY) {
            out.push(file);
        } else if let Some((key, id)) = points_identity(&file) {
            by_path.entry(id.path.clone()).or_default().push((file, key));
        }
    }
    #[allow(
        clippy::iter_over_hash_type,
        reason = "the stale files to delete, sorted below; the order of deleting cannot show"
    )]
    for (bundle, files) in by_path {
        let now = File::open(&bundle).map_err(FbrbError::Io).and_then(|mut f| {
            let id = Identity::of(&bundle, &f)?;
            let header = read_header(&mut f)?;
            Ok(ContentKey::of(&mut f, id.len, header.payload_start)?)
        });
        match now {
            Ok(key) => out.extend(files.into_iter().filter(|f| f.1 != key.bytes).map(|f| f.0)),
            Err(FbrbError::Io(e)) if e.kind() == io::ErrorKind::NotFound => {
                out.extend(files.into_iter().map(|f| f.0));
            }
            // a file there that is no bundle any more: its points serve nothing
            Err(FbrbError::Io(_)) => {}
            Err(_) => out.extend(files.into_iter().map(|f| f.0)),
        }
    }
    out.sort();
    out
}

/// Deletes the points files made from the bundle `id` names other than `own` (the bundle was rewritten: no read
/// through that path uses them again). Only files whose header names the same canonical path are touched; one
/// another process holds open stays (Windows) and goes with the least-recently-used cap or the caches tool's
/// `--prune`.
fn drop_superseded(folder: &Path, id: &Identity, own: &Path) {
    for (file, _, _) in cache_files(folder) {
        if file != own
            && file.extension().is_some_and(|x| x == POINTS)
            && points_identity(&file).is_some_and(|(_, other)| other.path == id.path)
        {
            let _ = fs::remove_file(&file);
        }
    }
}

/// `[(path, length, modified)]` of the cache files in `folder`: points files and retired payloads.
fn cache_files(folder: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let Ok(rd) = fs::read_dir(folder) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = e.metadata().ok()?;
            (p.extension().is_some_and(|x| x == POINTS || x == LEGACY) && m.is_file())
                .then(|| (p, m.len(), m.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect()
}

/// A bundle cache: headers and payloads kept in memory, with a disk cache's folder (see the module's text). The
/// process has one ([`global`]); tests make their own.
pub struct Cache {
    state: Mutex<State>,
    /// Spans decoded through points files, within the memory budget.
    spans: Spans,
}

impl Cache {
    /// A cache with `config`, empty.
    #[must_use]
    pub fn new(config: CacheConfig) -> Self {
        Self {
            state: Mutex::new(State {
                config,
                sessions: HashMap::new(),
                charged: 0,
                headers: VecDeque::new(),
                no_disk: HashSet::new(),
                touched: HashSet::new(),
                keys: HashMap::new(),
                indexes: VecDeque::new(),
                on_disk: HashSet::new(),
                stats: CacheStats::default(),
            }),
            spans: Spans::new(0),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// The configuration in use.
    #[must_use]
    pub fn config(&self) -> CacheConfig {
        self.state().config.clone()
    }

    /// Uses `config` from now on, and forgets what was kept in memory; the disk cache's files stay.
    pub fn configure(&self, config: CacheConfig) {
        let mut s = self.state();
        s.config = config;
        s.sessions.clear();
        s.charged = 0;
        s.headers.clear();
        s.no_disk.clear();
        s.touched.clear();
        s.keys.clear();
        s.indexes.clear();
        s.on_disk.clear();
        drop(s);
        self.spans.clear();
    }

    /// Forgets what was kept in memory (the configuration and the disk cache stay).
    pub fn clear_memory(&self) {
        let mut s = self.state();
        s.sessions.clear();
        s.charged = 0;
        s.headers.clear();
        s.keys.clear();
        s.indexes.clear();
        s.on_disk.clear();
        drop(s);
        self.spans.clear();
    }

    /// What the cache did since it was made or [`Cache::reset_stats`].
    #[must_use]
    pub fn stats(&self) -> CacheStats {
        self.state().stats
    }

    /// Zeroes [`Cache::stats`].
    pub fn reset_stats(&self) {
        self.state().stats = CacheStats::default();
    }

    /// The bytes of decompressed payload held in memory now.
    #[must_use]
    pub fn memory_used(&self) -> u64 {
        self.state().charged
    }

    /// [`super::read_path`] through this cache.
    ///
    /// # Errors
    /// As [`super::read`].
    pub fn read_path<S: AsRef<str>>(&self, path: &Path, names: &[S]) -> Result<Vec<Vec<u8>>> {
        let mut f = File::open(path)?;
        let Ok(id) = Identity::of(path, &f) else {
            return super::read(&mut f, names);
        };
        let ix = self.header_of(&id, &mut f)?;
        let wanted = ix.lookup(names)?;
        self.payload(&id, &mut f, &ix.header, &wanted)
    }

    /// [`super::extract_path`] through this cache.
    ///
    /// # Errors
    /// As [`super::extract`].
    pub fn extract_path(&self, path: &Path) -> Result<(Header, Vec<Vec<u8>>)> {
        let mut f = File::open(path)?;
        let Ok(id) = Identity::of(path, &f) else {
            return super::extract(&mut f);
        };
        let ix = self.header_of(&id, &mut f)?;
        let live: Vec<&Entry> = ix.header.entries.iter().filter(|e| !e.is_deleted()).collect();
        let mut data = self.payload(&id, &mut f, &ix.header, &live)?.into_iter();
        let all = ix
            .header
            .entries
            .iter()
            .map(|e| {
                if e.is_deleted() {
                    Vec::new()
                } else {
                    data.next().unwrap_or_default()
                }
            })
            .collect();
        Ok((ix.header.clone(), all))
    }

    fn header_of(&self, id: &Identity, f: &mut File) -> Result<Arc<Indexed>> {
        {
            let mut s = self.state();
            if let Some((_, h)) = s.headers.iter().find(|(k, _)| k == id) {
                let h = Arc::clone(h);
                s.stats.header_hits += 1;
                return Ok(h);
            }
        }
        let ix = Arc::new(Indexed::new(read_header(f)?));
        let mut s = self.state();
        if !s.headers.iter().any(|(k, _)| k == id) {
            s.headers.push_back((id.clone(), Arc::clone(&ix)));
            while s.headers.len() > HEADERS {
                s.headers.pop_front();
            }
        }
        Ok(ix)
    }

    /// The entries' data from the bundle `f` (identity `id`, `header` read from it).
    fn payload(&self, id: &Identity, f: &mut File, header: &Header, entries: &[&Entry]) -> Result<Vec<Vec<u8>>> {
        let end = entries.iter().map(|e| e.end()).max().unwrap_or(0);
        if !header.zipped || end == 0 {
            return read_payload(f, header, entries);
        }
        let config = self.config();
        self.state().stats.reads += 1;
        let big = id.len.saturating_sub(header.payload_start) >= config.disk_min;
        self.spans.set_budget(config.memory);
        if let Some(folder) = config.folder.as_deref().filter(|_| big) {
            if let Some((got, from_disk)) = self.points_read(Some(folder), id, f, header, entries) {
                if from_disk {
                    self.state().stats.disk_hits += 1;
                } else {
                    self.state().stats.memory_hits += 1;
                    // points made in memory at the bundle's first read: written once it is seen again
                    self.persist(folder, &config, id, f, header);
                }
                return got;
            }
            // (another process writing it now: this read goes on as an unseen one, and later ones find the file)
            if seen_before(folder, id) && !self.state().no_disk.contains(id) {
                return self.make_points(Some(folder), &config, id, f, header, entries, end);
            }
        }
        if end > config.memory {
            // more than the cache keeps in sessions: the payload's restart points in the process (round
            // PR1b-1-rest, audit AU5 ALG-5: an export's walks read the same large bundles step after step, each
            // read decompressing from the start), made by one pass over the stream; none when nothing is kept
            if config.memory > 0 && !self.state().no_disk.contains(id) {
                if let Some((got, _)) = self.points_read(None, id, f, header, entries) {
                    self.state().stats.memory_hits += 1;
                    return got;
                }
                return self.make_points(None, &config, id, f, header, entries, end);
            }
            // read as the reference does
            let mut st = self.state();
            st.stats.decompressed_reads += 1;
            st.stats.decompressed_bytes += end;
            drop(st);
            return read_payload(f, header, entries);
        }
        // a bundle whose points were made (its content key known) is read through them, not a new session
        if self.state().keys.contains_key(id)
            && let Some((got, _)) = self.points_read(None, id, f, header, entries)
        {
            self.state().stats.memory_hits += 1;
            return got;
        }
        let session = self.session_of(id, header);
        let mut s = lock(&session);
        let mut made = 0;
        let filled = s.fill(f, end, &mut made);
        let size = s.data.capacity() as u64;
        let got = filled.and_then(|()| s.answer(entries, end));
        drop(s);
        let mut st = self.state();
        if made > 0 {
            st.stats.decompressed_reads += 1;
            st.stats.decompressed_bytes += made;
        } else {
            st.stats.memory_hits += 1;
        }
        charge(&mut st, id, &session, size);
        got
    }

    /// The session of a bundle, made when there is none.
    fn session_of(&self, id: &Identity, header: &Header) -> Arc<Mutex<Session>> {
        let mut st = self.state();
        // a bundle at the same path with another identity was replaced: its bytes are of no more use
        let old: Vec<Identity> = st
            .sessions
            .keys()
            .filter(|k| k.path == id.path && *k != id)
            .cloned()
            .collect();
        for k in old {
            if let Some(slot) = st.sessions.remove(&k) {
                st.charged -= slot.charged;
            }
        }
        let used = TICK.fetch_add(1, Ordering::Relaxed);
        let slot = st.sessions.entry(id.clone()).or_insert_with(|| Slot {
            session: Arc::new(Mutex::new(Session::new(header.payload_start))),
            charged: 0,
            used,
        });
        slot.used = used;
        Arc::clone(&slot.session)
    }

    fn drop_session(&self, id: &Identity) {
        let mut st = self.state();
        if let Some(slot) = st.sessions.remove(id) {
            st.charged -= slot.charged;
        }
    }

    /// The bundle's content key (made once per identity).
    fn key_of(&self, id: &Identity, f: &mut File, header: &Header) -> Option<ContentKey> {
        if let Some(k) = self.state().keys.get(id) {
            return Some(k.clone());
        }
        let k = ContentKey::of(f, id.len, header.payload_start).ok()?;
        self.state().keys.insert(id.clone(), k.clone());
        Some(k)
    }

    /// The points of `key` kept in the process, else read from `folder`'s points file (`None`: no folder): the
    /// points and whether they came from the file. `None` when neither has them.
    fn points_of(&self, folder: Option<&Path>, key: &ContentKey) -> Option<(Arc<Points>, bool)> {
        {
            let mut s = self.state();
            if let Some(at) = s.indexes.iter().position(|(h, _)| *h == key.hash) {
                let got = s.indexes.remove(at)?;
                let points = Arc::clone(&got.1);
                s.indexes.push_back(got);
                return Some((points, false));
            }
        }
        let path = points_path(folder?, key);
        let data = fs::read(&path).ok()?;
        let (stored, _, at) = points_head(&data)?;
        if stored != key.bytes {
            return None;
        }
        let header_len = u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?);
        let index = Index::from_bytes(data.get(at + 8..)?).ok()?;
        let points = self.keep_points(key, Points { header_len, index });
        if self.state().touched.insert(path.clone()) {
            // the least recently used go first when the cache is trimmed: this one was just used
            if let Ok(w) = OpenOptions::new().write(true).open(&path) {
                let _ = w.set_modified(SystemTime::now());
            }
        }
        Some((points, true))
    }

    /// Keeps `points` in the process: at most [`INDEXES`], and their windows within the memory budget (the least
    /// recently used dropped first; the newest always kept).
    fn keep_points(&self, key: &ContentKey, points: Points) -> Arc<Points> {
        let points = Arc::new(points);
        let mut s = self.state();
        s.indexes.retain(|(h, _)| *h != key.hash);
        s.indexes.push_back((key.hash, Arc::clone(&points)));
        let budget = s.config.memory;
        while s.indexes.len() > 1
            && (s.indexes.len() > INDEXES || s.indexes.iter().map(|(_, p)| p.index.cost()).sum::<u64>() > budget)
        {
            if let Some((h, _)) = s.indexes.pop_front() {
                self.spans.forget(h);
            }
        }
        points
    }

    /// Writes the points this process made of the bundle `id` to `folder` when the folder has none and the bundle
    /// was read before (the disk cache's rule); once per bundle and process.
    fn persist(&self, folder: &Path, config: &CacheConfig, id: &Identity, f: &mut File, header: &Header) {
        let Some(key) = self.key_of(id, f, header) else { return };
        let points = {
            let mut s = self.state();
            if !s.on_disk.insert(key.hash) {
                return;
            }
            s.indexes
                .iter()
                .find(|(h, _)| *h == key.hash)
                .map(|(_, p)| Arc::clone(p))
        };
        let Some(points) = points else { return };
        let target = points_path(folder, &key);
        if target.is_file() || !seen_before(folder, id) {
            return;
        }
        if let Some(n) = write_points(folder, &key, id, &points) {
            let mut st = self.state();
            st.stats.written += 1;
            st.stats.written_bytes += n;
            drop(st);
            drop_superseded(folder, id, &target);
            trim(folder, config.disk_cap, &target);
        }
    }

    /// Forgets a points file that failed a read (another process may have replaced it; the next read parses it
    /// again).
    fn forget_points(&self, key: &ContentKey) {
        self.state().indexes.retain(|(h, _)| *h != key.hash);
        self.spans.forget(key.hash);
    }

    /// The entries through the bundle's restart points (kept in the process, else `folder`'s points file), and
    /// whether the points came from the file; `None` when there are none, or a span read through them fails (the
    /// bytes are not the ones the points were made from).
    fn points_read(
        &self,
        folder: Option<&Path>,
        id: &Identity,
        f: &mut File,
        header: &Header,
        entries: &[&Entry],
    ) -> Option<(Result<Vec<Vec<u8>>>, bool)> {
        let key = self.key_of(id, f, header)?;
        let (points, from_disk) = self.points_of(folder, &key)?;
        let base = header.payload_start + points.header_len;
        if entries.iter().any(|e| e.size > 0 && e.end() > points.index.output_len) {
            return Some((Err(FbrbError::PayloadEnds), from_disk));
        }
        let ranges: Vec<(u64, u64)> = entries
            .iter()
            .map(|e| (u64::from(e.offset), u64::from(e.size)))
            .collect();
        // the spans the entries need decoded side by side (each task its own handle on the bundle)
        let Ok(out) = restart::read_ranges(&points.index, &id.path, base, &ranges, &self.spans, key.hash) else {
            self.forget_points(&key);
            return None; // read the bundle instead
        };
        Some((Ok(out), from_disk))
    }

    /// Decompresses the whole payload once more, making its restart points (kept in the process, and written to a
    /// points file in `folder` when given), and answers the read on the way. A stream that fails, ends early or has
    /// anything but its trailer after it gets no points: the bundle is read as the reference reads it, through its
    /// session, from then on.
    #[allow(clippy::too_many_arguments)]
    fn make_points(
        &self,
        folder: Option<&Path>,
        config: &CacheConfig,
        id: &Identity,
        f: &mut File,
        header: &Header,
        entries: &[&Entry],
        end: u64,
    ) -> Result<Vec<Vec<u8>>> {
        let session = self.session_of(id, header);
        let s = lock(&session);
        // another thread may have made them while this one waited
        if let Some((got, from_disk)) = self.points_read(folder, id, f, header, entries) {
            let mut st = self.state();
            if from_disk {
                st.stats.disk_hits += 1;
            } else {
                st.stats.memory_hits += 1;
            }
            return got;
        }
        if matches!(s.flow, Flow::Failed(_)) {
            // the stream is known to fail: never written; the session answers as the reference would
            self.state().no_disk.insert(id.clone());
            return s.answer(entries, end);
        }
        // (the session stays locked while the points are made: another thread reading the bundle waits for them)
        let Some(key) = self.key_of(id, f, header) else {
            self.state().no_disk.insert(id.clone());
            return read_payload(f, header, entries);
        };
        let mut c = Collector::new(entries);
        let made = index_payload(f, header.payload_start, &mut |d: &[u8]| c.take(d));
        // the points serve every later read (or the stream fails): the session's bytes are of no more use
        self.drop_session(id);
        drop(s);
        let Some((header_len, index)) = made else {
            // the stream failed, stopped short or has more after it: the reference's answer for this read, and
            // nothing on disk later
            let mut st = self.state();
            st.no_disk.insert(id.clone());
            st.stats.decompressed_reads += 1;
            drop(st);
            return read_payload(f, header, entries);
        };
        let output_len = index.output_len;
        let points = self.keep_points(&key, Points { header_len, index });
        let written = folder.and_then(|d| write_points(d, &key, id, &points).map(|n| (d, n)));
        {
            let mut st = self.state();
            st.stats.decompressed_reads += 1;
            st.stats.decompressed_bytes += output_len;
            if let Some((_, n)) = written {
                st.stats.written += 1;
                st.stats.written_bytes += n;
            }
        }
        if let Some((d, _)) = written {
            self.state().on_disk.insert(key.hash);
            let target = points_path(d, &key);
            drop_superseded(d, id, &target);
            trim(d, config.disk_cap, &target);
        }
        c.finish()
    }
}

/// Writes `points` to their file in `folder` (made from the bundle `id`): the file's bytes, `None` when it could not
/// be written (only a cache: the next process makes them again).
fn write_points(folder: &Path, key: &ContentKey, id: &Identity, points: &Points) -> Option<u64> {
    let mut body = Vec::new();
    body.extend(MAGIC);
    body.extend((key.bytes.len() as u32).to_le_bytes());
    body.extend(&key.bytes);
    let idb = id.bytes();
    body.extend((idb.len() as u32).to_le_bytes());
    body.extend(idb);
    body.extend(points.header_len.to_le_bytes());
    body.extend(points.index.to_bytes());
    crate::atomic::write(&points_path(folder, key), &[&body])
        .ok()
        .map(|()| body.len() as u64)
}

/// A gzip payload's restart points, made by decompressing it once from `payload_start` (zlib's header rules, as the
/// reference's `zlib.decompressobj(16 + MAX_WBITS)`), its output given to `sink`: `(the header's length, the index)`
/// when the member is whole - its trailer's CRC-32 and length checked - and `None` when it fails, ends early, or the
/// file has anything after its trailer other than the member's end.
fn index_payload(f: &mut File, payload_start: u64, sink: &mut impl FnMut(&[u8])) -> Option<(u64, Index)> {
    let len = f.metadata().ok()?.len();
    f.seek(SeekFrom::Start(payload_start)).ok()?;
    let mut head = vec![0u8; 1 << 16];
    let n = read_full(f, &mut head).ok()?;
    let header_len = super::gzip::zlib_header(head.get(..n)?).ok()?? as u64;
    let base = payload_start + header_len;
    f.seek(SeekFrom::Start(base)).ok()?;
    let built = restart::build(&mut BufReader::with_capacity(CHUNK, &mut *f), restart::SPAN, sink).ok()?;
    let ix = built.index;
    let trailer_at = base + ix.input_len;
    if trailer_at + 8 > len {
        return None;
    }
    let mut tail = [0u8; 8];
    f.seek(SeekFrom::Start(trailer_at)).ok()?;
    f.read_exact(&mut tail).ok()?;
    let crc = u32::from_le_bytes(tail.get(..4)?.try_into().ok()?);
    let isize = u32::from_le_bytes(tail.get(4..)?.try_into().ok()?);
    (crc == ix.crc && isize == (ix.output_len & 0xFFFF_FFFF) as u32).then_some((header_len, ix))
}

/// Records a session's size after a read and drops the least recently used beyond the budget (the session itself
/// too when it alone is larger).
fn charge(st: &mut State, id: &Identity, session: &Arc<Mutex<Session>>, size: u64) {
    if let Some(slot) = st.sessions.get_mut(id)
        && Arc::ptr_eq(&slot.session, session)
    {
        st.charged = st.charged - slot.charged + size;
        slot.charged = size;
    }
    while st.charged > st.config.memory {
        let Some(victim) = st.sessions.iter().min_by_key(|(_, s)| s.used).map(|(k, _)| k.clone()) else {
            break;
        };
        if let Some(slot) = st.sessions.remove(&victim) {
            st.charged -= slot.charged;
        }
    }
}

/// [`super::read_path`]: the entries named, through the process's cache.
pub(crate) fn read_path<S: AsRef<str>>(path: &Path, names: &[S]) -> Result<Vec<Vec<u8>>> {
    global().read_path(path, names)
}

/// [`super::extract_path`]: every entry, through the process's cache.
pub(crate) fn extract_path(path: &Path) -> Result<(Header, Vec<Vec<u8>>)> {
    global().extract_path(path)
}

fn points_path(folder: &Path, key: &ContentKey) -> PathBuf {
    folder.join(format!("{:016x}.{POINTS}", key.hash))
}

/// Whether a read of this bundle came before (this one leaves the marker for the next).
fn seen_before(folder: &Path, id: &Identity) -> bool {
    let marker = folder.join(format!("{}.seen", id.stem()));
    if fs::create_dir_all(folder).is_err() {
        return false;
    }
    match OpenOptions::new().write(true).create_new(true).open(&marker) {
        Ok(_) => false,
        Err(e) => e.kind() == io::ErrorKind::AlreadyExists,
    }
}

/// Deletes the least recently used cache files beyond `cap` bytes (never `keep`), temporary files of writers that
/// died, and old markers.
fn trim(folder: &Path, cap: u64, keep: &Path) {
    let now = SystemTime::now();
    if let Ok(rd) = fs::read_dir(folder) {
        for e in rd.flatten() {
            let p = e.path();
            let age = e
                .metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .unwrap_or_default();
            let ext = p.extension().and_then(|x| x.to_str()).unwrap_or("");
            if (ext == "tmp" && age > STALE_TMP) || (ext == "seen" && age > STALE_MARKER) {
                let _ = fs::remove_file(&p);
            }
        }
    }
    let mut files = cache_files(folder);
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort_by_key(|f| f.2);
    for (p, len, _) in files {
        if total <= cap {
            break;
        }
        if p != keep && fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
#[path = "cache_tests.rs"]
mod tests;
