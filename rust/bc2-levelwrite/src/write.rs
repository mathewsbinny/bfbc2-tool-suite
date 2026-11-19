//! levelwrite: one mutation API for a level partition (`EDITOR_ARCHITECTURE.md` 3.2, item 4), on
//! [`EditDbx`] - the port of `bfbc2-stockmp-ai/dbxbin.py` - so whatever an edit does not touch is written back byte
//! for byte (the string table as stored until a string is added, then new strings at its end, as DbxEdit and
//! `dbxbin` add them).
//!
//! | Edit | What it does | Its reference |
//! |---|---|---|
//! | [`LevelWriter::set_transform`] | the instance's `Transform` rows (each row's fourth word kept) | `dbxbin.set_transform` (`author.py`'s `transform`) |
//! | [`LevelWriter::delete`] | the event connections from or to it removed, then DbxEdit's `drop`: the instance removed, array items naming it removed, single references to it made `null` | DbxEdit `drop=<guid>` (`bc2_formats::dbxedit::editor::drop`) |
//! | [`LevelWriter::duplicate`] | a clone with a new guid (attributes `guid` and `type` only), at the end of the partition; an `Objects` item and copies of its event connections when the original has them | `dbxbin.clone` / `set_attr` / `set_ref`, as `author.py`'s `instance` and `finish` |
//! | [`LevelWriter::set_field`] | a field set in the type the schema reads ([`crate::schema`]) | DbxEdit `setfield=` (text, numbers); `dbxbin.set_text` / `set_numbers` / `set_bool` / `set_ref` |
//!
//! [`apply`] replays a project's operations on its base partition, in order.

use bc2_formats::dbx::CONTAINER;
use bc2_formats::dbx::edit::{EditDbx, EditError, NodeId};
use thiserror::Error;

use std::collections::{HashMap, HashSet};

use crate::ops::{FieldIs, FieldValue, Op, Operation, Placed, Rows, SubCopy};
use crate::schema::{FieldType, Schema};

/// Why an edit was not made (in words for the Problems panel).
#[derive(Debug, Error, Clone, PartialEq)]
#[non_exhaustive]
pub enum LevelWriteError {
    /// The partition did not read.
    #[error("the level partition does not read: {0}")]
    Read(String),
    /// No instance with this guid.
    #[error("{0} is not in the level (deleted, or the base level changed)")]
    NotFound(String),
    /// A new guid that is already taken.
    #[error("{0} is already in the level (the operation was applied twice?)")]
    Exists(String),
    /// The instance has no such field.
    #[error("{guid} has no field {field}")]
    NoField {
        /// The instance.
        guid: String,
        /// The field.
        field: String,
    },
    /// A value the field's type does not take.
    #[error("{field} of {guid}: {why}")]
    Value {
        /// The instance.
        guid: String,
        /// The field.
        field: String,
        /// Why.
        why: String,
    },
    /// Something the editor does not change.
    #[error("{0}")]
    Refused(String),
    /// The DBX writer refused.
    #[error("{0}")]
    Dbx(String),
}

impl From<EditError> for LevelWriteError {
    fn from(e: EditError) -> Self {
        Self::Dbx(e.0)
    }
}

/// The result type of this module.
pub type Result<T> = std::result::Result<T, LevelWriteError>;

/// What a delete changed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Deleted {
    /// Instances removed (1).
    pub instances: usize,
    /// References met (array items removed, single references made `null`).
    pub references: usize,
    /// Event connections from or to it removed.
    pub connections: usize,
}

/// What a duplicate added.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Duplicated {
    /// An item in `LevelData.Objects` (when the original has one).
    pub objects_item: bool,
    /// Event connections copied.
    pub connections: usize,
}

/// `refers_to` of DbxEdit's `drop`: a reference to the guid, local (`guid`) or into a partition (`path/guid`).
#[must_use]
pub fn refers_to(r: &[u8], guid: &[u8]) -> bool {
    r.eq_ignore_ascii_case(guid)
        || (r.len() > guid.len()
            && r.get(r.len() - guid.len() - 1) == Some(&b'/')
            && r.get(r.len() - guid.len()..)
                .is_some_and(|t| t.eq_ignore_ascii_case(guid)))
}

/// A level partition being edited.
#[derive(Debug, Clone)]
pub struct LevelWriter {
    /// The partition.
    pub d: EditDbx,
}

