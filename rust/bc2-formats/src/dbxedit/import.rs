//! `XmlImport` and `TypeBook` of `DbxEdit.cs` (FORMAT.md section 6): DbxEdit's XML back to a binary `.dbx`.
//! Elements are matched to a reference file (`--like`, the original `.dbx`): unchanged values keep their exact bytes,
//! changed ones take the reference's types. New elements are typed from what the reference and `--types` files show
//! of the same field; whatever is left is guessed from the XML and reported.
//!
//! The XML reader is DbxEdit's own (not a strict XML parser): the same attribute twice, control characters and exact
//! line breaks in values are kept; comments, `<?...?>`, either quote, entity and character references and CDATA are
//! read. Its errors name the line as DbxEdit's do.

use std::borrow::Cow;
use std::collections::{HashMap, VecDeque};
use std::fmt;

use super::export::{four_byte_text, is_whole_number_field, value_text};
use super::fault::{Fault, Result};
use super::file::{
    CONTAINER, EditFile, Element, MAX_DEPTH, NUMBERS, TEXT, Table, be_u32, read_varint, varint_bytes, write_varint,
};
use super::net;

/// One element of the XML.
#[derive(Debug, Clone, Default)]
pub struct Node {
    /// The tag.
    pub tag: String,
    /// The text (empty for an element with children).
    pub text: String,
    /// `(name, value)` in order, duplicates kept.
    pub attributes: Vec<(String, String)>,
    /// Child elements (`None`: none).
    pub children: Option<Vec<Node>>,
    /// Written `<x ... />`.
    pub self_closing: bool,
    /// No text but a line break inside (`<x>\r\n</x>`: an empty container as the export writes it).
    pub multiline: bool,
    /// The line it starts on.
    pub line: usize,
}

impl Node {
    fn first_value(&self) -> &str {
        self.attributes.first().map_or("", |(_, v)| v.as_str())
    }
}

/// DbxEdit's `TextParser`, on byte positions of the text (every character it looks for is ASCII).
struct TextParser<'a> {
    s: &'a str,
    pos: usize,
    line: usize,
}

