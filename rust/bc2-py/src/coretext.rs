//! Bindings for two small core modules (round F2b): `core/bfbc2core/xmlinst.py` (`xmlinst_instances`,
//! `xmlinst_field_ref`, on `bc2_formats::xmlinst`) and `core/bfbc2core/navtune.py` (`navtune_validate`,
//! `navtune_dumps`, on `bc2_formats::navtune`). Each returns `None` where the port refuses (the caller then runs the
//! Python reference).

use bc2_formats::xmlinst;
use pyo3::prelude::*;
use pyo3::types::PyTuple;

/// `xmlinst.instances(text, type_suffix)` as a list of `(type, guid, body)`; `None`: refused.
#[pyfunction]
#[pyo3(signature = (text, type_suffix=None))]
pub fn xmlinst_instances(
    py: Python<'_>,
    text: &str,
    type_suffix: Option<&str>,
) -> Option<Vec<(String, String, String)>> {
    let found = py.detach(|| xmlinst::instances(text, type_suffix))?;
    Some(
        found
            .into_iter()
            .map(|(t, g, b)| (t.to_owned(), g.to_owned(), b.to_owned()))
            .collect(),
    )
}

/// `xmlinst.field_ref(body, name)`: `(ref or None,)`; `None`: refused.
#[pyfunction]
pub fn xmlinst_field_ref<'py>(py: Python<'py>, body: &str, name: &str) -> PyResult<Option<Bound<'py, PyTuple>>> {
    match xmlinst::field_ref(body, name) {
        Some(r) => Ok(Some(PyTuple::new(py, [r])?)),
        None => Ok(None),
    }
}
