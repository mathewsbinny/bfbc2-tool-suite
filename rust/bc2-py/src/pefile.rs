//! Bindings for the menu tools' executable reader (`bfbc2-menu-tools/menucommon.py`'s `PEImage`, round F2b), on
//! `bc2_harness::pe::raw`: the header, the data sections' strings, a C string's and a dword's places.

use bc2_harness::pe::raw::{self, Header, RawError, Section};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict, PyList};

/// `PEImage.__init__`'s reading: `('ok', timestamp, base, [(name, va, vsize, raw, rsize, flags)])`, `('not_exe',)`,
/// `('not_32',)`, or `None` where the reference raises another exception.
#[pyfunction]
pub fn pe_raw_header<'py>(py: Python<'py>, data: &Bound<'py, PyBytes>) -> PyResult<Option<Bound<'py, PyAny>>> {
    let h = match raw::parse(data.as_bytes()) {
        Ok(h) => h,
        Err(RawError::NotExe) => return Ok(Some(("not_exe",).into_pyobject(py)?.into_any())),
        Err(RawError::Not32) => return Ok(Some(("not_32",).into_pyobject(py)?.into_any())),
        Err(RawError::Refused) => return Ok(None),
    };
    let secs = PyList::empty(py);
    for s in &h.sections {
        secs.append((&s.name, s.va, s.vsize, s.raw, s.rsize, s.flags))?;
    }
    Ok(Some(("ok", h.timestamp, h.base, secs).into_pyobject(py)?.into_any()))
}

/// A header of the caller's data sections (`(va, file offset, raw size)` each).
fn header_of(sections: &[(u64, u32, u32)]) -> Header {
    Header {
        timestamp: 0,
        base: 0,
        sections: sections
            .iter()
            .map(|&(va, ra, rs)| Section {
                name: String::new(),
                va,
                vsize: 0,
                raw: ra,
                rsize: rs,
                flags: 0,
            })
            .collect(),
    }
}

/// `PEImage.strings(minimum)` over the data sections given: `{text: [va, ...]}` in the reference's order.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn pe_raw_strings<'py>(
    py: Python<'py>,
    data: &Bound<'py, PyBytes>,
    sections: Vec<(u64, u32, u32)>,
    minimum: usize,
) -> PyResult<Bound<'py, PyDict>> {
    let b = data.as_bytes();
    let h = header_of(&sections);
    let found = py.detach(|| raw::strings(b, &h, minimum));
    let out = PyDict::new(py);
    for (text, vas) in found {
        out.set_item(text, vas)?;
    }
    Ok(out)
}

/// `PEImage.find_cstring(text)` over the data sections given (`text` its ASCII bytes).
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn pe_raw_find_cstring(data: &Bound<'_, PyBytes>, sections: Vec<(u64, u32, u32)>, text: &[u8]) -> Vec<u64> {
    raw::find_cstring(data.as_bytes(), &header_of(&sections), text)
}

/// `PEImage.find_dword(value, sections)`: `sections` as `(file offset, raw size)`.
#[pyfunction]
#[expect(
    clippy::needless_pass_by_value,
    reason = "PyO3 extracts a list argument as an owned Vec"
)]
pub fn pe_raw_find_dword(data: &Bound<'_, PyBytes>, sections: Vec<(u32, u32)>, value: u32) -> Vec<usize> {
    raw::find_dword(data.as_bytes(), &sections, value)
}
