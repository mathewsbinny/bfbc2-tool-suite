//! The console Onslaught port's shader audit (round OR13): the port of Project OPFOR's `console_shader_audit.py` -
//! a port's shader databases checked against everything it draws. Read-only for the game copy.
//!
//! Why: a mesh drawn with a (mesh, shader) pair its level's database has no entry for draws invisibly. On Nelson Bay
//! the console house across the creek at A drew see-through (its facade / roof / glass / metal pairs were missing;
//! the merge counted the mesh as covered because it had other entries) and the AI's TOZ-194, flags and vehicles were
//! invisible at first. Stage 7 merges by mesh; this checks by pair:
//! - **placed**: every `ReferenceObjectData` of the level, its blueprint's mesh with the object's `MeshShaderSet`
//!   over the mesh's own first set;
//! - **reached**: every mesh partition reachable from the level through references (spawns -> kits -> weapons,
//!   vehicles -> parts), the mesh's own first set;
//!
//! against the level's Dx9, Dx10 and Dx11 databases. A gap counts only when the stock PC map does not share it (its
//! own databases, the same pairs drawn the same way) and some PC level draws the pair. In depth: each spawned
//! vehicle's parts in the shader set its spawn draws, each placed object's parts, by what the engine looks a part's
//! program up by (entry, vertex layout, pass, skinning kind); and the seats' PC input.
//!
//! The work folder's files are the retired Python's byte for byte (the XML DbxEdit leaves in the unpacks,
//! `pc_pairs.txt`) and so are the lines printed, but one decided: a placed part's line named the database `DxDx9`
//! (the kind printed twice); it says `Dx9` now (rust/BENCHMARKS.md "OR13", spec XCONS-ONS-15).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::LazyLock;

use bc2_convert::shadercmd;
use regex::Regex;

use super::port::{StageOut, find};
use super::{OnsError, R, fbrb, join, joins, read, read_text, rmtree, write};

#[allow(clippy::expect_used, reason = "constant patterns, compiled by the tests")]
fn re(p: &str) -> Regex {
    Regex::new(p).expect("a constant pattern")
}

