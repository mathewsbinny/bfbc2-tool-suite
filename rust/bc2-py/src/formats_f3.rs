//! Round F3: the rest of the suite's Python format code, each function answering `None` where the Python reference
//! must run (it raises, a value is a NaN, or a value is not one the port reads exactly as the reference does: a
//! sequence that is not a list or tuple, a `bytes`-like that is not `bytes`, a number of another type), so a result
//! here is always the reference's and every error stays its own.
//!
//! | Python (reference kept) | here | Rust |
//! |---|---|---|
//! | `aimanim.parse` / `write` / `to_pc` | `aimanim_parse`, `aimanim_write`, `aimanim_to_pc` | `bc2_formats::aimanim` |
//! | `hpdtool.mesh_layout` / `mesh_coverage` | `hpd_mesh_layout`, `hpd_mesh_coverage` | `bc2_formats::hpdlayout` |
//! | `audio.xma_parts` / `xma2_riff` / `_xas_stream`'s blocks | `audio_xma_parts`, `audio_xma2_riff`, `audio_xas_blocks` | `bc2_formats::audioframe` |
//! | `navsample.load` (version 3), `navsample_offline.write` | `navkit_navsample_v3`, `navkit_navsample_write` | `bc2_navbuild::navsample` |
//! | `hkpack.Packfile` | `hkpack_parse` | `bc2_formats::havok::Packfile` |
//!
//! (`xenos.disasm`'s listing is `xenos_disasm_body` in `xenos.rs`.)

#![allow(clippy::many_single_char_names)] // the references' names (b, a, m, o, p, q)

use std::collections::HashMap;

use bc2_formats::aimanim::{self, Aim, Block, Quad};
use bc2_formats::{audioframe, havok, hpdlayout};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyDict, PyInt, PyList, PyTuple};

use crate::pathdata::{f32_of, items};

// ------------------------------------------------------------------------------------------------- aimanim

fn quad_py<'py>(py: Python<'py>, q: &Quad) -> PyResult<Bound<'py, PyTuple>> {
    PyTuple::new(py, q.iter().map(|&v| f64::from(v)))
}

/// `aimanim.parse(b, big_endian)`'s dict, or `None`.
#[pyfunction]
pub fn aimanim_parse<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    big_endian: bool,
) -> PyResult<Option<Bound<'py, PyDict>>> {
    let bytes = b.as_bytes();
    let Ok(a) = py.detach(|| aimanim::parse(bytes, big_endian)) else {
        return Ok(None);
    };
    let bones = PyList::empty(py);
    for (bone, kind, block) in &a.bones {
        let blk: Bound<'py, PyAny> = match block {
            Block::Fixed(p, q) => PyTuple::new(py, [quad_py(py, p)?, quad_py(py, q)?])?.into_any(),
            Block::Rotation(p, qs) => {
                let l = PyList::empty(py);
                for q in qs {
                    l.append(quad_py(py, q)?)?;
                }
                PyTuple::new(py, [quad_py(py, p)?.into_any(), l.into_any()])?.into_any()
            }
            Block::Moving(ps) => {
                let l = PyList::empty(py);
                for (p, q) in ps {
                    l.append(PyTuple::new(py, [quad_py(py, p)?, quad_py(py, q)?])?)?;
                }
                l.into_any()
            }
        };
        bones.append((bone, kind, blk))?;
    }
    let d = PyDict::new(py);
    d.set_item("addr", a.addr)?;
    d.set_item("poses", a.poses)?;
    d.set_item("bones", bones)?;
    Ok(Some(d))
}

fn quad_of(o: &Bound<'_, PyAny>) -> Option<Quad> {
    let v = items(o)?;
    let [a, b, c, d] = v.as_slice() else { return None };
    Some([f32_of(a)?, f32_of(b)?, f32_of(c)?, f32_of(d)?])
}

fn pair_of(o: &Bound<'_, PyAny>) -> Option<(Quad, Quad)> {
    let v = items(o)?;
    let [p, q] = v.as_slice() else { return None };
    Some((quad_of(p)?, quad_of(q)?))
}

