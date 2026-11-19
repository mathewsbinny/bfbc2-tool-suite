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

use crate::ops::{FieldValue, Op, Operation, Rows};
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
    #[must_use]
    pub fn connection_arrays(&self) -> Vec<NodeId> {
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

    /// [`LevelWriter::delete`] of many instances in one pass over the partition (an export removes thousands; one
    /// walk per instance is quadratic): the same result as deleting them one by one, in any order. `guids` upper
    /// case; guids not in the partition are skipped. The level's `LevelData` is never removed.
    ///
    /// # Errors
    /// The DBX writer refuses an element (none of these edits makes one it refuses).
    pub fn delete_many(&mut self, guids: &std::collections::HashSet<String>) -> Result<Deleted> {
        let mut out = Deleted::default();
        if guids.is_empty() {
            return Ok(out);
        }
        let hit = |r: &[u8]| -> bool {
            let tail = r.rsplit(|&b| b == b'/').next().unwrap_or(r);
            guids.contains(&String::from_utf8_lossy(tail).to_ascii_uppercase())
        };
        for a in self.connection_arrays() {
            let kids = self.d.children(a).unwrap_or_default().to_vec();
            let keep: Vec<NodeId> = kids
                .iter()
                .copied()
                .filter(|&c| {
                    ![&b"Source"[..], b"Target"]
                        .iter()
                        .any(|f| self.d.child(c, f).and_then(|x| self.d.attr(x, b"ref")).is_some_and(hit))
                })
                .collect();
            out.connections += kids.len() - keep.len();
            *self.d.children_mut(a)? = keep;
        }
        let level = self.level_data();
        let mut roots = self.d.roots().to_vec();
        out.instances = self.drop_many_in(&mut roots, &hit, level, &mut out.references)?;
        *self.d.roots_mut() = roots;
        Ok(out)
    }

    fn drop_many_in(
        &mut self,
        list: &mut Vec<NodeId>,
        hit: &dyn Fn(&[u8]) -> bool,
        level: Option<NodeId>,
        references: &mut usize,
    ) -> Result<usize> {
        let mut removed = 0;
        let mut i = list.len();
        while i > 0 {
            i -= 1;
            let Some(&e) = list.get(i) else { continue };
            if self.d.tag(e) == b"instance" && Some(e) != level && self.d.attr(e, b"guid").is_some_and(hit) {
                list.remove(i);
                removed += 1;
                continue;
            }
            if self.d.attr(e, b"ref").is_some_and(hit) {
                *references += 1;
                if self.d.tag(e) == b"item" {
                    list.remove(i);
                    continue;
                }
                self.d.set_attr(e, b"ref", b"null")?;
            }
            if let Some(kids) = self.d.children(e) {
                let mut kids = kids.to_vec();
                removed += self.drop_many_in(&mut kids, hit, level, references)?;
                *self.d.children_mut(e)? = kids;
            }
        }
        Ok(removed)
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
                w.duplicate(&d.source, &d.guid, Some(&d.transform)).map_err(fail)?;
                out.duplicated += 1;
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
}