static INST: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)<instance guid="([^"]+)" type="([^"]+)"[^>]*>(.*?)</instance>"#));
static REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f\-]{36}""#));
static KNOWN: LazyLock<Regex> =
    LazyLock::new(|| re(r"/kitupgrade_\d+b$|^objects/objects/roadsigns/speedsign_40/speedsign_40_mesh/"));
static SURFACE: LazyLock<Regex> =
    LazyLock::new(|| re(r#"<field name="Name">([^<]*)</field>\s*<field ref="([^"]+)" name="Shader""#));
static NAME: LazyLock<Regex> = LazyLock::new(|| re(r#"<field name="Name">([^<]*)</field>"#));
static SETS: LazyLock<Regex> = LazyLock::new(|| re(r#"(?s)<array name="ShaderSets">(.*?)</array>"#));
static FIRST_REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]+)""#));
static MESH_ASSET: LazyLock<Regex> = LazyLock::new(|| re(r#"type="Render\.\w*MeshAsset""#));
static MESH_NAMED: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)type="Render\.(\w*)MeshAsset"[^>]*>.*?<field name="Name">([^<]*)</field>"#));
static REFERENCED: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]+)" name="ReferencedObject""#));
static SHADER_SET: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]+)" name="MeshShaderSet""#));
static VSPAWN: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)type="[^"]*VehicleSpawnEntityData"[^>]*>(.*?)</instance>"#));
static VEHICLE: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f-]{36}" name="Vehicle""#));
static SETNO: LazyLock<Regex> = LazyLock::new(|| re(r#"name="MeshShaderSetNumber">(\d+)<"#));
static CONCEPT: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]*)" name="InputConceptDefinition""#));
static MAPPING: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([^"]*)" name="InputMapping""#));
static ORDER: LazyLock<Regex> = LazyLock::new(|| re(r#"name="EntryOrderNumber">(\d+)<"#));
static ONS_TAIL: LazyLock<Regex> = LazyLock::new(|| re(r"(_adv)?_ons$"));

/// The shared bundles unpacked into `work\console_shaders\shared\<key>`: (key, where, level, bundle).
pub const SHARED: [(&str, &str, &str, &str); 3] = [
    ("mp_common_level", "Dist", "mp_common", "level"),
    ("mp_common_ingame", "Dist", "mp_common", "ingame"),
    ("sp_common_level", "Dist", "sp_common", "level"),
];
/// The databases checked, in order.
const DXS: [&str; 3] = ["Dx9", "Dx10", "Dx11"];

fn is_dir(p: &str) -> bool {
    std::path::Path::new(p).is_dir()
}

/// `os.walk`-like: every file under `d` (following folder links, as the Python's glob and walk do).
fn files_under(d: &str, out: &mut Vec<String>) {
    super::port::walk_all(d, out);
}

/// `Index(roots)`: partitions by lower-case path (the first root wins) and their XML beside them (a root with
/// partitions and no XML at all converted whole first).
pub struct Index {
    files: HashMap<String, String>,
    cache: RefCell<HashMap<String, Option<Rc<str>>>>,
}

impl Index {
    /// The roots indexed.
    #[must_use]
    pub fn new(roots: &[String]) -> Self {
        let mut files = HashMap::new();
        for r in roots {
            let mut all = Vec::new();
            files_under(r, &mut all);
            let has_dbx = all.iter().any(|f| f.to_lowercase().ends_with(".dbx"));
            let has_xml = all.iter().any(|f| f.to_lowercase().ends_with(".xml"));
            if has_dbx && !has_xml {
                // once per folder; files it cannot read stay without XML (skipped)
                let _ = bc2_tools::dbxedit::run(&["xml".to_owned(), r.clone()], None);
            }
            for f in all.iter().filter(|f| f.to_lowercase().ends_with(".dbx")) {
                let rel = super::rel(f, r);
                let key = super::stem4(&rel).replace('\\', "/").to_lowercase();
                files.entry(key).or_insert_with(|| f.clone());
            }
        }
        Self {
            files,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// The XML of a partition (lower-case path), `None` when it is not indexed or has no XML.
    pub fn xml(&self, part: &str) -> Option<Rc<str>> {
        let f = self.files.get(part)?;
        if let Some(x) = self.cache.borrow().get(f) {
            return x.clone();
        }
        let x = format!("{}.xml", super::stem4(f));
        let got: Option<Rc<str>> = if std::path::Path::new(&x).is_file() {
            read_text(&x).ok().map(Rc::from)
        } else {
            None
        };
        self.cache.borrow_mut().insert(f.clone(), got.clone());
        got
    }
}

/// The game copy's mesh index, loaded once.
struct Game<'a> {
    root: &'a str,
    ix: RefCell<Option<Rc<bc2_convert::meshview::Index>>>,
}

impl Game<'_> {
    fn get(&self) -> R<Rc<bc2_convert::meshview::Index>> {
        if let Some(ix) = self.ix.borrow().as_ref() {
            return Ok(ix.clone());
        }
        let ix = Rc::new(
            bc2_convert::meshview::Index::load(self.root, &mut |s| eprintln!("{s}"))
                .map_err(|e| OnsError::Data(e.to_string()))?,
        );
        *self.ix.borrow_mut() = Some(ix.clone());
        Ok(ix)
    }
}

/// Partitions' XML: an [`Index`], or (`WithGame`) the game copy's for what the unpacked folders lack.
trait Xml {
    fn xml(&self, part: &str) -> R<Option<Rc<str>>>;
}

impl Xml for Index {
    fn xml(&self, part: &str) -> R<Option<Rc<str>>> {
        Ok(Self::xml(self, part))
    }
}

/// `WithGame(ix)`.
struct WithGame<'a> {
    ix: &'a Index,
    game: &'a Game<'a>,
    cache: RefCell<HashMap<String, Option<Rc<str>>>>,
}

impl Xml for WithGame<'_> {
    fn xml(&self, part: &str) -> R<Option<Rc<str>>> {
        if let Some(x) = self.ix.xml(part) {
            return Ok(Some(x));
        }
        if let Some(x) = self.cache.borrow().get(part) {
            return Ok(x.clone());
        }
        let data = self.game.get()?.read(&format!("{part}.dbx"));
        let x = match data.filter(|d| !d.is_empty()) {
            Some(d) => Some(Rc::from(
                bc2_formats::dbx::to_xml(&d).map_err(|e| OnsError::Data(format!("{part}: {e}")))?,
            )),
            None => None,
        };
        self.cache.borrow_mut().insert(part.to_owned(), x.clone());
        Ok(x)
    }
}

/// `surfaces(body)`: {surface name (lower case): shader ref}, a later surface of a name winning.
fn surfaces(body: &str) -> HashMap<String, String> {
    SURFACE
        .captures_iter(body)
        .map(|c| (c[1].to_lowercase(), c[2].to_owned()))
        .collect()
}

/// `{g.upper(): (t, b) for g, t, b in INST.findall(x)}`: in first-met order, a later instance of a GUID replacing the
/// earlier's value.
fn insts(x: &str) -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut at: HashMap<String, usize> = HashMap::new();
    for c in INST.captures_iter(x) {
        let g = c[1].to_uppercase();
        if let Some(&i) = at.get(&g) {
            out[i] = (g, c[2].to_owned(), c[3].to_owned());
        } else {
            at.insert(g.clone(), out.len());
            out.push((g, c[2].to_owned(), c[3].to_owned()));
        }
    }
    out
}

/// The shader set instance `set_ref` (`partition/guid`) names: its surfaces (none when it is not found).
fn set_surfaces(ix: &dyn Xml, set_ref: Option<&str>) -> R<HashMap<String, String>> {
    let mut use_ = HashMap::new();
    if let Some(s) = set_ref.filter(|s| *s != "null") {
        let (part, g) = s.rsplit_once('/').unwrap_or(("", s));
        let sx = ix.xml(&part.to_lowercase())?;
        for c in INST.captures_iter(sx.as_deref().unwrap_or("")) {
            if c[1].to_uppercase() == g.to_uppercase() {
                use_.extend(surfaces(&c[3]));
            }
        }
    }
    Ok(use_)
}

/// `mesh_pairs(ix, mesh_part, set_ref)`: (mesh name, {shader partition}) a mesh partition draws with.
fn mesh_pairs(ix: &dyn Xml, mesh_part: &str, set_ref: Option<&str>) -> R<Option<(String, BTreeSet<String>)>> {
    let Some(mx) = ix.xml(mesh_part)?.filter(|x| !x.is_empty()) else {
        return Ok(None);
    };
    let ins = insts(&mx);
    let Some((_, _, body)) = ins
        .iter()
        .find(|(_, t, _)| t.ends_with("MeshAsset") && !t.contains("ShaderSet"))
    else {
        return Ok(None);
    };
    let name = NAME
        .captures(body)
        .ok_or_else(|| OnsError::Data("not found: Name".into()))?[1]
        .to_owned();
    let first = SETS
        .captures(body)
        .and_then(|s| FIRST_REF.captures(&s[1]).map(|f| f[1].to_uppercase()));
    let mut use_: HashMap<String, String> = match first.and_then(|f| ins.iter().find(|(g, _, _)| *g == f)) {
        Some((_, _, b)) => surfaces(b),
        None => HashMap::new(),
    };
    // set_ref's surfaces over the first set's ({}.update: a name already there keeps its place, takes the value)
    use_.extend(set_surfaces(ix, set_ref)?);
    let shaders = use_
        .values()
        .map(|s| s.rsplit_once('/').map_or(s.as_str(), |(a, _)| a).to_owned())
        .collect();
    Ok(Some((name, shaders)))
}

/// `mesh_of(ix, blueprint)`: the blueprint's own mesh (`<blueprint>_mesh` when it references it), else the first
/// mesh it references.
fn mesh_of(ix: &dyn Xml, blueprint: &str) -> R<Option<String>> {
    let x = ix.xml(blueprint)?;
    let mut seen: Vec<String> = Vec::new();
    for c in REF.captures_iter(x.as_deref().unwrap_or("")) {
        let p = c[1].to_lowercase();
        if !seen.contains(&p) {
            seen.push(p);
        }
    }
    let mut meshes = Vec::new();
    for p in seen {
        if p.ends_with("_mesh") && ix.xml(&p)?.is_some_and(|x| MESH_ASSET.is_match(&x)) {
            meshes.push(p);
        }
    }
    let own = format!("{blueprint}_mesh");
    Ok(if meshes.contains(&own) {
        Some(own)
    } else {
        meshes.into_iter().next()
    })
}

/// A level's needs: {(mesh name, how): {shader}}, in first-met order.
type Need = Vec<((String, String), BTreeSet<String>)>;

fn need_add(need: &mut Need, key: (String, String), add: BTreeSet<String>) {
    match need.iter_mut().find(|(k, _)| *k == key) {
        Some((_, v)) => v.extend(add),
        None => need.push((key, add)),
    }
}

/// `{(mesh name, how): [(shader name, [databases without the pair])]}`.
type Gaps = BTreeMap<(String, String), Vec<(String, Vec<String>)>>;

/// `gaps_of(ix, level_part, dbs)`: the gaps and (placed, reached, partitions followed).
fn gaps_of(ix: &dyn Xml, level_part: &str, dbs: &[(String, BTreeSet<String>)]) -> R<(Gaps, (usize, usize, usize))> {
    let level = ix.xml(level_part)?.ok_or_else(|| {
        OnsError::Data(format!(
            "TypeError: {level_part}: no XML (expected string or bytes-like object)"
        ))
    })?;
    let mut need: Need = Vec::new();
    let mut placed = 0;
    for c in INST.captures_iter(&level) {
        if !c[2].ends_with("ReferenceObjectData") {
            continue;
        }
        let b = &c[3];
        let Some(ro) = REFERENCED.captures(b) else { continue };
        let bp = ro[1].rsplit_once('/').map_or(&ro[1], |(a, _)| a).to_lowercase();
        let Some(mp) = mesh_of(ix, &bp)? else { continue };
        let ms = SHADER_SET.captures(b).map(|m| m[1].to_owned());
        if let Some((mesh, shaders)) = mesh_pairs(ix, &mp, ms.as_deref())? {
            placed += 1;
            let how = match ms.as_deref().filter(|s| *s != "null") {
                Some(s) => {
                    let part = s.rsplit_once('/').map_or(s, |(a, _)| a);
                    format!("placed (shader set {})", part.rsplit('/').next().unwrap_or(part))
                }
                None => "placed".to_owned(),
            };
            need_add(&mut need, (mesh, how), shaders);
        }
    }
    // every mesh reachable through references (kits, weapons, vehicles, their parts)
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut stack = vec![level_part.to_owned()];
    let mut reached = 0;
    while let Some(part) = stack.pop() {
        if !seen.insert(part.clone()) {
            continue;
        }
        let Some(x) = ix.xml(&part)?.filter(|x| !x.is_empty()) else {
            continue;
        };
        if part.ends_with("_mesh") && MESH_ASSET.is_match(&x) {
            if let Some((mesh, shaders)) = mesh_pairs(ix, &part, None)? {
                reached += 1;
                need_add(&mut need, (mesh, "reached".to_owned()), shaders);
            }
            continue;
        }
        for c in REF.captures_iter(&x) {
            let p = c[1].to_lowercase();
            if !seen.contains(&p) {
                stack.push(p);
            }
        }
    }
    let mut gaps: Gaps = BTreeMap::new();
    for ((mesh, how), shaders) in &need {
        for s in shaders {
            let pair = format!("{mesh}/{s}").to_lowercase();
            if KNOWN.is_match(&pair) {
                continue;
            }
            let miss: Vec<String> = dbs
                .iter()
                .filter(|(_, have)| !have.contains(&pair))
                .map(|(dx, _)| dx.clone())
                .collect();
            if !miss.is_empty() {
                gaps.entry((mesh.clone(), how.clone()))
                    .or_default()
                    .push((s.rsplit('/').next().unwrap_or(s).to_owned(), miss));
            }
        }
    }
    Ok((gaps, (placed, reached, seen.len())))
}

fn env_of(root: &str) -> impl Fn() -> shadercmd::Result<String> + '_ {
    move || Ok(root.to_owned())
}

