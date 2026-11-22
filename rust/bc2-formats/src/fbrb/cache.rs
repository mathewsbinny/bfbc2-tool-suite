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
//! - **on disk, shared by every process**: the whole decompressed payload of a large bundle (from
//!   [`CacheConfig::disk_min`] compressed bytes) that has been read before (by any process: a marker file is left at
//!   the first read), written once in full and then read with a seek. The files live in
//!   [`CacheConfig::folder`], `%LOCALAPPDATA%\bfbc2-tool-suite\cache\fbrb` by default, up to
//!   [`CacheConfig::disk_cap`] bytes, the least recently used deleted first.
//!
//! Every layer is keyed by the bundle's [`Identity`]: its canonical path, length, modification and creation times,
//! taken from the open file the read uses, so a bundle replaced or rewritten is a different bundle, and a cache file
//! repeats the identity in its own header, checked before use. Cache files are written beside their final name and
//! moved over it ([`crate::atomic::replace`]), so a reader in another process sees a whole file or none.
//!
//! **Superseded payloads** (round QW1, audit AU2 S1): a bundle rewritten at the same path (an install, a scratch
//! copy) is a new identity, and its old payload can never be read again; nor can a deleted bundle's (a temporary
//! folder's). Writing a bundle's payload deletes the other payloads of the same canonical path
//! ([`drop_superseded`]); [`superseded`] lists every payload of a folder whose bundle was rewritten or deleted, for
//! the caches tool's `--prune` (`bfbc2core.caches`, `bfbc2 caches`). (The audit's "28 bundle names with 2-3
//! payloads" were mostly two bundles each - a level's `Dist` and `Package` copies -, both current.)
//!
//! **The same results.** The reference reads the payload 1 MiB of compressed bytes at a time, from the start, until
//! the decompressed bytes reach the end of the last entry asked for; an error in any chunk it reads is raised, and
//! nothing after that chunk is looked at. A [`Session`] feeds the same chunks to the same inflater, so its state
//! after `k` chunks is the reference's; it keeps only what whole chunks gave, and remembers a gzip error with the
//! output before the failing chunk, so a read that would have reached that chunk gets the same error and one that
//! would not gets its bytes. A payload goes to disk only once its whole stream has been read with its checksum and
//! length checked: every chunk is then valid, so any read of it is a copy (or [`FbrbError::PayloadEnds`] past its
//! end), as the reference's would be. A bundle whose stream fails is never written; its reads keep the session's
//! rules. Stored (unzipped) payloads need none of this: [`super::read_payload`] seeks to each entry.

use std::collections::{HashMap, HashSet, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::error::{FbrbError, Result};
use super::gzip::Member;
use super::header::{Entry, Header, read_header};
use super::read::{CHUNK, Collector, read_full, read_payload};

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

/// A cache file's first bytes (the format's version is in them).
const MAGIC: &[u8; 8] = b"BC2FBPL1";
/// A temporary file written to this recently is another writer at work: a reader does not write the bundle too.
const BUSY: Duration = Duration::from_secs(60);
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
    /// Reads answered from a payload on disk.
    pub disk_hits: u64,
    /// Reads answered from a session's bytes without decompressing anything.
    pub memory_hits: u64,
    /// Reads that decompressed (from the start, or continuing a session).
    pub decompressed_reads: u64,
    /// Payload bytes decompressed.
    pub decompressed_bytes: u64,
    /// Payloads written to disk, and their bytes.
    pub written: u64,
    /// The bytes of the payloads written.
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
    stats: CacheStats,
}

static GLOBAL: OnceLock<Cache> = OnceLock::new();
static TICK: AtomicU64 = AtomicU64::new(0);
static TMP: AtomicU64 = AtomicU64::new(0);

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

/// The bytes of the payloads in the disk cache's `folder`, and their count.
#[must_use]
pub fn disk_usage(folder: &Path) -> (u64, usize) {
    let files = payload_files(folder);
    (files.iter().map(|f| f.1).sum(), files.len())
}

