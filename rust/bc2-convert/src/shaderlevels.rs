//! The game copy's level shader databases read as a whole (round OR3b): the level cover index and the level scan,
//! for `shaderdb.level_cover_prefixes` / `level_coverage` / `donor_levels` and `shaderdb.level_scan` (through
//! `bc2_native.shadercover_levels` and `shaderdb_level_scan`).
//!
//! A level's database is found as `shaderdb.load_levels` finds it ([`level_database`]): the bundle's databases
//! (entries whose type ends in `ShaderDatabase`), the one a Dx means (`pick_database`), read through the bundle
//! cache and parsed as `ShaderDb.parse(data, 10 if dx == 11 else dx, where=f'{bundle}:{name}')` - with the same
//! messages for a level without one, with several, or with one this reader does not read.
//!
//! **The cover index** ([`levels`]): per level bundle and Dx, the entry-name prefixes of its database
//! ([`bc2_formats::shaderdb::levels::cover_prefixes`]) or its error, kept in the suite cache `shadercover` (one
//! `<24 hex>.bin` per bundle and Dx, named by the bundle's path and the Dx) and **keyed by content**: [`VERSION`],
//! the Dx and the bundle's content key ([`crate::dbxcatalogue::cache::bundle_key`]: its length, its head and its
//! gzip trailer - the CRC-32 of the whole payload; the Python index hashed every byte of every bundle). A file
//! that is damaged, of another version or of other content is not read; it is written again.
//!
//! **The level scan** ([`scan_levels`]): each level database's entries as
//! [`bc2_formats::shaderdb::levels::scan`] gives them and its mesh parts, levels read in parallel.

use std::fs;
use std::io;
use std::path::Path;

use bc2_formats::fbrb::FbrbError;
use bc2_formats::shaderdb::levels::{EntryScan, Malformed, cover_prefixes, scan};
use bc2_formats::shaderdb::open::{open, pick_database};
use bc2_gamefs::budget;
use bc2_gamefs::sha256::{self, Sha256};
use rayon::prelude::*;

/// The index's layout, rules and key: a file of another is not read (3: the Rust index, round `OR3b`; 2 was the
/// Python's pickle by content, 1 by size and time).
pub const VERSION: u32 = 3;
/// The index files' first bytes.
const MAGIC: &[u8; 8] = b"BC2SHCOV";

/// A bundle that cannot be read: the error, the bundle and the entry names asked for (`fbrb.read` names them).
#[derive(Debug)]
pub struct ReadFailure {
    /// What went wrong.
    pub error: FbrbError,
    /// The bundle.
    pub path: String,
    /// The entry names asked for, as given.
    pub keys: Vec<String>,
}

/// A level's database as `load_levels` gives it for one Dx: the file's bytes and label, or the `ShaderDbError`'s
/// message.
#[derive(Debug)]
pub enum Loaded {
    /// The database's bytes and its label (`<bundle>:<entry>`).
    Db {
        /// The bytes.
        data: Vec<u8>,
        /// `<bundle>:<entry>`.
        label: String,
    },
    /// The `ShaderDbError`'s message.
    Error(String),
}

/// `load_levels`' message for a level the game copy does not have.
#[must_use]
pub fn unknown_level(level: &str) -> String {
    format!("unknown level {level}: the game copy has no levels\\{level}\\level-00.fbrb")
}

/// `os.path.basename(os.path.dirname(path))`.
fn folder_of(path: &str) -> String {
    let p = path.trim_end_matches(['/', '\\']);
    let dir = p.rfind(['/', '\\']).map_or("", |i| p.get(..i).unwrap_or_default());
    let dir = dir.trim_end_matches(['/', '\\']);
    dir.rfind(['/', '\\'])
        .map_or(dir, |i| dir.get(i + 1..).unwrap_or_default())
        .to_owned()
}

/// `_pick_python` for names `pick_database` does not lower-case (a character past latin-1: Python's `str.lower`,
/// which `to_lowercase` follows for them).
fn pick_unicode(path: &str, dbs: &[(String, String)], dx: u32, folder: &str) -> Result<String, String> {
    let want = format!("dx{dx}shaderdatabase");
    let names: Vec<&str> = dbs
        .iter()
        .filter(|(_, t)| t.to_lowercase() == want)
        .map(|(n, _)| n.as_str())
        .collect();
    if let [one] = names.as_slice() {
        return Ok((*one).to_owned());
    }
    let own = format!("levels/{}/shaders/", folder.to_lowercase());
    let hit: Vec<&str> = names
        .iter()
        .copied()
        .filter(|n| n.to_lowercase().starts_with(&own))
        .collect();
    match hit.as_slice() {
        [one] => Ok((*one).to_owned()),
        _ => Err(format!(
            "{path} has {} Dx{dx} shader databases; name one (<bundle>:<entry>): {}",
            names.len(),
            names.join(", ")
        )),
    }
}

