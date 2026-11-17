//! Bindings for `bc2-convert`, the converter pipeline's hot spots: `bfbc2-console-convert/texture.py` calls
//! `texture_convert` when the module has it and runs its own code otherwise, or when the port reports an error (the
//! reference then raises its own exception, so a caller sees the same error either way).

use bc2_convert::texture;
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyList};

/// `texture.convert` through Rust: the PC texture's bytes, or `ValueError` with the reason (`ConvertError`'s
/// message, or that the layout reads past the data).
#[pyfunction]
pub fn texture_convert<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Bound<'py, PyBytes>> {
    let bytes = data.as_bytes();
    let out = py
        .detach(|| texture::convert(bytes))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    Ok(PyBytes::new(py, &out))
}

/// `texture_convert` of many textures on all cores: each result the bytes or the `ValueError`, returned, not raised.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn texture_convert_many<'py>(py: Python<'py>, items: Vec<Bound<'py, PyBytes>>) -> PyResult<Bound<'py, PyList>> {
    let slices: Vec<&[u8]> = items.iter().map(PyBytesMethods::as_bytes).collect();
    let results = py.detach(|| texture::convert_all(&slices));
    let out = PyList::empty(py);
    for r in results {
        match r {
            Ok(b) => out.append(PyBytes::new(py, &b))?,
            Err(e) => out.append(PyValueError::new_err(e.to_string()).into_value(py))?,
        }
    }
    Ok(out)
}
