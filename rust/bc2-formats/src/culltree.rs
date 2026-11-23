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

use crate::dbx::edit::{EditDbx, NodeId, Parts};

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
    /// A string index of the element at this byte is past 32 bits ([`document`] / [`stream`] only: the suite's one
    /// document keeps 32-bit indexes; the Python reader fails on such an index where it reads it).
    #[error("a string index of the element at byte {0} is past 32 bits (not a cull tree or level partition)")]
    Wide(usize),
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

/// A varint of the element stream ([`crate::varint::read_u64`]: past 64 bits is an error).
///
/// # Errors
/// The data ends inside it, or it is past 64 bits.
pub fn varint(data: &[u8], pos: usize) -> Result<(u64, usize), ElementError> {
    crate::varint::read_u64(data, pos).map_err(|e| match e {
        crate::varint::VarintError::Truncated(s) => ElementError::Truncated(s),
        crate::varint::VarintError::TooLong(s) => ElementError::Varint(s),
    })
}

/// One element of the stream as [`walk`] reads it: where it starts, its tag, head byte and attributes, and by kind
/// a container's extra byte or the value's bytes in the file.
#[derive(Debug, Clone)]
pub struct Step<'a> {
    /// Where the element (its tag) starts.
    pub at: usize,
    /// The tag (a string table index).
    pub tag: u64,
    /// The head byte: kind << 4 | attribute count.
    pub head: u8,
    /// Its attributes (key, value).
    pub attrs: &'a [(u64, u64)],
    /// A container's extra byte (0 for the others).
    pub extra: u8,
    /// The value's bytes in the file (empty for containers; see [`Element::value`]).
    pub value: Range<usize>,
}

impl Step<'_> {
    /// The kind (head byte's high nibble).
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.head >> 4
    }
}

/// What [`walk`] hands each element to: `open` for a container (its children follow, then `close`), `value` for the
/// others; `depth` is the number of containers open around it. An `Err` stops the walk with it.
pub trait Visit {
    /// A container opens.
    ///
    /// # Errors
    /// The visitor's own refusal.
    fn open(&mut self, step: &Step, depth: usize) -> Result<(), ElementError>;
    /// A value element.
    ///
    /// # Errors
    /// The visitor's own refusal.
    fn value(&mut self, step: &Step, depth: usize) -> Result<(), ElementError>;
    /// The container opened at `depth` closes.
    fn close(&mut self, depth: usize);
}

/// The element stream from `start` (24 + the string area's size) walked as `culltree.Doc` reads it, each element
/// handed to `v` (round PR1b-2-rest2: the one walk under [`elements`], [`document`] and [`stream`]). The trailer's
/// start.
///
/// # Errors
/// [`ElementError`] where the retired Python reader raised, a varint past 64 bits, or the visitor's.
pub fn walk(data: &[u8], start: usize, v: &mut impl Visit) -> Result<usize, ElementError> {
    let mut depth = 0usize;
    let mut attrs: Vec<(u64, u64)> = Vec::new();
    let mut pos = start;
    while pos < data.len() {
        let at = pos;
        let (tag, p) = varint(data, pos)?;
        if tag == 0 {
            if depth == 0 {
                break;
            }
            depth -= 1;
            v.close(depth);
            pos = p;
            continue;
        }
        pos = p;
        let head = *data.get(pos).ok_or(ElementError::Truncated(pos))?;
        pos += 1;
        attrs.clear();
        for _ in 0..head & 0xf {
            let (k, p) = varint(data, pos)?;
            let (val, p) = varint(data, p)?;
            attrs.push((k, val));
            pos = p;
        }
        let mut step = Step {
            at,
            tag,
            head,
            attrs: &attrs,
            extra: 0,
            value: 0..0,
        };
        match head >> 4 {
            CONTAINER => {
                step.extra = *data.get(pos).ok_or(ElementError::Truncated(pos))?;
                pos += 1;
                v.open(&step, depth)?;
                depth += 1;
                continue;
            }
            TEXT => {
                let (_, p) = varint(data, pos)?;
                step.value = pos..p;
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
                step.value = s..pos;
            }
            _ => {
                step.value = pos..(pos + 2).min(data.len());
                pos += 2;
            }
        }
        v.value(&step, depth)?;
    }
    if depth > 0 {
        return Err(ElementError::Unclosed);
    }
    Ok(pos.min(data.len()))
}