/// The Dx`dx` database of a level bundle (module text).
///
/// # Errors
/// The bundle cannot be read ([`ReadFailure`], as `fbrb.entries` / `fbrb.read_many` raise it).
pub fn level_database(bundle: &str, dx: u32) -> Result<Loaded, ReadFailure> {
    let fail = |error, keys: Vec<String>| ReadFailure {
        error,
        path: bundle.to_owned(),
        keys,
    };
    let head = bc2_gamefs::bundles::entries_path(bundle).map_err(|e| fail(e, Vec::new()))?;
    let dbs: Vec<(String, String)> = head
        .entries
        .iter()
        .filter(|e| e.type_name.ends_with("ShaderDatabase"))
        .map(|e| (e.name.clone(), e.type_name.clone()))
        .collect();
    let folder = folder_of(bundle);
    let picked =
        pick_database(bundle, &dbs, dx, None, &folder).unwrap_or_else(|| pick_unicode(bundle, &dbs, dx, &folder));
    let name = match picked {
        Ok(n) => n,
        Err(msg) => return Ok(Loaded::Error(msg)),
    };
    let mut data =
        bc2_gamefs::bundles::read_path(bundle, &[name.to_lowercase()]).map_err(|e| fail(e, vec![name.clone()]))?;
    let data = if data.is_empty() {
        Vec::new()
    } else {
        data.swap_remove(0)
    };
    Ok(Loaded::Db {
        data,
        label: format!("{bundle}:{name}"),
    })
}

/// The kind `ShaderDb.parse` is asked for on a level's Dx (Dx11 files have Dx10's layout).
const fn parse_dx(dx: u32) -> u32 {
    if dx == 11 { 10 } else { dx }
}

// ------------------------------------------------------------------------------------------------ the cover index

/// A level's cover: its database's prefixes (in the order first met) or the error it gives.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cover {
    /// `cover_prefixes` of the database.
    Prefixes(Vec<(Vec<u8>, u32)>),
    /// The `ShaderDbError`'s message.
    Error(String),
}

/// One level's answer.
#[derive(Debug)]
pub enum Row {
    /// From the index.
    Hit(Cover),
    /// Loaded (and indexed, but for an unknown level).
    Loaded(Cover),
    /// The bundle cannot be read.
    Failed(ReadFailure),
}

/// The index file's name for a bundle and Dx (by the bundle's place).
#[must_use]
pub fn file_name(bundle: &str, dx: u32) -> String {
    let abs = std::path::absolute(bundle).map_or_else(|_| bundle.to_owned(), |a| a.to_string_lossy().into_owned());
    let mut s = Sha256::default();
    s.update(format!("bc2 shadercover v{VERSION}\0{}\0{dx}", budget::normcase(&abs)).as_bytes());
    let mut h = s.hexdigest();
    h.truncate(24);
    h + ".bin"
}

/// The index entry's key for a bundle and Dx (by the bundle's content).
///
/// # Errors
/// The bundle cannot be read.
pub fn key(bundle: &str, dx: u32) -> io::Result<String> {
    let k = crate::dbxcatalogue::cache::bundle_key(bundle)?;
    let mut s = Sha256::default();
    s.update(format!("bc2 shadercover v{VERSION}\0{k}\0{dx}").as_bytes());
    Ok(s.hexdigest())
}

fn put_u32(out: &mut Vec<u8>, v: usize) {
    out.extend_from_slice(&u32::try_from(v).unwrap_or(u32::MAX).to_le_bytes());
}

/// The index file's bytes for a cover under `key`.
#[must_use]
pub fn file_bytes(key: &str, dx: u32, cover: &Cover) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&dx.to_le_bytes());
    put_u32(&mut out, key.len());
    out.extend_from_slice(key.as_bytes());
    match cover {
        Cover::Prefixes(p) => {
            out.push(0);
            put_u32(&mut out, p.len());
            for (k, n) in p {
                put_u32(&mut out, k.len());
                out.extend_from_slice(k);
                out.extend_from_slice(&n.to_le_bytes());
            }
        }
        Cover::Error(m) => {
            out.push(1);
            put_u32(&mut out, m.len());
            out.extend_from_slice(m.as_bytes());
        }
    }
    let sum = sha256::hex(&out);
    out.extend_from_slice(sum.as_bytes());
    out
}

/// A reader over an index file's body.
struct Rd<'a>(&'a [u8]);

impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let (a, b) = self.0.split_at_checked(n)?;
        self.0 = b;
        Some(a)
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(*self.take(4)?.first_chunk::<4>()?))
    }

    fn len(&mut self) -> Option<usize> {
        self.u32().map(|v| v as usize)
    }
}

/// The cover an index file holds for `key` and `dx`, or `None` (damaged, another version, Dx or key).
#[must_use]
pub fn read_file(data: &[u8], key: &str, dx: u32) -> Option<Cover> {
    let (body, sum) = data.split_at_checked(data.len().checked_sub(64)?)?;
    if sha256::hex(body).as_bytes() != sum {
        return None;
    }
    let mut r = Rd(body);
    if r.take(8)? != MAGIC || r.u32()? != VERSION || r.u32()? != dx {
        return None;
    }
    let n = r.len()?;
    if r.take(n)? != key.as_bytes() {
        return None;
    }
    let cover = match r.take(1)?.first()? {
        0 => {
            let n = r.len()?;
            let mut p = Vec::with_capacity(n.min(1 << 20));
            for _ in 0..n {
                let len = r.len()?;
                let k = r.take(len)?.to_vec();
                p.push((k, r.u32()?));
            }
            Cover::Prefixes(p)
        }
        1 => {
            let len = r.len()?;
            Cover::Error(String::from_utf8(r.take(len)?.to_vec()).ok()?)
        }
        _ => return None,
    };
    r.0.is_empty().then_some(cover)
}

/// The cover of a level bundle read from it (module text).
///
/// # Errors
/// The bundle cannot be read.
pub fn load_cover(bundle: &str, dx: u32) -> Result<Cover, ReadFailure> {
    Ok(match level_database(bundle, dx)? {
        Loaded::Error(m) => Cover::Error(m),
        Loaded::Db { data, label } => match open(&data, Some(parse_dx(dx)), None, &label) {
            Ok(o) => Cover::Prefixes(cover_prefixes(o.db.entries.iter().map(|e| e.name.as_ref()))),
            Err(m) => Cover::Error(m),
        },
    })
}

fn cached(folder: &str, bundle: &str, dx: u32) -> Option<Cover> {
    let k = key(bundle, dx).ok()?;
    let p = budget::join(folder, &[&file_name(bundle, dx)]);
    let data = fs::read(&p).ok()?;
    let got = read_file(&data, &k, dx)?;
    let _ = budget::utime_now(&p);
    Some(got)
}

fn save(folder: &str, bundle: &str, dx: u32, cover: &Cover) {
    let Ok(k) = key(bundle, dx) else { return };
    if fs::create_dir_all(folder).is_err() {
        return;
    }
    let p = budget::join(folder, &[&file_name(bundle, dx)]);
    // only a cache: a write that fails leaves the next run to read the bundle again
    let _ = bc2_gamefs::atomic::write(Path::new(&p), &[&file_bytes(&k, dx, cover)]);
}

/// The cover of each level, in order (`level_cover_prefixes`): `(level, its bundle or None)`; from the index in
/// `folder` where it holds the bundle's content, else read and indexed (an unknown level is not indexed). `None`:
/// no index.
#[must_use]
pub fn levels(jobs: &[(String, Option<String>)], dx: u32, folder: Option<&str>) -> Vec<Row> {
    let rows: Vec<Row> = jobs
        .par_iter()
        .map(|(lv, bundle)| {
            let Some(b) = bundle else {
                return Row::Loaded(Cover::Error(unknown_level(lv)));
            };
            if let Some(hit) = folder.and_then(|f| cached(f, b, dx)) {
                return Row::Hit(hit);
            }
            match load_cover(b, dx) {
                Ok(c) => {
                    if let Some(f) = folder {
                        save(f, b, dx, &c);
                    }
                    Row::Loaded(c)
                }
                Err(e) => Row::Failed(e),
            }
        })
        .collect();
    if let Some(f) = folder
        && let Some(s) = budget::spec("shadercover")
    {
        let _ = budget::maybe_prune(s, f, None);
    }
    rows
}

// ------------------------------------------------------------------------------------------------ the level scan

/// One entry of a level's scan, owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The name (latin-1).
    pub name: Vec<u8>,
    /// Constant names, sorted.
    pub constants: Vec<Vec<u8>>,
    /// Samplers bound.
    pub samplers: usize,
    /// (sampler, cube, name), sorted.
    pub textures: Vec<(u8, u8, Vec<u8>)>,
    /// Vertex layout GUIDs, sorted.
    pub layouts: Vec<Vec<u8>>,
    /// Main-pass skinning kinds, sorted.
    pub kinds: Vec<u8>,
    /// The layout record bodies its layouts name, sorted (round `OR3c`).
    pub layout_records: Vec<Vec<u8>>,
}