impl LevelWriter {
    /// A partition's bytes, to edit.
    ///
    /// # Errors
    /// Not a binary `.dbx`, or one the editable reader refuses (damaged).
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        let d = EditDbx::parse(bytes).map_err(|e| LevelWriteError::Read(e.0))?;
        if !d.has_partition() {
            return Err(LevelWriteError::Read("no partition element".into()));
        }
        Ok(Self { d })
    }

    /// The partition's bytes.
    ///
    /// # Errors
    /// An element with more than 15 attributes (none of the edits makes one).
    pub fn write(&self) -> Result<Vec<u8>> {
        Ok(self.d.write()?)
    }

    /// The instance with this guid (any case).
    #[must_use]
    pub fn find(&self, guid: &str) -> Option<NodeId> {
        self.d.instance(guid.as_bytes(), <[u8]>::to_ascii_uppercase)
    }

    fn need(&self, guid: &str) -> Result<NodeId> {
        self.find(guid)
            .ok_or_else(|| LevelWriteError::NotFound(guid.to_owned()))
    }

    /// The level's `LevelData` instance.
    #[must_use]
    pub fn level_data(&self) -> Option<NodeId> {
        self.d.of_type(b".LevelData").first().copied()
    }

    /// Every guid of the partition's instances, upper case.
    #[must_use]
    pub fn guids(&self) -> Vec<String> {
        self.d
            .instances()
            .into_iter()
            .map(|e| String::from_utf8_lossy(self.d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase())
            .collect()
    }

    /// The instance's type.
    #[must_use]
    pub fn type_of(&self, e: NodeId) -> String {
        String::from_utf8_lossy(self.d.attr(e, b"type").unwrap_or_default()).into_owned()
    }

    /// An instance's `Transform` rows.
    #[must_use]
    pub fn transform(&self, guid: &str) -> Option<Rows> {
        let e = self.find(guid)?;
        let t = self.d.child(e, b"Transform")?;
        let (size, vals) = bc2_formats::dbx::edit::numbers_of(self.d.raw(t))?;
        if size != 4 || vals.len() != 16 {
            return None;
        }
        let f = |k: usize| -> f32 {
            vals.get(k)
                .and_then(|v| <[u8; 4]>::try_from(*v).ok())
                .map_or(0.0, f32::from_be_bytes)
        };
        Some(std::array::from_fn(|r| std::array::from_fn(|c| f(r * 4 + c))))
    }

    /// Sets an instance's transform (`dbxbin.set_transform`).
    ///
    /// # Errors
    /// No such instance, or one without a 16-number `Transform`.
    pub fn set_transform(&mut self, guid: &str, rows: &Rows) -> Result<()> {
        let e = self.need(guid)?;
        let t = self.d.child(e, b"Transform").ok_or_else(|| LevelWriteError::NoField {
            guid: guid.to_owned(),
            field: "Transform".into(),
        })?;
        self.d.set_transform(t, rows)?;
        Ok(())
    }

    /// The event connection arrays of the partition (every instance's `EventConnections` that is a container).
    fn connection_arrays(&self) -> Vec<NodeId> {
        self.d
            .instances()
            .into_iter()
            .filter_map(|i| self.d.child(i, b"EventConnections"))
            .filter(|&a| self.d.kind(a) == CONTAINER)
            .collect()
    }

    fn connects(&self, c: NodeId, guid: &[u8]) -> bool {
        [&b"Source"[..], b"Target"].iter().any(|f| {
            self.d
                .child(c, f)
                .and_then(|x| self.d.attr(x, b"ref"))
                .is_some_and(|r| refers_to(r, guid))
        })
    }

    /// Deletes an instance: its event connections removed first, then DbxEdit's `drop` (the instance, items naming
    /// it, single references to it made `null`).
    ///
    /// # Errors
    /// No such instance, or the level's `LevelData`.
    pub fn delete(&mut self, guid: &str) -> Result<Deleted> {
        let e = self.need(guid)?;
        if self.type_of(e).ends_with(".LevelData") {
            return Err(LevelWriteError::Refused(
                "the level itself (LevelData) cannot be deleted".into(),
            ));
        }
        let g = guid.as_bytes();
        let mut out = Deleted::default();
        for a in self.connection_arrays() {
            let kids = self.d.children(a).unwrap_or_default().to_vec();
            let keep: Vec<NodeId> = kids.iter().copied().filter(|&c| !self.connects(c, g)).collect();
            out.connections += kids.len() - keep.len();
            *self.d.children_mut(a)? = keep;
        }
        let mut roots = self.d.roots().to_vec();
        out.instances = self.drop_in(&mut roots, g, &mut out.references)?;
        *self.d.roots_mut() = roots;
        Ok(out)
    }

    /// DbxEdit's `drop_in`, on a list of elements (from the end, as DbxEdit walks it).
    fn drop_in(&mut self, list: &mut Vec<NodeId>, guid: &[u8], references: &mut usize) -> Result<usize> {
        let mut removed = 0;
        let mut i = list.len();
        while i > 0 {
            i -= 1;
            let Some(&e) = list.get(i) else { continue };
            let tag = self.d.tag(e).to_vec();
            if tag == b"instance" && self.d.attr(e, b"guid").is_some_and(|x| x.eq_ignore_ascii_case(guid)) {
                list.remove(i);
                removed += 1;
                continue;
            }
            if self.d.attr(e, b"ref").is_some_and(|r| refers_to(r, guid)) {
                *references += 1;
                if tag == b"item" {
                    list.remove(i);
                    continue;
                }
                self.d.set_attr(e, b"ref", b"null")?;
            }
            if let Some(kids) = self.d.children(e) {
                let mut kids = kids.to_vec();
                removed += self.drop_in(&mut kids, guid, references)?;
                *self.d.children_mut(e)? = kids;
            }
        }
        Ok(removed)
    }

    /// Duplicates an instance as `new_guid` (with `transform`, else the original's): the clone's attributes are its
    /// `guid` and `type` (`author.py`'s `instance`), it goes at the end of the partition, an `Objects` item is added
    /// when the original has one, and each event connection from or to the original is copied for the clone.
    ///
    /// # Errors
    /// No such source, the new guid taken, the level's `LevelData`, or a transform for an instance without one.
    pub fn duplicate(&mut self, source: &str, new_guid: &str, transform: Option<&Rows>) -> Result<Duplicated> {
        let src = self.need(source)?;
        if self.find(new_guid).is_some() {
            return Err(LevelWriteError::Exists(new_guid.to_owned()));
        }
        if self.type_of(src).ends_with(".LevelData") {
            return Err(LevelWriteError::Refused(
                "the level itself (LevelData) cannot be duplicated".into(),
            ));
        }
        let upper = new_guid.to_ascii_uppercase();
        let lower = new_guid.to_ascii_lowercase();
        let e = self.d.clone_node(src);
        self.d.set_attr(e, b"guid", upper.as_bytes())?;
        self.d.retain_attrs(e, |k| k == b"guid" || k == b"type")?;
        if let Some(rows) = transform {
            let t = self.d.child(e, b"Transform").ok_or_else(|| LevelWriteError::NoField {
                guid: source.to_owned(),
                field: "Transform".into(),
            })?;
            self.d.set_transform(t, rows)?;
        }
        let part = self
            .d
            .partition()
            .ok_or_else(|| LevelWriteError::Read("no partition element".into()))?;
        self.d.children_mut(part)?.push(e);
        let mut out = Duplicated::default();
        let g = source.as_bytes();
        if let Some(objs) = self.level_data().and_then(|l| self.d.child(l, b"Objects"))
            && let Some(item) = self
                .d
                .children(objs)
                .unwrap_or_default()
                .iter()
                .copied()
                .find(|&it| self.d.attr(it, b"ref").is_some_and(|r| refers_to(r, g)))
        {
            let it = self.d.clone_node(item);
            self.d.set_ref(it, lower.as_bytes())?;
            self.d.children_mut(objs)?.push(it);
            out.objects_item = true;
        }
        for a in self.connection_arrays() {
            let found: Vec<NodeId> = self
                .d
                .children(a)
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|&c| self.connects(c, g))
                .collect();
            for c in found {
                let copy = self.d.clone_node(c);
                for f in [&b"Source"[..], b"Target"] {
                    if let Some(x) = self.d.child(copy, f)
                        && self.d.attr(x, b"ref").is_some_and(|r| refers_to(r, g))
                    {
                        self.d.set_ref(x, lower.as_bytes())?;
                    }
                }
                self.d.children_mut(a)?.push(copy);
                out.connections += 1;
            }
        }
        Ok(out)
    }

    /// The element of a field path (`Name`, `Struct.Field`) of an instance.
    #[must_use]
    pub fn field(&self, inst: NodeId, path: &str) -> Option<NodeId> {
        let mut e = inst;
        for part in path.split('.') {
            e = self.d.child(e, part.as_bytes())?;
        }
        Some(e)
    }

    /// The type and value of a field (the value `None` where it is not a single value).
    ///
    /// # Errors
    /// No such instance or field.
    pub fn get_field(&self, schema: &Schema, guid: &str, path: &str) -> Result<(FieldType, Option<FieldValue>)> {
        let inst = self.need(guid)?;
        let e = self.field(inst, path).ok_or_else(|| LevelWriteError::NoField {
            guid: guid.to_owned(),
            field: path.to_owned(),
        })?;
        let ty = schema.type_of(&self.d, &self.type_of(inst), &format!(".{path}"), e);
        let v = Schema::value(&self.d, e, &ty);
        Ok((ty, v))
    }

    /// An instance's fields as the properties panel lists them: each direct field, and the fields of its structures
    /// one level down (`Struct.Field`), with its type and value; arrays and the transform are left out.
    #[must_use]
    pub fn fields(&self, schema: &Schema, guid: &str) -> Vec<(String, FieldType, Option<FieldValue>)> {
        let Some(inst) = self.find(guid) else { return Vec::new() };
        let t = self.type_of(inst);
        let mut out = Vec::new();
        let mut add = |path: String, e: NodeId| {
            let ty = schema.type_of(&self.d, &t, &format!(".{path}"), e);
            if !matches!(ty, FieldType::Array | FieldType::Transform | FieldType::Complex) {
                let v = Schema::value(&self.d, e, &ty);
                out.push((path, ty, v));
            }
        };
        for &c in self.d.children(inst).unwrap_or_default() {
            let Some(n) = self.d.attr(c, b"name") else { continue };
            let n = String::from_utf8_lossy(n).into_owned();
            if self.d.kind(c) == CONTAINER && self.d.tag(c) != b"array" {
                for &cc in self.d.children(c).unwrap_or_default() {
                    if let Some(m) = self.d.attr(cc, b"name") {
                        add(format!("{n}.{}", String::from_utf8_lossy(m)), cc);
                    }
                }
            } else {
                add(n, c);
            }
        }
        out
    }

    /// Sets a field in the type the schema reads (see [`crate::schema`]): text, reference, bool, byte, whole number,
    /// decimal, or a vector of either; a number of unknown kind only to 0.
    ///
    /// # Errors
    /// No such instance or field, a value of another type, a whole number out of range, or a field that is set
    /// otherwise (a transform: move and rotate; arrays and structures).
    pub fn set_field(&mut self, schema: &Schema, guid: &str, path: &str, value: &FieldValue) -> Result<()> {
        let inst = self.need(guid)?;
        let e = self.field(inst, path).ok_or_else(|| LevelWriteError::NoField {
            guid: guid.to_owned(),
            field: path.to_owned(),
        })?;
        let ty = schema.type_of(&self.d, &self.type_of(inst), &format!(".{path}"), e);
        let bad = |why: String| LevelWriteError::Value {
            guid: guid.to_owned(),
            field: path.to_owned(),
            why,
        };
        let whole32 = |v: f64| -> Result<[u8; 4]> {
            if v.fract() != 0.0 || v < f64::from(i32::MIN) || v > f64::from(i32::MAX) {
                return Err(bad(format!("{v} is not a whole number of 32 bits")));
            }
            Ok((v as i32).to_be_bytes())
        };
        let int32 = |n: i64| -> Result<i32> {
            i32::try_from(n).map_err(|_| bad(format!("{n} is not a whole number of 32 bits")))
        };
        let single = |v: f64| -> [u8; 4] { (v as f32).to_be_bytes() };
        match (&ty, value) {
            (FieldType::Text, FieldValue::Text(s)) => self.d.set_text(e, s.as_bytes())?,
            (FieldType::Ref, FieldValue::Ref(s)) => self.d.set_ref(e, s.as_bytes())?,
            (FieldType::Bool, FieldValue::Bool(b)) => self.d.set_bool(e, u8::from(*b))?,
            (FieldType::Byte, FieldValue::Byte(b)) => self.d.set_bool(e, *b)?,
            (FieldType::Bool, FieldValue::Byte(b)) if *b <= 1 => self.d.set_bool(e, *b)?,
            (FieldType::Int, FieldValue::Int(n)) => self.d.set_numbers(e, &[int32(*n)?.to_be_bytes()])?,
            (FieldType::Float, FieldValue::Float(f)) => self.d.set_numbers(e, &[single(*f)])?,
            (FieldType::Float, FieldValue::Int(n)) => self.d.set_numbers(e, &[single(f64::from(int32(*n)?))])?,
            (FieldType::Vector { count, whole }, FieldValue::Vector(v)) => {
                if v.len() != *count {
                    return Err(bad(format!("{} values where the field has {count}", v.len())));
                }
                let vals = v
                    .iter()
                    .map(|x| if *whole { whole32(*x) } else { Ok(single(*x)) })
                    .collect::<Result<Vec<_>>>()?;
                self.d.set_numbers(e, &vals)?;
            }
            (FieldType::UnknownNumber { count }, v) => {
                let zeros = match v {
                    FieldValue::Int(0) => *count == 1,
                    FieldValue::Float(f) => *f == 0.0 && *count == 1,
                    FieldValue::Vector(x) => x.len() == *count && x.iter().all(|y| *y == 0.0),
                    _ => false,
                };
                if !zeros {
                    return Err(bad(
                        "whether it holds whole numbers or decimals is not known from the level's data, so only 0 \
                         can be written"
                            .into(),
                    ));
                }
                self.d.set_numbers(e, &vec![[0u8; 4]; *count])?;
            }
            (FieldType::Transform, _) => return Err(bad("a transform: move or rotate the object instead".into())),
            (ty, v) => {
                return Err(bad(format!("a {} field does not take {v:?}", ty.name())));
            }
        }
        Ok(())
    }
}

