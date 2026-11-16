//! The element tree (FORMAT.md 2), the model of `core/bfbc2core/dbx.py`: a file is a string table and root
//! elements; an element has a tag, a kind, attributes and a value or children. Strings are indices into the file's
//! table ([`StrId`]), read with [`DbxFile::str`].

/// An index into a [`DbxFile`]'s string table.
pub type StrId = u32;

/// A container: an element with children (`dbx.CONTAINER`).
pub const CONTAINER: u8 = 0xA;
/// A text value or reference (`dbx.TEXT`).
pub const TEXT: u8 = 2;
/// Big-endian numbers (`dbx.NUMBERS`).
pub const NUMBERS: u8 = 7;

/// What an element holds besides its tag and attributes. Its variant follows the element's kind: [`CONTAINER`],
/// [`TEXT`] and [`NUMBERS`] have their own; every other kind is a two-byte value (bools, bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Body {
    /// Children, in file order.
    Container(Vec<Element>),
    /// The text's string.
    Text(StrId),
    /// The raw bytes as the file stores them: the count and size varints, then `count * size` big-endian bytes
    /// (Python's `Element.raw` for a numbers element).
    Numbers(Vec<u8>),
    /// The two bytes of a value of any other kind (Python's `Element.raw`).
    Scalar([u8; 2]),
}

/// One element of the tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    /// The tag (`partition`, `instance`, `field`, `complex`, `array`, `item` ...).
    pub tag: StrId,
    /// The kind, the high four bits of the element's head byte.
    pub kind: u8,
    /// `(name, value)` pairs in file order, at most 15.
    pub attributes: Vec<(StrId, StrId)>,
    /// The value or children.
    pub body: Body,
}

impl Element {
    /// The children of a container, or an empty slice.
    #[must_use]
    pub fn children(&self) -> &[Element] {
        match &self.body {
            Body::Container(c) => c,
            _ => &[],
        }
    }

    /// The raw value bytes as Python's `Element.raw` (empty for containers and text).
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        match &self.body {
            Body::Numbers(r) => r,
            Body::Scalar(r) => r,
            Body::Container(_) | Body::Text(_) => &[],
        }
    }

    fn with_attributes(file: &mut DbxFile, tag: &str, kind: u8, attributes: &[(&str, &str)], body: Body) -> Self {
        let tag = file.intern(tag);
        let attributes = attributes
            .iter()
            .map(|(k, v)| (file.intern(k), file.intern(v)))
            .collect();
        Self {
            tag,
            kind,
            attributes,
            body,
        }
    }

    /// A container with its children, its strings added to `file`'s table.
    pub fn container(file: &mut DbxFile, tag: &str, attributes: &[(&str, &str)], children: Vec<Element>) -> Self {
        Self::with_attributes(file, tag, CONTAINER, attributes, Body::Container(children))
    }

    /// A text element.
    pub fn text(file: &mut DbxFile, tag: &str, attributes: &[(&str, &str)], text: &str) -> Self {
        let t = file.intern(text);
        Self::with_attributes(file, tag, TEXT, attributes, Body::Text(t))
    }

    /// A numbers element from its values, each `size` big-endian bytes (`values.len()` a multiple of `size`).
    pub fn numbers(file: &mut DbxFile, tag: &str, attributes: &[(&str, &str)], size: usize, values: &[u8]) -> Self {
        let mut raw = Vec::with_capacity(values.len() + 4);
        let count = values.len().checked_div(size).unwrap_or(0);
        super::writer::put_varint(&mut raw, count as u64);
        super::writer::put_varint(&mut raw, size as u64);
        raw.extend_from_slice(values);
        Self::with_attributes(file, tag, NUMBERS, attributes, Body::Numbers(raw))
    }

    /// A two-byte value of `kind` (any kind but the three above).
    pub fn scalar(file: &mut DbxFile, tag: &str, kind: u8, attributes: &[(&str, &str)], raw: [u8; 2]) -> Self {
        Self::with_attributes(file, tag, kind, attributes, Body::Scalar(raw))
    }
}

/// A parsed `.dbx` file: its string table and root elements (the partition first).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DbxFile {
    /// The string table in file order (the reader keeps the file's own table, duplicates included).
    pub strings: Vec<String>,
    /// The root elements.
    pub roots: Vec<Element>,
}

impl DbxFile {
    /// A file of these roots with an empty string table (build them with a `&mut` of it first: see
    /// [`Element::text`]).
    #[must_use]
    pub fn new(roots: Vec<Element>) -> Self {
        Self {
            strings: Vec::new(),
            roots,
        }
    }

