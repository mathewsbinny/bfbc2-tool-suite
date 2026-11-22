//! LevelDiff (`bfbc2-diff-workbench/level-diff/LevelDiff.cs`, retired in round EX1) in process: two binary `.dbx`
//! files compared instance by instance - what was added, removed or changed, field by field - with LevelDiff's text,
//! JSON and exit codes byte for byte (`bfbc2.exe leveldiff`, proven on recorded real level pairs). The map editor's
//! Diff panel reads the same [`LevelDiff`] ([`diff`]).
//!
//! Instances are matched by GUID (any case); `--match-objects` also pairs placed objects (`ReferenceObjectData`)
//! whose GUIDs differ, by blueprint and position to 0.1 m. Each changed instance lists its fields as
//! `path: old  ->  new`; lists of references are compared as sets (`-` only in A, `+` only in B), and a list with the
//! same entries in another order is reported as such.
//!
//! Where LevelDiff's C# has a quirk, the port keeps it: a list reorder is reported only while the instance has no
//! list entry added or removed so far; a field path seen twice keeps its first place with the last value; texts are
//! cut at 120 UTF-16 units.

#![allow(
    clippy::many_single_char_names,
    reason = "LevelDiff's names: a / b (the files), d (a difference), o (the options), i / j (instances)"
)]

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use super::check::PlacedObject;
use super::export::value_text;
use super::fault::{Fault, Result};
use super::file::{EditDbx, NUMBERS, NodeId, Table, be_u32, kids, partition_children, read};
use super::net;
use crate::dbx::net_g;

/// The usage line (standard error, exit code 2).
pub const USAGE: &str = "usage: LevelDiff <a.dbx> <b.dbx> [--match-objects] [--type <text>] [--ignore <field>] [--summary] \
                         [--all] [--json <file>]";

/// The command line's switches.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Options {
    /// Also pair placed objects whose GUIDs differ, by blueprint and position.
    pub match_objects: bool,
    /// Counts only.
    pub summary: bool,
    /// List everything (default: the first 40 per section).
    pub all: bool,
    /// Only instances whose type contains this (any case).
    pub type_filter: Option<String>,
    /// Fields left out (by name, exact case).
    pub ignore: HashSet<String>,
    /// Also write the result as JSON here.
    pub json: Option<String>,
}

/// One field that differs: its path, its value in A and in B (`None`: not there).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    /// The field's path (`Transform`, `Objects`, `Description.Name`, `(type)`, `(id)`).
    pub path: String,
    /// Its value in A.
    pub a: Option<String>,
    /// Its value in B.
    pub b: Option<String>,
}

/// An instance in both files that differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceDiff {
    /// Its GUID in A.
    pub guid: Option<String>,
    /// Its GUID in B (another one when it was paired by position).
    pub guid_b: Option<String>,
    /// Its type, short (`ReferenceObjectData`).
    pub ty: String,
    /// Its id in A.
    pub id: Option<String>,
    /// The fields that differ, in A's order then B's.
    pub changes: Vec<Change>,
    /// Reference list entries only in B (`Objects: <guid>`).
    pub added: Vec<String>,
    /// Reference list entries only in A.
    pub removed: Vec<String>,
    /// Its index among A's partition children.
    pub index_a: usize,
    /// Its index among B's partition children.
    pub index_b: usize,
}

impl InstanceDiff {
    /// Whether its transform changed (the editor's "moved").
    #[must_use]
    pub fn moved(&self) -> bool {
        self.changes.iter().any(|c| c.path == "Transform")
    }
}

/// An instance only in one file.
#[derive(Debug, Clone, PartialEq)]
pub struct Listed {
    /// Its index among the file's partition children.
    pub index: usize,
    /// Its type, short.
    pub ty: String,
    /// Its GUID.
    pub guid: Option<String>,
    /// Its id.
    pub id: Option<String>,
    /// A placed object's blueprint and position.
    pub placed: Option<(String, [f32; 3])>,
}

