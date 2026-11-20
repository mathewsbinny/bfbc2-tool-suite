//! Havok binary packfiles (32-bit, little-endian, as BC2's `*_Havok.res` / `*_HavokAsset.res` hold them) and the
//! collision triangles of a blueprint, ported from NavKit's `bfbc2-auto-navkit/tools/hkpack.py` (the packfile) and
//! `levelobjects.triangles` (the triangles the level loader and NavKit use).
//!
//! A packfile is found by its magic anywhere in the file. Its sections carry three fixup tables: local and global
//! pointers (a pointer slot and its target) and virtual fixups (an object and its class name). The `hkClass`
//! objects in the file describe the classes (name, parent, size, members); they are read as the reference reads
//! them, so the same damaged files are refused.
//!
//! | Python | here |
//! |---|---|
//! | `hkpack.Packfile(data)` / `hkpack.load(path)` | [`Packfile::parse`] |
//! | `Packfile.deref(slot)`, `.objects`, `.objects_of(name)`, `.classes` | [`Packfile::deref`], [`Packfile::objects`], [`Packfile::objects_of`], [`Packfile::classes`] |
//! | `levelobjects.triangles(path)` | [`triangles`] |
//! | | [`build`]: a small test writer |
//!
//! The reference's outcomes map to this module's as follows: a file `hkpack.load` refuses with a `ValueError`
//! (no packfile, a name that is not UTF-8 or has no end) is [`HavokError::NotPackfile`] / [`HavokError::Refused`],
//! for which [`triangles`] gives `Ok(None)` as `levelobjects.triangles` gives `None`; a file whose fields or arrays
//! run past its end, or that points at a section it does not have, raised `struct.error`, `IndexError`,
//! `TypeError` or NumPy's `ValueError` in the reference and is [`HavokError::Damaged`] here.
//!
//! ```
//! use bc2_formats::havok;
//!
//! assert!(matches!(havok::Packfile::parse(b"no packfile"), Err(havok::HavokError::NotPackfile)));
//! assert_eq!(havok::triangles(b"no packfile")?, None);
//! # Ok::<(), havok::HavokError>(())
//! ```

use std::collections::HashMap;

use thiserror::Error;

use crate::bytes;

pub mod reflect;

/// The packfile magic: the file is read from its first occurrence.
pub const MAGIC: [u8; 8] = [0x57, 0xe0, 0xe0, 0x57, 0x10, 0xc0, 0xc0, 0x10];

/// The class of a storage extended mesh's subpart: its vertices and 16-bit indices.
pub const MESH_SUBPART: &str = "hkpStorageExtendedMeshShapeMeshSubpartStorage";
/// The class of a convex shape (boxes, rails, doors): its vertices, as `FourVector`s.
pub const CONVEX_VERTICES: &str = "hkpConvexVerticesShape";

/// Why a packfile could not be read.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum HavokError {
    /// The file holds no packfile magic.
    #[error("no Havok packfile")]
    NotPackfile,
    /// A name the packfile points at has no end, or a section name is not UTF-8 (the reference's `ValueError` while
    /// loading).
    #[error("refused: {0}")]
    Refused(String),
    /// A field or array runs past the end of the file, a fixup names a section the file does not have, or an
    /// `hkClass` has members but no member array (the reference's `struct.error`, `IndexError`, `TypeError`, and
    /// NumPy's `ValueError` for an array past the end or not of whole index groups).
    #[error("damaged packfile: {0}")]
    Damaged(String),
}

/// The result type of this module.
pub type Result<T, E = HavokError> = std::result::Result<T, E>;

/// A packfile section's header (offsets relative to the section's start, `start` absolute).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The section's name (`__classnames__`, `__types__`, `__data__`).
    pub name: String,
    /// The section's start in the file (absolute).
    pub start: u64,
    /// Where the local fixups start.
    pub local: u32,
    /// Where the global fixups start (and the local ones end).
    pub global: u32,
    /// Where the virtual fixups start.
    pub virtual_fixups: u32,
    /// Where the exports start (and the virtual fixups end).
    pub exports: u32,
    /// Where the imports start.
    pub imports: u32,
    /// The section's end.
    pub end: u32,
}

/// A member of an `hkClass` as the file describes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassMember {
    /// The member's name.
    pub name: String,
    /// Where the member's class description is (absolute), if it has one.
    pub class: Option<u64>,
    /// `hkClassMember::Type`.
    pub kind: u8,
    /// The element type of an array or pointer.
    pub subtype: u8,
    /// The C array length (0: not an array).
    pub carray: i16,
    /// The member's flags.
    pub flags: u16,
    /// The member's offset in its object.
    pub offset: u16,
}

