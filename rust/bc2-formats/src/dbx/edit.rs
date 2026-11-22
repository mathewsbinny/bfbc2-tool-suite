//! A binary `.dbx` as an editable tree that writes back byte for byte: the port of `bfbc2-stockmp-ai/dbxbin.py`
//! (round E4a), the stock-MP AI tool's model of a level partition.
//!
//! Unlike [`DbxFile`](super::DbxFile) (whose [`write`](super::write) makes a fresh string table), an [`EditDbx`]
//! keeps the file's own string *indexes* on every element, its string table byte for byte while no string is added
//! (duplicates and their area included), the header's word at offset 12, each container's flag byte, and whatever
//! follows the tree; new strings are appended at the end of the table, as DbxEdit does. An unchanged file comes back
//! exactly. Elements live in one arena and are named by [`NodeId`]s (handles), so a caller - Rust, or Python through
//! `bc2-py` - edits a level without a tree of its own.
//!
//! Strings are kept as their bytes: `dbxbin` decodes them with `surrogateescape`, which maps bytes to text one to
//! one, so comparing bytes is comparing its strings. Where `dbxbin` would fail later (a string index outside the
//! table, a value cut short by the end of the file, a varint past 32 bits) [`EditDbx::parse`] refuses at once: the
//! caller then leaves the file to the Python reference, which fails or not by its own rules.
//!
//! It is also the one editable document of the suite (round PR1b-2: `DbxEdit.cs`' own model, the GR4 port's
//! `EditFile`, merged into it). DbxEdit's reader ([`crate::dbxedit::read`]) builds the same arena under DbxEdit's
//! rules instead of refusing: string indexes kept as the file gives them (checked only where a command reads them),
//! varints read as C#'s 32-bit `int`, numbers sized by C#'s `int` product, a file that is not binary kept as its bytes
//! ([`EditDbx::is_binary`]). Either way every value keeps its bytes as stored - a text value's string index varint
//! too, until the value is set - and a string replaced in place ([`EditDbx::set_string`]) writes the table anew.
//!
//! ```
//! use bc2_formats::dbx::edit::EditDbx;
//! use bc2_formats::dbx::{self, DbxFile, Element};
//!
//! let mut f = DbxFile::default();
//! let name = Element::text(&mut f, "field", &[("name", "Name")], "Levels/MP_005");
//! let inst = Element::container(&mut f, "instance", &[("guid", "AB"), ("type", "X.LevelData")], vec![name]);
//! let part = Element::container(&mut f, "partition", &[], vec![inst]);
//! f.roots.push(part);
//! let bytes = dbx::write(&f)?;
//!
//! let mut d = EditDbx::parse(&bytes).expect("a binary file");
//! assert_eq!(d.write().expect("written"), bytes);
//! let inst = d.of_type(b".LevelData")[0];
//! let name = d.child(inst, b"Name").expect("the field");
//! d.set_text(name, b"Levels/MP_002").expect("a text field");
//! assert_eq!(d.text(name), Some(&b"Levels/MP_002"[..]));
//! assert_eq!(dbx::parse(&d.write().expect("written"))?.strings.last().map(String::as_str), Some("Levels/MP_002"));
//! # Ok::<(), dbx::DbxError>(())
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use super::model::{CONTAINER, NUMBERS, TEXT};
use super::strings::MAGIC;
use super::writer::put_varint;

/// An element of an [`EditDbx`]: its index in the tree's arena.
pub type NodeId = u32;

/// Why a file was not read or an edit not made: the Python reference's `ValueError` / `IndexError` cases (the
/// caller leaves the work to the reference, which raises its own).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct EditError(pub String);

impl EditError {
    fn new(why: &str) -> Self {
        Self(why.to_owned())
    }
}

/// The result type of this module.
pub type EditResult<T> = Result<T, EditError>;

/// A value's bytes: a range of the parsed file, or bytes of its own (an edit, or a clone from another file).
#[derive(Debug, Clone, PartialEq, Eq)]
enum Raw {
    At(u32, u32),
    Own(Vec<u8>),
}

/// One element (`dbxbin.Element`). A value's bytes as stored are in `raw`: a text value's string index varint (its
/// index is `text`), a numbers element's count and size varints and values, any other value's two bytes.
#[derive(Debug, Clone)]
struct ENode {
    tag: u32,
    kind: u8,
    attrs: Vec<(u32, u32)>,
    text: u32,
    raw: Raw,
    cbyte: u8,
    children: Option<Vec<NodeId>>,
}

/// What a new element is made of ([`EditDbx::push`]): string indexes as stored, the value's bytes as stored.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Parts {
    /// The tag's string index.
    pub tag: u32,
    /// The kind (the high four bits of the head byte).
    pub kind: u8,
    /// `(name, value)` string indexes.
    pub attrs: Vec<(u32, u32)>,
    /// A container's flag byte.
    pub cbyte: u8,
    /// The value's bytes as stored ([`EditDbx::payload`]; a text value's index varint); empty for a container.
    pub payload: Vec<u8>,
    /// A container's children (`None`: a value).
    pub children: Option<Vec<NodeId>>,
}

pub use crate::varint::read_i32 as varint_i32;

/// An element and its subtree copied out of a file, its strings as bytes (what [`EditDbx::copy_from`] inserts).
#[derive(Debug, Clone)]
struct Snapshot {
    tag: Vec<u8>,
    kind: u8,
    attrs: Vec<(Vec<u8>, Vec<u8>)>,
    text: Option<Vec<u8>>,
    raw: Vec<u8>,
    cbyte: u8,
    children: Option<Vec<Snapshot>>,
}

