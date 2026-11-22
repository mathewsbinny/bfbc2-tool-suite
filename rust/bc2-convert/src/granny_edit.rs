//! The writing side of `bfbc2-anim-tools/granny.py` (round F2a): the CRC of a patched resource (`Granny.to_bytes`),
//! tracks added to an animation (`add_tracks`, `_Graft`: objects and type definitions copied from a donor, the
//! sections laid out again), and the values `shift_position` / `set_orientations` write into a curve.
//!
//! | Python (`granny.py`) | here |
//! |---|---|
//! | `Granny.to_bytes` (the CRC) | [`with_crc`] |
//! | `add_tracks`, `_Graft`, `_curve_templates`, `_member_at` | [`add_tracks`] |
//! | `shift_position` | [`shift_plan`]: the puts, in order, and the report |
//! | `set_orientations` | [`orientation_plan`] |
//!
//! The reference's rules to the byte: the graft's string table, allocations, relocations and type signatures are
//! made in the reference's order (a type's signature is memoised at the depth it is first met, a type that refers
//! to itself is `None` inside itself; `types` keeps the first reference of each signature), so the files are the
//! same. The plans compute in doubles as Python does (sums from 0, Python's `min` / `max` and `round`); the caller
//! applies their puts through `Granny.put` (its byte-order check, `struct`'s packing). Where the reference raises or
//! meets a value these do not mirror (a NaN, an overflow), they return `None` and the Python runs.

#![allow(
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::chunks_exact_to_as_chunks,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::float_cmp,
    clippy::manual_midpoint,
    clippy::approx_constant,
    clippy::option_option,
    clippy::type_complexity,
    clippy::similar_names,
    clippy::manual_bit_width,
    clippy::verbose_bit_mask,
    clippy::needless_range_loop,
    clippy::nonminimal_bool,
    clippy::result_unit_err,
    clippy::map_unwrap_or,
    clippy::format_push_string,
    clippy::unnested_or_patterns,
    clippy::unnecessary_wraps,
    clippy::doc_markdown,
    reason = "every index is checked against the data's length first (the walks' need(), the decoders' block counts, the plans' key counts); the reference's names, comparisons, casts and arithmetic as it has them (a float midpoint, an exact float compare and a table constant are its values, not approximations)"
)]

use std::collections::HashMap;
use std::rc::Rc;

use bc2_formats::granny::{Granny, Object, Ref, Text, Value, type_size};
use bc2_formats::zlib::crc32;

/// The resource with its Granny CRC recomputed (`Granny.to_bytes` of a PC resource): CRC-32 of the Granny file from
/// `crc_from` to `total` (cut at the data's end), written at the file's byte 40. `None` where the reference raises.
#[must_use]
pub fn with_crc(data: &[u8], gr2: usize, crc_from: usize, total: usize) -> Option<Vec<u8>> {
    let n = data.len();
    let a = gr2.saturating_add(crc_from).min(n);
    let b = gr2.saturating_add(total).min(n).max(a);
    let crc = crc32(&data[a..b]);
    let mut out = data.to_vec();
    let at = gr2.checked_add(40)?;
    out.get_mut(at..at + 4)?.copy_from_slice(&crc.to_le_bytes());
    Some(out)
}

#[derive(PartialEq, Eq, Hash, Debug)]
struct SigNode(Vec<(u32, Vec<u8>, Option<Sig>, u32)>);

type Sig = Rc<SigNode>;

fn text(g: &Granny<'_>, t: Text<'_>) -> Vec<u8> {
    g.text_bytes(t)
}

fn le32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// How a new track's curve is made (`add_tracks`' specs).
#[derive(Debug, Clone, PartialEq)]
pub enum Spec {
    /// `None`: identity (the donor's `DaIdentity`, its dimension set).
    Identity,
    /// `('constant', values)`.
    Constant(Vec<f32>),
    /// `('copy', curve)`: the donor's curve data at (type, object).
    Copy(Ref, (u32, u64)),
    /// Another first word: the donor's constant curve, its values as they are.
    Other,
}

/// A track to add: its name (latin-1), orientation and position curves.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// The name.
    pub name: Vec<u8>,
    /// The orientation curve.
    pub orientation: Spec,
    /// The position curve.
    pub position: Spec,
}

struct Graft<'a, 'g> {
    g: &'g Granny<'a>,
    sec: Vec<Vec<u8>>,
    reloc: HashMap<Ref, Ref>,
    strings: Vec<Vec<u8>>,
    index: HashMap<Vec<u8>, u32>,
    types: HashMap<Sig, Ref>,
    memo: HashMap<(u8, Ref), Option<Sig>>,
    type_sec: u32,
}

/// Which file a signature belongs to (the reference's `id(g)`).
const MINE: u8 = 0;
const DONOR: u8 = 1;

impl<'a, 'g> Graft<'a, 'g> {
    fn new(g: &'g Granny<'a>, root: &Value<'_>) -> Option<Self> {
        let mut index = HashMap::new();
        for (i, s) in g.strings.iter().enumerate().rev() {
            index.insert(s.to_vec(), i as u32);
        }
        let mut w = Self {
            g,
            sec: g.sections.iter().map(|s| s.to_vec()).collect(),
            reloc: g.reloc.iter().map(|(k, v)| (*k, *v)).collect(),
            strings: g.strings.iter().map(|s| s.to_vec()).collect(),
            index,
            types: HashMap::new(),
            memo: HashMap::new(),
            type_sec: g.root_type.0,
        };
        let mut todo = types_in_use(g, root)?;
        while let Some(r) = todo.pop() {
            let sig = w.signature(g, MINE, r, 0)?;
            if let std::collections::hash_map::Entry::Vacant(e) = w.types.entry(sig?) {
                e.insert(r);
                todo.extend(g.members(r).ok()?.iter().filter_map(|m| m.child));
            }
        }
        Some(w)
    }