/// What differs between two files.
#[derive(Debug, Clone, PartialEq)]
pub struct LevelDiff {
    /// The instances compared in A (after `--type`).
    pub count_a: usize,
    /// In B.
    pub count_b: usize,
    /// Instances in both (paired by GUID or position).
    pub same: usize,
    /// Of those, placed objects paired by blueprint and position.
    pub paired_by_position: usize,
    /// Only in A (removed in B).
    pub only_a: Vec<Listed>,
    /// Only in B (added in B).
    pub only_b: Vec<Listed>,
    /// In both, different.
    pub changed: Vec<InstanceDiff>,
}

impl LevelDiff {
    /// Whether the files have no difference (exit code 0).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.only_a.is_empty() && self.only_b.is_empty() && self.changed.is_empty()
    }
}

/// A `Dictionary<string, T>` as .NET enumerates it while nothing is removed: insertion order; a key set again keeps its
/// place.
#[derive(Debug, Default)]
struct Ordered<T> {
    keys: Vec<String>,
    values: Vec<T>,
    at: HashMap<String, usize>,
}

impl<T> Ordered<T> {
    fn slot(&mut self, k: &str, make: impl FnOnce() -> T) -> &mut T {
        let i = if let Some(&i) = self.at.get(k) {
            i
        } else {
            self.keys.push(k.to_owned());
            self.values.push(make());
            self.at.insert(k.to_owned(), self.keys.len() - 1);
            self.keys.len() - 1
        };
        // `i` is a place `values` has: it was pushed with its key
        self.values
            .get_mut(i)
            .unwrap_or_else(|| unreachable!("a key without its value"))
    }

    fn get(&self, k: &str) -> Option<&T> {
        self.at.get(k).and_then(|&i| self.values.get(i))
    }
}

/// `TypeOf`: the type after its last dot.
fn type_of(t: Table<'_>, e: NodeId) -> Result<String> {
    let ty = t.get(e, "type")?.unwrap_or("");
    Ok(ty.rsplit_once('.').map_or(ty, |(_, s)| s).to_owned())
}

fn get(t: Table<'_>, e: NodeId, a: &str) -> Result<Option<String>> {
    Ok(t.get(e, a)?.map(str::to_owned))
}

/// `IndexOf(text, StringComparison.OrdinalIgnoreCase) >= 0`.
fn contains_ignore_case(s: &str, part: &str) -> bool {
    if part.is_empty() {
        return true;
    }
    if s.is_ascii() && part.is_ascii() {
        return s.to_ascii_uppercase().contains(&part.to_ascii_uppercase());
    }
    net::to_upper(s).contains(&net::to_upper(part))
}

/// The instances a side compares: (partition child index, element), `--type` applied.
fn pick(f: &EditDbx, o: &Options) -> Result<Vec<(usize, NodeId)>> {
    let t = Table(f);
    let mut out = Vec::new();
    for (k, &e) in partition_children(f)?.iter().enumerate() {
        if t.tag(e)? != "instance" {
            continue;
        }
        if let Some(ty) = &o.type_filter
            && !contains_ignore_case(t.get(e, "type")?.unwrap_or(""), ty)
        {
            continue;
        }
        out.push((k, e));
    }
    Ok(out)
}

