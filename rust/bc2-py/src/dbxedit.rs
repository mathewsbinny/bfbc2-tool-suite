//! DbxEdit's commands in process (`bc2_formats::dbxedit`), for `core/bfbc2core/dbxedit.py`: a command line in, its
//! exit code, output and log lines out, run with the interpreter released; a batch on all cores.

use bc2_formats::dbxedit::{self, Outcome};
use pyo3::prelude::*;

/// `(exit code, stdout, stderr, [(level, message), ...])`.
type PyOutcome = (i32, String, String, Vec<(String, String)>);

fn tuple(o: Outcome) -> PyOutcome {
    (o.code, o.stdout, o.stderr, o.log)
}

/// One DbxEdit command line (without the program name): `(code, stdout, stderr, log)`. `log_path` is the log named
/// in an unexpected error's message.
#[pyfunction]
#[pyo3(signature = (args, log_path=None))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn dbxedit_run(py: Python<'_>, args: Vec<String>, log_path: Option<String>) -> PyOutcome {
    tuple(py.detach(|| dbxedit::run(&args, log_path.as_deref())))
}

/// Many command lines on all cores, in order (none may write a file another one reads or writes).
#[pyfunction]
#[pyo3(signature = (runs, log_path=None))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn dbxedit_run_many(py: Python<'_>, runs: Vec<Vec<String>>, log_path: Option<String>) -> Vec<PyOutcome> {
    py.detach(|| dbxedit::run_many(&runs, log_path.as_deref()))
        .into_iter()
        .map(tuple)
        .collect()
}
