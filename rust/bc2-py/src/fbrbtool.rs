//! FbrbTool's commands in process (`bc2_formats::fbrbtool`), for `core/bfbc2core/fbrbtool.py`: a command line in, its
//! exit code, output and log lines out, run with the interpreter released; a batch on all cores.

use bc2_tools::fbrbtool::{self as tool, Outcome};
use pyo3::prelude::*;

/// `(exit code, stdout, stderr, [(level, message), ...])`.
type PyOutcome = (i32, String, String, Vec<(String, String)>);

fn tuple(o: Outcome) -> PyOutcome {
    (o.code, o.stdout, o.stderr, o.log)
}

/// One FbrbTool command line (without the program name): `(code, stdout, stderr, log)`. `log_path` is the log named
/// in an unexpected error's message.
#[pyfunction]
#[pyo3(signature = (args, log_path=None))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn fbrbtool_run(py: Python<'_>, args: Vec<String>, log_path: Option<String>) -> PyOutcome {
    tuple(py.detach(|| tool::run(&args, log_path.as_deref())))
}

/// Many command lines on all cores, in order (none may write a file or folder another one reads or writes).
#[pyfunction]
#[pyo3(signature = (runs, log_path=None))]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn fbrbtool_run_many(py: Python<'_>, runs: Vec<Vec<String>>, log_path: Option<String>) -> Vec<PyOutcome> {
    py.detach(|| tool::run_many(&runs, log_path.as_deref()))
        .into_iter()
        .map(tuple)
        .collect()
}

/// FbrbTool's `Gunzip` (.NET's `GZipStream`) of `data`: `(True, bytes)` or `(False, "<exception>: <message>")`.
/// `managed`: the managed inflater's port alone, without zlib first (for the tests).
#[pyfunction]
#[pyo3(signature = (data, managed=false))]
pub fn fbrbtool_gunzip(py: Python<'_>, data: &[u8], managed: bool) -> (bool, Py<PyAny>) {
    use bc2_formats::fbrb::netgzip::{self, NetGzipError};
    let r = py.detach(|| {
        if managed {
            netgzip::gunzip_managed(data)
        } else {
            netgzip::gunzip(data)
        }
    });
    match r {
        Ok(v) => (true, pyo3::types::PyBytes::new(py, &v).into_any().unbind()),
        Err(NetGzipError::InvalidData(m)) => (
            false,
            pyo3::types::PyString::new(py, &format!("InvalidDataException: {m}"))
                .into_any()
                .unbind(),
        ),
        Err(NetGzipError::IndexOutOfRange) => (
            false,
            pyo3::types::PyString::new(
                py,
                "IndexOutOfRangeException: Index was outside the bounds of the array.",
            )
            .into_any()
            .unbind(),
        ),
    }
}
