//! `hkpack.Packfile`'s reflection walk (`read_object`, `read_value`, `all_members`, `elem_size`, `cstr`; round F4):
//! an object's fields decoded through the `hkClass` descriptions the file carries, as NavKit's
//! `bfbc2-auto-navkit/tools/hkpack.py` decodes them. Moved here from `bfbc2.exe hkpack` (round O2) so the Python
//! module's handle (`bc2_native.HkPackfile`) and the command line share it.
//!
//! [`Index`] holds what the walk looks up (owned: a handle keeps it beside the file's bytes); [`Reader`] reads over
//! the bytes. Every read answers `None` where the reference raises (a read past the data or before it - Python's
//! `struct` reads a negative offset from the end -, a C string without an end, a parent class that is not described,
//! a C array of a type without a size, nesting deeper than Python's recursion allows): the caller hands the call to
//! the Python, whose error it then is.

use std::collections::HashMap;

use super::{Class, ClassMember, Packfile};

// hkClassMember::Type (Havok 6.x)
const T_VOID: u8 = 0;
const T_BOOL: u8 = 1;
const T_CHAR: u8 = 2;
const T_INT8: u8 = 3;
const T_UINT8: u8 = 4;
const T_INT16: u8 = 5;
const T_UINT16: u8 = 6;
const T_INT32: u8 = 7;
const T_UINT32: u8 = 8;
const T_INT64: u8 = 9;
const T_UINT64: u8 = 10;
const T_REAL: u8 = 11;
const T_ZERO: u8 = 19;
const T_POINTER: u8 = 20;
const T_FUNCTIONPOINTER: u8 = 21;
const T_ARRAY: u8 = 22;
const T_ENUM: u8 = 24;
const T_STRUCT: u8 = 25;
const T_SIMPLEARRAY: u8 = 26;
const T_CSTRING: u8 = 29;
const T_ULONG: u8 = 30;
const T_FLAGS: u8 = 31;

/// Python's recursion limit allows about this many nested structs; deeper is left to Python's `RecursionError`.
pub const MAX_DEPTH: usize = 300;

/// A value `read_value` gives.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// An integer (`SIMPLE`'s integer types, enums and flags).
    Int(i128),
    /// `T_BOOL` (`struct`'s `?`).
    Bool(bool),
    /// `T_REAL`.
    Float(f64),
    /// A `VEC` type: a tuple of floats.
    Floats(Vec<f64>),
    /// A pointer: `('ptr', target, by_offset.get(target))`.
    Ptr(u64, Option<String>),
    /// `None`.
    None,
    /// A C string.
    Str(String),
    /// A list (a C array, an array).
    List(Vec<Value>),
    /// An object: its members' values by name, in order.
    Dict(Vec<(String, Value)>),
    /// `('?', n)`: an array of a type without a size.
    Unknown(i32),
}

/// `SIMPLE`'s (signed, size) of a type, a float for `T_REAL`.
fn simple(t: u8) -> Option<(bool, usize)> {
    Some(match t {
        T_BOOL | T_UINT8 => (false, 1),
        T_CHAR | T_INT8 => (true, 1),
        T_INT16 => (true, 2),
        T_UINT16 => (false, 2),
        T_INT32 => (true, 4),
        T_UINT32 | T_ULONG | T_REAL => (false, 4),
        T_INT64 => (true, 8),
        T_UINT64 => (false, 8),
        _ => return None,
    })
}

/// `VEC`'s float count of a type.
fn vec_len(t: u8) -> Option<usize> {
    match t {
        12 | 13 => Some(4),
        14..=16 => Some(12),
        17 | 18 => Some(16),
        _ => None,
    }
}

/// What the walk looks up, as the reference's dicts hold it: the pointer fixups (`ptr`), the class descriptions by
/// offset (`classes`: first place, last value), by name (`class_by_name`: the last wins) and the objects' classes
/// by offset (`by_offset`: the last wins).
#[derive(Debug, Clone)]
pub struct Index {
    ptr: HashMap<u64, u64>,
    classes: Vec<(u64, Class)>,
    by_off: HashMap<u64, usize>,
    by_name: HashMap<String, u64>,
    objects: HashMap<u64, String>,
    order: Vec<(u64, String)>,
}

impl Index {
    /// The lookups of a parsed packfile.
    #[must_use]
    pub fn new(pf: &Packfile<'_>) -> Self {
        let mut classes: Vec<(u64, Class)> = Vec::new();
        let mut by_off = HashMap::new();
        for (off, c) in pf.classes() {
            if let Some(&k) = by_off.get(off) {
                if let Some(slot) = classes.get_mut(k) {
                    *slot = (*off, c.clone());
                }
            } else {
                by_off.insert(*off, classes.len());
                classes.push((*off, c.clone()));
            }
        }
        let by_name = classes.iter().map(|(o, c)| (c.name.clone(), *o)).collect();
        let objects = pf.objects().iter().map(|(o, c)| (*o, c.clone())).collect();
        Self {
            ptr: pf.pointers().clone(),
            classes,
            by_off,
            by_name,
            objects,
            order: pf.objects().to_vec(),
        }
    }

