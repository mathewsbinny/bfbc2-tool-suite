//! Per-level compiled shader databases: `Levels/<LV>/Shaders/Dx9_Single.res`, `Dx10_Single.res`, `Dx11_Single.res`
//! (FbrbTool types `Dx9ShaderDatabase` ...) and the Xbox 360 `Single.res` of BFBC2's Xbox levels and Battlefield
//! 1943; a port of the reader and writer of `bfbc2-shader-tools/shaderdb.py` (`ShaderDb._parse`, `ShaderDb.to_bytes`;
//! the format: `bfbc2-shader-tools/FORMAT.md`).
//!
//! The file is a list of sections, each a `u32` count and its records: parameter blocks, binding tables, register
//! tables, vertex shaders, pixel shaders, programs, program keys, vertex layouts, entry / fragment / mesh-part
//! names, entry groups. PC files are little-endian; Xbox 360 files big-endian (recognised by [`is_big_endian`]).
//! Records are kept as slices of the file, in its byte order, so writing a parsed file gives its bytes back.
//!
//! | Python (`shaderdb.py`) | here |
//! |---|---|
//! | `ShaderDb.is_big_endian(data)` | [`is_big_endian`] |
//! | `ShaderDb._parse(data, dx, where, big_endian)` | [`parse`] (the message without `where: `) |
//! | `ShaderDb.to_bytes()` | [`write`] |
//!
//! ```
//! use bc2_formats::shaderdb::{self, Named, ShaderDb};
//!
//! let mut db = ShaderDb { version: 120, ..ShaderDb::default() };
//! db.entries.push(Named { name: b"Meshes/Box/Shaders/Box".as_slice().into(), guid: &[7; 16] });
//! db.groups.push((&[7; 16], vec![]));
//! let bytes = shaderdb::write(&db);
//! let again = shaderdb::parse(&bytes, 9, false)?;
//! assert_eq!(again.entries, db.entries);
//! assert_eq!(shaderdb::write(&again), bytes);
//! # Ok::<(), shaderdb::ShaderDbError>(())
//! ```

pub mod levels;
pub mod merge;
pub mod open;
pub mod query;
pub mod terrain;

use std::borrow::Cow;

use thiserror::Error;

/// A program record (section 6a).
pub const PROGRAM_SIZE: usize = 0x30;
/// A program key (section 6b) in BFBC2's files.
pub const KEY_SIZE: usize = 0x58;
/// A program key in version-117 files (Battlefield 1943, Xbox 360): four permutation bytes more.
pub const KEY_SIZE_117: usize = 0x5c;
/// A vertex layout record (section 7): GUID + 0x49 bytes.
pub const LAYOUT_SIZE: usize = 0x59;

/// Why a file is not a shader database this reader reads. The messages are `shaderdb.py`'s, without its
/// `<where>: ` prefix.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShaderDbError {
    /// A record runs past the end of the file.
    #[error("ends inside a record at 0x{at:x}")]
    EndsInside {
        /// Where the record (or the field) starts.
        at: usize,
    },
    /// A section's count cannot fit in the rest of the file.
    #[error("count {count} at 0x{at:x} does not fit in the file")]
    Count {
        /// The count.
        count: u32,
        /// Where the count is.
        at: usize,
    },
    /// A parameter block's size is less than its own size field.
    #[error("parameter block at 0x{at:x} has size {size}")]
    ParamSize {
        /// Where the block starts.
        at: usize,
        /// Its size field.
        size: u32,
    },
    /// The program and program key sections differ in length.
    #[error("{programs} programs but {keys} program keys")]
    KeyCount {
        /// Programs.
        programs: u32,
        /// Program keys.
        keys: u32,
    },
    /// A name has no terminating zero.
    #[error("unterminated name at 0x{at:x}")]
    Unterminated {
        /// Where the name starts.
        at: usize,
    },
    /// An entry group's program list runs past the end.
    #[error("entry group runs past the end of the file")]
    GroupPastEnd,
    /// Bytes follow the last section.
    #[error("{0} bytes after the last section")]
    Trailing(usize),
    /// A shader's parameter block, binding table or register table index is past its section.
    #[error("a shader names a table past the end of its section")]
    TableIndex,
}

