//! `meshview.Index`'s tables (`bfbc2-mesh-viewer\meshview.py`, rounds O3 and PR1b-2-rest): which bundle holds each
//! entry (the first stock bundle that lists it), and every Dx9 shader database's facts the mesh tools read - mesh part
//! name -> vertex layout record (after its GUID), database entry name -> the texture names its programs' parameter
//! blocks name - and the index's cache ([`load`], `work\tools_out\meshview_index.bin`, this module's own format since
//! round PR1b-2-rest: the Python's pickle retired).
//!
//! The bundles are given in `meshview._bundles`' order. A bundle whose header cannot be read, or a database that
//! cannot be picked by its name, read or parsed (or whose records point past their sections), is left out of the index
//! and named in [`MeshIndex::skipped`] (the retired Python reference skipped the first two kinds and raised on the
//! last; no DICE database has any of them): an index with something skipped is used but never cached, so a bundle
//! locked for a moment is not left out of every later run.

use std::collections::{HashMap, HashSet};

use bc2_formats::fbrb;
use bc2_formats::shaderdb::{self, query};

/// The index: each table in the reference's dict order (first insertion).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct MeshIndex {
    /// (entry name lower case, index of its bundle in the list given, entry name as the bundle holds it)
    pub where_: Vec<(String, usize, String)>,
    /// (mesh part name lower case, index into [`MeshIndex::layouts`]): parts sharing a layout record share the
    /// object in the reference (the same `bytes` from its `layouts` dict)
    pub parts: Vec<(String, usize)>,
    /// the vertex layout records (each one object in the reference)
    pub layouts: Vec<Vec<u8>>,
    /// (database entry name lower case, its texture names)
    pub entries: Vec<(String, Vec<String>)>,
    /// the shader databases read from the bundles (each an `fbrb.read`: the reference's caches' budget asks then)
    pub databases: usize,
    /// what was left out, in words (`<bundle>` or `<bundle>:<database>` and why), in the bundles' order
    pub skipped: Vec<String>,
}

/// Latin-1 bytes as a `str`.
fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// `str.lower()` of a latin-1 name.
fn lower(b: &[u8]) -> String {
    latin1(&query::lower(b))
}

/// `re.findall(rb'[A-Za-z0-9_/\.\- ]{6,}', blk)`: the runs of those bytes six or more long.
fn texture_runs(blk: &[u8]) -> Vec<String> {
    let ok = |c: u8| c.is_ascii_alphanumeric() || matches!(c, b'_' | b'/' | b'.' | b'-' | b' ');
    let mut out = Vec::new();
    let mut start = None;
    for (i, &c) in blk.iter().chain(std::iter::once(&0u8)).enumerate() {
        match (ok(c) && i < blk.len(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                if i - s >= 6 {
                    out.push(latin1(blk.get(s..i).unwrap_or_default()));
                }
                start = None;
            }
            _ => {}
        }
    }
    out
}

/// `shaderdb.pick_database(bundle, 9, name)`: the name must match exactly one Dx9 database of the bundle (by name, or
/// as a folder above it).
fn picks_one(dbs: &[&str], name: &str) -> bool {
    let want = name.to_lowercase();
    let folder = format!("{}/", want.trim_end_matches('/'));
    dbs.iter()
        .filter(|n| {
            let l = n.to_lowercase();
            l == want || l.starts_with(&folder)
        })
        .count()
        == 1
}

/// One database's part of the index, its names not yet checked against the bundles before it.
#[derive(Debug, Default)]
struct DbPart {
    /// the layout records in order (each one object)
    layouts: Vec<Vec<u8>>,
    /// (part name lower case, index into `layouts`), first of each name in the database
    parts: Vec<(String, usize)>,
    /// (entry name lower case, its texture names), first of each name
    entries: Vec<(String, Vec<String>)>,
}

/// One bundle's part: its live entries in order, then each Dx9 database's part (stopping at the first error, as the
/// reference's build stops there).
#[derive(Debug, Default)]
struct BundlePart {
    names: Vec<String>,
    databases: Vec<Result<DbPart, String>>,
}

