//! The Havok class layouts BFBC2Game.exe carries (its hkClass initializers), a port of
//! `bfbc2-console-convert/havok.py`'s `classes` and its `_PE` reader (round F0).
//!
//! An initializer is a small function that pushes the constructor's 12 arguments (name, parent, size, interfaces,
//! count, enums, count, members, count, defaults, attributes, flags), loads the static hkClass object into `ecx` and
//! calls the constructor; the members are 24-byte hkClassMember records. The same classes in the same order as the
//! reference's dict, with the same members; where the reference raises (no initializer found, a member table outside
//! the file's sections, a header cut short) this returns `None` and the Python reads the executable itself.

use std::collections::HashMap;

/// One member: name, class, type, subtype, C array size, flags, offset.
pub type Member = (Option<String>, Option<String>, u8, u8, i16, u16, u16);

/// One class: name, parent, size, members.
pub type Klass = (String, Option<String>, u32, Vec<Member>);

/// The sections of a 32-bit PE image as `_PE` keeps them: (virtual address, size, file offset, raw size).
struct Pe<'a> {
    d: &'a [u8],
    secs: Vec<(u64, u64, u64, u64)>,
}

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    d.get(at..at + 2)?.try_into().ok().map(u16::from_le_bytes)
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    d.get(at..at + 4)?.try_into().ok().map(u32::from_le_bytes)
}

impl<'a> Pe<'a> {
    fn new(d: &'a [u8]) -> Option<Self> {
        let pe = u32_at(d, 0x3C)? as usize;
        let nsec = u16_at(d, pe + 6)?;
        let opt = usize::from(u16_at(d, pe + 20)?);
        let base = u64::from(u32_at(d, pe + 52)?);
        let mut secs = Vec::with_capacity(usize::from(nsec));
        for i in 0..usize::from(nsec) {
            let o = pe + 24 + opt + 40 * i + 8;
            let (vsize, va, rsize, raw) = (u32_at(d, o)?, u32_at(d, o + 4)?, u32_at(d, o + 8)?, u32_at(d, o + 12)?);
            secs.push((
                base + u64::from(va),
                u64::from(vsize.max(rsize)),
                u64::from(raw),
                u64::from(rsize),
            ));
        }
        Some(Self { d, secs })
    }

    fn off(&self, va: i64) -> Option<usize> {
        let va = u64::try_from(va).ok()?;
        for &(s, size, raw, rsize) in &self.secs {
            if s <= va && va < s + size {
                return (va - s < rsize).then(|| (raw + va - s) as usize);
            }
        }
        None
    }

    fn va(&self, off: usize) -> Option<i64> {
        let off = off as u64;
        self.secs
            .iter()
            .find(|&&(_, _, raw, rsize)| raw <= off && off < raw + rsize)
            .map(|&(s, _, raw, _)| (s + off - raw) as i64)
    }

    /// The printable ASCII C string at `va`, or `None` (the reference's `cstr`: a string without its NUL runs to the
    /// byte before the file's end, as `d[o:-1]` does).
    fn cstr(&self, va: i64) -> Option<String> {
        if va == 0 {
            return None;
        }
        let o = self.off(va)?;
        let rest = self.d.get(o..)?;
        let end = rest
            .iter()
            .position(|&c| c == 0)
            .map_or(self.d.len().checked_sub(1)?, |p| o + p);
        let s = self.d.get(o..end.max(o))?;
        (!s.is_empty() && s.iter().all(|&c| (32..127).contains(&c))).then(|| s.iter().map(|&c| char::from(c)).collect())
    }
}

/// `_pushes(d, start, end)`: the immediates pushed from `start` to `end` (push imm8 / imm32 only), first argument
/// first; `None` for anything else.
fn pushes(d: &[u8], start: usize, end: usize) -> Option<Vec<i64>> {
    let mut args = Vec::new();
    let mut p = start;
    while p < end {
        match d.get(p)? {
            0x6A => {
                args.push(i64::from(i32::from(*d.get(p + 1)? as i8) as u32));
                p += 2;
            }
            0x68 => {
                args.push(i64::from(u32_at(d, p + 1)?));
                p += 5;
            }
            _ => return None,
        }
    }
    args.reverse();
    Some(args)
}

/// A class as the scan first reads it: name, parent object, size, members with their class objects.
type RawClass = (
    Option<String>,
    i64,
    i64,
    Vec<(Option<String>, i64, u8, u8, i16, u16, u16)>,
);

