//! `bfbc2-bf1943-import/portparity.py` in Rust (round PR1b-1d): every model BFBC2 also ships, converted again with
//! meshport's Porter (`bc2_convert::meshport::Porter`) and texture conversion, compared with DICE's PC version element
//! by element. The same report and JSON as the Python wrote, byte for byte on the user's BF1943 and game copy (rust/
//! BENCHMARKS.md "PR1b-1d"): NumPy's arithmetic where it reaches a printed digit is NumPy's (float64 from the
//! elements' halves and singles, sums along a row in order and over a whole picture pairwise, `np.round` as
//! multiply / round half even / divide), the numbers printed as Python's format specs print them.

#![allow(
    clippy::many_single_char_names,
    clippy::format_push_string,
    clippy::indexing_slicing,
    clippy::cast_precision_loss,
    clippy::comparison_chain,
    clippy::assigning_clones,
    clippy::map_unwrap_or,
    clippy::too_many_lines,
    clippy::case_sensitive_file_extension_comparisons,
    reason = "portparity.py's functions step for step (its names, NumPy's float arithmetic on counts, its report \
              built line by line); every index is within a length checked first (chunks_exact, the header's 32 bytes)"
)]

use std::f64::consts::PI;

use bc2_convert::meshport::{Ported, Porter};
use bc2_convert::meshview::Index;
use bc2_convert::pyfmt::{OrderedMap, fixed};
use bc2_formats::meshdata::{self, Kind, MeshHeader, MeshPart};
use bc2_pyrt::json::Value;

use super::{Run, py_dict, write_report};
use crate::pyfmt::{float_repr, str_repr};
use crate::pyproc::{Py, PyResult};

const PRECISION_DEG: f64 = 0.25;
const PRECISION_POS: f64 = 1e-4;
const PRECISION_UV: f64 = 1e-3;
const POSITION: u8 = 1;
const BLEND_INDICES: u8 = 2;

/// A component type of `DTYPE`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ty {
    F32,
    F16,
    U8,
}

/// `DTYPE`: an element format's component type and count. Format 2 is `float2` (8 bytes, the game's format table:
/// spec MESH-VTX-1): two singles. The retired Python's table read it as three (the next element's first four bytes
/// with it); no part the tool compares has one, so the reports are the same (spec rule XWW2-PAR-3).
// spec: XWW2-PAR-3
fn dtype(fmt: u8) -> Option<(Ty, usize)> {
    Some(match fmt {
        0x02 => (Ty::F32, 2),
        0x03 => (Ty::F32, 3),
        0x04 => (Ty::F32, 4),
        0x05 => (Ty::F16, 1),
        0x06 => (Ty::F16, 2),
        0x07 => (Ty::F16, 3),
        0x08 => (Ty::F16, 4),
        0x0C => (Ty::U8, 4),
        _ => return None,
    })
}

/// A row-error of the Python's `except Exception as e`: `type(e).__name__` and `str(e)`.
struct Raised(String, String);

type R<T> = Result<T, Raised>;

fn raised<T>(kind: &str, msg: impl Into<String>) -> R<T> {
    Err(Raised(kind.to_owned(), msg.into()))
}

