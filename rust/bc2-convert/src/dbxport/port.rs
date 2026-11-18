//! `dbxport.port`: a BF1943 partition in BFBC2's schema - `_Port`'s alignment and reference rules, then the value
//! passes (`_part_link_names`, `_values`, `_conventions`, `_bones`, `rebuild_audio_connections`, `_gate`), each a
//! line-for-line port of the reference in `bfbc2-bf1943-import/dbxport.py` (its comments say why each rule is).
#![allow(
    clippy::many_single_char_names,
    clippy::similar_names,
    clippy::too_many_lines,
    clippy::struct_excessive_bools,
    clippy::unnecessary_wraps,
    clippy::type_complexity,
    clippy::items_after_statements,
    reason = "a line-for-line port of the Python reference: its names (e, t, p, g, r), its long rules and its option flags kept as they are, to be read side by side"
)]

use std::collections::{HashMap, HashSet};

use bc2_formats::dbx::{CONTAINER, NUMBERS, TEXT};

use super::Fail;
use super::arena::{Arena, Id, Node, Tn};
use super::catalogue::Catalogue;
use super::py::{self, R};
use super::rules;
use super::templates::{self, Chosen, Role, Step, Templates};

/// The catalogue a port asks, with the class defaults it chooses from (a fair test's are not the catalogue's own).
pub struct CatView<'a> {
    /// The tables.
    pub cat: &'a Catalogue,
    /// The prototypes' and defaults' nodes.
    pub els: &'a Arena,
    /// type -> (partition, node in `els`).
    pub defaults: &'a HashMap<String, (String, Id)>,
}

/// `ours`: partition -> its guids.
pub type Ours = HashMap<String, HashSet<String>>;

/// A `stand_ins` key: a target, or `(field, target)`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SiKey {
    /// `target`.
    Target(String),
    /// `(field, target)`.
    Field(String, String),
}

/// A `stand_ins` value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SiVal {
    /// `'partition/guid'`, `'partition/*'` or a partition.
    Ref(String),
    /// `None`.
    Null,
    /// `TEMPLATE`.
    Template,
    /// Anything else (the reference fails on it: left to Python).
    Other,
}

/// `BoneTables`.
#[derive(Debug, Clone, Default)]
pub struct Bones {
    /// The source skeleton's bone names.
    pub source: Vec<String>,
    /// How many bones the target has.
    pub target_len: usize,
    /// `at`: a target bone's index by name.
    pub at: HashMap<String, i128>,
    /// The skeleton's name (lower case).
    pub skeleton: String,
}

impl Bones {
    fn index(&self, i: i128) -> Option<i128> {
        let i = usize::try_from(i).ok()?;
        self.at.get(self.source.get(i)?).copied()
    }

    fn map(&self, i: i128) -> i128 {
        self.index(i).unwrap_or(i)
    }
}

/// A port's options (`dbxport.port`'s arguments).
pub struct Options<'a> {
    /// `drop_untemplated`.
    pub drop_untemplated: bool,
    /// `drop_types`.
    pub drop_types: HashSet<String>,
    /// `prune`.
    pub prune: bool,
    /// `required` (`{}` for `None`).
    pub required: HashMap<String, String>,
    /// The catalogue (`catalogue` or the templates').
    pub catalogue: Option<CatView<'a>>,
    /// `ours`.
    pub ours: Option<&'a Ours>,
    /// `stand_ins`.
    pub stand_ins: &'a HashMap<SiKey, SiVal>,
    /// `donors`, lower case.
    pub donors: Vec<String>,
    /// `strict`.
    pub strict: bool,
    /// `schema == 'error'`.
    pub schema_error: bool,
    /// `conventions`.
    pub conventions: bool,
    /// `bones`.
    pub bones: Option<&'a Bones>,
    /// `(name or '').lower()`.
    pub name: String,
    /// `resolve(partition)`.
    pub resolve: Option<&'a (dyn Fn(&str) -> R<bool> + Sync)>,
}

/// One thing the port did (`dbxport.Record`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Record {
    /// `path`.
    pub path: String,
    /// `action`.
    pub action: &'static str,
    /// `old`.
    pub old: Option<String>,
    /// `new`.
    pub new: Option<String>,
    /// `source`.
    pub source: Option<String>,
    /// `step`.
    pub step: Option<String>,
    /// `note`.
    pub note: Option<String>,
}

/// A port's result.
#[derive(Debug)]
pub struct Ported {
    /// The `.dbx` bytes.
    pub data: Vec<u8>,
    /// The records, in order.
    pub records: Vec<Record>,
}

fn some(s: impl Into<String>) -> Option<String> {
    Some(s.into())
}

/// An ordered `{name: element}` of named children and the names seen again (`_named`).
struct Named {
    order: Vec<String>,
    map: HashMap<String, Id>,
    dups: Vec<String>,
}

impl Named {
    fn of(ar: &Arena, kids: &[Id]) -> Self {
        let mut n = Self {
            order: Vec::new(),
            map: HashMap::new(),
            dups: Vec::new(),
        };
        for &c in kids {
            let Some(name) = ar.attr(c, "name") else { continue };
            if n.map.contains_key(name) {
                n.dups.push(name.to_owned());
            } else {
                n.order.push(name.to_owned());
                n.map.insert(name.to_owned(), c);
            }
        }
        n
    }

    fn get(&self, k: &str) -> Option<Id> {
        self.map.get(k).copied()
    }

    fn has(&self, k: Option<&str>) -> bool {
        k.is_some_and(|k| self.map.contains_key(k))
    }
}

struct P<'a> {
    ar: Arena,
    t: &'a Templates,
    o: &'a Options<'a>,
    report: Vec<Record>,
    dropped: HashSet<String>,
    local: HashSet<String>,
    by_guid: HashMap<String, Id>,
    inlined: HashSet<String>,
    errors: Vec<String>,
    chosen: HashMap<String, (Step, Option<Chosen<'a>>)>,
    checking: bool,
    chosen_partition: Option<String>,
    depth: usize,
    budget: usize,
}

/// How deep `align` nests, and how deep an input or template tree may be, before the port leaves the partition to
/// Python (whose recursion limit, 1,000 frames, then decides; real partitions are under 20 deep).
pub const MAX_DEPTH: usize = 150;

/// The nodes a port may make beyond 32 per node of its partition before it is left to Python (a few hundred MB).
pub const NODE_BUDGET: usize = 2_000_000;

fn unsupported(why: &str) -> Fail {
    Fail::Unsupported(why.to_owned())
}

impl<'a> P<'a> {
    fn cat(&self) -> Option<&'a CatView<'a>> {
        self.o.catalogue.as_ref()
    }

    fn rec(&mut self, path: impl Into<String>, action: &'static str) -> &mut Record {
        self.report.push(Record {
            path: path.into(),
            action,
            ..Record::default()
        });
        let n = self.report.len() - 1;
        // just pushed
        #[allow(clippy::indexing_slicing)]
        &mut self.report[n]
    }

    fn fail(&self, msg: impl FnOnce() -> String) -> R<()> {
        if self.o.strict { Err(Fail::Port(msg())) } else { Ok(()) }
    }

    // ---- references
    fn exists(&self, p: &str, g: &str) -> R<Option<bool>> {
        let (pl, gl) = (py::lower(p), py::lower(g));
        if let Some(ours) = self.o.ours
            && let Some(set) = ours.get(&pl)
        {
            return Ok(Some(set.contains(&gl)));
        }
        if let Some(c) = self.cat()
            && c.cat.exists(&pl, &gl)
        {
            return Ok(Some(true));
        }
        if let Some(r) = self.o.resolve
            && r(p)?
        {
            return Ok(None);
        }
        Ok(Some(false))
    }

    fn relocation(&self, p: &str, g: &str) -> Option<String> {
        let c = self.cat()?;
        let pl = py::lower(p);
        if self.o.ours.is_some_and(|o| o.contains_key(&pl)) {
            return None;
        }
        let mut w = c.cat.where_(g).filter(|x| *x != pl);
        let first = w.next()?;
        w.next().is_none().then(|| first.to_owned())
    }

    fn stand_in_ref(&self, si: &str, target_ref: &str) -> R<String> {
        if let Some(p) = si.strip_suffix("/*") {
            return Ok(format!("{}/{}", py::lower(p), py::rlast(target_ref, '/')));
        }
        if py::ends_in_guid(si) {
            return Ok(si.to_owned());
        }
        self.primary_ref(si)
    }

    fn ref_ok(&self, r: &str) -> R<bool> {
        if r.is_empty() || r == "null" {
            return Ok(true);
        }
        let Some((p, g)) = py::rsplit1(r, '/') else {
            let g = py::lower(r);
            return Ok(self.local.contains(&g) && !self.dropped.contains(&g));
        };
        if !self.checking {
            return Ok(true);
        }
        Ok(self.exists(p, g)? != Some(false))
    }