/// An `hkClass` description.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Class {
    /// The class name (`?` when the description has no name pointer).
    pub name: String,
    /// Where the parent's description is (absolute), if any.
    pub parent: Option<u64>,
    /// The object size.
    pub size: i32,
    /// The members the class adds to its parent's.
    pub members: Vec<ClassMember>,
}

/// A packfile, read as `hkpack.Packfile` reads it. Offsets are absolute file offsets.
#[derive(Debug, Clone)]
pub struct Packfile<'a> {
    data: &'a [u8],
    /// Where the packfile starts in the file.
    pub base: u64,
    /// The header's file version.
    pub version: u32,
    /// The pointer size (4).
    pub ptr_size: u8,
    /// The contents version (`Havok-6.5.0-r1`, ...).
    pub contents_version: String,
    /// The sections.
    pub sections: Vec<Section>,
    ptr: HashMap<u64, u64>,
    objects: Vec<(u64, String)>,
    classes: Vec<(u64, Class)>,
}

/// `size` bytes at `at`, or the reference's `struct.error`.
fn field(data: &[u8], at: u64, size: u64) -> Result<&[u8]> {
    usize::try_from(at)
        .ok()
        .and_then(|a| data.get(a..a.checked_add(usize::try_from(size).ok()?)?))
        .ok_or_else(|| {
            HavokError::Damaged(format!(
                "a field of {size} bytes at {at} is past the end ({})",
                data.len()
            ))
        })
}

fn u32_at(data: &[u8], at: u64) -> Result<u32> {
    Ok(bytes::u32s(field(data, at, 4)?).next().unwrap_or(0))
}

fn i32_at(data: &[u8], at: u64) -> Result<i32> {
    Ok(u32_at(data, at)? as i32)
}

/// The words of a fixup table entry at `at` (the caller keeps it inside the file).
fn words<const N: usize>(data: &[u8], at: u64) -> [u32; N] {
    let mut out = [0u32; N];
    for (k, w) in out.iter_mut().enumerate() {
        *w = u32_at(data, at + 4 * k as u64).unwrap_or(0);
    }
    out
}

/// The reference's `cstr`: the text from `at` (the file's start when `None`) to the next 0 byte, as ASCII with any
/// other byte U+FFFD; [`HavokError::Refused`] when there is no 0 byte after `at`.
fn cstr(data: &[u8], at: Option<u64>) -> Result<String> {
    let from = usize::try_from(at.unwrap_or(0)).unwrap_or(usize::MAX);
    let s = data
        .get(from..)
        .and_then(|s| s.split(|&b| b == 0).next().filter(|t| t.len() < s.len()))
        .ok_or_else(|| HavokError::Refused(format!("a name at {from} has no end")))?;
    Ok(ascii(s))
}

/// Bytes as Python's `decode('ascii', 'replace')`: each byte above 0x7F is U+FFFD.
fn ascii(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| {
            if b.is_ascii() {
                char::from(b)
            } else {
                char::REPLACEMENT_CHARACTER
            }
        })
        .collect()
}

/// The fixup table's entries in `[from, to)` by `step`, kept inside the file as the reference's `range` keeps them
/// (`min(to, len - start - (step - 1))`).
fn table(len: u64, start: u64, from: u32, to: u32, step: u64) -> impl Iterator<Item = u64> {
    let limit = i128::from(len) - i128::from(start) - (i128::from(step) - 1);
    let end = i128::from(to).min(limit);
    let from = i128::from(from);
    let n = if end > from {
        ((end - from) as u64).div_ceil(step)
    } else {
        0
    };
    (0..n).map(move |k| start + u64::from(from as u32) + k * step)
}

