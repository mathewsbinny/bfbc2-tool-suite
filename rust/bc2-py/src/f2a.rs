//! Round F2a: the rest of the console converter (`terrain.py`, `soot.py`, `mesh.py`'s `console_layouts` /
//! `decode` / `relayout`, `havok.convert_wrapper`, `texture.check`), the model tools (`texwrite.py`,
//! `skinmesh.py`) and Granny's writing side (`granny.py`'s `to_bytes`, `add_tracks`, `shift_position`,
//! `set_orientations`) in the Rust core (`bc2_convert`'s `terrain`, `meshconv`, `havok`, `texcheck`, `texwrite`,
//! `skinmesh`, `granny_edit`).
//!
//! The convention: a function answers `None` where the Python reference must run (it raises its own exception, or
//! meets what the port does not mirror), `(False, message)` where the reference raises its module's own error with
//! that message (`TerrainError`, `SootError`, `ConvertError`), and `(True, value)` otherwise. The Python keeps its
//! code as the reference (`*_reference`, `use_native(False)`, `BFBC2_PURE_PYTHON=1`).

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
    clippy::needless_pass_by_value,
    clippy::redundant_closure_for_method_calls,
    reason = "every index is checked against the data's length first (the walks' need(), the decoders' block counts, the plans' key counts); the reference's names, comparisons, casts and arithmetic as it has them (a float midpoint, an exact float compare and a table constant are its values, not approximations)"
)]

use bc2_convert::granny_edit::{self, Curve, Put, Spec, Track};
use bc2_convert::meshconv::{self, Kind, Refusal, Values, Which};
use bc2_convert::{havok, skinmesh, terrain, texcheck, texwrite};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyList, PyString, PyTuple};

use crate::pathdata::{f32_of, items};

fn answer<'py, T: IntoPyObject<'py>>(py: Python<'py>, r: Result<T, Refusal>) -> PyResult<Option<Bound<'py, PyAny>>> {
    match r {
        Ok(v) => Ok(Some((true, v).into_pyobject(py)?.into_any())),
        Err(Refusal::Error(m)) => Ok(Some((false, m).into_pyobject(py)?.into_any())),
        Err(Refusal::Reference) => Ok(None),
    }
}

fn latin1<'py>(py: Python<'py>, b: &[u8]) -> Bound<'py, PyString> {
    PyString::new(py, &b.iter().map(|&c| char::from(c)).collect::<String>())
}

// ------------------------------------------------------------------------------------------------ terrain, soot

/// `terrain.heightfield(x, source)` (`bfbc2`: source == 'bfbc2').
#[pyfunction]
pub fn terrain_heightfield<'py>(
    py: Python<'py>,
    x: &Bound<'py, PyBytes>,
    bfbc2: bool,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let r = terrain::heightfield(x.as_bytes(), bfbc2).map(|v| PyBytes::new(py, &v));
    answer(py, r)
}

/// `terrain.water_mesh(x)`, `material_map`, `visual_terrain`, `visual_water`, `soot.soot_mesh` by name.
#[pyfunction]
pub fn terrain_convert<'py>(
    py: Python<'py>,
    kind: &str,
    x: &Bound<'py, PyBytes>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let x = x.as_bytes();
    let r = match kind {
        "water_mesh" => terrain::water_mesh(x),
        "material_map" => terrain::material_map(x),
        "visual_terrain" => terrain::visual_terrain(x),
        "visual_water" => terrain::visual_water(x),
        "soot_mesh" => terrain::soot_mesh(x),
        _ => Err(Refusal::Reference),
    };
    answer(py, r.map(|v| PyBytes::new(py, &v)))
}

fn parts<'py>(py: Python<'py>, data: &[u8], p: &[terrain::Part]) -> PyResult<Bound<'py, PyList>> {
    let out = PyList::empty(py);
    for (name, a, b) in p {
        out.append((name.as_str(), PyBytes::new(py, &data[*a..*b])))?;
    }
    Ok(out)
}

