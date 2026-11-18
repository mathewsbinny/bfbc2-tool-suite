//! Xbox / Xbox 360 game discs (XDVDFS), read only: a port of `bfbc2-console-readers/xiso/xiso.py`
//! (`find_partition`, `walk`), round F0. The layout is in that module's docstring.
//!
//! The same files in the same order (the directory tables' binary trees walked left, node, right; folders as they
//! are met), read as the reference reads them (a table cut short by the end of the image gives the entries that fit).
//! Where the reference recurses past what the port follows (a tree or folder chain deeper than [`MAX_DEPTH`]: the
//! reference's `RecursionError` on a looping folder) this returns [`Error::Deep`] and the Python walks the disc itself.

use std::collections::HashSet;
use std::io::{self, Read, Seek, SeekFrom};

/// A sector's size.
pub const SECTOR: u64 = 2048;
/// The volume descriptor's magic.
pub const MAGIC: &[u8; 20] = b"MICROSOFT*XBOX*MEDIA";
/// Where the game partition may start: plain / xiso, XGD2, XGD3, XGD1.
pub const PARTITIONS: [u64; 4] = [0x0, 0x0FD9_0000, 0x0208_0000, 0x1830_0000];
/// The deepest nesting of tree nodes and folders the port follows.
pub const MAX_DEPTH: usize = 120;

/// Why a disc is left to the reference.
#[derive(Debug)]
pub enum Error {
    /// No XDVDFS volume at any known partition.
    NoVolume,
    /// Nested deeper than [`MAX_DEPTH`].
    Deep,
    /// The image cannot be read.
    Io(io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoVolume => f.write_str("no XDVDFS volume found"),
            Self::Deep => f.write_str("the directory tree is nested too deep"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

fn read_at<R: Read + Seek>(f: &mut R, at: u64, n: u64) -> io::Result<Vec<u8>> {
    f.seek(SeekFrom::Start(at))?;
    let mut out = Vec::new();
    f.take(n).read_to_end(&mut out)?;
    Ok(out)
}

/// `find_partition(f)`: the first partition offset with the volume descriptor at its sector 32.
///
/// # Errors
/// [`Error::NoVolume`] (the reference's `SystemExit`) or a read error.
pub fn find_partition<R: Read + Seek>(f: &mut R) -> Result<u64, Error> {
    for base in PARTITIONS {
        if read_at(f, base + 32 * SECTOR, 20)? == MAGIC {
            return Ok(base);
        }
    }
    Err(Error::NoVolume)
}

/// The root directory's sector and size (the 8 bytes after the magic), `None` when the image ends before them.
///
/// # Errors
/// A read error.
pub fn root<R: Read + Seek>(f: &mut R, base: u64) -> io::Result<Option<(u32, u32)>> {
    let b = read_at(f, base + 32 * SECTOR + 20, 8)?;
    let w = |k: usize| b.get(k..k + 4).and_then(|s| s.try_into().ok()).map(u32::from_le_bytes);
    Ok(w(0).zip(w(4)))
}

/// One file: its path (latin-1 bytes, `/`-joined), sector and size.
pub type File = (Vec<u8>, u32, u32);

struct Walker<'f, R> {
    f: &'f mut R,
    base: u64,
    out: Vec<File>,
    /// Where the last table read left the file (the reference's file position after its generator ends).
    pos: u64,
}

impl<R: Read + Seek> Walker<'_, R> {
    fn walk(&mut self, sector: u32, size: u32, prefix: &[u8], depth: usize) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Deep);
        }
        let at = self.base + u64::from(sector) * SECTOR;
        let table = read_at(self.f, at, u64::from(size))?;
        self.pos = at + table.len() as u64;
        let mut seen = HashSet::new();
        self.node(&table, 0, prefix, &mut seen, depth)
    }

    fn node(
        &mut self,
        table: &[u8],
        off: usize,
        prefix: &[u8],
        seen: &mut HashSet<usize>,
        depth: usize,
    ) -> Result<(), Error> {
        if depth > MAX_DEPTH {
            return Err(Error::Deep);
        }
        if seen.contains(&off) || off + 14 > table.len() {
            return Ok(());
        }
        seen.insert(off);
        let Some(e) = table.get(off..off + 14) else {
            return Ok(());
        };
        let u16_at = |k: usize| {
            e.get(k..k + 2)
                .and_then(|s| s.try_into().ok())
                .map_or(0, u16::from_le_bytes)
        };
        let u32_at = |k: usize| {
            e.get(k..k + 4)
                .and_then(|s| s.try_into().ok())
                .map_or(0, u32::from_le_bytes)
        };
        let (left, right, sec, sz) = (u16_at(0), u16_at(2), u32_at(4), u32_at(8));
        let (attr, nlen) = (e.get(12).copied().unwrap_or(0), e.get(13).copied().unwrap_or(0));
        if left == 0xFFFF {
            return Ok(());
        }
        let name = table
            .get(off + 14..(off + 14 + usize::from(nlen)).min(table.len()))
            .unwrap_or(&[]);
        if left != 0 {
            self.node(table, usize::from(left) * 4, prefix, seen, depth + 1)?;
        }
        let mut path = prefix.to_vec();
        path.extend_from_slice(name);
        if attr & 0x10 != 0 {
            if sz != 0 {
                path.push(b'/');
                self.walk(sec, sz, &path, depth + 1)?;
            }
        } else {
            self.out.push((path, sec, sz));
        }
        if right != 0 {
            self.node(table, usize::from(right) * 4, prefix, seen, depth + 1)?;
        }
        Ok(())
    }
}

