//! A parsed file as one compact, read-only tree that borrows the file's bytes ([`DbxTree`]): the reader's own
//! result, and what reading many files at once should keep.
//!
//! [`parse`](super::parse)'s [`DbxFile`] is an owned tree of [`Element`]s, each with its own vectors of attributes and
//! children and its own copy of a numbers value: the shape to edit and write. A [`DbxTree`] holds the same tree in four
//! allocations however large the file: every element in one vector in file order (each knows where its subtree ends),
//! every attribute in another, the strings borrowed from the file (copied only when a byte is not UTF-8) and values as
//! ranges of the file's bytes. [`DbxTree::to_file`] gives the owned tree; [`Node`] reads the tree as [`DbxFile`] and
//! [`Element`] do.

use std::borrow::Cow;

use super::cursor::Cursor;
use super::error::{DbxError, Result};
use super::model::{Body, CONTAINER, DbxFile, Element, NUMBERS, StrId, TEXT};
use super::number::{number_text, numbers, scalar_text};
use super::reader::string_id;
use super::strings::{is_binary, read_table};

/// What an element holds besides its tag and attributes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Value {
    /// Children: the elements after it up to its subtree's end.
    Container,
    /// A text element's string.
    Text(StrId),
    /// The raw value bytes (a numbers element's varints and values, or a two-byte value): a range of the file.
    Raw { start: u32, len: u32 },
}

/// One element, as stored: 28 bytes, no allocation of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeData {
    tag: StrId,
    kind: u8,
    attr_count: u8,
    attr_start: u32,
    /// The index one past the last element of this element's subtree (its next sibling, if it has one).
    end: u32,
    value: Value,
}

/// A binary `.dbx` file as a compact read-only tree borrowing the file's bytes (see the module documentation).
///
/// ```
/// use bc2_formats::dbx::{self, DbxFile, Element};
///
/// let mut f = DbxFile::default();
/// let name = Element::text(&mut f, "field", &[("name", "Name")], "Levels/MP_005");
/// let inst = Element::container(&mut f, "instance", &[("guid", "AB")], vec![name]);
/// let part = Element::container(&mut f, "partition", &[], vec![inst]);
/// f.roots.push(part);
/// let bytes = dbx::write(&f)?;
///
/// let tree = dbx::parse_tree(&bytes)?;
/// let inst = tree.instance("ab").expect("the instance");
/// assert_eq!(inst.child("Name").map(|n| n.text()), Some("Levels/MP_005"));
/// assert_eq!(tree.to_file(), dbx::parse(&bytes)?);
/// # Ok::<(), dbx::DbxError>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbxTree<'a> {
    data: &'a [u8],
    strings: Vec<Cow<'a, str>>,
    nodes: Vec<NodeData>,
    attributes: Vec<(StrId, StrId)>,
}

/// One element of a [`DbxTree`]: a cheap handle (a reference to the tree and an index).
#[derive(Debug, Clone, Copy)]
pub struct Node<'t, 'a> {
    tree: &'t DbxTree<'a>,
    index: u32,
}

/// The elements of one level of a [`DbxTree`] in file order: the roots, or an element's children.
#[derive(Debug, Clone)]
pub struct Siblings<'t, 'a> {
    tree: &'t DbxTree<'a>,
    next: u32,
    end: u32,
}

impl<'t, 'a> Iterator for Siblings<'t, 'a> {
    type Item = Node<'t, 'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.next >= self.end {
            return None;
        }
        let index = self.next;
        let after = self.tree.nodes.get(index as usize).map_or(self.end, |n| n.end);
        self.next = after.max(index + 1);
        Some(Node { tree: self.tree, index })
    }
}

fn to_u32(v: usize) -> Result<u32> {
    u32::try_from(v).map_err(|_| DbxError::TooLarge)
}