/// `Flatten`: an instance's fields as path -> value, lists of references as path -> entries.
fn flatten(
    t: Table<'_>,
    e: NodeId,
    path: &str,
    o: &Options,
    values: &mut Ordered<String>,
    lists: &mut Ordered<Vec<String>>,
) -> Result<()> {
    let mut seen: HashMap<String, usize> = HashMap::new();
    for &c in t.0.children(e).ok_or_else(Fault::null)? {
        let name = t.get(c, "name")?;
        if let Some(n) = name
            && o.ignore.contains(n)
        {
            continue;
        }
        let tag = t.tag(c)?;
        let key = name.unwrap_or(tag);
        let n = seen.get(key).copied().unwrap_or(0);
        seen.insert(key.to_owned(), n + 1);
        let mut p = if path.is_empty() {
            key.to_owned()
        } else {
            format!("{path}.{key}")
        };
        if n > 0 || (name.is_none() && tag != "item") {
            p = format!("{p}[{n}]");
        }
        if let Some(kids) = t.0.children(c) {
            let mut ref_list = !kids.is_empty();
            for &x in kids {
                if !ref_list {
                    break;
                }
                ref_list = t.tag(x)? == "item" && t.get(x, "ref")?.is_some();
            }
            if ref_list {
                let mut refs = Vec::with_capacity(kids.len());
                for &x in kids {
                    refs.push(t.get(x, "ref")?.unwrap_or("").to_owned());
                }
                lists.slot(&p, Vec::new).extend(refs);
            } else {
                *values.slot(&p, String::new) = format!("({} entries)", kids.len());
                flatten(t, c, &p, o, values, lists)?;
            }
            continue;
        }
        let v = match t.get(c, "ref")? {
            Some(r) => format!("-> {r}"),
            None => value_text(t.0, c)?,
        };
        *values.slot(&p, String::new) = v;
    }
    Ok(())
}

/// `Counts`: each entry's count, keys compared ignoring case (the first spelling kept), in first-seen order; and the
/// count of each key upper case (the lookups of the other list's keys).
fn counts(l: &[String]) -> (Ordered<usize>, HashMap<String, usize>) {
    let mut c = Ordered::default();
    let mut spelling: HashMap<String, String> = HashMap::new();
    for s in l {
        let up = net::to_upper(s);
        let k = spelling.entry(up).or_insert_with(|| s.clone()).clone();
        *c.slot(&k, || 0) += 1;
    }
    let by_upper = spelling
        .into_iter()
        .map(|(up, k)| {
            let n = c.get(&k).copied().unwrap_or(0);
            (up, n)
        })
        .collect();
    (c, by_upper)
}

fn count_of(c: &HashMap<String, usize>, k: &str) -> usize {
    c.get(&net::to_upper(k)).copied().unwrap_or(0)
}

/// `Union`: A's keys then B's new ones.
fn union<'a>(a: &'a [String], b: &'a [String]) -> Vec<&'a str> {
    let mut seen = HashSet::new();
    a.iter()
        .chain(b)
        .filter(|k| seen.insert(k.as_str()))
        .map(String::as_str)
        .collect()
}

fn compare(a: &EditDbx, ia: (usize, NodeId), b: &EditDbx, ib: (usize, NodeId), o: &Options) -> Result<InstanceDiff> {
    let (ta, tb) = (Table(a), Table(b));
    let mut d = InstanceDiff {
        guid: get(ta, ia.1, "guid")?,
        guid_b: get(tb, ib.1, "guid")?,
        ty: type_of(ta, ia.1)?,
        id: get(ta, ia.1, "id")?,
        changes: Vec::new(),
        added: Vec::new(),
        removed: Vec::new(),
        index_a: ia.0,
        index_b: ib.0,
    };
    if type_of(tb, ib.1)? != d.ty {
        d.changes.push(Change {
            path: "(type)".into(),
            a: get(ta, ia.1, "type")?,
            b: get(tb, ib.1, "type")?,
        });
    }
    let id_b = get(tb, ib.1, "id")?;
    if d.id != id_b {
        d.changes.push(Change {
            path: "(id)".into(),
            a: d.id.clone(),
            b: id_b,
        });
    }
    let (mut va, mut vb, mut la, mut lb) = (
        Ordered::default(),
        Ordered::default(),
        Ordered::default(),
        Ordered::default(),
    );
    flatten(ta, ia.1, "", o, &mut va, &mut la)?;
    flatten(tb, ib.1, "", o, &mut vb, &mut lb)?;
    for k in union(&va.keys, &vb.keys) {
        let (x, y) = (va.get(k), vb.get(k));
        if x != y {
            d.changes.push(Change {
                path: k.to_owned(),
                a: x.cloned(),
                b: y.cloned(),
            });
        }
    }
    let empty: Vec<String> = Vec::new();
    for k in union(&la.keys, &lb.keys) {
        let x = la.get(k).unwrap_or(&empty);
        let y = lb.get(k).unwrap_or(&empty);
        let ((cx, ux), (cy, uy)) = (counts(x), counts(y));
        for (key, &n) in cx.keys.iter().zip(&cx.values) {
            for _ in count_of(&uy, key)..n {
                d.removed.push(format!("{k}: {key}"));
            }
        }
        for (key, &n) in cy.keys.iter().zip(&cy.values) {
            for _ in count_of(&ux, key)..n {
                d.added.push(format!("{k}: {key}"));
            }
        }
        let same_order = x.len() == y.len() && x.iter().zip(y).all(|(p, q)| net::same_ignore_case(p, q));
        if d.removed.is_empty() && d.added.is_empty() && !same_order {
            d.changes.push(Change {
                path: k.to_owned(),
                a: Some(format!("same {} entries", x.len())),
                b: Some("in another order".into()),
            });
        }
    }
    Ok(d)
}

