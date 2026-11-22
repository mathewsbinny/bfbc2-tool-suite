//! `DbxEdit.cs`' reading and writing rules on the suite's one editable document ([`EditDbx`], round PR1b-2: the
//! GR4 port's own tree, `EditFile`, merged into it). DbxEdit's reader keeps the string table as stored (offsets and
//! padding included, written back byte for byte until a string is added or changed), the header's kept value, each
//! container's flag byte, every value's bytes as stored, and whatever follows the tree. Anything an edit does not
//! change is written back byte for byte.
//!
//! String indexes are kept as the file gives them (C#'s `int` bits, possibly out of range in a damaged file): DbxEdit
//! checks them only where it reads the string ([`Table`]), so a command fails exactly where DbxEdit's would. Strings
//! are text as .NET decodes them (UTF-8, a malformed sequence replaced by U+FFFD), which only shows once the table is
//! written anew; until then the stored bytes are written back.

use std::borrow::Cow;

use crate::dbx::edit::varint_i32;
pub use crate::dbx::edit::{EditDbx, NodeId, Parts};
pub use crate::dbx::{CONTAINER, NUMBERS, TEXT};

use super::fault::{Fault, Result};

/// The deepest nesting read or written (real files nest about 10 deep; DbxEdit would overflow its stack).
pub const MAX_DEPTH: usize = 256;

/// `Varint.Read`: 7 bits per byte, low group first, as a 32-bit `int` (shifts past 31 bits wrap, as C#'s do).
///
/// # Errors
/// A read past the end of `data`.
pub fn read_varint(data: &[u8], pos: &mut usize) -> Result<i32> {
    varint_i32(data, pos).ok_or_else(Fault::index)
}

/// `Varint.Write`: a negative value takes five bytes.
pub fn write_varint(out: &mut Vec<u8>, value: i32) {
    crate::dbx::put_varint(out, u64::from(value as u32));
}

/// `Varint.Bytes`.
#[must_use]
pub fn varint_bytes(value: i32) -> Vec<u8> {
    let mut v = Vec::with_capacity(5);
    write_varint(&mut v, value);
    v
}

/// `BigEndian.U32`.
///
/// # Errors
/// A read past the end.
pub fn be_u32(data: &[u8], at: usize) -> Result<u32> {
    data.get(at..at.checked_add(4).ok_or_else(Fault::index)?)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_be_bytes)
        .ok_or_else(Fault::index)
}

/// A new, empty file as DbxEdit makes one: string 0 the empty string, header value 0, no trailing bytes.
#[must_use]
pub fn empty() -> EditDbx {
    let mut d = EditDbx::new();
    d.intern(b"");
    d
}

/// Reads `data` as DbxEdit's `new DbxFile(data)`: a file that is not binary (`{binary}` and more than the 24-byte
/// header) is kept as its bytes, with no tree.
///
/// # Errors
/// What DbxEdit's reader throws on a damaged file (a read past the end, a negative size).
pub fn read(data: &[u8]) -> Result<EditDbx> {
    if !(data.len() > 24 && data.starts_with(b"{binary}")) {
        return Ok(EditDbx::not_binary(data));
    }
    let area_size = be_u32(data, 16)?;
    let count = be_u32(data, 20)?;
    let kept = be_u32(data, 12)?;
    let mut offsets = Vec::new();
    for i in 0..u64::from(count) {
        let at = usize::try_from(24 + 4 * i).map_err(|_| Fault::index())?;
        offsets.push(be_u32(data, at)?);
    }
    let strings_at = 24i64 + 4 * i64::from(count as i32);
    let payload_at = 24i64 + i64::from(area_size as i32);
    let mut strings = Vec::with_capacity(offsets.len());
    for &o in &offsets {
        let from = strings_at + i64::from(o as i32);
        let start = usize::try_from(from).map_err(|_| Fault::index())?;
        let tail = data.get(start..).ok_or_else(Fault::index)?;
        let len = tail.iter().position(|&b| b == 0).ok_or_else(Fault::index)?;
        let bytes = tail.get(..len).unwrap_or_default();
        strings.push(match String::from_utf8_lossy(bytes) {
            Cow::Borrowed(_) => bytes.to_vec(),
            Cow::Owned(s) => s.into_bytes(),
        });
    }
    if payload_at < strings_at {
        return Err(Fault::overflow());
    }
    if data.get(strings_at as usize..payload_at as usize).is_none() {
        return Err(Fault::array_copy());
    }
    let mut d = EditDbx::from_file(data, kept, strings, (24, payload_at as usize));
    let mut pos = usize::try_from(payload_at).map_err(|_| Fault::index())?;
    let mut roots = Vec::new();
    while pos < data.len() {
        let tag = read_varint(data, &mut pos)?;
        if tag == 0 {
            pos -= 1;
            break;
        }
        roots.push(read_element(&mut d, data, &mut pos, tag, 0)?);
    }
    *d.roots_mut() = roots;
    d.set_trailing(data.get(pos..).ok_or_else(Fault::overflow)?.to_vec());
    Ok(d)
}

