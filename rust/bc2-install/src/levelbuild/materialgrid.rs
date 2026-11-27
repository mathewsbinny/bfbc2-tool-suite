//! The WW2 levels' merged material grid (round OR4, "OPFOR in Rust" stage 4: Project OPFOR's `material_grid.py`).
//! The grid says, for every pair of materials, what happens where they meet (damage, the impact effect and sound, the
//! decal, the vehicle's dust). A WW2 level mixes both games' materials, so its grid holds Isla Inocentes' materials,
//! every BF1943 material of the level's own grid and the BFBC2 materials the level's data names; each pair's relation
//! is DICE's (any BFBC2 level grid holding the pair: BFBC2's weapons keep their balance), else BF1943's (its grid
//! ported onto BFBC2's types), else a stand-in (the nearest BFBC2 material's relation, the BF1943 material in its
//! place).
//!
//! | Project OPFOR (`material_grid.py`, Python until OR4) | here |
//! |---|---|
//! | `build_roots` | [`build_roots`] |
//! | `wake_names` | [`level_names`] |
//! | `redirect` | [`redirect`] |
//! | `Grid`, `key`, `clone` and `main`'s merge, the written grid's check | [`merge`] |
//!
//! OPFOR keeps the porting around it (the effects through the suite's `effectport`, the impact sounds through its
//! sound porter, BF1943's grid through `dbxport`) and its decisions (`STAND_IN`, `REDIRECT`, the names).

#![allow(
    clippy::indexing_slicing,
    clippy::items_after_statements,
    reason = "instance indexes found by position in the partition they index; the Python's nested helpers kept where               it has them, for review against it"
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::sync::LazyLock;

use bc2_formats::dbx::{self, Body, DbxFile, Element};
use regex::Regex;
use regex::bytes::Regex as BytesRegex;

use super::describe::ww2_guid;
use super::index::{StockIndex, live_entries, masked_names, nested_roots};
use super::{OMap, Result, bundle_read, read, rel_slash, to_xml, walk};
use crate::text::py_repr;

// ------------------------------------------------------------------------------------------------ the names

static REF: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"ref="([A-Za-z0-9_/.!\- ]+)/[0-9A-Fa-f-]{36}""#).ok());
static TOKEN: LazyLock<Option<BytesRegex>> = LazyLock::new(|| BytesRegex::new(r"[A-Za-z0-9_]{3,40}").ok());

fn refs_of(x: &str) -> impl Iterator<Item = String> + '_ {
    REF.as_ref()
        .into_iter()
        .flat_map(move |r| r.captures_iter(x))
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_lowercase()))
}

fn tokens_into(data: &[u8], out: &mut HashSet<Vec<u8>>) {
    if let Some(r) = TOKEN.as_ref() {
        out.extend(r.find_iter(data).map(|m| m.as_bytes().to_vec()));
    }
}

/// What the level's names are gathered from (OPFOR's `wake_install` bindings of the level).
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct NamesSpec {
    /// `W`: `work\ww2`.
    pub w: String,
    /// `OURS`: the WW2 builds' roots, in order.
    pub ours: Vec<String>,
    /// `SKIP`: {root: [prefixes left out]}.
    pub skip: HashMap<String, Vec<String>>,
    /// `LEVEL`: the level's partition name.
    pub level: String,
    /// `LOADED`: the bundles the level loads besides its own.
    pub loaded: Vec<String>,
    /// `MASKED` / `MASKED_BY`: the bundles whose entries nam_common's deleted markers hide, and the marker bundle.
    pub masked: Vec<String>,
    /// See `masked`.
    pub masked_by: String,
    /// `TEAMS` then `REMAP`: {partition: its replacement, or `None`: nothing}.
    pub remap: Vec<(String, Option<String>)>,
    /// `ROOT_FILES`.
    pub root_files: Vec<String>,
    /// `ROOT_FILE_DONORS`: {file: (level, bundle)}.
    pub root_file_donors: HashMap<String, (String, String)>,
    /// `ROOT_DONOR`: (level, bundle).
    pub root_donor: (String, String),
}

