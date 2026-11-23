//! `bfbc2-bf1943-import/dbxparity.py` in Rust (round PR1b-1d): BF1943's data partitions against DICE's own versions,
//! field by field (`compare`), and `--predict` (dbxport's value passes and DICE's conventions scored on fair pools).
//!
//! The same report and enum JSON as the Python wrote, byte for byte on the user's BF1943 and game copy (rust/
//! BENCHMARKS.md "PR1b-1d"): every instance flattened as `flatten` flattens it (`dbx.value` of each field, stripped
//! as `str.strip` strips), the classes counted and their examples kept in the order the Python's dicts and Counters
//! keep them. `--predict` ports with the Rust `dbxport` (`bc2_convert::dbxport::port`, the port the Python called
//! through `bc2_native`) on templates read from the game copy, with the catalogue `dbxcatalogue.load` gives (its
//! cache, `bc2_convert::dbxcatalogue::cache`, round PR1b-1e; built in Rust when the game copy changed: every table
//! equal to the Python's, in order) and the class defaults swapped per partition (`fair_catalogue`).

#![allow(
    clippy::too_many_lines,
    clippy::format_push_string,
    clippy::map_unwrap_or,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::many_single_char_names,
    reason = "dbxparity.py's functions step for step: its names, its report built line by line, Python's \
              case-sensitive endswith('.dbx')"
)]

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use bc2_convert::dbxport::{self, Arena, Bones, CatView, Catalogue, Fail as PortFail, Id, Options, Ours, Templates};
use bc2_convert::meshview::Index;
use bc2_convert::pyfmt::OrderedMap;
use bc2_formats::dbx::{self, CONTAINER, DbxFile, Element};
use regex::Regex;

use super::{Run, py_dict, write_report};
use crate::bf1943src::Source;
use crate::pyfmt::str_repr;
use crate::pyproc::{Fail, Py, PyErr, PyResult};

/// A field's kind: an element kind's number, `ref` or `len`.
type Kind = String;
/// An instance's fields: path -> (kind, value), in `flatten`'s order.
type Fields = OrderedMap<String, (Kind, String)>;
/// `instances(data)`: upper-case guid -> (type, fields).
type Instances = OrderedMap<String, (String, Fields)>;
/// A `Counter` of strings, in insertion order.
type Counter = OrderedMap<String, i64>;

static INDEX: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"\[\d+\]").ok());
static INT: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^-?\d+$").ok());
static NAME: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").ok());
static UPTO_DOT: LazyLock<Option<Regex>> = LazyLock::new(|| Regex::new(r"(?-s).*\.").ok());

fn is(r: &LazyLock<Option<Regex>>, s: &str) -> bool {
    r.as_ref().is_some_and(|r| r.is_match(s))
}

/// `re.sub(r'\[\d+\]', '[]', p)`.
fn generic(p: &str) -> String {
    INDEX
        .as_ref()
        .map_or_else(|| p.to_owned(), |r| r.replace_all(p, "[]").into_owned())
}

/// `str.strip()`: Python's whitespace (`str.isspace`: Unicode's, and the separators `\x1c` - `\x1f`).
fn strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\x1c'..='\x1f').contains(&c))
}

fn bump(c: &mut Counter, k: &str, by: i64) {
    *c.entry_default(k.to_owned()) += by;
}

fn count(c: &Counter, k: &str) -> i64 {
    c.get(&k.to_owned()).copied().unwrap_or(0)
}

/// `flatten(e, path, out)`.
fn flatten(f: &DbxFile, e: &Element, path: &str, out: &mut Fields) {
    let kids = e.children();
    let tag = f.tag(e);
    if e.kind == CONTAINER || (matches!(tag, "array" | "complex" | "instance") && !kids.is_empty()) {
        for (i, c) in kids.iter().enumerate() {
            let sub = match f.attr(c, "name") {
                Some(n) => format!("{path}.{n}"),
                None => format!("{path}[{i}]"),
            };
            flatten(f, c, &sub, out);
        }
        if tag == "array" {
            out.insert(format!("{path}#len"), ("len".to_owned(), kids.len().to_string()));
        }
        return;
    }
    if let Some(r) = f.attr(e, "ref") {
        let v = if r == "null" {
            "null".to_owned()
        } else {
            r.rsplit('/').next().unwrap_or("").to_lowercase()
        };
        out.insert(path.to_owned(), ("ref".to_owned(), v));
        return;
    }
    let v = dbx::value(f, e).map_or_else(|_| "?".to_owned(), |v| strip(&v).to_owned());
    out.insert(path.to_owned(), (e.kind.to_string(), v));
}