/// The reference's dict as an [`Aim`]: each bone's block read the way `write`'s branch for its kind reads it.
fn aim_of(a: &Bound<'_, PyAny>) -> Option<Aim> {
    let d = a.cast_exact::<PyDict>().ok()?;
    let bones = items(&d.get_item("bones").ok()??)?;
    let poses: u8 = d.get_item("poses").ok()??.extract().ok()?;
    let addr: u32 = d.get_item("addr").ok()??.extract().ok()?;
    let mut out = Vec::with_capacity(bones.len());
    for t in &bones {
        let t = items(t)?;
        let [bone, kind, block] = t.as_slice() else { return None };
        let (bone, kind): (u8, u8) = (bone.extract().ok()?, kind.extract().ok()?);
        let blk = match kind {
            0 | 1 => {
                let v = items(block)?;
                let (p, rest) = (v.first()?, v.get(1)?);
                if kind == 0 {
                    Block::Fixed(quad_of(p)?, quad_of(rest)?)
                } else {
                    Block::Rotation(quad_of(p)?, items(rest)?.iter().map(quad_of).collect::<Option<_>>()?)
                }
            }
            _ => Block::Moving(items(block)?.iter().map(pair_of).collect::<Option<_>>()?),
        };
        out.push((bone, kind, blk));
    }
    Some(Aim {
        addr,
        poses,
        bones: out,
    })
}

/// `aimanim.write(a)`, or `None`.
#[pyfunction]
pub fn aimanim_write<'py>(py: Python<'py>, a: &Bound<'py, PyAny>) -> Option<Bound<'py, PyBytes>> {
    let aim = aim_of(a)?;
    aimanim::write(&aim).ok().map(|b| PyBytes::new(py, &b))
}

/// `aimanim.to_pc(b, bones)`, or `None`.
#[pyfunction]
#[pyo3(signature = (b, bones=None))]
pub fn aimanim_to_pc<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    bones: Option<&Bound<'py, PyAny>>,
) -> Option<Bound<'py, PyBytes>> {
    let a = aimanim::parse(b.as_bytes(), true).ok()?;
    let map = match bones {
        Some(m) if m.is_truthy().ok()? => {
            let d = m.cast_exact::<PyDict>().ok()?;
            let mut map: HashMap<u8, Option<u8>> = HashMap::new();
            for (bone, _, _) in &a.bones {
                if !map.contains_key(bone) {
                    let new = match d.get_item(*bone).ok()? {
                        Some(v) => Some(v.extract::<u8>().ok()?),
                        None => None,
                    };
                    map.insert(*bone, new);
                }
            }
            Some(map)
        }
        Some(_) | None => None,
    };
    let out = match map {
        Some(m) => aimanim::write(&aimanim::renumbered(&a, |b| m.get(&b).copied().flatten()).ok()?),
        None => aimanim::write(&a),
    };
    out.ok().map(|v| PyBytes::new(py, &v))
}

// ------------------------------------------------------------------------------------------------- hpdtool

/// The reference's exception for a walk that stops: its `ValueError`, or `struct.error` (round RT2).
fn hpd_err(py: Python<'_>, e: &hpdlayout::Error) -> PyErr {
    if e.is_struct() {
        match py.import("struct").and_then(|m| m.getattr("error")) {
            Ok(cls) => PyErr::from_type(
                cls.cast_into().unwrap_or_else(|_| py.get_type::<PyValueError>()),
                e.to_string(),
            ),
            Err(err) => err,
        }
    } else {
        PyValueError::new_err(e.to_string())
    }
}

/// `hpdtool.mesh_layout(m)`: `(regions, tiles, objects, links, tile_offs)`; where the reference raises, its
/// `ValueError` or `struct.error` with its message (round RT2: the reference was retired).
#[pyfunction]
pub fn hpd_mesh_layout<'py>(py: Python<'py>, m: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = m.as_bytes();
    let l = py.detach(|| hpdlayout::layout(bytes)).map_err(|e| hpd_err(py, &e))?;
    let regions = PyList::empty(py);
    for (s, e, label) in &l.regions {
        regions.append((s, e, label))?;
    }
    let tiles = PyList::empty(py);
    for (to, its, end) in &l.tiles {
        let li = PyList::empty(py);
        for (p, a, b) in its {
            li.append((p, hpdlayout::c_double(*a), hpdlayout::c_double(*b)))?;
        }
        tiles.append((to, li, end))?;
    }
    let objects = PyDict::new(py);
    for (p, e) in &l.objects {
        objects.set_item(p, e)?;
    }
    let links = PyList::empty(py);
    for t in &l.links {
        links.append(t)?;
    }
    let offs = PyList::new(py, &l.tile_offs)?;
    Ok(Some((regions, tiles, objects, links, offs).into_pyobject(py)?))
}

/// `hpdtool.mesh_coverage(m)`: `(gaps, overlaps)`; the reference's error where it raises.
#[pyfunction]
pub fn hpd_mesh_coverage<'py>(py: Python<'py>, m: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = m.as_bytes();
    let (gaps, overlaps) = py.detach(|| hpdlayout::coverage(bytes)).map_err(|e| hpd_err(py, &e))?;
    let g = PyList::empty(py);
    for t in &gaps {
        g.append(t)?;
    }
    let o = PyList::empty(py);
    for t in &overlaps {
        o.append(t)?;
    }
    Ok(Some((g, o).into_pyobject(py)?))
}