impl TextParser<'_> {
    fn at(&self, t: &str) -> bool {
        self.s
            .as_bytes()
            .get(self.pos..)
            .is_some_and(|r| r.starts_with(t.as_bytes()))
    }

    fn fail(&self, what: &str) -> Fault {
        Fault::invalid(format!("line {}: {what}", self.line))
    }

    /// Moves to byte `to`, counting the line breaks passed.
    fn advance_to(&mut self, to: usize) {
        let to = to.min(self.s.len());
        if let Some(passed) = self.s.as_bytes().get(self.pos..to) {
            self.line += passed.iter().fold(0, |n, &b| n + usize::from(b == b'\n'));
        }
        self.pos = to;
    }

    fn char_here(&self) -> Option<char> {
        self.s.get(self.pos..).and_then(|r| r.chars().next())
    }

    fn advance_char(&mut self) {
        if let Some(c) = self.char_here() {
            self.advance_to(self.pos + c.len_utf8());
        }
    }

    fn skip_to(&mut self, end: &str) -> Result<()> {
        match self.s.get(self.pos..).and_then(|r| r.find(end)) {
            Some(i) => {
                self.advance_to(self.pos + i + end.len());
                Ok(())
            }
            None => Err(self.fail(&format!("'{end}' missing"))),
        }
    }

    fn skip_space(&mut self) {
        while self.char_here().is_some_and(char::is_whitespace) {
            self.advance_char();
        }
    }

    fn skip_prolog(&mut self) -> Result<()> {
        loop {
            self.skip_space();
            if self.at("\u{FEFF}") {
                self.advance_char();
            } else if self.at("<?") {
                self.skip_to("?>")?;
            } else if self.at("<!--") {
                self.skip_to("-->")?;
            } else if self.at("<!DOCTYPE") {
                self.skip_to(">")?;
            } else {
                return Ok(());
            }
        }
    }

    fn name(&mut self) -> Result<String> {
        let start = self.pos;
        while let Some(c) = self.char_here() {
            if c.is_whitespace() || c == '/' || c == '>' || c == '=' {
                break;
            }
            self.advance_char();
        }
        if self.pos == start {
            return Err(self.fail("a name was expected"));
        }
        Ok(self.s.get(start..self.pos).unwrap_or("").to_owned())
    }

    fn decode(&self, raw: &str) -> Result<String> {
        if !raw.contains('&') {
            return Ok(raw.to_owned());
        }
        let mut b = String::with_capacity(raw.len());
        let mut rest = raw;
        while let Some(amp) = rest.find('&') {
            b.push_str(rest.get(..amp).unwrap_or(""));
            let after = rest.get(amp + 1..).unwrap_or("");
            let semi = after.find(';').ok_or_else(|| self.fail("'&' without ';'"))?;
            let e = after.get(..semi).unwrap_or("");
            match e {
                "amp" => b.push('&'),
                "lt" => b.push('<'),
                "gt" => b.push('>'),
                "quot" => b.push('"'),
                "apos" => b.push('\''),
                _ if e.starts_with("#x") || e.starts_with("#X") => {
                    b.push(net::from_utf32(net::parse_hex(e.get(2..).unwrap_or(""))?)?);
                }
                _ if e.starts_with('#') => b.push(net::from_utf32(net::parse_int(e.get(1..).unwrap_or(""))?)?),
                _ => return Err(self.fail(&format!("unknown entity &{e};"))),
            }
            rest = after.get(semi + 1..).unwrap_or("");
        }
        b.push_str(rest);
        Ok(b)
    }

    fn element(&mut self, depth: usize) -> Result<Node> {
        if depth > MAX_DEPTH {
            return Err(self.fail(&format!("elements nested more than {MAX_DEPTH} deep")));
        }
        self.advance_to(self.pos + 1); // '<'
        let line = self.line;
        let tag = self.name()?;
        let mut n = Node {
            tag,
            line,
            ..Node::default()
        };
        if self.attributes(&mut n)? {
            n.self_closing = true;
            return Ok(n);
        }
        self.content(n, depth)
    }

    /// The attributes of the element begun (after its tag) up to its `>`; true for `/>`.
    fn attributes(&mut self, n: &mut Node) -> Result<bool> {
        loop {
            self.skip_space();
            if self.pos >= self.s.len() {
                return Err(self.fail(&format!("<{}> is not closed", n.tag)));
            }
            if self.at("/>") {
                self.advance_to(self.pos + 2);
                return Ok(true);
            }
            if self.at(">") {
                self.advance_to(self.pos + 1);
                return Ok(false);
            }
            let key = self.name()?;
            self.skip_space();
            if !self.at("=") {
                return Err(self.fail(&format!("'=' expected after {key}")));
            }
            self.advance_to(self.pos + 1);
            self.skip_space();
            let Some(&q @ (b'"' | b'\'')) = self.s.as_bytes().get(self.pos) else {
                return Err(self.fail(&format!("a quoted value expected for {key}")));
            };
            self.advance_to(self.pos + 1);
            let Some(len) = self
                .s
                .as_bytes()
                .get(self.pos..)
                .and_then(|r| r.iter().position(|&b| b == q))
            else {
                return Err(self.fail(&format!("value of {key} not closed")));
            };
            let raw = self.s.get(self.pos..self.pos + len).unwrap_or("");
            self.advance_to(self.pos + len + 1);
            let value = self.decode(raw)?;
            n.attributes.push((key, value));
        }
    }

    /// The text or child elements of an element up to its closing tag.
    fn content(&mut self, mut n: Node, depth: usize) -> Result<Node> {
        let mut text = String::new();
        let mut real_text = false;
        loop {
            if self.pos >= self.s.len() {
                return Err(self.fail(&format!("<{}> is not closed", n.tag)));
            }
            if self.at("</") {
                self.advance_to(self.pos + 2);
                let close = self.name()?;
                if close != n.tag {
                    return Err(self.fail(&format!("</{close}> closes <{}>", n.tag)));
                }
                self.skip_space();
                if !self.at(">") {
                    return Err(self.fail("'>' expected"));
                }
                self.advance_to(self.pos + 1);
                if n.children.is_some() && real_text {
                    return Err(self.fail(&format!("<{}> has both text and child elements", n.tag)));
                }
                n.text = if n.children.is_none() { text } else { String::new() };
                n.multiline = n.children.is_none() && !real_text && n.text.contains('\n');
                return Ok(n);
            }
            if self.at("<!--") {
                self.skip_to("-->")?;
                continue;
            }
            if self.at("<?") {
                self.skip_to("?>")?;
                continue;
            }
            if self.at("<![CDATA[") {
                self.advance_to(self.pos + 9);
                let Some(end) = self.s.get(self.pos..).and_then(|r| r.find("]]>")) else {
                    return Err(self.fail("CDATA not closed"));
                };
                text.push_str(self.s.get(self.pos..self.pos + end).unwrap_or(""));
                real_text = true;
                self.advance_to(self.pos + end + 3);
                continue;
            }
            if self.at("<") {
                let child = self.element(depth + 1)?;
                n.children.get_or_insert_with(Vec::new).push(child);
                continue;
            }
            let next = self
                .s
                .as_bytes()
                .get(self.pos..)
                .and_then(|r| r.iter().position(|&b| b == b'<'))
                .map_or(self.s.len(), |i| self.pos + i);
            let chunk = self.s.get(self.pos..next).unwrap_or("");
            if chunk.chars().any(|c| !c.is_whitespace()) {
                real_text = true;
            }
            text.push_str(&self.decode(chunk)?);
            self.advance_to(next);
        }
    }
}

