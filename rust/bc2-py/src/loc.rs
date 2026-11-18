//! Round F0: the game's localized texts (`bc2_formats::loc`) for `bfbc2-menu-tools/bc2loc.py`: `loc_parse` (a
//! `LocFile`'s entries, as its own dicts and list), `loc_histogram` / `LocTable` / `loc_decode` (a text decoded through
//! the histogram), `loc_write` (`LocFile.to_bytes`) and `loc_khash`. Each answers `None` where the reference raises or
//! where the port leaves the case to it (a lone surrogate in a text, a value that is not the type the reference
//! writes), so the Python then runs and the result or error is always the reference's.

use bc2_formats::loc;
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList, PyString, PyTuple};

/// A histogram's table, kept in the core so a text is decoded without handing the table over each time.
#[pyclass(frozen, module = "bc2_native")]
pub struct LocTable {
    table: Vec<u16>,
}

#[pymethods]
impl LocTable {
    /// `LocFile.decode(raw)` with this table, or `None` (a lone surrogate: the reference decodes it).
    fn decode(&self, raw: &Bound<'_, PyBytes>) -> Option<String> {
        loc::decode(raw.as_bytes(), Some(&self.table))
    }

    /// The decoded texts of many raw texts (`None` for one the reference must decode).
    #[expect(
        clippy::needless_pass_by_value,
        reason = "PyO3 extracts a list argument as an owned Vec"
    )]
    fn decode_many(&self, raws: Vec<Bound<'_, PyBytes>>) -> Vec<Option<String>> {
        raws.iter()
            .map(|r| loc::decode(r.as_bytes(), Some(&self.table)))
            .collect()
    }

    fn __len__(&self) -> usize {
        self.table.len()
    }
}

/// `LocFile(data)`'s reading as `(name, order, raw, offsets, strings_start)`: `order` the table's hashes in order,
/// `raw` {hash: text bytes} and `offsets` {hash: offset} as the reference's dicts build them (a repeated hash keeps
/// its first place and its last value); `None` where the reference raises.
#[pyfunction]
pub fn loc_parse<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let bytes = data.as_bytes();
    let Ok(f) = py.detach(|| loc::parse(bytes)) else {
        return Ok(None);
    };
    let order = PyList::empty(py);
    let raw = PyDict::new(py);
    let offsets = PyDict::new(py);
    for (h, o, r) in &f.entries {
        raw.set_item(h, PyBytes::new(py, bytes.get(r.clone()).unwrap_or(&[])))?;
        offsets.set_item(h, o)?;
        order.append(h)?;
    }
    let name: String = f.name.iter().map(|&b| char::from(b)).collect();
    Ok(Some((name, order, raw, offsets, f.strings).into_pyobject(py)?))
}

/// A histogram's `(table as a tuple, LocTable)`, or `None` where the reference raises.
#[pyfunction]
pub fn loc_histogram<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyTuple>>> {
    let Ok(table) = loc::histogram(data.as_bytes()) else {
        return Ok(None);
    };
    let tuple = PyTuple::new(py, &table)?;
    Ok(Some((tuple, LocTable { table }).into_pyobject(py)?))
}

/// `LocFile.decode(raw)` without a histogram (`table` None) or with one; `None` where the reference must decode.
#[pyfunction]
#[pyo3(signature = (raw, table=None))]
pub fn loc_decode(raw: &Bound<'_, PyBytes>, table: Option<&Bound<'_, LocTable>>) -> Option<String> {
    loc::decode(raw.as_bytes(), table.map(|t| t.get().table.as_slice()))
}

/// `LocFile.to_bytes()` from its parts: `name16` (`data[20:36]`), `strings`, `offsets` {hash: offset}, `changed`
/// {hash: text}; `None` where a value is not the reference's kind (an int past `u32`, a text not `bytes`).
#[pyfunction]
pub fn loc_write<'py>(
    py: Python<'py>,
    name16: &Bound<'py, PyAny>,
    strings: &Bound<'py, PyAny>,
    offsets: &Bound<'py, PyAny>,
    changed: &Bound<'py, PyAny>,
) -> Option<Bound<'py, PyBytes>> {
    let parts = (|| {
        let name: [u8; 16] = name16.cast::<PyBytes>().ok()?.as_bytes().try_into().ok()?;
        let strings = strings.cast::<PyBytes>().ok()?.as_bytes().to_vec();
        let offsets = offsets
            .cast::<PyDict>()
            .ok()?
            .iter()
            .map(|(k, v)| Some((k.extract::<u32>().ok()?, v.extract::<u32>().ok()?)))
            .collect::<Option<Vec<_>>>()?;
        let changed = changed
            .cast::<PyDict>()
            .ok()?
            .iter()
            .map(|(k, v)| Some((k.extract::<u32>().ok()?, v.cast::<PyBytes>().ok()?.as_bytes().to_vec())))
            .collect::<Option<Vec<_>>>()?;
        Some((name, strings, offsets, changed))
    })();
    let (name, strings, offsets, changed) = parts?;
    let out = py.detach(|| loc::write(&name, &strings, &offsets, &changed));
    out.ok().map(|b| PyBytes::new(py, &b))
}

/// `khash(key)` of a `str` or `bytes` key, or `None` for anything else (a `str` with a lone surrogate included).
#[pyfunction]
pub fn loc_khash(key: &Bound<'_, PyAny>) -> Option<u32> {
    if let Ok(s) = key.cast::<PyString>() {
        return s.to_str().ok().map(loc::khash_str);
    }
    key.cast::<PyBytes>().ok().map(|b| loc::khash(b.as_bytes()))
}

/// `menucommon.parse_bigf(b)` / `apt.read_bigf`'s members as the reference's dict {name: bytes} (a repeated name
/// keeps its first place and takes its last data), or `None` where the reference raises.
#[pyfunction]
pub fn bigf_parse<'py>(py: Python<'py>, b: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyDict>>> {
    let data = b.as_bytes();
    let Some(members) = bc2_formats::bigf::parse(data) else {
        return Ok(None);
    };
    let out = PyDict::new(py);
    for (name, r) in members {
        out.set_item(name, PyBytes::new(py, data.get(r).unwrap_or(&[])))?;
    }
    Ok(Some(out))
}