/// A vertex or pixel shader (sections 4 and 5).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shader<'a> {
    /// The shader's GUID (16 bytes, the file's byte order).
    pub guid: &'a [u8],
    /// The compiled code.
    pub code: &'a [u8],
    /// Its parameter block (section 1 index).
    pub param: u32,
    /// Its binding table (section 2 index).
    pub binding: u32,
    /// Its register table (section 3 index).
    pub regs: u32,
    /// A vertex shader's input description, raw (empty for pixel shaders).
    pub tail: &'a [u8],
}

/// An entry, fragment or mesh-part name (section 8): latin-1 bytes and a 16-byte GUID.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Named<'a> {
    /// The name's bytes (latin-1, no terminator).
    pub name: Cow<'a, [u8]>,
    /// Its GUID.
    pub guid: &'a [u8],
}

/// One shader database, its sections in file order (as `shaderdb.ShaderDb`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ShaderDb<'a> {
    /// Xbox 360 byte order.
    pub big_endian: bool,
    /// The version (120 BFBC2, 117 Battlefield 1943).
    pub version: u32,
    /// The byte after the version.
    pub flag: u8,
    /// Section 1: parameter blocks (their `u32` size included).
    pub params: Vec<&'a [u8]>,
    /// Section 2: binding tables (their `u16` count included).
    pub bindings: Vec<&'a [u8]>,
    /// Section 3: register tables (their `u32` count included).
    pub regs: Vec<&'a [u8]>,
    /// Section 4.
    pub vs: Vec<Shader<'a>>,
    /// Section 5.
    pub ps: Vec<Shader<'a>>,
    /// Section 6a: program records.
    pub programs: Vec<&'a [u8]>,
    /// Section 6b: program keys.
    pub keys: Vec<&'a [u8]>,
    /// Section 7: vertex layouts.
    pub layouts: Vec<&'a [u8]>,
    /// Section 8.1.
    pub entries: Vec<Named<'a>>,
    /// Section 8.2.
    pub fragments: Vec<Named<'a>>,
    /// Section 8.3.
    pub parts: Vec<Named<'a>>,
    /// Section 8.4: (entry GUID, program indices).
    pub groups: Vec<(&'a [u8], Vec<u16>)>,
}

impl ShaderDb<'_> {
    /// The program key size of this file's version.
    #[must_use]
    pub fn key_size(&self) -> usize {
        key_size(self.version)
    }
}

/// The program key size of a version.
#[must_use]
pub fn key_size(version: u32) -> usize {
    if version == 117 { KEY_SIZE_117 } else { KEY_SIZE }
}

/// Does the data look like an Xbox 360 (big-endian) database? The version is a small number: read little-endian
/// it is huge.
#[must_use]
pub fn is_big_endian(data: &[u8]) -> bool {
    matches!(data, [0, _, _, d3, ..] if *d3 != 0)
}

struct Reader<'a> {
    d: &'a [u8],
    o: usize,
    be: bool,
}

impl<'a> Reader<'a> {
    fn need(&self, n: u64) -> Result<(), ShaderDbError> {
        if self.o as u64 + n > self.d.len() as u64 {
            return Err(ShaderDbError::EndsInside { at: self.o });
        }
        Ok(())
    }

