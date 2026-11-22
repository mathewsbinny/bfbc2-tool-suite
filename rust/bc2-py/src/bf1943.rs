//! BF1943's bundle reader (`bc2_formats::bf1943`, round PR1b-1-rest), for `bfbc2-bf1943-import/bf1943import.py`'s
//! `Source` and `RESTARTS`: `bf1943_check` (`fbrb1943.Bundle`'s verdict on a whole file), `bf1943_read` (an entry
//! of a compressed payload through the process's restart points), `bf1943_restarts` (what the process holds) and
//! `bf1943_restarts_clear`.

use std::path::PathBuf;

use bc2_formats::bf1943::{self, Budgets, PayloadRef, ReadError};
use pyo3::exceptions::{PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

/// `fbrb1943.Bundle(raw)`'s verdict: `(entries, compressed, payload start, payload length)` with the entries as
/// the reader's dicts (`name`, `type`, `flags`, `offset`, `size`, `size2`, in that order), or `None` where it raises
/// `ValueError`. `bf1943_check.READER` is the reader's SHA-1 this follows.
#[pyfunction]
pub fn bf1943_check<'py>(py: Python<'py>, raw: &[u8]) -> PyResult<Option<Bound<'py, PyAny>>> {
    let Some(c) = py.detach(|| bf1943::check(raw)) else {
        return Ok(None);
    };
    let entries = PyList::empty(py);
    for e in &c.entries {
        let d = PyDict::new(py);
        d.set_item("name", &e.name)?;
        d.set_item("type", &e.kind)?;
        d.set_item("flags", e.flags)?;
        d.set_item("offset", e.offset)?;
        d.set_item("size", e.size)?;
        d.set_item("size2", e.size)?;
        entries.append(d)?;
    }
    Ok(Some(
        (entries, c.compressed, c.payload_at, c.payload_len)
            .into_pyobject(py)?
            .into_any(),
    ))
}

/// The SHA-1 of `fbrb1943.py` that `bf1943_check` follows.
#[pyfunction]
pub fn bf1943_reader() -> &'static str {
    bf1943::READER_SHA1
}

/// Bytes `[offset, offset + size)` of the compressed payload of the bundle at `path` (opened at `file_size` /
/// `mtime_ns`; its payload at `payload_at`, `payload_len` bytes decompressed), through the process's restart points
/// (kept on disk in `folder`, `None`: not kept): `(data, info)`, info the read's counts (`indexed`, `shared_points`,
/// `from_disk`, `whole`, `spans`, `hits`, `decompressed_bytes`). `OSError` when the file cannot be read; `ValueError`
/// `('damaged', ...)` when the payload no longer decompresses, `('changed', ...)` when it is not the one opened.
#[pyfunction]
#[pyo3(signature = (path, file_size, mtime_ns, payload_at, payload_len, offset, size, folder=None))]
#[expect(
    clippy::too_many_arguments,
    reason = "the payload's identity and the range, as Source holds them"
)]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts an optional path argument as an owned PathBuf"
)]
pub fn bf1943_read(
    py: Python<'_>,
    path: PathBuf,
    file_size: u64,
    mtime_ns: i128,
    payload_at: u64,
    payload_len: u64,
    offset: u64,
    size: u64,
    folder: Option<PathBuf>,
) -> PyResult<(Bound<'_, PyBytes>, Bound<'_, PyDict>)> {
    let r = PayloadRef {
        path,
        size: file_size,
        mtime_ns,
        payload_at,
        payload_len,
    };
    let budgets = Budgets::from_env();
    let got = py.detach(|| bf1943::global().read(&r, offset, size, folder.as_deref(), budgets));
    let (data, info) = got.map_err(|e| match e {
        ReadError::Io(e) => PyOSError::new_err(e.to_string()),
        ReadError::Damaged => PyValueError::new_err(("damaged", e.to_string())),
        ReadError::Changed => PyValueError::new_err(("changed", e.to_string())),
    })?;
    let d = PyDict::new(py);
    d.set_item("indexed", u64::from(info.indexed))?;
    d.set_item("shared_points", u64::from(info.shared))?;
    d.set_item("from_disk", u64::from(info.from_disk))?;
    d.set_item("whole", u64::from(info.whole))?;
    d.set_item("spans", info.spans)?;
    d.set_item("hits", info.hits)?;
    d.set_item("decompressed_bytes", info.decompressed_bytes)?;
    Ok((PyBytes::new(py, &data), d))
}

/// What the process's restart points hold: `(payloads, bytes as costed, hits, misses, dropped)`.
#[pyfunction]
pub fn bf1943_restarts() -> (usize, u64, u64, u64, u64) {
    let g = bf1943::global();
    let (n, held) = g.held();
    let (hits, misses, dropped) = g.counts();
    (n, held, hits, misses, dropped)
}

/// Forgets the process's restart points and decoded spans.
#[pyfunction]
pub fn bf1943_restarts_clear() {
    bf1943::global().clear();
}