/// A bundle's part of the index: its header, every Dx9 database read with one read of the bundle, each parsed.
fn bundle_part(b: &str) -> Result<BundlePart, String> {
    let h = fbrb::entries_path(b).map_err(|e| format!("{b}: {e}"))?;
    let live: Vec<&fbrb::Entry> = h.entries.iter().filter(|e| !e.is_deleted()).collect();
    let mut part = BundlePart {
        names: live.iter().map(|e| e.name.clone()).collect(),
        databases: Vec::new(),
    };
    // the databases picked by name (`shaderdb.pick_database`'s list) and those read (the reference's two filters)
    let dx9: Vec<&str> = live
        .iter()
        .filter(|e| e.type_name.ends_with("ShaderDatabase") && e.type_name.to_lowercase() == "dx9shaderdatabase")
        .map(|e| e.name.as_str())
        .collect();
    let names: Vec<&str> = live
        .iter()
        .filter(|e| e.type_name.to_lowercase() == "dx9shaderdatabase")
        .map(|e| e.name.as_str())
        .collect();
    // every database by its name; a name that does not pick exactly one is left out (said), the rest are read
    let mut wanted: Vec<&str> = Vec::new();
    let mut order: Vec<Result<usize, String>> = Vec::new();
    for n in &names {
        if picks_one(&dx9, n) {
            order.push(Ok(wanted.len()));
            wanted.push(n);
        } else {
            order.push(Err(format!("{b}:{n} (not one database by that name)")));
        }
    }
    let datas = if wanted.is_empty() {
        Vec::new()
    } else {
        match crate::bundles::read_path(b, &wanted) {
            Ok(d) => d,
            Err(e) => {
                part.databases.extend(
                    wanted
                        .iter()
                        .map(|n| Err(format!("{b}:{n} (the bundle's databases could not be read: {e})"))),
                );
                return Ok(part);
            }
        }
    };
    for o in order {
        let at = match o {
            Ok(at) => at,
            Err(e) => {
                part.databases.push(Err(e));
                continue;
            }
        };
        let (Some(name), Some(data)) = (wanted.get(at), datas.get(at)) else {
            continue;
        };
        let db = if data.len() < 9 {
            Err(format!("{b}:{name} (too short)"))
        } else {
            let be = shaderdb::is_big_endian(data);
            shaderdb::parse(data, 9, be)
                .map_err(|e| format!("{b}:{name} ({e})"))
                .and_then(|db| db_part(&db, be).map_err(|e| format!("{b}:{name} ({e})")))
        };
        part.databases.push(db);
    }
    Ok(part)
}

/// The index of the bundles, in order; what cannot be read is left out and named in [`MeshIndex::skipped`].
///
/// Each bundle is read on its own worker (one read of the bundle for all its databases; audit AU5 ALG-6: one reopen
/// per database, one bundle at a time, before), and the parts are merged in the bundles' order with the reference's
/// first-wins rules, so the index is the one the serial walk made.
#[must_use]
pub fn build(bundles: &[String]) -> MeshIndex {
    use rayon::prelude::*;
    let parts: Vec<Result<BundlePart, String>> = bundles.par_iter().map(|b| bundle_part(b)).collect();
    let mut ix = MeshIndex::default();
    let mut seen: HashSet<String> = HashSet::new();
    let mut parts_seen: HashSet<String> = HashSet::new();
    let mut entries_seen: HashSet<String> = HashSet::new();
    for (bi, part) in parts.into_iter().enumerate() {
        let part = match part {
            Ok(p) => p,
            Err(e) => {
                ix.skipped.push(e);
                continue;
            }
        };
        for name in part.names {
            let key = name.to_lowercase();
            if seen.insert(key.clone()) {
                ix.where_.push((key, bi, name));
            }
        }
        for db in part.databases {
            let db = match db {
                Ok(db) => db,
                Err(e) => {
                    ix.skipped.push(e);
                    continue;
                }
            };
            ix.databases += 1;
            let base = ix.layouts.len();
            ix.layouts.extend(db.layouts);
            for (key, id) in db.parts {
                if parts_seen.insert(key.clone()) {
                    ix.parts.push((key, base + id));
                }
            }
            for (key, tex) in db.entries {
                if entries_seen.insert(key.clone()) {
                    ix.entries.push((key, tex));
                }
            }
        }
    }
    ix
}

