//! Xbox 360 -> PC Havok 5.5.0 packfiles, ported from `bfbc2-console-convert/havok.py` (`Packfile.convert`; the
//! format: that folder's `FORMAT.md` section 1): the packfile's objects converted field by field from the class
//! layouts the game carries (`havok.classes(exe)`, handed over as [`Classes`]), each value's bytes reversed by its
//! width.
//!
//! | Python (`havok.py`) | here |
//! |---|---|
//! | `Packfile(data, start, classes).convert()` | [`convert_packfile`] |
//! | `Klass`, `Member` (the `classes` dict) | [`Klass`], [`Member`], [`Classes`] |
//! | `ConvertError` | [`HavokError::Refused`] (its message) |
//!
//! The walk is the reference's, step for step: the same order of objects, members and pointers followed, the same
//! record of which bytes were converted as which width and for which member (`claim` / `owner`: a byte converted
//! twice the same way is converted once, as two widths it is refused), so the bytes are the reference's. Where the
//! reference would do something other than convert or refuse - read past the data (a short slice, a `struct.error`),
//! look up a class or type it has no entry for (a `KeyError`), nest deeper than Python's recursion allows - the
//! walk stops with [`HavokError::Reference`]. The reference was retired on 2026-11-18 (its results recorded in
//! `bfbc2-bf1943-import/tests/ports_golden.json`): `havok.py` now raises its `ConvertError` for those, saying the
//! packfile reads past its data or names what the layouts lack. [`verify_packfile`] is `havok.verify`'s packfile
//! part (the per-byte reasons a conversion differs from a PC file).
//!
//! ```
//! use bc2_convert::havok::{self, Classes, HavokError};
//!
//! // not a packfile: refused, as the reference refuses it
//! let classes = Classes::new(&[]);
//! assert!(matches!(havok::convert_packfile(&[0u8; 64], 0, &classes), Err(HavokError::Refused(_))));
//! ```

use std::collections::{HashMap, HashSet};

use thiserror::Error;

/// The packfile magic, the same bytes on both platforms.
pub const MAGIC: [u8; 8] = [0x57, 0xE0, 0xE0, 0x57, 0x10, 0xC0, 0xC0, 0x10];

// hkClassMember types (havok.py's names)
const VOID: u8 = 0;
const ZERO: u8 = 19;
const POINTER: u8 = 20;
const ARRAY: u8 = 22;
const INPLACEARRAY: u8 = 23;
const ENUM: u8 = 24;
const STRUCT: u8 = 25;
const SIMPLEARRAY: u8 = 26;
const HOMOGENEOUSARRAY: u8 = 27;
const VARIANT: u8 = 28;
const FLAGS: u8 = 31;

/// `havok.py`'s `PLAIN`: (size, width of each value) of the member types with a fixed size.
#[must_use]
pub const fn plain(ty: u8) -> Option<(usize, usize)> {
    match ty {
        1..=4 => Some((1, 1)),      // BOOL, CHAR, INT8, UINT8
        5 | 6 | 32 => Some((2, 2)), // INT16, UINT16, HALF
        // INT32, UINT32, REAL, ULONG; POINTER, FUNCTIONPOINTER, CSTRING, STRINGPTR
        7 | 8 | 11 | 30 | 20 | 21 | 29 | 33 => Some((4, 4)),
        9 | 10 => Some((8, 8)),   // INT64, UINT64
        12 | 13 => Some((16, 4)), // VECTOR4, QUATERNION
        14..=16 => Some((48, 4)), // MATRIX3, ROTATION, QSTRANSFORM
        17 | 18 => Some((64, 4)), // MATRIX4, TRANSFORM
        _ => None,
    }
}

/// Why a packfile is not converted here.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum HavokError {
    /// The reference's `ConvertError`, with its message.
    #[error("{0}")]
    Refused(String),
    /// The retired reference would not have refused it this way (a short read, a missing table entry, deep
    /// nesting): not converted.
    #[error("not converted (it reads past the data, names what the layouts lack or nests too deep)")]
    Reference,
}

type Result<T> = std::result::Result<T, HavokError>;

fn refused<T>(msg: String) -> Result<T> {
    Err(HavokError::Refused(msg))
}