/// The elements a [`Nodes`] chunk holds.
const CHUNK: usize = 1024;

/// The element arena in shared chunks (round PR1b-0, audit AU5 ALG-4): a clone of the file shares every chunk, and an
/// edit copies only the chunk of the element it changes (`Arc::make_mut`), so an editor checkpoint's copy of a level
/// partition costs the chunks edited since, not the whole tree.
#[derive(Debug, Clone, Default)]
struct Nodes {
    chunks: Vec<Arc<Vec<ENode>>>,
    len: usize,
}

impl Nodes {
    fn with_capacity(n: usize) -> Self {
        Self {
            chunks: Vec::with_capacity(n.div_ceil(CHUNK)),
            len: 0,
        }
    }

    fn len(&self) -> usize {
        self.len
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn get(&self, i: usize) -> Option<&ENode> {
        self.chunks.get(i / CHUNK)?.get(i % CHUNK)
    }

    fn get_mut(&mut self, i: usize) -> Option<&mut ENode> {
        Arc::make_mut(self.chunks.get_mut(i / CHUNK)?).get_mut(i % CHUNK)
    }

    fn push(&mut self, e: ENode) {
        match self.chunks.last_mut() {
            Some(c) if c.len() < CHUNK => Arc::make_mut(c).push(e),
            _ => {
                let mut c = Vec::with_capacity(CHUNK);
                c.push(e);
                self.chunks.push(Arc::new(c));
            }
        }
        self.len += 1;
    }
}

/// A binary `.dbx` as `dbxbin.Dbx` holds it (see the module documentation).
///
/// A clone is cheap (round PR1b-0): the file's bytes, the string table and the elements are shared until an edit
/// changes them.
#[derive(Debug, Clone, Default)]
pub struct EditDbx {
    /// The file as read: never edited, shared by the clones.
    data: Arc<[u8]>,
    /// The header's word at offset 12, kept as it is.
    pub header: u32,
    strings: Arc<Vec<Vec<u8>>>,
    index: Arc<HashMap<Vec<u8>, u32>>,
    /// The original string area (offsets and strings) as a range of `data`, written back while no string was added.
    area: Option<(usize, usize)>,
    n_orig: usize,
    /// A string was replaced in place ([`EditDbx::set_string`]): the area is written anew.
    strings_changed: bool,
    /// The index no longer names each string's first entry (a string was replaced); rebuilt by the next intern.
    index_stale: bool,
    /// Not a binary file (DbxEdit's reader only): `data` is the file, there is no tree.
    not_binary: bool,
    nodes: Nodes,
    roots: Vec<NodeId>,
    trailing: Vec<u8>,
}

fn be_u32(data: &[u8], at: usize) -> EditResult<u32> {
    data.get(at..at + 4)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_be_bytes)
        .ok_or_else(|| EditError::new("the header runs past the end"))
}

/// `dbxbin._varint`: a varint at `pos`; refused past the end, or past 32 bits (a value over 32 bits, or a seventh
/// byte, which the shortest form never needs and a write would not give back).
fn varint(data: &[u8], pos: &mut usize) -> EditResult<u32> {
    let start = *pos;
    let past_32 = || EditError::new("a varint past 32 bits");
    match crate::varint::read_saturating(data, start) {
        Some((v, end)) if end - start <= 6 => {
            *pos = end;
            u32::try_from(v).map_err(|_| past_32())
        }
        Some(_) => Err(past_32()),
        None if data.len().saturating_sub(start) >= 7 => Err(past_32()),
        None => Err(EditError::new("a varint runs past the end")),
    }
}

/// `(count, size, values)` of a numbers element's raw bytes, as `Element.numbers` slices them (a value short of
/// `size` bytes where the bytes end; `None` where a varint does).
#[must_use]
pub fn numbers_of(raw: &[u8]) -> Option<(u32, Vec<&[u8]>)> {
    let mut p = 0;
    let count = varint(raw, &mut p).ok()?;
    let size = varint(raw, &mut p).ok()?;
    let mut vals = Vec::with_capacity(count.min(1 << 16) as usize);
    for i in 0..u64::from(count) {
        let a = (p as u64 + i * u64::from(size)).min(raw.len() as u64) as usize;
        let b = (p as u64 + (i + 1) * u64::from(size)).min(raw.len() as u64) as usize;
        vals.push(raw.get(a..b).unwrap_or_default());
    }
    Some((size, vals))
}

/// `dbxbin._varint_bytes`.
#[must_use]
pub fn varint_bytes(v: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(5);
    put_varint(&mut out, u64::from(v));
    out
}