/// `format(x, f'.{p}g')`: `p` significant digits of the exact value (a tie to the even digit), trailing zeros off,
/// an exponent below 1e-4 or from 10^p on.
fn fmt_g(x: f64, p: usize) -> String {
    if x.is_nan() {
        return "nan".to_owned();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf" } else { "-inf" }.to_owned();
    }
    let sign = if x.is_sign_negative() { "-" } else { "" };
    let a = x.abs();
    if a == 0.0 {
        return format!("{sign}0");
    }
    let pi = p as i64;
    let mut e = a.log10().floor() as i64;
    // the digits of the exact value rounded to p significant digits (fixed() rounds the exact binary value)
    let digits_at = |e: i64| -> Option<String> {
        let d = pi - 1 - e;
        if !(0..=18).contains(&d) || a >= 9.0e15 {
            return None;
        }
        let s = fixed(a, d as usize).replace('.', "");
        Some(s.trim_start_matches('0').to_owned())
    };
    let mut digits = None;
    for _ in 0..3 {
        match digits_at(e) {
            Some(d) if d.len() == p + 1 => e += 1,
            Some(d) if d.len() < p && !d.is_empty() => e -= 1,
            other => {
                digits = other;
                break;
            }
        }
    }
    let Some(digits) = digits.filter(|d| d.len() == p) else {
        return bc2_pyrt::fmt::float_g_prec(x, p);
    };
    if -4 <= e && e < pi {
        let body = fixed(a, (pi - 1 - e) as usize);
        let body = if body.contains('.') {
            body.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            body
        };
        format!("{sign}{body}")
    } else {
        let (h, t) = digits.split_at(1);
        let t = t.trim_end_matches('0');
        let mant = if t.is_empty() { h.to_owned() } else { format!("{h}.{t}") };
        format!("{sign}{mant}e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
    }
}

/// NumPy's `np.round(x, d)` for d > 0: multiply, round half to even, divide.
fn np_round(x: f64, d: i32) -> f64 {
    let f = 10f64.powi(d);
    (x * f).round_ties_even() / f
}

/// NumPy's pairwise summation (`pairwise_sum`, the sum over a contiguous array).
fn pairwise(a: &[f64]) -> f64 {
    let n = a.len();
    if n < 8 {
        let mut r = 0.0;
        for v in a {
            r += v;
        }
        return r;
    }
    if n <= 128 {
        let mut r = [0.0f64; 8];
        r.copy_from_slice(a.get(..8).unwrap_or(&[0.0; 8]));
        let mut i = 8;
        while i < n - n % 8 {
            for (j, rj) in r.iter_mut().enumerate() {
                *rj += a.get(i + j).copied().unwrap_or(0.0);
            }
            i += 8;
        }
        let mut res = ((r[0] + r[1]) + (r[2] + r[3])) + ((r[4] + r[5]) + (r[6] + r[7]));
        for v in a.get(i..).unwrap_or_default() {
            res += v;
        }
        return res;
    }
    let mut n2 = n / 2;
    n2 -= n2 % 8;
    let (l, r) = a.split_at(n2);
    pairwise(l) + pairwise(r)
}

/// NumPy's `max` / `min` of float64s: a NaN anywhere is the answer.
fn np_max(v: impl Iterator<Item = f64>) -> f64 {
    let mut m = f64::NEG_INFINITY;
    for x in v {
        if x.is_nan() {
            return f64::NAN;
        }
        if x > m {
            m = x;
        }
    }
    m
}

fn np_min(v: impl Iterator<Item = f64>) -> f64 {
    let mut m = f64::INFINITY;
    for x in v {
        if x.is_nan() {
            return f64::NAN;
        }
        if x < m {
            m = x;
        }
    }
    m
}

/// `_element(raw, off, fmt)`: each vertex's components as float64 (rows of `stride` bytes).
fn element(raw: &[u8], n: usize, stride: usize, off: usize, fmt: u8) -> R<Vec<Vec<f64>>> {
    let Some((ty, comps)) = dtype(fmt) else {
        return raised("KeyError", fmt.to_string());
    };
    let w = match ty {
        Ty::F32 => 4,
        Ty::F16 => 2,
        Ty::U8 => 1,
    };
    let size = w * comps;
    // raw[:, off:off + size] is cut short by the vertex's end: NumPy's view or reshape then refuses it
    let avail = size.min(stride.saturating_sub(off));
    if avail < size {
        if avail % w != 0 {
            return raised(
                "ValueError",
                "When changing to a larger dtype, its size must be a divisor of the total size in bytes of the last \
                 axis of the array.",
            );
        }
        return raised(
            "ValueError",
            format!(
                "cannot reshape array of size {} into shape ({n},{comps})",
                n * avail / w
            ),
        );
    }
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let base = i * stride + off;
        let row = raw.get(base..base + size).unwrap_or_default();
        let v: Vec<f64> = row
            .chunks_exact(w)
            .map(|c| match ty {
                Ty::F32 => f64::from(f32::from_le_bytes([c[0], c[1], c[2], c[3]])),
                Ty::F16 => bc2_convert::mesh::f16_to_f64(u16::from_le_bytes([c[0], c[1]])),
                Ty::U8 => f64::from(c[0]),
            })
            .collect();
        out.push(v);
    }
    Ok(out)
}