/// One hkClassMember of a class layout (`havok.Member`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The member's name (`None`: the executable's string was not printable).
    pub name: Option<String>,
    /// The class of a struct / pointer / array member, by name.
    pub cls: Option<String>,
    /// hkClassMember type.
    pub ty: u8,
    /// Its subtype (an array's or pointer's element type, an enum's storage).
    pub subtype: u8,
    /// The C array size (0: not an array).
    pub carray: i16,
    /// The offset in the object.
    pub offset: u32,
}

/// One class layout (`havok.Klass`): parent by name, size, its own members.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Klass {
    /// The name it is looked up by (the `classes` dict's key).
    pub name: String,
    /// The parent class, by name.
    pub parent: Option<String>,
    /// The object size.
    pub size: u32,
    /// The class's own members (the parent's come first in a walk).
    pub members: Vec<Member>,
}

#[derive(Debug, Clone)]
struct Flat {
    cls: Option<usize>,
    cls_name: Option<String>,
    ty: u8,
    subtype: u8,
    carray: i16,
    offset: u32,
    note: u32,
    name: Option<String>,
}

type NoteKey = (String, Option<String>);

/// The class layouts by name, each class's members with its parents' flattened and resolved once.
#[derive(Debug, Clone)]
pub struct Classes {
    by_name: HashMap<String, usize>,
    names: Vec<String>,
    sizes: Vec<u32>,
    flat: Vec<Vec<Flat>>,
    notes: HashMap<NoteKey, u32>,
}

impl Classes {
    /// The layouts (`havok.classes(exe)`'s values, each under its key).
    #[must_use]
    pub fn new(klasses: &[Klass]) -> Self {
        let mut by_name = HashMap::new();
        for (i, k) in klasses.iter().enumerate() {
            by_name.insert(k.name.clone(), i);
        }
        let mut notes: HashMap<NoteKey, u32> = HashMap::new();
        let mut flat = Vec::with_capacity(klasses.len());
        for k in klasses {
            // the chain as _members walks it: the class, its parent while the parent is known; root first
            let mut chain = vec![k];
            let mut seen = HashSet::from([k.name.as_str()]);
            let mut cur = k;
            while let Some(p) = cur
                .parent
                .as_ref()
                .and_then(|p| by_name.get(p))
                .and_then(|&i| klasses.get(i))
            {
                if !seen.insert(p.name.as_str()) {
                    break; // a cycle: the reference would loop for ever; never in the game's layouts
                }
                chain.push(p);
                cur = p;
            }
            let mut members = Vec::new();
            for c in chain.iter().rev() {
                for m in &c.members {
                    let n = notes.len() as u32;
                    let note = *notes.entry((k.name.clone(), m.name.clone())).or_insert(n);
                    members.push(Flat {
                        cls: m.cls.as_ref().and_then(|c| by_name.get(c).copied()),
                        cls_name: m.cls.clone(),
                        ty: m.ty,
                        subtype: m.subtype,
                        carray: m.carray,
                        offset: m.offset,
                        note,
                        name: m.name.clone(),
                    });
                }
            }
            flat.push(members);
        }
        Self {
            by_name,
            names: klasses.iter().map(|k| k.name.clone()).collect(),
            sizes: klasses.iter().map(|k| k.size).collect(),
            flat,
            notes,
        }
    }

    /// How many classes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }
}

/// Deeper than this many nested objects, the reference's Python recursion (three frames an object) may run out:
/// the walk leaves it to the reference.
const MAX_DEPTH: usize = 150;

struct Walk<'a> {
    src: &'a [u8],
    out: Vec<u8>,
    p0: usize,
    k: &'a Classes,
    claim: Vec<u8>,
    owner: Vec<u32>,
    seen: HashSet<(usize, usize)>,
    note: u32,
    extra_notes: HashMap<NoteKey, u32>,
    cur: Option<usize>,
    names: Option<HashMap<usize, String>>,
    local: HashMap<usize, usize>,
    glob: HashMap<usize, (usize, usize)>,
    base: usize,
    secindex: usize,
    depth: usize,
    /// `Packfile.arrays`' (member name, data start, count), in order, when recorded.
    arrays: Option<Vec<(Option<String>, usize, usize)>>,
}

