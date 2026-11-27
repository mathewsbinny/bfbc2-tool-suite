//! Stage 8 of the console Onslaught port (round OR11): the port of Project OPFOR's `console_markv.py`'s build and
//! `install` up to the ledger - Valparaiso Onslaught's Mark V patrol boat brought to PC:
//!
//! - its console resources converted: the physics ([`bc2_convert::havok::convert`]), each mesh LOD with each part in
//!   the vertex layout its donor material's programs were compiled for ([`bc2_convert::meshconv::convert`], the
//!   console's fifth two-half element left out), the mesh set with the PC LOD sizes, textures the level lacks (the
//!   user's PC bundles first, else the console's converted);
//! - its partitions back in the level bundle (the vehicle and what it reaches that is the boat's own);
//! - its nine materials' shader entries cloned from PC donors of the same shader and texture roles ([`MATERIALS`])
//!   into the level's Dx9 / Dx10 / Dx11 databases, with its mesh parts' layouts;
//! - its spawn back in the level, as the level was before `console_vehicles.py` (stage 5) dropped it.
//!
//! The work folder's files are the retired Python's byte for byte (rust/BENCHMARKS.md "OR11"); the ledger's parts
//! (the check before the build, the bundle before stage 5, the install, the revert) stay in OPFOR's
//! `console_markv.py`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

use bc2_convert::meshconv;
use bc2_formats::meshdata::{self, Kind as SetKind};
use bc2_formats::shaderdb::merge::{ArenaStore, Kind, Merger, entry_programs, latin, program_refs};
use bc2_formats::shaderdb::{self, ShaderDb, query};
use bc2_pyrt::py::OMap;
use regex::Regex;

use super::files::{drop_xml, glob_fbrb, manifest_lines, opfor_installed, under, write_manifest, xml_of};
use super::port::{StageOut, find, write_text_utf8};
use super::{OnsError, R, Source, dbx, fbrb, join, joins, pack, read, rmtree, size, write};

/// The level the Mark V is Valparaiso's.
pub const LV: &str = "ons_mp_002";
const NAME: &str = "ONS_MP_002";
const BOAT: &str = "Objects/Vehicles/Sea/MarkV/";
const MESH: &str = "Objects/Vehicles/Sea/MarkV/MarkV_Mesh";
const VEHICLE: &str = "objects/vehicles/sea/markv/markv_ons";
const T: &str = "Textures/Vehicles/";
const VADS: &str = "Textures/Weapons/Stationary/VADS/";
const PBL_LV: &str = "bc1_oasis_gr";
const PBL: &str = "Objects/Vehicles/Sea/Pbl/Pbl_Mesh/Objects/Vehicles/Sea/Pbl/Shaders/Main";
const AH64: &str = "Objects/Vehicles/Air/AH64/AH64_Mesh/Objects/Vehicles/Air/AH64/shaders/";

/// One Mark V material's donor: (material, donor level, donor entry, texture renames in their order).
pub type Donor = (&'static str, &'static str, String, Vec<(String, String)>);

/// The Mark V's materials and their PC donors (`console_markv.MATERIALS`, in its order): each donor shares the
/// material's shader and texture roles (the console database's `Single.res`: the materials' textures, 2026-10-30); the
/// boat's main material is the PC PBL patrol boat's shader with the boat's own textures.
#[must_use]
pub fn materials() -> Vec<Donor> {
    let t = |a: &str, b: &str| (format!("{T}{a}"), b.to_owned());
    let mut out: Vec<Donor> = vec![
        (
            "Main",
            PBL_LV,
            PBL.into(),
            vec![
                t("PBL/PBL_C", &format!("{T}MarkV/MarkV_C")),
                t("PBL/PBL_D", &format!("{T}MarkV/MarkV_D")),
                t("PBL/PBL_N", &format!("{T}MarkV/MarkV_N")),
            ],
        ),
        (
            "MarkV_Vads",
            PBL_LV,
            PBL.into(),
            vec![
                t("PBL/PBL_C", &format!("{VADS}VADS_MarkV_C")),
                t("PBL/PBL_D", &format!("{VADS}VADS_d")),
                t("PBL/PBL_N", &format!("{VADS}VADS_n")),
                t("Humvee/HumveeWreck", &format!("{T}ACVS/ACVS_Wreck")),
            ],
        ),
    ];
    out.push((
        "RocletLauncher",
        "mp_005gr",
        "Objects/Weapons/Stationary/CruisePlatform/CruisePlatform_Mesh/Objects/Weapons/Stationary/CruisePlatform/Shaders/\
         Main"
            .into(),
        "CDN"
            .chars()
            .map(|x| {
                (
                    format!("Textures/Weapons/Stationary/CruiseMissile/CruiseMissile_{x}"),
                    format!("{T}MarkV/MarkVLauncher_{}", x.to_ascii_lowercase()),
                )
            })
            .collect(),
    ));
    out.push(("Wreck", PBL_LV, format!("{AH64}Wreck"), Vec::new()));
    out.push((
        "Cockpit",
        "mp_002",
        "Objects/Vehicles/Air/AH60/AH60_Mesh/Objects/Vehicles/Air/AH60/shaders/Cockpit".into(),
        Vec::new(),
    ));
    out.push((
        "LCD",
        "mp_005",
        "Objects/Objects/CruiseMissilePlatform/CruiseMissilePlatform_Mesh/Objects/Objects/CruiseMissilePlatform/\
         Shaders/Screen"
            .into(),
        Vec::new(),
    ));
    out.push((
        "MarkV_LOD",
        PBL_LV,
        format!("{AH64}AH64_LOD"),
        vec![t("AH64/Apache_C", &format!("{T}MarkV/MarkV_C"))],
    ));
    out.push((
        "MarkV_Vads_LOD",
        PBL_LV,
        format!("{AH64}AH64_LOD"),
        vec![t("AH64/Apache_C", &format!("{VADS}VADS_MarkV_C"))],
    ));
    out.push(("Glass", PBL_LV, format!("{AH64}AH64_Mesh_Glass"), Vec::new()));
    out
}

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

