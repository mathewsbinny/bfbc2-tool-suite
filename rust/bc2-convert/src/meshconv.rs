//! The rest of `bfbc2-console-convert/mesh.py` around the header port (round F2a): `console_layouts` (a console
//! shader database's part layouts), `decode` (one part's vertices as arrays) and `relayout` (a PC mesh's vertices
//! re-laid into other PC layouts).
//!
//! | Python (`mesh.py`) | here |
//! |---|---|
//! | `console_layouts(db)` | [`console_layouts`] |
//! | `decode(b, layout, part, big_endian)` | [`decode`]: the arrays as [`Array`]s, in the dict's order |
//! | `relayout(b, old, new)` | [`relayout`]; with an `edit` callback [`relayout_blocks`] (the columns each block's edit sees) |
//! | `elements`, `texcoord_element`, `index_count`, `strip_to_list` | [`elements`], [`texcoord_element`], [`index_count`], [`strip_to_list`] |
//!
//! Values are the reference's to the bit: halves and floats read exactly, a float cast to a half rounded to nearest
//! (ties to even), weights divided by 255 in 32-bit floats as NumPy divides a float32 array. Where the reference
//! raises its `ConvertError` this returns [`Refusal::Error`] with the same message (Python's `repr` of a part name
//! included); where it would fail another way (NumPy's buffer too small, a reshape, an index past the palette)
//! [`Refusal::Reference`]: the caller runs the Python.

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

use bc2_formats::meshdata::{self, MeshHeader};

use crate::mesh::{element, size};
pub use crate::terrain::Refusal;

type Res<T> = Result<T, Refusal>;

fn fail<T>(msg: impl Into<String>) -> Res<T> {
    Err(Refusal::Error(msg.into()))
}

/// Python's `repr` of a `bytes` value.
#[must_use]
pub fn bytes_repr(b: &[u8]) -> String {
    let q = if b.contains(&b'\'') && !b.contains(&b'"') {
        '"'
    } else {
        '\''
    };
    let mut s = String::from("b");
    s.push(q);
    for &c in b {
        match c {
            b'\\' => s.push_str("\\\\"),
            b'\t' => s.push_str("\\t"),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            c if c == q as u8 => {
                s.push('\\');
                s.push(q);
            }
            0x20..=0x7E => s.push(c as char),
            _ => s.push_str(&format!("\\x{c:02x}")),
        }
    }
    s.push(q);
    s
}

/// A vertex element: (format, usage, offset, stream).
pub type Element = (u8, u8, u8, u8);

/// `elements(layout)`: a layout record body's elements and stream 0's stride; `None` where the record is too short
/// for what it says (the reference would read short tuples).
#[must_use]
pub fn elements(layout: &[u8]) -> Option<(Vec<Element>, usize)> {
    let count = usize::from(*layout.get(64)?);
    let stride = usize::from(*layout.get(69)?);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let e = layout.get(4 * i..4 * i + 4)?;
        out.push((e[0], e[1], e[2], e[3]));
    }
    Some((out, stride))
}

/// A part's primitive type: a triangle strip.
pub const STRIP: u8 = 5;

/// How many indices a part's triangles take (`index_count`).
#[must_use]
pub fn index_count(triangles: u32, k: u8) -> u64 {
    if k == STRIP {
        u64::from(triangles) + 2
    } else {
        u64::from(triangles) * 3
    }
}

/// A triangle strip's indices as a list's (`strip_to_list`).
#[must_use]
pub fn strip_to_list(s: &[i64]) -> Vec<i64> {
    let mut out = Vec::new();
    for i in 0..s.len().saturating_sub(2) {
        let (a, b, c) = (s[i], s[i + 1], s[i + 2]);
        if a == b || a == c || b == c {
            continue;
        }
        if i % 2 == 0 {
            out.extend([a, b, c]);
        } else {
            out.extend([b, a, c]);
        }
    }
    out
}