// ------------------------------------------------------------------------------------------------- audio

/// An exact `int` (or `bool`) as `i64`.
fn int_of(o: &Bound<'_, PyAny>) -> Option<i64> {
    if o.is_instance_of::<PyInt>() {
        o.extract().ok()
    } else {
        None
    }
}

/// The reference's error as `(python type name, message)` (`audio.py` raises it: round RT2), or `None` where the
/// port does not answer ([`audioframe::Error::Refused`]).
fn audio_err<'py>(py: Python<'py>, e: &audioframe::Error) -> Option<Bound<'py, PyAny>> {
    let t = e.python_type()?;
    (t, e.to_string()).into_pyobject(py).ok().map(Bound::into_any)
}

/// `audio.xma_parts(st)` (its `channels` and `blocks`' data): the parts (bytearrays), the reference's error as
/// `(type name, message)`, or `None` (an argument the port does not take).
#[pyfunction]
pub fn audio_xma_parts<'py>(py: Python<'py>, st: &Bound<'py, PyAny>) -> Option<Bound<'py, PyAny>> {
    let channels = int_of(&st.getattr("channels").ok()?)?;
    let blocks = items(&st.getattr("blocks").ok()?)?;
    let data = blocks
        .iter()
        .map(|b| {
            let v = items(b)?;
            let [_, d] = v.as_slice() else { return None };
            Some(d.cast::<PyBytes>().ok()?.clone())
        })
        .collect::<Option<Vec<_>>>()?;
    let views: Vec<&[u8]> = data.iter().map(PyBytesMethods::as_bytes).collect();
    match py.detach(|| audioframe::xma_parts(channels, &views)) {
        Ok(parts) => {
            let l = PyList::empty(py);
            for p in &parts {
                l.append(PyByteArray::new(py, p)).ok()?;
            }
            Some(l.into_any())
        }
        Err(e) => audio_err(py, &e),
    }
}

/// `audio.xma2_riff(packets, channels, rate, samples)`: the bytes, the reference's error as `(type name, message)`,
/// or `None`.
#[pyfunction]
pub fn audio_xma2_riff<'py>(
    py: Python<'py>,
    packets: &Bound<'py, PyBytes>,
    channels: &Bound<'py, PyAny>,
    rate: &Bound<'py, PyAny>,
    samples: &Bound<'py, PyAny>,
) -> Option<Bound<'py, PyAny>> {
    match audioframe::xma2_riff(packets.as_bytes(), int_of(channels)?, int_of(rate)?, int_of(samples)?) {
        Ok(out) => Some(PyBytes::new(py, &out).into_any()),
        Err(e) => audio_err(py, &e),
    }
}

/// `audio._xas_stream`'s blocks `[(samples, bytes)]` of a sound's frames, the reference's error as `(type name,
/// message)` (a rate whose blocks hold no samples: its `ZeroDivisionError`), or `None`.
#[pyfunction]
pub fn audio_xas_blocks<'py>(
    py: Python<'py>,
    frames: &Bound<'py, PyBytes>,
    n: &Bound<'py, PyAny>,
    channels: &Bound<'py, PyAny>,
    rate: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyAny>>> {
    let (Some(n), Some(ch), Some(rate)) = (int_of(n), int_of(channels), int_of(rate)) else {
        return Ok(None);
    };
    // the reference's per = 256 * -(-rate // 1280) is 0 for -1280 < rate <= 0: n // per divides by zero first
    if rate > -1280 && rate <= 0 {
        return Ok(audio_err(py, &audioframe::Error::ZeroDivision));
    }
    let (Ok(n), Ok(ch), Ok(rate)) = (u64::try_from(n), u64::try_from(ch), u32::try_from(rate)) else {
        return Ok(None);
    };
    let blocks = match audioframe::xas_blocks(frames.as_bytes(), n, ch, rate) {
        Ok(b) => b,
        Err(e) => return Ok(audio_err(py, &e)),
    };
    let out = PyList::empty(py);
    for (k, b) in blocks {
        out.append((k, PyBytes::new(py, b)))?;
    }
    Ok(Some(out.into_any()))
}

// ------------------------------------------------------------------------------------------------- navsample

type Arrays<'py> = (
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    Bound<'py, PyByteArray>,
    usize,
);

