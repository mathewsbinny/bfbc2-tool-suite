//! Xbox 360 content packages (STFS: LIVE / PIRS / CON), read only: a port of
//! `bfbc2-console-readers/stfs/stfs.py` (`Package`: the header, the file table with each entry's path, a file's
//! bytes through the hash tables' block chains), round F0. The layout is in that module's docstring.
//!
//! The reader reads the file as the reference does, a short read included (a read past the end gives what is
//! there, a status byte past the end reads as 0), so a damaged package gives the reference's entries and bytes.
//! Where the reference raises (a header shorter than its fields, a file table entry cut short) or never finishes (a
//! folder that is its own ancestor) this returns an [`Error`] and the Python reads the package itself.

use std::io::{self, Read, Seek, SeekFrom};

/// A data block's size.
pub const BLOCK: u64 = 0x1000;

/// Why a package is left to the reference.
#[derive(Debug)]
pub enum Error {
    /// The header is shorter than its fields (the reference's `IndexError` / `struct.error`).
    Header,
    /// A file table entry cut short.
    Table,
    /// An entry's parents never reach the root (the reference loops for ever).
    Cycle,
    /// The file cannot be read.
    Io(io::Error),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Header => f.write_str("the header is cut short"),
            Self::Table => f.write_str("a file table entry is cut short"),
            Self::Cycle => f.write_str("an entry's folders never reach the root"),
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

/// A file table entry (`stfs.Entry`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The name's bytes (latin-1).
    pub name: Vec<u8>,
    /// A folder.
    pub dir: bool,
    /// Its blocks are consecutive.
    pub consecutive: bool,
    /// Its block count.
    pub count: u32,
    /// Its first block.
    pub start: u32,
    /// The index of its folder (0xFFFF: the root).
    pub parent: u16,
    /// Its size in bytes.
    pub size: u32,
    /// The names from the root down, `/`-joined (latin-1 bytes).
    pub path: Vec<u8>,
}

/// The header's values and the file table (`stfs.Package`'s attributes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    /// The first four bytes (`LIVE`, `PIRS`, `CON `).
    pub magic: Vec<u8>,
    /// The display name.
    pub name: String,
    /// The header's size.
    pub header_size: u32,
    /// The block separation byte.
    pub separation: u8,
    /// The file table's block count.
    pub ft_blocks: u16,
    /// The file table's first block.
    pub ft_start: u32,
    /// Where block 0 starts.
    pub base: u64,
    /// The allocated block count.
    pub allocated: u32,
    /// The hash tables' top level (0, 1 or 2).
    pub top_level: u8,
    /// 0 for one copy of each hash table (read-only packages), 1 for two.
    pub shift: u32,
}

/// A package being read.
pub struct Package<R> {
    f: R,
    /// The header.
    pub header: Header,
    /// The file table.
    pub entries: Vec<Entry>,
}

/// Up to `n` bytes at `at` (fewer at the end of the file), as `f.seek(at); f.read(n)`.
fn read_at<R: Read + Seek>(f: &mut R, at: u64, n: u64) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    read_into(f, at, n, &mut out)?;
    Ok(out)
}

/// `f.seek(at); out += f.read(n)`: up to `n` bytes appended (fewer at the end of the file), read straight into `out`.
fn read_into<R: Read + Seek>(f: &mut R, at: u64, n: u64, out: &mut Vec<u8>) -> io::Result<()> {
    f.seek(SeekFrom::Start(at))?;
    let n = usize::try_from(n).map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
    let start = out.len();
    out.try_reserve(n)
        .map_err(|_| io::Error::from(io::ErrorKind::OutOfMemory))?;
    out.resize(start + n, 0);
    let mut got = 0;
    while got < n {
        match f.read(out.get_mut(start + got..).unwrap_or(&mut [])) {
            Ok(0) => break,
            Ok(k) => got += k,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e),
        }
    }
    out.truncate(start + got);
    Ok(())
}

fn be(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0, |v, &b| (v << 8) | u64::from(b))
}

fn le(bytes: &[u8]) -> u64 {
    bytes.iter().rev().fold(0, |v, &b| (v << 8) | u64::from(b))
}

/// `bytes.decode('utf-16-be', 'ignore').split('\0')[0]`.
fn utf16_name(bytes: &[u8]) -> String {
    let units = bytes.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c));
    let s: String = char::decode_utf16(units).filter_map(Result::ok).collect();
    s.split('\0').next().unwrap_or("").to_owned()
}

