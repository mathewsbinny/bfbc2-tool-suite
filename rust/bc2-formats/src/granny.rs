//! Bad Company 2's Granny resources (`GrannyModel` skeletons, `GrannyAnimation` / `DeltaAnimation` animations): a
//! port of the reader of `bfbc2-anim-tools/granny.py` (its docstring describes the format), PC (little-endian) and
//! Xbox 360 (big-endian) alike.
//!
//! | Python (`granny.py`) | here |
//! |---|---|
//! | `Granny.__init__` (DICE's table, the string table, the sections, the relocations) | [`Granny::parse`] |
//! | `Granny.members` / `size` / `type_name` / `name` | [`Granny::members`], [`Granny::size`], [`Granny::type_name`], [`Granny::name`] |
//! | `Granny.root` / `read` / `_value` (the object tree as dicts and lists) | [`Granny::root`]: a [`Value`] tree, built as the same dicts by the bindings |
//! | `bone_table`, `renumber` (DICE's per-bone table) | [`bone_table`], [`renumber`] |
//!
//! The Xbox 360 -> PC conversion (`to_pc`) is `bc2_convert::granny`.
//!
//! The rules are the Python reader's to the error: whatever it reads, this reads the same (the same values, the same
//! slices of a section cut short by the file's end, the same `#<index>` for a string index past the table); where it
//! raises (a `GrannyError`, or `struct.error`, `IndexError`, `TypeError`, `RecursionError` on damaged data) this
//! returns a [`GrannyError`], and the bindings let the Python reader raise its own exception. Deeper nesting than
//! [`MAX_DEPTH`] and more than [`MAX_ITEMS`] elements in one array are errors here too (the Python reader then runs).
//!
//! ```
//! use bc2_formats::granny::{bone_table, renumber, TableKind};
//! use std::collections::HashMap;
//!
//! // an animation's table: 10, then 1 record {bone 4, 1 interval (0.0, 1.0)}
//! let words = [10, 1, 4, 1, 0, 0x3f80_0000];
//! assert_eq!(bone_table(&words)?, (TableKind::Animation, vec![2]));
//! let bones = HashMap::from([(4, 9)]);
//! assert_eq!(renumber(&words, &bones)?, vec![10, 1, 9, 1, 0, 0x3f80_0000]);
//! # Ok::<(), bc2_formats::granny::GrannyError>(())
//! ```

use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;

use thiserror::Error;

/// The Granny 2 file's magic on PC (32-bit little-endian).
pub const MAGIC: [u8; 16] = [
    0x29, 0xDE, 0x6C, 0xC0, 0xBA, 0xA4, 0x53, 0x2B, 0x25, 0xF5, 0xB7, 0xA5, 0xF6, 0x66, 0xE2, 0xEE,
];
/// The same format 32-bit big-endian (Xbox 360).
pub const MAGIC_BE: [u8; 16] = [
    0xB5, 0x95, 0x11, 0x0E, 0x4B, 0xB5, 0xA5, 0x6A, 0x50, 0x28, 0x28, 0xEB, 0x04, 0xB3, 0x78, 0x25,
];
/// Nesting deeper than this (type definitions inside type definitions, objects inside objects) is an error here.
pub const MAX_DEPTH: usize = 256;
/// More elements than this in one array is an error here.
pub const MAX_ITEMS: u64 = 1 << 24;

/// A reference: (section, offset in the section).
pub type Ref = (u32, u32);

/// A hasher for [`Ref`] keys (`FxHash`'s step: a rotate, an xor and a multiply per word). The maps are looked up
/// for every pointer and type of a file; `SipHash`'s protection against chosen keys buys nothing here.
#[derive(Default, Clone, Copy)]
pub struct RefHasher(u64);

impl std::hash::Hasher for RefHasher {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.write_u32(u32::from(b));
        }
    }

    fn write_u32(&mut self, i: u32) {
        self.0 = (self.0.rotate_left(5) ^ u64::from(i)).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
    }
}

/// A map keyed by [`Ref`]s.
pub type RefMap<V> = HashMap<Ref, V, std::hash::BuildHasherDefault<RefHasher>>;

/// Why a resource does not read here (the Python reader raises its own exception for each, with its message).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[error("{0}")]
pub struct GrannyError(pub String);

fn err<T>(what: impl Into<String>) -> Result<T, GrannyError> {
    Err(GrannyError(what.into()))
}

/// The bytes of a member type (`SIZES`); 0 for the others.
#[must_use]
pub fn type_size(t: u32) -> u64 {
    match t {
        11..=14 => 1,
        15..=18 | 21 => 2,
        2 | 8 | 10 | 19 | 20 | 22 => 4,
        3..=5 => 8,
        7 => 12,
        9 => 68,
        _ => 0,
    }
}

