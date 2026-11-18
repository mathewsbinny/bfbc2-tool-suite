//! Static cull trees and the level partitions they belong to: a port of the hot parts of
//! `bfbc2-culltree/culltree.py` (its README, "Static cull trees", describes the format).
//!
//! `culltree.py` reads `.dbx` files with a minimal binary DBX codec of its own (`Doc`) that keeps every byte, so an
//! unchanged file is written back exactly. Its element stream is what [`elements`] reads: each element a varint tag
//! (0 closes the open container; at the top level it ends the stream), a head byte (kind in the high nibble,
//! attribute count in the low one), varint attribute pairs, then by kind a container's extra byte (its children
//! follow), a text value's varint, a numbers value's varint count, varint size and count x size bytes, or two
//! bytes. The bytes after the stream are the file's trailer.
//!
//! The add command's references (`ref_pairs`) are found by reading every `.dbx` under the given folders for
//! `ExclusiveStaticCullTree`: [`dbx_files`] lists them as `glob('<dir>/**/*.dbx', recursive=True)` does and
//! [`contains`] searches them on all cores.
//!
//! | Python (`culltree.py`) | here |
//! |---|---|
//! | `Doc.__init__`'s element loop (retired 2026-11-18, round F1a) | [`elements`] (every stream the Python read, read the same; else an error saying why) |
//! | `glob.glob(os.path.join(d, '**', '*.dbx'), recursive=True)` | [`dbx_files`] |
//! | `needle in open(p, 'rb').read()` per file | [`contains`] |
//!
//! ```
//! use bc2_formats::culltree;
//!
//! // a container (kind 0xA) with one attribute and a numbers child (kind 7: 1 value of 4 bytes), then the end
//! let data = [1, 0xA1, 2, 3, 0, 4, 0x70, 1, 4, 0, 0, 0, 9, 0, 0];
//! let read = culltree::elements(&data, 0)?;
//! assert_eq!(read.elements.len(), 2);
//! assert_eq!(read.attrs, vec![(2, 3)]);
//! assert_eq!(&data[read.elements[1].value.clone()], &[1, 4, 0, 0, 0, 9]);
//! assert_eq!(read.end, 14); // the trailer: the last 0
//! # Ok::<(), culltree::ElementError>(())
//! ```

use std::ops::Range;
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use thiserror::Error;

/// A container element's kind (the head byte's high nibble).
pub const CONTAINER: u8 = 0xA;
/// A text element's kind.
pub const TEXT: u8 = 2;
/// A numbers element's kind.
pub const NUMBERS: u8 = 7;

/// Why the element stream does not read; `culltree.py` raises `CullTreeError` with the message (the retired Python
/// reader raised `IndexError` for the first, read the second as a big integer, and raised the third's message).
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ElementError {
    /// The data ends inside an element.
    #[error("truncated file (it ends inside the element at byte {0})")]
    Truncated(usize),
    /// A varint is longer than 64 bits (no cull tree or level partition holds one).
    #[error("a number at byte {0} is longer than 64 bits (not a cull tree or level partition)")]
    Varint(usize),
    /// Containers are still open at the end.
    #[error("truncated file (unclosed elements)")]
    Unclosed,
}

/// One element of the stream, in file order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Element {
    /// The tag (a string table index).
    pub tag: u64,
    /// The head byte: kind << 4 | attribute count.
    pub head: u8,
    /// Its attributes: a range of [`Elements::attrs`].
    pub attrs: Range<usize>,
    /// A container's extra byte (0 for the others).
    pub extra: u8,
    /// The value's bytes in the file (empty for containers): a text's varint, a numbers element's count, size and
    /// values, or the two bytes of any other kind (fewer at the end of the file).
    pub value: Range<usize>,
    /// A container: the index one past its last descendant. `None` for the other kinds.
    pub end: Option<usize>,
}

impl Element {
    /// The kind (head byte's high nibble).
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.head >> 4
    }
}

/// The element stream of a file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Elements {
    /// Every element in file order (a container before its children).
    pub elements: Vec<Element>,
    /// Every attribute (key, value), by element.
    pub attrs: Vec<(u64, u64)>,
    /// Where the stream ended: the trailer starts here (at most the data's length).
    pub end: usize,
}

