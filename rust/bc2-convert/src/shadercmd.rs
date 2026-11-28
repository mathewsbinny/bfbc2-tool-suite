//! `shaderdb.py`'s `needs` and `merge` as a library (round `OR10`): a level folder's meshes (`level_meshes`,
//! `read_need`), a database found and read as `load` finds it (a file, a bundle entry or `level:<name>`), the merge
//! (`bc2_formats::shaderdb::merge::Merger`) with its read-back check, `verify_merge` and its report - moved from
//! `bfbc2.exe shaderdb` (round O5, `bc2-cli`'s `shader` group), which keeps its command line over these, so that
//! Project OPFOR's console port stage 7 (`bc2_install::onslaught::shaders`) runs the same merge in process.
//!
//! The errors are the Python's: a [`CmdError::Shader`] is its `ShaderDbError`, [`CmdError::NotFound`] its
//! `FileNotFoundError`, [`CmdError::Os`] another `OSError`; [`CmdError::Python`] is what this port does not decide
//! (the reference's own failure: `bfbc2.exe` hands the command to the Python).

#![allow(
    clippy::indexing_slicing,
    reason = "regex-free byte slices bounded by the search before them and the reference's own indices (bc2-cli's port)"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::case_sensitive_file_extension_comparisons,
    reason = "the reference's names; a manifest path compared is ASCII .dbx"
)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use bc2_formats::dbx;
use bc2_formats::fbrb;
use bc2_formats::shaderdb::merge::{Kind, MergeError, Merger, coverage, entry_programs, field, latin, shader_key};
use bc2_formats::shaderdb::query;
use bc2_formats::shaderdb::{self, ShaderDb};

/// What went wrong, as the Python's exception.
#[derive(Debug)]
pub enum CmdError {
    /// `ShaderDbError(message)`.
    Shader(String),
    /// `FileNotFoundError(2, text, path)`.
    NotFound {
        /// The text (`No such file`, `No such folder`, `No such file or folder`).
        text: &'static str,
        /// The path.
        path: String,
    },
    /// An `OSError` reading or writing `path`.
    Os {
        /// The file.
        path: String,
        /// The error.
        err: std::io::Error,
    },
    /// The reference's own failure (a record or text this port does not decide).
    Python,
}

impl std::fmt::Display for CmdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Shader(m) => write!(f, "ShaderDbError: {m}"),
            Self::NotFound { text, path } => write!(f, "FileNotFoundError: [Errno 2] {text}: '{path}'"),
            Self::Os { path, err } => write!(f, "OSError: {path}: {err}"),
            Self::Python => write!(f, "the database or folder is not one this reader decides (malformed)"),
        }
    }
}

impl std::error::Error for CmdError {}

/// The result of these commands.
pub type Result<T> = std::result::Result<T, CmdError>;

fn os(path: &str, err: std::io::Error) -> CmdError {
    CmdError::Os {
        path: path.to_owned(),
        err,
    }
}

fn merge_err(e: MergeError) -> CmdError {
    match e {
        MergeError::Msg(m) => CmdError::Shader(m),
        MergeError::Malformed => CmdError::Python,
    }
}

/// `os.path.join` (Windows) of plain names.
fn join(a: &str, names: &[&str]) -> String {
    let mut out = a.to_owned();
    for n in names {
        if !(out.is_empty() || out.ends_with(['\\', '/']) || (out.len() == 2 && out.ends_with(':'))) {
            out.push('\\');
        }
        out.push_str(n);
    }
    out
}

fn is_file(p: &str) -> bool {
    std::path::Path::new(p).is_file()
}

fn is_dir(p: &str) -> bool {
    std::path::Path::new(p).is_dir()
}

/// `os.path.basename` (Windows).
fn basename(p: &str) -> &str {
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

/// Python's `str.lower()` where this port knows it: latin-1 by Python's rules, and characters past U+00FF whose
/// lower case is themselves; None otherwise (the caller hands the line to the Python).
#[must_use]
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

fn lower_or_py(s: &str) -> Result<String> {
    py_lower(s).ok_or(CmdError::Python)
}

/// Python's `str.strip()`: white space as `str.isspace` has it (Unicode white space, U+001C - U+001F).
fn py_strip(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
}

// ------------------------------------------------------------------------------------------- loading

/// What `load` asks of its caller: the OPFOR folder (`find_opfor`, asked only for a `level:<name>` spec) and what runs
/// after a bundle read (the Python's `fbrb.read` runs the caches' budget once).
pub struct Env<'a> {
    /// The OPFOR folder.
    pub opfor: &'a dyn Fn() -> Result<String>,
    /// Called after each database read from a bundle.
    pub on_bundle_read: &'a dyn Fn(),
}