fn read_element(d: &mut EditDbx, data: &[u8], pos: &mut usize, tag: i32, depth: usize) -> Result<NodeId> {
    if depth > MAX_DEPTH {
        return Err(Fault::invalid(format!("elements nested more than {MAX_DEPTH} deep")));
    }
    let head = *data.get(*pos).ok_or_else(Fault::index)?;
    *pos += 1;
    let kind = head >> 4;
    let mut attrs = Vec::with_capacity(usize::from(head & 0xF));
    for _ in 0..(head & 0xF) {
        let name = read_varint(data, pos)?;
        attrs.push((name as u32, read_varint(data, pos)? as u32));
    }
    let mut parts = Parts {
        tag: tag as u32,
        kind,
        attrs,
        ..Parts::default()
    };
    if kind == CONTAINER {
        parts.cbyte = *data.get(*pos).ok_or_else(Fault::index)?;
        *pos += 1;
        parts.children = Some(Vec::new());
        let id = d.push_read(parts, (0, 0), 0);
        let mut children = Vec::new();
        loop {
            let child = read_varint(data, pos)?;
            if child == 0 {
                break;
            }
            children.push(read_element(d, data, pos, child, depth + 1)?);
        }
        d.set_children(id, children);
        return Ok(id);
    }
    let value_at = *pos;
    let mut text = 0;
    let end: i64 = if kind == TEXT {
        text = read_varint(data, pos)? as u32;
        *pos as i64
    } else if kind == NUMBERS {
        let n = read_varint(data, pos)?;
        let size = read_varint(data, pos)?;
        *pos as i64 + i64::from(n.wrapping_mul(size))
    } else {
        *pos as i64 + 2
    };
    if end < value_at as i64 {
        return Err(Fault::overflow());
    }
    if data.get(value_at..end as usize).is_none() {
        return Err(Fault::array_copy());
    }
    *pos = end as usize;
    Ok(d.push_read(parts, (value_at as u32, (end as usize - value_at) as u32), text))
}

/// `Serialize()`: the header, the string table (as stored unless it changed), the tree, the trailer.
///
/// # Errors
/// An element with more than 15 attributes, or nesting past [`MAX_DEPTH`] (DbxEdit's order: the first such element
/// in the file's order).
pub fn serialize(d: &EditDbx) -> Result<Vec<u8>> {
    let mut stack: Vec<(NodeId, usize)> = d.roots().iter().rev().map(|&r| (r, 0)).collect();
    while let Some((n, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(Fault::invalid(format!("elements nested more than {MAX_DEPTH} deep")));
        }
        let attrs = d.attr_ids(n).len();
        if attrs > 15 {
            return Err(Fault::invalid(format!(
                "an element with {attrs} attributes (15 at most fit)"
            )));
        }
        if d.kind(n) == CONTAINER {
            let kids = d.children(n).ok_or_else(Fault::null)?;
            stack.extend(kids.iter().rev().map(|&c| (c, depth + 1)));
        }
    }
    d.write().map_err(|e| Fault::invalid(e.0))
}