/// Reads the element stream from `start` (24 + the string area's size) as `culltree.Doc` does.
///
/// # Errors
/// [`ElementError`] where the retired Python reader raised, and for a varint past 64 bits.
pub fn elements(data: &[u8], start: usize) -> Result<Elements, ElementError> {
    struct Flat {
        out: Elements,
        open: Vec<usize>,
    }
    impl Flat {
        fn push(&mut self, step: &Step) {
            let a0 = self.out.attrs.len();
            self.out.attrs.extend_from_slice(step.attrs);
            self.out.elements.push(Element {
                tag: step.tag,
                head: step.head,
                attrs: a0..self.out.attrs.len(),
                extra: step.extra,
                value: step.value.clone(),
                end: None,
            });
        }
    }
    impl Visit for Flat {
        fn open(&mut self, step: &Step, _: usize) -> Result<(), ElementError> {
            self.open.push(self.out.elements.len());
            self.push(step);
            if let Some(e) = self.out.elements.last_mut() {
                e.end = Some(usize::MAX);
            }
            Ok(())
        }
        fn value(&mut self, step: &Step, _: usize) -> Result<(), ElementError> {
            self.push(step);
            Ok(())
        }
        fn close(&mut self, _: usize) {
            let n = self.out.elements.len();
            if let Some(c) = self.open.pop()
                && let Some(e) = self.out.elements.get_mut(c)
            {
                e.end = Some(n);
            }
        }
    }
    let mut f = Flat {
        out: Elements::default(),
        open: Vec::new(),
    };
    f.out.end = walk(data, start, &mut f)?;
    Ok(f.out)
}

/// A string index (a tag or an attribute's key or value) as the one document keeps it: 32 bits.
fn index32(i: u64, at: usize) -> Result<u32, ElementError> {
    u32::try_from(i).map_err(|_| ElementError::Wide(at))
}

/// The parts of a stream element for the one document (string indexes as stored, not checked: a cull tree reader
/// checks them where it reads them, as `culltree.Doc` does).
fn parts(step: &Step, container: bool) -> Result<(Parts, u32), ElementError> {
    let mut attrs = Vec::with_capacity(step.attrs.len());
    for &(k, val) in step.attrs {
        attrs.push((index32(k, step.at)?, index32(val, step.at)?));
    }
    Ok((
        Parts {
            tag: index32(step.tag, step.at)?,
            kind: step.kind(),
            attrs,
            cbyte: step.extra,
            payload: Vec::new(),
            children: container.then(Vec::new),
        },
        0,
    ))
}

/// A text value's string index for [`EditDbx::text`] (the bytes as stored are its payload either way).
fn text_index(data: &[u8], step: &Step) -> u32 {
    if step.kind() != TEXT {
        return 0;
    }
    varint(data, step.value.start).map_or(u32::MAX, |(t, _)| u32::try_from(t).unwrap_or(u32::MAX))
}

/// A binary `.dbx` read under `culltree.Doc`'s rules into the suite's one document ([`EditDbx`], round
/// PR1b-2-rest2: culltool's own codec retired): the element stream as [`elements`] reads it, string indexes kept as
/// stored and not checked (the reader checks those it reads), every value's bytes a range of the file, the trailer
/// kept; the string table is the caller's (`strings`, read from the header with its rules) and `start` the end of
/// its area. Written back ([`EditDbx::write`]) it gives the file byte for byte.
///
/// # Errors
/// [`ElementError`] as [`elements`], and [`ElementError::Wide`] for a string index past 32 bits.
pub fn document(data: &[u8], header: u32, strings: Vec<Vec<u8>>, start: usize) -> Result<EditDbx, ElementError> {
    struct Doc<'d> {
        data: &'d [u8],
        d: EditDbx,
        open: Vec<NodeId>,
        roots: Vec<NodeId>,
    }
    impl Doc<'_> {
        fn attach(&mut self, id: NodeId) {
            match self.open.last() {
                Some(&p) => {
                    if let Ok(c) = self.d.children_mut(p) {
                        c.push(id);
                    }
                }
                None => self.roots.push(id),
            }
        }
    }
    impl Visit for Doc<'_> {
        fn open(&mut self, step: &Step, _: usize) -> Result<(), ElementError> {
            let (p, _) = parts(step, true)?;
            let id = self.d.push_read(p, (0, 0), 0);
            self.attach(id);
            self.open.push(id);
            Ok(())
        }
        fn value(&mut self, step: &Step, _: usize) -> Result<(), ElementError> {
            let (p, _) = parts(step, false)?;
            let r = &step.value;
            let id = self.d.push_read(
                p,
                (r.start as u32, (r.end - r.start) as u32),
                text_index(self.data, step),
            );
            self.attach(id);
            Ok(())
        }
        fn close(&mut self, _: usize) {
            self.open.pop();
        }
    }
    let area = (24.min(data.len()), start.clamp(24.min(data.len()), data.len()));
    let mut v = Doc {
        data,
        d: EditDbx::from_file(data, header, strings, area),
        open: Vec::new(),
        roots: Vec::new(),
    };
    let end = walk(data, start, &mut v)?;
    let mut d = v.d;
    *d.roots_mut() = v.roots;
    d.set_trailing(data.get(end..).unwrap_or_default().to_vec());
    Ok(d)
}