impl EditDbx {
    /// An empty file (`dbxbin.Dbx()`): no strings, no elements; [`EditDbx::write`] builds its string table.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `dbxbin.parse(data)` (see the module documentation for what is refused).
    ///
    /// # Errors
    /// Not a binary file, or a file `dbxbin` would fail on or not give back byte for byte.
    #[expect(clippy::too_many_lines, reason = "dbxbin.parse's one loop, kept as it reads")]
    pub fn parse(data: &[u8]) -> EditResult<Self> {
        if !(data.len() > 24 && data.starts_with(MAGIC)) {
            return Err(EditError::new("not a binary .dbx file"));
        }
        let header = be_u32(data, 12)?;
        let area = be_u32(data, 16)? as usize;
        let count = be_u32(data, 20)? as usize;
        let base = 24 + 4 * count;
        if base > data.len() {
            return Err(EditError::new("the string offsets run past the end"));
        }
        let mut strings = Vec::with_capacity(count);
        let mut index = HashMap::with_capacity(count);
        for i in 0..count {
            let off = be_u32(data, 24 + 4 * i)? as usize;
            let start = base + off;
            let tail = data
                .get(start..)
                .ok_or_else(|| EditError::new("a string without its end"))?;
            let len = tail
                .iter()
                .position(|&b| b == 0)
                .ok_or_else(|| EditError::new("a string without its end"))?;
            let s = tail.get(..len).unwrap_or_default().to_vec();
            index.entry(s.clone()).or_insert(i as u32);
            strings.push(s);
        }
        let n = data.len();
        if 24 + area > n {
            return Err(EditError::new("the string area runs past the end"));
        }
        let mut d = Self {
            data: Arc::from(data),
            header,
            strings: Arc::new(strings),
            index: Arc::new(index),
            area: Some((24, 24 + area)),
            n_orig: count,
            strings_changed: false,
            index_stale: false,
            not_binary: false,
            nodes: Nodes::with_capacity((n - 24 - area) / 5),
            roots: Vec::new(),
            trailing: Vec::new(),
        };
        let check = |id: u32| -> EditResult<u32> {
            if (id as usize) < count {
                Ok(id)
            } else {
                Err(EditError::new("a string index outside the table"))
            }
        };
        let mut pos = 24 + area;
        let mut stack: Vec<NodeId> = Vec::new();
        while pos < n {
            let tag = varint(data, &mut pos)?;
            if tag == 0 {
                if stack.pop().is_none() {
                    d.trailing = data.get(pos - 1..).unwrap_or_default().to_vec();
                    break;
                }
                continue;
            }
            let tag = check(tag)?;
            let head = *data.get(pos).ok_or_else(|| EditError::new("an element cut short"))?;
            pos += 1;
            let kind = head >> 4;
            let mut attrs = Vec::with_capacity(usize::from(head & 0xF));
            for _ in 0..(head & 0xF) {
                let k = check(varint(data, &mut pos)?)?;
                let v = check(varint(data, &mut pos)?)?;
                attrs.push((k, v));
            }
            let mut e = ENode {
                tag,
                kind,
                attrs,
                text: 0,
                raw: Raw::Own(Vec::new()),
                cbyte: 0,
                children: None,
            };
            if kind == CONTAINER {
                e.cbyte = *data.get(pos).ok_or_else(|| EditError::new("a container cut short"))?;
                pos += 1;
                e.children = Some(Vec::new());
            } else if kind == TEXT {
                let start = pos;
                e.text = check(varint(data, &mut pos)?)?;
                e.raw = Raw::At(start as u32, (pos - start) as u32);
            } else if kind == NUMBERS {
                let start = pos;
                let c = varint(data, &mut pos)?;
                let size = varint(data, &mut pos)?;
                let end = (pos as u64) + u64::from(c) * u64::from(size);
                if end > n as u64 {
                    return Err(EditError::new("numbers run past the end"));
                }
                pos = end as usize;
                e.raw = Raw::At(start as u32, (pos - start) as u32);
            } else {
                if pos + 2 > n {
                    return Err(EditError::new("a value cut short"));
                }
                e.raw = Raw::At(pos as u32, 2);
                pos += 2;
            }
            let id = d.nodes.len() as NodeId;
            d.nodes.push(e);
            match stack.last() {
                Some(&p) => {
                    if let Some(Some(c)) = d.nodes.get_mut(p as usize).map(|n| n.children.as_mut()) {
                        c.push(id);
                    }
                }
                None => d.roots.push(id),
            }
            if kind == CONTAINER {
                stack.push(id);
            }
        }
        if !stack.is_empty() {
            return Err(EditError::new("the element tree is not closed"));
        }
        Ok(d)
    }

    // -- strings

    /// `Dbx.intern`: the first index of `s`, appended to the table when it is new.
    pub fn intern(&mut self, s: &[u8]) -> u32 {
        if self.index_stale {
            let mut index = HashMap::with_capacity(self.strings.len());
            for (i, t) in self.strings.iter().enumerate() {
                index.entry(t.clone()).or_insert(i as u32);
            }
            self.index = Arc::new(index);
            self.index_stale = false;
        }
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.strings.len() as u32;
        Arc::make_mut(&mut self.strings).push(s.to_vec());
        Arc::make_mut(&mut self.index).insert(s.to_vec(), i);
        i
    }

    /// The string table (duplicates included, new strings at the end).
    #[must_use]
    pub fn strings(&self) -> &[Vec<u8>] {
        &self.strings
    }

    /// String `id` (empty for an index the table does not have).
    #[must_use]
    pub fn s(&self, id: u32) -> &[u8] {
        self.strings.get(id as usize).map_or(&[][..], Vec::as_slice)
    }

    /// String `id`, or `None` for an index the table does not have (DbxEdit's reader keeps such indexes).
    #[must_use]
    pub fn string(&self, id: u32) -> Option<&[u8]> {
        self.strings.get(id as usize).map(Vec::as_slice)
    }

    /// Entry `i` of the table replaced by `s` (DbxEdit's `replace` / `replaceprefix`): the string area is then written
    /// anew, and interning finds the first entry of the table as it is now. An index past the table does nothing.
    pub fn set_string(&mut self, i: usize, s: Vec<u8>) {
        if let Some(slot) = Arc::make_mut(&mut self.strings).get_mut(i) {
            *slot = s;
            self.strings_changed = true;
            self.index_stale = true;
        }
    }