/// `terrain.visual_terrain_parts(pc)`: `[(name, bytes)]`.
#[pyfunction]
pub fn terrain_visual_terrain_parts<'py>(
    py: Python<'py>,
    pc: &Bound<'py, PyBytes>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let d = pc.as_bytes();
    match terrain::visual_terrain_parts(d) {
        Ok(p) => answer(py, Ok(parts(py, d, &p)?)),
        Err(e) => answer::<bool>(py, Err(e)),
    }
}

/// `terrain.visual_water_parts(pc)`'s vertex count and parts: `(count, [(name, bytes)])` (the vertices are the
/// 36-byte records from byte 12).
#[pyfunction]
pub fn terrain_visual_water_parts<'py>(
    py: Python<'py>,
    pc: &Bound<'py, PyBytes>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let d = pc.as_bytes();
    match terrain::visual_water_parts(d) {
        Ok((n, p)) => answer(py, Ok((n, parts(py, d, &p)?))),
        Err(e) => answer::<bool>(py, Err(e)),
    }
}

/// `terrain.road_strips(pc)`: the dicts, `indices` as bytes (little-endian u32: the caller makes the array).
#[pyfunction]
pub fn terrain_road_strips<'py>(py: Python<'py>, pc: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyAny>>> {
    let blocks = match terrain::road_strips(pc.as_bytes()) {
        Ok(b) => b,
        Err(e) => return answer::<bool>(py, Err(e)),
    };
    let out = PyList::empty(py);
    for (i, (strips, verts, idx)) in blocks.iter().enumerate() {
        let sl = PyList::empty(py);
        for s in strips {
            let d = PyDict::new(py);
            d.set_item("shader", latin1(py, s.names[0]))?;
            d.set_item("second", latin1(py, s.names[1]))?;
            d.set_item("mask", latin1(py, s.names[2]))?;
            d.set_item("first", s.first)?;
            d.set_item("triangles", s.triangles)?;
            d.set_item("bounds", PyTuple::new(py, s.bounds.iter().map(|&v| f64::from(v)))?)?;
            d.set_item("layer", s.layer)?;
            sl.append(d)?;
        }
        let d = PyDict::new(py);
        d.set_item("block", i)?;
        d.set_item("strips", sl)?;
        d.set_item("vertices", PyBytes::new(py, verts))?;
        d.set_item("indices", PyBytes::new(py, idx))?;
        out.append(d)?;
    }
    answer(py, Ok(out))
}

/// `terrain.material_names(pc)`.
#[pyfunction]
pub fn terrain_material_names<'py>(py: Python<'py>, pc: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyAny>>> {
    match terrain::material_names(pc.as_bytes()) {
        Ok(v) => answer(py, Ok(PyList::new(py, v.iter().map(|n| latin1(py, n)))?)),
        Err(e) => answer::<bool>(py, Err(e)),
    }
}

/// `havok.convert_wrapper(b, o)`.
#[pyfunction]
pub fn havok_convert_wrapper<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    o: usize,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let r = match havok::convert_wrapper(b.as_bytes(), o) {
        Ok(v) => Ok(PyBytes::new(py, &v)),
        Err(havok::WrapperError::Refused(m)) => Err(Refusal::Error(m)),
        Err(havok::WrapperError::Reference) => Err(Refusal::Reference),
    };
    answer(py, r)
}

// ------------------------------------------------------------------------------------------------ mesh

/// `mesh.console_layouts(db)`: `{name: layout record body}`.
#[pyfunction]
pub fn mesh_console_layouts<'py>(py: Python<'py>, db: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyDict>> {
    let data = db.as_bytes();
    let got = py.detach(|| meshconv::console_layouts(data));
    let d = PyDict::new(py);
    for (n, r) in &got {
        d.set_item(latin1(py, n), PyBytes::new(py, r))?;
    }
    Ok(d)
}