/// A binary `.dbx` file as a [`DbxTree`], as `dbx.parse` reads it (the same elements, values and errors as
/// [`parse`](super::parse), which is this and [`DbxTree::to_file`]).
///
/// # Errors
/// As [`parse`](super::parse); and [`DbxError::TooLarge`] for a file of 4 GiB or more.
pub fn parse_tree(data: &[u8]) -> Result<DbxTree<'_>> {
    if data.is_empty() {
        return Err(DbxError::Deleted);
    }
    if !is_binary(data) {
        return Err(DbxError::NotBinary);
    }
    let n = data.len();
    to_u32(n)?;
    let (strings, body) = read_table(data, false)?;
    let count = strings.len();
    let mut c = Cursor::new(data, body);
    // each element takes at least two bytes, a container three and its end one; a level partition has about one
    // element per 6 bytes and one attribute per element
    let guess = n.saturating_sub(body) / 6;
    let mut nodes: Vec<NodeData> = Vec::with_capacity(guess);
    let mut attributes: Vec<(StrId, StrId)> = Vec::with_capacity(guess);
    let mut open: Vec<usize> = Vec::new();
    while !c.at_end() {
        let tag = c.varint()?;
        if tag == 0 {
            let Some(i) = open.pop() else { break };
            let end = to_u32(nodes.len())?;
            if let Some(node) = nodes.get_mut(i) {
                node.end = end;
            }
            continue;
        }
        let tag = string_id(tag, count)?;
        let head = c.byte()?;
        let kind = head >> 4;
        let attr_start = to_u32(attributes.len())?;
        for _ in 0..(head & 0xF) {
            let k = string_id(c.varint()?, count)?;
            let v = string_id(c.varint()?, count)?;
            attributes.push((k, v));
        }
        let index = nodes.len();
        let value = match kind {
            CONTAINER => {
                c.skip(1);
                open.push(index);
                Value::Container
            }
            TEXT => Value::Text(string_id(c.varint()?, count)?),
            NUMBERS => {
                let start = c.pos();
                let num = c.varint()?;
                let size = c.varint()?;
                let end = num
                    .checked_mul(size)
                    .and_then(|len| usize::try_from(len).ok())
                    .and_then(|len| c.pos().checked_add(len))
                    .filter(|&end| end <= n)
                    .ok_or(DbxError::NumbersPastEnd { count: num, size })?;
                c.skip(end - c.pos());
                Value::Raw {
                    start: to_u32(start)?,
                    len: to_u32(end - start)?,
                }
            }
            _ => {
                if c.peek_at(0).is_err() || c.peek_at(1).is_err() {
                    return Err(DbxError::ValueCutShort);
                }
                let start = to_u32(c.pos())?;
                c.skip(2);
                Value::Raw { start, len: 2 }
            }
        };
        nodes.push(NodeData {
            tag,
            kind,
            attr_count: head & 0xF,
            attr_start,
            end: to_u32(index + 1)?,
            value,
        });
    }
    if !open.is_empty() {
        return Err(DbxError::Unclosed);
    }
    nodes.shrink_to_fit();
    attributes.shrink_to_fit();
    Ok(DbxTree {
        data,
        strings,
        nodes,
        attributes,
    })
}

impl<'a> DbxTree<'a> {
    /// The file's bytes the tree reads its strings and values from.
    #[must_use]
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// The string table in file order (duplicates included), as [`DbxFile::strings`].
    #[must_use]
    pub fn strings(&self) -> &[Cow<'a, str>] {
        &self.strings
    }

    /// The string `id` names, or `""` for an index the table does not have.
    #[must_use]
    pub fn str(&self, id: StrId) -> &str {
        self.strings.get(id as usize).map_or("", |s| s.as_ref())
    }

    /// The number of elements in the whole tree.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether the tree has no elements.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// The root elements in file order (the partition first).
    #[must_use]
    pub fn roots(&self) -> Siblings<'_, 'a> {
        Siblings {
            tree: self,
            next: 0,
            end: self.nodes.len() as u32,
        }
    }

    /// The partition: the first root.
    #[must_use]
    pub fn partition(&self) -> Option<Node<'_, 'a>> {
        self.roots().next()
    }

    /// The partition's `instance` children.
    pub fn instances(&self) -> impl Iterator<Item = Node<'_, 'a>> {
        self.partition()
            .map(|p| p.children())
            .into_iter()
            .flatten()
            .filter(|e| e.tag() == "instance")
    }

    /// The instance whose `guid` is `guid` (any case).
    #[must_use]
    pub fn instance(&self, guid: &str) -> Option<Node<'_, 'a>> {
        self.instances()
            .find(|e| e.attr("guid").is_some_and(|g| g.eq_ignore_ascii_case(guid)))
    }

    /// The owned tree [`parse`](super::parse) returns: to edit, or to write with [`write`](super::write).
    #[must_use]
    pub fn to_file(&self) -> DbxFile {
        DbxFile {
            strings: self.strings.iter().map(|s| s.as_ref().to_owned()).collect(),
            roots: self.elements(&self.nodes, 0),
        }
    }