    /// `signature(g, ref, depth)`: `Some(None)` for a type met inside itself.
    fn signature(&mut self, g: &Granny<'_>, file: u8, r: Ref, depth: usize) -> Option<Option<Sig>> {
        if let Some(s) = self.memo.get(&(file, r)) {
            return Some(s.clone());
        }
        if depth > 4096 {
            return None;
        }
        self.memo.insert((file, r), None);
        let mut out = Vec::new();
        for m in g.members(r).ok()?.iter() {
            let child = match m.child {
                Some(c) if depth < 32 => self.signature(g, file, c, depth + 1)?,
                _ => None,
            };
            out.push((m.kind, text(g, g.name(m.name)), child, m.arr));
        }
        let s = Some(Rc::new(SigNode(out)));
        self.memo.insert((file, r), s.clone());
        Some(s)
    }

    fn string(&mut self, s: Vec<u8>) -> u32 {
        if let Some(&i) = self.index.get(&s) {
            return i;
        }
        let i = self.strings.len() as u32;
        self.index.insert(s.clone(), i);
        self.strings.push(s);
        i
    }

    fn alloc(&mut self, sec: u32, size: usize) -> Option<u32> {
        let s = self.sec.get_mut(sec as usize)?;
        while !s.len().is_multiple_of(4) {
            s.push(0);
        }
        let at = s.len();
        s.resize(at + size, 0);
        u32::try_from(at).ok()
    }

    fn type_from(&mut self, d: &Granny<'_>, r: Option<Ref>) -> Option<Option<Ref>> {
        let Some(r) = r else { return Some(None) };
        let sig = self.signature(d, DONOR, r, 0)??;
        if let Some(&t) = self.types.get(&sig) {
            return Some(Some(t));
        }
        let n = d.members(r).ok()?.len();
        let ts = self.type_sec;
        let at = self.alloc(ts, 32 * (n + 1))?;
        self.types.insert(sig.clone(), (ts, at));
        let src = *d.sections.get(r.0 as usize)?;
        for i in 0..=n {
            let base = r.1 as usize + 32 * i;
            let mut words = [0u32; 8];
            for (k, w) in words.iter_mut().enumerate() {
                *w = le32(src, base + 4 * k)?;
            }
            if i < n {
                words[1] = self.string(text(d, d.name(words[1])));
                if let Some(child) = d.ptr(r.0, (base + 8) as u64) {
                    let t = self.type_from(d, Some(child))??;
                    self.reloc.insert((ts, at + 32 * i as u32 + 8), t);
                }
            }
            let dst = self.sec.get_mut(ts as usize)?;
            for (k, w) in words.iter().enumerate() {
                let p = at as usize + 32 * i + 4 * k;
                dst.get_mut(p..p + 4)?.copy_from_slice(&w.to_le_bytes());
            }
        }
        Some(Some(self.types[&sig]))
    }

    fn copy_bytes(&mut self, d: &Granny<'_>, from: (u32, u64), sec: u32, at: u32, size: usize) -> Option<()> {
        let src = d.sections.get(from.0 as usize)?;
        let a = usize::try_from(from.1).ok()?;
        let bytes = src.get(a..a.checked_add(size)?)?; // a short slice would shrink the section: left to the reference
        self.sec
            .get_mut(sec as usize)?
            .get_mut(at as usize..at as usize + size)?
            .copy_from_slice(bytes);
        Some(())
    }

    fn object_from(&mut self, d: &Granny<'_>, tref: Ref, oref: (u32, u64), sec: u32) -> Option<Ref> {
        let size = usize::try_from(d.size(tref).ok()?).ok()?;
        let at = self.alloc(sec, size)?;
        self.copy_bytes(d, oref, sec, at, size)?;
        self.fix(d, tref, oref, (sec, u64::from(at)), sec)?;
        Some((sec, at))
    }

    fn array_from(&mut self, d: &Granny<'_>, child: Ref, p: Ref, n: u32, sec: u32) -> Option<Ref> {
        let size = usize::try_from(d.size(child).ok()?).ok()?;
        let at = self.alloc(sec, size.checked_mul(n as usize)?)?;
        self.copy_bytes(d, (p.0, u64::from(p.1)), sec, at, size * n as usize)?;
        for k in 0..n as usize {
            self.fix(
                d,
                child,
                (p.0, u64::from(p.1) + (k * size) as u64),
                (sec, u64::from(at) + (k * size) as u64),
                sec,
            )?;
        }
        Some((sec, at))
    }

    fn put32(&mut self, at: (u32, u64), v: u32) -> Option<()> {
        let p = usize::try_from(at.1).ok()?;
        self.sec
            .get_mut(at.0 as usize)?
            .get_mut(p..p + 4)?
            .copy_from_slice(&v.to_le_bytes());
        Some(())
    }