/// A member type that is one number (`SCALAR`: 10 real32, 11-14 8-bit, 15-18 16-bit, 19-20 32-bit, 21 real16).
#[must_use]
pub fn is_scalar(t: u32) -> bool {
    (10..=21).contains(&t)
}

/// Byte order of a resource.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Endian {
    /// Big-endian (Xbox 360).
    pub big: bool,
}

impl Endian {
    /// `N` bytes at `at` in `d`, in reading order (swapped for little-endian), or an error past the end.
    fn get<const N: usize>(d: &[u8], at: u64) -> Result<[u8; N], GrannyError> {
        usize::try_from(at)
            .ok()
            .and_then(|a| d.get(a..)?.first_chunk::<N>().copied())
            .ok_or_else(|| GrannyError(format!("{N} bytes at {at}: past the end ({} bytes)", d.len())))
    }

    /// A `u16` at `at`.
    ///
    /// # Errors
    /// Past the end of `d`.
    pub fn u16(self, d: &[u8], at: u64) -> Result<u16, GrannyError> {
        let b = Self::get::<2>(d, at)?;
        Ok(if self.big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    }

    /// A `u32` at `at`.
    ///
    /// # Errors
    /// Past the end of `d`.
    pub fn u32(self, d: &[u8], at: u64) -> Result<u32, GrannyError> {
        let b = Self::get::<4>(d, at)?;
        Ok(if self.big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    }

    /// An `f32` at `at`.
    ///
    /// # Errors
    /// Past the end of `d`.
    pub fn f32(self, d: &[u8], at: u64) -> Result<f32, GrannyError> {
        Ok(f32::from_bits(self.u32(d, at)?))
    }

    /// One scalar of member type `t` (see [`is_scalar`]) at `at`.
    ///
    /// # Errors
    /// Past the end of `d`, or `t` not a scalar type.
    pub fn scalar(self, t: u32, d: &[u8], at: u64) -> Result<Scalar, GrannyError> {
        Ok(match t {
            10 => Scalar::Float(f64::from(self.f32(d, at)?)),
            11 | 13 => Scalar::Int(i64::from(Self::get::<1>(d, at)?[0].cast_signed())),
            12 | 14 => Scalar::Int(i64::from(Self::get::<1>(d, at)?[0])),
            15 | 17 => Scalar::Int(i64::from(self.u16(d, at)?.cast_signed())),
            16 | 18 => Scalar::Int(i64::from(self.u16(d, at)?)),
            19 => Scalar::Int(i64::from(self.u32(d, at)?.cast_signed())),
            20 => Scalar::Int(i64::from(self.u32(d, at)?)),
            21 => Scalar::Float(half(self.u16(d, at)?)),
            _ => return err(format!("member type {t} is not a number")),
        })
    }
}

/// An IEEE half as Python's `struct` (`'e'`) reads it: exact; a NaN without its payload (CPython 3.13's
/// `PyFloat_Unpack2`).
#[must_use]
pub fn half(h: u16) -> f64 {
    let sign = if h & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = i32::from((h >> 10) & 0x1F);
    let f = f64::from(h & 0x3FF);
    if e == 0x1F {
        return if f == 0.0 {
            sign * f64::INFINITY
        } else {
            sign * f64::NAN.abs()
        };
    }
    if e == 0 {
        return sign * f * 2f64.powi(-24);
    }
    sign * (1.0 + f / 1024.0) * 2f64.powi(e - 15)
}

/// A number read from the file.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scalar {
    /// An integer member (8, 16, 32 bits).
    Int(i64),
    /// A real32 or real16 member.
    Float(f64),
}

/// One member definition: (type, name index, child type, array size).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Member {
    /// The member type (1 inline struct, 2 reference, ... see the module).
    pub kind: u32,
    /// Its name: an index into the string table.
    pub name: u32,
    /// The child type definition (a relocation of the definition's third word), if any.
    pub child: Option<Ref>,
    /// The array size (0: one value, not a list).
    pub arr: u32,
}

/// A string: bytes of the file (latin-1), or a string index past the table (`#<index>`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Text<'a> {
    /// A string table entry or a C string in a section.
    Bytes(&'a [u8]),
    /// An index past the string table: Python's `f'#{i}'`.
    Missing(u32),
    /// The string table's entry of this index (the bindings make each such string once).
    Index(u32),
}

