//! Export: a map project built into a NEW multiplayer level, staged (`EDITOR_ARCHITECTURE.md` 3.12, round E3).
//!
//! [`build`] reads the game copy (never writes it): the base level's partition with the project's operations
//! ([`crate::apply`]), made a multiplayer level ([`crate::mpconvert`]), renamed, its moved objects given new cull
//! tree leaves ([`crate::culltree`]), and the files the engine finds by a level's name copied under the new name. It
//! writes the new level's bundles into a stage folder with `stage.json` (what is in them, their sha1, the
//! registration an install adds) - the base's bundles are read by the new level at run time, not copied
//! (`package.mft` lists them among its targets, as DICE's mode levels list their base level's, `MP_005GR` -> `MP_005`).
//!
//! [`check`] proves a stage offline against the game's rules (every reference resolves in the bundles the level
//! loads, every bundle and partition parses, the registration names every bundle once, the cull tree is one tree with
//! one object per slot). [`patch_startup`] builds the one shared bundle an install changes besides the text files
//! (`startup-00`: the descriptions and the menus' level list). Installing (copying the stage into the game copy,
//! the backups, the registration last, the revert) is Project OPFOR's (`src\tools\mapedit\mapedit_install.py`).

#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "entry names are compared lower case (DICE's names mix cases)"
)]

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bc2_formats::dbx::edit::EditDbx;
use bc2_formats::fbrb::{self, Entry, Layout, PackItem};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mpconvert::{self, Converted, DescriptionKeep, Ground, LayerSpec};
use crate::project::{MapProject, sha1_hex};
use crate::schema::Schema;
use crate::write::LevelWriter;

/// The export file's `format`.
pub const FORMAT: &str = "bfbc2-mapexport";
/// The export file's `version`.
pub const VERSION: u32 = 1;
/// The bundles of a new level, as `package.mft` and `bundleManifest` name them (`wake_install.KINDS` less `vo/en`:
/// the stock levels' voice-over bundles are registered but absent from the game copy, and a new level registered
/// with a bundle it does not have crashes the start-up checksum - so it ships and registers none; its announcer's
/// lines are the common bundles').
pub const KINDS: [&str; 4] = ["level", "terrain", "ingame", "loader"];
/// An entry's flags in every level bundle of the game (`0x10000`).
pub const FLAGS: u32 = 0x10000;
/// The `.dbx` entries' type.
pub const NONRES: &str = "<non-resource>";
/// The level list the menus read (`startup-00`; `wake_install.LEVEL_LIST`).
pub const LEVEL_LIST: &str = "LevelListQuery.dbx";

/// Why an export failed (in words for the Problems panel).
#[derive(Debug, Error)]
#[error("{0}")]
pub struct ExportError(pub String);

/// The result type of this module.
pub type Result<T> = std::result::Result<T, ExportError>;

fn err(why: impl Into<String>) -> ExportError {
    ExportError(why.into())
}

impl From<crate::write::LevelWriteError> for ExportError {
    fn from(e: crate::write::LevelWriteError) -> Self {
        Self(e.to_string())
    }
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self(e.to_string())
    }
}

impl From<fbrb::FbrbError> for ExportError {
    fn from(e: fbrb::FbrbError) -> Self {
        Self(e.to_string())
    }
}

impl From<fbrb::PackError> for ExportError {
    fn from(e: fbrb::PackError) -> Self {
        Self(e.to_string())
    }
}

/// What the Export form says: the new level's name and its game mode's layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExportSpec {
    /// [`FORMAT`].
    pub format: String,
    /// [`VERSION`].
    pub version: u32,
    /// The new level's id (`Levels/<level>`; letters, digits and `_`; never a stock level's).
    pub level: String,
    /// The name the launcher and the server browser show (OPFOR's `maps.ini`).
    pub title: String,
    /// The game mode (`conquest`).
    pub mode: String,
    /// The mode's layer.
    pub layer: LayerSpec,
}

impl ExportSpec {
    /// The level id checked: letters, digits and `_`, 3 to 40 of them, upper case.
    ///
    /// # Errors
    /// Another character, or a length outside that.
    pub fn level_id(&self) -> Result<String> {
        let id = self.level.trim().to_ascii_uppercase();
        if !(3..=40).contains(&id.len()) || !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
            return Err(err(format!(
                "the level name {:?} must be 3 to 40 letters, digits or _",
                self.level
            )));
        }
        Ok(id)
    }

    /// The spec from its file's text.
    ///
    /// # Errors
    /// Not JSON, another format, or a newer version.
    pub fn from_json(text: &str) -> Result<Self> {
        let s: Self = serde_json::from_str(text).map_err(|e| err(format!("not an export file: {e}")))?;
        if s.format != FORMAT {
            return Err(err(format!("not an export file (format {:?})", s.format)));
        }
        if s.version > VERSION {
            return Err(err(format!("made by a newer editor (version {})", s.version)));
        }
        Ok(s)
    }
}

// ------------------------------------------------------------------------------------------------ the game copy

/// The game copy, read only: its `Package` and `Dist\win32` bundles and `package.mft`.
#[derive(Debug, Clone)]
pub struct Game {
    /// The game folder (OPFOR's `game\`).
    pub root: PathBuf,
    mft: Vec<(String, Vec<String>)>,
}

/// A live entry of a set of bundles: (bundle, entry).
pub type Found = (PathBuf, Entry);

impl Game {
    /// The game folder; its `package.mft` read.
    ///
    /// # Errors
    /// No `Dist\win32\package.mft`.
    pub fn open(root: &Path) -> Result<Self> {
        let p = root.join("Dist").join("win32").join("package.mft");
        let text = std::fs::read(&p).map_err(|e| err(format!("{}: {e}", p.display())))?;
        Ok(Self {
            root: root.to_path_buf(),
            mft: parse_mft(&String::from_utf8_lossy(&text)),
        })
    }

    /// A bundle named as `package.mft` names it (`game:levels/x/level-00.fbrb` is `Dist\win32\...`, `package:...`
    /// is `Package\...`).
    #[must_use]
    pub fn bundle(&self, name: &str) -> PathBuf {
        let (base, rel) = match name.split_once(':') {
            Some(("package", r)) => (self.root.join("Package"), r),
            Some((_, r)) => (self.root.join("Dist").join("win32"), r),
            None => (self.root.join("Dist").join("win32"), name),
        };
        rel.split('/').fold(base, |p, s| p.join(s))
    }

    /// The targets of a `BundleSource game:<source>` block (in order), empty when there is none.
    #[must_use]
    pub fn targets(&self, source: &str) -> Vec<String> {
        let want = format!("game:{source}").to_ascii_lowercase();
        self.mft
            .iter()
            .find(|(s, _)| s.to_ascii_lowercase() == want)
            .map_or_default(|(_, t)| t.clone())
    }

    /// The live entries of bundles in priority order (the first bundle's entry of a name wins; a deleted marker
    /// hides the name in the bundles after it), by lower-case name.
    ///
    /// # Errors
    /// A bundle that does not read.
    pub fn live(&self, bundles: &[PathBuf]) -> Result<HashMap<String, Found>> {
        let mut out: HashMap<String, Found> = HashMap::new();
        let mut hidden = HashSet::new();
        for b in bundles.iter().filter(|b| b.is_file()) {
            let h = fbrb::entries_path(b).map_err(|e| err(format!("{}: {e}", b.display())))?;
            for e in h.entries {
                let low = e.name.to_ascii_lowercase();
                if out.contains_key(&low) || hidden.contains(&low) {
                    continue;
                }
                if e.is_deleted() {
                    hidden.insert(low);
                } else {
                    out.insert(low, (b.clone(), e));
                }
            }
        }
        Ok(out)
    }

