//! Bindings for two small core modules (round F2b): `core/bfbc2core/xmlinst.py` (`xmlinst_instances`,
//! `xmlinst_field_ref`, on `bc2_formats::xmlinst`) and `core/bfbc2core/navtune.py` (`navtune_validate`,
//! `navtune_dumps`, on `bc2_formats::navtune`). Each returns `None` where the port refuses (the caller then runs the
//! Python reference).

use bc2_formats::xmlinst;
use bc2_navbuild::navtune::{self, Value};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PyString, PyTuple};

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

/// A Python value as the side file's JSON holds it, or `None` for one the port does not take.
fn value_of(v: &Bound<'_, PyAny>) -> PyResult<Option<Value>> {
    if v.is_none() {
        return Ok(Some(Value::Null));
    }
    if v.is_exact_instance_of::<PyBool>() {
        return Ok(Some(Value::Bool(v.extract()?)));
    }
    if v.is_exact_instance_of::<PyInt>() {
        return Ok(v.extract::<i64>().ok().map(Value::Int));
    }
    if v.is_exact_instance_of::<PyFloat>() {
        return Ok(Some(Value::Float(v.extract()?)));
    }
    if v.is_exact_instance_of::<PyString>() {
        return Ok(v.extract::<String>().ok().map(Value::Str));
    }
    if v.is_exact_instance_of::<PyList>() || v.is_exact_instance_of::<PyTuple>() {
        let mut out = Vec::new();
        for x in v.try_iter()? {
            let Some(x) = value_of(&x?)? else { return Ok(None) };
            out.push(x);
        }
        return Ok(Some(Value::List(out)));
    }
    if let Ok(d) = v.cast_exact::<PyDict>() {
        let mut out = Vec::with_capacity(d.len());
        for (k, x) in d.iter() {
            if !k.is_exact_instance_of::<PyString>() {
                return Ok(None);
            }
            let Ok(k) = k.extract::<String>() else { return Ok(None) };
            let Some(x) = value_of(&x)? else { return Ok(None) };
            out.push((k, x));
        }
        return Ok(Some(Value::Dict(out)));
    }
    Ok(None)
}

/// `navtune.validate(o, kind)`: the problems; `None`: refused.
#[pyfunction]
pub fn navtune_validate(o: &Bound<'_, PyAny>, kind: &str) -> PyResult<Option<Vec<String>>> {
    Ok(value_of(o)?.and_then(|v| navtune::validate(&v, kind)))
}

/// A side file's text as `NavTune.save` writes it (`to_json()` dumped, a newline after); `None`: refused.
#[pyfunction]
pub fn navtune_dumps(obj: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    Ok(value_of(obj)?.map(|v| navtune::dumps(&v)))
}

/// `NavTune.summary`'s line of one operation; `None`: refused.
#[pyfunction]
pub fn navtune_summary(o: &Bound<'_, PyAny>) -> PyResult<Option<String>> {
    Ok(value_of(o)?.and_then(|v| navtune::summary(&v)))
}