fn re_bytes(p: &str) -> regex::bytes::Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    regex::bytes::Regex::new(p).expect("a constant pattern")
}

static REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-\.]+)/[0-9A-Fa-f-]{36}""#));
/// `TEXNAME` without its look-ahead: the terminating zero is matched and left out of the name (a match never starts at
/// a zero, so the next search starts where Python's would).
static TEXNAME: LazyLock<regex::bytes::Regex> =
    LazyLock::new(|| re_bytes(r"(?-u)((?:Textures|Objects|Characters|Terrains)/[ -~]{3,120}?)\x00"));
static LOD_NAME: LazyLock<regex::bytes::Regex> =
    LazyLock::new(|| re_bytes(r"(?-u)([ -~]{4,300}?_lod[0-9]+/[ -~]+?)\x00"));
static SKIP_BUNDLE: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)[\\/](ons_|opfor_)"));
/// `console_common.INST`.
static INST: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)(<instance guid="([^"]+)" type="([^"]+)"[^>]*>)(.*?)(</instance>)"#));

/// The stage's arguments.
#[derive(Debug, Clone)]
pub struct Args {
    /// Project OPFOR's folder.
    pub root: String,
    /// Its game copy.
    pub game: String,
    /// The bundle the stage builds from (the ledger's `source_bundle`: the level as stage 7 left it, or this stage's
    /// own prior on a redo).
    pub src_bundle: String,
    /// The level as it was before stage 5 dropped the spawn (the ledger's `before_path` of `vehicles:ons_mp_002`).
    pub pre_bundle: String,
    /// `--apply` (else `--plan`: built and reported, nothing to install).
    pub write: bool,
}

struct Paths {
    work: String,
    bundle: String,
    /// The bundle the stage builds from: its databases and its textures are the base (spec XCONS-ONS-8).
    source: String,
    con: String,
    con_mp2: String,
    game: String,
    root: String,
}

impl Paths {
    fn new(a: &Args) -> Self {
        let con = joins(&a.root, &["work", "onslaught", &format!("build_{LV}"), "con_level"]);
        let con_mp2 = joins(&a.root, &["work", "onslaught", &format!("build_{LV}"), "con_mp2"]);
        Self {
            work: joins(&a.root, &["work", "console_markv"]),
            bundle: joins(&a.game, &["Package", "levels", LV, "level-00.fbrb"]),
            source: a.src_bundle.clone(),
            con,
            con_mp2,
            game: a.game.clone(),
            root: a.root.clone(),
        }
    }
}

/// `console_files()`: {lower relative path: (path, manifest line)} of the console level and mp2 bundles (the level
/// first), the files that are there.
fn console_files(p: &Paths) -> R<OMap<String, (String, String)>> {
    let mut out: OMap<String, (String, String)> = OMap::default();
    for root in [&p.con, &p.con_mp2] {
        for l in manifest_lines(&join(root, "fbrb.manifest"))? {
            let n = l.split('|').next().unwrap_or_default().to_owned();
            let path = under(root, &n);
            if std::path::Path::new(&path).is_file() {
                out.entry_or(n.to_lowercase(), || (path, l.clone()));
            }
        }
    }
    Ok(out)
}

/// A database as `shaderdb.load(spec, dx)` reads it: its bytes, the dx it keeps, its label.
struct Loaded {
    data: Vec<u8>,
    dx: u32,
    label: String,
}

impl Loaded {
    fn open(&self) -> R<(ShaderDb<'_>, Kind)> {
        let parse_dx = if self.dx == 11 { 10 } else { self.dx };
        let o = shaderdb::open::open(&self.data, Some(parse_dx), None, &self.label).map_err(OnsError::Exit)?;
        let kind = Kind {
            dx: self.dx,
            big: o.big_endian,
            key_size: o.db.key_size(),
            version: o.db.version,
        };
        Ok((o.db, kind))
    }
}

/// `shaderdb.level_bundles(root)`: {level folder (lower case): its level-00.fbrb}, Package over Dist.
fn level_bundles(root: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for top in [
        joins(root, &["game", "Dist", "win32", "levels"]),
        joins(root, &["game", "Package", "levels"]),
    ] {
        let Ok(rd) = std::fs::read_dir(&top) else { continue };
        for e in rd.flatten() {
            let lv = e.file_name().to_string_lossy().into_owned();
            let p = joins(&top, &[&lv, "level-00.fbrb"]);
            if std::path::Path::new(&p).is_file() {
                out.insert(lv.to_lowercase(), p);
            }
        }
    }
    out
}

/// `shaderdb.load(bundle, dx)` of a bundle's Dx`dx` database (`pick_database`: the only one, else the one in the
/// level `folder`'s own: `Levels/<folder>/Shaders/`).
fn load_bundle(bundle: &str, dx: u32, folder: &str) -> R<Loaded> {
    if !std::path::Path::new(bundle).is_file() {
        return Err(OnsError::Io(format!("[Errno 2] No such file: '{bundle}'")));
    }
    let h = bc2_gamefs::bundles::entries_path(bundle).map_err(|e| OnsError::Data(format!("{bundle}: {e}")))?;
    let dbs: Vec<(String, String)> = h
        .entries
        .iter()
        .filter(|e| e.type_name.ends_with("ShaderDatabase") && !e.is_deleted())
        .map(|e| (e.name.clone(), e.type_name.clone()))
        .collect();
    let name = shaderdb::open::pick_database(bundle, &dbs, dx, None, folder)
        .ok_or_else(|| OnsError::Data(format!("{bundle}: a database name past latin-1")))?
        .map_err(OnsError::Exit)?;
    let data = bc2_gamefs::bundles::read_path(bundle, &[name.as_str()])
        .map_err(|e| OnsError::Data(format!("{bundle}: {e}")))?
        .pop()
        .unwrap_or_default();
    Ok(Loaded {
        data,
        dx,
        label: format!("{bundle}:{name}"),
    })
}

/// A layout's GUID in each database kind (9, 10, 11).
type Guids = BTreeMap<u32, Vec<u8>>;
/// A mesh part's name, its layout's GUIDs and its donor level.
type PartLayout = (String, Guids, &'static str);

/// The databases the stage reads (`console_markv.db(spec, dx)`'s cache): the level's own and the donors'.
struct Dbs {
    loaded: HashMap<(String, u32), Loaded>,
}

impl Dbs {
    fn load(p: &Paths, donors: &[Donor]) -> R<Self> {
        let levels = level_bundles(&p.root);
        let mut loaded = HashMap::new();
        for dx in [9, 10, 11] {
            loaded.insert((p.source.clone(), dx), load_bundle(&p.source, dx, LV)?);
            for (_, lv, _, _) in donors {
                let spec = format!("level:{lv}");
                if loaded.contains_key(&(spec.clone(), dx)) {
                    continue;
                }
                let b = levels.get(*lv).ok_or_else(|| {
                    OnsError::Exit(format!(
                        "unknown level {lv}: the game copy has no levels\\{lv}\\level-00.fbrb"
                    ))
                })?;
                loaded.insert((spec, dx), load_bundle(b, dx, lv)?);
            }
        }
        Ok(Self { loaded })
    }

    fn get(&self, spec: &str, dx: u32) -> R<&Loaded> {
        self.loaded
            .get(&(spec.to_owned(), dx))
            .ok_or_else(|| OnsError::Data(format!("{spec} Dx{dx}: not loaded")))
    }
}

/// `dict(d.entries)[entry]`: the entry's GUID (the last entry of that name).
fn entry_guid<'d>(d: &'d ShaderDb<'_>, entry: &str) -> R<&'d [u8]> {
    d.entries
        .iter()
        .rev()
        .find(|e| latin(&e.name) == entry)
        .map(|e| e.guid)
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{entry}'")))
}

/// `d.entry_programs()[dict(d.entries)[entry]]`.
fn entry_program_list(d: &ShaderDb<'_>, entry: &str) -> R<Vec<usize>> {
    let g = entry_guid(d, entry)?;
    let ep = entry_programs(d);
    let progs = ep
        .get(g)
        .ok_or_else(|| OnsError::Data(format!("KeyError: {}", meshconv::bytes_repr(g))))?;
    Ok(progs.iter().map(|&i| usize::from(i)).collect())
}

/// `donor_layouts(lv, entry, dx)`: {layout GUID: layout record body} of the donor entry's programs, in their order.
fn donor_layouts(d: &ShaderDb<'_>, entry: &str) -> R<Vec<(Vec<u8>, Vec<u8>)>> {
    let mut lay: HashMap<&[u8], &[u8]> = HashMap::new();
    for r in &d.layouts {
        lay.insert(r.get(..16).unwrap_or(r), r.get(16..).unwrap_or_default());
    }
    let mut out: OMap<Vec<u8>, Vec<u8>> = OMap::default();
    for i in entry_program_list(d, entry)? {
        let k = d
            .keys
            .get(i)
            .ok_or_else(|| OnsError::Data(format!("IndexError: key {i}")))?;
        let g = k.get(32..48).unwrap_or_default();
        let r = lay
            .get(g)
            .ok_or_else(|| OnsError::Data(format!("KeyError: {}", meshconv::bytes_repr(g))))?;
        out.insert(g.to_vec(), r.to_vec());
    }
    Ok(out.items)
}

/// `choose(console, lv, entry)`: the donor's Dx9 layout (stream 0 only, the most elements, then the lowest GUID) the
/// console layout pairs into, and that layout's GUID in Dx9, Dx10 and Dx11 (an equal record).
fn choose(console: &[u8], dbs: &Dbs, lv: &str, entry: &str) -> R<(Vec<u8>, Guids)> {
    let spec = format!("level:{lv}");
    let ec = meshconv::elements(console)
        .ok_or_else(|| OnsError::Data(format!("{entry}: a console layout record cut short")))?
        .0;
    let (d9, _) = dbs.get(&spec, 9)?.open()?;
    let mut cands: Vec<(usize, Vec<u8>, Vec<u8>)> = Vec::new();
    for (g, r) in donor_layouts(&d9, entry)? {
        let Some((er, _)) = meshconv::elements(&r) else {
            return Err(OnsError::Data(format!("{entry}: a donor layout record cut short")));
        };
        if er.iter().any(|e| e.3 != 0) || meshconv::pair_by_kind(&ec, &er).is_none() {
            continue;
        }
        cands.push((er.len(), g, r));
    }
    cands.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)).then_with(|| a.2.cmp(&b.2)));
    let Some((_, g9, rec)) = cands.into_iter().next() else {
        return Err(OnsError::Exit(format!(
            "{entry}: no layout of the donor fits the Mark V"
        )));
    };
    let mut guids = BTreeMap::new();
    guids.insert(9, g9);
    for dx in [10, 11] {
        let (d, _) = dbs.get(&spec, dx)?.open()?;
        let same = donor_layouts(&d, entry)?.into_iter().find(|(_, r)| *r == rec);
        let Some((g, _)) = same else {
            return Err(OnsError::Exit(format!(
                "{entry}: the Dx{dx} donor has no layout equal to the Dx9 one"
            )));
        };
        guids.insert(dx, g);
    }
    Ok((rec, guids))
}

