//! `bfbc2-bf1943-import/dbxcatalogue.py`'s `build` (round F2b): one pass over every BFBC2 partition the game index
//! names, giving the catalogue's tables in the reference's order - every dict and `Counter` filled in the order the
//! reference's walk fills it, so the tables come out equal key for key and in the same iteration order.
//!
//! The bundles are read a group at a time on all cores and their partitions parsed on all cores (the compact
//! [`DbxTree`]); the walk itself runs in the reference's order: bundles by path, each bundle's names in chunks of 400,
//! each chunk's names sorted. The elements the catalogue keeps (class defaults, a second instance, array item
//! prototypes) are named by (partition, element index): [`Built::kept`] holds those partitions' bytes so the caller
//! builds them (the bindings make the Python elements, sharing an element between a default and a prototype as the
//! reference's one tree does).
//!
//! Refused ([`None`] from [`build`]), so that the reference runs and records its own messages: a chunk that cannot be
//! read, a partition whose tree the reference would trip over (no root, a root that is not a container) or whose
//! numbers it reads otherwise (a count or size past 64 bits).

#![allow(clippy::many_single_char_names)] // the reference's names (d, e, c, p, t), kept to read side by side

use std::collections::HashMap;

use bc2_formats::dbx::{self, CONTAINER, DbxTree, NUMBERS, Node, TEXT};
use rayon::prelude::*;

use crate::pyfmt::OrderedMap;

/// Instances of a type walked for shapes / texts (`PER_TYPE`).
pub const PER_TYPE: u32 = 200;
/// Distinct text values kept per path (`TEXTS_CAP`).
pub const TEXTS_CAP: usize = 400;
/// The names read from a bundle at once (the reference's chunks).
pub const CHUNK: usize = 400;

/// A (type, field path) key.
pub type Key = (String, String);
/// A `Counter` in insertion order.
pub type Counter = OrderedMap<String, u64>;
/// An element the catalogue keeps: (partition number in [`Built::names`], element index in its tree).
pub type ElemRef = (usize, usize);

/// What `build` gives.
#[derive(Debug, Default)]
pub struct Built {
    /// `partitions`.
    pub partitions: usize,
    /// `classes` in first-seen order.
    pub classes: Vec<String>,
    /// `order`.
    pub order: OrderedMap<String, Vec<String>>,
    /// `shapes`.
    pub shapes: OrderedMap<Key, Counter>,
    /// `texts`.
    pub texts: OrderedMap<Key, Counter>,
    /// `numeric` (keys `whole` / `float`).
    pub numeric: OrderedMap<Key, Counter>,
    /// `protos`.
    pub protos: OrderedMap<Key, ElemRef>,
    /// `guids`: one partition or several (the reference's str / tuple).
    pub guids: OrderedMap<String, Vec<String>>,
    /// `primary`.
    pub primary: OrderedMap<String, String>,
    /// `defaults`.
    pub defaults: OrderedMap<String, (String, ElemRef)>,
    /// `others`.
    pub others: OrderedMap<String, (String, ElemRef)>,
    /// `problems` (partitions not parsed), the reference's messages.
    pub problems: Vec<String>,
    /// The partition names in walk order (lowered, `.dbx` dropped), for [`ElemRef`].
    pub names: Vec<String>,
    /// The bytes of the partitions whose elements are kept, by partition number.
    pub kept: HashMap<usize, Vec<u8>>,
}

/// `str.lower()` (ASCII fast path; Unicode's default mapping otherwise, as Python's).
fn lower(s: &str) -> String {
    if s.is_ascii() {
        s.to_ascii_lowercase()
    } else {
        s.to_lowercase()
    }
}

/// `exe_identifiers(path)`'s scan of the executable's bytes: every `\0` followed by an identifier of 2 to 81
/// characters and a `\0`, in file order (the caller makes the set).
#[must_use]
pub fn exe_identifiers(d: &[u8]) -> Vec<String> {
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_';
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < d.len() {
        if d.get(i) != Some(&0) {
            i += 1;
            continue;
        }
        let s = i + 1;
        let first = d.get(s).copied();
        if !first.is_some_and(|c| c.is_ascii_alphabetic() || c == b'_') {
            i += 1;
            continue;
        }
        let mut e = s + 1;
        while e < d.len() && d.get(e).copied().is_some_and(ident) {
            e += 1;
        }
        let n = e - s;
        if (2..=81).contains(&n) && d.get(e) == Some(&0) {
            out.push(String::from_utf8_lossy(d.get(s..e).unwrap_or_default()).into_owned());
            i = e; // the match ends before the closing NUL, which may open the next one
        } else {
            i += 1;
        }
    }
    out
}