/// A value of the object tree (`Granny.read` / `_value`).
#[derive(Clone, Debug, PartialEq)]
pub enum Value<'a> {
    /// `None` (no reference, a depth limit, a type that reads as nothing).
    None,
    /// A number.
    Scalar(Scalar),
    /// A string member.
    Text(Text<'a>),
    /// A transform: flags, then 16 floats (position 3, orientation 4, scale / shear 9).
    Transform(u32, [f32; 16]),
    /// A list (an array member, a reference to an array, an array of references).
    List(Vec<Value<'a>>),
    /// A list of integers (a reference to an array of a one-integer type).
    Ints(Vec<i64>),
    /// A list of floats (a reference to an array of a one-real type).
    Floats(Vec<f64>),
    /// An object (a dict with `__at__`, `__tref__`, its members in order, and `__type__` for a variant).
    Object(Box<Object<'a>>),
}

/// An object: where it is, its type, its members (name, value) in definition order, and a variant's type name.
#[derive(Clone, Debug, PartialEq)]
pub struct Object<'a> {
    /// (section, offset): `__at__`.
    pub at: (u32, u64),
    /// The type: `__tref__`.
    pub tref: Ref,
    /// The members, in definition order (a repeated name: Python's dict keeps the first place, the last value).
    pub fields: Vec<(Text<'a>, Value<'a>)>,
    /// A variant's type name (`__type__`), latin-1 bytes.
    pub type_name: Option<Vec<u8>>,
}

/// A parsed resource (`Granny`): DICE's table and the string table, the Granny file's sections and relocations.
pub struct Granny<'a> {
    /// The resource.
    pub data: &'a [u8],
    /// Its byte order.
    pub endian: Endian,
    /// Where the string table starts (DICE's table is `data[4..strings_from]`).
    pub strings_from: usize,
    /// The string table (latin-1 bytes; an entry cut short by the end of the data is what remains).
    pub strings: Vec<&'a [u8]>,
    /// Where the Granny file starts in the resource.
    pub gr2: usize,
    /// The root type and object.
    pub root_type: Ref,
    /// The root object.
    pub root_obj: Ref,
    /// The Granny file's total size (its header).
    pub total: u32,
    /// Where the CRC'd part starts in the Granny file (32 + the section table's offset).
    pub crc_from: u64,
    /// Each section's bytes (cut short by the end of the data).
    pub sections: Vec<&'a [u8]>,
    /// Each section's offset in the resource.
    pub section_at: Vec<u64>,
    /// The relocations: (section, offset) -> (target section, target offset); a repeated one: the last.
    pub reloc: RefMap<Ref>,
    types: RefCell<RefMap<Arc<[Member]>>>,
    sizes: RefCell<RefMap<u64>>,
}

/// `b[from..to]` as Python slices: both ends clamped to the data.
fn slice(b: &[u8], from: u64, to: u64) -> &[u8] {
    let n = b.len() as u64;
    let (from, to) = (from.min(n) as usize, to.min(n) as usize);
    b.get(from..to.max(from)).unwrap_or_default()
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    hay.get(from..)?
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|k| k + from)
}