/// `XmlImport.Parse` of an XML text: its root element.
///
/// # Errors
/// The text is not XML DbxEdit reads (an `InvalidDataException` naming the line), or a bad character reference.
pub fn parse_xml(text: &str, path: &str) -> Result<Node> {
    let mut p = TextParser {
        s: text,
        pos: 0,
        line: 1,
    };
    p.skip_prolog()?;
    if !p.at("<") {
        return Err(Fault::invalid(format!("no root element in {path}")));
    }
    p.element(0)
}

/// What one field (`tag/first attribute value`) looks like in the files learned from.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The kind first seen.
    pub kind: i32,
    /// The number size last seen with values (or any, while none had values).
    pub size: i32,
    /// A bool / byte value's first byte.
    pub first_byte: i32,
    /// 4-byte values that read as decimals.
    pub floats: u32,
    /// 4-byte values that read as integers.
    pub ints: u32,
    /// The bool / byte kind seen for the field (-1: none).
    pub byte_kind: i32,
    /// A container's flag byte.
    pub container_byte: u8,
}

/// `TypeBook`: field types learned from binary files.
#[derive(Debug, Clone, Default)]
pub struct TypeBook {
    entries: HashMap<String, Entry>,
}

/// `tag/first attribute value` into `buf` (DbxEdit's key: the two joined, so `a/b` + `c` is `a` + `b/c`).
fn key<'b>(buf: &'b mut String, tag: &str, first_value: &str) -> &'b str {
    buf.clear();
    buf.push_str(tag);
    buf.push('/');
    buf.push_str(first_value);
    buf
}

/// `PrintsAsWhole`: whether the XML shows these 4 bytes as a whole number.
#[must_use]
pub fn prints_as_whole(bits: u32, field: &str) -> bool {
    is_whole_number_field(field) || (bits >> 24) == 0 || ((bits >> 23) & 0xFF) == 0xFF
}

impl TypeBook {
    /// Learns every element of `f`.
    ///
    /// # Errors
    /// A damaged string index or value.
    pub fn learn(&mut self, f: &EditFile) -> Result<()> {
        let mut buf = String::new();
        for e in &f.top_level {
            self.learn_element(&f.table, e, &mut buf)?;
        }
        Ok(())
    }