/// One element's shape (`shape(e)`), or `None` where the reference reads its numbers otherwise.
fn shape(n: &Node<'_, '_>) -> Option<String> {
    if n.kind() == CONTAINER {
        return Some(format!("{}-container", n.tag()));
    }
    if n.attr("ref").is_some() {
        return Some("ref".to_owned());
    }
    if n.kind() == TEXT {
        return Some(format!("{}-text", n.tag()));
    }
    if n.kind() == NUMBERS {
        let (size, vals) = dbx::numbers(n.raw()).ok()?;
        if size == u64::MAX {
            return None;
        }
        return Some(format!("num{size}x{}", vals.len()));
    }
    Some(format!("k{}", n.kind()))
}

/// `looks_whole(raw)`.
fn looks_whole(raw: &[u8]) -> Option<bool> {
    if raw.is_empty() {
        return None;
    }
    let mut v: i64 = if raw.first().is_some_and(|&b| b & 0x80 != 0) {
        -1
    } else {
        0
    };
    for &b in raw {
        v = (v << 8) | i64::from(b);
    }
    if v == 0 {
        return None;
    }
    Some(matches!(v >> 24, 0 | -1))
}

struct Walker<'b> {
    out: &'b mut Built,
    seen: HashMap<String, u32>,
    classes: std::collections::HashSet<String>,
    keep: std::collections::HashSet<usize>,
}

impl Walker<'_> {
    /// `add_partition(name, f, seen)`; `None`: refused.
    fn add(&mut self, pi: usize, name: &str, tree: &DbxTree<'_>) -> Option<()> {
        let part = tree.roots().next()?;
        if !part.is_container() {
            return None;
        }
        self.out.partitions += 1;
        if let Some(prim) = part.attr("primaryInstance")
            && !prim.is_empty()
        {
            self.out.primary.insert(name.to_owned(), lower(prim));
        }
        for inst in part.children().filter(|c| c.tag() == "instance") {
            let g = lower(inst.attr("guid").unwrap_or(""));
            match self.out.guids.get_mut(&g) {
                None => self.out.guids.insert(g, vec![name.to_owned()]),
                Some(w) => {
                    if !w.iter().any(|x| x == name) {
                        w.push(name.to_owned());
                    }
                }
            }
            let t = inst.attr("type").unwrap_or("").to_owned();
            if self.classes.insert(t.clone()) {
                self.out.classes.push(t.clone());
            }
            let seen = self.seen.entry(t.clone()).or_insert(0);
            if *seen >= PER_TYPE {
                continue;
            }
            *seen += 1;
            if !self.out.order.contains_key(&t) {
                let names: Vec<String> = inst
                    .children()
                    .map(|c| c.attr("name").unwrap_or("").to_owned())
                    .collect();
                self.out.order.insert(t.clone(), names);
                self.out
                    .defaults
                    .insert(t.clone(), (name.to_owned(), (pi, inst.index())));
                self.keep.insert(pi);
            } else if !self.out.others.contains_key(&t) && self.out.defaults.get(&t).is_some_and(|d| d.0 != name) {
                self.out.others.insert(t.clone(), (name.to_owned(), (pi, inst.index())));
                self.keep.insert(pi);
            }
            self.walk(pi, &inst, &t, "")?;
        }
        Some(())
    }

    fn walk(&mut self, pi: usize, e: &Node<'_, '_>, t: &str, path: &str) -> Option<()> {
        for c in e.children() {
            let p = match c.attr("name") {
                Some(n) => format!("{path}.{n}"),
                None => format!("{path}[]"),
            };
            let s = shape(&c)?;
            let key = (t.to_owned(), p);
            let is_text = s == "field-text" || s == "complex-text";
            let is_num = s.starts_with("num4x");
            *self.out.shapes.entry_default(key.clone()).entry_default(s) += 1;
            if is_text {
                let tc = self.out.texts.entry_default(key.clone());
                if tc.len() < TEXTS_CAP {
                    *tc.entry_default(c.text().to_owned()) += 1;
                }
            } else if is_num {
                let (_, vals) = dbx::numbers(c.raw()).ok()?;
                for v in vals {
                    if let Some(w) = looks_whole(v) {
                        let k = if w { "whole" } else { "float" };
                        *self.out.numeric.entry_default(key.clone()).entry_default(k.to_owned()) += 1;
                    }
                }
            }
            if c.tag() == "array" && c.kind() == CONTAINER && c.is_container() {
                let pk = (t.to_owned(), format!("{}[]", key.1));
                if let Some(first) = c.children().next()
                    && !self.out.protos.contains_key(&pk)
                {
                    self.out.protos.insert(pk, (pi, first.index()));
                    self.keep.insert(pi);
                }
            }
            if c.kind() == CONTAINER {
                self.walk(pi, &c, t, &key.1)?;
            }
        }
        Some(())
    }
}