fn array<'py>(py: Python<'py>, a: &meshconv::Array) -> PyResult<Bound<'py, PyTuple>> {
    let (code, bytes): (&str, Vec<u8>) = match &a.data {
        Values::F32(v) => ("<f4", v.iter().flat_map(|x| x.to_le_bytes()).collect()),
        Values::U8(v) => ("u1", v.clone()),
        Values::I64(v) => ("<i8", v.iter().flat_map(|x| x.to_le_bytes()).collect()),
    };
    PyTuple::new(
        py,
        [
            a.key.into_pyobject(py)?.into_any(),
            code.into_pyobject(py)?.into_any(),
            a.rows.into_pyobject(py)?.into_any(),
            a.cols.into_pyobject(py)?.into_any(),
            PyByteArray::new(py, &bytes).into_any(),
        ],
    )
}

/// `mesh.decode(b, layout, part, big_endian)`: `[(key, dtype, rows, cols, bytearray)]` in the dict's order.
#[pyfunction]
pub fn mesh_decode<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    layout: &Bound<'py, PyBytes>,
    part: &Bound<'py, PyAny>,
    big_endian: bool,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let name;
    let which = if let Ok(s) = part.cast::<PyString>() {
        let s = s.to_str()?;
        if s.chars().any(|c| u32::from(c) > 255) {
            return Ok(None);
        }
        name = s.chars().map(|c| c as u8).collect::<Vec<u8>>();
        Which::Name(&name)
    } else if let Ok(i) = part.extract::<i64>() {
        Which::Index(i)
    } else {
        return Ok(None);
    };
    match meshconv::decode(b.as_bytes(), layout.as_bytes(), which, big_endian) {
        Ok(arrays) => {
            let out = PyList::empty(py);
            for a in &arrays {
                out.append(array(py, a)?)?;
            }
            answer(py, Ok(out))
        }
        Err(e) => answer::<bool>(py, Err(e)),
    }
}

fn layouts(o: &Bound<'_, PyAny>) -> Option<Vec<Vec<u8>>> {
    items(o)?
        .iter()
        .map(|x| x.cast::<PyBytes>().ok().map(|b| b.as_bytes().to_vec()))
        .collect()
}

/// `mesh.relayout(b, old, new)` without an edit.
#[pyfunction]
pub fn mesh_relayout<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    old: &Bound<'py, PyAny>,
    new: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let (Some(old), Some(new)) = (layouts(old), layouts(new)) else {
        return Ok(None);
    };
    let (o, n): (Vec<&[u8]>, Vec<&[u8]>) = (
        old.iter().map(Vec::as_slice).collect(),
        new.iter().map(Vec::as_slice).collect(),
    );
    let r = meshconv::relayout(b.as_bytes(), &o, &n).map(|v| PyBytes::new(py, &v));
    answer(py, r)
}

/// `mesh.relayout`'s blocks for an edit: `(True, [(start, members, n, so, sn, [(dtype, components, bytearray)])])`
/// (each column as the reference makes it, float32 or uint8), `(False, message)` when the reference raises its
/// error after the blocks before it (their edits run first), `None` where the reference must run from the start.
#[pyfunction]
pub fn mesh_relayout_blocks<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    old: &Bound<'py, PyAny>,
    new: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let (Some(old), Some(new)) = (layouts(old), layouts(new)) else {
        return Ok(None);
    };
    let (o, n): (Vec<&[u8]>, Vec<&[u8]>) = (
        old.iter().map(Vec::as_slice).collect(),
        new.iter().map(Vec::as_slice).collect(),
    );
    let (_, blocks, err) = match meshconv::relayout_blocks(b.as_bytes(), &o, &n) {
        Ok(x) => x,
        Err(e) => return answer::<bool>(py, Err(e)),
    };
    let tail = match err {
        None => py.None().into_bound(py),
        Some(Refusal::Error(m)) => m.into_pyobject(py)?.into_any(),
        Some(Refusal::Reference) => return Ok(None),
    };
    let out = PyList::empty(py);
    for blk in &blocks {
        let cols = PyList::empty(py);
        for (kind, comps, data) in &blk.cols {
            let code = if *kind == Kind::U8 { "u1" } else { "<f4" };
            cols.append((code, comps, PyByteArray::new(py, data)))?;
        }
        out.append((blk.start, blk.members.clone(), blk.n, blk.so, blk.sn, cols))?;
    }
    answer(py, Ok((out, tail)))
}

