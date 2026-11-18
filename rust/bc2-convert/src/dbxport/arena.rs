//! The element tree dbxport works on: an arena of nodes, each Python's `dbx.Element` (tag, kind, attributes, text,
//! raw bytes, children), strings interned per arena.
//!
//! A node id stands for a Python object: where the reference returns the same element (a leaf kept as BF1943 has
//! it), the port keeps the same id, so an element placed twice in the output is one node, and the value passes that
//! change it in place (the reference's `c.text = ...`) change it everywhere it is, as they do in Python. A deep copy
//! (`copy.deepcopy`) makes new nodes; a shallow one (`copy.copy`) a new node sharing the children's ids.

use std::collections::HashMap;
use std::sync::Arc;

use bc2_formats::dbx::{self, CONTAINER, DbxTree, TEXT, Writer};

/// A node's index in its [`Arena`].
pub type Id = u32;
/// A string's index in its [`Arena`]'s table.
pub type S = u32;

/// One element (`dbx.Element`).
#[derive(Debug, Clone)]
pub struct Node {
    /// The tag.
    pub tag: S,
    /// The kind (`dbx.CONTAINER`, `TEXT`, `NUMBERS` or a two-byte value's).
    pub kind: u8,
    /// `(name, value)` pairs in order.
    pub attrs: Vec<(S, S)>,
    /// The text (`''` but for text elements).
    pub text: S,
    /// The raw value bytes (numbers: count and size varints, then the values; other values: two bytes).
    pub raw: Vec<u8>,
    /// The children (none: Python's `None` or `[]`, which the reference treats alike).
    pub children: Vec<Id>,
}

/// Nodes and their strings. String 0 is `""`.
#[derive(Debug, Clone)]
pub struct Arena {
    strs: Vec<Arc<str>>,
    index: HashMap<Arc<str>, S>,
    /// The nodes.
    pub nodes: Vec<Node>,
    /// The file's roots (the partition first), for an arena read from a file.
    pub roots: Vec<Id>,
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}

impl Arena {
    /// An empty arena (its table holds `""`).
    #[must_use]
    pub fn new() -> Self {
        let mut a = Self {
            strs: Vec::new(),
            index: HashMap::new(),
            nodes: Vec::new(),
            roots: Vec::new(),
        };
        a.intern("");
        a
    }

    /// The id of `s`, added the first time.
    pub fn intern(&mut self, s: &str) -> S {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let a: Arc<str> = Arc::from(s);
        let i = S::try_from(self.strs.len()).unwrap_or(S::MAX);
        self.strs.push(a.clone());
        self.index.insert(a, i);
        i
    }

    /// The string `id` names (`""` for one the table lacks).
    #[must_use]
    pub fn s(&self, id: S) -> &str {
        self.strs.get(id as usize).map_or("", |a| a)
    }

    /// The id `s` has here, if it is in the table.
    #[must_use]
    pub fn find(&self, s: &str) -> Option<S> {
        self.index.get(s).copied()
    }

    /// The node `id`. Ids come from this arena; a foreign one panics in debug builds only through `get`'s `None`,
    /// which [`Arena::n`] turns into the first node (never reached: every id the port uses was made here).
    #[must_use]
    pub fn n(&self, id: Id) -> &Node {
        static EMPTY: Node = Node {
            tag: 0,
            kind: TEXT,
            attrs: Vec::new(),
            text: 0,
            raw: Vec::new(),
            children: Vec::new(),
        };
        self.nodes.get(id as usize).unwrap_or(&EMPTY)
    }

    /// The node `id`, to change.
    pub fn m(&mut self, id: Id) -> Option<&mut Node> {
        self.nodes.get_mut(id as usize)
    }

    /// Adds a node, returning its id.
    pub fn push(&mut self, n: Node) -> Id {
        let id = Id::try_from(self.nodes.len()).unwrap_or(Id::MAX);
        self.nodes.push(n);
        id
    }

    /// A new node from strings.
    pub fn make(
        &mut self,
        tag: &str,
        kind: u8,
        attrs: &[(&str, &str)],
        text: &str,
        raw: Vec<u8>,
        children: Vec<Id>,
    ) -> Id {
        let tag = self.intern(tag);
        let attrs = attrs.iter().map(|(k, v)| (self.intern(k), self.intern(v))).collect();
        let text = self.intern(text);
        self.push(Node {
            tag,
            kind,
            attrs,
            text,
            raw,
            children,
        })
    }

    /// The tag of `id`.
    #[must_use]
    pub fn tag(&self, id: Id) -> &str {
        self.s(self.n(id).tag)
    }

    /// The kind of `id`.
    #[must_use]
    pub fn kind(&self, id: Id) -> u8 {
        self.n(id).kind
    }

    /// The text of `id`.
    #[must_use]
    pub fn text(&self, id: Id) -> &str {
        self.s(self.n(id).text)
    }

