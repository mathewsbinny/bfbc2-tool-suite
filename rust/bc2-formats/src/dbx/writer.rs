//! The binary writer, as `dbx.write`: a fresh string table (the empty string first, then each string in the order
//! the tree first uses it), container flag bytes 0, header value 0. Not byte-identical to a game file (DbxEdit
//! keeps a file's own string order), but the same tree: `parse(write(f))` gives `f`'s tree back.

use std::collections::HashMap;

use super::error::{DbxError, Result};
use super::model::{Body, CONTAINER, DbxFile, Element};
use super::strings::MAGIC;

/// Appends `v` as an unsigned LEB128 varint, as `dbx.put_varint`.
///
/// ```
/// let mut out = Vec::new();
/// bc2_formats::dbx::put_varint(&mut out, 300);
/// assert_eq!(out, [0xAC, 0x02]);
/// ```
pub fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    loop {
        let b = (v & 0x7F) as u8;
        v >>= 7;
        if v == 0 {
            out.push(b);
            return;
        }
        out.push(b | 0x80);
    }
}

struct Table<'a> {
    index: HashMap<&'a str, u64>,
    order: Vec<&'a str>,
}

impl<'a> Table<'a> {
    fn id(&mut self, s: &'a str) -> u64 {
        if let Some(&i) = self.index.get(s) {
            return i;
        }
        let i = self.order.len() as u64;
        self.index.insert(s, i);
        self.order.push(s);
        i
    }
}

fn emit<'a>(f: &'a DbxFile, e: &Element, table: &mut Table<'a>, body: &mut Vec<u8>) -> Result<()> {
    let n_attr = e.attributes.len();
    if n_attr > 15 {
        return Err(DbxError::TooManyAttributes {
            tag: f.tag(e).to_owned(),
            count: n_attr,
        });
    }
    if e.kind > 0xF {
        return Err(DbxError::Unwritable(format!(
            "{}: kind {} does not fit 4 bits",
            f.tag(e),
            e.kind
        )));
    }
    let tag = table.id(f.str(e.tag));
    if tag == 0 {
        // string 0 is the empty string, and tag 0 closes a container: the file would not read back
        return Err(DbxError::Unwritable("an element with an empty tag".into()));
    }
    put_varint(body, tag);
    body.push((e.kind << 4) | n_attr as u8);
    for (k, v) in &e.attributes {
        let k = table.id(f.str(*k));
        let v = table.id(f.str(*v));
        put_varint(body, k);
        put_varint(body, v);
    }
    match &e.body {
        Body::Container(children) if e.kind == CONTAINER => {
            body.push(0);
            for c in children {
                emit(f, c, table, body)?;
            }
            body.push(0);
        }
        Body::Text(t) => {
            let t = table.id(f.str(*t));
            put_varint(body, t);
        }
        // as the reference: a body that does not match its kind writes its raw bytes (none for a container)
        other => body.extend_from_slice(match other {
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
    let mut table = Table {
        index: HashMap::from([("", 0)]),
        order: vec![""],
    };
    let mut body = Vec::new();
    for r in &f.roots {
        emit(f, r, &mut table, &mut body)?;
    }
    let mut offsets = Vec::with_capacity(table.order.len());
    let mut blob = Vec::new();
    for s in &table.order {
        offsets.push(u32::try_from(blob.len()).map_err(|_| DbxError::Unwritable("string table past 4 GB".into()))?);
        blob.extend_from_slice(s.as_bytes());
        blob.push(0);
    }
    let too_big = || DbxError::Unwritable("string table past 4 GB".into());
    let count = u32::try_from(table.order.len()).map_err(|_| too_big())?;
    let area = u32::try_from(4 * table.order.len() + blob.len()).map_err(|_| too_big())?;
    let mut out = Vec::with_capacity(24 + area as usize + body.len());
    out.extend_from_slice(MAGIC);
    for w in [area.checked_add(24).ok_or_else(too_big)?, 0, area, count] {
        out.extend_from_slice(&w.to_be_bytes());
    }
    for o in offsets {
        out.extend_from_slice(&o.to_be_bytes());
    }
    out.extend_from_slice(&blob);
    out.extend_from_slice(&body);
    Ok(out)
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
}