    /// A level's own bundles of a kind: `Package\levels\<lv>\<kind>-00.fbrb` then `Dist\win32\levels\...` (those
    /// that exist).
    #[must_use]
    pub fn level_bundles(&self, lv: &str, kind: &str) -> Vec<PathBuf> {
        let rel = format!("levels/{}/{kind}-00.fbrb", lv.to_ascii_lowercase());
        [format!("package:{rel}"), format!("game:{rel}")]
            .iter()
            .map(|n| self.bundle(n))
            .filter(|p| p.is_file())
            .collect()
    }

    /// Every bundle a level's bundle of a kind loads: its own, then its block's targets (deduplicated).
    #[must_use]
    pub fn loaded(&self, lv: &str, kind: &str) -> Vec<PathBuf> {
        let mut out = self.level_bundles(lv, kind);
        for t in self.targets(&format!("levels/{}/{kind}-00.fbrb", lv.to_ascii_lowercase())) {
            let p = self.bundle(&t);
            if !out.contains(&p) && p.is_file() {
                out.push(p);
            }
        }
        out
    }

    /// The startup bundles (`Package\async`, then `Dist\win32\async`): descriptions, game modes, the level list.
    #[must_use]
    pub fn startup(&self) -> Vec<PathBuf> {
        ["package:async/startup-00.fbrb", "game:async/startup-00.fbrb"]
            .iter()
            .map(|n| self.bundle(n))
            .filter(|p| p.is_file())
            .collect()
    }
}

/// `package.mft`'s blocks: (`BundleSource` value, its `BundleTarget` values).
#[must_use]
pub fn parse_mft(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out: Vec<(String, Vec<String>)> = Vec::new();
    for line in text.lines() {
        let s = line.trim();
        if let Some(src) = s.strip_prefix("BundleSource ") {
            out.push((src.trim().to_owned(), Vec::new()));
        } else if let Some(t) = s.strip_prefix("BundleTarget ")
            && let Some(last) = out.last_mut()
        {
            last.1.push(t.trim().to_owned());
        }
    }
    out
}

/// Reads entries (by their [`Found`]) grouped by bundle, uncached (an export reads each bundle once).
///
/// # Errors
/// A bundle that does not read.
pub fn read_found(found: &[&Found]) -> Result<Vec<Vec<u8>>> {
    let mut by_bundle: BTreeMap<&PathBuf, Vec<(usize, &str)>> = BTreeMap::new();
    for (i, (b, e)) in found.iter().enumerate() {
        by_bundle.entry(b).or_default().push((i, e.name.as_str()));
    }
    let mut out = vec![Vec::new(); found.len()];
    for (b, items) in by_bundle {
        let names: Vec<&str> = items.iter().map(|x| x.1).collect();
        let data = fbrb::read_path_uncached(b, &names).map_err(|e| err(format!("{}: {e}", b.display())))?;
        for ((i, _), d) in items.into_iter().zip(data) {
            if let Some(slot) = out.get_mut(i) {
                *slot = d;
            }
        }
    }
    Ok(out)
}

fn read_one(game: &Game, bundles: &[PathBuf], name: &str) -> Result<Vec<u8>> {
    let live = game.live(bundles)?;
    let f = live
        .get(&name.to_ascii_lowercase())
        .ok_or_else(|| err(format!("{name} is in none of {}", show(bundles))))?;
    read_found(&[f])?.pop().ok_or_else(|| err("nothing read"))
}

fn show(bundles: &[PathBuf]) -> String {
    bundles
        .iter()
        .map(|b| b.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

// ------------------------------------------------------------------------------------------------ building

/// One staged bundle's entry.
#[derive(Debug, Clone)]
pub struct StagedEntry {
    /// The archive name.
    pub name: String,
    /// The resource type.
    pub type_name: String,
    /// The data.
    pub data: Vec<u8>,
}

/// A staged file in `stage.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StagedFile {
    /// Path relative to the stage folder (`level-00.fbrb`, `vo/en-00.fbrb`).
    pub path: String,
    /// Where an install puts it, relative to the game folder (`Package/levels/<lv>/level-00.fbrb`).
    pub install: String,
    /// Its sha1.
    pub sha1: String,
    /// Its size.
    pub size: u64,
    /// Its entries.
    pub entries: usize,
}

/// `stage.json`: what [`build`] staged and what an install adds.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StageRecord {
    /// [`FORMAT`] + `-stage`.
    pub format: String,
    /// The new level (`MAPEDIT_AURORA_CQ`).
    pub level: String,
    /// Its title (maps.ini).
    pub title: String,
    /// Its game mode (maps.ini's key: `Conquest`).
    pub mode: String,
    /// The base level (`sp_01`).
    pub base: String,
    /// The mode layer's donor level (`mp_009cq`).
    #[serde(default)]
    pub donor: String,
    /// The base partition's sha1 in the game copy.
    pub base_sha1: String,
    /// The project's id and file.
    pub project: String,
    /// The staged bundles.
    pub files: Vec<StagedFile>,
    /// The description partition (for the startup patch): its file in the stage and its entry name.
    pub description: (String, String),
    /// `package.mft` blocks an install adds (each a block's lines).
    pub mft_blocks: Vec<Vec<String>>,
    /// `bundleManifest` lines an install adds.
    pub bundle_manifest: Vec<String>,
    /// The menus' level list items an install adds (`LevelListQuery`).
    pub level_list: Vec<String>,
    /// The report: what was done, in order.
    pub report: Vec<String>,
    /// The conversion's counts.
    pub converted: Converted,
}

/// Root files the engine finds by a level's name (`console_onslaught.rename_level_files`, `wake_install.ROOT_FILES`):
/// copied under the new name, their names re-pointed. The rest of `Levels/<base>/...` (the material grid, the
/// minimap, emitters and effects, waypoint trees, the campaign's subtitles and facial animation) is read where it is.
#[must_use]
pub fn is_root_file(rest: &str) -> bool {
    let r = rest.to_ascii_lowercase();
    if r.starts_with("shaders/") && r.ends_with(".res") {
        return true;
    }
    if r.contains('/') {
        return false;
    }
    matches!(
        r.as_str(),
        "shaders.dbx"
            | "emittersystem.dbx"
            | "emittertexturealpha.dbx"
            | "emittertexturealpha.res"
            | "emittertexturenormals.dbx"
            | "emittertexturenormals.res"
            | "animationsoundmap.dbx"
            | "staticculltree_client.dbx"
            | "staticculltree_exclusive.dbx"
    ) || ((r.contains("modelgroupentity") || r.contains("forestentity")) && r.contains("_havokasset."))
}

fn renamed_entry(name: &str, base: &str, new: &str) -> String {
    let n = base.len();
    let tail = name.get(7 + n..).unwrap_or_default();
    format!(
        "{}{}{tail}",
        name.get(..7).unwrap_or("Levels/"),
        new.to_ascii_uppercase()
    )
}

