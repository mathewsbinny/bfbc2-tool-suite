//! `bfbc2 shaderdb info|list|textures|merge|cover|roundtrip|find|needs ...`: `bfbc2-shader-tools\shaderdb.py`'s command
//! line over `bc2_formats::shaderdb` (the reader and writer, E1c; the queries and checks, E4b) and this round's port of
//! its merge (`Merger`, `verify_merge`: [`super::merge`]) and of `level_meshes` (a level folder's meshes, read from its
//! binary partitions with `bc2_formats::dbx`).
//!
//! Handed to the Python: what `argparse` decides itself ([`super::argp`]); a partition `level_meshes` cannot read (the
//! reference's exception text names the core's own error) or one shaped as the reference cannot walk; a bundle the
//! core cannot read; a database record the reference would raise on (a program record too short for its indices, an
//! index past its section); names and texts outside latin-1 where Python's case rules decide.

use std::collections::{BTreeMap, HashMap, HashSet};

use bc2_formats::dbx;
use bc2_formats::fbrb;
use bc2_formats::shaderdb::query;
use bc2_formats::shaderdb::{self, ShaderDb};

use super::argp::{self, Kind, Nargs, Opt, Parsed, Parser, Pos, Ty};
use super::merge;
use crate::pyfmt::{docstring, float_g, str_repr, thousands};
use crate::pyos;
use crate::pyproc::{Fail, Py, PyErr, PyResult, os_error, suite_root};

const SOURCE: &str = include_str!("../../../../bfbc2-shader-tools/shaderdb.py");

// ------------------------------------------------------------------------------------------- the parsers

const COMMANDS: &[&str] = &[
    "info",
    "list",
    "textures",
    "merge",
    "cover",
    "roundtrip",
    "find",
    "needs",
];
const MAIN_USAGE: &str = "usage: shaderdb.py [-h] [--dx {9,10,11}] [--opfor OPFOR]\n                   \
                          {info,list,textures,merge,cover,roundtrip,find,needs} ...\n";

fn main_help() -> String {
    let doc = docstring(SOURCE);
    let first = doc.split("\n\n").next().unwrap_or_default();
    format!(
        "{MAIN_USAGE}\n{first}\n\npositional arguments:\n  {{info,list,textures,merge,cover,roundtrip,find,needs}}\n\n\
         options:\n  -h, --help            show this help message and exit\n  --dx {{9,10,11}}        the database \
         kind (default: from the file name)\n  --opfor OPFOR         Project OPFOR BFBC2 folder (for level:<name> \
         and find)\n"
    )
}

const fn opt(strings: &'static [&'static str], kind: Kind) -> Opt {
    Opt {
        strings,
        kind,
        ty: Ty::Str,
        choices: &[],
        required: false,
    }
}

const fn pos(name: &'static str, nargs: Nargs) -> Pos {
    Pos {
        name,
        nargs,
        choices: &[],
    }
}

const MAIN_OPTS: &[Opt] = &[
    Opt {
        strings: &["--dx"],
        kind: Kind::Store,
        ty: Ty::Int,
        choices: &["9", "10", "11"],
        required: false,
    },
    opt(&["--opfor"], Kind::Store),
];
const MAIN_POS: &[Pos] = &[Pos {
    name: "cmd",
    nargs: Nargs::One,
    choices: COMMANDS,
}];