impl<R: Read + Seek> Package<R> {
    /// `Package(path)`: the header and the file table.
    ///
    /// # Errors
    /// Where the reference raises or never finishes ([`Error`]).
    pub fn open(mut f: R) -> Result<Self, Error> {
        let h = read_at(&mut f, 0, 0x1000)?;
        if h.len() < 0x399 {
            return Err(Error::Header);
        }
        let get = |a: usize, b: usize| h.get(a..b.min(h.len())).unwrap_or(&[]);
        let header_size = be(get(0x340, 0x344)) as u32;
        let separation = h.get(0x37B).copied().unwrap_or(0);
        let allocated = be(get(0x395, 0x399)) as u32;
        let header = Header {
            magic: get(0, 4).to_vec(),
            name: utf16_name(get(0x411, 0x491)),
            header_size,
            separation,
            ft_blocks: le(get(0x37C, 0x37E)) as u16,
            ft_start: le(get(0x37E, 0x381)) as u32,
            base: (u64::from(header_size) + 0xFFF) & !0xFFF,
            allocated,
            top_level: if allocated <= 0xAA {
                0
            } else if allocated <= 0x70E4 {
                1
            } else {
                2
            },
            shift: u32::from(separation & 1 == 0),
        };
        let mut p = Self {
            f,
            header,
            entries: Vec::new(),
        };
        p.entries = p.file_table()?;
        Ok(p)
    }

    /// A package whose header was read before (a file reopened to read entries): no file table.
    pub fn from_header(f: R, header: Header) -> Self {
        Self {
            f,
            header,
            entries: Vec::new(),
        }
    }

    /// A data block's raw block (`_fix`).
    fn fix(&self, block: u64) -> u64 {
        let s = self.header.shift;
        let n = (((block + 0xAA) / 0xAA) << s) + block;
        if block < 0xAA {
            return n;
        }
        if block < 0x70E4 {
            return n + (((block + 0x70E4) / 0x70E4) << s);
        }
        (1 << s) + n + (((block + 0x70E4) / 0x70E4) << s)
    }

    fn addr(&self, block: u64) -> u64 {
        self.header.base + self.fix(block) * BLOCK
    }

    fn level0_raw(&self, block: u64) -> u64 {
        let s = self.header.shift;
        if block < 0xAA {
            return 0;
        }
        let mut raw = (block / 0xAA) * (0xAA + (1 << s)) + (((block / 0x70E4) + 1) << s);
        if block / 0x70E4 != 0 {
            raw += 1 << s;
        }
        raw
    }

    fn level1_raw(&self, block: u64) -> u64 {
        let s = self.header.shift;
        if block < 0x70E4 {
            return 0xAA + (1 << s);
        }
        let raw = (block / 0x70E4) * (0x70E4 + (0xAB << s)) + (((block / 0x004A_F768) + 1) << s);
        if block / 0x004A_F768 == 0 { raw } else { raw + (1 << s) }
    }

    fn status(&mut self, raw: u64, index: u64) -> io::Result<u8> {
        let at = self.header.base + raw * BLOCK + index * 0x18 + 0x14;
        Ok(read_at(&mut self.f, at, 1)?.first().copied().unwrap_or(0))
    }

    /// The level-0 hash entry of a data block, from the live copy of its table (`_hash_entry`).
    fn hash_entry(&mut self, block: u64) -> io::Result<Vec<u8>> {
        let mut live = 0;
        if self.header.shift != 0 {
            let top_copy = u64::from(self.header.separation & 2 != 0);
            match self.header.top_level {
                0 => live = top_copy,
                1 => live = u64::from(self.status(self.level1_raw(block) + top_copy, (block / 0xAA) % 0xAA)? >> 6 & 1),
                _ => {
                    let l2 = 0x70E4 + (0xAB << self.header.shift) + top_copy;
                    let l1_copy = u64::from(self.status(l2, (block / 0x70E4) % 0xAA)? >> 6 & 1);
                    live = u64::from(self.status(self.level1_raw(block) + l1_copy, (block / 0xAA) % 0xAA)? >> 6 & 1);
                }
            }
        }
        let at = self.header.base + (self.level0_raw(block) + live) * BLOCK + (block % 0xAA) * 0x18;
        read_at(&mut self.f, at, 0x18)
    }

