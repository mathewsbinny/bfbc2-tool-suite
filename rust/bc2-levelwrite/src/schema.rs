//! Field types from the partitions themselves, by `bfbc2-bf1943-import/dbxcatalogue.py`'s rules: per (instance type,
//! field path) the shapes the data has (`shape`), whether its 4-byte numbers read as whole numbers or as decimals
//! (`looks_whole`, `is_whole`), and the texts it takes. A field is written only in the type this evidence shows: a
//! 4-byte number whose values never say whole or decimal is refused unless the value is 0 (the same bits either way;
//! `author.py`'s rule), where DbxEdit's import would guess a decimal (`EDITOR_ARCHITECTURE.md` 3.2, item 4).
//!
//! The catalogue itself walks every partition of the game copy (minutes, in Python); the editor scans the
//! partitions it edits (the level partition, its static groups) the same way in milliseconds. Every instance is
//! walked (the catalogue stops at 200 per type).

use std::collections::{BTreeMap, BTreeSet, HashMap};

use bc2_formats::dbx::edit::{EditDbx, NodeId, numbers_of};
use bc2_formats::dbx::{CONTAINER, NUMBERS, TEXT};

use crate::ops::FieldValue;

/// The distinct texts kept per path (`dbxcatalogue.TEXTS_CAP`).
pub const TEXTS_CAP: usize = 400;

/// What the data says about one (type, path).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Facts {
    /// The shapes seen and how often (`complex-container`, `array-text`, `ref`, `field-text`, `num4x1`, `k6` ...).
    pub shapes: BTreeMap<String, u32>,
    /// 4-byte values that read as whole numbers.
    pub whole: u32,
    /// 4-byte values that read as decimals.
    pub float: u32,
    /// The second bytes of two-byte values seen.
    pub bytes: BTreeSet<u8>,
    /// The texts seen and how often (at most [`TEXTS_CAP`] distinct).
    pub texts: BTreeMap<String, u32>,
}

/// A field's type, as the schema reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldType {
    /// Text (a name, an enum value).
    Text,
    /// A reference.
    Ref,
    /// A two-byte value seen only as 0 or 1.
    Bool,
    /// A two-byte value seen with other values.
    Byte,
    /// One 4-byte whole number.
    Int,
    /// One 4-byte decimal.
    Float,
    /// Several 4-byte numbers, whole or decimal.
    Vector {
        /// How many.
        count: usize,
        /// Whole numbers (`false`: decimals).
        whole: bool,
    },
    /// The 16-number `Transform` (edited by moving and rotating).
    Transform,
    /// An array.
    Array,
    /// A structure (its fields are paths of their own).
    Complex,
    /// 4-byte numbers the data does not say are whole or decimal: only 0 can be written.
    UnknownNumber {
        /// How many.
        count: usize,
    },
    /// Numbers of another size.
    OtherNumbers,
}

impl FieldType {
    /// Its name in the properties panel.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Text => "text".into(),
            Self::Ref => "reference".into(),
            Self::Bool => "yes / no".into(),
            Self::Byte => "byte".into(),
            Self::Int => "whole number".into(),
            Self::Float => "decimal".into(),
            Self::Vector { count, whole } => {
                format!("{count} {}", if *whole { "whole numbers" } else { "decimals" })
            }
            Self::Transform => "transform".into(),
            Self::Array => "array".into(),
            Self::Complex => "structure".into(),
            Self::UnknownNumber { .. } => "number (whole or decimal unknown)".into(),
            Self::OtherNumbers => "numbers".into(),
        }
    }

    /// Whether a value of it can be set from the properties panel.
    #[must_use]
    pub fn settable(&self) -> bool {
        !matches!(self, Self::Transform | Self::Array | Self::Complex | Self::OtherNumbers)
    }
}

/// `dbxcatalogue.looks_whole`: whether a nonzero 4-byte value reads as a whole number rather than a float's bits.
#[must_use]
pub fn looks_whole(raw: &[u8]) -> Option<bool> {
    let b: [u8; 4] = raw.try_into().ok()?;
    let v = i32::from_be_bytes(b);
    if v == 0 {
        return None;
    }
    Some(matches!(v >> 24, 0 | -1))
}

/// `dbxcatalogue.shape`: an element's shape.
#[must_use]
pub fn shape(d: &EditDbx, e: NodeId) -> String {
    let tag = String::from_utf8_lossy(d.tag(e)).into_owned();
    let kind = d.kind(e);
    if kind == CONTAINER {
        return format!("{tag}-container");
    }
    if d.attr(e, b"ref").is_some() {
        return "ref".into();
    }
    if kind == TEXT {
        return format!("{tag}-text");
    }
    if kind == NUMBERS {
        return match numbers_of(d.raw(e)) {
            Some((size, vals)) => format!("num{size}x{}", vals.len()),
            None => "num?".into(),
        };
    }
    format!("k{kind}")
}

