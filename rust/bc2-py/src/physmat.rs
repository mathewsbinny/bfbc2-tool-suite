//! Round `OR4` ("OPFOR in Rust", stage 4: the material grid): Project OPFOR's `physmat.py` - a destructible part's
//! material and a model group member's face materials (`bc2_convert::physmat`). Errors raise what the Python raised:
//! `SystemExit` with its message, `AssertionError`, `ValueError` (a partition or resource that does not read).

#![allow(
    clippy::type_complexity,
    clippy::needless_pass_by_value,
    reason = "the Python's shapes; PyO3 extracts a dict argument as an owned map"
)]

use std::collections::{BTreeMap, HashMap};

use bc2_convert::physmat::{self, PhysmatError};
use pyo3::exceptions::{PyAssertionError, PySystemExit, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

fn err(e: PhysmatError) -> PyErr {
    match e {
        PhysmatError::Exit(m) => PySystemExit::new_err(m),
        PhysmatError::Assert(m) => PyAssertionError::new_err(m),
        PhysmatError::Dbx(m) | PhysmatError::Havok(m) => PyValueError::new_err(m),
    }
}

/// `physmat.havok_lists(data)`: (the dbx `MaterialMappings` names, `[(part, material)]`).
#[pyfunction]
pub fn physmat_havok_lists(py: Python<'_>, data: &[u8]) -> PyResult<(Vec<String>, Vec<(String, String)>)> {
    py.detach(|| physmat::havok_lists(data)).map_err(err)
}

/// `physmat.list_hashes(res, data)`: `{lower material name: hash}`.
#[pyfunction]
pub fn physmat_list_hashes(py: Python<'_>, res: &[u8], data: &[u8]) -> PyResult<HashMap<String, u32>> {
    py.detach(|| physmat::list_hashes(res, data)).map_err(err)
}

/// `physmat.set_parts(res, data, want, hashes)`: `(res, data, [(part, old, new)])`.
#[pyfunction]
#[pyo3(signature = (res, data, want, hashes=None))]
pub fn physmat_set_parts<'py>(
    py: Python<'py>,
    res: &[u8],
    data: &[u8],
    want: BTreeMap<String, String>,
    hashes: Option<HashMap<String, u32>>,
) -> PyResult<(Bound<'py, PyBytes>, Bound<'py, PyBytes>, Vec<(String, String, String)>)> {
    let hashes = hashes.unwrap_or_default();
    let (r, d, c) = py
        .detach(|| physmat::set_parts(res, data, &want, &hashes))
        .map_err(err)?;
    Ok((PyBytes::new(py, &r), PyBytes::new(py, &d), c))
}

/// `physmat.group_faces(res, member_types, remap)`: `(res, [(member, subpart, old, new)])`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn physmat_group_faces<'py>(
    py: Python<'py>,
    res: &[u8],
    member_types: Vec<String>,
    remap: HashMap<String, HashMap<String, String>>,
) -> PyResult<(Bound<'py, PyBytes>, Vec<(String, usize, String, String)>)> {
    let (r, c) = py
        .detach(|| physmat::group_faces(res, &member_types, &remap))
        .map_err(err)?;
    Ok((PyBytes::new(py, &r), c))
}