    /// The owned elements of `nodes`, a run of whole subtrees whose first is the tree's element `base`.
    fn elements(&self, nodes: &[NodeData], base: u32) -> Vec<Element> {
        fn attach(open: &mut [(usize, Element)], roots: &mut Vec<Element>, done: Element) {
            match open.last_mut() {
                Some((
                    _,
                    Element {
                        body: Body::Container(children),
                        ..
                    },
                )) => children.push(done),
                _ => roots.push(done),
            }
        }
        let mut roots = Vec::new();
        // open containers, innermost last, each with its subtree's end; a container joins its parent when the
        // walk passes its end (no recursion: the depth is the file's)
        let mut open: Vec<(usize, Element)> = Vec::new();
        for (i, n) in nodes.iter().enumerate() {
            while open.last().is_some_and(|(end, _)| *end <= i) {
                if let Some((_, done)) = open.pop() {
                    attach(&mut open, &mut roots, done);
                }
            }
            let raw = self.raw_of(n);
            let body = match (n.value, n.kind, raw) {
                (Value::Container, _, _) => Body::Container(Vec::new()),
                (Value::Text(t), _, _) => Body::Text(t),
                (Value::Raw { .. }, k, &[a, b]) if k != NUMBERS => Body::Scalar([a, b]),
                (Value::Raw { .. }, _, _) => Body::Numbers(raw.to_vec()),
            };
            let e = Element {
                tag: n.tag,
                kind: n.kind,
                attributes: self.attrs_of(n).to_vec(),
                body,
            };
            if n.value == Value::Container {
                open.push(((n.end - base) as usize, e));
            } else {
                attach(&mut open, &mut roots, e);
            }
        }
        while let Some((_, done)) = open.pop() {
            attach(&mut open, &mut roots, done);
        }
        roots
    }

    fn attrs_of(&self, n: &NodeData) -> &[(StrId, StrId)] {
        let start = n.attr_start as usize;
        self.attributes
            .get(start..start + usize::from(n.attr_count))
            .unwrap_or_default()
    }

    fn raw_of(&self, n: &NodeData) -> &'a [u8] {
        match n.value {
            Value::Raw { start, len } => self
                .data
                .get(start as usize..start as usize + len as usize)
                .unwrap_or_default(),
            Value::Container | Value::Text(_) => &[],
        }
    }
}

