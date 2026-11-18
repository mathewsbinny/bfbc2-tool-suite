//! `dbxport.Templates`: BFBC2's instances by type, from partitions kept here (parsed once, never as Python trees),
//! and the template choice (`Templates.choose`) with E3d's ranking index (`_Ranking`, proven equal to the scan
//! `_best`).

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, PoisonError};

use bc2_formats::dbx::{CONTAINER, NUMBERS};

use super::arena::{Arena, Id, Tn};
use super::py::{self, R};
use super::rules::Rules;

/// `(array field, index)`: the local reference array naming an instance, and where.
pub type Slot = (String, usize);
/// An instance's input-action fields' values (normalized), `(field, value)`.
pub type Inputs = Vec<(String, String)>;
/// `(ordinal, slot, inputs)`: an instance's role among its partition's instances.
pub type Role = (usize, Option<Slot>, Inputs);

/// One BFBC2 instance of a type (`dbxport.Candidate`).
#[derive(Debug, Clone)]
pub struct Cand {
    /// Its partition's index in [`Templates::parts`].
    pub part: usize,
    /// Its node there.
    pub node: Id,
    /// Its role.
    pub role: Role,
}

/// One template partition: its name (`dbxport`'s key: lower case, `/`, no `.dbx`) and tree.
#[derive(Debug)]
pub struct Part {
    /// The key.
    pub name: String,
    /// The tree.
    pub arena: Arena,
}

/// `_Ranking`: `_best`'s choice over one candidate list from an index.
#[derive(Debug, Default)]
pub struct Ranking {
    /// Indices into the type's candidate list, in ranking order.
    pub cands: Vec<usize>,
    by_input: HashMap<(String, String), Vec<usize>>,
    by_slot: HashMap<Slot, Vec<usize>>,
    first_of_ordinal: HashMap<usize, usize>,
}

impl Ranking {
    fn new(all: &[Cand], cands: Vec<usize>) -> Self {
        let mut r = Self {
            cands,
            ..Self::default()
        };
        for (i, &ci) in r.cands.iter().enumerate() {
            let Some(c) = all.get(ci) else { continue };
            let (ordinal, slot, inputs) = &c.role;
            let set: HashSet<&(String, String)> = inputs.iter().collect();
            for p in set {
                r.by_input.entry(p.clone()).or_default().push(i);
            }
            if let Some(s) = slot {
                r.by_slot.entry(s.clone()).or_default().push(i);
            }
            r.first_of_ordinal.entry(*ordinal).or_insert(i);
        }
        r
    }

    /// `(score, the index into the type's candidates)` of the best candidate for `role`.
    fn best(&self, all: &[Cand], role: &Role) -> Option<(usize, usize)> {
        let (ordinal, slot, inputs) = role;
        let mut scores: HashMap<usize, usize> = HashMap::new();
        let set: HashSet<&(String, String)> = inputs.iter().collect();
        for p in set {
            for &i in self.by_input.get(p).map_or(&[][..], Vec::as_slice) {
                *scores.entry(i).or_insert(0) += 4;
            }
        }
        if let Some(s) = slot {
            for &i in self.by_slot.get(s).map_or(&[][..], Vec::as_slice) {
                *scores.entry(i).or_insert(0) += 2;
            }
        }
        if !scores.is_empty() {
            let mut best: Option<(usize, usize)> = None; // (score, i)
            for (&i, &s) in &scores {
                let ci = *self.cands.get(i)?;
                let s = s + usize::from(all.get(ci)?.role.0 == *ordinal);
                if best.is_none_or(|(top, win)| s > top || (s == top && i < win)) {
                    best = Some((s, i));
                }
            }
            let (s, i) = best?;
            return Some((s, *self.cands.get(i)?));
        }
        if let Some(&first) = self.first_of_ordinal.get(ordinal) {
            return Some((1, *self.cands.get(first)?));
        }
        Some((0, *self.cands.first()?))
    }
}

/// The step that chose a template.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    /// DICE's own version of the instance (the same guid).
    Counterpart,
    /// An instance of a partition the caller pairs with this one.
    Donor,
    /// The pool's instance whose role matches best.
    Role,
    /// The pool's first (nothing tells candidates apart).
    Pool,
    /// The catalogue's class default.
    Default,
    /// No template.
    None,
}

impl Step {
    /// The reference's name of the step.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Counterpart => "counterpart",
            Self::Donor => "donor",
            Self::Role => "role",
            Self::Pool => "pool",
            Self::Default => "default",
            Self::None => "none",
        }
    }
}