/// The guid of an instance, upper case.
fn guid_of(d: &EditDbx, e: NodeId) -> String {
    String::from_utf8_lossy(d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase()
}

/// A local reference (a guid of this partition, not `null`, not `path/guid`), upper case.
fn local_ref(r: &[u8]) -> Option<String> {
    (!r.is_empty() && !r.contains(&b'/') && !r.eq_ignore_ascii_case(b"null"))
        .then(|| String::from_utf8_lossy(r).to_ascii_uppercase())
}

/// The value that most of `values` share (at least two of them, and more than half), if any.
fn shared_value(values: &[FieldValue]) -> Option<FieldValue> {
    let mut counts: Vec<(&FieldValue, usize)> = Vec::new();
    for v in values {
        match counts.iter_mut().find(|(x, _)| *x == v) {
            Some((_, n)) => *n += 1,
            None => counts.push((v, 1)),
        }
    }
    let (best, n) = counts.into_iter().max_by_key(|(_, n)| *n)?;
    (n >= 2 && n * 2 > values.len()).then(|| best.clone())
}

impl LevelWriter {
    /// The local references of each instance (upper-case guids of this partition), except those in `LevelData`'s
    /// `Objects` list and in event connections (every object is listed there; they are copied for copies anyway).
    fn ownership_graph(&self) -> (Vec<(String, NodeId)>, HashMap<String, Vec<String>>) {
        let level = self.level_data();
        let insts: Vec<(String, NodeId)> = self.d.instances().into_iter().map(|e| (guid_of(&self.d, e), e)).collect();
        let known: HashSet<&str> = insts.iter().map(|(g, _)| g.as_str()).collect();
        let mut out: HashMap<String, Vec<String>> = HashMap::new();
        for (g, e) in &insts {
            let mut refs = Vec::new();
            for &c in self.d.children(*e).unwrap_or_default() {
                let name = self.d.attr(c, b"name").unwrap_or_default();
                if name == b"EventConnections" || (Some(*e) == level && name == b"Objects") {
                    continue;
                }
                for x in self.d.walk(c) {
                    if let Some(r) = self.d.attr(x, b"ref").and_then(local_ref)
                        && known.contains(r.as_str())
                    {
                        refs.push(r);
                    }
                }
            }
            out.insert(g.clone(), refs);
        }
        (insts, out)
    }

    /// The sub-instances an object owns, in partition order: those it reaches through its references (directly or
    /// through other owned ones) that nothing else refers to - only the object, other owned sub-instances,
    /// `LevelData.Objects` and event connections (a flag's capture area points, its map marker, spawners only it
    /// names). A deep duplicate copies them with it; a plain duplicate's copy shares them. Empty for an unknown guid.
    #[must_use]
    pub fn owned(&self, guid: &str) -> Vec<String> {
        let x = guid.to_ascii_uppercase();
        let (insts, refs) = self.ownership_graph();
        if !refs.contains_key(&x) {
            return Vec::new();
        }
        let level = self.level_data().map(|l| guid_of(&self.d, l));
        let mut referrers: HashMap<&str, Vec<&str>> = HashMap::new();
        for (from, tos) in &refs {
            for t in tos {
                referrers.entry(t.as_str()).or_default().push(from.as_str());
            }
        }
        // the greatest set: reachable from the object, every referrer inside it
        let mut alive: HashSet<&str> = refs
            .keys()
            .map(String::as_str)
            .filter(|g| Some(*g) != level.as_deref())
            .collect();
        loop {
            let mut reach: HashSet<&str> = HashSet::from([x.as_str()]);
            let mut todo = vec![x.as_str()];
            while let Some(g) = todo.pop() {
                for t in refs.get(g).into_iter().flatten() {
                    if alive.contains(t.as_str()) && reach.insert(t.as_str()) {
                        todo.push(t.as_str());
                    }
                }
            }
            let out: Vec<&str> = reach
                .iter()
                .copied()
                .filter(|g| *g != x && referrers.get(g).into_iter().flatten().any(|r| !reach.contains(r)))
                .collect();
            if out.is_empty() {
                return insts
                    .iter()
                    .filter(|(g, _)| *g != x && reach.contains(g.as_str()))
                    .map(|(g, _)| g.clone())
                    .collect();
            }
            for g in out {
                alive.remove(g);
            }
        }
    }

    /// A deep duplicate: the object as `new_guid` (at `transform`) and each owned sub-instance as its [`SubCopy`]'s
    /// guid (at its transform, when it has one), at the end of the partition in that order; every local reference
    /// inside the copies to one of the originals remapped to its copy; an `Objects` item for each original that
    /// has one; each event connection from or to any of the originals copied once, both ends remapped.
    ///
    /// # Errors
    /// A source missing, a new guid taken, the level's `LevelData`, or a transform for an instance without one.
    pub fn deep_duplicate(
        &mut self,
        source: &str,
        new_guid: &str,
        transform: &Rows,
        owned: &[SubCopy],
    ) -> Result<Duplicated> {
        let pairs: Vec<(&str, &str, Option<&Rows>)> = std::iter::once((source, new_guid, Some(transform)))
            .chain(owned.iter().map(|o| (o.source.as_str(), o.guid.as_str(), o.transform.as_ref())))
            .collect();
        let mut srcs = Vec::new();
        for (s, n, _) in &pairs {
            let e = self.need(s)?;
            if self.find(n).is_some() {
                return Err(LevelWriteError::Exists((*n).to_owned()));
            }
            if self.type_of(e).ends_with(".LevelData") {
                return Err(LevelWriteError::Refused(
                    "the level itself (LevelData) cannot be duplicated".into(),
                ));
            }
            srcs.push(e);
        }
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(s, n, _)| (s.to_ascii_uppercase(), n.to_ascii_lowercase()))
            .collect();
        let part = self
            .d
            .partition()
            .ok_or_else(|| LevelWriteError::Read("no partition element".into()))?;
        let mut clones = Vec::new();
        for ((s, n, t), src) in pairs.iter().zip(srcs) {
            let e = self.d.clone_node(src);
            self.d.set_attr(e, b"guid", n.to_ascii_uppercase().as_bytes())?;
            self.d.retain_attrs(e, |k| k == b"guid" || k == b"type")?;
            if let Some(rows) = t {
                let tr = self.d.child(e, b"Transform").ok_or_else(|| LevelWriteError::NoField {
                    guid: (*s).to_owned(),
                    field: "Transform".into(),
                })?;
                self.d.set_transform(tr, rows)?;
            }
            self.d.children_mut(part)?.push(e);
            clones.push(e);
        }
        for e in clones {
            for x in self.d.walk(e) {
                if let Some(to) = self.d.attr(x, b"ref").and_then(local_ref).and_then(|r| map.get(&r)).cloned() {
                    self.d.set_ref(x, to.as_bytes())?;
                }
            }
        }
        let mut out = Duplicated::default();
        if let Some(objs) = self.level_data().and_then(|l| self.d.child(l, b"Objects")) {
            for (k, (s, n, _)) in pairs.iter().enumerate() {
                let item = self
                    .d
                    .children(objs)
                    .unwrap_or_default()
                    .iter()
                    .copied()
                    .find(|&it| self.d.attr(it, b"ref").is_some_and(|r| refers_to(r, s.as_bytes())));
                if let Some(item) = item {
                    let it = self.d.clone_node(item);
                    self.d.set_ref(it, n.to_ascii_lowercase().as_bytes())?;
                    self.d.children_mut(objs)?.push(it);
                    out.objects_item |= k == 0;
                }
            }
        }
        let mapped = |r: &[u8]| -> Option<String> {
            map.iter()
                .find(|(s, _)| refers_to(r, s.as_bytes()))
                .map(|(_, n)| n.clone())
        };
        for a in self.connection_arrays() {
            let found: Vec<NodeId> = self
                .d
                .children(a)
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|&c| {
                    [&b"Source"[..], b"Target"].iter().any(|f| {
                        self.d
                            .child(c, f)
                            .and_then(|x| self.d.attr(x, b"ref"))
                            .is_some_and(|r| mapped(r).is_some())
                    })
                })
                .collect();
            for c in found {
                let copy = self.d.clone_node(c);
                for f in [&b"Source"[..], b"Target"] {
                    if let Some(x) = self.d.child(copy, f)
                        && let Some(to) = self.d.attr(x, b"ref").and_then(mapped)
                    {
                        self.d.set_ref(x, to.as_bytes())?;
                    }
                }
                self.d.children_mut(a)?.push(copy);
                out.connections += 1;
            }
        }
        Ok(out)
    }

    /// The `ReferencedObject` reference of an instance, as stored.
    fn referenced(&self, e: NodeId) -> Option<&[u8]> {
        self.d.child(e, b"ReferencedObject").and_then(|c| self.d.attr(c, b"ref"))
    }

    /// The placed objects (`ReferenceObjectData`) of a blueprint reference (`path/guid`, any case); all of them
    /// for `None`.
    fn placements(&self, blueprint: Option<&str>) -> Vec<NodeId> {
        self.d
            .of_type(b".ReferenceObjectData")
            .into_iter()
            .filter(|&e| blueprint.is_none_or(|b| self.referenced(e).is_some_and(|r| r.eq_ignore_ascii_case(b.as_bytes()))))
            .collect()
    }

    /// The instance a new placement of `blueprint` is made from (levelwrite's "add instance"): a placement of the
    /// same blueprint when the level has one, else any placed object; of those, the one with the fewest local
    /// references (no object sets, no other instances named), the first of equals. `None` in a level without placed
    /// objects.
    #[must_use]
    pub fn template_for(&self, blueprint: &str) -> Option<String> {
        let same = self.placements(Some(blueprint));
        let pool = if same.is_empty() { self.placements(None) } else { same };
        let local = |e: NodeId| {
            self.d
                .walk(e)
                .into_iter()
                .filter(|&x| self.d.attr(x, b"ref").and_then(local_ref).is_some())
                .count()
        };
        pool.into_iter().min_by_key(|&e| local(e)).map(|e| guid_of(&self.d, e))
    }

    /// The fields a new placement of `blueprint` made from `template` takes from the level's data: for each field the
    /// template has (settable, not a reference), the value most of the level's placements of the blueprint share
    /// (more than half of them, two at least; every placed object when the level has none of it) where the
    /// template's differs. A value no majority shares stays the template's.
    #[must_use]
    pub fn place_defaults(&self, schema: &Schema, template: &str, blueprint: &str) -> Vec<FieldIs> {
        let same = self.placements(Some(blueprint));
        let peers = if same.is_empty() { self.placements(None) } else { same };
        let peers: Vec<String> = peers.into_iter().map(|e| guid_of(&self.d, e)).collect();
        let mut out = Vec::new();
        for (path, ty, value) in self.fields(schema, template) {
            if !ty.settable() || ty == FieldType::Ref {
                continue;
            }
            let values: Vec<FieldValue> = peers
                .iter()
                .filter_map(|g| self.get_field(schema, g, &path).ok().and_then(|(_, v)| v))
                .collect();
            if let Some(v) = shared_value(&values)
                && Some(&v) != value.as_ref()
            {
                out.push(FieldIs { field: path, value: v });
            }
        }
        out
    }

    /// Adds a placed object (levelwrite's "add instance", on the duplicate's machinery): a clone of the template as
    /// the new guid (attributes `guid` and `type`), its `ReferencedObject` the blueprint, its transform, its fields;
    /// at the end of the partition, with an `Objects` item (the template's copied, else the list's first). No event
    /// connections: a new object has none.
    ///
    /// # Errors
    /// The template missing or the level's `LevelData`, the new guid taken, a template without `Transform` or
    /// `ReferencedObject`, or a field value its type does not take.
    pub fn add_instance(&mut self, schema: &Schema, p: &Placed) -> Result<()> {
        let src = self.need(&p.template)?;
        if self.find(&p.guid).is_some() {
            return Err(LevelWriteError::Exists(p.guid.clone()));
        }
        if self.type_of(src).ends_with(".LevelData") {
            return Err(LevelWriteError::Refused(
                "the level itself (LevelData) cannot be a template".into(),
            ));
        }
        let missing = |field: &str| LevelWriteError::NoField {
            guid: p.template.clone(),
            field: field.into(),
        };
        let e = self.d.clone_node(src);
        self.d.set_attr(e, b"guid", p.guid.to_ascii_uppercase().as_bytes())?;
        self.d.retain_attrs(e, |k| k == b"guid" || k == b"type")?;
        let t = self.d.child(e, b"Transform").ok_or_else(|| missing("Transform"))?;
        self.d.set_transform(t, &p.transform)?;
        let r = self
            .d
            .child(e, b"ReferencedObject")
            .ok_or_else(|| missing("ReferencedObject"))?;
        self.d.set_ref(r, p.blueprint.as_bytes())?;
        let part = self
            .d
            .partition()
            .ok_or_else(|| LevelWriteError::Read("no partition element".into()))?;
        self.d.children_mut(part)?.push(e);
        if let Some(objs) = self.level_data().and_then(|l| self.d.child(l, b"Objects")) {
            let items = self.d.children(objs).unwrap_or_default();
            let g = p.template.as_bytes();
            let item = items
                .iter()
                .copied()
                .find(|&it| self.d.attr(it, b"ref").is_some_and(|r| refers_to(r, g)))
                .or_else(|| items.first().copied());
            if let Some(item) = item {
                let it = self.d.clone_node(item);
                self.d.set_ref(it, p.guid.to_ascii_lowercase().as_bytes())?;
                self.d.children_mut(objs)?.push(it);
            }
        }
        for f in &p.fields {
            self.set_field(schema, &p.guid, &f.field, &f.value)?;
        }
        Ok(())
    }
}

