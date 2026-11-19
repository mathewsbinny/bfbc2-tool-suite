//! The crash report's XML as `crashreader.read_report` reads it with `xml.etree.ElementTree`: the root element's
//! direct children by tag, each one's text (what comes before its first child element: character data, the five
//! entities and character references, CDATA; comments and processing instructions skipped, the text joined across
//! them; line ends made `\n`, as expat does).
//!
//! Strict: a document that is UTF-8 (or declares it), without a DOCTYPE, namespaces or anything expat might read
//! otherwise or reject is read here; anything else is `None` and the Python reads it (its `ParseError` texts are
//! expat's own).

use std::collections::HashMap;

/// The root's children: tag -> the first such child's text.
pub type Children = HashMap<String, String>;

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

fn name_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'-' | b'.')
}

fn xml_space(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// A character XML allows in a document.
fn xml_char(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r')
        || ('\u{20}'..='\u{d7ff}').contains(&c)
        || ('\u{e000}'..='\u{fffd}').contains(&c)
        || c >= '\u{10000}'
}

impl P<'_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.i).copied()
    }

    fn starts(&self, t: &[u8]) -> bool {
        self.s.get(self.i..).is_some_and(|r| r.starts_with(t))
    }

    fn skip_space(&mut self) {
        while self.peek().is_some_and(xml_space) {
            self.i += 1;
        }
    }

    fn name(&mut self) -> Option<String> {
        let start = self.i;
        if !self.peek().is_some_and(name_start) {
            return None;
        }
        while self.peek().is_some_and(name_char) {
            self.i += 1;
        }
        std::str::from_utf8(self.s.get(start..self.i)?).ok().map(str::to_owned)
    }

    /// `<!-- ... -->` (no `--` inside).
    fn comment(&mut self) -> Option<()> {
        self.i += 4;
        let rest = self.s.get(self.i..)?;
        let k = find(rest, b"--")?;
        if rest.get(k + 2) != Some(&b'>') {
            return None;
        }
        self.i += k + 3;
        Some(())
    }

    /// `<?target ...?>` (not `xml`).
    fn pi(&mut self) -> Option<()> {
        self.i += 2;
        let target = self.name()?;
        if target.eq_ignore_ascii_case("xml") {
            return None;
        }
        let rest = self.s.get(self.i..)?;
        if !rest.first().is_some_and(|&c| xml_space(c) || c == b'?') {
            return None;
        }
        let k = find(rest, b"?>")?;
        self.i += k + 2;
        Some(())
    }

    /// `&...;` into `out`.
    fn reference(&mut self, out: &mut String) -> Option<()> {
        self.i += 1;
        let rest = self.s.get(self.i..)?;
        let k = rest.iter().position(|&c| c == b';')?;
        let body = std::str::from_utf8(rest.get(..k)?).ok()?;
        self.i += k + 1;
        let c = match body {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let v = if let Some(h) = body.strip_prefix("#x") {
                    if h.is_empty() || !h.bytes().all(|c| c.is_ascii_hexdigit()) || h.len() > 8 {
                        return None;
                    }
                    u32::from_str_radix(h, 16).ok()?
                } else {
                    let d = body.strip_prefix('#')?;
                    if d.is_empty() || !d.bytes().all(|c| c.is_ascii_digit()) || d.len() > 10 {
                        return None;
                    }
                    d.parse().ok()?
                };
                let c = char::from_u32(v)?;
                if !xml_char(c) {
                    return None;
                }
                c
            }
        };
        out.push(c);
        Some(())
    }

    /// An attribute value: quoted, no `<`, references checked.
    fn attr_value(&mut self) -> Option<()> {
        let q = self.peek()?;
        if q != b'"' && q != b'\'' {
            return None;
        }
        self.i += 1;
        let mut sink = String::new();
        loop {
            match self.peek()? {
                c if c == q => {
                    self.i += 1;
                    return Some(());
                }
                b'<' => return None,
                b'&' => self.reference(&mut sink)?,
                _ => self.i += 1,
            }
        }
    }

    /// After `<name`: the attributes and `>` / `/>`; true when the element is empty.
    fn attrs(&mut self) -> Option<bool> {
        let mut seen: Vec<String> = Vec::new();
        loop {
            let had_space = self.peek().is_some_and(xml_space);
            self.skip_space();
            match self.peek()? {
                b'>' => {
                    self.i += 1;
                    return Some(false);
                }
                b'/' => {
                    if self.s.get(self.i + 1) != Some(&b'>') {
                        return None;
                    }
                    self.i += 2;
                    return Some(true);
                }
                _ => {
                    if !had_space {
                        return None;
                    }
                    let n = self.name()?;
                    if n.starts_with("xmlns") || seen.contains(&n) {
                        return None;
                    }
                    seen.push(n);
                    self.skip_space();
                    if self.peek()? != b'=' {
                        return None;
                    }
                    self.i += 1;
                    self.skip_space();
                    self.attr_value()?;
                }
            }
        }
    }

    /// An element's content up to its end tag; `text` gets the text before the first child when given.
    fn content(&mut self, tag: &str, mut text: Option<&mut String>, mut children: Option<&mut Children>) -> Option<()> {
        let mut before_child = true;
        let mut sink = String::new();
        loop {
            if self.starts(b"</") {
                self.i += 2;
                let n = self.name()?;
                self.skip_space();
                if n != tag || self.peek()? != b'>' {
                    return None;
                }
                self.i += 1;
                return Some(());
            }
            if self.starts(b"<!--") {
                self.comment()?;
                continue;
            }
            if self.starts(b"<![CDATA[") {
                self.i += 9;
                let rest = self.s.get(self.i..)?;
                let k = find(rest, b"]]>")?;
                let t = std::str::from_utf8(rest.get(..k)?).ok()?;
                let out = if before_child {
                    text.as_deref_mut().unwrap_or(&mut sink)
                } else {
                    &mut sink
                };
                out.push_str(t);
                self.i += k + 3;
                continue;
            }
            if self.starts(b"<?") {
                self.pi()?;
                continue;
            }
            match self.peek()? {
                b'<' => {
                    if self.starts(b"<!") {
                        return None;
                    }
                    self.i += 1;
                    let n = self.name()?;
                    let empty = self.attrs()?;
                    let mut t = String::new();
                    if !empty {
                        self.content(&n, Some(&mut t), None)?;
                    }
                    if let Some(c) = children.as_deref_mut() {
                        c.entry(n).or_insert(t);
                    }
                    before_child = false;
                }
                b'&' => {
                    let out = if before_child {
                        text.as_deref_mut().unwrap_or(&mut sink)
                    } else {
                        &mut sink
                    };
                    self.reference(out)?;
                }
                _ => {
                    let start = self.i;
                    while self.peek().is_some_and(|c| c != b'<' && c != b'&') {
                        if self.starts(b"]]>") {
                            return None;
                        }
                        self.i += 1;
                    }
                    let t = std::str::from_utf8(self.s.get(start..self.i)?).ok()?;
                    let out = if before_child {
                        text.as_deref_mut().unwrap_or(&mut sink)
                    } else {
                        &mut sink
                    };
                    out.push_str(t);
                }
            }
        }
    }

    /// Whitespace, comments and processing instructions.
    fn misc(&mut self) -> Option<()> {
        loop {
            self.skip_space();
            if self.starts(b"<!--") {
                self.comment()?;
            } else if self.starts(b"<?") {
                self.pi()?;
            } else {
                return Some(());
            }
        }
    }
}