/// The element holding a part's texture coordinates (`texcoord_element`).
#[must_use]
pub fn texcoord_element(els: &[Element]) -> Option<usize> {
    let zero: Vec<(usize, u8)> = els
        .iter()
        .enumerate()
        .filter(|(_, e)| e.1 == 0 && e.3 == 0)
        .map(|(i, e)| (i, e.0))
        .collect();
    zero.iter()
        .find(|(_, f)| matches!(f, 0x06 | 0x02))
        .or_else(|| {
            zero.iter()
                .find(|(_, f)| matches!(f, 0x06 | 0x02 | 0x07 | 0x08 | 0x03 | 0x04))
        })
        .map(|(i, _)| *i)
}

/// `_PC_DTYPE`: (half / float / byte, components) of a PC format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `<f4`.
    F32,
    /// `<f2`.
    F16,
    /// `u1`.
    U8,
}

/// The dtype and component count of a PC element format (`_PC_DTYPE`).
#[must_use]
pub const fn pc_dtype(f: u8) -> Option<(Kind, usize)> {
    Some(match f {
        0x02 => (Kind::F32, 2),
        0x03 => (Kind::F32, 3),
        0x04 => (Kind::F32, 4),
        0x05 => (Kind::F16, 1),
        0x06 => (Kind::F16, 2),
        0x07 => (Kind::F16, 3),
        0x08 => (Kind::F16, 4),
        0x0C => (Kind::U8, 4),
        _ => return None,
    })
}

const fn pc_format(f: u8) -> u8 {
    match f {
        0x2B | 0x0D => 0x08,
        _ => f,
    }
}

/// An array of `decode`'s dict: its key, rows x columns, and its values.
#[derive(Debug, Clone, PartialEq)]
pub struct Array {
    /// The dict key.
    pub key: &'static str,
    /// Rows.
    pub rows: usize,
    /// Columns.
    pub cols: usize,
    /// The values.
    pub data: Values,
}

/// The values of an [`Array`].
#[derive(Debug, Clone, PartialEq)]
pub enum Values {
    /// `float32`.
    F32(Vec<f32>),
    /// `uint8`.
    U8(Vec<u8>),
    /// `int64`.
    I64(Vec<i64>),
}

fn half_to_f32(h: u16) -> f32 {
    crate::mesh::f16_to_f32(h)
}

const USAGES: [&str; 7] = [
    "uvs",
    "positions",
    "blend_indices",
    "weights",
    "normals",
    "tangents",
    "binormals",
];

fn index_words(b: &[u8], wide: bool, big: bool, count: u64, at: usize) -> Res<Vec<i64>> {
    let isz = if wide { 4 } else { 2 };
    let end = (count as u128) * isz as u128 + at as u128;
    if end > b.len() as u128 {
        return Err(Refusal::Reference);
    }
    let s = &b[at..end as usize];
    Ok(s.chunks_exact(isz)
        .map(|c| match (wide, big) {
            (true, true) => i64::from(u32::from_be_bytes([c[0], c[1], c[2], c[3]])),
            (true, false) => i64::from(u32::from_le_bytes([c[0], c[1], c[2], c[3]])),
            (false, true) => i64::from(u16::from_be_bytes([c[0], c[1]])),
            (false, false) => i64::from(u16::from_le_bytes([c[0], c[1]])),
        })
        .collect())
}

/// Which part `decode` reads: an index (negative from the end, as Python's) or a name.
#[derive(Debug, Clone, Copy)]
pub enum Which<'a> {
    /// `m['parts'][i]`.
    Index(i64),
    /// The first part of this name (latin-1).
    Name(&'a [u8]),
}

