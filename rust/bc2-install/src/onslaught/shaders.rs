//! Stage 7 of the console Onslaught port (round OR10): the port of Project OPFOR's `console_shaders.py`'s `plan` and
//! `apply` up to the ledger - the level's shader databases (`Levels/<LV>/Shaders/Dx9|Dx10|Dx11_Single.res`) merged
//! so they cover every mesh the level draws: its own database plus the entries of other PC levels ([`donors`], in
//! order: a name two donors have comes from the first) for the meshes it lacks.
//!
//! Why (Project OPFOR's DEVELOPMENT_NOTES.md section 4, "Nelson Bay"): a level's compiled shader databases hold
//! entries only for the meshes that level places, and a mesh with no entry draws invisibly. The needs are every mesh
//! the level's own partitions name (`shaderdb.py needs`), [`EXTRA`] (meshes reached through shared bundles: the flags,
//! the AI's shotgun, the Onslaught vehicles, the XM307, and [`PAIRS`]' whole entry names) and the meshes the level's
//! vehicle spawns draw ([`spawned_meshes`]: with the shader set each spawn draws with). The merge is the suite's
//! (`bc2_convert::shadercmd::cmd_merge`, `shaderdb.py merge`'s): it copies DICE's compiled programs from the donors
//! and translates nothing. Nothing is shipped: every database is read from the user's game copy at build time.
//!
//! The work folder's files are the Python's byte for byte (rust/BENCHMARKS.md "OR10").

use std::collections::{BTreeSet, HashMap};
use std::sync::LazyLock;

use bc2_convert::shadercmd;
use regex::Regex;

use super::port::{StageOut, manifest_lines, walk_all, write_manifest};
use super::{OnsError, R, Source, copy, fbrb, join, joins, pack, read, read_text, rmtree, size, stem4, write};

const H1: &str = "objects/buildings/house_01_10x8/house_01_10x8_mesh/objects/buildings/house_01_10x8/shaders/";
const BARRACK: &str = "objects/buildings/barrack/barrack_mesh/objects/buildings/barrack/shaders/";

/// Whole entry names (mesh/shader) a level draws that its base and donors lack although they have other entries of
/// the mesh: a need names a mesh, and one entry of it counts as covered. The console house across the creek at A
/// (`house_01_10x8`) draws with the mesh's own set; the donor that had the mesh placed it with the snow set, so the
/// house was see-through (play test 2026-10-29; `sp_02` has them). Atacama's barracks are placed with the mesh's own
/// set (the base has their green / yellow variants only). Not the 40 km/h sign: drawn with `mp_012cq`'s entries it
/// crashed the renderer at 0x98BD6E (automated load test 2026-10-30); the signs stay invisible.
pub const PAIRS: [(&str, &str, &[&str]); 2] = [
    (
        "house_01_10x8",
        H1,
        &[
            "facade",
            "facade_lod1",
            "facade_lod2",
            "roof",
            "roof_lod1",
            "roof_lod2",
            "glass",
            "metal",
            "metal_lod1",
        ],
    ),
    (
        "barrack",
        BARRACK,
        &[
            "metal_containerplates",
            "metal_containerplates_lod1",
            "metal_containerplates_lod2",
        ],
    ),
];

