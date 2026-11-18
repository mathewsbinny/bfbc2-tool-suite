//! `core/bfbc2core/navtune.py` (round F2b): the navigation side files' operations checked ([`validate`]), a side
//! file's text as `NavTune.save` writes it (`json.dump(..., indent=1, ensure_ascii=False)` and a newline:
//! [`dumps`]) and an operation's one-line summary ([`summary`]), on a JSON value ([`Value`]) the bindings build from
//! the Python objects.
//!
//! Each function returns `None` where the reference raises or decides by a Python rule not reproduced here (a
//! comparison of mismatched types, a value printed with `repr` that is not plain ASCII, a dict key that is not a
//! string): the caller then runs the reference.

use std::fmt::Write as _;

use bc2_shader::pyrepr::float_repr;

/// A JSON value as the side file holds it (Python's dict keeps its order: a list of pairs).
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `None`.
    Null,
    /// A bool.
    Bool(bool),
    /// An int that fits 64 bits.
    Int(i64),
    /// A float.
    Float(f64),
    /// A str.
    Str(String),
    /// A list (or a tuple: `json` writes it as a list).
    List(Vec<Value>),
    /// A dict with str keys, in insertion order.
    Dict(Vec<(String, Value)>),
}

impl Value {
    fn get(&self, k: &str) -> Option<&Value> {
        match self {
            Value::Dict(d) => d.iter().find(|(n, _)| n == k).map(|x| &x.1),
            _ => None,
        }
    }

    fn has(&self, k: &str) -> bool {
        self.get(k).is_some()
    }

    /// `isinstance(v, (int, float))` (a bool is an int).
    fn is_number(&self) -> bool {
        matches!(self, Value::Bool(_) | Value::Int(_) | Value::Float(_))
    }

    /// `len(v)`, `None` where Python's `len` raises.
    fn len(&self) -> Option<usize> {
        match self {
            Value::Str(s) => Some(s.chars().count()),
            Value::List(l) => Some(l.len()),
            Value::Dict(d) => Some(d.len()),
            _ => None,
        }
    }