/// `texture.check(pc, reference)`'s statistics: `(True, None)` for a format it does not compare, else `(True,
/// (worst, same, total, (w, h, (a, b), (c, d)) or None))` - the last for the correlation the caller computes.
#[pyfunction]
#[pyo3(signature = (pc, reference=None))]
pub fn texture_check<'py>(
    py: Python<'py>,
    pc: &Bound<'py, PyBytes>,
    reference: Option<&Bound<'py, PyBytes>>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let (p, r) = (pc.as_bytes(), reference.map(|x| x.as_bytes()));
    let Some(got) = py.detach(|| texcheck::check(p, r)) else {
        return Ok(None);
    };
    let Some(c) = got else { return answer(py, Ok(py.None())) };
    let blocks = c
        .blocks
        .map(|(same, total, (w, h, d, rr))| (same, total, (w, h, (d.start, d.end), (rr.start, rr.end))));
    let v = match blocks {
        Some((same, total, args)) => (c.worst, Some(same), Some(total), Some(args))
            .into_pyobject(py)?
            .into_any(),
        None => (c.worst, py.None(), py.None(), py.None()).into_pyobject(py)?.into_any(),
    };
    answer(py, Ok(v))
}

// ------------------------------------------------------------------------------------------------ texwrite

/// `texwrite.levels(img, normal, channels)` of a `uint8` picture (`h` x `w` x `ch` bytes): `[(h, w, bytes)]`, or
/// `None` where the reference must run.
#[pyfunction]
pub fn texwrite_levels<'py>(
    py: Python<'py>,
    px: &Bound<'py, PyBytes>,
    h: usize,
    w: usize,
    ch: usize,
    normal: bool,
    channels: usize,
) -> PyResult<Option<Bound<'py, PyList>>> {
    let img = texwrite::Image {
        h,
        w,
        ch,
        px: px.as_bytes().to_vec(),
    };
    let Some(lv) = py.detach(|| texwrite::levels(&img, normal, channels)) else {
        return Ok(None);
    };
    let out = PyList::empty(py);
    for l in &lv {
        out.append((l.h, l.w, PyBytes::new(py, &l.px)))?;
    }
    Ok(Some(out))
}

/// Levels being encoded (`dxt1` / `dxt5` of each): the covariances for `numpy.linalg.eigh`, then the blocks.
#[pyclass(module = "bc2_native")]
pub struct TexWriteJob {
    levels: Vec<texwrite::Prepared>,
}

#[pymethods]
impl TexWriteJob {
    /// Levels `[(h, w, ch, bytes)]` encoded as DXT1 (`dxt5` false) or DXT5; `None` for pictures the port does not
    /// read (the caller runs the reference).
    #[staticmethod]
    fn prepare(py: Python<'_>, levels: Vec<(usize, usize, usize, Vec<u8>)>, dxt5: bool) -> Option<Self> {
        let enc = if dxt5 {
            texwrite::Encoding::Dxt5
        } else {
            texwrite::Encoding::Dxt1
        };
        py.detach(|| {
            let mut out = Vec::with_capacity(levels.len());
            for (h, w, ch, px) in levels {
                if px.len() != h * w * ch {
                    return None;
                }
                out.push(texwrite::Prepared::new(&texwrite::Image { h, w, ch, px }, enc)?);
            }
            Some(Self { levels: out })
        })
    }

