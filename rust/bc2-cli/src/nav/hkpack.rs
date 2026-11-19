//! `bfbc2 hkpack classes|dump <file> ...`: NavKit's `bfbc2-auto-navkit\tools\hkpack.py` command line over
//! `bc2_formats::havok::Packfile` (the sections, fixups, objects and class descriptions), with the reflection walk
//! of `Packfile.read_object` / `read_value` and `short()` ported here.
//!
//! Where the core refuses the file, or the walk would raise in Python (a read past the data, a C string without an
//! end, a parent class that is not described, a C array of a type without a size, nesting deeper than Python's
//! recursion allows), the command line is handed to the Python.

#![allow(
    clippy::indexing_slicing,
    reason = "the arguments after their count is checked; classes by the positions the offset map holds; 4-byte \
              reads after the slice is taken; the objects' head within its length"
)]

use std::collections::HashMap;

use bc2_formats::havok::{Class, Packfile};

use super::common::{arg, head_len, py_int};
use crate::pyfmt::{docstring, float_g_prec};
use crate::pyproc::{Fail, Py, PyResult, read_file};

const SOURCE: &str = include_str!("../../../../bfbc2-auto-navkit/tools/hkpack.py");

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
const MAX_DEPTH: usize = 300;

/// A value `read_value` gives.
enum Value {
    Int(i128),
    Bool(bool),
    Float(f64),
    Floats(Vec<f64>),
    Ptr(u64, Option<String>),
    None,
    Str(String),
    List(Vec<Value>),
    Dict(Vec<(String, Value)>),
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

struct Walk<'a> {
    pf: &'a Packfile<'a>,
    d: &'a [u8],
    classes: Vec<(u64, &'a Class)>,
    by_off: HashMap<u64, usize>,
    by_name: HashMap<&'a str, u64>,
    objects: HashMap<u64, &'a str>,
}

impl<'a> Walk<'a> {
    fn new(pf: &'a Packfile<'a>) -> Self {
        // the dicts of the reference: classes by offset (first place, last value), names and objects (last wins)
        let mut classes: Vec<(u64, &Class)> = Vec::new();
        let mut by_off = HashMap::new();
        for (off, c) in pf.classes() {
            if let Some(&k) = by_off.get(off) {
                classes[k] = (*off, c);
            } else {
                by_off.insert(*off, classes.len());
                classes.push((*off, c));
            }
        }
        let by_name = classes.iter().map(|(o, c)| (c.name.as_str(), *o)).collect();
        let objects = pf.objects().iter().map(|(o, c)| (*o, c.as_str())).collect();
        Self {
            pf,
            d: pf.data(),
            classes,
            by_off,
            by_name,
            objects,
        }
    }

    fn class(&self, off: u64) -> Option<&'a Class> {
        self.by_off.get(&off).map(|&k| self.classes[k].1)
    }

