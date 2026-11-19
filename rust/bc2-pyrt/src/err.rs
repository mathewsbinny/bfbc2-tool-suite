//! The Python exceptions the ported command lines raise or report (round O2's `pyproc`, moved here in round O3):
//! an exception's type, `str()` and filename, the `OSError` texts of `open()`, and `errors.explain`'s words.

use std::io;

use crate::fmt as pyfmt;

/// A Python exception: its type's name, `str()` of it, its `filename` (an `OSError`'s) and a tool error's
/// `what_to_do`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PyErr {
    pub kind: String,
    pub msg: String,
    pub filename: Option<String>,
    pub what_to_do: Option<String>,
}

impl PyErr {
    pub fn new(kind: &str, msg: impl Into<String>) -> Self {
        Self {
            kind: kind.to_owned(),
            msg: msg.into(),
            filename: None,
            what_to_do: None,
        }
    }

    /// `FileNotFoundError(path)` as the tools raise it themselves (`str()` the path, no filename).
    pub fn file_not_found(path: &str) -> Self {
        Self::new("FileNotFoundError", path)
    }

    /// The last line of its traceback: `Kind: message`, or `Kind` alone.
    pub fn line(&self) -> String {
        if self.msg.is_empty() {
            self.kind.clone()
        } else {
            format!("{}: {}", self.kind, self.msg)
        }
    }
}

/// How a run ends other than with an exit code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// The exception the Python command line raises at this point.
    Err(PyErr),
    /// The Rust core refuses, or the Python's answer here is its own (an exception text only Python makes): the
    /// whole command line is handed to the Python.
    Python,
}

impl From<PyErr> for Fail {
    fn from(e: PyErr) -> Self {
        Self::Err(e)
    }
}

pub type PyResult<T> = Result<T, Fail>;

/// The `OSError` Python's `open()` (and the `os` calls on a file) raises for `path`: `[Errno 2] No such file or
/// directory: 'path'` and its kin; an error whose text only Python knows hands off.
pub fn os_error(path: &str, e: &io::Error) -> Fail {
    let (kind, errno, text) = match e.kind() {
        io::ErrorKind::NotFound => ("FileNotFoundError", 2, "No such file or directory"),
        io::ErrorKind::PermissionDenied => ("PermissionError", 13, "Permission denied"),
        io::ErrorKind::InvalidFilename | io::ErrorKind::InvalidInput => ("OSError", 22, "Invalid argument"),
        _ => return Fail::Python,
    };
    Fail::Err(PyErr {
        kind: kind.to_owned(),
        msg: format!("[Errno {errno}] {text}: {}", pyfmt::str_repr(path)),
        filename: Some(path.to_owned()),
        what_to_do: None,
    })
}

/// `open(path, 'rb').read()`.
pub fn read_file(path: &str) -> PyResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| os_error(path, &e))
}

/// `open(path, 'wb').write(data)`.
pub fn write_file(path: &str, data: &[u8]) -> PyResult<()> {
    std::fs::write(path, data).map_err(|e| os_error(path, &e))
}

/// `errors.explain(e)`: (the cause, what to do), in words.
pub fn explain(e: &PyErr) -> (String, String) {
    let s = e.msg.clone();
    let low = s.to_lowercase();
    if let Some(w) = e.what_to_do.as_ref().filter(|w| !w.is_empty()) {
        return (s, w.clone());
    }
    if e.kind == "ModuleNotFoundError" {
        let name = e.filename.clone().unwrap_or_default();
        return (
            format!("A Python library the tool needs is not installed ({name})."),
            format!("Install it: pip install {name}"),
        );
    }
    if low.contains("opengl") || low.contains("glfw") {
        return (
            "The 3D view could not start OpenGL 3.3 (the graphics driver is too old, or Python runs on a GPU \
             without it)."
                .to_owned(),
            "Update the graphics driver; on a laptop with two GPUs set python.exe to \"High performance\" in \
             Windows Settings > System > Display > Graphics."
                .to_owned(),
        );
    }
    if e.kind == "GameCopyNotFound"
        || low.contains("no game copy")
        || (low.contains("project opfor") && low.contains("not found"))
    {
        return (
            "The tools could not find Project OPFOR BFBC2's game copy.".to_owned(),
            "Put the suite folder next to the \"project-opfor-bfbc2\" folder, or set OPFOR_ROOT to it.".to_owned(),
        );
    }
    if low.contains("not prepared") || low.contains("unknown level") {
        return (
            format!("The level is not available: {s}"),
            "Choose a level from the list, or prepare it first (Levels tab: Prepare level).".to_owned(),
        );
    }
    let file = || {
        e.filename
            .clone()
            .filter(|f| !f.is_empty())
            .unwrap_or_else(|| s.clone())
    };
    if e.kind == "PermissionError" {
        return (
            format!("A file could not be written or read: {}", file()),
            "Close the program using it (the game, an editor), or check the folder is not read-only.".to_owned(),
        );
    }
    if e.kind == "FileNotFoundError" {
        return (
            format!("A file the tool needs is missing: {}", file()),
            "Check the path; if it is a level's file, prepare the level again.".to_owned(),
        );
    }
    if e.kind == "MemoryError" {
        return (
            "The PC ran out of memory.".to_owned(),
            "Close other programs, or open a smaller level.".to_owned(),
        );
    }
    (
        format!("{}: {s}", e.kind),
        "If it happens again, send the log file to the tool's author.".to_owned(),
    )
}