    /// Every block's covariance, level after level: float64 bytes, 9 per block.
    fn covariances<'py>(&self, py: Python<'py>) -> Bound<'py, PyByteArray> {
        let mut out = Vec::new();
        for l in &self.levels {
            for c in l.covariances() {
                for v in c {
                    out.extend_from_slice(&v.to_le_bytes());
                }
            }
        }
        PyByteArray::new(py, &out)
    }

    /// Each level's blocks from the axes (float64 bytes, 3 per block, in the covariances' order); `None` for axes
    /// of another count.
    fn finish<'py>(&self, py: Python<'py>, axes: &Bound<'py, PyBytes>) -> Option<Bound<'py, PyList>> {
        let raw = axes.as_bytes();
        let total: usize = self.levels.iter().map(texwrite::Prepared::len).sum();
        if raw.len() != total * 24 {
            return None;
        }
        let all: Vec<[f64; 3]> = raw
            .chunks_exact(24)
            .map(|c| std::array::from_fn(|k| f64::from_le_bytes(c[8 * k..8 * k + 8].try_into().unwrap_or_default())))
            .collect();
        let data: Vec<Vec<u8>> = py.detach(|| {
            let mut at = 0;
            self.levels
                .iter()
                .map(|l| {
                    let d = l.finish(&all[at..at + l.len()]);
                    at += l.len();
                    d
                })
                .collect()
        });
        PyList::new(py, data.iter().map(|d| PyBytes::new(py, d))).ok()
    }
}

fn i64_array<'py>(py: Python<'py>, rows: usize, cols: usize, v: &[i64]) -> PyResult<Bound<'py, PyTuple>> {
    let bytes: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
    PyTuple::new(
        py,
        [
            rows.into_pyobject(py)?.into_any(),
            cols.into_pyobject(py)?.into_any(),
            PyByteArray::new(py, &bytes).into_any(),
        ],
    )
}

/// `texwrite.alpha1(level, w, h)` / `alpha5` (`five`): `(rows, cols, int64 bytearray)`, or `None`.
#[pyfunction]
pub fn texwrite_alpha<'py>(
    py: Python<'py>,
    level: &Bound<'py, PyBytes>,
    w: usize,
    h: usize,
    five: bool,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let d = level.as_bytes();
    let got = if five {
        texwrite::alpha5(d, w, h)
    } else {
        texwrite::alpha1(d, w, h)
    };
    got.map(|v| i64_array(py, h, w, &v)).transpose()
}

/// `texwrite.l8_down(level)` of an `h` x `w` uint8 level, or `None`.
#[pyfunction]
pub fn texwrite_l8_down<'py>(
    py: Python<'py>,
    px: &Bound<'py, PyBytes>,
    h: usize,
    w: usize,
) -> Option<Bound<'py, PyBytes>> {
    texwrite::l8_down(px.as_bytes(), h, w).map(|v| PyBytes::new(py, &v))
}

// ------------------------------------------------------------------------------------------------ skinmesh

fn elements(layout: &[u8]) -> Option<(Vec<meshconv::Element>, usize)> {
    meshconv::elements(layout)
}

fn i64s(b: &[u8]) -> Option<Vec<i64>> {
    if !b.len().is_multiple_of(8) {
        return None;
    }
    Some(
        b.chunks_exact(8)
            .map(|c| i64::from_le_bytes(c.try_into().unwrap_or_default()))
            .collect(),
    )
}

fn f64s(b: &[u8]) -> Option<Vec<f64>> {
    if !b.len().is_multiple_of(8) {
        return None;
    }
    Some(
        b.chunks_exact(8)
            .map(|c| f64::from_le_bytes(c.try_into().unwrap_or_default()))
            .collect(),
    )
}

/// `SkinnedLod.__init__`'s part: `(rows, n, idx int64 bytes)`, or `None`.
#[pyfunction]
pub fn skin_cut<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    ibase: usize,
    vbase: usize,
    counts: [u32; 4],
    stride: usize,
) -> Option<Bound<'py, PyTuple>> {
    let c = skinmesh::cut(b.as_bytes(), ibase, vbase, counts, stride)?;
    let idx: Vec<u8> = c.idx.iter().flat_map(|x| x.to_le_bytes()).collect();
    PyTuple::new(
        py,
        [
            PyBytes::new(py, &c.rows).into_any(),
            c.n.into_pyobject(py).ok()?.into_any(),
            PyByteArray::new(py, &idx).into_any(),
        ],
    )
    .ok()
}