fn stem(name: &str) -> String {
    let low = name.to_ascii_lowercase();
    match low.rfind('.') {
        Some(i) if !low.get(i..).unwrap_or_default().contains('/') => low.get(..i).unwrap_or_default().to_owned(),
        _ => low,
    }
}

/// Gives every instance of a partition a new guid (UUID v5 over `seed` and the old guid) and re-points its local
/// references; the partition's own guid too.
fn new_guids(d: &mut EditDbx, seed: &str) -> crate::write::Result<()> {
    let mut map = HashMap::new();
    for e in d.instances() {
        let g = String::from_utf8_lossy(d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase();
        let n = bc2_stockmp::guid::new_guid(crate::NAMESPACE, &[seed, &g]);
        d.set_attr(e, b"guid", n.as_bytes())?;
        map.insert(g, n);
    }
    let roots = d.roots().to_vec();
    for r in roots {
        for e in d.walk(r) {
            for k in [&b"ref"[..], b"primaryInstance"] {
                let Some(v) = d.attr(e, k).map(<[u8]>::to_vec) else {
                    continue;
                };
                if v.len() == 36
                    && !v.contains(&b'/')
                    && let Some(n) = map.get(&String::from_utf8_lossy(&v).to_ascii_uppercase())
                {
                    let n = if k == b"ref" { n.to_ascii_lowercase() } else { n.clone() };
                    d.set_attr(e, k, n.as_bytes())?;
                }
            }
        }
    }
    mpconvert::set_partition_guid(d, &bc2_stockmp::guid::new_guid(crate::NAMESPACE, &[seed, "partition"]))
}

/// The main height field of a level (its `TerrainEntityData`'s terrain), from the bundles its terrain and level
/// load; `None` when there is none.
fn heightfield(game: &Game, lv: &str, partition: &LevelWriter) -> Result<Option<bc2_formats::terrain::Heightfield>> {
    let terrain = partition
        .d
        .of_type(b".TerrainEntityData")
        .first()
        .and_then(|&e| {
            partition.d.walk(e).into_iter().find_map(|x| {
                partition
                    .d
                    .attr(x, b"ref")
                    .filter(|r| r.to_ascii_lowercase().starts_with(b"terrains/"))
            })
        })
        .map(|r| String::from_utf8_lossy(r).into_owned());
    let Some(r) = terrain else { return Ok(None) };
    // terrains/sp_01/sp_01/<guid> -> Terrains/SP_01/SP_01.Heightfield-000.res
    let parts: Vec<&str> = r.split('/').collect();
    let (Some(a), Some(b)) = (parts.get(1), parts.get(2)) else {
        return Ok(None);
    };
    let name = format!("terrains/{a}/{b}.heightfield-000.res");
    let mut bundles = game.loaded(lv, "terrain");
    bundles.extend(game.loaded(lv, "level"));
    let live = game.live(&bundles)?;
    let Some(f) = live.get(&name) else { return Ok(None) };
    let data = read_found(&[f])?.pop().unwrap_or_default();
    Ok(bc2_formats::terrain::parse(&data).ok())
}

/// The new level's `package.mft` blocks: its own bundle first, then (level, terrain) the base level's bundles and
/// the base's own targets with `mp_common` before them; (ingame, loader) `mp_common`'s; (vo/en) `mp1_common`'s, as
/// the stock multiplayer levels and the WW2 levels have.
#[must_use]
pub fn mft_blocks(game: &Game, lv: &str, base: &str) -> Vec<Vec<String>> {
    let lv = lv.to_ascii_lowercase();
    let base = base.to_ascii_lowercase();
    let mut out = Vec::new();
    for kind in KINDS {
        let own = format!("levels/{lv}/{kind}-00.fbrb");
        let mut lines = vec![format!("BundleSource game:{own}")];
        let mut targets: Vec<String> = Vec::new();
        match kind {
            "level" | "terrain" => {
                targets.push(format!("package:{own}"));
                let base_targets = game.targets(&format!("levels/{base}/{kind}-00.fbrb"));
                let (mine, common): (Vec<String>, Vec<String>) = base_targets
                    .into_iter()
                    .partition(|t| t.to_ascii_lowercase().contains(&format!("levels/{base}/")));
                targets.extend(mine.into_iter().filter(|t| game.bundle(t).is_file()));
                if kind == "level" {
                    targets.push("game:levels/mp_common/level-00.fbrb".into());
                    targets.extend(
                        common
                            .into_iter()
                            .filter(|t| !t.to_ascii_lowercase().contains("mp_common")),
                    );
                }
            }
            _ => {
                targets.push(format!("package:{own}"));
                targets.push(format!("game:levels/mp_common/{kind}-00.fbrb"));
            }
        }
        lines.extend(targets.into_iter().map(|t| format!("BundleTarget {t}")));
        out.push(lines);
    }
    out
}

/// The new level's `bundleManifest` lines (`wake_install.bm_with_level`).
#[must_use]
pub fn bundle_manifest_lines(lv: &str) -> Vec<String> {
    KINDS
        .iter()
        .map(|k| {
            format!(
                "levels/{}/{k}\t00000000-0000-0000-0000-000000000000\t1",
                lv.to_ascii_lowercase()
            )
        })
        .collect()
}

fn pack_entries(entries: &[StagedEntry]) -> Result<Vec<u8>> {
    let items: Vec<PackItem<'_>> = entries
        .iter()
        .map(|e| PackItem {
            name: &e.name,
            type_name: &e.type_name,
            flags: FLAGS,
            size2: e.data.len() as u32,
            data: &e.data,
        })
        .collect();
    Ok(fbrb::pack(&items, &Layout::default())?)
}

/// Builds the project into the new level's bundles in `out` (a stage folder: emptied of a former stage's files)
/// and writes `stage.json`, `report.txt` and `registration.txt` beside them.
///
/// # Errors
/// The base, the donor or a file the level needs is missing, the project does not apply, or a write fails.
#[allow(
    clippy::too_many_lines,
    reason = "the export's steps in order, each named in the report"
)]
pub fn build(
    game: &Game,
    project: &MapProject,
    project_file: &str,
    spec: &ExportSpec,
    out: &Path,
) -> Result<StageRecord> {
    let lv = spec.level_id()?;
    let low = lv.to_ascii_lowercase();
    let base = project.base.level.to_ascii_lowercase();
    let base_up = base.to_ascii_uppercase();
    if game.bundle(&format!("game:levels/{low}/level-00.fbrb")).exists()
        || game.bundle(&format!("package:levels/{low}/level-00.fbrb")).exists() && !is_custom(game, &low)
    {
        return Err(err(format!(
            "{lv} is a level of the game copy: an export never replaces one; choose another name"
        )));
    }
    if !game.targets(&format!("levels/{low}/level-00.fbrb")).is_empty() && !is_custom(game, &low) {
        return Err(err(format!(
            "{lv} is registered in package.mft by another tool: choose another name"
        )));
    }
    let mut report = Vec::new();
    // 1. the base partition with the project's operations
    let base_bundles = game.loaded(&base, "level");
    let base_bytes = read_one(game, &base_bundles, &format!("Levels/{base_up}.dbx"))?;
    let base_sha1 = sha1_hex(&base_bytes);
    if base_sha1 == project.base.sha1 {
        report.push(format!(
            "base: Levels/{base_up} as the project was made on (sha1 {base_sha1})"
        ));
    } else {
        let r = project.base_report(&base_bytes);
        if !r.missing.is_empty() {
            return Err(err(format!(
                "the base level changed since the project was made and {} operations name objects it lacks (first: {:?})",
                r.missing.len(),
                r.missing.first()
            )));
        }
        report.push(format!(
            "base: Levels/{base_up} differs from the project's (sha1 {base_sha1}, project {}); every operation's \
             objects are there",
            project.base.sha1
        ));
    }
    let base_w = LevelWriter::parse(&base_bytes)?;
    let schema = Schema::scan([&base_w.d]);
    let applied = crate::write::apply(&base_bytes, &project.operations, &schema).map_err(|e| err(e.to_string()))?;
    report.push(format!(
        "project: {} operations applied ({} transforms, {} deleted, {} duplicated, {} fields)",
        applied.operations, applied.transforms, applied.deleted, applied.duplicated, applied.fields
    ));
    let mut w = LevelWriter::parse(&applied.bytes)?;
    // 2. multiplayer: the campaign out, the donor's layer in, placed, the level's fields
    let donor_lv = spec.layer.donor.to_ascii_lowercase();
    let donor_bytes = read_one(
        game,
        &game.loaded(&donor_lv, "level"),
        &format!("Levels/{}.dbx", donor_lv.to_ascii_uppercase()),
    )?;
    let donor = LevelWriter::parse(&donor_bytes)?;
    let keep = DescriptionKeep::of(&w).ok_or_else(|| err("the base level has no LevelDescription"))?;
    let mut conv = Converted::default();
    let keep_area: HashSet<String> = [spec.layer.combat_area.to_ascii_uppercase()].into();
    if w.find(&spec.layer.combat_area).is_none() {
        return Err(err(format!(
            "the combat area {} is not in the base level",
            spec.layer.combat_area
        )));
    }
    mpconvert::strip(&mut w, &keep_area, &mut conv)?;
    let seed = format!("{}:{low}", project.id);
    let map = mpconvert::graft(&mut w, &donor, &spec.layer, &seed, &mut conv)?;
    let hb = heightfield(game, &base, &base_w)?;
    let hd = heightfield(game, &donor_lv, &donor)?;
    let ground = match (&hd, &hb) {
        (Some(d), Some(b)) => Some(Ground { donor: d, base: b }),
        _ => None,
    };
    report.push(format!(
        "ground: {}",
        if ground.is_some() {
            "the donor's and the base's height fields"
        } else {
            "not found: heights moved as given"
        }
    ));
    mpconvert::place(&mut w, &donor, &spec.layer, &map, ground.as_ref(), &mut conv)?;
    mpconvert::enable(&mut w, &spec.layer.combat_area)?;
    mpconvert::level_fields(&mut w, &donor, &keep, &map, &mut conv)?;
    report.push(format!(
        "multiplayer: {} campaign instances removed ({} event connections, {} references cut); {} instances and {} \
         event connections of {}'s {} layer grafted; {} references of the layer not followed",
        conv.removed.iter().map(|x| x.1).sum::<usize>(),
        conv.connections_removed,
        conv.references_cut,
        conv.grafted.iter().map(|x| x.1).sum::<usize>(),
        conv.connections_grafted,
        donor_lv,
        spec.mode,
        conv.not_followed.len()
    ));
    for (n, k, m) in &conv.placed {
        report.push(format!("  placed {n:?}: {k} instances moved {m:.0} m"));
    }
    // 3. the root files under the new name
    let live = game.live(&base_bundles)?;
    let prefix = format!("levels/{base}/");
    let mut roots: Vec<(&String, &Found)> = live
        .iter()
        .filter(|(n, _)| n.strip_prefix(&prefix).is_some_and(is_root_file))
        .collect();
    roots.sort_by(|a, b| a.0.cmp(b.0));
    let renamed: HashSet<String> = roots.iter().map(|(n, _)| stem(n)).collect();
    let found: Vec<&Found> = roots.iter().map(|x| x.1).collect();
    let datas = read_found(&found)?;
    let mut level_entries: Vec<StagedEntry> = Vec::new();
    let mut tree_slots = None;
    let mut exclusive: Option<(String, EditDbx)> = None;
    for ((_, (_, e)), data) in roots.iter().zip(datas) {
        let new_name = renamed_entry(&e.name, &base, &low);
        if e.name.to_ascii_lowercase().ends_with(".dbx") {
            let mut d = EditDbx::parse(&data).map_err(|x| err(format!("{}: {x}", e.name)))?;
            mpconvert::rename_strings(&mut d, &base, &low, &renamed)?;
            mpconvert::set_partition_guid(&mut d, &mpconvert::partition_guid_for(&new_name))?;
            if e.name.to_ascii_lowercase().ends_with("staticculltree_exclusive.dbx") {
                exclusive = Some((new_name, d));
                continue;
            }
            level_entries.push(StagedEntry {
                name: new_name,
                type_name: e.type_name.clone(),
                data: d.write().map_err(|x| err(x.0))?,
            });
        } else {
            level_entries.push(StagedEntry {
                name: new_name,
                type_name: e.type_name.clone(),
                data,
            });
        }
    }
    report.push(format!(
        "root files: {} copied under Levels/{lv}/ (shader databases, emitter system and textures, model groups, cull \
         trees, animation sound map)",
        roots.len()
    ));
    // 4. the cull tree: moved and copied objects get new leaves
    if let Some((name, mut d)) = exclusive {
        let r = crate::culltree::reslot(&base_w, &mut w, &mut d)?;
        report.push(format!(
            "cull tree: {} objects given new leaves (leaves {} -> {})",
            r.objects.len(),
            r.leaves.0,
            r.leaves.1
        ));
        for (g, a, b, n) in &r.objects {
            report.push(format!("  {g}: slot {a} -> {b} ({n} leaves)"));
        }
        tree_slots = Some(r);
        level_entries.push(StagedEntry {
            name,
            type_name: NONRES.into(),
            data: d.write().map_err(|x| err(x.0))?,
        });
    }
    let _ = tree_slots;
    // 5. the level partition renamed
    mpconvert::rename_strings(&mut w.d, &base, &low, &renamed)?;
    mpconvert::set_partition_guid(&mut w.d, &mpconvert::partition_guid_for(&format!("Levels/{lv}")))?;
    mpconvert::set_level_id(&mut w, &lv)?;
    let level_bytes = w.write()?;
    // 6. the description: the donor's (its mode, kits, pictures) with the level's name, map and music
    let st = game.startup();
    let dd = read_one(
        game,
        &st,
        &format!("Levels/{}_description.dbx", donor_lv.to_ascii_uppercase()),
    )?;
    let mut dw = LevelWriter::parse(&dd)?;
    new_guids(&mut dw.d, &format!("{seed}:description"))?;
    let asset =
        dw.d.of_type(b".LevelDescriptionAsset")
            .first()
            .copied()
            .ok_or_else(|| err("the donor's description has no LevelDescriptionAsset"))?;
    let set_text = |dw: &mut LevelWriter, f: &[u8], v: &str| -> Result<()> {
        let x =
            dw.d.child(asset, f)
                .ok_or_else(|| err(format!("description: no {}", String::from_utf8_lossy(f))))?;
        dw.d.set_text(x, v.as_bytes()).map_err(|e| err(e.0))
    };
    set_text(&mut dw, b"Name", &format!("Levels/{lv}_description"))?;
    set_text(&mut dw, b"LevelName", &format!("Levels/{lv}"))?;
    dw.d.set_attr(asset, b"id", format!("{lv}_description").as_bytes())
        .map_err(|e| err(e.0))?;
    let dc =
        dw.d.child(asset, b"Description")
            .ok_or_else(|| err("description: no Description"))?;
    let keep_desc = DescriptionKeep {
        refs: keep
            .refs
            .iter()
            .filter(|(f, _)| f != b"RichPresence")
            .cloned()
            .collect(),
        name: keep.name.clone(),
        description: keep.description.clone(),
    };
    keep_desc.apply(&mut dw, dc)?;
    let desc_bytes = dw.write()?;
    let desc_name = format!("Levels/{lv}_description.dbx");
    report.push(format!(
        "description: {desc_name} ({}'s: {} mode, kits; the base's name {:?}, map and music)",
        donor_lv, spec.mode, keep.name
    ));
    let mut level_all = vec![
        StagedEntry {
            name: format!("Levels/{lv}.dbx"),
            type_name: NONRES.into(),
            data: level_bytes,
        },
        StagedEntry {
            name: desc_name.clone(),
            type_name: NONRES.into(),
            data: desc_bytes.clone(),
        },
    ];
    level_all.extend(level_entries);
    // the partitions the layer names that the base's bundles hide (SP_01's deleted markers over mp_common's
    // flags and spawn camera): copied from mp_common into the level's own bundle, which comes first
    let mut block_loaded: Vec<PathBuf> = mft_blocks(game, &low, &base).first().map_or_default(|b| {
        b.iter()
            .filter_map(|l| l.strip_prefix("BundleTarget "))
            .filter(|t| !t.to_ascii_lowercase().contains(&format!("levels/{low}/")))
            .map(|t| game.bundle(t))
            .collect()
    });
    block_loaded.extend(game.startup());
    let filled = fill_hidden(game, &block_loaded, &level_all)?;
    if !filled.is_empty() {
        report.push(format!(
            "hidden partitions: {} entries copied from mp_common (the base's deleted markers hide them): {}",
            filled.len(),
            filled.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    level_all.extend(filled);
    // 7. the other bundles: the base's terrain overlay, its minimap, its loading music
    let pick = |bundles: Vec<PathBuf>, keep: &dyn Fn(&str) -> bool| -> Result<Vec<StagedEntry>> {
        let live = game.live(&bundles)?;
        let mut f: Vec<&Found> = live.iter().filter(|(n, _)| keep(n)).map(|x| x.1).collect();
        f.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        let data = read_found(&f)?;
        Ok(f.iter()
            .zip(data)
            .map(|((_, e), d)| StagedEntry {
                name: e.name.clone(),
                type_name: e.type_name.clone(),
                data: d,
            })
            .collect())
    };
    let terrain = pick(
        game.level_bundles(&base, "terrain").into_iter().take(1).collect(),
        &|_| true,
    )?;
    let ingame = pick(game.level_bundles(&base, "ingame"), &|n| {
        n.starts_with(&format!("levels/{base}/ui/"))
    })?;
    let loader = pick(game.level_bundles(&base, "loader"), &|n| n.starts_with("sound/"))?;
    // write the stage
    if out.is_dir() {
        for k in KINDS {
            let _ = std::fs::remove_file(out.join(format!("{k}-00.fbrb")));
        }
    }
    std::fs::create_dir_all(out)?;
    let mut files = Vec::new();
    let mut put = |kind: &str, bytes: Vec<u8>, entries: usize| -> Result<()> {
        let path = format!("{kind}-00.fbrb");
        std::fs::write(out.join(&path), &bytes)?;
        files.push(StagedFile {
            install: format!("Package/levels/{low}/{path}"),
            path,
            sha1: sha1_hex(&bytes),
            size: bytes.len() as u64,
            entries,
        });
        Ok(())
    };
    put("level", pack_entries(&level_all)?, level_all.len())?;
    put("terrain", pack_entries(&terrain)?, terrain.len())?;
    put("ingame", pack_entries(&ingame)?, ingame.len())?;
    put("loader", pack_entries(&loader)?, loader.len())?;
    let desc_file = format!("{lv}_description.dbx");
    std::fs::write(out.join(&desc_file), &desc_bytes)?;
    report.push(format!(
        "bundles: level {} entries, terrain {}, ingame {}, loader {}",
        level_all.len(),
        terrain.len(),
        ingame.len(),
        loader.len()
    ));
    let rec = StageRecord {
        format: format!("{FORMAT}-stage"),
        level: lv.clone(),
        title: spec.title.clone(),
        mode: mode_title(&spec.mode),
        base: base.clone(),
        donor: donor_lv.clone(),
        base_sha1,
        project: format!("{} ({project_file})", project.id),
        files,
        description: (desc_file, desc_name),
        mft_blocks: mft_blocks(game, &low, &base),
        bundle_manifest: bundle_manifest_lines(&low),
        level_list: vec![format!("{low}.dbx"), format!("{low}_description.dbx")],
        report,
        converted: conv,
    };
    write_record(out, &rec)?;
    Ok(rec)
}

/// The partitions `ours` reference (transitively) that no bundle of `loaded` gives - hidden by an earlier bundle's
/// deleted marker - taken from `mp_common`'s level bundle, each with its resources (`<stem>.res`, `<stem>_*.res`).
///
/// # Errors
/// A bundle does not read.
fn fill_hidden(game: &Game, loaded: &[PathBuf], ours: &[StagedEntry]) -> Result<Vec<StagedEntry>> {
    let live = game.live(loaded)?;
    let common = game.live(&[game.bundle("game:levels/mp_common/level-00.fbrb")])?;
    let mut have: HashSet<String> = ours.iter().map(|e| e.name.to_ascii_lowercase()).collect();
    let mut out: Vec<StagedEntry> = Vec::new();
    let mut todo: Vec<Vec<u8>> = ours
        .iter()
        .filter(|e| e.name.to_ascii_lowercase().ends_with(".dbx"))
        .map(|e| e.data.clone())
        .collect();
    while let Some(data) = todo.pop() {
        let Ok(d) = EditDbx::parse(&data) else { continue };
        let mut want: Vec<String> = refs_of(&d)
            .into_iter()
            .filter_map(|r| {
                r.rsplit_once('/')
                    .map(|(p, _)| format!("{}.dbx", p.to_ascii_lowercase()))
            })
            .filter(|n| !have.contains(n) && !live.contains_key(n) && common.contains_key(n))
            .collect();
        want.sort();
        want.dedup();
        for n in want {
            let st = n.trim_end_matches(".dbx").to_owned();
            let mut names: Vec<&String> = common
                .keys()
                .filter(|k| {
                    **k == n
                        || (k.ends_with(".res")
                            && (k.trim_end_matches(".res") == st || k.starts_with(&format!("{st}_"))))
                })
                .filter(|k| !have.contains(*k))
                .collect();
            names.sort();
            let found: Vec<&Found> = names.iter().filter_map(|k| common.get(*k)).collect();
            for ((_, e), bytes) in found.iter().zip(read_found(&found)?) {
                have.insert(e.name.to_ascii_lowercase());
                if e.name.to_ascii_lowercase().ends_with(".dbx") {
                    todo.push(bytes.clone());
                }
                out.push(StagedEntry {
                    name: e.name.clone(),
                    type_name: e.type_name.clone(),
                    data: bytes,
                });
            }
        }
    }
    Ok(out)
}

/// The maps.ini key of a mode (`conquest` -> `Conquest`).
#[must_use]
pub fn mode_title(mode: &str) -> String {
    match mode.to_ascii_lowercase().as_str() {
        "conquest" => "Conquest".into(),
        "rush" => "Rush".into(),
        "squadrush" | "squad rush" => "Squad Rush".into(),
        "squaddeathmatch" | "squad deathmatch" => "Squad Deathmatch".into(),
        other => other.to_owned(),
    }
}

/// Whether a level folder of the game copy is an earlier install of an export (`opfor-mapedit-installed.json`
/// beside its bundles): only such a level may be replaced by a later export of the same name (section 6, ruling 1).
#[must_use]
pub fn is_custom(game: &Game, low: &str) -> bool {
    game.bundle(&format!("package:levels/{low}/opfor-mapedit-installed.json"))
        .is_file()
}

fn write_record(out: &Path, rec: &StageRecord) -> Result<()> {
    let json = serde_json::to_string_pretty(rec).map_err(|e| err(e.to_string()))?;
    std::fs::write(out.join("stage.json"), json + "\n")?;
    std::fs::write(out.join("report.txt"), rec.report.join("\n") + "\n")?;
    let mut reg = format!(
        "# {} ({}): what an install adds to the game copy (nothing was written)\n# bundles: Package\\levels\\{}\\\n",
        rec.title,
        rec.level,
        rec.level.to_ascii_lowercase()
    );
    for f in &rec.files {
        let _ = writeln!(reg, "#   {} ({} bytes, sha1 {})", f.install, f.size, f.sha1);
    }
    reg.push_str("# package.mft blocks:\n");
    for b in &rec.mft_blocks {
        reg.push_str(&(b.join("\n") + "\n\n"));
    }
    reg.push_str("# bundleManifest lines:\n");
    reg.push_str(&(rec.bundle_manifest.join("\n") + "\n"));
    let _ = writeln!(
        reg,
        "# startup-00: {} and LevelListQuery items {:?}",
        rec.description.1, rec.level_list
    );
    std::fs::write(out.join("registration.txt"), reg)?;
    Ok(())
}

/// A stage folder's record.
///
/// # Errors
/// No `stage.json`, or not one.
pub fn read_record(stage: &Path) -> Result<StageRecord> {
    let t = std::fs::read_to_string(stage.join("stage.json"))?;
    serde_json::from_str(&t).map_err(|e| err(format!("stage.json: {e}")))
}

// ------------------------------------------------------------------------------------------------ checking

/// What [`check`] found: each check with its result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckReport {
    /// (passed, what) per check.
    pub checks: Vec<(bool, String)>,
}

impl CheckReport {
    fn add(&mut self, ok: bool, what: impl Into<String>) {
        self.checks.push((ok, what.into()));
    }

    /// Every check passed.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.checks.iter().all(|c| c.0)
    }
}

fn refs_of(d: &EditDbx) -> Vec<String> {
    let mut out = Vec::new();
    for r in d.roots() {
        for e in d.walk(*r) {
            if let Some(v) = d.attr(e, b"ref")
                && v != b"null"
            {
                out.push(String::from_utf8_lossy(v).into_owned());
            }
        }
    }
    out
}

fn instance_guids(d: &EditDbx) -> HashSet<String> {
    d.instances()
        .into_iter()
        .map(|e| String::from_utf8_lossy(d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase())
        .collect()
}

/// Every live entry name (lower case) of every bundle of the game copy.
fn all_partition_names(root: &Path) -> Result<HashSet<String>> {
    let mut out = HashSet::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            let p = e.path();
            if p.is_dir() {
                dirs.push(p);
            } else if p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fbrb"))
                && let Ok(h) = fbrb::entries_path(&p)
            {
                out.extend(
                    h.entries
                        .iter()
                        .filter(|x| !x.is_deleted())
                        .map(|x| x.name.to_ascii_lowercase()),
                );
            }
        }
    }
    Ok(out)
}