/// `build_roots()`: {lower partition name: path} of the WW2 builds' partitions (the later build of a name wins, in
/// the first's place), the material grid's own output and the install ports left out.
///
/// # Errors
/// (Python's `SystemExit`) a root is missing: the names would drop silently.
pub fn build_roots(spec: &NamesSpec) -> Result<OMap<String>> {
    let roots: Vec<&String> = spec
        .ours
        .iter()
        .filter(|r| !(r.starts_with("material_grid") || r.starts_with("install_ports")))
        .collect();
    let missing: Vec<&str> = roots
        .iter()
        .filter(|r| !Path::new(&super::pjoin(&spec.w, r)).is_dir())
        .map(|r| r.as_str())
        .collect();
    if !missing.is_empty() {
        return Err(format!(
            "material_grid: the WW2 builds are not all there ({} missing under {}) - run them first (rebuild_all.py)",
            missing.join(", "),
            spec.w
        ));
    }
    let mut out = OMap::new();
    for root in roots {
        let top = super::pjoin(&spec.w, root);
        let mut skip: Vec<String> = spec
            .skip
            .get(root.as_str())
            .map(|v| v.iter().map(|s| s.to_lowercase()).collect())
            .unwrap_or_default();
        skip.extend(nested_roots(root, &spec.ours).iter().map(|s| s.to_lowercase()));
        for p in walk(&top) {
            if !p.ends_with(".dbx") {
                continue;
            }
            let rel = rel_slash(&p, &top).to_lowercase();
            if !skip.iter().any(|s| rel.starts_with(s.as_str())) {
                out.insert(&rel[..rel.len() - 4], p);
            }
        }
    }
    Ok(out)
}

/// `wake_names()`: the names (byte tokens of 3 - 40 word characters) in the partitions the level reaches - the WW2
/// builds' own and the stock partitions their references lead to that no bundle the level loads has, followed
/// through the loaded ones too (their tokens not taken) - read from the build outputs and the game copy, and the
/// line saying how far the walk went.
///
/// # Errors
/// (Python's `SystemExit`) a root is missing, the level is not built, a root file is not in its donor; a bundle or
/// partition that does not read.
pub fn level_names(spec: &NamesSpec, index: &StockIndex) -> Result<(HashSet<Vec<u8>>, String)> {
    let ours = build_roots(spec)?;
    if !ours.contains(&spec.level) {
        return Err(format!(
            "material_grid: no {}.dbx among the WW2 builds - run wake_level.py first",
            spec.level
        ));
    }
    let mut stock: HashMap<String, (String, String)> = HashMap::new();
    for b in &index.bundles {
        for (low, name, _) in &b.names {
            if let Some(stem) = low.strip_suffix(".dbx") {
                stock
                    .entry(stem.to_owned())
                    .or_insert_with(|| (b.path.clone(), name.clone()));
            }
        }
    }
    let hide = masked_names(&spec.masked_by);
    let mut loaded: HashMap<String, (String, String)> = HashMap::new();
    for b in &spec.loaded {
        if Path::new(b).exists() {
            for (name, _) in live_entries(b, &spec.masked, &hide)? {
                let low = name.to_lowercase();
                if let Some(stem) = low.strip_suffix(".dbx") {
                    loaded
                        .entry(stem.to_owned())
                        .or_insert_with(|| (b.clone(), name.clone()));
                }
            }
        }
    }
    let mut remap: HashMap<String, Option<String>> = HashMap::new();
    for (k, v) in &spec.remap {
        remap.insert(k.to_lowercase(), v.as_ref().map(|v| v.to_lowercase()));
    }
    let mut tokens = HashSet::new();
    let mut seen: HashSet<String> = HashSet::new();
    let mut todo = vec![spec.level.clone()];
    for f in spec.root_files.iter().filter(|f| f.ends_with(".dbx")) {
        let (donor, b) = spec.root_file_donors.get(f).unwrap_or(&spec.root_donor);
        let want = format!("levels/{donor}/{f}").to_lowercase();
        let h = bc2_gamefs::bundles::entries_path(b).map_err(|e| format!("{b}: {e}"))?;
        let Some(hit) = h.entries.iter().find(|e| e.name.to_lowercase() == want) else {
            return Err(format!("material_grid: {want} is not in {b}"));
        };
        let data = super::bundle_one(b, &hit.name)?;
        tokens_into(&data, &mut tokens);
        todo.extend(refs_of(&to_xml(&data)?));
    }
    let mut waves = 0;
    while !todo.is_empty() {
        waves += 1;
        let mut wave: Vec<(Vec<u8>, bool)> = Vec::new();
        let mut reads: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
        for r in &todo {
            let p = match remap.get(r) {
                Some(Some(v)) => v.clone(),
                Some(None) => continue,
                None => r.clone(),
            };
            if seen.contains(&p) {
                continue;
            }
            seen.insert(p.clone());
            if let Some(path) = ours.get(&p) {
                wave.push((read(path)?, true));
            } else if let Some((b, n)) = loaded.get(&p) {
                reads.entry(b.clone()).or_default().push((n.clone(), false));
            } else if let Some((b, n)) = stock.get(&p) {
                reads.entry(b.clone()).or_default().push((n.clone(), true));
            }
        }
        for (b, items) in reads {
            let names: Vec<String> = items.iter().map(|(n, _)| n.clone()).collect();
            let got = bundle_read(&b, &names)?;
            for (n, own) in items {
                if let Some(d) = got.get(&n) {
                    wave.push((d.clone(), own));
                }
            }
        }
        todo.clear();
        for (data, own) in &wave {
            if *own {
                tokens_into(data, &mut tokens);
            }
            todo.extend(refs_of(&to_xml(data)?).filter(|r| !seen.contains(r)));
        }
    }
    let line = format!(
        "material names: {} partitions reached from the level in {waves} steps",
        seen.len()
    );
    Ok((tokens, line))
}