/// `navsample.load`'s version 3 records: `(start, y, normal, flags, hit, columns)` as little-endian `bytearray`s
/// (the arrays `navkit_navsample_parse` gives), or `None`.
#[pyfunction]
pub fn navkit_navsample_v3<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>, nx: i64, nz: i64) -> Option<Arrays<'py>> {
    use bc2_navbuild::navsample::V3;
    let v = V3::new(data.as_bytes(), nx, nz).ok()?;
    let (cols, n) = (v.counts.len(), v.records.len());
    let fill = |len: usize, f: &dyn Fn(&mut [u8])| {
        PyByteArray::new_with(py, len, |buf| {
            f(buf);
            Ok(())
        })
        .ok()
    };
    Some((
        fill(8 * (cols + 1), &|b| v.fill_start(b))?,
        fill(4 * n, &|b| v.fill_y(b))?,
        fill(12 * n, &|b| v.fill_normal(b))?,
        fill(n, &|b| v.fill_flags(b))?,
        fill(12 * n, &|b| v.fill_hit(b))?,
        cols,
    ))
}

/// `navsample_offline.write`'s file bytes from the header's values (`pitch, min_x, min_z, nx, nz, y_top, y_bottom,
/// radius, cap_bottom, cap_top, ray_radius`: `float()` / `int()` already taken by the caller) and the arrays'
/// little-endian bytes (`start` int64, `y` float32, `normal` float32 n x 3, `flags` uint8, `hit` uint32 n x 3 or
/// `None` for zeros), or `None`.
#[pyfunction]
#[pyo3(signature = (header, start, y, normal, flags, hit=None))]
pub fn navkit_navsample_write<'py>(
    py: Python<'py>,
    header: (f64, f64, f64, i64, i64, f64, f64, f64, f64, f64, f64),
    start: &Bound<'py, PyBytes>,
    y: &Bound<'py, PyBytes>,
    normal: &Bound<'py, PyBytes>,
    flags: &Bound<'py, PyBytes>,
    hit: Option<&Bound<'py, PyBytes>>,
) -> Option<Bound<'py, PyBytes>> {
    let (pitch, min_x, min_z, nx, nz, y_top, y_bottom, radius, cap_bottom, cap_top, ray_radius) = header;
    let h = bc2_navbuild::navsample::Header {
        pitch,
        min_x,
        min_z,
        nx,
        nz,
        y_top,
        y_bottom,
        radius,
        cap_bottom,
        cap_top,
        ray_radius,
    };
    let cols = bc2_navbuild::navsample::Columns {
        start: start.as_bytes(),
        y: y.as_bytes(),
        normal: normal.as_bytes(),
        flags: flags.as_bytes(),
        hit: hit.map(PyBytesMethods::as_bytes),
    };
    let out = py.detach(|| bc2_navbuild::navsample::write_v3(&h, &cols)).ok()?;
    Some(PyBytes::new(py, &out))
}

// ------------------------------------------------------------------------------------------------- hkpack

/// `hkpack.Packfile(data)`'s fields: `(base, header words (8), contents_version, sections, fixups, objects,
/// classes)`; sections `(name, start, local, global, virtual, exports, imports, end)`, fixups `(slot, target)` in the
/// order applied, objects `(offset, class name)`, classes `(offset, name, parent, size, members)` with members
/// `(name, class, type, subtype, carray, offset)`. `None` where `hkpack` raises.
#[pyfunction]
pub fn hkpack_parse<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = data.as_bytes();
    let Ok(pf) = havok::Packfile::parse(bytes) else {
        return Ok(None);
    };
    let base = pf.base as usize;
    let mut header = [0u32; 8];
    for (k, w) in header.iter_mut().enumerate() {
        let Some(b) = bytes.get(base + 4 * k..base + 4 * k + 4) else {
            return Ok(None);
        };
        *w = u32::from_le_bytes(b.try_into().unwrap_or_default());
    }
    let sections = PyList::empty(py);
    for s in &pf.sections {
        sections.append((
            &s.name,
            s.start,
            s.local,
            s.global,
            s.virtual_fixups,
            s.exports,
            s.imports,
            s.end,
        ))?;
    }
    let fixups = pf.pointer_fixups();
    let slots = PyList::new(py, fixups.iter().map(|f| f.0))?;
    let targets = PyList::new(py, fixups.iter().map(|f| f.1))?;
    let objects = PyList::empty(py);
    for (o, c) in pf.objects() {
        objects.append((o, c))?;
    }
    let classes = PyList::empty(py);
    for (off, c) in pf.classes() {
        let members = PyList::empty(py);
        for m in &c.members {
            members.append((&m.name, m.class, m.kind, m.subtype, m.carray, m.offset))?;
        }
        classes.append((off, &c.name, c.parent, c.size, members))?;
    }
    Ok(Some(
        (
            pf.base,
            PyTuple::new(py, header)?,
            &pf.contents_version,
            sections,
            (slots, targets),
            objects,
            classes,
        )
            .into_pyobject(py)?,
    ))
}
