//! The bundle cache's controls (round E3e: `bc2_formats::fbrb::cache`), for `bfbc2core.fbrb`: the configuration,
//! what the cache did, and a read without it (the tests' reference).

use std::path::PathBuf;

use bc2_formats::fbrb::{self, cache};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

use super::{bytes_list, fbrb_error};

/// `fbrb_read` without the cache: the payload decompressed from its start, as before round E3e.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn fbrb_read_uncached<'py>(
    py: Python<'py>,
    path: &str,
    keys: Vec<String>,
    lowered: Vec<String>,
) -> PyResult<Bound<'py, PyList>> {
    let data = py
        .detach(|| fbrb::read_path_uncached(path, &lowered))
        .map_err(|e| fbrb_error(e, path, &keys))?;
    bytes_list(py, &data)
}

/// The cache's configuration: `{'memory': bytes, 'folder': str or None, 'disk_cap': bytes, 'disk_min': bytes}`.
#[pyfunction]
pub fn fbrb_cache_config(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let c = cache::config();
    let d = PyDict::new(py);
    d.set_item("memory", c.memory)?;
    d.set_item("folder", c.folder.map(|f| f.to_string_lossy().into_owned()))?;
    d.set_item("disk_cap", c.disk_cap)?;
    d.set_item("disk_min", c.disk_min)?;
    Ok(d)
}

/// Sets the cache's configuration (any of the keys `fbrb_cache_config` gives; `folder=None` turns the disk cache
/// off) and forgets what the process kept in memory. `env=True` first takes the environment's configuration again.
#[pyfunction]
#[pyo3(signature = (env=false, memory=None, folder=None, disk_cap=None, disk_min=None, no_disk=false))]
pub fn fbrb_cache_configure(
    env: bool,
    memory: Option<u64>,
    folder: Option<PathBuf>,
    disk_cap: Option<u64>,
    disk_min: Option<u64>,
    no_disk: bool,
) {
    let mut c = if env {
        cache::CacheConfig::from_env()
    } else {
        cache::config()
    };
    if let Some(m) = memory {
        c.memory = m;
    }
    if folder.is_some() {
        c.folder = folder;
    }
    if no_disk {
        c.folder = None;
    }
    if let Some(m) = disk_cap {
        c.disk_cap = m;
    }
    if let Some(m) = disk_min {
        c.disk_min = m;
    }
    cache::configure(c);
}

/// What the cache did in this process: `reads`, `disk_hits`, `memory_hits`, `decompressed_reads`,
/// `decompressed_bytes`, `written`, `written_bytes`, `header_hits`. `reset=True` zeroes them after reading.
#[pyfunction]
#[pyo3(signature = (reset=false))]
pub fn fbrb_cache_stats(py: Python<'_>, reset: bool) -> PyResult<Bound<'_, PyDict>> {
    let s = cache::stats();
    if reset {
        cache::reset_stats();
    }
    let d = PyDict::new(py);
    for (k, v) in [
        ("reads", s.reads),
        ("disk_hits", s.disk_hits),
        ("memory_hits", s.memory_hits),
        ("decompressed_reads", s.decompressed_reads),
        ("decompressed_bytes", s.decompressed_bytes),
        ("written", s.written),
        ("written_bytes", s.written_bytes),
        ("header_hits", s.header_hits),
    ] {
        d.set_item(k, v)?;
    }
    Ok(d)
}

/// Forgets what the process kept in memory (headers and decompressed payloads); the disk cache stays.
#[pyfunction]
pub fn fbrb_cache_clear() {
    cache::clear_memory();
}

/// The payload files of the disk cache's `folder` that no read can use again (their bundle rewritten or deleted;
/// round QW1): `bfbc2core.caches`' `--prune` deletes them first.
#[pyfunction]
pub fn fbrb_cache_superseded(folder: &str) -> Vec<String> {
    cache::superseded(std::path::Path::new(folder))
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}