/// The schema: [`Facts`] per instance type and path (`.Field`, `.Struct.Field`, `.Array[]`).
#[derive(Debug, Clone, Default)]
pub struct Schema {
    by: HashMap<String, HashMap<String, Facts>>,
    /// Partitions scanned.
    pub partitions: usize,
    /// Instances walked.
    pub instances: usize,
}

fn lossy(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

impl Schema {
    /// The schema of these partitions.
    #[must_use]
    pub fn scan<'a>(files: impl IntoIterator<Item = &'a EditDbx>) -> Self {
        let mut s = Self::default();
        for d in files {
            s.add(d);
        }
        s
    }

    /// Adds a partition's instances (`Catalogue.add_partition`'s walk).
    pub fn add(&mut self, d: &EditDbx) {
        self.partitions += 1;
        for inst in d.instances() {
            self.instances += 1;
            let t = lossy(d.attr(inst, b"type").unwrap_or_default());
            let table = self.by.entry(t).or_default();
            walk(d, inst, "", table);
        }
    }

    /// The facts of a (type, path).
    #[must_use]
    pub fn facts(&self, t: &str, path: &str) -> Option<&Facts> {
        self.by.get(t)?.get(path)
    }

    /// `Catalogue.is_whole`: whole numbers (`Some(true)`), decimals (`Some(false)`), or not known.
    #[must_use]
    pub fn is_whole(&self, t: &str, path: &str) -> Option<bool> {
        let f = self.facts(t, path)?;
        match (f.whole > 0, f.float > 0) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        }
    }

    /// The texts a text field takes in the data, most used first (an enum field's names, for a choice).
    #[must_use]
    pub fn texts(&self, t: &str, path: &str) -> Vec<String> {
        let Some(f) = self.facts(t, path) else {
            return Vec::new();
        };
        let mut v: Vec<(&String, &u32)> = f.texts.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
        v.into_iter().map(|(s, _)| s.clone()).collect()
    }

    /// The type of element `e` at `path` of an instance of type `t`.
    #[must_use]
    pub fn type_of(&self, d: &EditDbx, t: &str, path: &str, e: NodeId) -> FieldType {
        let kind = d.kind(e);
        let tag = d.tag(e);
        if kind == CONTAINER {
            return if tag == b"array" {
                FieldType::Array
            } else {
                FieldType::Complex
            };
        }
        if tag == b"array" {
            // DICE writes an empty array as text
            return FieldType::Array;
        }
        if d.attr(e, b"ref").is_some() {
            return FieldType::Ref;
        }
        if kind == TEXT {
            return FieldType::Text;
        }
        if kind == NUMBERS {
            let Some((size, vals)) = numbers_of(d.raw(e)) else {
                return FieldType::OtherNumbers;
            };
            if size != 4 {
                return FieldType::OtherNumbers;
            }
            if vals.len() == 16 && d.attr(e, b"name") == Some(b"Transform") {
                return FieldType::Transform;
            }
            return match (self.is_whole(t, path), vals.len()) {
                (Some(true), 1) => FieldType::Int,
                (Some(false), 1) => FieldType::Float,
                (Some(whole), n) => FieldType::Vector { count: n, whole },
                (None, n) => FieldType::UnknownNumber { count: n },
            };
        }
        match self.facts(t, path) {
            Some(f) if f.bytes.iter().all(|b| *b <= 1) => FieldType::Bool,
            Some(_) => FieldType::Byte,
            None => FieldType::Bool,
        }
    }

    /// The value of element `e` read in type `ty` (`None` for arrays, structures, transforms and unknown numbers).
    #[must_use]
    pub fn value(d: &EditDbx, e: NodeId, ty: &FieldType) -> Option<FieldValue> {
        let four = |raw: &[u8]| -> Option<[u8; 4]> { raw.try_into().ok() };
        match ty {
            FieldType::Text => Some(FieldValue::Text(lossy(d.text(e)?))),
            FieldType::Ref => Some(FieldValue::Ref(lossy(d.attr(e, b"ref")?))),
            FieldType::Bool => Some(FieldValue::Bool(d.raw(e).get(1).copied()? != 0)),
            FieldType::Byte => Some(FieldValue::Byte(d.raw(e).get(1).copied()?)),
            FieldType::Int => {
                let (_, v) = numbers_of(d.raw(e))?;
                Some(FieldValue::Int(i64::from(i32::from_be_bytes(four(v.first()?)?))))
            }
            FieldType::Float => {
                let (_, v) = numbers_of(d.raw(e))?;
                Some(FieldValue::Float(f64::from(f32::from_be_bytes(four(v.first()?)?))))
            }
            FieldType::Vector { whole, .. } => {
                let (_, v) = numbers_of(d.raw(e))?;
                v.iter()
                    .map(|x| {
                        let b = four(x)?;
                        Some(if *whole {
                            f64::from(i32::from_be_bytes(b))
                        } else {
                            f64::from(f32::from_be_bytes(b))
                        })
                    })
                    .collect::<Option<Vec<f64>>>()
                    .map(FieldValue::Vector)
            }
            _ => None,
        }
    }
}

