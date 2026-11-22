//! The binary writer, as `dbx.write`: a fresh string table (the empty string first, then each string in the order
//! the tree first uses it), container flag bytes 0, header value 0. Not byte-identical to a game file (DbxEdit
//! keeps a file's own string order), but the same tree: `parse(write(f))` gives `f`'s tree back.
//!
//! [`Writer`] writes a file element by element, from any tree: [`write`] drives it from a [`DbxFile`], and
//! `bc2-py` from the Python tree `dbx.write` is given (no Rust tree in between), so both give the Python writer's
//! bytes.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use super::error::{DbxError, Result};
use super::model::{Body, CONTAINER, DbxFile, Element, TEXT};
use super::strings::MAGIC;

/// Appends `v` as an unsigned LEB128 varint, as `dbx.put_varint`.
///
/// ```
/// let mut out = Vec::new();
/// bc2_formats::dbx::put_varint(&mut out, 300);
/// assert_eq!(out, [0xAC, 0x02]);
/// ```
pub fn put_varint(out: &mut Vec<u8>, v: u64) {
    crate::varint::put(out, v);
}

/// `FxHash` (rustc's hasher): the string table's lookups are most of a write, and its keys are not attacker-chosen
/// in a way that matters (a slow file, at worst).
#[derive(Default, Clone, Copy)]
struct Fx(u64);

impl Hasher for Fx {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        const K: u64 = 0x517c_c1b7_2722_0a95;
        let (chunks, rest) = bytes.as_chunks::<8>();
        for c in chunks {
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(*c)).wrapping_mul(K);
        }
        for &b in rest {
            self.0 = (self.0.rotate_left(5) ^ u64::from(b)).wrapping_mul(K);
        }
    }
}

const TOO_BIG: &str = "string table past 4 GB";

/// A `.dbx` written element by element, in file order: [`Writer::element`] for each element (then its
/// [`Writer::attribute`]s, then [`Writer::open`] ... [`Writer::close`] around a container's children,
/// [`Writer::text`] for a text element or [`Writer::raw`] for any other value), and [`Writer::finish`] for the
/// file. Strings are numbered as `dbx.write` numbers them: the empty string 0, then each in the order first used.
///
/// The writer checks what each call is given (an element's tag, kind and attribute count) but not the order of the
/// calls: a caller that leaves a container open or writes a value of the wrong shape gets a file that does not
/// read back, as the Python writer would give for the same tree.
///
/// ```
/// use bc2_formats::dbx::{self, Writer, CONTAINER, TEXT};
///
/// let mut w = Writer::new();
/// w.element("instance", CONTAINER, 1)?;
/// w.attribute("guid", "AB");
/// w.open();
/// w.element("field", TEXT, 1)?;
/// w.attribute("name", "Name");
/// w.text("MP_005");
/// w.close();
/// let back = dbx::parse(&w.finish()?)?;
/// assert_eq!(back.strings, ["", "instance", "guid", "AB", "field", "name", "Name", "MP_005"]);
/// # Ok::<(), dbx::DbxError>(())
/// ```
#[derive(Default)]
pub struct Writer {
    index: HashMap<Box<str>, u32, BuildHasherDefault<Fx>>,
    offsets: Vec<u32>,
    blob: Vec<u8>,
    body: Vec<u8>,
}

impl Writer {
    /// An empty file: the string table holds the empty string.
    #[must_use]
    pub fn new() -> Self {
        let mut w = Self::default();
        w.id("");
        w
    }

    /// The id of `s` in the file's table, added the first time: the id the string calls take, for a caller that
    /// meets the same string many times and keeps its id ([`Writer::element_id`], [`Writer::attribute_ids`],
    /// [`Writer::text_id`]). Ids are given in the order strings are first used, so a string must be passed here
    /// where the string call would have taken it.
    pub fn id(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        // past u32::MAX strings the ids saturate, and finish() refuses the table (it is past 4 GB by then)
        let i = u32::try_from(self.offsets.len()).unwrap_or(u32::MAX);
        self.offsets.push(u32::try_from(self.blob.len()).unwrap_or(u32::MAX));
        self.blob.extend_from_slice(s.as_bytes());
        self.blob.push(0);
        self.index.insert(s.into(), i);
        i
    }

    fn put(&mut self, id: u32) {
        put_varint(&mut self.body, u64::from(id));
    }

    /// The string of an id (empty for one the table does not have).
    fn string(&self, id: u32) -> String {
        let at = self.offsets.get(id as usize).map_or(self.blob.len(), |&o| o as usize);
        let s = self.blob.get(at..).unwrap_or_default();
        String::from_utf8_lossy(s.split(|&b| b == 0).next().unwrap_or_default()).into_owned()
    }

