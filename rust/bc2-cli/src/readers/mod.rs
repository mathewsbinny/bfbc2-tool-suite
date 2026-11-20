//! Round O5's `readers` group: the console readers' command lines (`bfbc2-console-readers`): `apt.py`, `stfs.py`
//! and `xiso.py`, over the Rust readers of rounds E3n and F0 (`bc2_formats::apt`, `bigf`, `stfs`, `xiso`).

use crate::pyproc::{Fail, PyErr, PyResult, os_error};

mod apt;
mod stfs;
mod xiso;

use crate::pyproc::Tool;

/// The group's tools.
pub const TOOLS: &[Tool] = &[
    Tool {
        name: "apt",
        script: "bfbc2-console-readers\\apt\\apt.py",
        log: Some("apt"),
        run: apt::run,
    },
    Tool {
        name: "stfs",
        script: "bfbc2-console-readers\\stfs\\stfs.py",
        log: Some("stfs"),
        run: stfs::run,
    },
    Tool {
        name: "xiso",
        script: "bfbc2-console-readers\\xiso\\xiso.py",
        log: Some("xiso"),
        run: xiso::run,
    },
];

/// Latin-1 bytes as Python's `bytes.decode('latin-1')`.
fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect()
}

/// `s.lower()` of a text the readers compare: Python's and Rust's lower case agree on Latin-1 (every name a console
/// container holds); a text with other characters is left to the Python.
fn lower(s: &str) -> PyResult<String> {
    if s.chars().any(|c| u32::from(c) > 0xFF) {
        return Err(Fail::Python);
    }
    Ok(s.to_lowercase())
}

/// `open(path, 'rb')`'s errors before any read: a missing file, a folder (`PermissionError` on Windows).
fn open_check(path: &str) -> PyResult<()> {
    match std::fs::metadata(path) {
        Ok(m) if m.is_dir() => Err(os_error(
            path,
            &std::io::Error::from(std::io::ErrorKind::PermissionDenied),
        )),
        Ok(_) => Ok(()),
        Err(e) => Err(os_error(path, &e)),
    }
}

/// `os.path.join(out, *path.split('/'))` for the names a container holds; a part that would reset the path (a drive,
/// a leading backslash) is left to the Python.
fn target(out: &str, path: &str) -> PyResult<String> {
    let parts: Vec<&str> = path.split('/').collect();
    if parts.iter().any(|p| p.contains(':') || p.starts_with('\\')) {
        return Err(Fail::Python);
    }
    Ok(bc2_pyrt::py::join(out, &parts))
}

/// `os.makedirs(os.path.dirname(dst), exist_ok=True)`; an error (or an empty folder name) is left to the Python.
fn makedirs_for(dst: &str) -> PyResult<()> {
    let d = bc2_pyrt::py::dirname(dst);
    if d.is_empty() {
        return Err(Fail::Python);
    }
    std::fs::create_dir_all(&d).map_err(|_| Fail::Python)
}

/// `IndexError: list index out of range` (a missing command-line argument read as `sys.argv[k]`).
fn index_error() -> Fail {
    Fail::Err(PyErr::new("IndexError", "list index out of range"))
}
