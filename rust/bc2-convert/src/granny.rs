//! Xbox 360 Granny resources (skeletons, animations) as the PC's: a port of `bfbc2-anim-tools/granny.py`'s
//! `to_pc` (and `_swap`, `_marshal`), byte for byte.
//!
//! DICE's table and the string table are swapped field by field (the table optionally renumbered onto another
//! skeleton's bones, [`bc2_formats::granny::renumber`]); the Granny file as Granny reverses a file: the header as
//! u32, each section's 32-bit part as u32 and its 16-bit part as u16, its relocations and marshalling list as u32,
//! then every object of the marshalling list member by member from its type (8-bit members restored); the CRC is
//! recomputed. The console's layout is kept.
//!
//! Where `to_pc` raises, [`to_pc`] returns an error (the bindings then let the Python function raise its own
//! exception). Writes past the end of the data are nothing, as Python's slice assignments; an object list that runs
//! past the end is not walked further (each object's writes depend only on its type and position, so the Python
//! function's remaining iterations write nothing; its errors are the first object's).

use std::collections::HashMap;
use std::hash::BuildHasher;

use bc2_formats::granny::{Endian, Granny, GrannyError, MAGIC, Ref, renumber, type_size};
use bc2_formats::zlib::crc32;
use rayon::prelude::*;

/// Members whose fields are 32-bit words (`_WORDS`): type -> words per element.
fn words_of(t: u32) -> Option<u64> {
    Some(match t {
        2 | 8 | 10 | 19 | 20 | 22 => 1,
        3..=5 => 2,
        7 => 3,
        9 => 17,
        _ => return None,
    })
}

/// Members of 16-bit fields (`_HALVES`).
fn is_half(t: u32) -> bool {
    matches!(t, 15..=18 | 21)
}

/// `_swap`: `count` fields of `size` bytes from `at`, each reversed from `src` into `dst`; a field cut short by the
/// end reversed as far as it goes, past the end nothing.
fn swap(dst: &mut [u8], src: &[u8], at: u64, size: u64, count: u64) {
    let len = dst.len().min(src.len()) as u64;
    let end = at.saturating_add(size.saturating_mul(count)).min(len);
    let mut i = at;
    while i < end {
        let hi = (i + size).min(len);
        if let (Some(d), Some(s)) = (dst.get_mut(i as usize..hi as usize), src.get(i as usize..hi as usize)) {
            for (a, b) in d.iter_mut().zip(s.iter().rev()) {
                *a = *b;
            }
        }
        i += size;
    }
}

/// `_marshal`: one object of type `tref` at `at`, member by member from the big-endian bytes.
fn marshal(g: &Granny<'_>, out: &mut [u8], b: &[u8], tref: Ref, mut at: u64, depth: usize) -> Result<(), GrannyError> {
    if depth > bc2_formats::granny::MAX_DEPTH {
        return Err(GrannyError("objects nested too deep".into()));
    }
    let len = b.len() as u64;
    for m in g.members(tref)?.iter() {
        let n = u64::from(m.arr.max(1));
        if m.kind == 1 {
            let child = m
                .child
                .ok_or_else(|| GrannyError("an inline struct without a type".into()))?;
            let sz = g.size(child)?;
            for i in 0..n {
                let pos = at.saturating_add(i.saturating_mul(sz));
                if i > 0 && (pos >= len || sz == 0) {
                    break;
                }
                marshal(g, out, b, child, pos, depth + 1)?;
            }
            at = at.saturating_add(sz.saturating_mul(n));
        } else if let Some(w) = words_of(m.kind) {
            swap(out, b, at, 4, w * n);
            at = at.saturating_add(type_size(m.kind) * n);
        } else if is_half(m.kind) {
            swap(out, b, at, 2, n);
            at = at.saturating_add(2 * n);
        } else {
            let sz = type_size(m.kind) * n;
            let (lo, hi) = (at.min(len) as usize, at.saturating_add(sz).min(len) as usize);
            if let (Some(d), Some(s)) = (out.get_mut(lo..hi), b.get(lo..hi)) {
                d.copy_from_slice(s);
            }
            at = at.saturating_add(sz);
        }
    }
    Ok(())
}