fn varint(data: &[u8], mut pos: usize) -> Result<(u64, usize), ElementError> {
    let start = pos;
    let mut v = 0u64;
    let mut shift = 0u32;
    loop {
        let b = *data.get(pos).ok_or(ElementError::Truncated(start))?;
        pos += 1;
        let low = u64::from(b & 0x7f);
        if shift > 63 || (shift > 0 && low >> (64 - shift) != 0) {
            return Err(ElementError::Varint(start));
        }
        v |= low << shift;
        if b < 0x80 {
            return Ok((v, pos));
        }
        shift += 7;
    }
}

/// Reads the element stream from `start` (24 + the string area's size) as `culltree.Doc` does.
///
/// # Errors
/// [`ElementError`] where the retired Python reader raised, and for a varint past 64 bits.
pub fn elements(data: &[u8], start: usize) -> Result<Elements, ElementError> {
    let mut out = Elements::default();
    let mut open: Vec<usize> = Vec::new();
    let mut pos = start;
    while pos < data.len() {
        let (tag, p) = varint(data, pos)?;
        if tag == 0 {
            let Some(c) = open.pop() else { break };
            let n = out.elements.len();
            if let Some(e) = out.elements.get_mut(c) {
                e.end = Some(n);
            }
            pos = p;
            continue;
        }
        pos = p;
        let head = *data.get(pos).ok_or(ElementError::Truncated(pos))?;
        pos += 1;
        let a0 = out.attrs.len();
        for _ in 0..head & 0xf {
            let (k, p) = varint(data, pos)?;
            let (v, p) = varint(data, p)?;
            out.attrs.push((k, v));
            pos = p;
        }
        let mut e = Element {
            tag,
            head,
            attrs: a0..out.attrs.len(),
            ..Element::default()
        };
        match head >> 4 {
            CONTAINER => {
                e.extra = *data.get(pos).ok_or(ElementError::Truncated(pos))?;
                pos += 1;
                e.end = Some(usize::MAX);
                open.push(out.elements.len());
            }
            TEXT => {
                let (_, p) = varint(data, pos)?;
                e.value = pos..p;
                pos = p;
            }
            NUMBERS => {
                let s = pos;
                let (n, p) = varint(data, pos)?;
                let (size, p) = varint(data, p)?;
                let end = n
                    .checked_mul(size)
                    .and_then(|b| (p as u64).checked_add(b))
                    .filter(|&e| e <= data.len() as u64)
                    .ok_or(ElementError::Truncated(s))?;
                pos = end as usize;
                e.value = s..pos;
            }
            _ => {
                e.value = pos..(pos + 2).min(data.len());
                pos += 2;
            }
        }
        out.elements.push(e);
    }
    if !open.is_empty() {
        return Err(ElementError::Unclosed);
    }
    out.end = pos.min(data.len());
    Ok(out)
}

/// The files `glob.glob(os.path.join(dir, '**', '*.dbx'), recursive=True)` lists, as paths relative to `dir`
/// (Python joins them to `dir`; the caller sorts): every name ending in `.dbx` (any case) in `dir` and in every
/// folder below it, names starting with `.` left out (files and folders), folders followed through links, folders
/// that cannot be listed skipped. Like glob, a folder named `*.dbx` is listed too.
#[must_use]
pub fn dbx_files(dir: &Path) -> Vec<PathBuf> {
    fn walk(dir: &Path, rel: &Path) -> Vec<PathBuf> {
        let Ok(rd) = std::fs::read_dir(dir.join(rel)) else {
            return Vec::new();
        };
        let mut files = Vec::new();
        let mut subdirs = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name();
            let s = name.to_string_lossy();
            if s.starts_with('.') {
                continue;
            }
            let r = rel.join(&name);
            if s.len() >= 4 && s.get(s.len() - 4..).is_some_and(|x| x.eq_ignore_ascii_case(".dbx")) {
                files.push(r.clone());
            }
            // glob's is_dir follows links: a link to a folder is walked
            if e.path().is_dir() {
                subdirs.push(r);
            }
        }
        let below: Vec<Vec<PathBuf>> = subdirs.par_iter().map(|r| walk(dir, r)).collect();
        files.extend(below.into_iter().flatten());
        files
    }
    walk(dir, Path::new(""))
}