/// A part's counts as the Python's dict names them.
fn vertices(p: &MeshPart) -> u32 {
    p.counts[1]
}

fn triangles(p: &MeshPart) -> u32 {
    p.counts[0]
}

/// `compare_part(ours, dice, po, pd, layout)`: (verdict, notes).
#[allow(clippy::too_many_lines, reason = "compare_part, element by element")]
fn compare_part(
    ours: (&[u8], usize),
    dice: (&[u8], usize),
    po: &MeshPart,
    pd: &MeshPart,
    layout: &[u8],
) -> R<(String, Vec<String>)> {
    if (vertices(po), triangles(po), po.bytes[0]) != (vertices(pd), triangles(pd), pd.bytes[0]) {
        return Ok((
            "other-model".to_owned(),
            vec![format!(
                "{} / {} verts / tris, DICE {} / {}",
                vertices(po),
                triangles(po),
                vertices(pd),
                triangles(pd)
            )],
        ));
    }
    let (n, st) = (vertices(po) as usize, usize::from(po.bytes[0]));
    let take = |(data, at): (&[u8], usize), p: &MeshPart| -> R<Vec<u8>> {
        let from = at + p.counts[3] as usize;
        match data.get(from..from + n * st) {
            Some(b) => Ok(b.to_vec()),
            None => raised("ValueError", "buffer is smaller than requested size"),
        }
    };
    let (a, b) = (take(ours, po)?, take(dice, pd)?);
    if a == b {
        return Ok(("identical".to_owned(), Vec::new()));
    }
    let mut notes: Vec<String> = Vec::new();
    let mut beyond: Vec<String> = Vec::new();
    let Some((els, _)) = bc2_convert::meshconv::elements(layout) else {
        return raised("IndexError", "index out of range");
    };
    for (fmt, usage, off, stream) in els {
        let off = usize::from(off);
        if stream != 0 || dtype(fmt).is_none() {
            continue;
        }
        let col = |raw: &[u8]| -> Vec<u8> { (0..n).filter_map(|i| raw.get(i * st + off).copied()).collect() };
        let x = element(&a, n, st, off, fmt)?;
        let y = element(&b, n, st, off, fmt)?;
        if col(&a) == col(&b) && x == y {
            continue;
        }
        if matches!(usage, 4..=6) {
            // the normal frame: the angle between the two directions (their first three components)
            let mut cos = Vec::with_capacity(n);
            for (u, v) in x.iter().zip(&y) {
                let (u, v) = (&u[..u.len().min(3)], &v[..v.len().min(3)]);
                let mut dot = 0.0;
                let (mut nu, mut nv) = (0.0, 0.0);
                for (p, q) in u.iter().zip(v) {
                    dot += p * q;
                }
                for p in u {
                    nu += p * p;
                }
                for q in v {
                    nv += q * q;
                }
                let den = nu.sqrt() * nv.sqrt();
                let den = if den.is_nan() { den } else { den.max(1e-12) };
                cos.push(dot / den);
            }
            let c = np_min(cos.into_iter());
            let c = if c.is_nan() { c } else { c.clamp(-1.0, 1.0) };
            let worst = c.acos() * (180.0 / PI);
            let text = format!("usage {usage} @{off}: up to {} deg", fixed(worst, 3));
            if worst > PRECISION_DEG {
                beyond.push(text);
            } else {
                notes.push(text);
            }
        } else if usage == BLEND_INDICES {
            #[allow(clippy::float_cmp, reason = "NumPy's != of the values")]
            let k = x
                .iter()
                .zip(&y)
                .filter(|(p, q)| p.iter().zip(q.iter()).any(|(s, t)| s != t))
                .count();
            beyond.push(format!("part / bone indices @{off}: {k} vertices differ"));
        } else {
            let worst = np_max(
                x.iter()
                    .zip(&y)
                    .flat_map(|(p, q)| p.iter().zip(q).map(|(s, t)| (s - t).abs())),
            );
            // texture coordinates and every other element: PRECISION_UV (the reference's two branches agree)
            let tol = if usage == POSITION { PRECISION_POS } else { PRECISION_UV };
            let mut text = format!("usage {usage} @{off}: up to {}", fmt_g(worst, 4));
            if usage == POSITION && worst > tol {
                let k = x.first().map_or(0, |r| r.len().min(3));
                let diffs: Vec<Vec<f64>> = x
                    .iter()
                    .zip(&y)
                    .map(|(p, q)| p.iter().zip(q).take(3).map(|(s, t)| s - t).collect())
                    .collect();
                let mut shift = vec![0.0f64; k];
                for (j, s) in shift.iter_mut().enumerate() {
                    let mut acc = diffs.first().and_then(|r| r.get(j)).copied().unwrap_or(0.0);
                    for r in diffs.iter().skip(1) {
                        acc += r.get(j).copied().unwrap_or(0.0);
                    }
                    *s = acc / n as f64;
                }
                let spread = np_max(
                    diffs
                        .iter()
                        .flat_map(|r| r.iter().zip(&shift).map(|(d, s)| (d - s).abs())),
                );
                if spread < 1e-3 {
                    let shown: Vec<String> = shift.iter().map(|s| float_repr(np_round(*s, 3))).collect();
                    text.push_str(&format!(
                        " (one offset for every vertex: [{}] - the part rebased)",
                        shown.join(", ")
                    ));
                }
            }
            if worst > tol {
                beyond.push(text);
            } else {
                notes.push(text);
            }
        }
    }
    let verdict = if beyond.is_empty() { "precision" } else { "differs" };
    beyond.extend(notes);
    Ok((verdict.to_owned(), beyond))
}

