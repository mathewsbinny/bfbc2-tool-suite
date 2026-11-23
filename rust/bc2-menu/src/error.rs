//! The menu tools' typed errors (round PR1b-2-rest2): what `menucommon` / `aptgraft` raise, as values the library's
//! callers can match without the Python emulation. `bfbc2.exe` turns each back into the exception the Python
//! command line prints (the `From` impl below), so the menu group's recorded answers stay the same.

use std::io;

use bc2_pyrt::err::{Fail, PyErr};

/// What went wrong in the menu tools' library.
#[derive(Debug, thiserror::Error)]
pub enum MenuError {
    /// `MenuToolError(msg)`: the tools' own refusal, in words.
    #[error("{0}")]
    Menu(String),
    /// `apt.AptError(msg)`: a movie the graft cannot read or change.
    #[error("{0}")]
    Apt(String),
    /// `FileNotFoundError(path)`: a movie, bundle or executable named that is not there.
    #[error("{0}: not found")]
    NotFound(String),
    /// A file could not be read or written (`open()`'s `OSError`).
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: String,
        /// The error.
        source: io::Error,
    },
    /// A text the movie's Latin-1 strings cannot hold (`str.encode('latin-1')`'s `UnicodeEncodeError`: its words).
    #[error("{0}")]
    Latin1(String),
    /// A damaged or unusual input the Python reference fails on with an exception of its own (an index or `struct`
    /// error, a value its writer cannot pack), or an argument form the port does not read.
    #[error("a damaged or unusual input (the Python reference fails on it with an exception of its own)")]
    Reference,
}

/// The library's results.
pub type MenuResult<T> = Result<T, MenuError>;

impl MenuError {
    /// `FileNotFoundError(path)`.
    pub fn not_found(path: &str) -> Self {
        Self::NotFound(path.to_owned())
    }

    /// Whether the Python's exception is a `FileNotFoundError` (a missing file named, or `open()` on one).
    pub fn is_file_not_found(&self) -> bool {
        match self {
            Self::NotFound(_) => true,
            Self::Io { source, .. } => source.kind() == io::ErrorKind::NotFound,
            _ => false,
        }
    }

    /// A file's I/O error.
    pub fn io(path: &str, source: io::Error) -> Self {
        Self::Io {
            path: path.to_owned(),
            source,
        }
    }
}

/// The Python's exception for it (`bfbc2.exe`'s menu and graft groups): the same kind and words, or the hand-off to
/// the reference.
impl From<MenuError> for Fail {
    fn from(e: MenuError) -> Self {
        match e {
            MenuError::Menu(m) => Fail::Err(PyErr::new("MenuToolError", m)),
            MenuError::Apt(m) => Fail::Err(PyErr::new("AptError", m)),
            MenuError::NotFound(p) => Fail::Err(PyErr::file_not_found(&p)),
            MenuError::Io { path, source } => bc2_pyrt::err::os_error(&path, &source),
            MenuError::Latin1(m) => Fail::Err(PyErr::new("UnicodeEncodeError", m)),
            MenuError::Reference => Fail::Python,
        }
    }
}

/// `open(path, 'rb').read()`.
pub fn read_file(path: &str) -> MenuResult<Vec<u8>> {
    std::fs::read(path).map_err(|e| MenuError::io(path, e))
}

/// `open(path, encoding='utf-8', errors='replace').read()` with universal newlines (`bc2_pyrt::py::read_text`).
pub fn read_text(path: &str) -> MenuResult<String> {
    let b = read_file(path)?;
    let t = String::from_utf8_lossy(&b);
    Ok(t.replace("\r\n", "\n").replace('\r', "\n"))
}