/// What [`stream`] hands out, each element on a scratch document that holds only it (and is cleared after): each
/// child of the first root once it is closed, and - for the containers among a root child's own children that
/// `streamed` picks - each of their items instead (the picked container is kept with no children).
pub trait Sink {
    /// A child of the first root (its index among them), with its subtree.
    fn child(&mut self, index: usize, d: &EditDbx, n: NodeId);
    /// Whether a root child's child container's items are streamed (asked when it opens: its attributes only).
    fn streamed(&mut self, d: &EditDbx, n: NodeId) -> bool;
    /// An item (with its subtree) of a picked container: the root child's index, the container's index among that
    /// child's children.
    fn item(&mut self, index: usize, slot: usize, d: &EditDbx, n: NodeId);
}

/// [`document`]'s read with the memory of one root child at a time (round CT1's streamed readers, on the one
/// document since round PR1b-2-rest2): the first root's children are handed to `sink` one by one and dropped;
/// whatever else the file holds is read (and checked) but not kept. Whether the file has a root.
///
/// # Errors
/// As [`document`], at the same bytes.
#[allow(
    clippy::too_many_lines,
    reason = "the streamer and its visitor in one piece, as `document`'s"
)]
pub fn stream(
    data: &[u8],
    header: u32,
    strings: Vec<Vec<u8>>,
    start: usize,
    sink: &mut dyn Sink,
) -> Result<bool, ElementError> {
    struct Streamer<'d, 's> {
        data: &'d [u8],
        d: EditDbx,
        sink: &'s mut dyn Sink,
        /// the open containers kept (inside a root child): (id, its index among its root child's children when it
        /// is a picked container)
        open: Vec<(NodeId, Option<usize>)>,
        /// whether the open root is the first
        in_root: bool,
        roots: usize,
        index: usize,
    }
    impl Streamer<'_, '_> {
        /// Whether an element at `depth` is kept: inside the first root.
        fn kept(&self, depth: usize) -> bool {
            self.in_root && depth > 0
        }
        /// A finished element (closed, or a value): handed out when it is a root child or an item, dropped after.
        fn done(&mut self, id: NodeId, depth: usize) {
            if depth == 1 {
                self.sink.child(self.index, &self.d, id);
                self.index += 1;
                self.d.truncate_nodes(0);
                return;
            }
            // an item: its picked container is the innermost open one, at depth - 1
            if let Some(&(_, Some(slot))) = self.open.last()
                && self.open.len() == depth - 1
            {
                self.sink.item(self.index, slot, &self.d, id);
                self.d.truncate_nodes(id as usize);
            }
        }
        fn add(&mut self, id: NodeId, depth: usize) {
            // a root child has no parent; an item is not attached to its picked container
            if depth > 1
                && let Some(&(p, slot)) = self.open.last()
                && slot.is_none()
                && let Ok(c) = self.d.children_mut(p)
            {
                c.push(id);
            }
        }
    }
    impl Visit for Streamer<'_, '_> {
        fn open(&mut self, step: &Step, depth: usize) -> Result<(), ElementError> {
            if depth == 0 {
                self.roots += 1;
                self.in_root = self.roots == 1;
                return Ok(());
            }
            if !self.kept(depth) {
                return Ok(());
            }
            let (p, _) = parts(step, true)?;
            let id = self.d.push_read(p, (0, 0), 0);
            // a root child's child container: picked or not, its index the root child's children before it
            let slot = if depth == 2 {
                let before = self
                    .open
                    .last()
                    .and_then(|&(r, _)| self.d.children(r))
                    .map_or(0, <[NodeId]>::len);
                self.sink.streamed(&self.d, id).then_some(before)
            } else {
                None
            };
            self.add(id, depth);
            self.open.push((id, slot));
            Ok(())
        }
        fn value(&mut self, step: &Step, depth: usize) -> Result<(), ElementError> {
            if depth == 0 {
                self.roots += 1;
                return Ok(());
            }
            if !self.kept(depth) {
                return Ok(());
            }
            let (p, _) = parts(step, false)?;
            let r = &step.value;
            let id = self.d.push_read(
                p,
                (r.start as u32, (r.end - r.start) as u32),
                text_index(self.data, step),
            );
            self.add(id, depth);
            self.done(id, depth);
            Ok(())
        }
        fn close(&mut self, depth: usize) {
            if depth == 0 {
                self.in_root = false;
                return;
            }
            if !self.in_root {
                return;
            }
            if let Some((id, _)) = self.open.pop() {
                self.done(id, depth);
            }
        }
    }
    let area = (24.min(data.len()), start.clamp(24.min(data.len()), data.len()));
    let mut v = Streamer {
        data,
        d: EditDbx::from_file(data, header, strings, area),
        sink,
        open: Vec::new(),
        in_root: false,
        roots: 0,
        index: 0,
    };
    walk(data, start, &mut v)?;
    Ok(v.roots > 0)
}