/// A subcommand's parser (its usage and help as `argparse` prints them).
fn sub_parser(cmd: &str) -> Option<Parser> {
    Some(match cmd {
        "info" => {
            const {
                Parser {
                    prog: "shaderdb.py info",
                    usage: "usage: shaderdb.py info [-h] res\n",
                    help: "usage: shaderdb.py info [-h] res\n\npositional arguments:\n  res\n\noptions:\n  -h, --help  show \
                   this help message and exit\n",
                    opts: &[],
                    pos: &[pos("res", Nargs::One)],
                    order: &[Ok(0)],
                }
            }
        }
        "list" => {
            const {
                Parser {
                    prog: "shaderdb.py list",
                    usage: "usage: shaderdb.py list [-h] [--parts] res [filter]\n",
                    help: "usage: shaderdb.py list [-h] [--parts] res [filter]\n\npositional arguments:\n  res\n  filter\n\n\
                   options:\n  -h, --help  show this help message and exit\n  --parts\n",
                    opts: &[opt(&["--parts"], Kind::Flag)],
                    pos: &[pos("res", Nargs::One), pos("filter", Nargs::Optional)],
                    order: &[Ok(0), Ok(1), Err(0)],
                }
            }
        }
        "textures" => {
            const {
                Parser {
                    prog: "shaderdb.py textures",
                    usage: "usage: shaderdb.py textures [-h] [--constants] res [filter]\n",
                    help: "usage: shaderdb.py textures [-h] [--constants] res [filter]\n\npositional arguments:\n  res\n  \
                   filter\n\noptions:\n  -h, --help   show this help message and exit\n  --constants  also the named \
                   constants and their values\n",
                    opts: &[opt(&["--constants"], Kind::Flag)],
                    pos: &[pos("res", Nargs::One), pos("filter", Nargs::Optional)],
                    order: &[Ok(0), Ok(1), Err(0)],
                }
            }
        }
        "merge" => {
            const {
                Parser {
                    prog: "shaderdb.py merge",
                    usage: "usage: shaderdb.py merge [-h] --need NEED -o OUT [--all-names]\n                         base \
                    donors [donors ...]\n",
                    help: "usage: shaderdb.py merge [-h] --need NEED -o OUT [--all-names]\n                         base \
                   donors [donors ...]\n\npositional arguments:\n  base\n  donors\n\noptions:\n  -h, --help     show \
                   this help message and exit\n  --need NEED\n  -o, --out OUT\n  --all-names    also add donor entry \
                   names for meshes the base already has\n                 (other shader sets)\n",
                    opts: &[
                        Opt {
                            strings: &["--need"],
                            kind: Kind::Store,
                            ty: Ty::Str,
                            choices: &[],
                            required: true,
                        },
                        Opt {
                            strings: &["-o", "--out"],
                            kind: Kind::Store,
                            ty: Ty::Str,
                            choices: &[],
                            required: true,
                        },
                        opt(&["--all-names"], Kind::Flag),
                    ],
                    pos: &[pos("base", Nargs::One), pos("donors", Nargs::Plus)],
                    order: &[Ok(0), Ok(1), Err(0), Err(1), Err(2)],
                }
            }
        }
        "cover" => {
            const {
                Parser {
                    prog: "shaderdb.py cover",
                    usage: "usage: shaderdb.py cover [-h] [--all] res level\n",
                    help: "usage: shaderdb.py cover [-h] [--all] res level\n\npositional arguments:\n  res\n  level\n\n\
                   options:\n  -h, --help  show this help message and exit\n  --all\n",
                    opts: &[opt(&["--all"], Kind::Flag)],
                    pos: &[pos("res", Nargs::One), pos("level", Nargs::One)],
                    order: &[Ok(0), Ok(1), Err(0)],
                }
            }
        }
        "roundtrip" => {
            const {
                Parser {
                    prog: "shaderdb.py roundtrip",
                    usage: "usage: shaderdb.py roundtrip [-h] [--levels] [res ...]\n",
                    help: "usage: shaderdb.py roundtrip [-h] [--levels] [res ...]\n\npositional arguments:\n  res\n\n\
                   options:\n  -h, --help  show this help message and exit\n  --levels    every shader database of the \
                   game copy's level bundles\n",
                    opts: &[opt(&["--levels"], Kind::Flag)],
                    pos: &[pos("res", Nargs::Star)],
                    order: &[Ok(0), Err(0)],
                }
            }
        }
        "find" => {
            const {
                Parser {
                    prog: "shaderdb.py find",
                    usage: "usage: shaderdb.py find [-h] [--base BASE] need\n",
                    help: "usage: shaderdb.py find [-h] [--base BASE] need\n\npositional arguments:\n  need\n\noptions:\n  -h, \
                   --help   show this help message and exit\n  --base BASE\n",
                    opts: &[opt(&["--base"], Kind::Store)],
                    pos: &[pos("need", Nargs::One)],
                    order: &[Ok(0), Err(0)],
                }
            }
        }
        "needs" => {
            const {
                Parser {
                    prog: "shaderdb.py needs",
                    usage: "usage: shaderdb.py needs [-h] [-o OUT] level\n",
                    help: "usage: shaderdb.py needs [-h] [-o OUT] level\n\npositional arguments:\n  level\n\noptions:\n  -h, \
                   --help     show this help message and exit\n  -o, --out OUT\n",
                    opts: &[opt(&["-o", "--out"], Kind::Store)],
                    pos: &[pos("level", Nargs::One)],
                    order: &[Ok(0), Err(0)],
                }
            }
        }
        _ => return None,
    })
}

// ------------------------------------------------------------------------------------------- Python's text

// latin-1 names as Python's `str` holds them, hex, and the database's views: the library's merge module's (one copy)
pub use bc2_formats::shaderdb::merge::{coverage, entry_programs, hex, latin, to_latin};

/// Python's `str.lower()` where this port knows it: latin-1 by Python's rules, and characters past U+00FF whose
/// lower case is themselves; None otherwise (the caller hands the line to the Python).
pub fn py_lower(s: &str) -> Option<String> {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if let Ok(b) = u8::try_from(u32::from(c)) {
            out.push(char::from(query::lower_byte(b)));
        } else {
            let mut l = c.to_lowercase();
            if l.next() != Some(c) || l.next().is_some() || c.is_uppercase() {
                return None;
            }
            out.push(c);
        }
    }
    Some(out)
}

fn lower_or_py(s: &str) -> PyResult<String> {
    py_lower(s).ok_or(Fail::Python)
}

pub fn shader_error(msg: impl Into<String>) -> Fail {
    Fail::Err(PyErr::new("ShaderDbError", msg))
}

/// `FileNotFoundError(2, text, path)`: `[Errno 2] <text>: '<path>'`, its filename the path.
fn not_found(text: &str, path: &str) -> Fail {
    Fail::Err(PyErr {
        kind: "FileNotFoundError".into(),
        msg: format!("[Errno 2] {text}: {}", str_repr(path)),
        filename: Some(path.to_owned()),
        what_to_do: None,
    })
}

// ------------------------------------------------------------------------------------------- loading

/// `shaderdb.find_opfor(given)`: given, `OPFOR_ROOT`, then the `project-opfor-bfbc2` folder beside the suite; the
/// first with a `game` folder, made absolute.
pub fn find_opfor(given: Option<&str>) -> PyResult<String> {
    let suite = pyos::abspath(&suite_root().to_string_lossy())?;
    let beside = pyos::join2(&pyos::dirname(&suite)?, "project-opfor-bfbc2")?;
    let env = std::env::var("OPFOR_ROOT").ok();
    for c in [given.map(str::to_owned), env, Some(beside)].into_iter().flatten() {
        if !c.is_empty() && pyos::isdir(&pyos::join2(&c, "game")?) {
            return pyos::abspath(&c);
        }
    }
    Err(shader_error(
        "Project OPFOR BFBC2 not found (no game copy): put the suite next to the project-opfor-bfbc2 folder, set \
         OPFOR_ROOT, or give --opfor",
    ))
}