    fn bytes(&self, at: i128, n: usize) -> PyResult<&'a [u8]> {
        let a = usize::try_from(at).map_err(|_| Fail::Python)?;
        self.d.get(a..a.checked_add(n).ok_or(Fail::Python)?).ok_or(Fail::Python)
    }

    fn int(&self, at: i128, n: usize, signed: bool) -> PyResult<i128> {
        let b = self.bytes(at, n)?;
        let mut v: u64 = 0;
        for (k, &x) in b.iter().enumerate() {
            v |= u64::from(x) << (8 * k);
        }
        let bits = 8 * n;
        Ok(if signed && bits < 64 && v >> (bits - 1) & 1 == 1 {
            i128::from(v) - (1i128 << bits)
        } else if signed && bits == 64 {
            i128::from(v.cast_signed())
        } else {
            i128::from(v)
        })
    }

    fn f32(&self, at: i128) -> PyResult<f64> {
        let b = self.bytes(at, 4)?;
        Ok(f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]])))
    }

    fn deref(&self, at: i128) -> Option<u64> {
        u64::try_from(at).ok().and_then(|a| self.pf.deref(a))
    }

    fn cstr(&self, off: u64) -> PyResult<String> {
        let o = usize::try_from(off).map_err(|_| Fail::Python)?;
        let rest = self.d.get(o..).ok_or(Fail::Python)?;
        let e = rest.iter().position(|&b| b == 0).ok_or(Fail::Python)?;
        Ok(rest[..e]
            .iter()
            .map(|&b| if b < 0x80 { char::from(b) } else { '\u{fffd}' })
            .collect())
    }

    fn all_members(&self, cname: Option<&str>) -> PyResult<Vec<&'a bc2_formats::havok::ClassMember>> {
        let mut off = cname.and_then(|n| self.by_name.get(n).copied());
        let mut chain = Vec::new();
        while let Some(o) = off {
            let c = self.class(o).ok_or(Fail::Python)?;
            chain.push(c);
            if chain.len() > 10_000 {
                return Err(Fail::Python);
            }
            off = c.parent;
        }
        Ok(chain.iter().rev().flat_map(|c| c.members.iter()).collect())
    }

    fn member_class_name(&self, class: Option<u64>) -> Option<&'a str> {
        class.and_then(|o| self.class(o)).map(|c| c.name.as_str())
    }

    fn struct_size(&self, cname: Option<&str>) -> Option<i128> {
        let off = self.by_name.get(cname?)?;
        self.class(*off).map(|c| i128::from(c.size))
    }

    fn elem_size(&self, t: u8, mclass: Option<&str>) -> Option<i128> {
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

    fn value(&self, at: i128, t: u8, sub: u8, mclass: Option<&str>, carray: i16, depth: usize) -> PyResult<Value> {
        if depth > MAX_DEPTH {
            return Err(Fail::Python);
        }
        if carray != 0 {
            let mut out = Vec::new();
            if carray > 0 {
                let size = self.elem_size(t, mclass).ok_or(Fail::Python)?;
                for i in 0..i128::from(carray) {
                    out.push(self.value(at + i * size, t, sub, mclass, 0, depth + 1)?);
                }
            }
            return Ok(Value::List(out));
        }
        if let Some((signed, n)) = simple(t) {
            return Ok(match t {
                T_REAL => Value::Float(self.f32(at)?),
                T_BOOL => Value::Bool(self.bytes(at, 1)?[0] != 0),
                _ => Value::Int(self.int(at, n, signed)?),
            });
        }
        if let Some(n) = vec_len(t) {
            let mut v = Vec::with_capacity(n);
            for k in 0..n {
                v.push(self.f32(at + 4 * k as i128)?);
            }
            return Ok(Value::Floats(v));
        }
        Ok(match t {
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
                Some(target) if target != 0 => Value::Ptr(target, self.objects.get(&target).map(|s| (*s).to_owned())),
                _ => Value::None,
            },
            T_CSTRING => match self.deref(at) {
                Some(t) if t != 0 => Value::Str(self.cstr(t)?),
                _ => Value::None,
            },
            T_STRUCT => self.object(at, mclass, depth + 1)?,
            T_ARRAY | T_SIMPLEARRAY => {
                let data_p = self.deref(at);
                let n = self.int(at + 4, 4, true)?;
                match data_p {
                    Some(p) if p != 0 && n > 0 => match self.elem_size(sub, mclass) {
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

    fn object(&self, at: i128, cname: Option<&str>, depth: usize) -> PyResult<Value> {
        let mut out: Vec<(String, Value)> = Vec::new();
        for m in self.all_members(cname)? {
            if matches!(m.kind, T_ZERO | T_VOID | T_FUNCTIONPOINTER) {
                continue;
            }
            let v = self.value(
                at + i128::from(m.offset),
                m.kind,
                m.subtype,
                self.member_class_name(m.class),
                m.carray,
                depth,
            )?;
            if let Some(slot) = out.iter_mut().find(|(k, _)| *k == m.name) {
                slot.1 = v;
            } else {
                out.push((m.name.clone(), v));
            }
        }
        Ok(Value::Dict(out))
    }
}

/// `short(v)`.
fn short(v: &Value) -> String {
    match v {
        Value::Dict(items) => {
            let parts: Vec<String> = items.iter().map(|(k, x)| format!("{k}: {}", short(x))).collect();
            format!("{{{}}}", parts.join(", "))
        }
        Value::List(items) if items.len() > 6 => {
            let parts: Vec<String> = items.iter().take(3).map(short).collect();
            format!("[{} items: {}, ...]", items.len(), parts.join(", "))
        }
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(short).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Ptr(target, name) => match name.as_deref() {
            Some(n) if !n.is_empty() => format!("->{n}"),
            _ => format!("->{target:#x}"),
        },
        Value::Floats(xs) => {
            let parts: Vec<String> = xs.iter().map(|x| format!("{x:.3}")).collect();
            format!("({})", parts.join(", "))
        }
        Value::Unknown(n) => format!("(?, {n})"),
        Value::Float(x) => float_g_prec(*x, 4),
        Value::Int(i) => i.to_string(),
        Value::Bool(b) => if *b { "True" } else { "False" }.to_owned(),
        Value::None => "None".to_owned(),
        Value::Str(s) => s.clone(),
    }
}

pub fn run(py: &mut Py, args: &[String]) -> PyResult<i32> {
    if args.len() < 2 {
        py.print(&docstring(SOURCE))?;
        return Ok(1);
    }
    let data = read_file(&args[1])?;
    let pf = Packfile::parse(&data).map_err(|_| Fail::Python)?;
    let w = Walk::new(&pf);
    match args[0].as_str() {
        "classes" => {
            py.print(&format!(
                "Havok packfile at 0x{:X}, version {}, {}, {} sections, {} class descriptions",
                pf.base,
                pf.version,
                pf.contents_version,
                pf.sections.len(),
                w.classes.len()
            ))?;
            // Counter(...).most_common(): by count, ties in the order first seen
            let mut counts: Vec<(&str, usize)> = Vec::new();
            for (_, c) in pf.objects() {
                if let Some(e) = counts.iter_mut().find(|e| e.0 == c) {
                    e.1 += 1;
                } else {
                    counts.push((c, 1));
                }
            }
            counts.sort_by_key(|e| std::cmp::Reverse(e.1));
            for (c, n) in counts {
                py.print(&format!("{n:7}  {c}"))?;
            }
        }
        "dump" => {
            let cname = arg(args, 2)?;
            let n = match args.get(3) {
                Some(s) => py_int(s)?,
                None => 2,
            };
            let offs: Vec<u64> = pf.objects_of(cname).collect();
            for &off in &offs[..head_len(offs.len(), n)] {
                let v = w.object(i128::from(off), Some(cname), 0)?;
                py.write(&format!("@0x{off:X}"))?;
                py.write(" ")?;
                py.write(&short(&v))?;
                py.write("\n")?;
            }
        }
        _ => {
            py.print(&docstring(SOURCE))?;
            return Ok(1);
        }
    }
    Ok(0)
}