    /// The number as a double (bools and ints as Python compares them; ints beyond 2^53 are refused).
    fn num(&self) -> Option<f64> {
        match self {
            Value::Bool(b) => Some(f64::from(u8::from(*b))),
            Value::Int(i) => i32::try_from(*i).ok().map(f64::from).or_else(|| {
                (i.unsigned_abs() < 1 << 53).then(|| {
                    #[allow(clippy::cast_precision_loss)] // exact below 2^53
                    let x = *i as f64;
                    x
                })
            }),
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }
}

/// `a > b` as Python compares two side-file values; `None` where it raises (or for a NaN, compared otherwise).
fn greater(a: &Value, b: &Value) -> Option<bool> {
    if let (Value::Str(x), Value::Str(y)) = (a, b) {
        return Some(x.chars().gt(y.chars()));
    }
    let (x, y) = (a.num()?, b.num()?);
    if x.is_nan() || y.is_nan() {
        return None;
    }
    Some(x > y)
}

/// `x[i]` of a list (or a str's character), `None` where Python raises.
fn item(v: &Value, i: usize) -> Option<Value> {
    match v {
        Value::List(l) => l.get(i).cloned(),
        Value::Str(s) => s.chars().nth(i).map(|c| Value::Str(c.to_string())),
        _ => None,
    }
}

/// Python's `repr` of a value the messages print (plain ASCII strings, numbers, `None`); `None` otherwise.
fn repr(v: &Value) -> Option<String> {
    match v {
        Value::Null => Some("None".into()),
        Value::Bool(b) => Some(if *b { "True" } else { "False" }.into()),
        Value::Int(i) => Some(i.to_string()),
        Value::Float(f) => Some(float_repr(*f)),
        Value::Str(s) => s
            .bytes()
            .all(|c| (0x20..0x7F).contains(&c) && c != b'\'' && c != b'\\')
            .then(|| format!("'{s}'")),
        _ => None,
    }
}

/// The operations and their required fields.
pub const OPS: [(&str, &[&str]); 8] = [
    ("walkable", &["region"]),
    ("unwalkable", &["region"]),
    ("seed", &["at"]),
    ("unseed", &["at"]),
    ("link-areas", &["a", "b"]),
    ("route-edge", &["a", "b", "action"]),
    ("traversal", &["kind", "bottom", "top"]),
    ("note", &["at", "text"]),
];
/// Region edits (NavKit bases only).
pub const REGION_OPS: [&str; 4] = ["walkable", "unwalkable", "seed", "unseed"];
/// Traversal kinds.
pub const TRAVERSALS: [&str; 4] = ["ladder", "jump", "vault", "drop"];

/// Python's `str.isspace()` of one character: Unicode's white space and the four separators U+001C - U+001F.
fn py_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `validate(o, kind)`: the problems of one operation (empty: fine); `None`: left to the reference.
#[must_use]
#[expect(clippy::too_many_lines, reason = "the reference's one function, rule for rule")]
pub fn validate(o: &Value, kind: &str) -> Option<Vec<String>> {
    if !matches!(o, Value::Dict(_)) {
        return None;
    }
    let mut problems = Vec::new();
    let name_v = o.get("op").cloned().unwrap_or(Value::Null);
    let req = match &name_v {
        Value::Str(s) => OPS.iter().find(|(n, _)| n == s).map(|x| x.1),
        Value::List(_) | Value::Dict(_) => return None, // unhashable: `in OPS` raises
        _ => None,
    };
    let Some(req) = req else {
        return Some(vec![format!("unknown operation {}", repr(&name_v)?)]);
    };
    let Value::Str(name) = &name_v else { return None };
    let name = name.as_str();
    for k in req {
        if !o.has(k) {
            problems.push(format!("{name}: {k} missing"));
        }
    }
    if kind == "dice" && REGION_OPS.contains(&name) {
        problems.push(format!(
            "{name}: not allowed on a DICE base (region edits need a NavKit base)"
        ));
    }
    for k in ["at", "a", "b", "bottom", "top"] {
        if let Some(v) = o.get(k) {
            let ok = matches!(v, Value::List(l) if l.len() == 3 && l.iter().all(Value::is_number));
            if !ok {
                problems.push(format!("{name}: {k} must be [x, y, z]"));
            }
        }
    }
    if let Some(r) = o.get("region") {
        if !matches!(r, Value::Dict(_)) {
            return None; // `in` on another type means something else
        }
        if let Some(poly) = r.get("polygon") {
            let n = poly.len()?;
            let bad = if n < 3 {
                true
            } else {
                let Value::List(pts) = poly else { return None };
                let mut any = false;
                for p in pts {
                    if p.len()? != 2 {
                        any = true;
                        break;
                    }
                }
                any
            };
            if bad {
                problems.push(format!("{name}: a polygon needs 3 or more [x, z] points"));
            }
            let y = r.get("y");
            let ylen = match y {
                None => 0,
                Some(v) => v.len()?,
            };
            let ybad = if ylen == 2 {
                let y = y?;
                if matches!(y, Value::Dict(_)) {
                    return None;
                }
                greater(&item(y, 0)?, &item(y, 1)?)?
            } else {
                true
            };
            if ybad {
                problems.push(format!("{name}: y must be [low, high]"));
            }
        } else if let Some(b) = r.get("box") {
            let bad = if b.len()? == 6 {
                if matches!(b, Value::Dict(_)) {
                    return None;
                }
                let mut any = false;
                for k in 0..3 {
                    if greater(&item(b, k)?, &item(b, k + 3)?)? {
                        any = true;
                        break;
                    }
                }
                any
            } else {
                true
            };
            if bad {
                problems.push(format!(
                    "{name}: box must be [x0, y0, z0, x1, y1, z1] with each low <= high"
                ));
            }
        } else {
            problems.push(format!("{name}: region must be a polygon or a box"));
        }
    }
    if let Some(v) = o.get("two-way")
        && !matches!(v, Value::Bool(_))
    {
        problems.push(format!("{name}: two-way must be true or false"));
    }
    if name == "route-edge" && !matches!(o.get("action"), Some(Value::Str(s)) if s == "add" || s == "remove") {
        problems.push("route-edge: action must be add or remove".into());
    }
    if name == "traversal" && !matches!(o.get("kind"), Some(Value::Str(s)) if TRAVERSALS.contains(&s.as_str())) {
        problems.push(format!("traversal: kind must be one of {}", TRAVERSALS.join(", ")));
    }
    if name == "note" {
        // str(o.get('text', '')).strip(): only a str can be empty after it (every other value prints something)
        if let Value::Str(s) = o.get("text").unwrap_or(&Value::Str(String::new()))
            && s.chars().all(py_space)
        {
            problems.push("note: text is empty".into());
        }
    }
    Some(problems)
}

/// `json`'s string: `"` and `\` escaped, `\n \r \t \b \f`, other controls as `\u00XX`; everything else as it is.
fn json_str(s: &str, out: &mut String) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn json_value(v: &Value, level: usize, out: &mut String) {
    match v {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => {
            if f.is_nan() {
                out.push_str("NaN");
            } else if f.is_infinite() {
                out.push_str(if *f > 0.0 { "Infinity" } else { "-Infinity" });
            } else {
                out.push_str(&float_repr(*f));
            }
        }
        Value::Str(s) => json_str(s, out),
        Value::List(l) => {
            if l.is_empty() {
                out.push_str("[]");
                return;
            }
            out.push('[');
            for (i, x) in l.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.extend(std::iter::repeat_n(' ', level + 1));
                json_value(x, level + 1, out);
            }
            out.push('\n');
            out.extend(std::iter::repeat_n(' ', level));
            out.push(']');
        }
        Value::Dict(d) => {
            if d.is_empty() {
                out.push_str("{}");
                return;
            }
            out.push('{');
            for (i, (k, x)) in d.iter().enumerate() {
                out.push_str(if i == 0 { "\n" } else { ",\n" });
                out.extend(std::iter::repeat_n(' ', level + 1));
                json_str(k, out);
                out.push_str(": ");
                json_value(x, level + 1, out);
            }
            out.push('\n');
            out.extend(std::iter::repeat_n(' ', level));
            out.push('}');
        }
    }
}

