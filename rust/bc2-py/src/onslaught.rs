//! Round `OR5` ("OPFOR in Rust", stage 5): Project OPFOR's console Onslaught import in the Rust core
//! (`bc2_install::onslaught`). OPFOR's `console_onslaught.py` keeps its command line and the ledger commit
//! (`console_common.install`): `onslaught_build` builds the level in the work folder, `onslaught_install_plan` the
//! bundles and the game copy's writes in their order.
//!
//! Python's shapes: the added files `{rel path: manifest line (str) or size (int)}`; the plan
//! `([(game path, built path (str) or bytes)], [printed line])`. A failure raises `SystemExit` with the message the
//! Python's `run()` gave (`failed: <command line>` and the tool's output), else `OSError` / `ValueError`.

use std::collections::BTreeMap;

use bc2_install::onslaught::{self, Ctx, Entry, OnsError, Source};
use pyo3::exceptions::{PyOSError, PySystemExit, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

fn err(e: OnsError) -> PyErr {
    match e {
        OnsError::Tool(_) => PySystemExit::new_err(e.to_string()),
        OnsError::Io(m) => PyOSError::new_err(m),
        OnsError::Data(m) => PyValueError::new_err(m),
    }
}

fn added_dict<'py>(py: Python<'py>, added: &onslaught::Added) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    for (k, v) in added {
        match v {
            Entry::Line(l) => d.set_item(k, l)?,
            Entry::Size(n) => d.set_item(k, n)?,
        }
    }
    Ok(d)
}

/// `console_onslaught.build(Ctx(root, xbox, ons, pc, work))`: the added files.
#[pyfunction]
pub fn onslaught_build<'py>(
    py: Python<'py>,
    root: &str,
    xbox: &str,
    ons: &str,
    pc: &str,
    work: &str,
) -> PyResult<Bound<'py, PyDict>> {
    let c = Ctx::new(root, xbox, ons, pc, work);
    let added = py.detach(|| onslaught::build(&c)).map_err(err)?;
    added_dict(py, &added)
}

/// `console_onslaught.install(c, added)` up to the ledger commit: (writes in order, printed lines).
#[pyfunction]
pub fn onslaught_install_plan<'py>(
    py: Python<'py>,
    root: &str,
    xbox: &str,
    ons: &str,
    pc: &str,
    work: &str,
    added: &Bound<'py, PyDict>,
) -> PyResult<Bound<'py, PyTuple>> {
    let mut a: onslaught::Added = BTreeMap::new();
    for (k, v) in added.iter() {
        let k: String = k.extract()?;
        let e = match v.extract::<String>() {
            Ok(l) => Entry::Line(l),
            Err(_) => Entry::Size(v.extract::<u64>()?),
        };
        a.insert(k, e);
    }
    let c = Ctx::new(root, xbox, ons, pc, work);
    let plan = py.detach(|| onslaught::install_plan(&c, &a)).map_err(err)?;
    let writes = PyList::empty(py);
    for (dst, src) in &plan.writes {
        match src {
            Source::File(p) => writes.append((dst, p))?,
            Source::Bytes(b) => writes.append((dst, PyBytes::new(py, b)))?,
        }
    }
    (writes, plan.printed).into_pyobject(py)
}
