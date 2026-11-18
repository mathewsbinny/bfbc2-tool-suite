//! Round F0: the console containers (`bc2_formats::stfs`, `bc2_formats::xiso`) for
//! `bfbc2-console-readers/stfs/stfs.py` and `xiso/xiso.py`.
//!
//! `stfs_open` reads a package's header and file table (the values `stfs.Package` keeps, the entries as its dicts)
//! and gives an `StfsReader` that reads an entry's bytes; `xiso_find_partition` / `xiso_walk` are `xiso.py`'s
//! functions on an image's path. Each answers `None` where the reference raises, never finishes or recurses too
//! deep, or the file cannot be read: the Python then runs, so a result here is always the reference's. The files are
//! opened for each call and closed after it, so the caller's own handle decides when the file is in use.

use std::fs::File;
use std::io::BufReader;

use bc2_formats::{stfs, xiso};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyTuple};

fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// Reads a package's entries (the header it was opened with kept).
#[pyclass(frozen, module = "bc2_native")]
pub struct StfsReader {
    path: String,
    header: stfs::Header,
}

#[pymethods]
impl StfsReader {
    /// `Package.read(e)` from the entry's start, count, consecutive and size, or `None` where the Python must read
    /// it (a value that is not a non-negative int, a file that cannot be read).
    fn read<'py>(
        &self,
        py: Python<'py>,
        start: &Bound<'py, PyAny>,
        count: &Bound<'py, PyAny>,
        consecutive: &Bound<'py, PyAny>,
        size: &Bound<'py, PyAny>,
    ) -> PyResult<Option<Bound<'py, PyBytes>>> {
        let (Ok(start), Ok(count), Ok(size)) = (start.extract::<u32>(), count.extract::<u32>(), size.extract::<u64>())
        else {
            return Ok(None);
        };
        let consecutive = consecutive.is_truthy()?;
        let got = py.detach(|| {
            let f = File::open(&self.path).ok()?;
            stfs::Package::from_header(f, self.header.clone())
                .read(u64::from(start), u64::from(count), consecutive, size)
                .ok()
        });
        Ok(got.map(|b| PyBytes::new(py, &b)))
    }
}

/// `stfs.Package(path)`'s values: `(magic, name, header_size, separation, ft_blocks, ft_start, base, allocated,
/// top_level, shift, entries, reader)`, the entries as the reference's dicts; `None` where the reference raises or
/// never finishes.
#[pyfunction]
pub fn stfs_open<'py>(py: Python<'py>, path: &str) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let got = py.detach(|| {
        let f = File::open(path).ok()?;
        stfs::Package::open(f).ok().map(|p| (p.header, p.entries))
    });
    let Some((h, entries)) = got else {
        return Ok(None);
    };
    let list = PyList::empty(py);
    for e in &entries {
        let d = PyDict::new(py);
        d.set_item("name", latin1(&e.name))?;
        d.set_item("dir", e.dir)?;
        d.set_item("consecutive", e.consecutive)?;
        d.set_item("count", e.count)?;
        d.set_item("start", e.start)?;
        d.set_item("parent", e.parent)?;
        d.set_item("size", e.size)?;
        d.set_item("path", latin1(&e.path))?;
        list.append(d)?;
    }
    let values = (
        PyBytes::new(py, &h.magic),
        h.name.clone(),
        h.header_size,
        h.separation,
        h.ft_blocks,
        h.ft_start,
        h.base,
        h.allocated,
        h.top_level,
        h.shift,
        list,
        StfsReader {
            path: path.to_owned(),
            header: h,
        },
    );
    Ok(Some(values.into_pyobject(py)?))
}

/// `xiso.find_partition` of the image at `path`, or `None` (no volume: the reference raises; or a read error).
#[pyfunction]
pub fn xiso_find_partition(py: Python<'_>, path: &str) -> Option<u64> {
    py.detach(|| {
        let mut f = BufReader::new(File::open(path).ok()?);
        xiso::find_partition(&mut f).ok()
    })
}

/// `list(xiso.walk(f, base, sector, size, prefix))` of the image at `path` as `(files, position)`: each file
/// `(path, sector, size)`, and where the walk leaves the file; `None` where the reference must walk it.
#[pyfunction]
pub fn xiso_walk<'py>(
    py: Python<'py>,
    path: &str,
    base: &Bound<'py, PyAny>,
    sector: &Bound<'py, PyAny>,
    size: &Bound<'py, PyAny>,
    prefix: &Bound<'py, PyAny>,
) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let (Ok(base), Ok(sector), Ok(size), Ok(prefix)) = (
        base.extract::<u64>(),
        sector.extract::<u32>(),
        size.extract::<u32>(),
        prefix.extract::<String>(),
    ) else {
        return Ok(None);
    };
    let Some(prefix) = prefix
        .chars()
        .map(|c| u8::try_from(u32::from(c)).ok())
        .collect::<Option<Vec<u8>>>()
    else {
        return Ok(None);
    };
    let got = py.detach(|| {
        let mut f = BufReader::new(File::open(path).ok()?);
        xiso::walk(&mut f, base, sector, size, &prefix).ok()
    });
    let Some((files, pos)) = got else {
        return Ok(None);
    };
    let list = PyList::empty(py);
    for (p, sec, sz) in &files {
        list.append((latin1(p), sec, sz))?;
    }
    Ok(Some((list, pos).into_pyobject(py)?))
}

/// Why `stfs_open` left a package to the reference: `"header"`, `"table"`, `"cycle"` (the reference never
/// finishes), `"io"`, or `None` when it did not.
#[pyfunction]
pub fn stfs_refusal(py: Python<'_>, path: &str) -> Option<&'static str> {
    py.detach(|| {
        let Ok(f) = File::open(path) else { return Some("io") };
        match stfs::Package::open(f) {
            Ok(_) => None,
            Err(stfs::Error::Header) => Some("header"),
            Err(stfs::Error::Table) => Some("table"),
            Err(stfs::Error::Cycle) => Some("cycle"),
            Err(stfs::Error::Io(_)) => Some("io"),
        }
    })
}
