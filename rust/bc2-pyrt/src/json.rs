//! Python's `json` module as the round O4 tools use it (`json.dumps` / `json.dump` and `json.load` / `json.loads`):
//! the same text out, the same values in.
//!
//! **Writing** ([`dumps`]): `ensure_ascii` (every character past `~` as `\uXXXX`, surrogate pairs above the BMP),
//! `allow_nan` (`NaN`, `Infinity`, `-Infinity`), floats as `float.__repr__`, ints as `int.__repr__`; without `indent`
//! the separators `', '` and `': '`, with it `','` and `': '`, one item per line indented by `indent` spaces per level,
//! an empty list or dict as `[]` / `{}`.
//!
//! **Reading** ([`loads`]): `json.loads`'s grammar (`json.scanner` / `json.decoder`, `strict=True`): whitespace `' \t\n\r'`,
//! numbers by the scanner's pattern (`-?(0|[1-9]\d*)(\.\d+)?([eE][-+]?\d+)?`: an int without fraction or exponent,
//! else `float()` of the text), `NaN`, `Infinity`, `-Infinity`, strings with their escapes (a control character
//! inside one is an error), a dict keeping each key's first position and its last value. Text Python would reject
//! gives `None`: the caller hands the command line to the Python, whose error text is its own.

// `float(x)` of a JSON int: Python's own rounding of a large int to a float
#![allow(clippy::cast_precision_loss)]

use std::fmt::Write as _;

use crate::fmt as pyfmt;

/// A JSON value as Python holds it: ints of any size the tools meet (`i128`), floats, dicts in insertion order.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Int(i128),
    Float(f64),
    Str(String),
    List(Vec<Value>),
    Dict(Vec<(String, Value)>),
}

impl Value {
    /// `d.get(key)` of a dict (None for another type or a missing key).
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Dict(items) => items.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    /// The items of a list (None for another type).
    #[must_use]
    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(v) => Some(v),
            _ => None,
        }
    }

    /// A str's text.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Value::Str(s) => Some(s),
            _ => None,
        }
    }

    /// A number as a float (`float(x)` of an int or float; bools are ints in Python).
    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Value::Int(i) => Some(*i as f64),
            Value::Float(f) => Some(*f),
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            _ => None,
        }
    }

    /// An int's value (a bool counts, as in Python).
    #[must_use]
    pub fn as_int(&self) -> Option<i128> {
        match self {
            Value::Int(i) => Some(*i),
            Value::Bool(b) => Some(i128::from(*b)),
            _ => None,
        }
    }

    /// Python's truth value.
    #[must_use]
    pub fn truthy(&self) -> bool {
        match self {
            Value::Null => false,
            Value::Bool(b) => *b,
            Value::Int(i) => *i != 0,
            Value::Float(f) => *f != 0.0,
            Value::Str(s) => !s.is_empty(),
            Value::List(v) => !v.is_empty(),
            Value::Dict(v) => !v.is_empty(),
        }
    }

    /// A dict made from (key, value) pairs.
    #[must_use]
    pub fn dict<K: Into<String>>(items: impl IntoIterator<Item = (K, Value)>) -> Value {
        Value::Dict(items.into_iter().map(|(k, v)| (k.into(), v)).collect())
    }

    /// A list of floats.
    #[must_use]
    pub fn floats(v: &[f64]) -> Value {
        Value::List(v.iter().map(|x| Value::Float(*x)).collect())
    }
}