/// The references of the stock partition(s) one of ours was made from: the level partition from the base and
/// the donor levels, a root file from the base's, anything else from `mp_common`'s copy of the same name.
fn stock_refs(game: &Game, rec: &StageRecord, ours: &str) -> Result<Vec<String>> {
    let low = rec.level.to_ascii_lowercase();
    let o = ours.to_ascii_lowercase();
    let mut sources: Vec<(Vec<PathBuf>, String)> = Vec::new();
    if o == format!("levels/{low}.dbx") {
        sources.push((game.loaded(&rec.base, "level"), format!("levels/{}.dbx", rec.base)));
        if !rec.donor.is_empty() {
            sources.push((game.loaded(&rec.donor, "level"), format!("levels/{}.dbx", rec.donor)));
        }
    } else if let Some(rest) = o.strip_prefix(&format!("levels/{low}/")) {
        sources.push((game.loaded(&rec.base, "level"), format!("levels/{}/{rest}", rec.base)));
    } else {
        sources.push((vec![game.bundle("game:levels/mp_common/level-00.fbrb")], o));
    }
    let mut out = Vec::new();
    for (bundles, name) in sources {
        let live = game.live(&bundles)?;
        if let Some(f) = live.get(&name)
            && let Some(data) = read_found(&[f])?.pop()
            && let Ok(d) = EditDbx::parse(&data)
        {
            out.extend(refs_of(&d));
        }
    }
    Ok(out)
}