    /// The string `id` names, or `""` for an index the table does not have (the reader checks every index it
    /// stores, so that happens only for elements built by hand with ids from another table).
    #[must_use]
    pub fn str(&self, id: StrId) -> &str {
        self.strings.get(id as usize).map_or("", String::as_str)
    }

    /// The id of `s`, added to the table if it is not there yet (a table past `u32::MAX` strings saturates at the
    /// last id).
    pub fn intern(&mut self, s: &str) -> StrId {
        if let Some(i) = self.strings.iter().position(|t| t == s) {
            return StrId::try_from(i).unwrap_or(StrId::MAX);
        }
        self.strings.push(s.to_owned());
        StrId::try_from(self.strings.len() - 1).unwrap_or(StrId::MAX)
    }

    /// The tag of `e`.
    #[must_use]
    pub fn tag<'a>(&'a self, e: &Element) -> &'a str {
        self.str(e.tag)
    }

    /// The first attribute of `e` named `name`, as `Element.attr`.
    #[must_use]
    pub fn attr<'a>(&'a self, e: &Element, name: &str) -> Option<&'a str> {
        e.attributes
            .iter()
            .find(|(k, _)| self.str(*k) == name)
            .map(|(_, v)| self.str(*v))
    }

    /// The text of a text element, or `""`.
    #[must_use]
    pub fn text<'a>(&'a self, e: &Element) -> &'a str {
        match e.body {
            Body::Text(t) => self.str(t),
            _ => "",
        }
    }

    /// The first direct child of `e` whose `name` attribute is `name`, as `Element.child`.
    #[must_use]
    pub fn child<'e>(&self, e: &'e Element, name: &str) -> Option<&'e Element> {
        e.children().iter().find(|c| self.attr(c, "name") == Some(name))
    }

    /// The partition: the first root.
    #[must_use]
    pub fn partition(&self) -> Option<&Element> {
        self.roots.first()
    }

    /// The partition's `instance` children.
    pub fn instances(&self) -> impl Iterator<Item = &Element> {
        self.partition()
            .map_or(&[][..], Element::children)
            .iter()
            .filter(|e| self.tag(e) == "instance")
    }

    /// The instance whose `guid` is `guid` (any case).
    #[must_use]
    pub fn instance(&self, guid: &str) -> Option<&Element> {
        self.instances()
            .find(|e| self.attr(e, "guid").is_some_and(|g| g.eq_ignore_ascii_case(guid)))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    fn sample() -> DbxFile {
        let mut f = DbxFile::default();
        let name = Element::text(&mut f, "field", &[("name", "Name")], "Levels/MP_005");
        let flag = Element::scalar(&mut f, "field", 6, &[("name", "Enabled")], [0, 1]);
        let n = Element::numbers(&mut f, "field", &[("name", "Health")], 4, &100f32.to_be_bytes());
        let inst = Element::container(
            &mut f,
            "instance",
            &[("guid", "AB-cd"), ("type", "X")],
            vec![name, flag, n],
        );
        let other = Element::container(&mut f, "array", &[], vec![]);
        let part = Element::container(&mut f, "partition", &[("guid", "p")], vec![inst, other]);
        f.roots.push(part);
        f
    }

    #[test]
    fn lookups() {
        let f = sample();
        let inst = f.instance("ab-CD").unwrap();
        assert_eq!(f.attr(inst, "type"), Some("X"));
        assert_eq!(f.attr(inst, "missing"), None);
        assert_eq!(f.text(f.child(inst, "Name").unwrap()), "Levels/MP_005");
        assert_eq!(f.child(inst, "Enabled").unwrap().raw(), &[0, 1]);
        assert_eq!(f.child(inst, "Health").unwrap().raw(), &[1, 4, 0x42, 0xC8, 0, 0]);
        assert!(f.child(inst, "Nope").is_none());
        assert_eq!(f.instances().count(), 1);
        assert!(f.instance("zz").is_none());
        assert_eq!(f.text(inst), "");
        assert_eq!(inst.raw(), &[] as &[u8]);
    }

    #[test]
    fn interning_reuses_strings() {
        let mut f = DbxFile::default();
        let a = f.intern("x");
        let b = f.intern("y");
        assert_eq!(f.intern("x"), a);
        assert_ne!(a, b);
        assert_eq!(f.str(99), "");
    }

    #[test]
    fn empty_file_has_no_partition() {
        let f = DbxFile::new(vec![]);
        assert!(f.partition().is_none());
        assert_eq!(f.instances().count(), 0);
    }
}
