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
//! | `soot.soot_mesh` | `soot_mesh` | `bc2_convert::soot` |
//! | `havok.convert_wrapper` | `havok_convert_wrapper` | `bc2_convert::havokwrap` |
//! | `navsample.load` (version 3), `navsample_offline.write` | `navkit_navsample_v3`, `navkit_navsample_write` | `bc2_navbuild::navsample` |
//! | `hkpack.Packfile` | `hkpack_parse` | `bc2_formats::havok::Packfile` |
//!
//! (`xenos.disasm`'s listing is `xenos_disasm_body` in `xenos.rs`.)

#![allow(clippy::many_single_char_names)] // the references' names (b, a, m, o, p, q)

use std::collections::HashMap;

use bc2_formats::aimanim::{self, Aim, Block, Quad};
use bc2_formats::{audioframe, havok, hpdlayout};
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

/// `hpdtool.mesh_layout(m)`: `(regions, tiles, objects, links, tile_offs)`, or `None`.
#[pyfunction]
pub fn hpd_mesh_layout<'py>(py: Python<'py>, m: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = m.as_bytes();
    let Ok(l) = py.detach(|| hpdlayout::layout(bytes)) else {
        return Ok(None);
    };
    let regions = PyList::empty(py);
    for (s, e, label) in &l.regions {
        regions.append((s, e, label))?;
    }
    let tiles = PyList::empty(py);
    for (to, its, end) in &l.tiles {
        let li = PyList::empty(py);
        for (p, a, b) in its {
            li.append((p, f64::from(*a), f64::from(*b)))?;
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

/// `hpdtool.mesh_coverage(m)`: `(gaps, overlaps)`, or `None`.
#[pyfunction]
pub fn hpd_mesh_coverage<'py>(py: Python<'py>, m: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = m.as_bytes();
    let Ok((gaps, overlaps)) = py.detach(|| hpdlayout::coverage(bytes)) else {
        return Ok(None);
    };
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

/// `audio.xma_parts(st)` (its `channels` and `blocks`' data), or `None`.
#[pyfunction]
pub fn audio_xma_parts<'py>(py: Python<'py>, st: &Bound<'py, PyAny>) -> Option<Vec<Bound<'py, PyByteArray>>> {
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
    let parts = py.detach(|| audioframe::xma_parts(channels, &views)).ok()?;
    Some(parts.iter().map(|p| PyByteArray::new(py, p)).collect())
}

/// `audio.xma2_riff(packets, channels, rate, samples)`, or `None`.
#[pyfunction]
pub fn audio_xma2_riff<'py>(
    py: Python<'py>,
    packets: &Bound<'py, PyBytes>,
    channels: &Bound<'py, PyAny>,
    rate: &Bound<'py, PyAny>,
    samples: &Bound<'py, PyAny>,
) -> Option<Bound<'py, PyBytes>> {
    let out = audioframe::xma2_riff(packets.as_bytes(), int_of(channels)?, int_of(rate)?, int_of(samples)?).ok()?;
    Some(PyBytes::new(py, &out))
}

/// `audio._xas_stream`'s blocks `[(samples, bytes)]` of a sound's frames, or `None`.
#[pyfunction]
pub fn audio_xas_blocks<'py>(
    py: Python<'py>,
    frames: &Bound<'py, PyBytes>,
    n: &Bound<'py, PyAny>,
    channels: &Bound<'py, PyAny>,
    rate: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyList>>> {
    let (Some(n), Some(ch), Some(rate)) = (int_of(n), int_of(channels), int_of(rate)) else {
        return Ok(None);
    };
    let (Ok(n), Ok(ch), Ok(rate)) = (u64::try_from(n), u64::try_from(ch), u32::try_from(rate)) else {
        return Ok(None);
    };
    let Ok(blocks) = audioframe::xas_blocks(frames.as_bytes(), n, ch, rate) else {
        return Ok(None);
    };
    let out = PyList::empty(py);
    for (k, b) in blocks {
        out.append((k, PyBytes::new(py, b)))?;
    }
    Ok(Some(out))
}

// ------------------------------------------------------------------------------------------------- console

/// `soot.soot_mesh(x)`, or `None`.
#[pyfunction]
pub fn soot_mesh<'py>(py: Python<'py>, x: &Bound<'py, PyBytes>) -> Option<Bound<'py, PyBytes>> {
    let bytes = x.as_bytes();
    let out = py.detach(|| bc2_convert::soot::soot_mesh(bytes)).ok()?;
    Some(PyBytes::new(py, &out))
}

/// `havok.convert_wrapper(b, o)`, or `None`.
#[pyfunction]
pub fn havok_convert_wrapper<'py>(
    py: Python<'py>,
    b: &Bound<'py, PyBytes>,
    o: &Bound<'py, PyAny>,
) -> Option<Bound<'py, PyBytes>> {
    let o = usize::try_from(int_of(o)?).ok()?;
    let bytes = b.as_bytes();
    let out = py.detach(|| bc2_convert::havokwrap::convert_wrapper(bytes, o)).ok()?;
    Some(PyBytes::new(py, &out))
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
    let bytes = data.as_bytes();
    let r = py.detach(|| bc2_navbuild::navsample::parse_v3(bytes, nx, nz)).ok()?;
    let le = |v: Vec<u8>| PyByteArray::new(py, &v);
    Some((
        le(r.start.iter().flat_map(|v| v.to_le_bytes()).collect()),
        le(r.y.iter().flat_map(|v| v.to_le_bytes()).collect()),
        le(r.normal.iter().flat_map(|v| v.to_le_bytes()).collect()),
        PyByteArray::new(py, &r.flags),
        le(r.hit.iter().flat_map(|v| v.to_le_bytes()).collect()),
        r.read,
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
    let words = |b: &[u8]| {
        b.as_chunks::<4>()
            .0
            .iter()
            .map(|c| u32::from_le_bytes(*c))
            .collect::<Vec<_>>()
    };
    let (s, yb, nb) = (start.as_bytes(), y.as_bytes(), normal.as_bytes());
    if s.len() % 8 != 0 || yb.len() % 4 != 0 || nb.len() % 4 != 0 || hit.is_some_and(|h| h.as_bytes().len() % 4 != 0) {
        return None;
    }
    let start: Vec<i64> = s.as_chunks::<8>().0.iter().map(|c| i64::from_le_bytes(*c)).collect();
    let yv: Vec<f32> = words(yb).into_iter().map(f32::from_bits).collect();
    let nv: Vec<f32> = words(nb).into_iter().map(f32::from_bits).collect();
    let hv = hit.map(|h| words(h.as_bytes()));
    let fl = flags.as_bytes();
    let out = py
        .detach(|| bc2_navbuild::navsample::write_v3(&h, &start, &yv, &nv, fl, hv.as_deref()))
        .ok()?;
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