fn shader_err(e: &shadercmd::CmdError) -> OnsError {
    OnsError::Exit(e.to_string())
}

/// `shaderdb.load(path)` of a level folder's database.
fn load_db(root: &str, path: &str) -> R<shadercmd::Loaded> {
    let opfor = env_of(root);
    let env = shadercmd::Env {
        opfor: &opfor,
        on_bundle_read: &|| {},
    };
    shadercmd::load(path, None, &env).map_err(|e| shader_err(&e))
}

fn latin_lower(b: &[u8]) -> String {
    b.iter().map(|&c| char::from(c)).collect::<String>().to_lowercase()
}

/// `databases(folder, name)`: each kind's entry names (lower case).
fn databases(root: &str, folder: &str, name: &str) -> R<Vec<(String, BTreeSet<String>)>> {
    let mut out = Vec::new();
    for dx in DXS {
        let f = find(folder, &format!("Levels/{name}/Shaders/{dx}_Single.res"))?;
        let l = load_db(root, &f)?;
        let db = l.db().map_err(|e| shader_err(&e))?;
        out.push((dx.to_owned(), db.entries.iter().map(|e| latin_lower(&e.name)).collect()));
    }
    Ok(out)
}

/// An entry's programs: {vertex layout: {(pass, skinning kind)}}.
type Programs = HashMap<Vec<u8>, BTreeSet<(u8, u8)>>;
/// One kind's lookup: ({entry: its programs}, {part name: layout}).
type Full = (HashMap<String, Programs>, HashMap<String, Vec<u8>>);