impl<'a> Granny<'a> {
    /// Reads the resource's tables (`Granny.__init__`).
    ///
    /// # Errors
    /// Where `Granny.__init__` raises.
    #[expect(
        clippy::many_single_char_names,
        reason = "the names of granny.py's reader, kept for comparison"
    )]
    pub fn parse(b: &'a [u8]) -> Result<Self, GrannyError> {
        if b.len() < 4 {
            return err("not a Granny resource of this game");
        }
        let le = Endian { big: false }.u32(b, 0)?;
        let be = Endian { big: true }.u32(b, 0)?;
        if le != 10 && be != 10 {
            return err(format!("not a Granny resource of this game (version {le})"));
        }
        let e = Endian { big: le != 10 };
        let mut p = 4u64;
        if !strings_at(b, p, e) {
            let mut needle = if e.big { 6u16.to_be_bytes() } else { 6u16.to_le_bytes() }.to_vec();
            needle.extend_from_slice(b"<file>");
            if let Some(k) = find(b, &needle, 4) {
                p = k as u64 - 4;
            }
        }
        if !strings_at(b, p, e) {
            return err("no string table where one is expected");
        }
        let strings_from = p as usize;
        let n = e.u32(b, p)?;
        p += 4;
        let mut strings = Vec::with_capacity(n as usize);
        for _ in 0..n {
            let ln = u64::from(e.u16(b, p)?);
            strings.push(slice(b, p + 2, p + 2 + ln));
            p += 2 + ln;
        }
        let gr2 = p + 4;
        let g = slice(b, gr2, u64::MAX);
        let magic = if e.big { MAGIC_BE } else { MAGIC };
        if g.get(..16) != Some(&magic[..]) {
            return err("no Granny 2 file after the string table");
        }
        let rev = e.u32(g, 32)?;
        let total = e.u32(g, 36)?;
        let soff = u64::from(e.u32(g, 44)?);
        let count = e.u32(g, 48)?;
        if rev != 7 {
            return err(format!("Granny format revision {rev}; this reader knows 7"));
        }
        let root_type = (e.u32(g, 52)?, e.u32(g, 56)?);
        let root_obj = (e.u32(g, 60)?, e.u32(g, 64)?);
        let mut sections = Vec::new();
        let mut section_at = Vec::new();
        let mut reloc = RefMap::default();
        for k in 0..count {
            let row = 32 + soff + 44 * u64::from(k);
            let w = |i: u64| e.u32(g, row + 4 * i);
            let (comp, off, size, roff, rcount) = (w(0)?, w(1)?, w(2)?, w(7)?, w(8)?);
            w(10)?; // the whole row is read
            if comp != 0 {
                return err(format!("section {k} is compressed (type {comp}); not read"));
            }
            let (off, size, roff) = (u64::from(off), u64::from(size), u64::from(roff));
            sections.push(slice(g, off, off + size));
            section_at.push(gr2 + off);
            for r in 0..u64::from(rcount) {
                let at = roff + 12 * r;
                let (o, ts, to) = (e.u32(g, at)?, e.u32(g, at + 4)?, e.u32(g, at + 8)?);
                reloc.insert((k, o), (ts, to));
            }
        }
        Ok(Self {
            data: b,
            endian: e,
            strings_from,
            strings,
            gr2: usize::try_from(gr2).unwrap_or(usize::MAX),
            root_type,
            root_obj,
            total,
            crc_from: 32 + soff,
            sections,
            section_at,
            reloc,
            types: RefCell::default(),
            sizes: RefCell::default(),
        })
    }

    /// The same tables over another copy of the resource, of the same layout: each section's bytes read from `data`
    /// at the place they were read from (a resource written in place keeps the tables it was read with: GRANNY-EDIT-2).
    pub fn over(&mut self, data: &'a [u8]) {
        for (s, &at) in self.sections.iter_mut().zip(&self.section_at) {
            let at = usize::try_from(at).unwrap_or(usize::MAX);
            *s = data.get(at..at.saturating_add(s.len())).unwrap_or_default();
        }
    }

    /// Where member `name` (latin-1 bytes) of the type at `tref` starts in its object (`Granny.field`'s walk): the
    /// first member of that name, `None` when there is none.
    ///
    /// # Errors
    /// As [`members`](Self::members) and [`size`](Self::size) (an inline struct before it without a type).
    pub fn member_offset(&self, tref: Ref, name: &[u8]) -> Result<Option<u64>, GrannyError> {
        let mut off = 0u64;
        for m in self.members(tref)?.iter() {
            if self.text_bytes(self.name(m.name)) == name {
                return Ok(Some(off));
            }
            off = mul_add(self.one_size(m, 0)?, u64::from(m.arr.max(1)), off)?;
        }
        Ok(None)
    }

    /// The string of index `i` (`name`): the table's entry, else `#<i>`.
    #[must_use]
    pub fn name(&self, i: u32) -> Text<'a> {
        if (i as usize) < self.strings.len() {
            Text::Index(i)
        } else {
            Text::Missing(i)
        }
    }

    /// The bytes of a [`Text`] (`#<i>` for a missing index).
    #[must_use]
    pub fn text_bytes(&self, t: Text<'a>) -> Vec<u8> {
        match t {
            Text::Bytes(b) => b.to_vec(),
            Text::Index(i) => self.strings.get(i as usize).copied().unwrap_or_default().to_vec(),
            Text::Missing(i) => format!("#{i}").into_bytes(),
        }
    }

    fn section(&self, sec: u32) -> Result<&'a [u8], GrannyError> {
        self.sections
            .get(sec as usize)
            .copied()
            .ok_or_else(|| GrannyError(format!("section {sec}: there are {}", self.sections.len())))
    }

    /// The relocation at (sec, off) (`_ptr`).
    #[must_use]
    pub fn ptr(&self, sec: u32, off: u64) -> Option<Ref> {
        self.reloc.get(&(sec, u32::try_from(off).ok()?)).copied()
    }

    /// The members of the type at `r` (`members`).
    ///
    /// # Errors
    /// No such section, or the definitions run past its end.
    pub fn members(&self, r: Ref) -> Result<Arc<[Member]>, GrannyError> {
        if let Some(m) = self.types.borrow().get(&r) {
            return Ok(m.clone());
        }
        let d = self.section(r.0)?;
        let mut off = u64::from(r.1);
        let mut out = Vec::new();
        loop {
            let t = self.endian.u32(d, off)?;
            let nm = self.endian.u32(d, off + 4)?;
            self.endian.u32(d, off + 8)?;
            let arr = self.endian.u32(d, off + 12)?;
            if t == 0 {
                break;
            }
            out.push(Member {
                kind: t,
                name: nm,
                child: self.ptr(r.0, off + 8),
                arr,
            });
            off += 32;
        }
        let m: Arc<[Member]> = out.into();
        self.types.borrow_mut().insert(r, m.clone());
        Ok(m)
    }

    /// The bytes of one object of the type at `r` (`size`).
    ///
    /// # Errors
    /// An inline struct without a child type, a definition past its section, nesting deeper than [`MAX_DEPTH`] or a
    /// size past 64 bits.
    pub fn size(&self, r: Ref) -> Result<u64, GrannyError> {
        self.size_at(r, 0)
    }

    fn size_at(&self, r: Ref, depth: usize) -> Result<u64, GrannyError> {
        if let Some(s) = self.sizes.borrow().get(&r) {
            return Ok(*s);
        }
        if depth > MAX_DEPTH {
            return err("type definitions nested too deep");
        }
        let mut s = 0u64;
        for m in self.members(r)?.iter() {
            let one = self.one_size(m, depth)?;
            s = one
                .checked_mul(u64::from(m.arr.max(1)))
                .and_then(|x| x.checked_add(s))
                .ok_or_else(|| GrannyError("a type's size is past 64 bits".into()))?;
        }
        self.sizes.borrow_mut().insert(r, s);
        Ok(s)
    }

    /// The size of one element of member `m`: its child's for an inline struct, else its type's.
    fn one_size(&self, m: &Member, depth: usize) -> Result<u64, GrannyError> {
        if m.kind == 1 {
            let Some(c) = m.child else {
                return err("an inline struct without a type");
            };
            self.size_at(c, depth + 1)
        } else {
            Ok(type_size(m.kind))
        }
    }

    /// A variant's type name (`type_name`): its first member's name when that starts with `CurveDataHeader`, else
    /// the member names joined by `/`.
    ///
    /// # Errors
    /// As [`members`](Self::members).
    pub fn type_name(&self, r: Ref) -> Result<Vec<u8>, GrannyError> {
        let ms = self.members(r)?;
        if let Some(first) = ms.first() {
            let n = self.text_bytes(self.name(first.name));
            if n.starts_with(b"CurveDataHeader") {
                return Ok(n);
            }
        }
        let mut out = Vec::new();
        for (i, m) in ms.iter().enumerate() {
            if i > 0 {
                out.push(b'/');
            }
            out.extend(self.text_bytes(self.name(m.name)));
        }
        Ok(out)
    }

    /// The object tree (`root(limit)`).
    ///
    /// # Errors
    /// Where `Granny.root` raises (and past [`MAX_DEPTH`] / [`MAX_ITEMS`]).
    pub fn root(&self, limit: i64) -> Result<Value<'a>, GrannyError> {
        self.read(
            Some(self.root_type),
            Some((self.root_obj.0, u64::from(self.root_obj.1))),
            0,
            limit,
        )
    }

    /// `read(tref, oref, depth, limit)`.
    ///
    /// # Errors
    /// See [`root`](Self::root).
    pub fn read(
        &self,
        tref: Option<Ref>,
        oref: Option<(u32, u64)>,
        depth: i64,
        limit: i64,
    ) -> Result<Value<'a>, GrannyError> {
        let Some(at) = oref else { return Ok(Value::None) };
        let (sec, mut off) = at;
        if depth > limit {
            return Ok(Value::None);
        }
        if depth > MAX_DEPTH as i64 {
            return err("objects nested too deep");
        }
        let Some(tref) = tref else {
            return err("an object without a type");
        };
        let ms = self.members(tref)?;
        let mut fields = Vec::with_capacity(ms.len());
        for m in ms.iter() {
            let one = self.one_size(m, 0)?;
            let n = u64::from(m.arr.max(1));
            if n > MAX_ITEMS {
                return err("an array of too many elements");
            }
            let mut vals = Vec::with_capacity(n as usize);
            for k in 0..n {
                let at = mul_add(k, one, off)?;
                vals.push(self.value(m.kind, m.child, sec, at, depth, limit)?);
            }
            let v = if m.arr != 0 {
                Value::List(vals)
            } else {
                vals.pop().unwrap_or(Value::None)
            };
            fields.push((self.name(m.name), v));
            off = mul_add(one, n, off)?;
        }
        Ok(Value::Object(Box::new(Object {
            at,
            tref,
            fields,
            type_name: None,
        })))
    }

    #[expect(
        clippy::many_single_char_names,
        reason = "the names of granny.py's reader, kept for comparison"
    )]
    fn value(
        &self,
        t: u32,
        child: Option<Ref>,
        sec: u32,
        off: u64,
        depth: i64,
        limit: i64,
    ) -> Result<Value<'a>, GrannyError> {
        let d = self.section(sec)?;
        let e = self.endian;
        Ok(match t {
            1 => self.read(child, Some((sec, off)), depth + 1, limit)?,
            2 => self.read(child, self.ptr(sec, off).map(wide), depth + 1, limit)?,
            3 => self.ref_array(child, sec, off, depth, limit)?,
            4 => {
                let n = e.u32(d, off)?;
                let Some(p) = self.ptr(sec, off + 4) else {
                    return Ok(Value::List(Vec::new()));
                };
                if u64::from(n) > MAX_ITEMS {
                    return err("an array of too many elements");
                }
                let mut out = Vec::with_capacity(n as usize);
                for k in 0..u64::from(n) {
                    let q = self.ptr(p.0, u64::from(p.1) + 4 * k).map(wide);
                    out.push(self.read(child, q, depth + 1, limit)?);
                }
                Value::List(out)
            }
            5 => {
                let (tp, op) = (self.ptr(sec, off), self.ptr(sec, off + 4));
                let (Some(tp), Some(op)) = (tp, op) else {
                    return Ok(Value::None);
                };
                let mut v = self.read(Some(tp), Some(wide(op)), depth + 1, limit)?;
                if let Value::Object(o) = &mut v {
                    o.type_name = Some(self.type_name(tp)?);
                }
                v
            }
            7 => {
                let tp = self.ptr(sec, off);
                let n = e.u32(d, off + 4)?;
                let p = self.ptr(sec, off + 8);
                let (Some(tp), Some(p)) = (tp, p) else {
                    return Ok(Value::List(Vec::new()));
                };
                let sz = self.size(tp)?;
                if u64::from(n) > MAX_ITEMS {
                    return err("an array of too many elements");
                }
                let mut out = Vec::with_capacity(n as usize);
                for k in 0..u64::from(n) {
                    out.push(self.read(Some(tp), Some((p.0, mul_add(k, sz, u64::from(p.1))?)), depth + 1, limit)?);
                }
                Value::List(out)
            }
            8 => {
                if let Some(p) = self.ptr(sec, off) {
                    let s = self.section(p.0)?;
                    let from = p.1 as usize;
                    let Some(end) = s.get(from..).and_then(|r| r.iter().position(|&c| c == 0)) else {
                        return err("a string without its end");
                    };
                    Value::Text(Text::Bytes(s.get(from..from + end).unwrap_or_default()))
                } else {
                    Value::Text(self.name(e.u32(d, off)?))
                }
            }
            9 => {
                let flags = e.u32(d, off)?;
                let mut v = [0f32; 16];
                for (i, x) in v.iter_mut().enumerate() {
                    *x = e.f32(d, off + 4 + 4 * i as u64)?;
                }
                Value::Transform(flags, v)
            }
            t if is_scalar(t) => Value::Scalar(e.scalar(t, d, off)?),
            _ => Value::None,
        })
    }

    /// A reference to an array (member type 3) at (sec, off): a list of objects, or of numbers for a one-number type.
    #[expect(
        clippy::many_single_char_names,
        reason = "the names of granny.py's reader, kept for comparison"
    )]
    fn ref_array(
        &self,
        child: Option<Ref>,
        sec: u32,
        off: u64,
        depth: i64,
        limit: i64,
    ) -> Result<Value<'a>, GrannyError> {
        let d = self.section(sec)?;
        let e = self.endian;
        let n = e.u32(d, off)?;
        let Some(p) = self.ptr(sec, off + 4) else {
            return Ok(Value::List(Vec::new()));
        };
        if n == 0 {
            return Ok(Value::List(Vec::new()));
        }
        let Some(c) = child else {
            return err("an array without a type");
        };
        if u64::from(n) > MAX_ITEMS {
            return err("an array of too many elements");
        }
        let ms = self.members(c)?;
        if let [m] = &ms[..]
            && is_scalar(m.kind)
            && m.arr == 0
        {
            let s = self.section(p.0)?;
            let w = type_size(m.kind);
            // struct.unpack_from of the whole array: all of it must be there
            if u64::from(p.1) + w * u64::from(n) > s.len() as u64 {
                return err("an array runs past its section");
            }
            let at = |k: u64| u64::from(p.1) + k * w;
            if matches!(m.kind, 10 | 21) {
                let mut out = Vec::with_capacity(n as usize);
                for k in 0..u64::from(n) {
                    if let Scalar::Float(x) = e.scalar(m.kind, s, at(k))? {
                        out.push(x);
                    }
                }
                return Ok(Value::Floats(out));
            }
            let mut out = Vec::with_capacity(n as usize);
            for k in 0..u64::from(n) {
                if let Scalar::Int(x) = e.scalar(m.kind, s, at(k))? {
                    out.push(x);
                }
            }
            return Ok(Value::Ints(out));
        }
        let sz = self.size(c)?;
        let mut out = Vec::with_capacity(n as usize);
        for k in 0..u64::from(n) {
            out.push(self.read(Some(c), Some((p.0, mul_add(k, sz, u64::from(p.1))?)), depth + 1, limit)?);
        }
        Ok(Value::List(out))
    }
}