    fn stand_in(&self, field: &str, target: &str, wildcard: bool) -> Option<&'a SiVal> {
        let si = self.o.stand_ins;
        if si.is_empty() {
            return None;
        }
        si.get(&SiKey::Field(field.to_owned(), target.to_owned()))
            .or_else(|| si.get(&SiKey::Target(target.to_owned())))
            .or_else(|| {
                wildcard
                    .then(|| si.get(&SiKey::Field(field.to_owned(), "*".to_owned())))
                    .flatten()
            })
    }

    fn use_stand_in(
        &mut self,
        si: &SiVal,
        path: &str,
        target: &str,
        t: Option<Tn<'a>>,
        step: Step,
        r: &str,
    ) -> R<String> {
        match si {
            SiVal::Null => {
                let rec = self.rec(path, "substituted");
                rec.old = some(target);
                rec.new = some("null");
                rec.note = some("stand-in: null");
                Ok("null".into())
            }
            SiVal::Template => {
                let Some(tr) = t.and_then(|t| t.attr("ref")) else {
                    let rec = self.rec(path, "nulled");
                    rec.old = some(target);
                    rec.note = some("stand-in TEMPLATE: the template has no value");
                    return Ok("null".into());
                };
                self.template_ref(tr, path, step, "stand-in: the template's", true, Some(target))
            }
            SiVal::Ref(s) => {
                let new = self.stand_in_ref(s, if r.is_empty() { target } else { r })?;
                let rec = self.rec(path, "substituted");
                rec.old = some(target);
                rec.source = some(py::rhead(&new, '/'));
                rec.new = Some(new.clone());
                rec.note = some("stand-in");
                Ok(new)
            }
            SiVal::Other => Err(unsupported("a stand-in that is no string, None or TEMPLATE")),
        }
    }

    /// `(the reference to write, whether it was nulled)`.
    fn resolve_ref(
        &mut self,
        r: &str,
        path: &str,
        t: Option<Tn<'a>>,
        step: Step,
        from_template: bool,
    ) -> R<(String, bool)> {
        if r.is_empty() || r == "null" {
            return Ok((r.to_owned(), false));
        }
        let field = py::last_field(path).to_owned();
        let (p, g, target) = match py::rsplit1(r, '/') {
            None => {
                let g = py::lower(r);
                if self.local.contains(&g) && !self.dropped.contains(&g) && !from_template {
                    return Ok((r.to_owned(), false));
                }
                (None, g.clone(), format!("#{g}"))
            }
            Some((p, g)) => {
                if !self.checking {
                    return Ok((r.to_owned(), false));
                }
                let ex = self.exists(p, g)?;
                if ex == Some(true) {
                    return Ok((r.to_owned(), false));
                }
                if ex.is_none() && !self.cat().is_some_and(|c| c.cat.has_partition(p)) {
                    return Ok((r.to_owned(), false));
                }
                (Some(p), g.to_owned(), py::lower(p))
            }
        };
        if let Some(p) = p
            && let Some(moved) = self.relocation(p, &g)
        {
            let new = format!("{moved}/{g}");
            let rec = self.rec(path, "relocated");
            rec.old = some(r);
            rec.new = Some(new.clone());
            rec.source = Some(moved);
            return Ok((new, false));
        }
        if from_template {
            return Ok(("null".into(), true));
        }
        if let Some(si) = self.stand_in(&field, &target, false) {
            return Ok((self.use_stand_in(si, path, &target, t, step, r)?, false));
        }
        if step == Step::Counterpart
            && let Some(t) = t
            && let Some(tr) = t.attr("ref")
        {
            let new = self.template_ref(tr, path, step, "the counterpart's", false, Some(&target))?;
            return Ok((new, false));
        }
        if let Some(si) = self.stand_in(&field, &target, true) {
            return Ok((self.use_stand_in(si, path, &target, t, step, r)?, false));
        }
        let rec = self.rec(path, "nulled");
        rec.old = Some(target.clone());
        rec.note = some("no BFBC2 value: nothing stands in for it");
        self.fail(|| format!("{path}: refers to {target}, which will not exist (no stand-in)"))?;
        Ok(("null".into(), true))
    }

    fn primary_ref(&self, partition: &str) -> R<String> {
        let p = py::lower(partition);
        let g = self.cat().and_then(|c| c.cat.primary.get(&p));
        match g {
            Some(g) => Ok(format!("{p}/{g}")),
            None => Err(Fail::Port(format!(
                "stand-in {partition}: no such BFBC2 partition (give partition/guid)"
            ))),
        }
    }

    fn template_ref(
        &mut self,
        tr: &str,
        path: &str,
        step: Step,
        why: &str,
        guard: bool,
        target: Option<&str>,
    ) -> R<String> {
        if tr == "null" {
            let rec = self.rec(path, "substituted");
            rec.old = target.map(str::to_owned);
            rec.new = some("null");
            rec.note = some(why);
            return Ok("null".into());
        }
        if !tr.contains('/') {
            let g = py::lower(tr);
            if step == Step::Counterpart && self.local.contains(&g) && !self.dropped.contains(&g) {
                let rec = self.rec(path, "substituted");
                rec.old = target.map(str::to_owned);
                rec.new = some(tr);
                rec.note = some(why);
                return Ok(tr.to_owned());
            }
            let rec = self.rec(path, "nulled");
            rec.old = Some(format!("#{g}"));
            rec.note = Some(format!("{why} names the template's own instance"));
            return Ok("null".into());
        }
        if guard && !self.guard_ok(path, tr)? {
            return Ok("null".into());
        }
        let (new, nulled) = self.resolve_ref(tr, path, None, step, true)?;
        if nulled {
            let rec = self.rec(path, "nulled");
            rec.old = some(py::rhead(tr, '/'));
            rec.note = Some(format!("{why} will not exist either"));
            return Ok("null".into());
        }
        let rec = self.rec(path, "substituted");
        rec.old = target.map(str::to_owned);
        rec.source = some(py::rhead(&new, '/'));
        rec.new = Some(new.clone());
        rec.note = some(why);
        Ok(new)
    }

    fn guard_ok(&mut self, path: &str, r: &str) -> R<bool> {
        let target = py::rhead(r, '/');
        if let Some(why) = rules::guard(path, target, &self.o.name) {
            let rec = self.rec(path, "guarded");
            rec.new = some(r);
            rec.source = some(target);
            rec.note = some(why);
            self.fail(|| format!("{path}: {target} refused ({why})"))?;
            return Ok(false);
        }
        Ok(true)
    }

    // ---- nodes
    fn with_ref(&mut self, e: Id, r: &str) -> Id {
        let rid = self.ar.intern(r);
        let mut n = self.ar.n(e).clone();
        for (k, v) in &mut n.attrs {
            if self.ar.s(*k) == "ref" {
                *v = rid;
            }
        }
        self.ar.push(n)
    }

    fn empty_array(&mut self, e: Id) -> Id {
        let n = self.ar.n(e);
        let node = Node {
            tag: n.tag,
            kind: TEXT,
            attrs: n.attrs.clone(),
            text: 0,
            raw: Vec::new(),
            children: Vec::new(),
        };
        self.ar.push(node)
    }

    fn empty_array_t(&mut self, t: Tn<'a>) -> Id {
        let attrs: Vec<(String, String)> = t.ar.attrs(t.id);
        let a: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.ar.make(t.tag(), TEXT, &a, "", Vec::new(), Vec::new())
    }

    /// `dbx.Element(e.tag, e.kind, list(e.attributes), e.text, e.raw, children)`.
    fn rebuilt(&mut self, e: Id, children: Vec<Id>) -> Id {
        let n = self.ar.n(e);
        let node = Node {
            tag: n.tag,
            kind: n.kind,
            attrs: n.attrs.clone(),
            text: n.text,
            raw: n.raw.clone(),
            children,
        };
        self.ar.push(node)
    }

    fn rebuilt_t(&mut self, t: Tn<'a>, children: Vec<Id>) -> Id {
        let attrs = t.ar.attrs(t.id);
        let a: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.ar
            .make(t.tag(), t.kind(), &a, t.text(), t.raw().to_vec(), children)
    }

    fn set_numbers<B: AsRef<[u8]>>(&mut self, e: Id, vals: &[B], size: u64) {
        let raw = rules::numbers_raw(vals, size);
        if let Some(n) = self.ar.m(e) {
            n.raw = raw;
        }
    }

    fn set_text(&mut self, e: Id, s: &str) {
        let id = self.ar.intern(s);
        if let Some(n) = self.ar.m(e) {
            n.text = id;
        }
    }

    fn shape(&self, e: Id) -> R<String> {
        rules::shape(&self.ar, e)
    }

    fn schema(&mut self, path: &str, e: Id, t: Tn<'a>) -> R<()> {
        let msg = format!("{path}: BF1943 {}, BFBC2 {}", self.shape(e)?, rules::shape(t.ar, t.id)?);
        self.errors.push(msg);
        Ok(())
    }

    fn value(&self, e: Id) -> R<String> {
        rules::value(&self.ar, e).map_err(Fail::from)
    }

    // ---- alignment
    fn item_proto(&self, itype: &str, tpath: &str) -> Option<Tn<'a>> {
        for c in self.t.cands.get(itype).map_or(&[][..], Vec::as_slice) {
            if let Some(el) = self.t.el(c)
                && let Some(a) = rules::at_path(el, tpath)
                && a.tag() == "array"
                && a.kind() == CONTAINER
                && a.has_kids()
            {
                return a.kid(0);
            }
        }
        let c = self.cat()?;
        let id = c.cat.proto(itype, &format!("{tpath}[]"))?;
        Some(Tn { ar: c.els, id })
    }

    /// `align`, nested at most [`MAX_DEPTH`] deep: deeper (an instance held inline in itself, a tree deeper than any
    /// file) the reference's own recursion limit decides (`RecursionError`), so it is left to Python.
    fn align(&mut self, e: Id, t: Option<Tn<'a>>, path: &str, tpath: &str, itype: &str, step: Step) -> R<Option<Id>> {
        if self.depth >= MAX_DEPTH {
            return Err(unsupported(
                "alignment nested past the depth limit (the reference's RecursionError)",
            ));
        }
        if self.ar.nodes.len() > self.budget {
            // instances held inline in each other many times over: the copies grow exponentially (the reference's
            // too, but it frees what it drops; the arena keeps every node) - left to Python
            return Err(unsupported("the port's copies past the node budget"));
        }
        self.depth += 1;
        let got = self.align_one(e, t, path, tpath, itype, step);
        self.depth -= 1;
        got
    }

    fn align_one(
        &mut self,
        e: Id,
        t: Option<Tn<'a>>,
        path: &str,
        tpath: &str,
        itype: &str,
        step: Step,
    ) -> R<Option<Id>> {
        if let Some(r) = self.ar.attr(e, "ref").map(str::to_owned)
            && self.ar.kids(e).is_empty()
        {
            if let Some(t) = t
                && t.attr("ref").is_none()
                && t.kind() != CONTAINER
                && t.tag() != "array"
            {
                self.schema(path, e, t)?;
                return Ok(Some(e));
            }
            let (new, _) = self.resolve_ref(&r, path, t, step, false)?;
            return Ok(Some(if new == r { e } else { self.with_ref(e, &new) }));
        }
        let (kind, is_array) = (self.ar.kind(e), self.ar.tag(e) == "array");
        if is_array && (kind == CONTAINER || kind == TEXT) {
            return self.align_array(e, t, path, tpath, itype, step).map(Some);
        }
        if kind == CONTAINER {
            if let Some(t) = t
                && (t.kind() != CONTAINER || t.tag() != self.ar.tag(e))
            {
                self.schema(path, e, t)?;
                return Ok(Some(e));
            }
            return self.align_struct(e, t, path, tpath, itype, step).map(Some);
        }
        let Some(t) = t else {
            return self.convert_untemplated(e, path, tpath, itype).map(Some);
        };
        if kind == NUMBERS
            && is_array
            && t.tag() == "array"
            && t.attr("ref").is_none()
            && let Some(items) = self.numbers_to_items(e, t, path, tpath, itype)?
        {
            return Ok(Some(items));
        }
        if t.kind() == CONTAINER || t.attr("ref").is_some() {
            self.schema(path, e, t)?;
            return Ok(Some(e));
        }
        if kind == t.kind() && (kind != NUMBERS || self.shape(e)? == rules::shape(t.ar, t.id)?) {
            return Ok(Some(e));
        }
        let Some(conv) = self.convert(e, t, path, tpath, itype)? else {
            if kind == NUMBERS && t.kind() == NUMBERS && self.variable_count(itype, tpath, e, t)? {
                return Ok(Some(e));
            }
            self.schema(path, e, t)?;
            return Ok(Some(e));
        };
        // conv's name attributes: e's name (when it has one), as the reference sets them
        let ename = self.ar.attr(e, "name").filter(|s| !s.is_empty()).map(str::to_owned);
        if let Some(name) = ename {
            let nid = self.ar.intern(&name);
            let name_key = self.ar.find("name");
            if let Some(n) = self.ar.m(conv) {
                for (k, v) in &mut n.attrs {
                    if Some(*k) == name_key {
                        *v = nid;
                    }
                }
            }
        }
        Ok(Some(conv))
    }

    fn variable_count(&self, itype: &str, tpath: &str, e: Id, t: Tn<'a>) -> R<bool> {
        let (se, ve) = rules::numbers(self.ar.raw(e))?;
        let (st, _) = rules::numbers(t.raw())?;
        if se != st && !ve.is_empty() {
            return Ok(false);
        }
        if self.ar.tag(e) == "array" && t.tag() == "array" {
            return Ok(!self.fixed_count(itype, tpath, st));
        }
        let Some(c) = self.cat() else { return Ok(false) };
        if ve.is_empty() {
            return Ok(false);
        }
        let pre = format!("num{st}x");
        let counts: HashSet<&String> = c
            .cat
            .shapes_of(itype, tpath)
            .iter()
            .filter(|s| s.starts_with(&pre))
            .collect();
        Ok(counts.len() > 1)
    }

    fn fixed_count(&self, itype: &str, tpath: &str, size: u64) -> bool {
        let Some(c) = self.cat() else { return false };
        let pre = format!("num{size}x");
        let counts: HashSet<&String> = c
            .cat
            .shapes_of(itype, tpath)
            .iter()
            .filter(|s| s.starts_with(&pre))
            .collect();
        counts.len() == 1
    }

    fn whole_field(&self, itype: &str, tpath: &str, t: Tn<'a>) -> R<Option<bool>> {
        if let Some(c) = self.cat() {
            if let Some(w) = c.cat.is_whole(itype, tpath) {
                return Ok(Some(w));
            }
            if c.cat.numeric_seen(itype, tpath) {
                return Ok(None);
            }
        }
        let mut seen: HashSet<bool> = HashSet::new();
        let mut els: Vec<Option<Tn<'a>>> = vec![Some(t)];
        for c in self.t.cands.get(itype).map_or(&[][..], Vec::as_slice) {
            els.push(self.t.el(c).and_then(|el| rules::at_path(el, tpath)));
        }
        for el in els.into_iter().flatten() {
            if el.kind() == NUMBERS {
                let (size, vals) = rules::numbers(el.raw())?;
                if size == 4 {
                    for v in vals {
                        let x = py::int_be(v)?;
                        if x != 0 {
                            seen.insert((x >> 24) == 0 || (x >> 24) == -1);
                        }
                    }
                }
            }
        }
        Ok(if seen.len() == 1 { seen.into_iter().next() } else { None })
    }

    fn convert(&mut self, e: Id, t: Tn<'a>, path: &str, tpath: &str, itype: &str) -> R<Option<Id>> {
        let n = self.ar.attr(e, "name").unwrap_or("").to_owned();
        let owner = py::owner(path).to_owned();
        let ek = self.ar.kind(e);
        if ek == NUMBERS && t.kind() == TEXT {
            return self.enum_number(e, Some(t), path, &owner, &n);
        }
        if t.kind() == NUMBERS && !matches!(ek, TEXT | NUMBERS | CONTAINER) && self.ar.raw(e).len() == 2 {
            let (size, vals) = rules::numbers(t.raw())?;
            if vals.len() != 1 {
                return Ok(None);
            }
            let v = self.ar.raw(e).get(1).copied().unwrap_or(0);
            let packed: Vec<u8> = match size {
                4 => {
                    let Some(whole) = self.whole_field(itype, tpath, t)? else {
                        self.errors.push(format!(
                            "{path}: a bool into a 4-byte number field DICE writes only as 0 (integer or float \
                             unknown: no evidence in the catalogue or the templates)"
                        ));
                        return Ok(Some(e));
                    };
                    if whole {
                        i32::from(v).to_be_bytes().to_vec()
                    } else {
                        f32::from(v).to_be_bytes().to_vec()
                    }
                }
                8 => f64::from(v).to_be_bytes().to_vec(),
                2 => u16::from(v).to_be_bytes().to_vec(),
                _ => return Ok(None),
            };
            let out = self.ar.import(t.ar, t.id);
            self.set_numbers(out, &[packed], size);
            let old = match v {
                1 => "true".to_owned(),
                0 => "false".to_owned(),
                v => v.to_string(),
            };
            let new = self.value(out)?;
            let rec = self.rec(path, "value");
            rec.old = Some(old);
            rec.new = Some(new);
            rec.note = some("a bool / byte as BFBC2's number");
            return Ok(Some(out));
        }
        if ek == NUMBERS && t.kind() == NUMBERS {
            let (se, ve) = rules::numbers(self.ar.raw(e))?;
            let (st, vt) = rules::numbers(t.raw())?;
            let (ve_len, vt_len) = (ve.len(), vt.len());
            if ve_len == 0
                && vt_len > 0
                && (se == st || se == 0)
                && (self.ar.tag(e) != "array" || self.fixed_count(itype, tpath, st))
            {
                let out = self.ar.import(t.ar, t.id);
                let new = self.value(out)?;
                let rec = self.rec(path, "value");
                rec.old = some("(empty)");
                rec.new = Some(new);
                rec.note = some("an empty number list where BFBC2 has values: the template's");
                return Ok(Some(out));
            }
            if rules::has_gradient_rows(&owner, &n) && se == st && ve_len > 0 && vt_len > 0 {
                let ve: Vec<Vec<u8>> = ve.into_iter().map(<[u8]>::to_vec).collect();
                let refs: Vec<&[u8]> = ve.iter().map(Vec::as_slice).collect();
                if let Some(got) = rules::gradient_rows(&refs, vt_len) {
                    let got: Vec<Vec<u8>> = got.into_iter().map(<[u8]>::to_vec).collect();
                    let out = self.ar.deep(e);
                    self.set_numbers(out, &got, se);
                    let rec = self.rec(path, "converted");
                    rec.note = Some(format!("{ve_len} numbers -> {} (gradient_rows)", got.len()));
                    return Ok(Some(out));
                }
            }
        }
        Ok(None)
    }

    fn numbers_to_items(&mut self, e: Id, t: Tn<'a>, path: &str, tpath: &str, itype: &str) -> R<Option<Id>> {
        let n = self.ar.attr(e, "name").unwrap_or("");
        let owner = py::owner(path);
        let Some((key, others)) = rules::numbers_to_items(owner, n) else {
            return Ok(None);
        };
        let proto = if t.kind() == CONTAINER && t.has_kids() {
            t.kid(0)
        } else {
            self.item_proto(itype, tpath)
        };
        let Some(proto) = proto else { return Ok(None) };
        if proto.kind() != CONTAINER {
            return Ok(None);
        }
        let fields: Vec<Option<&str>> = proto.kids().map(|c| c.attr("name")).collect();
        let mut got: Vec<&str> = fields.iter().map(|f| f.unwrap_or("")).collect();
        got.sort_unstable();
        let mut want: Vec<&str> = std::iter::once(key).chain(others.iter().map(|(f, _)| *f)).collect();
        want.sort_unstable();
        if got != want {
            return Ok(None);
        }
        for c in proto.kids() {
            if c.kind() != NUMBERS || rules::numbers(c.raw())?.1.len() != 1 {
                return Ok(None);
            }
        }
        let (size, vals) = rules::numbers(self.ar.raw(e))?;
        let mut width: HashMap<&str, u64> = HashMap::new();
        for c in proto.kids() {
            width.insert(c.attr("name").unwrap_or(""), rules::numbers(c.raw())?.0);
        }
        if Some(&size) != width.get(key) {
            return Ok(None);
        }
        let vals: Vec<Vec<u8>> = vals.into_iter().map(<[u8]>::to_vec).collect();
        let mut items = Vec::with_capacity(vals.len());
        for v in &vals {
            let item = self.ar.import(proto.ar, proto.id);
            for c in self.ar.kids(item).to_vec() {
                let f = self.ar.attr(c, "name").unwrap_or("").to_owned();
                let w = width.get(f.as_str()).copied().unwrap_or(0);
                let bytes = if f == key {
                    v.clone()
                } else {
                    let x = others
                        .iter()
                        .find(|(o, _)| *o == f)
                        .map(|(_, x)| *x)
                        .ok_or_else(|| unsupported("NUMBERS_TO_ITEMS: an item field the map lacks"))?;
                    to_bytes_signed(x, w)?
                };
                self.set_numbers(c, &[bytes], w);
            }
            items.push(item);
        }
        let rest: Vec<String> = others.iter().map(|(f, v)| format!("{f} {v}")).collect();
        let names: Vec<&str> = fields.iter().map(|f| f.unwrap_or("")).collect();
        let note = format!(
            "{} numbers -> {{{}}} items ({key} the number; {}: DICE's)",
            vals.len(),
            names.join(", "),
            rest.join(", ")
        );
        self.rec(path, "converted").note = Some(note);
        if items.is_empty() {
            return Ok(Some(self.empty_array(e)));
        }
        let n = self.ar.n(e);
        let node = Node {
            tag: n.tag,
            kind: CONTAINER,
            attrs: n.attrs.clone(),
            text: 0,
            raw: Vec::new(),
            children: items,
        };
        Ok(Some(self.ar.push(node)))
    }

    fn enum_number(&mut self, e: Id, t: Option<Tn<'a>>, path: &str, owner: &str, n: &str) -> R<Option<Id>> {
        let (_, vals) = rules::numbers(self.ar.raw(e))?;
        let [one] = vals.as_slice() else { return Ok(None) };
        if !rules::has_enum_table(owner, n) {
            return Ok(None);
        }
        let num = py::int_be(one)?;
        let Some(name) = rules::enum_name(owner, n, num) else {
            let Some(t) = t else { return Ok(None) };
            self.rec(path, "UNMAPPED").old = Some(num.to_string());
            self.fail(|| format!("{path}: BF1943 number {num} has no known BFBC2 name"))?;
            return Ok(Some(self.ar.import(t.ar, t.id)));
        };
        let en = self.ar.n(e);
        let (tag, attrs) = (en.tag, en.attrs.clone());
        let text = self.ar.intern(name);
        let out = self.ar.push(Node {
            tag,
            kind: TEXT,
            attrs,
            text,
            raw: Vec::new(),
            children: Vec::new(),
        });
        let rec = self.rec(path, "value");
        rec.old = Some(num.to_string());
        rec.new = some(name);
        Ok(Some(out))
    }

    fn convert_untemplated(&mut self, e: Id, path: &str, tpath: &str, itype: &str) -> R<Id> {
        let n = self.ar.attr(e, "name").unwrap_or("").to_owned();
        let owner = if tpath.matches('.').count() == 1 {
            py::rlast(itype, '.')
        } else {
            py::owner(path)
        }
        .to_owned();
        if self.ar.kind(e) == NUMBERS
            && let Some(conv) = self.enum_number(e, None, path, &owner, &n)?
        {
            return Ok(conv);
        }
        if let Some(c) = self.cat() {
            let seen = c.cat.shapes_of(itype, tpath);
            if !seen.is_empty() {
                let s = self.shape(e)?;
                let want: HashSet<&str> = seen
                    .iter()
                    .map(|x| {
                        if x == "field-text" || x == "complex-text" {
                            "text"
                        } else {
                            x.as_str()
                        }
                    })
                    .collect();
                let base = format!("{}x", py::head(&s, 'x'));
                let ok = want.contains(s.as_str())
                    || (s.starts_with("num")
                        && want.iter().any(|w| w.starts_with(&base))
                        && want.iter().filter(|w| w.starts_with("num")).count() > 1);
                if !ok {
                    let mut w: Vec<&str> = want.into_iter().collect();
                    w.sort_unstable();
                    self.errors
                        .push(format!("{path}: BF1943 {s}, BFBC2 {} (no template)", w.join("/")));
                }
            }
        }
        Ok(e)
    }

    fn align_array(&mut self, e: Id, t: Option<Tn<'a>>, path: &str, tpath: &str, itype: &str, step: Step) -> R<Id> {
        let items = self.ar.kids(e).to_vec();
        if let Some(t) = t
            && t.tag() != "array"
        {
            self.schema(path, e, t)?;
            return Ok(e);
        }
        let titems: Vec<Tn<'a>> = match t {
            Some(t) if t.kind() == CONTAINER => t.kids().collect(),
            _ => Vec::new(),
        };
        let mut proto = None;
        if titems.is_empty() && t.is_some() && items.iter().any(|&c| self.ar.kind(c) == CONTAINER) {
            proto = self.item_proto(itype, tpath);
        }
        let mut keep = Vec::new();
        let mut pruned = 0usize;
        let tref = titems
            .first()
            .is_some_and(|t0| t0.attr("ref").is_some() && !t0.has_kids());
        let field = py::last_field(path).to_owned();
        for (i, &c) in items.iter().enumerate() {
            let ipath = format!("{path}[{i}]");
            let r = self.ar.attr(c, "ref").map(str::to_owned);
            let is_ref = r.is_some() && self.ar.kids(c).is_empty();
            if let Some(&t0) = titems.first()
                && is_ref != tref
            {
                let ti = titems.get(i).copied().unwrap_or(t0);
                if let Some(inline) = self.inlined_item(c, ti, &ipath, tpath, itype, step)? {
                    keep.push(inline);
                    continue;
                }
                self.schema(&ipath, c, t0)?;
                keep.push(c);
                continue;
            }
            if let Some(r) = r.filter(|_| is_ref) {
                if self.ref_ok(&r)? {
                    keep.push(c);
                    continue;
                }
                if self.checking
                    && let Some((p, g)) = py::rsplit1(&r, '/')
                    && let Some(moved) = self.relocation(p, g)
                {
                    let new = format!("{moved}/{g}");
                    let rec = self.rec(ipath.as_str(), "relocated");
                    rec.old = Some(r.clone());
                    rec.new = Some(new.clone());
                    rec.source = Some(moved);
                    keep.push(self.with_ref(c, &new));
                    continue;
                }
                let target = match py::rsplit1(&r, '/') {
                    None => format!("#{}", py::lower(&r)),
                    Some((p, _)) => py::lower(p),
                };
                if let Some(SiVal::Ref(si)) = self.stand_in(&field, &target, true) {
                    let new = self.stand_in_ref(si, &r)?;
                    let rec = self.rec(ipath.as_str(), "substituted");
                    rec.old = Some(target);
                    rec.source = some(py::rhead(&new, '/'));
                    rec.new = Some(new.clone());
                    rec.note = some("stand-in");
                    keep.push(self.with_ref(c, &new));
                    continue;
                }
                pruned += 1;
                continue;
            }
            if self.ar.kind(c) == CONTAINER {
                if self.o.prune && self.lost_inside(c)? {
                    pruned += 1;
                    continue;
                }
                let ti = titems.get(i).copied().or_else(|| titems.first().copied()).or(proto);
                let ti = ti.filter(|x| x.kind() == CONTAINER);
                if let Some(got) = self.align(c, ti, &ipath, &format!("{tpath}[]"), itype, step)? {
                    keep.push(got);
                }
                continue;
            }
            keep.push(c);
        }
        if pruned > 0 {
            self.rec(path, "pruned").note = Some(format!("{pruned} item(s) referring to what will not exist"));
        }
        if keep.is_empty() {
            return Ok(self.empty_array(e));
        }
        let n = self.ar.n(e);
        let node = Node {
            tag: n.tag,
            kind: CONTAINER,
            attrs: n.attrs.clone(),
            text: n.text,
            raw: n.raw.clone(),
            children: keep,
        };
        Ok(self.ar.push(node))
    }

    fn inlined_item(&mut self, c: Id, ti: Tn<'a>, ipath: &str, tpath: &str, itype: &str, step: Step) -> R<Option<Id>> {
        let Some(r) = self.ar.attr(c, "ref").map(str::to_owned) else {
            return Ok(None);
        };
        if !self.ar.kids(c).is_empty() || r.contains('/') || ti.kind() != CONTAINER || ti.attr("ref").is_some() {
            return Ok(None);
        }
        let rl = py::lower(&r);
        let Some(&inst) = self.by_guid.get(&rl) else {
            return Ok(None);
        };
        let kids: Vec<Id> = self.ar.kids(inst).to_vec();
        let copies: Vec<Id> = kids.into_iter().map(|k| self.ar.deep(k)).collect();
        let item = self.ar.make("complex", CONTAINER, &[], "", Vec::new(), copies);
        let out = self.align(item, Some(ti), ipath, &format!("{tpath}[]"), itype, step)?;
        self.inlined.insert(rl);
        let note = format!(
            "a reference to {} ({}) as BFBC2's inline struct",
            py::opt(self.ar.attr(inst, "type")),
            py::opt(self.ar.attr(inst, "guid"))
        );
        self.rec(ipath, "converted").note = Some(note);
        Ok(out)
    }

    fn lost_inside(&self, e: Id) -> R<bool> {
        if let Some(r) = self.ar.attr(e, "ref")
            && self.ar.kids(e).is_empty()
            && !self.ref_ok(r)?
        {
            if self.checking
                && let Some((p, g)) = py::rsplit1(r, '/')
                && self.relocation(p, g).is_some()
            {
                return Ok(false);
            }
            return Ok(true);
        }
        for &c in self.ar.kids(e) {
            if self.lost_inside(c)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn align_struct(&mut self, e: Id, t: Option<Tn<'a>>, path: &str, tpath: &str, itype: &str, step: Step) -> R<Id> {
        let kids = self.ar.kids(e).to_vec();
        let mine = Named::of(&self.ar, &kids);
        for dn in &mine.dups {
            self.rec(format!("{path}.{dn}"), "duplicate");
        }
        let Some(t) = t else {
            let mut kept = Vec::new();
            for &c in &kids {
                let Some(cn) = self.ar.attr(c, "name").map(str::to_owned) else {
                    kept.push(c);
                    continue;
                };
                if mine.dups.contains(&cn) && mine.get(&cn) != Some(c) {
                    continue;
                }
                if let Some(a) = self.align(c, None, &format!("{path}.{cn}"), &format!("{tpath}.{cn}"), itype, step)? {
                    kept.push(a);
                }
            }
            return Ok(self.rebuilt(e, kept));
        };
        let theirs: HashSet<&str> = t.kids().filter_map(|c| c.attr("name")).collect();
        let here = py::last_field(path).to_owned();
        let mut new = Vec::new();
        let mut hoisted: HashSet<&'static str> = HashSet::new();
        for c in t.kids() {
            let Some(n) = c.attr("name") else { continue };
            let (p, tp) = (format!("{path}.{n}"), format!("{tpath}.{n}"));
            let single = rules::one_to_array(n);
            if !mine.has(Some(n)) && mine.has(single) && c.tag() == "array" {
                let single = single.unwrap_or_default();
                let r = mine
                    .get(single)
                    .and_then(|m| self.ar.attr(m, "ref"))
                    .filter(|r| !r.is_empty())
                    .unwrap_or("null")
                    .to_owned();
                let arr = if r == "null" {
                    let arr = self.empty_array_t(c);
                    self.rec(p.as_str(), "converted").note = Some(format!("{single} -> {n}: null, an empty array"));
                    arr
                } else {
                    let item = self.ar.make("item", TEXT, &[("ref", &r)], "", Vec::new(), Vec::new());
                    let attrs = c.ar.attrs(c.id);
                    let a: Vec<(&str, &str)> = attrs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                    let wrapper = self.ar.make("array", CONTAINER, &a, "", Vec::new(), vec![item]);
                    let arr = match self.align(wrapper, Some(c), &p, &tp, itype, step)? {
                        Some(x) => x,
                        None => self.empty_array_t(c),
                    };
                    self.rec(p.as_str(), "converted").note = Some(format!("{single} -> {n} (one item)"));
                    arr
                };
                new.push(arr);
                continue;
            }
            let old = rules::renamed(n);
            if !mine.has(Some(n))
                && let Some(old) = old
                && let Some(m) = mine.get(old)
            {
                let el = self.ar.deep(m);
                let nid = self.ar.intern(n);
                let name_key = self.ar.find("name");
                if let Some(node) = self.ar.m(el) {
                    for (k, v) in &mut node.attrs {
                        if Some(*k) == name_key {
                            *v = nid;
                        }
                    }
                }
                let a = self.align(el, Some(c), &p, &tp, itype, step)?;
                new.push(a.ok_or_else(|| unsupported("RENAMED: a template element in the output"))?);
                self.rec(p.as_str(), "renamed").note = Some(format!("{old} -> {n}"));
                continue;
            }
            if !mine.has(Some(n))
                && let Some(moved) = rules::nested(&here, n)
                && c.kind() == CONTAINER
                && moved.iter().any(|m| mine.has(Some(m)))
            {
                let x = self.nested(&mine, moved, c, &p, &tp, itype, step)?;
                new.push(x);
                hoisted.extend(moved.iter().filter(|m| mine.has(Some(m))));
                continue;
            }
            if let Some(m) = mine.get(n) {
                if let Some(a) = self.align(m, Some(c), &p, &tp, itype, step)? {
                    new.push(a);
                }
            } else {
                let a = self.added(c, &p, step)?;
                new.push(a);
            }
        }
        let converted: HashSet<&str> = theirs
            .iter()
            .filter_map(|n| rules::one_to_array(n))
            .chain(theirs.iter().filter_map(|n| rules::renamed(n)))
            .collect();
        for n in &mine.order {
            if !theirs.contains(n.as_str()) && !converted.contains(n.as_str()) && !hoisted.contains(n.as_str()) {
                self.rec(format!("{path}.{n}"), "left out");
            }
        }
        if self.ar.tag(e) != "array" {
            for &c in &kids {
                if self.ar.attr(c, "name").is_none() {
                    new.push(c);
                }
            }
        }
        Ok(self.rebuilt(e, new))
    }

    #[allow(clippy::too_many_arguments)]
    fn nested(&mut self, mine: &Named, moved: &[&str], c: Tn<'a>, p: &str, tp: &str, itype: &str, step: Step) -> R<Id> {
        let conv = self.o.conventions;
        let mut kids = Vec::new();
        let mut used: Vec<String> = Vec::new();
        for k in c.kids() {
            let kn = k.attr("name");
            let kp = format!("{p}.{}", py::opt(kn));
            if let Some(kn) = kn
                && let Some(m) = mine.get(kn)
                && moved.contains(&kn)
            {
                used.push(kn.to_owned());
                if conv {
                    kids.push(self.ar.import(k.ar, k.id));
                    let old = if self.ar.kind(m) == CONTAINER {
                        "...".to_owned()
                    } else {
                        self.value(m)?
                    };
                    let new = if k.kind() == CONTAINER {
                        "...".to_owned()
                    } else {
                        rules::tvalue(k).map_err(Fail::from)?
                    };
                    if old != new {
                        let rec = self.rec(kp, "convention");
                        rec.old = Some(old);
                        rec.new = Some(new);
                        rec.note = some("recoil: the template's (DICE retuned BF1943's)");
                    }
                } else {
                    let a = self.align(m, Some(k), &kp, &format!("{tp}.{kn}"), itype, step)?;
                    kids.push(a.ok_or_else(|| unsupported("NESTED: a template element in the output"))?);
                }
            } else {
                let a = self.added(k, &kp, step)?;
                kids.push(a);
            }
        }
        let note = format!("{{{}}} -> {}", used.join(", "), py::rlast(p, '.'))
            + if conv {
                " (the template's values: DICE's convention)"
            } else {
                ""
            };
        self.rec(p, "nested").note = Some(note);
        Ok(self.rebuilt_t(c, kids))
    }

    fn added(&mut self, c: Tn<'a>, path: &str, step: Step) -> R<Id> {
        let out = self.ar.import(c.ar, c.id);
        let short = rules::short(&self.ar, out)?;
        let src = self.chosen_partition.clone();
        let rec = self.rec(path, "added");
        rec.new = Some(short);
        rec.source = src;
        self.template_refs(out, path, step)
    }

    fn template_refs(&mut self, e: Id, path: &str, step: Step) -> R<Id> {
        if let Some(r) = self.ar.attr(e, "ref").map(str::to_owned)
            && self.ar.kids(e).is_empty()
        {
            if r == "null" {
                return Ok(e);
            }
            if step == Step::Default {
                let old = match py::rsplit1(&r, '/') {
                    Some((h, _)) => h.to_owned(),
                    None => format!("#{}", py::lower(&r)),
                };
                let rec = self.rec(path, "nulled");
                rec.old = Some(old);
                rec.note = some("the class default's reference names another object");
                return Ok(self.with_ref(e, "null"));
            }
            if !r.contains('/') {
                let rl = py::lower(&r);
                if step == Step::Counterpart && self.local.contains(&rl) && !self.dropped.contains(&rl) {
                    return Ok(e);
                }
                let rec = self.rec(path, "nulled");
                rec.old = Some(format!("#{rl}"));
                rec.note = some("the template's own instance");
                return Ok(self.with_ref(e, "null"));
            }
            if !self.guard_ok(path, &r)? {
                return Ok(self.with_ref(e, "null"));
            }
            let (new, nulled) = self.resolve_ref(&r, path, None, step, true)?;
            if nulled {
                let rec = self.rec(path, "nulled");
                rec.old = some(py::rhead(&r, '/'));
                rec.note = some("the template's reference will not exist");
            }
            return Ok(if new == r { e } else { self.with_ref(e, &new) });
        }
        let kids = self.ar.kids(e).to_vec();
        if kids.is_empty() {
            return Ok(e);
        }
        let is_array = self.ar.tag(e) == "array";
        let mut keep = Vec::new();
        for (i, &c) in kids.iter().enumerate() {
            if is_array
                && self.ar.kids(c).is_empty()
                && let Some(cr) = self.ar.attr(c, "ref").map(str::to_owned)
            {
                if cr == "null" {
                    keep.push(c);
                    continue;
                }
                let crl = py::lower(&cr);
                let own = step == Step::Counterpart && self.local.contains(&crl) && !self.dropped.contains(&crl);
                if step == Step::Default || (!cr.contains('/') && !own) {
                    continue;
                }
                if cr.contains('/') {
                    let ip = format!("{path}[{i}]");
                    if !self.guard_ok(&ip, &cr)? {
                        continue;
                    }
                    let (new, nulled) = self.resolve_ref(&cr, &ip, None, step, true)?;
                    if nulled {
                        continue;
                    }
                    keep.push(if new == cr { c } else { self.with_ref(c, &new) });
                    continue;
                }
                keep.push(c);
                continue;
            }
            let cp = match self.ar.attr(c, "name") {
                Some(n) => format!("{path}.{n}"),
                None => format!("{path}[{i}]"),
            };
            keep.push(self.template_refs(c, &cp, step)?);
        }
        if keep.len() != kids.len() {
            self.rec(path, "emptied").note = Some(format!(
                "{} item(s) naming what will not exist",
                kids.len() - keep.len()
            ));
        }
        if is_array && keep.is_empty() {
            return Ok(self.empty_array(e));
        }
        if let Some(n) = self.ar.m(e) {
            n.children = keep;
        }
        Ok(e)
    }

    // ---- value passes
    fn values(&mut self, e: Id, itype: &str, path: &str) -> R<()> {
        for c in self.ar.kids(e).to_vec() {
            let n = self.ar.attr(c, "name").map(str::to_owned);
            let p = match &n {
                Some(n) => format!("{path}.{n}"),
                None => format!("{path}[]"),
            };
            if self.ar.kind(c) == TEXT
                && self.ar.attr(c, "ref").is_none()
                && self.ar.tag(c) != "array"
                && !self.ar.text(c).is_empty()
            {
                let v = py::strip(self.ar.text(c)).to_owned();
                let mut new = rules::eia_value(&v)?;
                if new == v && matches!(n.as_deref(), Some("HintLine1" | "HintLine2")) {
                    rules::hint_text(&v).unwrap_or(&v).clone_into(&mut new);
                }
                if new != v {
                    let rec = self.rec(format!("{itype}{p}"), "value");
                    rec.old = Some(v);
                    rec.new = Some(new.clone());
                    self.set_text(c, &new);
                }
            }
            if self.ar.kind(c) == CONTAINER {
                self.values(c, itype, &p)?;
            }
        }
        Ok(())
    }

    fn set_value<B: AsRef<[u8]>>(
        &mut self,
        e: Id,
        path: String,
        new_raw: &[B],
        size: u64,
        why: &str,
        action: &'static str,
    ) -> R<()> {
        let old = self.value(e)?;
        self.set_numbers(e, new_raw, size);
        let new = self.value(e)?;
        if new != old {
            let rec = self.rec(path, action);
            rec.old = Some(old);
            rec.new = Some(new);
            rec.note = some(why);
        }
        Ok(())
    }

    fn weaponish(&self) -> bool {
        if !self.o.name.is_empty() {
            return rules::weapon_partition(&self.o.name);
        }
        self.chosen
            .values()
            .any(|(_, c)| c.is_some_and(|c| rules::weapon_partition(c.partition)))
    }

    fn scalar_is(raw: &[u8], v: u8) -> bool {
        raw.get(1..) == Some(&[v][..])
    }

    #[allow(clippy::too_many_lines)]
    fn conventions(&mut self, inst: Id, by: &HashMap<String, Id>) -> R<()> {
        let t = self.ar.attr(inst, "type").unwrap_or("").to_owned();
        let short = py::rlast(&t, '.').to_owned();
        let g = py::lower(self.ar.attr(inst, "guid").unwrap_or(""));
        let (step, tpl) = match self.chosen.get(&g) {
            Some((s, c)) => (*s, c.map(|c| c.el)),
            None => (Step::None, None),
        };
        let path = t.clone();
        if short == "WeaponFiringData"
            && let Some(a) = self.ar.field(inst, "UseAutoAiming")
            && !matches!(self.ar.kind(a), TEXT | NUMBERS | CONTAINER)
            && Self::scalar_is(self.ar.raw(a), 1)
        {
            if let Some(n) = self.ar.m(a) {
                n.raw.truncate(1);
                n.raw.push(0);
            }
            let rec = self.rec(format!("{path}.UseAutoAiming"), "convention");
            rec.old = some("true");
            rec.new = some("false");
            rec.note = some("DICE: no auto-aim");
        }
        if !self.weaponish() {
            return Ok(());
        }
        let tfield = |name: &str| tpl.and_then(|t| t.field(name));
        match short.as_str() {
            "FiringFunctionData" => {
                let shot = self.ar.field(inst, "Shot");
                let speed = shot.and_then(|s| self.ar.field(s, "InitialSpeed"));
                let proj = shot.and_then(|s| self.ar.field(s, "ProjectileData"));
                let target = proj.and_then(|p| by.get(&py::lower(self.ar.attr(p, "ref").unwrap_or(""))).copied());
                let ptype = target.map_or(String::new(), |x| {
                    py::rlast(self.ar.attr(x, "type").unwrap_or(""), '.').to_owned()
                });
                let z = match speed {
                    Some(s) => {
                        let v = rules::floats(self.ar.raw(s))?;
                        if v.len() >= 3 { v.get(2).copied() } else { None }
                    }
                    None => None,
                };
                if let (Some(z), Some(speed)) = (z, speed)
                    && ptype == "BulletEntityData"
                    && (300.0..600.0).contains(&z)
                {
                    let mut vals = rules::floats(self.ar.raw(speed))?;
                    if let Some(x) = vals.get_mut(2) {
                        *x = 600.0;
                    }
                    let packed = vals.iter().map(|&v| py::pack_f(v)).collect::<R<Vec<_>>>()?;
                    self.set_value(
                        speed,
                        format!("{path}.Shot.InitialSpeed"),
                        &packed,
                        4,
                        "DICE: firearm bullets at 600 m/s",
                        "convention",
                    )?;
                }
                if let (Some(z), Some(speed)) = (z, speed)
                    && ptype == "GrenadeEntityData"
                    && (z - 24.0).abs() < 1e-3
                {
                    let mut vals = rules::floats(self.ar.raw(speed))?;
                    if let Some(x) = vals.get_mut(2) {
                        *x = 20.0;
                    }
                    let packed = vals.iter().map(|&v| py::pack_f(v)).collect::<R<Vec<_>>>()?;
                    self.set_value(
                        speed,
                        format!("{path}.Shot.InitialSpeed"),
                        &packed,
                        4,
                        "DICE: hand grenades thrown at 20",
                        "convention",
                    )?;
                }
                if let (Some(target), Some(z)) = (target, z)
                    && ptype == "BulletEntityData"
                    && z >= FIREARM_SPEED
                    && let Some(gr) = self.ar.field(target, "Gravity")
                    && rules::floats(self.ar.raw(gr))? == [0.0]
                {
                    let tt = py::opt(self.ar.attr(target, "type")).to_owned();
                    self.set_value(
                        gr,
                        format!("{tt}.Gravity"),
                        &[py::pack_f(-9.81)?],
                        4,
                        "DICE: bullet drop on firearms",
                        "convention",
                    )?;
                }
                if let Some(target) = target
                    && ptype == "GrenadeEntityData"
                    && let Some(gr) = self.ar.field(target, "Gravity")
                {
                    let v = rules::floats(self.ar.raw(gr))?;
                    if let Some(&first) = v.first()
                        && (first + 9.0).abs() < 1e-3
                    {
                        let tt = py::opt(self.ar.attr(target, "type")).to_owned();
                        self.set_value(
                            gr,
                            format!("{tt}.Gravity"),
                            &[py::pack_f(-8.0)?],
                            4,
                            "DICE: hand grenades",
                            "convention",
                        )?;
                    }
                }
                let ammo = self.ar.field(inst, "Ammo");
                let tammo = tfield("Ammo");
                let firearm = z.is_some_and(|z| z >= FIREARM_SPEED) && ptype == "BulletEntityData";
                if let (Some(ammo), Some(tammo)) = (ammo, tammo)
                    && firearm
                {
                    let mags = self.ar.field(ammo, "NumberOfMagazines");
                    let tm = tammo.field("NumberOfMagazines");
                    if let (Some(mags), Some(tm)) = (mags, tm)
                        && self.ar.kind(mags) == NUMBERS
                        && rules::ints(self.ar.raw(mags))? == [-1]
                        && rules::ints(tm.raw())? != [-1]
                    {
                        let vals: Vec<Vec<u8>> = rules::numbers(tm.raw())?.1.into_iter().map(<[u8]>::to_vec).collect();
                        self.set_value(
                            mags,
                            format!("{path}.Ammo.NumberOfMagazines"),
                            &vals,
                            4,
                            "DICE: finite magazines (the template's)",
                            "convention",
                        )?;
                    }
                    let lo = self.ar.field(ammo, "AmmoPickupMinAmount");
                    let hi = self.ar.field(ammo, "AmmoPickupMaxAmount");
                    let tlo = tammo.field("AmmoPickupMinAmount");
                    let thi = tammo.field("AmmoPickupMaxAmount");
                    if let (Some(lo), Some(hi), Some(tlo), Some(thi)) = (lo, hi, tlo, thi)
                        && rules::ints(self.ar.raw(lo))? == [0]
                        && rules::ints(self.ar.raw(hi))? == [0]
                    {
                        let vl: Vec<Vec<u8>> = rules::numbers(tlo.raw())?.1.into_iter().map(<[u8]>::to_vec).collect();
                        self.set_value(
                            lo,
                            format!("{path}.Ammo.AmmoPickupMinAmount"),
                            &vl,
                            4,
                            "DICE: ammo pickups (the template's)",
                            "convention",
                        )?;
                        let vh: Vec<Vec<u8>> = rules::numbers(thi.raw())?.1.into_iter().map(<[u8]>::to_vec).collect();
                        self.set_value(
                            hi,
                            format!("{path}.Ammo.AmmoPickupMaxAmount"),
                            &vh,
                            4,
                            "DICE: ammo pickups (the template's)",
                            "convention",
                        )?;
                    }
                }
            }
            "SoldierAimingSimulationData" => {
                let zt = self.ar.field(inst, "ZoomType");
                let tz = tfield("ZoomType");
                if let Some(zt) = zt
                    && self.ar.text(zt) == "ztHoldButtonToZoom"
                    && tz.is_none_or(|tz| tz.text() != "ztHoldButtonToZoom")
                {
                    let old = self.ar.text(zt).to_owned();
                    let rec = self.rec(format!("{path}.ZoomType"), "convention");
                    rec.old = Some(old);
                    rec.new = some("ztPressButtonToSwitchZoomLevel");
                    rec.note = some("DICE: press to zoom");
                    self.set_text(zt, "ztPressButtonToSwitchZoomLevel");
                }
            }
            "ZoomLevelData" => {
                if let Some(s) = self.ar.field(inst, "SprintLookSpeedMultiplier")
                    && self.ar.kind(s) == NUMBERS
                    && rules::floats(self.ar.raw(s))? == [0.5]
                {
                    self.set_value(
                        s,
                        format!("{path}.SprintLookSpeedMultiplier"),
                        &[py::pack_f(1.0)?],
                        4,
                        "DICE: full look speed while sprinting",
                        "convention",
                    )?;
                }
            }
            "SoldierWeaponData" => {
                let a = self.ar.field(inst, "AnimBaseSet");
                let ta = tfield("AnimBaseSet");
                if let Some(a) = a {
                    let at = self.ar.text(a).to_owned();
                    let mut new = None;
                    if at == "wabsShg" {
                        new = Some("wabsRif".to_owned());
                    } else if matches!(step, Step::Counterpart | Step::Donor)
                        && let Some(ta) = ta
                        && !ta.text().is_empty()
                        && ta.text() != at
                    {
                        new = Some(ta.text().to_owned());
                    }
                    if let Some(new) = new.filter(|s| !s.is_empty()) {
                        let rec = self.rec(format!("{path}.AnimBaseSet"), "convention");
                        rec.old = Some(at);
                        rec.new = Some(new.clone());
                        rec.note = some("DICE's body set for the class");
                        self.set_text(a, &new);
                    }
                }
                let states = self.ar.field(inst, "WeaponStates");
                let tlist: Vec<Tn<'a>> = tfield("WeaponStates").map_or_else(Vec::new, |t| t.kids().collect());
                let skids = states.map_or_else(Vec::new, |s| self.ar.kids(s).to_vec());
                for (i, s) in skids.into_iter().enumerate() {
                    let ts = if tlist.is_empty() {
                        None
                    } else {
                        tlist.get(i.min(tlist.len() - 1)).copied()
                    };
                    let h = self.ar.field(s, "AlwaysAimHead");
                    let th = ts.and_then(|ts| ts.field("AlwaysAimHead"));
                    if let (Some(h), Some(th)) = (h, th)
                        && Self::scalar_is(self.ar.raw(h), 0)
                        && Self::scalar_is(th.raw(), 1)
                    {
                        if let Some(n) = self.ar.m(h) {
                            n.raw.truncate(1);
                            n.raw.push(1);
                        }
                        let rec = self.rec(format!("{path}.WeaponStates[{i}].AlwaysAimHead"), "convention");
                        rec.old = some("false");
                        rec.new = some("true");
                        rec.note = some("DICE: the class's (the template's)");
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn bones(&mut self, inst: Id, b: &Bones) -> R<()> {
        let t = self.ar.attr(inst, "type").unwrap_or("").to_owned();
        let short = py::rlast(&t, '.').to_owned();
        if short == "BoneFakePhysicsData" {
            let name = self.ar.field(inst, "BoneName");
            let bid = self.ar.field(inst, "BoneId");
            if let (Some(name), Some(bid)) = (name, bid)
                && self.ar.kind(bid) == NUMBERS
            {
                let bn = py::strip(self.ar.text(name)).to_owned();
                let new = b.at.get(&bn).copied();
                let old = rules::ints(self.ar.raw(bid))?;
                if let Some(new) = new
                    && old != [new]
                    && old != [0]
                {
                    self.set_value(
                        bid,
                        format!("{t}.BoneId"),
                        &[py::pack_i(new)?],
                        4,
                        &format!("bone {bn}"),
                        "bones",
                    )?;
                }
            }
            return Ok(());
        }
        let nm = self.ar.field(inst, "Name");
        if short == "SkeletonAsset" && nm.is_none_or(|nm| py::lower(py::strip(self.ar.text(nm))) != b.skeleton) {
            return Ok(());
        }
        if short != "SkeletonAsset" && short != "SoldierIKData" && self.ar.field(inst, "WeaponBoneIndex").is_none() {
            return Ok(());
        }
        self.bones_walk(inst, "", &t, b)
    }

    fn bones_walk(&mut self, e: Id, path: &str, t: &str, b: &Bones) -> R<()> {
        for c in self.ar.kids(e).to_vec() {
            let n = self.ar.attr(c, "name").unwrap_or("").to_owned();
            let p = if n.is_empty() {
                format!("{path}[]")
            } else {
                format!("{path}.{n}")
            };
            if self.ar.kind(c) == NUMBERS
                && matches!(
                    n.as_str(),
                    "WeaponBoneIndex" | "BoneIndices" | "MinBoneIndex" | "MaxMainBoneIndex"
                )
            {
                let old = rules::ints(self.ar.raw(c))?;
                let new: Vec<i128> = if n == "MinBoneIndex"
                    && let Some(mbn) = self.ar.field(e, "MinBoneName")
                {
                    let w = b.at.get(py::strip(self.ar.text(mbn))).copied();
                    old.iter().map(|&v| w.unwrap_or(v)).collect()
                } else if n == "MaxMainBoneIndex" {
                    match old.first() {
                        None => old.clone(),
                        Some(&o0) => {
                            let mapped: Option<Vec<i128>> = if o0 >= 0 {
                                // range(old[0] + 1): an index past the source skeleton maps to nothing
                                if usize::try_from(o0).is_ok_and(|x| x < b.source.len()) {
                                    (0..=o0).map(|i| b.index(i)).collect()
                                } else {
                                    None
                                }
                            } else {
                                Some(Vec::new())
                            };
                            let contiguous = match &mapped {
                                Some(m) if !m.is_empty() => {
                                    let mut s = m.clone();
                                    s.sort_unstable();
                                    s.iter().zip(0i128..).all(|(&a, b)| a == b)
                                }
                                _ => false,
                            };
                            let v = if contiguous {
                                mapped.as_ref().and_then(|m| m.iter().max().copied()).unwrap_or(0)
                            } else {
                                b.target_len as i128 - 1
                            };
                            vec![v]
                        }
                    }
                } else {
                    old.iter().map(|&v| b.map(v)).collect()
                };
                if new != old {
                    let packed = new.iter().map(|&v| py::pack_i(v)).collect::<R<Vec<_>>>()?;
                    self.set_value(c, format!("{t}{p}"), &packed, 4, "BFBC2's skeleton", "bones")?;
                }
            } else if self.ar.tag(c) == "array" && n == "BoneIndices" {
                for it in self.ar.kids(c).to_vec() {
                    if self.ar.kind(it) == NUMBERS {
                        let old = rules::ints(self.ar.raw(it))?;
                        let new: Vec<i128> = old.iter().map(|&v| b.map(v)).collect();
                        if new != old {
                            let packed = new.iter().map(|&v| py::pack_i(v)).collect::<R<Vec<_>>>()?;
                            self.set_value(it, format!("{t}{p}[]"), &packed, 4, "BFBC2's skeleton", "bones")?;
                        }
                    }
                }
            }
            if self.ar.kind(c) == CONTAINER {
                self.bones_walk(c, &p, t, b)?;
            }
        }
        Ok(())
    }

    fn part_link_names(&mut self, instances: &[Id]) -> R<()> {
        let by = last_by_guid(
            &self.ar,
            instances.iter().copied().filter(|&i| self.ar.tag(i) == "instance"),
        );
        for i in &by.order {
            let Some(&inst) = by.map.get(i) else { continue };
            let (Some(links), Some(comps)) = (self.ar.field(inst, "PartLinks"), self.ar.field(inst, "Components"))
            else {
                continue;
            };
            let mut names: Vec<Option<String>> = Vec::new();
            for &c in self.ar.kids(comps) {
                let target = by.get(&py::lower(self.ar.attr(c, "ref").unwrap_or("")));
                let n = target.and_then(|t| self.ar.field(t, "Name"));
                names.push(n.map(|n| self.ar.text(n).to_owned()));
            }
            let mut fixed = 0usize;
            for item in self.ar.kids(links).to_vec() {
                let Some(link) = by.get(&py::lower(self.ar.attr(item, "ref").unwrap_or(""))) else {
                    continue;
                };
                for k in ["1", "2"] {
                    let idx = self.ar.field(link, &format!("PartComponentIndex{k}"));
                    let name = self.ar.field(link, &format!("PartComponentName{k}"));
                    let (Some(idx), Some(name)) = (idx, name) else { continue };
                    let j = py_int(&self.value(idx)?)?;
                    let want = usize::try_from(j)
                        .ok()
                        .and_then(|j| names.get(j))
                        .map_or(String::new(), |n| n.clone().unwrap_or_default());
                    if self.ar.text(name) != want {
                        self.set_text(name, &want);
                        fixed += 1;
                    }
                }
            }
            if fixed > 0 {
                let t = self.ar.attr(inst, "type").unwrap_or("").to_owned();
                self.rec(t, "derived").note = Some(format!("{fixed} part link names from their indices"));
            }
        }
        Ok(())
    }

    fn integer(&self, e: Option<Id>) -> R<i128> {
        let Some(e) = e.filter(|&e| self.ar.kind(e) == NUMBERS) else {
            return Err(Fail::Port("sound graph has a missing/non-numeric buffer id".into()));
        };
        let vals = rules::ints(self.ar.raw(e))?;
        match vals.as_slice() {
            [v] => Ok(*v),
            _ => Err(Fail::Port("sound graph buffer id is not a scalar".into())),
        }
    }

    /// `rebuild_audio_connections(new)`: the graphs renumbered, in order (each changed before the next is read).
    fn audio(&mut self, instances: &[Id]) -> R<Vec<String>> {
        let by = last_by_guid(&self.ar, instances.iter().copied());
        let mut changed = Vec::new();
        let mut repaired: HashSet<String> = HashSet::new();
        let graphs: Vec<Id> = instances
            .iter()
            .copied()
            .filter(|&i| self.ar.attr(i, "type") == Some("Audio.SoundTemplate"))
            .collect();
        for graph in graphs {
            if let Some(plan) = self.audio_plan(graph, instances, &by, &mut repaired)? {
                let guid = plan.guid.clone();
                self.apply_audio(plan)?;
                changed.push(guid);
            }
        }
        Ok(changed)
    }

    /// One graph of `rebuild_audio_connections`: `None` when it is DICE's numbering already (or has no
    /// `AudioConnections`), else what to write.
    #[allow(clippy::too_many_lines)]
    fn audio_plan(
        &self,
        graph: Id,
        instances: &[Id],
        by: &LastBy,
        repaired: &mut HashSet<String>,
    ) -> R<Option<AudioPlan>> {
        let ar = &self.ar;
        let is_audio = |e: Id| {
            ar.attr(e, "type") == Some("Audio.SoundNodePort")
                && ar.field(e, "PortType").is_some_and(|t| ar.text(t) == "SgptAudio")
        };
        let wired = |e: Id| ar.field(e, "WireIndex").is_some_and(|w| ar.kind(w) == NUMBERS);
        let guid = py::lower(ar.attr(graph, "guid").unwrap_or(""));
        let wires = ar.field(graph, "WireCount");
        let Some(connections) = ar.field(graph, "AudioConnections") else {
            return Ok(None);
        };
        let mut reachable: HashSet<String> = HashSet::new();
        let mut todo = vec![guid.clone()];
        while let Some(g) = todo.pop() {
            if !reachable.insert(g.clone()) {
                continue;
            }
            let Some(e) = by.get(&g) else {
                return Err(unsupported("sound graph: a guid the instances lack (KeyError)"));
            };
            let mut refs = HashSet::new();
            local_refs(ar, e, by, &mut refs);
            todo.extend(refs.into_iter().filter(|r| !reachable.contains(r)));
        }
        let ports: HashMap<&str, Id> = reachable
            .iter()
            .filter_map(|g| by.get(g).filter(|&e| is_audio(e)).map(|e| (g.as_str(), e)))
            .collect();
        let mut edges: Vec<(String, String)> = Vec::new();
        for &conn in instances {
            if ar.attr(conn, "type") != Some("Audio.SoundNodeConnection") {
                continue;
            }
            let end = |f: &str| {
                ar.field(conn, f)
                    .map_or(String::new(), |s| py::lower(ar.attr(s, "ref").unwrap_or("")))
            };
            let (a, b) = (end("SourcePort"), end("TargetPort"));
            let (ina, inb) = (ports.contains_key(a.as_str()), ports.contains_key(b.as_str()));
            if !ina && !inb {
                continue;
            }
            if !ina || !inb {
                return Err(Fail::Port(format!(
                    "sound graph {guid} has an audio connection with an invalid endpoint"
                )));
            }
            edges.push((a, b));
        }
        let connected: HashSet<&str> = edges.iter().flat_map(|(a, b)| [a.as_str(), b.as_str()]).collect();
        let holders: HashMap<&str, Id> = reachable
            .iter()
            .filter(|g| !connected.contains(g.as_str()))
            .filter_map(|g| by.get(g).filter(|&e| wired(e)).map(|e| (g.as_str(), e)))
            .collect();
        let mut control: Vec<(&str, i128)> = Vec::new();
        for (&g, &e) in &holders {
            control.push((g, self.integer(ar.field(e, "WireIndex"))?));
        }
        let mut vals: Vec<i128> = control.iter().map(|&(_, v)| v).filter(|&v| v >= 0).collect();
        vals.sort_unstable();
        vals.dedup();
        let rank: HashMap<i128, i128> = vals.iter().zip(0i128..).map(|(&v, n)| (v, n)).collect();
        let wire_count = rank.len() as i128;
        let mut assigned: Vec<(&str, i128)> = Vec::new();
        for (a, b) in &edges {
            for g in [a.as_str(), b.as_str()] {
                if !assigned.iter().any(|(x, _)| *x == g) {
                    let n = wire_count + 1 + assigned.len() as i128;
                    assigned.push((g, n));
                }
            }
        }
        let at = |g: &str| assigned.iter().find(|(x, _)| *x == g).map_or(0, |(_, v)| *v);
        let mut actual = Vec::new();
        for &c in ar.kids(connections) {
            actual.push((
                self.integer(ar.field(c, "SourceId"))?,
                self.integer(ar.field(c, "TargetId"))?,
            ));
        }
        let mut sized = true;
        'rows: for &c in ar.kids(connections) {
            for f in ["SourceId", "TargetId"] {
                if let Some(x) = ar.field(c, f)
                    && rules::numbers(ar.raw(x))?.0 != AUDIO_ID_SIZE
                {
                    sized = false;
                    break 'rows;
                }
            }
        }
        let mut native =
            self.integer(wires)? == wire_count && control.iter().all(|&(_, v)| v < 0 || rank.get(&v) == Some(&v));
        if native {
            for &(g, v) in &assigned {
                let port = ports.get(g).copied();
                if self.integer(port.and_then(|p| ar.field(p, "WireIndex")))? != v {
                    native = false;
                    break;
                }
            }
        }
        let rows: Vec<(i128, i128)> = edges.iter().map(|(a, b)| (at(a), at(b))).collect();
        if native && sized && actual == rows {
            return Ok(None);
        }
        let touched: Vec<&str> = holders.keys().copied().chain(connected.iter().copied()).collect();
        if touched.iter().any(|g| repaired.contains(*g)) {
            return Err(Fail::Port(format!(
                "sound graph {guid} shares wired ports with another template"
            )));
        }
        repaired.extend(touched.iter().map(|g| (*g).to_owned()));
        let mut ranks = Vec::new();
        for &(g, v) in &control {
            if v >= 0 && rank.get(&v) != Some(&v) {
                let index = holders.get(g).and_then(|&h| ar.field(h, "WireIndex"));
                ranks.push((
                    index.ok_or_else(|| unsupported("WireIndex"))?,
                    rank.get(&v).copied().unwrap_or(v),
                ));
            }
        }
        let mut ids = Vec::new();
        for &(g, v) in &assigned {
            // the reference asserts the field is there (an AssertionError otherwise: left to it)
            let index = ports.get(g).and_then(|&p| ar.field(p, "WireIndex"));
            ids.push((index.ok_or_else(|| unsupported("an audio port without WireIndex"))?, v));
        }
        for &(a, b) in &rows {
            for v in [a, b] {
                if !(0..1 << 15).contains(&v) {
                    return Err(Fail::Port(format!(
                        "sound graph {guid}: audio buffer id {v} does not fit DICE's 2-byte field"
                    )));
                }
            }
        }
        Ok(Some(AudioPlan {
            guid,
            wires: wires.ok_or_else(|| unsupported("WireCount"))?,
            connections,
            wire_count,
            ranks,
            ids,
            rows,
        }))
    }

    fn apply_audio(&mut self, plan: AudioPlan) -> R<()> {
        self.set_numbers(plan.wires, &[py::pack_i(plan.wire_count)?], 4);
        for (e, v) in plan.ranks.into_iter().chain(plan.ids) {
            self.set_numbers(e, &[py::pack_i(v)?], 4);
        }
        let mut new_rows = Vec::new();
        for (a, b) in plan.rows {
            let mut num = |name: &str, v: i128| -> R<Id> {
                let x = i16::try_from(v).map_err(|_| unsupported("audio id"))?;
                let raw = rules::numbers_raw(&[x.to_be_bytes()], AUDIO_ID_SIZE);
                Ok(self.ar.make("field", NUMBERS, &[("name", name)], "", raw, Vec::new()))
            };
            let s = num("SourceId", a)?;
            let t = num("TargetId", b)?;
            new_rows.push(self.ar.make("complex", CONTAINER, &[], "", Vec::new(), vec![s, t]));
        }
        if let Some(n) = self.ar.m(plan.connections) {
            n.text = 0;
            n.raw = Vec::new();
            if new_rows.is_empty() {
                n.kind = TEXT;
                n.children = Vec::new();
            } else {
                n.kind = CONTAINER;
                n.children = new_rows;
            }
        }
        Ok(())
    }

    fn gate(&mut self, instances: &[Id]) -> R<()> {
        let Some(c) = self.cat() else { return Ok(()) };
        let mut problems = Vec::new();
        for &inst in instances {
            let t = self.ar.attr(inst, "type").unwrap_or("").to_owned();
            if let Some(order) = c.cat.order.get(&t) {
                let names: Vec<&str> = self
                    .ar
                    .kids(inst)
                    .iter()
                    .filter_map(|&k| self.ar.attr(k, "name"))
                    .collect();
                if names.iter().copied().ne(order.iter().map(String::as_str)) {
                    let miss: Vec<&str> = order
                        .iter()
                        .map(String::as_str)
                        .filter(|x| !names.contains(x))
                        .collect();
                    let extra: Vec<&str> = names
                        .iter()
                        .copied()
                        .filter(|x| !order.iter().any(|o| o == x))
                        .collect();
                    let m = if miss.is_empty() {
                        String::new()
                    } else {
                        format!("missing {}", miss.iter().take(4).copied().collect::<Vec<_>>().join(","))
                    };
                    let x = if extra.is_empty() {
                        String::new()
                    } else {
                        format!(" extra {}", extra.iter().take(4).copied().collect::<Vec<_>>().join(","))
                    };
                    let o = if miss.is_empty() && extra.is_empty() {
                        " order"
                    } else {
                        ""
                    };
                    problems.push(format!("{t}: fields not DICE's ({m}{x}{o})"));
                }
            }
            self.gate_walk(inst, "", &t, c.cat)?;
        }
        if !problems.is_empty() {
            if self.o.schema_error {
                return Err(Fail::Schema(problems));
            }
            for pr in problems {
                self.schema_record(&pr)?;
            }
        }
        Ok(())
    }

    fn schema_record(&mut self, pr: &str) -> R<()> {
        // the reference's err.split(':', 1)[1] (an IndexError without a ':': every message has one)
        let (h, rest) = pr
            .split_once(':')
            .ok_or_else(|| unsupported("a schema message without ':'"))?;
        self.rec(h, "schema").note = Some(py::strip(rest).to_owned());
        Ok(())
    }

    fn gate_walk(&mut self, e: Id, path: &str, t: &str, cat: &Catalogue) -> R<()> {
        for c in self.ar.kids(e).to_vec() {
            let p = match self.ar.attr(c, "name") {
                Some(n) => format!("{path}.{n}"),
                None => format!("{path}[]"),
            };
            if self.ar.kind(c) == TEXT
                && self.ar.attr(c, "ref").is_none()
                && self.ar.tag(c) != "array"
                && !self.ar.text(c).is_empty()
                && cat.is_enum(t, &p)
            {
                let text = self.ar.text(c).to_owned();
                if py::is_identifier(&text) && !cat.valid_enum(&text) {
                    let rec = self.rec(format!("{t}{p}"), "enum");
                    rec.old = Some(text.clone());
                    rec.note = some("not a name BFBC2's executable has");
                    self.fail(|| format!("{t}{p}: {text} is no enum name BFBC2 has"))?;
                }
            }
            if self.ar.kind(c) == CONTAINER {
                self.gate_walk(c, &p, t, cat)?;
            }
        }
        Ok(())
    }
}

const FIREARM_SPEED: f64 = 100.0;
const AUDIO_ID_SIZE: u64 = 2;

/// What `rebuild_audio_connections` writes into one graph.
struct AudioPlan {
    guid: String,
    wires: Id,
    connections: Id,
    wire_count: i128,
    ranks: Vec<(Id, i128)>,
    ids: Vec<(Id, i128)>,
    rows: Vec<(i128, i128)>,
}

/// `{guid.lower(): i for i in instances}`: the last instance of each guid, in the order each guid first came.
struct LastBy {
    order: Vec<String>,
    map: HashMap<String, Id>,
}

impl LastBy {
    fn get(&self, g: &str) -> Option<Id> {
        self.map.get(g).copied()
    }
}

fn last_by_guid(ar: &Arena, instances: impl Iterator<Item = Id>) -> LastBy {
    let mut by = LastBy {
        order: Vec::new(),
        map: HashMap::new(),
    };
    for i in instances {
        let g = py::lower(ar.attr(i, "guid").unwrap_or(""));
        if by.map.insert(g.clone(), i).is_none() {
            by.order.push(g);
        }
    }
    by
}

fn local_refs(ar: &Arena, e: Id, by: &LastBy, out: &mut HashSet<String>) {
    if let Some(r) = ar.attr(e, "ref")
        && !r.is_empty()
        && !r.contains('/')
    {
        let l = py::lower(r);
        if by.map.contains_key(&l) {
            out.insert(l);
        }
    }
    for &c in ar.kids(e) {
        local_refs(ar, c, by, out);
    }
}

/// `int(s)` of a value's text: plain decimal integers only; anything else Python's `int` decides.
fn py_int(s: &str) -> R<i128> {
    let b = s.strip_prefix('-').unwrap_or(s);
    if b.is_empty() || !b.bytes().all(|c| c.is_ascii_digit()) {
        return Err(unsupported("int() of a value that is no plain integer"));
    }
    s.parse().map_err(|_| unsupported("int() past 128 bits"))
}

/// `x.to_bytes(width, 'big', signed=True)`.
fn to_bytes_signed(x: i128, width: u64) -> R<Vec<u8>> {
    let w = usize::try_from(width).map_err(|_| unsupported("to_bytes width"))?;
    if w == 0 || w > 16 {
        return Err(unsupported("to_bytes width"));
    }
    let bits = 8 * w as u32;
    if bits < 128 {
        let lim = 1i128 << (bits - 1);
        if x < -lim || x >= lim {
            return Err(unsupported("to_bytes overflow"));
        }
    }
    let full = x.to_be_bytes();
    Ok(full.get(16 - w..).unwrap_or_default().to_vec())
}

/// `dbxport.port`.
///
/// # Errors
/// [`Fail::Parse`] when the partition does not parse, [`Fail::Port`] and [`Fail::Schema`] as the reference
/// raises `PortError` and `SchemaError`, [`Fail::Unsupported`] where the reference would answer otherwise than this
/// port can (its own exceptions, Unicode cases): the caller ports in Python.
#[allow(clippy::too_many_lines)]
pub fn port<'a>(data: &[u8], templates: &'a Templates, o: &'a Options<'a>) -> R<Ported> {
    let tree = bc2_formats::dbx::parse_tree(data).map_err(|e| Fail::Parse(e.to_string()))?;
    let ar = Arena::from_tree(&tree);
    if ar.depth() > MAX_DEPTH {
        return Err(unsupported("a tree deeper than the depth limit"));
    }
    drop(tree);
    let Some(&part) = ar.roots.first() else {
        return Err(unsupported("no partition"));
    };
    if ar.kind(part) != CONTAINER {
        return Err(unsupported("a partition that is no container"));
    }
    let originals = templates::partition_instances(&ar);
    let budget = NODE_BUDGET + 32 * ar.nodes.len();
    let mut p = P {
        local: originals
            .iter()
            .map(|&i| py::lower(ar.attr(i, "guid").unwrap_or("")))
            .collect(),
        by_guid: last_by_guid(&ar, originals.iter().copied()).map,
        ar,
        t: templates,
        o,
        report: Vec::new(),
        dropped: HashSet::new(),
        inlined: HashSet::new(),
        errors: Vec::new(),
        chosen: HashMap::new(),
        checking: o.resolve.is_some() || o.ours.is_some(),
        chosen_partition: None,
        depth: 0,
        budget,
    };
    let cat = p.cat();
    if o.drop_untemplated && cat.is_none() {
        return Err(Fail::Port(
            "drop_untemplated needs a catalogue (BFBC2's class names)".into(),
        ));
    }
    let mut insts: Vec<Id> = Vec::new();
    for c in p.ar.kids(part).to_vec() {
        if p.ar.tag(c) != "instance" {
            continue;
        }
        let mut inst = c;
        let mut t = p.ar.attr(inst, "type").unwrap_or("").to_owned();
        if let Some(new_t) = rules::type_as(&t)
            && !templates.has(&t)
            && !cat.is_some_and(|c| c.cat.has_class(&t))
        {
            let nt = p.ar.intern(new_t);
            let mut n = p.ar.n(inst).clone();
            let type_key = p.ar.find("type");
            for (k, v) in &mut n.attrs {
                if Some(*k) == type_key {
                    *v = nt;
                }
            }
            inst = p.ar.push(n);
            let guid = py::opt(p.ar.attr(inst, "guid")).to_owned();
            let rec = p.rec(format!("{t} ({guid})"), "type");
            rec.old = Some(t.clone());
            rec.new = some(new_t);
            t = new_t.to_owned();
        }
        let g = py::lower(p.ar.attr(inst, "guid").unwrap_or(""));
        let guid = py::opt(p.ar.attr(inst, "guid")).to_owned();
        if o.drop_types.contains(&t) {
            p.dropped.insert(g);
            p.rec(format!("{t} ({guid})"), "left out instance").note = some("BFBC2 has no such class");
        } else if o.drop_untemplated && !cat.is_some_and(|c| c.cat.has_class(&t)) && !templates.has(&t) {
            p.dropped.insert(g);
            p.rec(format!("{t} ({guid})"), "left out instance").note = some("BFBC2 has no such class (catalogue)");
        } else if o.conventions && rules::dice_moved(&t) {
            p.dropped.insert(g);
            p.rec(format!("{t} ({guid})"), "left out instance").note =
                some("DICE keeps these out of the partition (persistence/weapons)");
        }
        insts.push(inst);
    }
    let mut changed = !o.required.is_empty();
    while changed {
        changed = false;
        for &inst in &insts {
            let g = py::lower(p.ar.attr(inst, "guid").unwrap_or(""));
            let Some(fld) = o
                .required
                .get(p.ar.attr(inst, "type").unwrap_or(""))
                .filter(|f| !f.is_empty())
            else {
                continue;
            };
            if p.dropped.contains(&g) {
                continue;
            }
            let r =
                p.ar.field(inst, fld)
                    .and_then(|fe| p.ar.attr(fe, "ref"))
                    .map(str::to_owned);
            let Some(r) = r.filter(|r| !r.is_empty() && r != "null") else {
                continue;
            };
            if p.ref_ok(&r)? {
                continue;
            }
            if p.checking
                && let Some((rp, rg)) = py::rsplit1(&r, '/')
                && p.relocation(rp, rg).is_some()
            {
                continue;
            }
            p.dropped.insert(g);
            changed = true;
            let what = py::rsplit1(&r, '/').map_or(r.as_str(), |(h, _)| h).to_owned();
            let path = format!(
                "{} ({})",
                py::opt(p.ar.attr(inst, "type")),
                py::opt(p.ar.attr(inst, "guid"))
            );
            p.rec(path, "left out instance").note = Some(format!("its {fld} {what} will not exist"));
        }
    }
    let roles = templates::roles(&p.ar, &insts)?;
    let defaults_of = |t: &str| -> Option<Chosen<'_>> {
        let c = cat?;
        let (part, id) = c.defaults.get(t)?;
        Some(Chosen {
            partition: part.as_str(),
            el: Tn { ar: c.els, id: *id },
        })
    };
    let mut new: Vec<Id> = Vec::new();
    for &inst in &insts {
        let t = p.ar.attr(inst, "type").unwrap_or("").to_owned();
        let g = py::lower(p.ar.attr(inst, "guid").unwrap_or(""));
        if p.dropped.contains(&g) {
            continue;
        }
        let role: Role = roles.get(&g).cloned().unwrap_or_default();
        let (step, cand) = templates.choose(&t, &g, &role, &o.donors, Some(&defaults_of));
        p.chosen.insert(g, (step, cand));
        p.chosen_partition = cand.map(|c| c.partition.to_owned());
        let guid = py::opt(p.ar.attr(inst, "guid")).to_owned();
        let src = p.chosen_partition.clone();
        let rec = p.rec(format!("{t} ({guid})"), "template");
        rec.step = some(step.name());
        rec.source = src;
        if cand.is_none() {
            p.rec(format!("{t} ({guid})"), "untemplated");
            p.fail(|| format!("{t}: no BFBC2 template and no class default"))?;
        }
        if let Some(out) = p.align(inst, cand.map(|c| c.el), &t, "", &t, step)? {
            new.push(out);
        }
    }
    if !p.inlined.is_empty() {
        let mut still: HashSet<String> = HashSet::new();
        fn named(ar: &Arena, e: Id, still: &mut HashSet<String>) {
            if let Some(r) = ar.attr(e, "ref")
                && !r.is_empty()
                && !r.contains('/')
            {
                still.insert(py::lower(r));
            }
            for &c in ar.kids(e) {
                named(ar, c, still);
            }
        }
        for &inst in &new {
            if !p.inlined.contains(&py::lower(p.ar.attr(inst, "guid").unwrap_or(""))) {
                named(&p.ar, inst, &mut still);
            }
        }
        let mut gone: Vec<String> = p.inlined.iter().filter(|g| !still.contains(*g)).cloned().collect();
        gone.sort_unstable();
        new.retain(|&i| !gone.contains(&py::lower(p.ar.attr(i, "guid").unwrap_or(""))));
        p.errors.retain(|e| {
            let el = py::lower(e);
            !gone.iter().any(|g| el.contains(g.as_str()))
        });
        for g in &gone {
            p.rec(format!("#{g}"), "left out instance").note = some("held inline by what named it (BFBC2's shape)");
        }
    }
    if !p.errors.is_empty() {
        if o.schema_error {
            return Err(Fail::Schema(std::mem::take(&mut p.errors)));
        }
        for err in std::mem::take(&mut p.errors) {
            p.schema_record(&err)?;
        }
    }
    p.part_link_names(&new)?;
    let by = last_by_guid(&p.ar, new.iter().copied()).map;
    for &inst in &new {
        let t = p.ar.attr(inst, "type").unwrap_or("").to_owned();
        p.values(inst, &t, "")?;
        if o.conventions {
            p.conventions(inst, &by)?;
        }
        if let Some(b) = o.bones {
            p.bones(inst, b)?;
        }
    }
    for graph in p.audio(&new)? {
        p.rec(format!("Audio.SoundTemplate ({graph})"), "converted").note =
            some("PC audio buffer ids and AudioConnections rebuilt from this graph, not its donor");
    }
    p.gate(&new)?;
    let mut pnode = p.ar.n(part).clone();
    let prim = py::lower(p.ar.attr(part, "primaryInstance").unwrap_or(""));
    if !prim.is_empty()
        && !by.contains_key(&prim)
        && let Some(&first) = new.first()
    {
        let pick = new
            .iter()
            .copied()
            .find(|&i| p.ar.attr(i, "id").is_some())
            .unwrap_or(first);
        let pick_guid = p.ar.attr(pick, "guid").map(str::to_owned);
        let gid = p.ar.intern(pick_guid.as_deref().unwrap_or(""));
        let key = p.ar.find("primaryInstance");
        for (k, v) in &mut pnode.attrs {
            if Some(*k) == key {
                *v = gid;
            }
        }
        let rec = p.rec("primaryInstance", "primary");
        rec.old = Some(prim);
        rec.new = pick_guid;
        rec.note = some("the primary instance was left out");
    }
    let new: Vec<Id> = new.into_iter().map(|i| text_empty_arrays(&mut p.ar, i)).collect();
    pnode.children = new;
    let pid = p.ar.push(pnode);
    let mut roots = vec![pid];
    roots.extend(p.ar.roots.iter().skip(1).copied());
    let data =
        p.ar.write(&roots)
            .ok_or_else(|| unsupported("an element the writer refuses"))?;
    Ok(Ported {
        data,
        records: p.report,
    })
}

/// `_text_empty_arrays`.
fn text_empty_arrays(ar: &mut Arena, e: Id) -> Id {
    let n = ar.n(e);
    if ar.s(n.tag) == "array" && n.kind == CONTAINER && n.children.is_empty() {
        let node = Node {
            tag: n.tag,
            kind: TEXT,
            attrs: n.attrs.clone(),
            text: 0,
            raw: Vec::new(),
            children: Vec::new(),
        };
        return ar.push(node);
    }
    if !n.children.is_empty() {
        let kids = n.children.clone();
        let new: Vec<Id> = kids.into_iter().map(|c| text_empty_arrays(ar, c)).collect();
        if let Some(m) = ar.m(e) {
            m.children = new;
        }
    }
    e
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn helpers() {
        assert_eq!(to_bytes_signed(-1, 4).unwrap(), vec![0xFF; 4]);
        assert_eq!(to_bytes_signed(3, 2).unwrap(), vec![0, 3]);
        assert!(to_bytes_signed(128, 1).is_err());
        assert_eq!(py_int("-12").unwrap(), -12);
        assert!(py_int("1.5").is_err() && py_int("").is_err());
        let b = Bones {
            source: vec!["a".into(), "b".into()],
            target_len: 3,
            at: [("b".to_owned(), 0), ("a".to_owned(), 2)].into_iter().collect(),
            skeleton: String::new(),
        };
        assert_eq!(b.map(0), 2);
        assert_eq!(b.map(5), 5);
    }
}