fn walk(d: &EditDbx, e: NodeId, path: &str, table: &mut HashMap<String, Facts>) {
    let Some(children) = d.children(e) else { return };
    for &c in children {
        let p = match d.attr(c, b"name") {
            Some(n) => format!("{path}.{}", lossy(n)),
            None => format!("{path}[]"),
        };
        let s = shape(d, c);
        let f = table.entry(p.clone()).or_default();
        *f.shapes.entry(s.clone()).or_default() += 1;
        if (s == "field-text" || s == "complex-text") && f.texts.len() < TEXTS_CAP {
            *f.texts.entry(lossy(d.text(c).unwrap_or_default())).or_default() += 1;
        } else if s.starts_with("num4x") {
            if let Some((_, vals)) = numbers_of(d.raw(c)) {
                for v in vals {
                    match looks_whole(v) {
                        Some(true) => f.whole += 1,
                        Some(false) => f.float += 1,
                        None => {}
                    }
                }
            }
        } else if s.starts_with('k')
            && let Some(b) = d.raw(c).get(1)
        {
            f.bytes.insert(*b);
        }
        if d.kind(c) == CONTAINER {
            walk(d, c, &p, table);
        }
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assert_is_empty,
    clippy::doc_markdown
)]
mod tests {
    use super::*;
    use bc2_formats::dbx::{self, DbxFile, Element};

    fn file() -> Vec<u8> {
        let mut f = DbxFile::default();
        let mut insts = Vec::new();
        for (k, (team, radius, flag)) in [(1i32, 2.5f32, 1u8), (2, 0.0, 0), (0, 7.0, 1)].into_iter().enumerate() {
            let name = Element::text(&mut f, "field", &[("name", "Name")], &format!("Flag {k}"));
            let team = Element::numbers(&mut f, "field", &[("name", "Team")], 4, &team.to_be_bytes());
            let rad = Element::numbers(&mut f, "field", &[("name", "Radius")], 4, &radius.to_be_bytes());
            let zero = Element::numbers(&mut f, "field", &[("name", "Mystery")], 4, &0i32.to_be_bytes());
            let on = Element::scalar(&mut f, "field", 6, &[("name", "Enabled")], [1, flag]);
            let r = Element::text(&mut f, "field", &[("ref", "null"), ("name", "Area")], "");
            let arr = Element::text(&mut f, "array", &[("name", "Soldiers")], "");
            insts.push(Element::container(
                &mut f,
                "instance",
                &[("guid", &format!("G{k}")), ("type", "X.CapturePointEntityData")],
                vec![name, team, rad, zero, on, r, arr],
            ));
        }
        let part = Element::container(&mut f, "partition", &[("guid", "p")], insts);
        f.roots.push(part);
        dbx::write(&f).unwrap()
    }

    #[test]
    fn types_from_the_data() {
        let b = file();
        let d = EditDbx::parse(&b).unwrap();
        let s = Schema::scan([&d]);
        let t = "X.CapturePointEntityData";
        assert_eq!(s.is_whole(t, ".Team"), Some(true));
        assert_eq!(s.is_whole(t, ".Radius"), Some(false));
        assert_eq!(s.is_whole(t, ".Mystery"), None);
        let inst = d.instances()[0];
        let ty = |name: &[u8]| s.type_of(&d, t, &format!(".{}", lossy(name)), d.child(inst, name).unwrap());
        assert_eq!(ty(b"Team"), FieldType::Int);
        assert_eq!(ty(b"Radius"), FieldType::Float);
        assert_eq!(ty(b"Mystery"), FieldType::UnknownNumber { count: 1 });
        assert_eq!(ty(b"Enabled"), FieldType::Bool);
        assert_eq!(ty(b"Area"), FieldType::Ref);
        assert_eq!(ty(b"Name"), FieldType::Text);
        assert_eq!(ty(b"Soldiers"), FieldType::Array);
        assert_eq!(s.texts(t, ".Name").len(), 3);
        assert_eq!(
            Schema::value(&d, d.child(inst, b"Radius").unwrap(), &FieldType::Float),
            Some(FieldValue::Float(2.5))
        );
        assert_eq!(looks_whole(&1.0f32.to_be_bytes()), Some(false));
        assert_eq!(looks_whole(&(-3i32).to_be_bytes()), Some(true));
        assert_eq!(looks_whole(&[0; 4]), None);
    }
}