impl<'t, 'a> Node<'t, 'a> {
    fn data(&self) -> &'t NodeData {
        // every Node is made from an index of its tree; an empty default only to avoid a panic path
        const EMPTY: NodeData = NodeData {
            tag: 0,
            kind: 0,
            attr_count: 0,
            attr_start: 0,
            end: 0,
            value: Value::Container,
        };
        self.tree.nodes.get(self.index as usize).unwrap_or(&EMPTY)
    }

    /// The element's position in the whole tree (file order).
    #[must_use]
    pub fn index(&self) -> usize {
        self.index as usize
    }

    /// The tag (`partition`, `instance`, `field` ...).
    #[must_use]
    pub fn tag(&self) -> &'t str {
        self.tree.str(self.data().tag)
    }

    /// The tag's id in [`DbxTree::strings`].
    #[must_use]
    pub fn tag_id(&self) -> StrId {
        self.data().tag
    }

    /// The attributes as ids in [`DbxTree::strings`], in file order.
    pub fn attribute_ids(&self) -> impl ExactSizeIterator<Item = (StrId, StrId)> + use<'t, 'a> {
        self.tree.attrs_of(self.data()).iter().copied()
    }

    /// A text element's string id in [`DbxTree::strings`] (`None` for any other element).
    #[must_use]
    pub fn text_id(&self) -> Option<StrId> {
        match self.data().value {
            Value::Text(t) => Some(t),
            _ => None,
        }
    }

    /// The kind, the high four bits of the element's head byte ([`CONTAINER`], [`TEXT`], [`NUMBERS`] or a two-byte
    /// value's).
    #[must_use]
    pub fn kind(&self) -> u8 {
        self.data().kind
    }

    /// Whether the element is a container.
    #[must_use]
    pub fn is_container(&self) -> bool {
        self.data().value == Value::Container
    }

    /// `(name, value)` pairs in file order.
    pub fn attributes(&self) -> impl ExactSizeIterator<Item = (&'t str, &'t str)> + use<'t, 'a> {
        let tree = self.tree;
        tree.attrs_of(self.data())
            .iter()
            .map(move |(k, v)| (tree.str(*k), tree.str(*v)))
    }

    /// The first attribute named `name`, as `Element.attr`.
    #[must_use]
    pub fn attr(&self, name: &str) -> Option<&'t str> {
        self.attributes().find(|(k, _)| *k == name).map(|(_, v)| v)
    }

    /// The children of a container (none for any other element).
    #[must_use]
    pub fn children(&self) -> Siblings<'t, 'a> {
        let n = self.data();
        let next = self.index + 1;
        Siblings {
            tree: self.tree,
            next,
            end: if n.value == Value::Container { n.end } else { next },
        }
    }

    /// The first direct child whose `name` attribute is `name`, as `Element.child`.
    #[must_use]
    pub fn child(&self, name: &str) -> Option<Node<'t, 'a>> {
        self.children().find(|c| c.attr("name") == Some(name))
    }

    /// The text of a text element, or `""`.
    #[must_use]
    pub fn text(&self) -> &'t str {
        match self.data().value {
            Value::Text(t) => self.tree.str(t),
            _ => "",
        }
    }

    /// The raw value bytes as Python's `Element.raw` (empty for containers and text), borrowed from the file.
    #[must_use]
    pub fn raw(&self) -> &'a [u8] {
        self.tree.raw_of(self.data())
    }

    /// The element's value as the XML shows it, as [`value`](super::value).
    ///
    /// # Errors
    /// As [`value`](super::value).
    pub fn value(&self) -> Result<String> {
        let raw = self.raw();
        match (self.data().value, self.kind()) {
            (Value::Text(t), _) => Ok(self.tree.str(t).to_owned()),
            (Value::Raw { .. }, NUMBERS) => {
                let (size, vals) = numbers(raw)?;
                let first = self.attributes().next().map_or("", |(_, v)| v);
                let texts = vals
                    .into_iter()
                    .map(|v| number_text(v, size, first))
                    .collect::<Result<Vec<_>>>()?;
                Ok(texts.join("/"))
            }
            (Value::Raw { .. }, _) if raw.len() == 2 => Ok(scalar_text(raw.get(1).copied().unwrap_or(0))),
            _ => Err(DbxError::ValueSize {
                len: if self.kind() == NUMBERS { raw.len() } else { 0 },
            }),
        }
    }

    /// A 4-byte numbers element's values as singles, as [`floats`](super::floats).
    #[must_use]
    pub fn floats(&self) -> Option<Vec<f32>> {
        if self.kind() != NUMBERS {
            return None;
        }
        let (size, vals) = numbers(self.raw()).ok()?;
        if size != 4 {
            return None;
        }
        vals.into_iter()
            .map(|v| <[u8; 4]>::try_from(v).ok().map(f32::from_be_bytes))
            .collect()
    }

    /// This element and its subtree as an owned [`Element`] whose string ids are the tree's (to go with
    /// [`DbxTree::to_file`]'s table).
    #[must_use]
    pub fn to_element(&self) -> Element {
        let start = self.index as usize;
        let end = (self.data().end as usize).max(start + 1);
        let nodes = self.tree.nodes.get(start..end).unwrap_or_default();
        self.tree
            .elements(nodes, self.index)
            .into_iter()
            .next()
            .unwrap_or(Element {
                tag: 0,
                kind: 0,
                attributes: Vec::new(),
                body: Body::Container(Vec::new()),
            })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::dbx::{floats, parse, value, write};

    fn sample() -> (DbxFile, Vec<u8>) {
        let mut f = DbxFile::default();
        let name = Element::text(&mut f, "field", &[("name", "Name")], "Levels/MP_005");
        let flag = Element::scalar(&mut f, "field", 6, &[("name", "Enabled")], [0, 1]);
        let mut t = Vec::new();
        for v in [1f32, 0.5, -2.0, 7.0] {
            t.extend(v.to_be_bytes());
        }
        let tr = Element::numbers(&mut f, "field", &[("name", "Transform")], 4, &t);
        let id = Element::numbers(&mut f, "field", &[("name", "Id")], 2, &[0, 7]);
        let empty = Element::container(&mut f, "array", &[("name", "Empty")], vec![]);
        let inner = Element::text(&mut f, "item", &[], "x");
        let deep = Element::container(&mut f, "complex", &[("name", "Deep")], vec![inner]);
        let inst = Element::container(
            &mut f,
            "instance",
            &[("guid", "AB-cd"), ("type", "X")],
            vec![name, flag, tr, id, empty, deep],
        );
        let other = Element::container(&mut f, "instance", &[("guid", "zz")], vec![]);
        let part = Element::container(&mut f, "partition", &[("guid", "p")], vec![inst, other]);
        let trailer = Element::text(&mut f, "field", &[], "after");
        f.roots.extend([part, trailer]);
        let bytes = write(&f).unwrap();
        (parse(&bytes).unwrap(), bytes)
    }

    #[test]
    fn same_tree_as_parse() {
        let (owned, bytes) = sample();
        let tree = parse_tree(&bytes).unwrap();
        assert_eq!(tree.to_file(), owned);
        assert_eq!(tree.len(), 11);
        assert!(!tree.is_empty());
        assert_eq!(tree.data(), &bytes[..]);
        assert_eq!(tree.strings().len(), owned.strings.len());
        assert_eq!(tree.roots().count(), 2);
    }

    #[test]
    fn reads_as_the_owned_tree_does() {
        let (owned, bytes) = sample();
        let tree = parse_tree(&bytes).unwrap();
        let inst = tree.instance("ab-CD").unwrap();
        let oinst = owned.instance("AB-CD").unwrap();
        assert_eq!(inst.tag(), "instance");
        assert_eq!(inst.attr("type"), Some("X"));
        assert_eq!(inst.attr("nope"), None);
        assert_eq!(inst.attributes().len(), 2);
        assert_eq!(inst.attribute_ids().len(), 2);
        assert_eq!(tree.str(inst.tag_id()), "instance");
        assert_eq!(inst.text_id(), None);
        assert_eq!(
            inst.child("Name").unwrap().text_id().map(|t| tree.str(t)),
            Some("Levels/MP_005")
        );
        assert!(inst.is_container());
        assert_eq!(inst.children().count(), oinst.children().len());
        for (c, o) in inst.children().zip(oinst.children()) {
            assert_eq!(c.kind(), o.kind);
            assert_eq!(c.raw(), o.raw());
            assert_eq!(c.text(), owned.text(o));
            assert_eq!(c.value(), value(&owned, o));
            assert_eq!(c.floats(), floats(o));
            assert_eq!(c.to_element(), *o);
        }
        assert_eq!(
            inst.child("Transform").unwrap().floats(),
            Some(vec![1.0, 0.5, -2.0, 7.0])
        );
        assert_eq!(inst.child("Id").unwrap().value().unwrap(), "7");
        assert_eq!(inst.child("Id").unwrap().floats(), None);
        assert_eq!(inst.child("Name").unwrap().floats(), None);
        assert_eq!(inst.child("Enabled").unwrap().value().unwrap(), "true");
        assert!(inst.child("Empty").unwrap().children().next().is_none());
        assert!(inst.value().is_err());
        assert!(inst.child("Missing").is_none());
        assert_eq!(inst.child("Name").unwrap().children().count(), 0);
        let deep = inst.child("Deep").unwrap();
        assert_eq!(deep.children().next().unwrap().text(), "x");
        assert_eq!(deep.index(), 7);
        assert_eq!(inst.to_element(), *oinst);
        assert_eq!(tree.instances().count(), 2);
        assert!(tree.instance("none").is_none());
        assert_eq!(tree.str(9999), "");
        assert_eq!(tree.partition().unwrap().tag(), "partition");
    }

    #[test]
    fn refuses_as_parse_does() {
        let (_, bytes) = sample();
        for cut in 0..bytes.len() {
            let d = &bytes[..cut];
            assert_eq!(parse_tree(d).map(|t| t.to_file()), parse(d), "cut at {cut}");
        }
        assert_eq!(parse_tree(b""), Err(DbxError::Deleted));
        assert_eq!(parse_tree(b"nope"), Err(DbxError::NotBinary));
    }

    #[test]
    fn empty_tree() {
        // the string table, then a body that closes nothing: no elements
        let (_, bytes) = sample();
        let body_at = 24 + u32::from_be_bytes(bytes[16..20].try_into().unwrap()) as usize;
        let mut d = bytes[..body_at].to_vec();
        d.push(0);
        let t = parse_tree(&d).unwrap();
        assert!(t.is_empty());
        assert!(t.partition().is_none());
        assert_eq!(t.instances().count(), 0);
        assert_eq!(t.to_file(), parse(&d).unwrap());
    }
}
