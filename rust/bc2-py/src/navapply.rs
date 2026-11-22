//! Bindings for round NK1's `bc2_navbuild::graphedit`: `navkit apply`'s edit stage (the side file's links and route
//! edges, its traversal annotations) for `navkit.py`'s apply on a NavKit base - the Python (`navlinks.py`,
//! `navtraversal.py`) was retired. The operations come as JSON text (`json.dumps` of the side file's values).
//! Where the reference raised: its exception (`ValueError`, `struct.error`, `IndexError`); an input the port does
//! not take: `None` (the caller raises `bfbc2core.native.Refused`).

use bc2_navbuild::graphedit::{self, EditError};
use bc2_pyrt::json::{self, Value};
use pyo3::exceptions::{PyIndexError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyBytes;

/// The reference's exception for an [`EditError::Raised`]; `None` for a refusal.
fn raised(py: Python<'_>, e: EditError) -> PyResult<()> {
    match e {
        EditError::Refused(_) => Ok(()),
        EditError::Raised { kind, msg } => Err(match kind {
            "IndexError" => PyIndexError::new_err(msg),
            "error" => match py.import("struct").and_then(|m| m.getattr("error")) {
                Ok(cls) => PyErr::from_type(cls.cast_into().unwrap_or_else(|_| py.get_type::<PyValueError>()), msg),
                Err(err) => err,
            },
            _ => PyValueError::new_err(msg),
        }),
    }
}

fn ops_of(text: &str) -> Option<Vec<Value>> {
    match json::loads(text)? {
        Value::List(v) => Some(v),
        _ => None,
    }
}

/// The edited file, the lines printed, the count line.
type Edited<'py> = (Bound<'py, PyBytes>, Vec<String>, String);

/// `navlinks.apply(f, graph)` on the file's bytes, then `hpdtool.write(f)`: (the edited file, the lines it prints,
/// the count line `_apply` prints after writing it), or `None` where the port refuses.
#[pyfunction]
pub fn navkit_graph_edits<'py>(py: Python<'py>, data: &[u8], ops_json: &str) -> PyResult<Option<Edited<'py>>> {
    let Some(ops) = ops_of(ops_json) else {
        return Ok(None);
    };
    match py.detach(|| graphedit::graph_edits(data, &ops)) {
        Ok(g) => Ok(Some((PyBytes::new(py, &g.data), g.lines, g.summary))),
        Err(e) => raised(py, e).map(|()| None),
    }
}

/// `navtraversal.write`'s answers for the path data `data` named `pathdata_name`: (the annotations file's text, the
/// lines it prints), or `None` where the port refuses.
#[pyfunction]
pub fn navkit_traversal(
    py: Python<'_>,
    data: &[u8],
    pathdata_name: &str,
    ops_json: &str,
    level_json: &str,
) -> PyResult<Option<(String, Vec<String>)>> {
    let (Some(ops), Some(level)) = (ops_of(ops_json), json::loads(level_json)) else {
        return Ok(None);
    };
    match py.detach(|| graphedit::traversal(data, pathdata_name, &ops, &level)) {
        Ok(t) => Ok(Some((t.text, t.lines))),
        Err(e) => raised(py, e).map(|()| None),
    }
}