/// A texture row: its keys in the Python dict's order.
type Row = OrderedMap<String, Value>;

/// The header words of a PC texture (`struct.unpack_from('<8I', b, 0)`).
fn header(b: &[u8]) -> R<[u32; 8]> {
    if b.len() < 32 {
        return raised(
            "error",
            format!(
                "unpack_from requires a buffer of at least 32 bytes for unpacking 32 bytes at offset 0 (actual buffer size is {})",
                b.len()
            ),
        );
    }
    Ok(std::array::from_fn(|i| {
        u32::from_le_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]])
    }))
}

/// A decoded picture: rows, columns, three float64 channels per pixel.
struct Img {
    h: usize,
    w: usize,
    px: Vec<f64>,
}

impl Img {
    fn shape(&self) -> String {
        format!("({},{},3)", self.h, self.w)
    }

    /// `img[r0::2, c0::2]`.
    fn take(&self, r0: usize, c0: usize) -> Img {
        let (h, w) = ((self.h + 1 - r0.min(self.h)) / 2, (self.w + 1 - c0.min(self.w)) / 2);
        let h = if r0 >= self.h { 0 } else { h };
        let w = if c0 >= self.w { 0 } else { w };
        let mut px = Vec::with_capacity(h * w * 3);
        for y in 0..h {
            for x in 0..w {
                let at = ((r0 + 2 * y) * self.w + c0 + 2 * x) * 3;
                px.extend_from_slice(self.px.get(at..at + 3).unwrap_or(&[0.0; 3]));
            }
        }
        Img { h, w, px }
    }

    /// `a + b` (NumPy's broadcast error for other shapes).
    fn add(&self, o: &Img) -> R<Img> {
        if (self.h, self.w) != (o.h, o.w) {
            return raised(
                "ValueError",
                format!(
                    "operands could not be broadcast together with shapes {} {} ",
                    self.shape(),
                    o.shape()
                ),
            );
        }
        Ok(Img {
            h: self.h,
            w: self.w,
            px: self.px.iter().zip(&o.px).map(|(a, b)| a + b).collect(),
        })
    }

    /// `(a[0::2, 0::2] + a[1::2, 0::2] + a[0::2, 1::2] + a[1::2, 1::2]) / 4`.
    fn half(&self) -> R<Img> {
        let s = self
            .take(0, 0)
            .add(&self.take(1, 0))?
            .add(&self.take(0, 1))?
            .add(&self.take(1, 1))?;
        Ok(Img {
            px: s.px.iter().map(|v| v / 4.0).collect(),
            ..s
        })
    }
}

