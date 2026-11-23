//! The catalogue's cache (round PR1b-1e): `work\tools_out\dbx_catalogue.bin` in Project OPFOR BFBC2's folder, this
//! module's own format (the Python's pickle of `Catalogue` with `dbx.Element` trees retired), read and written for
//! `bfbc2.exe dbxcatalogue|dbxparity` and, through `bc2_native.catalogue_load`, for `dbxcatalogue.load` (the WW2
//! tools of Project OPFOR).
//!
//! **Keyed by content** ([`key`]): the SHA-256 of what the pass reads - each bundle's path below the project folder,
//! the entry names the index gives it (in its order) and the bundle's content key, and the executable's SHA-256.
//! A bundle's content key is its length, the SHA-256 of its first 64 KiB (or of its whole compressed header when
//! that is longer: the header and the payload's start) and its last 8 bytes, the gzip trailer's CRC-32 of the whole
//! decompressed payload and its length (the bundle cache's points key, `bc2_gamefs::bundles`, with the header
//! whole); a bundle whose payload is stored rather than zipped, or that is not a bundle, is hashed whole. Hashing
//! every bundle whole (8.9 GB) would cost more than the catalogue it keys; the trailer's CRC-32 covers every byte
//! of the payload, the header hash every name and offset. A game copy moved or copied keeps its catalogue; a
//! bundle rewritten with other bytes and the old time does not.
//!
//! **The file**: the magic, [`CACHE_VERSION`], the key, a string table (every name, type, path and text once),
//! the tables of [`Built`] in their insertion order, the bytes of the partitions whose elements the catalogue keeps
//! (class defaults, second instances, array prototypes: parsed again by the reader, as [`super::build`] hands them
//! over; each deflated at the fast level: on the user's copy 463 partitions, 65 MB as they are), the executable's identifiers, then the SHA-256 of everything before it (a damaged file is rebuilt, never
//! half read).
//!
//! [`load`] reads it when it is this game copy's, else builds the catalogue (under the file's lock: one process
//! builds, one that waited reads what the other wrote) and writes it, unless the build had problems (an incomplete
//! catalogue is used but never cached, as before); then the cache's folder and the overall budget are pruned
//! (`bc2_gamefs::budget`, the `dbxcatalogue` cache).

use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use bc2_gamefs::sha256::Sha256;
use rayon::prelude::*;

use super::{Built, Counter, ElemRef, Job, Key, build, exe_identifiers};
use crate::pyfmt::OrderedMap;

/// The cache's file name in `work\tools_out`.
pub const FILE: &str = "dbx_catalogue.bin";
/// The file's first bytes.
const MAGIC: &[u8; 8] = b"BC2DBXCT";
/// The format's version: a file of another is rebuilt.
pub const CACHE_VERSION: u32 = 1;
/// The bytes of a zipped bundle's start that its content key hashes (at least).
const HEAD: u64 = 1 << 16;
/// The bundles read at once by a build (`dbxcatalogue.GROUP`).
pub const GROUP: usize = 8;

/// The catalogue: the pass's tables and the executable's identifiers (`exe_names`: each once, in file order).
#[derive(Debug, Default)]
pub struct Catalogue {
    /// The pass's tables.
    pub built: Built,
    /// The executable's identifiers.
    pub exe_names: Vec<String>,
}

// ------------------------------------------------------------------------------------------------ the key

/// A bundle's content key (the module's text), as hex.
///
/// # Errors
/// The file cannot be opened or read.
pub fn bundle_key(path: &str) -> io::Result<String> {
    let mut f = File::open(path)?;
    let len = f.metadata()?.len();
    let start = match bc2_formats::fbrb::read_header(&mut f) {
        Ok(h) if h.zipped && len >= 8 => Some(h.payload_start),
        _ => None,
    };
    let mut s = Sha256::default();
    s.update(&len.to_le_bytes());
    f.seek(SeekFrom::Start(0))?;
    if let Some(p) = start {
        s.update(b"zipped");
        let mut head = Vec::new();
        (&mut f).take(p.max(HEAD).min(len)).read_to_end(&mut head)?;
        s.update(&head);
        let mut tail = [0u8; 8];
        f.seek(SeekFrom::Start(len - 8))?;
        f.read_exact(&mut tail)?;
        s.update(&tail);
        return Ok(s.hexdigest());
    }
    s.update(b"whole");
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        s.update(buf.get(..n).unwrap_or_default());
    }
    Ok(s.hexdigest())
}