    /// An element's tag and head byte (its kind and the number of attributes that follow).
    ///
    /// # Errors
    /// More than 15 attributes, a kind above 15, or an empty tag (string 0, which reads back as the end of a
    /// container; the Python writer writes it).
    pub fn element(&mut self, tag: &str, kind: u8, attributes: usize) -> Result<()> {
        if attributes > 15 || kind > 0xF || tag.is_empty() {
            // the error, before the tag is added to the table
            return Err(Self::element_error(tag.to_owned(), kind, attributes));
        }
        let id = self.id(tag);
        self.element_id(id, kind, attributes)
    }

    /// Why an element cannot be written (one of its attribute count, kind and tag is out of bounds).
    fn element_error(tag: String, kind: u8, attributes: usize) -> DbxError {
        if attributes > 15 {
            DbxError::TooManyAttributes { tag, count: attributes }
        } else if kind > 0xF {
            DbxError::Unwritable(format!("{tag}: kind {kind} does not fit 4 bits"))
        } else {
            DbxError::Unwritable("an element with an empty tag (string 0 reads back as the end of a container)".into())
        }
    }

    /// [`Writer::element`] of a tag given by its [`Writer::id`].
    ///
    /// # Errors
    /// As [`Writer::element`] (id 0 is the empty string).
    pub fn element_id(&mut self, tag: u32, kind: u8, attributes: usize) -> Result<()> {
        if attributes > 15 || kind > 0xF || tag == 0 {
            return Err(Self::element_error(self.string(tag), kind, attributes));
        }
        self.put(tag);
        self.body.push((kind << 4) | attributes as u8);
        Ok(())
    }

    /// One `(name, value)` attribute of the element just begun.
    pub fn attribute(&mut self, name: &str, value: &str) {
        let k = self.id(name);
        self.put(k);
        let v = self.id(value);
        self.put(v);
    }

    /// [`Writer::attribute`] of two [`Writer::id`]s.
    pub fn attribute_ids(&mut self, name: u32, value: u32) {
        self.put(name);
        self.put(value);
    }

    /// The start of a container's children (its flag byte).
    pub fn open(&mut self) {
        self.body.push(0);
    }

    /// The end of a container's children.
    pub fn close(&mut self) {
        self.body.push(0);
    }

    /// A text element's string.
    pub fn text(&mut self, text: &str) {
        let t = self.id(text);
        self.put(t);
    }

    /// [`Writer::text`] of a [`Writer::id`].
    pub fn text_id(&mut self, text: u32) {
        self.put(text);
    }

    /// Any other element's value bytes as they are stored (a numbers element's count and size varints and values,
    /// or a two-byte value).
    pub fn raw(&mut self, bytes: &[u8]) {
        self.body.extend_from_slice(bytes);
    }

    /// The file: header, string table, body.
    ///
    /// # Errors
    /// A string table past 4 GB.
    pub fn finish(self) -> Result<Vec<u8>> {
        let big = |_| DbxError::Unwritable(TOO_BIG.into());
        let count = u32::try_from(self.offsets.len()).map_err(big)?;
        let area = u32::try_from(4 * self.offsets.len() + self.blob.len() + 24).map_err(big)? - 24;
        let mut out = Vec::with_capacity(24 + area as usize + self.body.len());
        out.extend_from_slice(MAGIC);
        for w in [area + 24, 0, area, count] {
            out.extend_from_slice(&w.to_be_bytes());
        }
        for o in &self.offsets {
            out.extend_from_slice(&o.to_be_bytes());
        }
        out.extend_from_slice(&self.blob);
        out.extend_from_slice(&self.body);
        Ok(out)
    }
}

fn emit(f: &DbxFile, e: &Element, w: &mut Writer) -> Result<()> {
    w.element(f.tag(e), e.kind, e.attributes.len())?;
    for (k, v) in &e.attributes {
        w.attribute(f.str(*k), f.str(*v));
    }
    // by kind, as the reference: a body that does not match its kind writes nothing (a container's children, a
    // text) or its raw bytes
    match (e.kind, &e.body) {
        (CONTAINER, body) => {
            w.open();
            if let Body::Container(children) = body {
                for c in children {
                    emit(f, c, w)?;
                }
            }
            w.close();
        }
        (TEXT, body) => w.text(match body {
            Body::Text(t) => f.str(*t),
            _ => "",
        }),
        (_, body) => w.raw(match body {
            Body::Numbers(r) => r,
            Body::Scalar(r) => r,
            Body::Container(_) | Body::Text(_) => &[],
        }),
    }
    Ok(())
}

