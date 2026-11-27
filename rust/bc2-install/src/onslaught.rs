//! The console Onslaught import (round OR5): the port of Project OPFOR's `src\tools\onslaught\console_onslaught.py`
//! `build` and `install`'s preparation (feature `onslaught`; `bc2_native.onslaught_build` / `onslaught_install_plan`).
//!
//! A console Onslaught level for the PC game, from the user's own Xbox 360 Update #3 package (never committed: the
//! level data comes from the user's files through OPFOR's import step) and the PC map of the same place:
//!
//! | part | what |
//! |---|---|
//! | world | the PC map's world objects (terrain, static / rigid / middle-growth / forest groups) grafted in place of the console ones |
//! | logic | the console level, its console-only references re-pointed to the PC map's (material grid, static cull trees, loading movie, weather) |
//! | shared | console shared data the level needs and PC lacks (mp2_common's game mode, AI soldiers and templates, teams), data files only; references resolving nowhere on PC become null |
//! | dropped | objects only console resources make (music cues, the position smoke, props without a PC version) |
//! | bundles | copies of the PC map's bundles without their deleted markers, its level files renamed to the Onslaught level, a Package overlay with the level and shared data; package.mft / bundleManifest blocks; the description in startup-00 with the Rush game mode |
//!
//! Everything is built under the work folder: [`build`] writes `<work>\out` (the level and its shared files),
//! `<work>\desc`, `level.cmd` and `report.json`; [`install_plan`] the bundles to install and returns the game copy's
//! writes in their order (the level's files first, the registration last: REG-NEW-4). Project OPFOR writes them into
//! the game copy in one journalled ledger commit (`console_common.install`, owner `onslaught:<level>`). The work
//! folder's files are the retired Python's byte for byte (rust/BENCHMARKS.md "OR5"); DbxEdit's and FbrbTool's commands
//! run in process ([`bc2_tools`]).

#![allow(
    clippy::doc_markdown,
    clippy::many_single_char_names,
    clippy::case_sensitive_file_extension_comparisons,
    clippy::missing_errors_doc,
    reason = "console_onslaught.py's names and words; every name compared is lower case first; each function's errors are OnsError's"
)]
#![allow(
    clippy::indexing_slicing,
    reason = "regex captures and slices bounded by the find before them, as the Python's"
)]

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;

use regex::Regex;

pub mod linux;
pub mod loadset;
pub mod mode;
pub mod port;

/// What went wrong: a tool's failure (its command line and output) or a file.
#[derive(Debug, thiserror::Error)]
pub enum OnsError {
    /// DbxEdit's or FbrbTool's command failed: its command line and output.
    #[error("failed: {0}")]
    Tool(String),
    /// A file could not be read or written.
    #[error("{0}")]
    Io(String),
    /// The data is not what the import reads.
    #[error("{0}")]
    Data(String),
    /// A refusal in plain words (the Python's `SystemExit(message)`).
    #[error("{0}")]
    Exit(String),
}

pub(crate) type R<T> = Result<T, OnsError>;

pub(crate) fn io_err(path: &str, e: &std::io::Error) -> OnsError {
    OnsError::Io(format!("{path}: {e}"))
}

/// The world object types swapped for the PC map's.
const WORLD: [&str; 5] = [
    "TerrainEntityData",
    "ForestEntityData",
    "MiddlegrowthModelGroupEntityData",
    "RigidModelGroupEntityData",
    "StaticModelGroupEntityData",
];

/// Console Onslaught soldier blueprints -> PC blueprints (console soldiers cannot load on PC: their input set-up is
/// the console's). As in Napisal's levels, the AI uses the campaign's RU soldiers and the players the MP US kits.
pub const SOLDIERS: [(&str, &str); 19] = [
    (
        "objects/soldiers/ons_ru_ai/ai_ru_assault",
        "objects/soldiers/sp_ru/sp_ru_assault",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_assault_an94scoped",
        "objects/soldiers/sp_ru/sp_ru_assault_an94scoped",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_assault_g3",
        "objects/soldiers/sp_ru/sp_ru_assault",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_assault_sniper",
        "objects/soldiers/sp_ru/sp_ru_sniper",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_sniper",
        "objects/soldiers/sp_ru/sp_ru_sniper",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_demo",
        "objects/soldiers/sp_ru/sp_ru_demo",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_rpg",
        "objects/soldiers/sp_ru/sp_ru_rpg",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_rpg_elite",
        "objects/soldiers/sp_ru/sp_ru_rpg_antivehicle",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_support",
        "objects/soldiers/sp_ru/sp_ru_support",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_support_mg3",
        "objects/soldiers/sp_ru/sp_ru_support_mg3",
    ),
    (
        "objects/soldiers/ons_ru_ai/ai_ru_vehiclemonkey",
        "objects/soldiers/sp_ru/sp_ru_vehiclemonkey",
    ),
    ("objects/soldiers/ons_us/ons_assault", "objects/soldiers/us/assault"),
    (
        "objects/soldiers/ons_us/ons_assault_upg",
        "objects/soldiers/us/upg_assault",
    ),
    (
        "objects/soldiers/ons_us/ons_demolition",
        "objects/soldiers/us/demolition",
    ),
    (
        "objects/soldiers/ons_us/ons_demolition_upg",
        "objects/soldiers/us/upg_demolition",
    ),
    ("objects/soldiers/ons_us/ons_recon", "objects/soldiers/us/recon"),
    ("objects/soldiers/ons_us/ons_recon_upg", "objects/soldiers/us/upg_recon"),
    ("objects/soldiers/ons_us/ons_support", "objects/soldiers/us/support"),
    (
        "objects/soldiers/ons_us/ons_support_upg",
        "objects/soldiers/us/upg_support",
    ),
];

/// PC campaign bundles searched for data the level needs but that does not load with it (PC versions are preferred
/// to console data; e.g. the campaign BMD3_Adv in sp_005).
const CAMPAIGN: [&str; 5] = ["sp_005", "sp_005_b", "sp_004_b", "sp_02", "sp_01"];

/// Console-only fields of classes the PC build has (removed; the PC build defaults them). The level-part set-up ones
/// (PartIndex ... BoundingBoxForwardXZ) make the server create no terrain entity and crash the AI path world set-up.
const CONSOLE_FIELDS: [&str; 6] = [
    "AutobalanceTeams",
    "LoseOnWholeTeamDead",
    "PartIndex",
    "CenterPos",
    "BoundingBoxExtentsXZ",
    "BoundingBoxForwardXZ",
];