    /// The raw bytes of `id`.
    #[must_use]
    pub fn raw(&self, id: Id) -> &[u8] {
        &self.n(id).raw
    }

    /// The children of `id`.
    #[must_use]
    pub fn kids(&self, id: Id) -> &[Id] {
        &self.n(id).children
    }

    /// `Element.attr(name)`: the first attribute so named.
    #[must_use]
    pub fn attr(&self, id: Id, name: &str) -> Option<&str> {
        self.n(id)
            .attrs
            .iter()
            .find(|(k, _)| self.s(*k) == name)
            .map(|(_, v)| self.s(*v))
    }

    /// `_field(e, name)`: the first child whose `name` attribute is `name`.
    #[must_use]
    pub fn field(&self, id: Id, name: &str) -> Option<Id> {
        self.kids(id)
            .iter()
            .copied()
            .find(|&c| self.attr(c, "name") == Some(name))
    }

    /// The attributes of `id` as strings.
    #[must_use]
    pub fn attrs(&self, id: Id) -> Vec<(String, String)> {
        self.n(id)
            .attrs
            .iter()
            .map(|(k, v)| (self.s(*k).to_owned(), self.s(*v).to_owned()))
            .collect()
    }

    /// Reads a parsed file into a new arena, roots in [`Arena::roots`] (as `dbx.parse`'s tree: containers' text and
    /// raw empty, text elements' raw empty, other elements' text empty).
    #[must_use]
    pub fn from_tree(tree: &DbxTree<'_>) -> Self {
        let mut a = Self::new();
        a.nodes.reserve(tree.len());
        let ids: Vec<S> = tree.strings().iter().map(|s| a.intern(s)).collect();
        let sid = |i: dbx::StrId| ids.get(i as usize).copied().unwrap_or(0);
        // the levels being walked: their remaining siblings and the node whose children they become
        let mut roots = Vec::new();
        let mut stack: Vec<(dbx::Siblings<'_, '_>, Option<Id>)> = vec![(tree.roots(), None)];
        while let Some((level, parent)) = stack.last_mut() {
            let parent = *parent;
            let Some(n) = level.next() else {
                stack.pop();
                continue;
            };
            let attrs = n.attribute_ids().map(|(k, v)| (sid(k), sid(v))).collect();
            let container = n.is_container();
            let (text, raw) = match (container, n.text_id()) {
                (true, _) => (0, Vec::new()),
                (false, Some(t)) => (sid(t), Vec::new()),
                (false, None) => (0, n.raw().to_vec()),
            };
            let id = a.push(Node {
                tag: sid(n.tag_id()),
                kind: n.kind(),
                attrs,
                text,
                raw,
                children: Vec::new(),
            });
            match parent {
                Some(p) => {
                    if let Some(pn) = a.m(p) {
                        pn.children.push(id);
                    }
                }
                None => roots.push(id),
            }
            if container {
                stack.push((n.children(), Some(id)));
            }
        }
        a.roots = roots;
        a
    }

    /// Drops the string index (a read-only arena - a template partition's - is never interned into again; the
    /// index is a third of its size). [`Arena::find`] answers `None` afterwards; [`Arena::intern`] still works,
    /// adding a string again.
    pub fn freeze(&mut self) {
        self.index = HashMap::new();
        self.nodes.shrink_to_fit();
        self.strs.shrink_to_fit();
    }

    /// How deep the trees under [`Arena::roots`] go (a root alone: 1), counted without recursion.
    #[must_use]
    pub fn depth(&self) -> usize {
        let mut deepest = 0;
        let mut stack: Vec<(Id, usize)> = self.roots.iter().map(|&r| (r, 1)).collect();
        while let Some((id, d)) = stack.pop() {
            deepest = deepest.max(d);
            stack.extend(self.kids(id).iter().map(|&c| (c, d + 1)));
        }
        deepest
    }

    /// `copy.deepcopy` of `src`'s node `id` into this arena (new nodes, strings re-interned).
    pub fn import(&mut self, src: &Arena, id: Id) -> Id {
        let mut map: HashMap<S, S> = HashMap::new();
        self.import_with(src, id, &mut map)
    }

    fn import_with(&mut self, src: &Arena, id: Id, map: &mut HashMap<S, S>) -> Id {
        let n = src.n(id);
        let mut tr = |s: S, me: &mut Self| -> S {
            if let Some(&t) = map.get(&s) {
                return t;
            }
            let t = me.intern(src.s(s));
            map.insert(s, t);
            t
        };
        let tag = tr(n.tag, self);
        let attrs = n.attrs.iter().map(|&(k, v)| (tr(k, self), tr(v, self))).collect();
        let text = tr(n.text, self);
        let children = n.children.iter().map(|&c| self.import_with(src, c, map)).collect();
        self.push(Node {
            tag,
            kind: n.kind,
            attrs,
            text,
            raw: n.raw.clone(),
            children,
        })
    }

