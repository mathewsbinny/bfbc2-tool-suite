//! `json.dump(value, f, indent=1)` (`ensure_ascii` on) of the values the menu tools write.

use std::fmt::Write as _;

/// A JSON value with its dicts in insertion order.
pub enum J {
    Null,
    Bool(bool),
    Int(i64),
    Str(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

/// A string as `json.dumps` writes it (`ensure_ascii`).
fn string(s: &str, out: &mut String) {
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
            c if (c as u32) < 0x20 || (c as u32) > 0x7E && (c as u32) != 0x7F => {
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

fn write(v: &J, level: usize, out: &mut String) {
    match v {
        J::Null => out.push_str("null"),
        J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        J::Int(i) => out.push_str(&i.to_string()),
        J::Str(s) => string(s, out),
        J::Arr(a) => {
            if a.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (k, x) in a.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&" ".repeat(level + 1));
                write(x, level + 1, out);
            }
            out.push('\n');
            out.push_str(&" ".repeat(level));
            out.push(']');
        }
        J::Obj(o) => {
            if o.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (k, (key, x)) in o.iter().enumerate() {
                if k > 0 {
                    out.push(',');
                }
                out.push('\n');
                out.push_str(&" ".repeat(level + 1));
                string(key, out);
                out.push_str(": ");
                write(x, level + 1, out);
            }
            out.push('\n');
            out.push_str(&" ".repeat(level));
            out.push('}');
        }
    }
}

/// The text `json.dump(v, f, indent=1)` writes (no newline at the end).
pub fn dumps(v: &J) -> String {
    let mut out = String::new();
    write(v, 0, &mut out);
    out
}