// ------------------------------------------------------------------------------------------------ the tree

/// An element of a partition with its strings (Python's `dbx.Element`): what moves between grids.
#[derive(Debug, Clone, PartialEq, Eq)]
struct E {
    tag: String,
    kind: u8,
    attrs: Vec<(String, String)>,
    body: B,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum B {
    Kids(Vec<E>),
    Text(String),
    Numbers(Vec<u8>),
    Scalar([u8; 2]),
}

impl E {
    fn of(f: &DbxFile, e: &Element) -> Self {
        Self {
            tag: f.str(e.tag).to_owned(),
            kind: e.kind,
            attrs: e
                .attributes
                .iter()
                .map(|(k, v)| (f.str(*k).to_owned(), f.str(*v).to_owned()))
                .collect(),
            body: match &e.body {
                Body::Container(c) => B::Kids(c.iter().map(|x| Self::of(f, x)).collect()),
                Body::Text(t) => B::Text(f.str(*t).to_owned()),
                Body::Numbers(r) => B::Numbers(r.clone()),
                Body::Scalar(r) => B::Scalar(*r),
            },
        }
    }

    fn attr(&self, name: &str) -> Option<&str> {
        self.attrs.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }

    fn set_all(&mut self, name: &str, value: &str) {
        for (k, v) in &mut self.attrs {
            if k == name {
                value.clone_into(v);
            }
        }
    }

    fn kids(&self) -> &[E] {
        match &self.body {
            B::Kids(c) => c,
            _ => &[],
        }
    }

    fn kids_mut(&mut self) -> Option<&mut Vec<E>> {
        match &mut self.body {
            B::Kids(c) => Some(c),
            _ => None,
        }
    }

    fn child(&self, name: &str) -> Option<&E> {
        self.kids().iter().find(|c| c.attr("name") == Some(name))
    }

    fn child_mut(&mut self, name: &str) -> Option<&mut E> {
        self.kids_mut()?.iter_mut().find(|c| c.attr("name") == Some(name))
    }

    fn guid_low(&self) -> String {
        self.attr("guid").unwrap_or_default().to_lowercase()
    }

    /// `dbx.Element(proto.tag, proto.kind, [(a, g if a == 'ref' else v)], proto.text, proto.raw, None)`.
    fn item_like(proto: &Self, r: &str) -> Self {
        let mut e = proto.clone();
        e.set_all("ref", r);
        if let B::Kids(_) = e.body {
            e.body = B::Kids(Vec::new());
        }
        e
    }
}

struct Strings {
    at: HashMap<String, u32>,
    list: Vec<String>,
}

impl Strings {
    fn id(&mut self, s: &str) -> u32 {
        if let Some(&i) = self.at.get(s) {
            return i;
        }
        let i = self.list.len() as u32;
        self.list.push(s.to_owned());
        self.at.insert(s.to_owned(), i);
        i
    }