/// `SkinnedLod.decode(p, usage)`: `('absent',)`, `('f', k, float64 bytearray)` or `('u', size, bytearray)`; `None`.
#[pyfunction]
pub fn skin_decode<'py>(
    py: Python<'py>,
    rows: &Bound<'py, PyBytes>,
    n: usize,
    layout: &Bound<'py, PyBytes>,
    usage: u8,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let Some((els, stride)) = elements(layout.as_bytes()) else {
        return Ok(None);
    };
    let Ok(got) = skinmesh::decode(rows.as_bytes(), n, stride, &els, usage) else {
        return Ok(None);
    };
    Ok(Some(match got {
        None => ("absent",).into_pyobject(py)?.into_any(),
        Some(skinmesh::Decoded::Floats(k, v)) => {
            let b: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
            ("f", k, PyByteArray::new(py, &b)).into_pyobject(py)?.into_any()
        }
        Some(skinmesh::Decoded::Bytes(z, v)) => ("u", z, PyByteArray::new(py, &v)).into_pyobject(py)?.into_any(),
    }))
}

type SkinPart = (Vec<u8>, usize, Vec<u8>, Vec<u8>);

/// `SkinnedLod._winding()` over parts `[(rows, n, layout, idx int64 bytes)]`, or `None`.
#[pyfunction]
pub fn skin_winding(py: Python<'_>, parts: Vec<SkinPart>) -> Option<i32> {
    py.detach(|| {
        let mut owned = Vec::with_capacity(parts.len());
        for (rows, n, layout, idx) in &parts {
            let (els, stride) = elements(layout)?;
            owned.push((rows, *n, stride, els, i64s(idx)?));
        }
        let wp: Vec<skinmesh::WindingPart<'_>> = owned
            .iter()
            .map(|(rows, n, stride, els, idx)| skinmesh::WindingPart {
                rows,
                n: *n,
                stride: *stride,
                els,
                idx,
            })
            .collect();
        skinmesh::winding(&wp)
    })
}

/// `SkinnedLod._encode`'s rows: layout, n, then float64 bytes of pos, nrm (n x 3), uv (n x 2), the blend index
/// bytes, tangents and binormals (n x 3); `None` where the reference must run.
#[pyfunction]
#[expect(clippy::too_many_arguments, reason = "the geometry's arrays")]
pub fn skin_encode<'py>(
    py: Python<'py>,
    layout: &Bound<'py, PyBytes>,
    n: usize,
    pos: &Bound<'py, PyBytes>,
    nrm: &Bound<'py, PyBytes>,
    uv: &Bound<'py, PyBytes>,
    bi: &Bound<'py, PyBytes>,
    tan: &Bound<'py, PyBytes>,
    bin: &Bound<'py, PyBytes>,
) -> Option<Bound<'py, PyBytes>> {
    let (els, stride) = elements(layout.as_bytes())?;
    let (p, nm, u, t, bn) = (
        f64s(pos.as_bytes())?,
        f64s(nrm.as_bytes())?,
        f64s(uv.as_bytes())?,
        f64s(tan.as_bytes())?,
        f64s(bin.as_bytes())?,
    );
    let g = skinmesh::Geometry {
        pos: &p,
        nrm: &nm,
        uv: &u,
        bi: bi.as_bytes(),
        tan: &t,
        bin: &bn,
    };
    skinmesh::encode(&els, stride, n, &g).map(|v| PyBytes::new(py, &v))
}