/// `_level0(pc)`: level 0 of slice 0, decoded (None: no such level).
fn level0(pc: &[u8]) -> R<Option<Img>> {
    let fmt = header(pc)?[2];
    let Some(levels) = bc2_convert::texcheck::levels(pc) else {
        return raised(
            "Refused",
            "levels: a PC texture the reader does not read (cut short or damaged)",
        );
    };
    for (s, m, w, h, r) in levels {
        if m == 0 && s == 0 {
            let data = pc.get(r).unwrap_or_default();
            let Some((rows, cols, px)) = bc2_convert::texcheck::decode(data, i64::from(w), i64::from(h), fmt) else {
                return raised(
                    "Refused",
                    format!(
                        "decode: a {w} x {h} level of format {fmt} the decoder does not read (cut short, or a format it \
                         does not know)"
                    ),
                );
            };
            return Ok(Some(Img {
                h: rows,
                w: cols,
                px: px.iter().flat_map(|p| p.iter().map(|&v| f64::from(v))).collect(),
            }));
        }
    }
    Ok(None)
}

/// `compare_texture(name, ours, dice)`.
fn compare_texture(name: &str, ours: &[u8], dice: Option<&[u8]>) -> R<Row> {
    let mut row = Row::new();
    row.insert("texture".to_owned(), Value::Str(name.to_owned()));
    let Some(dice) = dice else {
        row.insert("verdict".to_owned(), Value::Str("DICE has none".to_owned()));
        return Ok(row);
    };
    let (ho, hd) = (header(ours)?, header(dice)?);
    row.insert(
        "ours".to_owned(),
        Value::Str(format!("{}x{} fmt {} mips {}", ho[4], ho[5], ho[2], ho[7])),
    );
    row.insert(
        "dice".to_owned(),
        Value::Str(format!("{}x{} fmt {} mips {}", hd[4], hd[5], hd[2], hd[7])),
    );
    if ours == dice {
        row.insert("verdict".to_owned(), Value::Str("identical".to_owned()));
        return Ok(row);
    }
    let measured = (|| -> R<Option<f64>> {
        let (a, b) = (level0(ours)?, level0(dice)?);
        let (Some(mut a), Some(mut b)) = (a, b) else {
            return raised("AttributeError", "'NoneType' object has no attribute 'shape'");
        };
        while a.h > b.h && a.w > b.w {
            a = a.half()?;
        }
        while b.h > a.h && b.w > a.w {
            b = b.half()?;
        }
        if (a.h, a.w) != (b.h, b.w) {
            return Ok(None);
        }
        let sq: Vec<f64> = a.px.iter().zip(&b.px).map(|(p, q)| (p - q) * (p - q)).collect();
        let mse = pairwise(&sq) / sq.len() as f64;
        Ok(Some(if mse == 0.0 {
            f64::INFINITY
        } else {
            np_round(10.0 * (255.0f64.powi(2) / mse).log10(), 1)
        }))
    })();
    match measured {
        Ok(None) => {
            row.insert("verdict".to_owned(), Value::Str("other shape".to_owned()));
        }
        Ok(Some(psnr)) => {
            row.insert("psnr".to_owned(), Value::Float(psnr));
            let size = if hd[4] > ho[4] {
                "DICE larger"
            } else if hd[4] < ho[4] {
                "DICE smaller"
            } else {
                "same size"
            };
            let pic = if psnr >= 35.0 {
                ", same picture"
            } else if psnr < 20.0 {
                ", other picture"
            } else {
                ", close"
            };
            row.insert("verdict".to_owned(), Value::Str(format!("{size}{pic}")));
        }
        Err(Raised(_, m)) => {
            row.insert("verdict".to_owned(), Value::Str(format!("not compared ({m})")));
        }
    }
    Ok(row)
}

/// One LOD's row.
#[derive(Default)]
struct LodRow {
    lod: usize,
    parts: OrderedMap<String, (String, Vec<String>)>,
    note: Option<String>,
    identical: bool,
    index_buffer: Option<&'static str>,
}

/// One model's row (or its error).
struct ModelRow {
    mesh: String,
    error: Option<String>,
    kind: String,
    lods: Vec<LodRow>,
    meshset: Vec<String>,
    textures: Vec<Row>,
}

