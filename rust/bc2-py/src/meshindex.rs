//! Round O3: `meshview.Index._build`'s tables in the Rust core (`bc2_gamefs::meshindex`), for the Python shim
//! (`bfbc2-mesh-viewer\meshview.py`). The objects the reference shares are shared here too (each bundle path is the
//! caller's own `str`, a layout record one `bytes` for every part using it, the one empty `bytes`), so the cache the
//! caller pickles is the reference's byte for byte.

use bc2_gamefs::meshindex;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyTuple};

/// (where, parts, entries, the databases read).
type Tables<'py> = (Bound<'py, PyDict>, Bound<'py, PyDict>, Bound<'py, PyDict>, usize);

/// `meshview_index(bundles)`: `(where, parts, entries, databases read)` of the bundles (the caller's `str` objects, in
/// `meshview._bundles`' order), or `None` where the reference would leave a bundle or database out of its index or
/// raise (the Python builds the index then, with its own messages).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn meshview_index<'py>(py: Python<'py>, bundles: Vec<Bound<'py, PyString>>) -> PyResult<Option<Tables<'py>>> {
    let paths: Vec<String> = bundles.iter().map(|b| b.to_string_lossy().into_owned()).collect();
    let Ok(ix) = py.detach(|| meshindex::build(&paths)) else {
        return Ok(None);
    };
    let where_ = PyDict::new(py);
    for (key, bi, name) in &ix.where_ {
        let Some(b) = bundles.get(*bi) else { return Ok(None) };
        where_.set_item(
            key,
            PyTuple::new(py, [b.clone().into_any(), PyString::new(py, name).into_any()])?,
        )?;
    }
    let empty = PyBytes::new(py, b"");
    let layouts: Vec<Bound<'py, PyBytes>> = ix
        .layouts
        .iter()
        .map(|l| {
            if l.is_empty() {
                empty.clone()
            } else {
                PyBytes::new(py, l)
            }
        })
        .collect();
    let parts = PyDict::new(py);
    for (key, id) in &ix.parts {
        let Some(l) = layouts.get(*id) else { return Ok(None) };
        parts.set_item(key, l)?;
    }
    let entries = PyDict::new(py);
    for (key, tex) in &ix.entries {
        entries.set_item(key, PyList::new(py, tex)?)?;
    }
    Ok(Some((where_, parts, entries, ix.databases)))
}
