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

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use bc2_formats::dbx::edit::EditDbx;
use bc2_formats::fbrb::{self, Entry, Layout, PackItem};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::mpconvert::{self, Converted, DescriptionKeep, Ground, LayerSpec};
use crate::profile::ContentProfile;
use crate::project::{MapProject, Profile, sha1_hex};
use crate::schema::Schema;
use crate::write::LevelWriter;

/// The export file's `format`.
pub const FORMAT: &str = "bfbc2-mapexport";
/// The export file's `version`.
pub const VERSION: u32 = 1;
/// The bundles every new level has, as `package.mft` and `bundleManifest` name them (`wake_install.KINDS` less
/// `vo/en`). Its voice-over blocks are the donor's (round E3c, [`crate::profile::Announcer`]): the stock levels'
/// `vo/<lang>` sources are absent from the game copy and their blocks target the common voice-over bundles, which
/// are there; a WW2 level's own `vo/en-00` is in `Package`, and the export ships a copy of it.
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
    /// The folder under `Package\levels\` (round EX1; empty: the id in lower case, the only folder the game takes:
    /// [`crate::names`]).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub folder: String,
    /// The name the launcher and the server browser show (OPFOR's `maps.ini`).
    pub title: String,
    /// The game mode (`conquest`).
    pub mode: String,
    /// The mode's layer.
    pub layer: LayerSpec,
    /// The content profile (round E3b): the set of maps the level is made for. The layer's donor is one of that
    /// set's levels of the mode, and the common bundles, kits, factions and level fields are the set's
    /// ([`crate::profile`]). Older export files: BFBC2.
    #[serde(default)]
    pub profile: Profile,
    /// The level's own name and pictures (round E3b): `title` as its localized name, a description, a picture.
    /// `None` (older export files): the base level's names and loading picture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
}

/// The level's own name and pictures (round E3b): the menus, the server browser and the loading screen show the
/// export's `title` (loc key `<LV>_NAME`) and this description (`<LV>_DESC`); the loading screen and the map list
/// show the picture.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Identity {
    /// The on-screen name (round EX1; empty: the export's `title`), every language without its own.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    /// The description under the name (the loading screen, the map list).
    #[serde(default)]
    pub description: String,
    /// The picture: a PNG or JPEG file. Empty: the base level's own loading picture.
    #[serde(default)]
    pub picture: String,
    /// Per language (`french` ...): its own name and description (round EX1).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub languages: BTreeMap<String, crate::names::LangText>,
}

/// The export file beside a project (`<project>.export.json`; the editor keeps the Export form there).
#[must_use]
pub fn spec_path_of(project: &Path) -> PathBuf {
    let mut s = project.as_os_str().to_owned();
    s.push(".export.json");
    PathBuf::from(s)
}

/// The loc keys of a level's own name and description (the loading screen cuts the name's at `_NAME` and finds the
/// picture under the level's id).
#[must_use]
pub fn loc_keys(lv: &str) -> (String, String) {
    let up = lv.to_ascii_uppercase();
    (format!("{up}_NAME"), format!("{up}_DESC"))
}

/// The on-demand entries of a level's pictures: its loading screen and its map-list picture.
#[must_use]
pub fn picture_entries(lv: &str) -> [String; 2] {
    let low = lv.to_ascii_lowercase();
    [
        format!("Awards/images/loadingScreens/{low}.res"),
        format!("Awards/images/Levels/{low}.res"),
    ]
}