/// A chosen template: its partition (as the reference's `Candidate.partition`) and element.
#[derive(Clone, Copy)]
pub struct Chosen<'a> {
    /// The partition.
    pub partition: &'a str,
    /// The element.
    pub el: Tn<'a>,
}

type RankKey = (String, Option<Vec<String>>);

/// The catalogue's class default of a type, for [`Templates::choose`].
pub type Defaults<'d, 'a> = &'d dyn Fn(&str) -> Option<Chosen<'a>>;

/// BFBC2's instances by type (`dbxport.Templates`).
#[derive(Debug, Default)]
pub struct Templates {
    /// The partitions, in the order added.
    pub parts: Vec<Part>,
    /// Every instance with a type, by type, in the order added.
    pub cands: HashMap<String, Vec<Cand>>,
    by_guid: HashMap<String, (String, usize)>,
    rankings: Mutex<HashMap<RankKey, Arc<Ranking>>>,
}

/// `_local(r)`.
fn is_local(r: Option<&str>) -> bool {
    r.is_some_and(|r| !r.is_empty() && r != "null" && !r.contains('/'))
}

/// `_inputs(inst)`.
///
/// # Errors
/// As [`Rules::eia_value`], and a numbers value the reference reads otherwise.
pub fn inputs(ar: &Arena, inst: Id, tables: &Rules) -> R<Inputs> {
    let mut out = Vec::new();
    let t = py::rlast(ar.attr(inst, "type").unwrap_or(""), '.');
    for &c in ar.kids(inst) {
        let n = ar.attr(c, "name").unwrap_or("");
        if !super::rules::is_input_field(n) || ar.kind(c) == CONTAINER {
            continue;
        }
        let v = if ar.kind(c) == NUMBERS {
            let (_, vals) = super::rules::numbers(ar.raw(c))?;
            let [one] = vals.as_slice() else { continue };
            let num = py::int_be(one)?;
            tables
                .enum_name(t, n, num)
                .map_or_else(|| num.to_string(), str::to_owned)
        } else {
            tables.eia_value(py::strip(ar.text(c)))?
        };
        out.push((n.to_owned(), v));
    }
    Ok(out)
}

/// `_roles(instances)`: `{guid: (ordinal, slot, inputs)}` (a guid twice: the last instance's role).
///
/// # Errors
/// As [`inputs`].
pub fn roles(ar: &Arena, instances: &[Id], tables: &Rules) -> R<HashMap<String, Role>> {
    // the reference's walk: each child's array items, then the child's own subtree, then the next child
    fn walk(ar: &Arena, e: Id, slots: &mut HashMap<String, Slot>) {
        for &c in ar.kids(e) {
            if ar.tag(c) == "array" {
                for (i, &it) in ar.kids(c).iter().enumerate() {
                    let r = ar.attr(it, "ref");
                    if ar.kids(it).is_empty() && is_local(r) {
                        slots
                            .entry(py::lower(r.unwrap_or("")))
                            .or_insert_with(|| (ar.attr(c, "name").unwrap_or("").to_owned(), i));
                    }
                }
            }
            if ar.kind(c) == CONTAINER {
                walk(ar, c, slots);
            }
        }
    }
    let mut slots: HashMap<String, Slot> = HashMap::new();
    for &inst in instances {
        walk(ar, inst, &mut slots);
    }
    let mut seen: HashMap<&str, usize> = HashMap::new();
    let mut out = HashMap::new();
    for &inst in instances {
        let t = ar.attr(inst, "type").unwrap_or("");
        let n = seen.entry(t).or_insert(0);
        let ordinal = *n;
        *n += 1;
        let g = py::lower(ar.attr(inst, "guid").unwrap_or(""));
        let slot = slots.get(&g).cloned();
        out.insert(g, (ordinal, slot, inputs(ar, inst, tables)?));
    }
    Ok(out)
}