/// `list(walk(f, base, sector, size, prefix))` and the file position the walk leaves.
///
/// # Errors
/// [`Error::Deep`] (left to the reference) or a read error.
pub fn walk<R: Read + Seek>(
    f: &mut R,
    base: u64,
    sector: u32,
    size: u32,
    prefix: &[u8],
) -> Result<(Vec<File>, u64), Error> {
    let mut w = Walker {
        f,
        base,
        out: Vec::new(),
        pos: 0,
    };
    w.walk(sector, size, prefix, 0)?;
    Ok((w.out, w.pos))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn entry(left: u16, right: u16, sec: u32, size: u32, attr: u8, name: &[u8]) -> Vec<u8> {
        let mut e = Vec::new();
        e.extend_from_slice(&left.to_le_bytes());
        e.extend_from_slice(&right.to_le_bytes());
        e.extend_from_slice(&sec.to_le_bytes());
        e.extend_from_slice(&size.to_le_bytes());
        e.push(attr);
        e.push(name.len() as u8);
        e.extend_from_slice(name);
        while e.len() % 4 != 0 {
            e.push(0xFF);
        }
        e
    }

    fn disc(base: usize) -> Vec<u8> {
        let mut d = vec![0u8; base + 40 * SECTOR as usize];
        let v = base + 32 * SECTOR as usize;
        d[v..v + 20].copy_from_slice(MAGIC);
        // root at sector 33: "b.txt" with "a" (a folder at sector 34) on its left
        let a_off = 20; // after the first entry (14 bytes and a 5-byte name, padded to 20)
        let mut root = entry((a_off / 4) as u16, 0, 35, 5, 0x20, b"b.txt");
        root.extend(entry(0, 0, 34, 16, 0x10, b"a"));
        d[v + 20..v + 24].copy_from_slice(&33u32.to_le_bytes());
        d[v + 24..v + 28].copy_from_slice(&(root.len() as u32).to_le_bytes());
        let r = base + 33 * SECTOR as usize;
        d[r..r + root.len()].copy_from_slice(&root);
        let sub = entry(0, 0, 36, 3, 0, b"c");
        let s = base + 34 * SECTOR as usize;
        d[s..s + sub.len()].copy_from_slice(&sub);
        d
    }

    #[test]
    fn walks_a_disc() {
        for base in [0usize, 0x0208_0000] {
            let mut f = Cursor::new(disc(base));
            assert_eq!(find_partition(&mut f).unwrap(), base as u64);
            let (sec, size) = root(&mut f, base as u64).unwrap().unwrap();
            let (files, _) = walk(&mut f, base as u64, sec, size, b"").unwrap();
            let names: Vec<&[u8]> = files.iter().map(|f| f.0.as_slice()).collect();
            assert_eq!(names, vec![&b"a/c"[..], b"b.txt"]);
        }
        assert!(matches!(
            find_partition(&mut Cursor::new(vec![0u8; 100])),
            Err(Error::NoVolume)
        ));
    }

    #[test]
    fn a_looping_folder_is_left_to_the_reference() {
        let mut d = disc(0);
        let s = 34 * SECTOR as usize;
        let looping = entry(0, 0, 34, 16, 0x10, b"x");
        d[s..s + looping.len()].copy_from_slice(&looping);
        let mut f = Cursor::new(d);
        assert!(matches!(walk(&mut f, 0, 33, 40, b""), Err(Error::Deep)));
    }
}
