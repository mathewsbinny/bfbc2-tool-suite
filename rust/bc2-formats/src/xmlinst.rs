//! `core/bfbc2core/xmlinst.py` (round F2b): the `<instance>` elements of a converted level partition (DbxEdit's
//! XML) and their fields' references, whatever the attribute order - the reference's three patterns
//! (`<instance\b([^>]*)>(.*?)</instance>` with `re.S`, `(\w+)="([^"]*)"`, `<field\b([^>]*?)/?>`) matched by hand,
//! with their exact semantics (leftmost matches, the lazy body up to the first `</instance>`, the last of two
//! attributes of one name winning).
//!
//! Python's `\w` on `str` is Unicode-aware; where a decision would rest on whether a non-ASCII character is a word
//! character, the functions return `None` and the caller runs the reference.

/// A word character as Python's `\w` sees an ASCII byte; `None` for a byte of a non-ASCII character.
fn word(c: u8) -> Option<bool> {
    (c < 0x80).then(|| c.is_ascii_alphanumeric() || c == b'_')
}

/// Whether `\b` holds after a word character at `at` (the end of the text, or a non-word character next).
fn boundary_after(b: &[u8], at: usize) -> Option<bool> {
    match b.get(at) {
        None => Some(true),
        Some(&c) => word(c).map(|w| !w),
    }
}

/// `_ATTR.findall(text)` as a dict: `(name, value)` pairs, a later name replacing an earlier one in place (the
/// dict's order); `None` where a word decision meets a non-ASCII character.
#[must_use]
pub fn attrs(text: &str) -> Option<Vec<(&str, &str)>> {
    let b = text.as_bytes();
    let mut out: Vec<(&str, &str)> = Vec::new();
    let mut i = 0usize;
    while i < b.len() {
        let c = b.get(i).copied()?;
        if !word(c)? {
            i += 1;
            continue;
        }
        let s = i;
        while i < b.len() && word(b.get(i).copied()?)? {
            i += 1;
        }
        // the run s..i, then ="value"
        if b.get(i..i + 2) == Some(b"=\"")
            && let Some(q) = b.get(i + 2..).and_then(|r| r.iter().position(|&x| x == b'"'))
        {
            let name = text.get(s..i)?;
            let value = text.get(i + 2..i + 2 + q)?;
            match out.iter_mut().find(|(k, _)| *k == name) {
                Some(slot) => slot.1 = value,
                None => out.push((name, value)),
            }
            i = i + 2 + q + 1;
        }
    }
    Some(out)
}

/// One instance: (type, guid, body).
pub type Instance<'t> = (&'t str, &'t str, &'t str);

/// `instances(text, type_suffix)`: every instance in document order (with a suffix, those whose type ends with it);
/// `None`: left to the reference.
#[must_use]
pub fn instances<'t>(text: &'t str, suffix: Option<&str>) -> Option<Vec<Instance<'t>>> {
    const OPEN: &str = "<instance";
    const CLOSE: &str = "</instance>";
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut from = 0usize;
    while let Some(p) = text.get(from..)?.find(OPEN) {
        let at = from + p;
        let after = at + OPEN.len();
        if !boundary_after(b, after)? {
            from = at + 1;
            continue;
        }
        // ([^>]*)> then (.*?)</instance>
        let Some(gt) = text.get(after..)?.find('>') else {
            break; // no '>' after this or any later start
        };
        let attr_end = after + gt;
        let body_start = attr_end + 1;
        let Some(cl) = text.get(body_start..)?.find(CLOSE) else {
            break; // no closing tag after this or any later start
        };
        let body_end = body_start + cl;
        let a = attrs(text.get(after..attr_end)?)?;
        let get = |k: &str| a.iter().find(|(n, _)| *n == k).map_or("", |x| x.1);
        let typ = get("type");
        if suffix.is_none_or(|s| typ.ends_with(s)) {
            out.push((typ, get("guid"), text.get(body_start..body_end)?));
        }
        from = body_end + CLOSE.len();
    }
    Some(out)
}

/// `field_ref(body, name)`: the `ref` of the first field named `name` that has one; `Some(None)` when there is
/// none, `None`: left to the reference.
#[must_use]
pub fn field_ref<'t>(body: &'t str, name: &str) -> Option<Option<&'t str>> {
    const OPEN: &str = "<field";
    let b = body.as_bytes();
    let mut from = 0usize;
    while let Some(p) = body.get(from..)?.find(OPEN) {
        let at = from + p;
        let after = at + OPEN.len();
        if !boundary_after(b, after)? {
            from = at + 1;
            continue;
        }
        let Some(gt) = body.get(after..)?.find('>') else {
            break;
        };
        let end = after + gt;
        // ([^>]*?)/?> : a '/' right before the '>' is not the group's
        let group_end = if end > after && b.get(end - 1) == Some(&b'/') {
            end - 1
        } else {
            end
        };
        let a = attrs(body.get(after..group_end)?)?;
        let get = |k: &str| a.iter().find(|(n, _)| *n == k).map(|x| x.1);
        if get("name") == Some(name)
            && let Some(r) = get("ref")
        {
            return Some(Some(r));
        }
        from = end + 1;
    }
    Some(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn either_attribute_order() {
        let x = r#"<instance guid="G1" type="A.B"><field ref="r1" name="Mesh"/></instance>
<instance type="C.D" guid="G2"><field name="Mesh" ref="r2" /></instance><instancex type="Z"></instance>"#;
        let all = instances(x, None);
        assert_eq!(all.as_ref().map(Vec::len), Some(2));
        assert_eq!(
            instances(x, Some(".D")).and_then(|v| v.first().map(|i| i.1)),
            Some("G2")
        );
        let bodies: Vec<&str> = all.unwrap_or_default().iter().map(|i| i.2).collect();
        assert_eq!(bodies.first().and_then(|b| field_ref(b, "Mesh")), Some(Some("r1")));
        assert_eq!(bodies.get(1).and_then(|b| field_ref(b, "Mesh")), Some(Some("r2")));
        assert_eq!(field_ref("<field name=\"X\"/>", "X"), Some(None));
        assert_eq!(instances("<instance\u{e9} type=\"x\"></instance>", None), None);
    }
}