impl<'a> Packfile<'a> {
    /// Reads the packfile in `data` (found by its [`MAGIC`]): sections, fixups, objects and class descriptions.
    ///
    /// # Errors
    ///
    /// [`HavokError`]; see the module's documentation for how each maps to the reference.
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        let base = data
            .windows(MAGIC.len())
            .position(|w| w == MAGIC)
            .ok_or(HavokError::NotPackfile)? as u64;
        let len = data.len() as u64;
        let mut h = [0u32; 8];
        for (k, w) in h.iter_mut().enumerate() {
            *w = u32_at(data, base + 4 * k as u64)?;
        }
        let [_, _, _, version, layout, nsections, _, _] = h;
        let cv = usize::try_from(base + 0x28)
            .ok()
            .and_then(|a| data.get(a..data.len().min(a + 16)));
        let cv = cv.unwrap_or_default();
        let contents_version = ascii(cv.split(|&b| b == 0).next().unwrap_or_default());
        let mut sections = Vec::new();
        for i in 0..u64::from(nsections) {
            let o = base + 0x40 + i * 0x30;
            let raw = usize::try_from(o)
                .ok()
                .and_then(|a| data.get(a.min(data.len())..data.len().min(a.saturating_add(20))))
                .unwrap_or_default();
            let name = std::str::from_utf8(raw.split(|&b| b == 0).next().unwrap_or_default())
                .map_err(|e| HavokError::Refused(format!("section {i}'s name is not UTF-8 ({e})")))?
                .to_owned();
            let mut w = [0u32; 7];
            for (k, v) in w.iter_mut().enumerate() {
                *v = u32_at(data, o + 20 + 4 * k as u64)?;
            }
            sections.push(Section {
                name,
                start: base + u64::from(w[0]),
                local: w[1],
                global: w[2],
                virtual_fixups: w[3],
                exports: w[4],
                imports: w[5],
                end: w[6],
            });
        }
        let section_start = |sec: u32| {
            sections
                .get(sec as usize)
                .map(|s| s.start)
                .ok_or_else(|| HavokError::Damaged(format!("a fixup names section {sec} of {}", sections.len())))
        };
        let mut ptr = HashMap::new();
        for s in &sections {
            for at in table(len, s.start, s.local, s.global, 8) {
                let [src, dst] = words::<2>(data, at);
                if src != u32::MAX {
                    ptr.insert(s.start + u64::from(src), s.start + u64::from(dst));
                }
            }
            for at in table(len, s.start, s.global, s.virtual_fixups, 12) {
                let [src, sec, dst] = words::<3>(data, at);
                if src != u32::MAX {
                    ptr.insert(s.start + u64::from(src), section_start(sec)? + u64::from(dst));
                }
            }
        }
        let mut objects = Vec::new();
        for s in &sections {
            for at in table(len, s.start, s.virtual_fixups, s.exports, 12) {
                let [src, sec, name] = words::<3>(data, at);
                if src != u32::MAX {
                    let cname = cstr(data, Some(section_start(sec)? + u64::from(name)))?;
                    objects.push((s.start + u64::from(src), cname));
                }
            }
        }
        let mut pf = Self {
            data,
            base,
            version,
            ptr_size: (layout & 0xFF) as u8,
            contents_version,
            sections,
            ptr,
            objects,
            classes: Vec::new(),
        };
        pf.classes = pf.read_classes()?;
        Ok(pf)
    }

    /// The reference's `_read_classes`, in its order of reads (so the same damage fails the same way).
    fn read_classes(&self) -> Result<Vec<(u64, Class)>> {
        let d = self.data;
        let mut out = Vec::new();
        for (off, cname) in &self.objects {
            if cname != "hkClass" {
                continue;
            }
            let off = *off;
            let name = match self.deref(off) {
                Some(p) if p != 0 => cstr(d, Some(p))?,
                _ => "?".to_owned(),
            };
            let parent = self.deref(off + 4);
            let size = i32_at(d, off + 8)?;
            field(d, off + 12, 4)?;
            let members_p = self.deref(off + 24);
            let nmembers = i32_at(d, off + 28)?;
            let mut members = Vec::new();
            for k in 0..u64::try_from(nmembers).unwrap_or(0) {
                let mp = members_p.ok_or_else(|| {
                    HavokError::Damaged(format!("class {name} has {nmembers} members and no member array"))
                })?;
                let mo = mp + k * 24;
                let mname = cstr(d, self.deref(mo))?;
                let class = self.deref(mo + 4);
                let [kind, subtype, c0, c1, f0, f1, o0, o1]: [u8; 8] =
                    bytes::first(field(d, mo + 12, 8)?.iter().copied());
                members.push(ClassMember {
                    name: mname,
                    class,
                    kind,
                    subtype,
                    carray: i16::from_le_bytes([c0, c1]),
                    flags: u16::from_le_bytes([f0, f1]),
                    offset: u16::from_le_bytes([o0, o1]),
                });
            }
            out.push((
                off,
                Class {
                    name,
                    parent,
                    size,
                    members,
                },
            ));
        }
        Ok(out)
    }

    /// The file's bytes.
    #[must_use]
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The target of the pointer slot at `slot`, if a fixup sets it.
    #[must_use]
    pub fn deref(&self, slot: u64) -> Option<u64> {
        self.ptr.get(&slot).copied()
    }

    /// Every pointer slot and its target (`hkpack.Packfile.ptr`, a later fixup of the same slot winning).
    #[must_use]
    pub fn pointers(&self) -> &HashMap<u64, u64> {
        &self.ptr
    }

    /// Every object (its offset and class name) in the order of the virtual fixups.
    #[must_use]
    pub fn objects(&self) -> &[(u64, String)] {
        &self.objects
    }

    /// The offsets of the objects of class `name`, in order.
    pub fn objects_of<'s>(&'s self, name: &'s str) -> impl Iterator<Item = u64> + 's {
        self.objects.iter().filter(move |(_, c)| c == name).map(|(o, _)| *o)
    }

    /// The class descriptions (the offset of each `hkClass` object and what it says), in object order.
    #[must_use]
    pub fn classes(&self) -> &[(u64, Class)] {
        &self.classes
    }

    /// Every pointer fixup `(slot, target)` in the order [`Packfile::parse`] applied them (each section's local
    /// fixups, then its global ones; a later one for the same slot wins): the insertion order of `hkpack.Packfile.ptr`
    /// (round F3).
    #[must_use]
    pub fn pointer_fixups(&self) -> Vec<(u64, u64)> {
        let (data, len) = (self.data, self.data.len() as u64);
        let mut out = Vec::new();
        for s in &self.sections {
            for at in table(len, s.start, s.local, s.global, 8) {
                let [src, dst] = words::<2>(data, at);
                if src != u32::MAX {
                    out.push((s.start + u64::from(src), s.start + u64::from(dst)));
                }
            }
            for at in table(len, s.start, s.global, s.virtual_fixups, 12) {
                let [src, sec, dst] = words::<3>(data, at);
                if src != u32::MAX
                    && let Some(t) = self.sections.get(sec as usize)
                {
                    out.push((s.start + u64::from(src), t.start + u64::from(dst)));
                }
            }
        }
        out
    }
}