    /// Whether the string table will be written anew (a string was added or replaced).
    #[must_use]
    pub fn strings_changed(&self) -> bool {
        self.strings_changed || self.area.is_none() || self.strings.len() != self.n_orig
    }

    // -- elements

    fn node(&self, n: NodeId) -> Option<&ENode> {
        self.nodes.get(n as usize)
    }

    fn node_mut(&mut self, n: NodeId) -> EditResult<&mut ENode> {
        self.nodes
            .get_mut(n as usize)
            .ok_or_else(|| EditError::new("no such element"))
    }

    /// The number of elements in the arena (every element ever made, attached or not).
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the arena is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The root elements (the partition first).
    #[must_use]
    pub fn roots(&self) -> &[NodeId] {
        &self.roots
    }

    /// `Element.tag`.
    #[must_use]
    pub fn tag(&self, n: NodeId) -> &[u8] {
        self.node(n).map_or(&[][..], |e| self.s(e.tag))
    }

    /// `Element.kind`.
    #[must_use]
    pub fn kind(&self, n: NodeId) -> u8 {
        self.node(n).map_or(0, |e| e.kind)
    }

    /// `Element.attrs` as strings: `(name, value)` pairs in order.
    pub fn attributes(&self, n: NodeId) -> impl Iterator<Item = (&[u8], &[u8])> {
        self.node(n)
            .map_or(&[][..], |e| e.attrs.as_slice())
            .iter()
            .map(|&(k, v)| (self.s(k), self.s(v)))
    }

    /// `Element.attr(name)`: the value of the first attribute named `name`.
    #[must_use]
    pub fn attr(&self, n: NodeId, name: &[u8]) -> Option<&[u8]> {
        self.attributes(n).find(|(k, _)| *k == name).map(|(_, v)| v)
    }

    /// `Element.text` (`None` for any kind but text).
    #[must_use]
    pub fn text(&self, n: NodeId) -> Option<&[u8]> {
        self.node(n).filter(|e| e.kind == TEXT).map(|e| self.s(e.text))
    }

    /// `Element.raw`: a numbers element's count and size varints and values, any other value's two bytes; empty for
    /// containers and text.
    #[must_use]
    pub fn raw(&self, n: NodeId) -> &[u8] {
        match self.node(n) {
            Some(e) if e.kind == TEXT => &[],
            _ => self.payload(n),
        }
    }

    /// A value's bytes as stored (DbxEdit's `Element.Payload`): [`EditDbx::raw`], and for a text value its string
    /// index's varint (as the file stored it, until the value is set). Empty for a container.
    #[must_use]
    pub fn payload(&self, n: NodeId) -> &[u8] {
        match self.node(n).map(|e| &e.raw) {
            Some(Raw::At(s, l)) => self.data.get(*s as usize..(*s + *l) as usize).unwrap_or_default(),
            Some(Raw::Own(v)) => v,
            None => &[],
        }
    }

    /// A text value's string index as stored (C#'s `int` bits under DbxEdit's reader; `None` for any kind but text).
    #[must_use]
    pub fn text_id(&self, n: NodeId) -> Option<u32> {
        self.node(n).filter(|e| e.kind == TEXT).map(|e| e.text)
    }

    /// The tag's string index as stored.
    #[must_use]
    pub fn tag_id(&self, n: NodeId) -> u32 {
        self.node(n).map_or(0, |e| e.tag)
    }

    /// The `(name, value)` string indexes as stored.
    #[must_use]
    pub fn attr_ids(&self, n: NodeId) -> &[(u32, u32)] {
        self.node(n).map_or(&[][..], |e| e.attrs.as_slice())
    }

    /// The `(name, value)` string indexes, to change in place.
    ///
    /// # Errors
    /// No such element.
    pub fn attr_ids_mut(&mut self, n: NodeId) -> EditResult<&mut Vec<(u32, u32)>> {
        Ok(&mut self.node_mut(n)?.attrs)
    }

    /// A container's flag byte (0 for anything else).
    #[must_use]
    pub fn container_byte(&self, n: NodeId) -> u8 {
        self.node(n).map_or(0, |e| e.cbyte)
    }

    /// Whether it has children (a container's list, empty or not).
    #[must_use]
    pub fn is_container(&self, n: NodeId) -> bool {
        self.children(n).is_some()
    }

    /// A value's stored bytes set as they are ([`EditDbx::payload`]); for a text value the bytes are its index's
    /// varint, read as C#'s `int` (index 0 where the varint is cut short; the bytes are written as given).
    ///
    /// # Errors
    /// No such element.
    pub fn set_payload(&mut self, n: NodeId, payload: Vec<u8>) -> EditResult<()> {
        let e = self.node_mut(n)?;
        if e.kind == TEXT {
            e.text = varint_i32(&payload, &mut 0).unwrap_or(0) as u32;
        }
        e.raw = Raw::Own(payload);
        Ok(())
    }

    /// A new element of `parts` (string indexes as given, not checked), not attached anywhere yet.
    pub fn push(&mut self, parts: Parts) -> NodeId {
        let text = if parts.kind == TEXT {
            varint_i32(&parts.payload, &mut 0).unwrap_or(0) as u32
        } else {
            0
        };
        let id = self.nodes.len() as NodeId;
        self.nodes.push(ENode {
            tag: parts.tag,
            kind: parts.kind,
            attrs: parts.attrs,
            text,
            raw: Raw::Own(parts.payload),
            cbyte: parts.cbyte,
            children: parts.children,
        });
        id
    }