/// A text the localization files hold as it is: printable ASCII, 1 to `max` characters.
///
/// # Errors
/// Empty, too long, or a character outside printable ASCII (the files store other characters through their
/// histogram; the export writes plain text only).
pub fn loc_text(what: &str, t: &str, max: usize) -> Result<()> {
    if t.trim().is_empty() || t.len() > max {
        return Err(err(format!("{what} must be 1 to {max} characters")));
    }
    if let Some(c) = t.chars().find(|c| !(' '..='~').contains(c)) {
        return Err(err(format!(
            "{what}: {c:?} is not plain ASCII (letters, digits, punctuation and spaces)"
        )));
    }
    Ok(())
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

    /// Every name of the level (round EX1): the id, the folder, the server-browser name, the on-screen names.
    #[must_use]
    pub fn names(&self) -> crate::names::LevelNames {
        let id = self.identity.clone().unwrap_or_default();
        crate::names::LevelNames {
            level: self.level.clone(),
            folder: self.folder.clone(),
            browser: self.title.clone(),
            own: self.identity.is_some(),
            name: id.name,
            description: id.description,
            languages: id.languages,
        }
    }

    /// The names set (the picture kept; the base's names: no identity).
    pub fn set_names(&mut self, n: &crate::names::LevelNames) {
        self.level.clone_from(&n.level);
        self.folder.clone_from(&n.folder);
        self.title.clone_from(&n.browser);
        if n.own {
            let id = self.identity.get_or_insert_with(Identity::default);
            id.name.clone_from(&n.name);
            id.description.clone_from(&n.description);
            id.languages.clone_from(&n.languages);
        } else {
            self.identity = None;
        }
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
    /// `bundleManifest`'s bundle names (`levels/mp_009cq/vo/en`), lower case; empty when there is none.
    manifest: Vec<String>,
    /// The game copy has no `Dist\win32\bundleManifest` (round DF1, audit AU4-BUG-10).
    manifest_missing: bool,
}

/// A live entry of a set of bundles: (bundle, entry).
pub type Found = (PathBuf, Entry);

impl Game {
    /// The game folder; its `package.mft` read.
    ///
    /// # Errors
    /// No `Dist\win32\package.mft`, or a `bundleManifest` that is there but does not read (round DF1, audit
    /// AU4-BUG-10: read as empty, an export would register voice-over bundles in `package.mft` without their
    /// `bundleManifest` lines, the checksum crash).
    pub fn open(root: &Path) -> Result<Self> {
        let p = root.join("Dist").join("win32").join("package.mft");
        let text = std::fs::read(&p).map_err(|e| err(format!("{}: {e}", p.display())))?;
        let bm_path = root.join("Dist").join("win32").join("bundleManifest");
        let (bm, manifest_missing) = match std::fs::read(&bm_path) {
            Ok(b) => (b, false),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Vec::new(), true),
            Err(e) => return Err(err(format!("{}: {e}", bm_path.display()))),
        };
        Ok(Self {
            root: root.to_path_buf(),
            manifest_missing,
            mft: parse_mft(&String::from_utf8_lossy(&text)),
            manifest: String::from_utf8_lossy(&bm)
                .lines()
                .filter_map(|l| l.split('\t').next())
                .map(|n| n.trim().to_ascii_lowercase())
                .filter(|n| !n.is_empty())
                .collect(),
        })
    }

    /// Whether the game copy has a `Dist\win32\bundleManifest` (read when it opened).
    #[must_use]
    pub fn has_manifest(&self) -> bool {
        !self.manifest_missing
    }

    /// Whether `bundleManifest` names a bundle (`levels/mp_009cq/vo/en`).
    #[must_use]
    pub fn in_manifest(&self, name: &str) -> bool {
        let n = name.to_ascii_lowercase();
        self.manifest.contains(&n)
    }

    /// A level's voice-over blocks of `package.mft`, in its order: (`vo/en`, the block's targets).
    #[must_use]
    pub fn vo_blocks(&self, lv: &str) -> Vec<(String, Vec<String>)> {
        let prefix = format!("game:levels/{}/", lv.to_ascii_lowercase());
        self.mft
            .iter()
            .filter_map(|(s, t)| {
                let rest = s.to_ascii_lowercase().strip_prefix(&prefix)?.to_owned();
                let kind = rest.strip_suffix("-00.fbrb")?;
                kind.starts_with("vo/").then(|| (kind.to_owned(), t.clone()))
            })
            .collect()
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

    /// Every `BundleSource` of `package.mft`, in its order (`game:levels/mp_005/level-00.fbrb`).
    #[must_use]
    pub fn sources(&self) -> Vec<&str> {
        self.mft.iter().map(|(s, _)| s.as_str()).collect()
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

    /// The on-demand awards bundles (`Package\async`, then `Dist\win32\async`): the loading screens, the map-list
    /// pictures.
    #[must_use]
    pub fn ondemand_awards(&self) -> Vec<PathBuf> {
        [
            "package:async/ondemand_awards-00.fbrb",
            "game:async/ondemand_awards-00.fbrb",
        ]
        .iter()
        .map(|n| self.bundle(n))
        .filter(|p| p.is_file())
        .collect()
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
/// Reads entries (by their [`Found`]) grouped by bundle, through the bundle cache (round E3d: the closure reads the same
/// large bundles round after round; the disk cache keeps their payloads decompressed once).
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
        let data = fbrb::read_path(b, &names).map_err(|e| err(format!("{}: {e}", b.display())))?;
        for ((i, _), d) in items.into_iter().zip(data) {
            if let Some(slot) = out.get_mut(i) {
                *slot = d;
            }
        }
    }
    Ok(out)
}

/// The project's partition as its log leaves it (round E3d: a mode choice's donor read from the game copy), and the
/// base's.
///
/// # Errors
/// The base or a donor is not there, or an operation does not apply.
pub fn current(game: &Game, project: &MapProject) -> Result<(LevelWriter, LevelWriter)> {
    let base_bytes = level_partition(game, &project.base.level)?;
    let base_w = LevelWriter::parse(&base_bytes)?;
    let schema = Schema::scan([&base_w.d]);
    let a = crate::write::apply_with(&base_bytes, &project.operations, &schema, &|lv| {
        level_partition(game, lv).ok()
    })
    .map_err(|e| err(e.to_string()))?;
    Ok((LevelWriter::parse(&a.bytes)?, base_w))
}

/// The project's target chosen (round E3d): era `profile`, mode `mode` (a key or a name: `goldrush`, `Rush`), its
/// layer from `donor` (else the profile's default donor of the mode), as the next operation of the log
/// ([`crate::Op::SetMode`]): placed by the first guess ([`crate::modelayer::choose`]) on the project as it is now,
/// heights over the two height fields.
///
/// # Errors
/// The mode is not offered or the era has no level of it, the donor is not one, or the project does not apply.
pub fn choose_mode(
    game: &Game,
    project: &MapProject,
    profile: Profile,
    mode: &str,
    donor: Option<&str>,
) -> Result<crate::modelayer::ModeChoice> {
    let mode_k = crate::modelayer::mode_key(mode);
    if let Some(why) = crate::modelayer::not_offered(&mode_k) {
        return Err(err(why));
    }
    let profiles = crate::profile::scan(game, &mode_k)?;
    let prof = profiles.get(profile)?;
    let donor_lv = match donor {
        Some(d) => d.to_ascii_lowercase(),
        None => prof
            .default_donor()
            .map(|d| d.level.clone())
            .ok_or_else(|| err("the era has no level of the mode"))?,
    };
    if prof.donor(&donor_lv).is_none() {
        return Err(err(format!(
            "{donor_lv} is not a {} level of the {} mode",
            profile.word(),
            crate::modelayer::mode_name(&mode_k)
        )));
    }
    let (cur, base_w) = current(game, project)?;
    let donor_bytes = level_partition(game, &donor_lv)?;
    let dw = LevelWriter::parse(&donor_bytes)?;
    let hb = heightfield(game, &project.base.level.to_ascii_lowercase(), &base_w)?;
    let hd = heightfield(game, &donor_lv, &dw)?;
    let ground = match (&hd, &hb) {
        (Some(d), Some(b)) => Some(Ground { donor: d, base: b }),
        _ => None,
    };
    let seed = format!("{}:mode:{}", project.id, project.next_n());
    Ok(crate::modelayer::choose(
        &cur,
        &dw,
        &donor_lv,
        &sha1_hex(&donor_bytes),
        profile,
        &mode_k,
        &seed,
        ground.as_ref(),
        None,
    ))
}

/// "Use selection" for a flag, base or M-COM of the project's layer (round E3d): the move that puts it, with what
/// belongs to it ([`crate::modelayer::cluster`]), at `to` (x, z), heights following the base's ground.
///
/// # Errors
/// The project does not apply.
pub fn move_anchor(game: &Game, project: &MapProject, guid: &str, to: [f32; 2]) -> Result<Vec<crate::ops::Moved>> {
    let (cur, base_w) = current(game, project)?;
    let hb = heightfield(game, &project.base.level.to_ascii_lowercase(), &base_w)?;
    Ok(crate::modelayer::move_cluster(&cur, guid, to, hb.as_ref()))
}

/// A blueprint's reference (`<partition>/<primary instance guid>`, lower case) for a partition of a bundle of the
/// game copy (an object placed from another level's or set's bundle: E2b's "+bundle").
///
/// # Errors
/// The partition is not in the bundles, or it does not parse.
pub fn blueprint_ref(game: &Game, bundles: &[PathBuf], partition: &str) -> Result<String> {
    let name = if partition.to_ascii_lowercase().ends_with(".dbx") {
        partition.to_owned()
    } else {
        format!("{partition}.dbx")
    };
    let data = read_one(game, bundles, &name)?;
    let d = EditDbx::parse(&data).map_err(|e| err(format!("{name}: {e}")))?;
    let p = d.partition().ok_or_else(|| err(format!("{name}: no partition")))?;
    let g = d
        .attr(p, b"primaryInstance")
        .ok_or_else(|| err(format!("{name}: no primary instance")))?;
    Ok(format!(
        "{}/{}",
        stem(&name),
        String::from_utf8_lossy(g).to_ascii_lowercase()
    ))
}

/// A level's partition (`Levels/<LV>.dbx`) from the bundles its level block loads.
///
/// # Errors
/// It is not there, or a bundle does not read.
pub fn level_partition(game: &Game, lv: &str) -> Result<Vec<u8>> {
    let low = lv.to_ascii_lowercase();
    read_one(
        game,
        &game.loaded(&low, "level"),
        &format!("Levels/{}.dbx", low.to_ascii_uppercase()),
    )
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
    /// The content profile (round E3b).
    #[serde(default)]
    pub profile: Profile,
    /// The texts an install adds to every language's localization (loc key, text): the level's own name and
    /// description (round E3b); empty: the base level's.
    #[serde(default)]
    pub strings: Vec<(String, String)>,
    /// The texts of languages that have their own (round EX1): language -> (loc key, text), each in place of
    /// `strings`' text of that key in that language.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub strings_by_language: BTreeMap<String, Vec<(String, String)>>,
    /// The pictures an install adds to the on-demand awards bundle (`install`: the entry's name).
    #[serde(default)]
    pub awards: Vec<StagedFile>,
    /// The donor's own announcer partitions copied into the level's bundle (round E3c; a WW2 donor's line banks and
    /// trigger sets), by name.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub announcer: Vec<String>,
    /// The partitions the closure copied into the level's bundle (round E3d): from where and why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub closure: Vec<Copied>,
    /// The mode layer came from the project's own target (round E3d) rather than the export file's graft.
    #[serde(default)]
    pub project_mode: bool,
    /// The objects given new cull leaves and whose shape each took (round XF1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cull: Vec<SlotRecord>,
    /// Objects left without a slot because no level slots their blueprint: (guid, blueprint) (round XF1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unslotted: Vec<(String, String)>,
    /// The shader entries added to the level's databases (round XF1).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shaders: Vec<crate::shaders::DatabaseAdds>,
    /// The weather the export gave the level (round VW2); `None`: the base's own, untouched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weather: Option<crate::weather::WeatherRecord>,
}

/// An object the export gave new cull leaves (round XF1): its blueprint, its slot and the shape's source
/// (`base <guid> slot <n>`, `<level> <guid> slot <n>`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotRecord {
    /// The object.
    pub guid: String,
    /// Its blueprint (partition, lower case).
    pub blueprint: String,
    /// Its slot.
    pub slot: u32,
    /// Where its shape was read.
    pub source: String,
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
        let n = bc2_formats::guid::new_guid(crate::NAMESPACE, &[seed, &g]);
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
    mpconvert::set_partition_guid(d, &bc2_formats::guid::new_guid(crate::NAMESPACE, &[seed, "partition"]))
}

/// The main height field of a level (its `TerrainEntityData`'s terrain), from the bundles its terrain and level
/// load; `None` when there is none.
fn heightfield(game: &Game, lv: &str, partition: &LevelWriter) -> Result<Option<bc2_formats::terrain::Heightfield>> {
    let Some(name) = heightfield_name(partition) else {
        return Ok(None);
    };
    let mut bundles = game.loaded(lv, "terrain");
    bundles.extend(game.loaded(lv, "level"));
    let live = game.live(&bundles)?;
    let Some(f) = live.get(&name) else { return Ok(None) };
    let data = read_found(&[f])?.pop().unwrap_or_default();
    Ok(bc2_formats::terrain::parse(&data).ok())
}

/// The entry of a level's main height field (`terrains/sp_01/sp_01.heightfield-000.res`, lower case), from its
/// `TerrainEntityData`'s terrain reference; `None` when it has none.
fn heightfield_name(partition: &LevelWriter) -> Option<String> {
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
    // terrains/sp_01/sp_01/<guid> -> Terrains/SP_01/SP_01.Heightfield-000.res
    let r = terrain?;
    let parts: Vec<&str> = r.split('/').collect();
    let (a, b) = (parts.get(1)?, parts.get(2)?);
    Some(format!("terrains/{a}/{b}.heightfield-000.res").to_ascii_lowercase())
}

/// The new level's `package.mft` blocks: its own bundle first, then (level, terrain) the base level's bundles, the
/// profile's common bundles of the kind ([`ContentProfile::commons`]: BFBC2 `mp_common`, Vietnam `nam_common` and
/// `mp_common` ...) and, for the level, the base's other targets after them; (ingame, loader) the profile's common
/// bundles, as the stock multiplayer levels of the set have. The base's targets that are another set's common bundles
/// (`foreign`: a WW2 base's `ww2_common` under a BFBC2 profile) are left out (round E3d): the level's closure brings
/// what it needs of them into its own bundle.
#[must_use]
pub fn mft_blocks(game: &Game, lv: &str, base: &str, profile: &ContentProfile, foreign: &[String]) -> Vec<Vec<String>> {
    let lv = lv.to_ascii_lowercase();
    let base = base.to_ascii_lowercase();
    let mut out = Vec::new();
    for kind in KINDS {
        let own = format!("levels/{lv}/{kind}-00.fbrb");
        let mut lines = vec![format!("BundleSource game:{own}")];
        let mut targets: Vec<String> = vec![format!("package:{own}")];
        let commons = profile.commons(kind);
        match kind {
            "level" | "terrain" => {
                let base_targets = game.targets(&format!("levels/{base}/{kind}-00.fbrb"));
                let (mine, other): (Vec<String>, Vec<String>) = base_targets
                    .into_iter()
                    .partition(|t| t.to_ascii_lowercase().contains(&format!("levels/{base}/")));
                targets.extend(mine.into_iter().filter(|t| game.bundle(t).is_file()));
                targets.extend(commons.iter().cloned());
                if kind == "level" {
                    targets.extend(other.into_iter().filter(|t| {
                        !commons.iter().any(|c| c.eq_ignore_ascii_case(t))
                            && !foreign.iter().any(|c| c.eq_ignore_ascii_case(t))
                    }));
                } else {
                    // the terrain is the map's, wherever its era keeps it (a Vietnam map's height field is in
                    // nam_common's terrain bundle): every terrain bundle the base loads stays (round E3d)
                    for t in other {
                        if game.bundle(&t).is_file() && !targets.iter().any(|x| x.eq_ignore_ascii_case(&t)) {
                            targets.push(t);
                        }
                    }
                }
            }
            _ => targets.extend(commons.iter().cloned()),
        }
        lines.extend(targets.into_iter().map(|t| format!("BundleTarget {t}")));
        out.push(lines);
    }
    out
}

/// The new level's `bundleManifest` lines (`wake_install.bm_with_level`).
#[must_use]
pub fn bundle_manifest_lines(lv: &str) -> Vec<String> {
    KINDS.iter().map(|k| bundle_manifest_line(lv, k)).collect()
}

/// One `bundleManifest` line of a level's bundle (`levels/<lv>/vo/en\t<null guid>\t1`).
#[must_use]
pub fn bundle_manifest_line(lv: &str, kind: &str) -> String {
    format!(
        "levels/{}/{kind}\t00000000-0000-0000-0000-000000000000\t1",
        lv.to_ascii_lowercase()
    )
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
    // 0. the target: the project's own (round E3d: its era and mode, the mode's layer in its log), else the export
    // file's (E3's graft at export time)
    let target = project.mode();
    let (profile_sel, mode_k, donor_lv) = match target {
        Some(c) => (c.profile, c.mode.clone(), c.donor.to_ascii_lowercase()),
        None => (
            spec.profile,
            crate::modelayer::mode_key(&spec.mode),
            spec.layer.donor.to_ascii_lowercase(),
        ),
    };
    if let Some(why) = crate::modelayer::not_offered(&mode_k) {
        return Err(err(why));
    }
    // the content profile: the set's common bundles, donors and level fields, read from its stock levels
    let profiles = crate::profile::scan(game, &mode_k)?;
    let prof = profiles.get(profile_sel)?;
    let donor_info = prof.donor(&donor_lv).ok_or_else(|| {
        err(format!(
            "{donor_lv} is not a {} level of the {} mode (the profile's: {})",
            profile_sel.word(),
            crate::modelayer::mode_name(&mode_k),
            prof.donors
                .iter()
                .map(|d| d.level.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))
    })?;
    report.push(format!(
        "profile: {} (from its {} multiplayer levels): level commons {:?}; donor {} - game mode {}, factions {} / {}, \
         {} kit pictures; level fields {:?}",
        profile_sel.word(),
        prof.levels.len(),
        prof.commons("level"),
        donor_lv,
        donor_info.game_mode,
        donor_info.factions[0],
        donor_info.factions[1],
        donor_info.kits.len(),
        prof.fields.iter().map(|f| f.0.as_str()).collect::<Vec<_>>()
    ));
    // the names together (round EX1): the id, its folder, the server-browser name, the on-screen names per language
    let problems = crate::names::check(
        &spec.names(),
        Some(&project.id),
        &mode_title(&mode_k),
        Some(&crate::names::Taken::read(game)),
        Some(&crate::names::Languages::read(game)),
    );
    if !problems.is_empty() {
        return Err(err(problems
            .iter()
            .map(|p| p.message.as_str())
            .collect::<Vec<_>>()
            .join(" ")));
    }
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
    let applied = crate::write::apply_with(&base_bytes, &project.operations, &schema, &|lv| {
        level_partition(game, lv).ok()
    })
    .map_err(|e| err(e.to_string()))?;
    report.push(format!(
        "project: {} operations applied ({} transforms, {} deleted, {} duplicated, {} placed, {} fields)",
        applied.operations, applied.transforms, applied.deleted, applied.duplicated, applied.placed, applied.fields
    ));
    let mut w = LevelWriter::parse(&applied.bytes)?;
    // 2. multiplayer: the campaign out, the donor's layer in, placed, the level's fields
    let donor_bytes = level_partition(game, &donor_lv)?;
    let donor = LevelWriter::parse(&donor_bytes)?;
    let mut keep = DescriptionKeep::of(&w).ok_or_else(|| err("the base level has no LevelDescription"))?;
    // the base's own name key names its loading picture (SP_01_NAME -> sp_01)
    let base_picture = base_picture_name(game, &keep.name, &base)?;
    if spec.identity.is_some() {
        (keep.name, keep.description) = loc_keys(&lv);
    }
    let mut conv = Converted::default();
    let choice = project.strip();
    report.push(format!("campaign content: {}", choice.describe()));
    let seed = format!("{}:{low}", project.id);
    let map = if let Some(c) = target {
        // round E3d: the mode's layer is the project's own (its log brought it in and took the base's out); the
        // campaign checklist removes the base's campaign scripting, never what the log made
        let made: HashSet<String> = project
            .operations
            .iter()
            .flat_map(|o| o.op.creates())
            .map(str::to_ascii_uppercase)
            .collect();
        mpconvert::strip(&mut w, &made, &choice, &mut conv)?;
        let have: HashSet<String> = w.guids().into_iter().collect();
        let map: HashMap<String, String> = c.map().into_iter().filter(|(_, n)| have.contains(n)).collect();
        let sig = crate::modelayer::signature(&w);
        report.push(format!(
            "mode layer (the project's): {} {} from {}: {} of its {} instances in the level, {} of the base's taken \
             out; first guess fitted {:?} into {:?} at scale {:.2}; {} bases {:?}, {} flags, {} combat areas, \
             counters {:?}, {} teams, {} spawns, stages {:?}",
            c.profile.word(),
            crate::modelayer::mode_name(&c.mode),
            c.donor,
            map.len(),
            c.items.len(),
            c.removed.len(),
            c.fit.from,
            c.fit.to,
            c.fit.scale,
            sig.bases.iter().sum::<usize>(),
            sig.bases,
            sig.flags,
            sig.combat_areas,
            sig.counters,
            sig.teams,
            sig.spawns,
            sig.stages
        ));
        if sha1_hex(&donor_bytes) != c.donor_sha1 {
            report.push(format!(
                "  note: {}'s partition changed since the mode was chosen (sha1 {} then)",
                c.donor, c.donor_sha1
            ));
        }
        map
    } else {
        let keep_area: HashSet<String> = [spec.layer.combat_area.to_ascii_uppercase()].into();
        if w.find(&spec.layer.combat_area).is_none() {
            return Err(err(format!(
                "the combat area {} is not in the base level",
                spec.layer.combat_area
            )));
        }
        mpconvert::strip(&mut w, &keep_area, &choice, &mut conv)?;
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
        map
    };
    mpconvert::level_fields(&mut w, &donor, &keep, &map, &mut conv)?;
    // the profile's own level fields (Vietnam's sound states ...): the donor's, where the base's differ
    let mine = crate::profile::level_fields(&w);
    for (f, v) in &prof.fields {
        if mine.get(f) != Some(v) {
            mpconvert::take_field(&mut w, &donor, f, &map, &mut conv)?;
        }
    }
    report.push(format!(
        "multiplayer: {} campaign instances removed ({} event connections, {} references cut); {} instances and {} \
         event connections of {}'s {} layer grafted; {} references of the layer not followed",
        conv.removed.iter().map(|x| x.1).sum::<usize>(),
        conv.connections_removed,
        conv.references_cut,
        conv.grafted.iter().map(|x| x.1).sum::<usize>(),
        conv.connections_grafted,
        donor_lv,
        crate::modelayer::mode_name(&mode_k),
        conv.not_followed.len()
    ));
    for (n, k, m) in &conv.placed {
        report.push(format!("  placed {n:?}: {k} instances moved {m:.0} m"));
    }
    // round VW2: the project's weather (a state of the level's, another level's, the night look)
    let weather = match project.weather() {
        Some(c) => {
            let s = crate::weather::stage(game, c, &mut w, &base, &low, &seed)?;
            report.extend(s.report.iter().cloned());
            Some(s)
        }
        None => None,
    };
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
    let mut cull_record: Vec<SlotRecord> = Vec::new();
    let mut unslotted_record: Vec<(String, String)> = Vec::new();
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
    // 4. the cull tree: moved, copied, placed and grafted objects get new leaves of their own blueprint's shape
    // (round XF1: never a template's), from the base's placements of it, else from another level's
    if let Some((name, mut d)) = exclusive {
        let t_cull = std::time::Instant::now();
        let mut others = LevelShapes::new(game, std::slice::from_ref(&donor_lv));
        let r = crate::culltree::reslot(&base_w, &mut w, &mut d, &mut |bp| others.find(bp))?;
        report.push(format!(
            "cull tree: {} objects given new leaves (leaves {} -> {}); {} left without a slot (no level slots their \
             blueprint)",
            r.objects.len(),
            r.leaves.0,
            r.leaves.1,
            r.unslotted.len()
        ));
        for o in &r.objects {
            report.push(format!(
                "  {}: slot {} -> {} ({} leaves, {}'s shape from {})",
                o.guid, o.old, o.new, o.leaves, o.blueprint, o.source
            ));
        }
        for (g, bp, s) in &r.unslotted {
            report.push(format!("  {g}: {bp}: slot {s} -> 0 (no level slots it)"));
        }
        report.push(format!(
            "cull tree: shapes found in {:.1} s",
            t_cull.elapsed().as_secs_f64()
        ));
        cull_record = r
            .objects
            .iter()
            .map(|o| SlotRecord {
                guid: o.guid.clone(),
                blueprint: o.blueprint.clone(),
                slot: o.new,
                source: o.source.clone(),
            })
            .collect();
        unslotted_record = r.unslotted.iter().map(|(g, bp, _)| (g.clone(), bp.clone())).collect();
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
        donor_lv,
        crate::modelayer::mode_name(&mode_k),
        keep.name
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
    let weather_record = weather.map(|s| {
        level_all.extend(s.entries);
        s.record
    });
    // the announcer's voice-over as the donor's set has it (round E3c): the donor's own line banks and trigger sets
    // (a WW2 level's: BF1943's lines; BFBC2's, BFBC1's and Vietnam's levels have none, theirs are the commons') into
    // the level's own bundle, which comes first, so the layer's triggers name what the donor's do
    let ann = &donor_info.announcer;
    let mut announcer = Vec::new();
    if !ann.partitions.is_empty() {
        // what the layer reaches of them: the level's references into announcer partitions, followed through the
        // donor's own copies (OPFOR's mp_001 holds Squad Deathmatch trigger sets no Conquest layer names)
        let own = game.live(&game.level_bundles(&donor_lv, "level"))?;
        let mut have: HashSet<String> = level_all.iter().map(|e| e.name.to_ascii_lowercase()).collect();
        let announcer_refs = |data: &[u8]| -> Vec<String> {
            EditDbx::parse(data).map_or_default(|d| {
                refs_of(&d)
                    .into_iter()
                    .filter_map(|r| {
                        r.rsplit_once('/')
                            .map(|(p, _)| format!("{}.dbx", p.to_ascii_lowercase()))
                    })
                    .filter(|n| crate::profile::is_announcer(n))
                    .collect()
            })
        };
        let mut todo: Vec<String> = level_all.first().map_or_default(|e| announcer_refs(&e.data));
        while let Some(n) = todo.pop() {
            if have.contains(&n) {
                continue;
            }
            let Some(f) = own.get(&n) else { continue };
            have.insert(n);
            let data = read_found(&[f])?.pop().unwrap_or_default();
            todo.extend(announcer_refs(&data));
            announcer.push(StagedEntry {
                name: f.1.name.clone(),
                type_name: f.1.type_name.clone(),
                data,
            });
        }
        announcer.sort_by(|a, b| a.name.cmp(&b.name));
    }
    report.push(format!(
        "announcer: {} of the donor's own partitions copied{}",
        announcer.len(),
        if announcer.is_empty() {
            String::new()
        } else {
            format!(
                " ({})",
                announcer.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
            )
        }
    ));
    let announcer: Vec<String> = announcer
        .into_iter()
        .map(|e| {
            let n = e.name.clone();
            level_all.push(e);
            n
        })
        .collect();
    report.push(format!(
        "announcer: {}'s ({} voice-over): {}",
        donor_lv,
        profile_sel.word(),
        ann.describe()
    ));
    // the partitions the layer names that the bundles the level loads do not give: the base's deleted markers hide
    // mp_common's flags and spawn camera (SP_01), a WW2 donor's team kits and flags are in its own bundle - copied
    // from the profile's common bundles, else the donor's, into the level's own bundle, which comes first
    let foreign: Vec<String> = crate::profile::set_commons(game)?
        .into_iter()
        .filter(|(p, _)| *p != profile_sel)
        .flat_map(|(_, c)| c)
        .filter(|c| !prof.commons("level").iter().any(|m| m.eq_ignore_ascii_case(c)))
        .collect();
    let mut blocks = mft_blocks(game, &low, &base, prof, &foreign);
    let mut block_loaded: Vec<PathBuf> = blocks.first().map_or_default(|b| {
        b.iter()
            .filter_map(|l| l.strip_prefix("BundleTarget "))
            .filter(|t| !t.to_ascii_lowercase().contains(&format!("levels/{low}/")))
            .map(|t| game.bundle(t))
            .collect()
    });
    block_loaded.extend(game.startup());
    // every bundle of the level block, each read alone (Vietnam's nam_common hides sp_common's partitions SP_01's
    // own world uses), then the donor's own
    let mut sources: Vec<PathBuf> = blocks.first().map_or_default(|b| {
        b.iter()
            .filter_map(|l| l.strip_prefix("BundleTarget "))
            .filter(|t| !t.to_ascii_lowercase().contains(&format!("levels/{low}/")))
            .map(|t| game.bundle(t))
            .collect()
    });
    sources.extend(game.level_bundles(&donor_lv, "level"));
    // round E3d: the base's own bundle may hold another era's copies of the announcer's banks (a WW2 map's: BF1943's
    // lines merged into vo_common and vo_en), which come before the commons: the donor's own versions go into the
    // level's bundle, which comes first, so the level's voices are the target era's
    let shadowed = announcer_over_base(game, &block_loaded, &donor_lv, &level_all)?;
    if !shadowed.is_empty() {
        report.push(format!(
            "announcer: {} of the base's own copies replaced by the donor's (the target era's voices): {}",
            shadowed.len(),
            shadowed.iter().map(|e| e.name.as_str()).collect::<Vec<_>>().join(", ")
        ));
    }
    let mut announcer = announcer;
    for e in shadowed {
        announcer.push(e.name.clone());
        level_all.push(e);
    }
    // round E3d: the level's whole closure (what it names, and what the partitions it reaches in its own and its
    // base's bundles name) resolves under the profile's bundles; what does not is copied (from those bundles read
    // alone, the donor's, else any level bundle of the game copy: another set's common, another level's own)
    let mut base_loaded = game.loaded(&base, "level");
    base_loaded.extend(game.startup());
    // then what the base itself loads, each read alone (a WW2 base's ww2_common, which a BFBC2 profile drops)
    for b in game.loaded(&base, "level") {
        if !sources.contains(&b) {
            sources.push(b);
        }
    }
    let cl = closure(game, &block_loaded, &base_loaded, &level_all, &sources, true)?;
    report.push(format!(
        "closure: {} partitions of the level's own and its base's bundles walked; {} entries copied into the level's \
         bundle (what the bundles it loads hide or lack); {} references to partitions in no bundle at all (DICE's own)",
        cl.walked,
        cl.copied.len(),
        cl.nowhere.len()
    ));
    report.extend(closure_summary(&cl.record));
    let closure_record = cl.record;
    level_all.extend(cl.copied);
    // round XF1: the level's shader databases get the entries of every mesh it reaches that the base's lack (an
    // object placed from another level or set, another mode's layer, another set's kits): a level draws only through
    // its own databases (SHDB-LVL-1), and a part with no program crashes the renderer (0x98BD6E, RNDR-DRAW-6)
    let t_shaders = std::time::Instant::now();
    let live_block = game.live(&block_loaded)?;
    let reached = crate::shaders::reach_staged(&level_all, &live_block)?;
    let mut prefer = vec![donor_lv.clone(), base.clone()];
    for c in &closure_record {
        if let Some(l) = c
            .from
            .replace('\\', "/")
            .to_ascii_lowercase()
            .strip_suffix("/level-00.fbrb")
            .and_then(|x| x.rsplit('/').next())
        {
            prefer.push(l.to_owned());
        }
    }
    let shader_record = crate::shaders::add_entries(game, &mut level_all, &reached, &prefer)?;
    for s in &shader_record {
        report.push(format!(
            "shaders: {}: {} of the {} meshes the level reaches had no entry; {} entries added for {} of them ({}); \
             {} in no level's database",
            s.database,
            s.missing,
            reached.len(),
            s.added.iter().map(|a| a.2).sum::<usize>(),
            s.added.len(),
            {
                let mut by: BTreeMap<&str, usize> = BTreeMap::new();
                for a in &s.added {
                    *by.entry(a.1.as_str()).or_default() += 1;
                }
                by.iter()
                    .map(|(l, n)| format!("{n} from {l}"))
                    .collect::<Vec<_>>()
                    .join(", ")
            },
            s.nowhere.len()
        ));
    }
    report.push(format!(
        "shaders: the reach and the merges in {:.1} s",
        t_shaders.elapsed().as_secs_f64()
    ));
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
    // the level's UI (its minimap) from the base's own ingame bundles and the levels' they load: a mode level's
    // minimap is its map's (`MP_005GR` reads `levels/mp_005/ui/`), whose bundle the new level does not load
    let ingame_bundles: Vec<PathBuf> = game
        .loaded(&base, "ingame")
        .into_iter()
        .filter(|b| !is_shared(game, b))
        .collect();
    let ingame = pick(ingame_bundles, &|n| n.starts_with("levels/") && n.contains("/ui/"))?;
    let loader = pick(game.level_bundles(&base, "loader"), &|n| n.starts_with("sound/"))?;
    // 8. the level's own name and pictures (round E3b)
    let (strings, strings_by_language, pictures, previews) = match &spec.identity {
        Some(id) => identity(game, spec, id, &lv, &donor_lv, &base_picture, &dd, &mut report)?,
        None => Default::default(),
    };
    // write the stage
    if out.is_dir() {
        for k in KINDS {
            let _ = std::fs::remove_file(out.join(format!("{k}-00.fbrb")));
        }
        let _ = std::fs::remove_dir_all(out.join("awards"));
        let _ = std::fs::remove_dir_all(out.join("vo"));
        let _ = std::fs::remove_file(out.join("closure.txt"));
        for p in ["preview_loading.png", "preview_level.png"] {
            let _ = std::fs::remove_file(out.join(p));
        }
    }
    std::fs::create_dir_all(out)?;
    let mut awards = Vec::new();
    for (entry, path, bytes) in &pictures {
        let p = out.join(path);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, bytes)?;
        awards.push(StagedFile {
            path: path.clone(),
            install: entry.clone(),
            sha1: sha1_hex(bytes),
            size: bytes.len() as u64,
            entries: 1,
        });
    }
    for (name, bytes) in &previews {
        std::fs::write(out.join(name), bytes)?;
    }
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
    // the donor's own voice-over bundles (a WW2 level's vo/en-00: BF1943's waves), byte for byte, and the donor's
    // voice-over blocks and manifest lines under the new name (round E3c)
    for (kind, b) in &ann.bundles {
        let bytes = std::fs::read(game.bundle(b))?;
        let n = fbrb::entries_path(game.bundle(b))
            .map_err(|e| err(format!("{b}: {e}")))?
            .entries
            .len();
        let path = format!("{kind}-00.fbrb");
        let p = out.join(&path);
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, &bytes)?;
        files.push(StagedFile {
            install: format!("Package/levels/{low}/{path}"),
            path,
            sha1: sha1_hex(&bytes),
            size: bytes.len() as u64,
            entries: n,
        });
    }
    blocks.extend(ann.blocks_for(&donor_lv, &low));
    let mut manifest = bundle_manifest_lines(&low);
    manifest.extend(ann.manifest.iter().map(|k| bundle_manifest_line(&low, k)));
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
        mode: mode_title(&mode_k),
        base: base.clone(),
        donor: donor_lv.clone(),
        base_sha1,
        project: format!("{} ({project_file})", project.id),
        files,
        description: (desc_file, desc_name),
        mft_blocks: blocks,
        bundle_manifest: manifest,
        level_list: vec![format!("{low}.dbx"), format!("{low}_description.dbx")],
        report,
        converted: conv,
        profile: profile_sel,
        strings,
        strings_by_language,
        awards,
        announcer,
        closure: closure_record,
        project_mode: target.is_some(),
        cull: cull_record,
        unslotted: unslotted_record,
        shaders: shader_record,
        weather: weather_record,
    };
    write_record(out, &rec)?;
    Ok(rec)
}