/// `full_databases(folder, name)`.
fn full_databases(root: &str, folder: &str, name: &str) -> R<Vec<(String, Full)>> {
    let mut out = Vec::new();
    for dx in DXS {
        let f = find(folder, &format!("Levels/{name}/Shaders/{dx}_Single.res"))?;
        let l = load_db(root, &f)?;
        let db = l.db().map_err(|e| shader_err(&e))?;
        let ep = bc2_formats::shaderdb::merge::entry_programs(&db);
        let mut kinds: HashMap<String, Programs> = HashMap::new();
        for e in &db.entries {
            let mut d: Programs = HashMap::new();
            for &i in ep.get(e.guid).copied().unwrap_or_default() {
                let k = db.keys.get(usize::from(i)).copied().unwrap_or_default();
                let lay = k.get(32..48).unwrap_or_default().to_vec();
                let ps = (k.get(0x30).copied().unwrap_or(0), k.get(0x33).copied().unwrap_or(0));
                d.entry(lay).or_default().insert(ps);
            }
            kinds.insert(latin_lower(&e.name), d);
        }
        let parts = db
            .parts
            .iter()
            .map(|p| (latin_lower(&p.name), p.guid.to_vec()))
            .collect();
        out.push((dx.to_owned(), (kinds, parts)));
    }
    Ok(out)
}

/// `ResReader(roots)`: resource files by lower name, the unpacked folders first, then the game copy.
struct ResReader<'a> {
    files: HashMap<String, String>,
    game: &'a Game<'a>,
}

impl<'a> ResReader<'a> {
    fn new(roots: &[String], game: &'a Game<'a>) -> Self {
        let mut files = HashMap::new();
        for r in roots {
            let mut all = Vec::new();
            files_under(r, &mut all);
            for f in all.into_iter().filter(|f| f.ends_with(".res")) {
                let key = super::rel(&f, r).replace('\\', "/").to_lowercase();
                files.entry(key).or_insert(f);
            }
        }
        Self { files, game }
    }

    fn read(&self, name: &str) -> R<Option<Vec<u8>>> {
        if let Some(p) = self.files.get(&name.to_lowercase()) {
            return read(p).map(Some);
        }
        Ok(self.game.get()?.read(&name.to_lowercase()))
    }
}

/// One in-depth gap: (mesh name, lod, part, shader, database, why).
type PartGap = (String, usize, String, String, String, String);