/// A triangle: three corners (x, y, z).
pub type Triangle = [[f32; 3]; 3];

/// The `count` values of `size` bytes at `at`, as NumPy's `frombuffer` reads them (an error past the end).
fn array(data: &[u8], at: u64, count: u64, size: u64) -> Result<&[u8]> {
    let len = count
        .checked_mul(size)
        .ok_or_else(|| HavokError::Damaged(format!("an array of {count} values")))?;
    field(data, at, len)
}

/// `count` rows of four `f32`s at `at`.
fn rows(data: &[u8], at: u64, count: u64) -> Result<Vec<[f32; 4]>> {
    let floats: Vec<f32> = bytes::f32s(array(data, at, count, 16)?).collect();
    Ok(floats.as_chunks::<4>().0.to_vec())
}

/// NumPy's `min` / `max` of a column: NaN when any value is NaN.
fn extreme(values: impl Iterator<Item = f32>, take_new: fn(f32, f32) -> bool) -> f32 {
    let mut acc: Option<f32> = None;
    for v in values {
        acc = Some(match acc {
            None => v,
            Some(a) if a.is_nan() => a,
            Some(_) if v.is_nan() => v,
            Some(a) => {
                if take_new(v, a) {
                    v
                } else {
                    a
                }
            }
        });
    }
    acc.unwrap_or(f32::NAN)
}

/// The faces of a box's 12 triangles over its corners (x outermost, z innermost), as the reference lists them.
const BOX_FACES: [[usize; 3]; 12] = [
    [0, 1, 3],
    [0, 3, 2],
    [4, 6, 7],
    [4, 7, 5],
    [0, 4, 5],
    [0, 5, 1],
    [2, 3, 7],
    [2, 7, 6],
    [0, 2, 6],
    [0, 6, 4],
    [1, 5, 7],
    [1, 7, 3],
];