/// The string table as DbxEdit reads it: each string checked where it is read.
#[derive(Debug, Clone, Copy)]
pub struct Table<'d>(pub &'d EditDbx);

impl<'d> Table<'d> {
    /// `Table[id]`.
    ///
    /// # Errors
    /// An index the table does not have (`ArgumentOutOfRangeException`).
    pub fn s(self, id: u32) -> Result<&'d str> {
        if (id as i32) < 0 {
            return Err(Fault::list_index());
        }
        let b = self.0.string(id).ok_or_else(Fault::list_index)?;
        std::str::from_utf8(b).map_err(|_| Fault::invalid("a string that is not UTF-8"))
    }

    /// `Get(e, name)`: the value of the first attribute named `name`.
    ///
    /// # Errors
    /// A string index the table does not have, met before the attribute.
    pub fn get(self, e: NodeId, name: &str) -> Result<Option<&'d str>> {
        for &(k, v) in self.0.attr_ids(e) {
            if self.s(k)? == name {
                return Ok(Some(self.s(v)?));
            }
        }
        Ok(None)
    }

    /// `TagOf(e)`.
    ///
    /// # Errors
    /// A tag index the table does not have.
    pub fn tag(self, e: NodeId) -> Result<&'d str> {
        self.s(self.0.tag_id(e))
    }

    /// The value of the first attribute, or `""` (`FirstValue`, the field a value is typed by).
    ///
    /// # Errors
    /// A string index the table does not have.
    pub fn first_value(self, e: NodeId) -> Result<&'d str> {
        match self.0.attr_ids(e).first() {
            Some(&(_, v)) => self.s(v),
            None => Ok(""),
        }
    }

    /// The number of strings.
    #[must_use]
    pub fn len(self) -> usize {
        self.0.strings().len()
    }

    /// Whether the table is empty.
    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0.strings().is_empty()
    }
}

/// `Intern`: the first identical entry, or a new one at the end.
pub fn intern(d: &mut EditDbx, s: &str) -> u32 {
    d.intern(s.as_bytes())
}

/// The partition's children (`TopLevel[0].Children`).
///
/// # Errors
/// A file without a partition (not binary, or empty), or a partition that is not a container.
pub fn partition_children(d: &EditDbx) -> Result<&[NodeId]> {
    if !d.is_binary() {
        return Err(Fault::null());
    }
    let p = *d.roots().first().ok_or_else(Fault::list_index)?;
    d.children(p).ok_or_else(Fault::null)
}

/// The partition's children, to change.
///
/// # Errors
/// As [`partition_children`].
pub fn partition_children_mut(d: &mut EditDbx) -> Result<&mut Vec<NodeId>> {
    if !d.is_binary() {
        return Err(Fault::null());
    }
    let p = *d.roots().first().ok_or_else(Fault::list_index)?;
    d.children_mut(p).map_err(|_| Fault::null())
}

/// `Instances()`: the partition's `instance` children, read lazily as DbxEdit's LINQ query is (a damaged tag fails
/// only when reached).
///
/// # Errors
/// As [`partition_children`].
pub fn instances(d: &EditDbx) -> Result<impl Iterator<Item = Result<NodeId>> + '_> {
    let t = Table(d);
    Ok(partition_children(d)?.iter().filter_map(move |&e| match t.tag(e) {
        Ok("instance") => Some(Ok(e)),
        Ok(_) => None,
        Err(f) => Some(Err(f)),
    }))
}