impl From<EntryScan<'_>> for Entry {
    fn from(e: EntryScan<'_>) -> Self {
        Self {
            name: e.name.to_vec(),
            constants: e.constants.into_iter().map(<[u8]>::to_vec).collect(),
            samplers: e.samplers,
            textures: e.textures.into_iter().map(|(s, c, n)| (s, c, n.to_vec())).collect(),
            layouts: e.layouts.into_iter().map(<[u8]>::to_vec).collect(),
            kinds: e.kinds,
            layout_records: e.layout_records.into_iter().map(<[u8]>::to_vec).collect(),
        }
    }
}

/// A level's scan: its entries in file order and its mesh parts `(name, layout GUID)`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelScan {
    /// The entries.
    pub entries: Vec<Entry>,
    /// The mesh parts.
    pub parts: Vec<(Vec<u8>, Vec<u8>)>,
}

/// One level's scan answer.
#[derive(Debug)]
pub enum ScanRow {
    /// The scan.
    Scan(LevelScan),
    /// The `ShaderDbError`'s message (`load` raised it).
    Error(String),
    /// A database the queries cannot answer on (the label).
    Malformed(String),
    /// The bundle cannot be read.
    Failed(ReadFailure),
}

/// The scan of each level's Dx`dx` database, in order (`(level, its bundle or None)`), read in parallel.
#[must_use]
pub fn scan_levels(jobs: &[(String, Option<String>)], dx: u32) -> Vec<ScanRow> {
    jobs.par_iter()
        .map(|(lv, bundle)| {
            let Some(b) = bundle else {
                return ScanRow::Error(unknown_level(lv));
            };
            match level_database(b, dx) {
                Err(e) => ScanRow::Failed(e),
                Ok(Loaded::Error(m)) => ScanRow::Error(m),
                Ok(Loaded::Db { data, label }) => match open(&data, Some(parse_dx(dx)), None, &label) {
                    Err(m) => ScanRow::Error(m),
                    Ok(o) => match scan(&o.db) {
                        Err(Malformed) => ScanRow::Malformed(label),
                        Ok(entries) => ScanRow::Scan(LevelScan {
                            entries: entries.into_iter().map(Entry::from).collect(),
                            parts: o.db.parts.iter().map(|p| (p.name.to_vec(), p.guid.to_vec())).collect(),
                        }),
                    },
                },
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_round_trip_and_damage() {
        let c = Cover::Prefixes(vec![(b"objects".to_vec(), 3), (b"objects/\xe9".to_vec(), 1)]);
        let b = file_bytes("k1", 9, &c);
        assert_eq!(read_file(&b, "k1", 9), Some(c.clone()));
        assert_eq!(read_file(&b, "k2", 9), None);
        assert_eq!(read_file(&b, "k1", 10), None);
        let mut d = b.clone();
        if let Some(x) = d.get_mut(20) {
            *x ^= 1;
        }
        assert_eq!(read_file(&d, "k1", 9), None);
        assert_eq!(read_file(b.get(..b.len() - 1).unwrap_or_default(), "k1", 9), None);
        let e = Cover::Error("levels\\x: no".to_owned());
        assert_eq!(read_file(&file_bytes("k", 11, &e), "k", 11), Some(e));
    }

    #[test]
    fn folder_of_is_the_bundles_folder() {
        assert_eq!(folder_of(r"C:\g\levels\mp_005\level-00.fbrb"), "mp_005");
        assert_eq!(folder_of("a/b/level-00.fbrb"), "b");
        assert_eq!(folder_of("level-00.fbrb"), "");
    }

    #[test]
    fn names_by_place_keys_by_content() {
        let dir = std::env::temp_dir().join(format!("shaderlevels_{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let a = dir.join("a.fbrb");
        let b = dir.join("b.fbrb");
        let _ = fs::write(&a, b"not a bundle, hashed whole");
        let _ = fs::write(&b, b"not a bundle, hashed whole");
        let (a, b) = (a.to_string_lossy().into_owned(), b.to_string_lossy().into_owned());
        assert_ne!(file_name(&a, 9), file_name(&b, 9));
        assert_ne!(file_name(&a, 9), file_name(&a, 10));
        assert_eq!(key(&a, 9).ok(), key(&b, 9).ok());
        let _ = fs::write(&b, b"not a bundle, other bytes");
        assert_ne!(key(&a, 9).ok(), key(&b, 9).ok());
        let _ = fs::remove_dir_all(&dir);
    }
}