/// `SkinnedLod.to_bytes`' buffers: `(vb, ib, [None or (vertex_offset, first_index, vertices, triangles)])` from
/// the file's buffers and the parts (`None`: unchanged; `(rows, stride, idx int64 bytes)`); or `None`.
#[pyfunction]
pub fn skin_lay_out<'py>(
    py: Python<'py>,
    vb: &Bound<'py, PyBytes>,
    ib: &Bound<'py, PyBytes>,
    parts: Vec<Option<(Vec<u8>, usize, Vec<u8>)>>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let mut idx = Vec::with_capacity(parts.len());
    for p in &parts {
        idx.push(match p {
            Some((_, _, i)) => match i64s(i) {
                Some(v) => Some(v),
                None => return Ok(None),
            },
            None => None,
        });
    }
    let ps: Vec<skinmesh::OutPart<'_>> = parts
        .iter()
        .zip(&idx)
        .map(|(p, i)| skinmesh::OutPart {
            changed: p
                .as_ref()
                .zip(i.as_ref())
                .map(|((rows, stride, _), i)| (rows.as_slice(), *stride, i.as_slice())),
        })
        .collect();
    let Some((v, i, at)) = skinmesh::lay_out(vb.as_bytes(), ib.as_bytes(), &ps) else {
        return Ok(None);
    };
    Ok(Some(PyTuple::new(
        py,
        [
            PyBytes::new(py, &v).into_any(),
            PyBytes::new(py, &i).into_any(),
            at.into_pyobject(py)?.into_any(),
        ],
    )?))
}

/// `SkinnedLod.bounds()` over parts `[(rows, n, layout)]`, or `None`.
#[pyfunction]
pub fn skin_bounds(parts: Vec<(Vec<u8>, usize, Vec<u8>)>) -> Option<[f64; 6]> {
    let mut owned = Vec::with_capacity(parts.len());
    for (rows, n, layout) in &parts {
        let (els, stride) = elements(layout)?;
        owned.push((rows.as_slice(), *n, stride, els));
    }
    let refs: Vec<(&[u8], usize, usize, &[meshconv::Element])> =
        owned.iter().map(|(r, n, s, e)| (*r, *n, *s, e.as_slice())).collect();
    skinmesh::bounds(&refs)
}

// ------------------------------------------------------------------------------------------------ granny

/// `Granny.to_bytes()`'s resource (the CRC recomputed), or `None`.
#[pyfunction]
pub fn granny_with_crc<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyAny>,
    gr2: usize,
    crc_from: usize,
    total: usize,
) -> Option<Bound<'py, PyBytes>> {
    let v = if let Ok(b) = data.cast::<PyByteArray>() {
        b.to_vec()
    } else {
        data.cast::<PyBytes>().ok()?.as_bytes().to_vec()
    };
    granny_edit::with_crc(&v, gr2, crc_from, total).map(|o| PyBytes::new(py, &o))
}

fn at_pair(o: &Bound<'_, PyAny>) -> Option<(u32, u64)> {
    let v = items(o)?;
    let [a, b] = v.as_slice() else { return None };
    Some((a.extract().ok()?, b.extract().ok()?))
}

fn spec(o: &Bound<'_, PyAny>) -> Option<Spec> {
    if o.is_none() {
        return Some(Spec::Identity);
    }
    let v = items(o)?;
    let kind = v.first()?.extract::<String>().ok()?;
    Some(match kind.as_str() {
        "constant" => Spec::Constant(items(v.get(1)?)?.iter().map(f32_of).collect::<Option<_>>()?),
        "copy" => {
            let pair = items(v.get(1)?)?;
            let [t, a] = pair.as_slice() else { return None };
            let t = at_pair(t)?;
            Spec::Copy((t.0, u32::try_from(t.1).ok()?), at_pair(a)?)
        }
        _ => Spec::Other,
    })
}