    fn fix(&mut self, d: &Granny<'_>, tref: Ref, src: (u32, u64), dst: (u32, u64), sec: u32) -> Option<()> {
        let (mut so, mut dof) = (src.1, dst.1);
        for m in d.members(tref).ok()?.iter() {
            let n = u64::from(m.arr.max(1));
            let one = if m.kind == 1 {
                d.size(m.child?).ok()?
            } else {
                type_size(m.kind)
            };
            for k in 0..n {
                let s = (src.0, so + k * one);
                let o = (dst.0, dof + k * one);
                let key = |x: (u32, u64)| -> Option<Ref> { Some((x.0, u32::try_from(x.1).ok()?)) };
                match m.kind {
                    1 => self.fix(d, m.child?, s, o, sec)?,
                    2 => {
                        if let Some(p) = d.ptr(s.0, s.1) {
                            let r = self.object_from(d, m.child?, (p.0, u64::from(p.1)), sec)?;
                            self.reloc.insert(key(o)?, r);
                        }
                    }
                    3 | 4 | 7 => {
                        let head = if m.kind == 7 { 4 } else { 0 };
                        let cnt = le32(d.sections.get(s.0 as usize)?, usize::try_from(s.1 + head).ok()?)?;
                        let p = d.ptr(s.0, s.1 + head + 4);
                        let tp = if m.kind == 7 { d.ptr(s.0, s.1) } else { m.child };
                        if m.kind == 7 {
                            let t = self.type_from(d, tp)??; // a None relocation: the reference fails later
                            self.reloc.insert(key(o)?, t);
                        }
                        let Some(p) = p else { continue };
                        if cnt == 0 {
                            continue;
                        }
                        if m.kind == 4 {
                            let at = self.alloc(sec, 4 * cnt as usize)?;
                            for i in 0..cnt {
                                if let Some(q) = d.ptr(p.0, u64::from(p.1) + 4 * u64::from(i)) {
                                    let r = self.object_from(d, m.child?, (q.0, u64::from(q.1)), sec)?;
                                    self.reloc.insert((sec, at + 4 * i), r);
                                }
                            }
                            self.reloc.insert(key((o.0, o.1 + 4))?, (sec, at));
                        } else {
                            let r = self.array_from(d, tp?, p, cnt, sec)?;
                            self.reloc.insert(key((o.0, o.1 + head + 4))?, r);
                        }
                    }
                    5 => {
                        let (tp, p) = (d.ptr(s.0, s.1), d.ptr(s.0, s.1 + 4));
                        if let Some(tp) = tp {
                            let t = self.type_from(d, Some(tp))??;
                            self.reloc.insert(key(o)?, t);
                            if let Some(p) = p {
                                let r = self.object_from(d, tp, (p.0, u64::from(p.1)), sec)?;
                                self.reloc.insert(key((o.0, o.1 + 4))?, r);
                            }
                        }
                    }
                    8 => {
                        if let Some(p) = d.ptr(s.0, s.1) {
                            let ss = d.sections.get(p.0 as usize)?;
                            let from = p.1 as usize;
                            let end = ss.get(from..)?.iter().position(|&c| c == 0)?;
                            let txt = ss[from..=from + end].to_vec();
                            let at = self.alloc(sec, txt.len())?;
                            self.sec.get_mut(sec as usize)?[at as usize..at as usize + txt.len()].copy_from_slice(&txt);
                            self.reloc.insert(key(o)?, (sec, at));
                        } else {
                            let i = le32(d.sections.get(s.0 as usize)?, usize::try_from(s.1).ok()?)?;
                            let v = self.string(text(d, d.name(i)));
                            self.put32(o, v)?;
                        }
                    }
                    _ => {}
                }
            }
            so += one * n;
            dof += one * n;
        }
        Some(())
    }