/// `level_bundles(root)`: {level folder (lower case): its `level-00.fbrb`}, Package over Dist, sorted by name.
pub fn level_bundles(root: &str) -> PyResult<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for top in [
        pyos::join(root, &["game", "Dist", "win32", "levels"])?,
        pyos::join(root, &["game", "Package", "levels"])?,
    ] {
        if !pyos::isdir(&top) {
            continue;
        }
        for lv in pyos::listdir(&top)? {
            let p = pyos::join(&top, &[&lv, "level-00.fbrb"])?;
            if pyos::isfile(&p) {
                out.insert(lower_or_py(&lv)?, p);
            }
        }
    }
    Ok(out)
}

/// `bundle_databases(path)`: [(entry name, FbrbTool type)] of a bundle's shader databases.
pub fn bundle_databases(path: &str) -> PyResult<Vec<(String, String)>> {
    let h = fbrb::entries_path(path).map_err(|_| Fail::Python)?;
    Ok(h.entries
        .into_iter()
        .filter(|e| e.type_name.ends_with("ShaderDatabase") && !e.is_deleted())
        .map(|e| (e.name, e.type_name))
        .collect())
}

/// `dx_from_name(path)`: `dx9` / `dx10` / `dx11` in the file name (any case).
pub fn dx_from_name(path: &str) -> Option<u32> {
    shaderdb::open::dx_in_name(&pyos::basename(path).ok()?)
}

/// `pick_database(path, dx, entry)` (`bc2_formats::shaderdb::open::pick_database`; a name past latin-1: the Python).
fn pick_database(path: &str, dx: u32, entry: Option<&str>) -> PyResult<String> {
    let dbs = bundle_databases(path)?;
    let folder = pyos::basename(&pyos::dirname(path)?)?;
    match shaderdb::open::pick_database(path, &dbs, dx, entry, &folder) {
        Some(Ok(name)) => Ok(name),
        Some(Err(msg)) => Err(shader_error(msg)),
        None => Err(Fail::Python),
    }
}

/// A database's file read and its kind decided, as `load` gives it: parse it with [`Loaded::db`].
pub struct Loaded {
    pub data: Vec<u8>,
    pub label: String,
    /// `db.dx` after `load` (the kind asked for or named, else the layout it parsed as)
    pub dx: u32,
    /// the layout it parses as (9 or 10)
    pub parse_dx: u32,
    pub big: bool,
}

impl Loaded {
    /// The database (it parsed once already: `load` checked it).
    pub fn db(&self) -> PyResult<ShaderDb<'_>> {
        shaderdb::parse(&self.data, self.parse_dx, self.big).map_err(|_| Fail::Python)
    }

    /// `db.platform`
    pub fn platform(&self) -> String {
        if self.big {
            "Xbox 360".into()
        } else {
            format!("Dx{}", self.dx)
        }
    }
}

/// `ShaderDb.parse(data, dx, where)`: (the layout it parsed as, big-endian) or its `ShaderDbError`
/// (`bc2_formats::shaderdb::open::open`).
pub fn parse_kind(data: &[u8], dx: Option<u32>, place: &str) -> PyResult<(u32, bool)> {
    shaderdb::open::open(data, dx, None, place)
        .map(|o| (o.dx, o.big_endian))
        .map_err(shader_error)
}

/// `resolve(spec, dx, opfor)`: (bytes, label, dx).
fn resolve(spec: &str, dx: Option<u32>, opfor: Option<&str>) -> PyResult<(Vec<u8>, String, Option<u32>)> {
    let mut bundle: Option<String> = None;
    let mut entry: Option<String> = None;
    if spec.len() >= 6 && spec.get(..6).is_some_and(|p| p.eq_ignore_ascii_case("level:")) {
        let rest = &spec[6..];
        let (name, e) = rest.split_once(':').unwrap_or((rest, ""));
        entry = Some(e.to_owned());
        let bundles = level_bundles(&find_opfor(opfor)?)?;
        match bundles.get(&lower_or_py(name)?) {
            Some(b) => bundle = Some(b.clone()),
            None => {
                return Err(shader_error(format!(
                    "unknown level {name}: the game copy has no levels\\{name}\\level-00.fbrb"
                )));
            }
        }
    } else {
        if spec.contains('\n') || spec.contains('\r') {
            return Err(Fail::Python);
        }
        // re.match(r'(.+?\.fbrb)(?::(.*))?$', spec, re.I): the first '.fbrb' (after one character at least) that is
        // the end or followed by ':'
        let low = spec.to_ascii_lowercase();
        let mut m: Option<(String, Option<String>)> = None;
        let mut from = 1;
        while let Some(k) = low.get(from..).and_then(|s| s.find(".fbrb")) {
            let end = from + k + 5;
            if end == spec.len() {
                m = Some((spec.to_owned(), None));
                break;
            }
            if spec.as_bytes()[end] == b':' {
                m = Some((spec[..end].to_owned(), Some(spec[end + 1..].to_owned())));
                break;
            }
            from = from + k + 1;
        }
        if let Some((b, e)) = m.filter(|_| !pyos::isfile(spec)) {
            bundle = Some(b);
            entry = e;
        } else if low.ends_with(".fbrb") {
            bundle = Some(spec.to_owned());
        }
    }
    let Some(bundle) = bundle else {
        if !pyos::isfile(spec) {
            return Err(not_found("No such file", spec));
        }
        let named = dx_from_name(spec);
        if let (Some(d), Some(n)) = (dx, named)
            && d != n
        {
            return Err(shader_error(format!(
                "{spec} is a Dx{n} database by its name, not Dx{d}"
            )));
        }
        let data = std::fs::read(spec).map_err(|e| os_error(spec, &e))?;
        return Ok((data, spec.to_owned(), dx.or(named)));
    };
    if !pyos::isfile(&bundle) {
        return Err(not_found("No such file", &bundle));
    }
    let entry = entry.filter(|e| !e.is_empty());
    let dx = dx.or_else(|| dx_from_name(entry.as_deref().unwrap_or(""))).unwrap_or(9);
    let name = pick_database(&bundle, dx, entry.as_deref())?;
    let data = fbrb::read_path(&bundle, &[name.to_ascii_lowercase()])
        .map_err(|_| Fail::Python)?
        .pop()
        .ok_or(Fail::Python)?;
    crate::budget::after_bundle_read();
    Ok((data, format!("{bundle}:{name}"), Some(dx)))
}