/// `Index._add_database(db)` for one database alone (its names checked against the earlier ones when merged).
fn db_part(db: &shaderdb::ShaderDb<'_>, be: bool) -> Result<DbPart, &'static str> {
    let mut out = DbPart::default();
    // {r[:16]: r[16:]}: the last record of a GUID wins; each record one object
    let mut layouts: HashMap<&[u8], usize> = HashMap::new();
    for r in &db.layouts {
        let (g, rest) = r.split_at(r.len().min(16));
        let id = out.layouts.len();
        out.layouts.push(rest.to_vec());
        layouts.insert(g, id);
    }
    let mut parts_seen: HashSet<String> = HashSet::new();
    for p in &db.parts {
        if let Some(&id) = layouts.get(p.guid) {
            let key = lower(&p.name);
            if parts_seen.insert(key.clone()) {
                out.parts.push((key, id));
            }
        }
    }
    let groups: HashMap<&[u8], &Vec<u16>> = db.groups.iter().map(|(g, idx)| (*g, idx)).collect();
    let mut entries_seen: HashSet<String> = HashSet::new();
    for e in &db.entries {
        let key = lower(&e.name);
        if !entries_seen.insert(key.clone()) {
            continue;
        }
        let mut tex: Vec<String> = Vec::new();
        for &pi in groups.get(e.guid).map_or(&[][..], |v| v.as_slice()) {
            let prog = db
                .programs
                .get(usize::from(pi))
                .ok_or("a program index past its section")?;
            let [_, _, a, c] = query::program_refs(prog, be).ok_or("a program record too short")?;
            for blk in [a, c] {
                let blk = db
                    .params
                    .get(blk as usize)
                    .ok_or("a parameter block index past its section")?;
                for s in texture_runs(blk) {
                    if s.contains('/') && !tex.contains(&s) {
                        tex.push(s);
                    }
                }
            }
        }
        out.entries.push((key, tex));
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------- the cache

/// The cache file's name in the OPFOR folder's `work\tools_out` (the budget's `meshview` cache).
pub const CACHE_NAME: &str = "meshview_index.bin";
/// The cache format's first bytes.
const MAGIC: &[u8; 10] = b"BC2MESHIX\0";
/// The cache format's version (the pickle's `CACHE_VERSION` was 3).
pub const CACHE_VERSION: u32 = 4;

/// A bundle's signature: (path, size, modification time in nanoseconds since 1970: `os.stat`'s `st_mtime_ns`).
pub type Sig = (String, u64, i128);

/// `os.stat(path).st_mtime_ns` as Python gives it on Windows (C's truncation toward zero before 1970).
#[must_use]
pub fn mtime_ns(m: &std::fs::Metadata) -> i128 {
    let t = m.modified().unwrap_or(std::time::UNIX_EPOCH);
    match t.duration_since(std::time::UNIX_EPOCH) {
        Ok(d) => i128::from(d.as_secs()) * 1_000_000_000 + i128::from(d.subsec_nanos()),
        Err(e) => {
            let d = e.duration();
            -(i128::from(d.as_secs()) * 1_000_000_000 + i128::from(d.subsec_nanos()))
        }
    }
}

/// The bundles' signature (the full modification time: whole seconds missed a same-size rewrite within the second).
///
/// # Errors
/// A bundle cannot be stat'ed.
pub fn signature(bundles: &[String]) -> std::io::Result<Vec<Sig>> {
    bundles
        .iter()
        .map(|b| {
            let m = std::fs::metadata(b)?;
            Ok((b.clone(), m.len(), mtime_ns(&m)))
        })
        .collect()
}

fn put_len(out: &mut Vec<u8>, n: usize) {
    out.extend_from_slice(&u32::try_from(n).unwrap_or(u32::MAX).to_le_bytes());
}

fn put_bytes(out: &mut Vec<u8>, b: &[u8]) {
    put_len(out, b.len());
    out.extend_from_slice(b);
}

/// The cache's bytes: the magic, the version, the signature, the four tables, the databases read, then the SHA-1 of
/// everything before it (a damaged file is rebuilt, never half read).
#[must_use]
pub fn cache_bytes(ix: &MeshIndex, sig: &[Sig]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 << 20);
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&CACHE_VERSION.to_le_bytes());
    put_len(&mut out, sig.len());
    for (path, size, ns) in sig {
        put_bytes(&mut out, path.as_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&ns.to_le_bytes());
    }
    put_len(&mut out, ix.where_.len());
    for (key, bi, name) in &ix.where_ {
        put_bytes(&mut out, key.as_bytes());
        put_len(&mut out, *bi);
        put_bytes(&mut out, name.as_bytes());
    }
    put_len(&mut out, ix.layouts.len());
    for l in &ix.layouts {
        put_bytes(&mut out, l);
    }
    put_len(&mut out, ix.parts.len());
    for (key, id) in &ix.parts {
        put_bytes(&mut out, key.as_bytes());
        put_len(&mut out, *id);
    }
    put_len(&mut out, ix.entries.len());
    for (key, tex) in &ix.entries {
        put_bytes(&mut out, key.as_bytes());
        put_len(&mut out, tex.len());
        for t in tex {
            put_bytes(&mut out, t.as_bytes());
        }
    }
    out.extend_from_slice(&(ix.databases as u64).to_le_bytes());
    let digest = sha1_smol::Sha1::from(&out).digest().bytes();
    out.extend_from_slice(&digest);
    out
}

/// A cursor over the cache's bytes.
struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.d.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }

    fn array<const N: usize>(&mut self) -> Option<[u8; N]> {
        self.take(N)?.try_into().ok()
    }

    fn len(&mut self) -> Option<usize> {
        usize::try_from(u32::from_le_bytes(self.array()?)).ok()
    }

    fn bytes(&mut self) -> Option<&'a [u8]> {
        let n = self.len()?;
        self.take(n)
    }

    fn string(&mut self) -> Option<String> {
        std::str::from_utf8(self.bytes()?).ok().map(str::to_owned)
    }

    /// A count of items each at least `min` bytes long (a damaged count never allocates past the file).
    fn count(&mut self, min: usize) -> Option<usize> {
        let n = self.len()?;
        (n.checked_mul(min)? <= self.d.len().saturating_sub(self.at)).then_some(n)
    }
}