/// `decode(b, layout, part, big_endian)`: the dict's arrays in order.
///
/// # Errors
/// See the module.
#[expect(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn decode(b: &[u8], layout: &[u8], part: Which<'_>, big: bool) -> Res<Vec<Array>> {
    let m = meshdata::parse_mesh(b, big).map_err(|_| Refusal::Reference)?;
    let p = match part {
        Which::Index(i) => {
            let n = m.parts.len() as i64;
            let k = if i < 0 { i + n } else { i };
            if !(0..n).contains(&k) {
                return Err(Refusal::Reference);
            }
            &m.parts[k as usize]
        }
        Which::Name(nm) => m.parts.iter().find(|q| q.name == nm).ok_or(Refusal::Reference)?,
    };
    let (els, stride) = elements(layout).ok_or(Refusal::Reference)?;
    if usize::from(p.bytes[0]) != stride {
        return fail(format!(
            "part {}: stride {}, its layout says {stride}",
            bytes_repr(&p.name),
            p.bytes[0]
        ));
    }
    let wide = m.sizes[2] != 0;
    let isz = if wide { 4 } else { 2 };
    let at = m.data as u128 + u128::from(m.sizes[0]) + u128::from(p.counts[2]) * isz;
    let at = usize::try_from(at).map_err(|_| Refusal::Reference)?;
    let mut idx = index_words(b, wide, big, index_count(p.counts[0], p.bytes[1]), at)?;
    if p.bytes[1] == STRIP {
        idx = strip_to_list(&idx);
    }
    let lo = idx.iter().copied().min().unwrap_or(0);
    let n = idx.iter().copied().max().map_or(0, |hi| (hi - lo + 1) as usize);
    let base = m.data + p.counts[3] as usize + lo as usize * stride;
    let raw: &[u8] = if n > 0 {
        b.get(base..base + n * stride).ok_or(Refusal::Reference)?
    } else {
        &[]
    };
    if !idx.len().is_multiple_of(3) {
        return Err(Refusal::Reference);
    }
    let mut out = vec![Array {
        key: "indices",
        rows: idx.len() / 3,
        cols: 3,
        data: Values::I64(idx.iter().map(|v| v - lo).collect()),
    }];
    let uv = texcoord_element(&els);
    for (i, &(f, usage, off, stream)) in els.iter().enumerate() {
        let Some(&key) = USAGES.get(usize::from(usage)) else {
            continue;
        };
        if stream != 0 || out.iter().any(|a| a.key == key) || (usage == 0 && Some(i) != uv) {
            continue;
        }
        let fp = if big { pc_format(f) } else { f };
        let Some((kind, k)) = pc_dtype(fp) else {
            return fail(format!(
                "part {}: vertex element format {f:#04x} is not decoded",
                bytes_repr(&p.name)
            ));
        };
        let (sc, sp) = (size(f).ok_or(Refusal::Reference)?, size(fp).ok_or(Refusal::Reference)?);
        let off = usize::from(off);
        if off + sc > stride {
            return Err(Refusal::Reference);
        }
        let mut pc = vec![0u8; n * sp];
        for (r, dst) in pc.chunks_exact_mut(sp.max(1)).enumerate().take(n) {
            let src = &raw[r * stride + off..r * stride + off + sc];
            if big {
                if !element(f, fp, src, false, dst) {
                    return Err(Refusal::Reference);
                }
            } else {
                dst.copy_from_slice(src);
            }
        }
        let data = match kind {
            Kind::U8 => Values::U8(pc),
            Kind::F16 => Values::F32(
                pc.chunks_exact(2)
                    .map(|c| half_to_f32(u16::from_le_bytes([c[0], c[1]])))
                    .collect(),
            ),
            Kind::F32 => Values::F32(
                pc.chunks_exact(4)
                    .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                    .collect(),
            ),
        };
        out.push(Array {
            key,
            rows: n,
            cols: k,
            data,
        });
    }
    if let Some(w) = out.iter_mut().find(|a| a.key == "weights") {
        let v: Vec<f32> = match &w.data {
            Values::U8(v) => v.iter().map(|&x| f32::from(x) / 255.0).collect(),
            Values::F32(v) => v.iter().map(|&x| x / 255.0).collect(),
            Values::I64(_) => return Err(Refusal::Reference),
        };
        w.data = Values::F32(v);
    }
    if !p.bones.is_empty()
        && let Some(bi) = out.iter().find(|a| a.key == "blend_indices")
    {
        let Values::U8(v) = &bi.data else {
            return Err(Refusal::Reference);
        };
        let last = p.bones.len() - 1;
        if last > 255 {
            return Err(Refusal::Reference);
        }
        let bones = v
            .iter()
            .map(|&x| i64::from(p.bones[usize::from(x).min(last)]))
            .collect();
        let (rows, cols) = (bi.rows, bi.cols);
        out.push(Array {
            key: "bones",
            rows,
            cols,
            data: Values::I64(bones),
        });
    }
    Ok(out)
}