/// A relocation target as an object position.
fn wide(r: Ref) -> (u32, u64) {
    (r.0, u64::from(r.1))
}

/// `k * size + base`, or an error past 64 bits.
fn mul_add(k: u64, size: u64, base: u64) -> Result<u64, GrannyError> {
    k.checked_mul(size)
        .and_then(|x| x.checked_add(base))
        .ok_or_else(|| GrannyError("an offset past 64 bits".into()))
}

/// `_strings_at`: a plausible string count at `p` and the first string `<file>`.
fn strings_at(b: &[u8], p: u64, e: Endian) -> bool {
    let (Ok(n), Ok(ln)) = (e.u32(b, p), e.u16(b, p + 4)) else {
        return false;
    };
    0 < n && n < 100_000 && slice(b, p + 6, p + 6 + u64::from(ln)) == b"<file>"
}

/// The layout of DICE's per-bone table (`bone_table`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableKind {
    /// A skeleton: no table.
    None,
    /// n records {bone, k, k x (start, end)}.
    Animation,
    /// n bones, then such records.
    Delta,
}

/// `_intervals`: the positions of the records of the count-prefixed list at `p`, and its end.
fn intervals(words: &[u32], mut p: usize) -> Result<(Vec<usize>, usize), GrannyError> {
    let Some(&n) = words.get(p) else {
        return err("DICE bone table: no count");
    };
    p += 1;
    let mut at = Vec::new();
    for _ in 0..n {
        if p + 2 > words.len() {
            return err("DICE bone table: a record runs past its end");
        }
        at.push(p);
        p += 2 + 2 * words.get(p + 1).copied().unwrap_or_default() as usize;
    }
    Ok((at, p))
}

