//! The mesh-alias cache of [`crate::collision::index_cached`]: per base folder, each converted XML file's `Mesh`
//! reference as the index read it, with the size and modification time of the XML file and of the binary `.dbx` next
//! to it when the reference was read.
//!
//! A reference is reused only while both files' stamps are the ones stored; any other file is read again, a file no
//! longer there drops out, and a new one is read. The stamps come from the folder listing the index makes anyway, so
//! a level whose files have not changed is indexed without opening them (`rust/BENCHMARKS.md`, "The alias table").
//! The stamp is taken before the file is read, so a file changed while it was being read is read again next time.
//!
//! One file per base folder (its name a hash of the folder's path, the path itself inside), written whole to a
//! temporary file and renamed over the old one, so a reader never sees half a file. A file that is cut short, does not
//! end with the checksum of its contents, has another version or names another folder is ignored (everything is read
//! again and it is replaced); the cache never makes an index fail, and an error writing it is ignored.

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::walk::Stamp;

const MAGIC: &[u8; 8] = b"BC2MALC1";

/// One XML file's cached answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The XML file's stamp.
    pub xml: Stamp,
    /// The binary `.dbx` next to it (`None`: there was none).
    pub dbx: Option<Stamp>,
    /// Its `Mesh` reference, as `xmlinst.field_ref` gives it (`None`: it has none).
    pub mesh: Option<String>,
}

/// 64-bit FNV-1a: the file's checksum and its name's hash.
fn fnv(data: &[u8]) -> u64 {
    data.iter().fold(0xcbf2_9ce4_8422_2325, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The cache file of `base` in `dir`.
pub(crate) fn file_of(dir: &Path, base: &Path) -> PathBuf {
    let key = base.to_string_lossy().to_lowercase();
    dir.join(format!("aliases-{:016x}.bin", fnv(key.as_bytes())))
}

fn put_str(out: &mut Vec<u8>, s: &str) {
    out.extend(u32::try_from(s.len()).unwrap_or(u32::MAX).to_le_bytes());
    out.extend(s.as_bytes());
}

fn put_stamp(out: &mut Vec<u8>, s: Stamp) {
    out.extend(s.size.to_le_bytes());
    out.extend(s.modified.to_le_bytes());
}

/// The file's bytes for `base`'s `entries` (keyed by the XML file's full path).
fn encode(base: &Path, entries: &[(&str, &Entry)]) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    put_str(&mut out, &base.to_string_lossy());
    out.extend(u32::try_from(entries.len()).unwrap_or(u32::MAX).to_le_bytes());
    for (key, e) in entries {
        put_str(&mut out, key);
        put_stamp(&mut out, e.xml);
        match e.dbx {
            Some(s) => {
                out.push(1);
                put_stamp(&mut out, s);
            }
            None => out.push(0),
        }
        match &e.mesh {
            Some(m) => {
                out.push(1);
                put_str(&mut out, m);
            }
            None => out.push(0),
        }
    }
    let sum = fnv(&out);
    out.extend(sum.to_le_bytes());
    out
}

/// A reading position; every read fails past the end.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        if n > self.0.len() {
            return None;
        }
        let (head, rest) = self.0.split_at(n);
        self.0 = rest;
        Some(head)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn u128(&mut self) -> Option<u128> {
        Some(u128::from_le_bytes(self.take(16)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).ok()
    }

    fn stamp(&mut self) -> Option<Stamp> {
        Some(Stamp {
            size: self.u64()?,
            modified: self.u128()?,
        })
    }
}

/// `base`'s entries from a cache file's bytes, or `None` when they are not a whole, matching file.
fn decode(base: &Path, data: &[u8]) -> Option<HashMap<String, Entry>> {
    let (body, sum) = data.split_at_checked(data.len().checked_sub(8)?)?;
    if fnv(body).to_le_bytes() != sum || !body.starts_with(MAGIC) {
        return None;
    }
    let mut r = Reader(body.get(MAGIC.len()..)?);
    if r.string()? != base.to_string_lossy() {
        return None;
    }
    let n = r.u32()?;
    let mut out = HashMap::new();
    for _ in 0..n {
        let key = r.string()?;
        let xml = r.stamp()?;
        let dbx = match r.u8()? {
            0 => None,
            1 => Some(r.stamp()?),
            _ => return None,
        };
        let mesh = match r.u8()? {
            0 => None,
            1 => Some(r.string()?),
            _ => return None,
        };
        out.insert(key, Entry { xml, dbx, mesh });
    }
    r.0.is_empty().then_some(out)
}