/// `path` below `root` when it is there (compared without regard to case or slash direction), with `/`; else
/// itself.
fn relative(path: &str, root: &str) -> String {
    let norm = |p: &str| p.replace('\\', "/").to_lowercase();
    let (p, r) = (norm(path), norm(root));
    let r = r.trim_end_matches('/');
    match p.strip_prefix(r).and_then(|rest| rest.strip_prefix('/')) {
        Some(rest) if !r.is_empty() => rest.to_owned(),
        _ => p,
    }
}

/// The project folder of a cache file at `<project>\work\tools_out\<file>` (three folders up; empty when there are
/// fewer).
#[must_use]
pub fn project_of(cache: &Path) -> String {
    cache
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent)
        .map_or_default(|p| p.to_string_lossy().into_owned())
}

/// The catalogue's key for `jobs` (the bundles and their names, in the index's order), the executable (`None`, or a
/// path that is not a file: none) and the project folder the paths are taken below; `None` when a bundle cannot be
/// read (the build then says so, and nothing is cached).
#[must_use]
pub fn key(jobs: &[Job], exe: Option<&str>, root: &str) -> Option<String> {
    let keys: Vec<io::Result<String>> = jobs.par_iter().map(|(b, _)| bundle_key(b)).collect();
    let mut s = Sha256::default();
    s.update(format!("bc2 dbxcatalogue v{CACHE_VERSION}\n").as_bytes());
    for ((b, names), k) in jobs.iter().zip(keys) {
        let k = k.ok()?;
        s.update(format!("{}\0{}\0{k}\n", relative(b, root), names.len()).as_bytes());
        for n in names {
            s.update(n.as_bytes());
            s.update(b"\0");
        }
    }
    match exe.filter(|e| Path::new(e).is_file()) {
        Some(e) => s.update(format!("exe {}\n", bc2_gamefs::sha256::file(e).ok()?).as_bytes()),
        None => s.update(b"no exe\n"),
    }
    Some(s.hexdigest())
}

// ------------------------------------------------------------------------------------------------ the file

/// The file's writer: the body with strings as numbers into the table.
#[derive(Default)]
struct Writer {
    body: Vec<u8>,
    table: Vec<String>,
    at: HashMap<String, u32>,
}

impl Writer {
    fn u32(&mut self, v: usize) {
        self.body
            .extend_from_slice(&u32::try_from(v).unwrap_or(u32::MAX).to_le_bytes());
    }

    fn u64(&mut self, v: u64) {
        self.body.extend_from_slice(&v.to_le_bytes());
    }

    fn s(&mut self, v: &str) {
        let id = if let Some(&id) = self.at.get(v) {
            id
        } else {
            let id = u32::try_from(self.table.len()).unwrap_or(u32::MAX);
            self.table.push(v.to_owned());
            self.at.insert(v.to_owned(), id);
            id
        };
        self.body.extend_from_slice(&id.to_le_bytes());
    }

    fn strs(&mut self, v: &[String]) {
        self.u32(v.len());
        for x in v {
            self.s(x);
        }
    }

    fn counters(&mut self, m: &OrderedMap<Key, Counter>) {
        self.u32(m.len());
        for ((t, p), c) in m.iter() {
            self.s(t);
            self.s(p);
            self.u32(c.len());
            for (k, n) in c.iter() {
                self.s(k);
                self.u64(*n);
            }
        }
    }

    fn elem(&mut self, r: ElemRef) {
        self.u32(r.0);
        self.u32(r.1);
    }

    fn named(&mut self, m: &OrderedMap<String, (String, ElemRef)>) {
        self.u32(m.len());
        for (t, (name, r)) in m.iter() {
            self.s(t);
            self.s(name);
            self.elem(*r);
        }
    }
}