    fn next(&mut self, block: u64) -> io::Result<u64> {
        let e = self.hash_entry(block)?;
        Ok(be(e.get(0x15.min(e.len())..0x18.min(e.len())).unwrap_or(&[])))
    }

    fn blocks(&mut self, start: u64, count: u64, consecutive: bool) -> io::Result<Vec<u64>> {
        if consecutive {
            return Ok((start..start + count).collect());
        }
        let mut out = Vec::with_capacity(count.min(1 << 20) as usize);
        let mut b = start;
        for _ in 0..count {
            out.push(b);
            b = self.next(b)?;
        }
        Ok(out)
    }

    fn file_table(&mut self) -> Result<Vec<Entry>, Error> {
        let mut raw = Vec::new();
        for b in self.blocks(u64::from(self.header.ft_start), u64::from(self.header.ft_blocks), false)? {
            let at = self.addr(b);
            read_into(&mut self.f, at, BLOCK, &mut raw)?;
        }
        let mut entries = Vec::new();
        for e in raw.chunks(0x40) {
            let flags = *e.get(0x28).ok_or(Error::Table)?;
            if flags.trailing_zeros() >= 6 {
                break; // no name: the table's end
            }
            if e.len() < 0x38 {
                return Err(Error::Table);
            }
            let s = |a: usize, b: usize| e.get(a..b).unwrap_or(&[]);
            entries.push(Entry {
                name: s(0, usize::from(flags & 0x3F)).to_vec(),
                dir: flags & 0x80 != 0,
                consecutive: flags & 0x40 != 0,
                count: le(s(0x29, 0x2C)) as u32,
                start: le(s(0x2F, 0x32)) as u32,
                parent: be(s(0x32, 0x34)) as u16,
                size: be(s(0x34, 0x38)) as u32,
                path: Vec::new(),
            });
        }
        for i in 0..entries.len() {
            let mut parts = vec![entries.get(i).map_or_else(Vec::new, |e| e.name.clone())];
            let mut p = entries.get(i).map_or(0xFFFF, |e| e.parent);
            let mut steps = 0;
            while p != 0xFFFF && usize::from(p) < entries.len() {
                steps += 1;
                if steps > entries.len() {
                    return Err(Error::Cycle);
                }
                let parent = entries.get(usize::from(p)).ok_or(Error::Table)?;
                parts.push(parent.name.clone());
                p = parent.parent;
            }
            parts.reverse();
            if let Some(e) = entries.get_mut(i) {
                e.path = parts.join(&b'/');
            }
        }
        Ok(entries)
    }