/// An Xbox 360 (big-endian) Granny resource as the PC's little-endian one (`to_pc`); `bones` renumbers DICE's
/// per-bone table ({old index: new index}).
///
/// # Errors
/// Where `to_pc` raises: not a big-endian Granny resource, a bone missing from `bones`, damaged data.
#[expect(
    clippy::many_single_char_names,
    reason = "the names of granny.py's to_pc, kept for comparison"
)]
pub fn to_pc<S: BuildHasher>(b: &[u8], bones: Option<&HashMap<u32, u32, S>>) -> Result<Vec<u8>, GrannyError> {
    let g = Granny::parse(b)?;
    if !g.endian.big {
        return Err(GrannyError("already little-endian (a PC resource)".into()));
    }
    let be = Endian { big: true };
    let mut out = b.to_vec();
    let nwords = g.strings_from / 4;
    let mut words: Vec<u32> = b
        .get(..4 * nwords)
        .unwrap_or_default()
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_be_bytes(*c))
        .collect();
    if let Some(bones) = bones {
        words = renumber(&words, bones)?;
    }
    let packed = out
        .get_mut(..4 * words.len())
        .ok_or_else(|| GrannyError("DICE's table past the end".into()))?;
    for (c, w) in packed.as_chunks_mut::<4>().0.iter_mut().zip(&words) {
        *c = w.to_le_bytes();
    }
    let mut p = g.strings_from as u64;
    swap(&mut out, b, p, 4, 1);
    p += 4;
    for _ in 0..g.strings.len() {
        let ln = u64::from(be.u16(b, p)?);
        swap(&mut out, b, p, 2, 1);
        p += 2 + ln;
    }
    swap(&mut out, b, p, 4, 1);
    let gg = g.gr2 as u64;
    if let Some(m) = out.get_mut(g.gr2..g.gr2 + 16) {
        m.copy_from_slice(&MAGIC);
    }
    let header = i64::from(be.u32(b, gg + 16)?);
    let hw = (header - 16).div_euclid(4);
    if hw > 0 {
        swap(&mut out, b, gg + 16, 4, hw as u64);
    }
    let soff = u64::from(be.u32(b, gg + 44)?);
    let count = be.u32(b, gg + 48)?;
    let len = b.len() as u64;
    for k in 0..u64::from(count) {
        let row = gg + 32 + soff + 44 * k;
        let w = |i: u64| be.u32(b, row + 4 * i).map(u64::from);
        let (off, size, f16, f8, roff, rcount, moff, mcount) =
            (w(1)?, w(2)?, w(5)?, w(6)?, w(7)?, w(8)?, w(9)?, w(10)?);
        w(0)?;
        if f16 % 4 != 0 || f16 > f8 || f8 > size {
            return Err(GrannyError(format!(
                "section {k}: 16-bit part at {f16}, 8-bit at {f8}, size {size}"
            )));
        }
        let base = gg + off;
        swap(&mut out, b, base, 4, f16 / 4);
        swap(&mut out, b, base + f16, 2, (f8 - f16) / 2);
        swap(&mut out, b, gg + roff, 4, 3 * rcount);
        swap(&mut out, b, gg + moff, 4, 4 * mcount);
        for m in 0..mcount {
            let at = gg + moff + 16 * m;
            let (n, o, ts, to) = (
                be.u32(b, at)?,
                be.u32(b, at + 4)?,
                be.u32(b, at + 8)?,
                be.u32(b, at + 12)?,
            );
            let sz = g.size((ts, to))?;
            for i in 0..u64::from(n) {
                let pos = (base + u64::from(o)).saturating_add(i.saturating_mul(sz));
                if i > 0 && (pos >= len || sz == 0) {
                    break;
                }
                marshal(&g, &mut out, b, (ts, to), pos, 0)?;
            }
        }
    }
    let n = out.len() as u64;
    let (lo, hi) = ((gg + g.crc_from).min(n), (gg + u64::from(g.total)).min(n));
    let crc = crc32(out.get(lo as usize..hi.max(lo) as usize).unwrap_or_default());
    out.get_mut(g.gr2 + 40..g.gr2 + 44)
        .ok_or_else(|| GrannyError("no room for the CRC".into()))?
        .copy_from_slice(&crc.to_le_bytes());
    Ok(out)
}

/// One resource to convert and its bone map, if any.
pub type Job<'a, S> = (&'a [u8], Option<&'a HashMap<u32, u32, S>>);