    fn element(&mut self, e: &E) -> Element {
        Element {
            tag: self.id(&e.tag),
            kind: e.kind,
            attributes: e.attrs.iter().map(|(k, v)| (self.id(k), self.id(v))).collect(),
            body: match &e.body {
                B::Kids(c) => Body::Container(c.iter().map(|x| self.element(x)).collect()),
                B::Text(t) => Body::Text(self.id(t)),
                B::Numbers(r) => Body::Numbers(r.clone()),
                B::Scalar(r) => Body::Scalar(*r),
            },
        }
    }
}

fn write(roots: &[E]) -> Result<Vec<u8>> {
    let mut s = Strings {
        at: HashMap::new(),
        list: Vec::new(),
    };
    let roots: Vec<Element> = roots.iter().map(|e| s.element(e)).collect();
    let f = DbxFile { strings: s.list, roots };
    dbx::write(&f).map_err(|e| format!("ValueError: {e}"))
}

fn parse(data: &[u8]) -> Result<Vec<E>> {
    let f = dbx::parse(data).map_err(|e| format!("ValueError: {e}"))?;
    Ok(f.roots.iter().map(|e| E::of(&f, e)).collect())
}

// ------------------------------------------------------------------------------------------------ redirect

/// `redirect(data, ix)`: BF1943's grid with its references to partitions neither game ships pointed at BFBC2's
/// nearest (`primary`: [(old partition, `<new partition>/<its primary instance, lower case>`)] in `REDIRECT`'s
/// order, `named` the new partition's name for the report), and the report lines (one per partition redirected, in
/// the order first met).
///
/// # Errors
/// The grid does not read or write.
pub fn redirect(data: &[u8], primary: &[(String, String, String)]) -> Result<(Vec<u8>, Vec<String>)> {
    let mut roots = parse(data)?;
    let mut count: Vec<(String, usize)> = Vec::new();
    fn visit(e: &mut E, primary: &[(String, String, String)], count: &mut Vec<(String, usize)>) {
        for (a, v) in &mut e.attrs {
            if a != "ref" {
                continue;
            }
            let low = v.to_lowercase();
            let head = low.rsplit_once('/').map_or(low.as_str(), |(h, _)| h);
            if let Some((old, new, _)) = primary.iter().find(|(o, _, _)| o == head) {
                new.clone_into(v);
                match count.iter_mut().find(|(o, _)| o == old) {
                    Some(c) => c.1 += 1,
                    None => count.push((old.clone(), 1)),
                }
            }
        }
        if let B::Kids(c) = &mut e.body {
            for x in c {
                visit(x, primary, count);
            }
        }
    }
    if let Some(p) = roots.first_mut() {
        visit(p, primary, &mut count);
    }
    let lines = count
        .iter()
        .map(|(o, n)| {
            let named = primary
                .iter()
                .find(|(p, _, _)| p == o)
                .map_or("", |(_, _, m)| m.as_str());
            format!("redirected {o} -> {named} (neither game ships it) x{n}")
        })
        .collect();
    Ok((write(&roots)?, lines))
}

// ------------------------------------------------------------------------------------------------ the grids

type Key = (Option<String>, Option<String>);

/// `key(a, b)`: the unordered pair, sorted with `None` last.
fn key(a: Option<&str>, b: Option<&str>) -> Key {
    let (a, b) = (a.map(str::to_owned), b.map(str::to_owned));
    let rank = |x: &Option<String>| (x.is_none(), x.clone().unwrap_or_default());
    if rank(&b) < rank(&a) { (b, a) } else { (a, b) }
}

/// `repr(k)` of a key (`('Rock', 'Tank')`, `('Rock', None)`).
fn key_repr(k: &Key) -> String {
    let r = |x: &Option<String>| x.as_deref().map_or_else(|| "None".to_owned(), py_repr);
    format!("({}, {})", r(&k.0), r(&k.1))
}

/// A grid partition (`Grid`): materials by name, relations by unordered name pair (`None`: any), with their data.
struct Grid {
    name: String,
    roots: Vec<E>,
    /// lower guid -> index among the partition's children
    by: HashMap<String, usize>,
    /// name -> index of its `MaterialData` (a name given twice: the later instance, in the first's place)
    materials: Vec<(String, usize)>,
    mat_of: HashMap<String, String>,
    relations: HashMap<Key, usize>,
}

impl Grid {
    fn new(name: &str, data: &[u8]) -> Result<Self> {
        let roots = parse(data)?;
        let f_roots = roots.first().map(E::kids).unwrap_or_default();
        let mut by = HashMap::new();
        let mut materials: Vec<(String, usize)> = Vec::new();
        for (i, c) in f_roots.iter().enumerate() {
            if c.tag != "instance" {
                continue;
            }
            by.insert(c.guid_low(), i);
            if c.attr("type") == Some("Entity.MaterialData") {
                let n = c
                    .child("Name")
                    .ok_or_else(|| "AssertionError: a MaterialData without a Name".to_owned())?;
                let n = value_of(n);
                match materials.iter_mut().find(|(m, _)| *m == n) {
                    Some(slot) => slot.1 = i,
                    None => materials.push((n, i)),
                }
            }
        }
        let mut mat_of = HashMap::new();
        for (n, i) in &materials {
            mat_of.insert(f_roots[*i].guid_low(), n.clone());
        }
        let mut relations = HashMap::new();
        for (i, c) in f_roots.iter().enumerate() {
            if c.tag != "instance" || c.attr("type") != Some("Entity.MaterialRelationData") {
                continue;
            }
            let (Some(m1), Some(m2)) = (c.child("Material1"), c.child("Material2")) else {
                return Err("AssertionError: a MaterialRelationData without its two materials".to_owned());
            };
            let a = mat_of.get(&m1.attr("ref").unwrap_or_default().to_lowercase());
            let b = mat_of.get(&m2.attr("ref").unwrap_or_default().to_lowercase());
            relations
                .entry(key(a.map(String::as_str), b.map(String::as_str)))
                .or_insert(i);
        }
        Ok(Self {
            name: name.to_owned(),
            roots,
            by,
            materials,
            mat_of,
            relations,
        })
    }