impl Templates {
    /// No partitions.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `Templates.add`: a partition's instances (its key as the reference makes it), their roles read with the
    /// tables the caller has when it adds them (the reference computes them then).
    ///
    /// # Errors
    /// As [`roles`] (the partition is not added).
    pub fn add(&mut self, name: String, mut arena: Arena, tables: &Rules) -> R<()> {
        arena.freeze();
        // the reference's f.instances raises on a file without a partition holding children
        if arena.roots.first().is_none_or(|&p| arena.kind(p) != CONTAINER) {
            return Err(super::Fail::Unsupported("a template without a partition".into()));
        }
        if arena.depth() > super::port::MAX_DEPTH {
            return Err(super::Fail::Unsupported(
                "a template deeper than the depth limit".into(),
            ));
        }
        let insts: Vec<Id> = partition_instances(&arena);
        let roles = roles(&arena, &insts, tables)?;
        self.rankings.lock().unwrap_or_else(PoisonError::into_inner).clear();
        let part = self.parts.len();
        for &inst in &insts {
            let Some(t) = arena.attr(inst, "type").filter(|t| !t.is_empty()) else {
                continue;
            };
            let g = py::lower(arena.attr(inst, "guid").unwrap_or(""));
            let role = roles.get(&g).cloned().unwrap_or_default();
            let list = self.cands.entry(t.to_owned()).or_default();
            list.push(Cand { part, node: inst, role });
            let idx = list.len() - 1;
            self.by_guid.entry(g).or_insert_with(|| (t.to_owned(), idx));
        }
        self.parts.push(Part { name, arena });
        Ok(())
    }

    /// `t in templates`.
    #[must_use]
    pub fn has(&self, t: &str) -> bool {
        self.cands.contains_key(t)
    }

    /// A candidate's element.
    #[must_use]
    pub fn el(&self, c: &Cand) -> Option<Tn<'_>> {
        self.parts.get(c.part).map(|p| Tn {
            ar: &p.arena,
            id: c.node,
        })
    }

    /// A candidate's partition key.
    #[must_use]
    pub fn partition(&self, c: &Cand) -> &str {
        self.parts.get(c.part).map_or("", |p| p.name.as_str())
    }

    fn ranking(&self, key: RankKey, make: impl FnOnce() -> Vec<usize>) -> Arc<Ranking> {
        let mut m = self.rankings.lock().unwrap_or_else(PoisonError::into_inner);
        if let Some(r) = m.get(&key) {
            return r.clone();
        }
        let all = self.cands.get(&key.0).map_or(&[][..], Vec::as_slice);
        let r = Arc::new(Ranking::new(all, make()));
        m.insert(key, r.clone());
        r
    }

    /// `Templates.choose`: `(step, the chosen template)` of an instance of type `t` and guid `g` (lower case),
    /// `donors` lower case; `defaults` the catalogue's class defaults.
    #[must_use]
    pub fn choose<'a>(
        &'a self,
        t: &str,
        g: &str,
        role: &Role,
        donors: &[String],
        defaults: Option<Defaults<'_, 'a>>,
    ) -> (Step, Option<Chosen<'a>>) {
        if let Some((ct, i)) = self.by_guid.get(g)
            && ct == t
            && let Some(c) = self.cands.get(ct).and_then(|l| l.get(*i))
            && let Some(el) = self.el(c)
        {
            return (
                Step::Counterpart,
                Some(Chosen {
                    partition: self.partition(c),
                    el,
                }),
            );
        }
        let all = self.cands.get(t).map_or(&[][..], Vec::as_slice);
        let pick = |i: usize| {
            let c = all.get(i)?;
            Some(Chosen {
                partition: self.partition(c),
                el: self.el(c)?,
            })
        };
        if !donors.is_empty() {
            let r = self.ranking((t.to_owned(), Some(donors.to_vec())), || {
                let mut v = Vec::new();
                for d in donors {
                    v.extend((0..all.len()).filter(|&i| all.get(i).is_some_and(|c| self.partition(c) == d)));
                }
                v
            });
            if !r.cands.is_empty() {
                return (Step::Donor, r.best(all, role).and_then(|(_, i)| pick(i)));
            }
        }
        if !all.is_empty() {
            let r = self.ranking((t.to_owned(), None), || (0..all.len()).collect());
            let Some((score, i)) = r.best(all, role) else {
                return (Step::None, None);
            };
            let step = if score > 0 && all.len() > 1 {
                Step::Role
            } else {
                Step::Pool
            };
            return (step, pick(i));
        }
        if let Some(d) = defaults.and_then(|f| f(t)) {
            return (Step::Default, Some(d));
        }
        (Step::None, None)
    }
}

/// `f.instances`: the partition's `instance` children (none when the file has no partition).
#[must_use]
pub fn partition_instances(a: &Arena) -> Vec<Id> {
    a.roots.first().map_or_else(Vec::new, |&p| {
        a.kids(p).iter().copied().filter(|&c| a.tag(c) == "instance").collect()
    })
}
