//! The readable XML (FORMAT.md 3), as `dbx.to_xml` and so `DbxEdit xml`: a streaming pass over the bytes, with the
//! export's tolerance (no `{binary}` check, a last string without its zero byte read to the end).

use super::cursor::Cursor;
use super::error::{DbxError, Result};
use super::model::{CONTAINER, NUMBERS, TEXT};
use super::number::{check_count, number_text, scalar_text};
use super::reader::string_id;
use super::strings::read_table;

/// `s` with `&`, `<`, `>` and `"` escaped, appended to `out`.
pub(crate) fn push_escaped(out: &mut String, s: &str) {
    let mut rest = s;
    while let Some(i) = rest.find(['&', '<', '>', '"']) {
        let (plain, tail) = rest.split_at(i);
        out.push_str(plain);
        let mut chars = tail.chars();
        out.push_str(match chars.next() {
            Some('&') => "&amp;",
            Some('<') => "&lt;",
            Some('>') => "&gt;",
            _ => "&quot;",
        });
        rest = chars.as_str();
    }
    out.push_str(rest);
}

fn indent(out: &mut String, depth: usize) {
    out.extend(std::iter::repeat_n('\t', depth));
}

/// A binary `.dbx` file as `DbxEdit xml` writes it (CRLF line ends, tab indents), as `dbx.to_xml`.
///
/// # Errors
/// A damaged file: a read past the end, an index past the string table, a 4- or 8-byte number cut short.
///
/// ```
/// use bc2_formats::dbx::{self, DbxFile, Element};
///
/// let mut f = DbxFile::default();
/// let flag = Element::scalar(&mut f, "field", 6, &[("name", "Enabled")], [0, 1]);
/// let text = Element::text(&mut f, "field", &[("name", "Note")], "a < b");
/// let empty = Element::text(&mut f, "field", &[("name", "Empty")], "");
/// let inst = Element::container(&mut f, "instance", &[("guid", "1")], vec![flag, text, empty]);
/// f.roots.push(inst);
/// assert_eq!(
///     dbx::to_xml(&dbx::write(&f)?)?,
///     "<?xml version=\"1.0\"?>\r\n<instance guid=\"1\">\r\n\t<field name=\"Enabled\">true</field>\r\n\
///      \t<field name=\"Note\">a &lt; b</field>\r\n\t<field name=\"Empty\" />\r\n</instance>\r\n"
/// );
/// # Ok::<(), dbx::DbxError>(())
/// ```
pub fn to_xml(data: &[u8]) -> Result<String> {
    let (strings, body) = read_table(data, true)?;
    let count = strings.len();
    let s = |i: u64| -> Result<&str> { Ok(strings.get(string_id(i, count)? as usize).map_or("", |c| c.as_ref())) };
    let mut out = String::with_capacity(data.len().saturating_mul(3));
    out.push_str("<?xml version=\"1.0\"?>\r\n");
    let mut open: Vec<&str> = Vec::new();
    let mut c = Cursor::new(data, body);
    while !c.at_end() {
        let tag = c.varint()?;
        if tag == 0 {
            let Some(closing) = open.pop() else { break };
            indent(&mut out, open.len());
            out.push_str("</");
            out.push_str(closing);
            out.push_str(">\r\n");
            continue;
        }
        let name = s(tag)?;
        let head = c.byte()?;
        let kind = head >> 4;
        indent(&mut out, open.len());
        out.push('<');
        out.push_str(name);
        let mut first = "";
        for i in 0..(head & 0xF) {
            let k = s(c.varint()?)?;
            let v = s(c.varint()?)?;
            if i == 0 {
                first = v;
            }
            out.push(' ');
            out.push_str(k);
            out.push_str("=\"");
            push_escaped(&mut out, v);
            out.push('"');
        }
        match kind {
            CONTAINER => {
                c.skip(1);
                out.push_str(">\r\n");
                open.push(name);
                continue;
            }
            TEXT => {
                let text = s(c.varint()?)?;
                if text.is_empty() {
                    out.push_str(" />\r\n");
                    continue;
                }
                out.push('>');
                push_escaped(&mut out, text);
            }
            NUMBERS => {
                let num = c.varint()?;
                let size = c.varint()?;
                check_count(num, data.len())?;
                // each value's text from its first 4, 8 or 2 bytes (a Python slice: shorter past the end)
                let take = match size {
                    4 => 4,
                    8 => 8,
                    _ => 2,
                };
                let sz = usize::try_from(size).unwrap_or(usize::MAX);
                out.push('>');
                for i in 0..num as usize {
                    if i > 0 {
                        out.push('/');
                    }
                    let at = c.pos().saturating_add(i.saturating_mul(sz));
                    out.push_str(&number_text(c.clamped(at, take), size, first)?);
                }
                c.skip((num as usize).saturating_mul(sz));
            }
            _ => {
                let v = c.peek_at(1).map_err(|_| DbxError::ValueCutShort)?;
                c.skip(2);
                out.push('>');
                out.push_str(&scalar_text(v));
            }
        }
        out.push_str("</");
        out.push_str(name);
        out.push_str(">\r\n");
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn escaping() {
        let mut s = String::new();
        push_escaped(&mut s, "a&b<c>d\"e'f é");
        assert_eq!(s, "a&amp;b&lt;c&gt;d&quot;e'f é");
    }

    #[test]
    fn short_data_is_an_error() {
        assert!(to_xml(b"").is_err());
        assert!(to_xml(b"{binary}\0\0\0\0").is_err());
    }
}