/// What [`apply`] did.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Applied {
    /// The edited partition.
    pub bytes: Vec<u8>,
    /// Operations that changed the partition.
    pub operations: usize,
    /// Transforms set, deletes, duplicates, fields set.
    pub transforms: usize,
    /// Instances deleted.
    pub deleted: usize,
    /// Instances added.
    pub duplicated: usize,
    /// Fields set.
    pub fields: usize,
    /// Objects placed (added instances).
    pub placed: usize,
}

/// Why [`apply`] stopped: the operation and the edit's error.
#[derive(Debug, Clone, PartialEq, Error)]
#[error("operation {n} ({name}): {error}")]
pub struct ApplyError {
    /// The operation's number.
    pub n: u64,
    /// Its name.
    pub name: String,
    /// What went wrong.
    pub error: LevelWriteError,
}

/// The base partition with `ops` applied in order (the organisation operations change nothing in it).
///
/// # Errors
/// The base does not read, or an operation cannot be applied (named, with why).
pub fn apply<'a>(
    base: &[u8],
    ops: impl IntoIterator<Item = &'a Operation>,
    schema: &Schema,
) -> std::result::Result<Applied, ApplyError> {
    let mut w = LevelWriter::parse(base).map_err(|error| ApplyError {
        n: 0,
        name: "read the base".into(),
        error,
    })?;
    let mut out = Applied::default();
    for o in ops {
        apply_one(&mut w, o, schema, &mut out)?;
    }
    out.bytes = w.write().map_err(|error| ApplyError {
        n: 0,
        name: "write the partition".into(),
        error,
    })?;
    Ok(out)
}