/// `load(spec, dx, opfor)`.
pub fn load(spec: &str, dx: Option<u32>, opfor: Option<&str>) -> PyResult<Loaded> {
    let (data, label, dx) = resolve(spec, dx, opfor)?;
    let (parse_dx, big) = parse_kind(&data, dx.map(|d| if d == 11 { 10 } else { d }), &label)?;
    Ok(Loaded {
        data,
        label,
        dx: dx.unwrap_or(parse_dx),
        parse_dx,
        big,
    })
}

// ------------------------------------------------------------------------------------------- the database's views

/// `ShaderDb.program_refs(i)` (a short record: the reference's `struct.error`, handed off).
pub fn program_refs(db: &ShaderDb<'_>, i: usize) -> PyResult<[usize; 4]> {
    shaderdb::merge::program_refs(db, i).map_err(|_| Fail::Python)
}

/// `ShaderDb.mesh_names()`
fn mesh_names(db: &ShaderDb<'_>) -> HashSet<Vec<u8>> {
    db.parts.iter().filter_map(|n| query::part_mesh(&n.name)).collect()
}

/// `ShaderDb.check()`: None where the reference raises (handed off).
pub fn check(db: &ShaderDb<'_>) -> PyResult<Vec<String>> {
    let input = query::CheckInput {
        big_endian: db.big_endian,
        numbers_big_endian: db.big_endian,
        params: db.params.len(),
        vs: db.vs.len(),
        ps: db.ps.len(),
        programs: db.programs.clone(),
        keys: db.keys.clone(),
        layouts: db.layouts.clone(),
        entries: db.entries.iter().map(|n| (&*n.name, n.guid)).collect(),
        fragments: db.fragments.iter().map(|n| n.guid).collect(),
        parts: db.parts.iter().map(|n| n.guid).collect(),
        groups: db
            .groups
            .iter()
            .map(|(g, idx)| (*g, idx.iter().map(|&i| u32::from(i)).collect()))
            .collect(),
    };
    query::check(&input).ok_or(Fail::Python)
}

/// `split_name(name, meshes)`.
fn split_name(name: &[u8], meshes: &HashSet<Vec<u8>>) -> (Vec<u8>, Vec<u8>) {
    let parts: Vec<&[u8]> = name.split(|&c| c == b'/').collect();
    let join = |s: &[&[u8]]| s.join(&b'/');
    for i in (1..parts.len()).rev() {
        if meshes.contains(&*query::lower(&join(&parts[..i]))) {
            return (join(&parts[..i]), join(&parts[i..]));
        }
    }
    for i in 1..parts.len().saturating_sub(1) {
        if query::lower(parts[i]).windows(4).any(|w| w == b"mesh") {
            return (join(&parts[..=i]), join(&parts[i + 1..]));
        }
    }
    (Vec::new(), name.to_vec())
}