/// Blueprints' cull shapes from the game copy's levels (round XF1): for a blueprint the base does not slot, the
/// first slotted placement of it in the levels of `first` (the mode's donor), then in the levels whose own level
/// bundle holds its partition, then in every other level ([`crate::shaders::game_levels`]: DICE's first). Each
/// level's partition and exclusive tree are read once.
pub struct LevelShapes<'g> {
    game: &'g Game,
    first: Vec<String>,
    levels: Option<Vec<String>>,
    read: HashMap<String, HashMap<String, crate::culltree::Shape>>,
}

impl<'g> LevelShapes<'g> {
    /// Looks in `first` before the others.
    #[must_use]
    pub fn new(game: &'g Game, first: &[String]) -> Self {
        Self {
            game,
            first: first.iter().map(|l| l.to_ascii_lowercase()).collect(),
            levels: None,
            read: HashMap::new(),
        }
    }

    /// The levels to look in after `first` ([`crate::shaders::game_levels`]).
    fn levels(&mut self) -> &[String] {
        let game = self.game;
        self.levels.get_or_insert_with(|| crate::shaders::game_levels(game))
    }

    /// A level's blueprint shapes ([`crate::culltree::shapes`]); none where its partition or tree does not read.
    fn of_level(&mut self, lv: &str) -> &HashMap<String, crate::culltree::Shape> {
        let game = self.game;
        self.read.entry(lv.to_owned()).or_insert_with(|| {
            let read = || -> Result<HashMap<String, crate::culltree::Shape>> {
                let w = LevelWriter::parse(&level_partition(game, lv)?)?;
                let tree = read_one(
                    game,
                    &game.loaded(lv, "level"),
                    &format!("levels/{lv}/staticculltree_exclusive.dbx"),
                )?;
                let d = EditDbx::parse(&tree).map_err(|e| err(e.0))?;
                let t = crate::culltree::CullTree::read(&d)?;
                Ok(crate::culltree::shapes(&w, &t, lv).1)
            };
            read().unwrap_or_default()
        })
    }

    /// The blueprint's shape (`objects/props/crate/crate`, lower case), its source naming the level.
    pub fn find(&mut self, blueprint: &str) -> Option<crate::culltree::Shape> {
        if blueprint.is_empty() {
            return None;
        }
        let part = format!("{blueprint}.dbx");
        let mut order: Vec<String> = self.first.clone();
        let all: Vec<String> = self.levels().to_vec();
        let holders: Vec<String> = all
            .iter()
            .filter(|lv| {
                self.game
                    .live(&self.game.level_bundles(lv, "level"))
                    .is_ok_and(|l| l.contains_key(&part))
            })
            .cloned()
            .collect();
        for lv in holders.into_iter().chain(all) {
            if !order.contains(&lv) {
                order.push(lv);
            }
        }
        for lv in order {
            if let Some(s) = self.of_level(&lv).get(blueprint) {
                return Some(s.clone());
            }
        }
        None
    }
}

/// A partition the export copied into the level's own bundle (round E3d's closure): its entry, the bundle it was
/// copied from (relative to the game folder), why, and the first partition that names it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Copied {
    /// The entry (`objects/props/x/x.dbx`, or one of its resources).
    pub name: String,
    /// The bundle it came from, relative to the game folder.
    pub from: String,
    /// Why: hidden by a deleted marker of a bundle the level loads, or in none of them.
    pub why: String,
    /// The partition that names it first.
    pub by: String,
}

/// Whether a bundle is shared (a `_common` folder's or an `async` one): what such a bundle names resolves in the
/// shared bundles (DICE's and OPFOR's levels load them as they are), so the closure stops there.
fn is_shared(game: &Game, b: &Path) -> bool {
    b.strip_prefix(&game.root).is_ok_and(|rel| {
        rel.components().any(|c| {
            let n = c.as_os_str().to_string_lossy().to_ascii_lowercase();
            n.ends_with("_common") || n == "async"
        })
    })
}

fn rel_name(game: &Game, b: &Path) -> String {
    b.strip_prefix(&game.root)
        .map_or_else(|_| b.display().to_string(), |x| x.display().to_string())
}