/// `part_gaps(ix, reader, full, mesh_part, set_ref, set_number)`.
#[allow(clippy::too_many_lines, reason = "the Python's checks in its order")]
fn part_gaps(
    ix: &dyn Xml,
    reader: &ResReader<'_>,
    full: &[(String, Full)],
    mesh_part: &str,
    set_ref: Option<&str>,
    set_number: usize,
) -> R<Vec<PartGap>> {
    let mx = ix.xml(mesh_part)?;
    let Some(m) = MESH_NAMED.captures(mx.as_deref().unwrap_or("")) else {
        return Ok(Vec::new());
    };
    let typ = format!("{}MeshSet", &m[1]);
    let name = m[2].to_owned();
    let Some(kind) = bc2_formats::meshdata::Kind::of(&typ)
        .filter(|_| ["RigidMeshSet", "CompositeMeshSet", "SkinnedMeshSet"].contains(&typ.as_str()))
    else {
        return Ok(Vec::new());
    };
    let gap =
        |lod: usize, what: &str, why: String| (name.clone(), lod, what.to_owned(), "-".to_owned(), "-".to_owned(), why);
    // a mesh asset whose mesh set is missing or does not parse is a failure, not a pass
    let Some(data) = reader.read(&format!("{name}.res"))? else {
        return Ok(vec![gap(
            0,
            "(mesh set)",
            format!("{name}.res not found - not checked"),
        )]);
    };
    let Ok(ms) = bc2_formats::meshdata::parse_set(&data, kind, false, None) else {
        return Ok(vec![gap(
            0,
            "(mesh set)",
            format!(
                "{name}.res does not parse as a {typ} ({typ}: a mesh set the reader does not read (cut short or \
                 damaged)) - not checked"
            ),
        )]);
    };
    let over = set_surfaces(ix, set_ref)?;
    let mut out = Vec::new();
    for (lod, l) in ms.lods.iter().enumerate() {
        if l.sets.is_empty() {
            continue;
        }
        let d = reader
            .read(&format!("{name}_lod{lod}_data.res"))?
            .filter(|d| !d.is_empty());
        let Some(d) = d else {
            out.push(gap(
                lod,
                "(lod data)",
                format!("{name}_lod{lod}_data.res not found - not checked"),
            ));
            continue;
        };
        let Ok(head) = bc2_formats::meshdata::parse_mesh(&d, false) else {
            out.push(gap(
                lod,
                "(lod data)",
                format!(
                    "{name}_lod{lod}_data.res does not parse (mesh data: {} bytes the header reader does not read \
                     (cut short or damaged)) - not checked",
                    d.len()
                ),
            ));
            continue;
        };
        let i = if set_number < l.sets.len() { set_number } else { 0 };
        let shaders: Vec<String> = l.sets[i]
            .1
            .iter()
            .map(|x| x.iter().map(|&c| char::from(c)).collect())
            .collect();
        for (j, p) in head.parts.iter().enumerate() {
            let Some(sh) = shaders.get(j) else { continue };
            let mut shader = sh.clone();
            let leaf = shader.rsplit('/').next().unwrap_or(&shader).to_lowercase();
            if let Some(o) = over.get(&leaf) {
                o.rsplit_once('/')
                    .map_or(o.as_str(), |(a, _)| a)
                    .clone_into(&mut shader);
            }
            let pair = format!("{name}/{shader}").to_lowercase();
            if KNOWN.is_match(&pair) {
                continue;
            }
            let pn: String = p.name.iter().map(|&c| char::from(c)).collect();
            let zonly = pn.to_lowercase().contains("_zonly");
            let fa = p.bytes[2];
            let want: Vec<u8> = if fa == 0 { vec![0] } else { vec![fa, fa | 8] };
            for (dx, (kinds, parts)) in full {
                let ent = kinds.get(&pair);
                let lg = parts.get(&format!("{name}_lod{lod}/{pn}/standard").to_lowercase());
                let why = match (ent, lg) {
                    (None, _) => "no entry".to_owned(),
                    (Some(_), None) => "no part layout".to_owned(),
                    (Some(e), Some(lg)) => match e.get(lg) {
                        None => "no program for its layout".to_owned(),
                        Some(s) if !zonly && !s.iter().any(|(ps, _)| *ps == 0) => "no main-pass program".to_owned(),
                        Some(s)
                            if !s
                                .iter()
                                .any(|(ps, k)| *ps == if zonly { 5 } else { 0 } && want.contains(k)) =>
                        {
                            format!("no program of its skinning kind {fa}")
                        }
                        Some(_) => continue,
                    },
                };
                out.push((
                    name.clone(),
                    lod,
                    pn.clone(),
                    shader.rsplit('/').next().unwrap_or(&shader).to_owned(),
                    dx.clone(),
                    why,
                ));
            }
        }
    }
    Ok(out)
}

/// `spawned_vehicles(level)`: {(vehicle partition, MeshShaderSetNumber)} of a level's vehicle spawns.
fn spawned_vehicles(level: &str) -> BTreeSet<(String, usize)> {
    let mut out = BTreeSet::new();
    for c in VSPAWN.captures_iter(level) {
        let b = &c[1];
        if let Some(v) = VEHICLE.captures(b) {
            let n = SETNO.captures(b).and_then(|m| m[1].parse().ok()).unwrap_or(0);
            out.insert((v[1].to_lowercase(), n));
        }
    }
    out
}

/// `vehicle_meshes(ix, vehicle)`: the mesh partitions a vehicle draws.
fn vehicle_meshes(ix: &dyn Xml, vehicle: &str) -> R<BTreeSet<String>> {
    let mut out = BTreeSet::new();
    let mut seen = std::collections::HashSet::new();
    let mut stack = vec![vehicle.to_owned()];
    while let Some(part) = stack.pop() {
        if !seen.insert(part.clone()) {
            continue;
        }
        let Some(x) = ix.xml(&part)?.filter(|x| !x.is_empty()) else {
            continue;
        };
        if MESH_ASSET.is_match(&x) {
            out.insert(part);
            continue;
        }
        for c in REF.captures_iter(&x) {
            let p = c[1].to_lowercase();
            if (p.starts_with("objects/vehicles/") || p.starts_with("objects/weapons/stationary/"))
                && !seen.contains(&p)
            {
                stack.push(p);
            }
        }
    }
    Ok(out)
}

fn last_seg(s: &str) -> &str {
    s.rsplit('/').next().unwrap_or(s)
}

/// `vehicles_in_depth(ix, reader, full, level)`: lines.
fn vehicles_in_depth(
    ix: &Index,
    game: &Game<'_>,
    reader: &ResReader<'_>,
    full: &[(String, Full)],
    level: &str,
) -> R<BTreeSet<String>> {
    let wg = WithGame {
        ix,
        game,
        cache: RefCell::new(HashMap::new()),
    };
    let mut out = BTreeSet::new();
    for (vehicle, number) in spawned_vehicles(level) {
        let meshes = vehicle_meshes(&wg, &vehicle)?;
        if meshes.is_empty() {
            out.insert(format!(
                "{}: no mesh found (its partition in no bundle read) - not checked",
                last_seg(&vehicle)
            ));
        }
        for mp in &meshes {
            for (name, lod, pn, shader, dx, why) in part_gaps(&wg, reader, full, mp, None, number)? {
                out.insert(format!(
                    "{} (set {number}): {} lod{lod} {pn} -> {shader} {dx}: {why}",
                    last_seg(&vehicle),
                    last_seg(&name)
                ));
            }
        }
    }
    Ok(out)
}