/// A resource's manifest line for a file with no resource type.
const NONRES: &str = "<non-resource>|00010000";
/// The Rush (gold rush) game mode the level runs as: the PC game has no Onslaught mode.
const GOLDRUSH: &str = "gamemodes/goldrush/3eb0392d-dc32-4667-bd0e-9e6051144a1f";
/// The environment variable of an experiment: instances of these types (comma-separated) dropped.
pub const DROP_TYPES_VAR: &str = "OPFOR_ONS_DROP_TYPES";

fn re(p: &str) -> Regex {
    #[allow(
        clippy::expect_used,
        reason = "the patterns are constants, each compiled by the tests"
    )]
    Regex::new(p).expect("a constant pattern")
}

static REF: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| re(r#"ref="([A-Za-z0-9_/\-]+/[0-9A-Fa-f\-]{36})""#));
static INST: std::sync::LazyLock<Regex> =
    std::sync::LazyLock::new(|| re(r#"(?s)<instance guid="([^"]*)" type="([^"]*)">(.*?)</instance>"#));

// ------------------------------------------------------------------------------------------------ paths and files

/// `os.path.join` (ntpath) of two parts.
pub(crate) fn join(a: &str, b: &str) -> String {
    if a.is_empty() || a.ends_with(['\\', '/']) {
        format!("{a}{b}")
    } else {
        format!("{a}\\{b}")
    }
}

pub(crate) fn joins(a: &str, parts: &[&str]) -> String {
    parts.iter().fold(a.to_owned(), |acc, p| join(&acc, p))
}

/// A path without its last 4 characters (`f[:-4]`).
pub(crate) fn stem4(f: &str) -> &str {
    let n = f.chars().count();
    let cut = f.char_indices().nth(n.saturating_sub(4)).map_or(0, |(i, _)| i);
    &f[..cut]
}

pub(crate) fn exists(p: &str) -> bool {
    !p.is_empty() && Path::new(p).exists()
}

pub(crate) fn read(path: &str) -> R<Vec<u8>> {
    std::fs::read(path).map_err(|e| io_err(path, &e))
}

/// A text file as Python's text mode reads it with `errors='replace'`: UTF-8 (bad bytes as U+FFFD), universal newlines.
pub(crate) fn read_text(path: &str) -> R<String> {
    let b = read(path)?;
    Ok(String::from_utf8_lossy(&b).replace("\r\n", "\n").replace('\r', "\n"))
}

/// A file Python opened with the ANSI code page (the manifests: ASCII in practice), its lines (`splitlines()`).
pub(crate) fn read_lines(path: &str) -> R<Vec<String>> {
    let b = read(path)?;
    let t: String = b.iter().map(|&c| char::from(c)).collect();
    Ok(bc2_pyrt::py::splitlines(&t).into_iter().map(str::to_owned).collect())
}

pub(crate) fn write(path: &str, data: &[u8]) -> R<()> {
    if let Some(d) = Path::new(path).parent() {
        std::fs::create_dir_all(d).map_err(|e| io_err(&d.to_string_lossy(), &e))?;
    }
    bc2_gamefs::atomic::write(Path::new(path), &[data]).map_err(|e| OnsError::Io(format!("{path}: {e}")))
}

/// A text file written as Python's text mode writes it on Windows (`\n` as `\r\n`), characters as latin-1.
fn write_text(path: &str, text: &str) -> R<()> {
    let t = text.replace('\n', "\r\n");
    let b: Vec<u8> = t.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect();
    write(path, &b)
}

pub(crate) fn copy(src: &str, dst: &str) -> R<()> {
    write(dst, &read(src)?)
}

pub(crate) fn size(p: &str) -> R<u64> {
    std::fs::metadata(p).map(|m| m.len()).map_err(|e| io_err(p, &e))
}

pub(crate) fn rmtree(p: &str) -> R<()> {
    if Path::new(p).is_dir() {
        std::fs::remove_dir_all(p).map_err(|e| io_err(p, &e))?;
    }
    Ok(())
}

/// The files under `d` (recursively, in the folder listing's order) whose names end with `ext` (any case).
pub(crate) fn walk_files(d: &str, ext: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let p = join(d, &name);
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            walk_files(&p, ext, out);
        } else if name.to_lowercase().ends_with(ext) {
            out.push(p);
        }
    }
}

/// `p` relative to `base` (`p` lies under it).
pub(crate) fn rel(p: &str, base: &str) -> String {
    let b = base.trim_end_matches(['\\', '/']);
    p.get(b.len() + 1..).unwrap_or(p).to_owned()
}

pub(crate) fn slash(p: &str) -> String {
    p.replace('\\', "/")
}

// ------------------------------------------------------------------------------------------------ the tools

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

/// DbxEdit's command line in process; its stdout.
pub(crate) fn dbx(a: &[&str]) -> R<String> {
    let o = bc2_tools::dbxedit::run(&args(a), None);
    if o.code != 0 {
        return Err(OnsError::Tool(format!("{}\n{}{}", a.join(" "), o.stdout, o.stderr)));
    }
    Ok(o.stdout)
}

/// FbrbTool's command line in process; its stdout.
pub(crate) fn fbrb(a: &[&str]) -> R<String> {
    let o = bc2_tools::fbrbtool::run(&args(a), None);
    if o.code != 0 {
        return Err(OnsError::Tool(format!("{}\n{}{}", a.join(" "), o.stdout, o.stderr)));
    }
    Ok(o.stdout)
}

/// `cc.stamp_of(*inputs)`.
pub(crate) fn stamp_of(inputs: &[&str]) -> R<String> {
    let mut lines = Vec::new();
    for p in inputs {
        let name = bc2_pyrt::py::basename(p);
        let v = if Path::new(p).is_file() {
            bc2_gamefs::sha256::file(p).map_err(|e| io_err(p, &e))?
        } else {
            "missing".to_owned()
        };
        lines.push(format!("{name} {v}"));
    }
    Ok(lines.join("\n"))
}

pub(crate) fn stamp_path(out: &str) -> String {
    format!("{}.opfor-stamp", out.trim_end_matches(['\\', '/']))
}

/// `unpack(c, bundle, out)`: a cached unpack with its partitions' XML, made again when the bundle changed.
fn unpack(bundle: &str, out: &str) -> R<String> {
    let st = stamp_path(out);
    let stamp = stamp_of(&[bundle])?;
    if exists(out) && Path::new(&st).is_file() && std::fs::read_to_string(&st).ok().as_deref() == Some(stamp.as_str()) {
        return Ok(out.to_owned());
    }
    rmtree(out)?;
    fbrb(&["unpack", bundle, out])?;
    dbx(&["xml", out])?;
    write(&st, stamp.as_bytes())?;
    Ok(out.to_owned())
}

/// `index(*dirs)`: lower-case partition path -> .dbx file (the first directory wins).
fn index(dirs: &[String]) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for d in dirs {
        let mut files = Vec::new();
        walk_files(d, ".dbx", &mut files);
        for f in files {
            let k = slash(&rel(&f, d)).to_lowercase();
            out.entry(stem4(&k).to_owned()).or_insert(f);
        }
    }
    out
}