fn listed(f: &EditDbx, (k, e): (usize, NodeId), placed: &HashMap<usize, PlacedObject>) -> Result<Listed> {
    let t = Table(f);
    Ok(Listed {
        index: k,
        ty: type_of(t, e)?,
        guid: get(t, e, "guid")?,
        id: get(t, e, "id")?,
        placed: placed
            .get(&k)
            .map(|p| (p.blueprint.clone().unwrap_or_default(), [p.x, p.y, p.z])),
    })
}

/// Compares two parsed binary files as LevelDiff does.
///
/// # Errors
/// A damaged file (where LevelDiff would fail with exit code 3).
pub fn diff(a: &EditDbx, b: &EditDbx, o: &Options) -> Result<LevelDiff> {
    let (ia, ib) = (pick(a, o)?, pick(b, o)?);
    let mut by_guid_b: HashMap<String, usize> = HashMap::new();
    for (n, &(_, e)) in ib.iter().enumerate() {
        by_guid_b.insert(net::to_upper(Table(b).get(e, "guid")?.unwrap_or("")), n);
    }
    let mut pairs: Vec<(usize, usize)> = Vec::new();
    let mut only_a: Vec<usize> = Vec::new();
    let mut matched_b: HashSet<usize> = HashSet::new();
    for (n, &(_, e)) in ia.iter().enumerate() {
        match by_guid_b.get(&net::to_upper(Table(a).get(e, "guid")?.unwrap_or(""))) {
            Some(&m) => {
                pairs.push((n, m));
                matched_b.insert(m);
            }
            None => only_a.push(n),
        }
    }
    let mut only_b: Vec<usize> = (0..ib.len()).filter(|m| !matched_b.contains(m)).collect();
    let placed_a: HashMap<usize, PlacedObject> = PlacedObject::all(a)?.into_iter().map(|p| (p.index, p)).collect();
    let placed_b: HashMap<usize, PlacedObject> = PlacedObject::all(b)?.into_iter().map(|p| (p.index, p)).collect();
    let mut paired_by_position = 0;
    if o.match_objects {
        let mut pos_b: HashMap<String, std::collections::VecDeque<usize>> = HashMap::new();
        for &m in &only_b {
            if let Some(p) = ib.get(m).and_then(|(k, _)| placed_b.get(k)) {
                pos_b.entry(p.position_key()).or_default().push_back(m);
            }
        }
        for n in only_a.clone() {
            let Some(p) = ia.get(n).and_then(|(k, _)| placed_a.get(k)) else {
                continue;
            };
            let Some(m) = pos_b
                .get_mut(&p.position_key())
                .and_then(std::collections::VecDeque::pop_front)
            else {
                continue;
            };
            pairs.push((n, m));
            if let Some(at) = only_a.iter().position(|&x| x == n) {
                only_a.remove(at);
            }
            if let Some(at) = only_b.iter().position(|&x| x == m) {
                only_b.remove(at);
            }
            paired_by_position += 1;
        }
    }
    let mut changed = Vec::new();
    for &(n, m) in &pairs {
        let (Some(&x), Some(&y)) = (ia.get(n), ib.get(m)) else {
            continue;
        };
        let d = compare(a, x, b, y, o)?;
        if d.changes.len() + d.added.len() + d.removed.len() > 0 {
            changed.push(d);
        }
    }
    let list = |f: &EditDbx, side: &[(usize, NodeId)], which: &[usize], placed: &HashMap<usize, PlacedObject>| {
        which
            .iter()
            .filter_map(|&n| side.get(n).copied())
            .map(|x| listed(f, x, placed))
            .collect::<Result<Vec<_>>>()
    };
    Ok(LevelDiff {
        count_a: ia.len(),
        count_b: ib.len(),
        same: pairs.len(),
        paired_by_position,
        only_a: list(a, &ia, &only_a, &placed_a)?,
        only_b: list(b, &ib, &only_b, &placed_b)?,
        changed,
    })
}

