//! The errors of DbxEdit's commands, as the .NET exceptions `DbxEdit.cs` raises: their kind decides what the tool
//! prints (an import catches some, `ToolLog.Explain` words the rest) and the exit code.

use std::fmt;
use std::io;
use std::path::Path;

/// The .NET exception a failure corresponds to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaultKind {
    /// `InvalidDataException`: a file DbxEdit refuses with its own message.
    InvalidData,
    /// `IndexOutOfRangeException`: a read past an array (a damaged file).
    IndexOutOfRange,
    /// `ArgumentOutOfRangeException`: an index past a list, a bad character reference.
    ArgumentOutOfRange,
    /// `ArgumentException`: a copy past the end of an array.
    Argument,
    /// `FileNotFoundException`.
    FileNotFound,
    /// `DirectoryNotFoundException`.
    DirectoryNotFound,
    /// `IOException` with a sharing or lock violation (Windows error 32 / 33).
    Sharing,
    /// Any other `IOException`.
    Io,
    /// `UnauthorizedAccessException`.
    UnauthorizedAccess,
    /// `NullReferenceException` (a file without the parts a command needs: no partition, a non-binary file).
    NullReference,
    /// `FormatException` (a number that does not parse where DbxEdit calls `Parse`).
    Format,
    /// `OverflowException` (a number out of range, a negative array size).
    Overflow,
    /// `OutOfMemoryException` (a damaged count asking for more than any file has).
    OutOfMemory,
}

/// A failure: its kind and the exception's message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// What .NET exception DbxEdit would raise.
    pub kind: FaultKind,
    /// The exception's message.
    pub message: String,
}

/// The result type of DbxEdit's commands.
pub type Result<T, E = Fault> = std::result::Result<T, E>;

impl Fault {
    /// A fault of `kind` with `message`.
    pub fn new(kind: FaultKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// DbxEdit's own refusal (`InvalidDataException`).
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(FaultKind::InvalidData, message)
    }

    /// A read past an array.
    #[must_use]
    pub fn index() -> Self {
        Self::new(FaultKind::IndexOutOfRange, "Index was outside the bounds of the array.")
    }

    /// An index past a list.
    #[must_use]
    pub fn list_index() -> Self {
        Self::new(
            FaultKind::ArgumentOutOfRange,
            "Index was out of range. Must be non-negative and less than the size of the collection.\nParameter name: index",
        )
    }

    /// A copy past the end of an array.
    #[must_use]
    pub fn array_copy() -> Self {
        Self::new(
            FaultKind::Argument,
            "Destination array was not long enough. Check destIndex and length, and the array's lower bounds.",
        )
    }

    /// An array of negative size.
    #[must_use]
    pub fn overflow() -> Self {
        Self::new(FaultKind::Overflow, "Arithmetic operation resulted in an overflow.")
    }

    /// A missing object.
    #[must_use]
    pub fn null() -> Self {
        Self::new(
            FaultKind::NullReference,
            "Object reference not set to an instance of an object.",
        )
    }

    /// The .NET type name of the exception.
    #[must_use]
    pub fn type_name(&self) -> &'static str {
        match self.kind {
            FaultKind::InvalidData => "InvalidDataException",
            FaultKind::IndexOutOfRange => "IndexOutOfRangeException",
            FaultKind::ArgumentOutOfRange => "ArgumentOutOfRangeException",
            FaultKind::Argument => "ArgumentException",
            FaultKind::FileNotFound => "FileNotFoundException",
            FaultKind::DirectoryNotFound => "DirectoryNotFoundException",
            FaultKind::Sharing | FaultKind::Io => "IOException",
            FaultKind::UnauthorizedAccess => "UnauthorizedAccessException",
            FaultKind::NullReference => "NullReferenceException",
            FaultKind::Format => "FormatException",
            FaultKind::Overflow => "OverflowException",
            FaultKind::OutOfMemory => "OutOfMemoryException",
        }
    }

    /// Whether `import` reports it itself ("import failed: ...", exit code 1): `InvalidDataException`,
    /// `XmlException`, `IOException` (and its subclasses) and `UnauthorizedAccessException`.
    #[must_use]
    pub fn caught_by_import(&self) -> bool {
        matches!(
            self.kind,
            FaultKind::InvalidData
                | FaultKind::FileNotFound
                | FaultKind::DirectoryNotFound
                | FaultKind::Sharing
                | FaultKind::Io
                | FaultKind::UnauthorizedAccess
        )
    }

    /// `ToolLog.Explain`: the cause and what to do, in words.
    #[must_use]
    pub fn explain(&self) -> (String, &'static str) {
        let m = &self.message;
        match self.kind {
            FaultKind::Sharing => (
                format!("A file is in use by another program: {m}"),
                "Close the program using it (the game, an editor) and try again.",
            ),
            FaultKind::UnauthorizedAccess => (
                format!("A file or folder could not be written or read: {m}"),
                "Check it is not read-only, or choose another folder.",
            ),
            FaultKind::FileNotFound | FaultKind::DirectoryNotFound => {
                (format!("A file the tool needs is missing: {m}"), "Check the path.")
            }
            FaultKind::InvalidData | FaultKind::ArgumentOutOfRange | FaultKind::IndexOutOfRange => (
                format!("The file is not the kind the tool reads, or it is damaged: {m}"),
                "Check it is the right file; unpack it again from the game if it is damaged.",
            ),
            FaultKind::OutOfMemory => (
                "The PC ran out of memory.".to_owned(),
                "Close other programs, or work on fewer files at once.",
            ),
            _ => (
                format!("{}: {m}", self.type_name()),
                "If it happens again, send the log file to the tool's author.",
            ),
        }
    }

    /// An I/O error on `path` as .NET words it (`Could not find file 'C:\full\path'.`).
    #[must_use]
    pub fn io(e: &io::Error, path: &Path) -> Self {
        let full = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
        let full = full.display();
        match (e.kind(), e.raw_os_error()) {
            (io::ErrorKind::NotFound, Some(3)) => Self::new(
                FaultKind::DirectoryNotFound,
                format!("Could not find a part of the path '{full}'."),
            ),
            (io::ErrorKind::NotFound, _) => {
                Self::new(FaultKind::FileNotFound, format!("Could not find file '{full}'."))
            }
            (_, Some(32 | 33)) => Self::new(
                FaultKind::Sharing,
                format!("The process cannot access the file '{full}' because it is being used by another process."),
            ),
            (io::ErrorKind::PermissionDenied, _) | (_, Some(5)) => Self::new(
                FaultKind::UnauthorizedAccess,
                format!("Access to the path '{full}' is denied."),
            ),
            _ => Self::new(FaultKind::Io, format!("{e}: '{full}'")),
        }
    }
}

impl fmt::Display for Fault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Fault {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explanations_follow_the_kind() {
        assert!(Fault::index().explain().0.starts_with("The file is not the kind"));
        assert!(Fault::null().explain().0.starts_with("NullReferenceException: "));
        assert!(Fault::new(FaultKind::FileNotFound, "x").explain().0.contains("missing"));
        assert!(Fault::invalid("x").caught_by_import());
        assert!(!Fault::index().caught_by_import());
        let e = io::Error::from(io::ErrorKind::NotFound);
        assert_eq!(Fault::io(&e, Path::new("a")).kind, FaultKind::FileNotFound);
        let e = io::Error::from_raw_os_error(32);
        assert_eq!(Fault::io(&e, Path::new("a")).kind, FaultKind::Sharing);
        assert_eq!(Fault::overflow().type_name(), "OverflowException");
        assert_eq!(Fault::array_copy().to_string(), Fault::array_copy().message);
    }
}