    /// `deref(slot)`.
    #[must_use]
    pub fn deref(&self, slot: u64) -> Option<u64> {
        self.ptr.get(&slot).copied()
    }

    /// `by_offset.get(off)`.
    #[must_use]
    pub fn class_of(&self, off: u64) -> Option<&str> {
        self.objects.get(&off).map(String::as_str)
    }

    /// `objects_of(name)`: the offsets of the objects of a class, in order.
    #[must_use]
    pub fn objects_of(&self, name: &str) -> Vec<u64> {
        self.order.iter().filter(|(_, c)| c == name).map(|(o, _)| *o).collect()
    }

    /// The class descriptions in the order of `classes` (each offset once).
    #[must_use]
    pub fn classes(&self) -> &[(u64, Class)] {
        &self.classes
    }

    fn class(&self, off: u64) -> Option<&Class> {
        self.by_off.get(&off).and_then(|&k| self.classes.get(k)).map(|c| &c.1)
    }

    /// `struct_size(cname)`.
    #[must_use]
    pub fn struct_size(&self, cname: Option<&str>) -> Option<i128> {
        let off = self.by_name.get(cname?)?;
        self.class(*off).map(|c| i128::from(c.size))
    }

    /// `member_class_name(m)`: the name of a member's class.
    #[must_use]
    pub fn member_class_name(&self, class: Option<u64>) -> Option<&str> {
        class.and_then(|o| self.class(o)).map(|c| c.name.as_str())
    }

    /// `all_members(cname)`: the members with the inherited ones first; `None` where the reference raises (a parent
    /// that is not described).
    #[must_use]
    pub fn all_members(&self, cname: Option<&str>) -> Option<Vec<&ClassMember>> {
        let mut off = cname.and_then(|n| self.by_name.get(n).copied());
        let mut chain = Vec::new();
        while let Some(o) = off {
            let c = self.class(o)?;
            chain.push(c);
            if chain.len() > 10_000 {
                return None;
            }
            off = c.parent;
        }
        Some(chain.iter().rev().flat_map(|c| c.members.iter()).collect())
    }

    /// `elem_size(t, mclass_name)`.
    #[must_use]
    pub fn elem_size(&self, t: u8, mclass: Option<&str>) -> Option<i128> {
        if let Some((_, n)) = simple(t) {
            return Some(n as i128);
        }
        if let Some(n) = vec_len(t) {
            return Some(4 * n as i128);
        }
        match t {
            T_POINTER | T_CSTRING | T_ULONG => Some(4),
            T_STRUCT => self.struct_size(mclass),
            _ => None,
        }
    }
}

/// `cstr(off)` over the file's bytes: the bytes to the next zero, each byte past ASCII a U+FFFD; `None` without an
/// end (the reference's `ValueError`).
#[must_use]
pub fn cstr(d: &[u8], off: u64) -> Option<String> {
    let o = usize::try_from(off).ok()?;
    let rest = d.get(o..)?;
    let e = rest.iter().position(|&b| b == 0)?;
    Some(
        rest.get(..e)?
            .iter()
            .map(|&b| if b < 0x80 { char::from(b) } else { '\u{fffd}' })
            .collect(),
    )
}

/// The walk over a file's bytes and its [`Index`].
#[derive(Debug, Clone, Copy)]
pub struct Reader<'a> {
    d: &'a [u8],
    ix: &'a Index,
}

impl<'a> Reader<'a> {
    /// A reader of `d` (the bytes the index was made from).
    #[must_use]
    pub fn new(d: &'a [u8], ix: &'a Index) -> Self {
        Self { d, ix }
    }