    /// An element read from the file (DbxEdit's reader; see [`EditDbx::from_file`]): `parts` without a payload, the
    /// value's stored bytes `value` (start, length) a range of the file, `text` a text value's index.
    pub(crate) fn push_read(&mut self, parts: Parts, value: (u32, u32), text: u32) -> NodeId {
        let id = self.nodes.len() as NodeId;
        self.nodes.push(ENode {
            tag: parts.tag,
            kind: parts.kind,
            attrs: parts.attrs,
            text,
            raw: if parts.children.is_some() {
                Raw::Own(Vec::new())
            } else {
                Raw::At(value.0, value.1)
            },
            cbyte: parts.cbyte,
            children: parts.children,
        });
        id
    }

    /// A container's children set (DbxEdit's reader, once they are read).
    pub(crate) fn set_children(&mut self, n: NodeId, children: Vec<NodeId>) {
        if let Some(e) = self.nodes.get_mut(n as usize) {
            e.children = Some(children);
        }
    }

    /// The bytes after the tree (its closing 0 and whatever follows).
    #[must_use]
    pub fn trailing(&self) -> &[u8] {
        &self.trailing
    }

    /// Whether the file was binary (always, but for a file DbxEdit's reader kept as it was: [`EditDbx::not_binary`]).
    #[must_use]
    pub fn is_binary(&self) -> bool {
        !self.not_binary
    }

    /// The bytes the document was read from (empty for a new one).
    #[must_use]
    pub fn original(&self) -> &[u8] {
        &self.data
    }

    /// A file that is not a binary `.dbx`, kept as its bytes (DbxEdit's reader: a command on it then fails where
    /// DbxEdit's does). Written, it gives an empty binary file, as DbxEdit's `Serialize` does.
    #[must_use]
    pub fn not_binary(data: &[u8]) -> Self {
        Self {
            data: Arc::from(data),
            not_binary: true,
            ..Self::default()
        }
    }

    /// The document of a binary file whose header and strings DbxEdit's reader has read: `area` the string area's
    /// range of `data` (offsets and strings), written back while no string is added or replaced; the tree is pushed
    /// next ([`EditDbx::push_read`], [`EditDbx::roots_mut`], [`EditDbx::set_trailing`]).
    pub(crate) fn from_file(data: &[u8], header: u32, strings: Vec<Vec<u8>>, area: (usize, usize)) -> Self {
        let mut index = HashMap::with_capacity(strings.len());
        for (i, s) in strings.iter().enumerate() {
            index.entry(s.clone()).or_insert(i as u32);
        }
        Self {
            data: Arc::from(data),
            header,
            n_orig: strings.len(),
            strings: Arc::new(strings),
            index: Arc::new(index),
            area: Some(area),
            nodes: Nodes::with_capacity(data.len().saturating_sub(area.1) / 5),
            ..Self::default()
        }
    }

    /// The bytes after the tree, set.
    pub(crate) fn set_trailing(&mut self, trailing: Vec<u8>) {
        self.trailing = trailing;
    }

    /// `Element.children` (`None` for anything but a container).
    #[must_use]
    pub fn children(&self, n: NodeId) -> Option<&[NodeId]> {
        self.node(n).and_then(|e| e.children.as_deref())
    }

    /// The children of a container as a mutable list (to append, remove or reorder).
    ///
    /// # Errors
    /// Not a container.
    pub fn children_mut(&mut self, n: NodeId) -> EditResult<&mut Vec<NodeId>> {
        self.node_mut(n)?
            .children
            .as_mut()
            .ok_or_else(|| EditError::new("not a container"))
    }

    /// `Element.child(name)`: the first direct child whose attribute `name` is `name`.
    #[must_use]
    pub fn child(&self, n: NodeId, name: &[u8]) -> Option<NodeId> {
        self.children(n)?
            .iter()
            .copied()
            .find(|&c| self.attr(c, b"name") == Some(name))
    }

    /// `Element.walk()`: the element and its subtree in file order.
    #[must_use]
    pub fn walk(&self, n: NodeId) -> Vec<NodeId> {
        let mut out = Vec::new();
        let mut stack = vec![n];
        while let Some(x) = stack.pop() {
            out.push(x);
            if let Some(c) = self.children(x) {
                stack.extend(c.iter().rev());
            }
        }
        out
    }

    /// `Dbx.partition`: the first root.
    #[must_use]
    pub fn partition(&self) -> Option<NodeId> {
        self.roots.first().copied()
    }

    /// `Dbx.instances()`: the partition's `instance` children (none when the partition is missing or not a
    /// container, where `dbxbin` fails).
    #[must_use]
    pub fn instances(&self) -> Vec<NodeId> {
        self.partition()
            .and_then(|p| self.children(p))
            .unwrap_or_default()
            .iter()
            .copied()
            .filter(|&e| self.tag(e) == b"instance")
            .collect()
    }

    /// Whether `dbxbin`'s `instances()` would run (a partition that is a container).
    #[must_use]
    pub fn has_partition(&self) -> bool {
        self.partition().and_then(|p| self.children(p)).is_some()
    }

    /// `Dbx.of_type(suffix)`: the instances whose `type` ends with `suffix`.
    #[must_use]
    pub fn of_type(&self, suffix: &[u8]) -> Vec<NodeId> {
        self.instances()
            .into_iter()
            .filter(|&e| self.attr(e, b"type").unwrap_or_default().ends_with(suffix))
            .collect()
    }

