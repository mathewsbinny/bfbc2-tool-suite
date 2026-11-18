//! The bindings of the cull tree tool's hot parts (`bfbc2-culltree/culltree.py` calls them when the module is
//! built): `bc2_formats::culltree`.
//!
//! `culltree_elements` builds `Doc`'s element tree with the caller's `El` class (each element made as `El.__new__`
//! and given the attributes `El.__init__` and the reader set), so the objects are the ones the retired Python reader
//! made. A stream that does not read raises `ValueError` saying why (`Doc` raises it as `CullTreeError`).

use std::path::PathBuf;

use bc2_formats::culltree;
use pyo3::exceptions::PyValueError;
use pyo3::intern;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyList, PyTuple};

/// `Doc.__init__`'s element loop: `(roots, end)` (the trailer starts at `end`), the elements made as `el_cls`
/// instances belonging to `doc`; `ValueError` when the stream does not read.
#[pyfunction]
pub fn culltree_elements<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    start: usize,
    doc: &Bound<'py, PyAny>,
    el_cls: &Bound<'py, PyAny>,
) -> PyResult<(Bound<'py, PyList>, usize)> {
    let bytes = data.as_bytes();
    let read = py
        .detach(|| culltree::elements(bytes, start))
        .map_err(|e| PyValueError::new_err(e.to_string()))?;
    let new = el_cls.getattr(intern!(py, "__new__"))?;
    let names = [
        intern!(py, "doc"),
        intern!(py, "tag"),
        intern!(py, "head"),
        intern!(py, "attrs"),
        intern!(py, "extra"),
        intern!(py, "value"),
        intern!(py, "children"),
    ];
    let empty = PyBytes::new(py, b"");
    let none = py.None().into_bound(py);
    let roots = PyList::empty(py);
    // the open containers' children lists and the index their subtree ends at
    let mut open: Vec<(Bound<'py, PyList>, usize)> = Vec::new();
    for (i, e) in read.elements.iter().enumerate() {
        while open.last().is_some_and(|(_, end)| *end <= i) {
            open.pop();
        }
        let el = new.call1((el_cls,))?;
        let attrs = PyList::empty(py);
        for &(k, v) in read.attrs.get(e.attrs.clone()).unwrap_or_default() {
            attrs.append(PyTuple::new(py, [k, v])?)?;
        }
        let (children, value) = if e.end.is_some() {
            (PyList::empty(py).into_any(), empty.clone())
        } else {
            let v = bytes.get(e.value.clone()).unwrap_or_default();
            let value = if v.is_empty() {
                empty.clone()
            } else {
                PyBytes::new(py, v)
            };
            (none.clone(), value)
        };
        let values = [
            doc.clone(),
            e.tag.into_pyobject(py)?.into_any(),
            e.head.into_pyobject(py)?.into_any(),
            attrs.into_any(),
            e.extra.into_pyobject(py)?.into_any(),
            value.into_any(),
            children.clone(),
        ];
        for (n, v) in names.iter().zip(values) {
            el.setattr(n, v)?;
        }
        open.last().map_or(&roots, |(l, _)| l).append(&el)?;
        if let Some(end) = e.end {
            open.push((children.cast_into::<PyList>()?, end));
        }
    }
    Ok((roots, read.end))
}

/// `glob.glob(os.path.join(dir, '**', '*.dbx'), recursive=True)` through Rust, as paths relative to `dir` (not
/// sorted); `None` when a name is not valid Unicode (glob then lists the folder).
#[pyfunction]
pub fn culltree_dbx_files(py: Python<'_>, dir: &str) -> Option<Vec<String>> {
    let files = py.detach(|| culltree::dbx_files(std::path::Path::new(dir)));
    files.iter().map(|p| p.to_str().map(str::to_owned)).collect()
}

/// For each path: does the file contain `needle`? `None` for a file that cannot be read. Read on all cores.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn culltree_contains(py: Python<'_>, paths: Vec<PathBuf>, needle: &Bound<'_, PyBytes>) -> Vec<Option<bool>> {
    let needle = needle.as_bytes();
    py.detach(|| culltree::contains(&paths, needle))
}