/// `console_layouts(path)`: mesh part name (lower case) -> layout record body, from the console level's shader
/// database (each part name followed by its layout's GUID; the layout record found where that GUID starts a record:
/// 1 - 16 elements, the byte after the count zero).
fn console_layouts(b: &[u8]) -> HashMap<String, Vec<u8>> {
    const REC: usize = 16 + 0x49;
    let mut names: HashMap<String, [u8; 16]> = HashMap::new();
    for c in LOD_NAME.captures_iter(b) {
        let (Some(m0), Some(m1)) = (c.get(0), c.get(1)) else {
            continue;
        };
        let g = b.get(m0.end()..m0.end() + 16).unwrap_or_default();
        let mut key = [0u8; 16];
        // a GUID cut short by the file's end: Python's slice is shorter and found nowhere below
        if g.len() == 16 {
            key.copy_from_slice(g);
        }
        let name: String = m1.as_bytes().iter().map(|&x| char::from(x)).collect();
        if g.len() == 16 {
            names.insert(name.to_lowercase(), key);
        } else {
            names.remove(&name.to_lowercase());
        }
    }
    let want: HashSet<[u8; 16]> = names.values().copied().collect();
    let mut found: HashMap<[u8; 16], Vec<u8>> = HashMap::new();
    // re.finditer(re.escape(g), b) for each GUID at once: a GUID's matches do not overlap (the next search starts at
    // the end of the last match)
    let mut next: HashMap<[u8; 16], usize> = HashMap::new();
    if b.len() >= 16 {
        for p in 0..=b.len() - 16 {
            let mut k = [0u8; 16];
            k.copy_from_slice(b.get(p..p + 16).unwrap_or(&[0; 16]));
            if !want.contains(&k) || next.get(&k).is_some_and(|&n| p < n) {
                continue;
            }
            next.insert(k, p + 16);
            if found.contains_key(&k) {
                continue;
            }
            if p + REC <= b.len() {
                let count = b.get(p + 16 + 64).copied().unwrap_or(0);
                let zero = b.get(p + 16 + 65).copied().unwrap_or(1);
                if count > 0 && count <= 16 && zero == 0 {
                    found.insert(k, b.get(p + 16..p + REC).unwrap_or_default().to_vec());
                }
            }
        }
    }
    names
        .into_iter()
        .filter_map(|(n, g)| found.get(&g).map(|r| (n, r.clone())))
        .collect()
}