/// `level_bundles(root)`: {level folder (lower case): its `level-00.fbrb`}, Package over Dist, sorted by name.
///
/// # Errors
/// A folder that cannot be listed; a level name outside latin-1 ([`CmdError::Python`]).
pub fn level_bundles(root: &str) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    for top in [
        join(root, &["game", "Dist", "win32", "levels"]),
        join(root, &["game", "Package", "levels"]),
    ] {
        if !is_dir(&top) {
            continue;
        }
        let rd = std::fs::read_dir(&top).map_err(|e| os(&top, e))?;
        for e in rd {
            let e = e.map_err(|e| os(&top, e))?;
            let lv = e.file_name().to_string_lossy().into_owned();
            let p = join(&top, &[&lv, "level-00.fbrb"]);
            if is_file(&p) {
                out.insert(lower_or_py(&lv)?, p);
            }
        }
    }
    Ok(out)
}

/// `bundle_databases(path)`: [(entry name, FbrbTool type)] of a bundle's shader databases.
///
/// # Errors
/// A bundle that does not read ([`CmdError::Python`]).
pub fn bundle_databases(path: &str) -> Result<Vec<(String, String)>> {
    let h = fbrb::entries_path(path).map_err(|_| CmdError::Python)?;
    Ok(h.entries
        .into_iter()
        .filter(|e| e.type_name.ends_with("ShaderDatabase") && !e.is_deleted())
        .map(|e| (e.name, e.type_name))
        .collect())
}

/// `dx_from_name(path)`: `dx9` / `dx10` / `dx11` in the file name (any case).
#[must_use]
pub fn dx_from_name(path: &str) -> Option<u32> {
    shaderdb::open::dx_in_name(basename(path))
}

/// `pick_database(path, dx, entry)` (a name past latin-1: the Python).
fn pick_database(path: &str, dx: u32, entry: Option<&str>) -> Result<String> {
    let dbs = bundle_databases(path)?;
    let dir = path.rfind(['\\', '/']).map_or("", |i| &path[..i]);
    match shaderdb::open::pick_database(path, &dbs, dx, entry, basename(dir)) {
        Some(Ok(name)) => Ok(name),
        Some(Err(msg)) => Err(CmdError::Shader(msg)),
        None => Err(CmdError::Python),
    }
}

/// A database's file read and its kind decided, as `load` gives it: parse it with [`Loaded::db`].
pub struct Loaded {
    /// The database's bytes.
    pub data: Vec<u8>,
    /// `db.label`: the file, or `<bundle>:<entry>`.
    pub label: String,
    /// `db.dx` after `load` (the kind asked for or named, else the layout it parsed as)
    pub dx: u32,
    /// the layout it parses as (9 or 10)
    pub parse_dx: u32,
    /// Xbox 360 (big-endian).
    pub big: bool,
}

impl Loaded {
    /// The database (it parsed once already: `load` checked it).
    ///
    /// # Errors
    /// [`CmdError::Python`] (not reached: `load` parsed it).
    pub fn db(&self) -> Result<ShaderDb<'_>> {
        shaderdb::parse(&self.data, self.parse_dx, self.big).map_err(|_| CmdError::Python)
    }

    /// `db.platform`
    #[must_use]
    pub fn platform(&self) -> String {
        if self.big {
            "Xbox 360".into()
        } else {
            format!("Dx{}", self.dx)
        }
    }

    fn kind(&self, db: &ShaderDb<'_>) -> Kind {
        Kind {
            dx: self.dx,
            big: self.big,
            key_size: db.key_size(),
            version: db.version,
        }
    }
}

/// `ShaderDb.parse(data, dx, where)`: (the layout it parsed as, big-endian) or its `ShaderDbError`.
///
/// # Errors
/// The `ShaderDbError` the reader gives.
pub fn parse_kind(data: &[u8], dx: Option<u32>, place: &str) -> Result<(u32, bool)> {
    shaderdb::open::open(data, dx, None, place)
        .map(|o| (o.dx, o.big_endian))
        .map_err(CmdError::Shader)
}

