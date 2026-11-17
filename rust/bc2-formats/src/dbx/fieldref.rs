//! `xmlinst.field_ref` (`core/bfbc2core/xmlinst.py`): the `ref` of the first `<field>` with a given name, on the
//! readable XML ([`field_ref_text`]) and straight from a binary `.dbx` ([`field_ref`]), the same answer as
//! `field_ref_text(&to_xml(data)?, name)` without building the text.

use super::cursor::Cursor;
use super::error::{DbxError, Result};
use super::model::{CONTAINER, NUMBERS, TEXT};
use super::number::check_count;
use super::reader::string_id;
use super::strings::read_table;
use super::xml::{push_escaped, to_xml};

/// A word character of Python's `re` on `str` patterns.
fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// `xmlinst._ATTR.findall(s)`: every `(\w+)="([^"]*)"`, left to right.
fn attributes(s: &str) -> Vec<(&str, &str)> {
    let mut out = Vec::new();
    let mut rest = s;
    while let Some(start) = rest.find(is_word) {
        let run = rest.get(start..).unwrap_or_default();
        let end = run.find(|c: char| !is_word(c)).unwrap_or(run.len());
        let (name, after) = run.split_at(end);
        if let Some(value) = after.strip_prefix("=\"")
            && let Some(close) = value.find('"')
        {
            out.push((name, value.get(..close).unwrap_or_default()));
            rest = value.get(close + 1..).unwrap_or_default();
        } else {
            rest = after;
        }
    }
    out
}

/// The `ref` of one tag's attribute text (what `<field\b([^>]*?)/?>` captures) when its `name` is `name`.
fn ref_of_tag(inner: &str, name: &str) -> Option<String> {
    let inner = inner.strip_suffix('/').unwrap_or(inner);
    let mut found_name = None;
    let mut found_ref = None;
    for (k, v) in attributes(inner) {
        match k {
            "name" => found_name = Some(v),
            "ref" => found_ref = Some(v),
            _ => {}
        }
    }
    if found_name == Some(name) {
        found_ref.map(str::to_owned)
    } else {
        None
    }
}

/// `xmlinst.field_ref(text, name)`: the `ref` attribute of the first `<field ...>` whose `name` is `name` and that
/// has a `ref` (attributes in any order; a repeated attribute counts by its last value), or `None`.
///
/// ```
/// use bc2_formats::dbx::field_ref_text;
///
/// let t = r#"<field name="Other" ref="a/b"/><field ref="Objects/X/X_Mesh/1" name="Mesh"/>"#;
/// assert_eq!(field_ref_text(t, "Mesh").as_deref(), Some("Objects/X/X_Mesh/1"));
/// ```
#[must_use]
pub fn field_ref_text(text: &str, name: &str) -> Option<String> {
    let mut rest = text;
    while let Some(at) = rest.find("<field") {
        let after = rest.get(at + "<field".len()..).unwrap_or_default();
        // \b: the next character is not a word character
        if after.chars().next().is_some_and(is_word) {
            rest = after;
            continue;
        }
        // ([^>]*?)/?> : up to the first '>', a '/' just before it left out
        let gt = after.find('>')?;
        if let Some(r) = ref_of_tag(after.get(..gt).unwrap_or_default(), name) {
            return Some(r);
        }
        rest = after.get(gt + 1..).unwrap_or_default();
    }
    None
}