/// The identity a payload file's header names (`None`: not a payload file of this format, or damaged).
fn payload_identity(file: &Path) -> Option<Identity> {
    let mut f = File::open(file).ok()?;
    let mut head = [0u8; 20];
    f.read_exact(&mut head).ok()?;
    let (magic, rest) = head.split_at(8);
    if magic != MAGIC {
        return None;
    }
    let key_len = u32::from_le_bytes(rest.get(8..12)?.try_into().ok()?) as usize;
    if !(44..=44 + 32_768).contains(&key_len) {
        return None;
    }
    let mut key = vec![0; key_len];
    f.read_exact(&mut key).ok()?;
    let len = u64::from_le_bytes(key.get(..8)?.try_into().ok()?);
    let modified = u128::from_le_bytes(key.get(8..24)?.try_into().ok()?);
    let created = u128::from_le_bytes(key.get(24..40)?.try_into().ok()?);
    let path_len = u32::from_le_bytes(key.get(40..44)?.try_into().ok()?) as usize;
    let path = String::from_utf8(key.get(44..44 + path_len)?.to_vec()).ok()?;
    (44 + path_len == key_len).then(|| Identity {
        path: PathBuf::from(path),
        len,
        modified,
        created,
    })
}

/// The payload files of `folder` no read can use again (round QW1): those whose bundle was rewritten (the file at
/// their header's path has another identity now) or deleted (nothing is at that path). A bundle that is there but
/// cannot be opened (held by another program) keeps its payloads, and a file that is not a readable payload of this
/// format is never listed.
#[must_use]
pub fn superseded(folder: &Path) -> Vec<PathBuf> {
    let mut by_path: HashMap<PathBuf, Vec<(PathBuf, Identity)>> = HashMap::new();
    for (file, _, _) in payload_files(folder) {
        if let Some(id) = payload_identity(&file) {
            by_path.entry(id.path.clone()).or_default().push((file, id));
        }
    }
    let mut out = Vec::new();
    #[allow(
        clippy::iter_over_hash_type,
        reason = "the stale files to delete; the order of deleting cannot show"
    )]
    for (bundle, files) in by_path {
        match File::open(&bundle).and_then(|f| Identity::of(&bundle, &f)) {
            Ok(now) => out.extend(files.into_iter().filter(|f| f.1 != now).map(|f| f.0)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => out.extend(files.into_iter().map(|f| f.0)),
            Err(_) => {}
        }
    }
    out.sort();
    out
}

/// Deletes the payload files of the bundle `id` names other than its own (the bundle was rewritten: they are never
/// read again). Only files whose header names the same canonical path are touched; one another process holds open
/// stays (Windows) and goes with the least-recently-used cap or the caches tool's `--prune`.
fn drop_superseded(folder: &Path, id: &Identity) {
    let own = payload_path(folder, id);
    let stem = id.stem();
    let prefix = stem.rsplit_once('-').map_or(stem.as_str(), |(p, _)| p);
    for (file, _, _) in payload_files(folder) {
        let named = file.file_name().map_or_default(|n| n.to_string_lossy().into_owned());
        if file != own
            && named.starts_with(prefix)
            && payload_identity(&file).is_some_and(|other| other.path == id.path && other != *id)
        {
            let _ = fs::remove_file(&file);
        }
    }
}

/// `[(path, length, modified)]` of the payload files in `folder`.
fn payload_files(folder: &Path) -> Vec<(PathBuf, u64, SystemTime)> {
    let Ok(rd) = fs::read_dir(folder) else {
        return Vec::new();
    };
    rd.flatten()
        .filter_map(|e| {
            let p = e.path();
            let m = e.metadata().ok()?;
            (p.extension().is_some_and(|x| x == "payload") && m.is_file())
                .then(|| (p, m.len(), m.modified().unwrap_or(UNIX_EPOCH)))
        })
        .collect()
}

