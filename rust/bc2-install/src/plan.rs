//! The install plan: what OPFOR's build staged and decided, as data (`bfbc2-install-plan/1`, written by
//! `ww2_level_install.install_plan`). Paths are absolute, spelled as OPFOR spells them (report lines and sorting
//! follow the spelling); ordered maps are lists of pairs.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::marks::MarksSpec;
use crate::text::MapsLevel;

/// The plan's format tag.
pub const FORMAT: &str = "bfbc2-install-plan/1";

/// One level of the install set.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Level {
    /// The level folder's name (`wake_island_s`).
    pub lv: String,
    /// Its name (`WAKE_ISLAND_S`).
    pub name: String,
    /// The game copy's folder of its bundles (`<game>\Package\levels\<lv>`).
    pub pkg: String,
    /// Its install record (`<pkg>\opfor-<stage>-installed.json`: the sha256 of each bundle installed).
    pub record: String,
    /// Its staging folder (`work\ww2\install[_<lv>]`: report.txt).
    pub work: String,
    /// Its bundles' kinds, in the registration's order.
    pub kinds: Vec<String>,
    /// (kind, staged bundle) in the kinds' order; empty in a revert plan.
    #[serde(default)]
    pub built: Vec<(String, String)>,
    /// Its `package.mft` blocks (CRLF inside a block).
    #[serde(default)]
    pub mft_blocks: Vec<String>,
    /// The build's report lines (the model-group and icon gates' last).
    #[serde(default)]
    pub report: Vec<String>,
    /// Both gates passed (the model groups fit their members; every vehicle has its icon).
    #[serde(default)]
    pub ok: bool,
}

/// A level folder an earlier install left that the set no longer has (the revert takes it out too).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Other {
    /// The folder.
    pub pkg: String,
    /// Its install record.
    pub record: String,
}

/// The WW2 common bundles (`Package\levels\ww2_common`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Common {
    /// The game copy's folder.
    pub folder: String,
    /// Its install record.
    pub record: String,
    /// (kind, staged bundle); empty in a revert plan.
    #[serde(default)]
    pub built: Vec<(String, String)>,
}

/// A marker the core DLL reads (`<bundle>.opfor-stings`, `.opfor-tips`): written ("<size> <FILETIME>") when the
/// installed bundle holds every name, else removed.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Marker {
    /// The marker file.
    pub marker: String,
    /// The installed bundle it vouches for.
    pub bundle: String,
    /// The entry names (lower case) the bundle must hold.
    pub names: Vec<String>,
    /// The line when written.
    pub yes: String,
    /// The line when not (or removed).
    pub no: String,
    /// `names` empty counts as not held (the tips: a producer that wrote nothing).
    #[serde(default)]
    pub need_names: bool,
}

/// A shared bundle patched for the install (built in the staging, put in place through the ledger).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Shared {
    /// The game file.
    pub file: String,
    /// The built bundle.
    pub built: String,
}

/// `opfor.ini`'s keys the install sets.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Ini {
    /// The file.
    pub file: String,
    /// `[dev] level` (`Levels/WAKE_ISLAND_S`).
    pub dev_level: String,
    /// `[gamemode] pacific_levels`.
    pub pacific_levels: String,
}

/// `maps.ini`'s sections of the installed levels.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Maps {
    /// The file (the report says when the game copy has none).
    pub file: String,
    /// The marker line before the sections.
    pub mark: String,
    /// Every section name of the install's (lower case): out first.
    pub strip: Vec<String>,
    /// The sections, in order.
    pub levels: Vec<MapsLevel>,
}

/// What `verify` checks after an apply (`wake_install.verify`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Verify {
    /// The startup bundle.
    pub startup: String,
    /// The on-demand awards bundle.
    pub ondemand: String,
    /// The overlay bundle.
    pub overlay: String,
    /// The pictures' entry folder (`Awards/images/weapons/WW2`).
    pub pictures: String,
    /// The level list's entry (`LevelListQuery.dbx`).
    pub level_list: String,
    /// (field, value) the description must hold.
    pub description_fields: Vec<(String, String)>,
}

/// The registration text of a level (`registration_all.txt`).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct RegLevel {
    /// The level folder's name.
    pub lv: String,
    /// Its title.
    pub title: String,
    /// Its bundles' kinds.
    pub kinds: Vec<String>,
    /// Its `package.mft` blocks.
    pub mft_blocks: Vec<String>,
}

/// What `--apply-staged` checks: the input fingerprint's walk.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Staged {
    /// `work\ww2\staged.json`.
    pub path: String,
    /// `work\ww2` (walked, less `not_input` at its top).
    pub w: String,
    /// The top names of `w` that are no inputs (Python's `re.I` match).
    pub not_input: String,
    /// The code files the plan ran.
    pub code: Vec<String>,
}

/// The install plan.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Plan {
    /// [`FORMAT`].
    pub format: String,
    /// The game copy.
    pub game: String,
    /// OPFOR's checkout (log lines name folders relative to it).
    pub opfor: String,
    /// The ledger owner of the shared files (`wake`).
    pub owner: String,
    /// The owner of the bundleManifest repair a revert keeps (`manifestfix`).
    pub fix_owner: String,
    /// The full set: a good plan is staged for `--apply-staged`.
    pub full: bool,
    /// The set, in order.
    pub levels: Vec<Level>,
    /// Every installed level's registration (`registration_all.txt`; the text files the set's registration writes).
    pub registration: Vec<RegLevel>,
    /// `work\ww2\install\registration_all.txt`.
    pub registration_all: String,
    /// The WW2 common bundles.
    pub common: Common,
    /// The DLL's markers.
    #[serde(default)]
    pub markers: Vec<Marker>,
    /// The shared bundles' patches.
    #[serde(default)]
    pub shared: Vec<Shared>,
    /// The guard's files that are always written (the shared bundles, also in a revert plan).
    pub shared_files: Vec<String>,
    /// The build's report on the shared patches.
    #[serde(default)]
    pub shared_report: Vec<String>,
    /// `opfor.ini`.
    pub ini: Ini,
    /// `maps.ini`.
    pub maps: Maps,
    /// `Dist\win32\package.mft`.
    pub mft: String,
    /// `Dist\win32\bundleManifest`.
    pub bm: String,
    /// The guard's marks.
    pub marks: MarksSpec,
    /// The checks after an apply.
    pub verify: Verify,
    /// The staged record.
    pub staged: Staged,
    /// Level folders of earlier installs no longer in the set.
    #[serde(default)]
    pub others: Vec<Other>,
    /// The rebuild lock (`work\ww2\_rebuild\rebuild.lock`).
    pub lock: String,
}

impl Plan {
    /// Reads a plan file.
    ///
    /// # Errors
    /// Not readable, not JSON, or not a plan.
    pub fn read(path: &Path) -> Result<Self, String> {
        let d = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let p: Self =
            serde_json::from_slice(&d).map_err(|e| format!("{}: not an install plan ({e})", path.display()))?;
        if p.format != FORMAT {
            return Err(format!("{}: format {:?}, not {FORMAT}", path.display(), p.format));
        }
        Ok(p)
    }
}