/// Proves a stage offline against the game's rules (nothing is written).
///
/// # Errors
/// The stage's record or a bundle does not read.
#[allow(clippy::too_many_lines, reason = "the checks in order, each named")]
pub fn check(game: &Game, stage: &Path) -> Result<CheckReport> {
    let rec = read_record(stage)?;
    let low = rec.level.to_ascii_lowercase();
    let mut r = CheckReport::default();
    // the bundles parse, each entry reads, every partition parses; the files are the ones recorded
    let mut ours: Vec<(String, EditDbx)> = Vec::new();
    for f in &rec.files {
        let p = stage.join(&f.path);
        let bytes = std::fs::read(&p)?;
        r.add(
            sha1_hex(&bytes) == f.sha1,
            format!("{}: the staged file is the one recorded", f.path),
        );
        let (h, data) = fbrb::extract_path_uncached(&p).map_err(|e| err(format!("{}: {e}", p.display())))?;
        let mut bad = Vec::new();
        for (e, d) in h.entries.iter().zip(&data) {
            if e.name.to_ascii_lowercase().ends_with(".dbx") && !e.is_deleted() {
                match EditDbx::parse(d) {
                    Ok(x) if bc2_formats::dbx::to_xml(d).is_ok() => {
                        if f.path == "level-00.fbrb" {
                            ours.push((e.name.clone(), x));
                        }
                    }
                    _ => bad.push(e.name.clone()),
                }
            }
        }
        r.add(
            bad.is_empty() && h.entries.len() == f.entries,
            format!(
                "{}: opens, {} entries, every partition parses{}",
                f.path,
                h.entries.len(),
                if bad.is_empty() {
                    String::new()
                } else {
                    format!(" (not: {bad:?})")
                }
            ),
        );
    }
    // the registration: one block and one manifest line per bundle, each block's targets present
    let kinds_ok = rec.mft_blocks.len() == KINDS.len()
        && KINDS.iter().all(|k| {
            rec.mft_blocks
                .iter()
                .filter(|b| {
                    b.first()
                        .is_some_and(|s| *s == format!("BundleSource game:levels/{low}/{k}-00.fbrb"))
                })
                .count()
                == 1
        })
        && rec.bundle_manifest == bundle_manifest_lines(&low)
        && rec.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>()
            == KINDS.iter().map(|k| format!("{k}-00.fbrb")).collect::<Vec<_>>();
    r.add(
        kinds_ok,
        format!("registration: a package.mft block and a bundleManifest line for each of {KINDS:?}"),
    );
    let mut missing_targets = Vec::new();
    for b in &rec.mft_blocks {
        for t in b.iter().filter_map(|l| l.strip_prefix("BundleTarget ")) {
            let own = t.to_ascii_lowercase().contains(&format!("levels/{low}/"));
            if !own && !game.bundle(t).is_file() {
                missing_targets.push(t.to_owned());
            }
        }
    }
    r.add(
        missing_targets.is_empty(),
        format!("registration: every target bundle exists {missing_targets:?}"),
    );
    r.add(
        game.targets(&format!("levels/{low}/level-00.fbrb")).is_empty() || is_custom(game, &low),
        format!("the name {} is not a level of the game copy", rec.level),
    );
    // every reference of the new level's partitions resolves in the bundles the level loads
    let level_block = rec.mft_blocks.first().cloned().unwrap_or_default();
    let mut loaded = vec![stage.join("level-00.fbrb")];
    for t in level_block.iter().filter_map(|l| l.strip_prefix("BundleTarget ")) {
        if !t.to_ascii_lowercase().contains(&format!("levels/{low}/")) {
            loaded.push(game.bundle(t));
        }
    }
    loaded.extend(game.startup());
    for kind in ["ingame", "loader", "terrain"] {
        loaded.push(stage.join(format!("{kind}-00.fbrb")));
    }
    let live = game.live(&loaded)?;
    let mut wanted: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new(); // partition -> (guid, from)
    let mut local_bad = Vec::new();
    for (name, d) in &ours {
        let own = instance_guids(d);
        for rf in refs_of(d) {
            match rf.rsplit_once('/') {
                Some((part, guid)) => wanted
                    .entry(part.to_ascii_lowercase())
                    .or_default()
                    .push((guid.to_ascii_uppercase(), name.clone())),
                None => {
                    if !own.contains(&rf.to_ascii_uppercase()) {
                        local_bad.push(format!("{name}: {rf}"));
                    }
                }
            }
        }
    }
    r.add(
        local_bad.is_empty(),
        format!(
            "local references: every one names an instance of its partition {:?}",
            local_bad.iter().take(5).collect::<Vec<_>>()
        ),
    );
    let mut no_partition = Vec::new();
    let mut need: Vec<(&String, &Found)> = Vec::new();
    for part in wanted.keys() {
        match live.get(&format!("{part}.dbx")) {
            Some(f) => need.push((part, f)),
            None => no_partition.push(part.clone()),
        }
    }
    let found: Vec<&Found> = need.iter().map(|x| x.1).collect();
    let datas = read_found(&found)?;
    let mut no_instance = Vec::new();
    for ((part, _), data) in need.iter().zip(datas) {
        let Ok(d) = EditDbx::parse(&data) else {
            no_partition.push(format!("{part} (does not parse)"));
            continue;
        };
        let have = instance_guids(&d);
        for (g, from) in wanted.get(*part).map_or_default(Vec::as_slice) {
            if !have.contains(g) {
                no_instance.push(format!("{from} -> {part}/{g}"));
            }
        }
    }
    // a partition no bundle of the whole game copy has, named by the same reference in the stock partition ours was
    // made from, is DICE's own dangling reference (the stock levels load with it): listed, not a failure
    let mut dangling = Vec::new();
    if !no_partition.is_empty() {
        let everywhere = all_partition_names(&game.root)?;
        let mut stock_cache: HashMap<String, Vec<String>> = HashMap::new();
        let mut keep = Vec::new();
        for part in no_partition {
            let absent = !everywhere.contains(&format!("{part}.dbx"));
            let froms: HashSet<&String> = wanted.get(&part).map_or_default(|v| v.iter().map(|x| &x.1).collect());
            let mut stock_too = absent && !froms.is_empty();
            for from in froms {
                let refs = if let Some(v) = stock_cache.get(from) {
                    v.clone()
                } else {
                    let v = stock_refs(game, &rec, from)?;
                    stock_cache.insert(from.clone(), v.clone());
                    v
                };
                stock_too &= refs
                    .iter()
                    .any(|x| x.rsplit_once('/').is_some_and(|(p, _)| p.eq_ignore_ascii_case(&part)));
            }
            if stock_too {
                dangling.push(part);
            } else {
                keep.push(part);
            }
        }
        no_partition = keep;
    }
    r.add(
        true,
        format!(
            "references to partitions in no bundle of the game copy, as DICE's own source partitions have them: {dangling:?}"
        ),
    );
    let total: usize = wanted.values().map(Vec::len).sum();
    r.add(
        no_partition.is_empty(),
        format!(
            "references: {total} into {} partitions; every partition is in a bundle the level loads {:?}",
            wanted.len(),
            no_partition.iter().take(8).collect::<Vec<_>>()
        ),
    );
    r.add(
        no_instance.is_empty(),
        format!(
            "references: every one names an instance of its partition {:?}",
            no_instance.iter().take(8).collect::<Vec<_>>()
        ),
    );
    // the level and its description
    let lvl = ours
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(&format!("Levels/{}.dbx", rec.level)));
    let level_name = lvl.and_then(|(_, d)| {
        let ld = d.of_type(b".LevelData").first().copied()?;
        d.child(ld, b"Name")
            .and_then(|x| d.text(x))
            .map(|t| String::from_utf8_lossy(t).into_owned())
    });
    r.add(
        level_name.as_deref() == Some(format!("Levels/{}", rec.level).as_str()),
        format!("level partition: LevelData.Name {level_name:?}"),
    );
    let desc = ours.iter().find(|(n, _)| n.eq_ignore_ascii_case(&rec.description.1));
    let desc_level = desc.and_then(|(_, d)| {
        let a = d.of_type(b".LevelDescriptionAsset").first().copied()?;
        d.child(a, b"LevelName")
            .and_then(|x| d.text(x))
            .map(|t| String::from_utf8_lossy(t).into_owned())
    });
    r.add(
        desc_level.as_deref() == Some(format!("Levels/{}", rec.level).as_str()),
        format!("description: LevelName {desc_level:?}"),
    );
    // the cull tree: one tree, and one object per slot, each slot a leaf of a tree
    if let (Some((_, lw)), Some((_, tree))) = (
        lvl,
        ours.iter()
            .find(|(n, _)| n.to_ascii_lowercase().ends_with("staticculltree_exclusive.dbx")),
    ) {
        match crate::culltree::CullTree::read(tree) {
            Ok(t) => {
                let w = LevelWriter { d: lw.clone() };
                let mut slots: HashMap<u32, usize> = HashMap::new();
                let mut outside = 0;
                for e in w.d.of_type(b".ReferenceObjectData") {
                    let s = crate::culltree::slot_of(&w, e);
                    if s > t.enum_offset {
                        *slots.entry(s).or_default() += 1;
                        if t.leaf(s - t.enum_offset).is_none() {
                            outside += 1;
                        }
                    }
                }
                let shared = slots.values().filter(|&&n| n > 1).count();
                r.add(
                    shared == 0 && outside == 0,
                    format!(
                        "cull tree: one preorder tree of {} leaves; {} slotted objects, {shared} slots shared, {outside} \
                         outside the tree",
                        t.leaf_count(),
                        slots.len()
                    ),
                );
            }
            Err(e) => r.add(false, format!("cull tree: {e}")),
        }
    }
    Ok(r)
}