/// `refs_of(xml)`: the partitions an XML file's references name (lower case).
fn refs_of(xml: &str) -> R<HashSet<String>> {
    if !exists(xml) {
        return Ok(HashSet::new());
    }
    let t = read_text(xml)?;
    Ok(REF
        .captures_iter(&t)
        .map(|c| partition_of(&c[1]).to_lowercase())
        .collect())
}

/// `r.rsplit('/', 1)[0]`.
fn partition_of(r: &str) -> &str {
    r.rsplit_once('/').map_or(r, |(a, _)| a)
}

/// An instance: (GUID upper case, its type's last name, its body).
type Inst = (String, String, String);

/// `instances(xml)`: the instances and the text.
fn instances(xml: &str) -> R<(Vec<Inst>, String)> {
    let t = read_text(xml)?;
    let v = INST
        .captures_iter(&t)
        .map(|c| {
            let ty = c[2].rsplit('.').next().unwrap_or("").to_owned();
            (c[1].to_uppercase(), ty, c[3].to_owned())
        })
        .collect();
    Ok((v, t))
}

/// The files beside `f` (a .dbx) whose names start with its stem and end `.res` (`glob(f[:-4] + '*.res')`).
fn res_files(f: &str) -> Vec<String> {
    let stem = stem4(f);
    let (dir, base) = match stem.rfind(['\\', '/']) {
        Some(i) => (&stem[..i], stem[i + 1..].to_lowercase()),
        None => (".", stem.to_lowercase()),
    };
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let n = e.file_name().to_string_lossy().into_owned();
            let l = n.to_lowercase();
            if l.starts_with(&base) && l.ends_with(".res") && l.len() >= base.len() + 4 {
                out.push(join(dir, &n));
            }
        }
    }
    out
}

// ------------------------------------------------------------------------------------------------ the build

/// The import's names and folders (`console_onslaught.Ctx`).
#[derive(Debug, Clone)]
pub struct Ctx {
    /// The OPFOR folder (its `game\` is the PC game copy, read only).
    pub root: String,
    /// The user's Update #3 package.
    pub xbox: String,
    /// The Onslaught level, lower case (`ons_mp_008`).
    pub ons: String,
    /// The PC map, lower case (`mp_008`).
    pub pc: String,
    /// The work folder.
    pub work: String,
}

impl Ctx {
    /// The context of `console_onslaught.Ctx(root, xbox, ons, pc, work)`.
    #[must_use]
    pub fn new(root: &str, xbox: &str, ons: &str, pc: &str, work: &str) -> Self {
        Self {
            root: root.to_owned(),
            xbox: xbox.to_owned(),
            ons: ons.to_lowercase(),
            pc: pc.to_lowercase(),
            work: work.to_owned(),
        }
    }

    fn game(&self) -> String {
        join(&self.root, "game")
    }

    fn path(&self, parts: &[&str]) -> String {
        joins(&self.work, parts)
    }

    fn ons_up(&self) -> String {
        self.ons.to_uppercase()
    }

    fn pc_up(&self) -> String {
        self.pc.to_uppercase()
    }
}

/// `extract(c)`: the level's console files out of the package (`<work>\xbox\...`), each once.
fn extract(c: &Ctx) -> R<()> {
    let f = std::fs::File::open(&c.xbox).map_err(|e| io_err(&c.xbox, &e))?;
    let mut pkg = bc2_formats::stfs::Package::open(f).map_err(|e| OnsError::Data(format!("{}: {e}", c.xbox)))?;
    let want = [
        format!("levels/{}/", c.ons),
        "levels/mp2_common/level-00.fbrb".to_owned(),
        format!("ai/levels/{}/", c.ons),
    ];
    for e in pkg.entries.clone() {
        let path: String = e.path.iter().map(|&b| char::from(b)).collect();
        if e.dir || !want.iter().any(|w| path.to_lowercase().starts_with(w.as_str())) {
            continue;
        }
        let parts: Vec<&str> = std::iter::once("xbox").chain(path.split('/')).collect();
        let dst = c.path(&parts);
        if !exists(&dst) {
            let data = pkg
                .read(u64::from(e.start), u64::from(e.count), e.consecutive, u64::from(e.size))
                .map_err(|x| OnsError::Data(format!("{}: {path}: {x}", c.xbox)))?;
            write(&dst, &data)?;
        }
    }
    Ok(())
}

/// `pc_index(c)`: the partitions PC has when the level loads, and the unpacked PC folders.
fn pc_index(c: &Ctx) -> R<(HashSet<String>, Vec<String>)> {
    let g = c.game();
    let mut dirs = Vec::new();
    for (lvl, sub) in [
        (c.pc.as_str(), "level"),
        ("mp_common", "level"),
        ("sp_common", "level"),
        ("mp2_common", "level"),
        (c.pc.as_str(), "ingame"),
        (c.pc.as_str(), "loader"),
        ("mp_common", "ingame"),
    ] {
        for (base, kind) in [
            (joins(&g, &["Package", "levels", lvl]), "pkg"),
            (joins(&g, &["Dist", "win32", "levels", lvl]), "dist"),
        ] {
            let b = join(&base, &format!("{sub}-00.fbrb"));
            if exists(&b) {
                dirs.push(unpack(&b, &c.path(&["pc", &format!("{lvl}_{sub}_{kind}")]))?);
            }
        }
    }
    let mut names: HashSet<String> = index(&dirs).into_keys().collect();
    let mut bundles = Vec::new();
    for (d, pat) in [
        (joins(&g, &["Package", "async"]), None),
        (joins(&g, &["Package", "menu", "main"]), Some("mainmenu-00.fbrb")),
        (joins(&g, &["Dist", "win32"]), None),
    ] {
        if let Ok(rd) = std::fs::read_dir(&d) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                let hit = match pat {
                    Some(p) => n.eq_ignore_ascii_case(p),
                    None => n.to_lowercase().ends_with(".fbrb"),
                };
                if hit && e.file_type().is_ok_and(|t| t.is_file()) {
                    bundles.push(join(&d, &n));
                }
            }
        }
    }
    for b in bundles {
        for line in fbrb(&["list", &b])?.lines() {
            if let Some(last) = line.split_whitespace().last() {
                let l = last.to_lowercase();
                if l.ends_with(".dbx") {
                    names.insert(stem4(&l).to_owned());
                }
            }
        }
    }
    Ok((names, dirs))
}