/// `instances(data)`: Err the message `dbx.parse` (or the tree's partition) raises.
fn instances(data: Option<&[u8]>) -> Result<Instances, String> {
    let data = data.unwrap_or_default();
    let f = dbx::parse(data).map_err(|e| e.to_string())?;
    let Some(p) = f.roots.first() else {
        return Err("list index out of range".to_owned());
    };
    if p.kind != CONTAINER {
        return Err("'NoneType' object is not iterable".to_owned());
    }
    let mut out = Instances::new();
    for inst in f.instances() {
        let mut fields = Fields::new();
        flatten(&f, inst, "", &mut fields);
        let g = f.attr(inst, "guid").unwrap_or("").to_uppercase();
        out.insert(g, (f.attr(inst, "type").unwrap_or("").to_owned(), fields));
    }
    Ok(out)
}

/// `compare`'s tables.
#[derive(Default)]
struct Compared {
    partitions: usize,
    matched: usize,
    stats: OrderedMap<(String, String), Counter>,
    examples: OrderedMap<(String, String), Vec<String>>,
    enums: OrderedMap<(String, String), OrderedMap<(String, String), i64>>,
}

impl Compared {
    fn example_count(&self, key: &(String, String)) -> usize {
        self.examples.get(key).map_or(0, Vec::len)
    }

    fn example(&mut self, key: &(String, String), text: String) {
        self.examples.entry_default(key.clone()).push(text);
    }
}