/// The files `glob.glob(os.path.join(dir, '**', '*.dbx'), recursive=True)` lists, as paths relative to `dir`
/// (Python joins them to `dir`; the caller sorts): every name ending in `.dbx` (any case) in `dir` and in every
/// folder below it, names starting with `.` left out (files and folders), folders followed through links, folders
/// that cannot be listed skipped. Like glob, a folder named `*.dbx` is listed too.
#[must_use]
pub fn dbx_files(dir: &Path) -> Vec<PathBuf> {
    dbx_files_stat(dir).into_iter().map(|x| x.0).collect()
}

/// A listed file's size and modification time (nanoseconds since 1970) as its folder's listing gives them.
pub type ListedStat = (u64, i128);

/// [`dbx_files`] with each file's [`ListedStat`] from the folder listing itself (round CT1: no stat call per file;
/// None for a link, or when the listing has no time).
#[must_use]
pub fn dbx_files_stat(dir: &Path) -> Vec<(PathBuf, Option<ListedStat>)> {
    fn listed(e: &std::fs::DirEntry) -> Option<ListedStat> {
        let m = e.metadata().ok().filter(|m| !m.file_type().is_symlink())?;
        let ns = match m.modified().ok()?.duration_since(std::time::UNIX_EPOCH) {
            Ok(d) => i128::from(d.as_secs()) * 1_000_000_000 + i128::from(d.subsec_nanos()),
            Err(e) => -(i128::from(e.duration().as_secs()) * 1_000_000_000 + i128::from(e.duration().subsec_nanos())),
        };
        Some((m.len(), ns))
    }
    fn walk(dir: &Path, rel: &Path) -> Vec<(PathBuf, Option<ListedStat>)> {
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
                files.push((r.clone(), listed(&e)));
            }
            // glob's is_dir follows links: a link to a folder is walked (the entry's own type otherwise: no stat
            // per entry, round CT1)
            let is_dir = match e.file_type() {
                Ok(t) if !t.is_symlink() => t.is_dir(),
                _ => e.path().is_dir(),
            };
            if is_dir {
                subdirs.push(r);
            }
        }
        let below: Vec<Vec<(PathBuf, Option<ListedStat>)>> = subdirs.par_iter().map(|r| walk(dir, r)).collect();
        files.extend(below.into_iter().flatten());
        files
    }
    walk(dir, Path::new(""))
}