    fn raw(&mut self, n: u64) -> Result<&'a [u8], ShaderDbError> {
        self.need(n)?;
        let s = self.o;
        self.o += n as usize;
        self.d.get(s..self.o).ok_or(ShaderDbError::EndsInside { at: s })
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], ShaderDbError> {
        let at = self.o;
        let r = self.raw(N as u64)?;
        r.first_chunk::<N>().copied().ok_or(ShaderDbError::EndsInside { at })
    }

    fn u8(&mut self) -> Result<u8, ShaderDbError> {
        Ok(self.array::<1>()?[0])
    }

    fn u16(&mut self) -> Result<u16, ShaderDbError> {
        let b = self.array::<2>()?;
        Ok(if self.be {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }

    fn u32(&mut self) -> Result<u32, ShaderDbError> {
        let b = self.array::<4>()?;
        Ok(if self.be {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }

    fn skip(&mut self, n: u64) -> Result<(), ShaderDbError> {
        self.raw(n).map(|_| ())
    }

    /// A section count, refused when `count * min_size` bytes cannot follow.
    fn count(&mut self, min_size: u64) -> Result<u32, ShaderDbError> {
        let n = self.u32()?;
        if u64::from(n) * min_size > (self.d.len() - self.o) as u64 {
            return Err(ShaderDbError::Count {
                count: n,
                at: self.o - 4,
            });
        }
        Ok(n)
    }

    fn cstr(&mut self) -> Result<&'a [u8], ShaderDbError> {
        let rest = self.d.get(self.o..).unwrap_or_default();
        let end = rest
            .iter()
            .position(|&b| b == 0)
            .ok_or(ShaderDbError::Unterminated { at: self.o })?;
        let s = rest.get(..end).unwrap_or_default();
        self.o += end + 1;
        Ok(s)
    }

    /// The bytes from `start` to the current offset.
    fn since(&self, start: usize) -> &'a [u8] {
        self.d.get(start..self.o).unwrap_or_default()
    }

    fn shader(&mut self) -> Result<Shader<'a>, ShaderDbError> {
        let guid = self.raw(16)?;
        let n = self.u32()?;
        let code = self.raw(u64::from(n))?;
        Ok(Shader {
            guid,
            code,
            param: self.u32()?,
            binding: self.u32()?,
            regs: self.u32()?,
            tail: &[],
        })
    }

    /// A vertex shader's input description: Xbox 360 `u32` n + n 12-byte Xenon vertex elements + 16 stream strides;
    /// Dx9 `u32` n + n `D3DVERTEXELEMENT9`; Dx10 / Dx11 `u32` n + n bytes, `u32` m + m 0x1c input elements, `u32` k +
    /// k semantic names.
    fn vertex_input(&mut self, dx: u32) -> Result<&'a [u8], ShaderDbError> {
        let start = self.o;
        let k = u64::from(self.u32()?);
        if self.be {
            self.skip(12 * k + 16)?;
        } else if dx == 9 {
            self.skip(8 * k)?;
        } else {
            self.skip(k)?;
            let m = u64::from(self.u32()?);
            self.skip(0x1c * m)?;
            for _ in 0..self.count(1)? {
                self.cstr()?;
            }
        }
        Ok(self.since(start))
    }

    /// Sections 1-3: parameter blocks (`u32` size, itself included), binding tables (`u16` k, 4 k + 2 bytes),
    /// register tables (`u32` k, 4 k bytes).
    fn tables(&mut self, db: &mut ShaderDb<'a>) -> Result<(), ShaderDbError> {
        let n = self.count(8)?;
        db.params.reserve(n as usize);
        for _ in 0..n {
            let start = self.o;
            let size = self.u32()?;
            if size < 4 {
                return Err(ShaderDbError::ParamSize { at: start, size });
            }
            self.skip(u64::from(size) - 4)?;
            db.params.push(self.since(start));
        }
        let n = self.count(6)?;
        db.bindings.reserve(n as usize);
        for _ in 0..n {
            let start = self.o;
            let k = self.u16()?;
            self.skip(4 * u64::from(k) + 2)?;
            db.bindings.push(self.since(start));
        }
        let n = self.count(4)?;
        db.regs.reserve(n as usize);
        for _ in 0..n {
            let start = self.o;
            let k = self.u32()?;
            self.skip(4 * u64::from(k))?;
            db.regs.push(self.since(start));
        }
        Ok(())
    }

    /// An entry group: GUID, `u32` k, k `u16` program indices.
    fn group(&mut self) -> Result<(&'a [u8], Vec<u16>), ShaderDbError> {
        let guid = self.raw(16)?;
        let k = u64::from(self.u32()?);
        if self.o as u64 + 2 * k > self.d.len() as u64 {
            return Err(ShaderDbError::GroupPastEnd);
        }
        let be = self.be;
        let idx = self.raw(2 * k)?.as_chunks::<2>().0;
        let word = |c: &[u8; 2]| {
            if be {
                u16::from_be_bytes(*c)
            } else {
                u16::from_le_bytes(*c)
            }
        };
        Ok((guid, idx.iter().map(word).collect()))
    }
}