/// `havok.classes(<game>\BFBC2Game.exe)`.
fn havok_classes(game: &str) -> R<bc2_convert::havok::Classes> {
    let exe = join(game, "BFBC2Game.exe");
    let data = read(&exe)?;
    let rows = bc2_convert::hkclasses::classes(&data).ok_or_else(|| {
        OnsError::Data(format!(
            "{exe}: no Havok class initializers found (not BFBC2Game.exe, or damaged)"
        ))
    })?;
    Ok(bc2_convert::havok::classes_of(rows))
}

/// `meshset.convert(b, 'CompositeMeshSet', lods)`: the console mesh set with each LOD's size the PC MeshData's
/// (vertex bytes + index bytes + 12, `lod_size`).
fn meshset_convert(b: &[u8], lods: &[Vec<u8>]) -> R<Vec<u8>> {
    let mut s = meshdata::parse_set(b, SetKind::Composite, true, None).map_err(|_| set_refused())?;
    if lods.len() != s.lods.len() {
        return Err(OnsError::Data(format!("{} LODs, {} given", s.lods.len(), lods.len())));
    }
    for (l, n) in s.lods.iter_mut().zip(lods) {
        let m = meshdata::parse_mesh(n, false).map_err(|_| mesh_refused(n))?;
        l.size = m.sizes[0] + m.sizes[1] + 12;
    }
    meshdata::write_set(&s, SetKind::Composite).map_err(|_| set_refused())
}

/// `meshset.parse`'s refusal of a set it does not read.
fn set_refused() -> OnsError {
    OnsError::Data("ConvertError: CompositeMeshSet: a mesh set the reader does not read (cut short or damaged)".into())
}

/// `mesh.parse`'s refusal of a header it does not read.
fn mesh_refused(b: &[u8]) -> OnsError {
    OnsError::Data(format!(
        "ConvertError: mesh data: {} bytes the header reader does not read (cut short or damaged)",
        b.len()
    ))
}

/// A bundle's live entries (name, lower case) - `fbrb.entries(path)` without the deleted ones.
fn live_names(bundle: &str) -> R<HashSet<String>> {
    let h = bc2_gamefs::bundles::entries_path(bundle).map_err(|e| OnsError::Data(format!("{bundle}: {e}")))?;
    Ok(h.entries
        .iter()
        .filter(|e| !e.is_deleted())
        .map(|e| e.name.to_lowercase())
        .collect())
}

/// `pc_resource(rel)`'s search: the user's PC bundles (Package before Dist, each in sorted path order; no Onslaught or
/// OPFOR builds), their headers read once.
struct PcBundles {
    order: Vec<String>,
    headers: HashMap<String, Option<Vec<(String, bool)>>>,
}

impl PcBundles {
    fn new(game: &str) -> Self {
        let mut order = Vec::new();
        for w in ["Package", "Dist\\win32"] {
            let mut found = Vec::new();
            glob_fbrb(&join(game, w), &mut found);
            found.sort();
            // the stock bundles only: no Onslaught or OPFOR builds, no level OPFOR installed (the WW2 levels: spec
            // XCONS-ONS-8)
            order.extend(
                found
                    .into_iter()
                    .filter(|b| !SKIP_BUNDLE.is_match(b) && !opfor_installed(&bc2_pyrt::py::dirname(b))),
            );
        }
        Self {
            order,
            headers: HashMap::new(),
        }
    }

    /// `pc_resource(rel)`: (bundle, entry name) of the first live entry of that name (any case).
    fn find(&mut self, rel: &str) -> Option<(String, String)> {
        let want = rel.to_lowercase();
        for b in &self.order {
            let h = self.headers.entry(b.clone()).or_insert_with(|| {
                bc2_gamefs::bundles::entries_path(b)
                    .ok()
                    .map(|h| h.entries.iter().map(|e| (e.name.clone(), e.is_deleted())).collect())
            });
            if let Some(es) = h
                && let Some((n, _)) = es.iter().find(|(n, d)| !d && n.to_lowercase() == want)
            {
                return Some((b.clone(), n.clone()));
            }
        }
        None
    }
}

/// What [`build`] made: the resources in their order ({name: (bytes, type)}), the partitions ({lower path: file}), the
/// three databases and the report.
struct Built {
    res: OMap<String, (Vec<u8>, String)>,
    parts: BTreeMap<String, String>,
    dbs: Vec<(u32, Vec<u8>)>,
    report: Vec<String>,
}