const NO_OWNER: u32 = u32::MAX;

impl Walk<'_> {
    fn note_of(&mut self, a: &str, b: &str) -> u32 {
        let key = (a.to_owned(), Some(b.to_owned()));
        if let Some(&n) = self.k.notes.get(&key) {
            return n;
        }
        let n = (self.k.notes.len() + self.extra_notes.len()) as u32;
        *self.extra_notes.entry(key).or_insert(n)
    }

    fn be32(&self, at: usize) -> Result<usize> {
        let b = self
            .src
            .get(at..at.checked_add(4).ok_or(HavokError::Reference)?)
            .ok_or(HavokError::Reference)?;
        let b: [u8; 4] = b.try_into().map_err(|_| HavokError::Reference)?;
        Ok(u32::from_be_bytes(b) as usize)
    }

    fn swap(&mut self, at: usize, width: usize, count: usize) -> Result<()> {
        for k in 0..count {
            let o = at + k * width;
            let end = o + width;
            // past the data the reference's slices would come short: its own walk decides
            let (Some(claim), Some(owner), Some(out), Some(src)) = (
                self.claim.get_mut(o..end),
                self.owner.get_mut(o..end),
                self.out.get_mut(o..end),
                self.src.get(o..end),
            ) else {
                return Err(HavokError::Reference);
            };
            let first = claim.first().copied().unwrap_or(0);
            if first as usize == width && owner.first() == Some(&self.note) {
                continue;
            }
            if let Some((i, &c)) = claim.iter().enumerate().find(|&(_, &c)| c != 0 && c as usize != width) {
                return refused(format!(
                    "byte {:#x} converted as a {}-byte and a {}-byte value",
                    o + i - self.p0,
                    c,
                    width
                ));
            }
            if first == 0 {
                for (d, s) in out.iter_mut().zip(src.iter().rev()) {
                    *d = *s;
                }
            }
            claim.fill(width as u8);
            owner.fill(self.note);
        }
        Ok(())
    }

    /// `_entries`: the offsets of a fixup table's rows; the padding after them must be 0xFF.
    fn entries(&self, a: usize, end: usize, size: usize) -> Result<Vec<usize>> {
        let mut out = Vec::new();
        let mut o = a;
        while o + size <= end && self.be32(o)? != 0xFFFF_FFFF {
            out.push(o);
            o += size;
        }
        let tail = self
            .src
            .get(o.min(self.src.len())..end.min(self.src.len()))
            .unwrap_or(&[]);
        if tail.iter().any(|&x| x != 0xFF) {
            return refused(format!(
                "a fixup table has data after its 0xFF padding at {:#x}",
                o - self.p0
            ));
        }
        Ok(out)
    }

    fn classnames(&mut self, a: usize, end: usize) -> Result<()> {
        let mut names = HashMap::new();
        self.note = self.note_of("class names", "");
        let mut o = a;
        while o + 5 <= end && *self.src.get(o + 4).ok_or(HavokError::Reference)? == 0x09 {
            let rest = self.src.get(o + 5..).ok_or(HavokError::Reference)?;
            let len = rest.iter().position(|&c| c == 0).ok_or(HavokError::Reference)?;
            names.insert(o + 5 - a, rest.iter().take(len).map(|&c| char::from(c)).collect());
            let e = o + 5 + len;
            self.swap(o, 4, 1)?;
            o = e + 1;
        }
        self.names = Some(names);
        let tail = self
            .src
            .get(o.min(self.src.len())..end.min(self.src.len()))
            .unwrap_or(&[]);
        if tail.iter().any(|&x| x != 0xFF) {
            return refused("class-name section: unknown data after the names".to_owned());
        }
        Ok(())
    }

    fn target(&self, at: usize) -> Option<usize> {
        if let Some(&t) = self.local.get(&at) {
            return Some(t);
        }
        match self.glob.get(&at) {
            Some(&(sec, off)) if sec == self.secindex => Some(off),
            _ => None,
        }
    }

    fn class_index(&self, name: &str) -> Result<usize> {
        match self.k.by_name.get(name) {
            Some(&i) => Ok(i),
            None => refused(format!("class {name} is not among the game's Havok classes")),
        }
    }

    fn strukt(&mut self, cls: usize, off: usize) -> Result<()> {
        if !self.seen.insert((off, cls)) {
            return Ok(());
        }
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(HavokError::Reference);
        }
        let prev = self.cur.replace(cls);
        let k = self.k;
        for m in k.flat.get(cls).ok_or(HavokError::Reference)? {
            self.note = m.note;
            self.member(m, off.checked_add(m.offset as usize).ok_or(HavokError::Reference)?)?;
        }
        self.cur = prev;
        self.depth -= 1;
        Ok(())
    }

    fn ctx(&self, m: &Flat) -> String {
        let cur = self
            .cur
            .and_then(|c| self.k.names.get(c))
            .map_or("None", String::as_str);
        format!("{cur}.{}", m.name.as_deref().unwrap_or("None"))
    }

    fn size_of(&self, ty: u8, cls: Option<usize>) -> Result<usize> {
        if ty == STRUCT {
            // classes[cls].size: a KeyError in the reference when the class is unknown
            return cls
                .and_then(|c| self.k.sizes.get(c))
                .map(|&s| s as usize)
                .ok_or(HavokError::Reference);
        }
        plain(ty).map(|(s, _)| s).ok_or(HavokError::Reference)
    }

    /// `_value`: one value of type `ty` at section offset `at`; its size.
    fn value(&mut self, ty: u8, sub: u8, m: &Flat, at: usize) -> Result<usize> {
        let a = self.base.checked_add(at).ok_or(HavokError::Reference)?;
        if ty == ENUM || ty == FLAGS {
            let (size, w) = plain(sub).ok_or(HavokError::Reference)?;
            self.swap(a, w, size / w)?;
            return Ok(size);
        }
        if ty == STRUCT {
            let Some(c) = m.cls else {
                if m.cls_name.is_some() {
                    return Err(HavokError::Reference); // its name is the reference's to print
                }
                return refused(format!("{}: struct class None unknown", self.ctx(m)));
            };
            let note = self.note;
            self.strukt(c, at)?;
            self.note = note;
            return Ok(self.k.sizes.get(c).copied().unwrap_or(0) as usize);
        }
        if ty == ZERO {
            return self.value(sub, VOID, m, at);
        }
        if ty == VARIANT {
            self.swap(a, 4, 2)?;
            return Ok(8);
        }
        let Some((size, w)) = plain(ty) else {
            return refused(format!("{}: member type {ty} not handled", self.ctx(m)));
        };
        self.swap(a, w, size / w)?;
        if ty == POINTER
            && sub == STRUCT
            && m.cls_name.as_ref().is_some_and(|c| !c.is_empty())
            && let Some(t) = self.target(at)
        {
            {
                // _struct(cls, t): a class the layouts lack is the reference's ConvertError
                let Some(c) = m.cls else {
                    return Err(HavokError::Reference);
                };
                let note = self.note;
                self.strukt(c, t)?;
                self.note = note;
            }
        }
        Ok(size)
    }

    /// `_member`: a member at section offset `at` (each of a C array's elements; an array's elements, through its
    /// fixup).
    fn member(&mut self, m: &Flat, at: usize) -> Result<()> {
        let n = usize::try_from(m.carray.max(1)).unwrap_or(1);
        if m.ty == ARRAY || m.ty == SIMPLEARRAY {
            let head = if m.ty == ARRAY { 12 } else { 8 };
            for k in 0..n {
                let a = at + k * head;
                let count = self.be32(self.base + a + 4)?;
                self.swap(self.base + a, 4, head / 4)?;
                if count == 0 || m.subtype == VOID {
                    continue;
                }
                let Some(t) = self.target(a) else {
                    return refused(format!("{}: an array of {count} without a fixup", self.ctx(m)));
                };
                let es = self.size_of(m.subtype, m.cls)?;
                if let Some(arrays) = self.arrays.as_mut() {
                    arrays.push((m.name.clone(), self.base + t, count));
                }
                let sub = if m.subtype == POINTER { STRUCT } else { VOID };
                for e in 0..count {
                    self.value(m.subtype, sub, m, t + e * es)?;
                }
            }
            return Ok(());
        }
        if m.ty == HOMOGENEOUSARRAY || m.ty == INPLACEARRAY {
            return refused(format!("{}: member type {} not handled", self.ctx(m), m.ty));
        }
        let size = self.value(m.ty, m.subtype, m, at)?;
        for k in 1..n {
            self.value(m.ty, m.subtype, m, at + k * size)?;
        }
        Ok(())
    }

    fn convert(&mut self) -> Result<()> {
        let p = self.p0;
        let src = self.src;
        let slice = |a: usize, b: usize| src.get(a.min(src.len())..b.min(src.len())).unwrap_or(&[]);
        if slice(p, p + 8) != MAGIC {
            return refused("no packfile magic".to_owned());
        }
        let rules = slice(p + 16, p + 20);
        if rules != [4, 0, 0, 1] {
            let hex = rules.iter().fold(String::new(), |mut h, b| {
                use std::fmt::Write as _;
                let _ = write!(h, "{b:02x}");
                h
            });
            return refused(format!("layout rules {hex}, not the Xbox 360's 04 00 00 01"));
        }
        if !slice(p + 40, p + 56).starts_with(b"Havok-5.5.0") {
            return Err(HavokError::Reference); // refused, its message quoting bytes as Python prints them
        }
        self.note = self.note_of("header", "");
        self.swap(p + 8, 4, 2)?;
        *self.out.get_mut(p + 17).ok_or(HavokError::Reference)? = 1;
        let nsec = self.be32(p + 20)?;
        self.swap(p + 20, 4, 5)?;
        self.swap(p + 56, 4, 2)?;
        let mut sections = Vec::new();
        for i in 0..nsec {
            let h = p + 64 + 48 * i;
            let raw = slice(h, h + 19);
            let tag: String = raw
                .split(|&c| c == 0)
                .next()
                .unwrap_or(&[])
                .iter()
                .map(|&c| char::from(c))
                .collect();
            let mut v = [0usize; 7];
            for (j, x) in v.iter_mut().enumerate() {
                *x = self.be32(h + 20 + 4 * j)?;
            }
            sections.push((tag, v));
            self.swap(h + 20, 4, 7)?;
        }
        for (i, (tag, [start, loc, glob, virt, exp, imp, end])) in sections.iter().enumerate() {
            let base = p + start;
            if tag == "__classnames__" {
                self.classnames(base, base + loc)?;
                continue;
            }
            if loc == end {
                continue;
            }
            if exp != end || imp != end {
                return refused(format!("{tag}: exports or imports (not handled)"));
            }
            self.note = self.note_of("fixups", tag);
            self.local.clear();
            self.glob.clear();
            for o in self.entries(base + loc, base + glob, 8)? {
                self.swap(o, 4, 2)?;
                let (s, t) = (self.be32(o)?, self.be32(o + 4)?);
                self.local.insert(s, t);
            }
            for o in self.entries(base + glob, base + virt, 12)? {
                self.swap(o, 4, 3)?;
                let (s, sec, t) = (self.be32(o)?, self.be32(o + 4)?, self.be32(o + 8)?);
                self.glob.insert(s, (sec, t));
            }
            let mut objs = Vec::new();
            for o in self.entries(base + virt, base + exp, 12)? {
                self.swap(o, 4, 3)?;
                let (sec, off) = (self.be32(o + 4)?, self.be32(o + 8)?);
                let (stag, _) = sections.get(sec).ok_or(HavokError::Reference)?;
                // the reference's `or`: another section is refused before the class names are looked at
                let name = if stag == "__classnames__" {
                    self.names.as_ref().ok_or(HavokError::Reference)?.get(&off)
                } else {
                    None
                };
                let Some(name) = name else {
                    return refused(format!(
                        "{tag}: an object's class name is not in the class-name section"
                    ));
                };
                objs.push((self.be32(o)?, name.clone()));
            }
            self.base = base;
            self.secindex = i;
            for (off, name) in objs {
                let c = self.class_index(&name)?;
                self.strukt(c, off)?;
            }
        }
        Ok(())
    }
}