/// The live entries of bundles in priority order ([`Game::live`]) and, for each name a deleted marker hides, the
/// bundle whose marker it is.
fn live_and_hidden(game: &Game, bundles: &[PathBuf]) -> Result<(HashMap<String, Found>, HashMap<String, PathBuf>)> {
    let mut out: HashMap<String, Found> = HashMap::new();
    let mut hidden: HashMap<String, PathBuf> = HashMap::new();
    let _ = game;
    for b in bundles.iter().filter(|b| b.is_file()) {
        let h = fbrb::entries_path(b).map_err(|e| err(format!("{}: {e}", b.display())))?;
        for e in h.entries {
            let low = e.name.to_ascii_lowercase();
            if out.contains_key(&low) || hidden.contains_key(&low) {
                continue;
            }
            if e.is_deleted() {
                hidden.insert(low, b.clone());
            } else {
                out.insert(low, (b.clone(), e));
            }
        }
    }
    Ok((out, hidden))
}

/// Every level bundle of the game copy (`Package`, then `Dist\win32`: `levels\<x>\level-00.fbrb`): the closure's
/// last sources, the shared ones first, then the levels' own by name.
fn all_level_bundles(game: &Game) -> Vec<PathBuf> {
    let mut shared = Vec::new();
    let mut own = Vec::new();
    for top in [
        game.root.join("Package").join("levels"),
        game.root.join("Dist").join("win32").join("levels"),
    ] {
        let Ok(rd) = std::fs::read_dir(&top) else { continue };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            let b = d.join("level-00.fbrb");
            if b.is_file() {
                if is_shared(game, &b) {
                    shared.push(b);
                } else {
                    own.push(b);
                }
            }
        }
    }
    shared.extend(own);
    shared
}

/// The partition names a partition's references name (`<path>.dbx`, lower case).
fn ref_partitions(data: &[u8]) -> Vec<String> {
    EditDbx::parse(data).map_or_default(|d| {
        let mut v: Vec<String> = refs_of(&d)
            .into_iter()
            .filter_map(|r| {
                r.rsplit_once('/')
                    .map(|(p, _)| format!("{}.dbx", p.to_ascii_lowercase()))
            })
            .collect();
        v.sort();
        v.dedup();
        v
    })
}

/// Where a partition the closure follows is from: `None` the level's own, `Some(None)` a base's partition walked,
/// `Some(Some(b))` copied from bundle `b`.
type Origin = Option<Option<PathBuf>>;

/// What [`closure`] found.
#[derive(Debug, Default)]
struct Closure {
    /// Entries copied into the level's bundle.
    copied: Vec<StagedEntry>,
    /// Where each came from and why.
    record: Vec<Copied>,
    /// Partitions walked in the bundles the level loads that are not shared (the level's own and its base's).
    walked: usize,
    /// Partitions named that are in no bundle at all (stock dangling references): (partition, who names it).
    nowhere: Vec<(String, String)>,
    /// Partitions named that are in a bundle the level does not load (the check's failures; the build copies them).
    unresolved: Vec<(String, String)>,
}

/// The level's closure under the bundles it loads (round E3d, generalising E3b's copy-back): every partition `ours`
/// names, and what the partitions it reaches in the level's own and its base's bundles name in turn, must resolve in
/// `loaded` (the level block's bundles and the startup bundles). A shared bundle's partitions are not followed (the
/// stock levels load them as they are). A partition that does not resolve - hidden by a deleted marker of a bundle
/// before it (a WW2 export's `ww2_common` over `mp_common`), or in no loaded bundle (a WW2 level's own copies under a
/// BFBC2 profile, an object placed from another level's or set's bundle) - is copied (`copy`) with its resources
/// (`<stem>.res`, `<stem>_*.res`) from the first of `sources` that has it (each read alone), else from any level
/// bundle of the game copy (the shared ones first), and its references are followed too. Without `copy` (the check)
/// they are listed as unresolved. A partition that only the base's own partitions name and that the base's own
/// bundles (`base_loaded`: what the stock base level loads) lack too is DICE's dangling reference: the stock level
/// loads with it, so it is listed with the others in no bundle, never copied from an unrelated level. A copy's own
/// references are looked for first in the bundle it came from.
///
/// # Errors
/// A bundle does not read.
#[allow(clippy::too_many_lines, reason = "the walk's rounds: name, look up, walk or copy")]
fn closure(
    game: &Game,
    loaded: &[PathBuf],
    base_loaded: &[PathBuf],
    ours: &[StagedEntry],
    sources: &[PathBuf],
    copy: bool,
) -> Result<Closure> {
    let (live, hidden) = live_and_hidden(game, loaded)?;
    let base_live = game.live(base_loaded)?;
    let mut per: Vec<(PathBuf, HashMap<String, Found>)> = sources
        .iter()
        .map(|b| Ok((b.clone(), game.live(std::slice::from_ref(b))?)))
        .collect::<Result<_>>()?;
    let mut everywhere: Option<Vec<(PathBuf, HashMap<String, Found>)>> = None;
    let mut have: HashSet<String> = ours.iter().map(|e| e.name.to_ascii_lowercase()).collect();
    let mut out = Closure::default();
    let mut todo: Vec<(Vec<u8>, String, Origin)> = ours
        .iter()
        .filter(|e| e.name.to_ascii_lowercase().ends_with(".dbx"))
        .map(|e| (e.data.clone(), e.name.clone(), None))
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    while !todo.is_empty() {
        // one round: every partition the round's partitions name (a walked base partition's naming counts least)
        let mut wanted: BTreeMap<String, (String, Origin)> = BTreeMap::new();
        for (data, by, origin) in todo.drain(..) {
            for n in ref_partitions(&data) {
                if have.contains(&n) || seen.contains(&n) {
                    continue;
                }
                match wanted.get_mut(&n) {
                    Some(w) if w.1 == Some(None) && origin != Some(None) => *w = (by.clone(), origin.clone()),
                    Some(_) => {}
                    None => {
                        wanted.insert(n, (by.clone(), origin.clone()));
                    }
                }
            }
        }
        let mut visit: Vec<(&Found, String)> = Vec::new();
        let mut copies: Vec<(String, String, Origin)> = Vec::new();
        for (n, (by, origin)) in wanted {
            seen.insert(n.clone());
            match live.get(&n) {
                Some(f) if is_shared(game, &f.0) => {}
                Some(f) => visit.push((f, n)),
                None => copies.push((n, by, origin)),
            }
        }
        out.walked += visit.len();
        let found: Vec<&Found> = visit.iter().map(|x| x.0).collect();
        for ((_, n), data) in visit.iter().zip(read_found(&found)?) {
            todo.push((data, n.clone(), Some(None)));
        }
        // the round's copies: chosen one by one (a copy's resources are not another's), then read together, one pass
        // per source bundle (round HG1: read one copy at a time, a WW2 export decompressed its source bundles' payloads
        // of hundreds of MB once per copied partition - minutes in a release build)
        let mut planned: Vec<(Vec<Found>, PathBuf, String, String)> = Vec::new();
        for (n, by, origin) in copies {
            if origin == Some(None) && !base_live.contains_key(&n) {
                // the stock base names it and does not have it either: DICE's own dangling reference
                out.nowhere.push((n, by));
                continue;
            }
            let mut from = match &origin {
                Some(Some(b)) if per.iter().any(|(x, c)| x == b && c.contains_key(&n)) => Some(b.clone()),
                _ => None,
            };
            if from.is_none() {
                from = per.iter().find(|(_, c)| c.contains_key(&n)).map(|(b, _)| b.clone());
            }
            if from.is_none() {
                let all = everywhere.get_or_insert_with(|| {
                    all_level_bundles(game)
                        .into_iter()
                        .filter_map(|b| game.live(std::slice::from_ref(&b)).ok().map(|l| (b, l)))
                        .collect()
                });
                if let Some((b, l)) = all.iter().find(|(_, c)| c.contains_key(&n)) {
                    per.push((b.clone(), l.clone()));
                    from = Some(b.clone());
                }
            }
            let Some(from) = from else {
                out.nowhere.push((n, by));
                continue;
            };
            if !copy {
                out.unresolved.push((n, by));
                continue;
            }
            let common = per
                .iter()
                .find(|(b, _)| *b == from)
                .map(|x| &x.1)
                .ok_or_else(|| err("closure: a source went away"))?;
            let why = hidden.get(&n).map_or_else(
                || "in no bundle the level loads".to_owned(),
                |h| format!("hidden by a deleted marker in {}", rel_name(game, h)),
            );
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
            let found: Vec<Found> = names.iter().filter_map(|k| common.get(*k)).cloned().collect();
            for (_, e) in &found {
                have.insert(e.name.to_ascii_lowercase());
            }
            planned.push((found, from, why, by));
        }
        let all: Vec<&Found> = planned.iter().flat_map(|p| p.0.iter()).collect();
        let mut data = read_found(&all)?.into_iter();
        for (found, from, why, by) in planned {
            for ((_, e), bytes) in found.iter().zip(data.by_ref()) {
                if e.name.to_ascii_lowercase().ends_with(".dbx") {
                    todo.push((bytes.clone(), e.name.clone(), Some(Some(from.clone()))));
                }
                out.record.push(Copied {
                    name: e.name.clone(),
                    from: rel_name(game, &from),
                    why: why.clone(),
                    by: by.clone(),
                });
                out.copied.push(StagedEntry {
                    name: e.name.clone(),
                    type_name: e.type_name.clone(),
                    data: bytes,
                });
            }
        }
    }
    Ok(out)
}

/// The closure's copies in words: per source bundle and reason, how many entries (the report).
fn closure_summary(record: &[Copied]) -> Vec<String> {
    let mut by: BTreeMap<(String, String), usize> = BTreeMap::new();
    for c in record {
        *by.entry((c.from.clone(), c.why.clone())).or_default() += 1;
    }
    by.into_iter()
        .map(|((from, why), n)| format!("  {n} entries from {from} ({why})"))
        .collect()
}