    fn learn_element(&mut self, t: &Table, e: &Element, buf: &mut String) -> Result<()> {
        let k = key(buf, t.tag(e)?, t.first_value(e)?);
        if !self.entries.contains_key(k) {
            self.entries.insert(
                k.to_owned(),
                Entry {
                    kind: e.kind,
                    size: 0,
                    first_byte: 1,
                    floats: 0,
                    ints: 0,
                    byte_kind: -1,
                    container_byte: 0,
                },
            );
        }
        let Some(entry) = self.entries.get_mut(buf.as_str()) else {
            return Ok(());
        };
        if let Some(children) = &e.children {
            entry.container_byte = e.container_byte;
            for c in children {
                self.learn_element(t, c, buf)?;
            }
            return Ok(());
        }
        if e.kind == NUMBERS {
            let mut pos = 0;
            let n = read_varint(&e.payload, &mut pos)?;
            let size = read_varint(&e.payload, &mut pos)?;
            if n > 0 || entry.size == 0 {
                entry.size = size;
            }
            if size == 4 {
                for _ in 0..n.max(0) {
                    let bits = be_u32(&e.payload, pos)?;
                    pos += 4;
                    if !prints_as_whole(bits, "") {
                        entry.floats += 1;
                    } else if (bits != 0 && (bits >> 24) == 0) || ((bits >> 24) == 0xFF && (bits & 0x0080_0000) != 0) {
                        // small negative integers (-1 slot indices) are NaN as floats: integer evidence too
                        entry.ints += 1;
                    }
                }
            }
        } else if e.kind != TEXT {
            entry.first_byte = i32::from(*e.payload.first().ok_or_else(Fault::index)?);
            entry.byte_kind = e.kind;
        }
        Ok(())
    }

    fn find(&self, buf: &mut String, tag: &str, first_value: &str) -> Option<&Entry> {
        self.entries.get(key(buf, tag, first_value))
    }
}

/// What an import did: values kept, changed, added, removed and rounded, and the guesses made.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// Values kept byte for byte.
    pub kept: u32,
    /// Values changed.
    pub changed: u32,
    /// Elements added.
    pub added: u32,
    /// Reference elements left out.
    pub removed: u32,
    /// Decimals rounded to the nearest 4-byte value.
    pub rounded: u32,
    /// Values typed by guess (`line N: path: no type known, taken as ...`).
    pub guesses: Vec<String>,
}

/// The import in progress: the file built (the reference's copy, or a new file) and the types.
struct Importer<'a> {
    file: &'a mut EditFile,
    types: &'a TypeBook,
    report: ImportReport,
    key_buf: String,
    key_x: String,
    key_o: String,
}

fn norm(s: &str) -> Cow<'_, str> {
    if s.contains('\r') {
        Cow::Owned(s.replace("\r\n", "\n").replace('\r', "\n"))
    } else {
        Cow::Borrowed(s)
    }
}

fn norm_attribute(s: &str) -> Cow<'_, str> {
    if s.contains(['\r', '\n', '\t']) {
        Cow::Owned(s.replace("\r\n", " ").replace(['\r', '\n', '\t'], " "))
    } else {
        Cow::Borrowed(s)
    }
}

/// [`sibling_key_into`] as a new string (tests).
#[cfg(test)]
fn sibling_key<'s>(tag: &str, attrs: impl Iterator<Item = (&'s str, &'s str)> + Clone) -> String {
    let mut key = String::with_capacity(tag.len() + 48);
    sibling_key_into(&mut key, tag, attrs);
    key
}

/// `Key` into `key` (cleared first): what identifies an element among its siblings - its guid, else its name, else
/// its reference, else all attributes.
fn sibling_key_into<'s>(key: &mut String, tag: &str, attrs: impl Iterator<Item = (&'s str, &'s str)> + Clone) {
    key.clear();
    key.push_str(tag);
    key.push('\u{1}');
    for id in ["guid", "name", "ref"] {
        if let Some((_, v)) = attrs.clone().find(|(k, _)| *k == id) {
            key.push_str(id);
            key.push('=');
            key.push_str(&norm_attribute(v));
            return;
        }
    }
    for (i, (k, v)) in attrs.enumerate() {
        if i > 0 {
            key.push('\u{1}');
        }
        key.push_str(k);
        key.push('=');
        key.push_str(&norm_attribute(v));
    }
}

/// Where an element is (`partition/instance[guid]/field[Name]`, as DbxEdit names it in messages), made into text
/// only when a message needs it.
enum Place<'a> {
    /// The root element: its tag.
    Root(&'a Node),
    /// A child: its parent's place, then `tag[first attribute value]`.
    Child(&'a Place<'a>, &'a Node),
    /// Given as text.
    Text(&'a str),
}

impl fmt::Display for Place<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Place::Root(n) => f.write_str(&n.tag),
            Place::Text(t) => f.write_str(t),
            Place::Child(parent, n) if n.attributes.is_empty() => write!(f, "{parent}/{}", n.tag),
            Place::Child(parent, n) => write!(f, "{parent}/{}[{}]", n.tag, n.first_value()),
        }
    }
}