/// `havok.Packfile(data, start, classes).convert()`: `data` with the packfile at `start` converted to the PC's
/// byte order (the same length; the wrapper before it left as it is).
///
/// # Errors
///
/// [`HavokError::Refused`] where the reference raises its `ConvertError`; [`HavokError::Reference`] where it would
/// do something else (the module's docs): run the reference.
pub fn convert_packfile(data: &[u8], start: usize, classes: &Classes) -> Result<Vec<u8>> {
    let mut w = Walk::new(data, start, classes, false);
    w.convert()?;
    Ok(w.out)
}

impl<'a> Walk<'a> {
    fn new(data: &'a [u8], start: usize, classes: &'a Classes, record_arrays: bool) -> Self {
        Walk {
            src: data,
            out: data.to_vec(),
            p0: start,
            k: classes,
            claim: vec![0; data.len()],
            owner: vec![NO_OWNER; data.len()],
            seen: HashSet::new(),
            note: NO_OWNER,
            extra_notes: HashMap::new(),
            cur: None,
            names: None,
            local: HashMap::new(),
            glob: HashMap::new(),
            base: 0,
            secindex: 0,
            depth: 0,
            arrays: record_arrays.then(Vec::new),
        }
    }
}

/// Why a converted byte may differ from DICE's PC file although the conversion is right (`havok.ignorable`'s
/// reasons; FORMAT.md 1.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// "triangle padding index"
    Padding,
    /// "cached bounding volume marked invalid"
    Bounds,
    /// "triangle offset (computed at load)"
    TriangleOffset,
    /// "saved quality type without a saved motion"
    SavedQuality,
}