/// The cache file's bytes for catalogue `c` under `key`.
#[must_use]
pub fn cache_bytes(c: &Catalogue, key: &str) -> Vec<u8> {
    let b = &c.built;
    let mut w = Writer::default();
    w.u64(b.partitions as u64);
    w.strs(&b.classes);
    w.u32(b.order.len());
    for (t, names) in b.order.iter() {
        w.s(t);
        w.strs(names);
    }
    w.counters(&b.shapes);
    w.counters(&b.texts);
    w.counters(&b.numeric);
    w.u32(b.protos.len());
    for ((t, p), r) in b.protos.iter() {
        w.s(t);
        w.s(p);
        w.elem(*r);
    }
    w.u32(b.guids.len());
    for (g, parts) in b.guids.iter() {
        w.s(g);
        w.strs(parts);
    }
    w.u32(b.primary.len());
    for (k, v) in b.primary.iter() {
        w.s(k);
        w.s(v);
    }
    w.named(&b.defaults);
    w.named(&b.others);
    w.strs(&b.problems);
    w.strs(&b.names);
    let mut kept: Vec<(&usize, &Vec<u8>)> = b.kept.iter().collect();
    kept.sort_unstable_by_key(|(pi, _)| **pi);
    // each partition deflated (the fast level: the file is about half the size, read back in a fraction of a second)
    let packed: Vec<(usize, usize, Vec<u8>)> = kept
        .par_iter()
        .map(|(pi, d)| {
            let mut z = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
            let _ = z.write_all(d);
            (**pi, d.len(), z.finish().unwrap_or_default())
        })
        .collect();
    w.u32(packed.len());
    for (pi, len, z) in packed {
        w.u32(pi);
        w.u32(len);
        w.u32(z.len());
        w.body.extend_from_slice(&z);
    }
    w.strs(&c.exe_names);
    let mut out = Vec::with_capacity(w.body.len() + w.table.iter().map(|s| s.len() + 4).sum::<usize>() + 256);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    out.extend_from_slice(key.as_bytes().get(..64).unwrap_or_default());
    out.resize(MAGIC.len() + 4 + 64, b'0');
    out.extend_from_slice(&u32::try_from(w.table.len()).unwrap_or(u32::MAX).to_le_bytes());
    for s in &w.table {
        out.extend_from_slice(&u32::try_from(s.len()).unwrap_or(u32::MAX).to_le_bytes());
        out.extend_from_slice(s.as_bytes());
    }
    out.extend_from_slice(&w.body);
    let mut h = Sha256::default();
    h.update(&out);
    out.extend_from_slice(h.hexdigest().as_bytes());
    out
}

/// A cache file's bytes as format `version` would mark them (the tests' file of another version), its digest
/// made again.
#[must_use]
pub fn with_version(data: &[u8], version: u32) -> Vec<u8> {
    let mut out = data.get(..data.len().saturating_sub(64)).unwrap_or_default().to_vec();
    if let Some(v) = out.get_mut(MAGIC.len()..MAGIC.len() + 4) {
        v.copy_from_slice(&version.to_le_bytes());
    }
    let mut h = Sha256::default();
    h.update(&out);
    out.extend_from_slice(h.hexdigest().as_bytes());
    out
}

/// A cursor over the file's bytes.
struct Reader<'a> {
    d: &'a [u8],
    at: usize,
    table: Vec<String>,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }

    fn u32(&mut self) -> Option<usize> {
        usize::try_from(u32::from_le_bytes(self.take(4)?.try_into().ok()?)).ok()
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    /// A count of items each at least `min` bytes long (a damaged count never allocates past the file).
    fn count(&mut self, min: usize) -> Option<usize> {
        let n = self.u32()?;
        (n.checked_mul(min)? <= self.d.len().saturating_sub(self.at)).then_some(n)
    }

    fn s(&mut self) -> Option<String> {
        let id = self.u32()?;
        self.table.get(id).cloned()
    }

    fn strs(&mut self) -> Option<Vec<String>> {
        let n = self.count(4)?;
        (0..n).map(|_| self.s()).collect()
    }

    fn counters(&mut self) -> Option<OrderedMap<Key, Counter>> {
        let mut m = OrderedMap::new();
        for _ in 0..self.count(12)? {
            let k = (self.s()?, self.s()?);
            let mut c = Counter::new();
            for _ in 0..self.count(12)? {
                let name = self.s()?;
                c.insert(name, self.u64()?);
            }
            m.insert(k, c);
        }
        Some(m)
    }

    fn elem(&mut self) -> Option<ElemRef> {
        Some((self.u32()?, self.u32()?))
    }

    fn named(&mut self) -> Option<OrderedMap<String, (String, ElemRef)>> {
        let mut m = OrderedMap::new();
        for _ in 0..self.count(16)? {
            let t = self.s()?;
            let name = self.s()?;
            m.insert(t, (name, self.elem()?));
        }
        Some(m)
    }
}