impl Importer<'_> {
    /// `KeyOf` of a reference element into `key` (its strings checked by [`Importer::check_strings`]).
    fn key_of_into(&self, key: &mut String, e: &Element) {
        let t = &self.file.table;
        let attrs = e
            .attributes
            .iter()
            .map(|&(k, v)| (t.s(k).unwrap_or(""), t.s(v).unwrap_or("")));
        sibling_key_into(key, t.tag(e).unwrap_or(""), attrs);
    }

    /// The strings of the reference elements' keys exist (DbxEdit reads every key before it aligns).
    fn check_strings(&self, olds: &[Element]) -> Result<()> {
        let t = &self.file.table;
        for e in olds {
            t.tag(e)?;
            for &(k, v) in &e.attributes {
                t.s(k)?;
                t.s(v)?;
            }
        }
        Ok(())
    }

    fn guess(&mut self, x: &Node, path: &Place<'_>, what: &str) {
        self.report
            .guesses
            .push(format!("line {}: {path}: no type known, taken as {what}", x.line));
    }

    fn build(&mut self, x: &Node, old: Option<&Element>, path: &Place<'_>) -> Result<Element> {
        let tag = match old {
            Some(o) => o.tag,
            None => self.file.table.intern(&x.tag),
        };
        let mut e = Element {
            tag,
            ..Element::default()
        };
        for (i, (k, v)) in x.attributes.iter().enumerate() {
            let reuse = match old.and_then(|o| o.attributes.get(i)) {
                Some(&(ok, ov)) => {
                    let t = &self.file.table;
                    (t.s(ok)? == k && norm_attribute(t.s(ov)?) == norm_attribute(v)).then_some((ok, ov))
                }
                None => None,
            };
            let pair = if let Some(p) = reuse {
                p
            } else {
                let name = self.file.table.intern(k);
                (name, self.file.table.intern(v))
            };
            e.attributes.push(pair);
        }
        let learned = self.types.find(&mut self.key_buf, &x.tag, x.first_value()).cloned();
        let kind = self.kind_of(x, old, learned.as_ref(), path);
        e.kind = kind;
        if kind == CONTAINER {
            if x.children.is_none() && !x.text.trim().is_empty() {
                return Err(Fault::invalid(format!(
                    "line {}: {path} holds other elements, not the value '{}'",
                    x.line,
                    x.text.trim()
                )));
            }
            e.container_byte = match (old, &learned) {
                (Some(o), _) if o.is_container() => o.container_byte,
                (_, Some(l)) if l.kind == CONTAINER => l.container_byte,
                _ => 0,
            };
            let no_kids = Vec::new();
            let children = x.children.as_ref().unwrap_or(&no_kids);
            let old_children: &[Element] = old.filter(|o| o.is_container()).map_or(&[], Element::kids);
            let pairs = self.align(children, old_children)?;
            let mut built = Vec::with_capacity(children.len());
            for (c, m) in children.iter().zip(pairs) {
                let child_path = Place::Child(path, c);
                built.push(self.build(c, m.and_then(|i| old_children.get(i)), &child_path)?);
            }
            e.children = Some(built);
            if old.is_none() {
                self.report.added += 1;
            }
            return Ok(e);
        }
        if x.children.is_some() {
            return Err(Fault::invalid(format!(
                "line {}: {path} has child elements but is a value",
                x.line
            )));
        }
        if let Some(o) = old
            && o.kind == kind
            && norm(&value_text(&self.file.table, o)?) == norm(&x.text)
        {
            e.payload.clone_from(&o.payload);
            self.report.kept += 1;
            return Ok(e);
        }
        e.payload = self.encode(kind, x, old, learned.as_ref(), path)?;
        if old.is_some() {
            self.report.changed += 1;
        } else {
            self.report.added += 1;
        }
        Ok(e)
    }

    /// `Align`: each XML child's reference child (its index), the next unused one with the same key, in order.
    ///
    /// The reference child right after the last one matched is tried first: it is unused (every match so far is
    /// before it) and the first candidate the queues would give, so a match there is theirs. The queues are made
    /// only when that fails (an element added, removed or moved); entries left behind by the shortcut are before
    /// `next` and dropped as the queues drop used ones.
    fn align(&mut self, xs: &[Node], olds: &[Element]) -> Result<Vec<Option<usize>>> {
        self.check_strings(olds)?;
        let mut queues: Option<HashMap<String, VecDeque<usize>>> = None;
        let (mut kx, mut ko) = (std::mem::take(&mut self.key_x), std::mem::take(&mut self.key_o));
        let mut result = Vec::with_capacity(xs.len());
        let (mut next, mut used) = (0, 0);
        for x in xs {
            sibling_key_into(
                &mut kx,
                &x.tag,
                x.attributes.iter().map(|(a, b)| (a.as_str(), b.as_str())),
            );
            let mut found = None;
            if let Some(o) = olds.get(next) {
                self.key_of_into(&mut ko, o);
                if ko == kx {
                    found = Some(next);
                }
            }
            if found.is_none() {
                let q = queues.get_or_insert_with(|| {
                    let mut m: HashMap<String, VecDeque<usize>> = HashMap::new();
                    let mut k = String::new();
                    for (i, o) in olds.iter().enumerate() {
                        self.key_of_into(&mut k, o);
                        m.entry(k.clone()).or_default().push_back(i);
                    }
                    m
                });
                if let Some(q) = q.get_mut(kx.as_str()) {
                    while q.front().is_some_and(|&i| i < next) {
                        q.pop_front();
                    }
                    found = q.pop_front();
                }
            }
            if let Some(i) = found {
                next = i + 1;
                used += 1;
            }
            result.push(found);
        }
        self.key_x = kx;
        self.key_o = ko;
        self.report.removed += (olds.len() - used) as u32;
        Ok(result)
    }

    fn kind_of(&mut self, x: &Node, old: Option<&Element>, learned: Option<&Entry>, path: &Place<'_>) -> i32 {
        if x.children.is_some() {
            return CONTAINER;
        }
        if let Some(o) = old {
            return o.kind;
        }
        if x.self_closing {
            return TEXT;
        }
        if x.multiline {
            return CONTAINER;
        }
        if x.text.is_empty() {
            return NUMBERS;
        }
        if let Some(l) = learned {
            if l.byte_kind >= 0 && (x.text == "true" || x.text == "false") {
                return l.byte_kind;
            }
            if l.kind >= 0 && l.kind != CONTAINER {
                return l.kind;
            }
        }
        if x.text == "true" || x.text == "false" {
            self.guess(x, path, "bool");
            return 6;
        }
        if x.text.split('/').all(|t| net::try_parse_double(t).is_some()) {
            self.guess(x, path, "numbers (4 bytes each)");
            return NUMBERS;
        }
        self.guess(x, path, "text");
        TEXT
    }

    fn encode(
        &mut self,
        kind: i32,
        x: &Node,
        old: Option<&Element>,
        learned: Option<&Entry>,
        path: &Place<'_>,
    ) -> Result<Vec<u8>> {
        let text = &x.text;
        if kind == TEXT {
            return Ok(varint_bytes(self.file.table.intern(text)));
        }
        if kind != NUMBERS {
            let first = match old {
                Some(o) if o.payload.len() == 2 => o.payload.first().copied().unwrap_or(1),
                _ => learned.map_or(1, |l| l.first_byte as u8),
            };
            let t = text.trim();
            let value = match t {
                "true" => 1,
                "false" => 0,
                _ => net::try_parse_unsigned(t, 255).ok_or_else(|| {
                    Fault::invalid(format!("line {}: {path}: '{text}' is not true, false or 0-255", x.line))
                })? as u8,
            };
            return Ok(vec![first, value]);
        }
        let trimmed = text.trim();
        let tokens: Vec<&str> = if trimmed.is_empty() {
            Vec::new()
        } else {
            trimmed.split('/').collect()
        };
        let (mut old_count, mut old_size, mut old_at) = (0, 0, 0usize);
        if let Some(o) = old.filter(|o| o.kind == NUMBERS) {
            old_count = read_varint(&o.payload, &mut old_at)?;
            old_size = read_varint(&o.payload, &mut old_at)?;
        }
        let size = if old_size > 0 {
            old_size
        } else {
            learned.filter(|l| l.size > 0).map_or(4, |l| l.size)
        };
        if old_size == 0 && learned.is_none_or(|l| l.size == 0) && !tokens.is_empty() {
            self.guess(x, path, "4-byte numbers");
        }
        let mut out = Vec::with_capacity(2 + tokens.len() * 4);
        write_varint(&mut out, tokens.len() as i32);
        write_varint(&mut out, size);
        for (i, token) in tokens.iter().enumerate() {
            let token = token.trim();
            match size {
                4 => {
                    let old_bits = match old {
                        Some(o) if (i as i64) < i64::from(old_count) => Some(be_u32(&o.payload, old_at + 4 * i)?),
                        _ => None,
                    };
                    let bits = self.four_bytes(token, x.first_value(), old_bits, learned, x, path)?;
                    out.extend_from_slice(&bits.to_be_bytes());
                }
                8 => {
                    let d = net::try_parse_double(token)
                        .ok_or_else(|| Fault::invalid(format!("line {}: {path}: '{token}' is not a number", x.line)))?;
                    out.extend_from_slice(&d.to_bits().to_be_bytes());
                }
                2 => {
                    let v = net::try_parse_unsigned(token, 65535)
                        .ok_or_else(|| Fault::invalid(format!("line {}: {path}: '{token}' is not 0-65535", x.line)))?;
                    out.extend_from_slice(&(v as u16).to_be_bytes());
                }
                _ => {
                    return Err(Fault::invalid(format!(
                        "line {}: {path}: numbers of {size} bytes cannot be written",
                        x.line
                    )));
                }
            }
        }
        Ok(out)
    }

    /// `FourBytes`: the integer or the decimal reading of the token, whichever the XML would show as exactly this
    /// token; when both would, the original value's kind, else the field's usual kind, else decimal (reported).
    fn four_bytes(
        &mut self,
        token: &str,
        field: &str,
        old_bits: Option<u32>,
        learned: Option<&Entry>,
        x: &Node,
        path: &Place<'_>,
    ) -> Result<u32> {
        let mut options: Vec<u32> = Vec::with_capacity(2);
        let whole = net::try_parse_int(token);
        if let Some(w) = whole
            && four_byte_text(w as u32, field) == token
        {
            options.push(w as u32);
        }
        let single = net::try_parse_single(token);
        if let Some(f) = single {
            let bits = f.to_bits();
            if four_byte_text(bits, field) == token && !options.contains(&bits) {
                options.push(bits);
            }
        }
        match options.as_slice() {
            [] => {
                let integer_field = match old_bits {
                    Some(b) => prints_as_whole(b, field) && b != 0,
                    None => learned.is_some_and(|l| l.ints > l.floats),
                };
                if integer_field && let Some(w) = whole {
                    return Ok(w as u32);
                }
                if let Some(f) = single {
                    self.report.rounded += 1;
                    return Ok(f.to_bits());
                }
                Err(Fault::invalid(format!(
                    "line {}: {path}: '{token}' is not a number",
                    x.line
                )))
            }
            [one] => Ok(*one),
            [int, dec, ..] => {
                if let Some(b) = old_bits {
                    return Ok(if prints_as_whole(b, field) { *int } else { *dec });
                }
                if let Some(l) = learned.filter(|l| l.floats + l.ints > 0) {
                    return Ok(if l.floats >= l.ints { *dec } else { *int });
                }
                self.guess(x, path, &format!("decimal for '{token}'"));
                Ok(*dec)
            }
        }
    }
}