/// `havok.classes(exe)` of the executable's bytes, in the reference's dict order, or `None` where it raises.
#[must_use]
#[allow(clippy::many_single_char_names, clippy::too_many_lines)] // the reference's names, in its order
pub fn classes(d: &[u8]) -> Option<Vec<Klass>> {
    let pe = Pe::new(d)?;
    let mut sites: Vec<(i64, u32, Vec<i64>)> = Vec::new();
    let mut ctor_order: Vec<i64> = Vec::new();
    let mut ctors: HashMap<i64, usize> = HashMap::new();
    let limit = d.len().checked_sub(10)?;
    let mut i = 0;
    while i < limit {
        let Some(k) = d.get(i..limit)?.iter().position(|&c| c == 0xB9) else {
            break;
        };
        i += k;
        if i > 0
            && *d.get(i + 5)? == 0xE8
            && let Some(here) = pe.va(i)
        {
            let lo = i.saturating_sub(80);
            let mut s = i - 1;
            while s > lo {
                if matches!(d.get(s - 1), Some(0xCC | 0xC3))
                    && let Some(a) = pushes(d, s, i)
                    && a.len() == 12
                    && let Some(name) = a.first().and_then(|&v| pe.cstr(v))
                {
                    let rel = i64::from(i32::from_le_bytes(d.get(i + 6..i + 10)?.try_into().ok()?));
                    let target = here + 10 + rel;
                    if name.starts_with("hk") {
                        let c = ctors.entry(target).or_insert(0);
                        if *c == 0 {
                            ctor_order.push(target);
                        }
                        *c += 1;
                    }
                    sites.push((target, u32_at(d, i + 1)?, a));
                    break;
                }
                s -= 1;
            }
        }
        i += 1;
    }
    // Counter.most_common(1): the highest count, the first counted among equals
    let mut ctor = None;
    for t in &ctor_order {
        let c = ctors.get(t).copied().unwrap_or(0);
        if ctor.is_none_or(|(_, best)| c > best) {
            ctor = Some((*t, c));
        }
    }
    let (ctor, _) = ctor?;
    // raw: {obj: (name, parent, size, members)} in first-insertion order, a later site's value winning
    let mut order: Vec<u32> = Vec::new();
    let mut raw: HashMap<u32, RawClass> = HashMap::new();
    for (target, obj, a) in &sites {
        if *target != ctor {
            continue;
        }
        let (name, parent, size, members, count) = (*a.first()?, *a.get(1)?, *a.get(2)?, *a.get(7)?, *a.get(8)?);
        let mut mem = Vec::new();
        for k in 0..count {
            let o = pe.off(members + 24 * k)?;
            let (nm, cl) = (u32_at(d, o)?, u32_at(d, o + 4)?);
            u32_at(d, o + 8)?;
            let b = d.get(o + 12..o + 20)?;
            let (ty, st) = (*b.first()?, *b.get(1)?);
            let ca = i16::from_le_bytes(b.get(2..4)?.try_into().ok()?);
            let fl = u16::from_le_bytes(b.get(4..6)?.try_into().ok()?);
            let off = u16::from_le_bytes(b.get(6..8)?.try_into().ok()?);
            mem.push((pe.cstr(i64::from(nm)), i64::from(cl), ty, st, ca, fl, off));
        }
        if !raw.contains_key(obj) {
            order.push(*obj);
        }
        raw.insert(*obj, (pe.cstr(name), parent, size, mem));
    }
    let name_of = |v: i64| -> Option<String> {
        u32::try_from(v)
            .ok()
            .and_then(|k| raw.get(&k))
            .and_then(|r| r.0.clone())
    };
    let in_raw = |v: i64| u32::try_from(v).is_ok_and(|k| raw.contains_key(&k));
    let mut out: Vec<Klass> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for obj in &order {
        let (name, parent, size, mem) = raw.get(obj)?;
        let name = name.clone()?;
        let mem: Vec<Member> = mem
            .iter()
            .map(|(n, cl, ty, st, ca, fl, off)| {
                (
                    n.clone(),
                    if in_raw(*cl) { name_of(*cl) } else { None },
                    *ty,
                    *st,
                    *ca,
                    *fl,
                    *off,
                )
            })
            .collect();
        let k: Klass = (
            name.clone(),
            if in_raw(*parent) { name_of(*parent) } else { None },
            u32::try_from(*size).ok()?,
            mem,
        );
        if let Some(&j) = at.get(&name) {
            *out.get_mut(j)? = k;
        } else {
            at.insert(name, out.len());
            out.push(k);
        }
    }
    Some(out)
}
