//! The readings behind `validate.py`'s checks of a level partition against its stock original, on the suite's
//! reader ([`bc2_formats::dbx::DbxTree`], the port of `bfbc2core.dbx`: the tree `validate` reads with
//! `coredbx.parse`). Each gives the facts the check's message is made of; `validate.py` decides and words the
//! result. Where the check would raise (a missing field, a reference attribute missing where it is read, a value the
//! reader cannot format) the reading refuses and the check runs in Python.

use std::collections::{HashMap, HashSet};

use bc2_formats::dbx::{DbxTree, Node};

use crate::{R, Refuse};

/// `(file, {upper-case guid: instance})` as `validate._core_instances` reads it: the instances in order, and per
/// GUID (in the order first met) the last instance with it, as a Python dict comprehension keeps it.
struct Instances<'t, 'a> {
    all: Vec<Node<'t, 'a>>,
    order: Vec<String>,
    by: HashMap<String, Node<'t, 'a>>,
}

fn instances<'t, 'a>(t: &'t DbxTree<'a>) -> R<Instances<'t, 'a>> {
    let p = t.partition().ok_or(Refuse("no partition"))?;
    if !p.is_container() {
        return Err(Refuse("a partition that is not a container"));
    }
    let all: Vec<Node> = t.instances().collect();
    let mut order = Vec::new();
    let mut by = HashMap::with_capacity(all.len());
    for &e in &all {
        let g = e.attr("guid").unwrap_or_default().to_uppercase();
        if by.insert(g.clone(), e).is_none() {
            order.push(g);
        }
    }
    Ok(Instances { all, order, by })
}

/// `validate._refs(e)`: every `(element, ref)` below `e` whose `ref` is set and not `null`, depth first.
fn refs<'t>(e: Node<'t, '_>, out: &mut Vec<&'t str>) {
    for c in e.children() {
        if let Some(r) = c.attr("ref")
            && r != "null"
        {
            out.push(r);
        }
        refs(c, out);
    }
}

fn refs_of<'t>(e: Node<'t, '_>) -> Vec<&'t str> {
    let mut out = Vec::new();
    refs(e, &mut out);
    out
}

fn is_level(e: Node<'_, '_>) -> bool {
    e.attr("type").unwrap_or_default().ends_with(".LevelData")
}

fn parse(data: &[u8]) -> R<DbxTree<'_>> {
    bc2_formats::dbx::parse_tree(data).map_err(|_| Refuse("not read"))
}

/// The facts of `check_structure`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Structure {
    /// GUIDs used by more than one instance, in the order first met.
    pub twice: Vec<String>,
    /// `(guid, ref)` of each local reference that resolves to no instance.
    pub local_unresolved: Vec<(String, String)>,
    /// The number of entities listed more than once in `LevelData.Objects`.
    pub objects_twice: usize,
    /// The number of new instances (GUIDs the stock level lacks).
    pub new: usize,
    /// The number of new instances not in `Objects`.
    pub unlisted: usize,
    /// `(side, ref)` of each event source or target that resolves to nothing.
    pub event_unresolved: Vec<(String, String)>,
    /// The number of event connections.
    pub events: usize,
    /// `HasAI`'s value text.
    pub has_ai: String,
    /// `PathfindingTypes`' references.
    pub pathfinding: Vec<String>,
}