/// A bundle cache: headers and payloads kept in memory, with a disk cache's folder (see the module's text). The
/// process has one ([`global`]); tests make their own.
pub struct Cache {
    state: Mutex<State>,
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
                stats: CacheStats::default(),
            }),
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
    }

    /// Forgets what was kept in memory (the configuration and the disk cache stay).
    pub fn clear_memory(&self) {
        let mut s = self.state();
        s.sessions.clear();
        s.charged = 0;
        s.headers.clear();
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
        if let Some(folder) = config.folder.as_deref().filter(|_| big) {
            if let Some(got) = self.disk_read(folder, id, entries) {
                self.state().stats.disk_hits += 1;
                return got;
            }
            // (another process writing it now: this read goes on as an unseen one, and later ones find the file)
            if seen_before(folder, id) && !self.state().no_disk.contains(id) && !writer_busy(folder, id) {
                return self.to_disk(folder, &config, id, f, header, entries, end);
            }
        }
        if end > config.memory {
            // more than the cache keeps: read as the reference does
            let mut st = self.state();
            st.stats.decompressed_reads += 1;
            st.stats.decompressed_bytes += end;
            drop(st);
            return read_payload(f, header, entries);
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

    /// The entries from the disk cache, `None` when it has no (whole, matching) file for the bundle.
    fn disk_read(&self, folder: &Path, id: &Identity, entries: &[&Entry]) -> Option<Result<Vec<Vec<u8>>>> {
        let (mut f, start, len) = open_payload(folder, id)?;
        let path = payload_path(folder, id);
        if self.state().touched.insert(path.clone()) {
            // the least recently used go first when the cache is trimmed: this one was just used
            if let Ok(w) = OpenOptions::new().write(true).open(&path) {
                let _ = w.set_modified(SystemTime::now());
            }
        }
        let mut out = Vec::with_capacity(entries.len());
        for e in entries {
            if e.size == 0 {
                out.push(Vec::new());
                continue;
            }
            if e.end() > len {
                return Some(Err(FbrbError::PayloadEnds));
            }
            let mut b = vec![0; e.size as usize];
            let read = f
                .seek(SeekFrom::Start(start + u64::from(e.offset)))
                .and_then(|_| f.read_exact(&mut b));
            if read.is_err() {
                return None; // the file went away or was cut short: read the bundle instead
            }
            out.push(b);
        }
        Some(Ok(out))
    }

    /// Decompresses the whole payload (continuing this cache's session when there is one) into a new cache file and
    /// answers the read from it on the way. A stream that fails or ends early is not written: the bundle is read as
    /// the reference reads it, through its session, from then on.
    #[allow(clippy::too_many_arguments)]
    fn to_disk(
        &self,
        folder: &Path,
        config: &CacheConfig,
        id: &Identity,
        f: &mut File,
        header: &Header,
        entries: &[&Entry],
        end: u64,
    ) -> Result<Vec<Vec<u8>>> {
        let session = self.session_of(id, header);
        let mut s = lock(&session);
        // another thread may have written it while this one waited
        if let Some(got) = self.disk_read(folder, id, entries) {
            self.state().stats.disk_hits += 1;
            return got;
        }
        if matches!(s.flow, Flow::Failed(_)) {
            // the stream is known to fail: never written; the session answers as the reference would
            self.state().no_disk.insert(id.clone());
            return s.answer(entries, end);
        }
        // (a session whose member ended goes on below: there is nothing more to feed, and its bytes are written)
        let tmp = folder.join(format!(
            "{}.{}.{}.tmp",
            id.stem(),
            std::process::id(),
            TMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut out = File::create(&tmp).ok().map(|w| BufWriter::with_capacity(1 << 20, w));
        let mut c = Collector::new(entries);
        c.take(&s.data);
        let prefix = s.data.len() as u64;
        // the payload's length goes into the file's header once it is known
        if let Some(w) = out.as_mut()
            && (w.write_all(&file_header(id, 0)).is_err() || w.write_all(&s.data).is_err())
        {
            out = None;
        }
        let mut made = 0u64;
        let fed = feed_rest(&mut s, f, &mut |d: &[u8]| {
            c.take(d);
            made += d.len() as u64;
            if let Some(w) = out.as_mut()
                && w.write_all(d).is_err()
            {
                out = None;
            }
        });
        let done = s.member.done();
        // the session's member has gone past its bytes: a thread that waited for it starts again from the start
        *s = Session::new(header.payload_start);
        drop(s);
        self.drop_session(id);
        {
            let mut st = self.state();
            st.stats.decompressed_reads += 1;
            st.stats.decompressed_bytes += made;
        }
        let total = prefix + made;
        let target = payload_path(folder, id);
        let written = match (&fed, done, out) {
            (Ok(()), true, Some(w)) => {
                finish_file(w, id, total).is_ok() && crate::atomic::replace(&tmp, &target).is_ok()
            }
            _ => false,
        };
        if written {
            let mut st = self.state();
            st.stats.written += 1;
            st.stats.written_bytes += total;
            drop(st);
            drop_superseded(folder, id);
            trim(folder, config.disk_cap, &target);
        } else {
            let _ = fs::remove_file(&tmp);
        }
        if fed.is_ok() && done {
            c.finish()
        } else {
            // the stream failed or stopped short: the reference's answer for this read, and nothing on disk later
            self.state().no_disk.insert(id.clone());
            read_payload(f, header, entries)
        }
    }
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

fn payload_path(folder: &Path, id: &Identity) -> PathBuf {
    folder.join(format!("{}.payload", id.stem()))
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

/// Whether another writer's temporary file for this bundle was written to in the last [`BUSY`].
fn writer_busy(folder: &Path, id: &Identity) -> bool {
    let prefix = format!("{}.", id.stem());
    let now = SystemTime::now();
    let Ok(rd) = fs::read_dir(folder) else { return false };
    rd.flatten().any(|e| {
        let name = e.file_name();
        let name = name.to_string_lossy();
        name.starts_with(&prefix)
            && name.ends_with(".tmp")
            && e.metadata()
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| now.duration_since(t).ok())
                .is_some_and(|age| age < BUSY)
    })
}

/// A cache file's header for `id` and a payload of `len` bytes.
fn file_header(id: &Identity, len: u64) -> Vec<u8> {
    let key = id.bytes();
    let mut h = Vec::with_capacity(key.len() + 20);
    h.extend(MAGIC);
    h.extend(len.to_le_bytes());
    h.extend((key.len() as u32).to_le_bytes());
    h.extend(key);
    h
}

/// Opens the cache file of `id` and checks it: `(file, where the payload starts, its length)`.
fn open_payload(folder: &Path, id: &Identity) -> Option<(File, u64, u64)> {
    let mut f = File::open(payload_path(folder, id)).ok()?;
    let mut head = [0u8; 20];
    f.read_exact(&mut head).ok()?;
    let (magic, rest) = head.split_at(8);
    if magic != MAGIC {
        return None;
    }
    let len = u64::from_le_bytes(rest.get(..8)?.try_into().ok()?);
    let key_len = u32::from_le_bytes(rest.get(8..12)?.try_into().ok()?) as usize;
    let want = id.bytes();
    if key_len != want.len() {
        return None;
    }
    let mut key = vec![0; key_len];
    f.read_exact(&mut key).ok()?;
    let start = 20 + key_len as u64;
    if key != want || f.metadata().ok()?.len() != start + len {
        return None;
    }
    Some((f, start, len))
}

/// Feeds the rest of the session's stream, chunk by chunk as the reference does, to `sink`.
fn feed_rest(s: &mut Session, f: &mut File, sink: &mut impl FnMut(&[u8])) -> Result<()> {
    f.seek(SeekFrom::Start(s.next_in))?;
    let mut buf = vec![0; CHUNK];
    loop {
        let n = read_full(f, &mut buf)?;
        if n == 0 {
            return Ok(());
        }
        s.member.feed(buf.get(..n).unwrap_or_default(), sink)?;
        s.next_in += n as u64;
        if s.member.done() {
            return Ok(());
        }
    }
}

/// Writes the payload's length into the cache file's header and makes the file durable.
fn finish_file(w: BufWriter<File>, id: &Identity, len: u64) -> io::Result<()> {
    let mut f = w.into_inner().map_err(io::IntoInnerError::into_error)?;
    f.seek(SeekFrom::Start(0))?;
    f.write_all(&file_header(id, len))?;
    f.sync_all()
}

/// Deletes the least recently used payloads beyond `cap` bytes (never `keep`), temporary files of writers that
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
    let mut files = payload_files(folder);
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