    fn inst(&self, i: usize) -> &E {
        &self.roots[0].kids()[i]
    }

    fn material(&self, n: &str) -> Option<usize> {
        self.materials.iter().find(|(m, _)| m == n).map(|(_, i)| *i)
    }

    /// `data_of(rel)`: the instances the relation's `Data` items name, in order.
    fn data_of(&self, rel: &E) -> Vec<usize> {
        rel.child("Data")
            .map(E::kids)
            .unwrap_or_default()
            .iter()
            .filter_map(|c| self.by.get(&c.attr("ref").unwrap_or_default().to_lowercase()).copied())
            .collect()
    }
}

/// `dbx.value(e)` of a material's `Name` field: its text (a name is a text field).
fn value_of(e: &E) -> String {
    match &e.body {
        B::Text(t) => t.clone(),
        _ => String::new(),
    }
}

/// `clone(g, inst, tag, new, memo)`: a copy of `inst` under a guid of its own (from `tag`, the relation it serves),
/// every instance of `g` it references inside the partition copied the same way first; its guid (lower case).
fn clone(g: &Grid, i: usize, tag: &str, new: &mut Vec<E>, memo: &mut HashMap<String, String>) -> String {
    let inst = g.inst(i);
    let old = inst.guid_low();
    if let Some(m) = memo.get(&old) {
        return m.clone();
    }
    let gid = mg_guid(&format!("{tag}:{}", inst.attr("guid").unwrap_or_default())).to_lowercase();
    memo.insert(old, gid.clone());
    let mut dc = inst.clone();
    dc.set_all("guid", &gid.to_uppercase());
    fn fix(e: &mut E, g: &Grid, tag: &str, new: &mut Vec<E>, memo: &mut HashMap<String, String>) {
        let Some(kids) = e.kids_mut() else { return };
        for ch in kids {
            if let Some(r) = ch.attr("ref").map(str::to_owned)
                && !r.is_empty()
                && !r.contains('/')
                && let Some(&t) = g.by.get(&r.to_lowercase())
            {
                let target = clone(g, t, tag, new, memo);
                ch.set_all("ref", &target);
            }
            fix(ch, g, tag, new, memo);
        }
    }
    fix(&mut dc, g, tag, new, memo);
    new.push(dc);
    gid
}

fn mg_guid(tag: &str) -> String {
    ww2_guid(&["material-grid", tag])
}

/// What the merge is made of: the grids (OPFOR's bindings).
#[derive(Debug, Clone)]
pub struct MergeSpec<'a> {
    /// `BASE` (Isla Inocentes' grid) and its bytes.
    pub base: (&'a str, &'a [u8]),
    /// Every BFBC2 level grid, by name.
    pub others: Vec<(&'a str, &'a [u8])>,
    /// `GRID` and BF1943's grid ported onto BFBC2's types.
    pub bf: (&'a str, &'a [u8]),
    /// The names the level's data holds ([`level_names`]).
    pub names_in: &'a HashSet<Vec<u8>>,
    /// `STAND_IN`: (a material only BF1943 has, the BFBC2 material whose relations it takes), in order.
    pub stand_in: &'a [(String, String)],
    /// `CLONE`: the name of the level's grid clone (its guids are the ones the level names).
    pub clone: &'a str,
}

/// The merged grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Merged {
    /// The partition.
    pub data: Vec<u8>,
    /// The report lines (materials, relations, the grid's guids).
    pub report: Vec<String>,
    /// The instances written.
    pub instances: usize,
}

