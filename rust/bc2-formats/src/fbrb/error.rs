//! The one error type of the FBRB reader.

use thiserror::Error;

/// Why a bundle (or an entry of it) could not be read.
///
/// Every damaged input gives one of these, never a panic. The Python reference (`core/bfbc2core/fbrb.py`) raises
/// `ValueError` for a damaged bundle (its fallback turns the `struct.error`, `IndexError`, `EOFError`, `zlib.error`
/// and `gzip.BadGzipFile` of its standard-library readers into one), `KeyError` for [`FbrbError::MissingEntry`] and
/// `OSError` for [`FbrbError::Io`]; `bc2-py` maps them the same way.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum FbrbError {
    /// The file could not be opened or read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The file does not start with `FbRB` (or is shorter than 8 bytes).
    #[error("is not an FbRB bundle")]
    NotBundle,
    /// A gzip stream (the header, or a zipped payload) is damaged: a bad magic, method, flags, header or data
    /// checksum, length, or deflate data. The text says which, as Python's `gzip` / `zlib` would.
    #[error("damaged bundle (gzip: {0})")]
    Gzip(String),
    /// A gzip stream ends before its end-of-stream marker and checksum.
    #[error("damaged bundle (gzip: the compressed data ends before the end-of-stream marker)")]
    GzipEnd,
    /// The decompressed header is too short for a field, an entry table or the zipped byte.
    #[error("damaged bundle (the header ends at byte {len}, a field needs byte {needed})")]
    HeaderPastEnd {
        /// The decompressed header's length.
        len: usize,
        /// The offset one past the last byte the field needs.
        needed: u64,
    },
    /// An entry's name or type has no terminating zero byte in the header.
    #[error("damaged bundle (a string at header byte {offset} has no end)")]
    StringWithoutEnd {
        /// The string's offset in the decompressed header.
        offset: u64,
    },
    /// An entry asked for is not in the bundle (Python's `KeyError`; *deleted* markers are not entries).
    #[error("entry {index} of the names asked for is not an entry of the bundle")]
    MissingEntry {
        /// The index into the names given.
        index: usize,
    },
    /// The payload ends before the data of an entry asked for.
    #[error("the payload ends before entry data (damaged bundle)")]
    PayloadEnds,
}

/// The result type of this module.
pub type Result<T, E = FbrbError> = std::result::Result<T, E>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages() {
        assert_eq!(FbrbError::NotBundle.to_string(), "is not an FbRB bundle");
        assert!(
            FbrbError::PayloadEnds
                .to_string()
                .contains("payload ends before entry data")
        );
        assert!(
            FbrbError::Gzip("CRC check failed".into())
                .to_string()
                .contains("CRC check failed")
        );
        assert!(FbrbError::GzipEnd.to_string().contains("end-of-stream"));
        assert!(
            FbrbError::HeaderPastEnd { len: 3, needed: 8 }
                .to_string()
                .contains("byte 8")
        );
        assert!(FbrbError::StringWithoutEnd { offset: 9 }.to_string().contains("byte 9"));
        assert!(FbrbError::MissingEntry { index: 2 }.to_string().contains("entry 2"));
        let io: FbrbError = std::io::Error::new(std::io::ErrorKind::NotFound, "gone").into();
        assert_eq!(io.to_string(), "gone");
    }
}