/// `field_ref_text(&to_xml(data)?, name)` on a binary `.dbx`, without building the XML: the escaped `ref` of the
/// first `field` element named `name` that has one, in document order. The whole file is read, so a file
/// [`to_xml`] refuses is refused here too (with the same error). A file with an element or attribute name holding
/// `<` or `>` (which [`to_xml`] writes unescaped, so the text could hold more tags than the file) is answered from
/// its XML.
///
/// # Errors
/// As [`to_xml`]: a damaged file.
///
/// ```
/// use bc2_formats::dbx::{self, DbxFile, Element};
///
/// let mut f = DbxFile::default();
/// let mesh = Element::text(&mut f, "field", &[("name", "Mesh"), ("ref", "Objects/X/X_Mesh/1")], "");
/// let inst = Element::container(&mut f, "instance", &[("guid", "1")], vec![mesh]);
/// f.roots.push(inst);
/// let data = dbx::write(&f)?;
/// assert_eq!(dbx::field_ref(&data, "Mesh")?.as_deref(), Some("Objects/X/X_Mesh/1"));
/// assert_eq!(dbx::field_ref(&data, "Mesh")?, dbx::field_ref_text(&dbx::to_xml(&data)?, "Mesh"));
/// # Ok::<(), dbx::DbxError>(())
/// ```
pub fn field_ref(data: &[u8], name: &str) -> Result<Option<String>> {
    let (strings, body) = read_table(data, true)?;
    let count = strings.len();
    let s = |i: u64| -> Result<&str> { Ok(strings.get(string_id(i, count)? as usize).map_or("", |c| c.as_ref())) };
    let unsafe_name = |n: &str| n.contains(['<', '>']);
    let mut depth = 0usize;
    let mut found: Option<String> = None;
    let mut attrs: Vec<(&str, &str)> = Vec::new();
    let mut c = Cursor::new(data, body);
    while !c.at_end() {
        let tag = c.varint()?;
        if tag == 0 {
            let Some(d) = depth.checked_sub(1) else { break };
            depth = d;
            continue;
        }
        let tag_name = s(tag)?;
        let head = c.byte()?;
        attrs.clear();
        for _ in 0..(head & 0xF) {
            let k = s(c.varint()?)?;
            attrs.push((k, s(c.varint()?)?));
        }
        if unsafe_name(tag_name) || attrs.iter().any(|(k, _)| unsafe_name(k)) {
            return to_xml(data).map(|t| field_ref_text(&t, name));
        }
        let mut empty_text = false;
        match head >> 4 {
            CONTAINER => {
                c.skip(1);
                depth += 1;
            }
            TEXT => empty_text = s(c.varint()?)?.is_empty(),
            NUMBERS => {
                let num = c.varint()?;
                let size = c.varint()?;
                check_count(num, data.len())?;
                // to_xml formats each value from its first 4 or 8 bytes: one cut short is an error
                if num > 0 && (size == 4 || size == 8) {
                    let end = u128::from(num) * u128::from(size) + c.pos() as u128;
                    if end > data.len() as u128 {
                        let at = c.pos() + (data.len().saturating_sub(c.pos()) / size as usize) * size as usize;
                        let len = data.len().saturating_sub(at);
                        return Err(DbxError::NumberCutShort { size, len });
                    }
                }
                let sz = usize::try_from(size).unwrap_or(usize::MAX);
                c.skip(usize::try_from(num).unwrap_or(usize::MAX).saturating_mul(sz));
            }
            _ => {
                c.peek_at(1).map_err(|_| DbxError::ValueCutShort)?;
                c.skip(2);
            }
        }
        if found.is_none() {
            found = field_tag_ref(tag_name, &attrs, empty_text, name);
        }
    }
    Ok(found)
}

/// What [`field_ref_text`] finds in the opening tag [`to_xml`] writes for an element (`<` name, its attributes,
/// ` />` for an empty text element, else `>`), when that tag is a `<field\b`.
fn field_tag_ref(tag_name: &str, attrs: &[(&str, &str)], empty_text: bool, name: &str) -> Option<String> {
    let rest = tag_name.strip_prefix("field")?;
    // the character after "<field": the name's next one, else a space (attributes, " />") or '>'
    if rest.chars().next().is_some_and(is_word) {
        return None;
    }
    let mut inner = rest.to_owned();
    for (k, v) in attrs {
        inner.push(' ');
        inner.push_str(k);
        inner.push_str("=\"");
        push_escaped(&mut inner, v);
        inner.push('"');
    }
    if empty_text {
        inner.push_str(" /");
    }
    ref_of_tag(&inner, name)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::many_single_char_names)]
mod tests {
    use super::super::{DbxFile, Element, write};
    use super::*;