/// `material_grid.main`'s merge: the grid partition under the clone's name and guids, with Isla Inocentes' materials
/// (DICE's order kept), BF1943's and the BFBC2 materials the level names, and each pair's relation - DICE's, BF1943's
/// or a stand-in - with its data copied under guids of its own; read back and checked (no reference to an instance
/// it lacks, no guid twice).
///
/// # Errors
/// A grid does not read or lacks what every grid has; (Python's `SystemExit`) the written grid fails its check.
#[expect(
    clippy::too_many_lines,
    reason = "material_grid.main's merge, kept in one piece for review"
)]
pub fn merge(spec: &MergeSpec<'_>) -> Result<Merged> {
    let base = Grid::new(spec.base.0, spec.base.1)?;
    let others = spec
        .others
        .iter()
        .map(|(n, d)| Grid::new(n, d))
        .collect::<Result<Vec<_>>>()?;
    let bf = Grid::new(spec.bf.0, spec.bf.1)?;
    let dice: Vec<&Grid> = std::iter::once(&base).chain(others.iter()).collect();
    let mut report = Vec::new();
    // the materials: Isla Inocentes', BF1943's, and the BFBC2 materials the level's data names
    let mut bc2_materials: HashMap<String, (usize, usize)> = HashMap::new();
    for (gi, g) in dice.iter().enumerate() {
        for (n, i) in &g.materials {
            bc2_materials.entry(n.clone()).or_insert((gi, *i));
        }
    }
    let mut names: HashSet<String> = base.materials.iter().map(|(n, _)| n.clone()).collect();
    names.extend(bf.materials.iter().map(|(n, _)| n.clone()));
    #[allow(clippy::iter_over_hash_type, reason = "a set is built: the order cannot matter")]
    for n in bc2_materials.keys() {
        if spec.names_in.contains(n.as_bytes()) {
            names.insert(n.clone());
        }
    }
    let base_grid_i = base
        .roots
        .first()
        .map(E::kids)
        .unwrap_or_default()
        .iter()
        .position(|c| c.tag == "instance" && c.attr("type") == Some("Entity.MaterialGridData"))
        .ok_or_else(|| "StopIteration: no MaterialGridData".to_owned())?;
    let base_grid = base.inst(base_grid_i);
    let bm = base_grid
        .child("Materials")
        .filter(|m| matches!(m.body, B::Kids(_)))
        .ok_or_else(|| "AssertionError: the grid has no Materials".to_owned())?;
    let mut order: Vec<String> = Vec::new();
    for c in bm.kids() {
        let r = c.attr("ref").unwrap_or_default().to_lowercase();
        let n = base.mat_of.get(&r).ok_or_else(|| format!("KeyError: '{r}'"))?;
        order.push(n.clone());
    }
    let in_order: HashSet<&String> = order.iter().collect();
    let mut rest: Vec<String> = names.iter().filter(|n| !in_order.contains(n)).cloned().collect();
    rest.sort_unstable();
    order.extend(rest);
    let base_names: HashSet<&String> = base.materials.iter().map(|(n, _)| n).collect();
    let bf_names: HashSet<&String> = bf.materials.iter().map(|(n, _)| n).collect();
    let others_n = names
        .iter()
        .filter(|n| !base_names.contains(n) && !bf_names.contains(n))
        .count();
    report.push(format!(
        "materials: {} (Isla Inocentes {}, BF1943 Wake {}, BFBC2 others Wake names {others_n})",
        names.len(),
        base.materials.len(),
        bf.materials.len()
    ));
    // each pair's relation: DICE's, BF1943's, a stand-in
    #[derive(Clone)]
    struct Hit {
        grid: usize, // index into dice, or usize::MAX for bf
        rel: usize,
        sub: Option<(String, String)>,
    }
    let in_names = |x: &Option<String>| x.as_ref().is_none_or(|v| names.contains(v));
    let mut pairs: HashSet<Key> = HashSet::new();
    for g in dice.iter().copied().chain(std::iter::once(&bf)) {
        #[allow(clippy::iter_over_hash_type, reason = "a set is built, then sorted")]
        for k in g.relations.keys() {
            if in_names(&k.0) && in_names(&k.1) {
                pairs.insert(k.clone());
            }
        }
    }
    let sort_key = |k: &Key| (k.0.clone().unwrap_or_default(), k.1.clone().unwrap_or_default());
    let mut pairs: Vec<Key> = pairs.into_iter().collect();
    pairs.sort_by_key(|k| sort_key(k));
    let mut chosen: Vec<(Key, Hit)> = Vec::new();
    let mut at: HashMap<Key, usize> = HashMap::new();
    let mut source: Vec<(&str, usize)> = Vec::new();
    let count = |source: &mut Vec<(&str, usize)>, s: &'static str| match source.iter_mut().find(|(n, _)| *n == s) {
        Some(c) => c.1 += 1,
        None => source.push((s, 1)),
    };
    for k in pairs {
        let hit = dice
            .iter()
            .enumerate()
            .find_map(|(gi, g)| g.relations.get(&k).map(|&r| (gi, r)));
        if let Some((gi, r)) = hit {
            at.insert(k.clone(), chosen.len());
            chosen.push((
                k,
                Hit {
                    grid: gi,
                    rel: r,
                    sub: None,
                },
            ));
            count(&mut source, "DICE (a BFBC2 grid)");
        } else if let Some(&r) = bf.relations.get(&k) {
            at.insert(k.clone(), chosen.len());
            chosen.push((
                k,
                Hit {
                    grid: usize::MAX,
                    rel: r,
                    sub: None,
                },
            ));
            count(&mut source, "BF1943");
        }
    }
    // stand-ins: a BF1943-only material against BFBC2 materials it has no relation with
    for (m, s) in spec.stand_in {
        if !names.contains(m) {
            continue;
        }
        let snapshot: Vec<(Key, Hit)> = chosen.clone();
        for (k, hit) in snapshot {
            if hit.sub.is_some() || !(k.0.as_deref() == Some(s.as_str()) || k.1.as_deref() == Some(s.as_str())) {
                continue;
            }
            let other = if k.0.as_deref() == Some(s.as_str()) {
                k.1.clone()
            } else {
                k.0.clone()
            };
            let nk = key(Some(m), other.as_deref());
            if !at.contains_key(&nk) && in_names(&other) {
                at.insert(nk.clone(), chosen.len());
                chosen.push((
                    nk,
                    Hit {
                        sub: Some((m.clone(), s.clone())),
                        ..hit
                    },
                ));
                count(&mut source, "stand-in");
            }
        }
    }
    let parts: Vec<String> = source.iter().map(|(s, n)| format!("{n} from {s}")).collect();
    report.push(format!("relations: {} - {}", chosen.len(), parts.join(", ")));
    // the partition: Isla Inocentes' grid under the clone's name with the guids the level names it by, every instance
    // replaced; a material both games have keeps DICE's BFBC2 settings
    let mut part = base.roots[0].clone();
    let grid_guid = base_grid.attr("guid").unwrap_or_default().to_owned();
    let part_guid = part.attr("guid").unwrap_or_default().to_owned();
    let swap: Vec<(String, String)> = vec![
        (
            part_guid.to_lowercase(),
            ww2_guid(&[spec.clone, &part_guid.to_uppercase()]),
        ),
        (
            grid_guid.to_lowercase(),
            ww2_guid(&[spec.clone, &grid_guid.to_uppercase()]),
        ),
    ];
    let swapped = |v: &str| {
        swap.iter()
            .rev()
            .find(|(o, _)| *o == v.to_lowercase())
            .map_or_else(|| v.to_owned(), |(_, n)| n.clone())
    };
    for (_, v) in &mut part.attrs {
        *v = swapped(v);
    }
    part.set_all("name", spec.clone);
    let mut grid_inst = base_grid.clone();
    for (a, v) in &mut grid_inst.attrs {
        if a == "guid" {
            *v = swapped(v);
        }
    }
    let nm = grid_inst
        .child_mut("Name")
        .ok_or_else(|| "AssertionError: the grid has no Name".to_owned())?;
    if let B::Text(t) = &mut nm.body {
        spec.clone.clone_into(t);
    }
    let attrs: Vec<String> = part
        .attrs
        .iter()
        .map(|(a, v)| format!("({}, {})", py_repr(a), py_repr(v)))
        .collect();
    report.push(format!(
        "grid guid {} (the clone guids), partition [{}]",
        grid_inst.attr("guid").unwrap_or_default(),
        attrs.join(", ")
    ));
    let grid_of = |h: &Hit| -> &Grid { if h.grid == usize::MAX { &bf } else { dice[h.grid] } };
    let mut new: Vec<E> = Vec::new();
    let mut mat_guid: HashMap<String, String> = HashMap::new();
    for n in &order {
        let inst = if let Some(i) = base.material(n) {
            base.inst(i).clone()
        } else if let Some(i) = bf.material(n) {
            bf.inst(i).clone()
        } else {
            let (gi, i) = bc2_materials
                .get(n)
                .ok_or_else(|| format!("KeyError: {}", py_repr(n)))?;
            dice[*gi].inst(*i).clone()
        };
        mat_guid.insert(n.clone(), inst.guid_low());
        new.push(inst);
    }
    let mut chosen_sorted: Vec<&(Key, Hit)> = chosen.iter().collect();
    chosen_sorted.sort_by_key(|(k, _)| sort_key(k));
    let mut rel_items = Vec::new();
    for (k, hit) in chosen_sorted {
        let g = grid_of(hit);
        let rel = g.inst(hit.rel);
        let tag = format!("{}:{}:{}", g.name, rel.attr("guid").unwrap_or_default(), key_repr(k));
        let mut r = rel.clone();
        let rg = mg_guid(&tag);
        r.set_all("guid", &rg);
        for side in ["Material1", "Material2"] {
            let Some(c) = r.child_mut(side) else {
                return Err(format!("AttributeError: no {side}"));
            };
            let mut name = g.mat_of.get(&c.attr("ref").unwrap_or_default().to_lowercase()).cloned();
            if let Some((m, s)) = &hit.sub
                && name.as_deref() == Some(s.as_str())
            {
                name = Some(m.clone());
            }
            let value = match name.filter(|n| !n.is_empty()) {
                Some(n) => mat_guid
                    .get(&n)
                    .cloned()
                    .ok_or_else(|| format!("KeyError: {}", py_repr(&n)))?,
                None => "null".to_owned(),
            };
            c.set_all("ref", &value);
        }
        let items: Vec<String> = g
            .data_of(rel)
            .into_iter()
            .map(|d| clone(g, d, &tag, &mut new, &mut HashMap::new()))
            .collect();
        let arr = r
            .child_mut("Data")
            .ok_or_else(|| "AttributeError: 'NoneType' object has no attribute 'children'".to_owned())?;
        let proto = arr.kids().first().cloned();
        let kids = proto.map_or_else(Vec::new, |p| items.iter().map(|gg| E::item_like(&p, gg)).collect());
        arr.body = B::Kids(kids);
        rel_items.push(rg.to_lowercase());
        new.push(r);
    }
    for (field, guids) in [
        (
            "Materials",
            order.iter().map(|n| mat_guid[n].clone()).collect::<Vec<_>>(),
        ),
        ("MaterialRelations", rel_items),
    ] {
        let arr = grid_inst
            .child_mut(field)
            .ok_or_else(|| format!("AttributeError: the grid has no {field}"))?;
        let proto = arr
            .kids()
            .first()
            .cloned()
            .ok_or_else(|| "IndexError: list index out of range".to_owned())?;
        arr.body = B::Kids(guids.iter().map(|g| E::item_like(&proto, g)).collect());
    }
    let kids = part
        .kids_mut()
        .ok_or_else(|| "the partition has no children".to_owned())?;
    kids.retain(|c| c.tag != "instance");
    kids.push(grid_inst);
    kids.extend(new);
    let data = write(
        &[part]
            .into_iter()
            .chain(base.roots.iter().skip(1).cloned())
            .collect::<Vec<_>>(),
    )?;
    // read back: every reference inside the partition resolves, no guid twice
    let back = parse(&data)?;
    let insts: Vec<&E> = back
        .first()
        .map(E::kids)
        .unwrap_or_default()
        .iter()
        .filter(|c| c.tag == "instance")
        .collect();
    let have: HashSet<String> = insts.iter().map(|i| i.guid_low()).collect();
    let mut dangling = 0usize;
    fn refs_in(e: &E, have: &HashSet<String>, n: &mut usize) {
        for ch in e.kids() {
            if let Some(r) = ch.attr("ref")
                && !r.is_empty()
                && r != "null"
                && !r.contains('/')
                && !have.contains(&r.to_lowercase())
            {
                *n += 1;
            }
            refs_in(ch, have, n);
        }
    }
    for i in &insts {
        refs_in(i, &have, &mut dangling);
    }
    if dangling > 0 || have.len() != insts.len() {
        return Err(format!(
            "grid not written right: {dangling} references to instances it lacks, {} duplicate guids",
            insts.len() - have.len()
        ));
    }
    Ok(Merged {
        data,
        report,
        instances: insts.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_sort_none_last() {
        assert_eq!(key(Some("b"), Some("a")), (Some("a".into()), Some("b".into())));
        assert_eq!(key(None, Some("a")), (Some("a".into()), None));
        assert_eq!(key_repr(&key(None, Some("Rock"))), "('Rock', None)");
    }
}
