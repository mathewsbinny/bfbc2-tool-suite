//! Round SD1: how `shaderdb.py` finds and opens a database, through `bc2_formats::shaderdb::open` (the same code as
//! `bfbc2.exe shaderdb`): `ShaderDb.parse`'s dispatch (`shaderdb_open`), `pick_database`'s choice (`shaderdb_pick`)
//! and `dx_from_name`'s search (`shaderdb_dx_in_name`).
//!
//! Each answers `None` where the Python reference does something else than the plain case (an argument of another
//! type, a text Python's own rules decide), and `shaderdb.py` then runs its Python.

use bc2_formats::shaderdb::open;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyBytes, PyInt, PyString};

use crate::shaderdb_handle::ShaderDbHandle;

/// A `str` as Rust holds it (None: not exactly a `str`, or one with a lone surrogate).
fn text(v: &Bound<'_, PyAny>) -> Option<String> {
    if !v.is_exact_instance_of::<PyString>() {
        return None;
    }
    v.cast::<PyString>().ok()?.to_str().ok().map(str::to_owned)
}

/// A Python `int` (not `bool`) that fits a `u32`.
fn small_int(v: &Bound<'_, PyAny>) -> Option<u32> {
    if !v.is_exact_instance_of::<PyInt>() {
        return None;
    }
    v.extract().ok()
}

/// `ShaderDb.parse(data, dx, where, big_endian)` in one call: `(handle, dx, big_endian)` - the database kept in a
/// `ShaderDbHandle`, the `dx` it keeps, its byte order - or a `ValueError` with `shaderdb.py`'s message (the
/// `ShaderDbError` to raise). `None`: arguments only the Python takes (`dx` not `None` or an `int` within `u32`,
/// `big_endian` not `None` or a `bool`, `where` not a plain `str`).
#[pyfunction]
pub fn shaderdb_open<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    dx: &Bound<'py, PyAny>,
    big_endian: &Bound<'py, PyAny>,
    place: &Bound<'py, PyAny>,
) -> PyResult<Option<(ShaderDbHandle, u32, bool)>> {
    let dx = if dx.is_none() {
        None
    } else {
        match small_int(dx) {
            Some(d) => Some(d),
            None => return Ok(None),
        }
    };
    let big = if big_endian.is_none() {
        None
    } else if big_endian.is_exact_instance_of::<PyBool>() {
        Some(big_endian.is_truthy()?)
    } else {
        return Ok(None);
    };
    let Some(place) = text(place) else {
        return Ok(None);
    };
    let bytes = data.as_bytes();
    let got = py.detach(|| open::open(bytes, dx, big, &place).map(|o| (o.dx, o.big_endian, o.db)));
    match got {
        Ok((d, be, db)) => Ok(Some((ShaderDbHandle::of_file(data, &db), d, be))),
        Err(msg) => Err(PyValueError::new_err(msg)),
    }
}

/// `pick_database(path, dx, entry)` once `bundle_databases(path)` is read (`dbs`, `[(name, type)]`; `folder` the
/// bundle's folder name): `(True, entry name)` or `(False, the ShaderDbError's message)`. `None`: an argument only the
/// Python takes, or a name past latin-1 the choice had to lower-case.
#[pyfunction]
pub fn shaderdb_pick(
    path: &Bound<'_, PyAny>,
    dbs: &Bound<'_, PyAny>,
    dx: &Bound<'_, PyAny>,
    entry: &Bound<'_, PyAny>,
    folder: &Bound<'_, PyAny>,
) -> PyResult<Option<(bool, String)>> {
    let (Some(path), Some(dx), Some(folder)) = (text(path), small_int(dx), text(folder)) else {
        return Ok(None);
    };
    let entry = if entry.is_none() {
        None
    } else {
        match text(entry) {
            Some(e) => Some(e),
            None => return Ok(None),
        }
    };
    let mut list = Vec::new();
    for item in dbs.try_iter()? {
        let item = item?;
        let Ok((n, t)) = item.extract::<(Bound<'_, PyAny>, Bound<'_, PyAny>)>() else {
            return Ok(None);
        };
        let (Some(n), Some(t)) = (text(&n), text(&t)) else {
            return Ok(None);
        };
        list.push((n, t));
    }
    Ok(
        open::pick_database(&path, &list, dx, entry.as_deref(), &folder).map(|r| match r {
            Ok(name) => (true, name),
            Err(msg) => (false, msg),
        }),
    )
}

/// `dx_from_name`'s search of a base name: 9, 10 or 11, 0 for none; `None` for a name only the Python takes (not a
/// plain `str`).
#[pyfunction]
pub fn shaderdb_dx_in_name(name: &Bound<'_, PyAny>) -> Option<u32> {
    text(name).map(|n| open::dx_in_name(&n).unwrap_or(0))
}