/// `check_structure(orig, authored, c)`'s reading.
///
/// # Errors
/// Where the check raises (module documentation).
#[expect(clippy::many_single_char_names, reason = "validate's names: f, o, a, s")]
pub fn structure(orig: &[u8], authored: &[u8]) -> R<Structure> {
    let ft = parse(authored)?;
    let ot = parse(orig)?;
    let a = instances(&ft)?;
    let o = instances(&ot)?;
    let mut s = Structure::default();
    let mut count: HashMap<String, usize> = HashMap::new();
    let mut first: Vec<String> = Vec::new();
    for e in &a.all {
        let g = e.attr("guid").unwrap_or_default().to_uppercase();
        let n = count.entry(g.clone()).or_insert(0);
        if *n == 0 {
            first.push(g);
        }
        *n += 1;
    }
    s.twice = first
        .into_iter()
        .filter(|g| count.get(g).copied().unwrap_or(0) > 1)
        .collect();
    for g in &a.order {
        let Some(&e) = a.by.get(g) else { continue };
        for r in refs_of(e) {
            if !r.contains('/') && !a.by.contains_key(&r.to_uppercase()) {
                s.local_unresolved.push((g.clone(), r.to_owned()));
            }
        }
    }
    let lvl = a
        .all
        .iter()
        .copied()
        .find(|&e| is_level(e))
        .ok_or(Refuse("no LevelData"))?;
    let objects = lvl.child("Objects").ok_or(Refuse("no Objects"))?;
    let objs: Vec<String> = objects
        .children()
        .map(|i| {
            i.attr("ref")
                .map(str::to_uppercase)
                .ok_or(Refuse("an item without a ref"))
        })
        .collect::<R<_>>()?;
    let mut oc: HashMap<&str, usize> = HashMap::new();
    for x in &objs {
        *oc.entry(x).or_insert(0) += 1;
    }
    s.objects_twice = oc.values().filter(|&&v| v > 1).count();
    let listed: HashSet<&str> = objs.iter().map(String::as_str).collect();
    let new: Vec<&String> = a.order.iter().filter(|g| !o.by.contains_key(*g)).collect();
    s.new = new.len();
    s.unlisted = new.iter().filter(|g| !listed.contains(g.as_str())).count();
    let ec = lvl.child("EventConnections").ok_or(Refuse("no EventConnections"))?;
    for cx in ec.children() {
        for side in ["Source", "Target"] {
            let r = cx
                .child(side)
                .ok_or(Refuse("an event without a side"))?
                .attr("ref")
                .ok_or(Refuse("an event side without a ref"))?;
            if !r.contains('/') && !a.by.contains_key(&r.to_uppercase()) {
                s.event_unresolved.push((side.to_owned(), r.to_owned()));
            }
        }
        s.events += 1;
    }
    s.has_ai = lvl
        .child("HasAI")
        .ok_or(Refuse("no HasAI"))?
        .value()
        .map_err(|_| Refuse("HasAI's value"))?;
    s.pathfinding = lvl
        .child("PathfindingTypes")
        .ok_or(Refuse("no PathfindingTypes"))?
        .children()
        .map(|i| i.attr("ref").map(str::to_owned).ok_or(Refuse("an item without a ref")))
        .collect::<R<_>>()?;
    Ok(s)
}

/// A leaf as `check_changes`' `shown()` gives it: `(kind, value text, attributes)`.
pub type Shown = (u8, String, Vec<(String, String)>);

/// One stock instance's changes in the authored level.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Changes {
    /// The upper-case GUID.
    pub guid: String,
    /// Not in the authored level.
    pub removed: bool,
    /// `LevelData.Objects` / `EventConnections`: an existing item changed or moved (`LevelData` only).
    pub arrays_changed: Vec<String>,
    /// `(path, stock leaf, authored leaf)` of each leaf that differs (`None`: not there).
    pub leaves: Vec<(String, Option<Shown>, Option<Shown>)>,
}

fn flat<'t, 'a>(e: Node<'t, 'a>, path: &str, out: &mut Vec<(String, Node<'t, 'a>)>) {
    for (k, ch) in e.children().enumerate() {
        let name = match ch.attr("name") {
            Some(n) if !n.is_empty() => n,
            _ => ch.tag(),
        };
        let p = format!("{path}/{name}[{k}]");
        if ch.is_container() {
            flat(ch, &p, out);
        } else {
            out.push((p, ch));
        }
    }
}

fn shown(e: Node<'_, '_>) -> R<Shown> {
    let v = e.value().map_err(|_| Refuse("a value the reader cannot format"))?;
    Ok((
        e.kind(),
        v,
        e.attributes().map(|(k, v)| (k.to_owned(), v.to_owned())).collect(),
    ))
}

/// `same(x, y)`: equal kind, text, raw bytes and attributes, else equal `shown()`.
fn same(x: Node<'_, '_>, y: Node<'_, '_>) -> R<bool> {
    if x.kind() == y.kind() && x.text() == y.text() && x.raw() == y.raw() && x.attributes().eq(y.attributes()) {
        return Ok(true);
    }
    Ok(shown(x)? == shown(y)?)
}

/// The two elements are equal as the reader's `Element`s (`repr` equal): tag, kind, attributes, text, raw bytes and
/// children.
fn equal(x: Node<'_, '_>, y: Node<'_, '_>) -> bool {
    x.tag() == y.tag()
        && x.kind() == y.kind()
        && x.attributes().eq(y.attributes())
        && x.text() == y.text()
        && x.raw() == y.raw()
        && x.is_container() == y.is_container()
        && x.children().count() == y.children().count()
        && x.children().zip(y.children()).all(|(a, b)| equal(a, b))
}

const LEVEL_ARRAYS: [&str; 3] = ["/Objects[", "/EventConnections[", "/PathfindingTypes["];

