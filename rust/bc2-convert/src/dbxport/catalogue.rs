//! `dbxcatalogue.Catalogue`'s tables as dbxport asks them (BFBC2's classes, field order, shapes, enum fields,
//! number kinds, item prototypes, guids, primary instances), kept here once per catalogue. The elements it holds
//! (prototypes, class defaults) live in an [`Arena`] beside it.

use std::collections::{HashMap, HashSet};

use super::arena::Id;
use super::py;

/// Per type, per field path.
pub type ByPath<V> = HashMap<String, HashMap<String, V>>;

/// The catalogue's tables.
#[derive(Debug, Default)]
pub struct Catalogue {
    /// Every instance type BFBC2's partitions hold.
    pub classes: HashSet<String>,
    /// Every identifier of the executable's strings.
    pub exe_names: HashSet<String>,
    /// Each type's field names in DICE's order.
    pub order: HashMap<String, Vec<String>>,
    /// The shapes DICE writes per (type, path) (the Counter's keys).
    pub shapes: ByPath<Vec<String>>,
    /// (type, path)s whose `enum_names` is not `None`.
    pub enums: HashMap<String, HashSet<String>>,
    /// Per (type, path): (the Counter is not empty, its `whole` count, its `float` count).
    pub numeric: ByPath<(bool, i64, i64)>,
    /// An item prototype per (type, array path + `[]`): a node of the elements' arena.
    pub protos: ByPath<Id>,
    /// guid -> the partitions holding it (indices into `partitions`).
    pub guids: HashMap<String, Vec<u32>>,
    /// The partition names `guids` points into.
    pub partitions: Vec<String>,
    /// partition -> its primary instance's guid.
    pub primary: HashMap<String, String>,
}

impl Catalogue {
    /// `has_class(t)`.
    #[must_use]
    pub fn has_class(&self, t: &str) -> bool {
        self.classes.contains(t) || (!self.exe_names.is_empty() && self.exe_names.contains(py::rlast(t, '.')))
    }

    /// `where(guid)`.
    pub fn where_(&self, guid: &str) -> impl Iterator<Item = &str> {
        self.guids
            .get(&py::lower(guid))
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(|&i| self.partitions.get(i as usize).map(String::as_str))
    }

    /// `exists(partition, guid)`.
    #[must_use]
    pub fn exists(&self, partition: &str, guid: &str) -> bool {
        let p = py::lower(partition);
        self.where_(guid).any(|w| w == p)
    }

    /// `has_partition(partition)`.
    #[must_use]
    pub fn has_partition(&self, partition: &str) -> bool {
        self.primary.contains_key(&py::lower(partition))
    }

    /// `enum_names(t, path) is not None`.
    #[must_use]
    pub fn is_enum(&self, t: &str, path: &str) -> bool {
        self.enums.get(t).is_some_and(|m| m.contains(path))
    }

    /// `valid_enum(value)`.
    #[must_use]
    pub fn valid_enum(&self, v: &str) -> bool {
        self.exe_names.is_empty() || self.exe_names.contains(v)
    }

    /// `is_whole(t, path)`.
    #[must_use]
    pub fn is_whole(&self, t: &str, path: &str) -> Option<bool> {
        let &(nonempty, whole, float) = self.numeric.get(t)?.get(path)?;
        if !nonempty {
            return None;
        }
        match (whole != 0, float != 0) {
            (true, false) => Some(true),
            (false, true) => Some(false),
            _ => None,
        }
    }

    /// `bool(numeric.get((t, path)))`.
    #[must_use]
    pub fn numeric_seen(&self, t: &str, path: &str) -> bool {
        self.numeric.get(t).and_then(|m| m.get(path)).is_some_and(|x| x.0)
    }

    /// `shapes.get((t, path))` (its keys; empty for none).
    #[must_use]
    pub fn shapes_of(&self, t: &str, path: &str) -> &[String] {
        self.shapes
            .get(t)
            .and_then(|m| m.get(path))
            .map_or(&[][..], Vec::as_slice)
    }

    /// `protos.get((t, path))`.
    #[must_use]
    pub fn proto(&self, t: &str, path: &str) -> Option<Id> {
        self.protos.get(t).and_then(|m| m.get(path)).copied()
    }

    /// Sets `enums` from each (type, path)'s text values in the Counter's order (`enum_names`' rule).
    pub fn set_texts(&mut self, t: &str, path: &str, values: &[String]) {
        if values.is_empty() || self.exe_names.is_empty() {
            return;
        }
        let names: Vec<&String> = values.iter().filter(|v| !v.is_empty()).collect();
        let Some(first) = names.first() else { return };
        if !names
            .iter()
            .all(|v| py::is_identifier(v) && self.exe_names.contains(v.as_str()))
        {
            return;
        }
        if *first == "true" || *first == "false" {
            return;
        }
        self.enums.entry(t.to_owned()).or_default().insert(path.to_owned());
    }
}