/// The index in a cache's bytes when they are whole and this game copy's (`sig`); `None` when old, damaged or another
/// copy's.
#[must_use]
pub fn from_cache_bytes(data: &[u8], sig: &[Sig]) -> Option<MeshIndex> {
    let (body, digest) = data.split_at_checked(data.len().checked_sub(20)?)?;
    if sha1_smol::Sha1::from(body).digest().bytes() != digest {
        return None;
    }
    let mut r = Reader { d: body, at: 0 };
    if r.take(MAGIC.len())? != MAGIC || u32::from_le_bytes(r.array()?) != CACHE_VERSION {
        return None;
    }
    let n = r.count(28)?;
    if n != sig.len() {
        return None;
    }
    for (path, size, ns) in sig {
        let same = r.bytes()? == path.as_bytes()
            && u64::from_le_bytes(r.array()?) == *size
            && i128::from_le_bytes(r.array()?) == *ns;
        if !same {
            return None;
        }
    }
    let mut ix = MeshIndex::default();
    let n = r.count(12)?;
    ix.where_.reserve(n);
    for _ in 0..n {
        let key = r.string()?;
        let bi = r.len()?;
        if bi >= sig.len() {
            return None;
        }
        ix.where_.push((key, bi, r.string()?));
    }
    let n = r.count(4)?;
    for _ in 0..n {
        ix.layouts.push(r.bytes()?.to_vec());
    }
    let n = r.count(8)?;
    for _ in 0..n {
        let key = r.string()?;
        let id = r.len()?;
        if id >= ix.layouts.len() {
            return None;
        }
        ix.parts.push((key, id));
    }
    let n = r.count(8)?;
    for _ in 0..n {
        let key = r.string()?;
        let k = r.count(4)?;
        let tex = (0..k).map(|_| r.string()).collect::<Option<Vec<_>>>()?;
        ix.entries.push((key, tex));
    }
    ix.databases = usize::try_from(u64::from_le_bytes(r.array()?)).ok()?;
    (r.at == body.len()).then_some(ix)
}

/// Why the index could not be had.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// A bundle of the game copy could not be stat'ed (its signature).
    #[error("{0}: a bundle of the game copy could not be read ({1})")]
    Stat(String, std::io::Error),
    /// The cache's lock could not be taken.
    #[error("{0}.lock: {1}")]
    Lock(String, std::io::Error),
}

/// An index had: from the cache, or built.
#[derive(Debug)]
pub struct Loaded {
    /// The index.
    pub index: MeshIndex,
    /// It was built now (not read from the cache).
    pub built: bool,
}

