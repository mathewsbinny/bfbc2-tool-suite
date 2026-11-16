//! Python bindings for the suite's Rust core: the module `bc2_native`, built by `rust/build_native.py` into
//! `core/` (README.md, "The Rust core").
//!
//! The bindings only convert: every format lives in `bc2-formats`. `core/bfbc2core/dbx.py` calls this module when
//! it is importable and falls back to its own Python reader otherwise (`BFBC2_PURE_PYTHON=1` forces the
//! fallback). The work runs with the interpreter released, the batch functions on all cores.
//!
//! Errors are `ValueError`s with `bc2-formats`' message, as the Python reader raises them. The binding is tested
//! from Python (`core/tests/test_dbx_native.py`), where an interpreter exists.

use bc2_formats::dbx::{self, Body, DbxError, DbxFile, Element};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyBytesMethods, PyList, PyString, PyTuple};

/// `bc2-formats`' error as Python's: a `ValueError` with the same message.
fn value_error(e: &DbxError) -> PyErr {
    PyValueError::new_err(e.to_string())
}

/// The bytes of each item of `items` (a sequence of `bytes`).
fn byte_slices<'a>(items: &'a [Bound<'_, PyBytes>]) -> Vec<&'a [u8]> {
    items.iter().map(PyBytesMethods::as_bytes).collect()
}

/// Builds the Python tree of a parsed file through the caller's `Element` and `DbxFile` classes, so the result is
/// exactly what `dbx.parse` returns (dataclass instances, mutable, deep-copyable).
struct Builder<'py> {
    py: Python<'py>,
    element: Bound<'py, PyAny>,
    strings: Vec<Bound<'py, PyString>>,
    empty_str: Bound<'py, PyString>,
    empty_bytes: Bound<'py, PyBytes>,
}

impl<'py> Builder<'py> {
    fn new(py: Python<'py>, element: &Bound<'py, PyAny>, file: &DbxFile) -> Self {
        Self {
            py,
            element: element.clone(),
            strings: file.strings.iter().map(|s| PyString::new(py, s)).collect(),
            empty_str: PyString::new(py, ""),
            empty_bytes: PyBytes::new(py, b""),
        }
    }

    fn s(&self, id: dbx::StrId) -> Bound<'py, PyString> {
        self.strings.get(id as usize).unwrap_or(&self.empty_str).clone()
    }

    fn element(&self, e: &Element) -> PyResult<Bound<'py, PyAny>> {
        let attributes = PyList::empty(self.py);
        for (k, v) in &e.attributes {
            attributes.append(PyTuple::new(self.py, [self.s(*k), self.s(*v)])?)?;
        }
        let none = || self.py.None().into_bound(self.py);
        let (text, raw, children) = match &e.body {
            Body::Container(kids) => {
                let list = PyList::empty(self.py);
                for k in kids {
                    list.append(self.element(k)?)?;
                }
                (self.empty_str.clone(), self.empty_bytes.clone(), list.into_any())
            }
            Body::Text(t) => (self.s(*t), self.empty_bytes.clone(), none()),
            Body::Numbers(r) => (self.empty_str.clone(), PyBytes::new(self.py, r), none()),
            Body::Scalar(r) => (self.empty_str.clone(), PyBytes::new(self.py, r), none()),
        };
        self.element
            .call1((self.s(e.tag), e.kind, attributes, text, raw, children))
    }

    fn file(&self, file_cls: &Bound<'py, PyAny>, f: &DbxFile) -> PyResult<Bound<'py, PyAny>> {
        let strings = PyList::new(self.py, &self.strings)?;
        let roots = PyList::empty(self.py);
        for r in &f.roots {
            roots.append(self.element(r)?)?;
        }
        file_cls.call1((strings, roots))
    }
}

/// `dbx.parse(data)` through Rust: `element_cls(tag, kind, attributes, text, raw, children)` per element and
/// `file_cls(strings, roots)` for the file.
#[pyfunction]
fn dbx_parse<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    element_cls: &Bound<'py, PyAny>,
    file_cls: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyAny>> {
    let bytes = data.as_bytes();
    let f = py.detach(|| dbx::parse(bytes)).map_err(|e| value_error(&e))?;
    Builder::new(py, element_cls, &f).file(file_cls, &f)
}

/// `dbx.to_xml(data)` through Rust.
#[pyfunction]
fn dbx_to_xml(py: Python<'_>, data: &Bound<'_, PyBytes>) -> PyResult<String> {
    let bytes = data.as_bytes();
    py.detach(|| dbx::to_xml(bytes)).map_err(|e| value_error(&e))
}

/// `dbx_parse` of many files: parsed on all cores, then built as Python trees in order. Each item is a file or the
/// `ValueError` it raised (returned, not raised).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
fn dbx_parse_many<'py>(
    py: Python<'py>,
    items: Vec<Bound<'py, PyBytes>>,
    element_cls: &Bound<'py, PyAny>,
    file_cls: &Bound<'py, PyAny>,
) -> PyResult<Bound<'py, PyList>> {
    let slices = byte_slices(&items);
    let parsed = py.detach(|| dbx::parse_all(&slices));
    let out = PyList::empty(py);
    for r in parsed {
        match r {
            Ok(f) => out.append(Builder::new(py, element_cls, &f).file(file_cls, &f)?)?,
            Err(e) => out.append(value_error(&e).into_value(py))?,
        }
    }
    Ok(out)
}

/// `dbx_to_xml` of many files on all cores. Each item is the XML or the `ValueError` it raised (returned).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
fn dbx_to_xml_many<'py>(py: Python<'py>, items: Vec<Bound<'py, PyBytes>>) -> PyResult<Bound<'py, PyList>> {
    let slices = byte_slices(&items);
    let xml = py.detach(|| dbx::to_xml_all(&slices));
    let out = PyList::empty(py);
    for r in xml {
        match r {
            Ok(s) => out.append(s)?,
            Err(e) => out.append(value_error(&e).into_value(py))?,
        }
    }
    Ok(out)
}

/// The number of worker threads the batch functions use (`RAYON_NUM_THREADS`, else every core).
#[pyfunction]
fn threads() -> usize {
    bc2_formats::threads()
}

/// The suite's Rust core (`rust/`): binary DBX parsing and XML, used by `bfbc2core.dbx` when built.
#[pymodule]
mod bc2_native {
    #[pymodule_export]
    use super::{dbx_parse, dbx_parse_many, dbx_to_xml, dbx_to_xml_many, threads};

    /// The crate version, to tell builds apart.
    #[pymodule_export]
    const VERSION: &str = env!("CARGO_PKG_VERSION");
}
