//! `XmlExport` of `DbxEdit.cs` (FORMAT.md section 3): binary `.dbx` -> readable XML, a value's text, one number's
//! text. The number formatting is `dbx`'s (.NET's round-trip "R" of singles and doubles, proven identical to DbxEdit's
//! over the corpus); the walk is DbxEdit's own, so a damaged file gives what DbxEdit gives (a truncated tree ends the
//! XML where the data ends, an unknown string fails).

use crate::dbx::{WHOLE_NUMBER_FIELDS, net_r_double, net_r_single};

use super::fault::{Fault, FaultKind, Result};
use super::file::{CONTAINER, Element, NUMBERS, TEXT, Table, be_u32, read_varint};

/// Whether 4-byte values of `field` are always whole numbers (hashes and ids).
#[must_use]
pub fn is_whole_number_field(field: &str) -> bool {
    WHOLE_NUMBER_FIELDS.contains(&field)
}

/// `XmlExport.Number`: the number of `size` bytes at `at` of `data` as the XML shows it. A size other than 4 or 8
/// reads two bytes, whatever the size.
///
/// # Errors
/// A read past the end of `data`.
pub fn number(data: &[u8], at: usize, size: i32, field: &str) -> Result<String> {
    match size {
        4 => {
            let whole = be_u32(data, at)? as i32;
            let exponent = (whole >> 23) & 0xFF;
            Ok(
                if is_whole_number_field(field) || (whole >> 24) == 0 || exponent == 0xFF {
                    whole.to_string()
                } else {
                    net_r_single(whole as u32)
                },
            )
        }
        8 => {
            let hi = u64::from(be_u32(data, at)?);
            let lo = u64::from(be_u32(data, at + 4)?);
            Ok(net_r_double((hi << 32) | lo))
        }
        _ => {
            let a = *data.get(at).ok_or_else(Fault::index)?;
            let b = *data.get(at + 1).ok_or_else(Fault::index)?;
            Ok((u32::from(a) << 8 | u32::from(b)).to_string())
        }
    }
}

/// The text of 4-byte bits as the XML shows them in `field`.
#[must_use]
pub fn four_byte_text(bits: u32, field: &str) -> String {
    number(&bits.to_be_bytes(), 0, 4, field).unwrap_or_default()
}

/// A bool / byte value's text.
fn scalar(v: u8) -> String {
    match v {
        0 => "false".to_owned(),
        1 => "true".to_owned(),
        _ => v.to_string(),
    }
}

/// The numbers of a payload (`count`, `size` varints, values) joined by `/`.
fn numbers_text(payload: &[u8], pos: &mut usize, field: &str) -> Result<String> {
    let n = read_varint(payload, pos)?;
    let size = read_varint(payload, pos)?;
    if n < 0 {
        return Err(Fault::overflow());
    }
    if n > 0 && size <= 0 && n > 1 << 24 {
        return Err(Fault::new(
            FaultKind::OutOfMemory,
            "Exception of type 'System.OutOfMemoryException' was thrown.",
        ));
    }
    let mut out = String::new();
    for i in 0..n {
        if i > 0 {
            out.push('/');
        }
        out.push_str(&number(payload, *pos, size, field)?);
        *pos = usize::try_from(*pos as i64 + i64::from(size)).map_err(|_| Fault::index())?;
    }
    Ok(out)
}

/// `XmlExport.ValueText`: a value as its XML shows it (unescaped): text, numbers joined by `/`, or `true` / `false` /
/// a byte. The field is the element's first attribute value.
///
/// # Errors
/// A damaged value or string index.
pub fn value_text(t: &Table, e: &Element) -> Result<String> {
    let field = t.first_value(e)?;
    let mut pos = 0;
    if e.kind == TEXT {
        return Ok(t.s(read_varint(&e.payload, &mut pos)?)?.to_owned());
    }
    if e.kind == NUMBERS {
        return numbers_text(&e.payload, &mut pos, field);
    }
    Ok(scalar(*e.payload.get(1).ok_or_else(Fault::index)?))
}

/// `Escape`: `&`, `<`, `>` and `"` as entities.
fn push_escaped(out: &mut String, s: &str) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