/// `re.escape(s)` for a pattern.
fn esc(s: &str) -> String {
    regex::escape(s)
}

/// What `build` gives: the files added to the level's overlay (rel path -> a resource's manifest line, else size).
pub type Added = BTreeMap<String, Entry>;

/// One added file's manifest information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Entry {
    /// A resource's manifest line (from the campaign bundle's manifest).
    Line(String),
    /// The file's size (a non-resource line is made of it).
    Size(u64),
}

/// The level's own build: `build(c)` of `console_onslaught.py`.
#[expect(clippy::too_many_lines, reason = "console_onslaught.build, step by step")]
pub fn build(c: &Ctx) -> R<Added> {
    std::fs::create_dir_all(&c.work).map_err(|e| io_err(&c.work, &e))?;
    extract(c)?;
    let (ons, pc, ons_up, pc_up) = (c.ons.as_str(), c.pc.as_str(), c.ons_up(), c.pc_up());
    let con_level = unpack(
        &c.path(&["xbox", "levels", ons, "level-00.fbrb"]),
        &c.path(&["con_level"]),
    )?;
    let con_mp2 = unpack(
        &c.path(&["xbox", "levels", "mp2_common", "level-00.fbrb"]),
        &c.path(&["con_mp2"]),
    )?;
    let (pc_names, pc_dirs) = pc_index(c)?;
    let pc_level_dir = c.path(&["pc", &format!("{pc}_level_pkg")]);
    let mut pc_level = joins(&pc_level_dir, &["levels", &format!("{pc_up}.dbx")]);
    if !exists(&pc_level) {
        pc_level = joins(
            &c.path(&["pc", &format!("{pc}_level_dist")]),
            &["levels", &format!("{pc_up}.dbx")],
        );
    }
    let con_xml = joins(&con_level, &["levels", &format!("{ons_up}.xml")]);
    let (ci, ct) = instances(&con_xml)?;
    let (pi, pt) = instances(&format!("{}.xml", stem4(&pc_level)))?;
    let con_parts = index(&[con_level.clone(), con_mp2.clone()]);
    // campaign partitions: (unpacked folder, file); the first bundle wins
    let mut camp_parts: HashMap<String, (String, String)> = HashMap::new();
    let mut camp_roots = Vec::new();
    let g = c.game();
    for lvl in CAMPAIGN {
        for (kind, base) in [
            ("pkg", joins(&g, &["Package", "levels", lvl])),
            ("dist", joins(&g, &["Dist", "win32", "levels", lvl])),
        ] {
            let b = join(&base, "level-00.fbrb");
            if exists(&b) {
                let d = unpack(&b, &c.path(&["camp", &format!("{lvl}_{kind}")]))?;
                #[allow(
                    clippy::iter_over_hash_type,
                    reason = "one folder's keys are distinct: the order is not seen"
                )]
                for (k, v) in index(std::slice::from_ref(&d)) {
                    camp_parts.entry(k).or_insert((d.clone(), v));
                }
                camp_roots.push(d);
            }
        }
    }
    let pc_files = index(&pc_dirs);
    // console blueprints -> PC blueprints: each console instance a reference may name maps to the PC instance of the
    // same type in the PC file
    let mut remap: Vec<(String, String)> = Vec::new();
    let xml_of = |m: &HashMap<String, String>, k: &str| format!("{}.xml", stem4(m.get(k).map_or("", String::as_str)));
    for (cpart, ppart) in SOLDIERS {
        let (cx, px) = (xml_of(&con_parts, cpart), xml_of(&pc_files, ppart));
        if !(exists(&cx) && exists(&px)) {
            continue;
        }
        let mut ptypes: HashMap<String, String> = HashMap::new();
        for (gd, ty, _) in instances(&px)?.0 {
            ptypes.entry(ty).or_insert_with(|| gd.to_lowercase());
        }
        for (gd, ty, _) in instances(&cx)?.0 {
            if let Some(p) = ptypes.get(&ty) {
                let k = format!("{cpart}/{}", gd.to_lowercase());
                let v = format!("{ppart}/{p}");
                match remap.iter_mut().find(|e| e.0 == k) {
                    Some(e) => e.1 = v,
                    None => remap.push((k, v)),
                }
            }
        }
    }
    let remap_get = |k: &str| remap.iter().find(|e| e.0 == k).map(|e| e.1.clone());

    // --- the level: world swap, re-pointed level data, dropped console-resource objects
    let mut cmds: Vec<String> = Vec::new();
    let cguid: HashMap<&str, &str> = ci.iter().map(|(gd, ty, _)| (gd.as_str(), ty.as_str())).collect();
    let mut con_world: HashMap<&str, &str> = HashMap::new();
    for (gd, ty, _) in &ci {
        if WORLD.contains(&ty.as_str()) {
            con_world.insert(ty.as_str(), gd.as_str());
        }
    }
    for (gd, ty, _) in &pi {
        if WORLD.contains(&ty.as_str()) {
            let mut s = format!("graft={pc_level}|{gd}");
            if let Some(cg) = con_world.get(ty.as_str()) {
                let _ = write!(s, "|{cg}");
            }
            cmds.push(s);
        }
    }
    let bare = re(r#"ref="([0-9A-Fa-f-]{36})""#);
    for (gd, ty, b) in &ci {
        if WORLD.contains(&ty.as_str()) {
            let refs: Vec<String> = REF
                .captures_iter(b)
                .map(|x| x[1].to_owned())
                .chain(bare.captures_iter(b).map(|x| x[1].to_owned()))
                .collect();
            for r in refs {
                let r = r.to_uppercase();
                if cguid.get(r.as_str()).is_some_and(|t| t.ends_with("MemberData")) {
                    cmds.push(format!("drop={r}"));
                }
            }
            cmds.push(format!("drop={gd}"));
        }
    }
    for kind in ["staticculltree_client", "staticculltree_exclusive"] {
        let ma = re(&format!(r#"ref="(levels/{}/{kind}/[^"]*)""#, esc(ons)))
            .captures(&ct)
            .map(|m| m[1].to_owned());
        let mb = re(&format!(r#"ref="(levels/{}/{kind}/[^"]*)""#, esc(pc)))
            .captures(&pt)
            .map(|m| m[1].to_owned());
        if let (Some(a), Some(b)) = (ma, mb) {
            cmds.push(format!("replace={a}|{b}"));
        }
    }
    if let Some(m) = re(&format!(r#"ref="(levels/{}/materials_xenon/([^"]*))""#, esc(ons))).captures(&ct) {
        cmds.push(format!("replace={}|levels/{pc}/materials_win32/{}", &m[1], &m[2]));
    }
    // loading movie: the console Onslaught movie -> the PC map's Rush movie
    let movie = re(&format!(r#"ref="(levels/{}/ui/[a-z0-9_]*ons/[^"]*)""#, esc(pc)))
        .captures(&ct)
        .map(|m| m[1].to_owned());
    let mut pc_movie = None;
    let bink = re(r#"<instance guid="([^"]*)" type="Render.BinkMemoryAsset""#);
    for d in &pc_dirs {
        // glob(<d>\levels\<pc>\ui\*rush.xml) + glob(<d>\Levels\<PC>\UI\*RUSH.xml): the same files twice on Windows
        let ui = joins(d, &["levels", pc, "ui"]);
        let mut hits = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&ui) {
            for e in rd.flatten() {
                let n = e.file_name().to_string_lossy().into_owned();
                if n.to_lowercase().ends_with("rush.xml") {
                    hits.push(join(&ui, &n));
                }
            }
        }
        for f in hits.iter().chain(hits.iter()) {
            let t = std::fs::read(f).map_err(|e| io_err(f, &e))?;
            let t = String::from_utf8(t).map_err(|_| OnsError::Data(format!("{f}: not UTF-8")))?;
            if let Some(m) = bink.captures(&t) {
                let r = slash(&rel(f, d)).to_lowercase();
                pc_movie = Some(format!("{}/{}", stem4(&r), m[1].to_lowercase()));
            }
        }
    }
    if let (Some(m), Some(p)) = (&movie, &pc_movie) {
        cmds.push(format!("replace={m}|{p}"));
    }
    let console_only_res = |p: &str| con_parts.get(p).is_some_and(|f| !res_files(f).is_empty());
    // drop objects that need console resources or props PC does not have
    for (gd, ty, b) in &ci {
        if WORLD.contains(&ty.as_str()) {
            continue;
        }
        let parts: BTreeSet<String> = REF
            .captures_iter(b)
            .map(|x| partition_of(&x[1]).to_lowercase())
            .collect();
        for p in &parts {
            if pc_names.contains(p) {
                continue;
            }
            if matches!(
                ty.as_str(),
                "ReferenceObjectData" | "GameSoundEntityData" | "GameEffectEntityData"
            ) {
                let hit = console_only_res(p)
                    || match con_parts.get(p) {
                        Some(f) => refs_of(&format!("{}.xml", stem4(f)))?
                            .iter()
                            .any(|q| console_only_res(q) || (!pc_names.contains(q) && !con_parts.contains_key(q))),
                        None => false,
                    };
                if hit {
                    cmds.push(format!("drop={gd}"));
                    break;
                }
            }
        }
    }
    // weather: the PC map's own (the console level may use another mode's, whose states only the PC map's other
    // bundles have)
    let weather = re(r#"ref="(weather/[^"]*)""#);
    let cw: BTreeSet<String> = weather.captures_iter(&ct).map(|m| m[1].to_owned()).collect();
    let pw: BTreeSet<String> = weather.captures_iter(&pt).map(|m| m[1].to_owned()).collect();
    if cw.len() == 1 && pw.len() == 1 {
        let (a, b) = (cw.iter().next(), pw.iter().next());
        if let (Some(a), Some(b)) = (a, b)
            && a.to_lowercase() != b.to_lowercase()
        {
            cmds.push(format!("replace={a}|{b}"));
        }
    }
    for (a, b) in &remap {
        if let Some(m) = re(&format!("(?i){}", esc(a))).find(&ct) {
            cmds.push(format!("replace={}|{b}", m.as_str()));
        }
    }
    for f in CONSOLE_FIELDS {
        if ct.contains(&format!("name=\"{f}\"")) {
            cmds.push(format!("dropfield={f}"));
        }
    }
    // experiments: OPFOR_ONS_DROP_TYPES="TypeA,TypeB" drops every instance of those types
    let drop_types = std::env::var(DROP_TYPES_VAR).unwrap_or_default();
    for t in drop_types.split(',').filter(|t| !t.is_empty()) {
        for (gd, ty, _) in &ci {
            if ty == t {
                cmds.push(format!("drop={gd}"));
            }
        }
    }
    // as in Napisal's PC Onslaught levels: Rush, and no spawn point manager
    if let Some(m) = re(r#"ref="(gamemodes/onslaught/[^"]*)" name="GameMode""#).captures(&ct) {
        cmds.push(format!("replace={}|{GOLDRUSH}", &m[1]));
    }
    if let Some(m) = re(r#"ref="([0-9a-fA-F-]{36})" name="SpawnPointManager""#).captures(&ct) {
        cmds.push(format!("replace={}|null", &m[1]));
    }
    let mut seen = HashSet::new();
    cmds.retain(|x| seen.insert(x.clone()));
    write_text(&c.path(&["level.cmd"]), &(cmds.join("\n") + "\n"))?;
    let out_level = c.path(&["out", "Levels", &format!("{ons_up}.dbx")]);
    if let Some(d) = Path::new(&out_level).parent() {
        std::fs::create_dir_all(d).map_err(|e| io_err(&d.to_string_lossy(), &e))?;
    }
    let level_in = joins(&con_level, &["levels", &format!("{ons_up}.dbx")]);
    let at_cmd = format!("@{}", c.path(&["level.cmd"]));
    let prefix = format!("replaceprefix=levels/{pc}/|Levels/{ons_up}/|levels/{pc}/ui/|levels/{pc}/materials_");
    dbx(&["edit", &level_in, &out_level, &at_cmd, &prefix])?;
    dbx(&["xml", &out_level])?;
    let out_xml = format!("{}.xml", stem4(&out_level));

    // --- shared console data the level needs (its closure), data files only
    let mut need: HashMap<String, String> = HashMap::new();
    let mut todo: Vec<String> = refs_of(&out_xml)?.into_iter().collect();
    let mut seen: HashSet<String> = HashSet::new();
    let soldiers: HashSet<&str> = SOLDIERS.iter().map(|s| s.0).collect();
    while let Some(p) = todo.pop() {
        if !seen.insert(p.clone()) {
            continue;
        }
        if pc_names.contains(&p) || soldiers.contains(p.as_str()) {
            continue;
        }
        if let Some((_, f)) = camp_parts.get(&p) {
            need.insert(p.clone(), f.clone());
            todo.extend(
                refs_of(&format!("{}.xml", stem4(f)))?
                    .into_iter()
                    .filter(|q| !seen.contains(q)),
            );
            continue;
        }
        let Some(f) = con_parts.get(&p) else { continue };
        if console_only_res(&p) {
            continue;
        }
        need.insert(p.clone(), f.clone());
        todo.extend(
            refs_of(&format!("{}.xml", stem4(f)))?
                .into_iter()
                .filter(|q| !seen.contains(q)),
        );
    }
    // copied; references that resolve nowhere on PC become null
    let mut added: Added = BTreeMap::new();
    let roots: Vec<String> = [con_level.clone(), con_mp2.clone()]
        .into_iter()
        .chain(camp_roots.iter().cloned())
        .collect();
    let mut need_sorted: Vec<(&String, &String)> = need.iter().collect();
    need_sorted.sort();
    for (_p, f) in need_sorted {
        let root = roots
            .iter()
            .find(|r| f.starts_with(r.as_str()))
            .ok_or_else(|| OnsError::Data(format!("{f}: in none of the console level folders")))?;
        let relp = rel(f, root);
        // campaign PC files keep their .res resources
        if root != &con_level && root != &con_mp2 {
            let man = read_lines(&join(root, "fbrb.manifest"))?;
            let lines: HashMap<String, String> = man
                .iter()
                .skip(1)
                .map(|l| (l.split('|').next().unwrap_or("").to_lowercase(), l.clone()))
                .collect();
            for r in res_files(f) {
                let rr = slash(&rel(&r, root));
                let dst = c.path(&["out", &rr]);
                copy(&r, &dst)?;
                let line = match lines.get(&rr.to_lowercase()) {
                    Some(l) => l.clone(),
                    None => format!("{rr}|{NONRES}|{}", size(&r)?),
                };
                added.insert(rr, Entry::Line(line));
            }
        }
        let dst = c.path(&["out", &relp]);
        let fx = read_text(&format!("{}.xml", stem4(f)))?;
        let refs: BTreeSet<String> = REF.captures_iter(&fx).map(|x| x[1].to_owned()).collect();
        let mut edits: Vec<String> = refs
            .iter()
            .filter_map(|r| remap_get(&r.to_lowercase()).map(|v| format!("replace={r}|{v}")))
            .collect();
        let bad: BTreeSet<&String> = refs
            .iter()
            .filter(|r| {
                let p = partition_of(r).to_lowercase();
                !pc_names.contains(&p) && !need.contains_key(&p) && remap_get(&r.to_lowercase()).is_none()
            })
            .collect();
        edits.extend(bad.iter().map(|r| format!("replace={r}|null")));
        if edits.is_empty() {
            copy(f, &dst)?;
        } else {
            if let Some(d) = Path::new(&dst).parent() {
                std::fs::create_dir_all(d).map_err(|e| io_err(&d.to_string_lossy(), &e))?;
            }
            let mut a: Vec<&str> = vec!["edit", f, &dst];
            a.extend(edits.iter().map(String::as_str));
            dbx(&a)?;
        }
        added.insert(slash(&relp), Entry::Size(size(&dst)?));
    }
    // unresolved references left in the level itself become null too
    let ox = read_text(&out_xml)?;
    let keep_re = re(&format!(
        r"^levels/{}/(goldrush|gr_objects|sdm_logic|conquest)/",
        esc(pc)
    ));
    let lvl_bad: BTreeSet<String> = REF
        .captures_iter(&ox)
        .map(|x| x[1].to_owned())
        .filter(|r| {
            let p = partition_of(r).to_lowercase();
            !pc_names.contains(&p) && !need.contains_key(&p) && !keep_re.is_match(&r.to_lowercase())
        })
        .collect();
    if !lvl_bad.is_empty() {
        let edits: Vec<String> = lvl_bad.iter().map(|r| format!("replace={r}|null")).collect();
        let mut a: Vec<&str> = vec!["edit", &out_level, &out_level];
        a.extend(edits.iter().map(String::as_str));
        dbx(&a)?;
    }
    added.insert(format!("Levels/{ons_up}.dbx"), Entry::Size(size(&out_level)?));
    // the level's precomputed AI data (SectorOffsets of its path data; the PC map, without AI, has none)
    let mut pre = Vec::new();
    walk_files(&joins(&con_level, &["levels", ons, "Precomputed"]), ".dbx", &mut pre);
    for f in pre {
        let r = slash(&rel(&f, &con_level));
        let dst = c.path(&["out", &r]);
        copy(&f, &dst)?;
        added.insert(r, Entry::Size(size(&dst)?));
    }

    // --- description (startup bundle): Rush game mode, PC loading movie
    let desc_in = joins(&con_level, &["levels", &format!("{ons_up}_description.dbx")]);
    let desc_out = c.path(&["desc", "Levels", &format!("{ons_up}_description.dbx")]);
    if let Some(d) = Path::new(&desc_out).parent() {
        std::fs::create_dir_all(d).map_err(|e| io_err(&d.to_string_lossy(), &e))?;
    }
    let dx = read_text(&format!("{}.xml", stem4(&desc_in)))?;
    let mut dcmds = Vec::new();
    if let Some(m) = re(r#"ref="(gamemodes/onslaught/[^"]*)""#).captures(&dx) {
        dcmds.push(format!("replace={}|{GOLDRUSH}", &m[1]));
    }
    if let (Some(m), Some(p)) = (&movie, &pc_movie)
        && dx.contains(m.as_str())
    {
        dcmds.push(format!("replace={m}|{p}"));
    }
    let mut a: Vec<&str> = vec!["edit", &desc_in, &desc_out];
    a.extend(dcmds.iter().map(String::as_str));
    dbx(&a)?;
    write_text(
        &c.path(&["report.json"]),
        &report(cmds.len(), &added, &lvl_bad, movie.as_deref(), pc_movie.as_deref()),
    )?;
    Ok(added)
}

/// `json.dump({...}, indent=1)` of the build's report.
fn report(n: usize, added: &Added, lvl_bad: &BTreeSet<String>, movie: Option<&str>, pc_movie: Option<&str>) -> String {
    use bc2_pyrt::json::Value;
    let s = |x: &str| Value::Str(x.to_owned());
    let opt = |x: Option<&str>| x.map_or(Value::Null, s);
    let v = Value::Dict(vec![
        ("level_cmds".to_owned(), Value::Int(n as i128)),
        ("shared".to_owned(), Value::List(added.keys().map(|k| s(k)).collect())),
        (
            "nulled_in_level".to_owned(),
            Value::List(lvl_bad.iter().map(|k| s(k)).collect()),
        ),
        ("movie".to_owned(), Value::List(vec![opt(movie), opt(pc_movie)])),
    ]);
    bc2_pyrt::json::dumps(&v, Some(1))
}

// ------------------------------------------------------------------------------------------------ the install

/// `rename_level_files(c, root, lines)`: entries `Levels/<PC map>/...` (except UI and the material grid) get a copy
/// under `Levels/<ONS>/...` (the engine finds a level's own data under the loaded level's name); each copied
/// partition is its own (a new partition GUID from its path), its names re-pointed to the Onslaught level.
fn rename_level_files(c: &Ctx, root: &str, lines: &[String]) -> R<Vec<String>> {
    let mut out = lines.to_vec();
    let (pc, ons_up) = (c.pc.as_str(), c.ons_up());
    let pre = format!("levels/{pc}/");
    let ui = format!("levels/{pc}/ui/");
    let mat = format!("levels/{pc}/materials_");
    let have: HashSet<String> = lines
        .iter()
        .map(|l| l.split('|').next().unwrap_or("").to_lowercase())
        .collect();
    for l in lines {
        let name = l.split('|').next().unwrap_or("");
        let low = name.to_lowercase();
        if !low.starts_with(&pre) || low.starts_with(&ui) || low.starts_with(&mat) {
            continue;
        }
        let new = format!("Levels/{ons_up}/{}", &name[pre.len()..]);
        if have.contains(&new.to_lowercase()) {
            continue;
        }
        let src = joins(root, &name.split('/').collect::<Vec<_>>());
        let dst = joins(root, &new.split('/').collect::<Vec<_>>());
        if !exists(&src) {
            continue;
        }
        if let Some(d) = Path::new(&dst).parent() {
            std::fs::create_dir_all(d).map_err(|e| io_err(&d.to_string_lossy(), &e))?;
        }
        if low.ends_with(".dbx") && size(&src)? != 0 {
            let guid = bc2_formats::guid::new_guid("opfor", &[&new.to_lowercase()]);
            let prefix = format!("replaceprefix=levels/{pc}/|Levels/{ons_up}/|{ui}|{mat}");
            dbx(&["edit", &src, &dst, &prefix, &format!("partguid={guid}")])?;
        } else {
            copy(&src, &dst)?;
        }
        out.push(format!("{new}{}", &l[name.len()..]));
    }
    Ok(out)
}

/// FbrbTool's `pack` (`cc.pack`).
pub(crate) fn pack(folder: &str, out: &str) -> R<()> {
    fbrb(&["pack", folder, out]).map(|_| ())
}

/// `strip_deleted(c, src, dst)`: a copy of a bundle without its `*deleted*` markers (the PC map's bundles delete
/// shared data its own mode does not use; the Onslaught logic needs it), its level files renamed. The markers dropped.
fn strip_deleted(c: &Ctx, src: &str, dst: &str) -> R<i64> {
    let tmp = c.path(&["strip"]);
    rmtree(&tmp)?;
    fbrb(&["unpack", src, &tmp])?;
    let man = read_lines(&join(&tmp, "fbrb.manifest"))?;
    let first = man.first().cloned().unwrap_or_default();
    let kept: Vec<String> = man
        .iter()
        .skip(1)
        .filter(|l| !l.contains("|*deleted*|"))
        .cloned()
        .collect();
    let mut keep = vec![first];
    keep.extend(rename_level_files(c, &tmp, &kept)?);
    write_text(&join(&tmp, "fbrb.manifest"), &(keep.join("\n") + "\n"))?;
    if let Some(d) = Path::new(dst).parent() {
        std::fs::create_dir_all(d).map_err(|e| io_err(&d.to_string_lossy(), &e))?;
    }
    pack(&tmp, dst)?;
    // the markers dropped less the renamed copies added (negative when more were added), as the Python counts
    Ok(man.len() as i64 - keep.len() as i64)
}

/// One write into the game copy: the built file, or the bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A file built in the work folder.
    File(String),
    /// The bytes (the registration files).
    Bytes(Vec<u8>),
}

/// The game copy's writes in their order and the lines printed on the way.
pub struct Plan {
    /// (the game copy's path, what goes there), in the order the ledger writes them.
    pub writes: Vec<(String, Source)>,
    /// The lines the Python printed while building them.
    pub printed: Vec<String>,
}

/// `install(c, added)` up to the ledger commit: the bundles built, the writes in their order (the level's files first,
/// then startup-00, package.mft and bundleManifest: REG-NEW-4).
#[expect(clippy::too_many_lines, reason = "console_onslaught.install, step by step")]
pub fn install_plan(c: &Ctx, added: &Added) -> R<Plan> {
    let g = c.game();
    let (ons, pc, ons_up) = (c.ons.as_str(), c.pc.as_str(), c.ons_up());
    let mut writes: Vec<(String, Source)> = Vec::new();
    let mut printed = Vec::new();
    let set = |w: &mut Vec<(String, Source)>, k: String, v: Source| match w.iter_mut().find(|e| e.0 == k) {
        Some(e) => e.1 = v,
        None => w.push((k, v)),
    };
    // bundles: copies of the PC map's (without deleted markers), the Package level overlay + the Onslaught additions
    for sub in ["level", "ingame", "loader", "terrain"] {
        for base in [joins("Dist", &["win32", "levels"]), joins("Package", &["levels"])] {
            let src = joins(&g, &[&base, pc, &format!("{sub}-00.fbrb")]);
            let dst = joins(&g, &[&base, ons, &format!("{sub}-00.fbrb")]);
            if exists(&src) && !(base.starts_with("Package") && sub == "level") {
                let built = c.path(&["bundles", &base, &format!("{sub}-00.fbrb")]);
                let n = strip_deleted(c, &src, &built)?;
                set(&mut writes, dst, Source::File(built));
                if n != 0 {
                    printed.push(format!("  {base}/{ons}/{sub}: {n} deleted markers dropped"));
                }
            }
        }
    }
    let ov = c.path(&["overlay"]);
    rmtree(&ov)?;
    fbrb(&["unpack", &joins(&g, &["Package", "levels", pc, "level-00.fbrb"]), &ov])?;
    let man = read_lines(&join(&ov, "fbrb.manifest"))?;
    let dropped = man.iter().skip(1).filter(|l| l.contains("|*deleted*|")).count();
    let kept: Vec<String> = man
        .iter()
        .skip(1)
        .filter(|l| !l.contains("|*deleted*|"))
        .cloned()
        .collect();
    let mut man2 = vec![man.first().cloned().unwrap_or_default()];
    man2.extend(rename_level_files(c, &ov, &kept)?);
    printed.push(format!(
        "  Package/levels/{ons}/level: {dropped} deleted markers dropped"
    ));
    let names: HashSet<String> = man2
        .iter()
        .skip(1)
        .map(|l| l.split('|').next().unwrap_or("").to_lowercase())
        .collect();
    for (relp, sz) in added {
        let dst = joins(&ov, &[&relp.replace('/', "\\")]);
        copy(&c.path(&["out", &relp.replace('/', "\\")]), &dst)?;
        if !names.contains(&relp.to_lowercase()) {
            man2.push(match sz {
                Entry::Line(l) => l.clone(),
                Entry::Size(n) => format!("{relp}|{NONRES}|{n}"),
            });
        }
    }
    write_text(&join(&ov, "fbrb.manifest"), &(man2.join("\n") + "\n"))?;
    let level = joins(&g, &["Package", "levels", ons, "level-00.fbrb"]);
    let packed = c.path(&["level-00.packed.fbrb"]);
    pack(&ov, &packed)?;
    set(&mut writes, level, Source::File(packed));
    // path data
    for kind in ["infantry", "vehicle"] {
        let dst = joins(&g, &["Output", "win32", "ai", "levels", ons, kind, "pathdata.hpd"]);
        set(
            &mut writes,
            dst,
            Source::File(c.path(&["xbox", "ai", "levels", ons, kind, "pathdata.hpd"])),
        );
    }
    // registration: the PC map's package.mft blocks and bundleManifest lines under the new name
    let mft = joins(&g, &["Dist", "win32", "package.mft"]);
    let text: String = read(&mft)?.iter().map(|&b| char::from(b)).collect();
    if !text.contains(&format!("game:levels/{ons}/level-00.fbrb")) {
        let block_re = re(&format!(
            r"^BundleSource game:levels/{}/(level|ingame|loader|terrain)-00\.fbrb",
            esc(pc)
        ));
        let mut add = Vec::new();
        for b in text.replace("\r\n", "\n").split("\n\n") {
            let bs = b.trim_matches(|ch: char| bc2_pyrt::py::py_space(ch));
            if !block_re.is_match(bs) {
                continue;
            }
            let mut nb = bs.replace(&format!("levels/{pc}/"), &format!("levels/{ons}/"));
            let first = nb.lines().next().unwrap_or("").to_owned();
            if first.contains("level-00") || first.contains("ingame-00") || first.contains("loader-00") {
                let sub = first.rsplit('/').next().unwrap_or("").to_owned();
                let _ = write!(nb, "\nBundleTarget package:levels/mp2_common/{sub}");
                if sub == "level-00.fbrb" {
                    nb.push_str("\nBundleTarget game:levels/sp_common/level-00.fbrb");
                }
            }
            add.push(nb.replace('\n', "\r\n"));
        }
        let new = format!(
            "{}\r\n\r\n{}\r\n",
            text.trim_end_matches(['\r', '\n']),
            add.join("\r\n\r\n")
        );
        set(&mut writes, mft.clone(), Source::Bytes(latin1(&new)));
    }
    let bm = joins(&g, &["Dist", "win32", "bundleManifest"]);
    let bm_text: String = read(&bm)?.iter().map(|&b| char::from(b)).collect();
    let lines: Vec<&str> = bm_text.split('\n').collect();
    if !lines.iter().any(|l| l.starts_with(&format!("levels/{ons}/"))) {
        let line_re = re(&format!(r"^levels/{}/(level|ingame|loader|terrain)\t", esc(pc)));
        let new: Vec<String> = lines
            .iter()
            .filter(|l| line_re.is_match(l))
            .map(|l| l.replacen(&format!("levels/{pc}/"), &format!("levels/{ons}/"), 1))
            .collect();
        let mut all: Vec<String> = lines
            .iter()
            .filter(|l| !l.is_empty())
            .map(|l| (*l).to_owned())
            .collect();
        all.extend(new);
        set(&mut writes, bm.clone(), Source::Bytes(latin1(&(all.join("\n") + "\n"))));
    }
    // the description into the startup bundle
    let st = joins(&g, &["Package", "async", "startup-00.fbrb"]);
    let sd = c.path(&["startup"]);
    rmtree(&sd)?;
    fbrb(&["unpack", &st, &sd])?;
    let mut sman = read_lines(&join(&sd, "fbrb.manifest"))?;
    let relp = format!("Levels/{ons_up}_description.dbx");
    copy(
        &c.path(&["desc", "Levels", &format!("{ons_up}_description.dbx")]),
        &joins(&sd, &["Levels", &format!("{ons_up}_description.dbx")]),
    )?;
    if !sman
        .iter()
        .any(|l| l.to_lowercase().starts_with(&format!("{}|", relp.to_lowercase())))
    {
        let n = size(&join(&sd, &relp))?;
        sman.push(format!("{relp}|{NONRES}|{n}"));
    }
    write_text(&join(&sd, "fbrb.manifest"), &(sman.join("\n") + "\n"))?;
    let spacked = c.path(&["startup-00.packed.fbrb"]);
    pack(&sd, &spacked)?;
    set(&mut writes, st.clone(), Source::File(spacked));
    // the level's files first, the registration after them (REG-NEW-4)
    let reg = [st, mft, bm];
    let mut ordered: Vec<(String, Source)> = writes.iter().filter(|w| !reg.contains(&w.0)).cloned().collect();
    for r in &reg {
        if let Some(w) = writes.iter().find(|w| &w.0 == r) {
            ordered.push(w.clone());
        }
    }
    Ok(Plan {
        writes: ordered,
        printed,
    })
}

pub(crate) fn latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?')).collect()
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn patterns_and_paths() {
        let _ = (&*REF, &*INST);
        assert_eq!(stem4("a\\b.dbx"), "a\\b");
        assert_eq!(join("a\\", "b"), "a\\b");
        assert_eq!(join("a", "b"), "a\\b");
        assert_eq!(rel("w\\x\\levels\\a.dbx", "w\\x"), "levels\\a.dbx");
        assert_eq!(partition_of("objects/a/0123"), "objects/a");
        let r: Vec<String> = REF
            .captures_iter(r#"<x ref="objects/a/b/01234567-89ab-cdef-0123-456789abcdef" />"#)
            .map(|c| c[1].to_owned())
            .collect();
        assert_eq!(r, ["objects/a/b/01234567-89ab-cdef-0123-456789abcdef"]);
    }

    #[test]
    fn report_text() {
        let mut a = Added::new();
        a.insert("Levels/X.dbx".into(), Entry::Size(4));
        let t = report(3, &a, &BTreeSet::new(), Some("m"), None);
        assert_eq!(
            t,
            "{\n \"level_cmds\": 3,\n \"shared\": [\n  \"Levels/X.dbx\"\n ],\n \"nulled_in_level\": [],\n \"movie\": [\n  \"m\",\n  null\n ]\n}"
        );
    }
}