    fn to_bytes(&self) -> Option<Vec<u8>> {
        let g = self.g;
        let b = g.data;
        let gg = g.gr2;
        let w = |at: usize| le32(b, gg + at);
        let (total, soff, count) = (w(36)? as usize, w(44)? as usize, w(48)? as usize);
        let mut rows: Vec<[u32; 11]> = Vec::with_capacity(count);
        for k in 0..count {
            let mut r = [0u32; 11];
            for (i, x) in r.iter_mut().enumerate() {
                *x = w(32 + soff + 44 * k + 4 * i)?;
            }
            rows.push(r);
        }
        let first = rows.iter().map(|r| r[1] as usize).chain([total]).min()?;
        let mut head = b.get(gg..(gg + first).min(b.len()))?.to_vec();
        let pos = head.len();
        let mut body = Vec::new();
        for k in 0..count {
            let s = self.sec.get(k)?;
            rows[k][1] = u32::try_from(pos + body.len()).ok()?;
            rows[k][2] = u32::try_from(s.len()).ok()?;
            rows[k][3] = rows[k][2];
            let old = g.sections.get(k)?.len() as u32;
            if k as u32 == self.type_sec && rows[k][5] == rows[k][6] && rows[k][6] == old {
                rows[k][5] = rows[k][2];
                rows[k][6] = rows[k][2];
            }
            body.extend_from_slice(s);
        }
        let mut by_sec: Vec<Vec<(u32, u32, u32)>> = vec![Vec::new(); count];
        #[allow(clippy::iter_over_hash_type, reason = "each section's list is sorted next")]
        for (&(s, o), &(ts, to)) in &self.reloc {
            by_sec.get_mut(s as usize)?.push((o, ts, to));
        }
        for (k, list) in by_sec.iter_mut().enumerate() {
            rows[k][7] = u32::try_from(pos + body.len()).ok()?;
            rows[k][8] = list.len() as u32;
            list.sort_unstable();
            for &(o, ts, to) in list.iter() {
                for v in [o, ts, to] {
                    body.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        for row in &mut rows {
            let (old_at, n) = (row[9] as usize, row[10] as usize);
            row[9] = u32::try_from(pos + body.len()).ok()?;
            let a = (gg + old_at).min(b.len());
            let e = (gg + old_at + 16 * n).min(b.len()).max(a);
            body.extend_from_slice(&b[a..e]);
        }
        for (k, r) in rows.iter().enumerate() {
            for (i, v) in r.iter().enumerate() {
                let p = 32 + soff + 44 * k + 4 * i;
                head.get_mut(p..p + 4)?.copy_from_slice(&v.to_le_bytes());
            }
        }
        let mut gr = head;
        gr.extend_from_slice(&body);
        let len = u32::try_from(gr.len()).ok()?;
        gr.get_mut(36..40)?.copy_from_slice(&len.to_le_bytes());
        let crc = crc32(gr.get(32 + soff..)?);
        gr.get_mut(40..44)?.copy_from_slice(&crc.to_le_bytes());
        let mut out = b.get(..g.strings_from)?.to_vec();
        out.extend_from_slice(&u32::try_from(self.strings.len()).ok()?.to_le_bytes());
        for s in &self.strings {
            out.extend_from_slice(&u16::try_from(s.len()).ok()?.to_le_bytes());
            out.extend_from_slice(s);
        }
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&gr);
        Some(out)
    }
}

/// `_types_in_use`: the type of every object of the tree, first met first (a dict's own type, then its members'
/// values in order; a repeated member name: its last value, as the dict keeps it).
fn types_in_use(g: &Granny<'_>, root: &Value<'_>) -> Option<Vec<Ref>> {
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let mut stack = vec![root];
    while let Some(v) = stack.pop() {
        match v {
            Value::Object(o) => {
                if seen.insert(o.tref) {
                    out.push(o.tref);
                }
                let fields = dict_fields(g, o);
                for (name, x) in fields.iter().rev() {
                    if !name.starts_with(b"__") {
                        stack.push(x);
                    }
                }
            }
            Value::List(items) => stack.extend(items.iter().rev()),
            _ => {}
        }
    }
    Some(out)
}

/// An object's members as Python's dict has them: each name once, at its first place, with its last value.
fn dict_fields<'v, 'a>(g: &Granny<'_>, o: &'v Object<'a>) -> Vec<(Vec<u8>, &'v Value<'a>)> {
    let mut out: Vec<(Vec<u8>, &Value<'a>)> = Vec::with_capacity(o.fields.len());
    for (t, v) in &o.fields {
        let name = g.text_bytes(*t);
        if let Some(slot) = out.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = v;
        } else {
            out.push((name, v));
        }
    }
    out
}

fn field<'v, 'a>(g: &Granny<'_>, o: &'v Object<'a>, name: &[u8]) -> Option<&'v Value<'a>> {
    dict_fields(g, o).into_iter().find(|(n, _)| n == name).map(|(_, v)| v)
}

fn object<'v, 'a>(v: &'v Value<'a>) -> Option<&'v Object<'a>> {
    match v {
        Value::Object(o) => Some(o),
        _ => None,
    }
}

fn list<'v, 'a>(v: &'v Value<'a>) -> Option<&'v [Value<'a>]> {
    match v {
        Value::List(l) => Some(l),
        _ => None,
    }
}

/// `_member_at(g, tref, name)`: the offset of a member in its object.
fn member_at(g: &Granny<'_>, tref: Ref, name: &[u8]) -> Option<u64> {
    let mut off = 0u64;
    for m in g.members(tref).ok()?.iter() {
        if g.text_bytes(g.name(m.name)) == name {
            return Some(off);
        }
        let one = if m.kind == 1 {
            g.size(m.child?).ok()?
        } else {
            type_size(m.kind)
        };
        off = off.checked_add(one.checked_mul(u64::from(m.arr.max(1)))?)?;
    }
    None
}

fn animations<'v, 'a>(g: &Granny<'_>, root: &'v Value<'a>) -> Vec<&'v Value<'a>> {
    object(root)
        .and_then(|o| field(g, o, b"Animations"))
        .and_then(list)
        .map(|l| l.iter().collect())
        .unwrap_or_default()
}

/// `_curve_templates(d)`: the first curve data of each format in the donor, by format name.
fn curve_templates(d: &Granny<'_>, root: &Value<'_>) -> Option<HashMap<Vec<u8>, (Ref, (u32, u64))>> {
    let mut out = HashMap::new();
    for an in animations(d, root) {
        for tg in list(field(d, object(an)?, b"TrackGroups")?)? {
            for tr in list(field(d, object(tg)?, b"TransformTracks")?)? {
                for c in [&b"PositionCurve"[..], b"OrientationCurve", b"ScaleShearCurve"] {
                    let curve = object(field(d, object(tr)?, c)?)?;
                    let Some(Value::Object(cd)) = field(d, curve, b"CurveData") else {
                        continue;
                    };
                    let ty = cd.type_name.clone()?;
                    let name = replace_all(&ty, b"CurveDataHeader_", b"");
                    out.entry(name).or_insert((cd.tref, cd.at));
                }
            }
        }
    }
    Some(out)
}

fn replace_all(s: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

/// `add_tracks(pc, tracks, donor)`: the animation with the tracks added to its first track group (name order), the
/// curves made from the donor's; `None` where the reference raises (or would meet what this does not mirror).
#[must_use]
#[expect(clippy::too_many_lines, reason = "the reference's add_tracks, step for step")]
pub fn add_tracks(pc: &[u8], tracks: &[Track], donor: &[u8]) -> Option<Vec<u8>> {
    let g = Granny::parse(pc).ok()?;
    let d = Granny::parse(donor).ok()?;
    if g.endian.big {
        return None;
    }
    let root = g.root(64).ok()?;
    let mut w = Graft::new(&g, &root)?;
    let an = animations(&g, &root);
    let an0 = object(an.first()?)?;
    let tgs = list(field(&g, an0, b"TrackGroups")?)?;
    let tg = object(tgs.first()?)?;
    let old = list(field(&g, tg, b"TransformTracks")?)?;
    let mut old_names = Vec::with_capacity(old.len());
    for t in old {
        let Some(Value::Text(n)) = field(&g, object(t)?, b"Name") else {
            return None;
        };
        old_names.push(g.text_bytes(*n));
    }
    if tracks.iter().any(|t| old_names.contains(&t.name)) {
        return None;
    }
    let lod = match field(&g, tg, b"TransformLODErrors") {
        None | Some(Value::None) => 0,
        Some(Value::List(l)) => l.len(),
        Some(Value::Floats(l)) => l.len(),
        Some(Value::Ints(l)) => l.len(),
        _ => return None,
    };
    if lod != 0 && lod != old.len() {
        return None;
    }
    let droot = d.root(64).ok()?;
    let tmpl = curve_templates(&d, &droot)?;
    for need in [&b"DaIdentity"[..], b"D3Constant32f", b"D4Constant32f"] {
        tmpl.get(need)?;
    }
    let ttype = object(old.first()?)?.tref;
    let tsize = usize::try_from(g.size(ttype).ok()?).ok()?;
    let at_name = member_at(&g, ttype, b"Name")?;
    let at_flags = member_at(&g, ttype, b"Flags")?;
    let at_curve = [
        member_at(&g, ttype, b"OrientationCurve")?,
        member_at(&g, ttype, b"PositionCurve")?,
        member_at(&g, ttype, b"ScaleShearCurve")?,
    ];
    let mut rows: Vec<(Vec<u8>, Result<&Object<'_>, &Track>)> = old
        .iter()
        .zip(&old_names)
        .map(|(t, n)| Some((n.clone(), Ok(object(t)?))))
        .collect::<Option<_>>()?;
    rows.extend(tracks.iter().map(|t| (t.name.clone(), Err(t))));
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let arr = w.alloc(0, tsize.checked_mul(rows.len())?)?;
    for (i, (name, kind)) in rows.iter().enumerate() {
        let at = arr as usize + i * tsize;
        match kind {
            Ok(t) => {
                let (sec, off) = t.at;
                let src = g.sections.get(sec as usize)?;
                let off = usize::try_from(off).ok()?;
                let bytes = src.get(off..off + tsize)?;
                w.sec.get_mut(0)?.get_mut(at..at + tsize)?.copy_from_slice(bytes);
                for k in (0..tsize).step_by(4) {
                    if let Some(r) = g.ptr(sec, (off + k) as u64) {
                        w.reloc.insert((0, (at + k) as u32), r);
                    }
                }
            }
            Err(t) => {
                let ni = w.string(name.clone());
                w.put32((0, at as u64 + at_name), ni)?;
                w.put32((0, at as u64 + at_flags), 0)?;
                let specs = [(&t.orientation, 4usize), (&t.position, 3), (&Spec::Identity, 9)];
                for (c, (spec, dim)) in specs.into_iter().enumerate() {
                    let (tref, cat) = match spec {
                        Spec::Identity => tmpl[&b"DaIdentity"[..]],
                        Spec::Copy(tr, a) => (*tr, *a),
                        Spec::Constant(_) | Spec::Other => {
                            tmpl[if dim == 3 {
                                &b"D3Constant32f"[..]
                            } else {
                                b"D4Constant32f"
                            }]
                        }
                    };
                    let r = w.object_from(&d, tref, cat, 0)?;
                    match spec {
                        Spec::Identity => {
                            let p = r.1 as usize + usize::try_from(member_at(&d, tref, b"Dimension")?).ok()?;
                            w.sec
                                .get_mut(r.0 as usize)?
                                .get_mut(p..p + 2)?
                                .copy_from_slice(&(dim as i16).to_le_bytes());
                        }
                        Spec::Constant(v) => {
                            if v.len() != dim {
                                return None;
                            }
                            let p = r.1 as usize + usize::try_from(member_at(&d, tref, b"Controls")?).ok()?;
                            let s = w.sec.get_mut(r.0 as usize)?;
                            for (k, x) in v.iter().enumerate() {
                                s.get_mut(p + 4 * k..p + 4 * k + 4)?.copy_from_slice(&x.to_le_bytes());
                            }
                        }
                        _ => {}
                    }
                    let tp = w.type_from(&d, Some(tref))??;
                    let base = (at as u64 + at_curve[c]) as u32;
                    w.reloc.insert((0, base), tp);
                    w.reloc.insert((0, base + 4), r);
                }
            }
        }
    }
    let (sec, off) = tg.at;
    let off = off.checked_add(member_at(&g, tg.tref, b"TransformTracks")?)?;
    w.put32((sec, off), u32::try_from(rows.len()).ok()?)?;
    w.reloc.insert((sec, u32::try_from(off + 4).ok()?), (0, arr));
    w.to_bytes()
}

/// A put of a plan: where (a member of the curve, or element `k` of one of its arrays), the `struct` format and
/// the values.
#[derive(Debug, Clone, PartialEq)]
pub struct Put {
    /// The member (`field`) or array (`array_at`) it writes.
    pub name: &'static str,
    /// `Some(byte offset)` into the array's elements; `None`: the member itself.
    pub element: Option<u64>,
    /// The `struct` format.
    pub fmt: &'static str,
    /// The values (integers as exact doubles).
    pub values: Vec<f64>,
}

/// The curve values a plan reads (`curve['...']`; `None`: the key is missing).
#[derive(Debug, Clone, Default)]
pub struct Curve {
    /// `__type__`.
    pub kind: Vec<u8>,
    /// `Controls`.
    pub controls: Option<Vec<f64>>,
    /// `ControlOffsets`.
    pub offsets: Option<Vec<f64>>,
    /// `ControlScales`.
    pub scales: Option<Vec<f64>>,
    /// `KnotsControls` (integers).
    pub knots_controls: Option<Vec<i64>>,
    /// `Knots`.
    pub knots: Option<Vec<f64>>,
    /// `OneOverKnotScaleTrunc` (`curve_keys` divides the knots by it).
    pub trunc: Option<i64>,
    /// `KnotsControls` as numbers (a float format's too: `D3I1K32fC32f`).
    pub kc: Option<Vec<f64>>,
    /// `ScaleOffsetTableEntries` (the quantized orientations' selectors).
    pub selectors: Option<i64>,
    /// `OneOverKnotScale` (the quantized orientations' knot scale).
    pub knot_scale: Option<f64>,
}

fn ends(s: &[u8], tails: &[&[u8]]) -> bool {
    tails.iter().any(|t| s.ends_with(t))
}

/// Python's `max(a, b)` and `min(a, b)` on floats (the first unless the second compares greater / less).
fn py_max(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

fn py_min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// `sum(xs)` of floats from the integer 0, as CPython 3.12+ adds them: the first added to 0, the rest with
/// Neumaier's compensation, the compensation added at the end when it is finite and not zero (`builtin_sum_impl`).
fn py_sum(xs: impl Iterator<Item = f64>) -> f64 {
    let mut it = xs;
    let Some(first) = it.next() else { return 0.0 };
    let mut s = 0.0 + first;
    let mut c = 0.0f64;
    for x in it {
        let t = s + x;
        if s.abs() >= x.abs() {
            c += (s - t) + x;
        } else {
            c += (x - t) + s;
        }
        s = t;
    }
    if c != 0.0 && c.is_finite() {
        s += c;
    }
    s
}

/// `round(x)` to an integer, or `None` for a NaN / infinity (Python raises).
fn py_round(x: f64) -> Option<i64> {
    if !x.is_finite() {
        return None;
    }
    let r = x.round_ties_even();
    (r.abs() < 9.0e18).then_some(r as i64)
}

/// `shift_position(g, curve, delta, ramp)`'s puts, in order, and its report; `None` where the reference raises.
#[must_use]
#[expect(clippy::too_many_lines, reason = "the reference's function, branch by branch")]
pub fn shift_plan(c: &Curve, delta: [f64; 3], ramp: bool) -> Option<(Vec<Put>, String)> {
    if delta.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let t = c.kind.as_slice();
    let put3 = |name: &'static str, v: Vec<f64>| Put {
        name,
        element: None,
        fmt: "<3f",
        values: v,
    };
    if t.ends_with(b"D3Constant32f") {
        let ctl = c.controls.as_ref()?;
        let v: Vec<f64> = (0..3).map(|i| Some(ctl.get(i)? + delta[i])).collect::<Option<_>>()?;
        return Some((vec![put3("Controls", v)], "constant moved".into()));
    }
    let quant = ends(t, &[b"D3K16uC16u", b"D3K8uC8u"]);
    if ends(t, &[b"D3I1K8uC8u", b"D3I1K16uC16u", b"D3I1K32fC32f"]) || (quant && !ramp) {
        let of = c.offsets.as_ref()?;
        let v: Vec<f64> = (0..3).map(|i| Some(of.get(i)? + delta[i])).collect::<Option<_>>()?;
        let what = if ramp {
            "offsets moved (one control for all axes: the whole curve moves)"
        } else {
            "offsets moved"
        };
        return Some((vec![put3("ControlOffsets", v)], what.into()));
    }
    let unit = |d: f64| -> [f64; 3] { [delta[0] / d, delta[1] / d, delta[2] / d] };
    let length = {
        let s = py_sum(delta.iter().map(|x| x * x)).sqrt();
        if s == 0.0 { 1.0 } else { s }
    };
    if quant {
        let wide = t.ends_with(b"D3K16uC16u");
        let qmax = if wide { 65535.0 } else { 255.0 };
        let kc = c.knots_controls.as_ref()?;
        let n = kc.len() / 4;
        let sc = c.scales.as_ref()?;
        let of = c.offsets.as_ref()?;
        if sc.len() < 3 || of.len() < 3 || kc.len() < n + 3 * n {
            return None;
        }
        // curve_keys divides each knot by the truncated scale: a zero is the reference's ZeroDivisionError
        let s = f32::from_bits(((c.trunc? & 0xFFFF) << 16) as u32);
        if s == 0.0 {
            return None;
        }
        // curve_keys' values (the knots' times are not used here)
        let keys: Vec<[f64; 3]> = (0..n)
            .map(|i| std::array::from_fn(|j| kc[n + 3 * i + j] as f64 * sc[j] + of[j]))
            .collect();
        if n == 0 {
            return None;
        }
        let u = unit(length);
        let dot = |v: &[f64; 3]| py_sum((0..3).map(|i| v[i] * u[i]));
        let (a, b) = (dot(&keys[0]), dot(&keys[n - 1]));
        let mut vals = Vec::with_capacity(n);
        for v in &keys {
            let w = if b == a {
                1.0
            } else {
                py_min(py_max((dot(v) - a) / (b - a), 0.0), 1.0)
            };
            vals.push([v[0] + delta[0] * w, v[1] + delta[1] * w, v[2] + delta[2] * w]);
        }
        let mut scales = [0f64; 3];
        let mut offsets = [0f64; 3];
        for i in 0..3 {
            let (mut lo, mut hi) = (vals[0][i], vals[0][i]);
            for v in &vals[1..] {
                lo = py_min(lo, v[i]);
                hi = py_max(hi, v[i]);
            }
            scales[i] = if hi > lo { (hi - lo) / qmax } else { 0.0 };
            offsets[i] = lo;
        }
        if vals.iter().flatten().any(|x| x.is_nan()) {
            return None;
        }
        let size = if wide { 2 } else { 1 };
        let mut puts = Vec::with_capacity(3 * n + 2);
        for (k, v) in vals.iter().enumerate() {
            for i in 0..3 {
                let q = if scales[i] == 0.0 {
                    0
                } else {
                    py_round((v[i] - offsets[i]) / scales[i])?
                };
                let q = q.min(qmax as i64).max(0);
                puts.push(Put {
                    name: "KnotsControls",
                    element: Some(((n + 3 * k + i) * size) as u64),
                    fmt: if wide { "<H" } else { "<B" },
                    values: vec![q as f64],
                });
            }
        }
        puts.push(put3("ControlScales", scales.to_vec()));
        puts.push(put3("ControlOffsets", offsets.to_vec()));
        return Some((puts, format!("{n} keys re-quantized (ramp)")));
    }
    if t.ends_with(b"DaK32fC32f") {
        let k = c.knots.as_ref()?;
        let ctl = c.controls.as_ref()?;
        if k.is_empty() {
            return Some((Vec::new(), "no keys: nothing to move".into()));
        }
        let dim = ctl.len() / k.len();
        let m = dim.min(3);
        if dim == 0 {
            return None;
        }
        let u = unit(length);
        let key = |i: usize| &ctl[i * dim..i * dim + dim];
        let dot = |v: &[f64]| py_sum((0..m).map(|j| v[j] * u[j]));
        let (a, b) = (dot(key(0)), dot(key(k.len() - 1)));
        let mut puts = Vec::new();
        for i in 0..k.len() {
            let w = if !ramp || b == a {
                1.0
            } else {
                py_min(py_max((dot(key(i)) - a) / (b - a), 0.0), 1.0)
            };
            for j in 0..m {
                let v = ctl[i * dim + j] + delta[j] * w;
                if v.is_nan() {
                    return None;
                }
                puts.push(Put {
                    name: "Controls",
                    element: Some((4 * (i * dim + j)) as u64),
                    fmt: "<f",
                    values: vec![v],
                });
            }
        }
        return Some((
            puts,
            format!("float controls moved{}", if ramp { " (ramp)" } else { "" }),
        ));
    }
    None
}

const SCALE0: f64 = 1.414_213_5;
const OFFSET0: f64 = -0.707_106_77;

/// `set_orientations(g, curve, quats)`'s puts and report; `None` where the reference raises.
#[must_use]
pub fn orientation_plan(c: &Curve, quats: &[[f64; 4]]) -> Option<(Vec<Put>, String)> {
    let t = c.kind.as_slice();
    if !ends(t, &[b"D4nK16uC15u", b"D4nK8uC7u"]) {
        return None;
    }
    let wide = t.ends_with(b"D4nK16uC15u");
    let (top, unit, fmt): (i64, f64, &'static str) = if wide {
        (0x8000, 1.0 / 32767.0, "<H")
    } else {
        (0x80, 1.0 / 127.0, "<B")
    };
    let mask = top - 1;
    let n = c.knots_controls.as_ref()?.len() / 4;
    if quats.len() != n || n == 0 {
        return None;
    }
    let (scale, offset) = (SCALE0 * unit, OFFSET0);
    let size = if wide { 2 } else { 1 };
    let mut puts = Vec::with_capacity(3 * n + 1);
    for (k, q) in quats.iter().enumerate() {
        if q.iter().any(|x| !x.is_finite()) {
            return None;
        }
        let norm = {
            let s = py_sum(q.iter().map(|v| v * v)).sqrt();
            if s == 0.0 { 1.0 } else { s }
        };
        let mut q: [f64; 4] = std::array::from_fn(|i| q[i] / norm);
        let mut i1 = 0;
        for i in 1..4 {
            if q[i].abs() > q[i1].abs() {
                i1 = i;
            }
        }
        if q[i1] < 0.0 {
            q = q.map(|v| -v);
        }
        let mut enc = [0i64; 3];
        for (e, i) in enc.iter_mut().zip([(i1 + 1) & 3, (i1 + 2) & 3, (i1 + 3) & 3]) {
            *e = py_round((q[i] - offset) / scale)?.min(mask).max(0);
        }
        let vals = [
            enc[0],
            enc[1] | if i1 & 2 != 0 { top } else { 0 },
            enc[2] | if i1 & 1 != 0 { top } else { 0 },
        ];
        for (j, v) in vals.into_iter().enumerate() {
            puts.push(Put {
                name: "KnotsControls",
                element: Some(((n + 3 * k + j) * size) as u64),
                fmt,
                values: vec![v as f64],
            });
        }
    }
    puts.push(Put {
        name: "ScaleOffsetTableEntries",
        element: None,
        fmt: "<H",
        values: vec![0.0],
    });
    Some((puts, format!("{n} orientation keys written")))
}

// ------------------------------------------------------------------------------------------------- curve_keys

/// `granny.SCALE_TABLE`: the quantized orientation curves' scales, by selector.
pub const SCALE_TABLE: [f64; 16] = [
    1.414_213_5,
    0.707_106_77,
    0.353_553_38,
    0.353_553_38,
    0.353_553_38,
    0.176_776_69,
    0.176_776_69,
    0.176_776_69,
    -1.414_213_5,
    -0.707_106_77,
    -0.353_553_38,
    -0.353_553_38,
    -0.353_553_38,
    -0.176_776_69,
    -0.176_776_69,
    -0.176_776_69,
];

/// `granny.OFFSET_TABLE`: their offsets.
pub const OFFSET_TABLE: [f64; 16] = [
    -0.707_106_77,
    -0.353_553_38,
    -0.530_330_06,
    -0.176_776_69,
    0.176_776_69,
    -0.176_776_69,
    -0.088_388_346,
    0.0,
    0.707_106_77,
    0.353_553_38,
    0.530_330_06,
    0.176_776_69,
    -0.176_776_69,
    0.176_776_69,
    0.088_388_346,
    -0.0,
];

/// A key of a curve: its time and its values (3 for a position, 4 for a quaternion x, y, z, w).
pub type Key = (f64, Vec<f64>);

/// Why a curve's keys cannot be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CurveError {
    /// A format the reader does not know (`curve format {t} is not read`).
    #[error("curve format {0} is not read")]
    Format(String),
    /// A field the format needs is missing or short, or a knot scale is zero (the reference's `KeyError`,
    /// `IndexError` or `ZeroDivisionError`).
    #[error("curve {0}: {1}")]
    Field(String, &'static str),
}

/// `granny._trunc_scale(v)`: the float32 whose upper 16 bits are `v`'s lower 16.
fn trunc_scale(v: i64) -> f64 {
    f64::from(f32::from_bits(((v & 0xFFFF) as u32) << 16))
}

/// `granny._quat(a, b, c, scales, offsets, top)`.
fn quat(a: i64, b: i64, c: i64, scales: &[f64; 4], offsets: &[f64; 4], top: i64) -> Vec<f64> {
    let bits = i64::from(64 - top.leading_zeros());
    let i1 = (((b & top) >> (bits - 2)) | ((c & top) >> (bits - 1))) as usize & 3;
    let (i2, i3, i4) = ((i1 + 1) & 3, (i1 + 2) & 3, (i1 + 3) & 3);
    let mask = top - 1;
    let mut q = [0.0f64; 4];
    q[i2] = (a & mask) as f64 * scales[i2] + offsets[i2];
    q[i3] = (b & mask) as f64 * scales[i3] + offsets[i3];
    q[i4] = (c & mask) as f64 * scales[i4] + offsets[i4];
    let r = 1.0 - q[i2] * q[i2] - q[i3] * q[i3] - q[i4] * q[i4];
    let sign = if a & top != 0 { -1.0 } else { 1.0 };
    q[i1] = py_max(0.0, r).sqrt() * sign;
    q.to_vec()
}

/// `granny.curve_keys(cd)`: `[(time, values)]` of a curve (empty for an identity curve, or none: `c` = `None`).
///
/// # Errors
/// A format the reference does not read, or a field it needs missing (the reference's own exception then).
#[expect(clippy::too_many_lines, reason = "the reference's function, format by format")]
pub fn curve_keys(c: Option<&Curve>) -> Result<Vec<Key>, CurveError> {
    let Some(c) = c else { return Ok(Vec::new()) };
    let t = c.kind.as_slice();
    let name = || c.kind.iter().map(|&b| char::from(b)).collect::<String>();
    let field = |what: &'static str| CurveError::Field(name(), what);
    if t.ends_with(b"DaIdentity") {
        return Ok(Vec::new());
    }
    if t.ends_with(b"Constant32f") {
        return Ok(vec![(0.0, c.controls.clone().ok_or_else(|| field("no Controls"))?)]);
    }
    if t.ends_with(b"DaK32fC32f") {
        let k = c.knots.as_ref().ok_or_else(|| field("no Knots"))?;
        let ctl = c.controls.as_ref().ok_or_else(|| field("no Controls"))?;
        if k.is_empty() {
            return Ok(Vec::new());
        }
        let dim = ctl.len() / k.len();
        return Ok(k
            .iter()
            .enumerate()
            .map(|(i, &ki)| (ki, ctl.get(i * dim..(i + 1) * dim).unwrap_or_default().to_vec()))
            .collect());
    }
    let kc = || c.kc.as_ref().ok_or_else(|| field("no KnotsControls"));
    let three = |v: &Option<Vec<f64>>, what: &'static str| -> Result<[f64; 3], CurveError> {
        let v = v.as_ref().ok_or_else(|| field(what))?;
        Ok([
            *v.first().ok_or_else(|| field(what))?,
            *v.get(1).ok_or_else(|| field(what))?,
            *v.get(2).ok_or_else(|| field(what))?,
        ])
    };
    let at = |kc: &[f64], i: usize| kc.get(i).copied().ok_or_else(|| field("KnotsControls too short"));
    if ends(t, &[b"D3K16uC16u", b"D3K8uC8u"]) {
        let kc = kc()?;
        let n = kc.len() / 4;
        let s = trunc_scale(c.trunc.ok_or_else(|| field("no OneOverKnotScaleTrunc"))?);
        let (sc, of) = (three(&c.scales, "ControlScales")?, three(&c.offsets, "ControlOffsets")?);
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            if s == 0.0 {
                return Err(field("a zero knot scale"));
            }
            let mut v = Vec::with_capacity(3);
            for j in 0..3 {
                v.push(at(kc, n + 3 * i + j)? * sc[j] + of[j]);
            }
            out.push((at(kc, i)? / s, v));
        }
        return Ok(out);
    }
    if ends(t, &[b"D3I1K8uC8u", b"D3I1K16uC16u", b"D3I1K32fC32f"]) {
        let kc = kc()?;
        let n = kc.len() / 2;
        // float knots (K32f) are times already unless the header scales them too
        let s = c.trunc.map_or(1.0, trunc_scale);
        let (sc, of) = (three(&c.scales, "ControlScales")?, three(&c.offsets, "ControlOffsets")?);
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            if s == 0.0 {
                return Err(field("a zero knot scale"));
            }
            let x = at(kc, n + i)?;
            out.push((at(kc, i)? / s, (0..3).map(|j| x * sc[j] + of[j]).collect()));
        }
        return Ok(out);
    }
    if ends(t, &[b"D4nK16uC15u", b"D4nK8uC7u"]) {
        let kc = c
            .knots_controls
            .as_ref()
            .ok_or_else(|| field("no KnotsControls (integers)"))?;
        let n = kc.len() / 4;
        let wide = t.ends_with(b"D4nK16uC15u");
        let (top, unit) = if wide {
            (0x8000, 1.0 / 32767.0)
        } else {
            (0x80, 1.0 / 127.0)
        };
        let sel = c.selectors.ok_or_else(|| field("no ScaleOffsetTableEntries"))?;
        let scales: [f64; 4] = std::array::from_fn(|k| SCALE_TABLE[((sel >> (4 * k)) & 15) as usize] * unit);
        let offsets: [f64; 4] = std::array::from_fn(|k| OFFSET_TABLE[((sel >> (4 * k)) & 15) as usize]);
        let s = c.knot_scale.ok_or_else(|| field("no OneOverKnotScale"))?;
        let mut out = Vec::with_capacity(n);
        for i in 0..n {
            if s == 0.0 {
                return Err(field("a zero knot scale"));
            }
            let g = |j: usize| kc.get(j).copied().ok_or_else(|| field("KnotsControls too short"));
            let q = quat(
                g(n + 3 * i)?,
                g(n + 3 * i + 1)?,
                g(n + 3 * i + 2)?,
                &scales,
                &offsets,
                top,
            );
            out.push((g(i)? as f64 / s, q));
        }
        return Ok(out);
    }
    Err(CurveError::Format(name()))
}