/// Parses a database as `dx`'s layout (9: Dx9 vertex input; any other: Dx10 / Dx11) or, `big_endian`, as an Xbox
/// 360 file (whatever `dx` says). The whole file must be consumed and every shader's tables must exist.
///
/// # Errors
/// [`ShaderDbError`] for a file that does not read in this layout, with `shaderdb.py`'s message.
pub fn parse(data: &[u8], dx: u32, big_endian: bool) -> Result<ShaderDb<'_>, ShaderDbError> {
    let mut r = Reader {
        d: data,
        o: 0,
        be: big_endian,
    };
    let mut db = ShaderDb {
        big_endian,
        ..ShaderDb::default()
    };
    db.version = r.u32()?;
    db.flag = r.u8()?;
    let key_size = db.key_size() as u64;
    r.tables(&mut db)?;
    let n = r.count(32)?;
    db.vs.reserve(n as usize);
    for _ in 0..n {
        let mut s = r.shader()?;
        s.tail = r.vertex_input(dx)?;
        db.vs.push(s);
    }
    let n = r.count(32)?;
    db.ps.reserve(n as usize);
    for _ in 0..n {
        db.ps.push(r.shader()?);
    }
    let n = r.count(PROGRAM_SIZE as u64)?;
    db.programs = (0..n).map(|_| r.raw(PROGRAM_SIZE as u64)).collect::<Result<_, _>>()?;
    let n2 = r.count(key_size)?;
    if n2 != n {
        return Err(ShaderDbError::KeyCount { programs: n, keys: n2 });
    }
    db.keys = (0..n2).map(|_| r.raw(key_size)).collect::<Result<_, _>>()?;
    let n = r.count(LAYOUT_SIZE as u64)?;
    db.layouts = (0..n).map(|_| r.raw(LAYOUT_SIZE as u64)).collect::<Result<_, _>>()?;
    for list in [&mut db.entries, &mut db.fragments, &mut db.parts] {
        let n = r.count(17)?;
        list.reserve(n as usize);
        for _ in 0..n {
            let name = r.cstr()?;
            list.push(Named {
                name: Cow::Borrowed(name),
                guid: r.raw(16)?,
            });
        }
    }
    let n = r.count(20)?;
    db.groups.reserve(n as usize);
    for _ in 0..n {
        db.groups.push(r.group()?);
    }
    if r.o != data.len() {
        return Err(ShaderDbError::Trailing(data.len() - r.o));
    }
    let (np, nb, nr) = (db.params.len(), db.bindings.len(), db.regs.len());
    if db
        .vs
        .iter()
        .chain(&db.ps)
        .any(|s| !((s.param as usize) < np && (s.binding as usize) < nb && (s.regs as usize) < nr))
    {
        return Err(ShaderDbError::TableIndex);
    }
    Ok(db)
}

/// The size [`write`] gives the database.
#[must_use]
pub fn written_size(db: &ShaderDb<'_>) -> usize {
    let sum = |l: &[&[u8]]| l.iter().map(|r| r.len()).sum::<usize>();
    let shaders = |l: &[Shader<'_>]| {
        l.iter()
            .map(|s| s.guid.len() + 16 + s.code.len() + s.tail.len())
            .sum::<usize>()
    };
    let names = |l: &[Named<'_>]| l.iter().map(|n| n.name.len() + 1 + n.guid.len()).sum::<usize>();
    5 + 4 * 12
        + sum(&db.params)
        + sum(&db.bindings)
        + sum(&db.regs)
        + shaders(&db.vs)
        + shaders(&db.ps)
        + sum(&db.programs)
        + sum(&db.keys)
        + sum(&db.layouts)
        + names(&db.entries)
        + names(&db.fragments)
        + names(&db.parts)
        + db.groups.iter().map(|(g, i)| g.len() + 4 + 2 * i.len()).sum::<usize>()
}

/// Where [`emit`] puts the file's bytes.
trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

impl Sink for Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}

/// A buffer of the file's size: bytes past its end are counted, not written.
struct SliceSink<'b> {
    buf: &'b mut [u8],
    at: usize,
}

impl Sink for SliceSink<'_> {
    fn put(&mut self, bytes: &[u8]) {
        let room = self.buf.get_mut(self.at..).unwrap_or_default();
        let n = room.len().min(bytes.len());
        if let (Some(d), Some(s)) = (room.get_mut(..n), bytes.get(..n)) {
            d.copy_from_slice(s);
        }
        self.at += bytes.len();
    }
}