/// `build()`: everything into the work folder.
#[allow(clippy::too_many_lines, reason = "console_markv.build, step for step")]
fn build_all(p: &Paths) -> R<Built> {
    rmtree(&p.work)?;
    std::fs::create_dir_all(&p.work).map_err(|e| super::io_err(&p.work, &e))?;
    let files = console_files(p)?;
    let mut res: OMap<String, (Vec<u8>, String)> = OMap::default();
    // partitions: the vehicle and what it reaches that the console has and PC lacks (all under the boat's folder or
    // naming it), found from the blueprint
    let mut parts: BTreeMap<String, String> = BTreeMap::new();
    let mut stack = vec![VEHICLE.to_owned()];
    while let Some(q) = stack.pop() {
        let Some((f, _)) = files.get(&format!("{q}.dbx")) else {
            continue;
        };
        if parts.contains_key(&q) || !q.contains("markv") {
            continue;
        }
        parts.insert(q.clone(), f.clone());
        let x = xml_of(f)?;
        stack.extend(REF.captures_iter(&x).map(|c| c[1].to_lowercase()));
    }
    let mut short: Vec<&str> = parts.keys().map(|q| q.rsplit('/').next().unwrap_or(q)).collect();
    short.sort_unstable();
    let mut report = vec![format!("partitions: {} ({})", parts.len(), short.join(", "))];
    // physics
    let classes = havok_classes(&p.game)?;
    let boat = BOAT.to_lowercase();
    for (n, (path, line)) in &files.items {
        let cols: Vec<&str> = line.split('|').collect();
        if n.starts_with(&boat) && cols.get(1) == Some(&"HavokPhysicsData") {
            let pc = bc2_convert::havok::convert(&read(path)?, &classes)
                .map_err(|m| OnsError::Data(format!("ConvertError: {m}")))?;
            res.insert(cols[0].to_owned(), (pc, "HavokPhysicsData".into()));
        }
    }
    // meshes: each part in its donor material's layout
    let donors = materials();
    let dbs = Dbs::load(p, &donors)?;
    let single = first_single(&p.con)?;
    let clay = console_layouts(&read(&single)?);
    let need_file = |k: &str| -> R<&(String, String)> {
        files
            .get(&k.to_lowercase())
            .ok_or_else(|| OnsError::Data(format!("KeyError: '{}'", k.to_lowercase())))
    };
    let set_bytes = read(&need_file(&format!("{MESH}.res"))?.0)?;
    let sets = meshdata::parse_set(&set_bytes, SetKind::Composite, true, None).map_err(|_| set_refused())?;
    let mut sizes: Vec<Vec<u8>> = Vec::new();
    let mut part_layouts: Vec<PartLayout> = Vec::new();
    for (lod, l) in sets.lods.iter().enumerate() {
        let name = format!("{MESH}_lod{lod}_data.res");
        let b = read(&need_file(&name)?.0)?;
        let m = meshdata::parse_mesh(&b, true).map_err(|_| mesh_refused(&b))?;
        let shaders: Vec<String> = l.sets.first().map_or_else(Vec::new, |s| {
            s.1.iter()
                .map(|x| latin(x).rsplit('/').next().unwrap_or_default().to_owned())
                .collect()
        });
        let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        for (part, mat) in m.parts.iter().zip(&shaders) {
            let pn = latin(&part.name);
            let key = format!("{MESH}_lod{lod}/{pn}/standard").to_lowercase();
            let lc = clay
                .get(&key)
                .ok_or_else(|| OnsError::Data(format!("KeyError: '{key}'")))?;
            let d = donors
                .iter()
                .find(|d| d.0 == mat)
                .ok_or_else(|| OnsError::Data(format!("KeyError: '{mat}'")))?;
            let (rec, guids) = choose(lc, &dbs, d.1, &d.2)?;
            pairs.push((lc.clone(), rec));
            part_layouts.push((format!("{MESH}_lod{lod}/{pn}/Standard"), guids, d.1));
        }
        let layout_pairs: Vec<(&[u8], &[u8])> = pairs.iter().map(|(a, b)| (a.as_slice(), b.as_slice())).collect();
        let out =
            meshconv::convert::<std::collections::hash_map::RandomState>(&b, &layout_pairs, None).map_err(|e| {
                OnsError::Data(match e {
                    meshconv::Refusal::Error(m) => format!("ConvertError: {m}"),
                    meshconv::Refusal::Reference => format!("{name}: a mesh the converter does not read"),
                })
            })?;
        report.push(format!(
            "LOD{lod}: {} parts, {} bytes (console {})",
            m.parts.len(),
            bc2_pyrt::fmt::thousands(i128::try_from(out.len()).unwrap_or_default()),
            bc2_pyrt::fmt::thousands(i128::try_from(b.len()).unwrap_or_default())
        ));
        res.insert(name, (out.clone(), "MeshData".into()));
        sizes.push(out);
    }
    res.insert(
        format!("{MESH}.res"),
        (meshset_convert(&set_bytes, &sizes)?, "CompositeMeshSet".into()),
    );
    // shader databases: the installed ones with the boat's entries and mesh parts
    let mut out_dbs = Vec::new();
    for dx in [9, 10, 11] {
        out_dbs.push((dx, merged(p, &dbs, &donors, &part_layouts, dx)?));
    }
    // textures the cloned entries name: the level's own, else the user's PC copy, else the console's converted
    let mut names: HashSet<String> = HashSet::new();
    for (_, lv, entry, tex) in &donors {
        let (d, _) = dbs.get(&format!("level:{lv}"), 9)?.open()?;
        for pi in entry_program_list(&d, entry)? {
            let r = program_refs(&d, pi).map_err(|e| OnsError::Data(e.to_string()))?;
            for k in &r[2..] {
                let block = d.params.get(*k).copied().unwrap_or_default();
                for c in TEXNAME.captures_iter(block) {
                    // each material's names with its own renames: what its cloned entry names (spec XCONS-ONS-8; the
                    // retired Python renamed every name collected so far, so Main's HumveeWreck went unchecked)
                    let n: String = c[1].iter().map(|&x| char::from(x)).collect();
                    names.insert(tex.iter().find(|(a, _)| *a == n).map_or(n, |(_, b)| b.clone()));
                }
            }
        }
    }
    let level_has = live_names(&p.source)?;
    let mut shared = HashSet::new();
    for c in ["mp_common", "sp_common"] {
        shared.extend(live_names(&joins(
            &p.game,
            &["Dist", "win32", "levels", c, "level-00.fbrb"],
        ))?);
    }
    let mut sorted: Vec<String> = names.into_iter().collect();
    sorted.sort();
    let mut pcb = PcBundles::new(&p.game);
    for n in sorted {
        let rel = format!("{n}.res");
        let low = rel.to_lowercase();
        if level_has.contains(&low) || shared.contains(&low) {
            continue;
        }
        if let Some((b, e)) = pcb.find(&rel) {
            let data = bc2_gamefs::bundles::read_path(&b, &[e.as_str()])
                .map_err(|x| OnsError::Data(format!("{b}: {x}")))?
                .pop()
                .unwrap_or_default();
            res.insert(e, (data, "ITexture".into()));
            report.push(format!("texture {n}: from {}", super::rel(&b, &p.game)));
        } else if let Some((path, line)) = files.get(&low) {
            let pc = bc2_convert::texture::convert(&read(path)?)
                .map_err(|e| OnsError::Data(format!("ConvertError: {e}")))?;
            res.insert(
                line.split('|').next().unwrap_or_default().to_owned(),
                (pc, "ITexture".into()),
            );
            report.push(format!("texture {n}: converted from the console"));
        } else {
            return Err(OnsError::Exit(format!(
                "texture {n}: in no PC bundle and not in the console data"
            )));
        }
    }
    for (n, (data, _)) in &res.items {
        write(&under(&join(&p.work, "res"), n), data)?;
    }
    for (dx, data) in &out_dbs {
        write(&join(&p.work, &format!("Dx{dx}_Single.res")), data)?;
    }
    Ok(Built {
        res,
        parts,
        dbs: out_dbs,
        report,
    })
}

