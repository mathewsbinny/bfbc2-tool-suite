//! The binary reader: a `.dbx` file as an element tree (FORMAT.md 1-2), as `dbx.parse`.

use std::path::Path;

use super::error::{DbxError, Result};
use super::model::{DbxFile, StrId};
use super::tree::parse_tree;

/// A string index read from the file, checked against the table.
pub(crate) fn string_id(index: u64, count: usize) -> Result<StrId> {
    usize::try_from(index)
        .ok()
        .filter(|&i| i < count)
        .and_then(|i| StrId::try_from(i).ok())
        .ok_or(DbxError::StringIndex { index, count })
}

/// A binary `.dbx` file as an owned element tree, as `dbx.parse`: [`parse_tree`] and
/// [`DbxTree::to_file`](super::DbxTree::to_file). To read without editing, [`parse_tree`] alone allocates far less.
///
/// # Errors
/// [`DbxError::Deleted`] for an empty file, [`DbxError::NotBinary`] without the `{binary}` header, and another
/// variant for a damaged file (a read past the end, an index past the string table, an unclosed container ...).
///
/// ```
/// use bc2_formats::dbx::{self, DbxError};
///
/// assert_eq!(dbx::parse(b""), Err(DbxError::Deleted));
/// assert_eq!(dbx::parse(b"<?xml version=\"1.0\"?>"), Err(DbxError::NotBinary));
/// ```
pub fn parse(data: &[u8]) -> Result<DbxFile> {
    Ok(parse_tree(data)?.to_file())
}

/// A `.dbx` file read from disk and parsed, as `dbx.read`.
///
/// # Errors
/// The file's I/O error, or [`parse`]'s.
pub fn read(path: impl AsRef<Path>) -> std::result::Result<DbxFile, ReadError> {
    let data = std::fs::read(path)?;
    Ok(parse(&data)?)
}

/// Why [`read`] failed: the file could not be read, or did not parse.
#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    /// The file could not be read.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The file did not parse.
    #[error(transparent)]
    Dbx(#[from] DbxError),
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::dbx::Body;
    use crate::dbx::strings::MAGIC;

    /// A file of `strings` and body bytes, the header as the format has it.
    pub(crate) fn file(strings: &[&str], body: &[u8]) -> Vec<u8> {
        let mut blob = Vec::new();
        let mut offsets = Vec::new();
        for s in strings {
            offsets.push(blob.len() as u32);
            blob.extend(s.as_bytes());
            blob.push(0);
        }
        let area = (4 * strings.len() + blob.len()) as u32;
        let mut d = MAGIC.to_vec();
        for w in [24 + area, 0, area, strings.len() as u32] {
            d.extend(w.to_be_bytes());
        }
        for o in offsets {
            d.extend(o.to_be_bytes());
        }
        d.extend(blob);
        d.extend(body);
        d
    }

    const S: &[&str] = &["", "partition", "guid", "g", "field", "name", "x"];

    #[test]
    fn tree() {
        // partition guid=g { field name=x "g"; field(kind 6) name=x [0 1]; field(numbers) name=x 1 x 4 bytes }
        let body = [
            1, 0xA1, 2, 3, 0, // partition, container, 1 attr, flag
            4, 0x21, 5, 6, 3, // text field
            4, 0x61, 5, 6, 0, 1, // bool
            4, 0x71, 5, 6, 1, 4, 0x3F, 0x80, 0, 0, // numbers
            0, // close partition
        ];
        let f = parse(&file(S, &body)).unwrap();
        assert_eq!(f.roots.len(), 1);
        let p = &f.roots[0];
        assert_eq!(f.tag(p), "partition");
        assert_eq!(f.attr(p, "guid"), Some("g"));
        let kids = p.children();
        assert_eq!(kids.len(), 3);
        assert_eq!(f.text(&kids[0]), "g");
        assert_eq!(kids[1].body, Body::Scalar([0, 1]));
        assert_eq!(kids[1].kind, 6);
        assert_eq!(kids[2].raw(), &[1, 4, 0x3F, 0x80, 0, 0]);
    }

    #[test]
    fn nested_containers_keep_order() {
        let body = [1, 0xA0, 0, 4, 0xA0, 0, 4, 0x20, 3, 0, 4, 0x20, 6, 0];
        let f = parse(&file(S, &body)).unwrap();
        let kids = f.roots[0].children();
        assert_eq!(kids.len(), 2);
        assert_eq!(kids[0].children().len(), 1);
        assert_eq!(f.text(&kids[1]), "x");
    }

    #[test]
    fn stops_at_a_zero_tag_with_nothing_open() {
        let f = parse(&file(S, &[4, 0x20, 3, 0, 4, 0x20, 3])).unwrap();
        assert_eq!(f.roots.len(), 1);
    }

    #[test]
    fn errors() {
        assert_eq!(parse(&file(S, &[1, 0xA0, 0])), Err(DbxError::Unclosed));
        assert_eq!(
            parse(&file(S, &[9, 0x20, 3])),
            Err(DbxError::StringIndex { index: 9, count: 7 })
        );
        assert_eq!(parse(&file(S, &[4, 0x60, 1])), Err(DbxError::ValueCutShort));
        assert_eq!(
            parse(&file(S, &[4, 0x70, 2, 4, 0])),
            Err(DbxError::NumbersPastEnd { count: 2, size: 4 })
        );
        assert!(matches!(parse(&file(S, &[4])), Err(DbxError::UnexpectedEnd { .. })));
        assert!(matches!(
            parse(&file(&["a"], &[])[..29]),
            Err(DbxError::StringWithoutEnd)
        ));
        assert_eq!(parse(b"{binary}"), Err(DbxError::NotBinary));
        assert_eq!(parse(b""), Err(DbxError::Deleted));
    }

    #[test]
    fn huge_counts_are_errors() {
        let mut body = vec![4, 0x70];
        body.extend([0xFF; 10]);
        body.extend([0x01, 4]);
        assert!(matches!(parse(&file(S, &body)), Err(DbxError::NumbersPastEnd { .. })));
    }

    #[test]
    fn read_from_disk() {
        let dir = std::env::temp_dir().join(format!("bc2-formats-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.dbx");
        std::fs::write(&p, file(S, &[4, 0x20, 3])).unwrap();
        assert_eq!(read(&p).unwrap().roots.len(), 1);
        assert!(matches!(read(dir.join("missing.dbx")), Err(ReadError::Io(_))));
        std::fs::write(&p, b"").unwrap();
        assert!(matches!(read(&p), Err(ReadError::Dbx(DbxError::Deleted))));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