/// The mesh set comparison's notes.
fn compare_meshset(ours: &[u8], dice: Option<&[u8]>, kind: &str) -> Vec<String> {
    if Some(ours) == dice {
        return vec!["identical".to_owned()];
    }
    let parse = |b: Option<&[u8]>| -> Result<meshdata::MeshSet, String> {
        let Some(k) = Kind::of(kind) else {
            return Err(format!(
                "{kind}: only RigidMeshSet, CompositeMeshSet, SkinnedMeshSet, TreeMeshSet are converted"
            ));
        };
        b.and_then(|b| meshdata::parse_set(b, k, false, None).ok())
            .ok_or_else(|| format!("{kind}: a mesh set the reader does not read (cut short or damaged)"))
    };
    let mut out = Vec::new();
    let (a, b) = match (parse(Some(ours)), parse(dice)) {
        (Ok(a), Ok(b)) => (a, b),
        (Err(e), _) | (_, Err(e)) => return vec![format!("not compared ({e})")],
    };
    if a.lods.len() != b.lods.len() {
        out.push(format!("{} LODs, DICE {}", a.lods.len(), b.lods.len()));
    }
    let (Some(la), Some(lb)) = (a.lods.first(), b.lods.first()) else {
        out.push("not compared (list index out of range)".to_owned());
        return out;
    };
    let parts = |l: &meshdata::Lod| l.parts.as_ref().map(|(p, _)| p.clone()).unwrap_or_default();
    let (ca, cb) = (parts(la), parts(lb));
    if ca.iter().map(|c| c.0).ne(cb.iter().map(|c| c.0)) {
        out.push("composite part names / order differ".to_owned());
    } else {
        let worst = np_max(
            ca.iter()
                .zip(&cb)
                .map(|(x, y)| np_max(x.3.iter().zip(&y.3).map(|(p, q)| (f64::from(*p) - f64::from(*q)).abs()))),
        );
        let worst = if ca.is_empty() { 0.0 } else { worst };
        if worst > 1e-4 {
            out.push(format!("composite part transforms differ by up to {}", fmt_g(worst, 4)));
        }
    }
    if out.is_empty() {
        out.push("same LODs and parts (other bytes: sizes, flags)".to_owned());
    }
    out
}

fn mesh_parse(b: &[u8]) -> R<MeshHeader> {
    meshdata::parse_mesh(b, false).or_else(|_| {
        raised(
            "Refused",
            format!(
                "mesh data: {} bytes the header reader does not read (cut short or damaged)",
                b.len()
            ),
        )
    })
}

/// `compare_mesh(port, ix, name)`.
fn compare_mesh(port: &mut Porter<'_>, ix: &Index, name: &str) -> R<ModelRow> {
    let r: Ported = port.port(name).map_err(|e| Raised(e.kind, e.msg))?;
    let low = name.to_lowercase();
    let dice_ms = ix.read(&format!("{low}.res"));
    let mut out = ModelRow {
        mesh: name.to_owned(),
        error: None,
        kind: r.kind.clone(),
        lods: Vec::new(),
        meshset: compare_meshset(&r.meshset, dice_ms.as_deref(), &r.kind),
        textures: Vec::new(),
    };
    for (lod, ours) in r.lods.iter().enumerate() {
        let mut row = LodRow {
            lod,
            ..LodRow::default()
        };
        let Some(dice) = ix.read(&format!("{low}_lod{lod}_data.res")) else {
            row.note = Some("DICE has no such LOD".to_owned());
            out.lods.push(row);
            continue;
        };
        if *ours == dice {
            row.identical = true;
            out.lods.push(row);
            continue;
        }
        let (mo, md) = (mesh_parse(ours)?, mesh_parse(&dice)?);
        let by_place = mo.parts.len() == md.parts.len();
        for (i, po) in mo.parts.iter().enumerate() {
            let pd = md
                .parts
                .iter()
                .rev()
                .find(|p| p.name == po.name)
                .or_else(|| if by_place { md.parts.get(i) } else { None });
            let mut pn: String = po.name.iter().map(|&c| char::from(c)).collect();
            let Some(pd) = pd else {
                row.parts.insert(
                    pn,
                    (
                        "missing".to_owned(),
                        vec![format!(
                            "DICE has no part of this name ({} parts, ours {})",
                            md.parts.len(),
                            mo.parts.len()
                        )],
                    ),
                );
                continue;
            };
            if pd.name != po.name {
                let dn: String = pd.name.iter().map(|&c| char::from(c)).collect();
                pn.push_str(&format!(" (DICE: {dn})"));
            }
            let layout = r
                .pc_layouts
                .get(lod)
                .and_then(|l| l.get(i))
                .map_or(&[][..], Vec::as_slice);
            let v = compare_part((ours, mo.data), (&dice, md.data), po, pd, layout)?;
            row.parts.insert(pn, v);
        }
        let io = ours.get(mo.data + mo.sizes[0] as usize..).unwrap_or_default();
        let idd = dice.get(md.data + md.sizes[0] as usize..).unwrap_or_default();
        row.index_buffer = Some(if io == idd { "same" } else { "differs" });
        out.lods.push(row);
    }
    let mut textures: Vec<&(String, Vec<u8>)> = r.textures.iter().collect();
    textures.sort();
    for (t, tb) in textures {
        let dice = ix.read(&t.to_lowercase());
        out.textures.push(compare_texture(t, tb, dice.as_deref())?);
    }
    Ok(out)
}