    /// `Dbx.instance(guid)`: the first instance whose `guid` is `guid` (upper-cased by `upper`, as Python's
    /// `str.upper` on both sides).
    #[must_use]
    pub fn instance(&self, guid: &[u8], upper: impl Fn(&[u8]) -> Vec<u8>) -> Option<NodeId> {
        let g = upper(guid);
        self.instances()
            .into_iter()
            .find(|&e| upper(self.attr(e, b"guid").unwrap_or_default()) == g)
    }

    // -- edits (each keeps the element's kind)

    /// `Dbx.set_attr`: the value interned first, then the first attribute of that name changed or a new one added.
    ///
    /// # Errors
    /// No such element.
    pub fn set_attr(&mut self, n: NodeId, name: &[u8], value: &[u8]) -> EditResult<()> {
        let vi = self.intern(value);
        let found = self
            .node(n)
            .ok_or_else(|| EditError::new("no such element"))?
            .attrs
            .iter()
            .position(|&(a, _)| self.s(a) == name);
        if let Some(k) = found {
            if let Some(slot) = self.node_mut(n)?.attrs.get_mut(k) {
                slot.1 = vi;
            }
        } else {
            let ni = self.intern(name);
            self.node_mut(n)?.attrs.push((ni, vi));
        }
        Ok(())
    }

    /// Keeps only the attributes `keep` says yes to, in order.
    ///
    /// # Errors
    /// No such element.
    pub fn retain_attrs(&mut self, n: NodeId, keep: impl Fn(&[u8]) -> bool) -> EditResult<()> {
        let attrs = std::mem::take(&mut self.node_mut(n)?.attrs);
        let kept = attrs.into_iter().filter(|&(k, _)| keep(self.s(k))).collect();
        self.node_mut(n)?.attrs = kept;
        Ok(())
    }

    /// `Dbx.set_text`.
    ///
    /// # Errors
    /// Not a text element.
    pub fn set_text(&mut self, n: NodeId, value: &[u8]) -> EditResult<()> {
        if self.kind(n) != TEXT || self.node(n).is_none() {
            return Err(EditError::new("not text"));
        }
        let t = self.intern(value);
        let e = self.node_mut(n)?;
        e.text = t;
        e.raw = Raw::Own(varint_bytes(t));
        Ok(())
    }

    /// `Dbx.set_bool`: the second byte of a two-byte value set to `value`.
    ///
    /// # Errors
    /// Not a two-byte value.
    pub fn set_bool(&mut self, n: NodeId, value: u8) -> EditResult<()> {
        let k = self.kind(n);
        let raw = self.raw(n);
        if matches!(k, CONTAINER | TEXT | NUMBERS) || raw.len() != 2 || self.node(n).is_none() {
            return Err(EditError::new("not a bool / byte"));
        }
        let first = raw.first().copied().unwrap_or(0);
        self.node_mut(n)?.raw = Raw::Own(vec![first, value]);
        Ok(())
    }

    /// The raw bytes of a non-container element set as they are (a numbers element's varints and values).
    ///
    /// # Errors
    /// No such element.
    pub fn set_raw(&mut self, n: NodeId, raw: Vec<u8>) -> EditResult<()> {
        self.node_mut(n)?.raw = Raw::Own(raw);
        Ok(())
    }

    /// `Dbx.set_numbers`: 4-byte values (each already big-endian), the count free to change.
    ///
    /// # Errors
    /// Not a numbers element, or one holding numbers of another size.
    pub fn set_numbers(&mut self, n: NodeId, values: &[[u8; 4]]) -> EditResult<()> {
        if self.kind(n) != NUMBERS {
            return Err(EditError::new("not numbers"));
        }
        let (size, old) = numbers_of(self.raw(n)).ok_or_else(|| EditError::new("damaged numbers"))?;
        if !old.is_empty() && size != 4 {
            return Err(EditError::new("numbers of another size"));
        }
        let mut raw = varint_bytes(values.len() as u32);
        raw.extend(varint_bytes(4));
        for v in values {
            raw.extend_from_slice(v);
        }
        self.set_raw(n, raw)
    }

    /// `Dbx.set_transform`: a 16-number `LinearTransform`'s four rows of three singles, each row's fourth word kept.
    ///
    /// # Errors
    /// Not a 16 x 4-byte numbers element.
    pub fn set_transform(&mut self, n: NodeId, rows: &[[f32; 3]; 4]) -> EditResult<()> {
        let (size, old) = numbers_of(self.raw(n)).ok_or_else(|| EditError::new("damaged numbers"))?;
        if size != 4 || old.len() != 16 {
            return Err(EditError::new("not a 16-number transform"));
        }
        let mut raw = varint_bytes(16);
        raw.extend(varint_bytes(4));
        for (r, row) in rows.iter().enumerate() {
            for v in row {
                raw.extend_from_slice(&v.to_be_bytes());
            }
            raw.extend_from_slice(old.get(4 * r + 3).copied().unwrap_or_default());
        }
        self.set_raw(n, raw)
    }

    /// `Dbx.set_ref`: the `ref` attribute of a reference field or item.
    ///
    /// # Errors
    /// No `ref` attribute.
    pub fn set_ref(&mut self, n: NodeId, r: &[u8]) -> EditResult<()> {
        if self.attr(n, b"ref").is_none() {
            return Err(EditError::new("no ref attribute"));
        }
        self.set_attr(n, b"ref", r)
    }