    #[test]
    fn text_refs() {
        let t = r#"<instance><field name="Other" ref="a/b"/><field ref="Objects/X/X_Mesh/1" name="Mesh"/></instance>"#;
        assert_eq!(field_ref_text(t, "Mesh").as_deref(), Some("Objects/X/X_Mesh/1"));
        assert_eq!(field_ref_text(t, "Other").as_deref(), Some("a/b"));
        assert_eq!(field_ref_text(t, "None"), None);
        // \b, a field without ref, a repeated attribute, attribute names inside other words, no closing '>'
        let t = r#"<fields name="Mesh" ref="no"/><field name="Mesh">x</field><field x-name="Mesh" name="Mesh" ref="1" ref="2">"#;
        assert_eq!(field_ref_text(t, "Mesh").as_deref(), Some("2"));
        assert_eq!(field_ref_text(r#"<field name="Mesh" ref="a""#, "Mesh"), None);
        assert_eq!(field_ref_text(r#"<field name="Mesh ref="a"/>"#, "Mesh"), None);
        assert_eq!(
            field_ref_text(r#"<field name="Mesh" ref="a/b"/>"#, "Mesh").as_deref(),
            Some("a/b")
        );
        assert_eq!(field_ref_text(r#"<field name="Mesh" ref=x/>"#, "Mesh"), None);
        assert_eq!(field_ref_text("<field", "Mesh"), None);
    }

    /// Both readers on a file built from `roots`.
    fn both(f: &DbxFile, name: &str) -> (Option<String>, Option<String>) {
        let data = write(f).unwrap();
        (
            field_ref(&data, name).unwrap(),
            field_ref_text(&to_xml(&data).unwrap(), name),
        )
    }

    #[test]
    fn binary_matches_the_text() {
        let mut f = DbxFile::default();
        let a = Element::text(&mut f, "fields", &[("name", "Mesh"), ("ref", "no/1")], "");
        let b = Element::text(&mut f, "field", &[("name", "Mesh")], "x");
        let c = Element::scalar(&mut f, "field", 6, &[("name", "Mesh"), ("ref", "num/1")], [0, 1]);
        let d = Element::text(&mut f, "field", &[("ref", "a&b<c>\"/1"), ("name", "Mesh")], "");
        let e = Element::text(&mut f, "field-x", &[("name", "Other"), ("ref", "o/1")], "");
        let g = Element::text(&mut f, "field", &[], "");
        let inner = Element::container(&mut f, "array", &[("name", "Mesh"), ("ref", "arr/1")], vec![a, b]);
        let fieldc = Element::container(&mut f, "field", &[("name", "Box")], vec![]);
        // numbers of every size before the Mesh, and an attribute other than name and ref
        let n4 = Element::numbers(
            &mut f,
            "field",
            &[("name", "Mesh"), ("id", "4")],
            4,
            &[0, 0, 0, 1, 0x3f, 0x80, 0, 0],
        );
        let n8 = Element::numbers(&mut f, "field", &[("name", "Mesh")], 8, &[0x3f, 0xf0, 0, 0, 0, 0, 0, 0]);
        let n2 = Element::numbers(&mut f, "field", &[("name", "Mesh")], 2, &[1, 2, 3, 4]);
        let n0 = Element::numbers(&mut f, "field", &[("name", "Mesh")], 4, &[]);
        let inst = Element::container(
            &mut f,
            "instance",
            &[("guid", "1")],
            vec![inner, g, fieldc, n4, n8, n2, n0, e, c, d],
        );
        f.roots.push(inst);
        for name in ["Mesh", "Other", "Box", "None", "Me"] {
            let (bin, text) = both(&f, name);
            assert_eq!(bin, text, "{name}");
        }
        assert_eq!(both(&f, "Mesh").0.as_deref(), Some("num/1"));
        assert_eq!(both(&f, "Other").0.as_deref(), Some("o/1"));
    }

    #[test]
    fn escaped_and_unsafe_names() {
        let mut f = DbxFile::default();
        let d = Element::text(&mut f, "field", &[("ref", "a&b<c>\"/1"), ("name", "Mesh")], "");
        f.roots.push(d);
        let (bin, text) = both(&f, "Mesh");
        assert_eq!(bin.as_deref(), Some("a&amp;b&lt;c&gt;&quot;/1"));
        assert_eq!(bin, text);
        // a tag name with '<' writes more tags into the text: answered from the XML
        let mut f = DbxFile::default();
        let x = Element::text(&mut f, "x<field name=\"Mesh\" ref=\"hidden/1\"", &[], "");
        let y = Element::text(&mut f, "field", &[("name", "Mesh"), ("ref", "plain/1")], "");
        f.roots.extend([x, y]);
        let (bin, text) = both(&f, "Mesh");
        assert_eq!(bin.as_deref(), Some("hidden/1"));
        assert_eq!(bin, text);
        let mut f = DbxFile::default();
        let z = Element::text(&mut f, "field", &[("na>me", "x"), ("name", "Mesh"), ("ref", "r/1")], "");
        f.roots.push(z);
        let (bin, text) = both(&f, "Mesh");
        assert_eq!(bin, text);
    }

    #[test]
    fn damaged_files_refused_as_to_xml_refuses_them() {
        let mut f = DbxFile::default();
        let n = Element::text(&mut f, "field", &[("name", "Mesh"), ("ref", "r/1")], "");
        let m = Element::scalar(&mut f, "field", 6, &[("name", "On")], [0, 1]);
        let inst = Element::container(&mut f, "instance", &[], vec![n, m]);
        f.roots.push(inst);
        let data = write(&f).unwrap();
        for k in 0..data.len() {
            let cut = &data[..k];
            let bin = field_ref(cut, "Mesh");
            let text = to_xml(cut).map(|t| field_ref_text(&t, "Mesh"));
            assert_eq!(bin, text, "cut at {k}");
        }
        // numbers cut short
        let mut g = DbxFile::default();
        let num = Element::numbers(&mut g, "field", &[("name", "Values")], 4, &[1, 2, 3, 4, 5, 6, 7, 8]);
        g.roots.push(num);
        let full = write(&g).unwrap();
        for k in full.len() - 8..full.len() {
            assert_eq!(
                field_ref(&full[..k], "Mesh"),
                to_xml(&full[..k]).map(|t| field_ref_text(&t, "Mesh")),
                "numbers cut at {k}"
            );
        }
        assert!(field_ref(&full[..full.len() - 1], "Mesh").is_err());
    }
}
