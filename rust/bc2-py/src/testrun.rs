//! Round `OR5`: Project OPFOR's test run logic in the Rust core (`bc2_harness::testrun`): `opfor.ini`'s edits
//! (`testrun_ini_set` / `_get` / `_unset` / `_restore`, `testrun.py`'s `ini_*`) and the log's notes while the game runs
//! (`testrun_watch`: the lines read since the last poll, the session state and milestones said so far ->
//! `(notes, state, seen)`, each note `('state', n)`, `('event', label)`, `('levelfix', text)`, `('fault', text)` or
//! `('crash', None)`).

use bc2_harness::testrun::{self, Note, Watch};
use pyo3::prelude::*;
use pyo3::types::{PyList, PyTuple};

/// `ini_set(text, section, key, value)`.
#[pyfunction]
pub fn testrun_ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    testrun::ini_set(text, section, key, value)
}

/// `ini_get(text, section, key)`.
#[pyfunction]
pub fn testrun_ini_get(text: &str, section: &str, key: &str) -> Option<String> {
    testrun::ini_get(text, section, key)
}

/// `ini_unset(text, section, key, drop_empty_section=False)`.
#[pyfunction]
#[pyo3(signature = (text, section, key, drop_empty_section = false))]
pub fn testrun_ini_unset(text: &str, section: &str, key: &str, drop_empty_section: bool) -> String {
    testrun::ini_unset(text, section, key, drop_empty_section)
}

/// `ini_restore(text, original, changed)`.
#[pyfunction]
#[allow(clippy::needless_pass_by_value, reason = "pyo3 extracts the list as an owned Vec")]
pub fn testrun_ini_restore(text: &str, original: &str, changed: Vec<(String, String)>) -> String {
    testrun::ini_restore(text, original, &changed)
}

/// The notes of `lines`: (notes, state, seen).
#[pyfunction]
#[allow(clippy::needless_pass_by_value, reason = "pyo3 extracts the lists as owned Vecs")]
pub fn testrun_watch(
    py: Python<'_>,
    lines: Vec<String>,
    state: Option<i64>,
    seen: Vec<String>,
) -> PyResult<Bound<'_, PyTuple>> {
    let mut w = Watch {
        state,
        seen: testrun::EVENTS
            .iter()
            .map(|e| e.1)
            .filter(|l| seen.iter().any(|s| s == l))
            .collect(),
    };
    let notes = w.lines(lines.iter().map(String::as_str));
    let out = PyList::empty(py);
    for n in notes {
        match n {
            Note::State(s) => out.append(("state", s))?,
            Note::Event(l) => out.append(("event", l))?,
            Note::LevelFix(t) => out.append(("levelfix", t))?,
            Note::Fault(t) => out.append(("fault", t))?,
            Note::Crash => out.append(("crash", py.None()))?,
        }
    }
    let seen: Vec<&str> = w.seen.clone();
    (out, w.state, seen).into_pyobject(py)
}
