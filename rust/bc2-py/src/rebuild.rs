//! Round `OR7`: Project OPFOR's WW2 rebuild as a step graph (`bc2_install::rebuild`). `rebuild_all.py` hands its plan
//! over as JSON (the steps, their folders, the declared graph, the mode):
//! - `rebuild_run(plan, say)`: runs it, `say(line)` hearing every line to print; returns the outcome as JSON (`rc`,
//!   the steps that ran, the wall time, the most steps at once, the memory, the side-by-side check);
//! - `rebuild_graph(plan)`: the graph's description, a line a step (`rebuild_all.py --graph`);
//! - `rebuild_derive(plan)`: the declared graph from the records at hand, as JSON (`rebuild_graph.json`).

use bc2_install::rebuild::{self, Plan, Tracker, run};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;

fn plan_of(text: &str) -> PyResult<Plan> {
    Plan::from_json(text).map_err(PyValueError::new_err)
}

fn records(p: &Plan) -> PyResult<Vec<Option<rebuild::Record>>> {
    let tr = Tracker::new(&p.track, &p.work, p.skip.iter().cloned().collect())
        .map_err(|e| PyValueError::new_err(format!("{}: {e}", p.track.display())))?;
    Ok(p.steps.iter().map(|s| tr.record(&s.name)).collect())
}

/// Run the plan; `say(line)` for each line to print. The outcome as JSON.
#[pyfunction]
pub fn rebuild_run(py: Python<'_>, plan: &str, say: &Bound<'_, PyAny>) -> PyResult<String> {
    let p = plan_of(plan)?;
    let say = say.clone().unbind();
    let failed: std::sync::Mutex<Option<PyErr>> = std::sync::Mutex::new(None);
    let got = py.detach(|| {
        let emit = |line: &str| {
            if let Err(e) = Python::attach(|py| say.call1(py, (line,))) {
                let mut f = failed.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                if f.is_none() {
                    *f = Some(e);
                }
            }
        };
        run::run(&p, &emit)
    });
    if let Some(e) = failed.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner) {
        return Err(e);
    }
    let out = got.map_err(PyValueError::new_err)?;
    Ok(out.to_json().to_string())
}

/// The graph's description on the records at hand.
#[pyfunction]
pub fn rebuild_graph(py: Python<'_>, plan: &str) -> PyResult<Vec<String>> {
    let p = plan_of(plan)?;
    let recs = py.detach(|| records(&p))?;
    rebuild::describe(&p, &recs).map_err(PyValueError::new_err)
}

/// The declared graph from the records at hand, as JSON.
#[pyfunction]
pub fn rebuild_derive(py: Python<'_>, plan: &str) -> PyResult<String> {
    let p = plan_of(plan)?;
    let recs = py.detach(|| records(&p))?;
    let v = rebuild::derive(&p, &recs).map_err(PyValueError::new_err)?;
    serde_json::to_string_pretty(&v).map_err(|e| PyValueError::new_err(e.to_string()))
}