/// Compares two files' bytes ([`diff`] after DbxEdit's [`read`].
///
/// # Errors
/// A file that is not a binary `.dbx`, or a damaged one.
pub fn diff_bytes(a: &[u8], b: &[u8], o: &Options) -> Result<LevelDiff> {
    let (fa, fb) = (read(a)?, read(b)?);
    if !fa.is_binary() || !fb.is_binary() {
        return Err(Fault::invalid("both files must be binary .dbx"));
    }
    diff(&fa, &fb, o)
}

/// `Math.Round(v, 1)` of a single, as .NET formats the double (`ToString()`: 15 digits).
fn round1(v: f32) -> String {
    let v = f64::from(v);
    net_g(
        if v.abs() < 1e16 {
            (v * 10.0).round_ties_even() / 10.0
        } else {
            v
        },
        15,
    )
}

/// `Clip`: no line breaks, at most 120 UTF-16 units (then `...`); `(none)` for a missing value.
fn clip(s: Option<&str>) -> String {
    let s = s.unwrap_or("(none)").replace(['\r', '\n'], " ");
    let units = s.encode_utf16().count();
    if units <= 120 {
        return s;
    }
    let mut out = String::new();
    let mut n = 0;
    for c in s.chars() {
        if n + c.len_utf16() > 120 {
            break;
        }
        n += c.len_utf16();
        out.push(c);
    }
    out + "..."
}

fn opt(s: Option<&String>) -> &str {
    s.map_or("", String::as_str)
}