/// One operation applied to a partition being edited (the editor keeps one and applies each new operation), its
/// counts added to `out` (whose `bytes` it leaves alone).
///
/// # Errors
/// The operation cannot be applied (named, with why). Edits of it made before the failing one stay made.
pub fn apply_one(
    w: &mut LevelWriter,
    o: &Operation,
    schema: &Schema,
    out: &mut Applied,
) -> std::result::Result<(), ApplyError> {
    let fail = |error: LevelWriteError| ApplyError {
        n: o.n,
        name: o.name(),
        error,
    };
    match &o.op {
        Op::Move { items } | Op::Rotate { items } => {
            for m in items {
                w.set_transform(&m.guid, &m.to).map_err(fail)?;
                out.transforms += 1;
            }
        }
        Op::Delete { guids } => {
            for g in guids {
                out.deleted += w.delete(g).map_err(fail)?.instances;
            }
        }
        Op::Duplicate { items } => {
            for d in items {
                if d.owned.is_empty() {
                    w.duplicate(&d.source, &d.guid, Some(&d.transform)).map_err(fail)?;
                } else {
                    w.deep_duplicate(&d.source, &d.guid, &d.transform, &d.owned)
                        .map_err(fail)?;
                }
                out.duplicated += 1 + d.owned.len();
            }
        }
        Op::Place { items } => {
            for p in items {
                w.add_instance(schema, p).map_err(fail)?;
                out.placed += 1;
            }
        }
        Op::SetField { items } => {
            for f in items {
                w.set_field(schema, &f.guid, &f.field, &f.to).map_err(fail)?;
                out.fields += 1;
            }
        }
        _ => return Ok(()),
    }
    out.operations += 1;
    Ok(())
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assert_is_empty,
    clippy::doc_markdown
)]
pub(crate) mod tests {
    use super::*;
    use bc2_formats::dbx::{self, DbxFile, Element};