    /// A file's bytes (`Package.read(e)`): its blocks (consecutive, or chained through the hash tables) read in
    /// turn, cut at `size`.
    ///
    /// The reference reads every block before it cuts; reading stops here once `size` bytes are read, or (for
    /// consecutive blocks) at the end of the file, and a chain's links are looked up once each: the same bytes,
    /// without the reference's memory for an entry damaged to millions of blocks.
    ///
    /// # Errors
    /// When the file cannot be read, or the bytes would not fit in memory (the reference's `MemoryError`).
    pub fn read(&mut self, start: u64, count: u64, consecutive: bool, size: u64) -> io::Result<Vec<u8>> {
        let len = self.f.seek(SeekFrom::End(0))?;
        let mut out: Vec<u8> = Vec::new();
        if consecutive {
            // runs of blocks that lie next to each other in the file (between the hash tables) in one read each:
            // the bytes the reference's block-by-block reads give
            let mut k = 0;
            while k < count && (out.len() as u64) < size {
                let at = self.addr(start + k);
                if at >= len {
                    break; // every later block is further on: nothing more to read
                }
                let want = (size - out.len() as u64).div_ceil(BLOCK).max(1);
                let mut run = 1;
                while run < want && k + run < count && self.addr(start + k + run) == at + run * BLOCK {
                    run += 1;
                }
                read_into(&mut self.f, at, run * BLOCK, &mut out)?;
                k += run;
            }
            out.truncate(size.min(out.len() as u64) as usize);
            return Ok(out);
        }
        let mut links: std::collections::HashMap<u64, u64> = std::collections::HashMap::new();
        let mut b = start;
        for k in 0..count {
            if out.len() as u64 >= size {
                break;
            }
            let at = self.addr(b);
            if at < len {
                read_into(&mut self.f, at, BLOCK, &mut out)?;
            }
            if k + 1 < count {
                b = if let Some(&n) = links.get(&b) {
                    n
                } else {
                    let n = self.next(b)?;
                    links.insert(b, n);
                    n
                };
            }
        }
        out.truncate(size.min(out.len() as u64) as usize);
        Ok(out)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A read-only package: a folder and two files in consecutive blocks after the file table.
    fn package() -> Vec<u8> {
        let mut h = vec![0u8; 0x1000];
        h[..4].copy_from_slice(b"LIVE");
        for (i, c) in "Test".encode_utf16().enumerate() {
            h[0x411 + 2 * i..0x413 + 2 * i].copy_from_slice(&c.to_be_bytes());
        }
        h[0x340..0x344].copy_from_slice(&0x971Au32.to_be_bytes());
        h[0x37B] = 1; // read only: one hash table per level
        h[0x37C..0x37E].copy_from_slice(&1u16.to_le_bytes());
        h[0x395..0x399].copy_from_slice(&3u32.to_be_bytes());
        let base = 0xA000;
        let mut f = h;
        f.resize(base, 0);
        // raw block 0: the level-0 hash table; block 1 (data 0) the file table; data 1, 2 the files
        f.resize(base + 4 * 0x1000, 0);
        let entry = |name: &[u8], flags: u8, start: u32, count: u32, parent: u16, size: u32| {
            let mut e = vec![0u8; 0x40];
            e[..name.len()].copy_from_slice(name);
            e[0x28] = flags | name.len() as u8;
            e[0x29..0x2C].copy_from_slice(&count.to_le_bytes()[..3]);
            e[0x2F..0x32].copy_from_slice(&start.to_le_bytes()[..3]);
            e[0x32..0x34].copy_from_slice(&parent.to_be_bytes());
            e[0x34..0x38].copy_from_slice(&size.to_be_bytes());
            e
        };
        let mut table = entry(b"Content", 0x80, 0, 0, 0xFFFF, 0);
        table.extend(entry(b"a.bin", 0x40, 1, 1, 0, 5));
        table.extend(entry(b"b.bin", 0x40, 2, 1, 0xFFFF, 3));
        f[base + 0x1000..base + 0x1000 + table.len()].copy_from_slice(&table);
        f[base + 0x2000..base + 0x2005].copy_from_slice(b"hello");
        f[base + 0x3000..base + 0x3003].copy_from_slice(b"bye");
        f
    }

    #[test]
    fn reads_a_package() {
        let mut p = Package::open(Cursor::new(package())).unwrap();
        assert_eq!(p.header.magic, b"LIVE");
        assert_eq!(p.header.name, "Test");
        assert_eq!(p.header.base, 0xA000);
        assert_eq!(p.header.shift, 0);
        let paths: Vec<&[u8]> = p.entries.iter().map(|e| e.path.as_slice()).collect();
        assert_eq!(paths, vec![&b"Content"[..], b"Content/a.bin", b"b.bin"]);
        assert_eq!(p.read(1, 1, true, 5).unwrap(), b"hello");
        assert_eq!(p.read(2, 1, true, 3).unwrap(), b"bye");
    }

    #[test]
    fn refusals() {
        let f = package();
        assert!(matches!(
            Package::open(Cursor::new(f[..0x300].to_vec())),
            Err(Error::Header)
        ));
        let mut cyc = f.clone();
        let at = 0xA000 + 0x1000 + 0x32;
        cyc[at..at + 2].copy_from_slice(&0u16.to_be_bytes()); // Content is its own folder
        assert!(matches!(Package::open(Cursor::new(cyc)), Err(Error::Cycle)));
        // a file table past the end of the file reads as nothing
        let p = Package::open(Cursor::new(f[..0xA000].to_vec())).unwrap();
        assert_eq!(p.entries, vec![]);
        // one cut inside an entry is the reference's error
        assert!(matches!(
            Package::open(Cursor::new(f[..0xB000 + 0x50].to_vec())),
            Err(Error::Table)
        ));
    }

    #[test]
    fn names() {
        assert_eq!(utf16_name(&[0, b'A', 0xD8, 0, 0, b'B', 0, 0, 0, b'C', 7]), "AB");
    }
}