/// `console_layouts(db)`: (mesh part name, lower case; its layout record body), in the reference's dict order.
#[must_use]
pub fn console_layouts(db: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    // the reference's pattern (re's lazy quantifiers; the regex crate's leftmost-first match is the same)
    let Ok(re) = regex::bytes::Regex::new(r"(?s-u)[ -~]{4,300}?_lod[0-9]+/[ -~]+?\x00") else {
        return Vec::new();
    };
    let rec = 16 + 0x49;
    let mut raw: Vec<(Vec<u8>, &[u8], bool)> = Vec::new();
    let mut prev: Option<usize> = None;
    for m in re.find_iter(db) {
        let (mut start, end) = (m.start(), m.end());
        let anchored = prev.is_some_and(|p| start <= p + 16 && p + 16 < end);
        if anchored && let Some(p) = prev {
            start = p + 16;
        }
        let name = db[start..end - 1].to_ascii_lowercase();
        raw.push((name, &db[end..(end + 16).min(db.len())], anchored));
        prev = Some(end);
    }
    let top = |n: &[u8]| -> Vec<u8> { n.split(|&c| c == b'/').next().unwrap_or_default().to_vec() };
    let roots: std::collections::HashSet<Vec<u8>> = raw.iter().filter(|r| r.2).map(|r| top(&r.0)).collect();
    // out: a dict, a repeated name keeping its first place and its last guid
    let mut order: Vec<Vec<u8>> = Vec::new();
    let mut guid: std::collections::HashMap<Vec<u8>, &[u8]> = std::collections::HashMap::new();
    for (n, g, a) in &raw {
        let t = top(n);
        let n = if !a && !roots.contains(&t) && roots.contains(t.get(1..).unwrap_or_default()) {
            n[1..].to_vec()
        } else {
            n.clone()
        };
        if guid.insert(n.clone(), g).is_none() {
            order.push(n);
        }
    }
    let mut found: std::collections::HashMap<&[u8], &[u8]> = std::collections::HashMap::new();
    let mut searched: std::collections::HashSet<&[u8]> = std::collections::HashSet::new();
    for g in guid.values() {
        if !searched.insert(g) {
            continue;
        }
        let mut from = 0;
        while let Some(k) = find(db, g, from) {
            if k + rec <= db.len() && (1..=16).contains(&db[k + 16 + 64]) && db[k + 16 + 65] == 0 {
                found.insert(g, &db[k + 16..k + rec]);
                break;
            }
            from = k + 1;
        }
    }
    order
        .into_iter()
        .filter_map(|n| {
            let r = found.get(guid[&n])?;
            Some((n, r.to_vec()))
        })
        .collect()
}

/// `bytes.find(needle, from)` (an empty needle is found at `from` while `from <= len`).
fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    if needle.is_empty() {
        return Some(from);
    }
    memchr::memmem::find(&hay[from..], needle).map(|k| k + from)
}