/// `placed_in_depth(ix, reader, full, level)`.
fn placed_in_depth(
    ix: &Index,
    reader: &ResReader<'_>,
    full: &[(String, Full)],
    level: Option<&str>,
) -> R<BTreeSet<PartGap>> {
    let mut out = BTreeSet::new();
    for c in INST.captures_iter(level.unwrap_or("")) {
        if !c[2].ends_with("ReferenceObjectData") {
            continue;
        }
        let b = &c[3];
        let mp = match REFERENCED.captures(b) {
            Some(ro) => mesh_of(ix, &ro[1].rsplit_once('/').map_or(&ro[1], |(a, _)| a).to_lowercase())?,
            None => None,
        };
        let Some(mp) = mp else { continue };
        let ms = SHADER_SET.captures(b).map(|m| m[1].to_owned());
        out.extend(part_gaps(ix, reader, full, &mp, ms.as_deref(), 0)?);
    }
    Ok(out)
}

/// `seats_in_depth(ix, level)`: spawned vehicles' player seats whose PC input is not their PC version's (or DICE's
/// role rule's).
fn seats_in_depth(ix: &Index, game: &Game<'_>, level: &str) -> R<Vec<String>> {
    let g = game.get()?;
    let t90 = g
        .read("objects/vehicles/land/t90/t90.dbx")
        .ok_or_else(|| OnsError::Data("objects/vehicles/land/t90/t90.dbx: in no stock bundle".into()))?;
    let gunner =
        super::input::gunner_mapping(&bc2_formats::dbx::to_xml(&t90).map_err(|e| OnsError::Data(e.to_string()))?)?;
    let short = |r: &str| -> String {
        if r.contains('/') {
            let v: Vec<&str> = r.split('/').collect();
            v[v.len() - 2].to_owned()
        } else if r.is_empty() {
            "null".to_owned()
        } else {
            r.to_owned()
        }
    };
    let vehicles: BTreeSet<String> = spawned_vehicles(level).into_iter().map(|(v, _)| v).collect();
    let mut out = Vec::new();
    for vehicle in vehicles {
        let x = ix.xml(&vehicle);
        let stock = g.read(&format!("{vehicle}.dbx"));
        let Some(x) = x else {
            if stock.is_none() {
                out.push(format!("{vehicle}: found in no bundle"));
            }
            continue;
        };
        let base = ONS_TAIL.replace(&vehicle, "").into_owned();
        let cp = g.read(&format!("{base}.dbx"));
        let cpx = match &cp {
            Some(d) => Some(bc2_formats::dbx::to_xml(d).map_err(|e| OnsError::Data(e.to_string()))?),
            None => None,
        };
        let want: HashMap<String, (String, String)> = super::input::role_inputs(&x, cpx.as_deref(), &gunner)?
            .into_iter()
            .collect();
        for c in super::input::INST.captures_iter(&x) {
            let (gl, ty, b) = (c[2].to_lowercase(), &c[3], &c[4]);
            if !ty.ends_with("PlayerEntryComponentData") {
                continue;
            }
            let Some(w) = want.get(&gl) else { continue };
            let have = (
                CONCEPT.captures(b).map_or("null".to_owned(), |m| m[1].to_owned()),
                MAPPING.captures(b).map_or("null".to_owned(), |m| m[1].to_owned()),
            );
            if have.0 == "null" && have.1 == "null" {
                continue; // an AI seat (DICE's own _adv vehicles leave theirs null)
            }
            if have != *w {
                let order = ORDER
                    .captures(b)
                    .ok_or_else(|| OnsError::Data("not found: EntryOrderNumber".into()))?[1]
                    .to_owned();
                out.push(format!(
                    "{} seat {order}: {}/{}, {}: {}/{}",
                    last_seg(&vehicle),
                    short(&have.0),
                    short(&have.1),
                    if cp.is_some() {
                        "its PC version"
                    } else {
                        "the role rule"
                    },
                    short(&w.0),
                    short(&w.1)
                ));
            }
        }
    }
    Ok(out)
}

/// `pc_pairs()`: every (mesh, shader) pair some stock PC level's Dx9 database has, cached in
/// `work\console_shaders\pc_pairs.txt` under a stamp of the level bundles it was made from.
fn pc_pairs(root: &str, game: &str) -> R<BTreeSet<String>> {
    let cache = joins(root, &["work", "console_shaders", "pc_pairs.txt"]);
    let found = level_bundles(game);
    let mut levels: BTreeSet<String> = BTreeSet::new();
    let mut stamp_parts = Vec::new();
    for p in &found {
        let parent = std::path::Path::new(p)
            .parent()
            .and_then(|d| d.file_name())
            .map(|n| n.to_string_lossy().into_owned());
        levels.extend(parent);
        let md = std::fs::metadata(p).map_err(|e| super::io_err(p, &e))?;
        let t = md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_secs());
        stamp_parts.push(format!("{},{},{t}", super::rel(p, game), md.len()));
    }
    let stamp = format!("#stamp {}", stamp_parts.join(";"));
    if std::path::Path::new(&cache).is_file() {
        let t = String::from_utf8_lossy(&read(&cache)?).replace("\r\n", "\n");
        let lines: Vec<&str> = t.split('\n').collect();
        if lines.first() == Some(&stamp.as_str()) {
            return Ok(lines[1..].iter().map(|s| (*s).to_owned()).collect());
        }
    }
    let mut pairs = BTreeSet::new();
    let opfor = env_of(root);
    let env = shadercmd::Env {
        opfor: &opfor,
        on_bundle_read: &|| {},
    };
    let ports: Vec<&str> = super::shaders::PC_MAP.iter().map(|(l, _)| *l).collect();
    for lv in &levels {
        if ports.contains(&lv.as_str()) || lv.starts_with("ons_") {
            continue; // ports (ours, Napisal's): not stock evidence
        }
        let l = match shadercmd::load(&format!("level:{lv}"), Some(9), &env) {
            Ok(l) => l,
            Err(shadercmd::CmdError::Shader(_)) => continue, // a level without its own databases
            Err(e) => return Err(shader_err(&e)),
        };
        let db = l.db().map_err(|e| shader_err(&e))?;
        pairs.extend(db.entries.iter().map(|e| latin_lower(&e.name)));
    }
    let mut text = stamp;
    for p in &pairs {
        text.push('\n');
        text.push_str(p);
    }
    write(&cache, text.as_bytes())?;
    Ok(pairs)
}