/// The catalogue in a cache file's bytes when they are whole, of this format and keyed `key`; `None` otherwise.
#[must_use]
pub fn from_cache_bytes(data: &[u8], key: &str) -> Option<Catalogue> {
    let (body, digest) = data.split_at_checked(data.len().checked_sub(64)?)?;
    let head = MAGIC.len() + 4 + 64;
    if body.get(..MAGIC.len())? != MAGIC
        || body.get(MAGIC.len()..MAGIC.len() + 4)? != CACHE_VERSION.to_le_bytes()
        || body.get(MAGIC.len() + 4..head)? != key.as_bytes()
    {
        return None;
    }
    let mut h = Sha256::default();
    h.update(body);
    if h.hexdigest().as_bytes() != digest {
        return None;
    }
    let mut r = Reader {
        d: body,
        at: head,
        table: Vec::new(),
    };
    let n = r.count(4)?;
    let mut table = Vec::with_capacity(n);
    for _ in 0..n {
        let len = r.u32()?;
        table.push(std::str::from_utf8(r.take(len)?).ok()?.to_owned());
    }
    r.table = table;
    let mut b = Built {
        partitions: usize::try_from(r.u64()?).ok()?,
        classes: r.strs()?,
        ..Built::default()
    };
    for _ in 0..r.count(8)? {
        let t = r.s()?;
        b.order.insert(t, r.strs()?);
    }
    b.shapes = r.counters()?;
    b.texts = r.counters()?;
    b.numeric = r.counters()?;
    for _ in 0..r.count(16)? {
        let k = (r.s()?, r.s()?);
        b.protos.insert(k, r.elem()?);
    }
    for _ in 0..r.count(8)? {
        let g = r.s()?;
        b.guids.insert(g, r.strs()?);
    }
    for _ in 0..r.count(8)? {
        let k = r.s()?;
        b.primary.insert(k, r.s()?);
    }
    b.defaults = r.named()?;
    b.others = r.named()?;
    b.problems = r.strs()?;
    b.names = r.strs()?;
    let mut packed: Vec<(usize, usize, &[u8])> = Vec::new();
    for _ in 0..r.count(12)? {
        let pi = r.u32()?;
        let len = r.u32()?;
        let n = r.u32()?;
        packed.push((pi, len, r.take(n)?));
    }
    let unpacked: Vec<Option<(usize, Vec<u8>)>> = packed
        .par_iter()
        .map(|&(pi, len, z)| {
            let mut d = Vec::with_capacity(len);
            flate2::read::DeflateDecoder::new(z).read_to_end(&mut d).ok()?;
            (d.len() == len).then_some((pi, d))
        })
        .collect();
    for u in unpacked {
        let (pi, d) = u?;
        b.kept.insert(pi, d);
    }
    // every element named is in a kept partition
    let named = b.defaults.iter().chain(b.others.iter()).map(|(_, (_, r))| r);
    if b.protos
        .iter()
        .map(|(_, r)| r)
        .chain(named)
        .any(|(pi, _)| !b.kept.contains_key(pi))
    {
        return None;
    }
    let exe_names = r.strs()?;
    (r.at == body.len()).then_some(Catalogue { built: b, exe_names })
}

// ------------------------------------------------------------------------------------------------ load

/// What [`load`] says: `Start` before a build (the Python prints it on stderr whatever its log), `Log` the build's
/// lines (its log).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Said {
    /// "cataloguing BFBC2's partitions (once; cached) ..."
    Start,
    /// A line of the build's log.
    Log,
}

/// A catalogue had.
#[derive(Debug)]
pub struct Loaded {
    /// The catalogue.
    pub catalogue: Catalogue,
    /// Built now (not read from the cache).
    pub built: bool,
}