/// A tree as a binary `.dbx`, as `dbx.write`.
///
/// # Errors
/// An element with an empty tag, more than 15 attributes or a kind above 15, or a string table past 4 GB
/// (the Python writer writes an empty tag, which then reads back as the end of its container).
///
/// ```
/// use bc2_formats::dbx::{self, DbxFile, Element};
///
/// let mut f = DbxFile::default();
/// let inst = Element::container(&mut f, "instance", &[("guid", "1")], vec![]);
/// let part = Element::container(&mut f, "partition", &[("guid", "p")], vec![inst]);
/// f.roots.push(part);
/// let back = dbx::parse(&dbx::write(&f)?)?;
/// assert_eq!(back.attr(back.instances().next().unwrap(), "guid"), Some("1"));
/// # Ok::<(), dbx::DbxError>(())
/// ```
pub fn write(f: &DbxFile) -> Result<Vec<u8>> {
    let mut w = Writer::new();
    for r in &f.roots {
        emit(f, r, &mut w)?;
    }
    w.finish()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::dbx::parse;

    #[test]
    fn varints() {
        for (v, want) in [
            (0u64, vec![0u8]),
            (127, vec![0x7F]),
            (128, vec![0x80, 1]),
            (u64::MAX, {
                let mut m = vec![0xFF; 9];
                m.push(1);
                m
            }),
        ] {
            let mut out = Vec::new();
            put_varint(&mut out, v);
            assert_eq!(out, want);
        }
    }

    #[test]
    fn round_trip_keeps_the_tree() {
        let mut f = DbxFile::default();
        let a = Element::text(&mut f, "field", &[("name", "A")], "x");
        let b = Element::numbers(&mut f, "field", &[("name", "B")], 4, &[0, 0, 0, 1]);
        let c = Element::scalar(&mut f, "field", 6, &[("name", "C")], [0, 1]);
        let inst = Element::container(&mut f, "instance", &[("guid", "g")], vec![a, b, c]);
        f.roots.push(inst);
        let bytes = write(&f).unwrap();
        let back = parse(&bytes).unwrap();
        // the same tree, through each file's own table
        assert_eq!(write(&back).unwrap(), bytes);
        assert_eq!(back.strings.first().map(String::as_str), Some(""));
    }

    #[test]
    fn bodies_follow_the_kind() {
        // as dbx.write: the kind decides what is written, whatever the body holds
        let mut f = DbxFile::default();
        let tag = f.intern("e");
        let t = f.intern("t");
        let el = |kind, body| Element {
            tag,
            kind,
            attributes: vec![],
            body,
        };
        f.roots.extend([
            el(CONTAINER, Body::Text(t)),
            el(TEXT, Body::Scalar([1, 2])),
            el(6, Body::Container(vec![])),
            el(7, Body::Text(t)),
        ]);
        // a container with its flag and end, a text of string 0, kinds 6 and 7 without value bytes
        assert!(
            write(&f)
                .unwrap()
                .ends_with(&[1, 0xA0, 0, 0, 1, 0x20, 0, 1, 0x60, 1, 0x70])
        );
    }

    #[test]
    fn refuses_unwritable_elements() {
        let mut f = DbxFile::default();
        let attrs: Vec<(String, String)> = (0..16).map(|i| (format!("k{i}"), "v".to_owned())).collect();
        let attrs: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        let e = Element::text(&mut f, "field", &attrs, "x");
        f.roots.push(e);
        assert!(matches!(write(&f), Err(DbxError::TooManyAttributes { count: 16, .. })));
        let mut g = DbxFile::default();
        let e = Element::scalar(&mut g, "field", 0x10, &[], [0, 0]);
        g.roots.push(e);
        assert!(matches!(write(&g), Err(DbxError::Unwritable(_))));
        let mut h = DbxFile::default();
        let e = Element::text(&mut h, "", &[], "x");
        h.roots.push(e);
        assert!(matches!(write(&h), Err(DbxError::Unwritable(_))));
    }

    #[test]
    fn id_calls_name_the_tag_in_errors() {
        let mut w = Writer::new();
        let t = w.id("field");
        let attrs = w.element_id(t, 2, 16).unwrap_err();
        assert!(attrs.to_string().contains("field: 16 attributes"), "{attrs}");
        assert!(
            w.element_id(t, 16, 0)
                .unwrap_err()
                .to_string()
                .contains("field: kind 16")
        );
        assert!(w.element_id(0, 2, 0).unwrap_err().to_string().contains("empty tag"));
        // an id the table does not have reads as the empty string
        assert!(w.element_id(99, 2, 16).unwrap_err().to_string().starts_with(": 16"));
        assert!(w.element("", 2, 0).is_err() && w.element("x", 16, 0).is_err());
        w.element_id(t, 2, 1).unwrap();
        let (k, v) = (w.id("name"), w.id("N"));
        w.attribute_ids(k, v);
        w.text_id(v);
        let back = parse(&w.finish().unwrap()).unwrap();
        assert_eq!(back.strings, ["", "field", "name", "N"]);
        assert_eq!(back.roots.first().and_then(|r| back.attr(r, "name")), Some("N"));
    }

    #[test]
    fn hasher_reads_every_byte() {
        let h = |s: &[u8]| {
            let mut x = Fx::default();
            x.write(s);
            x.finish()
        };
        assert_ne!(h(b"0123456789"), h(b"0123456788"));
        assert_ne!(h(b"01234567"), h(b"01234568"));
    }
}