/// The collision triangles (local space) of a blueprint's Havok asset, as `levelobjects.triangles` gives them:
/// every storage extended mesh subpart's triangles (16-bit indices in groups of four: three corners and padding;
/// a subpart whose indices are not all below its vertex count is skipped), then for each convex vertices shape the
/// 12 triangles of its vertices' bounding box (the first array member, from offset 0x10 to 0x5C, holding 1 to 9,999
/// `FourVector`s).
///
/// `Ok(None)` when the reference gives `None`: the file is not a packfile, `hkpack.load` refuses it, or it has no
/// triangles.
///
/// # Errors
///
/// [`HavokError::Damaged`] when the reference raises (see the module's documentation).
pub fn triangles(data: &[u8]) -> Result<Option<Vec<Triangle>>> {
    let pf = match Packfile::parse(data) {
        Ok(pf) => pf,
        Err(HavokError::NotPackfile | HavokError::Refused(_)) => return Ok(None),
        Err(e) => return Err(e),
    };
    let mut tris: Vec<Triangle> = Vec::new();
    let nonzero = |p: Option<u64>| p.filter(|&p| p != 0);
    for off in pf.objects_of(MESH_SUBPART) {
        let p = nonzero(pf.deref(off + 8));
        let n = i32_at(data, off + 12)?;
        let ip = nonzero(pf.deref(off + 0x14));
        let ni = i32_at(data, off + 0x18)?;
        let (Some(p), Some(ip)) = (p, ip) else { continue };
        if n <= 0 || ni <= 0 {
            continue;
        }
        let verts = rows(data, p, n as u64)?;
        let idx: Vec<u16> = bytes::u16s(array(data, ip, ni as u64, 2)?).collect();
        if ni % 4 != 0 {
            return Err(HavokError::Damaged(format!("{ni} indices are not groups of four")));
        }
        let groups = || idx.as_chunks::<4>().0.iter().map(|&[a, b, c, _]| [a, b, c]);
        if groups().flatten().all(|i| i32::from(i) < n) {
            for g in groups() {
                tris.push(g.map(|i| {
                    let v = verts.get(usize::from(i)).copied().unwrap_or([f32::NAN; 4]);
                    [v[0], v[1], v[2]]
                }));
            }
        }
    }
    for off in pf.objects_of(CONVEX_VERTICES) {
        for k in (0x10..0x60).step_by(4) {
            let Some(p) = nonzero(pf.deref(off + k)) else { continue };
            let n = i32_at(data, off + k + 4)?;
            if !(1..10_000).contains(&n) {
                continue;
            }
            // n FourVectors: x0..x3, y0..y3, z0..z3 (a w lane is not stored)
            let fv = rows(data, p, 3 * n as u64)?;
            let axis = |a: usize| fv.iter().skip(a).step_by(3).flat_map(|r| r.iter().copied());
            let lo: [f32; 3] = std::array::from_fn(|a| extreme(axis(a), |v, acc| v < acc));
            let hi: [f32; 3] = std::array::from_fn(|a| extreme(axis(a), |v, acc| v > acc));
            let mut corners = Vec::with_capacity(8);
            for x in [lo[0], hi[0]] {
                for y in [lo[1], hi[1]] {
                    for z in [lo[2], hi[2]] {
                        corners.push([x, y, z]);
                    }
                }
            }
            for f in BOX_FACES {
                tris.push(f.map(|c| corners.get(c).copied().unwrap_or([f32::NAN; 3])));
            }
            break;
        }
    }
    Ok((!tris.is_empty()).then_some(tris))
}