    /// `copy.deepcopy` of a node of this arena (no node is shared within what the port copies: see the module
    /// documentation).
    pub fn deep(&mut self, id: Id) -> Id {
        let n = self.n(id).clone();
        let children = n.children.iter().map(|&c| self.deep(c)).collect();
        self.push(Node { children, ..n })
    }

    /// Writes `roots` as `dbx.write` does; `None` when the writer refuses an element (the reference's writer then
    /// decides: the caller falls back to it).
    #[must_use]
    pub fn write(&self, roots: &[Id]) -> Option<Vec<u8>> {
        let mut w = Writer::new();
        let mut stack: Vec<(&[Id], bool)> = vec![(roots, false)];
        while let Some((level, close)) = stack.pop() {
            let Some((&id, rest)) = level.split_first() else {
                if close {
                    w.close();
                }
                continue;
            };
            stack.push((rest, close));
            let n = self.n(id);
            w.element(self.s(n.tag), n.kind, n.attrs.len()).ok()?;
            for &(k, v) in &n.attrs {
                w.attribute(self.s(k), self.s(v));
            }
            match n.kind {
                CONTAINER => {
                    w.open();
                    stack.push((&n.children, true));
                }
                TEXT => w.text(self.s(n.text)),
                _ => w.raw(&n.raw),
            }
        }
        w.finish().ok()
    }
}

/// A read-only node of another arena (a template's, the catalogue's).
#[derive(Clone, Copy)]
pub struct Tn<'a> {
    /// Its arena.
    pub ar: &'a Arena,
    /// Its id there.
    pub id: Id,
}

impl<'a> Tn<'a> {
    /// The node.
    #[must_use]
    pub fn node(self) -> &'a Node {
        self.ar.n(self.id)
    }

    /// The tag.
    #[must_use]
    pub fn tag(self) -> &'a str {
        self.ar.tag(self.id)
    }

    /// The kind.
    #[must_use]
    pub fn kind(self) -> u8 {
        self.ar.kind(self.id)
    }

    /// The text.
    #[must_use]
    pub fn text(self) -> &'a str {
        self.ar.text(self.id)
    }

    /// The raw bytes.
    #[must_use]
    pub fn raw(self) -> &'a [u8] {
        self.ar.raw(self.id)
    }

    /// `attr(name)`.
    #[must_use]
    pub fn attr(self, name: &str) -> Option<&'a str> {
        self.ar.attr(self.id, name)
    }

    /// The children.
    pub fn kids(self) -> impl ExactSizeIterator<Item = Tn<'a>> + 'a {
        let ar = self.ar;
        self.ar.kids(self.id).iter().map(move |&id| Tn { ar, id })
    }

    /// Whether it has children.
    #[must_use]
    pub fn has_kids(self) -> bool {
        !self.ar.kids(self.id).is_empty()
    }

    /// The `i`-th child.
    #[must_use]
    pub fn kid(self, i: usize) -> Option<Tn<'a>> {
        self.ar.kids(self.id).get(i).map(|&id| Tn { ar: self.ar, id })
    }

    /// `_field(e, name)`.
    #[must_use]
    pub fn field(self, name: &str) -> Option<Tn<'a>> {
        self.ar.field(self.id, name).map(|id| Tn { ar: self.ar, id })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::many_single_char_names)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_a_file_back() {
        let mut f = dbx::DbxFile::default();
        let name = dbx::Element::text(&mut f, "field", &[("name", "Name")], "x");
        let n = dbx::Element::numbers(&mut f, "field", &[("name", "N")], 4, &[0, 0, 0, 1]);
        let b = dbx::Element::scalar(&mut f, "field", 6, &[("name", "B")], [0, 1]);
        let empty = dbx::Element::container(&mut f, "array", &[("name", "A")], vec![]);
        let inst = dbx::Element::container(&mut f, "instance", &[("guid", "g")], vec![name, n, b, empty]);
        let part = dbx::Element::container(&mut f, "partition", &[("guid", "p")], vec![inst]);
        f.roots.push(part);
        let bytes = dbx::write(&f).unwrap();
        let tree = dbx::parse_tree(&bytes).unwrap();
        let a = Arena::from_tree(&tree);
        assert_eq!(a.write(&a.roots).unwrap(), bytes);
        let inst = a.kids(a.roots[0])[0];
        assert_eq!(a.attr(inst, "guid"), Some("g"));
        assert_eq!(a.text(a.field(inst, "Name").unwrap()), "x");
        assert_eq!(a.raw(a.field(inst, "B").unwrap()), &[0, 1]);
        let mut b2 = Arena::new();
        let copy = b2.import(&a, a.roots[0]);
        assert_eq!(b2.write(&[copy]).unwrap(), bytes);
        let mut c = a.clone();
        let d = c.deep(c.roots[0]);
        assert_ne!(c.kids(d)[0], c.kids(c.roots[0])[0]);
        assert_eq!(c.write(&[d]).unwrap(), bytes);
    }
}