/// `entry_textures(db)` per entry index: the sorted (sampler, cube, name) its programs' blocks name.
fn entry_textures<'d>(db: &'d ShaderDb<'_>) -> PyResult<Vec<Vec<query::TextureRef<'d>>>> {
    let progs = entry_programs(db);
    let mut cache: HashMap<usize, Vec<query::TextureRef<'d>>> = HashMap::new();
    let mut out = Vec::new();
    for e in &db.entries {
        let mut found: Vec<query::TextureRef<'d>> = Vec::new();
        for &i in progs.get(e.guid).copied().unwrap_or_default() {
            let r = program_refs(db, usize::from(i))?;
            for k in [r[2], r[3]] {
                if let std::collections::hash_map::Entry::Vacant(v) = cache.entry(k) {
                    let block = db.params.get(k).ok_or(Fail::Python)?;
                    v.insert(query::block_textures(block, db.big_endian));
                }
                found.extend(cache[&k].iter().copied());
            }
        }
        found.sort();
        found.dedup();
        out.push(found);
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------- level_meshes

/// `MeshUse`: {lower-case path: (shown name, [partitions using it])}, in insertion order.
#[derive(Default)]
pub struct MeshUse {
    pub keys: Vec<String>,
    pub map: HashMap<String, (String, Vec<String>)>,
}

impl MeshUse {
    fn add(&mut self, path: &str, shown: &str, place: &str) -> PyResult<()> {
        let k = lower_or_py(path)?;
        if !self.map.contains_key(&k) {
            self.keys.push(k.clone());
            self.map.insert(k.clone(), (shown.to_owned(), Vec::new()));
        }
        let e = self.map.get_mut(&k).ok_or(Fail::Python)?;
        if !e.1.iter().any(|w| w == place) {
            e.1.push(place.to_owned());
        }
        Ok(())
    }

    pub fn sorted(&self) -> Vec<&String> {
        let mut k: Vec<&String> = self.keys.iter().collect();
        k.sort();
        k
    }
}

const MESH_TYPES: &[&str] = &[
    "Render.SkinnedMeshAsset",
    "Render.RigidMeshAsset",
    "Render.CompositeMeshAsset",
    "Render.TreeMeshAsset",
];
const NOT_MESH_FIELDS: &[&str] = &["lodsettings", "shadersets", "shader", "emitter", "asset"];

/// `level_meshes(folder)`: (found, unresolved).
#[allow(clippy::too_many_lines, reason = "the reference's function, step by step")]
pub fn level_meshes(folder: &str) -> PyResult<(MeshUse, MeshUse)> {
    if !pyos::isdir(folder) {
        return Err(not_found("No such folder", folder));
    }
    let mut files: Vec<String> = Vec::new();
    for (dir, _, fs) in pyos::walk(folder)? {
        for f in &fs {
            if f.to_lowercase().ends_with(".dbx") {
                files.push(pyos::join2(&dir, f)?);
            }
        }
    }
    if files.is_empty() {
        return Err(shader_error(format!(
            "{folder} has no .dbx files: give an unpacked level bundle folder (FbrbTool unpack)"
        )));
    }
    files.sort();
    let mut parts: HashMap<String, (Option<String>, String)> = HashMap::new();
    let mut part_keys: Vec<String> = Vec::new();
    let mut refs: Vec<(String, String)> = Vec::new();
    for path in &files {
        let rel_full = pyos::relpath(path, folder)?.replace('\\', "/");
        let rel = rel_full[..rel_full.len() - 4].to_owned();
        let data = std::fs::read(path).map_err(|_| Fail::Python)?;
        if data.is_empty() {
            continue; // a *deleted* entry
        }
        if !(data.len() > 24 && data.starts_with(&dbx::MAGIC[..])) {
            return Err(shader_error(format!(
                "{path}: cannot read (ValueError: not a binary .dbx file)"
            )));
        }
        let tree = dbx::parse_tree(&data).map_err(|_| Fail::Python)?;
        let part = tree.roots().next().ok_or(Fail::Python)?;
        if !part.is_container() {
            return Err(Fail::Python); // instances of no list: the reference raises TypeError
        }
        let insts: Vec<dbx::Node<'_, '_>> = part.children().filter(|e| e.tag() == "instance").collect();
        let kind = insts.first().and_then(|i| i.attr("type")).map(str::to_owned);
        let mut name = rel.clone();
        if let Some(k) = kind.as_deref()
            && MESH_TYPES.contains(&k)
        {
            let first = insts[0];
            let mut got: Option<String> = None;
            if first.is_container() {
                for c in first.children() {
                    if c.attr("name") == Some("Name") {
                        got = Some(c.value().map_err(|_| Fail::Python)?);
                        break;
                    }
                }
            }
            if let Some(g) = got.filter(|g| !g.is_empty()) {
                name = g;
            }
        }
        let key = lower_or_py(&rel)?;
        if !parts.contains_key(&key) {
            part_keys.push(key.clone());
        }
        parts.insert(key, (kind, name));
        for inst in insts {
            let mut stack: Vec<(dbx::Node<'_, '_>, Option<String>)> = vec![(inst, None)];
            while let Some((el, field)) = stack.pop() {
                if !el.is_container() {
                    continue;
                }
                for ch in el.children() {
                    let fname: Option<String> = if ch.tag() == "item" {
                        field.clone()
                    } else {
                        ch.attr("name").map(str::to_owned)
                    };
                    if let (Some(r), Some(f)) = (ch.attr("ref"), fname.as_deref())
                        && !r.is_empty()
                        && r != "null"
                        && !f.is_empty()
                        && r.contains('/')
                    {
                        if !f.is_ascii() {
                            return Err(Fail::Python); // re.I's case folding beyond ASCII
                        }
                        let fl = f.to_ascii_lowercase();
                        if fl.contains("mesh") && !NOT_MESH_FIELDS.contains(&fl.as_str()) {
                            let target = r.rsplit_once('/').map_or(r, |(t, _)| t);
                            if !lower_or_py(target)?.starts_with("systems/mesh/") {
                                refs.push((target.to_owned(), rel.clone()));
                            }
                        }
                    }
                    if ch.is_container() && ch.children().next().is_some() {
                        stack.push((ch, fname));
                    }
                }
            }
        }
    }
    let mut found = MeshUse::default();
    let mut unresolved = MeshUse::default();
    let mut sorted_parts: Vec<&String> = part_keys.iter().collect();
    sorted_parts.sort();
    for rel in sorted_parts {
        let (kind, name) = &parts[rel];
        if kind.as_deref().is_some_and(|k| MESH_TYPES.contains(&k)) {
            found.add(rel, name, name)?;
        }
    }
    for (target, place) in &refs {
        let low = lower_or_py(target)?;
        if let Some((kind, name)) = parts.get(&low) {
            if kind.as_deref().is_some_and(|k| MESH_TYPES.contains(&k)) {
                found.add(&low, name, place)?;
            }
        } else if low.rsplit('/').next().unwrap_or("").contains("mesh") {
            found.add(&low, target, place)?;
        } else {
            unresolved.add(&low, target, place)?;
        }
    }
    Ok((found, unresolved))
}

/// `read_need(spec)`: the needed names (lower case).
pub fn read_need(spec: &str) -> PyResult<Vec<String>> {
    if pyos::isdir(spec) {
        let (found, _) = level_meshes(spec)?;
        return Ok(found.sorted().into_iter().cloned().collect());
    }
    if !pyos::isfile(spec) {
        return Err(not_found("No such file or folder", spec));
    }
    let data = std::fs::read(spec).map_err(|e| os_error(spec, &e))?;
    let text = String::from_utf8_lossy(&data);
    let mut out = Vec::new();
    // universal newlines: \r\n, \r and \n end a line
    for line in text.split_inclusive(['\n', '\r']) {
        let line = line.split('#').next().unwrap_or("");
        let line = bc2_pyrt::py::strip(line).trim_matches('/');
        if !line.is_empty() {
            out.push(lower_or_py(line)?);
        }
    }
    if out.is_empty() {
        return Err(shader_error(format!("{spec} names nothing (one mesh path per line)")));
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------- the commands

fn cmd_info(py: &mut Py, res: &str, dx: Option<u32>, opfor: Option<&str>) -> PyResult<i32> {
    let l = load(res, dx, opfor)?;
    let db = l.db()?;
    py.print(&format!(
        "{}: {} shader database, {} bytes, version {}, flag {}",
        l.label,
        l.platform(),
        thousands(l.data.len() as i128),
        db.version,
        db.flag
    ))?;
    let sections: [(&str, usize); 11] = [
        ("1 parameter blocks", db.params.len()),
        ("2 binding tables", db.bindings.len()),
        ("3 register tables", db.regs.len()),
        ("4 vertex shaders", db.vs.len()),
        ("5 pixel shaders", db.ps.len()),
        ("6 programs (and keys)", db.programs.len()),
        ("7 vertex layouts", db.layouts.len()),
        ("8.1 entries (mesh + shader)", db.entries.len()),
        ("8.2 fragments", db.fragments.len()),
        ("8.3 mesh-part names", db.parts.len()),
        ("8.4 entry groups", db.groups.len()),
    ];
    for (label, n) in sections {
        py.print(&format!("  section {label:30} {:>7}", thousands(n as i128)))?;
    }
    let code: usize = db.vs.iter().chain(&db.ps).map(|s| s.code.len()).sum();
    py.print(&format!(
        "  shader code {} bytes; {} meshes named by mesh parts",
        thousands(code as i128),
        mesh_names(&db).len()
    ))?;
    let same = shaderdb::write(&db) == l.data;
    py.print(&format!(
        "  written again: {}",
        if same { "identical bytes" } else { "DIFFERENT BYTES" }
    ))?;
    let problems = check(&db)?;
    py.print(&format!(
        "  checks: {}",
        if problems.is_empty() {
            "consistent".to_owned()
        } else {
            format!("{} problem(s)", problems.len())
        }
    ))?;
    for p in &problems {
        py.print(&format!("    {p}"))?;
    }
    Ok(i32::from(!(same && problems.is_empty())))
}

/// The filter's lower case, as latin-1 bytes (a filter with a character no latin-1 name can hold matches nothing:
/// None).
fn filter_bytes(f: Option<&str>) -> PyResult<Option<Vec<u8>>> {
    let low = lower_or_py(f.unwrap_or(""))?;
    Ok(to_latin(&low))
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || hay.windows(needle.len()).any(|w| w == needle)
}

fn cmd_list(
    py: &mut Py,
    res: &str,
    filter: Option<&str>,
    parts: bool,
    dx: Option<u32>,
    opfor: Option<&str>,
) -> PyResult<i32> {
    let l = load(res, dx, opfor)?;
    let db = l.db()?;
    let flt = filter_bytes(filter)?;
    let hit = |name: &[u8]| flt.as_deref().is_some_and(|f| contains(&query::lower(name), f));
    if parts {
        let mut n = 0;
        for p in &db.parts {
            if hit(&p.name) {
                py.print(&format!("{}  -> layout {}", latin(&p.name), hex(p.guid)))?;
                n += 1;
            }
        }
        py.print(&format!("{n} of {} mesh-part names", db.parts.len()))?;
        return Ok(0);
    }
    let meshes = mesh_names(&db);
    let progs = entry_programs(&db);
    let mut rows: Vec<(Vec<u8>, Vec<u8>, usize, usize)> = Vec::new();
    for e in &db.entries {
        if !hit(&e.name) {
            continue;
        }
        let (mesh, shader) = split_name(&e.name, &meshes);
        let idx = progs.get(e.guid).copied().unwrap_or_default();
        let mut vs: HashSet<usize> = HashSet::new();
        let mut ps: HashSet<usize> = HashSet::new();
        for &i in idx {
            let r = program_refs(&db, usize::from(i))?;
            vs.insert(r[0]);
            ps.insert(r[1]);
        }
        let mut size = 0;
        for v in &vs {
            size += db.vs.get(*v).ok_or(Fail::Python)?.code.len();
        }
        for f in &ps {
            size += db.ps.get(*f).ok_or(Fail::Python)?.code.len();
        }
        rows.push((mesh, shader, idx.len(), size));
    }
    py.print(&format!("{:>8} {:>9}  mesh  |  shader", "programs", "bytes"))?;
    for (mesh, shader, n, size) in &rows {
        let m = if mesh.is_empty() { "-".to_owned() } else { latin(mesh) };
        py.print(&format!(
            "{n:8} {:>9}  {m}  |  {}",
            thousands(*size as i128),
            latin(shader)
        ))?;
    }
    let distinct: HashSet<Vec<u8>> = rows
        .iter()
        .filter(|r| !r.0.is_empty())
        .map(|r| query::lower(&r.0).into_owned())
        .collect();
    py.print(&format!(
        "{} of {} entries, {} meshes",
        rows.len(),
        db.entries.len(),
        distinct.len()
    ))?;
    Ok(0)
}

/// A constant's value as Python compares it (a float's == : NaN equal to nothing).
#[derive(PartialEq)]
enum Seen {
    Const(Vec<u8>, Vec<f64>),
    Engine(Vec<u8>, u16),
}

fn cmd_textures(
    py: &mut Py,
    res: &str,
    filter: Option<&str>,
    constants: bool,
    dx: Option<u32>,
    opfor: Option<&str>,
) -> PyResult<i32> {
    let l = load(res, dx, opfor)?;
    let db = l.db()?;
    let flt = filter_bytes(filter)?;
    let tex = entry_textures(&db)?;
    // tex is {name: ...}: a name listed twice shows its last entry's textures
    let mut last: HashMap<&[u8], usize> = HashMap::new();
    for (i, e) in db.entries.iter().enumerate() {
        last.insert(&*e.name, i);
    }
    let progs = entry_programs(&db);
    let mut n = 0;
    for e in &db.entries {
        if !flt.as_deref().is_some_and(|f| contains(&query::lower(&e.name), f)) {
            continue;
        }
        n += 1;
        py.print(&latin(&e.name))?;
        for t in &tex[last[&*e.name]] {
            let kind = if t.cube != 0 {
                "cube map"
            } else {
                query::texture_kind(t.name)
            };
            let shown = if kind.is_empty() {
                String::new()
            } else {
                format!("   [{kind}]")
            };
            py.print(&format!("    s{:<2}  {}{shown}", t.sampler, latin(t.name)))?;
        }
        if constants {
            let mut seen: Vec<Seen> = Vec::new();
            for &i in progs.get(e.guid).copied().unwrap_or_default() {
                let r = program_refs(&db, usize::from(i))?;
                for k in [r[2], r[3]] {
                    let block = db.params.get(k).ok_or(Fail::Python)?;
                    for c in query::block_constants(block, db.big_endian) {
                        let comps = usize::from(c.components).min(4);
                        let vals: Vec<f64> = c.values[..comps].iter().map(|&v| f64::from(v)).collect();
                        let key = Seen::Const(c.name.to_vec(), vals.clone());
                        if !seen.contains(&key) {
                            seen.push(key);
                            let text: Vec<String> = vals.iter().map(|&v| float_g(v)).collect();
                            py.print(&format!("    {} = {}", latin(c.name), text.join(", ")))?;
                        }
                    }
                    for (pname, s) in query::block_engine_textures(block, db.big_endian) {
                        let key = Seen::Engine(pname.to_vec(), s);
                        if !seen.contains(&key) {
                            seen.push(key);
                            py.print(&format!("    s{s:<2}  ({}: bound by the engine)", latin(pname)))?;
                        }
                    }
                }
            }
        }
    }
    let mut names: HashSet<&[u8]> = HashSet::new();
    for &i in last.values() {
        for t in &tex[i] {
            names.insert(t.name);
        }
    }
    py.print(&format!(
        "{n} of {} entries; {} textures named in all",
        db.entries.len(),
        names.len()
    ))?;
    Ok(0)
}

fn used_by(where_: &[String]) -> String {
    let more = if where_.len() > 1 {
        format!(" and {} more", where_.len() - 1)
    } else {
        String::new()
    };
    format!("(used by {}{more})", where_[0])
}

fn cmd_cover(py: &mut Py, res: &str, level: &str, all: bool, dx: Option<u32>, opfor: Option<&str>) -> PyResult<i32> {
    let l = load(res, dx, opfor)?;
    let db = l.db()?;
    let (meshes, unresolved) = level_meshes(level)?;
    let count = coverage(&db, &meshes.keys);
    let by: HashMap<&String, u32> = meshes.keys.iter().zip(count).collect();
    let missing = by.values().filter(|&&n| n == 0).count();
    py.print(&format!(
        "{}: {} meshes used by {level}; {} have entries, {missing} have none",
        l.label,
        meshes.keys.len(),
        meshes.keys.len() - missing
    ))?;
    for m in meshes.sorted() {
        let n = by[m];
        if n == 0 || all {
            let (shown, w) = &meshes.map[m];
            let c = if n == 0 { "MISSING".to_owned() } else { format!("{n:7}") };
            py.print(&format!("  {c}  {shown}   {}", used_by(w)))?;
        }
    }
    if !unresolved.keys.is_empty() {
        py.print(&format!(
            "not checked: {} referenced partitions outside the folder that are not named like a mesh (shader sets, \
             edge models ...):",
            unresolved.keys.len()
        ))?;
        for m in unresolved.sorted() {
            let (shown, w) = &unresolved.map[m];
            py.print(&format!("  {shown}   {}", used_by(w)))?;
        }
    }
    Ok(0)
}

fn cmd_needs(py: &mut Py, level: &str, out: Option<&str>) -> PyResult<i32> {
    let (meshes, _) = level_meshes(level)?;
    let mut text = String::new();
    for m in meshes.sorted() {
        text.push_str(&meshes.map[m].0);
        text.push('\n');
    }
    if let Some(out) = out {
        // open(out, 'w', encoding='utf-8'): text mode, each \n written as \r\n
        std::fs::write(out, text.replace('\n', "\r\n")).map_err(|e| os_error(out, &e))?;
        py.print(&format!("{} meshes written to {out}", meshes.keys.len()))?;
    } else {
        py.write(&text)?;
    }
    Ok(0)
}

fn cmd_roundtrip(py: &mut Py, res: &[String], levels: bool, dx: Option<u32>, opfor: Option<&str>) -> PyResult<i32> {
    let mut specs: Vec<(String, Option<u32>)> = res.iter().map(|r| (r.clone(), dx)).collect();
    if levels {
        for path in level_bundles(&find_opfor(opfor)?)?.values() {
            for (name, typ) in bundle_databases(path)? {
                let n = typ
                    .get(2..typ.len().saturating_sub(14))
                    .filter(|s| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit()))
                    .and_then(|s| s.parse::<u32>().ok())
                    .ok_or(Fail::Python)?;
                specs.push((format!("{path}:{name}"), Some(n)));
            }
        }
    }
    if specs.is_empty() {
        return Err(shader_error(
            "roundtrip needs databases: files, bundles, level:<name>, or --levels",
        ));
    }
    let mut bad = 0;
    for (spec, d) in specs {
        let l = load(&spec, d, opfor)?;
        let db = l.db()?;
        let same = shaderdb::write(&db) == l.data;
        let problems = check(&db)?;
        bad += i32::from(!same || !problems.is_empty());
        let extra = if problems.is_empty() {
            String::new()
        } else {
            format!("; checks: {}", problems.join("; "))
        };
        py.print(&format!(
            "{} {}: Dx{}, {} bytes, {} entries, {} programs{extra}",
            if same { "ok  " } else { "FAIL" },
            l.label,
            l.dx,
            thousands(l.data.len() as i128),
            db.entries.len(),
            db.programs.len()
        ))?;
    }
    Ok(i32::from(bad > 0))
}

fn cmd_find(py: &mut Py, need: &str, base: Option<&str>, dx: Option<u32>, opfor: Option<&str>) -> PyResult<i32> {
    let mut need = read_need(need)?;
    let root = find_opfor(opfor)?;
    let dx = dx.unwrap_or(9);
    if let Some(base) = base {
        let l = load(base, Some(dx), opfor)?;
        let db = l.db()?;
        let cover = coverage(&db, &need);
        let skip: HashSet<&String> = need
            .iter()
            .zip(&cover)
            .filter(|(_, n)| **n > 0)
            .map(|(m, _)| m)
            .collect();
        let skip: HashSet<String> = skip.into_iter().cloned().collect();
        need.retain(|m| !skip.contains(m));
        py.print(&format!(
            "{} needed meshes already have entries in {}; looking for the other {}",
            skip.len(),
            l.label,
            need.len()
        ))?;
    }
    let mut keys: Vec<String> = Vec::new();
    let mut have: HashMap<String, Vec<String>> = HashMap::new();
    for m in &need {
        if !have.contains_key(m) {
            keys.push(m.clone());
            have.insert(m.clone(), Vec::new());
        }
    }
    let want = format!("dx{dx}shaderdatabase");
    for (lv, path) in level_bundles(&root)? {
        for (name, typ) in bundle_databases(&path)? {
            if typ.to_ascii_lowercase() != want || !name.to_ascii_lowercase().starts_with(&format!("levels/{lv}/")) {
                continue;
            }
            let l = load(&format!("{path}:{name}"), Some(dx), opfor)?;
            let db = l.db()?;
            let cover = coverage(&db, &keys);
            for (m, n) in keys.iter().zip(cover) {
                if n > 0 {
                    have.get_mut(m).ok_or(Fail::Python)?.push(format!("{lv}({n})"));
                }
            }
        }
    }
    keys.sort();
    for m in &keys {
        let h = &have[m];
        let shown = if h.is_empty() {
            "NO LEVEL HAS IT".to_owned()
        } else {
            h.join(", ")
        };
        py.print(&format!("{m}: {shown}"))?;
    }
    Ok(0)
}

// ------------------------------------------------------------------------------------------- main

/// `shaderdb.main(argv)` under `errors.setup('shaderdb')` / `errors.guard()`, stdout `errors='replace'`.
pub fn run(py: &mut Py, argv: &[String]) -> PyResult<i32> {
    py.stdout_replace();
    let help = main_help();
    let main = Parser {
        prog: "shaderdb.py",
        usage: MAIN_USAGE,
        help: Box::leak(help.into_boxed_str()),
        opts: MAIN_OPTS,
        pos: MAIN_POS,
        order: &[Err(0), Err(1), Ok(0)],
    };
    let mut rest = Vec::new();
    let top = match argp::parse(py, &main, argv, true, &mut rest)? {
        Parsed::Args(a) => a,
        Parsed::Exit(c) => return Ok(c),
    };
    let cmd = top.one("cmd").ok_or(Fail::Python)?.to_owned();
    let sp = sub_parser(&cmd).ok_or(Fail::Python)?;
    let a = match argp::parse(py, &sp, &rest, false, &mut Vec::new())? {
        Parsed::Args(a) => a,
        Parsed::Exit(c) => return Ok(c),
    };
    let a = match argp::finish(py, &main, a)? {
        Parsed::Args(a) => a,
        Parsed::Exit(c) => return Ok(c),
    };
    let dx: Option<u32> = top
        .one("--dx")
        .map(|v| v.parse::<u32>().map_err(|_| Fail::Python))
        .transpose()?;
    let opfor = top.one("--opfor").map(str::to_owned);
    let opfor = opfor.as_deref();
    let one = |k: &str| a.one(k).map(str::to_owned).ok_or(Fail::Python);
    match cmd.as_str() {
        "info" => cmd_info(py, &one("res")?, dx, opfor),
        "list" => cmd_list(py, &one("res")?, a.one("filter"), a.flag("--parts"), dx, opfor),
        "textures" => cmd_textures(py, &one("res")?, a.one("filter"), a.flag("--constants"), dx, opfor),
        "cover" => cmd_cover(py, &one("res")?, &one("level")?, a.flag("--all"), dx, opfor),
        "needs" => cmd_needs(py, &one("level")?, a.one("--out")),
        "roundtrip" => cmd_roundtrip(py, a.all("res"), a.flag("--levels"), dx, opfor),
        "find" => cmd_find(py, &one("need")?, a.one("--base"), dx, opfor),
        "merge" => merge::cmd_merge(
            py,
            &merge::MergeArgs {
                base: one("base")?,
                donors: a.all("donors").to_vec(),
                need: one("--need")?,
                out: one("--out")?,
                all_names: a.flag("--all-names"),
                dx,
                opfor: opfor.map(str::to_owned),
            },
        ),
        _ => Err(Fail::Python),
    }
}