impl Why {
    /// The reference's text.
    #[must_use]
    pub const fn text(self) -> &'static str {
        match self {
            Self::Padding => "triangle padding index",
            Self::Bounds => "cached bounding volume marked invalid",
            Self::TriangleOffset => "triangle offset (computed at load)",
            Self::SavedQuality => "saved quality type without a saved motion",
        }
    }
}

const MOTIONS: [&str; 3] = ["hkpMaxSizeMotion", "hkpKeyframedRigidMotion", "hkpMotion"];
const BOUNDS: &str = "hkpCollidableBoundingVolumeData";

/// `havok.verify`'s packfile part: the packfile at `start` of `console` converted, and every byte from `start` to
/// the end of `pc` that differs from `pc` with its reason (`havok.ignorable`, from the walk's record of each byte's
/// owner, the arrays and the objects), or `None` for a difference it does not explain - in offset order.
///
/// # Errors
///
/// As [`convert_packfile`]; and [`HavokError::Reference`] where the reference's answer is not this one's to give:
/// `pc` longer than `console` (its `IndexError`), a bounding-volume read past the data (its `struct.error`), or a
/// byte inside two bounding-volume objects whose answers differ (the reference takes the first its set yields).
pub fn verify_packfile(
    console: &[u8],
    start: usize,
    classes: &Classes,
    pc: &[u8],
) -> Result<Vec<(usize, Option<Why>)>> {
    if pc.len() > console.len() {
        return Err(HavokError::Reference);
    }
    let mut w = Walk::new(console, start, classes, true);
    w.convert()?;
    let mut names: Vec<Option<(&str, Option<&str>)>> = vec![None; classes.notes.len() + w.extra_notes.len()];
    for ((a, b), &n) in classes.notes.iter().chain(w.extra_notes.iter()) {
        if let Some(slot) = names.get_mut(n as usize) {
            *slot = Some((a.as_str(), b.as_deref()));
        }
    }
    // the objects by class name, as (absolute start, section offset) in pf.seen
    let seen: Vec<(usize, &str)> = w
        .seen
        .iter()
        .map(|&(off, c)| (off, classes.names.get(c).map_or("", String::as_str)))
        .collect();
    let arrays = w.arrays.as_deref().unwrap_or(&[]);
    let u32le = |b: &[u8], at: usize| -> Result<u32> {
        b.get(at..at + 4)
            .and_then(|x| x.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or(HavokError::Reference)
    };
    let mut out = Vec::new();
    for (i, (&a, &b)) in w.out.iter().zip(pc).enumerate().skip(start) {
        if a == b {
            continue;
        }
        let owner = w.owner.get(i).and_then(|&n| names.get(n as usize).copied().flatten());
        let (cls, mem) = owner.unwrap_or(("", None));
        let mut why = None;
        if owner.is_some() && cls == "hkpStorageExtendedMeshShapeMeshSubpartStorage" && mem == Some("indices16") {
            if let Some(&(_, st, _)) = arrays
                .iter()
                .find(|(m, st, n)| m.as_deref() == Some("indices16") && *st <= i && i < st + 2 * n)
            {
                why = ((i - st) / 2 % 4 == 3).then_some(Why::Padding);
            }
        } else if owner.is_some() && cls == BOUNDS {
            let mut answer: Option<Option<Why>> = None;
            for &(off, name) in &seen {
                let base_a = w.base + off;
                if name == BOUNDS && base_a <= i && i < base_a + 0x20 {
                    let ours = u32le(&w.out, base_a)? > u32le(&w.out, base_a + 0x10)?;
                    let theirs = u32le(pc, base_a)? > u32le(pc, base_a + 0x10)?;
                    let marker = (base_a..base_a + 4).contains(&i) || (base_a + 0x10..base_a + 0x14).contains(&i);
                    let this = (ours && theirs && !marker).then_some(Why::Bounds);
                    if answer.is_some_and(|x| x != this) {
                        return Err(HavokError::Reference);
                    }
                    answer = Some(this);
                }
            }
            why = answer.flatten();
        } else if owner.is_some() && cls == "hkpExtendedMeshShapeTrianglesSubpart" && mem == Some("triangleOffset") {
            why = Some(Why::TriangleOffset);
        } else if owner.is_some() && MOTIONS.contains(&cls) && mem == Some("savedQualityTypeIndex") {
            let hit = seen.iter().any(|&(off, _)| {
                let base_a = w.base + off;
                base_a + 0x11C <= i
                    && i < base_a + 0x11E
                    && !w.local.contains_key(&(off + 0x118))
                    && !w.glob.contains_key(&(off + 0x118))
            });
            why = hit.then_some(Why::SavedQuality);
        }
        out.push((i, why));
    }
    Ok(out)
}

/// Why DICE's wrapper is not converted here ([`convert_wrapper`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WrapperError {
    /// `havok.py`'s `ConvertError`, with its message.
    Refused(String),
    /// A read past the data (the reference's `struct.error`) or a float `struct` would not give back as it was (a
    /// NaN): the reference runs.
    Reference,
}