/// `XmlExport.Convert`: a binary `.dbx` as DbxEdit's XML (`\r\n` line ends, a tab per level). Strings are read up to
/// a zero byte or the end of the file.
///
/// # Errors
/// What DbxEdit's export throws on a damaged file.
pub fn convert(data: &[u8]) -> Result<String> {
    let area_size = be_u32(data, 16)?;
    let count = be_u32(data, 20)?;
    let strings_at = 24i64 + 4 * i64::from(count as i32);
    // every offset first, then the strings, as DbxEdit reads them (a damaged file fails at the same step)
    let mut offsets = Vec::with_capacity(count.min(1 << 20) as usize);
    for i in 0..u64::from(count) {
        let at = usize::try_from(24 + 4 * i).map_err(|_| Fault::index())?;
        offsets.push(be_u32(data, at)?);
    }
    let mut table = Vec::with_capacity(offsets.len());
    for offset in offsets {
        let from = strings_at + i64::from(offset as i32);
        let from = usize::try_from(from).map_err(|_| Fault::index())?;
        if from > data.len() {
            return Err(Fault::new(
                FaultKind::ArgumentOutOfRange,
                "Index and count must refer to a location within the buffer.\nParameter name: bytes",
            ));
        }
        let tail = data.get(from..).unwrap_or_default();
        let len = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
        table.push(String::from_utf8_lossy(tail.get(..len).unwrap_or_default()).into_owned());
    }
    let s = |i: i32| -> Result<&str> {
        usize::try_from(i)
            .ok()
            .and_then(|i| table.get(i))
            .map(String::as_str)
            .ok_or_else(Fault::index)
    };
    let mut xml = String::with_capacity(data.len() * 3);
    xml.push_str("<?xml version=\"1.0\"?>\r\n");
    let mut open: Vec<&str> = Vec::new();
    let mut pos = usize::try_from(24i64 + i64::from(area_size as i32)).map_err(|_| Fault::index())?;
    let indent = |xml: &mut String, n: usize| {
        for _ in 0..n {
            xml.push('\t');
        }
    };
    while pos < data.len() {
        let tag = read_varint(data, &mut pos)?;
        if tag == 0 {
            let Some(closing) = open.pop() else { break };
            indent(&mut xml, open.len());
            xml.push_str("</");
            xml.push_str(closing);
            xml.push_str(">\r\n");
            continue;
        }
        let name = s(tag)?;
        let head = *data.get(pos).ok_or_else(Fault::index)?;
        pos += 1;
        let kind = i32::from(head >> 4);
        indent(&mut xml, open.len());
        xml.push('<');
        xml.push_str(name);
        let mut first_value = "";
        for i in 0..(head & 0xF) {
            let key = s(read_varint(data, &mut pos)?)?;
            let value = s(read_varint(data, &mut pos)?)?;
            if i == 0 {
                first_value = value;
            }
            xml.push(' ');
            xml.push_str(key);
            xml.push_str("=\"");
            push_escaped(&mut xml, value);
            xml.push('"');
        }
        if kind == CONTAINER {
            pos += 1;
            xml.push_str(">\r\n");
            open.push(name);
            continue;
        }
        let mut content = String::new();
        if kind == TEXT {
            push_escaped(&mut content, s(read_varint(data, &mut pos)?)?);
            if content.is_empty() {
                xml.push_str(" />\r\n");
                continue;
            }
        } else if kind == NUMBERS {
            content = numbers_text(data, &mut pos, first_value)?;
        } else {
            let v = *data.get(pos + 1).ok_or_else(Fault::index)?;
            pos += 2;
            content = scalar(v);
        }
        xml.push('>');
        xml.push_str(&content);
        xml.push_str("</");
        xml.push_str(name);
        xml.push_str(">\r\n");
    }
    Ok(xml)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn numbers() {
        assert_eq!(number(&1.5f32.to_bits().to_be_bytes(), 0, 4, "Scale").unwrap(), "1.5");
        assert_eq!(
            number(&0x3F80_0000u32.to_be_bytes(), 0, 4, "Hash").unwrap(),
            "1065353216"
        );
        assert_eq!(number(&7u32.to_be_bytes(), 0, 4, "").unwrap(), "7");
        assert_eq!(number(&(-1i32).to_be_bytes(), 0, 4, "").unwrap(), "-1");
        assert_eq!(number(&2.5f64.to_bits().to_be_bytes(), 0, 8, "").unwrap(), "2.5");
        assert_eq!(number(&[1, 2, 3], 0, 1, "").unwrap(), "258");
        assert!(number(&[1], 0, 1, "").is_err());
        assert_eq!(four_byte_text(1.0f32.to_bits(), "x"), "1");
    }

    #[test]
    fn damaged_headers_fail() {
        assert!(convert(b"{binary}").is_err());
        let mut d = b"{binary}".to_vec();
        d.extend([0u8; 16]);
        assert_eq!(convert(&d).unwrap(), "<?xml version=\"1.0\"?>\r\n");
    }
}