/// A small packfile with one storage extended mesh subpart holding `vertices` and `triangles` (a test writer, as
/// [`fbrb::build`](crate::fbrb::build): the game's packfiles come from Havok's tools). [`triangles`] reads them back.
///
/// ```
/// use bc2_formats::havok;
///
/// let file = havok::build(&[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]], &[[0, 1, 2]]);
/// let t = havok::triangles(&file)?.unwrap_or_default();
/// assert_eq!(t, vec![[[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]]);
/// # Ok::<(), havok::HavokError>(())
/// ```
#[must_use]
pub fn build(vertices: &[[f32; 3]], triangles: &[[u16; 3]]) -> Vec<u8> {
    let pad16 = |v: &mut Vec<u8>| v.resize(v.len().next_multiple_of(16), 0);
    // section 1 (__data__): the vertices (x, y, z, 1), the index groups (a, b, c, 0), the subpart object
    let mut data: Vec<u8> = vertices
        .iter()
        .flat_map(|v| [v[0], v[1], v[2], 1.0])
        .flat_map(f32::to_le_bytes)
        .collect();
    pad16(&mut data);
    let idx = data.len() as u32;
    data.extend(
        triangles
            .iter()
            .flat_map(|t| [t[0], t[1], t[2], 0])
            .flat_map(u16::to_le_bytes),
    );
    pad16(&mut data);
    let sub = data.len() as u32;
    let words = [0u32, 0, 0, vertices.len() as u32, 0, 0, 4 * triangles.len() as u32, 0];
    data.extend(words.iter().flat_map(|w| w.to_le_bytes()));
    let local = data.len() as u32;
    for (slot, target) in [(sub + 8, 0), (sub + 0x14, idx)] {
        data.extend(slot.to_le_bytes());
        data.extend(target.to_le_bytes());
    }
    let virt = data.len() as u32;
    data.extend([sub, 0, 0].iter().flat_map(|w| w.to_le_bytes()));
    let exports = data.len() as u32;
    let names = format!("{MESH_SUBPART}\0");
    let mut out = MAGIC.to_vec();
    for w in [0u32, 4, 0x0101_0004, 2, 1, 0, 0, 0] {
        out.extend(w.to_le_bytes());
    }
    out.extend(b"Havok-6.5.0-r1\0\0");
    let s0 = 0x40 + 2 * 0x30;
    let s1 = (s0 + names.len()).next_multiple_of(16);
    let n0 = names.len() as u32;
    let section = |name: &str, start: usize, w: [u32; 6]| {
        let mut h = name.as_bytes().to_vec();
        h.resize(20, 0);
        h.extend((start as u32).to_le_bytes());
        h.extend(w.iter().flat_map(|x| x.to_le_bytes()));
        h
    };
    out.resize(0x40, 0);
    out.extend(section("__classnames__", s0, [n0; 6]));
    out.extend(section("__data__", s1, [local, virt, virt, exports, exports, exports]));
    out.extend(names.as_bytes());
    out.resize(s1, 0);
    out.extend(data);
    out
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    /// A packfile builder: one `__data__` section holding the objects, its fixup tables after the data.
    pub(crate) struct Builder {
        data: Vec<u8>,
        local: Vec<(u32, u32)>,
        virt: Vec<(u32, u32)>,
        names: Vec<u8>,
    }

    impl Builder {
        pub(crate) fn new() -> Self {
            Self {
                data: Vec::new(),
                local: Vec::new(),
                virt: Vec::new(),
                names: Vec::new(),
            }
        }

        /// Appends bytes to the data section, returns their offset in it.
        pub(crate) fn put(&mut self, bytes: &[u8]) -> u32 {
            while !self.data.len().is_multiple_of(16) {
                self.data.push(0);
            }
            let at = self.data.len() as u32;
            self.data.extend(bytes);
            at
        }

        pub(crate) fn pointer(&mut self, slot: u32, target: u32) {
            self.local.push((slot, target));
        }

        pub(crate) fn object(&mut self, at: u32, class: &str) {
            let name = self.names.len() as u32;
            self.names.extend(class.as_bytes());
            self.names.push(0);
            self.virt.push((at, name));
        }

        /// The packfile: header, section 0 `__classnames__` (the names), section 1 `__data__`.
        pub(crate) fn build(&self, prefix: &[u8]) -> Vec<u8> {
            let mut out = prefix.to_vec();
            let base = out.len();
            out.extend(MAGIC);
            // tag, version, layout (4-byte pointers), sections, contents section and offset, two more words
            for w in [0u32, 4, 0x0101_0004, 2, 1, 0, 0, 0] {
                out.extend(w.to_le_bytes());
            }
            out.extend(b"Havok-6.5.0-r1\0\0");
            out.resize(base + 0x40 + 2 * 0x30, 0);
            // section 0: names
            let s0 = out.len() - base;
            out.extend(&self.names);
            let n0 = self.names.len() as u32;
            // section 1: data, then the fixups
            while !(out.len() - base).is_multiple_of(16) {
                out.push(0);
            }
            let s1 = out.len() - base;
            out.extend(&self.data);
            let local = self.data.len() as u32;
            for (a, b) in &self.local {
                out.extend(a.to_le_bytes());
                out.extend(b.to_le_bytes());
            }
            out.extend([0xFF; 8]);
            let global = (out.len() - base - s1) as u32;
            let virt = global;
            for (a, n) in &self.virt {
                out.extend(a.to_le_bytes());
                out.extend(0u32.to_le_bytes());
                out.extend(n.to_le_bytes());
            }
            let exports = (out.len() - base - s1) as u32;
            let sec = |o: &mut Vec<u8>, at: usize, name: &[u8], start: usize, w: [u32; 6]| {
                let mut n = name.to_vec();
                n.resize(20, 0);
                o[base + at..base + at + 20].copy_from_slice(&n);
                let mut v = (start as u32).to_le_bytes().to_vec();
                for x in w {
                    v.extend(x.to_le_bytes());
                }
                o[base + at + 20..base + at + 48].copy_from_slice(&v);
            };
            sec(&mut out, 0x40, b"__classnames__", s0, [n0, n0, n0, n0, n0, n0]);
            sec(
                &mut out,
                0x70,
                b"__data__",
                s1,
                [local, global, virt, exports, exports, exports],
            );
            out
        }
    }

    fn le(words: &[u32]) -> Vec<u8> {
        words.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    fn f(values: &[f32]) -> Vec<u8> {
        values.iter().flat_map(|w| w.to_le_bytes()).collect()
    }

    /// A packfile with one mesh subpart (a quad: two triangles) and one convex shape.
    pub(crate) fn sample() -> Builder {
        let mut b = Builder::new();
        let verts = b.put(&f(&[
            0.0, 0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0,
        ]));
        let idx = b.put(&[0, 0, 1, 0, 2, 0, 0, 0, 0, 0, 2, 0, 3, 0, 0, 0]);
        let sub = b.put(&[0u8; 0x20]);
        b.data[sub as usize + 12..sub as usize + 16].copy_from_slice(&4i32.to_le_bytes());
        b.data[sub as usize + 0x18..sub as usize + 0x1C].copy_from_slice(&8i32.to_le_bytes());
        b.pointer(sub + 8, verts);
        b.pointer(sub + 0x14, idx);
        b.object(sub, MESH_SUBPART);
        // a convex shape: two FourVectors (8 points) from -1 to 2
        let fv = b.put(&f(&[
            -1.0, 0.0, 0.0, 0.5, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, //
            0.0, 0.0, 2.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 2.0,
        ]));
        let cvx = b.put(&[0u8; 0x60]);
        b.data[cvx as usize + 0x34..cvx as usize + 0x38].copy_from_slice(&2i32.to_le_bytes());
        b.pointer(cvx + 0x30, fv);
        b.object(cvx, CONVEX_VERTICES);
        b
    }

    #[test]
    fn reads_meshes_and_boxes() {
        let file = sample().build(b"prefix");
        let pf = Packfile::parse(&file).unwrap();
        assert_eq!(pf.base, 6);
        assert_eq!(pf.version, 4);
        assert_eq!(pf.ptr_size, 4);
        assert_eq!(pf.contents_version, "Havok-6.5.0-r1");
        assert_eq!(pf.sections.len(), 2);
        assert_eq!(pf.sections[1].name, "__data__");
        assert_eq!(pf.objects().len(), 2);
        assert_eq!(pf.objects_of(MESH_SUBPART).count(), 1);
        assert_eq!(pf.classes().len(), 0);
        assert_eq!(pf.data().len(), file.len());
        let t = triangles(&file).unwrap().unwrap();
        assert_eq!(t.len(), 2 + 12);
        assert_eq!(t[0], [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [1.0, 0.0, 1.0]]);
        assert_eq!(t[1], [[0.0, 0.0, 0.0], [1.0, 0.0, 1.0], [0.0, 0.0, 1.0]]);
        // the box: corner 0 is (lo x, lo y, lo z), corner 7 (hi, hi, hi)
        assert_eq!(t[2][0], [-1.0, -1.0, -1.0]);
        assert_eq!(t[4][2], [2.0, 2.0, 2.0]);
    }

    #[test]
    fn subparts_with_indices_out_of_range_are_skipped() {
        let mut b = sample();
        b.data[0x40 + 12] = 9; // index group 2's first corner: 9 of 4 vertices
        let t = triangles(&b.build(b"")).unwrap().unwrap();
        assert_eq!(t.len(), 12);
    }

    #[test]
    fn nan_spreads_through_a_box() {
        let mut b = sample();
        let at = b.data.len() - 0x60 - 48;
        b.data[at..at + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        let t = triangles(&b.build(b"")).unwrap().unwrap();
        assert!(t[2][0][0].is_nan());
        assert!(!t[2][0][1].is_nan());
    }

    #[test]
    fn classes_are_read() {
        let mut b = Builder::new();
        let name = b.put(b"hkpShape\0");
        let mname = b.put(b"userData\0");
        let members = b.put(&[0u8; 24]);
        b.data[members as usize + 12..members as usize + 20].copy_from_slice(&[7, 0, 2, 0, 1, 0, 8, 0]);
        b.pointer(members, mname);
        let cls = b.put(&[0u8; 0x28]);
        b.data[cls as usize + 8..cls as usize + 12].copy_from_slice(&16i32.to_le_bytes());
        b.data[cls as usize + 28..cls as usize + 32].copy_from_slice(&1i32.to_le_bytes());
        b.pointer(cls, name);
        b.pointer(cls + 24, members);
        b.object(cls, "hkClass");
        let anon = b.put(&[0u8; 0x28]);
        b.object(anon, "hkClass");
        let file = b.build(b"");
        let pf = Packfile::parse(&file).unwrap();
        let c = &pf.classes()[0].1;
        assert_eq!((c.name.as_str(), c.size, c.parent), ("hkpShape", 16, None));
        assert_eq!(
            c.members[0],
            ClassMember {
                name: "userData".into(),
                class: None,
                kind: 7,
                subtype: 0,
                carray: 2,
                flags: 1,
                offset: 8
            }
        );
        assert_eq!(pf.classes()[1].1.name, "?");
        assert_eq!(triangles(&file).unwrap(), None);
        // members without a member array: the reference's TypeError
        let mut b2 = b;
        b2.local.retain(|&(s, _)| s != cls + 24);
        assert!(matches!(Packfile::parse(&b2.build(b"")), Err(HavokError::Damaged(_))));
    }

    #[test]
    fn refusals_and_damage() {
        assert_eq!(triangles(&[]).unwrap(), None);
        // magic only: the header runs past the end
        assert!(matches!(triangles(&MAGIC), Err(HavokError::Damaged(_))));
        let file = sample().build(b"");
        // every cut: an error or a result, never a panic; most cuts damage it
        let mut damaged = 0;
        for cut in 0..file.len() {
            if triangles(&file[..cut]).is_err() {
                damaged += 1;
            }
        }
        assert!(damaged > 0);
        // a section name that is not UTF-8: refused (None)
        let mut bad = file.clone();
        bad[0x40] = 0xFF;
        assert_eq!(triangles(&bad).unwrap(), None);
        assert!(matches!(Packfile::parse(&bad), Err(HavokError::Refused(_))));
        // a virtual fixup naming section 9 of 2
        let mut b = sample();
        b.virt.clear();
        let mut f2 = b.build(b"");
        let s1 = u32::from_le_bytes(f2[0x70 + 20..0x70 + 24].try_into().unwrap()) as usize;
        let virt = u32::from_le_bytes(f2[0x70 + 32..0x70 + 36].try_into().unwrap()) as usize;
        f2.extend(le(&[0, 9, 0]));
        let exports = (f2.len() - s1) as u32;
        f2[0x70 + 36..0x70 + 40].copy_from_slice(&exports.to_le_bytes());
        assert_eq!(virt + 12, exports as usize);
        assert!(matches!(triangles(&f2), Err(HavokError::Damaged(m)) if m.contains("section 9")));
        // a class name with no end: refused
        let mut f3 = f2.clone();
        let at = f3.len() - 8;
        f3[at..at + 4].copy_from_slice(&0u32.to_le_bytes());
        f3[at + 4..at + 8].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(triangles(&f3).unwrap(), None);
        assert!(HavokError::NotPackfile.to_string().contains("no Havok"));
    }

    #[test]
    fn index_groups_must_be_whole() {
        let mut b = sample();
        let sub = 0x50usize;
        b.data[sub + 0x18..sub + 0x1C].copy_from_slice(&7i32.to_le_bytes());
        assert!(matches!(triangles(&b.build(b"")), Err(HavokError::Damaged(m)) if m.contains("groups of four")));
    }

    proptest::proptest! {
        #![proptest_config(proptest::prelude::ProptestConfig::with_cases(500))]

        /// Any damaged copy of a packfile (cut short, bytes changed) gives triangles, `None` or an error, never a
        /// panic.
        #[test]
        fn damaged_packfiles_never_panic(
            cut in 0usize..2000,
            flips in proptest::collection::vec((proptest::prelude::any::<usize>(), proptest::prelude::any::<u8>()), 0..8),
        ) {
            let mut d = sample().build(b"x");
            d.truncate(cut.max(1).min(d.len()));
            for (at, b) in flips {
                let k = at % d.len();
                d[k] = b;
            }
            let _ = triangles(&d);
        }
    }
}