/// One block of `relayout`: the parts drawing from it, its vertex count and old / new strides, and the new
/// layout's columns (`cols`, before any edit).
#[derive(Debug, Clone)]
pub struct Block {
    /// Its first byte (from the vertex data).
    pub start: usize,
    /// The parts drawing from it.
    pub members: Vec<usize>,
    /// Vertices.
    pub n: usize,
    /// Old stride.
    pub so: usize,
    /// New stride.
    pub sn: usize,
    /// The new layout's elements.
    pub en: Vec<Element>,
    /// Per new element: (kind, components, values: f32 or u8 as their bytes, little-endian).
    pub cols: Vec<(Kind, usize, Vec<u8>)>,
}

fn names_repr(m: &MeshHeader, members: &[usize]) -> String {
    let v: Vec<String> = members.iter().map(|&i| bytes_repr(&m.parts[i].name)).collect();
    format!("[{}]", v.join(", "))
}

/// The header and `relayout`'s blocks, each with its columns; an error met after some blocks ends the list (the
/// reference calls `edit` on the blocks before it, then raises).
///
/// # Errors
/// Errors before any block ([`Refusal`]).
#[expect(clippy::too_many_lines, reason = "the reference's one function, step for step")]
pub fn relayout_blocks(b: &[u8], old: &[&[u8]], new: &[&[u8]]) -> Res<(MeshHeader, Vec<Block>, Option<Refusal>)> {
    let m = meshdata::parse_mesh(b, false).map_err(|_| Refusal::Reference)?;
    let np = m.parts.len();
    if old.len() != np || new.len() != np {
        return fail(format!("{np} parts, {} / {} layouts", old.len(), new.len()));
    }
    let wide = m.sizes[2] != 0;
    let isz: u128 = if wide { 4 } else { 2 };
    let mut spans: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for (i, p) in m.parts.iter().enumerate() {
        if p.counts[0] == 0 {
            continue;
        }
        let at = m.data as u128 + u128::from(m.sizes[0]) + u128::from(p.counts[2]) * isz;
        let at = usize::try_from(at).map_err(|_| Refusal::Reference)?;
        let idx = index_words(b, wide, false, index_count(p.counts[0], p.bytes[1]), at)?;
        let hi = idx.iter().copied().max().map_or(0, |h| h as usize + 1);
        let vo = p.counts[3] as usize;
        spans.push((vo, vo + hi * usize::from(p.bytes[0]), vec![i]));
    }
    spans.sort();
    let mut blocks: Vec<(usize, usize, Vec<usize>)> = Vec::new();
    for s in spans {
        match blocks.last_mut() {
            Some(last) if s.0 < last.1 => {
                last.1 = last.1.max(s.1);
                last.2.extend(s.2);
            }
            _ => blocks.push(s),
        }
    }
    let mut out = Vec::new();
    for (start, end, members) in blocks {
        let step = (|| -> Res<Block> {
            let first = members[0];
            if members.iter().any(|&i| old[i] != old[first]) || members.iter().any(|&i| new[i] != new[first]) {
                return fail(format!(
                    "parts {} share vertices in different layouts",
                    names_repr(&m, &members)
                ));
            }
            let (eo, so) = elements(old[first]).ok_or(Refusal::Reference)?;
            let (en, sn) = elements(new[first]).ok_or(Refusal::Reference)?;
            if so == 0 {
                return Err(Refusal::Reference);
            }
            if !(end - start).is_multiple_of(so)
                || members
                    .iter()
                    .any(|&i| !(m.parts[i].counts[3] as usize).wrapping_sub(start).is_multiple_of(so))
            {
                return fail(format!(
                    "parts {}: vertices not aligned on {so} bytes",
                    names_repr(&m, &members)
                ));
            }
            let n = (end - start) / so;
            let base = m.data + start;
            let raw = b.get(base..base + n * so).ok_or(Refusal::Reference)?;
            // pair each new element with an old one: same (usage, stream, components) first, then any of its usage
            let mut left: Vec<usize> = (0..eo.len()).collect();
            let mut pair: Vec<Option<usize>> = vec![None; en.len()];
            for exact in [true, false] {
                for (k, &(f, u, _, s)) in en.iter().enumerate() {
                    if pair[k].is_some() {
                        continue;
                    }
                    let mut src = None;
                    for (pos, &j) in left.iter().enumerate() {
                        if eo[j].1 != u || eo[j].3 != s {
                            continue;
                        }
                        if exact {
                            let a = pc_dtype(eo[j].0).ok_or(Refusal::Reference)?;
                            let c = pc_dtype(f).ok_or(Refusal::Reference)?;
                            if a.1 != c.1 {
                                continue;
                            }
                        }
                        src = Some(pos);
                        break;
                    }
                    if let Some(pos) = src {
                        pair[k] = Some(left.remove(pos));
                    }
                }
            }
            let mut cols = Vec::with_capacity(en.len());
            for (k, &(f, ..)) in en.iter().enumerate() {
                let (kind, comps) = pc_dtype(f).ok_or(Refusal::Reference)?;
                let mut col = vec![0u8; n * comps * if kind == Kind::U8 { 1 } else { 4 }];
                if let Some(j) = pair[k] {
                    let (fo, _, oo, _) = eo[j];
                    let (okind, oc) = pc_dtype(fo).ok_or(Refusal::Reference)?;
                    let oo = usize::from(oo);
                    let zo = size(fo).ok_or(Refusal::Reference)?;
                    if oo + zo > so {
                        return Err(Refusal::Reference);
                    }
                    if (okind == Kind::U8) != (kind == Kind::U8) {
                        let e = en[k];
                        return fail(format!(
                            "element ({}, {}, {}, {}): its old format {fo:#04x} is not carried to {f:#04x}",
                            e.0, e.1, e.2, e.3
                        ));
                    }
                    let c = comps.min(oc);
                    for r in 0..n {
                        let v = &raw[r * so + oo..r * so + oo + zo];
                        for q in 0..c {
                            match okind {
                                Kind::U8 => col[r * comps + q] = v[q],
                                Kind::F16 => {
                                    let x = half_to_f32(u16::from_le_bytes([v[2 * q], v[2 * q + 1]]));
                                    col[4 * (r * comps + q)..4 * (r * comps + q + 1)].copy_from_slice(&x.to_le_bytes());
                                }
                                Kind::F32 => col[4 * (r * comps + q)..4 * (r * comps + q + 1)]
                                    .copy_from_slice(&v[4 * q..4 * q + 4]),
                            }
                        }
                    }
                }
                cols.push((kind, comps, col));
            }
            Ok(Block {
                start,
                members: members.clone(),
                n,
                so,
                sn,
                en,
                cols,
            })
        })();
        match step {
            Ok(blk) => out.push(blk),
            Err(e) => return Ok((m, out, Some(e))),
        }
    }
    Ok((m, out, None))
}