/// `Instance(guid)`: the first instance whose guid is `guid` (any case).
///
/// # Errors
/// As [`instances`], or a damaged attribute met on the way.
pub fn instance(d: &EditDbx, guid: &str) -> Result<Option<NodeId>> {
    let t = Table(d);
    for i in instances(d)? {
        let i = i?;
        if t.get(i, "guid")?.is_some_and(|g| super::net::same_ignore_case(g, guid)) {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// The children of `e`, or none (`Element.Children ?? []`).
#[must_use]
pub fn kids(d: &EditDbx, e: NodeId) -> &[NodeId] {
    d.children(e).unwrap_or_default()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn varints_as_dbxedit() {
        for v in [
            0,
            1,
            127,
            128,
            255,
            16383,
            16384,
            2_097_151,
            2_097_152,
            i32::MAX,
            -1,
            i32::MIN,
        ] {
            let b = varint_bytes(v);
            let mut p = 0;
            assert_eq!(read_varint(&b, &mut p).unwrap(), v);
            assert_eq!(p, b.len());
        }
        assert_eq!(varint_bytes(-1).len(), 5);
        assert!(read_varint(&[0x80], &mut 0).is_err());
    }

    #[test]
    fn empty_file_round_trips() {
        let f = empty();
        let b = serialize(&f).unwrap();
        let back = read(&b).unwrap();
        assert!(back.is_binary() && back.roots().is_empty());
        assert_eq!(serialize(&back).unwrap(), b);
        let mut g = empty();
        let tag = intern(&mut g, "partition");
        let p = g.push(Parts {
            tag,
            kind: CONTAINER,
            children: Some(vec![]),
            ..Parts::default()
        });
        g.roots_mut().push(p);
        let b = serialize(&g).unwrap();
        let back = read(&b).unwrap();
        assert!(back.is_binary() && !back.strings_changed());
        assert_eq!(serialize(&back).unwrap(), b);
        assert_eq!(partition_children(&back).unwrap().len(), 0);
        assert!(instance(&back, "x").unwrap().is_none());
    }

    #[test]
    fn damaged_files_fail() {
        let mut g = empty();
        let tag = intern(&mut g, "p");
        let p = g.push(Parts {
            tag,
            kind: 6,
            payload: vec![1, 1],
            ..Parts::default()
        });
        g.roots_mut().push(p);
        let b = serialize(&g).unwrap();
        for cut in 25..b.len() {
            let _ = read(&b[..cut]);
        }
        assert!(read(&b[..b.len() - 1]).is_err());
        assert!(!read(b"short").unwrap().is_binary());
        assert!(partition_children(&read(b"short").unwrap()).is_err());
    }

    #[test]
    fn interning_reuses_the_first_entry() {
        let mut d = empty();
        assert_eq!(intern(&mut d, "a"), 1);
        let b = serialize(&d).unwrap();
        let mut t = read(&b).unwrap();
        assert!(!t.strings_changed());
        assert_eq!(intern(&mut t, "a"), 1);
        assert!(!t.strings_changed());
        t.set_string(0, b"a".to_vec());
        assert!(t.strings_changed());
        assert_eq!(intern(&mut t, "a"), 0);
        for i in 0..40 {
            intern(&mut t, &format!("s{i}"));
        }
        assert_eq!(intern(&mut t, "s3"), 5);
        t.set_string(0, b"b".to_vec());
        assert_eq!(intern(&mut t, "a"), 1);
        let tb = Table(&t);
        assert!(tb.s(u32::MAX).is_err() && tb.s(999).is_err());
    }

    #[test]
    fn stored_text_varints_are_kept() {
        // a text value whose index is stored in two bytes (0x81 0x00 = 1) comes back as stored
        let mut g = empty();
        let tag = intern(&mut g, "field");
        let v = intern(&mut g, "x");
        let e = g.push(Parts {
            tag,
            kind: TEXT,
            payload: vec![0x80 | v as u8, 0x00],
            ..Parts::default()
        });
        g.roots_mut().push(e);
        let b = serialize(&g).unwrap();
        let back = read(&b).unwrap();
        assert_eq!(serialize(&back).unwrap(), b);
        let e = back.roots()[0];
        assert_eq!(back.text_id(e), Some(v));
        assert_eq!(back.payload(e), [0x80 | v as u8, 0x00]);
        let strict = EditDbx::parse(&b).unwrap();
        assert_eq!(strict.write().unwrap(), b);
    }
}