/// `glob(os.path.join(CON, 'levels', '*', 'Shaders', 'Single.res'))[0]`.
fn first_single(con: &str) -> R<String> {
    let top = join(con, "levels");
    if let Ok(rd) = std::fs::read_dir(&top) {
        for e in rd.flatten() {
            let p = joins(&top, &[&e.file_name().to_string_lossy(), "Shaders", "Single.res"]);
            if std::path::Path::new(&p).is_file() {
                return Ok(p);
            }
        }
    }
    Err(OnsError::Data(
        "IndexError: list index out of range (no console Single.res)".into(),
    ))
}

/// The level's Dx`dx` database with the boat's entries cloned from their donors and its mesh parts' layouts.
fn merged(p: &Paths, dbs: &Dbs, donors: &[Donor], part_layouts: &[PartLayout], dx: u32) -> R<Vec<u8>> {
    let msg = |e: shaderdb::merge::MergeError| OnsError::Exit(format!("ShaderDbError: {e}"));
    let arena = ArenaStore::new();
    let store = &arena;
    let (base, kind) = dbs.get(&p.source, dx)?.open()?;
    let mut donor_dbs: HashMap<&str, (ShaderDb<'_>, Kind)> = HashMap::new();
    for (_, lv, _, _) in donors {
        if !donor_dbs.contains_key(lv) {
            donor_dbs.insert(lv, dbs.get(&format!("level:{lv}"), dx)?.open()?);
        }
    }
    let mut m = Merger::new(base, kind, Vec::new(), false).map_err(msg)?;
    for (mat, lv, entry, tex) in donors {
        let (d, k) = donor_dbs
            .get(lv)
            .ok_or_else(|| OnsError::Data(format!("{lv}: no donor")))?;
        m.clone_entry(
            &store,
            d,
            k,
            entry,
            &format!("{MESH}/{BOAT}Shaders/{mat}"),
            tex,
            &format!("{lv} Dx{dx}"),
        )
        .map_err(msg)?;
    }
    for (part, guids, lv) in part_layouts {
        let (d, _) = donor_dbs
            .get(lv)
            .ok_or_else(|| OnsError::Data(format!("{lv}: no donor")))?;
        let g = guids.get(&dx).map_or(&[][..], Vec::as_slice);
        m.add_part(&store, part, g, d, lv).map_err(msg)?;
    }
    let out = m.finished().map_err(msg)?;
    let problems = check(&out)?;
    if !problems.is_empty() {
        let list: Vec<String> = problems.iter().map(|s| bc2_pyrt::fmt::str_repr(s)).collect();
        return Err(OnsError::Exit(format!("Dx{dx}: [{}]", list.join(", "))));
    }
    Ok(shaderdb::write(&out))
}

/// `ShaderDb.check()`.
fn check(db: &ShaderDb<'_>) -> R<Vec<String>> {
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
    query::check(&input).ok_or_else(|| OnsError::Data("the merged database does not read back".into()))
}

/// `_children(body)`: [(name, text)] of an instance body's direct children (the XML indents them two tabs).
#[must_use]
pub fn children(body: &str) -> Vec<(String, String)> {
    static NAME: LazyLock<Regex> = LazyLock::new(|| re(r#"name="([^"]+)""#));
    static CLOSE_END: LazyLock<Regex> = LazyLock::new(|| re(r"</\w+>[\s\x1c-\x1f]*$"));
    let name_of = |l: &str| NAME.captures(l).map_or(String::new(), |c| c[1].to_owned());
    let mut out = Vec::new();
    let mut cur: Option<(String, Vec<String>)> = None;
    for line in body.split('\n') {
        if line.starts_with("\t\t<") {
            if cur.is_some() && line.starts_with("\t\t</") {
                if let Some((n, mut ls)) = cur.take() {
                    ls.push(line.to_owned());
                    out.push((n, ls.join("\n")));
                }
                continue;
            }
            if line.trim_end_matches(bc2_pyrt::py::py_space).ends_with("/>") || CLOSE_END.is_match(line) {
                out.push((name_of(line), line.to_owned()));
            } else {
                cur = Some((name_of(line), vec![line.to_owned()]));
            }
        } else if let Some((_, ls)) = cur.as_mut() {
            ls.push(line.to_owned());
        }
    }
    out
}

/// `_norm(text)`: white space runs as one space, stripped.
fn norm(text: &str) -> String {
    let mut out = String::new();
    let mut gap = false;
    for c in text.chars() {
        if bc2_pyrt::py::py_space(c) {
            gap = true;
        } else {
            if gap && !out.is_empty() {
                out.push(' ');
            }
            gap = false;
            out.push(c);
        }
    }
    out
}

/// `restore_spawn(level, pre_level)` on the two levels' XML: the spawners of the Mark V the level before stage 5 had
/// and the level lacks, and the DbxEdit commands that put them back - the spawner (graft, with what it references
/// locally) and every field that named it (copyfield) - refused where such a field changed since the drop.
///
/// # Errors
/// A field changed since the spawn was dropped: not restored blindly.
pub fn restore_spawn(now: &str, pre: &str, pre_level: &str) -> R<(Vec<String>, Vec<String>)> {
    let vehicle = re(&format!(r#"(?i)ref="{}/"#, regex::escape(VEHICLE)));
    let spawns: Vec<String> = INST
        .captures_iter(pre)
        .filter(|c| c[3].ends_with("VehicleSpawnEntityData") && vehicle.is_match(&c[4]))
        .map(|c| c[2].to_owned())
        .collect();
    let mut now_bodies: HashMap<String, String> = HashMap::new();
    for c in INST.captures_iter(now) {
        now_bodies.insert(c[2].to_owned(), c[4].to_owned());
    }
    let mut cmds = Vec::new();
    for g in &spawns {
        if now.contains(&format!("guid=\"{g}\"")) {
            continue;
        }
        cmds.push(format!("graft={pre_level}|{g}"));
        let gl = g.to_lowercase();
        let item = re(&format!(r#"(?i)<item ref="[^"]*{}""#, regex::escape(&gl)));
        let any_ref = re(&format!(r#"(?i)ref="[^"]*{}""#, regex::escape(&gl)));
        for c in INST.captures_iter(pre) {
            let (owner, body) = (&c[2], &c[4]);
            if owner == g || !body.to_lowercase().contains(&gl) {
                continue;
            }
            let mut cur: HashMap<String, String> = HashMap::new();
            for (n, t) in children(now_bodies.get(owner).map_or("", String::as_str)) {
                cur.insert(n, t);
            }
            for (name, block) in children(body) {
                if !block.to_lowercase().contains(&gl) {
                    continue;
                }
                let kept: Vec<&str> = block.split('\n').filter(|l| !item.is_match(l)).collect();
                let dropped = any_ref.replace_all(&kept.join("\n"), "ref=\"null\"").into_owned();
                if norm(&dropped) != norm(cur.get(&name).map_or("", String::as_str)) {
                    return Err(OnsError::Exit(format!(
                        "{owner}.{name}: changed since the spawn was dropped - not restored blindly"
                    )));
                }
                cmds.push(format!("copyfield={pre_level}|{owner}|{name}|{owner}"));
            }
        }
    }
    Ok((spawns, cmds))
}

/// `install(write)` up to the ledger: the build, the level bundle unpacked from the source bundle with the boat's
/// partitions, resources and databases put in (ahead of the level in the manifest), its spawn restored, packed.
///
/// # Errors
/// A tool's failure, a refusal in words, a file that does not read.
#[allow(clippy::too_many_lines, reason = "console_markv.install, step for step")]
pub fn build(a: &Args) -> R<StageOut> {
    let p = Paths::new(a);
    let b = build_all(&p)?;
    let mut report = b.report;
    let src = join(&p.work, "pkg");
    fbrb(&["unpack", &a.src_bundle, &src])?;
    let man = join(&src, "fbrb.manifest");
    let lines = manifest_lines(&man)?;
    let existing: HashSet<String> = lines
        .iter()
        .map(|l| l.split('|').next().unwrap_or_default().to_lowercase())
        .collect();
    let mut changed: HashMap<String, String> = HashMap::new();
    let mut added: Vec<String> = Vec::new();
    let files = console_files(&p)?;
    let mut put = |rel: &str, data: &[u8], kind: &str| -> R<()> {
        write(&under(&src, rel), data)?;
        let line = format!("{rel}|{kind}|00010000|{0}|{0}", data.len());
        if existing.contains(&rel.to_lowercase()) {
            changed.insert(rel.to_lowercase(), line);
        } else if !added.contains(&line) {
            added.push(line);
        }
        Ok(())
    };
    for (q, path) in &b.parts {
        let line = &files
            .get(&format!("{q}.dbx"))
            .ok_or_else(|| OnsError::Data(format!("KeyError: '{q}.dbx'")))?
            .1;
        put(
            line.split('|').next().unwrap_or_default(),
            &read(path)?,
            "<non-resource>",
        )?;
    }
    for (n, (data, kind)) in &b.res.items {
        put(n, data, kind)?;
    }
    for (dx, data) in &b.dbs {
        put(
            &format!("Levels/{NAME}/Shaders/Dx{dx}_Single.res"),
            data,
            &format!("Dx{dx}ShaderDatabase"),
        )?;
    }
    let level = find(&src, &format!("Levels/{NAME}.dbx"))?;
    let pre_dir = join(&p.work, "pre");
    fbrb(&["unpack", &a.pre_bundle, &pre_dir])?;
    let pre_level = find(&pre_dir, &format!("Levels/{NAME}.dbx"))?;
    let pre = xml_of(&pre_level)?;
    let now = xml_of(&level)?;
    let (spawns, cmds) = restore_spawn(&now, &pre, &pre_level)?;
    if !cmds.is_empty() {
        let cmd = join(&p.work, "spawn.cmd");
        write_text_utf8(&cmd, &(cmds.join("\n") + "\n"))?;
        let new = format!("{level}.new");
        dbx(&["edit", &level, &new, &format!("@{cmd}")])?;
        std::fs::rename(&new, &level).map_err(|e| super::io_err(&level, &e))?;
    }
    let level_key = format!("levels/{LV}.dbx");
    let mut out_lines: Vec<String> = Vec::new();
    for l in &lines {
        let cols: Vec<&str> = l.split('|').collect();
        let key = cols.first().copied().unwrap_or_default().to_lowercase();
        let mut l = l.clone();
        if key == level_key {
            out_lines.extend(added.iter().cloned());
            let sz = size(&level)?.to_string();
            let mut c: Vec<&str> = cols.iter().take(3).copied().collect();
            c.extend(std::iter::repeat_n(sz.as_str(), cols.len().saturating_sub(3)));
            l = c.join("|");
        }
        out_lines.push(changed.get(&key).cloned().unwrap_or(l));
    }
    write_manifest(&man, &out_lines)?;
    drop_xml(&src)?;
    let o = bc2_tools::dbxedit::run(&["check", &level, "--bundle", &src].map(str::to_owned), None);
    let both = format!("{}{}", o.stdout, o.stderr);
    let last = bc2_pyrt::py::splitlines(bc2_pyrt::py::strip(&both))
        .last()
        .copied()
        .unwrap_or_default()
        .to_owned();
    report.push(format!(
        "spawn: {} spawner(s), {} DbxEdit commands",
        spawns.len(),
        cmds.len()
    ));
    report.push(format!("manifest: {} added, {} replaced", added.len(), changed.len()));
    report.push(format!("check: {last}"));
    let built = join(&p.work, "level-00.fbrb");
    pack(&src, &built)?;
    let mut out = StageOut::default();
    out.printed.push(format!("{LV}: Mark V"));
    out.printed.extend(report.into_iter().map(|r| format!("  {r}")));
    if a.write {
        out.writes.push((p.bundle, Source::File(built)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patterns_compile() {
        let _ = (&*REF, &*TEXNAME, &*LOD_NAME, &*SKIP_BUNDLE, &*INST);
        assert_eq!(materials().len(), 9);
    }

    #[test]
    fn children_as_the_python_reads_them() {
        let body = "\n\t\t<field name=\"A\">1</field>\n\t\t<array name=\"B\">\n\t\t\t<item ref=\"x\" />\n\t\t</array>\n\
                    \t\t<field ref=\"null\" name=\"C\" />\n\t";
        let c = children(body);
        assert_eq!(c.len(), 3);
        assert_eq!(c[0], ("A".into(), "\t\t<field name=\"A\">1</field>".into()));
        assert_eq!(c[1].0, "B");
        assert!(c[1].1.ends_with("\t\t</array>") && c[1].1.contains("<item ref=\"x\" />"));
        assert_eq!(c[2].0, "C");
    }

    #[test]
    fn texture_names_end_at_a_zero() {
        let block = b"\x00Textures/Vehicles/PBL/PBL_C\x00\x00\x00Objects/ab\x00Terrains/x/yz\x00";
        let got: Vec<Vec<u8>> = TEXNAME.captures_iter(block).map(|c| c[1].to_vec()).collect();
        assert_eq!(
            got,
            vec![b"Textures/Vehicles/PBL/PBL_C".to_vec(), b"Terrains/x/yz".to_vec()]
        );
    }

    #[test]
    fn console_layouts_find_the_record_after_its_guid() {
        let g = [7u8; 16];
        let mut b = b"\x00Objects/Vehicles/Sea/MarkV/MarkV_Mesh_lod0/Hull/Standard\x00".to_vec();
        b.extend_from_slice(&g);
        b.extend_from_slice(&[0; 8]);
        // the layout record: its GUID, 64 bytes of elements, the count (1), a zero, the rest
        b.extend_from_slice(&g);
        let mut body = vec![0u8; 0x49];
        body[64] = 1;
        b.extend_from_slice(&body);
        let got = console_layouts(&b);
        let key = "objects/vehicles/sea/markv/markv_mesh_lod0/hull/standard";
        assert_eq!(got.get(key).map(Vec::len), Some(0x49));
    }

    #[test]
    fn restore_spawn_grafts_and_copies_the_fields_that_named_it() {
        let g = "AAAAAAAA-0000-0000-0000-000000000001";
        let spawner = format!(
            "<instance guid=\"{g}\" type=\"Entity.VehicleSpawnEntityData\">\n\t\t<field ref=\"{VEHICLE}/\
             11111111-0000-0000-0000-000000000000\" name=\"Vehicle\" />\n</instance>"
        );
        let owner_pre = format!(
            "<instance guid=\"O\" type=\"Entity.Bp\">\n\t\t<array name=\"Objects\">\n\t\t\t<item ref=\"lv/{g}\" />\n\
             \t\t\t<item ref=\"lv/B\" />\n\t\t</array>\n</instance>"
        );
        let owner_now = "<instance guid=\"O\" type=\"Entity.Bp\">\n\t\t<array name=\"Objects\">\n\t\t\t<item ref=\"lv/B\" />\n\
             \t\t</array>\n</instance>";
        let pre = format!("{spawner}\n{owner_pre}");
        let (s, cmds) = restore_spawn(owner_now, &pre, "P").unwrap_or_default();
        assert_eq!(s, vec![g.to_owned()]);
        assert_eq!(cmds, vec![format!("graft=P|{g}"), "copyfield=P|O|Objects|O".to_owned()]);
        // the field changed since the drop: refused
        let changed = owner_now.replace("lv/B", "lv/C");
        assert!(matches!(restore_spawn(&changed, &pre, "P"), Err(OnsError::Exit(_))));
        // the spawner still there: nothing
        let (_, none) = restore_spawn(&pre, &pre, "P").unwrap_or_default();
        assert_eq!(none, Vec::<String>::new());
    }
}