/// The text LevelDiff prints (each line ended by `\n`).
#[must_use]
pub fn text(d: &LevelDiff, name_a: &str, name_b: &str, o: &Options) -> String {
    let mut out = String::new();
    let mut say = |s: &str| {
        out.push_str(s);
        out.push('\n');
    };
    say(&format!("A: {name_a}  ({} instances)", d.count_a));
    say(&format!("B: {name_b}  ({} instances)", d.count_b));
    say(&format!(
        "same instance in both: {}{}; changed: {}; only in A: {}; only in B: {}",
        d.same,
        if d.paired_by_position > 0 {
            format!(
                " ({} placed objects paired by blueprint + position)",
                d.paired_by_position
            )
        } else {
            String::new()
        },
        d.changed.len(),
        d.only_a.len(),
        d.only_b.len()
    ));
    section(&mut say, "only in A (removed in B)", &d.only_a, o);
    section(&mut say, "only in B (added in B)", &d.only_b, o);
    if !o.summary && !d.changed.is_empty() {
        say("");
        say(&format!("== changed ({})", d.changed.len()));
        let shown = if o.all { d.changed.len() } else { 40 };
        for c in d.changed.iter().take(shown) {
            let other = match (&c.guid_b, &c.guid) {
                (Some(gb), g) if !net::same_ignore_case(gb, opt(g.as_ref())) || g.is_none() => {
                    format!(" = {gb} in B")
                }
                _ => String::new(),
            };
            say(&format!(
                "{}  {}  {}{other}",
                c.ty,
                opt(c.id.as_ref()),
                opt(c.guid.as_ref())
            ));
            let take = |n: usize| if o.all { usize::MAX } else { n };
            for ch in c.changes.iter().take(take(25)) {
                say(&format!(
                    "    {}: {}  ->  {}",
                    ch.path,
                    clip(ch.a.as_deref()),
                    clip(ch.b.as_deref())
                ));
            }
            if !o.all && c.changes.len() > 25 {
                say(&format!("    ... {} more field changes", c.changes.len() - 25));
            }
            for r in c.removed.iter().take(take(15)) {
                say(&format!("    - {r}"));
            }
            for r in c.added.iter().take(take(15)) {
                say(&format!("    + {r}"));
            }
            if !o.all && (c.removed.len() > 15 || c.added.len() > 15) {
                say(&format!(
                    "    ... list entries: {} removed, {} added in all",
                    c.removed.len(),
                    c.added.len()
                ));
            }
        }
        if !o.all && d.changed.len() > 40 {
            say(&format!(
                "... {} more changed instances (--all lists them)",
                d.changed.len() - 40
            ));
        }
    }
    out
}

fn section(say: &mut impl FnMut(&str), title: &str, list: &[Listed], o: &Options) {
    if list.is_empty() {
        return;
    }
    say("");
    // GroupBy (first-seen order), OrderByDescending (stable), Take(8)
    let mut groups: Vec<(&str, usize)> = Vec::new();
    for l in list {
        match groups.iter_mut().find(|g| g.0 == l.ty) {
            Some(g) => g.1 += 1,
            None => groups.push((&l.ty, 1)),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.1));
    let kinds: Vec<String> = groups.iter().take(8).map(|(t, n)| format!("{t} {n}")).collect();
    say(&format!("== {title}: {}  ({})", list.len(), kinds.join(", ")));
    if o.summary {
        return;
    }
    let shown = if o.all { list.len() } else { 40 };
    for l in list.iter().take(shown) {
        let place = l.placed.as_ref().map_or(String::new(), |(bp, p)| {
            format!("  {bp} at {}, {}, {}", round1(p[0]), round1(p[1]), round1(p[2]))
        });
        say(&format!(
            "    {}  {}  {}{place}",
            l.ty,
            opt(l.id.as_ref()),
            opt(l.guid.as_ref())
        ));
    }
    if !o.all && list.len() > 40 {
        say(&format!("    ... {} more (--all lists them)", list.len() - 40));
    }
}

/// `Js`: a JSON string (`null` for none): `"` and `\` escaped, control characters as `\u00XX`.
fn js(s: Option<&str>) -> String {
    let Some(s) = s else {
        return "null".into();
    };
    let mut b = String::with_capacity(s.len() + 2);
    b.push('"');
    for ch in s.chars() {
        if ch == '"' || ch == '\\' {
            b.push('\\');
            b.push(ch);
        } else if (ch as u32) < 32 {
            let _ = write!(b, "\\u{:04x}", ch as u32);
        } else {
            b.push(ch);
        }
    }
    b.push('"');
    b
}

