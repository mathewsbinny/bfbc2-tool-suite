//! Round `OR3b`: `shaderdb.py`'s level side in the Rust core - `retexture` (`shaderdb_retexture`), the level cover
//! index (`shadercover_levels`: `bc2_convert::shaderlevels::levels`) and the level scan (`shaderdb_level_scan`).
//! `shaderdb.py` keeps the arguments' shapes and raises its `ShaderDbError` from the messages returned here; a
//! bundle that cannot be read raises as `fbrb.read` raises.

use std::collections::HashMap;

use bc2_convert::shaderlevels::{self, Cover, ReadFailure, Row, ScanRow};
use bc2_formats::shaderdb::merge::retexture;
use pyo3::exceptions::{PyTypeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PySet, PyTuple};

use crate::shaders::latin1;

fn bytes_of(x: &Bound<'_, PyAny>, what: &str) -> PyResult<Vec<u8>> {
    if let Ok(b) = x.cast::<PyBytes>() {
        return Ok(b.as_bytes().to_vec());
    }
    Err(PyTypeError::new_err(format!(
        "retexture: {what} must be bytes, not {}",
        x.get_type().name()?
    )))
}

/// `retexture(block, textures, where, hits)`: the renamed block (`bytes`) or the `ShaderDbError`'s message
/// (`str`). `textures`: `[(old, new)]` in the dictionary's order; `hits` (a dict or None) is added to only on
/// success, each old name in the order the Python met it.
#[pyfunction]
pub fn shaderdb_retexture<'py>(
    py: Python<'py>,
    block: &[u8],
    textures: &Bound<'py, PyList>,
    place: &str,
    hits: Option<&Bound<'py, PyDict>>,
) -> PyResult<Bound<'py, PyAny>> {
    let mut pairs = Vec::with_capacity(textures.len());
    for t in textures.iter() {
        let t = t.cast::<PyTuple>()?;
        pairs.push((
            bytes_of(&t.get_item(0)?, "a texture name")?,
            bytes_of(&t.get_item(1)?, "a texture name")?,
        ));
    }
    let mut counts: HashMap<Vec<u8>, usize> = HashMap::new();
    match retexture(block, &pairs, place, Some(&mut counts)) {
        Err(e) => Ok(e.to_string().into_pyobject(py)?.into_any()),
        Ok(out) => {
            if let Some(h) = hits {
                for (old, _) in &pairs {
                    if let Some(n) = counts.remove(old) {
                        let k = PyBytes::new(py, old);
                        let was: usize = match h.get_item(&k)? {
                            Some(v) => v.extract()?,
                            None => 0,
                        };
                        h.set_item(k, was + n)?;
                    }
                }
            }
            Ok(PyBytes::new(py, &out).into_any())
        }
    }
}

fn failure(f: ReadFailure) -> PyErr {
    crate::fbrb_error(f.error, &f.path, &f.keys)
}

fn cover_py<'py>(py: Python<'py>, c: &Cover) -> PyResult<Bound<'py, PyAny>> {
    Ok(match c {
        Cover::Error(m) => m.into_pyobject(py)?.into_any(),
        Cover::Prefixes(p) => {
            let d = PyDict::new(py);
            for (k, n) in p {
                d.set_item(latin1(py, k), n)?;
            }
            d.into_any()
        }
    })
}

/// The cover of each `(level, bundle or None)` on Dx `dx` (`level_cover_prefixes`): `[(from the index, prefixes
/// dict or the error's message)]` in order; `folder` the index's (None: off). The first bundle that cannot be read
/// raises (as `load_levels` raises it).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn shadercover_levels(
    py: Python<'_>,
    jobs: Vec<(String, Option<String>)>,
    dx: u32,
    folder: Option<String>,
) -> PyResult<Bound<'_, PyList>> {
    let rows = py.detach(|| shaderlevels::levels(&jobs, dx, folder.as_deref()));
    let out = PyList::empty(py);
    let mut failed = None;
    for r in rows {
        match r {
            Row::Hit(c) => out.append((true, cover_py(py, &c)?))?,
            Row::Loaded(c) => out.append((false, cover_py(py, &c)?))?,
            Row::Failed(f) => {
                failed.get_or_insert(f);
            }
        }
    }
    match failed {
        Some(f) => Err(failure(f)),
        None => Ok(out),
    }
}

/// The level scan of each `(level, bundle or None)` on Dx `dx`: `[(signatures, textures, layouts, kinds, parts) or
/// the error's message]` in order - `{entry: (constant names, samplers)}`, `{entry: [(sampler, cube, name)]}`,
/// `{entry: {layout guid}}`, `{entry: {kind}}` (main-pass programs), `[(part, layout guid)]`. The first bundle that
/// cannot be read raises as `load` raises it; a database the queries cannot answer on raises `ValueError`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn shaderdb_level_scan(
    py: Python<'_>,
    jobs: Vec<(String, Option<String>)>,
    dx: u32,
) -> PyResult<Bound<'_, PyList>> {
    let rows = py.detach(|| shaderlevels::scan_levels(&jobs, dx));
    let out = PyList::empty(py);
    for r in rows {
        match r {
            ScanRow::Failed(f) => return Err(failure(f)),
            ScanRow::Malformed(label) => {
                return Err(PyValueError::new_err(format!(
                    "{label}: an index points outside its section or a record is short"
                )));
            }
            ScanRow::Error(m) => out.append(m)?,
            ScanRow::Scan(s) => {
                let (sig, tex, lay, kinds) = (PyDict::new(py), PyDict::new(py), PyDict::new(py), PyDict::new(py));
                for e in &s.entries {
                    let name = latin1(py, &e.name);
                    let consts = PyTuple::new(py, e.constants.iter().map(|c| latin1(py, c)))?;
                    sig.set_item(&name, (consts, e.samplers))?;
                    let t = PyList::empty(py);
                    for (sm, cube, n) in &e.textures {
                        t.append((sm, cube, latin1(py, n)))?;
                    }
                    tex.set_item(&name, t)?;
                    lay.set_item(&name, PySet::new(py, e.layouts.iter().map(|g| PyBytes::new(py, g)))?)?;
                    kinds.set_item(&name, PySet::new(py, &e.kinds)?)?;
                }
                let parts = PyList::empty(py);
                for (n, g) in &s.parts {
                    parts.append((latin1(py, n), PyBytes::new(py, g)))?;
                }
                out.append((sig, tex, lay, kinds, parts))?;
            }
        }
    }
    Ok(out)
}