/// `sorted(glob(<game>\*\**\levels\*\level-00.fbrb, recursive=True))`: every level bundle of the game copy, the
/// literal parts spelled as the pattern spells them.
fn level_bundles(game: &str) -> Vec<String> {
    fn listdir(d: &str) -> Vec<(String, bool)> {
        let Ok(rd) = std::fs::read_dir(d) else {
            return Vec::new();
        };
        let mut v: Vec<(String, bool)> = rd
            .flatten()
            .map(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                let dir = std::path::Path::new(&join(d, &n)).is_dir();
                (n, dir)
            })
            .filter(|(n, _)| !n.starts_with('.'))
            .collect();
        v.sort();
        v
    }
    fn all_dirs(d: &str, out: &mut Vec<String>) {
        out.push(d.to_owned());
        for (n, dir) in listdir(d) {
            if dir {
                all_dirs(&join(d, &n), out);
            }
        }
    }
    let mut found = BTreeSet::new();
    for (top, dir) in listdir(game) {
        if !dir {
            continue;
        }
        let mut dirs = Vec::new();
        all_dirs(&join(game, &top), &mut dirs);
        for d in dirs {
            let lv = join(&d, "levels");
            if !is_dir(&lv) {
                continue;
            }
            for (n, dir) in listdir(&lv) {
                let f = joins(&lv, &[&n, "level-00.fbrb"]);
                if dir && std::path::Path::new(&f).exists() {
                    found.insert(f);
                }
            }
        }
    }
    found.into_iter().collect()
}

/// What the audit is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder: `work\console_shaders\` written (the audit's unpack, the shared unpacks, `pc_pairs.txt`).
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The level (lower case).
    pub lv: String,
    /// Another build of the level (`--bundle`), else its installed Package `level-00`.
    pub bundle: Option<String>,
}