/// The JSON LevelDiff writes with `--json`.
#[must_use]
pub fn json(d: &LevelDiff, name_a: &str, name_b: &str) -> String {
    let list = |l: &[Listed]| {
        let items: Vec<String> = l
            .iter()
            .map(|i| {
                format!(
                    "\n  {{\"type\": {}, \"id\": {}, \"guid\": {}}}",
                    js(Some(&i.ty)),
                    js(i.id.as_deref()),
                    js(i.guid.as_deref())
                )
            })
            .collect();
        format!("[{}]", items.join(","))
    };
    let changed: Vec<String> = d
        .changed
        .iter()
        .map(|c| {
            let fields: Vec<String> = c
                .changes
                .iter()
                .map(|f| {
                    format!(
                        "{{\"path\": {}, \"a\": {}, \"b\": {}}}",
                        js(Some(&f.path)),
                        js(f.a.as_deref()),
                        js(f.b.as_deref())
                    )
                })
                .collect();
            let strs = |l: &[String]| l.iter().map(|s| js(Some(s))).collect::<Vec<_>>().join(", ");
            format!(
                "\n  {{\"type\": {}, \"id\": {}, \"guid\": {}, \"guid_b\": {}, \"fields\": [{}], \"removed\": [{}], \
                 \"added\": [{}]}}",
                js(Some(&c.ty)),
                js(c.id.as_deref()),
                js(c.guid.as_deref()),
                js(c.guid_b.as_deref()),
                fields.join(", "),
                strs(&c.removed),
                strs(&c.added)
            )
        })
        .collect();
    format!(
        "{{\n \"a\": {},\n \"b\": {},\n \"same_instances\": {},\n \"only_in_a\": {},\n \"only_in_b\": {},\n \"changed\": [{}]\n}}\n",
        js(Some(name_a)),
        js(Some(name_b)),
        d.same,
        list(&d.only_a),
        list(&d.only_b),
        changed.join(",")
    )
}

/// What a run printed and its exit code: 0 no differences, 1 differences, 2 bad input, 3 an unexpected error.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Run {
    /// The exit code.
    pub code: i32,
    /// Standard output, each line ended by `\n`.
    pub stdout: String,
    /// Standard error, each line ended by `\n`.
    pub stderr: String,
    /// `(level, message)` lines for the log.
    pub log: Vec<(String, String)>,
}

/// The command line parsed: options and file names, or `None` for a usage error.
#[must_use]
pub fn parse_args(args: &[String]) -> Option<(Options, Vec<String>)> {
    let mut o = Options::default();
    let mut files = Vec::new();
    let mut missing = false;
    let mut i = 0;
    while let Some(a) = args.get(i) {
        let mut next = || {
            i += 1;
            args.get(i).cloned()
        };
        match a.as_str() {
            "--match-objects" => o.match_objects = true,
            "--summary" => o.summary = true,
            "--all" => o.all = true,
            "--type" => {
                o.type_filter = next();
                missing |= o.type_filter.is_none();
            }
            "--ignore" => match next() {
                Some(v) => {
                    o.ignore.insert(v);
                }
                None => missing = true,
            },
            "--json" => {
                o.json = next();
                missing |= o.json.is_none();
            }
            _ => files.push(a.clone()),
        }
        i += 1;
    }
    (files.len() == 2 && !missing).then_some((o, files))
}