/// For each file: does it contain `needle`? `None` for a file that cannot be read (the caller reads it itself and
/// gets the error). Files are read on all cores, each in 256 KiB pieces (round CT1: not whole), and only until the
/// needle is found.
#[must_use]
pub fn contains<P: AsRef<Path> + Sync>(paths: &[P], needle: &[u8]) -> Vec<Option<bool>> {
    paths
        .par_iter()
        .map(|p| file_contains(p.as_ref(), needle).ok())
        .collect()
}

/// Does the file hold `needle`? Read piece by piece, the last `needle.len() - 1` bytes of a piece kept before the
/// next one so a needle across two pieces is found.
#[allow(
    clippy::indexing_slicing,
    reason = "have stays within the buffer: at most its length after a read, at most keep after the copy"
)]
fn file_contains(path: &Path, needle: &[u8]) -> std::io::Result<bool> {
    use std::io::Read;
    const PIECE: usize = 256 * 1024;
    let mut f = std::fs::File::open(path)?;
    if needle.is_empty() {
        return Ok(true);
    }
    let finder = memchr::memmem::Finder::new(needle);
    let keep = needle.len() - 1;
    let mut buf = vec![0u8; PIECE + keep];
    let mut have = 0;
    loop {
        let n = match f.read(&mut buf[have..]) {
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        if n == 0 {
            return Ok(false);
        }
        have += n;
        if finder.find(&buf[..have]).is_some() {
            return Ok(true);
        }
        let tail = have.min(keep);
        buf.copy_within(have - tail..have, 0);
        have = tail;
    }
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
    #[allow(clippy::items_after_statements, reason = "the sink next to its use")]
    fn document_writes_back() {
        // a header with three strings ("r", "instance", "x"), then a root with an instance holding a picked
        // container of two items and a text, a second root, and a trailer
        let mut data = b"{binary}".to_vec();
        let area = [0u32.to_be_bytes(), 2u32.to_be_bytes(), 11u32.to_be_bytes()].concat();
        let strings = b"r\0instance\0x\0".to_vec();
        let size = (area.len() + strings.len()) as u32;
        for w in [24 + size, 0x1234, size, 3] {
            data.extend(w.to_be_bytes());
        }
        data.extend(&area);
        data.extend(&strings);
        let start = data.len();
        data.extend([
            2, 0xA0, 5, // root
            1, 0xA1, 2, 2, 0, // instance (tag 1), attribute (2, 2)
            2, 0xA0, 0, // a container (picked)
            2, 0x70, 1, 2, 7, 8, // item: numbers
            2, 0x50, 3, 4, // item: a value
            0, // the container closes
            2, 0x20, 2, // a text
            0, 0, // the instance, the root
            2, 0x50, 9, 9, // a second root (a value)
            0, 0xEE, // end, trailer
        ]);
        let strs = vec![b"r".to_vec(), b"instance".to_vec(), b"x".to_vec()];
        let d = document(&data, 0x1234, strs.clone(), start).unwrap();
        assert_eq!(d.write().unwrap(), data);
        assert_eq!(d.roots().len(), 2);
        struct Got(Vec<(usize, usize)>, Vec<(usize, usize, usize)>);
        impl Sink for Got {
            fn child(&mut self, i: usize, d: &EditDbx, n: NodeId) {
                self.0.push((i, d.children(n).map_or(0, <[NodeId]>::len)));
            }
            fn streamed(&mut self, _: &EditDbx, _: NodeId) -> bool {
                true
            }
            fn item(&mut self, i: usize, slot: usize, d: &EditDbx, n: NodeId) {
                self.1.push((i, slot, d.payload(n).len()));
            }
        }
        let mut g = Got(Vec::new(), Vec::new());
        assert!(stream(&data, 0x1234, strs.clone(), start, &mut g).unwrap());
        assert_eq!(g.0, vec![(0, 2)]); // the instance: the picked container (no items kept) and the text
        assert_eq!(g.1, vec![(0, 0, 4), (0, 0, 2)]);
        let mut wide = data.clone();
        wide.splice(start + 3..start + 4, [0x80, 0x80, 0x80, 0x80, 0x10]); // tag 2^32
        assert_eq!(
            document(&wide, 0, strs, start).map(|_| ()),
            Err(ElementError::Wide(start + 3))
        );
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