/// `XmlImport.Import` of an XML text (read by `text` once the types are learned, as DbxEdit reads it): `reference` the original `.dbx` (or none), `type_files` more files to learn
/// field types from. The result reuses the reference's string table (new strings are appended) and keeps its header
/// and trailing bytes.
///
/// # Errors
/// The XML cannot be read, a value does not fit its type, or a file learned from is damaged.
pub fn import(
    text: impl FnOnce() -> Result<String>,
    xml_path: &str,
    reference: Option<&EditFile>,
    type_files: &[EditFile],
) -> Result<(EditFile, ImportReport)> {
    let mut types = TypeBook::default();
    if let Some(r) = reference {
        types.learn(r)?;
    }
    for t in type_files {
        types.learn(t)?;
    }
    let mut target = match reference {
        Some(r) => r.clone(),
        None => EditFile::empty(),
    };
    let root = parse_xml(&text()?, xml_path)?;
    let old_top = std::mem::take(&mut target.top_level);
    let old = match old_top.first() {
        Some(o) if target.table.tag(o)? == root.tag => Some(o),
        _ => None,
    };
    let mut imp = Importer {
        file: &mut target,
        types: &types,
        report: ImportReport::default(),
        key_buf: String::new(),
        key_x: String::new(),
        key_o: String::new(),
    };
    let top = imp.build(&root, old, &Place::Root(&root))?;
    let report = imp.report;
    target.top_level = vec![top];
    Ok((target, report))
}