/// The first `n` characters of `s` (`s[:n]`).
fn head(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

/// `compare(src, ix, names)`; `log` prints a partition left out.
fn compare(py: &mut Py, src: &Source, ix: &Index, names: &[String]) -> PyResult<Compared> {
    let mut c = Compared::default();
    let dice = ix.read_many(names);
    for k in names {
        let a = instances(src.data(k).as_deref());
        let b = match dice.get(k) {
            Some(d) => instances(d.as_deref()),
            None => instances(ix.read(k).as_deref()),
        };
        let (a, b) = match (a, b) {
            (Ok(a), Ok(b)) => (a, b),
            (Err(e), _) | (_, Err(e)) => {
                py.print(&format!("  {k}: not compared ({e})"))?;
                continue;
            }
        };
        c.partitions += 1;
        for (g, (t, fa)) in a.iter() {
            let Some((tb, fb)) = b.get(g) else { continue };
            if tb != t {
                continue;
            }
            c.matched += 1;
            let tshort = t.rsplit('.').next().unwrap_or("").to_owned();
            for (p, (ka, va)) in fa.iter() {
                let Some((kb, vb)) = fb.get(p) else { continue };
                let key = (tshort.clone(), generic(p));
                if ka == kb && va == vb {
                    bump(c.stats.entry_default(key), "same", 1);
                } else if is(&INT, va) && is(&NAME, vb) && !matches!(vb.as_str(), "true" | "false" | "null") {
                    bump(c.stats.entry_default(key.clone()), "enum", 1);
                    *c.enums.entry_default(key).entry_default((va.clone(), vb.clone())) += 1;
                } else if ka != kb {
                    bump(c.stats.entry_default(key.clone()), "kind", 1);
                    if c.example_count(&key) < 3 {
                        c.example(&key, format!("{k}: {ka}:{} / DICE {kb}:{}", head(va, 30), head(vb, 30)));
                    }
                } else {
                    bump(c.stats.entry_default(key.clone()), "value", 1);
                    if c.example_count(&key) < 3 {
                        c.example(&key, format!("{k}: {} / DICE {}", head(va, 30), head(vb, 30)));
                    }
                }
            }
            // DICE-only fields: by value, and by their last name
            let mut dice_values: OrderedMap<String, Vec<String>> = OrderedMap::new();
            let mut deeper: OrderedMap<String, Vec<String>> = OrderedMap::new();
            for (p, (_, vb)) in fb.iter() {
                if fa.contains_key(p) {
                    continue;
                }
                dice_values.entry_default(vb.clone()).push(p.clone());
                deeper
                    .entry_default(p.rsplit('.').next().unwrap_or("").to_owned())
                    .push(p.clone());
            }
            for (p, (_, va)) in fa.iter() {
                if fb.contains_key(p) {
                    continue;
                }
                let key = (tshort.clone(), generic(p));
                let (head_, leaf) = match p.rfind('.') {
                    Some(i) => (p.get(..i).unwrap_or(""), p.get(i + 1..).unwrap_or("")),
                    None => ("", p.as_str()),
                };
                let dots = p.matches('.').count();
                let prefix = format!("{head_}.");
                let nest: Vec<&String> = deeper
                    .get(&leaf.to_owned())
                    .map(|v| {
                        v.iter()
                            .filter(|q| q.starts_with(&prefix) && q.matches('.').count() > dots)
                            .collect()
                    })
                    .unwrap_or_default();
                let hits: &[String] = dice_values.get(va).map_or(&[], Vec::as_slice);
                if let Some(first) = nest.first() {
                    bump(c.stats.entry_default(key.clone()), "nested", 1);
                    let ex = format!("-> {first}");
                    if !c.examples.get(&key).is_some_and(|v| v.contains(&ex)) {
                        c.example(&key, ex);
                    }
                } else if !hits.is_empty() && !matches!(va.as_str(), "0" | "false" | "null" | "" | "1" | "true") {
                    bump(c.stats.entry_default(key.clone()), "renamed", 1);
                    let short: Vec<String> = hits
                        .iter()
                        .take(3)
                        .map(|h| {
                            UPTO_DOT
                                .as_ref()
                                .map_or_else(|| h.clone(), |r| r.replace_all(h, "").into_owned())
                        })
                        .collect();
                    let ex = format!("-> {}", short.join(", "));
                    if !c.examples.get(&key).is_some_and(|v| v.contains(&ex)) {
                        c.example(&key, ex);
                    }
                } else {
                    bump(c.stats.entry_default(key), "bf1943-only", 1);
                }
            }
            for (p, _) in fb.iter() {
                if !fa.contains_key(p) {
                    bump(c.stats.entry_default((tshort.clone(), generic(p))), "dice-only", 1);
                }
            }
        }
    }
    Ok(c)
}

/// The compare run's report, enum JSON and summary line.
fn compare_report(py: &mut Py, run: &Run, c: &Compared) -> PyResult<()> {
    let mut classes = Counter::new();
    for (_, s) in c.stats.iter() {
        for (k, v) in s.iter() {
            bump(&mut classes, k, *v);
        }
    }
    let totals = py_dict(classes.iter().map(|(k, v)| (str_repr(k), v.to_string())));
    let mut out = format!(
        "{} partitions in both games, {} instances matched by GUID and type; field comparisons: {totals}\n\n",
        c.partitions, c.matched
    );
    for cls in ["enum", "kind", "nested", "renamed", "value", "bf1943-only", "dice-only"] {
        let mut rows: Vec<(&(String, String), i64)> = c
            .stats
            .iter()
            .map(|(k, s)| (k, count(s, cls)))
            .filter(|(_, n)| *n != 0)
            .collect();
        rows.sort_by_key(|(_, n)| -n);
        out.push_str(&format!("== {cls}: {} fields\n", rows.len()));
        for (key, n) in rows {
            let (t, p) = key;
            let mut extra = String::new();
            if cls == "enum" {
                let mut pairs: Vec<&(String, String)> =
                    c.enums.get(key).map(|e| e.keys().collect()).unwrap_or_default();
                pairs.sort();
                let shown: Vec<String> = pairs.iter().map(|(a, b)| format!("{a}={b}")).collect();
                extra = format!(" {}", shown.join(", "));
            } else if let Some(ex) = c.examples.get(key).filter(|e| !e.is_empty())
                && matches!(cls, "kind" | "nested" | "renamed" | "value")
            {
                let two: Vec<&str> = ex.iter().take(2).map(String::as_str).collect();
                extra = format!(" | {}", two.join(" ; "));
            }
            out.push_str(&format!("  {n:5} {t}{p}{extra}\n"));
        }
        out.push('\n');
    }
    write_report(&run.o, &out, true)?;
    // the enum pairings: {type+field: {number: name}} from the sorted pairs (a number's last name kept)
    let mut root: OrderedMap<String, OrderedMap<String, String>> = OrderedMap::new();
    for ((t, p), e) in c.enums.iter() {
        let mut pairs: Vec<&(String, String)> = e.keys().collect();
        pairs.sort();
        let mut m: OrderedMap<String, String> = OrderedMap::new();
        for (n, v) in pairs {
            m.insert(n.clone(), v.clone());
        }
        root.insert(format!("{t}{p}"), m);
    }
    let json = bc2_pyrt::json::Value::dict(root.iter().map(|(k, m)| {
        (
            k.clone(),
            bc2_pyrt::json::Value::dict(
                m.iter()
                    .map(|(n, v)| (n.clone(), bc2_pyrt::json::Value::Str(v.clone()))),
            ),
        )
    }));
    let path = format!("{}_enums.json", bc2_pyrt::py::splitext(&run.o).0);
    write_report(&path, &bc2_pyrt::json::dumps(&json, Some(1)), false)?;
    py.print(&format!(
        "{} partitions, {} instances; {totals} -> {}",
        c.partitions, c.matched, run.o
    ))?;
    Ok(())
}

// ------------------------------------------------------------------------------------------------ --predict

const V: &str = "objects/vehicles/beach";
const VIET: &str = "objects/vehicles/vietnam";
const M24: &str = "objects/weapons/handheld/us_sni_m24";

/// `FAIR_POOLS[family]`: the BFBC2 donors of the kind the callers use.
fn fair_pools(family: &str) -> Vec<String> {
    let common = vec![
        format!("{V}/kurogane/kurogane_wreck"),
        format!("{V}/kurogane/kurogane_mesh_mesh"),
        format!("{V}/type97-chiha/chiha_mb_mesh"),
        "systems/mesh/defaultmeshlodsettings".to_owned(),
        "objects/weapons/handheld/beach/thompson/thompson_lodsettings".to_owned(),
        "objects/vehicles/sea/lcrs/lcrs".to_owned(),
        "objects/vehicles/air/ah60/ah60".to_owned(),
    ];
    let jeep = vec![
        format!("{VIET}/m151a1/m151a1"),
        format!("{V}/kurogane/kurogane"),
        "objects/vehicles/land/humvee/humvee".to_owned(),
    ];
    let plane = vec![
        format!("{V}/us_corsair/corsair"),
        format!("{V}/jap_zero_fighter/zero_fighter"),
    ];
    let cat = |parts: &[Vec<String>]| parts.concat();
    match family {
        "objects" => [
            "objects/objects/boat_house/boat_house",
            "objects/objects/boat_house/boat_house_mesh",
            "objects/objects/boat_house/boat_house_havok",
            "objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01",
            "objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01_mesh",
            "objects/street_objects/rowing_boat_t01v01/rowing_boat_t01v01_havok",
            "systems/mesh/defaultmeshlodsettings",
            "objects/vehicles/beach/kurogane/kurogane_wreck",
            "objects/buildings/barrack/barrack",
            "objects/buildings/barrack/barrack_mesh",
            "objects/buildings/barrack/barrack_havok",
            "objects/buildings/house_06_8x8/house_06_8x8",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect(),
        "zero" => cat(&[
            vec![
                format!("{V}/jap_zero_fighter/zero_fighter"),
                format!("{V}/jap_zero_fighter/zero_firing"),
                format!("{V}/jap_zero_fighter/zero_bombs_firing"),
            ],
            plane,
            common,
        ]),
        "corsair" => cat(&[
            vec![
                format!("{V}/us_corsair/corsair"),
                format!("{V}/us_corsair/corsair_firing"),
                format!("{V}/us_corsair/corsair_bombs_firing"),
            ],
            plane,
            common,
        ]),
        "kurogane" => cat(&[jeep, common]),
        "chiha" => cat(&[
            vec![
                format!("{V}/type97-chiha/type97-chiha"),
                format!("{VIET}/m48pattontank/m48"),
            ],
            jeep,
            common,
        ]),
        "bofors" => cat(&[
            vec![
                format!("{V}/stationary_aa_bofors/aa_bofors"),
                format!("{V}/stationary_aa_bofors/aa_bofors_firing"),
            ],
            common,
        ]),
        "vehicles" => cat(&[vec![format!("{V}/stationary_hmg_30cal/30cal_window")], common]),
        "weapons" => vec![
            format!("{M24}/us_sni_m24"),
            format!("{M24}/us_sni_m24_firing"),
            format!("{M24}/us_sni_m24_mesh"),
            "objects/weapons/handheld/beach/thompson/thompson_lodsettings".to_owned(),
            "systems/mesh/defaultmeshlodsettings".to_owned(),
        ],
        _ => Vec::new(),
    }
}

/// `family(k)`.
fn family(k: &str) -> &'static str {
    if k.starts_with("objects/weapons/") {
        return "weapons";
    }
    if k.starts_with("objects/vehicles/") {
        return ["zero", "corsair", "kurogane", "chiha", "bofors"]
            .into_iter()
            .find(|v| k.contains(v))
            .unwrap_or("vehicles");
    }
    if k.starts_with("objects/") {
        return "objects";
    }
    if k.starts_with("effects/") || k.starts_with("fx/") || k.starts_with("emitters/") {
        return "effects";
    }
    if k.starts_with("sound/") { "sounds" } else { "other" }
}

/// `fair_pool(k)`: its family's pool without the partition itself.
fn fair_pool(k: &str) -> Vec<String> {
    let name = k.strip_suffix(".dbx").unwrap_or(k);
    fair_pools(family(k)).into_iter().filter(|p| p != name).collect()
}

/// `dbxport.NOT_IN_BFBC2`.
const NOT_IN_BFBC2: [&str; 5] = [
    "GameSharedResources.AirRaidEffectsData",
    "GameSharedResources.TeamVehicleProjectileEntityData",
    "GameSharedResources.CarHornComponentData",
    "GameSharedResources.ScorePartComponentData",
    "GameSharedResources.HudVehicleTrackerData",
];

/// `PREDICTED`: (kind, type or '' for any, field path pattern).
static PREDICTED: LazyLock<Vec<(&'static str, &'static str, Option<Regex>)>> = LazyLock::new(|| {
    [
        ("rule", "BulletEntityData", r"\.Gravity$"),
        ("rule", "GrenadeEntityData", r"\.Gravity$"),
        ("rule", "FiringFunctionData", r"\.Shot\.InitialSpeed$"),
        ("rule", "WeaponFiringData", r"\.UseAutoAiming$"),
        ("rule", "ZoomLevelData", r"\.SprintLookSpeedMultiplier$"),
        ("rule", "BoneFakePhysicsData", r"\.BoneId$"),
        ("rule", "", r"(InputAction|ActionToSuppress|HintAction\d)$"),
        ("template", "SoldierAimingSimulationData", r"\.ZoomType$"),
        ("template", "SoldierWeaponData", r"\.AnimBaseSet$"),
        (
            "template",
            "SoldierWeaponData",
            r"\.WeaponStates\[\d+\]\.AlwaysAimHead$",
        ),
        (
            "template",
            "FiringFunctionData",
            r"\.Ammo\.(NumberOfMagazines|AmmoPickupMinAmount|AmmoPickupMaxAmount)$",
        ),
        ("template", "FiringFunctionData", r"\.FireLogic\.Recoil\.\w+$"),
    ]
    .into_iter()
    .map(|(k, t, p)| (k, t, Regex::new(p).ok()))
    .collect()
});

/// A DBX tree node copied into `ar` as `Arena::from_tree` copies it.
fn import_node(ar: &mut Arena, n: &dbx::Node<'_, '_>) -> Id {
    let tag = ar.intern(n.tag());
    let attrs = n.attributes().map(|(k, v)| (ar.intern(k), ar.intern(v))).collect();
    let (text, raw) = if n.is_container() {
        (0, Vec::new())
    } else if n.text_id().is_some() {
        (ar.intern(n.text()), Vec::new())
    } else {
        (0, n.raw().to_vec())
    };
    let children = if n.is_container() {
        n.children().map(|c| import_node(ar, &c)).collect()
    } else {
        Vec::new()
    };
    ar.push(dbxport::Node {
        tag,
        kind: n.kind(),
        attrs,
        text,
        raw,
        children,
    })
}

/// The catalogue as `dbxcatalogue.load` gives it and `bc2_native.DbxCatalogue` copies it: the tables, the elements'
/// arena, the class defaults and the second instances (`others`).
struct Cat {
    tables: Catalogue,
    els: Arena,
    defaults: HashMap<String, (String, Id)>,
    others: HashMap<String, (String, Id)>,
}

/// `dbxcatalogue.load(ix)` in Rust: from its cache (`work\tools_out\dbx_catalogue.bin`, round PR1b-1e) when it is
/// this game copy's, else built and cached ([`super::catalogue`]).
fn catalogue(py: &mut Py, ix: &Index, root: &str) -> PyResult<Cat> {
    let jobs = super::catalogue_jobs(ix.rows());
    let loaded = super::catalogue(py, &jobs, root).ok_or_else(|| {
        Fail::Err(PyErr::new(
            "OSError",
            "the catalogue's build refused a partition of the game copy (its Python reference is retired here)",
        ))
    })?;
    let built = loaded.catalogue.built;
    let mut t = Catalogue {
        exe_names: loaded.catalogue.exe_names.into_iter().collect(),
        classes: built.classes.iter().cloned().collect(),
        ..Catalogue::default()
    };
    for (k, v) in built.order.iter() {
        t.order.insert(k.clone(), v.clone());
    }
    for (k, v) in built.primary.iter() {
        t.primary.insert(k.clone(), v.clone());
    }
    for ((ty, p), c) in built.shapes.iter() {
        t.shapes
            .entry(ty.clone())
            .or_default()
            .insert(p.clone(), c.keys().cloned().collect());
    }
    for ((ty, p), c) in built.texts.iter() {
        let keys: Vec<String> = c.keys().cloned().collect();
        t.set_texts(ty, p, &keys);
    }
    for ((ty, p), c) in built.numeric.iter() {
        let n = |k: &str| {
            c.get(&k.to_owned())
                .map_or(0, |v| i64::try_from(*v).unwrap_or(i64::MAX))
        };
        t.numeric
            .entry(ty.clone())
            .or_default()
            .insert(p.clone(), (!c.is_empty(), n("whole"), n("float")));
    }
    let mut parts: HashMap<String, u32> = HashMap::new();
    for (g, w) in built.guids.iter() {
        let ids = w
            .iter()
            .map(|p| {
                *parts.entry(p.clone()).or_insert_with(|| {
                    t.partitions.push(p.clone());
                    u32::try_from(t.partitions.len() - 1).unwrap_or(u32::MAX)
                })
            })
            .collect();
        t.guids.insert(g.clone(), ids);
    }
    // the kept elements, partition by partition
    let mut els = Arena::new();
    let mut made: HashMap<(usize, usize), Id> = HashMap::new();
    let mut wanted: Vec<(usize, usize)> = built
        .defaults
        .iter()
        .chain(built.others.iter())
        .map(|(_, (_, r))| *r)
        .chain(built.protos.iter().map(|(_, r)| *r))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut i = 0;
    while let Some(&(pi, _)) = wanted.get(i) {
        let data = built.kept.get(&pi).map_or(&[][..], Vec::as_slice);
        let tree = dbx::parse_tree(data).map_err(|e| Fail::Err(PyErr::new("ValueError", e.to_string())))?;
        while let Some(&(p2, ei)) = wanted.get(i)
            && p2 == pi
        {
            if let Some(n) = tree.node(ei) {
                made.insert((pi, ei), import_node(&mut els, &n));
            }
            i += 1;
        }
    }
    let node = |r: &(usize, usize)| made.get(r).copied();
    for ((ty, p), r) in built.protos.iter() {
        if let Some(id) = node(r) {
            t.protos.entry(ty.clone()).or_default().insert(p.clone(), id);
        }
    }
    let pairs = |m: &OrderedMap<String, (String, (usize, usize))>| -> HashMap<String, (String, Id)> {
        m.iter()
            .filter_map(|(ty, (p, r))| node(r).map(|id| (ty.clone(), (p.clone(), id))))
            .collect()
    };
    let defaults = pairs(&built.defaults);
    let others = pairs(&built.others);
    Ok(Cat {
        tables: t,
        els,
        defaults,
        others,
    })
}

/// `fair_catalogue(cat, exclude)`'s class defaults: a default from the partition itself replaced by the type's
/// second instance (or left out without one).
fn fair_defaults(cat: &Cat, exclude: &str) -> HashMap<String, (String, Id)> {
    let mut out = HashMap::with_capacity(cat.defaults.len());
    #[allow(clippy::iter_over_hash_type, reason = "collected into a map keyed by type")]
    for (t, v) in &cat.defaults {
        if v.0 != exclude {
            out.insert(t.clone(), v.clone());
        } else if let Some(o) = cat.others.get(t) {
            out.insert(t.clone(), o.clone());
        }
    }
    out
}

/// `BoneTables.ske01(src, ix)`.
fn ske01(src: &Source, ix: &Index) -> PyResult<Bones> {
    let key = "characters/skeletons/ske01.res";
    let fail = || {
        Fail::Err(PyErr::new(
            "granny.GrannyError",
            "skeleton: the soldier skeleton does not read",
        ))
    };
    let names = |b: Option<Vec<u8>>| {
        let b = b.ok_or_else(fail)?;
        bc2_convert::granny::bone_names(&b)
            .map_err(|e| Fail::Err(PyErr::new("granny.GrannyError", format!("skeleton: {e}"))))
    };
    let source = names(src.data(key))?;
    let target = names(ix.read(key))?;
    let mut at = HashMap::new();
    for (i, n) in target.iter().enumerate() {
        at.insert(n.clone(), i as i128);
    }
    Ok(Bones {
        source,
        target_len: target.len(),
        at,
        skeleton: "characters/skeletons/ske01".to_owned(),
    })
}

/// `Templates.from_index(ix, names)`: `MissingTemplate` for a name the game copy lacks.
fn templates(ix: &Index, pool: &[String], rules: &dbxport::rules::Rules) -> PyResult<Templates> {
    let mut t = Templates::new();
    for n in pool {
        let Some(data) = ix.read(&format!("{n}.dbx")) else {
            return Err(Fail::Err(PyErr::new(
                "dbxport.MissingTemplate",
                format!("template partition {n}: not in the game copy"),
            )));
        };
        // a partition that does not parse is skipped (said in the first port's report, which this tool drops)
        let Ok(tree) = dbx::parse_tree(&data) else { continue };
        let key = n.replace('\\', "/").to_lowercase();
        let key = key.strip_suffix(".dbx").map_or(key.clone(), str::to_owned);
        if let Err(e) = t.add(key, Arena::from_tree(&tree), rules) {
            return Err(unsupported(&e));
        }
    }
    Ok(t)
}

fn unsupported(e: &PortFail) -> Fail {
    Fail::Err(PyErr::new(
        "dbxport.PortError",
        format!("the Rust port does not take this partition: {e}"),
    ))
}

/// The instance guids of a partition, lower case (`dbxport.guids_of`).
fn guids_of(data: Option<&[u8]>) -> HashSet<String> {
    let Some(d) = data else { return HashSet::new() };
    let Ok(tree) = dbx::parse_tree(d) else {
        return HashSet::new();
    };
    tree.instances()
        .map(|n| n.attr("guid").unwrap_or("").to_lowercase())
        .collect()
}

/// One field's counts: fields, then each run's hits, in the Python's Counter order.
type Score = Counter;

/// The `--predict` run.
#[allow(clippy::too_many_lines, reason = "predict's loop, step for step")]
fn predict(py: &mut Py, run: &Run, src: &Source, ix: &Index, root: &str) -> PyResult<()> {
    let mut names: Vec<String> = src
        .entries
        .keys()
        .filter(|k| {
            k.ends_with(".dbx")
                && ix.contains(k)
                && !matches!(family(k), "objects" | "effects" | "sounds" | "other")
                && (run.names.is_empty() || run.names.iter().any(|x| k.contains(&x.to_lowercase())))
        })
        .cloned()
        .collect();
    names.sort();
    let cat = catalogue(py, ix, root)?;
    let bones = ske01(src, ix)?;
    let rules = dbxport::rules::Rules::default();
    let bc2: HashSet<String> = ix
        .rows()
        .filter_map(|(k, _, _)| k.strip_suffix(".dbx").map(str::to_owned))
        .collect();
    let mut by_folder: HashMap<String, HashSet<String>> = HashMap::new();
    #[allow(clippy::iter_over_hash_type, reason = "collected into sets")]
    for e in src.entries.keys() {
        if let Some(n) = e.strip_suffix(".dbx") {
            let folder = n.rsplit_once('/').map_or("", |(f, _)| f);
            by_folder.entry(folder.to_owned()).or_default().insert(n.to_owned());
        }
    }
    let mut tcache: HashMap<Vec<String>, Templates> = HashMap::new();
    let dice_bytes = ix.read_many(&names);
    let mut out: OrderedMap<(String, String, String), Score> = OrderedMap::new();
    let mut examples: OrderedMap<(String, String, String), Vec<String>> = OrderedMap::new();
    let drop_types: HashSet<String> = NOT_IN_BFBC2.iter().map(|s| (*s).to_owned()).collect();
    let runs = ["predicted", "baseline", "counterpart"];
    for k in &names {
        let name = k.strip_suffix(".dbx").unwrap_or(k);
        let folder = name.rsplit_once('/').map_or("", |(f, _)| f);
        let ported = by_folder.get(folder).cloned().unwrap_or_default();
        let mut ours = Ours::new();
        #[allow(clippy::iter_over_hash_type, reason = "collected into a map")]
        for p in &ported {
            ours.insert(p.clone(), guids_of(src.data(&format!("{p}.dbx")).as_deref()));
        }
        let resolve = |p: &str| -> Result<bool, PortFail> {
            let q = p.to_lowercase();
            Ok(ported.contains(&q) || bc2.contains(&q) || q.contains("/shaders/") || q.contains("/shader/"))
        };
        let fair = fair_pool(k);
        let mut counterpart_pool = vec![name.to_owned()];
        counterpart_pool.extend(fair.iter().cloned());
        for pool in [&fair, &counterpart_pool] {
            if !tcache.contains_key(pool) {
                let t = templates(ix, pool, &rules)?;
                tcache.insert(pool.clone(), t);
            }
        }
        let fd = fair_defaults(&cat, name);
        let data = src.data(k).unwrap_or_default();
        let stand_ins = HashMap::new();
        let mut got: Vec<Vec<u8>> = Vec::with_capacity(3);
        let mut failed = None;
        for r in runs {
            let (pool, defaults) = if r == "counterpart" {
                (&counterpart_pool, &cat.defaults)
            } else {
                (&fair, &fd)
            };
            let Some(t) = tcache.get(pool) else { continue };
            let o = Options {
                drop_untemplated: true,
                drop_types: drop_types.clone(),
                prune: false,
                required: HashMap::new(),
                catalogue: Some(CatView {
                    cat: &cat.tables,
                    els: &cat.els,
                    defaults,
                }),
                ours: Some(&ours),
                stand_ins: &stand_ins,
                donors: Vec::new(),
                strict: false,
                schema_error: false,
                conventions: r != "baseline",
                bones: (r != "baseline").then_some(&bones),
                name: name.to_lowercase(),
                rules: &rules,
                resolve: Some(&resolve),
            };
            match dbxport::port(&data, t, &o) {
                Ok(p) => got.push(p.data),
                Err(PortFail::Port(m)) => {
                    failed = Some(m);
                    break;
                }
                Err(PortFail::Parse(m)) => return Err(Fail::Err(PyErr::new("ValueError", m))),
                Err(e) => return Err(unsupported(&e)),
            }
        }
        if let Some(m) = failed {
            py.print(&format!("  {k}: not ported ({m})"))?;
            continue;
        }
        let dice = match dice_bytes.get(k) {
            Some(d) => instances(d.as_deref()),
            None => instances(ix.read(k).as_deref()),
        }
        .map_err(|e| Fail::Err(PyErr::new("ValueError", e)))?;
        let mut ported_runs: Vec<Instances> = Vec::with_capacity(3);
        for d in &got {
            ported_runs.push(instances(Some(d)).map_err(|e| Fail::Err(PyErr::new("ValueError", e)))?);
        }
        let file = k.rsplit('/').next().unwrap_or("");
        for (g, (t, fd)) in dice.iter() {
            let short = t.rsplit('.').next().unwrap_or("");
            for (p, (_, vd)) in fd.iter() {
                for (kind, tp, pat) in PREDICTED.iter() {
                    if (!tp.is_empty() && *tp != short) || !pat.as_ref().is_some_and(|r| r.is_match(p)) {
                        continue;
                    }
                    let key = ((*kind).to_owned(), short.to_owned(), generic(p));
                    let hits: Vec<Option<&String>> = ported_runs
                        .iter()
                        .map(|inst| inst.get(g).and_then(|(_, f)| f.get(p)).map(|(_, v)| v))
                        .collect();
                    let [Some(predicted), Some(baseline), Some(counterpart)] = [hits.first(), hits.get(1), hits.get(2)]
                    else {
                        break;
                    };
                    if baseline.is_none() {
                        continue; // the next pattern may still match
                    }
                    let s = out.entry_default(key.clone());
                    bump(s, "fields", 1);
                    for (r, v) in runs.iter().zip([predicted, baseline, counterpart]) {
                        bump(s, r, i64::from(*v == Some(vd)));
                    }
                    if *predicted != Some(vd) && examples.get(&key).map_or(0, Vec::len) < 3 {
                        let shown = |v: Option<&String>| v.map_or_else(|| "None".to_owned(), |s| (*s).clone());
                        examples.entry_default(key).push(format!(
                            "{file}: DICE {vd} predicted {} BF1943 {}",
                            shown(*predicted),
                            shown(*baseline)
                        ));
                    }
                    break;
                }
            }
        }
    }
    let mut tot = Counter::new();
    let mut text = format!(
        "dbxport --predict on {} weapon / vehicle partitions both games have: of DICE's values, how many BF1943's \
         value (baseline), the passes on a fair pool (predicted) and with DICE's version in the pool (counterpart) \
         give\n\n",
        names.len()
    );
    let mut keys: Vec<&(String, String, String)> = out.keys().collect();
    keys.sort();
    for kind in ["rule", "template"] {
        text.push_str(&format!("== {kind}-derived\n"));
        for key in keys.iter().filter(|k| k.0 == kind) {
            let Some(c) = out.get(key) else { continue };
            for (x, v) in c.iter() {
                bump(&mut tot, &format!("{kind} {x}"), *v);
            }
            let ex = if count(c, "predicted") == count(c, "fields") {
                String::new()
            } else {
                format!(" | {}", examples.get(key).map(|e| e.join(" ; ")).unwrap_or_default())
            };
            text.push_str(&format!(
                "  {}{}: {} fields; baseline {}, predicted {}, counterpart {}{ex}\n",
                key.1,
                key.2,
                count(c, "fields"),
                count(c, "baseline"),
                count(c, "predicted"),
                count(c, "counterpart")
            ));
        }
    }
    let totals = py_dict(tot.iter().map(|(k, v)| (str_repr(k), v.to_string())));
    text.push_str(&format!("\ntotals: {totals}\n"));
    write_report(&run.o, &text, true)?;
    py.print(&format!("{} partitions; {totals} -> {}", names.len(), run.o))?;
    Ok(())
}

/// `python dbxparity.py ...` once its command line parsed.
pub fn run(py: &mut Py, run: &Run) -> PyResult<i32> {
    let (src, ix, root) = super::open(py)?;
    if run.predict {
        predict(py, run, &src, &ix, &root)?;
        return Ok(0);
    }
    let mut both: Vec<String> = src
        .entries
        .keys()
        .filter(|k| {
            k.ends_with(".dbx")
                && ix.contains(k)
                && (run.names.is_empty() || run.names.iter().any(|x| k.contains(&x.to_lowercase())))
        })
        .cloned()
        .collect();
    both.sort();
    let c = compare(py, &src, &ix, &both)?;
    compare_report(py, run, &c)?;
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_as_python() {
        assert_eq!(strip("\x1c a \u{3000}"), "a");
        assert_eq!(generic(".A[12].B[3]"), ".A[].B[]");
        assert_eq!(family("objects/vehicles/beach/jap_zero_fighter/zero.dbx"), "zero");
        assert_eq!(
            fair_pool("objects/vehicles/beach/jap_zero_fighter/zero_fighter.dbx").len(),
            10
        );
    }
}