    /// A small level: LevelData (Objects: A, B; EventConnections A -> B), A and B with transforms, C referring to A.
    pub(crate) fn level() -> Vec<u8> {
        let mut f = DbxFile::default();
        let tr = |f: &mut DbxFile, x: f32| {
            let mut t = Vec::new();
            for v in [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, 2.0, 3.0, 0.0f32,
            ] {
                t.extend(v.to_be_bytes());
            }
            Element::numbers(f, "complex", &[("name", "Transform")], 4, &t)
        };
        let items: Vec<Element> = [
            "aaaaaaaa-0000-0000-0000-000000000001",
            "aaaaaaaa-0000-0000-0000-000000000002",
        ]
        .iter()
        .map(|g| Element::text(&mut f, "item", &[("ref", g)], ""))
        .collect();
        let objs = Element::container(&mut f, "array", &[("name", "Objects")], items);
        let conn = |f: &mut DbxFile, s: &str, t: &str| {
            let src = Element::text(f, "field", &[("ref", s), ("name", "Source")], "");
            let tgt = Element::text(f, "field", &[("ref", t), ("name", "Target")], "");
            let sn = Element::text(f, "field", &[("name", "Name")], "OnEnter");
            let se = Element::container(f, "complex", &[("name", "SourceEvent")], vec![sn]);
            let tn = Element::text(f, "field", &[("name", "Name")], "Enable");
            let te = Element::container(f, "complex", &[("name", "TargetEvent")], vec![tn]);
            Element::container(f, "complex", &[], vec![src, tgt, se, te])
        };
        let c1 = conn(
            &mut f,
            "aaaaaaaa-0000-0000-0000-000000000001",
            "aaaaaaaa-0000-0000-0000-000000000002",
        );
        let ec = Element::container(&mut f, "array", &[("name", "EventConnections")], vec![c1]);
        let lname = Element::text(&mut f, "field", &[("name", "Name")], "Levels/TEST");
        let ld = Element::container(
            &mut f,
            "instance",
            &[
                ("guid", "AAAAAAAA-0000-0000-0000-000000000000"),
                ("type", "X.LevelData"),
                ("id", "TEST"),
            ],
            vec![lname, objs, ec],
        );
        let mut insts = vec![ld];
        for (k, x) in [(1, 10.0f32), (2, 20.0)] {
            let name = Element::text(&mut f, "field", &[("name", "Name")], &format!("Obj {k}"));
            let team = Element::numbers(&mut f, "field", &[("name", "Team")], 4, &1i32.to_be_bytes());
            let t = tr(&mut f, x);
            insts.push(Element::container(
                &mut f,
                "instance",
                &[
                    ("guid", &format!("AAAAAAAA-0000-0000-0000-00000000000{k}")),
                    ("type", "X.CapturePointEntityData"),
                ],
                vec![name, team, t],
            ));
        }
        let r = Element::text(
            &mut f,
            "field",
            &[("ref", "aaaaaaaa-0000-0000-0000-000000000001"), ("name", "Area")],
            "",
        );
        let it = Element::text(&mut f, "item", &[("ref", "aaaaaaaa-0000-0000-0000-000000000001")], "");
        let arr = Element::container(&mut f, "array", &[("name", "Soldiers")], vec![it]);
        insts.push(Element::container(
            &mut f,
            "instance",
            &[
                ("guid", "AAAAAAAA-0000-0000-0000-000000000003"),
                ("type", "X.AreaOrderEntityData"),
            ],
            vec![r, arr],
        ));
        let part = Element::container(&mut f, "partition", &[("guid", "P")], insts);
        f.roots.push(part);
        dbx::write(&f).unwrap()
    }

    const A: &str = "aaaaaaaa-0000-0000-0000-000000000001";
    const B: &str = "AAAAAAAA-0000-0000-0000-000000000002";