/// The partitions `ours` reference (transitively) that no bundle of `loaded` gives (E3b's copy-back; round E3d's
/// [`closure`] replaces it).
#[allow(dead_code, reason = "kept until E3d's closure is proven on every export")]
fn fill_hidden(game: &Game, loaded: &[PathBuf], ours: &[StagedEntry], sources: &[PathBuf]) -> Result<Vec<StagedEntry>> {
    let live = game.live(loaded)?;
    let per: Vec<HashMap<String, Found>> = sources
        .iter()
        .map(|b| game.live(std::slice::from_ref(b)))
        .collect::<Result<_>>()?;
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
            .filter(|n| !have.contains(n) && !live.contains_key(n) && per.iter().any(|c| c.contains_key(n)))
            .collect();
        want.sort();
        want.dedup();
        for n in want {
            // the first source with the partition gives it and its resources
            let Some(common) = per.iter().find(|c| c.contains_key(&n)) else {
                continue;
            };
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

/// The announcer partitions the level names (followed through their references) whose version in the bundles the
/// level loads (`loaded`, after `ours`) is not the donor's own load's (round E3d: a WW2 base's own bundle holds
/// BF1943's banks): the donor's versions, with their resources, to put first in the level's bundle.
///
/// # Errors
/// A bundle does not read.
fn announcer_over_base(game: &Game, loaded: &[PathBuf], donor: &str, ours: &[StagedEntry]) -> Result<Vec<StagedEntry>> {
    let mut dl = game.loaded(donor, "level");
    dl.extend(game.startup());
    let theirs_live = game.live(&dl)?;
    let mine_live = game.live(loaded)?;
    let mut have: HashSet<String> = ours.iter().map(|e| e.name.to_ascii_lowercase()).collect();
    let announcer_refs = |data: &[u8]| -> Vec<String> {
        ref_partitions(data)
            .into_iter()
            .filter(|n| crate::profile::is_announcer(n))
            .collect()
    };
    let mut todo: Vec<String> = ours.first().map_or_default(|e| announcer_refs(&e.data));
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    while let Some(n) = todo.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        let Some(t) = theirs_live.get(&n) else { continue };
        let theirs = read_found(&[t])?.pop().unwrap_or_default();
        todo.extend(announcer_refs(&theirs));
        if have.contains(&n) {
            continue;
        }
        let same = match mine_live.get(&n) {
            Some(m) if m.0 == t.0 => true,
            Some(m) => read_found(&[m])?.pop().is_some_and(|d| d == theirs),
            None => false,
        };
        if same {
            continue;
        }
        // the partition and its resources, from the donor's load
        let st = n.trim_end_matches(".dbx").to_owned();
        let mut names: Vec<&String> = theirs_live
            .keys()
            .filter(|k| {
                **k == n
                    || (k.ends_with(".res") && (k.trim_end_matches(".res") == st || k.starts_with(&format!("{st}_"))))
            })
            .filter(|k| !have.contains(*k))
            .collect();
        names.sort();
        let found: Vec<&Found> = names.iter().filter_map(|k| theirs_live.get(*k)).collect();
        for ((_, e), bytes) in found.iter().zip(read_found(&found)?) {
            have.insert(e.name.to_ascii_lowercase());
            out.push(StagedEntry {
                name: e.name.clone(),
                type_name: e.type_name.clone(),
                data: bytes,
            });
        }
    }
    Ok(out)
}

/// The `Name` loc key of a level's description in the startup bundles (`MP_009_NAME`, `SP_01_NAME`): cut at
/// `_NAME`, the name of its loading picture.
///
/// # Errors
/// No such description.
pub fn description_key(game: &Game, lv: &str) -> Result<String> {
    let d = read_one(
        game,
        &game.startup(),
        &format!("Levels/{}_description.dbx", lv.to_ascii_uppercase()),
    )?;
    description_name(&d)
}

/// The `Name` loc key of a description partition's level description (`MP_009_NAME`).
fn description_name(desc: &[u8]) -> Result<String> {
    let w = LevelWriter::parse(desc)?;
    let a =
        w.d.of_type(b".LevelDescriptionAsset")
            .first()
            .copied()
            .ok_or_else(|| err("a description without LevelDescriptionAsset"))?;
    w.d.child(a, b"Description")
        .and_then(|c| w.d.child(c, b"Name"))
        .and_then(|x| w.d.text(x))
        .map(|t| String::from_utf8_lossy(t).into_owned())
        .ok_or_else(|| err("a description without Name"))
}

/// The texts, pictures (entry, stage path, bytes) and previews (file, PNG bytes) of the level's own name and
/// pictures: the picture from the user's file or the base's loading picture, in the forms of the donor's own
/// loading picture (its name key's) and map-list picture.
#[allow(clippy::type_complexity, reason = "three lists the stage writes")]
#[allow(clippy::too_many_arguments, reason = "the export's form, the level and its donor")]
fn identity(
    game: &Game,
    spec: &ExportSpec,
    id: &Identity,
    lv: &str,
    donor_lv: &str,
    base_picture: &str,
    donor_desc: &[u8],
    report: &mut Vec<String>,
) -> Result<(
    Vec<(String, String)>,
    BTreeMap<String, Vec<(String, String)>>,
    Vec<(String, String, Vec<u8>)>,
    Vec<(String, Vec<u8>)>,
)> {
    let (nk, dk) = loc_keys(lv);
    let names = spec.names();
    let default_name = if id.name.trim().is_empty() {
        spec.title.trim()
    } else {
        id.name.trim()
    };
    let strings = vec![
        (nk.clone(), default_name.to_owned()),
        (dk.clone(), id.description.trim().to_owned()),
    ];
    // the languages with their own texts (round EX1): only the keys they change
    let mut by_language: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (lang, own) in &id.languages {
        let (name, desc) = names.texts(lang);
        let mut v = Vec::new();
        if !own.name.trim().is_empty() {
            v.push((nk.clone(), name));
        }
        if !own.description.trim().is_empty() {
            v.push((dk.clone(), desc));
        }
        if !v.is_empty() {
            by_language.insert(lang.clone(), v);
        }
    }
    report.push(format!(
        "names: id {lv}, folder Package\\levels\\{}\\, server browser {:?}; on screen {:?}{}",
        lv.to_ascii_lowercase(),
        spec.title.trim(),
        default_name,
        if by_language.is_empty() {
            String::new()
        } else {
            format!(
                ", own texts in {}",
                by_language.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        }
    ));
    let donor_key = description_name(donor_desc)?;
    let made = pictures(game, donor_lv, &donor_key, base_picture, &id.picture)?;
    let mut pictures = Vec::new();
    let mut previews = Vec::new();
    for (p, what) in made.into_iter().zip(["loading", "level"]) {
        let m = p.made;
        let entry = p.entry.replace("{lv}", &lv.to_ascii_lowercase());
        report.push(format!(
            "picture {}: from {}, in {}'s form (format {}, {} x {}, {} levels; the picture in {:?}), {} bytes, \
             {:.1} dB against what was encoded",
            entry,
            p.from,
            p.template,
            m.info.fmt,
            m.info.w,
            m.info.h,
            m.info.mips,
            m.area,
            m.wrapped.len(),
            m.psnr
        ));
        let path = format!(
            "awards/{}",
            entry.to_ascii_lowercase().trim_start_matches("awards/images/")
        );
        previews.push((
            format!("preview_{what}.png"),
            crate::awards::png_bytes(&m.decoded).map_err(err)?,
        ));
        pictures.push((entry, path, m.wrapped));
    }
    Ok((strings, by_language, pictures, previews))
}

/// The name of a base level's loading picture (`awards/images/loadingscreens/<name>.res`): its name key cut at
/// `_NAME` (`SP_01_NAME` -> `sp_01`), else, where the on-demand bundles have no picture of that name, the level's
/// own (OPFOR's Wake Island: key `WAKEISLAND2_NAME`, picture `wake_island_s.res`; round E3d).
///
/// # Errors
/// An on-demand bundle does not read.
pub fn base_picture_name(game: &Game, name_key: &str, base: &str) -> Result<String> {
    let by_key = name_key.strip_suffix("_NAME").unwrap_or(name_key).to_ascii_lowercase();
    let live = game.live(&game.ondemand_awards())?;
    let has = |n: &str| live.contains_key(&format!("awards/images/loadingscreens/{n}.res"));
    let own = base.to_ascii_lowercase();
    Ok(if !has(&by_key) && has(&own) { own } else { by_key })
}

/// One picture of the level made: the entry, the stock picture whose form it takes, where it came from, it.
#[derive(Debug, Clone)]
pub struct Picture {
    /// Its on-demand entry (`Awards/images/loadingScreens/<lv>.res`; `<lv>` until the level is named).
    pub entry: String,
    /// The donor's own picture whose form it has.
    pub template: String,
    /// Where the picture came from, in words.
    pub from: String,
    /// It.
    pub made: crate::awards::Made,
}

/// The level's loading and map-list pictures (in that order): `picture` (a PNG or JPEG file; empty: the base's
/// own loading picture, `base_picture` the base's name key cut at `_NAME`) in the forms of the donor's own pictures
/// (`donor_key`: the donor description's name key). The Export window's preview and the build make them alike.
///
/// # Errors
/// A picture is not there or does not decode, or the encoding fails.
pub fn pictures(
    game: &Game,
    donor_lv: &str,
    donor_key: &str,
    base_picture: &str,
    picture: &str,
) -> Result<Vec<Picture>> {
    let od = game.ondemand_awards();
    let (src, from) = if picture.trim().is_empty() {
        let name = format!("awards/images/loadingscreens/{base_picture}.res");
        let data = read_one(game, &od, &name).map_err(|e| {
            err(format!(
                "no picture given, and the base's loading picture is not there: {e}"
            ))
        })?;
        let (rgb, _) = crate::awards::decode(crate::awards::unwrap(&data))
            .ok_or_else(|| err(format!("{name} does not decode")))?;
        (rgb, format!("the base's loading picture {name}"))
    } else {
        let path = picture.trim();
        let bytes = std::fs::read(path).map_err(|e| err(format!("{path}: {e}")))?;
        let rgb = crate::awards::read_image(&bytes).map_err(|e| err(format!("{path}: {e}")))?;
        let from = format!("{path} ({} x {})", rgb.w, rgb.h);
        (rgb, from)
    };
    let templates = [
        format!(
            "awards/images/loadingscreens/{}.res",
            donor_key
                .strip_suffix("_NAME")
                .unwrap_or(donor_key)
                .to_ascii_lowercase()
        ),
        format!("awards/images/levels/{donor_lv}.res"),
    ];
    let mut out = Vec::new();
    for (entry, tmpl) in picture_entries("{lv}").iter().zip(&templates) {
        let t = read_one(game, &od, tmpl).map_err(|e| err(format!("the donor's picture: {e}")))?;
        let made = crate::awards::make(&src, &t).map_err(|e| err(format!("{entry}: {e}")))?;
        out.push(Picture {
            entry: entry.clone(),
            template: tmpl.clone(),
            from: from.clone(),
            made,
        });
    }
    Ok(out)
}

/// The maps.ini key of a mode (`conquest` -> `Conquest`, `goldrush` -> `Rush`, `teamdeathmatch` -> `Squad
/// Deathmatch`: [`crate::modelayer::mode_name`]).
#[must_use]
pub fn mode_title(mode: &str) -> String {
    crate::modelayer::mode_name(mode)
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
    let mut cl = String::from(
        "# the partitions the export copied into the level's bundle: entry, from, why, named by
",
    );
    for c in &rec.closure {
        let _ = writeln!(cl, "{}	{}	{}	{}", c.name, c.from, c.why, c.by);
    }
    std::fs::write(out.join("closure.txt"), cl)?;
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

/// Every live entry name (lower case) of every shared bundle of the game copy: the `_common` folders' and the
/// `async` ones (round E3b). A level's own bundles are left out: no other level loads them (OPFOR's converted WW2
/// levels hold copies of partitions DICE's levels only name, `iwo_jima_s`'s root `emitters/...`).
fn all_partition_names(root: &Path) -> Result<HashSet<String>> {
    let mut out = HashSet::new();
    let mut dirs = vec![root.to_path_buf()];
    while let Some(d) = dirs.pop() {
        for e in std::fs::read_dir(&d)?.flatten() {
            let p = e.path();
            let shared = p.strip_prefix(root).is_ok_and(|rel| {
                rel.components().any(|c| {
                    let n = c.as_os_str().to_string_lossy().to_ascii_lowercase();
                    n.ends_with("_common") || n == "async"
                })
            });
            if p.is_dir() {
                dirs.push(p);
            } else if shared
                && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("fbrb"))
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
    } else if let Some(c) = rec.closure.iter().find(|c| c.name.eq_ignore_ascii_case(ours)) {
        // copied by the closure (round E3d): the bundle it came from
        sources.push((vec![game.root.join(&c.from)], o));
    } else if let Some((src, lv)) = rec.weather.as_ref().and_then(|w| w.source_of(ours)) {
        // round VW2: a weather partition the export copied under the level's folder: its source in its level's
        // bundles
        let mut b = game.loaded(lv, "level");
        b.extend(game.startup());
        sources.push((b, format!("{src}.dbx")));
    } else if rec.announcer.iter().any(|a| a.eq_ignore_ascii_case(ours)) {
        // the donor's own announcer partition (round E3c)
        sources.push((game.level_bundles(&rec.donor, "level"), o));
    } else {
        // a partition copied into the level's bundle: the source it was copied from, the first bundle of the level
        // block (each read alone), else the donor's, that has it (`fill_hidden`'s order)
        let mut from: Vec<PathBuf> = rec.mft_blocks.first().map_or_default(|b| {
            b.iter()
                .filter_map(|l| l.strip_prefix("BundleTarget "))
                .filter(|t| !t.to_ascii_lowercase().contains(&format!("levels/{low}/")))
                .map(|t| game.bundle(t))
                .collect()
        });
        if !rec.donor.is_empty() {
            from.extend(game.level_bundles(&rec.donor, "level"));
        }
        for b in from {
            if game.live(std::slice::from_ref(&b))?.contains_key(&o) {
                sources.push((vec![b], o));
                break;
            }
        }
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

/// A bundle entry that holds streamed wave data (`<sound>_waveDataSns_<n>`, typed `<non-resource>`): never in a level
/// bundle (spec SND-STRM-1: the loader reads it as a partition and the load crashes at 0x49EDF4).
#[must_use]
pub fn is_streamed_wave(entry: &str) -> bool {
    entry.to_ascii_lowercase().contains("_wavedatasns_")
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
    let mut own_entries: Vec<StagedEntry> = Vec::new();
    let mut streamed: Vec<String> = Vec::new();
    for f in &rec.files {
        let p = stage.join(&f.path);
        let bytes = std::fs::read(&p)?;
        r.add(
            sha1_hex(&bytes) == f.sha1,
            format!("{}: the staged file is the one recorded", f.path),
        );
        let (h, data) = fbrb::extract_path_uncached(&p).map_err(|e| err(format!("{}: {e}", p.display())))?;
        let mut bad = Vec::new();
        // round QW2 (audit AU1 RISK-4, spec SND-STRM-1): streamed wave data in a level bundle is read as a partition
        // and the load crashes at 0x49EDF4
        streamed.extend(
            h.entries
                .iter()
                .filter(|e| !e.is_deleted() && is_streamed_wave(&e.name))
                .map(|e| format!("{}: {}", f.path, e.name)),
        );
        if f.path == "level-00.fbrb" {
            own_entries = h
                .entries
                .iter()
                .zip(&data)
                .filter(|(e, _)| !e.is_deleted())
                .map(|(e, d)| StagedEntry {
                    name: e.name.clone(),
                    type_name: e.type_name.clone(),
                    data: d.clone(),
                })
                .collect();
        }
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
    r.add(
        streamed.is_empty(),
        format!(
            "no streamed wave data (_waveDataSns_ entries) in the staged bundles (0x49EDF4){}",
            if streamed.is_empty() {
                String::new()
            } else {
                format!(
                    " (not: {})",
                    streamed.iter().take(5).cloned().collect::<Vec<_>>().join(", ")
                )
            }
        ),
    );
    // the registration: one block and one manifest line per bundle, each block's targets present
    // (the voice-over blocks, lines and bundles after them: the announcer's check, `check_announcer`)
    let is_vo = |l: &str| {
        l.to_ascii_lowercase()
            .starts_with(&format!("bundlesource game:levels/{low}/vo/"))
    };
    let staged: Vec<&str> = rec
        .files
        .iter()
        .map(|f| f.path.as_str())
        .filter(|p| !p.starts_with("vo/"))
        .collect();
    let kinds_ok = rec.mft_blocks.len()
        == KINDS.len()
            + rec
                .mft_blocks
                .iter()
                .filter(|b| b.first().is_some_and(|s| is_vo(s)))
                .count()
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
        && rec.bundle_manifest.get(..KINDS.len()) == Some(bundle_manifest_lines(&low).as_slice())
        && staged == KINDS.iter().map(|k| format!("{k}-00.fbrb")).collect::<Vec<_>>();
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
    check_names(game, &rec, &mut r);
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
    // round E3d: the level's closure resolves under the profile's bundles (what the partitions it reaches through its
    // own and its base's bundles name, too), and its mode layer is complete
    let mut block_loaded: Vec<PathBuf> = level_block
        .iter()
        .filter_map(|l| l.strip_prefix("BundleTarget "))
        .filter(|t| !t.to_ascii_lowercase().contains(&format!("levels/{low}/")))
        .map(|t| game.bundle(t))
        .collect();
    block_loaded.extend(game.startup());
    let mut base_loaded = game.loaded(&rec.base, "level");
    base_loaded.extend(game.startup());
    let cl = closure(game, &block_loaded, &base_loaded, &own_entries, &[], false)?;
    r.add(
        cl.unresolved.is_empty(),
        format!(
            "closure: every partition the level reaches ({} walked in its own and its base's bundles, {} copied in by              the export from {}) resolves in the bundles it loads under the {} profile; {} references to partitions in              no bundle at all (DICE's own) {:?}",
            cl.walked,
            rec.closure.len(),
            {
                let mut f: Vec<&str> = rec.closure.iter().map(|c| c.from.as_str()).collect();
                f.sort_unstable();
                f.dedup();
                if f.is_empty() { "nowhere".to_owned() } else { f.join(", ") }
            },
            rec.profile.word(),
            cl.nowhere.len(),
            cl.unresolved.iter().take(8).map(|(n, by)| format!("{n} (named by {by})")).collect::<Vec<_>>()
        ),
    );
    // round VW2: the weather the export gave the level
    if let (Some(wr), Some((_, lw))) = (&rec.weather, lvl) {
        for (ok, what) in crate::weather::check(wr, &LevelWriter { d: lw.clone() }, &live) {
            r.add(ok, what);
        }
    }
    // the map's ground: its height field is in a bundle the terrain or the level block loads (round E3d: a Vietnam
    // map's is in nam_common's terrain bundle, which its terrain block keeps under any profile)
    if let Some((_, lw)) = lvl
        && let Some(hf) = heightfield_name(&LevelWriter { d: lw.clone() })
    {
        let mut tb = vec![stage.join("terrain-00.fbrb")];
        for b in rec.mft_blocks.iter().take(2) {
            for t in b.iter().filter_map(|l| l.strip_prefix("BundleTarget ")) {
                if !t.to_ascii_lowercase().contains(&format!("levels/{low}/")) {
                    tb.push(game.bundle(t));
                }
            }
        }
        let there = game.live(&tb)?.get(&hf).map(|f| rel_name(game, &f.0));
        r.add(
            there.is_some(),
            format!(
                "terrain: the height field {hf} is in {} (the terrain and level blocks' bundles)",
                there.unwrap_or_else(|| {
                    "NONE of them: the map would load without ground; keep the base's terrain bundles".into()
                })
            ),
        );
    }
    if let Some((_, lw)) = lvl {
        let mode = mode_key(&rec.mode);
        let have = crate::modelayer::signature(&LevelWriter { d: lw.clone() });
        let donor = level_partition(game, &rec.donor)
            .ok()
            .and_then(|b| LevelWriter::parse(&b).ok())
            .map(|w| crate::modelayer::signature(&w));
        match donor {
            Some(d) => {
                let miss = crate::modelayer::missing(&mode, &have, &d);
                r.add(
                    miss.is_empty(),
                    format!(
                        "mode layer: {} {} - {} bases {:?}, {} flags, {} combat areas, counters {:?} (tickets, lives,                          kills), {} teams, {} spawns, M-COMs per stage {:?} (the donor {}'s: bases {:?}, {} flags,                          stages {:?}) {miss:?}",
                        if miss.is_empty() { "complete:" } else { "INCOMPLETE:" },
                        crate::modelayer::mode_name(&mode),
                        have.bases.iter().sum::<usize>(),
                        have.bases,
                        have.flags,
                        have.combat_areas,
                        have.counters,
                        have.teams,
                        have.spawns,
                        have.stages,
                        rec.donor,
                        d.bases,
                        d.flags,
                        d.stages
                    ),
                );
            }
            None => r.add(
                false,
                format!("mode layer: the donor {} is not in the game copy", rec.donor),
            ),
        }
    }
    check_profile(game, stage, &rec, &ours, &live, &mut r)?;
    check_identity(game, stage, &rec, &ours, &mut r)?;
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
    // round XF1: each re-slotted object stands in leaves of its own blueprint's shape, and a new object without a
    // slot is one no level slots
    if let (Some((_, lw)), Some((_, tree))) = (
        lvl,
        ours.iter()
            .find(|(n, _)| n.to_ascii_lowercase().ends_with("staticculltree_exclusive.dbx")),
    ) {
        check_cull_shapes(game, &rec, lw, tree, &mut r)?;
    }
    // round XF1: every mesh the level reaches has entries in its shader databases, except those DICE's base level
    // reaches without them too and those no level's database has
    check_shaders(game, &rec, &own_entries, &live, &mut r)?;
    Ok(r)
}

/// The leaves a slot owns in a tree: from its first leaf to the next slot's (at most two), as [`crate::culltree`]
/// counts them.
fn slot_leaves(t: &crate::culltree::CullTree, slot: u32, n: usize) -> Vec<(crate::culltree::Sphere, f32)> {
    let first = slot.saturating_sub(t.enum_offset);
    (first..first + n as u32).filter_map(|l| t.leaf(l)).collect()
}

/// The check's cull-shape lines (round XF1, XCULL-RES-3).
#[allow(
    clippy::too_many_lines,
    reason = "two check lines: the re-slotted objects' shapes, the new objects' slots"
)]
fn check_cull_shapes(game: &Game, rec: &StageRecord, lw: &EditDbx, tree: &EditDbx, r: &mut CheckReport) -> Result<()> {
    let t = crate::culltree::CullTree::read(tree)?;
    let w = LevelWriter { d: lw.clone() };
    let base_w = LevelWriter::parse(&level_partition(game, &rec.base)?)?;
    let base_tree = EditDbx::parse(&read_one(
        game,
        &game.loaded(&rec.base, "level"),
        &format!("levels/{}/staticculltree_exclusive.dbx", rec.base),
    )?)
    .map_err(|e| err(e.0))?;
    let bt = crate::culltree::CullTree::read(&base_tree)?;
    let (base_own, base_bp) = crate::culltree::shapes(&base_w, &bt, "base");
    let mut others: HashMap<String, HashMap<String, crate::culltree::OwnShape>> = HashMap::new();
    let mut wrong = Vec::new();
    for s in &rec.cull {
        let Some(e) = w.find(&s.guid) else {
            wrong.push(format!("{}: not in the level", s.guid));
            continue;
        };
        let bp = crate::culltree::blueprint_of(&w, e).unwrap_or_default();
        let (label, source_guid) = match s.source.split(' ').collect::<Vec<_>>().as_slice() {
            [l, g, ..] => ((*l).to_owned(), g.to_ascii_uppercase()),
            _ => (String::new(), String::new()),
        };
        let own = if label == "base" {
            base_own.get(&source_guid).cloned()
        } else {
            if !others.contains_key(&label) {
                let read = || -> Result<HashMap<String, crate::culltree::OwnShape>> {
                    let lw2 = LevelWriter::parse(&level_partition(game, &label)?)?;
                    let td = EditDbx::parse(&read_one(
                        game,
                        &game.loaded(&label, "level"),
                        &format!("levels/{label}/staticculltree_exclusive.dbx"),
                    )?)
                    .map_err(|x| err(x.0))?;
                    let tt = crate::culltree::CullTree::read(&td)?;
                    Ok(crate::culltree::shapes(&lw2, &tt, &label).0)
                };
                others.insert(label.clone(), read().unwrap_or_default());
            }
            others.get(&label).and_then(|m| m.get(&source_guid)).cloned()
        };
        let Some((src_bp, _, shape)) = own else {
            wrong.push(format!(
                "{}: its shape's source {} is not a slotted object",
                s.guid, s.source
            ));
            continue;
        };
        if src_bp != bp || s.blueprint != bp {
            wrong.push(format!(
                "{}: a {bp} took the shape of a {src_bp} ({})",
                s.guid, s.source
            ));
            continue;
        }
        let slot = crate::culltree::slot_of(&w, e);
        let Some(rows) = w.transform(&s.guid) else { continue };
        let want = shape.placed(&rows);
        let got = slot_leaves(&t, slot, want.len());
        // the keys hold centres to 1/16 m... a radius rounded up: CULL-SPH-1
        let close = got.len() == want.len()
            && got.iter().zip(&want).all(|(a, b)| {
                let d =
                    a.0.c
                        .iter()
                        .zip(&b.0.c)
                        .map(|(x, y)| (x - y).powi(2))
                        .sum::<f64>()
                        .sqrt();
                d <= 0.5 && (a.0.r - b.0.r).abs() <= 0.5 + 0.05 * b.0.r
            });
        if slot != s.slot || !close {
            wrong.push(format!(
                "{}: slot {slot} does not hold its {bp}'s shape where it stands",
                s.guid
            ));
        }
    }
    r.add(
        wrong.is_empty(),
        format!(
            "cull shapes: each of the {} re-slotted objects stands in its own blueprint's shape{}",
            rec.cull.len(),
            if wrong.is_empty() {
                String::new()
            } else {
                format!(
                    " (not: {})",
                    wrong.iter().take(5).cloned().collect::<Vec<_>>().join("; ")
                )
            }
        ),
    );
    // new objects at slot 0: only those recorded as slotted by no level, and none whose blueprint the base slots
    let in_base: HashSet<String> = base_w.guids().into_iter().collect();
    let recorded: HashMap<&str, &str> = rec.unslotted.iter().map(|(g, b)| (g.as_str(), b.as_str())).collect();
    let mut bad = Vec::new();
    let mut new_objects = 0;
    for e in w.d.of_type(b".ReferenceObjectData") {
        let g = String::from_utf8_lossy(w.d.attr(e, b"guid").unwrap_or_default()).to_ascii_uppercase();
        if in_base.contains(&g) {
            continue;
        }
        new_objects += 1;
        if crate::culltree::slot_of(&w, e) != 0 {
            continue;
        }
        let bp = crate::culltree::blueprint_of(&w, e).unwrap_or_default();
        if !recorded.contains_key(g.as_str()) || base_bp.contains_key(&bp) {
            bad.push(format!("{g} ({bp})"));
        }
    }
    r.add(
        bad.is_empty(),
        format!(
            "cull slots: every one of the {new_objects} objects not in the base has its own slot, or no level slots its \
             blueprint ({} such){}",
            rec.unslotted.len(),
            if bad.is_empty() {
                String::new()
            } else {
                format!(" (at slot 0: {})", bad.iter().take(5).cloned().collect::<Vec<_>>().join(", "))
            }
        ),
    );
    Ok(())
}

/// The check's shader lines (round XF1, SHDB-LVL-2).
fn check_shaders(
    game: &Game,
    rec: &StageRecord,
    own: &[StagedEntry],
    live: &HashMap<String, Found>,
    r: &mut CheckReport,
) -> Result<()> {
    let reached = crate::shaders::reach_staged(own, live)?;
    let base_reach = crate::shaders::stock_reach(game, &rec.base)?;
    for e in own {
        let Some(file) = crate::shaders::database_file(&e.name, &e.type_name) else {
            continue;
        };
        let lacking = crate::shaders::uncovered(&e.data, &file, &reached)?;
        let stock = crate::shaders::stock_uncovered(game, &rec.base, &file, &base_reach)?;
        let nowhere: HashSet<&str> = rec
            .shaders
            .iter()
            .filter(|s| s.database == e.name)
            .flat_map(|s| s.nowhere.iter().map(String::as_str))
            .collect();
        let bad: Vec<&String> = lacking
            .iter()
            .filter(|m| !stock.contains(*m) && !nowhere.contains(m.as_str()))
            .collect();
        let db_ok = {
            let dx = bc2_formats::shaderdb::open::dx_in_name(&file).unwrap_or(10);
            bc2_formats::shaderdb::open::open(&e.data, Some(if dx == 11 { 10 } else { dx }), Some(false), &e.name)
                .map_or_else(|x| vec![x], |o| crate::shaders::check_db(&o.db))
        };
        r.add(
            bad.is_empty() && db_ok.is_empty(),
            format!(
                "shaders: {}: entries for the {} meshes the level reaches but {} (the base's own reach without \
                 entries: {}, in no level's database: {}){}{}",
                e.name,
                reached.len(),
                lacking.len(),
                lacking.iter().filter(|m| stock.contains(*m)).count(),
                lacking.iter().filter(|m| nowhere.contains(m.as_str())).count(),
                if bad.is_empty() {
                    String::new()
                } else {
                    format!(
                        "; NO ENTRY: {}",
                        bad.iter().take(8).map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                    )
                },
                if db_ok.is_empty() {
                    String::new()
                } else {
                    format!("; the database fails its checks: {}", db_ok.join("; "))
                }
            ),
        );
    }
    Ok(())
}

/// The mode of a record's maps.ini key (`Conquest` -> `conquest`, `Rush` -> `goldrush`).
fn mode_key(title: &str) -> String {
    crate::modelayer::mode_key(title)
}

/// A level description's field (a text, or a reference) in a partition holding one.
fn desc_field(d: &EditDbx, f: &[u8]) -> Option<String> {
    let a = d.of_type(b".LevelDescriptionAsset").first().copied()?;
    let c = d.child(a, b"Description")?;
    let x = d.child(c, f)?;
    d.attr(x, b"ref")
        .or_else(|| d.text(x))
        .map(|t| String::from_utf8_lossy(t).into_owned())
}

/// The profile's checks (round E3b): its common bundles in every block, the donor one of the set's levels of the
/// mode, the description's game mode, factions and kit pictures the donor's (each resolving in the bundles the level
/// loads), the profile's level fields the set's.
#[allow(clippy::too_many_lines, reason = "the profile's checks in order, each named")]
fn check_profile(
    game: &Game,
    stage: &Path,
    rec: &StageRecord,
    ours: &[(String, EditDbx)],
    live: &HashMap<String, Found>,
    r: &mut CheckReport,
) -> Result<()> {
    let profiles = crate::profile::scan(game, &mode_key(&rec.mode))?;
    let prof = match profiles.get(rec.profile) {
        Ok(p) => p,
        Err(e) => {
            r.add(false, format!("profile {}: {e}", rec.profile.word()));
            return Ok(());
        }
    };
    let mut missing = Vec::new();
    for (k, block) in KINDS.iter().zip(&rec.mft_blocks) {
        let targets: Vec<String> = block
            .iter()
            .filter_map(|l| l.strip_prefix("BundleTarget "))
            .map(str::to_ascii_lowercase)
            .collect();
        // the commons in their order (a subsequence of the targets)
        let mut at = 0;
        for c in prof.commons(k) {
            match targets.iter().skip(at).position(|t| t.eq_ignore_ascii_case(c)) {
                Some(i) => at += i + 1,
                None => missing.push(format!("{k}: {c}")),
            }
        }
    }
    r.add(
        missing.is_empty(),
        format!(
            "profile {}: every block loads the set's common bundles in their order ({}) {missing:?}",
            prof.profile.word(),
            KINDS
                .iter()
                .map(|k| format!("{k} {:?}", prof.commons(k)))
                .collect::<Vec<_>>()
                .join("; ")
        ),
    );
    let Some(donor) = prof.donor(&rec.donor) else {
        r.add(
            false,
            format!(
                "profile {}: the donor {} is not one of its levels of the mode",
                prof.profile.word(),
                rec.donor
            ),
        );
        return Ok(());
    };
    let desc = ours.iter().find(|(n, _)| n.eq_ignore_ascii_case(&rec.description.1));
    let (mode, f1, f2, kits) = desc.map_or_default(|(_, d)| {
        let kits: Vec<String> = d
            .of_type(b".LevelDescriptionAsset")
            .first()
            .and_then(|&a| d.child(a, b"Description"))
            .and_then(|c| d.child(c, b"KitImages"))
            .map_or_default(|arr| {
                d.children(arr)
                    .unwrap_or_default()
                    .iter()
                    .map(|&k| {
                        let t = |f: &[u8]| {
                            d.child(k, f)
                                .and_then(|x| d.attr(x, b"ref").or_else(|| d.text(x)))
                                .map_or_else(String::new, |v| String::from_utf8_lossy(v).into_owned())
                        };
                        format!("{} {} {}", t(b"Team"), t(b"KitCategory"), t(b"Texture"))
                    })
                    .collect()
            });
        (
            desc_field(d, b"GameMode").unwrap_or_default(),
            desc_field(d, b"FactionTeam1").unwrap_or_default(),
            desc_field(d, b"FactionTeam2").unwrap_or_default(),
            kits,
        )
    });
    let resolves = |reference: &str| {
        reference
            .rsplit_once('/')
            .is_some_and(|(p, _)| live.contains_key(&format!("{}.dbx", p.to_ascii_lowercase())))
    };
    let kit_parts_ok = kits
        .iter()
        .all(|k| k.rsplit(' ').next().is_some_and(|t| t == "null" || resolves(t)));
    r.add(
        mode == donor.game_mode && resolves(&mode),
        format!("kits: the game mode {mode} is the donor's and resolves (its kits and HUD)"),
    );
    r.add(
        [f1.clone(), f2.clone()] == donor.factions,
        format!("kits: the factions {f1} / {f2} are the donor's {}", donor.level),
    );
    r.add(
        kits == donor.kits && kit_parts_ok,
        format!(
            "kits: {} kit pictures, the donor's, each resolving in the bundles the level loads",
            kits.len()
        ),
    );
    let lvl = ours
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(&format!("Levels/{}.dbx", rec.level)));
    let fields = lvl.map_or_default(|(_, d)| crate::profile::level_fields(&LevelWriter { d: d.clone() }));
    let wrong: Vec<&(String, String)> = prof.fields.iter().filter(|(f, v)| fields.get(f) != Some(v)).collect();
    r.add(
        wrong.is_empty(),
        format!(
            "profile {}: the level fields {:?} are the set's {wrong:?}",
            prof.profile.word(),
            prof.fields
        ),
    );
    check_announcer(game, stage, rec, &profiles, donor, lvl, live, r)
}

/// The set of maps a bundle belongs to, as the profiles read it: a level's own bundle its level's set, a common
/// bundle the set with the fewest level commons that loads it (`mp_common` BFBC2's, `nam_common` Vietnam's ...).
fn set_of_bundle(game: &Game, profiles: &crate::profile::Profiles, bundle: &Path) -> Option<Profile> {
    let rel = |name: &str| game.bundle(name) == bundle;
    for s in &profiles.sets {
        if s.levels
            .iter()
            .any(|lv| game.level_bundles(lv, "level").iter().any(|b| b == bundle))
        {
            return Some(s.profile);
        }
    }
    profiles
        .sets
        .iter()
        .filter(|s| s.commons("level").iter().any(|c| rel(c)))
        .min_by_key(|s| s.commons("level").len())
        .map(|s| s.profile)
}

/// The announcer's check (round E3c): the level's voice-over registration is the donor's (its `vo/<lang>` blocks
/// and manifest lines under the new name, its own voice-over bundles byte for byte), and every announcer partition
/// the level names - the trigger sets and line banks, followed through their references - resolves in the bundles
/// the level loads to the same data the donor's own load gives it. Names whose voices the level ends up with: the
/// set of the bundles they come from (the donor's set for the level's own copies).
#[allow(
    clippy::too_many_arguments,
    reason = "the stage, its record, the profiles and what the checks read"
)]
#[allow(clippy::too_many_lines, reason = "the registration, then the references, each named")]
fn check_announcer(
    game: &Game,
    stage: &Path,
    rec: &StageRecord,
    profiles: &crate::profile::Profiles,
    donor: &crate::profile::Donor,
    lvl: Option<&(String, EditDbx)>,
    live: &HashMap<String, Found>,
    r: &mut CheckReport,
) -> Result<()> {
    let low = rec.level.to_ascii_lowercase();
    let a = &donor.announcer;
    // the registration
    let vo_blocks: Vec<&Vec<String>> = rec
        .mft_blocks
        .iter()
        .filter(|b| {
            b.first().is_some_and(|s| {
                s.to_ascii_lowercase()
                    .starts_with(&format!("bundlesource game:levels/{low}/vo/"))
            })
        })
        .collect();
    let want_blocks = a.blocks_for(&donor.level, &low);
    let vo_lines: Vec<&String> = rec.bundle_manifest.iter().skip(KINDS.len()).collect();
    let want_lines: Vec<String> = a.manifest.iter().map(|k| bundle_manifest_line(&low, k)).collect();
    let mut bad_files = Vec::new();
    let vo_files: Vec<&StagedFile> = rec.files.iter().filter(|f| f.path.starts_with("vo/")).collect();
    for (kind, b) in &a.bundles {
        let path = format!("{kind}-00.fbrb");
        let same = vo_files.iter().any(|f| {
            f.path == path
                && f.install == format!("Package/levels/{low}/{path}")
                && std::fs::read(game.bundle(b)).is_ok_and(|d| sha1_hex(&d) == f.sha1)
        });
        if !same {
            bad_files.push(path);
        }
    }
    r.add(
        vo_blocks.iter().copied().eq(want_blocks.iter())
            && vo_lines.iter().copied().eq(want_lines.iter())
            && bad_files.is_empty()
            && vo_files.len() == a.bundles.len(),
        format!(
            "announcer: the voice-over registration is {}'s ({} vo blocks: {}; {} manifest lines; its own bundles {:?} \
             copied byte for byte) {bad_files:?}",
            donor.level,
            want_blocks.len(),
            want_blocks
                .iter()
                .map(|b| b
                    .iter()
                    .skip(1)
                    .map(|t| t.trim_start_matches("BundleTarget "))
                    .collect::<Vec<_>>()
                    .join(" + "))
                .collect::<BTreeSet<_>>()
                .into_iter()
                .collect::<Vec<_>>()
                .join("; "),
            want_lines.len(),
            a.bundles.iter().map(|b| &b.1).collect::<Vec<_>>()
        ),
    );
    // the references: the level's announcer partitions, followed through theirs
    let mut dl = game.loaded(&donor.level, "level");
    dl.extend(game.startup());
    let donor_live = game.live(&dl)?;
    let mut todo: Vec<String> = lvl.map_or_default(|(_, d)| {
        refs_of(d)
            .into_iter()
            .filter_map(|x| x.rsplit_once('/').map(|(p, _)| p.to_ascii_lowercase()))
            .filter(|p| crate::profile::is_announcer(&format!("{p}/")))
            .collect()
    });
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut from: BTreeMap<String, usize> = BTreeMap::new(); // where they come from -> how many
    let mut sets: Vec<Profile> = Vec::new();
    let mut differ = Vec::new();
    let ours_bundle = stage.join("level-00.fbrb");
    while let Some(p) = todo.pop() {
        if !seen.insert(p.clone()) {
            continue;
        }
        let name = format!("{p}.dbx");
        let (Some(mine), Some(theirs)) = (live.get(&name), donor_live.get(&name)) else {
            differ.push(format!(
                "{p} (not in {})",
                if live.contains_key(&name) {
                    &donor.level
                } else {
                    "the level's bundles"
                }
            ));
            continue;
        };
        let data = read_found(&[mine, theirs])?;
        let (Some(a1), Some(a2)) = (data.first(), data.get(1)) else {
            continue;
        };
        if a1 != a2 {
            differ.push(p.clone());
        }
        let (where_, set) = if mine.0 == ours_bundle {
            (
                format!("its own copy of {}'s", donor.level),
                set_of_bundle(game, profiles, &theirs.0),
            )
        } else {
            (
                mine.0
                    .strip_prefix(&game.root)
                    .map_or_else(|_| mine.0.display().to_string(), |x| x.display().to_string()),
                set_of_bundle(game, profiles, &mine.0),
            )
        };
        *from.entry(where_).or_default() += 1;
        match set {
            Some(s) => {
                if !sets.contains(&s) {
                    sets.push(s);
                }
            }
            None => differ.push(format!("{p} (from a bundle of no set's multiplayer levels)")),
        }
        if let Ok(d) = EditDbx::parse(a1) {
            for x in refs_of(&d) {
                if let Some((q, _)) = x.rsplit_once('/') {
                    let q = q.to_ascii_lowercase();
                    if crate::profile::is_announcer(&format!("{q}/")) && !seen.contains(&q) {
                        todo.push(q);
                    }
                }
            }
        }
    }
    // the voices are the set's whose bundles they come from: a set's own data over the shared BFBC2 commons
    let voices = if sets.len() > 1 {
        sets.iter().copied().find(|s| *s != Profile::Bfbc2)
    } else {
        sets.iter().copied().next()
    };
    let one_set = sets.len() <= 1 || (sets.len() == 2 && sets.contains(&Profile::Bfbc2));
    r.add(
        !seen.is_empty() && differ.is_empty() && voices == Some(rec.profile) && one_set,
        format!(
            "announcer: {} voices - {} partitions the level names (its trigger sets and line banks, followed) resolve \
             as on {} ({}) {:?}",
            voices.map_or_else(|| "no set's".to_owned(), |v| format!("{}'s", v.word())),
            seen.len(),
            donor.level,
            from.iter()
                .map(|(k, n)| format!("{n} from {k}"))
                .collect::<Vec<_>>()
                .join(", "),
            differ.iter().take(8).collect::<Vec<_>>()
        ),
    );
    Ok(())
}

/// The stage's names as the Export form had them.
fn stage_names(rec: &StageRecord) -> crate::names::LevelNames {
    let (nk, dk) = loc_keys(&rec.level);
    let text = |l: &[(String, String)], k: &str| l.iter().find(|x| x.0 == k).map_or_default(|x| x.1.clone());
    crate::names::LevelNames {
        level: rec.level.clone(),
        folder: String::new(),
        browser: rec.title.clone(),
        own: !rec.strings.is_empty(),
        name: text(&rec.strings, &nk),
        description: text(&rec.strings, &dk),
        languages: rec
            .strings_by_language
            .iter()
            .map(|(lang, l)| {
                (
                    lang.clone(),
                    crate::names::LangText {
                        name: text(l, &nk),
                        description: text(l, &dk),
                    },
                )
            })
            .collect(),
    }
}

/// The names (round EX1): the rules of [`crate::names::check`] (no clash with a stock level, another project's
/// export, a text or picture the game has; characters and lengths the files hold; no `_NAME` in the id), and every
/// place the level is named agreeing - the start-up checksum crash of a level registered with missing bundles or a
/// new name not registered, and the black loading screen of a `_NAME` prefix that names no picture.
fn check_names(game: &Game, rec: &StageRecord, r: &mut CheckReport) {
    let names = stage_names(rec);
    let project = rec.project.split(" (").next().unwrap_or(&rec.project);
    let problems = crate::names::check(
        &names,
        Some(project),
        &rec.mode,
        Some(&crate::names::Taken::read(game)),
        Some(&crate::names::Languages::read(game)),
    );
    if problems.is_empty() {
        r.add(
            true,
            format!(
                "names: the id {}, its folder, the server-browser name {:?} and the on-screen names follow the rules \
                 (no clash with the game copy or another project's export; characters and lengths the files hold)",
                rec.level, rec.title
            ),
        );
    }
    for p in problems {
        r.add(false, format!("names: {}", p.message));
    }
    let low = rec.level.to_ascii_lowercase();
    let mut wrong: Vec<String> = Vec::new();
    if !rec
        .description
        .1
        .eq_ignore_ascii_case(&format!("Levels/{}_description.dbx", rec.level))
    {
        wrong.push(format!("the description is {}", rec.description.1));
    }
    if rec.level_list != [format!("{low}.dbx"), format!("{low}_description.dbx")] {
        wrong.push(format!("the menus' level list items are {:?}", rec.level_list));
    }
    for b in &rec.mft_blocks {
        for l in b {
            let t = l.to_ascii_lowercase();
            if t.starts_with("bundlesource ") && !t.starts_with(&format!("bundlesource game:levels/{low}/")) {
                wrong.push(format!("package.mft block {l}"));
            }
        }
    }
    for l in &rec.bundle_manifest {
        let own = l.to_ascii_lowercase().starts_with(&format!("levels/{low}/"));
        if !own {
            wrong.push(format!("bundleManifest line {}", l.split('\t').next().unwrap_or(l)));
        }
    }
    if !rec.strings.is_empty() {
        let (nk, _) = loc_keys(&rec.level);
        let key_prefix = nk.find("_NAME").map_or("", |at| nk.get(..at).unwrap_or(""));
        let picture = format!("Awards/images/loadingScreens/{}.res", key_prefix.to_ascii_lowercase());
        if !key_prefix.eq_ignore_ascii_case(&rec.level) {
            wrong.push(format!("the name key {nk} is cut at {key_prefix:?}, not the level id"));
        }
        if !rec.awards.iter().any(|a| a.install.eq_ignore_ascii_case(&picture)) {
            wrong.push(format!("no loading picture {picture} for the name key {nk}"));
        }
    }
    r.add(
        wrong.is_empty(),
        format!(
            "names agree: Levels/{} is named by its description, the menus' list ({low}.dbx, {low}_description.dbx), \
             every package.mft block and bundleManifest line (levels/{low}/), and its name key's loading picture{}",
            rec.level,
            if wrong.is_empty() {
                String::new()
            } else {
                format!(" - not: {}", wrong.join("; "))
            }
        ),
    );
}

/// The level's own name and pictures (round E3b): the description's and the level's loc keys are the staged texts',
/// each text plain ASCII, each picture the one recorded, in its template's form (format, size, levels and entry
/// size: the donor's own picture), and decoding.
fn check_identity(
    game: &Game,
    stage: &Path,
    rec: &StageRecord,
    ours: &[(String, EditDbx)],
    r: &mut CheckReport,
) -> Result<()> {
    if rec.strings.is_empty() && rec.awards.is_empty() {
        r.add(true, "names and pictures: the base level's (no own name chosen)");
        return Ok(());
    }
    let (nk, dk) = loc_keys(&rec.level);
    let keys: Vec<&str> = rec.strings.iter().map(|x| x.0.as_str()).collect();
    // each text in each language's encoding (round EX1: names per language)
    let langs = crate::names::Languages::read(game);
    let all_langs: Vec<String> = if langs.tables.is_empty() {
        vec!["english".into()]
    } else {
        langs.names()
    };
    let texts_ok = all_langs.iter().all(|lang| {
        let own = rec.strings_by_language.get(lang);
        rec.strings.iter().all(|(k, t)| {
            let t = own
                .and_then(|o| o.iter().find(|x| x.0 == *k))
                .map_or(t.as_str(), |x| x.1.as_str());
            t.chars().count() <= 400
                && if langs.tables.is_empty() {
                    loc_text("text", t, 400).is_ok() || t.is_empty()
                } else {
                    langs.encode(lang, t).is_ok()
                }
        })
    }) && rec
        .strings_by_language
        .values()
        .flatten()
        .all(|(k, _)| *k == nk || *k == dk);
    let desc = ours.iter().find(|(n, _)| n.eq_ignore_ascii_case(&rec.description.1));
    let lvl = ours
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case(&format!("Levels/{}.dbx", rec.level)));
    let in_desc = desc.map(|(_, d)| (desc_field(d, b"Name"), desc_field(d, b"Description")));
    let in_level = lvl.and_then(|(_, d)| {
        let ld = d.of_type(b".LevelData").first().copied()?;
        let c = d.child(ld, b"LevelDescription")?;
        let t = |f: &[u8]| {
            d.child(c, f)
                .and_then(|x| d.text(x))
                .map(|v| String::from_utf8_lossy(v).into_owned())
        };
        Some((t(b"Name"), t(b"Description")))
    });
    let want = Some((Some(nk.clone()), Some(dk.clone())));
    r.add(
        keys == [nk.as_str(), dk.as_str()] && texts_ok && in_desc == want && in_level == want,
        format!(
            "names: {nk} and {dk} in the description and the level, their texts stored in every language's encoding{}",
            if rec.strings_by_language.is_empty() {
                String::new()
            } else {
                format!(
                    " ({} with own texts)",
                    rec.strings_by_language.keys().cloned().collect::<Vec<_>>().join(", ")
                )
            }
        ),
    );
    // the templates: the donor's own pictures
    let st = game.startup();
    let dd = read_one(
        game,
        &st,
        &format!("Levels/{}_description.dbx", rec.donor.to_ascii_uppercase()),
    )?;
    let key = description_name(&dd)?;
    let templates = [
        format!(
            "awards/images/loadingscreens/{}.res",
            key.strip_suffix("_NAME").unwrap_or(&key).to_ascii_lowercase()
        ),
        format!("awards/images/levels/{}.res", rec.donor),
    ];
    let entries = picture_entries(&rec.level);
    let od = game.ondemand_awards();
    for (want_entry, tmpl) in entries.iter().zip(&templates) {
        let Some(f) = rec.awards.iter().find(|a| a.install == *want_entry) else {
            r.add(false, format!("picture {want_entry}: not staged"));
            continue;
        };
        let bytes = std::fs::read(stage.join(&f.path))?;
        let t = read_one(game, &od, tmpl)?;
        let (a, b) = (
            crate::awards::tex_info(crate::awards::unwrap(&bytes)),
            crate::awards::tex_info(crate::awards::unwrap(&t)),
        );
        let decodes = crate::awards::decode(crate::awards::unwrap(&bytes)).is_some();
        r.add(
            sha1_hex(&bytes) == f.sha1 && a.is_some() && a == b && bytes.len() == t.len() && decodes,
            format!(
                "picture {want_entry}: the one recorded, decodes, in {tmpl}'s form ({} bytes, {:?})",
                bytes.len(),
                a.map(|i| (i.fmt, i.w, i.h, i.mips))
            ),
        );
    }
    Ok(())
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
    let _scratch = Scratch(work);
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

// ------------------------------------------------------------------------------------------------ overlay and awards

/// A bundle unpacked into `work` (FbrbTool's way), changed by `f` (the folder, the manifest's entry lines), its
/// manifest written again and packed to `out` with the source's layout; `work` removed.
fn repack(source: &Path, work: &Path, out: &Path, f: impl FnOnce(&Path, &mut Vec<String>) -> Result<()>) -> Result<()> {
    if work.exists() {
        std::fs::remove_dir_all(work)?;
    }
    let _scratch = Scratch(work);
    fbrb::unpack_path(source, work).map_err(|e| err(format!("{}: {e}", source.display())))?;
    let man_path = work.join("fbrb.manifest");
    let man = String::from_utf8_lossy(&std::fs::read(&man_path)?).into_owned();
    let mut lines: Vec<String> = man.lines().map(str::to_owned).collect();
    let head = if lines.is_empty() {
        String::new()
    } else {
        lines.remove(0)
    };
    f(work, &mut lines)?;
    std::fs::write(&man_path, format!("{head}\n{}\n", lines.join("\n")))?;
    fbrb::pack_path(work, out)?;
    std::fs::remove_dir_all(work)?;
    Ok(())
}

/// A scratch folder removed when it goes out of scope (round DF1, audit AU4-CQ-10): a bundle that does not unpack,
/// parse or pack no longer leaves its whole unpack behind.
struct Scratch<'a>(&'a Path);