/// Every section in order, records as they are (`ShaderDb.to_bytes`).
fn emit(db: &ShaderDb<'_>, out: &mut impl Sink) {
    let be = db.big_endian;
    let u32b = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
    let count = |out: &mut dyn Sink, n: usize| out.put(&u32b(n as u32));
    out.put(&u32b(db.version));
    out.put(&[db.flag]);
    for list in [&db.params, &db.bindings, &db.regs] {
        count(out, list.len());
        for r in list {
            out.put(r);
        }
    }
    for shaders in [&db.vs, &db.ps] {
        count(out, shaders.len());
        for s in shaders {
            out.put(s.guid);
            count(out, s.code.len());
            out.put(s.code);
            for v in [s.param, s.binding, s.regs] {
                out.put(&u32b(v));
            }
            out.put(s.tail);
        }
    }
    for list in [&db.programs, &db.keys, &db.layouts] {
        count(out, list.len());
        for r in list {
            out.put(r);
        }
    }
    for names in [&db.entries, &db.fragments, &db.parts] {
        count(out, names.len());
        for n in names {
            out.put(&n.name);
            out.put(&[0]);
            out.put(n.guid);
        }
    }
    count(out, db.groups.len());
    for (g, idx) in &db.groups {
        out.put(g);
        count(out, idx.len());
        for &i in idx {
            out.put(&if be { i.to_be_bytes() } else { i.to_le_bytes() });
        }
    }
}

/// The file's bytes: every section in order, records as they are (`ShaderDb.to_bytes`).
#[must_use]
pub fn write(db: &ShaderDb<'_>) -> Vec<u8> {
    let mut out = Vec::with_capacity(written_size(db));
    emit(db, &mut out);
    out
}

/// [`write`] into a buffer of [`written_size`] bytes (the Python binding's `bytes` object, written in place).
/// Returns the size the file needs; a shorter buffer holds the file's start, a longer one keeps its tail.
pub fn write_into(db: &ShaderDb<'_>, buf: &mut [u8]) -> usize {
    let mut sink = SliceSink { buf, at: 0 };
    emit(db, &mut sink);
    sink.at
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::range_plus_one)]
mod tests {
    use super::*;

    /// A small database with one record in every section; `dx` picks the vertex input layout.
    fn sample(be: bool, dx: u32, version: u32) -> Vec<u8> {
        let mut db = ShaderDb {
            big_endian: be,
            version,
            flag: 1,
            ..ShaderDb::default()
        };
        db.params
            .push(if be { &[0, 0, 0, 6, 9, 9] } else { &[6, 0, 0, 0, 9, 9] });
        db.bindings.push(if be {
            &[0, 1, 1, 2, 3, 4, 5, 6]
        } else {
            &[1, 0, 1, 2, 3, 4, 5, 6]
        });
        db.regs.push(if be {
            &[0, 0, 0, 1, 1, 2, 3, 4]
        } else {
            &[1, 0, 0, 0, 1, 2, 3, 4]
        });
        let tail: &[u8] = if be {
            &[
                0, 0, 0, 1, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ]
        } else if dx == 9 {
            &[1, 0, 0, 0, 1, 2, 3, 4, 5, 6, 7, 8]
        } else {
            &[2, 0, 0, 0, 5, 5, 0, 0, 0, 0, 1, 0, 0, 0, b'P', 0]
        };
        db.vs.push(Shader {
            guid: &[1; 16],
            code: &[0xaa, 0xbb],
            tail,
            ..Shader::default()
        });
        db.ps.push(Shader {
            guid: &[2; 16],
            code: &[0xcc],
            ..Shader::default()
        });
        db.programs.push(&[3; PROGRAM_SIZE]);
        let key = [4; KEY_SIZE_117];
        db.keys.push(key.get(..key_size(version)).unwrap_or_default());
        db.layouts.push(&[5; LAYOUT_SIZE]);
        for (list, name) in [
            (&mut db.entries, "E/\u{e9}"),
            (&mut db.fragments, "F"),
            (&mut db.parts, "M_lod0/P/T"),
        ] {
            let name: Vec<u8> = name.chars().map(|c| c as u8).collect();
            list.push(Named {
                name: name.into(),
                guid: &[6; 16],
            });
        }
        db.groups.push((&[6; 16], vec![0, 0x0102]));
        write(&db)
    }