/// `json.dump(v, f, indent=1, ensure_ascii=False)` then the newline `save` writes.
#[must_use]
pub fn dumps(v: &Value) -> String {
    let mut out = String::new();
    json_value(v, 0, &mut out);
    out.push('\n');
    out
}

/// `f'{v:.1f}'` of a value, `None` for one that is not a number.
fn f1(v: &Value) -> Option<String> {
    let x = v.num()?;
    if !x.is_finite() {
        return None;
    }
    Some(fixed1(x))
}

/// Fixed with one decimal, the exact value rounded half to even (Python's `format`).
fn fixed1(x: f64) -> String {
    // 0.05-ties are exact only for values with few binary digits: Rust's exact formatting rounds them as Python
    format!("{x:.1}")
}

/// `fmt(p)`: `(x, y, z)` with one decimal each.
fn fmt(p: &Value) -> Option<String> {
    let Value::List(l) = p else { return None };
    let parts: Option<Vec<String>> = l.iter().map(f1).collect();
    Some(format!("({})", parts?.join(", ")))
}

/// `str()` of `by` / `when`: plain strings as they are; others left to the reference.
fn plain(v: Option<&Value>) -> Option<String> {
    match v {
        None => Some("?".into()),
        Some(Value::Str(s)) => Some(s.clone()),
        Some(Value::Int(i)) => Some(i.to_string()),
        Some(Value::Null) => Some("None".into()),
        _ => None,
    }
}

/// `summary(k)` of one operation; `None`: left to the reference.
#[must_use]
pub fn summary(o: &Value) -> Option<String> {
    let Some(Value::Str(name)) = o.get("op") else {
        return None;
    };
    let where_ = if let Some(r) = o.get("region") {
        if let Some(poly) = r.get("polygon") {
            let y = r.get("y")?;
            format!(
                "polygon of {} points, y {}..{}",
                poly.len()?,
                f1(&item(y, 0)?)?,
                f1(&item(y, 1)?)?
            )
        } else {
            let b = r.get("box")?;
            let d = |i: usize| -> Option<String> {
                let (a, z) = (item(b, i + 3)?.num()?, item(b, i)?.num()?);
                (a - z).is_finite().then(|| fixed1(a - z))
            };
            if !matches!(b, Value::List(_)) {
                return None;
            }
            format!("box {} x {} x {} m", d(0)?, d(1)?, d(2)?)
        }
    } else if name == "traversal" {
        let two = match o.get("two-way") {
            None => true,
            Some(Value::Bool(b)) => *b,
            Some(_) => return None,
        };
        let Some(Value::Str(kind)) = o.get("kind") else {
            return None;
        };
        format!(
            "{kind}{} {} -> {}",
            if two { " (two-way)" } else { "" },
            fmt(o.get("bottom")?)?,
            fmt(o.get("top")?)?
        )
    } else if o.has("a") {
        let action = match o.get("action") {
            None => String::new(),
            Some(Value::Str(s)) => format!(" ({s})"),
            Some(_) => return None,
        };
        format!("{} -> {}{action}", fmt(o.get("a")?)?, fmt(o.get("b")?)?)
    } else {
        let text = match o.get("text") {
            None => String::new(),
            Some(Value::Str(s)) => format!(": {s}"),
            Some(_) => return None,
        };
        format!("{}{text}", fmt(o.get("at")?)?)
    };
    Some(format!(
        "{name}: {where_}  [{}, {}]",
        plain(o.get("by"))?,
        plain(o.get("when"))?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Value {
        Value::Str(x.into())
    }

    #[test]
    fn checks_and_writes() {
        let o = Value::Dict(vec![
            ("op".into(), s("seed")),
            (
                "at".into(),
                Value::List(vec![Value::Float(1.0), Value::Int(2), Value::Float(3.5)]),
            ),
        ]);
        assert_eq!(validate(&o, "navkit"), Some(vec![]));
        assert_eq!(validate(&o, "dice").map(|v| v.len()), Some(1));
        let bad = Value::Dict(vec![("op".into(), s("fly"))]);
        assert_eq!(validate(&bad, "navkit"), Some(vec!["unknown operation 'fly'".into()]));
        assert_eq!(
            dumps(&o),
            "{\n \"op\": \"seed\",\n \"at\": [\n  1.0,\n  2,\n  3.5\n ]\n}\n"
        );
        assert_eq!(summary(&o), Some("seed: (1.0, 2.0, 3.5)  [?, ?]".into()));
    }
}