    /// Makes a container a text element of `text` (`dbxbin`'s empty array: kind text, no children, no raw bytes).
    ///
    /// # Errors
    /// No such element.
    pub fn make_text(&mut self, n: NodeId, text: &[u8]) -> EditResult<()> {
        let t = self.intern(text);
        let e = self.node_mut(n)?;
        e.kind = TEXT;
        e.children = None;
        e.raw = Raw::Own(varint_bytes(t));
        e.text = t;
        Ok(())
    }

    /// Makes an element an empty container (flag byte 0).
    ///
    /// # Errors
    /// No such element.
    pub fn make_container(&mut self, n: NodeId) -> EditResult<()> {
        let e = self.node_mut(n)?;
        e.kind = CONTAINER;
        e.children = Some(Vec::new());
        e.raw = Raw::Own(Vec::new());
        e.cbyte = 0;
        Ok(())
    }

    fn snapshot(&self, n: NodeId) -> Snapshot {
        let e = self.node(n);
        Snapshot {
            tag: self.tag(n).to_vec(),
            kind: self.kind(n),
            attrs: self.attributes(n).map(|(k, v)| (k.to_vec(), v.to_vec())).collect(),
            text: self.text(n).map(<[u8]>::to_vec),
            raw: self.raw(n).to_vec(),
            cbyte: e.map_or(0, |e| e.cbyte),
            children: self.children(n).map(|c| c.iter().map(|&x| self.snapshot(x)).collect()),
        }
    }

    fn insert(&mut self, s: &Snapshot) -> NodeId {
        // dbxbin.clone's order: the tag, each attribute's name then value, the text, then the children
        let tag = self.intern(&s.tag);
        let attrs = s.attrs.iter().map(|(k, v)| (self.intern(k), self.intern(v))).collect();
        let text = s.text.as_ref().map_or(0, |t| self.intern(t));
        let id = self.nodes.len() as NodeId;
        self.nodes.push(ENode {
            tag,
            kind: s.kind,
            attrs,
            text,
            raw: Raw::Own(if s.kind == TEXT {
                varint_bytes(text)
            } else {
                s.raw.clone()
            }),
            cbyte: s.cbyte,
            children: None,
        });
        if let Some(cs) = &s.children {
            let kids: Vec<NodeId> = cs.iter().map(|c| self.insert(c)).collect();
            if let Some(e) = self.nodes.get_mut(id as usize) {
                e.children = Some(kids);
            }
        }
        id
    }

    /// `Dbx.clone(e, src=src)`: a deep copy of element `n` of `src` (another file) with its strings interned into
    /// this one; not attached anywhere yet.
    pub fn copy_from(&mut self, src: &EditDbx, n: NodeId) -> NodeId {
        let s = src.snapshot(n);
        self.insert(&s)
    }

    /// `Dbx.clone(e)`: [`EditDbx::copy_from`] of an element of this file.
    pub fn clone_node(&mut self, n: NodeId) -> NodeId {
        let s = self.snapshot(n);
        self.insert(&s)
    }

    /// A new element (strings interned in `dbxbin`'s order: tag, attributes, text), not attached anywhere yet.
    pub fn new_node(&mut self, tag: &[u8], kind: u8, attrs: &[(&[u8], &[u8])], text: &[u8], raw: Vec<u8>) -> NodeId {
        let tag = self.intern(tag);
        let attrs = attrs.iter().map(|(k, v)| (self.intern(k), self.intern(v))).collect();
        let text = if kind == TEXT { self.intern(text) } else { 0 };
        let id = self.nodes.len() as NodeId;
        self.nodes.push(ENode {
            tag,
            kind,
            attrs,
            text,
            raw: Raw::Own(match kind {
                CONTAINER => Vec::new(),
                TEXT => varint_bytes(text),
                _ => raw,
            }),
            cbyte: 0,
            children: (kind == CONTAINER).then(Vec::new),
        });
        id
    }

    /// The roots, to set (an empty file's partition) or reorder.
    pub fn roots_mut(&mut self) -> &mut Vec<NodeId> {
        &mut self.roots
    }

    // -- writing

    /// `dbxbin.write(d)`: the original string area while no string was added, else a new one of every string.
    ///
    /// # Errors
    /// An element with more than 15 attributes.
    pub fn write(&self) -> EditResult<Vec<u8>> {
        let mut tree = Vec::with_capacity(self.data.len() + 1024);
        let mut stack: Vec<(NodeId, usize)> = self.roots.iter().rev().map(|&r| (r, usize::MAX)).collect();
        // (element, usize::MAX) emits the element; (container, i) its children from i, then its end
        while let Some((n, at)) = stack.pop() {
            let Some(e) = self.node(n) else { continue };
            if at == usize::MAX {
                put_varint(&mut tree, u64::from(e.tag));
                if e.attrs.len() > 15 {
                    return Err(EditError::new("more than 15 attributes"));
                }
                tree.push((e.kind << 4) | e.attrs.len() as u8);
                for &(k, v) in &e.attrs {
                    put_varint(&mut tree, u64::from(k));
                    put_varint(&mut tree, u64::from(v));
                }
                if e.kind == CONTAINER {
                    tree.push(e.cbyte);
                    stack.push((n, 0));
                } else {
                    tree.extend_from_slice(self.payload(n));
                }
            } else {
                match e.children.as_deref().and_then(|c| c.get(at)) {
                    Some(&c) => {
                        stack.push((n, at + 1));
                        stack.push((c, usize::MAX));
                    }
                    None => tree.push(0),
                }
            }
        }
        let fresh;
        let (area, count): (&[u8], usize) = match self.area {
            Some((a, b)) if !self.strings_changed() => (self.data.get(a..b).unwrap_or_default(), self.n_orig),
            _ => {
                let mut offs = Vec::with_capacity(4 * self.strings.len());
                let mut blob = Vec::new();
                for s in self.strings.iter() {
                    offs.extend_from_slice(&(blob.len() as u32).to_be_bytes());
                    blob.extend_from_slice(s);
                    blob.push(0);
                }
                offs.extend(blob);
                fresh = offs;
                (&fresh, self.strings.len())
            }
        };
        let mut out = Vec::with_capacity(24 + area.len() + tree.len() + self.trailing.len());
        out.extend_from_slice(MAGIC);
        for w in [24 + area.len() as u32, self.header, area.len() as u32, count as u32] {
            out.extend_from_slice(&w.to_be_bytes());
        }
        out.extend_from_slice(area);
        out.extend(tree);
        out.extend_from_slice(&self.trailing);
        Ok(out)
    }
}

