//! `DbxEdit.cs`' own model of a binary `.dbx` (its `DbxFile`, `Element`, `Varint`): the string table as stored
//! (kept byte for byte, offsets and padding included, until a string is added or changed), the header's kept value,
//! each container's flag byte, every value's bytes as stored, and whatever follows the tree. Anything an edit does not
//! change is written back byte for byte.
//!
//! String indexes are kept as the file gives them (`i32`, possibly out of range in a damaged file): DbxEdit checks
//! them only where it reads the string, so a command fails exactly where DbxEdit's would.

use std::collections::HashMap;

use super::fault::{Fault, Result};

/// A container (children).
pub const CONTAINER: i32 = 0xA;
/// A text value or reference (a string index).
pub const TEXT: i32 = 2;
/// Big-endian numbers.
pub const NUMBERS: i32 = 7;

/// The deepest nesting read or written (real files nest about 10 deep; DbxEdit would overflow its stack).
pub const MAX_DEPTH: usize = 256;

/// `Varint.Read`: 7 bits per byte, low group first, as a 32-bit `int` (shifts past 31 bits wrap, as C#'s do).
///
/// # Errors
/// A read past the end of `data`.
pub fn read_varint(data: &[u8], pos: &mut usize) -> Result<i32> {
    let mut value: i32 = 0;
    let mut shift: u32 = 0;
    loop {
        let b = *data.get(*pos).ok_or_else(Fault::index)?;
        *pos += 1;
        value |= i32::from(b & 0x7F).wrapping_shl(shift);
        if b < 0x80 {
            return Ok(value);
        }
        shift = shift.wrapping_add(7);
    }
}