/// A block's rows from its (possibly edited) columns, each already in its element's dtype as bytes
/// (`np.ascontiguousarray(cols[k].astype(dt)).tobytes()`); `None` where the reference's assignment would fail.
#[must_use]
pub fn block_rows(n: usize, sn: usize, en: &[Element], packed: &[&[u8]]) -> Option<Vec<u8>> {
    let mut rows = vec![0u8; n * sn];
    for (&(f, _, off, _), col) in en.iter().zip(packed) {
        let z = size(f)?;
        let off = usize::from(off);
        if off + z > sn || col.len() != n * z {
            return None;
        }
        for r in 0..n {
            rows[r * sn + off..r * sn + off + z].copy_from_slice(&col[r * z..(r + 1) * z]);
        }
    }
    Some(rows)
}

/// `cols[k].astype(dt)` of an unedited column: the float32 values (every column but a byte one holds float32s) cast
/// to the element's halves or kept as floats; bytes as they are.
fn pack(kind: Kind, f: u8, col: &[u8]) -> Option<Vec<u8>> {
    let (dk, _) = pc_dtype(f)?;
    Some(match (kind, dk) {
        (Kind::U8, _) | (_, Kind::F32 | Kind::U8) => col.to_vec(),
        (_, Kind::F16) => col
            .chunks_exact(4)
            .flat_map(|c| crate::mesh::f32_to_f16(f32::from_le_bytes([c[0], c[1], c[2], c[3]])).to_le_bytes())
            .collect(),
    })
}