/// The executable's identifiers, each once in file order (`exe_identifiers`' set; none without the file).
fn exe_names(exe: Option<&str>) -> Vec<String> {
    let Some(d) = exe
        .filter(|e| Path::new(e).is_file())
        .and_then(|e| std::fs::read(e).ok())
    else {
        return Vec::new();
    };
    let mut seen = std::collections::HashSet::new();
    exe_identifiers(&d)
        .into_iter()
        .filter(|n| seen.insert(n.clone()))
        .collect()
}

/// The catalogue in the cache at `path` when it is keyed `key`; marked used (its time set to now, at most once a
/// minute: `caches.touch`).
fn read(path: &Path, key: &str) -> Option<Catalogue> {
    let c = from_cache_bytes(&std::fs::read(path).ok()?, key)?;
    let p = path.to_string_lossy();
    if bc2_gamefs::budget::stat(&p).is_ok_and(|st| bc2_gamefs::budget::now() - st.mtime >= 60.0) {
        let _ = bc2_gamefs::budget::utime_now(&p);
    }
    Some(c)
}

/// `dbxcatalogue.load(ix, exe, log)`: the catalogue of `jobs` (`_names(ix)`) from the cache at `cache` when it is
/// this game copy's ([`key`]), else built ([`super::build`], [`GROUP`] bundles at once) and written there unless the
/// build had problems (said, with the catalogue then not cached). `root`: the project folder (the key's paths, the
/// budget). `None`: the build refused (the caller runs the reference).
pub fn load(
    jobs: &[Job],
    exe: Option<&str>,
    root: &str,
    cache: Option<&Path>,
    say: &mut dyn FnMut(Said, &str),
) -> Option<Loaded> {
    let k = cache.and_then(|_| key(jobs, exe, root));
    let (Some(path), Some(k)) = (cache, k) else {
        return build_one(jobs, exe, root, None, say);
    };
    if let Some(catalogue) = read(path, &k) {
        return Some(Loaded {
            catalogue,
            built: false,
        });
    }
    let lock = PathBuf::from(format!("{}.lock", path.display()));
    let mut out = None;
    let locked = bc2_gamefs::atomic::locked(&lock, || {
        out = match read(path, &k) {
            Some(catalogue) => Some(Loaded {
                catalogue,
                built: false,
            }),
            None => build_one(jobs, exe, root, Some((path, &k)), say),
        };
    });
    if locked.is_err() {
        // the lock cannot be taken (a read-only folder): built, not cached
        return build_one(jobs, exe, root, None, say);
    }
    out
}