/// `resolve(spec, dx, opfor)`: (bytes, label, dx, from a bundle). `opfor` gives the OPFOR folder for `level:<name>`.
fn resolve(spec: &str, dx: Option<u32>, env: &Env<'_>) -> Result<(Vec<u8>, String, Option<u32>)> {
    let mut bundle: Option<String> = None;
    let mut entry: Option<String> = None;
    if spec.len() >= 6 && spec.get(..6).is_some_and(|p| p.eq_ignore_ascii_case("level:")) {
        let rest = &spec[6..];
        let (name, e) = rest.split_once(':').unwrap_or((rest, ""));
        entry = Some(e.to_owned());
        let bundles = level_bundles(&(env.opfor)()?)?;
        match bundles.get(&lower_or_py(name)?) {
            Some(b) => bundle = Some(b.clone()),
            None => {
                return Err(CmdError::Shader(format!(
                    "unknown level {name}: the game copy has no levels\\{name}\\level-00.fbrb"
                )));
            }
        }
    } else {
        if spec.contains('\n') || spec.contains('\r') {
            return Err(CmdError::Python);
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
        if let Some((b, e)) = m.filter(|_| !is_file(spec)) {
            bundle = Some(b);
            entry = e;
        } else if low.ends_with(".fbrb") {
            bundle = Some(spec.to_owned());
        }
    }
    let Some(bundle) = bundle else {
        if !is_file(spec) {
            return Err(CmdError::NotFound {
                text: "No such file",
                path: spec.to_owned(),
            });
        }
        let named = dx_from_name(spec);
        if let (Some(d), Some(n)) = (dx, named)
            && d != n
        {
            return Err(CmdError::Shader(format!(
                "{spec} is a Dx{n} database by its name, not Dx{d}"
            )));
        }
        let data = std::fs::read(spec).map_err(|e| os(spec, e))?;
        return Ok((data, spec.to_owned(), dx.or(named)));
    };
    if !is_file(&bundle) {
        return Err(CmdError::NotFound {
            text: "No such file",
            path: bundle,
        });
    }
    let entry = entry.filter(|e| !e.is_empty());
    let dx = dx.or_else(|| dx_from_name(entry.as_deref().unwrap_or(""))).unwrap_or(9);
    let name = pick_database(&bundle, dx, entry.as_deref())?;
    let data = bc2_gamefs::bundles::read_path(&bundle, &[name.to_ascii_lowercase()])
        .map_err(|_| CmdError::Python)?
        .pop()
        .ok_or(CmdError::Python)?;
    (env.on_bundle_read)();
    Ok((data, format!("{bundle}:{name}"), Some(dx)))
}

/// `load(spec, dx, opfor)`.
///
/// # Errors
/// As the Python's `load`.
pub fn load(spec: &str, dx: Option<u32>, env: &Env<'_>) -> Result<Loaded> {
    let (data, label, dx) = resolve(spec, dx, env)?;
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

/// `ShaderDb.program_refs(i)` (a short record: the reference's `struct.error`).
///
/// # Errors
/// [`CmdError::Python`].
pub fn program_refs(db: &ShaderDb<'_>, i: usize) -> Result<[usize; 4]> {
    shaderdb::merge::program_refs(db, i).map_err(|_| CmdError::Python)
}

/// `ShaderDb.check()`.
///
/// # Errors
/// [`CmdError::Python`] where the reference raises.
pub fn check(db: &ShaderDb<'_>) -> Result<Vec<String>> {
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
    query::check(&input).ok_or(CmdError::Python)
}

// ------------------------------------------------------------------------------------------- level_meshes

/// `MeshUse`: {lower-case path: (shown name, [partitions using it])}, in insertion order.
#[derive(Default)]
pub struct MeshUse {
    /// The keys in insertion order.
    pub keys: Vec<String>,
    /// key -> (shown name, partitions using it).
    pub map: HashMap<String, (String, Vec<String>)>,
}

impl MeshUse {
    fn add(&mut self, path: &str, shown: &str, place: &str) -> Result<()> {
        let k = lower_or_py(path)?;
        if !self.map.contains_key(&k) {
            self.keys.push(k.clone());
            self.map.insert(k.clone(), (shown.to_owned(), Vec::new()));
        }
        let e = self.map.get_mut(&k).ok_or(CmdError::Python)?;
        if !e.1.iter().any(|w| w == place) {
            e.1.push(place.to_owned());
        }
        Ok(())
    }

    /// The keys, sorted.
    #[must_use]
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

/// The `.dbx` files under `folder` (any case), each with its path relative to `folder` (`\` between the parts).
fn dbx_files(folder: &str) -> Result<Vec<(String, String)>> {
    fn walk(d: &str, rel: &str, out: &mut Vec<(String, String)>) -> Result<()> {
        let rd = std::fs::read_dir(d).map_err(|e| os(d, e))?;
        for e in rd {
            let e = e.map_err(|e| os(d, e))?;
            let name = e.file_name().to_string_lossy().into_owned();
            let p = join(d, &[&name]);
            let r = if rel.is_empty() {
                name.clone()
            } else {
                format!("{rel}\\{name}")
            };
            if std::fs::metadata(&p).is_ok_and(|m| m.is_dir()) {
                // os.walk goes into a junction (os.path.islink is false for it); folder links are followed alike
                walk(&p, &r, out)?;
            } else if name.to_lowercase().ends_with(".dbx") {
                out.push((p, r));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(folder, "", &mut out)?;
    Ok(out)
}

/// `level_meshes(folder)`: (found, unresolved).
///
/// # Errors
/// As the Python's: no such folder, no `.dbx` file, a partition that is not a binary `.dbx`.
#[allow(clippy::too_many_lines, reason = "the reference's function, step by step")]
pub fn level_meshes(folder: &str) -> Result<(MeshUse, MeshUse)> {
    if !is_dir(folder) {
        return Err(CmdError::NotFound {
            text: "No such folder",
            path: folder.to_owned(),
        });
    }
    let mut files = dbx_files(folder)?;
    if files.is_empty() {
        return Err(CmdError::Shader(format!(
            "{folder} has no .dbx files: give an unpacked level bundle folder (FbrbTool unpack)"
        )));
    }
    files.sort();
    let mut parts: HashMap<String, (Option<String>, String)> = HashMap::new();
    let mut part_keys: Vec<String> = Vec::new();
    let mut refs: Vec<(String, String)> = Vec::new();
    for (path, rel_full) in &files {
        let rel_full = rel_full.replace('\\', "/");
        let rel = rel_full[..rel_full.len() - 4].to_owned();
        let data = std::fs::read(path).map_err(|_| CmdError::Python)?;
        if data.is_empty() {
            continue; // a *deleted* entry
        }
        if !(data.len() > 24 && data.starts_with(&dbx::MAGIC[..])) {
            return Err(CmdError::Shader(format!(
                "{path}: cannot read (ValueError: not a binary .dbx file)"
            )));
        }
        let tree = dbx::parse_tree(&data).map_err(|_| CmdError::Python)?;
        let part = tree.roots().next().ok_or(CmdError::Python)?;
        if !part.is_container() {
            return Err(CmdError::Python); // instances of no list: the reference raises TypeError
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
                        got = Some(c.value().map_err(|_| CmdError::Python)?);
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
            while let Some((el, fld)) = stack.pop() {
                if !el.is_container() {
                    continue;
                }
                for ch in el.children() {
                    let fname: Option<String> = if ch.tag() == "item" {
                        fld.clone()
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
                            return Err(CmdError::Python); // re.I's case folding beyond ASCII
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

/// `cmd_needs`' file: each found mesh's shown name and `\n`, in the sorted keys' order (written in text mode: `\r\n`).
///
/// # Errors
/// As [`level_meshes`]; the file cannot be written.
pub fn write_needs(level: &str, out: &str) -> Result<usize> {
    let (meshes, _) = level_meshes(level)?;
    let mut text = String::new();
    for m in meshes.sorted() {
        text.push_str(&meshes.map[m].0);
        text.push('\n');
    }
    std::fs::write(out, text.replace('\n', "\r\n")).map_err(|e| os(out, e))?;
    Ok(meshes.keys.len())
}

/// `read_need(spec)`: the needed names (lower case).
///
/// # Errors
/// As the Python's.
pub fn read_need(spec: &str) -> Result<Vec<String>> {
    if is_dir(spec) {
        let (found, _) = level_meshes(spec)?;
        return Ok(found.sorted().into_iter().cloned().collect());
    }
    if !is_file(spec) {
        return Err(CmdError::NotFound {
            text: "No such file or folder",
            path: spec.to_owned(),
        });
    }
    let data = std::fs::read(spec).map_err(|e| os(spec, e))?;
    let text = String::from_utf8_lossy(&data);
    let mut out = Vec::new();
    // universal newlines: \r\n, \r and \n end a line
    for line in text.split_inclusive(['\n', '\r']) {
        let line = line.split('#').next().unwrap_or("");
        let line = py_strip(line).trim_matches('/');
        if !line.is_empty() {
            out.push(lower_or_py(line)?);
        }
    }
    if out.is_empty() {
        return Err(CmdError::Shader(format!(
            "{spec} names nothing (one mesh path per line)"
        )));
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------- the merge

/// An entry's programs by content.
type Contents = (Vec<Vec<u8>>, HashMap<Vec<u8>, BTreeSet<Vec<u8>>>);

/// `entry_contents(db, names)`: {entry name: its programs by content}, each program one byte string (the record up
/// to its indices, the key, both shaders with their tables, both parameter blocks; lengths first, so no two differ
/// and read the same), in the entries' order (a name twice: its last entry, at its first place).
fn entry_contents(db: &ShaderDb<'_>, names: Option<&HashSet<Vec<u8>>>) -> Result<Contents> {
    let progs = entry_programs(db);
    let mut order = Vec::new();
    let mut out: HashMap<Vec<u8>, BTreeSet<Vec<u8>>> = HashMap::new();
    for e in &db.entries {
        if names.is_some_and(|n| !n.contains(&*e.name)) {
            continue;
        }
        let mut items = BTreeSet::new();
        for &i in progs.get(e.guid).copied().unwrap_or_default() {
            let i = usize::from(i);
            let [v, f, x, y] = program_refs(db, i)?;
            let vk = shader_key(db, db.vs.get(v).ok_or(CmdError::Python)?).map_err(merge_err)?;
            let fk = shader_key(db, db.ps.get(f).ok_or(CmdError::Python)?).map_err(merge_err)?;
            let mut item = Vec::new();
            let mut put = |b: &[u8]| {
                item.extend((b.len() as u64).to_le_bytes());
                item.extend_from_slice(b);
            };
            put(field(db.programs.get(i).ok_or(CmdError::Python)?, 0, 0x20));
            put(db.keys.get(i).ok_or(CmdError::Python)?);
            for k in [vk, fk] {
                for b in [k.0, k.1, k.2, k.3, k.4, k.5] {
                    put(b);
                }
            }
            put(db.params.get(x).ok_or(CmdError::Python)?);
            put(db.params.get(y).ok_or(CmdError::Python)?);
            items.insert(item);
        }
        if !out.contains_key(&*e.name) {
            order.push(e.name.to_vec());
        }
        out.insert(e.name.to_vec(), items);
    }
    Ok((order, out))
}

/// `verify_merge(merged, sources, m)`.
fn verify_merge(
    merged: &ShaderDb<'_>,
    base_c: &Contents,
    donors: &[(&ShaderDb<'_>, &str)],
    added: &[(Vec<u8>, String)],
) -> Result<Vec<String>> {
    let mut p = Vec::new();
    let (_, got) = entry_contents(merged, None)?;
    let mut by_donor: HashMap<&str, HashSet<Vec<u8>>> = HashMap::new();
    for (name, label) in added {
        by_donor.entry(label.as_str()).or_default().insert(name.clone());
    }
    let mut order = base_c.0.clone();
    let mut expect = base_c.1.clone();
    for (db, label) in donors {
        if let Some(names) = by_donor.get(label).filter(|n| !n.is_empty()) {
            let (o, c) = entry_contents(db, Some(names))?;
            for n in o {
                if !expect.contains_key(&n) {
                    order.push(n.clone());
                }
                let v = c.get(&n).ok_or(CmdError::Python)?.clone();
                expect.insert(n, v);
            }
        }
    }
    let gk: HashSet<&Vec<u8>> = got.keys().collect();
    let ek: HashSet<&Vec<u8>> = expect.keys().collect();
    if gk != ek {
        p.push(format!(
            "{} entries differ between the merged file and its sources",
            gk.symmetric_difference(&ek).count()
        ));
    }
    let bad: Vec<&Vec<u8>> = order
        .iter()
        .filter(|n| got.get(*n).is_some_and(|g| Some(g) != expect.get(*n)))
        .collect();
    if let Some(first) = bad.first() {
        p.push(format!(
            "{} entries draw with other programs than in their source, e.g. {}",
            bad.len(),
            latin(first)
        ));
    }
    Ok(p)
}

/// Python's `f'{n:,}'`.
fn thousands(n: usize) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// `cmd_merge`'s arguments.
pub struct MergeArgs<'a> {
    /// The base database (a spec `load` takes).
    pub base: &'a str,
    /// The donors, in order.
    pub donors: &'a [String],
    /// `--need`: a file or a level folder.
    pub need: &'a str,
    /// `-o`: the merged file (its report beside it, `<out>.merge.txt`).
    pub out: &'a str,
    /// `--all-names`.
    pub all_names: bool,
    /// `--dx`.
    pub dx: Option<u32>,
}

/// What `cmd_merge` printed: two `print` calls' texts (the report's head, the summary line).
pub struct MergePrinted {
    /// The report's first lines, joined by `\n`.
    pub head: String,
    /// `<n> entries added; <m> needed names still have no entry; report: <out>.merge.txt`.
    pub summary: String,
}

/// `cmd_merge(a)`: the merged database and its report written; what it prints.
///
/// # Errors
/// As the Python's command.
#[allow(clippy::too_many_lines, reason = "the reference's command, step by step")]
pub fn cmd_merge(a: &MergeArgs<'_>, env: &Env<'_>) -> Result<MergePrinted> {
    let need = read_need(a.need)?;
    let base_l = load(a.base, a.dx, env)?;
    let mut donors_l: Vec<Loaded> = Vec::new();
    for spec in a.donors {
        // the donor's own kind (given, or its name's) - else the base's
        let named = dx_from_name(spec.rsplit(':').next().unwrap_or(spec));
        donors_l.push(load(spec, a.dx.or(named).or(Some(base_l.dx)), env)?);
    }
    let base = base_l.db()?;
    let before = (
        base.entries.len(),
        base.programs.len(),
        base.vs.len(),
        base.ps.len(),
        base.params.len(),
    );
    let base_c = entry_contents(&base, None)?;
    let donor_dbs: Vec<ShaderDb<'_>> = donors_l.iter().map(Loaded::db).collect::<Result<_>>()?;
    let mut m = Merger::new(base, base_l.kind(&base_l.db()?), need.clone(), a.all_names).map_err(merge_err)?;
    for (l, db) in donors_l.iter().zip(&donor_dbs) {
        m.add_donor(db, &l.kind(db), &l.label).map_err(merge_err)?;
    }
    // the base's programs no entry group owns (a base edited by hand: SHDB-MRG-1) are left out by finish
    let unowned = m.unowned_base_programs().len();
    let out = m.finish().map_err(merge_err)?;
    let parse_dx = if base_l.dx == 11 { 10 } else { base_l.dx };
    let (again_dx, again_big) = parse_kind(&out, Some(parse_dx), "the merged file")?;
    let again = shaderdb::parse(&out, again_dx, again_big).map_err(|_| CmdError::Python)?;
    let donors: Vec<(&ShaderDb<'_>, &str)> = donor_dbs
        .iter()
        .zip(&donors_l)
        .map(|(d, l)| (d, l.label.as_str()))
        .collect();
    let mut problems = check(&again)?;
    problems.extend(verify_merge(&again, &base_c, &donors, &m.added)?);
    if shaderdb::write(&again) != out || !problems.is_empty() {
        let first: Vec<&str> = problems.iter().take(5).map(String::as_str).collect();
        return Err(CmdError::Shader(format!(
            "the merged file does not read back consistently: {}",
            first.join("; ")
        )));
    }
    let abs = std::path::absolute(a.out).map_err(|e| os(a.out, e))?;
    if let Some(d) = abs.parent() {
        std::fs::create_dir_all(d).map_err(|e| os(&d.to_string_lossy(), e))?;
    }
    std::fs::write(a.out, &out).map_err(|e| os(a.out, e))?;
    let still = coverage(&again, &need);
    let mut nomesh: Vec<&String> = Vec::new();
    let mut seen: HashSet<&String> = HashSet::new();
    for (k, v) in need.iter().zip(&still) {
        if seen.insert(k) && *v == 0 {
            nomesh.push(k);
        }
    }
    nomesh.sort();
    let mut lines: Vec<String> = vec![
        format!("merged {} (Dx{}, {} bytes)", a.out, base_l.dx, thousands(out.len())),
        format!("base {}: {} entries, {} programs", base_l.label, before.0, before.1),
    ];
    if unowned > 0 {
        lines.push(format!(
            "base: {unowned} programs no entry group owns left out (an entry taken out of the file by hand, its \
             programs left; no DICE database has one)"
        ));
    }
    lines.extend(m.report.iter().cloned());
    lines.push(format!(
        "result: {} entries (+{}), {} programs (+{}), {} vertex shaders (+{}), {} pixel shaders (+{}), {} parameter \
         blocks (+{})",
        again.entries.len(),
        again.entries.len() - before.0,
        again.programs.len(),
        again.programs.len() - (before.1 - unowned),
        again.vs.len(),
        again.vs.len() - before.2,
        again.ps.len(),
        again.ps.len() - before.3,
        again.params.len(),
        again.params.len() - before.4
    ));
    lines.push(
        "read back: consistent, writes identical bytes; every entry uses the same programs, shaders, parameter blocks \
         and tables (by content) as in the file it came from"
            .into(),
    );
    lines.push(format!(
        "{} donor programs were already in the file (same GUID and content): shared",
        m.shared_programs
    ));
    if !m.part_conflicts.is_empty() {
        lines.push(format!(
            "{} mesh-part names a donor has with another vertex layout:",
            m.part_conflicts.len()
        ));
        lines.extend(m.part_conflicts.iter().take(50).map(|c| format!("  {c}")));
    }
    if !m.conflicts.is_empty() {
        lines.push(format!(
            "{} donor programs have a GUID the file has with other content (the existing one kept):",
            m.conflicts.len()
        ));
        lines.extend(m.conflicts.iter().take(50).map(|c| format!("  {c}")));
    }
    lines.push("entries added:".into());
    lines.extend(m.added.iter().map(|(n, l)| format!("  {}   <- {l}", latin(n))));
    lines.push(format!("needed names with no entry in the result: {}", nomesh.len()));
    lines.extend(nomesh.iter().map(|k| format!("  {k}")));
    let report_path = format!("{}.merge.txt", a.out);
    let text = format!("{}\n", lines.join("\n")).replace('\n', "\r\n");
    std::fs::write(&report_path, text.as_bytes()).map_err(|e| os(&report_path, e))?;
    let shown = (m.report.len() + 5 + usize::from(!m.conflicts.is_empty()) + usize::from(unowned > 0)).min(lines.len());
    Ok(MergePrinted {
        head: lines[..shown].join("\n"),
        summary: format!(
            "{} entries added; {} needed names still have no entry; report: {}.merge.txt",
            m.added.len(),
            nomesh.len(),
            a.out
        ),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn small_helpers() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1000), "1,000");
        assert_eq!(thousands(1_234_567), "1,234,567");
        assert_eq!(py_strip(" \t a b \x1c\n"), "a b");
        assert_eq!(py_lower("Levels/MP_002").unwrap(), "levels/mp_002");
        assert_eq!(dx_from_name("x\\Dx10_Single.res"), Some(10));
        assert_eq!(join("C:\\a", &["b", "c"]), "C:\\a\\b\\c");
    }

    #[test]
    fn needs_from_a_file() {
        let d = std::env::temp_dir().join(format!("bc2_shadercmd_{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        let f = d.join("needs.txt");
        std::fs::write(&f, b"Objects/A/Mesh\r\n# a comment\r\n/objects/b/\n\n").unwrap();
        let got = read_need(&f.to_string_lossy()).unwrap();
        assert_eq!(got, ["objects/a/mesh", "objects/b"]);
        std::fs::write(&f, b"# nothing\n").unwrap();
        assert!(matches!(read_need(&f.to_string_lossy()), Err(CmdError::Shader(_))));
        assert!(matches!(level_meshes(&d.to_string_lossy()), Err(CmdError::Shader(_))));
        std::fs::remove_dir_all(&d).unwrap();
        assert!(matches!(
            read_need(&f.to_string_lossy()),
            Err(CmdError::NotFound { .. })
        ));
    }
}