impl Drop for Scratch<'_> {
    fn drop(&mut self) {
        if self.0.exists() {
            let _ = std::fs::remove_dir_all(self.0);
        }
    }
}

/// The manifest line's entry name.
fn line_name(l: &str) -> &str {
    l.split('|').next().unwrap_or_default()
}

/// The overlay bundle (`Dist\win32\overlay-00.fbrb`) with texts in every language's localization
/// (`Localization/bin/language_<lang>_normal.bin`, as OPFOR's WW2 names are): the keys of `remove` taken out, each of
/// `add` (loc key, plain ASCII text) put in - a key already there gets the new text - every other entry and text as
/// it was. Returns the languages written.
///
/// # Errors
/// The source does not unpack, has no localization, a text is not plain ASCII, or the pack fails.
pub fn patch_overlay(
    source: &Path,
    add: &[(String, String)],
    remove: &[String],
    work: &Path,
    out: &Path,
) -> Result<Vec<String>> {
    patch_overlay_langs(source, add, &BTreeMap::new(), remove, work, out)
}

/// [`patch_overlay`] with texts per language (round EX1): `by_language`'s texts replace `add`'s of the same key in
/// their language. Each text is stored in its language's encoding (its histogram's codes: [`bc2_formats::loc::encode`]),
/// so a name may hold the letters that language's texts have.
///
/// # Errors
/// As [`patch_overlay`], or a text a language cannot store.
pub fn patch_overlay_langs(
    source: &Path,
    add: &[(String, String)],
    by_language: &BTreeMap<String, Vec<(String, String)>>,
    remove: &[String],
    work: &Path,
    out: &Path,
) -> Result<Vec<String>> {
    for (k, t) in add.iter().chain(by_language.values().flatten()) {
        if t.chars().count() > 400 || t.contains('\0') {
            return Err(err(format!("{k} must be at most 400 characters, without NUL")));
        }
    }
    let gone: HashSet<u32> = remove.iter().map(|k| bc2_formats::loc::khash_str(k)).collect();
    let mut langs = Vec::new();
    repack(source, work, out, |dir, lines| {
        let names: Vec<String> = lines.iter().map(|l| line_name(l).to_owned()).collect();
        for l in lines.iter_mut() {
            let name = line_name(l).to_owned();
            let low = name.to_ascii_lowercase();
            let Some(lang) = low
                .strip_prefix("localization/bin/language_")
                .and_then(|r| r.strip_suffix("_normal.bin"))
            else {
                continue;
            };
            let path = fbrb::entry_path(dir, &name)?;
            let data = std::fs::read(&path)?;
            let loc = bc2_formats::loc::parse(&data).map_err(|e| err(format!("{name}: {e}")))?;
            // the language's table: its texts' codes for what is not ASCII
            let hist_name = format!("localization/bin/language_{lang}_histogram.bin");
            let table = match names.iter().find(|n| n.eq_ignore_ascii_case(&hist_name)) {
                Some(h) => bc2_formats::loc::histogram(&std::fs::read(fbrb::entry_path(dir, h)?)?).ok(),
                None => None,
            };
            let mut texts: Vec<(String, String)> = add.to_vec();
            for (k, t) in by_language.get(lang).into_iter().flatten() {
                match texts.iter_mut().find(|x| x.0 == *k) {
                    Some(x) => x.1.clone_from(t),
                    None => texts.push((k.clone(), t.clone())),
                }
            }
            let mut changed: Vec<(u32, Vec<u8>)> = Vec::with_capacity(texts.len());
            for (k, t) in &texts {
                let bytes = bc2_formats::loc::encode(t, table.as_deref()).map_err(|c| {
                    err(format!(
                        "{k}: the {lang} text {t:?} holds {c:?}, which that language's texts have no code for"
                    ))
                })?;
                changed.push((bc2_formats::loc::khash_str(k), bytes));
            }
            let name16: [u8; 16] = data
                .get(20..36)
                .and_then(|b| b.try_into().ok())
                .ok_or_else(|| err(format!("{name}: short")))?;
            let offsets: Vec<(u32, u32)> = loc
                .entries
                .iter()
                .filter(|e| !gone.contains(&e.0))
                .map(|e| (e.0, e.1))
                .collect();
            let bytes =
                bc2_formats::loc::write(&name16, data.get(loc.strings..).unwrap_or_default(), &offsets, &changed)
                    .map_err(|e| err(format!("{name}: {e}")))?;
            std::fs::write(&path, &bytes)?;
            let mut f: Vec<String> = l.split('|').map(str::to_owned).collect();
            for k in [3, 4] {
                if let Some(v) = f.get_mut(k) {
                    *v = bytes.len().to_string();
                }
            }
            *l = f.join("|");
            langs.push(lang.to_owned());
        }
        if langs.is_empty() {
            return Err(err(format!(
                "{}: no Localization/bin/language_*_normal.bin",
                source.display()
            )));
        }
        Ok(())
    })?;
    Ok(langs)
}