/// `shared_meshes(src, ix)`: the models both games have (sorted).
fn shared_meshes(src: &crate::bf1943src::Source, ix: &Index) -> Vec<String> {
    const SUFFIX: &str = "_lod0_data.res";
    let bf: std::collections::BTreeSet<&str> = src.entries.keys().filter_map(|k| k.strip_suffix(SUFFIX)).collect();
    let bc: std::collections::HashSet<&str> = ix.rows().filter_map(|(k, _, _)| k.strip_suffix(SUFFIX)).collect();
    bf.into_iter()
        .filter(|k| bc.contains(k) && src.entries.contains_key(&format!("{k}.res")))
        .map(str::to_owned)
        .collect()
}

fn json_row(r: &ModelRow) -> Value {
    if let Some(e) = &r.error {
        return Value::dict([("mesh", Value::Str(r.mesh.clone())), ("error", Value::Str(e.clone()))]);
    }
    let lods = r
        .lods
        .iter()
        .map(|l| {
            let mut d = vec![("lod".to_owned(), Value::Int(l.lod as i128))];
            let parts = l
                .parts
                .iter()
                .map(|(k, (v, notes))| {
                    (
                        k.clone(),
                        Value::List(vec![
                            Value::Str(v.clone()),
                            Value::List(notes.iter().map(|n| Value::Str(n.clone())).collect()),
                        ]),
                    )
                })
                .collect();
            d.push(("parts".to_owned(), Value::Dict(parts)));
            if let Some(n) = &l.note {
                d.push(("note".to_owned(), Value::Str(n.clone())));
            }
            if l.identical {
                d.push(("identical".to_owned(), Value::Bool(true)));
            }
            if let Some(b) = l.index_buffer {
                d.push(("index_buffer".to_owned(), Value::Str(b.to_owned())));
            }
            Value::Dict(d)
        })
        .collect();
    Value::dict([
        ("mesh", Value::Str(r.mesh.clone())),
        ("kind", Value::Str(r.kind.clone())),
        ("lods", Value::List(lods)),
        (
            "meshset",
            Value::List(r.meshset.iter().map(|m| Value::Str(m.clone())).collect()),
        ),
        (
            "textures",
            Value::List(
                r.textures
                    .iter()
                    .map(|t| Value::Dict(t.iter().map(|(k, v)| (k.clone(), v.clone())).collect()))
                    .collect(),
            ),
        ),
    ])
}

/// `str()` of a row value as the report's f-strings print `t.get(key)`.
fn shown(t: &Row, key: &str) -> String {
    match t.get(&key.to_owned()) {
        None => "None".to_owned(),
        Some(Value::Str(s)) => s.clone(),
        Some(Value::Float(f)) => float_repr(*f),
        Some(v) => bc2_pyrt::json::dumps(v, None),
    }
}

