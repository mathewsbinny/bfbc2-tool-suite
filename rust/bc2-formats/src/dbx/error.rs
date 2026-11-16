//! The one error type of the DBX reader, writer and value formatting.

use thiserror::Error;

/// Why a `.dbx` file (or one of its values) could not be read or written.
///
/// Every damaged input gives one of these, never a panic. The Python reference raises `ValueError` for all of them
/// (`DeletedEntry`, a `ValueError` subclass, for [`DbxError::Deleted`]); `bc2-py` maps them the same way.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum DbxError {
    /// A zero-byte file: a bundle's *deleted* marker, not a broken file.
    #[error("empty file: a *deleted* bundle entry (older FbrbTool unpacks wrote them as files)")]
    Deleted,
    /// The file does not start with `{binary}` (or is 24 bytes or shorter).
    #[error("not a binary .dbx file")]
    NotBinary,
    /// A read ran past the end of the data.
    #[error("damaged .dbx file (read past the end at byte {offset})")]
    UnexpectedEnd {
        /// The offset of the first byte that was not there.
        offset: usize,
    },
    /// The string table is longer than the file.
    #[error("damaged .dbx file (a string table of {count} strings runs past the end)")]
    StringTablePastEnd {
        /// The number of strings the header gives.
        count: u64,
    },
    /// A string of the table has no terminating zero byte (only the strict reader, [`super::parse`], refuses it).
    #[error("string without end")]
    StringWithoutEnd,
    /// An element names a string the table does not have.
    #[error("damaged .dbx file (string {index} of a table of {count})")]
    StringIndex {
        /// The index read from the file (saturated at `u64::MAX` for a varint too long to hold).
        index: u64,
        /// The number of strings in the table.
        count: usize,
    },
    /// A numbers element's values run past the end of the file.
    #[error("truncated file: {count} numbers of {size} bytes run past the end")]
    NumbersPastEnd {
        /// The number of values.
        count: u64,
        /// The size of one value in bytes.
        size: u64,
    },
    /// A numbers element has more values than any file can hold (the Python reference would not finish).
    #[error("damaged .dbx file ({count} numbers)")]
    TooManyNumbers {
        /// The number of values.
        count: u64,
    },
    /// A 4- or 8-byte number with fewer bytes than its size.
    #[error("damaged .dbx file (a {size}-byte number of {len} bytes)")]
    NumberCutShort {
        /// The number's size.
        size: u64,
        /// The bytes there were.
        len: usize,
    },
    /// A bool / byte value cut short by the end of the file.
    #[error("truncated file: value cut short")]
    ValueCutShort,
    /// A bool / byte value element whose raw bytes are not two bytes.
    #[error("a {len}-byte bool / byte value (expected 2 bytes)")]
    ValueSize {
        /// The bytes there were.
        len: usize,
    },
    /// The file ends inside one or more containers.
    #[error("truncated file: unclosed container(s)")]
    Unclosed,
    /// The writer was given an element with more than 15 attributes (the format's limit).
    #[error("{tag}: {count} attributes (15 at most)")]
    TooManyAttributes {
        /// The element's tag.
        tag: String,
        /// Its number of attributes.
        count: usize,
    },
    /// A file of 4 GiB or more: the compact tree ([`super::DbxTree`]) keeps positions in 32 bits.
    #[error("a .dbx file of 4 GiB or more")]
    TooLarge,
    /// The writer was given an element whose kind does not fit the format's 4 bits, or a string table too large.
    #[error("cannot write: {0}")]
    Unwritable(String),
}

/// The result type of this module.
pub type Result<T, E = DbxError> = std::result::Result<T, E>;