/// The cached entries of `base` in `dir` (empty when there is no usable cache file).
pub(crate) fn load(dir: &Path, base: &Path) -> HashMap<String, Entry> {
    fs::read(file_of(dir, base))
        .ok()
        .and_then(|d| decode(base, &d))
        .unwrap_or_default()
}

/// Replaces `base`'s cache file in `dir` with `entries`: written to a temporary file, then renamed over it.
///
/// # Errors
///
/// The folder or the file could not be written (the temporary file is removed).
pub(crate) fn store(dir: &Path, base: &Path, entries: &[(&str, &Entry)]) -> io::Result<()> {
    fs::create_dir_all(dir)?;
    let target = file_of(dir, base);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let tmp = target.with_extension(format!("{}-{nanos}.tmp", std::process::id()));
    let written = fs::write(&tmp, encode(base, entries)).and_then(|()| bc2_formats::atomic::replace(&tmp, &target));
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn stamp(size: u64) -> Stamp {
        Stamp {
            size,
            modified: u128::from(size) << 70,
        }
    }

    #[test]
    fn round_trip_and_refusals() {
        let dir = std::env::temp_dir().join(format!("bc2_scene_alias_cache_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let base = Path::new(r"C:\levels\mp_005\dist");
        let a = Entry {
            xml: stamp(1),
            dbx: Some(stamp(2)),
            mesh: Some("Objects/A/A_Mesh/1".into()),
        };
        let b = Entry {
            xml: stamp(3),
            dbx: None,
            mesh: None,
        };
        assert!(load(&dir, base).is_empty());
        store(&dir, base, &[("x/a.xml", &a), ("x/b.xml", &b)]).unwrap();
        let back = load(&dir, base);
        assert_eq!(back.len(), 2);
        assert_eq!(back["x/a.xml"], a);
        assert_eq!(back["x/b.xml"], b);
        // another folder's name, any damage, a cut: nothing
        let other = Path::new(r"C:\levels\mp_006\dist");
        let data = encode(base, &[("x/a.xml", &a)]);
        assert!(decode(other, &data).is_none());
        for k in 0..data.len() {
            assert!(decode(base, &data[..k]).is_none(), "cut at {k}");
            let mut d = data.clone();
            d[k] ^= 0x40;
            assert!(decode(base, &d).is_none(), "byte {k} changed");
        }
        // a well-formed file with bad flags or trailing bytes (its checksum made to fit)
        for trailing in [true, false] {
            let mut body = data[..data.len() - 8].to_vec();
            if trailing {
                body.push(0);
            } else {
                let flag = body.len() - 1 - 4 - "Objects/A/A_Mesh/1".len();
                body[flag] = 7;
            }
            let sum = fnv(&body);
            body.extend(sum.to_le_bytes());
            assert!(decode(base, &body).is_none());
        }
        let mut body = MAGIC.to_vec();
        body.extend(b"junk");
        let sum = fnv(&body);
        body.extend(sum.to_le_bytes());
        assert!(decode(base, &body).is_none());
        // a damaged file on disk is ignored, and replaced by the next store
        fs::write(file_of(&dir, base), b"damaged").unwrap();
        assert!(load(&dir, base).is_empty());
        store(&dir, base, &[("x/b.xml", &b)]).unwrap();
        assert_eq!(load(&dir, base).len(), 1);
        // a folder that cannot be made: the error, no file left behind
        let blocked = dir.join("blocked");
        fs::write(&blocked, b"").unwrap();
        assert!(store(&blocked.join("sub"), base, &[]).is_err());
        fs::remove_dir_all(&dir).unwrap();
    }
}