/// A bundle and the entry names the index gives it, in the index's order.
pub type Job = (String, Vec<String>);

/// `build`'s pass over `bundles` (each with the names `ix.where` gives it, in its order); `None`: refused (see the
/// module's text). `group`: bundles read at once.
#[must_use]
pub fn build(bundles: &[Job], group: usize) -> Option<Built> {
    let mut sorted: Vec<&Job> = bundles.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    // the chunks in the reference's order: (bundle, its names deduplicated and sorted)
    let mut chunks: Vec<(&str, Vec<String>)> = Vec::new();
    for (b, ns) in sorted {
        for c in ns.chunks(CHUNK) {
            let mut keys: Vec<String> = Vec::with_capacity(c.len());
            for n in c {
                if !keys.contains(n) {
                    keys.push(n.clone());
                }
            }
            keys.sort();
            chunks.push((b.as_str(), keys));
        }
    }
    let mut out = Built::default();
    let mut walker_seen: HashMap<String, u32> = HashMap::new();
    let mut walker_classes = std::collections::HashSet::new();
    for grp in chunks.chunks(group.max(1)) {
        let jobs: Vec<(&str, Vec<String>)> = grp
            .iter()
            .map(|(b, ks)| (*b, ks.iter().map(|k| k.to_lowercase()).collect()))
            .collect();
        let read = bc2_gamefs::bundles::read_many(&jobs);
        let mut datas: Vec<(&str, &str, Vec<u8>)> = Vec::new();
        for (r, (b, ks)) in read.into_iter().zip(grp) {
            let data = r.ok()?;
            for (k, d) in ks.iter().zip(data) {
                datas.push((b, k.as_str(), d));
            }
        }
        let parsed: Vec<Option<Result<DbxTree<'_>, String>>> = datas
            .par_iter()
            .map(|(_, _, d)| {
                if d.is_empty() {
                    return None; // a *deleted* marker: nothing to read
                }
                if !dbx::is_binary(d) {
                    return Some(Err("not a binary .dbx file".to_owned()));
                }
                Some(dbx::parse_tree(d).map_err(|e| e.to_string()))
            })
            .collect();
        let mut w = Walker {
            out: &mut out,
            seen: std::mem::take(&mut walker_seen),
            classes: std::mem::take(&mut walker_classes),
            keep: std::collections::HashSet::new(),
        };
        let mut keep_data: Vec<(usize, usize)> = Vec::new();
        for (i, ((b, n, _), p)) in datas.iter().zip(&parsed).enumerate() {
            match p {
                None => {}
                Some(Err(msg)) => w.out.problems.push(format!("{b}: {n} not parsed (ValueError: {msg})")),
                Some(Ok(tree)) => {
                    let lowered = if n.to_lowercase().ends_with(".dbx") {
                        n.get(..n.len() - 4).unwrap_or(n)
                    } else {
                        n
                    };
                    let name = lower(lowered);
                    let pi = w.out.names.len();
                    w.out.names.push(name.clone());
                    w.add(pi, &name, tree)?;
                    if w.keep.remove(&pi) {
                        keep_data.push((pi, i));
                    }
                }
            }
        }
        walker_seen = std::mem::take(&mut w.seen);
        walker_classes = std::mem::take(&mut w.classes);
        drop(parsed);
        for (pi, i) in keep_data {
            if let Some((_, _, d)) = datas.get_mut(i) {
                out.kept.insert(pi, std::mem::take(d));
            }
        }
    }
    // the bytes of partitions none of whose elements stayed kept (a later default replaced none: defaults and
    // prototypes are set once, so every kept partition is still named)
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identifiers() {
        let d = b"\0ab\0x\0_c9\0\0z\x01\0abc";
        assert_eq!(exe_identifiers(d), vec!["ab".to_owned(), "_c9".to_owned()]);
        let long = [&[0u8][..], &[b'a'; 82][..], &[0][..]].concat();
        assert_eq!(exe_identifiers(&long), Vec::<String>::new());
    }

    #[test]
    fn whole_numbers() {
        assert_eq!(looks_whole(&[0, 0, 0, 0]), None);
        assert_eq!(looks_whole(&[0, 0, 0, 5]), Some(true));
        assert_eq!(looks_whole(&[0xFF, 0xFF, 0xFF, 0xFE]), Some(true));
        assert_eq!(looks_whole(&[0x3F, 0x80, 0, 0]), Some(false));
        assert_eq!(looks_whole(&[0x80]), Some(true));
    }
}