/// `granny.sample(keys, t, default)`: the curve's value at `t` - the first key's before or at it, linear between
/// keys (a quaternion: the nearer of q and -q, then made unit length), the last key's after; `default` for no keys.
#[must_use]
pub fn sample(keys: &[Key], t: f64, default: &[f64]) -> Vec<f64> {
    let Some(first) = keys.first() else {
        return default.to_vec();
    };
    if t <= first.0 || keys.len() == 1 {
        return first.1.clone();
    }
    for w in keys.windows(2) {
        let [(t0, v0), (t1, v1)] = w else { continue };
        if t <= *t1 {
            let f = if t1 > t0 { (t - t0) / (t1 - t0) } else { 0.0 };
            if v0.len() == 4 {
                let flip = py_sum(v0.iter().zip(v1).map(|(a, b)| a * b)) < 0.0;
                let q: Vec<f64> = v0
                    .iter()
                    .zip(v1)
                    .map(|(&a, &b)| {
                        let b = if flip { -b } else { b };
                        a + (b - a) * f
                    })
                    .collect();
                let n = py_sum(q.iter().map(|x| x * x)).sqrt();
                let n = if n == 0.0 { 1.0 } else { n };
                return q.iter().map(|x| x / n).collect();
            }
            return v0.iter().zip(v1).map(|(&a, &b)| a + (b - a) * f).collect();
        }
    }
    keys.last().map_or_else(|| default.to_vec(), |k| k.1.clone())
}
