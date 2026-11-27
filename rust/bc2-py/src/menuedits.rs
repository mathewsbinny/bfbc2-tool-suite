//! Round OR14: Project OPFOR's WW2 menu edits and texts without `bfbc2-menu-tools`' Python API.
//!
//! - `pacific_main_nav` / `pacific_main_root`: `ww2\menu_install.py`'s movie edits (`bc2_menu::pacific`);
//! - `apt_dump_text`: `aptgraft.dump_text(aptgraft.load_members(spec)[1])` (`bc2_formats::aptmodel::dump_text`), the
//!   dumps its semantic diff compares;
//! - `LocFile`, `loc_languages`, `loc_load`: `bc2loc.LocFile` / `languages` / `load` (`bc2_menu::loc`), which
//!   `names.py`, `ww2_awards.py`, `wake_install.py` and `menu_install.py` read and write the texts with.
//!
//! An `AptError` of the Python is `bc2_native.AptError` with its words, a `MenuToolError` a `ValueError` with its
//! words, a damaged input the Python fails on with an exception of its own a `ValueError` naming the input.

use bc2_menu::error::MenuError;
use bc2_menu::graft::{self, GRAFT_LAYOUT};
use bc2_menu::{loc as mloc, pacific};
use pyo3::exceptions::{PyFileNotFoundError, PyOSError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyBytes, PyDict};

use super::aptmovie::AptError;

fn err(what: &str, e: MenuError) -> PyErr {
    match e {
        MenuError::Apt(m) => AptError::new_err(m),
        MenuError::NotFound(p) => PyFileNotFoundError::new_err(p),
        MenuError::Io { path, source } => PyOSError::new_err(format!("{path}: {source}")),
        MenuError::Menu(m) | MenuError::Latin1(m) => PyValueError::new_err(m),
        MenuError::Reference => PyValueError::new_err(format!("{what}: {}", MenuError::Reference)),
    }
}

/// `edit_main_nav` on the movie at `spec` (a `.res`, or `<bundle>:<name>`): (the movie written in `GRAFT_LAYOUT`,
/// (background image, label image, logo image), the report's lines).
#[pyfunction]
#[pyo3(signature = (spec, label_size, logo_size))]
#[expect(clippy::type_complexity, reason = "the Python's tuple")]
pub fn pacific_main_nav<'py>(
    py: Python<'py>,
    spec: &str,
    label_size: (f64, f64),
    logo_size: (f64, f64),
) -> PyResult<(Bound<'py, PyBytes>, (i64, i64, i64), Vec<String>)> {
    let got = py.detach(|| -> Result<_, MenuError> {
        let mut m = graft::load(spec)?;
        let mut report = Vec::new();
        let ids = pacific::edit_main_nav(&mut m, &mut report, label_size, logo_size)?;
        let res = m.to_res(m.big, GRAFT_LAYOUT)?;
        Ok((res, ids, report))
    });
    let (res, ids, report) = got.map_err(|e| err(spec, e))?;
    Ok((PyBytes::new(py, &res), ids, report))
}

/// `edit_main_root` on the movie at `spec`: (the movie written in `GRAFT_LAYOUT`, the report's lines).
#[pyfunction]
pub fn pacific_main_root<'py>(py: Python<'py>, spec: &str) -> PyResult<(Bound<'py, PyBytes>, Vec<String>)> {
    let got = py.detach(|| -> Result<_, MenuError> {
        let mut m = graft::load(spec)?;
        let mut report = Vec::new();
        pacific::edit_main_root(&mut m, &mut report)?;
        Ok((m.to_res(m.big, GRAFT_LAYOUT)?, report))
    });
    let (res, report) = got.map_err(|e| err(spec, e))?;
    Ok((PyBytes::new(py, &res), report))
}

/// `aptgraft.dump_text(aptgraft.load_members(spec)[1])`: apt.py's dump of the movie, offsets taken out.
#[pyfunction]
pub fn apt_dump_text(py: Python<'_>, spec: &str) -> PyResult<Vec<String>> {
    py.detach(|| -> Result<Vec<String>, MenuError> {
        let (_, members) = graft::load_members(spec)?;
        bc2_formats::aptmodel::dump_text(&members).map_err(|r| MenuError::Menu(format!("{spec}: {}", r.0)))
    })
    .map_err(|e| err(spec, e))
}

/// `bc2loc.LocFile`: a `language_<lang>_<kind>.bin` and its texts.
#[pyclass(module = "bc2_native", name = "LocFile")]
pub struct LocFile {
    f: mloc::LocFile,
    name: String,
}

#[pymethods]
impl LocFile {
    /// `LocFile(data, histogram=None, name='localization file')`.
    #[new]
    #[pyo3(signature = (data, histogram=None, name="localization file"))]
    fn py_new(data: Vec<u8>, histogram: Option<Vec<u8>>, name: &str) -> PyResult<Self> {
        let f = mloc::LocFile::new(data, histogram, name).map_err(|e| err(name, e))?;
        Ok(Self {
            f,
            name: name.to_owned(),
        })
    }

    /// `lookup(key)`: (hash, the name it was found under, text), the PC text `WIN32_<key>` first; or `None`.
    fn lookup(&self, key: &str) -> PyResult<Option<(u32, String, String)>> {
        self.f.lookup(key).map_err(|e| err(&self.name, e))
    }

    /// `text(hash)`: the decoded text, or `None`.
    fn text(&self, h: u32) -> PyResult<Option<String>> {
        self.f.text(h).map_err(|e| err(&self.name, e))
    }

    /// `raw.get(hash)`: the text's bytes as stored, or `None`.
    fn raw_get<'py>(&self, py: Python<'py>, h: u32) -> Option<Bound<'py, PyBytes>> {
        self.f.raw.get(&h).map(|b| PyBytes::new(py, b))
    }

    /// `set(key, text)`: ASCII only, stored as it is.
    fn set(&mut self, key: &str, text: &str) -> PyResult<()> {
        self.f.set(key, text).map_err(|e| err(key, e))
    }

    /// `to_bytes()`: the file again (unchanged: the bytes it was read from).
    fn to_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyBytes>> {
        let b = self.f.to_bytes().map_err(|e| err(&self.name, e))?;
        Ok(PyBytes::new(py, &b))
    }

    /// The table's hashes in file order, then the ones `set` added.
    fn order(&self) -> Vec<u32> {
        self.f.order.clone()
    }
}

/// `bc2loc.languages(root)`: {language: the bundle holding its texts}.
#[pyfunction]
pub fn loc_languages<'py>(py: Python<'py>, root: &str) -> PyResult<Bound<'py, PyDict>> {
    let langs = mloc::languages(root).map_err(|e| err(root, e))?;
    let out = PyDict::new(py);
    for (k, v) in &langs.items {
        out.set_item(k, v)?;
    }
    Ok(out)
}

/// `bc2loc.load(lang, root, path)`.
#[pyfunction]
#[pyo3(signature = (lang, root=None, path=None))]
pub fn loc_load(lang: &str, root: Option<&str>, path: Option<&str>) -> PyResult<LocFile> {
    let f = mloc::load(lang, root, path).map_err(|e| err(lang, e))?;
    Ok(LocFile {
        f,
        name: lang.to_owned(),
    })
}
