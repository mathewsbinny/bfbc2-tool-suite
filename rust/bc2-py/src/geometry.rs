//! The bindings of the level's geometry readers: terrain height fields, water, Havok collision triangles and the
//! collision index (`core/bfbc2core/terrain.py`, `water.py`, `collision.py` call them when the module is built).
//!
//! Arrays go to Python as `bytes` / `bytearray` of little-endian values, which the Python side wraps with
//! `numpy.frombuffer` (no NumPy dependency here). Every format error is a `ValueError` with `bc2-formats`' message;
//! a file that cannot be read is an `OSError`.

use std::path::PathBuf;

use bc2_formats::{havok, terrain, water};
use bc2_scene::collision;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyByteArray, PyBytes, PyList, PyTuple};
use rayon::prelude::*;

/// `(min, max, size, heights)` of a height field.
type Heights<'py> = ([f32; 3], [f32; 3], u32, Bound<'py, PyByteArray>);
/// `(indices, slots, ranges)` of a `VisualWater`.
type Visual<'py> = (Bound<'py, PyBytes>, Vec<String>, Vec<(u32, u32, u32, u32)>);
/// `(havok, aliases)` of the collision index.
type Index<'py> = (Vec<(String, String)>, Bound<'py, PyAny>);

fn value_error(e: impl std::fmt::Display) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// Little-endian bytes of `values`.
fn le_bytes<T: Copy, const N: usize>(values: &[T], to_le: fn(T) -> [u8; N]) -> Vec<u8> {
    values.iter().flat_map(|&v| to_le(v)).collect()
}

fn f32_bytes(values: &[f32]) -> Vec<u8> {
    le_bytes(values, f32::to_le_bytes)
}

fn u32_bytes(values: &[u32]) -> Vec<u8> {
    le_bytes(values, u32::to_le_bytes)
}

/// `terrain.parse(data)` through Rust: `(min, max, size, heights)`, the heights a `bytearray` of `f32`s (row by row,
/// bit-identical to the reference's), min and max as `(x, y, z)`.
#[pyfunction]
pub fn terrain_heights<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Heights<'py>> {
    let bytes = data.as_bytes();
    let (min, max, size, heights) = py
        .detach(|| terrain::parse(bytes).map(|hf| (hf.min, hf.max, hf.size, f32_bytes(&hf.heights()))))
        .map_err(value_error)?;
    Ok((min, max, size, PyByteArray::new(py, &heights)))
}

/// The tail of a height field: `('sections', offsets, first, fixed)` (`u32` / `u16` / `u16` bytes),
/// `('quadtree', pairs)` (`u16` bytes), or `('unknown', bytes)`; and the header's words
/// `(log_a, log_b, w24, w26, w28, b2c, first_len)`.
#[pyfunction]
pub fn terrain_tail<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyTuple>> {
    let hf = terrain::parse(data.as_bytes()).map_err(value_error)?;
    let h = hf.header;
    let words = (h.log_a, h.log_b, h.w24, h.w26, h.w28, h.b2c, h.first_len).into_pyobject(py)?;
    let u16b = |v: &[u16]| PyBytes::new(py, &le_bytes(v, u16::to_le_bytes));
    let tail = match &hf.tail {
        terrain::Tail::Sections { offsets, first, fixed } => (
            "sections",
            PyBytes::new(py, &u32_bytes(offsets)),
            u16b(first),
            u16b(fixed),
        )
            .into_pyobject(py)?,
        terrain::Tail::Quadtree(pairs) => {
            let flat: Vec<u16> = pairs.iter().flatten().copied().collect();
            ("quadtree", u16b(&flat)).into_pyobject(py)?
        }
        terrain::Tail::Unknown(bytes) => ("unknown", PyBytes::new(py, bytes)).into_pyobject(py)?,
        _ => ("unknown", PyBytes::new(py, b"")).into_pyobject(py)?,
    };
    PyTuple::new(py, [tail.into_any(), words.into_any()])
}

/// `water.parse_mesh(data)` through Rust: `(vertices, triangles)` as `bytearray`s of `f32` x 3 and `u32` x 3.
#[pyfunction]
pub fn water_mesh<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
) -> PyResult<(Bound<'py, PyByteArray>, Bound<'py, PyByteArray>)> {
    let bytes = data.as_bytes();
    let (v, t) = py
        .detach(|| {
            water::parse_mesh(bytes).map(|m| {
                let v: Vec<f32> = m.vertices.iter().flatten().copied().collect();
                let t: Vec<u32> = m.triangles.iter().flatten().copied().collect();
                (f32_bytes(&v), u32_bytes(&t))
            })
        })
        .map_err(value_error)?;
    Ok((PyByteArray::new(py, &v), PyByteArray::new(py, &t)))
}