/// The index of `bundles` (`meshview._bundles`' list): from the cache at `cache` when it is this game copy's, else
/// built ([`build`], under the cache's lock: one process builds it, one that waited reads what the other wrote) and
/// written there, unless something was skipped. `say` hears "indexing the game copy" before a build, each bundle or
/// database left out, and a cache that could not be written.
///
/// # Errors
/// A bundle cannot be stat'ed, or the lock cannot be taken.
pub fn load(bundles: &[String], cache: &std::path::Path, say: &mut dyn FnMut(&str)) -> Result<Loaded, LoadError> {
    let sig = signature(bundles).map_err(|e| {
        let bad = bundles
            .iter()
            .find(|b| std::fs::metadata(b).is_err())
            .cloned()
            .unwrap_or_default();
        LoadError::Stat(bad, e)
    })?;
    let read = || std::fs::read(cache).ok().and_then(|d| from_cache_bytes(&d, &sig));
    if let Some(index) = read() {
        return Ok(Loaded { index, built: false });
    }
    let lock = std::path::PathBuf::from(format!("{}.lock", cache.display()));
    crate::atomic::locked(&lock, || {
        if let Some(index) = read() {
            return Loaded { index, built: false };
        }
        say("indexing the game copy (once; cached) ...");
        let index = build(bundles);
        if index.skipped.is_empty() {
            if let Err(e) = crate::atomic::write(cache, &[&cache_bytes(&index, &sig)]) {
                say(&format!("index: built but not cached this time ({e})"));
            }
        } else {
            // an incomplete index is not cached (suite deep scan 2026-11-15, C3): said, and built again next time
            say(&format!(
                "index: {} unreadable, left out of this run's index and not cached:\n  {}",
                index.skipped.len(),
                index.skipped.join("\n  ")
            ));
        }
        Loaded { index, built: true }
    })
    .map_err(|e| LoadError::Lock(cache.display().to_string(), e))
}

#[cfg(test)]
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn the_cache_round_trips_and_refuses_damage() {
        let ix = MeshIndex {
            where_: vec![("a.dbx".into(), 0, "A.dbx".into()), ("b.res".into(), 1, "B.res".into())],
            parts: vec![("m_lod0/p/standard".into(), 1), ("m_lod0/q/standard".into(), 1)],
            layouts: vec![vec![], vec![1, 2, 3]],
            entries: vec![
                ("m/s".into(), vec!["t/x_d".into(), "t/x_n".into()]),
                ("m/e".into(), vec![]),
            ],
            databases: 2,
            skipped: vec![],
        };
        let sig: Vec<Sig> = vec![
            ("x.fbrb".into(), 5, 1_700_000_000_123_456_789),
            ("y.fbrb".into(), 6, -5),
        ];
        let b = cache_bytes(&ix, &sig);
        assert_eq!(from_cache_bytes(&b, &sig), Some(ix));
        // another copy, a changed time, a flipped byte, a cut file: rebuilt
        assert_eq!(from_cache_bytes(&b, &sig[..1]), None);
        let mut other = sig.clone();
        other[1].2 += 1;
        assert_eq!(from_cache_bytes(&b, &other), None);
        for at in [0, 12, b.len() / 2, b.len() - 1] {
            let mut bad = b.clone();
            bad[at] ^= 1;
            assert_eq!(from_cache_bytes(&bad, &sig), None, "{at}");
        }
        assert_eq!(from_cache_bytes(&b[..b.len() - 1], &sig), None);
        assert_eq!(from_cache_bytes(&[], &sig), None);
    }

    #[test]
    fn runs_as_findall() {
        assert_eq!(
            texture_runs(b"\0abc/def\0ab/cd\0Textures/x_d.dds"),
            vec!["abc/def", "Textures/x_d.dds"]
        );
        assert_eq!(texture_runs(b"123456"), vec!["123456"]);
        assert_eq!(texture_runs(b"12345"), Vec::<String>::new());
    }

    #[test]
    fn database_picked_as_the_reference() {
        let dbs = ["levels/a/shaders/x", "levels/a/shaders/x/y"];
        assert!(!picks_one(&dbs, "levels/a/shaders/x"));
        assert!(picks_one(&dbs, "levels/a/shaders/x/y"));
    }
}