/// Where DICE's per-bone table keeps bone indices (`bone_table`), from the u32 words before the string table.
///
/// # Errors
/// A table of no known layout.
pub fn bone_table(words: &[u32]) -> Result<(TableKind, Vec<usize>), GrannyError> {
    if words.len() == 1 {
        return Ok((TableKind::None, Vec::new()));
    }
    if words.len() < 2 {
        return err("DICE bone table: no words");
    }
    if let Ok((at, end)) = intervals(words, 1)
        && end == words.len()
    {
        return Ok((TableKind::Animation, at));
    }
    let n = words.get(1).copied().unwrap_or_default() as usize;
    if 2 + n < words.len() {
        let (at, end) = intervals(words, 2 + n)?;
        if end == words.len() {
            return Ok((TableKind::Delta, (2..2 + n).chain(at).collect()));
        }
    }
    err(format!("unknown DICE bone table: {} words, count {n}", words.len()))
}

/// DICE's table with its bones renumbered (`renumber`): a delta's bone list and the records each sorted by the new
/// bone index.
///
/// # Errors
/// A table of no known layout, or a bone missing from `bones`.
pub fn renumber<S: std::hash::BuildHasher>(
    words: &[u32],
    bones: &HashMap<u32, u32, S>,
) -> Result<Vec<u32>, GrannyError> {
    let (kind, _) = bone_table(words)?;
    let new = |i: u32| {
        bones.get(&i).copied().ok_or_else(|| {
            GrannyError(format!(
                "bone {i} of its table has no counterpart in the target skeleton"
            ))
        })
    };
    let records = |p: usize| -> Result<Vec<u32>, GrannyError> {
        let mut recs: Vec<Vec<u32>> = Vec::new();
        let mut q = p + 1;
        for _ in 0..words.get(p).copied().unwrap_or_default() {
            let k = words.get(q + 1).copied().unwrap_or_default() as usize;
            let mut r = vec![new(words.get(q).copied().unwrap_or_default())?, k as u32];
            r.extend_from_slice(slice_words(words, q + 2, q + 2 + 2 * k));
            recs.push(r);
            q += 2 + 2 * k;
        }
        recs.sort_by_key(|r| r.first().copied().unwrap_or_default());
        let mut out = vec![recs.len() as u32];
        out.extend(recs.into_iter().flatten());
        Ok(out)
    };
    let first = words.first().copied().unwrap_or_default();
    match kind {
        TableKind::None => Ok(words.to_vec()),
        TableKind::Animation => {
            let mut out = vec![first];
            out.extend(records(1)?);
            Ok(out)
        }
        TableKind::Delta => {
            let n = words.get(1).copied().unwrap_or_default() as usize;
            let recs = records(2 + n)?;
            let mut ids = slice_words(words, 2, 2 + n)
                .iter()
                .map(|&i| new(i))
                .collect::<Result<Vec<_>, _>>()?;
            ids.sort_unstable();
            let mut out = vec![first, n as u32];
            out.extend(ids);
            out.extend(recs);
            Ok(out)
        }
    }
}