    fn bytes(&self, at: i128, n: usize) -> Option<&'a [u8]> {
        let a = usize::try_from(at).ok()?;
        self.d.get(a..a.checked_add(n)?)
    }

    fn int(&self, at: i128, n: usize, signed: bool) -> Option<i128> {
        let b = self.bytes(at, n)?;
        let mut v: u64 = 0;
        for (k, &x) in b.iter().enumerate() {
            v |= u64::from(x) << (8 * k);
        }
        let bits = 8 * n;
        Some(if signed && bits < 64 && v >> (bits - 1) & 1 == 1 {
            i128::from(v) - (1i128 << bits)
        } else if signed && bits == 64 {
            i128::from(v.cast_signed())
        } else {
            i128::from(v)
        })
    }

    fn f32(&self, at: i128) -> Option<f64> {
        let b: [u8; 4] = self.bytes(at, 4)?.try_into().ok()?;
        Some(f64::from(f32::from_le_bytes(b)))
    }

    fn deref(&self, at: i128) -> Option<u64> {
        u64::try_from(at).ok().and_then(|a| self.ix.deref(a))
    }

    /// `read_value(at, mtype, sub, mclass_name, carray)`.
    #[must_use]
    pub fn read_value(&self, at: i128, t: u8, sub: u8, mclass: Option<&str>, carray: i16) -> Option<Value> {
        self.value(at, t, sub, mclass, carray, 0)
    }

    /// `read_object(at, cname)`.
    #[must_use]
    pub fn read_object(&self, at: i128, cname: Option<&str>) -> Option<Value> {
        self.object(at, cname, 0)
    }

    fn value(&self, at: i128, t: u8, sub: u8, mclass: Option<&str>, carray: i16, depth: usize) -> Option<Value> {
        if depth > MAX_DEPTH {
            return None;
        }
        if carray != 0 {
            let mut out = Vec::new();
            if carray > 0 {
                let size = self.ix.elem_size(t, mclass)?;
                for i in 0..i128::from(carray) {
                    out.push(self.value(at + i * size, t, sub, mclass, 0, depth + 1)?);
                }
            }
            return Some(Value::List(out));
        }
        if let Some((signed, n)) = simple(t) {
            return Some(match t {
                T_REAL => Value::Float(self.f32(at)?),
                T_BOOL => Value::Bool(*self.bytes(at, 1)?.first()? != 0),
                _ => Value::Int(self.int(at, n, signed)?),
            });
        }
        if let Some(n) = vec_len(t) {
            let mut v = Vec::with_capacity(n);
            for k in 0..n {
                v.push(self.f32(at + 4 * k as i128)?);
            }
            return Some(Value::Floats(v));
        }
        Some(match t {
            T_ENUM | T_FLAGS => {
                let (signed, n) = match sub {
                    T_INT8 => (true, 1),
                    T_UINT8 => (false, 1),
                    T_INT16 => (true, 2),
                    T_UINT16 => (false, 2),
                    _ => (true, 4),
                };
                Value::Int(self.int(at, n, signed)?)
            }
            T_POINTER => match self.deref(at) {
                Some(target) if target != 0 => Value::Ptr(target, self.ix.class_of(target).map(str::to_owned)),
                _ => Value::None,
            },
            T_CSTRING => match self.deref(at) {
                Some(t) if t != 0 => Value::Str(cstr(self.d, t)?),
                _ => Value::None,
            },
            T_STRUCT => self.object(at, mclass, depth + 1)?,
            T_ARRAY | T_SIMPLEARRAY => {
                let data_p = self.deref(at);
                let n = self.int(at + 4, 4, true)?;
                match data_p {
                    Some(p) if p != 0 && n > 0 => match self.ix.elem_size(sub, mclass) {
                        None => Value::List(vec![Value::Unknown(i32::try_from(n).unwrap_or(0))]),
                        Some(size) => {
                            let mut out = Vec::new();
                            for i in 0..n {
                                out.push(self.value(i128::from(p) + i * size, sub, 0, mclass, 0, depth + 1)?);
                            }
                            Value::List(out)
                        }
                    },
                    _ => Value::List(Vec::new()),
                }
            }
            _ => Value::None,
        })
    }

    fn object(&self, at: i128, cname: Option<&str>, depth: usize) -> Option<Value> {
        let mut out: Vec<(String, Value)> = Vec::new();
        for m in self.ix.all_members(cname)? {
            if matches!(m.kind, T_ZERO | T_VOID | T_FUNCTIONPOINTER) {
                continue;
            }
            let v = self.value(
                at + i128::from(m.offset),
                m.kind,
                m.subtype,
                self.ix.member_class_name(m.class),
                m.carray,
                depth,
            )?;
            if let Some(slot) = out.iter_mut().find(|(k, _)| *k == m.name) {
                slot.1 = v;
            } else {
                out.push((m.name.clone(), v));
            }
        }
        Some(Value::Dict(out))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::havok::tests::sample;

    #[test]
    fn walks_the_sample_and_refuses_reads_past_the_end() {
        let file = sample().build(b"");
        let pf = Packfile::parse(&file).unwrap();
        let ix = Index::new(&pf);
        let r = Reader::new(&file, &ix);
        // no class descriptions: every object reads as an empty dict
        for (off, c) in pf.objects() {
            assert_eq!(r.read_object(i128::from(*off), Some(c)), Some(Value::Dict(Vec::new())));
        }
        assert_eq!(
            r.read_value(0, T_INT32, 0, None, 0),
            Some(Value::Int(i128::from(i32::from_le_bytes(
                file[..4].try_into().unwrap()
            ))))
        );
        assert_eq!(r.read_value(-4, T_INT32, 0, None, 0), None);
        assert_eq!(r.read_value(file.len() as i128 - 2, T_INT32, 0, None, 0), None);
        assert_eq!(r.read_value(0, T_VOID, 0, None, 0), Some(Value::None));
        assert_eq!(r.read_value(0, T_STRUCT, 0, None, 3), None);
        assert_eq!(r.read_value(0, T_INT8, 0, None, -2), Some(Value::List(Vec::new())));
        assert_eq!(cstr(b"ab\xffc\0", 0).as_deref(), Some("ab\u{fffd}c"));
        assert_eq!(cstr(b"abc", 0), None);
    }
}
