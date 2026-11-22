//! The caches' budget for `bfbc2core.caches` (round PR1b-2: [`bc2_gamefs::budget`], the one implementation, also
//! `bfbc2.exe caches`'): a folder pruned or cleared, every cache under the overall cap. Each returns what it did as
//! `caches.Result`'s fields: (kept bytes, kept files, deleted bytes, deleted files, busy, stale).

use bc2_gamefs::budget::{self, Done, Rules};
use pyo3::prelude::*;

/// `caches.Result`'s fields.
type Fields = (u64, u64, u64, u64, u64, u64);

fn fields(d: Done) -> Fields {
    (
        d.kept_bytes,
        d.kept_files,
        d.deleted_bytes,
        d.deleted_files,
        d.busy,
        d.stale,
    )
}

fn os_error(e: &std::io::Error) -> PyErr {
    pyo3::exceptions::PyOSError::new_err(e.to_string())
}

/// `caches.prune(top, cap_bytes, suffixes, markers=, marker_age=, keep=, superseded=)` under the folder's lock (its
/// stamp touched); with `unless_within` (seconds) not run, `None`, when the folder's stamp is newer (checked again
/// under the lock: `maybe_prune`).
#[pyfunction]
#[pyo3(signature = (top, cap_bytes, suffixes, markers, marker_age, keep, superseded=false, unless_within=None))]
#[expect(
    clippy::too_many_arguments,
    clippy::needless_pass_by_value,
    reason = "caches.prune's arguments, as PyO3 extracts them (owned lists)"
)]
pub fn caches_prune(
    py: Python<'_>,
    top: &str,
    cap_bytes: u64,
    suffixes: Vec<String>,
    markers: Vec<String>,
    marker_age: f64,
    keep: Vec<String>,
    superseded: bool,
    unless_within: Option<f64>,
) -> PyResult<Option<Fields>> {
    let suffixes: Vec<&str> = suffixes.iter().map(String::as_str).collect();
    let markers: Vec<&str> = markers.iter().map(String::as_str).collect();
    let rules = Rules {
        suffixes: &suffixes,
        markers: &markers,
        marker_age,
        keep: &keep,
        superseded,
        unless_within,
    };
    py.detach(|| budget::prune_folder(top, cap_bytes, &rules))
        .map(|d| d.map(fields))
        .map_err(|e| os_error(&e))
}

/// `caches.clear(top, suffixes, markers)` under the folder's lock.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn caches_clear(py: Python<'_>, top: &str, suffixes: Vec<String>, markers: Vec<String>) -> PyResult<Fields> {
    let suffixes: Vec<&str> = suffixes.iter().map(String::as_str).collect();
    let markers: Vec<&str> = markers.iter().map(String::as_str).collect();
    py.detach(|| budget::clear_files(top, &suffixes, &markers))
        .map(fields)
        .map_err(|e| os_error(&e))
}

/// `caches.prune_total(cap_bytes, opfor)` under the cache root's lock: `[(name, fields)]` in the members' order.
#[pyfunction]
#[pyo3(signature = (cap_bytes, opfor=None))]
pub fn caches_prune_total(py: Python<'_>, cap_bytes: u64, opfor: Option<&str>) -> PyResult<Vec<(String, Fields)>> {
    py.detach(|| budget::prune_total(cap_bytes, opfor))
        .map(|v| v.into_iter().map(|(n, d)| (n.to_owned(), fields(d))).collect())
        .map_err(|e| os_error(&e))
}