/// DICE's wrapper before the packfile at `o`, console -> PC (`havok.convert_wrapper`, round F2a; `FORMAT.md` 1.1):
/// a count, the part words (a blueprint's part-name hashes, a model group's owner leaves: spec HAVOK-WRAP-1), `a` vectors and `c` boxes swapped field by field, the byte run as it is, then 1,
/// `k` and `k` hashes ending at the packfile (the byte run's end: the one place where that holds).
///
/// # Errors
/// See [`WrapperError`].
#[allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    reason = "each slice is cut by get() first; the reference's names"
)]
pub fn convert_wrapper(b: &[u8], o: usize) -> std::result::Result<Vec<u8>, WrapperError> {
    use WrapperError::Reference;
    let word = |at: usize| -> std::result::Result<u32, WrapperError> {
        let s = b.get(at..at.checked_add(4).ok_or(Reference)?).ok_or(Reference)?;
        Ok(u32::from_be_bytes([s[0], s[1], s[2], s[3]]))
    };
    let half = |at: usize| -> std::result::Result<u16, WrapperError> {
        let s = b.get(at..at.checked_add(2).ok_or(Reference)?).ok_or(Reference)?;
        Ok(u16::from_be_bytes([s[0], s[1]]))
    };
    let mut out = Vec::new();
    let mut p = 0usize;
    // num('I' / 'H' / 'f', n): n values swapped
    let num = |out: &mut Vec<u8>, width: usize, float: bool, n: u64, p: &mut usize| {
        for _ in 0..n {
            let s = b.get(*p..*p + width).ok_or(Reference)?;
            if float && f32::from_bits(u32::from_be_bytes([s[0], s[1], s[2], s[3]])).is_nan() {
                return Err(Reference);
            }
            out.extend(s.iter().rev());
            *p += width;
        }
        Ok(())
    };
    let n = word(0)?;
    num(&mut out, 4, false, 2, &mut p)?;
    num(&mut out, 4, false, u64::from(word(4)?), &mut p)?;
    let a = half(p)?;
    num(&mut out, 2, false, 1, &mut p)?;
    num(&mut out, 4, true, 3 * u64::from(a), &mut p)?;
    let c = half(p)?;
    num(&mut out, 2, false, 1, &mut p)?;
    num(&mut out, 4, true, 6 * u64::from(c), &mut p)?;
    let mut tail = Vec::new();
    for q in p..o.saturating_sub(7) {
        if word(q)? == 1 && (q + 8) as u64 + 4 * u64::from(word(q + 4)?) == o as u64 {
            tail.push(q);
        }
    }
    let [t] = tail[..] else {
        return Err(WrapperError::Refused(format!(
            "wrapper: the end of the byte run is not unique ({} candidates; {n} parts)",
            tail.len()
        )));
    };
    out.extend_from_slice(b.get(p..t).unwrap_or_default());
    p = t;
    let k = word(p + 4)?;
    num(&mut out, 4, false, 2 + u64::from(k), &mut p)?;
    Ok(out)
}