    #[test]
    fn move_delete_duplicate_set() {
        let base = level();
        let mut w = LevelWriter::parse(&base).unwrap();
        assert_eq!(w.write().unwrap(), base);
        // move
        let mut rows = w.transform(A).unwrap();
        assert_eq!(rows[3], [10.0, 2.0, 3.0]);
        rows[3] = [11.5, 2.0, 3.0];
        w.set_transform(A, &rows).unwrap();
        assert_eq!(w.transform(A).unwrap()[3], [11.5, 2.0, 3.0]);
        // duplicate A: an Objects item and its connection copied
        let g = crate::new_guid("p", 1, A);
        let d = w.duplicate(A, &g, None).unwrap();
        assert_eq!(
            d,
            Duplicated {
                objects_item: true,
                connections: 1
            }
        );
        assert!(w.duplicate(A, &g, None).is_err());
        let back = LevelWriter::parse(&w.write().unwrap()).unwrap();
        assert_eq!(back.guids().len(), 5);
        assert_eq!(back.transform(&g).unwrap()[3], [11.5, 2.0, 3.0]);
        // set fields, typed from the data
        let s = Schema::scan([&w.d]);
        w.set_field(&s, B, "Team", &FieldValue::Int(2)).unwrap();
        w.set_field(&s, B, "Name", &FieldValue::Text("Flag B".into())).unwrap();
        assert!(w.set_field(&s, B, "Team", &FieldValue::Text("x".into())).is_err());
        assert!(w.set_field(&s, B, "Team", &FieldValue::Int(1 << 40)).is_err());
        assert!(w.set_field(&s, B, "Nope", &FieldValue::Int(1)).is_err());
        assert_eq!(
            w.get_field(&s, B, "Team").unwrap(),
            (FieldType::Int, Some(FieldValue::Int(2)))
        );
        // delete A: its connection removed, its Objects item removed, the order's reference made null, its item gone
        let del = w.delete(A).unwrap();
        assert_eq!(
            del,
            Deleted {
                instances: 1,
                references: 3,
                connections: 1
            }
        );
        let out = w.write().unwrap();
        let back = LevelWriter::parse(&out).unwrap();
        assert!(back.find(A).is_none() && back.find(&g).is_some());
        let order = back.find("AAAAAAAA-0000-0000-0000-000000000003").unwrap();
        assert_eq!(
            back.d.attr(back.d.child(order, b"Area").unwrap(), b"ref"),
            Some(&b"null"[..])
        );
        assert!(
            back.d
                .children(back.d.child(order, b"Soldiers").unwrap())
                .unwrap()
                .is_empty()
        );
        assert!(w.delete(A).is_err());
        let ld = w.level_data().unwrap();
        assert!(w.delete("AAAAAAAA-0000-0000-0000-000000000000").is_err());
        // the copy's connection stays (it refers to the copy)
        assert_eq!(
            w.d.children(w.d.child(ld, b"EventConnections").unwrap()).unwrap().len(),
            1
        );
        assert!(refers_to(b"levels/x/AB", b"ab") && !refers_to(b"xab", b"ab"));
    }