/// `check_changes`' reading: per stock instance (in the order of its GUIDs), what differs in the authored level.
/// Only instances with a change are listed.
///
/// # Errors
/// Where the check raises (module documentation).
#[expect(clippy::many_single_char_names, reason = "validate's names")]
pub fn changes(orig: &[u8], authored: &[u8]) -> R<Vec<Changes>> {
    let ot = parse(orig)?;
    let at = parse(authored)?;
    let o = instances(&ot)?;
    let a = instances(&at)?;
    let mut out = Vec::new();
    for g in &o.order {
        let Some(&e) = o.by.get(g) else { continue };
        let mut ch = Changes {
            guid: g.clone(),
            ..Changes::default()
        };
        let Some(&x) = a.by.get(g) else {
            ch.removed = true;
            out.push(ch);
            continue;
        };
        let mut fe = Vec::new();
        let mut fa = Vec::new();
        flat(e, "", &mut fe);
        flat(x, "", &mut fa);
        if is_level(e) {
            fe.retain(|(k, _)| !LEVEL_ARRAYS.iter().any(|p| k.starts_with(p)));
            fa.retain(|(k, _)| !LEVEL_ARRAYS.iter().any(|p| k.starts_with(p)));
            for arr in ["Objects", "EventConnections"] {
                let oi: Vec<Node> = e.child(arr).ok_or(Refuse("no array"))?.children().collect();
                let ai: Vec<Node> = x
                    .child(arr)
                    .ok_or(Refuse("no array"))?
                    .children()
                    .take(oi.len())
                    .collect();
                if oi.len() != ai.len() || oi.iter().zip(&ai).any(|(p, q)| !equal(*p, *q)) {
                    ch.arrays_changed.push(arr.to_owned());
                }
            }
        }
        let fa_by: HashMap<&str, Node> = fa.iter().map(|(k, n)| (k.as_str(), *n)).collect();
        let fe_keys: HashSet<&str> = fe.iter().map(|(k, _)| k.as_str()).collect();
        for (k, n) in &fe {
            match fa_by.get(k.as_str()) {
                Some(&m) => {
                    if !same(*n, m)? {
                        ch.leaves.push((k.clone(), Some(shown(*n)?), Some(shown(m)?)));
                    }
                }
                None => ch.leaves.push((k.clone(), Some(shown(*n)?), None)),
            }
        }
        for (k, m) in &fa {
            if !fe_keys.contains(k.as_str()) {
                ch.leaves.push((k.clone(), None, Some(shown(*m)?)));
            }
        }
        if !ch.arrays_changed.is_empty() || !ch.leaves.is_empty() {
            out.push(ch);
        }
    }
    Ok(out)
}

/// `check_external`'s reading: the references into other partitions the authored level adds (from its new
/// instances and its `LevelData`), lower case, less those the stock level has, sorted.
///
/// # Errors
/// A file the reader refuses.
pub fn external(orig: &[u8], authored: &[u8]) -> R<Vec<String>> {
    let ot = parse(orig)?;
    let at = parse(authored)?;
    let o = instances(&ot)?;
    let a = instances(&at)?;
    let mut wanted: HashSet<String> = HashSet::new();
    for g in &a.order {
        let Some(&e) = a.by.get(g) else { continue };
        if o.by.contains_key(g) && !is_level(e) {
            continue;
        }
        for r in refs_of(e) {
            if r.contains('/') {
                wanted.insert(r.to_lowercase());
            }
        }
    }
    for g in &o.order {
        let Some(&e) = o.by.get(g) else { continue };
        for r in refs_of(e) {
            if r.contains('/') {
                wanted.remove(&r.to_lowercase());
            }
        }
    }
    let mut w: Vec<String> = wanted.into_iter().collect();
    w.sort();
    Ok(w)
}

/// An instance's `(type, Enabled value text)`, or `None` for no instance of the GUID.
pub type InstanceFact = Option<(Option<String>, Option<String>)>;

/// `check_stages`' reading of the authored level: for each GUID asked (upper case) that names an instance, its
/// `type` and its `Enabled` value text (`None`: no `Enabled`, or a value the reader cannot format).
///
/// # Errors
/// A file the reader refuses.
pub fn instance_facts(authored: &[u8], guids: &[String]) -> R<Vec<InstanceFact>> {
    let at = parse(authored)?;
    let a = instances(&at)?;
    Ok(guids
        .iter()
        .map(|g| {
            a.by.get(g).map(|e| {
                (
                    e.attr("type").map(str::to_owned),
                    e.child("Enabled").and_then(|c| c.value().ok()),
                )
            })
        })
        .collect())
}