/// `havok.classes(exe)` as the converter's [`Classes`] (round OR11): the executable's hkClass rows
/// ([`crate::hkclasses::classes`]) in the reference's dict (by name: the first place, the last value).
#[must_use]
pub fn classes_of(rows: Vec<crate::hkclasses::Klass>) -> Classes {
    let mut out: Vec<Klass> = Vec::new();
    for (name, parent, size, members) in rows {
        let k = Klass {
            name: name.clone(),
            parent,
            size,
            members: members
                .into_iter()
                .map(|(name, cls, ty, subtype, carray, _flags, offset)| Member {
                    name,
                    cls,
                    ty,
                    subtype,
                    carray,
                    offset: u32::from(offset),
                })
                .collect(),
        };
        if let Some(slot) = out.iter_mut().find(|x| x.name == name) {
            *slot = k;
        } else {
            out.push(k);
        }
    }
    Classes::new(&out)
}

/// `havok.convert(b, classes)` (round OR11): a console `HavokPhysicsData` resource as the PC's (the same size) - DICE's
/// wrapper ([`convert_wrapper`]) and the packfile after it ([`convert_packfile`]).
///
/// # Errors
/// The reference's `ConvertError` message: no packfile, a wrapper cut short or damaged, a packfile not converted.
pub fn convert(b: &[u8], classes: &Classes) -> std::result::Result<Vec<u8>, String> {
    let o = b
        .windows(MAGIC.len())
        .position(|w| w == MAGIC)
        .ok_or_else(|| "no Havok packfile in the resource".to_owned())?;
    let w = match convert_wrapper(b, o) {
        Ok(w) => w,
        Err(WrapperError::Refused(m)) => return Err(m),
        Err(WrapperError::Reference) => {
            return Err(
                "wrapper: cut short or damaged (a read past the data, or a value the converter does not take)".into(),
            );
        }
    };
    let out = match convert_packfile(b, o, classes) {
        Ok(out) => out,
        Err(HavokError::Refused(m)) => return Err(m),
        Err(HavokError::Reference) => {
            let v = b.get(o + 40..(o + 56).min(b.len())).unwrap_or_default();
            return Err(if v.starts_with(b"Havok-5.5.0") {
                "the packfile is not converted: it reads past the end of the data, names a class or member type the \
                 game's layouts lack, or nests objects too deeply"
                    .to_owned()
            } else {
                format!("Havok version {}, not 5.5.0", crate::meshconv::bytes_repr(v))
            });
        }
    };
    let mut r = w;
    r.extend_from_slice(out.get(o..).unwrap_or_default());
    Ok(r)
}

#[cfg(test)]
#[path = "havok_tests.rs"]
mod tests;