    #[test]
    fn round_trips() {
        for (be, dx, version) in [(false, 9, 120), (false, 10, 120), (true, 9, 120), (true, 9, 117)] {
            let data = sample(be, dx, version);
            assert_eq!(is_big_endian(&data), be);
            let db = parse(&data, dx, be).unwrap_or_default();
            assert_eq!(db.version, version);
            assert_eq!(db.keys.first().map(|k| k.len()), Some(key_size(version)));
            assert_eq!(db.groups.first().map(|g| g.1.clone()), Some(vec![0, 0x0102]));
            assert_eq!(written_size(&db), data.len());
            assert_eq!(write(&db), data);
            let mut buf = vec![0xee; data.len() + 2];
            assert_eq!(write_into(&db, &mut buf), data.len());
            assert_eq!(&buf[..data.len()], data.as_slice());
            assert_eq!(&buf[data.len()..], &[0xee, 0xee]);
            let mut short = vec![0; 10];
            assert_eq!(write_into(&db, &mut short), data.len());
            assert_eq!(short.as_slice(), &data[..10]);
        }
    }

    #[test]
    fn layouts_disagree() {
        // a Dx9 file read as Dx10 (and the other way) runs into another record
        assert!(parse(&sample(false, 9, 120), 10, false).is_err());
        assert!(parse(&sample(false, 10, 120), 9, false).is_err());
    }

    #[test]
    fn errors() {
        let good = sample(false, 9, 120);
        let p = |d: &[u8]| parse(d, 9, false).err().map(|e| e.to_string());
        assert_eq!(p(&good[..3]).as_deref(), Some("ends inside a record at 0x0"));
        let mut d = good.clone();
        d.push(0);
        assert_eq!(p(&d).as_deref(), Some("1 bytes after the last section"));
        // the parameter block count, then the block's size field
        let mut d = good.clone();
        d[5] = 0xff;
        assert_eq!(p(&d).as_deref(), Some("count 255 at 0x5 does not fit in the file"));
        let mut d = good.clone();
        d[9] = 3;
        assert_eq!(p(&d).as_deref(), Some("parameter block at 0x9 has size 3"));
        // the key count
        let keys_at = good
            .windows(4 + KEY_SIZE)
            .position(|w| w[4..].iter().all(|&b| b == 4))
            .unwrap_or(0);
        let mut d = good.clone();
        d[keys_at] = 0xff;
        assert_eq!(
            p(&d),
            Some(format!("count 255 at 0x{keys_at:x} does not fit in the file"))
        );
        d[keys_at] = 2;
        assert_eq!(p(&d).as_deref(), Some("1 programs but 2 program keys"));
        d[keys_at] = 0;
        assert_eq!(p(&d).as_deref(), Some("1 programs but 0 program keys"));
        // a name without its terminator, a group past the end, a shader's table index
        let n = good.len();
        let mut d = good[..n - 8].to_vec();
        d.extend([9, 0, 0, 0, 1, 1, 1, 1]);
        assert_eq!(p(&d).as_deref(), Some("entry group runs past the end of the file"));
        let name_at = good.windows(3).position(|w| w == b"E/\xe9").unwrap_or(0);
        let d = good[..name_at + 1].to_vec();
        let mut e = good[..name_at - 4].to_vec();
        e.extend([1, 0, 0, 0]);
        e.extend([b'E'; 17]);
        assert_eq!(
            p(&e).as_deref(),
            Some(format!("unterminated name at 0x{name_at:x}").as_str())
        );
        assert!(p(&d).is_some());
        let mut d = good.clone();
        let vs_param = 5 + 4 + 6 + 4 + 8 + 4 + 8 + 4 + 16 + 4 + 2;
        d[vs_param] = 1;
        assert_eq!(
            p(&d).as_deref(),
            Some("a shader names a table past the end of its section")
        );
    }

    #[test]
    fn big_endian_detection() {
        assert!(!is_big_endian(&[0, 0, 0]));
        assert!(!is_big_endian(&[120, 0, 0, 0]));
        assert!(is_big_endian(&[0, 0, 0, 120]));
    }
}