/// [`to_pc`] of many resources on all cores, in order.
#[must_use]
pub fn to_pc_all<S: BuildHasher + Sync>(items: &[Job<'_, S>]) -> Vec<Result<Vec<u8>, GrannyError>> {
    items.par_iter().map(|(b, bones)| to_pc(b, *bones)).collect()
}

/// A skeleton resource's bone names (`[b['Name'] for b in Granny(b).root()['Skeletons'][0]['Bones']]`, latin-1;
/// a string index past the table as `#<index>`), any byte order (round PR1b-1d: `BoneTables` and `bone_map` of the
/// parity tools).
///
/// # Errors
/// The resource does not read, or holds no skeleton with bones.
pub fn bone_names(b: &[u8]) -> Result<Vec<String>, GrannyError> {
    use bc2_formats::granny::{Text, Value};
    fn text(g: &Granny<'_>, t: Text<'_>) -> String {
        match t {
            Text::Missing(i) => format!("#{i}"),
            _ => g.text_bytes(t).iter().map(|&c| char::from(c)).collect(),
        }
    }
    fn field<'a>(g: &Granny<'_>, v: &Value<'a>, name: &str) -> Option<Value<'a>> {
        let Value::Object(o) = v else { return None };
        o.fields
            .iter()
            .find(|(k, _)| text(g, *k) == name)
            .map(|(_, v)| v.clone())
    }
    let g = Granny::parse(b)?;
    let root = g.root(64)?;
    let none = || GrannyError("no skeleton bones in the resource".into());
    let Some(Value::List(skels)) = field(&g, &root, "Skeletons") else {
        return Err(none());
    };
    let Some(Value::List(bones)) = skels.first().and_then(|s| field(&g, s, "Bones")) else {
        return Err(none());
    };
    bones
        .iter()
        .map(|b| match field(&g, b, "Name") {
            Some(Value::Text(t)) => Ok(text(&g, t)),
            _ => Err(none()),
        })
        .collect()
}

/// `granny.bone_map(src_skeleton, dst_skeleton)`: `{index in src: index in dst}` by bone name (a name the target has
/// twice: its last index).
///
/// # Errors
/// As [`bone_names`].
pub fn bone_map(src: &[u8], dst: &[u8]) -> Result<HashMap<u32, u32>, GrannyError> {
    let s = bone_names(src)?;
    let d = bone_names(dst)?;
    let mut at: HashMap<&str, u32> = HashMap::with_capacity(d.len());
    for (i, n) in d.iter().enumerate() {
        at.insert(n.as_str(), u32::try_from(i).unwrap_or(u32::MAX));
    }
    Ok(s.iter()
        .enumerate()
        .filter_map(|(i, n)| at.get(n.as_str()).map(|&j| (u32::try_from(i).unwrap_or(u32::MAX), j)))
        .collect())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn swaps_like_python_slices() {
        let src = [1u8, 2, 3, 4, 5, 6];
        let mut dst = src;
        swap(&mut dst, &src, 0, 4, 2);
        assert_eq!(dst, [4, 3, 2, 1, 6, 5]);
        let mut dst = src;
        swap(&mut dst, &src, 10, 4, 3);
        assert_eq!(dst, src);
    }

    /// A minimal big-endian resource: a skeleton (no DICE table), one string, a Granny file of no sections.
    fn minimal() -> Vec<u8> {
        let mut gr2 = bc2_formats::granny::MAGIC_BE.to_vec();
        gr2.extend(72u32.to_be_bytes()); // header size: all of it
        gr2.extend([0; 12]);
        for w in [7u32, 72, 0, 40, 0] {
            gr2.extend(w.to_be_bytes());
        }
        gr2.extend([0; 20]);
        let mut res = 10u32.to_be_bytes().to_vec();
        res.extend(1u32.to_be_bytes());
        res.extend(6u16.to_be_bytes());
        res.extend(b"<file>");
        res.extend((gr2.len() as u32).to_be_bytes());
        res.extend(gr2);
        res
    }

    #[test]
    fn minimal_resource() {
        let b = minimal();
        let out = to_pc::<std::hash::RandomState>(&b, None).unwrap();
        assert_eq!(&out[..4], &10u32.to_le_bytes());
        let g = Granny::parse(&out).unwrap();
        assert!(!g.endian.big);
        assert_eq!(g.strings, vec![&b"<file>"[..]]);
        assert!(to_pc::<std::hash::RandomState>(&out, None).is_err());
        assert_eq!(
            to_pc_all::<std::hash::RandomState>(&[(&b, None), (&out, None)])
                .iter()
                .filter(|r| r.is_ok())
                .count(),
            1
        );
    }
}