fn find(h: &[u8], n: &[u8]) -> Option<usize> {
    h.windows(n.len()).position(|w| w == n)
}

/// The XML declaration (`<?xml version="1.0" encoding="UTF-8"?>`): only version 1.0 and UTF-8 are read here.
fn declaration(p: &mut P<'_>) -> Option<()> {
    if !p.starts(b"<?xml") || !p.s.get(5).is_some_and(|&c| xml_space(c)) {
        return Some(());
    }
    let rest = p.s.get(p.i..)?;
    let k = find(rest, b"?>")?;
    let body = std::str::from_utf8(rest.get(5..k)?).ok()?;
    p.i += k + 2;
    let mut fields: Vec<(String, String)> = Vec::new();
    for part in body.split_whitespace() {
        let (k, v) = part.split_once('=')?;
        let v = v
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| v.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))?;
        fields.push((k.to_owned(), v.to_owned()));
    }
    let names: Vec<&str> = fields.iter().map(|(k, _)| k.as_str()).collect();
    let order_ok = matches!(
        names.as_slice(),
        ["version"] | ["version", "encoding" | "standalone"] | ["version", "encoding", "standalone"]
    );
    if !order_ok {
        return None;
    }
    for (k, v) in &fields {
        match k.as_str() {
            "version" if v == "1.0" => {}
            "encoding" if v.eq_ignore_ascii_case("utf-8") => {}
            "standalone" if v == "yes" || v == "no" => {}
            _ => return None,
        }
    }
    Some(())
}

/// The root's children and their texts, or None (the Python reads it).
pub fn read(data: &[u8]) -> Option<Children> {
    let data = data.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(data);
    let text = std::str::from_utf8(data).ok()?;
    if !text.chars().all(xml_char) {
        return None;
    }
    let norm = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut p = P {
        s: norm.as_bytes(),
        i: 0,
    };
    declaration(&mut p)?;
    p.misc()?;
    if p.starts(b"<!DOCTYPE") {
        // only `<!DOCTYPE name>`: no external identifier, no internal subset (the Python reads those)
        p.i += 9;
        if !p.peek().is_some_and(xml_space) {
            return None;
        }
        p.skip_space();
        p.name()?;
        p.skip_space();
        if p.peek()? != b'>' {
            return None;
        }
        p.i += 1;
        p.misc()?;
    }
    if p.peek()? != b'<' || p.starts(b"<!") {
        return None;
    }
    p.i += 1;
    let root = p.name()?;
    let empty = p.attrs()?;
    let mut children = Children::new();
    if !empty {
        p.content(&root, None, Some(&mut children))?;
    }
    p.misc()?;
    if p.i != p.s.len() {
        return None;
    }
    Some(children)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_what_elementtree_reads() {
        let c = read(
            b"<?xml version=\"1.0\"?>\r\n<!-- c --><r a='1'>\r\n<t>a&amp;b&#33;<x/>tail</t><s><![CDATA[a<b]]> \
              x</s><u>t<!-- m -->u</u><t>second</t><e/></r>\r\n",
        )
        .unwrap_or_default();
        assert_eq!(c.get("t").map(String::as_str), Some("a&b!"));
        assert_eq!(c.get("s").map(String::as_str), Some("a<b x"));
        assert_eq!(c.get("u").map(String::as_str), Some("tu"));
        assert_eq!(c.get("e").map(String::as_str), Some(""));
        assert!(read(b"not xml <").is_none());
        assert!(read(b"<!DOCTYPE r><r/>").is_some());
        assert!(read(b"<!DOCTYPE r [<!ENTITY x \"y\">]><r/>").is_none());
        assert!(read(b"<r><a></b></r>").is_none());
        assert!(read(b"<r>&bogus;</r>").is_none());
        assert!(read(b"<r/>x").is_none());
        assert!(read(b"<?xml version=\"1.0\" encoding=\"UTF-16\"?><r/>").is_none());
    }
}
