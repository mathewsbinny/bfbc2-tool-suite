//! Round F0: `bfbc2-console-convert/texture.py`'s layout helpers and its PC texture reader in the Rust core:
//! `texture_tiled`, `texture_packed`, `texture_layout` (`bc2_convert::texture`'s, the converter's own) and
//! `texture_levels`, `texture_decode` (`bc2_convert::texcheck`). Each answers `None` where the reference raises or
//! the arguments are outside what the port computes as Python's integers do (a negative or huge value, a zero block
//! edge): the Python then runs.

use bc2_convert::{texcheck, texture};
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyList, PyTuple};

/// Python ints the port's `usize` arithmetic computes exactly (no shift or product past 64 bits).
const LIMIT: u64 = 1 << 31;

fn small(o: &Bound<'_, PyAny>) -> Option<usize> {
    let v = o.extract::<u64>().ok()?;
    (v < LIMIT).then_some(v as usize)
}

/// `texture.tiled(x, y, pitch, log_bpp)` for ints, or `None`.
#[pyfunction]
pub fn texture_tiled(
    x: &Bound<'_, PyAny>,
    y: &Bound<'_, PyAny>,
    pitch: &Bound<'_, PyAny>,
    log_bpp: &Bound<'_, PyAny>,
) -> Option<usize> {
    let lb = small(log_bpp)?;
    if lb > 8 {
        return None;
    }
    Some(texture::tiled(small(x)?, small(y)?, small(pitch)?, lb as u32))
}

/// `texture.packed(w, h, mip, edge)` as `(True, (x, y))`, `(True, None)` for a level with its own storage, or
/// `(False, None)` where the Python must answer.
#[pyfunction]
pub fn texture_packed(
    w: &Bound<'_, PyAny>,
    h: &Bound<'_, PyAny>,
    mip: &Bound<'_, PyAny>,
    edge: &Bound<'_, PyAny>,
) -> (bool, Option<(usize, usize)>) {
    let (Some(w), Some(h), Some(mip), Some(edge)) = (small(w), small(h), small(mip), small(edge)) else {
        return (false, None);
    };
    if edge == 0 || mip > 32 {
        return (false, None);
    }
    (true, texture::packed(w, h, mip, edge))
}

/// `texture.layout(w, h, mips, fmt, slices)`: `(where, size)`, each place `(start, pitch, x, y)`; or `None`.
#[pyfunction]
pub fn texture_layout<'py>(
    py: Python<'py>,
    w: &Bound<'py, PyAny>,
    h: &Bound<'py, PyAny>,
    mips: &Bound<'py, PyAny>,
    fmt: &Bound<'py, PyAny>,
    slices: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (Some(w), Some(h), Some(mips), Some(slices)) = (small(w), small(h), small(mips), small(slices)) else {
        return Ok(None);
    };
    let Ok(fmt) = fmt.extract::<u32>() else {
        return Ok(None);
    };
    if mips > 32 || slices > 4096 {
        return Ok(None);
    }
    let Some((places, size)) = texture::layout(w, h, mips, fmt, slices) else {
        return Ok(None);
    };
    let out = PyList::empty(py);
    for s in &places {
        let row = PyList::empty(py);
        for p in s {
            row.append((p.start, p.pitch, p.x, p.y))?;
        }
        out.append(row)?;
    }
    Ok(Some((out, size).into_pyobject(py)?))
}

/// `texture.levels(pc)`: `[(slice, level, w, h, bytes)]`, or `None` where the reference raises.
#[pyfunction]
pub fn texture_levels<'py>(py: Python<'py>, pc: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyList>>> {
    let data = pc.as_bytes();
    let Some(levels) = texcheck::levels(data) else {
        return Ok(None);
    };
    let out = PyList::empty(py);
    for (s, m, w, h, r) in levels {
        out.append((s, m, w, h, PyBytes::new(py, data.get(r).unwrap_or(&[]))))?;
    }
    Ok(Some(out))
}

/// `texture.decode(level, w, h, fmt)` as `(rows, columns, bytearray of int32 r, g, b)`, or `None` where the
/// reference raises.
#[pyfunction]
pub fn texture_decode<'py>(
    py: Python<'py>,
    level: &Bound<'py, PyBytes>,
    w: &Bound<'py, PyAny>,
    h: &Bound<'py, PyAny>,
    fmt: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (Ok(w), Ok(h), Ok(fmt)) = (w.extract::<i32>(), h.extract::<i32>(), fmt.extract::<u32>()) else {
        return Ok(None);
    };
    let (w, h) = (i64::from(w), i64::from(h));
    let data = level.as_bytes();
    let Some((rows, cols, px)) = py.detach(|| texcheck::decode(data, w, h, fmt)) else {
        return Ok(None);
    };
    let mut bytes = Vec::with_capacity(px.len() * 12);
    for p in &px {
        for c in p {
            bytes.extend_from_slice(&c.to_le_bytes());
        }
    }
    Ok(Some((rows, cols, PyByteArray::new(py, &bytes)).into_pyobject(py)?))
}

/// `havok.classes(exe)` of the executable at `path` as rows `(name, parent, size, members)`, each member `(name,
/// class, type, subtype, carray, flags, offset)`, in the reference's dict order; `None` where the reference raises.
#[pyfunction]
pub fn havok_classes(py: Python<'_>, path: &str) -> Option<Vec<bc2_convert::hkclasses::Klass>> {
    py.detach(|| {
        let data = std::fs::read(path).ok()?;
        bc2_convert::hkclasses::classes(&data)
    })
}