/// The on-demand awards bundle (`Package\async\ondemand_awards-00.fbrb`) with pictures: the entries of `remove`
/// (any case) taken out, each of `add` (entry name, the entry as the on-demand bundles keep it) put in at the end
/// (type `ITexture`, the level bundles' flags), every other entry as it was. Returns the entries in the bundle.
///
/// # Errors
/// The source does not unpack or the pack fails.
pub fn patch_awards(
    source: &Path,
    add: &[(String, Vec<u8>)],
    remove: &[String],
    work: &Path,
    out: &Path,
) -> Result<usize> {
    let drop: HashSet<String> = remove
        .iter()
        .chain(add.iter().map(|a| &a.0))
        .map(|n| n.to_ascii_lowercase())
        .collect();
    let mut count = 0;
    repack(source, work, out, |dir, lines| {
        let mut keep = Vec::with_capacity(lines.len());
        for l in lines.drain(..) {
            if drop.contains(&line_name(&l).to_ascii_lowercase()) {
                let _ = std::fs::remove_file(fbrb::entry_path(dir, line_name(&l))?);
            } else {
                keep.push(l);
            }
        }
        for (name, bytes) in add {
            let p = fbrb::entry_path(dir, name)?;
            if let Some(d) = p.parent() {
                std::fs::create_dir_all(d)?;
            }
            std::fs::write(&p, bytes)?;
            keep.push(format!("{name}|ITexture|{FLAGS:08x}|{}|{}", bytes.len(), bytes.len()));
        }
        count = keep.len();
        *lines = keep;
        Ok(())
    })?;
    Ok(count)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    /// Round QW2 (audit AU1 RISK-4): the entries `check` refuses in a staged bundle (spec SND-STRM-1).
    #[test]
    fn streamed_waves_named() {
        assert!(is_streamed_wave("sound/weapons/m1_garand_fire_waveDataSns_0"));
        assert!(is_streamed_wave("Sound/VO/x_WAVEDATASNS_12"));
        assert!(!is_streamed_wave("sound/weapons/m1_garand_fire"));
        assert!(!is_streamed_wave("sound/weapons/m1_garand_fire_wave"));
    }

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
            folder: String::new(),
            title: String::new(),
            mode: "conquest".into(),
            layer: LayerSpec {
                donor: "mp_009cq".into(),
                bases: vec![],
                flags: vec![],
                combat_area: String::new(),
            },
            profile: Profile::Bfbc2,
            identity: None,
        };
        assert!(spec.level_id().is_err());
        // an E3 export file (no profile, no identity) reads as BFBC2 with the base's names
        let old = serde_json::to_string(&spec).unwrap();
        assert!(!old.contains("identity"));
        let back = ExportSpec::from_json(&old.replace(",\"profile\":\"BFBC2\"", "")).unwrap();
        assert_eq!(back.profile, Profile::Bfbc2);
        assert_eq!(
            loc_keys("Mapedit_x"),
            ("MAPEDIT_X_NAME".into(), "MAPEDIT_X_DESC".into())
        );
        assert_eq!(
            picture_entries("MAPEDIT_X")[0],
            "Awards/images/loadingScreens/mapedit_x.res"
        );
        assert!(loc_text("t", "Operation Aurora (Conquest)", 64).is_ok());
        assert!(loc_text("t", "Op\u{e9}ration", 64).is_err());
        assert!(loc_text("t", " ", 64).is_err());
        assert_eq!(mode_key("Squad Rush"), "squadrush");
    }

    #[test]
    fn overlay_and_awards_patches() {
        let dir = std::env::temp_dir().join(format!("bc2_levelwrite_shared_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        // a bundle with one language file and a picture
        let mut n16 = [0u8; 16];
        n16[..6].copy_from_slice(b"NORMAL");
        let lang = bc2_formats::loc::write(
            &n16,
            b"",
            &[],
            &[(bc2_formats::loc::khash_str("MP_005_NAME"), b"Atacama".to_vec())],
        )
        .unwrap();
        let items = [
            PackItem {
                name: "Localization/bin/language_english_normal.bin",
                type_name: NONRES,
                flags: FLAGS,
                size2: lang.len() as u32,
                data: &lang,
            },
            PackItem {
                name: "Awards/images/loadingScreens/mp_005.res",
                type_name: "ITexture",
                flags: FLAGS,
                size2: 3,
                data: b"abc",
            },
        ];
        let src = dir.join("src.fbrb");
        std::fs::write(&src, fbrb::pack(&items, &Layout::default()).unwrap()).unwrap();
        let out = dir.join("out.fbrb");
        let add = [("MAPEDIT_X_NAME".to_owned(), "X".to_owned())];
        let langs = patch_overlay(&src, &add, &[], &dir.join("w"), &out).unwrap();
        assert_eq!(langs, ["english"]);
        let (_, data) = fbrb::extract_path_uncached(&out).unwrap();
        let f = bc2_formats::loc::parse(&data[0]).unwrap();
        let text = |k: &str| {
            let h = bc2_formats::loc::khash_str(k);
            f.entries
                .iter()
                .position(|e| e.0 == h)
                .map(|i| f.raw(&data[0], i).to_vec())
        };
        assert_eq!(text("MAPEDIT_X_NAME").as_deref(), Some(&b"X"[..]));
        assert_eq!(text("MP_005_NAME").as_deref(), Some(&b"Atacama"[..]));
        assert_eq!(data[1], b"abc");
        // taken out again: the language file is the source's
        let back = dir.join("back.fbrb");
        patch_overlay(&out, &[], &["MAPEDIT_X_NAME".into()], &dir.join("w"), &back).unwrap();
        let (_, b) = fbrb::extract_path_uncached(&back).unwrap();
        let g = bc2_formats::loc::parse(&b[0]).unwrap();
        assert_eq!(g.entries.len(), 1);
        // pictures in and out
        let pics = dir.join("pics.fbrb");
        let n = patch_awards(
            &src,
            &[("Awards/images/Levels/x.res".into(), b"pic".to_vec())],
            &[],
            &dir.join("w"),
            &pics,
        )
        .unwrap();
        assert_eq!(n, 3);
        let (h, d) = fbrb::extract_path_uncached(&pics).unwrap();
        assert_eq!(h.entries[2].name, "Awards/images/Levels/x.res");
        assert_eq!(d[2], b"pic");
        assert_eq!(d[0], lang);
        let none = dir.join("none.fbrb");
        assert_eq!(
            patch_awards(
                &pics,
                &[],
                &["awards/images/levels/X.res".into()],
                &dir.join("w"),
                &none
            )
            .unwrap(),
            2
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