    /// A level with ownership to find: a flag F (Geometry: a ring of points P1 <-> P2; MapMarker M; Shared S, which G
    /// also names; an event connection F -> M), and three placed objects (R1, R2 of blueprint `a`, R3 of `b`, in an
    /// object set O).
    pub(crate) fn owning_level() -> Vec<u8> {
        let mut f = DbxFile::default();
        let g = |k: &str| format!("BBBBBBBB-0000-0000-0000-00000000000{k}");
        let lg = |k: &str| g(k).to_ascii_lowercase();
        let tr = |f: &mut DbxFile, x: f32| {
            let mut t = Vec::new();
            for v in [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, 0.0, 0.0, 0.0f32,
            ] {
                t.extend(v.to_be_bytes());
            }
            Element::numbers(f, "complex", &[("name", "Transform")], 4, &t)
        };
        let names = ["F", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
        let items: Vec<Element> = names
            .iter()
            .skip(1)
            .map(|k| Element::text(&mut f, "item", &[("ref", &lg(k))], ""))
            .collect();
        let objs = Element::container(&mut f, "array", &[("name", "Objects")], items);
        let src = Element::text(&mut f, "field", &[("ref", &lg("1")), ("name", "Source")], "");
        let tgt = Element::text(&mut f, "field", &[("ref", &lg("3")), ("name", "Target")], "");
        let c = Element::container(&mut f, "complex", &[], vec![src, tgt]);
        let ec = Element::container(&mut f, "array", &[("name", "EventConnections")], vec![c]);
        let ld = Element::container(
            &mut f,
            "instance",
            &[("guid", &g("0")), ("type", "X.LevelData")],
            vec![objs, ec],
        );
        let mut insts = vec![ld];
        // 1: the flag F
        let geo_item = Element::text(&mut f, "item", &[("ref", &lg("2"))], "");
        let geo = Element::container(&mut f, "array", &[("name", "Geometry")], vec![geo_item]);
        let marker = Element::text(&mut f, "field", &[("ref", &lg("3")), ("name", "MapMarker")], "");
        let shared = Element::text(&mut f, "field", &[("ref", &lg("5")), ("name", "Shared")], "");
        let ext = Element::text(
            &mut f,
            "field",
            &[("ref", "objects/flag/flag/abc"), ("name", "CapturePoint")],
            "",
        );
        let t = tr(&mut f, 1.0);
        insts.push(Element::container(
            &mut f,
            "instance",
            &[("guid", &g("1")), ("type", "X.CapturePointEntityData")],
            vec![t, geo, marker, shared, ext],
        ));
        // 2 and 4: the ring of points
        for (k, next) in [("2", "4"), ("4", "2")] {
            let t = tr(&mut f, 2.0);
            let n = Element::text(&mut f, "field", &[("ref", &lg(next)), ("name", "Next")], "");
            insts.push(Element::container(
                &mut f,
                "instance",
                &[("guid", &g(k)), ("type", "X.AreaGeometryEntityData")],
                vec![t, n],
            ));
        }
        // 3: the marker; 5: shared; 6: names 5
        let t = tr(&mut f, 3.0);
        insts.push(Element::container(
            &mut f,
            "instance",
            &[("guid", &g("3")), ("type", "X.MapMarkerEntityData")],
            vec![t],
        ));
        let n5 = Element::text(&mut f, "field", &[("name", "Name")], "shared");
        insts.push(Element::container(
            &mut f,
            "instance",
            &[("guid", &g("5")), ("type", "X.TeamEntityData")],
            vec![n5],
        ));
        let r5 = Element::text(&mut f, "field", &[("ref", &lg("5")), ("name", "Team")], "");
        insts.push(Element::container(
            &mut f,
            "instance",
            &[("guid", &g("6")), ("type", "X.SpawnEntityData")],
            vec![r5],
        ));
        // 7, 8: blueprint a (Enumeration 11, 12; Mode 4); 9: blueprint b in object set (Mode 7)
        for (k, bp, en, set) in [("7", "a", 11i32, false), ("8", "a", 12, false), ("9", "b", 13, true)] {
            let en = Element::numbers(&mut f, "field", &[("name", "Enumeration")], 4, &en.to_be_bytes());
            let t = tr(&mut f, 9.0);
            let r = Element::text(
                &mut f,
                "field",
                &[("ref", &format!("objects/{bp}/{bp}/0000")), ("name", "ReferencedObject")],
                "",
            );
            let mode = Element::numbers(
                &mut f,
                "field",
                &[("name", "Mode")],
                4,
                &(if bp == "a" { 4i32 } else { 7 }).to_be_bytes(),
            );
            let sets: Vec<Element> = if set {
                vec![Element::text(&mut f, "item", &[("ref", &lg("6"))], "")]
            } else {
                Vec::new()
            };
            let arr = Element::container(&mut f, "array", &[("name", "ObjectSets")], sets);
            insts.push(Element::container(
                &mut f,
                "instance",
                &[("guid", &g(k)), ("type", "Entity.ReferenceObjectData")],
                vec![en, t, r, mode, arr],
            ));
        }
        let part = Element::container(&mut f, "partition", &[("guid", "P")], insts);
        f.roots.push(part);
        dbx::write(&f).unwrap()
    }

    #[test]
    fn deep_duplicate_and_place() {
        let g = |k: &str| format!("BBBBBBBB-0000-0000-0000-00000000000{k}");
        let base = owning_level();
        let mut w = LevelWriter::parse(&base).unwrap();
        assert_eq!(w.write().unwrap(), base);
        // the flag owns its ring and its marker, not what another instance also names
        assert_eq!(w.owned(&g("1")), [g("2"), g("4"), g("3")]);
        assert!(w.owned(&g("6")).is_empty());
        assert!(w.owned("nope").is_empty());
        let new = |k: &str| crate::new_guid("p", 1, &g(k));
        let subs: Vec<SubCopy> = [("2", Some(2.0f32)), ("4", Some(2.0)), ("3", Some(3.0))]
            .iter()
            .map(|(k, x)| SubCopy {
                source: g(k),
                guid: new(k),
                transform: x.map(|x| crate::rows_at([x + 10.0, 0.0, 0.0])),
            })
            .collect();
        let d = w
            .deep_duplicate(&g("1"), &new("1"), &crate::rows_at([11.0, 0.0, 0.0]), &subs)
            .unwrap();
        assert_eq!(
            d,
            Duplicated {
                objects_item: true,
                connections: 1
            }
        );
        assert!(w.deep_duplicate(&g("1"), &new("1"), &crate::rows_at([0.0; 3]), &[]).is_err());
        let back = LevelWriter::parse(&w.write().unwrap()).unwrap();
        let s = Schema::scan([&back.d]);
        let r = |guid: &str, field: &str| match back.get_field(&s, guid, field).unwrap().1 {
            Some(FieldValue::Ref(r)) => r,
            v => panic!("{v:?}"),
        };
        // the copy's references point at the copies; the shared one and the blueprint stay
        assert_eq!(r(&new("1"), "MapMarker"), new("3").to_ascii_lowercase());
        assert_eq!(r(&new("1"), "Shared"), g("5").to_ascii_lowercase());
        assert_eq!(r(&new("1"), "CapturePoint"), "objects/flag/flag/abc");
        assert_eq!(r(&new("2"), "Next"), new("4").to_ascii_lowercase());
        assert_eq!(r(&new("4"), "Next"), new("2").to_ascii_lowercase());
        // the originals are untouched
        assert_eq!(r(&g("1"), "MapMarker"), g("3").to_ascii_lowercase());
        assert_eq!(back.transform(&new("2")).unwrap()[3], [12.0, 0.0, 0.0]);
        // Objects: 9 + 4; the connection copied with both ends remapped
        let ld = back.level_data().unwrap();
        assert_eq!(back.d.children(back.d.child(ld, b"Objects").unwrap()).unwrap().len(), 13);
        let ec = back.d.children(back.d.child(ld, b"EventConnections").unwrap()).unwrap();
        assert_eq!(ec.len(), 2);
        let end = |c: NodeId, f: &[u8]| back.d.attr(back.d.child(c, f).unwrap(), b"ref").unwrap().to_vec();
        assert_eq!(end(ec[1], b"Source"), new("1").to_ascii_lowercase().into_bytes());
        assert_eq!(end(ec[1], b"Target"), new("3").to_ascii_lowercase().into_bytes());
        // placing: the template is a plain placement of the blueprint; another blueprint takes the shared values
        assert_eq!(w.template_for("OBJECTS/A/A/0000").as_deref(), Some(g("7").as_str()));
        assert_eq!(w.template_for("objects/new/new/1").as_deref(), Some(g("7").as_str()));
        let s = Schema::scan([&w.d]);
        assert!(w.place_defaults(&s, &g("7"), "objects/a/a/0000").is_empty());
        // a placement of b made from a's template: b's one placement shares nothing (one is not a majority of two)
        assert!(w.place_defaults(&s, &g("7"), "objects/b/b/0000").is_empty());
        // made from b's template for a new blueprint: every placed object is a peer, Mode 4 is the majority
        let d = w.place_defaults(&s, &g("9"), "objects/new/new/1");
        assert_eq!(
            d,
            [FieldIs {
                field: "Mode".into(),
                value: FieldValue::Int(4)
            }]
        );
        let p = Placed {
            guid: crate::new_guid("p", 2, "objects/new/new/1"),
            template: g("9"),
            blueprint: "objects/new/new/1".into(),
            transform: crate::rows_at([5.0, 6.0, 7.0]),
            fields: d,
            placement: None,
        };
        w.add_instance(&s, &p).unwrap();
        assert!(w.add_instance(&s, &p).is_err());
        let back = LevelWriter::parse(&w.write().unwrap()).unwrap();
        let s = Schema::scan([&back.d]);
        assert_eq!(back.transform(&p.guid).unwrap()[3], [5.0, 6.0, 7.0]);
        assert_eq!(
            back.get_field(&s, &p.guid, "ReferencedObject").unwrap().1,
            Some(FieldValue::Ref("objects/new/new/1".into()))
        );
        assert_eq!(
            back.get_field(&s, &p.guid, "Mode").unwrap().1,
            Some(FieldValue::Int(4))
        );
        assert_eq!(back.d.children(back.d.child(ld, b"Objects").unwrap()).unwrap().len(), 14);
        // no connection copied for a placement
        assert_eq!(back.d.children(back.d.child(ld, b"EventConnections").unwrap()).unwrap().len(), 2);
        // replayed through apply: the same bytes
        let ops = [
            Operation {
                n: 1,
                by: "PC".into(),
                when: "now".into(),
                label: String::new(),
                op: Op::Duplicate {
                    items: vec![crate::ops::Duplicated {
                        source: g("1"),
                        guid: new("1"),
                        transform: crate::rows_at([11.0, 0.0, 0.0]),
                        owned: subs,
                    }],
                },
            },
            Operation {
                n: 2,
                by: "PC".into(),
                when: "now".into(),
                label: String::new(),
                op: Op::Place { items: vec![p] },
            },
        ];
        let base_schema = Schema::scan([&LevelWriter::parse(&base).unwrap().d]);
        let a = apply(&base, &ops, &base_schema).unwrap();
        assert_eq!(a.bytes, w.write().unwrap());
        assert_eq!((a.duplicated, a.placed, a.operations), (4, 1, 2));
    }
}