/// LevelDiff's command line (`args` without the program name) in process: the same text, JSON file and exit code.
/// `log_path` is the log named in the error messages (`%LOCALAPPDATA%\bfbc2-tool-suite\logs\leveldiff-<date>.log`).
#[must_use]
pub fn run(args: &[String], log_path: Option<&str>) -> Run {
    let mut r = Run::default();
    let log = log_path.unwrap_or("no log");
    let fail_log = log_path.unwrap_or("(no log file)");
    let Some((o, files)) = parse_args(args) else {
        r.stderr = format!("{USAGE}\n");
        r.code = 2;
        return r;
    };
    let (Some(fa), Some(fb)) = (files.first(), files.get(1)) else {
        r.code = 2;
        return r;
    };
    let load = |p: &str| -> Result<EditDbx> { read(&net::read_bytes(p)?) };
    let (a, b) = match load(fa).and_then(|a| Ok((a, load(fb)?))) {
        Ok(x) => x,
        Err(f) => {
            let (cause, todo) = f.explain();
            r.log.push((
                "ERROR".into(),
                format!(
                    "leveldiff error in reading the files: {cause}\r\n{}: {}",
                    f.type_name(),
                    f.message
                ),
            ));
            r.stderr = format!("{}\n  what to do: {todo}  (details: {log})\n", f.message);
            r.code = 2;
            return r;
        }
    };
    if !a.is_binary() || !b.is_binary() {
        r.stderr = "both files must be binary .dbx\n".into();
        r.code = 2;
        return r;
    }
    let result = diff(&a, &b, &o).and_then(|d| {
        r.stdout = text(&d, fa, fb, &o);
        if let Some(j) = &o.json {
            std::fs::write(j, json(&d, fa, fb)).map_err(|e| Fault::io(&e, std::path::Path::new(j)))?;
        }
        Ok(i32::from(!d.is_empty()))
    });
    match result {
        Ok(code) => r.code = code,
        Err(f) => {
            let (cause, todo) = f.explain();
            r.log.push((
                "ERROR".into(),
                format!("leveldiff error: {cause}\r\n{}: {}", f.type_name(), f.message),
            ));
            r.stderr = format!("error: {cause}\n  what to do: {todo}\n  details: {fail_log}\n");
            r.code = 3;
        }
    }
    r
}

/// The position of an instance with a `Transform` (a 4 x 4 matrix of singles: its translation), by its index among
/// the partition's children.
#[must_use]
pub fn position_of(f: &EditDbx, index: usize) -> Option<[f32; 3]> {
    let i = *partition_children(f).ok()?.get(index)?;
    for &c in kids(f, i) {
        let payload = f.payload(c);
        if Table(f).get(c, "name").ok()? == Some("Transform") && f.kind(c) == NUMBERS && payload.len() >= 2 + 16 * 4 {
            let at = |k: usize| be_u32(payload, 2 + k * 4).map_or(0.0, f32::from_bits);
            return Some([at(12), at(13), at(14)]);
        }
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn arguments_as_leveldiff_reads_them() {
        let a = |s: &[&str]| s.iter().map(|x| (*x).to_owned()).collect::<Vec<_>>();
        let (o, f) = parse_args(&a(&[
            "x", "--type", "Spawn", "y", "--ignore", "A", "--ignore", "B", "--all",
        ]))
        .unwrap();
        assert_eq!(f, ["x", "y"]);
        assert_eq!(o.type_filter.as_deref(), Some("Spawn"));
        assert!(o.all && o.ignore.contains("A") && o.ignore.contains("B"));
        assert!(parse_args(&a(&["x", "y", "--type"])).is_none());
        assert!(parse_args(&a(&["x"])).is_none());
        assert!(parse_args(&a(&["x", "y", "z"])).is_none());
        // a switch's value is the next argument, whatever it is
        let (o, f) = parse_args(&a(&["--json", "--all", "x", "y"])).unwrap();
        assert_eq!(o.json.as_deref(), Some("--all"));
        assert!(!o.all && f.len() == 2);
    }

    #[test]
    fn texts_as_leveldiff_writes_them() {
        assert_eq!(clip(None), "(none)");
        assert_eq!(clip(Some("a\r\nb")), "a  b");
        let long = "x".repeat(130);
        assert_eq!(clip(Some(&long)), format!("{}...", "x".repeat(120)));
        assert_eq!(js(Some("a\"b\\c\u{1}")), "\"a\\\"b\\\\c\\u0001\"");
        assert_eq!(js(None), "null");
        assert_eq!(round1(5.25), "5.2");
        assert_eq!(round1(20.5), "20.5");
        assert_eq!(round1(-0.04), "0");
        assert!(contains_ignore_case("Entity.ReferenceObjectData", "referenceobject"));
        assert!(contains_ignore_case("x", ""));
    }
}