/// `audit(lv, bundle)`: the lines printed; true when nothing is missing.
#[allow(clippy::too_many_lines, reason = "the Python's audit, step by step")]
pub fn audit(a: &Args, out: &mut StageOut) -> R<bool> {
    let lv = a.lv.as_str();
    let name = lv.to_uppercase();
    let pc = super::shaders::PC_MAP
        .iter()
        .find(|(l, _)| *l == lv)
        .map(|(_, p)| *p)
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{lv}'")))?;
    let pcu = pc.to_uppercase();
    let work = joins(&a.root, &["work", "console_shaders"]);
    let base = joins(&work, &[lv, "audit"]);
    rmtree(&base)?;
    let bundle = a
        .bundle
        .clone()
        .unwrap_or_else(|| joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]));
    fbrb(&["unpack", &bundle, &base])?;
    let mut shared = Vec::new();
    for (key, where_, level, sub) in SHARED {
        let d = joins(&work, &["shared", key]);
        if !is_dir(&d) {
            let b = joins(&a.game, &[where_, "win32", "levels", level, &format!("{sub}-00.fbrb")]);
            fbrb(&["unpack", &b, &d])?;
        }
        shared.push(d);
    }
    let pcdir = joins(&a.root, &["work", "onslaught", &format!("build_{lv}"), "pc"]);
    let mut pcs = Vec::new();
    for s in ["level", "ingame"] {
        for w in ["pkg", "dist"] {
            let d = join(&pcdir, &format!("{pc}_{s}_{w}"));
            if is_dir(&d) {
                pcs.push(d);
            }
        }
    }
    // the control: the stock PC map with its own databases (a pair it lacks too is not drawn)
    let stock_roots: Vec<String> = pcs.iter().chain(&shared).cloned().collect();
    let stock_ix = Index::new(&stock_roots);
    let stock_db = pcs
        .iter()
        .find(|d| is_dir(&joins(d, &["Levels", &pcu, "Shaders"])))
        .ok_or_else(|| OnsError::Data("StopIteration".into()))?
        .clone();
    let (stock_gaps, _) = gaps_of(
        &stock_ix,
        &format!("levels/{pc}"),
        &databases(&a.root, &stock_db, &pcu)?,
    )?;
    let stock_pairs: BTreeSet<(String, String, String)> = stock_gaps
        .iter()
        .flat_map(|((m, how), items)| {
            items
                .iter()
                .map(move |(s, _)| (m.to_lowercase(), how.clone(), s.clone()))
        })
        .collect();
    let roots: Vec<String> = std::iter::once(base.clone())
        .chain(pcs.iter().cloned())
        .chain(shared.iter().cloned())
        .collect();
    let ix = Index::new(&roots);
    let level_part = format!("levels/{}", name.to_lowercase());
    let (gaps, counts) = gaps_of(&ix, &level_part, &databases(&a.root, &base, &name)?)?;
    let drawn = pc_pairs(&a.root, &a.game)?;
    let mut kept: Gaps = BTreeMap::new();
    for (k, v) in gaps {
        let items: Vec<(String, Vec<String>)> = v
            .into_iter()
            .filter(|(s, _)| !stock_pairs.contains(&(k.0.to_lowercase(), k.1.clone(), s.clone())))
            .filter(|(s, _)| {
                let pre = format!("{}/", k.0.to_lowercase());
                let suf = format!("/{s}");
                drawn.iter().any(|p| p.starts_with(&pre) && p.ends_with(&suf))
            })
            .collect();
        if !items.is_empty() {
            kept.insert(k, items);
        }
    }
    out.printed.push(format!(
        "{lv}: {} placed objects and {} reached meshes ({} partitions followed); {} pairs without a shader entry that \
         stock {pcu} draws with ({} pairs stock lacks too, not counted)",
        counts.0,
        counts.1,
        counts.2,
        kept.values().map(Vec::len).sum::<usize>(),
        stock_pairs.len()
    ));
    for ((mesh, how), items) in &kept {
        let dxs: BTreeSet<&str> = items.iter().flat_map(|(_, m)| m.iter().map(String::as_str)).collect();
        let mut shaders: Vec<&str> = items.iter().map(|(s, _)| s.as_str()).collect();
        shaders.sort_unstable();
        out.printed.push(format!(
            "  {mesh} [{how}] ({}): {}",
            dxs.into_iter().collect::<Vec<_>>().join("/"),
            shaders.join(", ")
        ));
    }
    // in depth: what the engine looks a part's program up by, for the spawned vehicles and the placed objects; and
    // the seats' PC input
    let game = Game {
        root: &a.root,
        ix: RefCell::new(None),
    };
    let reader = ResReader::new(&roots, &game);
    let full = full_databases(&a.root, &base, &name)?;
    let level = ix.xml(&level_part);
    let vehicle_gaps = vehicles_in_depth(&ix, &game, &reader, &full, level.as_deref().unwrap_or(""))?;
    let placed = placed_in_depth(&ix, &reader, &full, level.as_deref())?;
    let stock_full = full_databases(&a.root, &stock_db, &pcu)?;
    let stock_level = stock_ix.xml(&format!("levels/{pc}"));
    let stock_placed = placed_in_depth(&stock_ix, &reader, &stock_full, stock_level.as_deref())?;
    let placed: BTreeSet<PartGap> = placed.into_iter().filter(|k| !stock_placed.contains(k)).collect();
    let seats = seats_in_depth(&ix, &game, level.as_deref().unwrap_or(""))?;
    out.printed.push(format!(
        "  in depth: {} vehicle part gaps (each spawn's shader set: entry, layout, pass, skinning kind), {} \
         placed-object part gaps stock {pcu} does not share, {} seats without their PC input",
        vehicle_gaps.len(),
        placed.len(),
        seats.len()
    ));
    let mut placed_lines: Vec<String> = placed
        .iter()
        .map(|(m, lod, p, s, dx, why)| format!("{m} lod{lod} {p} -> {s} {dx}: {why}"))
        .collect();
    placed_lines.sort();
    let mut seat_lines = seats.clone();
    seat_lines.sort();
    for line in vehicle_gaps
        .iter()
        .take(40)
        .chain(placed_lines.iter().take(40))
        .chain(seat_lines.iter())
    {
        out.printed.push(format!("    {line}"));
    }
    rmtree(&base)?;
    Ok(kept.is_empty() && vehicle_gaps.is_empty() && placed.is_empty() && seats.is_empty())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn a_mesh_set_that_does_not_parse_is_a_gap_not_a_pass() {
        // QA 2026-11-14: part_gaps returned no gaps - a pass - for a mesh whose mesh set does not parse
        let d = std::env::temp_dir().join(format!("audit-gaps-{}", std::process::id()));
        let o = d.join("objects").join("x");
        std::fs::create_dir_all(&o).unwrap();
        std::fs::write(o.join("x_mesh.dbx"), b"x").unwrap();
        std::fs::write(
            o.join("x_mesh.xml"),
            "<instance guid=\"m\" type=\"Render.RigidMeshAsset\">\n<field name=\"Name\">objects/x/x_mesh</field>\n</instance>",
        )
        .unwrap();
        std::fs::write(o.join("x_mesh.res"), b"junk").unwrap();
        let root = d.to_string_lossy().into_owned();
        let ix = Index::new(std::slice::from_ref(&root));
        let game = Game {
            root: "no such folder",
            ix: RefCell::new(None),
        };
        let reader = ResReader::new(std::slice::from_ref(&root), &game);
        let gaps = part_gaps(&ix, &reader, &[], "objects/x/x_mesh", None, 0).unwrap();
        assert_eq!(gaps.len(), 1);
        assert!(gaps[0].5.contains("does not parse as a RigidMeshSet"), "{gaps:?}");
        std::fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn instances_surfaces_and_known_gaps() {
        let x = r#"<instance guid="a" type="T1">one</instance><instance guid="B" type="T2">two</instance><instance guid="A" type="T3">three</instance>"#;
        let i = insts(x);
        assert_eq!(i.len(), 2);
        assert_eq!(i[0], ("A".into(), "T3".into(), "three".into()));
        let b = r#"<field name="Name">Metal</field>
<field ref="objects/x/shaders/metal/1234" name="Shader" />"#;
        assert_eq!(surfaces(b)["metal"], "objects/x/shaders/metal/1234");
        assert!(KNOWN.is_match("objects/w/m_mesh/kitupgrade_3b"));
        assert!(KNOWN.is_match("objects/objects/roadsigns/speedsign_40/speedsign_40_mesh/x"));
        assert!(!KNOWN.is_match("objects/objects/roadsigns/speedsign_50/speedsign_50_mesh/x"));
        let lv = r#"<instance guid="s" type="Entity.VehicleSpawnEntityData"><field ref="Objects/Vehicles/Land/T90/T90/00000000-0000-0000-0000-000000000000" name="Vehicle" /><field name="MeshShaderSetNumber">2</field></instance>"#;
        assert_eq!(
            spawned_vehicles(lv).into_iter().collect::<Vec<_>>(),
            [("objects/vehicles/land/t90/t90".to_owned(), 2)]
        );
    }
}