/// `python portparity.py ...` once its command line parsed.
pub fn run(py: &mut Py, run: &Run) -> PyResult<i32> {
    let (src, ix, _root) = super::open(py)?;
    let mut port = Porter::new(&src, &ix);
    let names = shared_meshes(&src, &ix);
    let todo: Vec<&String> = names
        .iter()
        .filter(|k| run.names.is_empty() || run.names.iter().any(|x| k.contains(&x.to_lowercase())))
        .collect();
    let mut rows: Vec<ModelRow> = Vec::with_capacity(todo.len());
    for n in todo {
        match compare_mesh(&mut port, &ix, n) {
            Ok(r) => rows.push(r),
            Err(Raised(k, m)) => rows.push(ModelRow {
                mesh: n.clone(),
                error: Some(format!("{k}: {m}")),
                kind: String::new(),
                lods: Vec::new(),
                meshset: Vec::new(),
                textures: Vec::new(),
            }),
        }
        py.print(&format!("  {n}"))?;
        py.flush_now();
    }
    let json = Value::List(rows.iter().map(json_row).collect());
    let path = format!("{}.json", bc2_pyrt::py::splitext(&run.o).0);
    write_report(&path, &bc2_pyrt::json::dumps(&json, Some(1)), false)?;
    // summary(rows)
    let mut parts: OrderedMap<String, i64> = OrderedMap::new();
    let mut tex: OrderedMap<String, i64> = OrderedMap::new();
    for r in &rows {
        for l in &r.lods {
            if l.identical {
                *parts.entry_default("identical (whole LOD)".to_owned()) += 1;
            }
            for (_, (v, _)) in l.parts.iter() {
                *parts.entry_default(v.clone()) += 1;
            }
        }
        for t in &r.textures {
            *tex.entry_default(shown(t, "verdict")) += 1;
        }
    }
    let parts = py_dict(parts.iter().map(|(k, v)| (str_repr(k), v.to_string())));
    let tex = py_dict(tex.iter().map(|(k, v)| (str_repr(k), v.to_string())));
    let mut text = format!(
        "{} models both games have; parts: {parts}; textures: {tex}\n\n",
        rows.len()
    );
    for r in &rows {
        let head = r.error.clone().unwrap_or_else(|| r.meshset.join("; "));
        text.push_str(&format!("{}: {head}\n", r.mesh));
        for l in &r.lods {
            if l.identical {
                text.push_str(&format!("  lod{}: identical\n", l.lod));
                continue;
            }
            if let Some(n) = &l.note {
                text.push_str(&format!("  lod{}: {n}\n", l.lod));
            }
            for (pn, (v, notes)) in l.parts.iter() {
                let tail = if notes.is_empty() {
                    String::new()
                } else {
                    format!(" - {}", notes.join("; "))
                };
                text.push_str(&format!("  lod{} {pn}: {v}{tail}\n", l.lod));
            }
            if let Some(b) = l.index_buffer {
                text.push_str(&format!("  lod{} index buffer: {b}\n", l.lod));
            }
        }
        for t in &r.textures {
            text.push_str(&format!(
                "  texture {}: {} (ours {}, DICE {}, PSNR {})\n",
                shown(t, "texture"),
                shown(t, "verdict"),
                shown(t, "ours"),
                shown(t, "dice"),
                shown(t, "psnr")
            ));
        }
    }
    write_report(&run.o, &text, false)?;
    py.print(&format!(
        "{} models; parts {parts}; textures {tex} -> {}",
        rows.len(),
        run.o
    ))?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_as_python() {
        assert_eq!(fmt_g(0.000_976_562_5, 4), "0.0009766");
        assert_eq!(fmt_g(1.031_25, 4), "1.031");
        assert_eq!(fmt_g(2.0, 4), "2");
        assert_eq!(fmt_g(12345.0, 4), "1.234e+04");
        assert_eq!(fmt_g(0.000_01, 4), "1e-05");
        assert_eq!(float_repr(np_round(0.9386, 3)), "0.939");
        assert_eq!(float_repr(np_round(-0.0001, 3)), "-0.0");
        assert_eq!(pairwise(&[1.0; 300]), 300.0);
    }

    /// A float2 element is two singles (the game's format, not the retired Python's three).
    // spec: XWW2-PAR-3
    #[test]
    fn float2_is_two_singles() {
        assert!(dtype(0x02) == Some((Ty::F32, 2)));
        let row: Vec<u8> = [1.5f32, -2.0].iter().flat_map(|v| v.to_le_bytes()).collect();
        assert!(matches!(element(&row, 1, 8, 0, 0x02), Ok(v) if v == vec![vec![1.5, -2.0]]));
    }
}