/// Whether `parse` then `write` gives `data` back byte for byte, for each file (on all cores): `None` where
/// [`EditDbx::parse`] refuses the file.
#[must_use]
pub fn round_trips(items: &[&[u8]]) -> Vec<Option<bool>> {
    use rayon::prelude::*;
    items
        .par_iter()
        .map(|d| EditDbx::parse(d).ok().and_then(|e| e.write().ok()).map(|w| w == *d))
        .collect()
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::many_single_char_names,
    clippy::cast_precision_loss
)]
mod tests {
    use super::*;
    use crate::dbx::{self, DbxFile, Element};

    fn sample() -> Vec<u8> {
        let mut f = DbxFile::default();
        let name = Element::text(&mut f, "field", &[("name", "Name")], "Levels/MP_005");
        let flag = Element::scalar(&mut f, "field", 6, &[("name", "Enabled")], [1, 1]);
        let mut t = Vec::new();
        for v in 0..16 {
            t.extend((v as f32).to_be_bytes());
        }
        let tr = Element::numbers(&mut f, "complex", &[("name", "Transform")], 4, &t);
        let r = Element::text(&mut f, "field", &[("ref", "null"), ("name", "Template")], "");
        let item = Element::text(&mut f, "item", &[("ref", "abc")], "");
        let arr = Element::container(&mut f, "array", &[("name", "Objects")], vec![item]);
        let inst = Element::container(
            &mut f,
            "instance",
            &[("guid", "AB-cd"), ("type", "X.LevelData"), ("x", "y")],
            vec![name, flag, tr, r, arr],
        );
        let part = Element::container(&mut f, "partition", &[("guid", "p")], vec![inst]);
        f.roots.push(part);
        dbx::write(&f).unwrap()
    }

    #[test]
    fn unchanged_files_come_back() {
        let b = sample();
        let d = EditDbx::parse(&b).unwrap();
        assert_eq!(d.write().unwrap(), b);
        // a trailer and a duplicate string survive
        let mut t = b.clone();
        t.extend([0, 9, 9]);
        assert_eq!(EditDbx::parse(&t).unwrap().write().unwrap(), t);
        assert_eq!(round_trips(&[&b, b"nope"]), vec![Some(true), None]);
    }

    #[test]
    fn edits_and_clones() {
        let b = sample();
        let mut d = EditDbx::parse(&b).unwrap();
        let inst = d.of_type(b".LevelData")[0];
        assert_eq!(d.instance(b"ab-CD", <[u8]>::to_ascii_uppercase), Some(inst));
        let flag = d.child(inst, b"Enabled").unwrap();
        d.set_bool(flag, 0).unwrap();
        assert_eq!(d.raw(flag), [1, 0]);
        assert!(d.set_text(flag, b"x").is_err());
        let tr = d.child(inst, b"Transform").unwrap();
        d.set_transform(
            tr,
            &[[1.0, 0.0, -0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [5.0, 6.0, 7.0]],
        )
        .unwrap();
        let back = dbx::parse(&d.write().unwrap()).unwrap();
        let fl = dbx::floats(back.child(back.instances().next().unwrap(), "Transform").unwrap()).unwrap();
        assert_eq!(fl[12..16], [5.0, 6.0, 7.0, 15.0]);
        let c = d.clone_node(inst);
        d.set_attr(c, b"guid", b"NEW").unwrap();
        d.retain_attrs(c, |k| k == b"guid" || k == b"type").unwrap();
        let p = d.partition().unwrap();
        d.children_mut(p).unwrap().push(c);
        let out = d.write().unwrap();
        let back = dbx::parse(&out).unwrap();
        assert_eq!(back.instances().count(), 2);
        assert_eq!(back.strings.last().map(String::as_str), Some("NEW"));
        let arr = d.child(c, b"Objects").unwrap();
        d.make_text(arr, b"").unwrap();
        assert_eq!(d.kind(arr), TEXT);
        let mut other = EditDbx::parse(&b).unwrap();
        let x = other.copy_from(&d, c);
        assert_eq!(other.attr(x, b"guid"), Some(&b"NEW"[..]));
        assert_eq!(other.walk(x).len(), d.walk(c).len());
    }

    #[test]
    fn refuses_damaged_files() {
        let b = sample();
        for cut in 0..b.len() {
            let _ = EditDbx::parse(&b[..cut]);
        }
        assert!(EditDbx::parse(&b[..b.len() - 1]).is_err());
        assert!(EditDbx::parse(b"short").is_err());
    }
}