// ------------------------------------------------------------------------------------------------ startup patch

/// The startup bundle with the custom levels' descriptions and level list items: `source` unpacked (FbrbTool's
/// way), every custom level of `remove` taken out (its description, its list items), each of `add` (level id,
/// description bytes) put in, the list sorted as DICE's is; packed to `out` with the source's layout. Returns the
/// list's items.
///
/// # Errors
/// The source does not unpack, has no level list, or the pack fails.
pub fn patch_startup(
    source: &Path,
    add: &[(String, Vec<u8>)],
    remove: &[String],
    work: &Path,
    out: &Path,
) -> Result<Vec<String>> {
    if work.exists() {
        std::fs::remove_dir_all(work)?;
    }
    fbrb::unpack_path(source, work).map_err(|e| err(format!("{}: {e}", source.display())))?;
    let man_path = work.join("fbrb.manifest");
    let man = String::from_utf8_lossy(&std::fs::read(&man_path)?).into_owned();
    let mut lines: Vec<String> = man.lines().map(str::to_owned).collect();
    let drop: HashSet<String> = remove
        .iter()
        .chain(add.iter().map(|a| &a.0))
        .map(|l| format!("levels/{}_description.dbx", l.to_ascii_lowercase()))
        .collect();
    let head = lines.first().cloned().unwrap_or_default();
    let mut body: Vec<String> = lines
        .drain(1..)
        .filter(|l| !drop.contains(&l.split('|').next().unwrap_or_default().to_ascii_lowercase()))
        .collect();
    // the level list
    let list_line = body
        .iter()
        .find(|l| l.split('|').next().is_some_and(|n| n.eq_ignore_ascii_case(LEVEL_LIST)))
        .cloned()
        .ok_or_else(|| err(format!("{} has no {LEVEL_LIST}", source.display())))?;
    let list_name = list_line.split('|').next().unwrap_or(LEVEL_LIST).to_owned();
    let list_path = fbrb::entry_path(work, &list_name)?;
    let mut d = EditDbx::parse(&std::fs::read(&list_path)?).map_err(|e| err(format!("{LEVEL_LIST}: {e}")))?;
    let inst = d
        .instances()
        .first()
        .copied()
        .ok_or_else(|| err(format!("{LEVEL_LIST}: no instance")))?;
    let results = d
        .child(inst, b"Results")
        .ok_or_else(|| err(format!("{LEVEL_LIST}: no Results array")))?;
    let tmpl = *d
        .children(results)
        .and_then(<[u32]>::first)
        .ok_or_else(|| err(format!("{LEVEL_LIST}: an empty Results array")))?;
    let gone: HashSet<String> = remove
        .iter()
        .chain(add.iter().map(|a| &a.0))
        .flat_map(|l| {
            let l = l.to_ascii_lowercase();
            [format!("{l}.dbx"), format!("{l}_description.dbx")]
        })
        .collect();
    let mut items: Vec<String> = d
        .children(results)
        .unwrap_or_default()
        .iter()
        .filter_map(|&c| d.text(c).map(|t| String::from_utf8_lossy(t).into_owned()))
        .filter(|t| !gone.contains(&t.to_ascii_lowercase()))
        .collect();
    for (l, _) in add {
        let l = l.to_ascii_lowercase();
        items.push(format!("{l}.dbx"));
        items.push(format!("{l}_description.dbx"));
    }
    items.sort();
    let mut kids = Vec::with_capacity(items.len());
    for it in &items {
        let c = d.clone_node(tmpl);
        d.set_text(c, it.as_bytes()).map_err(|e| err(e.0))?;
        kids.push(c);
    }
    *d.children_mut(results).map_err(|e| err(e.0))? = kids;
    let list_bytes = d.write().map_err(|e| err(e.0))?;
    std::fs::write(&list_path, &list_bytes)?;
    for l in &mut body {
        let mut f: Vec<String> = l.split('|').map(str::to_owned).collect();
        if f.first().is_some_and(|n| n.eq_ignore_ascii_case(LEVEL_LIST)) {
            if let Some(s2) = f.get_mut(3) {
                *s2 = list_bytes.len().to_string();
            }
            if let Some(s) = f.get_mut(4) {
                *s = list_bytes.len().to_string();
            }
            *l = f.join("|");
        }
    }
    for (l, bytes) in add {
        let name = format!("Levels/{}_description.dbx", l.to_ascii_uppercase());
        let p = fbrb::entry_path(work, &name)?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, bytes)?;
        body.push(format!("{name}|{NONRES}|{FLAGS:08x}|{}|{}", bytes.len(), bytes.len()));
    }
    std::fs::write(&man_path, format!("{head}\n{}\n", body.join("\n")))?;
    fbrb::pack_path(work, out)?;
    std::fs::remove_dir_all(work)?;
    Ok(items)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn root_files_and_names() {
        assert!(is_root_file("shaders.dbx"));
        assert!(is_root_file("Shaders/Dx11_Single.res"));
        assert!(is_root_file("StaticModelGroupEntity_Grid00x00_HavokAsset.res"));
        assert!(is_root_file("ForestEntity_HavokAsset.dbx"));
        assert!(!is_root_file("Materials_Win32.dbx"));
        assert!(!is_root_file("UI/minimapTexture.dbx"));
        assert!(!is_root_file("fx/x/emitter.dbx"));
        assert!(!is_root_file("subtitles.dbx"));
        assert_eq!(
            renamed_entry("Levels/SP_01/shaders/Dx11_Single.res", "sp_01", "mapedit_x_cq"),
            "Levels/MAPEDIT_X_CQ/shaders/Dx11_Single.res"
        );
        assert_eq!(stem("Levels/SP_01/shaders.dbx"), "levels/sp_01/shaders");
        let m =
            parse_mft("Signature x\r\n\r\nBundleSource game:a\r\nBundleTarget package:a\r\nBundleTarget game:b\r\n");
        assert_eq!(
            m,
            vec![("game:a".to_owned(), vec!["package:a".to_owned(), "game:b".to_owned()])]
        );
        assert_eq!(
            bundle_manifest_lines("X")[3],
            "levels/x/loader\t00000000-0000-0000-0000-000000000000\t1"
        );
        let spec = ExportSpec {
            format: FORMAT.into(),
            version: 1,
            level: "bad name".into(),
            title: String::new(),
            mode: "conquest".into(),
            layer: LayerSpec {
                donor: "mp_009cq".into(),
                bases: vec![],
                flags: vec![],
                combat_area: String::new(),
            },
        };
        assert!(spec.level_id().is_err());
    }
}