/// The finished file of [`relayout_blocks`]' header and blocks: offsets and strides set, the index data kept.
///
/// # Errors
/// [`Refusal::Reference`] where the reference's write would fail.
pub fn relayout_finish(b: &[u8], mut m: MeshHeader, new: &[&[u8]], blocks: &[(Block, Vec<u8>)]) -> Res<Vec<u8>> {
    let mut vb = Vec::new();
    let mut new_off: Vec<Option<usize>> = vec![None; m.parts.len()];
    for (blk, rows) in blocks {
        let base = vb.len();
        vb.extend_from_slice(rows);
        for &i in &blk.members {
            new_off[i] = Some(base + (m.parts[i].counts[3] as usize - blk.start) / blk.so * blk.sn);
            m.parts[i].bytes[0] = blk.sn as u8;
        }
    }
    for (i, p) in m.parts.iter_mut().enumerate() {
        if let Some(o) = new_off[i] {
            p.counts[3] = u32::try_from(o).map_err(|_| Refusal::Reference)?;
        } else {
            p.counts[3] = u32::try_from(vb.len()).map_err(|_| Refusal::Reference)?;
            p.bytes[0] = elements(new[i]).ok_or(Refusal::Reference)?.1 as u8;
        }
    }
    let at = m.data + m.sizes[0] as usize;
    let ib = b.get(at..at + m.sizes[1] as usize).ok_or(Refusal::Reference)?;
    let mut res = meshdata::write_mesh(&m, &vb, ib).map_err(|_| Refusal::Reference)?;
    if m.sizes[2] != 0 {
        let k = res.len() - vb.len() - ib.len() - 4;
        res[k..k + 4].copy_from_slice(&1u32.to_le_bytes());
    }
    Ok(res)
}

/// `relayout(b, old, new)` without an edit.
///
/// # Errors
/// See the module.
pub fn relayout(b: &[u8], old: &[&[u8]], new: &[&[u8]]) -> Res<Vec<u8>> {
    let (m, blocks, err) = relayout_blocks(b, old, new)?;
    if let Some(e) = err {
        return Err(e);
    }
    let mut done = Vec::with_capacity(blocks.len());
    for blk in blocks {
        let packed: Vec<Vec<u8>> = blk
            .cols
            .iter()
            .zip(&blk.en)
            .map(|((kind, _, col), e)| pack(*kind, e.0, col))
            .collect::<Option<_>>()
            .ok_or(Refusal::Reference)?;
        let refs: Vec<&[u8]> = packed.iter().map(Vec::as_slice).collect();
        let rows = block_rows(blk.n, blk.sn, &blk.en, &refs).ok_or(Refusal::Reference)?;
        done.push((blk, rows));
    }
    relayout_finish(b, m, new, &done)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repr_of_bytes() {
        assert_eq!(bytes_repr(b"ab"), "b'ab'");
        assert_eq!(bytes_repr(b"a'b"), "b\"a'b\"");
        assert_eq!(bytes_repr(b"a'\"\x01"), "b'a\\'\"\\x01'");
    }

    #[test]
    fn strips() {
        assert_eq!(strip_to_list(&[0, 1, 2, 3]), vec![0, 1, 2, 2, 1, 3]);
        assert_eq!(strip_to_list(&[0, 0, 1, 2]), vec![1, 0, 2]);
    }
}