/// The build and, with `cache`, its file written (`_build_and_cache`).
fn build_one(
    jobs: &[Job],
    exe: Option<&str>,
    root: &str,
    cache: Option<(&Path, &str)>,
    say: &mut dyn FnMut(Said, &str),
) -> Option<Loaded> {
    say(Said::Start, "cataloguing BFBC2's partitions (once; cached) ...");
    let t0 = std::time::Instant::now();
    let built = build(jobs, GROUP)?;
    let catalogue = Catalogue {
        exe_names: exe_names(exe),
        built,
    };
    let b = &catalogue.built;
    say(
        Said::Log,
        &format!(
            "  catalogue: {n}/{n} bundles, {} partitions, {:.0} s",
            b.partitions,
            t0.elapsed().as_secs_f64(),
            n = jobs.len()
        ),
    );
    if !b.problems.is_empty() {
        // an incomplete catalogue (a bundle locked by the running game, say) is used but not cached
        say(
            Said::Log,
            &format!(
                "catalogue: {} read problem(s) - this catalogue is incomplete and NOT cached:",
                b.problems.len()
            ),
        );
        for p in b.problems.iter().take(20) {
            say(Said::Log, &format!("  {p}"));
        }
        if b.problems.len() > 20 {
            say(Said::Log, &format!("  ... {} more", b.problems.len() - 20));
        }
    } else if let Some((path, k)) = cache {
        match bc2_gamefs::atomic::write(path, &[&cache_bytes(&catalogue, k)]) {
            Ok(()) => {
                if let (Some(dir), Some(s)) = (path.parent(), bc2_gamefs::budget::spec("dbxcatalogue")) {
                    bc2_gamefs::budget::maybe_prune(s, &dir.to_string_lossy(), Some(root));
                }
            }
            Err(e) => say(Said::Log, &format!("catalogue: built but not cached this time ({e})")),
        }
    }
    Some(Loaded { catalogue, built: true })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn sample() -> Catalogue {
        let mut b = Built {
            partitions: 7,
            classes: vec!["A.T".into(), "B.U".into()],
            ..Built::default()
        };
        b.order.insert("A.T".into(), vec!["Name".into(), "Width".into()]);
        let mut c = Counter::new();
        c.insert("field-text".into(), 3);
        c.insert("num4x1".into(), 1);
        b.shapes.insert(("A.T".into(), ".Name".into()), c.clone());
        b.texts.insert(("A.T".into(), ".Name".into()), c.clone());
        b.numeric.insert(("A.T".into(), ".Width".into()), c);
        b.protos.insert(("A.T".into(), ".Items[]".into()), (0, 3));
        b.guids.insert("0f".into(), vec!["a".into()]);
        b.guids.insert("1a".into(), vec!["a".into(), "b".into()]);
        b.primary.insert("a".into(), "0f".into());
        b.defaults.insert("A.T".into(), ("a".into(), (0, 1)));
        b.others.insert("A.T".into(), ("b".into(), (1, 1)));
        b.names = vec!["a".into(), "b".into()];
        b.kept.insert(0, vec![1, 2, 3]);
        b.kept.insert(1, vec![]);
        Catalogue {
            built: b,
            exe_names: vec!["Name".into(), "café".into()],
        }
    }

    #[test]
    fn the_file_round_trips_and_refuses_damage() {
        let key = "ab".repeat(32);
        let c = sample();
        let d = cache_bytes(&c, &key);
        let got = from_cache_bytes(&d, &key).unwrap();
        assert_eq!(cache_bytes(&got, &key), d);
        assert_eq!(got.built.partitions, 7);
        assert_eq!(
            got.built.guids.get(&"1a".to_owned()).unwrap(),
            &vec!["a".to_owned(), "b".to_owned()]
        );
        assert_eq!(got.exe_names, c.exe_names);
        assert_eq!(got.built.kept[&0], vec![1, 2, 3]);
        assert!(from_cache_bytes(&d, &"cd".repeat(32)).is_none(), "another key");
        for i in [0, 9, 80, d.len() / 2, d.len() - 1] {
            let mut bad = d.clone();
            bad[i] ^= 1;
            assert!(from_cache_bytes(&bad, &key).is_none(), "byte {i} changed");
        }
        assert!(from_cache_bytes(&d[..d.len() - 1], &key).is_none());
        assert!(from_cache_bytes(b"", &key).is_none());
    }

    #[test]
    fn keys_follow_content_not_place() {
        let dir = std::env::temp_dir().join(format!("bc2-dbxcatalogue-key-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for sub in ["one", "two"] {
            std::fs::create_dir_all(dir.join(sub).join("game")).unwrap();
            std::fs::write(dir.join(sub).join("game").join("a.fbrb"), b"not a bundle").unwrap();
        }
        let root = |s: &str| dir.join(s).to_string_lossy().into_owned();
        let job = |s: &str| {
            vec![(
                dir.join(s).join("game").join("a.fbrb").to_string_lossy().into_owned(),
                vec!["x.dbx".to_owned()],
            )]
        };
        let one = key(&job("one"), None, &root("one")).unwrap();
        assert_eq!(
            one,
            key(&job("two"), None, &root("two")).unwrap(),
            "the same bytes elsewhere"
        );
        std::fs::write(dir.join("two").join("game").join("a.fbrb"), b"not a bundle!").unwrap();
        assert_ne!(one, key(&job("two"), None, &root("two")).unwrap(), "other bytes");
        let mut named = job("one");
        named[0].1.push("y.dbx".into());
        assert_ne!(one, key(&named, None, &root("one")).unwrap(), "other names");
        assert!(key(&job("three"), None, &root("three")).is_none(), "a bundle missing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relative_paths() {
        assert_eq!(relative("C:\\X\\Game\\a.fbrb", "c:/x"), "game/a.fbrb");
        assert_eq!(relative("C:\\Y\\a.fbrb", "c:/x"), "c:/y/a.fbrb");
        assert_eq!(relative("C:\\XY\\a.fbrb", "c:/x"), "c:/xy/a.fbrb");
    }
}