/// A str as `json.dumps` writes it (`ensure_ascii`).
pub fn string(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 || (c as u32) > 0x7E => {
                let mut buf = [0u16; 2];
                for u in c.encode_utf16(&mut buf) {
                    let _ = write!(out, "\\u{u:04x}");
                }
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

/// `float.__repr__` as `json` writes it (`NaN`, `Infinity`, `-Infinity` for the others).
#[must_use]
pub fn float(x: f64) -> String {
    if x.is_nan() {
        "NaN".to_owned()
    } else if x.is_infinite() {
        if x > 0.0 { "Infinity" } else { "-Infinity" }.to_owned()
    } else {
        pyfmt::float_repr(x)
    }
}

fn write(v: &Value, indent: Option<usize>, level: usize, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => out.push_str(&float(*f)),
        Value::Str(s) => string(s, out),
        Value::List(items) => {
            if items.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (n, item) in items.iter().enumerate() {
                separator(n, indent, level + 1, out);
                write(item, indent, level + 1, out);
            }
            close(indent, level, out);
            out.push(']');
        }
        Value::Dict(items) => {
            if items.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (n, (k, item)) in items.iter().enumerate() {
                separator(n, indent, level + 1, out);
                string(k, out);
                out.push_str(": ");
                write(item, indent, level + 1, out);
            }
            close(indent, level, out);
            out.push('}');
        }
    }
}

fn separator(n: usize, indent: Option<usize>, level: usize, out: &mut String) {
    match indent {
        Some(k) => {
            if n > 0 {
                out.push(',');
            }
            out.push('\n');
            out.push_str(&" ".repeat(k * level));
        }
        None => {
            if n > 0 {
                out.push_str(", ");
            }
        }
    }
}

fn close(indent: Option<usize>, level: usize, out: &mut String) {
    if let Some(k) = indent {
        out.push('\n');
        out.push_str(&" ".repeat(k * level));
    }
}

/// `json.dumps(v)` (indent None) or `json.dumps(v, indent=k)`.
#[must_use]
pub fn dumps(v: &Value, indent: Option<usize>) -> String {
    let mut out = String::new();
    write(v, indent, 0, &mut out);
    out
}

/// `json.loads(text)`; None where Python raises (the caller hands off).
#[must_use]
pub fn loads(text: &str) -> Option<Value> {
    let b = text.as_bytes();
    let mut p = Parser { b, text, i: 0 };
    p.ws();
    let v = p.value()?;
    p.ws();
    (p.i == b.len()).then_some(v)
}

/// `json.load(open(path))` of bytes read with the ANSI code page (Python's default text encoding): the files the
/// tools read so are ASCII (written with `ensure_ascii`); anything else hands off. A UTF-8 file
/// (`open(path, encoding='utf-8')`) uses [`loads_utf8`].
#[must_use]
pub fn loads_ansi(data: &[u8]) -> Option<Value> {
    if !data.is_ascii() {
        return None;
    }
    loads(std::str::from_utf8(data).ok()?)
}

/// `json.load(open(path, encoding='utf-8'))`: a BOM is an error in Python's `json.loads` of a str (it raises
/// "Unexpected UTF-8 BOM"), so it hands off.
#[must_use]
pub fn loads_utf8(data: &[u8]) -> Option<Value> {
    let t = std::str::from_utf8(data).ok()?;
    if t.starts_with('\u{feff}') {
        return None;
    }
    loads(t)
}

struct Parser<'a> {
    b: &'a [u8],
    text: &'a str,
    i: usize,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while let Some(c) = self.b.get(self.i) {
            if matches!(c, b' ' | b'\t' | b'\n' | b'\r') {
                self.i += 1;
            } else {
                break;
            }
        }
    }

    fn eat(&mut self, s: &str) -> bool {
        if self.b.get(self.i..self.i + s.len()) == Some(s.as_bytes()) {
            self.i += s.len();
            true
        } else {
            false
        }
    }

    fn value(&mut self) -> Option<Value> {
        let c = *self.b.get(self.i)?;
        match c {
            b'"' => {
                self.i += 1;
                self.string().map(Value::Str)
            }
            b'{' => {
                self.i += 1;
                self.object()
            }
            b'[' => {
                self.i += 1;
                self.array()
            }
            b'n' if self.eat("null") => Some(Value::Null),
            b't' if self.eat("true") => Some(Value::Bool(true)),
            b'f' if self.eat("false") => Some(Value::Bool(false)),
            b'N' if self.eat("NaN") => Some(Value::Float(f64::NAN)),
            b'I' if self.eat("Infinity") => Some(Value::Float(f64::INFINITY)),
            b'-' if self.eat("-Infinity") => Some(Value::Float(f64::NEG_INFINITY)),
            b'-' | b'0'..=b'9' => self.number(),
            _ => None,
        }
    }

    fn digits(&mut self) -> usize {
        let s = self.i;
        while self.b.get(self.i).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
        }
        self.i - s
    }

    fn number(&mut self) -> Option<Value> {
        let start = self.i;
        if self.b.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        match self.b.get(self.i) {
            Some(b'0') => self.i += 1,
            Some(b'1'..=b'9') => {
                self.digits();
            }
            _ => return None,
        }
        let mut float = false;
        if self.b.get(self.i) == Some(&b'.') && self.b.get(self.i + 1).is_some_and(u8::is_ascii_digit) {
            self.i += 1;
            self.digits();
            float = true;
        }
        if matches!(self.b.get(self.i), Some(b'e' | b'E')) {
            let save = self.i;
            self.i += 1;
            if matches!(self.b.get(self.i), Some(b'+' | b'-')) {
                self.i += 1;
            }
            if self.digits() == 0 {
                self.i = save;
            } else {
                float = true;
            }
        }
        let t = self.text.get(start..self.i)?;
        if float {
            t.parse::<f64>().ok().map(Value::Float)
        } else {
            t.parse::<i128>().ok().map(Value::Int)
        }
    }

    fn hex4(&mut self) -> Option<u32> {
        let t = self.text.get(self.i..self.i + 4)?;
        if !t.bytes().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        self.i += 4;
        u32::from_str_radix(t, 16).ok()
    }

    fn string(&mut self) -> Option<String> {
        let mut out = String::new();
        loop {
            let start = self.i;
            while let Some(&c) = self.b.get(self.i) {
                if c == b'"' || c == b'\\' || c < 0x20 {
                    break;
                }
                self.i += 1;
            }
            out.push_str(self.text.get(start..self.i)?);
            let c = *self.b.get(self.i)?;
            self.i += 1;
            match c {
                b'"' => return Some(out),
                b'\\' => {
                    let e = *self.b.get(self.i)?;
                    self.i += 1;
                    match e {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{08}'),
                        b'f' => out.push('\u{0c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let u = self.hex4()?;
                            let cp = if (0xD800..0xDC00).contains(&u) && self.b.get(self.i..self.i + 2) == Some(b"\\u")
                            {
                                let save = self.i;
                                self.i += 2;
                                match self.hex4() {
                                    Some(l) if (0xDC00..0xE000).contains(&l) => {
                                        0x10000 + ((u - 0xD800) << 10) + (l - 0xDC00)
                                    }
                                    _ => {
                                        self.i = save;
                                        u
                                    }
                                }
                            } else {
                                u
                            };
                            // a lone surrogate is a str Rust cannot hold: handed off
                            out.push(char::from_u32(cp)?);
                        }
                        _ => return None,
                    }
                }
                _ => return None, // a control character (strict)
            }
        }
    }

    fn array(&mut self) -> Option<Value> {
        let mut items = Vec::new();
        self.ws();
        if self.eat("]") {
            return Some(Value::List(items));
        }
        loop {
            self.ws();
            items.push(self.value()?);
            self.ws();
            if self.eat("]") {
                return Some(Value::List(items));
            }
            if !self.eat(",") {
                return None;
            }
        }
    }

    fn object(&mut self) -> Option<Value> {
        let mut items: Vec<(String, Value)> = Vec::new();
        self.ws();
        if self.eat("}") {
            return Some(Value::Dict(items));
        }
        loop {
            self.ws();
            if !self.eat("\"") {
                return None;
            }
            let k = self.string()?;
            self.ws();
            if !self.eat(":") {
                return None;
            }
            self.ws();
            let v = self.value()?;
            if let Some(slot) = items.iter_mut().find(|(key, _)| *key == k) {
                slot.1 = v;
            } else {
                items.push((k, v));
            }
            self.ws();
            if self.eat("}") {
                return Some(Value::Dict(items));
            }
            if !self.eat(",") {
                return None;
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn dumps_as_python() {
        let v = Value::dict([
            ("level", Value::Str("SP_01".into())),
            ("box", Value::floats(&[-120.0, 0.5, 1e16, 1e-5])),
            ("e", Value::List(vec![])),
            ("d", Value::Dict(vec![])),
            ("u", Value::Str("é\u{1F600}".into())),
            (
                "n",
                Value::List(vec![
                    Value::Null,
                    Value::Bool(true),
                    Value::Int(-3),
                    Value::Float(f64::NAN),
                ]),
            ),
        ]);
        assert_eq!(
            dumps(&v, None),
            "{\"level\": \"SP_01\", \"box\": [-120.0, 0.5, 1e+16, 1e-05], \"e\": [], \"d\": {}, \"u\": \"\\u00e9\\ud83d\\ude00\", \"n\": [null, true, -3, NaN]}"
        );
        assert_eq!(
            dumps(
                &Value::dict([("a", Value::List(vec![Value::Int(1), Value::Int(2)]))]),
                Some(1)
            ),
            "{\n \"a\": [\n  1,\n  2\n ]\n}"
        );
    }

    #[test]
    fn loads_as_python() {
        let v = loads(" {\"a\": [1, 2.0, -0, 1e2, NaN], \"a\": 3, \"b\": \"x\\u00e9\\ud83d\\ude00\"} ").unwrap();
        assert_eq!(v.get("a"), Some(&Value::Int(3)));
        assert_eq!(v.get("b").and_then(Value::as_str), Some("xé\u{1F600}"));
        assert!(loads("[1,]").is_none());
        assert!(loads("01").is_none());
        assert!(loads("\"a\tb\"").is_none());
        assert_eq!(loads("1.5e3"), Some(Value::Float(1500.0)));
        assert_eq!(loads("-Infinity"), Some(Value::Float(f64::NEG_INFINITY)));
    }
}