/// Meshes every console Onslaught level reaches through shared bundles (the flags, the AI's shotgun, the Onslaught
/// vehicles and the XM307), then [`PAIRS`]' entry names (needed wherever the mesh is placed; harmless elsewhere).
#[must_use]
pub fn extra() -> Vec<String> {
    let mut out: Vec<String> = [
        "objects/gameplay/flag_neutral",
        "objects/gameplay/flag_rus",
        "objects/gameplay/flag_us",
        "objects/objects/mp_flagpole_mobile_01",
        "objects/weapons/handheld/mec_shg_toz194",
        "objects/vehicles/land/bmd3",
        "objects/vehicles/land/m3a3",
        "objects/vehicles/land/cav",
        "objects/weapons/stationary/us_gmg_xm307",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for (_, prefix, shaders) in PAIRS {
        out.extend(shaders.iter().map(|s| format!("{prefix}{s}")));
    }
    out
}

/// Donors after the PC map's own other modes (same climate, same props first): Nelson Bay's set (snow), which also
/// covers the shared Onslaught objects, then campaign levels of the other climates; last the AA-BMD3 (ONS_MP_005's,
/// and every BMD-3's AA turret): only `mp_012gr` (its snow set) and `sp_005` (its own set) compile it, last so no
/// earlier donor's choice changes.
pub const GENERAL_DONORS: [&str; 7] = [
    "level:mp_012cq",
    "level:mp_001",
    "level:sp_005_b",
    "level:sp_04",
    "level:sp_02",
    "level:mp_012gr",
    "level:sp_005",
];

/// The PC map's own other modes, per level.
pub const OWN_MODES: [(&str, &[&str]); 4] = [
    ("ons_mp_002", &["level:mp_002sr"]),
    ("ons_mp_004", &["level:mp_004sdm"]),
    ("ons_mp_005", &["level:mp_005gr", "level:mp_005sr"]),
    ("ons_mp_008", &["level:mp_008cq"]),
];

/// Console Onslaught level -> the PC map it is built on (`console_common.PC_MAP`).
pub const PC_MAP: [(&str, &str); 4] = [
    ("ons_mp_002", "mp_002"),
    ("ons_mp_004", "mp_004"),
    ("ons_mp_005", "mp_005"),
    ("ons_mp_008", "mp_008"),
];

/// The shared bundles the shader audit unpacks into `work\console_shaders\shared\<key>` (`console_shader_audit.SHARED`):
/// (key, level, bundle).
pub const SHARED: [(&str, &str, &str); 3] = [
    ("mp_common_level", "mp_common", "level"),
    ("mp_common_ingame", "mp_common", "ingame"),
    ("sp_common_level", "sp_common", "level"),
];

/// A level's donors (`LEVELS[lv]['donors']`): its own modes, the general ones it does not have, and `mp_008cq` last
/// except on Nelson Bay itself.
///
/// # Errors
/// A level this stage does not know (`KeyError`).
pub fn donors(lv: &str) -> R<Vec<String>> {
    let own = OWN_MODES
        .iter()
        .find(|(l, _)| *l == lv)
        .map(|(_, d)| *d)
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{lv}'")))?;
    let mut out: Vec<String> = own.iter().map(|s| (*s).to_owned()).collect();
    out.extend(
        GENERAL_DONORS
            .iter()
            .filter(|d| !own.contains(d))
            .map(|s| (*s).to_owned()),
    );
    if lv != "ons_mp_008" {
        out.push("level:mp_008cq".into());
    }
    Ok(out)
}

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

static SPAWN: LazyLock<Regex> = LazyLock::new(|| re(r#"(?s)type="[^"]*VehicleSpawnEntityData"[^>]*>(.*?)</instance>"#));
static VEHICLE: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f-]{36}" name="Vehicle""#));
static SETNO: LazyLock<Regex> = LazyLock::new(|| re(r#"name="MeshShaderSetNumber">(\d+)<"#));
static MESH: LazyLock<Regex> =
    LazyLock::new(|| re(r#"(?s)type="Render\.(\w*)MeshAsset"[^>]*>.*?<field name="Name">([^<]*)</field>"#));
/// `console_shader_audit.REF`.
static REF: LazyLock<Regex> = LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+)/[0-9A-Fa-f\-]{36}""#));

/// What stage 7 is given.
#[derive(Debug, Clone)]
pub struct Args {
    /// The OPFOR folder: its `work\console_shaders\` is written (the shared unpacks made when missing), the PC map's
    /// unpacks under `work\onslaught\build_<lv>\pc` read (DbxEdit's XML written into a folder that has none), the
    /// mesh index `work\tools_out\meshview_index.bin` read (or made); its game copy gives the donors.
    pub root: String,
    /// The game copy: read only.
    pub game: String,
    /// The Onslaught level (lower case).
    pub lv: String,
    /// `--apply` (the merged databases put in, the bundle packed) rather than `--plan`.
    pub write: bool,
}

/// `console_shader_audit.Index(roots)`: the partitions of unpacked bundles (the first folder wins), their XML beside
/// them (a folder with none gets DbxEdit's for all of its partitions; a partition without XML reads as missing).
struct AuditIndex {
    files: HashMap<String, String>,
    cache: std::cell::RefCell<HashMap<String, Option<String>>>,
}

impl AuditIndex {
    fn new(roots: &[String]) -> Self {
        fn walk(d: &str, rel: &str, dbx: &mut Vec<(String, String)>, xml: &mut bool) {
            let Ok(rd) = std::fs::read_dir(d) else { return };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue; // glob's hidden names
                }
                let p = join(d, &name);
                let r = if rel.is_empty() {
                    name.clone()
                } else {
                    format!("{rel}/{name}")
                };
                if std::fs::metadata(&p).is_ok_and(|m| m.is_dir()) {
                    walk(&p, &r, dbx, xml);
                } else {
                    let low = name.to_lowercase();
                    if low.ends_with(".dbx") {
                        dbx.push((r, p));
                    } else if low.ends_with(".xml") {
                        *xml = true;
                    }
                }
            }
        }
        let mut files = HashMap::new();
        for r in roots {
            let (mut dbx, mut xml) = (Vec::new(), false);
            walk(r, "", &mut dbx, &mut xml);
            if !dbx.is_empty() && !xml {
                // once per folder (files it cannot read stay without XML: skipped); its failure is not the stage's
                let _ = bc2_tools::dbxedit::run(&["xml".to_owned(), r.clone()], None);
            }
            for (rel, p) in dbx {
                files.entry(stem4(&rel).to_lowercase()).or_insert(p);
            }
        }
        Self {
            files,
            cache: std::cell::RefCell::new(HashMap::new()),
        }
    }

    /// `xml(part)`: the partition's XML (read with `errors='replace'`), or None.
    fn xml(&self, part: &str) -> Option<String> {
        let f = self.files.get(part)?;
        if let Some(x) = self.cache.borrow().get(f) {
            return x.clone();
        }
        let x = format!("{}.xml", stem4(f));
        let got = if std::path::Path::new(&x).is_file() {
            read_text(&x).ok()
        } else {
            None
        };
        self.cache.borrow_mut().insert(f.clone(), got.clone());
        got
    }
}

/// `spawned_meshes(lv, src)`: the mesh names every vehicle the level spawns draws (`VehicleSpawnEntityData` -> the
/// vehicle -> every mesh partition its references reach: turrets, parts, cockpits), with the exact entries of the
/// shader set each spawn draws with (`MeshShaderSetNumber`; a mesh with fewer sets draws its first), read from the
/// unpacked level, the PC map's unpacks and the shared bundles, then the game copy's index. `EXTRA` named vehicles by
/// hand and missed the ones added since (the Vodnik, the quad bike, the AA-BMD3 and the KORD were invisible on their
/// Onslaught maps, 2026-11-13).
#[expect(clippy::too_many_lines, reason = "console_shaders.spawned_meshes, step by step")]
fn spawned_meshes(a: &Args, src: &str) -> R<Vec<String>> {
    let lv = a.lv.as_str();
    let pc = PC_MAP
        .iter()
        .find(|(l, _)| *l == lv)
        .map(|(_, p)| *p)
        .ok_or_else(|| OnsError::Data(format!("KeyError: '{lv}'")))?;
    let pcdir = joins(&a.root, &["work", "onslaught", &format!("build_{lv}"), "pc"]);
    let mut roots: Vec<String> = vec![src.to_owned()];
    for k in ["level", "ingame"] {
        for w in ["pkg", "dist"] {
            let d = join(&pcdir, &format!("{pc}_{k}_{w}"));
            if std::path::Path::new(&d).is_dir() {
                roots.push(d);
            }
        }
    }
    let shared = joins(&a.root, &["work", "console_shaders", "shared"]);
    for (key, level, sub) in SHARED {
        let d = join(&shared, key);
        if !std::path::Path::new(&d).is_dir() {
            let from = joins(&a.game, &["Dist", "win32", "levels", level, &format!("{sub}-00.fbrb")]);
            fbrb(&["unpack", &from, &d])?;
        }
        roots.push(d);
    }
    let ix = AuditIndex::new(&roots);
    // partitions those folders lack (a vehicle another bundle holds: the Vodnik, the Kornet): the game copy's index
    let game = bc2_convert::meshview::Index::load(&a.root, &mut |_| {})
        .map_err(|e| OnsError::Io(format!("the game copy's mesh index: {e}")))?;
    let xml = |part: &str| -> R<Option<String>> {
        if let Some(x) = ix.xml(part) {
            return Ok(Some(x));
        }
        match game.read(&format!("{part}.dbx")).filter(|d| !d.is_empty()) {
            Some(d) => bc2_formats::dbx::to_xml(&d)
                .map(Some)
                .map_err(|e| OnsError::Data(format!("{part}.dbx: {e}"))),
            None => Ok(None),
        }
    };
    let level = xml(&format!("levels/{lv}"))?
        .filter(|x| !x.is_empty())
        .ok_or_else(|| OnsError::Exit(format!("{lv}: no level partition to find its vehicles in")))?;
    // each spawn's vehicle and the shader set it draws with (MeshShaderSetNumber: Atacama's desert M1A2 is set 1)
    let mut spawns: BTreeSet<(String, u64)> = BTreeSet::new();
    for b in SPAWN.captures_iter(&level) {
        if let Some(v) = VEHICLE.captures(&b[1]) {
            let n = SETNO
                .captures(&b[1])
                .map_or(0, |n| n[1].parse::<u64>().unwrap_or(u64::MAX));
            spawns.insert((v[1].to_lowercase(), n));
        }
    }
    let mut missing: BTreeSet<&str> = BTreeSet::new();
    for (v, _) in &spawns {
        if xml(v)?.is_none() {
            missing.insert(v);
        }
    }
    if !missing.is_empty() {
        let shown: Vec<String> = missing.iter().map(|m| bc2_pyrt::fmt::str_repr(m)).collect();
        return Err(OnsError::Exit(format!(
            "{lv}: spawned vehicles found in no bundle: [{}]",
            shown.join(", ")
        )));
    }
    let mut files = Vec::new();
    walk_all(src, &mut files);
    let unpacked: HashMap<String, String> = files
        .into_iter()
        .map(|f| (super::rel(&f, src).replace('\\', "/").to_lowercase(), f))
        .collect();
    let res = |name: &str| -> R<Option<Vec<u8>>> {
        match unpacked.get(&name.to_lowercase()) {
            Some(p) => read(p).map(Some),
            None => Ok(game.read(&name.to_lowercase())),
        }
    };
    let mut out: BTreeSet<String> = BTreeSet::new();
    for (vehicle, setno) in &spawns {
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut stack: Vec<String> = vec![vehicle.clone()];
        while let Some(part) = stack.pop() {
            if !seen.insert(part.clone()) {
                continue;
            }
            let Some(x) = xml(&part)?.filter(|x| !x.is_empty()) else {
                continue;
            };
            if let Some(m) = MESH.captures(&x) {
                let name = m[2].to_owned();
                out.insert(name.clone());
                // the exact entries of the set the spawn draws (a mesh with one entry already counts as covered by
                // the merge: Laguna Presa's desert M3A3 and quad bike stayed invisible, the base having their first
                // set's)
                if let Some(data) = res(&format!("{name}.res"))?.filter(|d| !d.is_empty()) {
                    let kind = format!("{}MeshSet", &m[1]);
                    let k = bc2_formats::meshdata::Kind::of(&kind)
                        .ok_or_else(|| OnsError::Data(format!("{kind}: a mesh set the reader does not read")))?;
                    let sets = bc2_formats::meshdata::parse_set(&data, k, false, None).map_err(|_| {
                        OnsError::Data(format!(
                            "{kind}: a mesh set the reader does not read (cut short or damaged)"
                        ))
                    })?;
                    for l in &sets.lods {
                        if !l.sets.is_empty() {
                            let i = usize::try_from(*setno).ok().filter(|i| *i < l.sets.len()).unwrap_or(0);
                            for s in &l.sets[i].1 {
                                let shader: String = s.iter().map(|&c| char::from(c)).collect();
                                out.insert(format!("{name}/{shader}").to_lowercase());
                            }
                        }
                    }
                }
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
    }
    Ok(out.into_iter().collect())
}

/// A `shaderdb.py` command's failure as the stage's refusal (the Python's `run` raised `failed: <command line>`).
fn tool(cmd: &str, e: &shadercmd::CmdError) -> OnsError {
    OnsError::Tool(format!("shaderdb.py {cmd}\n{e}"))
}

/// The level's bundle, its unpacked folder, (each base database, its merged file).
type Merged = (String, String, Vec<(String, String)>);

/// `console_shaders.build(lv)`: the level's bundle unpacked, its three databases merged; the merged files
/// (base -> merged).
fn merge_all(a: &Args, out: &mut StageOut) -> R<Merged> {
    let lv = a.lv.as_str();
    let name = lv.to_uppercase();
    let donors = donors(lv)?;
    let work = joins(&a.root, &["work", "console_shaders"]);
    let bundle = joins(&a.game, &["Package", "levels", lv, "level-00.fbrb"]);
    let src = joins(&work, &[lv, "pkg"]);
    let merged_dir = joins(&work, &[lv, "merged"]);
    rmtree(&src)?;
    rmtree(&merged_dir)?;
    std::fs::create_dir_all(&merged_dir).map_err(|e| super::io_err(&merged_dir, &e))?;
    fbrb(&["unpack", &bundle, &src])?;
    let needs = joins(&work, &[lv, "needs.txt"]);
    shadercmd::write_needs(&src, &needs).map_err(|e| tool(&format!("needs {src} -o {needs}"), &e))?;
    let vehicle_meshes = spawned_meshes(a, &src)?;
    out.printed.push(format!(
        "  {} meshes the level's vehicles draw: {}",
        vehicle_meshes.len(),
        vehicle_meshes
            .iter()
            .map(|m| m.rsplit('/').next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join(", ")
    ));
    // open(needs, 'a'): '\n' + the names + '\n', in text mode
    let mut need_text = read(&needs)?;
    let mut all = extra();
    all.extend(vehicle_meshes);
    need_text.extend(format!("\n{}\n", all.join("\n")).replace('\n', "\r\n").as_bytes());
    write(&needs, &need_text)?;
    let root = a.root.clone();
    let opfor = move || Ok(root.clone());
    let env = shadercmd::Env {
        opfor: &opfor,
        on_bundle_read: &|| {},
    };
    let mut merged = Vec::new();
    for dx in ["Dx9", "Dx10", "Dx11"] {
        let base = joins(&src, &["Levels", &name, "Shaders", &format!("{dx}_Single.res")]);
        let dst = join(&merged_dir, &format!("{dx}_Single.res"));
        let args = shadercmd::MergeArgs {
            base: &base,
            donors: &donors,
            need: &needs,
            out: &dst,
            all_names: false,
            dx: None,
        };
        let p = shadercmd::cmd_merge(&args, &env).map_err(|e| {
            tool(
                &format!("merge {base} {} --need {needs} -o {dst}", donors.join(" ")),
                &e,
            )
        })?;
        out.printed.push(format!("  {dx}: {}", p.summary));
        merged.push((base, dst));
    }
    Ok((bundle, src, merged))
}

/// `console_shaders.plan(lv)` / `apply(lv)` up to the ledger.
pub fn build(a: &Args) -> R<StageOut> {
    let lv = a.lv.as_str();
    let donors = donors(lv)?;
    let mut out = StageOut::default();
    out.printed.push(format!(
        "{lv}: merging shader databases (donors: {})",
        donors.join(", ")
    ));
    // the lines printed before a refusal stay printed (the Python printed as it went)
    let (bundle, src, merged) = match merge_all(a, &mut out) {
        Ok(v) => v,
        Err(e) => {
            out.fail = Some(e.to_string());
            return Ok(out);
        }
    };
    if !a.write {
        return Ok(out);
    }
    if let Err(e) = install_merged(a, &bundle, &src, &merged, &mut out) {
        out.fail = Some(e.to_string());
    }
    Ok(out)
}

/// `apply`'s rest: the merged databases put in the unpacked bundle (unless they are what it has), its manifest's
/// sizes, the bundle packed.
fn install_merged(a: &Args, bundle: &str, src: &str, merged: &[(String, String)], out: &mut StageOut) -> R<()> {
    let lv = a.lv.as_str();
    // from the installed bundle: adds only what is still missing
    let mut same = true;
    for (b, d) in merged {
        if read(b)? != read(d)? {
            same = false;
            break;
        }
    }
    if same {
        out.printed.push(format!("{lv}: already applied"));
        return Ok(());
    }
    let man = join(src, "fbrb.manifest");
    let mut lines = manifest_lines(&man)?;
    for (base, dst) in merged {
        copy(dst, base)?;
        let rel = super::rel(base, src).replace('\\', "/").to_lowercase();
        let sz = size(base)?;
        for l in &mut lines {
            let parts: Vec<&str> = l.split('|').collect();
            if parts.len() == 5 && parts[0].to_lowercase() == rel {
                *l = format!("{}|{}|{}|{sz}|{sz}", parts[0], parts[1], parts[2]);
            }
        }
    }
    write_manifest(&man, &lines)?;
    let built = joins(&a.root, &["work", "console_shaders", lv, "level-00.fbrb"]);
    pack(src, &built)?;
    out.writes.push((bundle.to_owned(), Source::File(built)));
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn patterns_and_donors() {
        let _ = (&*SPAWN, &*VEHICLE, &*SETNO, &*MESH, &*REF);
        assert_eq!(
            donors("ons_mp_005").unwrap(),
            [
                "level:mp_005gr",
                "level:mp_005sr",
                "level:mp_012cq",
                "level:mp_001",
                "level:sp_005_b",
                "level:sp_04",
                "level:sp_02",
                "level:mp_012gr",
                "level:sp_005",
                "level:mp_008cq"
            ]
        );
        assert_eq!(donors("ons_mp_008").unwrap().first().unwrap(), "level:mp_008cq");
        assert!(!donors("ons_mp_008").unwrap()[1..].iter().any(|d| d == "level:mp_008cq"));
        assert!(donors("mp_001").is_err());
        assert_eq!(extra().len(), 9 + 9 + 3);
        let x =
            r#"<instance guid="A" type="Render.RigidMeshAsset"><field name="Name">objects/v/x_mesh</field></instance>"#;
        let m = MESH.captures(x).unwrap();
        assert_eq!((&m[1], &m[2]), ("Rigid", "objects/v/x_mesh"));
    }
}