/// `Varint.Write`: a negative value takes five bytes.
pub fn write_varint(out: &mut Vec<u8>, value: i32) {
    let mut v = value as u32;
    while v >= 0x80 {
        out.push((v & 0x7F) as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
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

/// One element (FORMAT.md section 2). Containers have `children`; every other kind keeps its value bytes exactly as
/// stored (`payload`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Element {
    /// The tag's string index.
    pub tag: i32,
    /// The kind (the high four bits of the head byte).
    pub kind: i32,
    /// `(name, value)` string indexes.
    pub attributes: Vec<(i32, i32)>,
    /// A container's flag byte.
    pub container_byte: u8,
    /// A container's children (`None`: a value).
    pub children: Option<Vec<Element>>,
    /// A value's bytes as stored (empty for a container).
    pub payload: Vec<u8>,
}

impl Element {
    /// Whether it has children.
    #[must_use]
    pub fn is_container(&self) -> bool {
        self.children.is_some()
    }

    /// The children, or none.
    #[must_use]
    pub fn kids(&self) -> &[Element] {
        self.children.as_deref().unwrap_or(&[])
    }
}

/// The string table, with an index of each string's first entry once lookups are many.
#[derive(Debug, Clone, Default)]
pub struct Table {
    /// The strings in file order (duplicates included).
    pub strings: Vec<String>,
    /// Whether a string was added or changed (the string area is then written anew).
    pub changed: bool,
    index: Option<HashMap<String, i32>>,
    lookups: u32,
}

impl Table {
    /// A table of these strings, unchanged.
    #[must_use]
    pub fn new(strings: Vec<String>) -> Self {
        Self {
            strings,
            ..Self::default()
        }
    }

    /// `Table[id]`.
    ///
    /// # Errors
    /// An index the table does not have (`ArgumentOutOfRangeException`).
    pub fn s(&self, id: i32) -> Result<&str> {
        usize::try_from(id)
            .ok()
            .and_then(|i| self.strings.get(i))
            .map(String::as_str)
            .ok_or_else(Fault::list_index)
    }

    /// `Intern`: the first identical entry, or a new one at the end.
    pub fn intern(&mut self, s: &str) -> i32 {
        self.lookups += 1;
        if self.index.is_none() && self.lookups > 16 {
            let mut index = HashMap::with_capacity(self.strings.len());
            for (i, t) in self.strings.iter().enumerate() {
                index.entry(t.clone()).or_insert(i as i32);
            }
            self.index = Some(index);
        }
        let found = match &self.index {
            Some(index) => index.get(s).copied(),
            None => self.strings.iter().position(|t| t == s).map(|i| i as i32),
        };
        if let Some(i) = found {
            return i;
        }
        self.strings.push(s.to_owned());
        self.changed = true;
        let i = (self.strings.len() - 1) as i32;
        if let Some(index) = &mut self.index {
            index.insert(s.to_owned(), i);
        }
        i
    }

    /// Entry `i` set to `s` (a replace).
    pub fn set(&mut self, i: usize, s: String) {
        if let Some(slot) = self.strings.get_mut(i) {
            *slot = s;
            self.changed = true;
            self.index = None;
        }
    }

    /// `Get(e, name)`: the value of the first attribute named `name`.
    ///
    /// # Errors
    /// A string index the table does not have, met before the attribute.
    pub fn get(&self, e: &Element, name: &str) -> Result<Option<&str>> {
        for &(k, v) in &e.attributes {
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
    pub fn tag(&self, e: &Element) -> Result<&str> {
        self.s(e.tag)
    }

    /// The value of the first attribute, or `""` (`FirstValue`, the field a value is typed by).
    ///
    /// # Errors
    /// A string index the table does not have.
    pub fn first_value(&self, e: &Element) -> Result<&str> {
        match e.attributes.first() {
            Some(&(_, v)) => self.s(v),
            None => Ok(""),
        }
    }
}

/// A parsed `.dbx` as DbxEdit holds it.
#[derive(Debug, Clone, Default)]
pub struct EditFile {
    /// The bytes it was read from.
    pub original: Vec<u8>,
    /// Whether it is a binary `.dbx` (`{binary}` and more than the 24-byte header); nothing else is read otherwise.
    pub is_binary: bool,
    /// The string table.
    pub table: Table,
    /// The root elements (the partition first).
    pub top_level: Vec<Element>,
    /// The header's word at offset 12, kept as it was.
    pub kept_header_value: u32,
    original_offsets: Vec<u32>,
    original_string_area: Vec<u8>,
    /// The closing 0 of the tree and anything after it.
    pub trailer: Vec<u8>,
}

impl EditFile {
    /// A new, empty file: string 0 the empty string, header value 0, no trailing bytes.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            is_binary: true,
            table: Table {
                strings: vec![String::new()],
                changed: true,
                ..Table::default()
            },
            ..Self::default()
        }
    }

    /// Reads `data` as DbxEdit's `new DbxFile(data)`.
    ///
    /// # Errors
    /// What DbxEdit's reader throws on a damaged file (a read past the end, a negative size).
    pub fn parse(data: Vec<u8>) -> Result<Self> {
        let is_binary = data.len() > 24 && data.starts_with(b"{binary}");
        if !is_binary {
            return Ok(Self {
                original: data,
                ..Self::default()
            });
        }
        let area_size = be_u32(&data, 16)?;
        let count = be_u32(&data, 20)?;
        let kept = be_u32(&data, 12)?;
        let mut offsets = Vec::new();
        for i in 0..u64::from(count) {
            let at = usize::try_from(24 + 4 * i).map_err(|_| Fault::index())?;
            offsets.push(be_u32(&data, at)?);
        }
        let strings_at = 24i64 + 4 * i64::from(count as i32);
        let payload_at = 24i64 + i64::from(area_size as i32);
        let mut strings = Vec::with_capacity(offsets.len());
        for &o in &offsets {
            let from = strings_at + i64::from(o as i32);
            let start = usize::try_from(from).map_err(|_| Fault::index())?;
            let tail = data.get(start..).ok_or_else(Fault::index)?;
            let len = tail.iter().position(|&b| b == 0).ok_or_else(Fault::index)?;
            strings.push(String::from_utf8_lossy(tail.get(..len).unwrap_or_default()).into_owned());
        }
        if payload_at < strings_at {
            return Err(Fault::overflow());
        }
        let area = data
            .get(strings_at as usize..payload_at as usize)
            .ok_or_else(Fault::array_copy)?
            .to_vec();
        let mut top = Vec::new();
        let mut pos = usize::try_from(payload_at).map_err(|_| Fault::index())?;
        while pos < data.len() {
            let tag = read_varint(&data, &mut pos)?;
            if tag == 0 {
                pos -= 1;
                break;
            }
            top.push(read_element(&data, &mut pos, tag, 0)?);
        }
        let trailer = data.get(pos..).ok_or_else(Fault::overflow)?.to_vec();
        Ok(Self {
            is_binary,
            table: Table::new(strings),
            top_level: top,
            kept_header_value: kept,
            original_offsets: offsets,
            original_string_area: area,
            trailer,
            original: data,
        })
    }

    /// `Serialize()`: the header, the string table (as stored unless it changed), the tree, the trailer.
    ///
    /// # Errors
    /// An element with more than 15 attributes, or nesting past [`MAX_DEPTH`].
    pub fn serialize(&self) -> Result<Vec<u8>> {
        let mut fresh_area = Vec::new();
        let mut fresh_offsets = Vec::new();
        let (area, offsets): (&[u8], &[u32]) = if self.table.changed {
            fresh_offsets.reserve(self.table.strings.len());
            for s in &self.table.strings {
                fresh_offsets.push(fresh_area.len() as u32);
                fresh_area.extend_from_slice(s.as_bytes());
                fresh_area.push(0);
            }
            (&fresh_area, &fresh_offsets)
        } else {
            (&self.original_string_area, &self.original_offsets)
        };
        let mut tree = Vec::with_capacity(self.original.len());
        for e in &self.top_level {
            write_element(&mut tree, e, 0)?;
        }
        tree.extend_from_slice(&self.trailer);
        let area_size = (4 * self.table.strings.len() + area.len()) as u32;
        let mut out = Vec::with_capacity(24 + area_size as usize + tree.len());
        out.extend_from_slice(b"{binary}");
        for w in [
            area_size.wrapping_add(24),
            self.kept_header_value,
            area_size,
            self.table.strings.len() as u32,
        ] {
            out.extend_from_slice(&w.to_be_bytes());
        }
        for o in offsets {
            out.extend_from_slice(&o.to_be_bytes());
        }
        out.extend_from_slice(area);
        out.extend_from_slice(&tree);
        Ok(out)
    }

    /// The partition's children (`TopLevel[0].Children`).
    ///
    /// # Errors
    /// A file without a partition (not binary, or empty), or a partition that is not a container.
    pub fn partition_children(&self) -> Result<&[Element]> {
        if !self.is_binary {
            return Err(Fault::null());
        }
        let p = self.top_level.first().ok_or_else(Fault::list_index)?;
        p.children.as_deref().ok_or_else(Fault::null)
    }

    /// `Instances()`: the partition's `instance` children, read lazily as DbxEdit's LINQ query is (a damaged tag
    /// fails only when reached).
    ///
    /// # Errors
    /// As [`EditFile::partition_children`].
    pub fn instances(&self) -> Result<impl Iterator<Item = Result<&Element>>> {
        let t = &self.table;
        Ok(self.partition_children()?.iter().filter_map(move |e| match t.tag(e) {
            Ok("instance") => Some(Ok(e)),
            Ok(_) => None,
            Err(f) => Some(Err(f)),
        }))
    }

    /// `Instance(guid)`: the first instance whose guid is `guid` (any case).
    ///
    /// # Errors
    /// As [`EditFile::instances`], or a damaged attribute met on the way.
    pub fn instance(&self, guid: &str) -> Result<Option<&Element>> {
        for i in self.instances()? {
            let i = i?;
            if self
                .table
                .get(i, "guid")?
                .is_some_and(|g| super::net::same_ignore_case(g, guid))
            {
                return Ok(Some(i));
            }
        }
        Ok(None)
    }
}

fn read_element(data: &[u8], pos: &mut usize, tag: i32, depth: usize) -> Result<Element> {
    if depth > MAX_DEPTH {
        return Err(Fault::invalid(format!("elements nested more than {MAX_DEPTH} deep")));
    }
    let head = *data.get(*pos).ok_or_else(Fault::index)?;
    *pos += 1;
    let mut e = Element {
        tag,
        kind: i32::from(head >> 4),
        ..Element::default()
    };
    for _ in 0..(head & 0xF) {
        let name = read_varint(data, pos)?;
        e.attributes.push((name, read_varint(data, pos)?));
    }
    if e.kind == CONTAINER {
        e.container_byte = *data.get(*pos).ok_or_else(Fault::index)?;
        *pos += 1;
        let mut children = Vec::new();
        loop {
            let child = read_varint(data, pos)?;
            if child == 0 {
                break;
            }
            children.push(read_element(data, pos, child, depth + 1)?);
        }
        e.children = Some(children);
        return Ok(e);
    }
    let value_at = *pos;
    let end: i64 = if e.kind == TEXT {
        read_varint(data, pos)?;
        *pos as i64
    } else if e.kind == NUMBERS {
        let n = read_varint(data, pos)?;
        let size = read_varint(data, pos)?;
        *pos as i64 + i64::from(n.wrapping_mul(size))
    } else {
        *pos as i64 + 2
    };
    if end < value_at as i64 {
        return Err(Fault::overflow());
    }
    e.payload = data.get(value_at..end as usize).ok_or_else(Fault::array_copy)?.to_vec();
    *pos = end as usize;
    Ok(e)
}

fn write_element(out: &mut Vec<u8>, e: &Element, depth: usize) -> Result<()> {
    if depth > MAX_DEPTH {
        return Err(Fault::invalid(format!("elements nested more than {MAX_DEPTH} deep")));
    }
    write_varint(out, e.tag);
    if e.attributes.len() > 15 {
        return Err(Fault::invalid(format!(
            "an element with {} attributes (15 at most fit)",
            e.attributes.len()
        )));
    }
    out.push(((e.kind << 4) | e.attributes.len() as i32) as u8);
    for &(k, v) in &e.attributes {
        write_varint(out, k);
        write_varint(out, v);
    }
    if e.kind != CONTAINER {
        out.extend_from_slice(&e.payload);
        return Ok(());
    }
    out.push(e.container_byte);
    for c in e.children.as_deref().ok_or_else(Fault::null)? {
        write_element(out, c, depth + 1)?;
    }
    out.push(0);
    Ok(())
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
        let f = EditFile::empty();
        let b = f.serialize().unwrap();
        let back = EditFile::parse(b.clone()).unwrap();
        assert!(back.is_binary && back.top_level.is_empty());
        assert_eq!(back.serialize().unwrap(), b);
        let mut g = EditFile::empty();
        let tag = g.table.intern("partition");
        g.top_level.push(Element {
            tag,
            kind: CONTAINER,
            children: Some(vec![]),
            ..Element::default()
        });
        let b = g.serialize().unwrap();
        let back = EditFile::parse(b.clone()).unwrap();
        assert!(back.is_binary && !back.table.changed);
        assert_eq!(back.serialize().unwrap(), b);
        assert_eq!(back.partition_children().unwrap().len(), 0);
        assert!(back.instance("x").unwrap().is_none());
    }

    #[test]
    fn damaged_files_fail() {
        let mut g = EditFile::empty();
        let tag = g.table.intern("p");
        g.top_level.push(Element {
            tag,
            kind: 6,
            payload: vec![1, 1],
            ..Element::default()
        });
        let b = g.serialize().unwrap();
        for cut in 25..b.len() {
            let _ = EditFile::parse(b[..cut].to_vec());
        }
        assert!(EditFile::parse(b[..b.len() - 1].to_vec()).is_err());
        assert!(!EditFile::parse(b"short".to_vec()).unwrap().is_binary);
        assert!(
            EditFile::parse(b"short".to_vec())
                .unwrap()
                .partition_children()
                .is_err()
        );
    }

    #[test]
    fn interning_reuses_the_first_entry() {
        let mut t = Table::new(vec![String::new(), "a".into(), "a".into()]);
        assert_eq!(t.intern("a"), 1);
        assert!(!t.changed);
        for i in 0..40 {
            t.intern(&format!("s{i}"));
        }
        assert_eq!(t.intern("a"), 1);
        assert_eq!(t.intern("s3"), 6);
        t.set(1, "b".into());
        assert_eq!(t.intern("a"), 2);
        assert!(t.s(-1).is_err() && t.s(999).is_err());
    }
}