fn slice_words(w: &[u32], from: usize, to: usize) -> &[u32] {
    let n = w.len();
    w.get(from.min(n)..to.min(n).max(from.min(n))).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn f(x: f32) -> u32 {
        x.to_bits()
    }

    #[test]
    fn tables() {
        assert_eq!(bone_table(&[10]).unwrap(), (TableKind::None, vec![]));
        let anim = [10, 2, 40, 1, f(0.1), f(0.9), 7, 2, f(0.0), f(0.2), f(0.5), f(0.7)];
        assert_eq!(bone_table(&anim).unwrap(), (TableKind::Animation, vec![2, 6]));
        let delta = [10, 3, 4, 2, 9, 1, 40, 1, f(0.0), f(1.0)];
        assert_eq!(bone_table(&delta).unwrap(), (TableKind::Delta, vec![2, 3, 4, 6]));
        assert_eq!(bone_table(&[10, 0]).unwrap(), (TableKind::Animation, vec![]));
        assert_eq!(bone_table(&[10, 2, 5, 6, 0]).unwrap(), (TableKind::Delta, vec![2, 3]));
        assert!(bone_table(&[10, 3, 1, 2]).is_err());
        let m = HashMap::from([(40, 3), (7, 50), (4, 9), (2, 1), (9, 2)]);
        assert_eq!(
            renumber(&anim, &m).unwrap(),
            vec![10, 2, 3, 1, f(0.1), f(0.9), 50, 2, f(0.0), f(0.2), f(0.5), f(0.7)]
        );
        assert_eq!(
            renumber(&delta, &m).unwrap(),
            vec![10, 3, 1, 2, 9, 1, 3, 1, f(0.0), f(1.0)]
        );
        assert!(renumber(&delta, &HashMap::from([(4, 1)])).is_err());
    }

    #[test]
    fn halves() {
        assert_eq!(half(0x3C00), 1.0);
        assert_eq!(half(0xC000), -2.0);
        assert_eq!(half(0x0001), 2f64.powi(-24));
        assert_eq!(half(0x7BFF), 65504.0);
        assert_eq!(half(0x7C00), f64::INFINITY);
        assert!(half(0x7E00).is_nan());
    }

    /// A PC resource of one section: a type {u8 Flag, i32 Count[2], f32 Scale} at 0 and its object at 128.
    fn one_section() -> Vec<u8> {
        let le = |v: &[u32]| v.iter().flat_map(|x| x.to_le_bytes()).collect::<Vec<u8>>();
        // three definitions and the zero type end at 128; the object (13 bytes, unaligned members) follows
        let mut sec = vec![0u8; 128];
        for (i, (t, nm, arr)) in [(12u32, 1u32, 0u32), (19, 2, 2), (10, 3, 0)].iter().enumerate() {
            sec[32 * i..32 * i + 16].copy_from_slice(&le(&[*t, *nm, 0, *arr]));
        }
        sec.push(1);
        sec.extend(le(&[7, 8, 2.5f32.to_bits()]));
        let soff = 40u32;
        let head = 72 + 44;
        let size = sec.len() as u32;
        let mut g = MAGIC.to_vec();
        g.extend(le(&[head, 0, 0, 0]));
        g.extend(le(&[7, head + size, 0, soff, 1, 0, 0, 0, 128, 0]));
        g.extend(le(&[
            0,
            head,
            size,
            size,
            4,
            size,
            size,
            head + size,
            0,
            head + size,
            0,
        ]));
        g.extend(&sec);
        let mut out = le(&[10, 4]);
        for s in ["<file>", "Flag", "Count", "Scale"] {
            out.extend((s.len() as u16).to_le_bytes());
            out.extend(s.as_bytes());
        }
        out.extend(le(&[g.len() as u32]));
        out.extend(g);
        out
    }

    #[test]
    fn member_offsets_and_another_copy() {
        let b = one_section();
        let g = Granny::parse(&b).unwrap();
        assert_eq!(g.member_offset((0, 0), b"Flag").unwrap(), Some(0));
        assert_eq!(g.member_offset((0, 0), b"Count").unwrap(), Some(1));
        assert_eq!(g.member_offset((0, 0), b"Scale").unwrap(), Some(9));
        assert_eq!(g.member_offset((0, 0), b"Nope").unwrap(), None);
        let Value::Object(o) = g.read(Some((0, 0)), Some((0, 128)), 0, 64).unwrap() else {
            panic!()
        };
        assert_eq!(o.fields[2].1, Value::Scalar(Scalar::Float(2.5)));
        // a write in place: the same tables over the written bytes read the new value
        let mut w = b.clone();
        let at = g.section_at[0] as usize + 128 + 9;
        w[at..at + 4].copy_from_slice(&(-1.0f32).to_le_bytes());
        let mut g2 = Granny::parse(&b).unwrap();
        g2.over(&w);
        let Value::Object(o) = g2.read(Some((0, 0)), Some((0, 128)), 0, 64).unwrap() else {
            panic!()
        };
        assert_eq!(o.fields[2].1, Value::Scalar(Scalar::Float(-1.0)));
        assert_eq!(g2.sections[0].len(), g.sections[0].len());
    }

    #[test]
    fn refuses_what_is_not_granny() {
        assert!(Granny::parse(b"").is_err());
        assert!(Granny::parse(&[11, 0, 0, 0]).is_err());
        assert!(Granny::parse(&[10, 0, 0, 0, 1, 0, 0, 0]).is_err());
    }
}