/// `water.parse_visual(data)` through Rust: `(indices, slots, ranges)`, the indices `bytes` of `u32`s, each range
/// `(slot, triangles, first index, index count)`.
#[pyfunction]
pub fn water_visual<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Visual<'py>> {
    let v = water::parse_visual(data.as_bytes()).map_err(value_error)?;
    let ranges = v.ranges.iter().map(|&[a, b, c, d]| (a, b, c, d)).collect();
    Ok((PyBytes::new(py, &u32_bytes(&v.indices)), v.slots, ranges))
}

/// The shader of each triangle as `water.read` assigns them: `(shader per triangle, slots)`, or `None` when the
/// `VisualWater` cannot be read or its indices differ from the mesh's (`ValueError` when the mesh cannot be read).
#[pyfunction]
pub fn water_shaders(
    mesh: &Bound<'_, PyBytes>,
    visual: &Bound<'_, PyBytes>,
) -> PyResult<Option<(Vec<String>, Vec<String>)>> {
    let m = water::parse_mesh(mesh.as_bytes()).map_err(value_error)?;
    let Ok(v) = water::parse_visual(visual.as_bytes()) else {
        return Ok(None);
    };
    Ok(water::triangle_shaders(&m, &v).map(|s| (s, v.slots)))
}

/// Collision triangles as `bytes`-like `bytearray` of `f32` x 9, `None`, or the error.
fn triangles_of(data: &[u8]) -> Result<Option<Vec<u8>>, havok::HavokError> {
    Ok(havok::triangles(data)?.map(|t| {
        let flat: Vec<f32> = t.iter().flatten().flatten().copied().collect();
        f32_bytes(&flat)
    }))
}

/// `levelobjects.triangles` on a file's bytes through Rust: a `bytearray` of `f32` x 9 per triangle, or `None`.
#[pyfunction]
pub fn havok_triangles<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyByteArray>>> {
    let bytes = data.as_bytes();
    let t = py.detach(|| triangles_of(bytes)).map_err(value_error)?;
    Ok(t.map(|t| PyByteArray::new(py, &t)))
}

/// `havok_triangles` of many files, read from disk and decoded on all cores. Each result is the `bytearray`,
/// `None`, or the exception (`OSError`, `ValueError`) returned, not raised.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn havok_triangles_files(py: Python<'_>, paths: Vec<PathBuf>) -> PyResult<Bound<'_, PyList>> {
    let results: Vec<Result<Option<Vec<u8>>, PyErrKind>> = py.detach(|| {
        paths
            .par_iter()
            .map(|p| {
                let data = std::fs::read(p).map_err(PyErrKind::Io)?;
                triangles_of(&data).map_err(PyErrKind::Havok)
            })
            .collect()
    });
    let out = PyList::empty(py);
    for r in results {
        match r {
            Ok(Some(t)) => out.append(PyByteArray::new(py, &t))?,
            Ok(None) => out.append(py.None())?,
            Err(PyErrKind::Io(e)) => out.append(PyErr::from(e).into_value(py))?,
            Err(PyErrKind::Havok(e)) => out.append(value_error(e).into_value(py))?,
        }
    }
    Ok(out)
}

/// An error made without the interpreter, turned into a Python exception after.
enum PyErrKind {
    Io(std::io::Error),
    Havok(havok::HavokError),
}

/// NavKit's `havok_index` of `bases` alone (no mesh aliases): `[(blueprint, packfile path)]` in the reference's
/// order.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn havok_index(py: Python<'_>, bases: Vec<PathBuf>) -> Vec<(String, String)> {
    py.detach(|| collision::havok_index(&bases))
        .entries()
        .iter()
        .map(|(k, p)| (k.clone(), p.to_string_lossy().into_owned()))
        .collect()
}

/// NavKit's `havok_index` and `mesh_aliases` of `bases` in one walk: `(havok, aliases)`, each a list of
/// `(blueprint, value)` in the reference's order; `aliases` is the `OSError` that stopped the reference instead,
/// when an XML file could not be read. Each blueprint's `Mesh` comes from its binary `.dbx` where that reads
/// (`bc2_scene::collision`); with `cache` (a folder) each reference is reused while its files' sizes and times are
/// unchanged (`index_cached`).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
#[pyo3(signature = (bases, cache=None))]
pub fn collision_index(py: Python<'_>, bases: Vec<PathBuf>, cache: Option<PathBuf>) -> PyResult<Index<'_>> {
    let index = py.detach(|| collision::index_cached(&bases, cache.as_deref()));
    let havok = index
        .havok
        .entries()
        .iter()
        .map(|(k, p)| (k.clone(), p.to_string_lossy().into_owned()))
        .collect();
    let aliases = match index.aliases {
        Ok(a) => a.entries().to_vec().into_pyobject(py)?.into_any(),
        Err(e) => PyErr::from(e).into_value(py).into_bound(py).into_any(),
    };
    Ok((havok, aliases))
}