/// `granny.add_tracks(pc, tracks, donor)`: tracks `[(name latin-1 bytes, orientation, position)]`, each curve
/// `None`, `('constant', values)`, `('copy', (tref, at))` or another word; `None` where the reference must run.
#[pyfunction]
pub fn granny_add_tracks<'py>(
    py: Python<'py>,
    pc: &Bound<'py, PyBytes>,
    tracks: &Bound<'py, PyAny>,
    donor: &Bound<'py, PyBytes>,
) -> Option<Bound<'py, PyBytes>> {
    let mut ts = Vec::new();
    for t in items(tracks)? {
        let v = items(&t)?;
        let [name, o, p] = v.as_slice() else { return None };
        ts.push(Track {
            name: name.cast::<PyBytes>().ok()?.as_bytes().to_vec(),
            orientation: spec(o)?,
            position: spec(p)?,
        });
    }
    let (a, b) = (pc.as_bytes(), donor.as_bytes());
    py.detach(|| granny_edit::add_tracks(a, &ts, b))
        .map(|v| PyBytes::new(py, &v))
}

fn floats_of(d: &Bound<'_, PyDict>, k: &str) -> Option<Option<Vec<f64>>> {
    match d.get_item(k).ok()? {
        None => Some(None),
        Some(v) => Some(Some(
            items(&v)?
                .iter()
                .map(|x| x.extract::<f64>().ok())
                .collect::<Option<_>>()?,
        )),
    }
}

fn curve(d: &Bound<'_, PyAny>) -> Option<Curve> {
    let d = d.cast::<PyDict>().ok()?;
    let kind = match d.get_item("__type__").ok()? {
        None => String::new(),
        Some(t) => t.extract::<String>().ok()?,
    };
    if kind.chars().any(|c| u32::from(c) > 255) {
        return None;
    }
    // integers for the quantized formats, the ones that read them; a float format's (D3I1K32fC32f) are not read
    let ints = match d.get_item("KnotsControls").ok()? {
        None => None,
        Some(v) => items(&v)?
            .iter()
            .map(|x| x.extract::<i64>().ok())
            .collect::<Option<Vec<_>>>(),
    };
    let trunc = match d.get_item("OneOverKnotScaleTrunc").ok()? {
        None => None,
        Some(v) => Some(v.extract::<i64>().ok()?),
    };
    Some(Curve {
        kind: kind.chars().map(|c| c as u8).collect(),
        controls: floats_of(d, "Controls")?,
        offsets: floats_of(d, "ControlOffsets")?,
        scales: floats_of(d, "ControlScales")?,
        knots_controls: ints,
        knots: floats_of(d, "Knots")?,
        trunc,
    })
}

fn puts<'py>(py: Python<'py>, p: &[Put], msg: &str) -> PyResult<Bound<'py, PyTuple>> {
    let out = PyList::empty(py);
    for x in p {
        let vals = PyList::empty(py);
        for &v in &x.values {
            if x.fmt.ends_with('f') {
                vals.append(v)?;
            } else {
                vals.append(v as i64)?;
            }
        }
        out.append((x.name, x.element, x.fmt, vals))?;
    }
    PyTuple::new(py, [out.into_any(), msg.into_pyobject(py)?.into_any()])
}

/// `granny.shift_position`'s puts `[(member, element offset or None, fmt, values)]` and report, or `None`.
#[pyfunction]
pub fn granny_shift_plan<'py>(
    py: Python<'py>,
    cd: &Bound<'py, PyAny>,
    delta: [f64; 3],
    ramp: bool,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let Some(c) = curve(cd) else { return Ok(None) };
    let Some((p, msg)) = granny_edit::shift_plan(&c, delta, ramp) else {
        return Ok(None);
    };
    Ok(Some(puts(py, &p, &msg)?))
}

/// `granny.set_orientations`' puts and report, or `None`.
#[pyfunction]
pub fn granny_orientation_plan<'py>(
    py: Python<'py>,
    cd: &Bound<'py, PyAny>,
    quats: Vec<[f64; 4]>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let Some(c) = curve(cd) else { return Ok(None) };
    let Some((p, msg)) = granny_edit::orientation_plan(&c, &quats) else {
        return Ok(None);
    };
    Ok(Some(puts(py, &p, &msg)?))
}