/// `XmlImport.EncodeValue`: the value bytes for a new text of an existing numbers element, typed like it (what the
/// import does for a changed value).
///
/// # Errors
/// The text is not a value of the element's type.
pub fn encode_value(file: &mut EditFile, e: &Element, text: &str) -> Result<Vec<u8>> {
    let mut attributes = Vec::with_capacity(e.attributes.len());
    for &(k, v) in &e.attributes {
        attributes.push((file.table.s(k)?.to_owned(), file.table.s(v)?.to_owned()));
    }
    let x = Node {
        tag: file.table.tag(e)?.to_owned(),
        text: text.to_owned(),
        attributes,
        ..Node::default()
    };
    let path = format!("{}[{}]", x.tag, x.first_value());
    let types = TypeBook::default();
    let mut imp = Importer {
        file,
        types: &types,
        report: ImportReport::default(),
        key_buf: String::new(),
        key_x: String::new(),
        key_o: String::new(),
    };
    imp.encode(e.kind, &x, Some(e), None, &Place::Text(&path))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn reader_keeps_what_dbxedit_keeps() {
        let doc = "<?xml version=\"1.0\"?>\r\n<!-- c -->\r\n<p g='1'>\r\n\t<f name=\"A\" name=\"B\">x</f>\r\n\t<f n=\"E\">a&amp;b&#65;&#x42;<![CDATA[<c>]]></f>\r\n\t<f>one\r\ntwo</f>\r\n\t<a>\r\n\t</a>\r\n\t<a></a>\r\n\t<e />\r\n</p>\r\n";
        let r = parse_xml(doc, "x").unwrap();
        let k = r.children.as_ref().unwrap();
        assert_eq!(k[0].attributes.len(), 2);
        assert_eq!(k[1].text, "a&bAB<c>");
        assert_eq!(k[2].text, "one\r\ntwo");
        assert!(k[3].multiline && !k[2].multiline);
        assert!(k[4].text.is_empty() && !k[4].multiline);
        assert!(k[5].self_closing);
        assert_eq!(k[5].line, 11);
        let e = parse_xml("<p><i></p>", "x").unwrap_err();
        assert_eq!(e.message, "line 1: </p> closes <i>");
        assert_eq!(parse_xml("x", "f.xml").unwrap_err().message, "no root element in f.xml");
        assert!(
            parse_xml("<p>&bad;</p>", "x")
                .unwrap_err()
                .message
                .contains("unknown entity &bad;")
        );
        assert!(parse_xml("<p>&#xD800;</p>", "x").is_err());
        assert!(
            parse_xml("<p a=1></p>", "x")
                .unwrap_err()
                .message
                .contains("quoted value")
        );
        assert!(
            parse_xml("<p>x<q/></p>", "x")
                .unwrap_err()
                .message
                .contains("both text and child")
        );
    }

    #[test]
    fn keys() {
        let k = sibling_key("i", [("type", "T"), ("guid", "G\tx")].into_iter());
        assert_eq!(k, "i\u{1}guid=G x");
        assert_eq!(
            sibling_key("i", [("a", "1"), ("b", "2")].into_iter()),
            "i\u{1}a=1\u{1}b=2"
        );
        assert_eq!(norm("a\r\nb\rc"), "a\nb\nc");
    }
}