/// For each file: does it contain `needle`? `None` for a file that cannot be read (the caller reads it itself and
/// gets the error). Files are read on all cores.
#[must_use]
pub fn contains<P: AsRef<Path> + Sync>(paths: &[P], needle: &[u8]) -> Vec<Option<bool>> {
    paths
        .par_iter()
        .map(|p| {
            let data = std::fs::read(p).ok()?;
            Some(needle.is_empty() || data.windows(needle.len()).any(|w| w == needle))
        })
        .collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_stream() {
        // root container (tag 1, 2 attrs), a text (tag 3), a container with a numbers child, a 2-byte value, close
        let data = [
            9, 9, // (before start)
            1, 0xA2, 1, 2, 3, 4, 7, // root: attrs (1,2) (3,4), extra 7
            3, 0x20, 0x85, 0x01, // text, varint value 0x85 0x01
            5, 0xA0, 0, // container, no attrs
            6, 0x70, 2, 1, 0xaa, 0xbb, // numbers: 2 x 1 byte
            0,    // close the inner container
            8, 0x50, 1, 2, // other kind: 2 bytes
            0, // close the root
            0, 0xff, // end of stream, trailer
        ];
        let r = elements(&data, 2).unwrap();
        assert_eq!(r.elements.len(), 5);
        assert_eq!(r.attrs, vec![(1, 2), (3, 4)]);
        assert_eq!(r.elements[0].extra, 7);
        assert_eq!(r.elements[0].end, Some(5));
        assert_eq!(r.elements[2].end, Some(4));
        assert_eq!(&data[r.elements[1].value.clone()], &[0x85, 0x01]);
        assert_eq!(&data[r.elements[3].value.clone()], &[2, 1, 0xaa, 0xbb]);
        assert_eq!(&data[r.elements[4].value.clone()], &[1, 2]);
        assert_eq!(r.elements[4].kind(), 5);
        assert_eq!(r.end, data.len() - 2);
    }

    #[test]
    fn edges() {
        // a two-byte value cut by the end of the file: one byte kept, the stream ends
        let r = elements(&[1, 0x50, 9], 0).unwrap();
        assert_eq!(r.elements[0].value, 2..3);
        assert_eq!(r.end, 3);
        assert_eq!(elements(&[], 0).unwrap().elements.len(), 0);
        assert_eq!(elements(&[1, 0xA0, 0], 0), Err(ElementError::Unclosed));
        assert_eq!(elements(&[1], 0), Err(ElementError::Truncated(1)));
        assert_eq!(elements(&[1, 0xA0], 0), Err(ElementError::Truncated(2)));
        assert_eq!(elements(&[1, 0x70, 2, 4, 0], 0), Err(ElementError::Truncated(2)));
        assert_eq!(
            elements(
                &[1, 0x70, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 2],
                0
            ),
            Err(ElementError::Truncated(2))
        );
        assert_eq!(elements(&[0x80], 0), Err(ElementError::Truncated(0)));
        let long = [0xffu8; 11];
        assert_eq!(elements(&long, 0), Err(ElementError::Varint(0)));
        let mut big = vec![0xff; 9];
        big.push(0x01); // bit 63
        big.extend([0x50, 0, 0]);
        assert_eq!(elements(&big, 0).unwrap().elements[0].tag, u64::MAX);
        let mut over = vec![0xff; 9];
        over.push(0x02); // bit 64
        assert_eq!(elements(&over, 0), Err(ElementError::Varint(0)));
    }

    #[test]
    fn files() {
        let dir = std::env::temp_dir().join(format!("bc2_culltree_{}", std::process::id()));
        let sub = dir.join("Sub");
        std::fs::create_dir_all(sub.join(".hidden")).unwrap();
        std::fs::create_dir_all(dir.join("x.DBX")).unwrap();
        for (p, d) in [
            (dir.join("a.dbx"), b"..ExclusiveStaticCullTree..".as_slice()),
            (sub.join("b.Dbx"), b"no"),
            (sub.join(".c.dbx"), b"hidden"),
            (sub.join(".hidden").join("d.dbx"), b"hidden folder"),
            (sub.join("e.txt"), b"other"),
        ] {
            std::fs::write(p, d).unwrap();
        }
        let mut got = dbx_files(&dir);
        got.sort();
        assert_eq!(
            got,
            vec![
                PathBuf::from("Sub").join("b.Dbx"),
                PathBuf::from("a.dbx"),
                PathBuf::from("x.DBX")
            ]
        );
        let paths: Vec<PathBuf> = got.iter().map(|r| dir.join(r)).collect();
        assert_eq!(
            contains(&paths, b"ExclusiveStaticCullTree"),
            vec![Some(false), Some(true), None]
        );
        assert_eq!(contains(&paths[..1], b""), vec![Some(true)]);
        assert_eq!(dbx_files(&dir.join("missing")).len(), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
