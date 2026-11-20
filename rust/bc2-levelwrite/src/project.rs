//! The map project: `<name>.mapedit.json` in a folder of the user's choosing (`EDITOR_ARCHITECTURE.md` 3.1), after
//! `navtune.json`'s pattern - the base (the level, its partition, the partition's sha1, the content profile), the
//! operations each with `by` and `when`, undo and redo as the log plus a redo stack, and the base-changed report
//! (operations whose target is gone are named, never dropped). The file is data only: guids, transforms, values and
//! names; no game content (section 6, ruling 2).
//!
//! Crash recovery: every change is written at once to `<name>.mapedit.json.autosave` beside the project; a clean
//! save removes it. A project opened with an autosave that differs from it offers to recover it
//! ([`MapProject::recover`]).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ops::{Op, Operation, Organisation};

/// The file's `format`.
pub const FORMAT: &str = "bfbc2-mapedit";
/// The file's `version` (this build reads it and older).
pub const VERSION: u32 = 1;
/// A project file's extension.
pub const EXTENSION: &str = ".mapedit.json";
/// The autosave's extension, after the project file's name.
pub const AUTOSAVE: &str = ".autosave";

/// The content profile (3.2, item 5): which assets and kits a map may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub enum Profile {
    /// Battlefield: Bad Company 2.
    #[serde(rename = "BFBC2")]
    #[default]
    Bfbc2,
    /// Vietnam.
    #[serde(rename = "Vietnam")]
    Vietnam,
    /// Pacific War (WW2): WW2 kits and assets only.
    #[serde(rename = "WW2")]
    Ww2,
    /// Bad Company 1's maps (DICE's `bc1_*` levels of `game\Package`, on `mp2_common`).
    #[serde(rename = "BFBC1")]
    Bfbc1,
}

impl Profile {
    /// Every profile, in the editor's order.
    pub const ALL: [Self; 4] = [Self::Bfbc2, Self::Bfbc1, Self::Vietnam, Self::Ww2];

    /// The profile of a level from the bundles it loads, as the editor's start screen recognises it (round G3): a
    /// level block that targets `ww2_common` is WW2, `nam_common` Vietnam; a `bc1_` level with its own bundle in
    /// `game\Package` is BFBC1; everything else BFBC2.
    #[must_use]
    pub fn of_stack(lv: &str, targets: &[String], package: bool) -> Self {
        let has = |c: &str| {
            targets
                .iter()
                .any(|t| t.to_ascii_lowercase().contains(&format!("levels/{c}/")))
        };
        if has("ww2_common") {
            Self::Ww2
        } else if has("nam_common") {
            Self::Vietnam
        } else if package && lv.to_ascii_lowercase().starts_with("bc1_") {
            Self::Bfbc1
        } else {
            Self::Bfbc2
        }
    }

    /// Its name in a sentence.
    #[must_use]
    pub fn word(self) -> &'static str {
        match self {
            Self::Bfbc2 => "BFBC2",
            Self::Bfbc1 => "BFBC1",
            Self::Vietnam => "Vietnam",
            Self::Ww2 => "WW2 (Pacific War)",
        }
    }

    /// The profile of a level by its name (`nam_*`: Vietnam; OPFOR's WW2 levels: WW2; `bc1_*`: BFBC1).
    #[must_use]
    pub fn of_level(lv: &str) -> Self {
        let l = lv.to_ascii_lowercase();
        if l.starts_with("bc1_") {
            Self::Bfbc1
        } else if l.starts_with("nam_") {
            Self::Vietnam
        } else if l.starts_with("ww2_")
            || l.starts_with("pw_")
            || ["wake", "guadal_canal", "coral_sea", "iwo_jima"]
                .iter()
                .any(|w| l.starts_with(w))
        {
            Self::Ww2
        } else {
            Self::Bfbc2
        }
    }

    /// The badge's text.
    #[must_use]
    pub fn badge(self) -> &'static str {
        match self {
            Self::Bfbc2 => "BFBC2",
            Self::Bfbc1 => "BFBC1",
            Self::Vietnam => "VIETNAM",
            Self::Ww2 => "WW2 - PACIFIC WAR",
        }
    }
}

/// The level a project edits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Base {
    /// The level (`mp_005`).
    pub level: String,
    /// The level partition's name in the game (`levels/MP_005`).
    pub partition: String,
    /// The partition's sha1 when the project was made.
    pub sha1: String,
    /// Its size in bytes.
    pub size: u64,
    /// The content profile.
    pub profile: Profile,
    /// What kind of level the base is, from its description's game mode (round G4): a campaign base is stripped of
    /// its campaign parts when exported. Absent in projects made before G4 (and where the mode was not known).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<BaseKind>,
}

/// The kind of a project's base level, decided from its description's `GameMode` (round G4): a campaign mode
/// (`gamemodes/singelplayer...`) or a multiplayer one (Conquest, Rush, Squad Rush, Squad Deathmatch, Onslaught, Air
/// Superiority ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaseKind {
    /// A multiplayer level.
    #[serde(rename = "multiplayer")]
    Multiplayer,
    /// A campaign (singleplayer) level.
    #[serde(rename = "campaign")]
    Campaign,
}

/// A file a project keeps beside itself (round G4: an imported height map, a colour map): its name in the project's
/// folder, where it was taken from, its size and sha1. It is the user's own file, never game content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeptFile {
    /// The copy's file name, in the project's folder.
    pub file: String,
    /// The path it was copied from.
    pub source: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its sha1.
    pub sha1: String,
}

/// How an imported height map's samples are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HeightFormat {
    /// A 16-bit greyscale PNG.
    #[serde(rename = "png16")]
    Png16,
    /// Raw 16-bit samples, row by row, `side` x `side`.
    #[serde(rename = "raw16")]
    Raw16 {
        /// Samples per side.
        side: u32,
        /// Byte order: little-endian (PC) or big-endian.
        little_endian: bool,
    },
}

/// A new level from a height map (round G4, "Create level - from a height map"): the import's settings and the files
/// it keeps. The terrain itself is generated in a later round (E7); until then the project records this only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerrainImport {
    /// The content set the level is made for (`BFBC1`, `BFBC2`, `Vietnam`, `WW2`).
    pub content_set: String,
    /// The height map, copied beside the project.
    pub heightmap: KeptFile,
    /// How its samples are stored.
    pub format: HeightFormat,
    /// Its samples per side as read (1024, 1025, 2048 or 2049).
    pub samples: u32,
    /// Its lowest and highest stored sample.
    pub sample_range: [u16; 2],
    /// The map's size along x and z in metres.
    pub map_size_m: f64,
    /// The height of sample 0 and of sample 65535 in metres.
    pub height_range_m: [f64; 2],
    /// The water's height in metres.
    pub water_level_m: f64,
    /// The optional colour map, copied beside the project.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub colour_map: Option<KeptFile>,
}

/// Why a project file did not load.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProjectError {
    /// The file did not read.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Not JSON, or not a project's.
    #[error("not a map project: {0}")]
    Json(#[from] serde_json::Error),
    /// Another tool's file.
    #[error("not a map project (format {0:?})")]
    Format(String),
    /// Made by a newer editor.
    #[error("made by a newer editor (version {0}; this one reads {VERSION})")]
    Version(u32),
}

/// A map project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapProject {
    /// [`FORMAT`].
    pub format: String,
    /// [`VERSION`].
    pub version: u32,
    /// The project's id (a UUID v5 of its name, level and time made): the namespace of its new guids.
    pub id: String,
    /// The map's name.
    pub name: String,
    /// When it was made.
    pub created: String,
    /// Who made it.
    pub by: String,
    /// The level it edits.
    pub base: Base,
    /// The operations, oldest first.
    pub operations: Vec<Operation>,
    /// Undone operations, the last undone last (redo takes it back).
    #[serde(default)]
    pub redo: Vec<Operation>,
    /// The next operation's number.
    pub next: u64,
    /// A new level from a height map: the import's settings (round G4). Its `base` names no level until the terrain
    /// is generated (E7).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terrain_import: Option<TerrainImport>,
}

/// The name recorded with edits: the PC's name (`navtune.author`).
#[must_use]
pub fn author() -> String {
    std::env::var("COMPUTERNAME")
        .ok()
        .filter(|s| !s.is_empty())
        .or_else(|| std::env::var("HOSTNAME").ok().filter(|s| !s.is_empty()))
        .unwrap_or_else(|| "unknown".into())
}

/// Now, UTC, as ISO 8601 (`2026-11-18T12:34:56Z`).
#[must_use]
pub fn now() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    iso(secs)
}

/// Seconds since 1970 as ISO 8601 UTC (the civil date of the day number, Hinnant's algorithm).
#[must_use]
pub fn iso(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// The sha1 of bytes, in hex.
#[must_use]
pub fn sha1_hex(bytes: &[u8]) -> String {
    sha1_smol::Sha1::from(bytes).digest().to_string()
}

/// What the base-changed check found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BaseReport {
    /// The partition is not the one the project was made on.
    pub changed: bool,
    /// Operations naming an object that is in neither the base nor made by an earlier operation: (operation number,
    /// its name, the guid).
    pub missing: Vec<(u64, String, String)>,
}

impl MapProject {
    /// A new project on a level partition (`base` its bytes).
    #[must_use]
    pub fn new(name: &str, level: &str, partition: &str, base: &[u8], profile: Profile) -> Self {
        let created = now();
        let by = author();
        let id = bc2_stockmp::guid::new_guid(crate::NAMESPACE, &["project", name, level, &created, &by]);
        Self {
            format: FORMAT.into(),
            version: VERSION,
            id,
            name: name.to_owned(),
            created,
            by,
            base: Base {
                level: level.to_ascii_lowercase(),
                partition: partition.to_owned(),
                sha1: sha1_hex(base),
                size: base.len() as u64,
                profile,
                kind: None,
            },
            operations: Vec::new(),
            redo: Vec::new(),
            next: 1,
            terrain_import: None,
        }
    }

    /// A new level from a height map (round G4): no base level yet, the import's settings recorded.
    #[must_use]
    pub fn new_import(name: &str, profile: Profile, import: TerrainImport) -> Self {
        let mut p = Self::new(name, "", "", &[], profile);
        p.base.sha1 = String::new();
        p.terrain_import = Some(import);
        p
    }

    /// Whether this is a height-map import waiting for terrain generation (no base level to open).
    #[must_use]
    pub fn is_import(&self) -> bool {
        self.terrain_import.is_some() && self.base.level.is_empty()
    }

    /// The project file's name: the map's name with only letters, digits, `-`, `_` and spaces kept.
    #[must_use]
    pub fn file_name(&self) -> String {
        let mut s: String = self
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || matches!(c, '-' | '_' | ' ') {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        s = s.trim().to_owned();
        if s.is_empty() {
            s = "map".into();
        }
        format!("{s}{EXTENSION}")
    }

    /// The number the next operation will get (its new guids are made from it before it is added).
    #[must_use]
    pub fn next_n(&self) -> u64 {
        self.next
    }

    /// A new entity's guid for the next operation ([`crate::new_guid`]).
    #[must_use]
    pub fn new_guid(&self, source: &str) -> String {
        crate::new_guid(&self.id, self.next, source)
    }

    /// Adds an operation (the redo stack is cleared, as in any editor); its number.
    pub fn push(&mut self, op: Op, label: &str) -> u64 {
        self.redo.clear();
        let n = self.next;
        self.operations.push(Operation {
            n,
            by: author(),
            when: now(),
            label: label.to_owned(),
            op,
        });
        self.next += 1;
        n
    }

    /// Replaces the last operation's change with `op` (its number, author and label kept, its time now): a gesture
    /// that goes on (a wheel burst, a slider drag) stays one operation, one undo step. `false` (nothing changed) when
    /// the log is empty or its last operation is not number `n`.
    pub fn amend_last(&mut self, n: u64, op: Op) -> bool {
        match self.operations.last_mut() {
            Some(o) if o.n == n => {
                o.op = op;
                o.when = now();
                self.redo.clear();
                true
            }
            _ => false,
        }
    }

    /// Undoes the last operation; `None` when there is none.
    pub fn undo(&mut self) -> Option<&Operation> {
        let o = self.operations.pop()?;
        self.redo.push(o);
        self.redo.last()
    }

    /// Redoes the last undone operation; `None` when there is none.
    pub fn redo(&mut self) -> Option<&Operation> {
        let o = self.redo.pop()?;
        self.operations.push(o);
        self.operations.last()
    }

    /// Undoes or redoes until `applied` operations are in the log (the History panel's "go to this point").
    pub fn go_to(&mut self, applied: usize) {
        while self.operations.len() > applied && self.undo().is_some() {}
        while self.operations.len() < applied && self.redo().is_some() {}
    }

    /// The organisation the log leaves (layers, folders, groups, placements, deletions).
    #[must_use]
    pub fn organisation(&self) -> Organisation {
        Organisation::of(&self.operations)
    }

    /// The campaign content an export keeps, as the log's last choice left it (round E3b; DICE's choice, all
    /// removed, until one is made).
    #[must_use]
    pub fn strip(&self) -> crate::mpconvert::Strip {
        self.strip_chosen().unwrap_or_default()
    }

    /// The log's last campaign-content choice, `None` when none was made (the editor offers the checklist then).
    #[must_use]
    pub fn strip_chosen(&self) -> Option<crate::mpconvert::Strip> {
        self.operations.iter().rev().find_map(|o| match &o.op {
            Op::SetStrip { to, .. } => Some(to.clone()),
            _ => None,
        })
    }

    /// The project's target (round E3d): the log's last era and mode choice, `None` before one is made (an export
    /// then grafts the export file's layer, as E3 did).
    #[must_use]
    pub fn mode(&self) -> Option<&crate::modelayer::ModeChoice> {
        self.operations.iter().rev().find_map(|o| match &o.op {
            Op::SetMode { to } => Some(to.as_ref()),
            _ => None,
        })
    }

    /// The era the project is made for: its target's, else the base level's.
    #[must_use]
    pub fn target_profile(&self) -> Profile {
        self.mode().map_or(self.base.profile, |m| m.profile)
    }

    /// The base-changed check on the partition's current bytes: whether it is another file, and which operations
    /// name objects it does not have.
    #[must_use]
    pub fn base_report(&self, bytes: &[u8]) -> BaseReport {
        let mut out = BaseReport {
            changed: sha1_hex(bytes) != self.base.sha1,
            missing: Vec::new(),
        };
        let mut known: std::collections::HashSet<String> = crate::write::LevelWriter::parse(bytes).map_or_else(
            |_| std::collections::HashSet::new(),
            |w| w.guids().into_iter().collect(),
        );
        for o in &self.operations {
            for t in o.op.targets() {
                if !known.contains(&t.to_ascii_uppercase()) {
                    out.missing.push((o.n, o.name(), t.to_ascii_uppercase()));
                }
            }
            for c in o.op.creates() {
                known.insert(c.to_ascii_uppercase());
            }
            let gone = match &o.op {
                Op::Delete { guids } => guids.as_slice(),
                Op::SetMode { to } => to.removed.as_slice(),
                _ => &[],
            };
            for g in gone {
                known.remove(&g.to_ascii_uppercase());
            }
        }
        out
    }

    /// The project as its file holds it.
    ///
    /// # Errors
    /// Never in practice (`serde_json` on these types).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut s = serde_json::to_string_pretty(self)?;
        s.push('\n');
        Ok(s)
    }

    /// A project from its file's text.
    ///
    /// # Errors
    /// Not JSON, another format, or a newer version.
    pub fn from_json(text: &str) -> Result<Self, ProjectError> {
        let v: serde_json::Value = serde_json::from_str(text)?;
        let format = v.get("format").and_then(|f| f.as_str()).unwrap_or_default();
        if format != FORMAT {
            return Err(ProjectError::Format(format.to_owned()));
        }
        let version = v.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
        if version > VERSION {
            return Err(ProjectError::Version(version));
        }
        Ok(serde_json::from_value(v)?)
    }

    /// Saves the project to `path` (whole or not at all: a temporary file replaced), and removes its autosave.
    ///
    /// # Errors
    /// The folder cannot be made or the file written.
    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        write_whole(path, &self.to_json()?)?;
        let _ = std::fs::remove_file(autosave_path(path));
        Ok(())
    }

    /// Loads a project file.
    ///
    /// # Errors
    /// The file does not read, or is not a project this editor reads.
    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Writes the autosave beside the project file (after every change).
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn autosave(&self, path: &Path) -> Result<(), ProjectError> {
        write_whole(&autosave_path(path), &self.to_json()?)?;
        Ok(())
    }

    /// The autosave of the project at `path`, when there is one that reads and differs from the saved project (the
    /// editor stopped before a save): what to offer to recover.
    #[must_use]
    pub fn recover(path: &Path) -> Option<Self> {
        let auto = Self::load(&autosave_path(path)).ok()?;
        match Self::load(path) {
            Ok(saved) if saved == auto => None,
            _ => Some(auto),
        }
    }

    /// Removes the autosave (the user chose the saved project over it).
    pub fn discard_autosave(path: &Path) {
        let _ = std::fs::remove_file(autosave_path(path));
    }
}

/// The autosave of a project file.
#[must_use]
pub fn autosave_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(AUTOSAVE);
    PathBuf::from(s)
}

fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(format!(".tmp-{}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::assert_is_empty,
    clippy::doc_markdown
)]
mod tests {
    use super::*;
    use crate::ops::{Duplicated, Moved};

    #[test]
    fn dates() {
        assert_eq!(iso(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(iso(1_795_000_000), "2026-11-18T11:06:40Z");
        assert_eq!(Profile::of_level("NAM_MP_002"), Profile::Vietnam);
        assert_eq!(Profile::of_level("guadal_canal"), Profile::Ww2);
        assert_eq!(Profile::of_level("mp_005"), Profile::Bfbc2);
        assert_eq!(Profile::of_level("bc1_oasis_cq"), Profile::Bfbc1);
        let t = |v: &[&str]| v.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let nam = t(&[
            "package:levels/nam_common/level-00.fbrb",
            "game:levels/mp_common/level-00.fbrb",
        ]);
        assert_eq!(Profile::of_stack("nam_mp_002cq", &nam, true), Profile::Vietnam);
        let ww2 = t(&["package:levels/ww2_common/level-00.fbrb"]);
        assert_eq!(Profile::of_stack("my_map", &ww2, true), Profile::Ww2);
        let mp2 = t(&["package:levels/mp2_common/level-00.fbrb"]);
        assert_eq!(Profile::of_stack("bc1_oasis_cq", &mp2, true), Profile::Bfbc1);
        assert_eq!(Profile::of_stack("bc1_oasis_cq", &mp2, false), Profile::Bfbc2);
        assert_eq!(Profile::of_stack("mp_sp_005cq", &mp2, true), Profile::Bfbc2);
    }

    #[test]
    fn base_kind_and_terrain_import() {
        // a project made before G4 reads (no kind, no import) and writes the same keys back
        let mut p = MapProject::new("Old", "mp_005", "levels/MP_005", b"x", Profile::Bfbc2);
        let old = p.to_json().unwrap();
        assert!(!old.contains("kind") && !old.contains("terrain_import"));
        assert_eq!(MapProject::from_json(&old).unwrap(), p);
        p.base.kind = Some(BaseKind::Campaign);
        let text = p.to_json().unwrap();
        assert!(text.contains("\"kind\": \"campaign\""));
        assert_eq!(
            MapProject::from_json(&text).unwrap().base.kind,
            Some(BaseKind::Campaign)
        );
        assert!(!p.is_import());
        let import = TerrainImport {
            content_set: "WW2".into(),
            heightmap: KeptFile {
                file: "Atoll.heightmap.raw".into(),
                source: "C:/maps/atoll.raw".into(),
                size: 2 * 1025 * 1025,
                sha1: "ab".into(),
            },
            format: HeightFormat::Raw16 {
                side: 1025,
                little_endian: true,
            },
            samples: 1025,
            sample_range: [3, 60_000],
            map_size_m: 2048.0,
            height_range_m: [-20.0, 300.0],
            water_level_m: 0.0,
            colour_map: None,
        };
        let q = MapProject::new_import("Atoll", Profile::Ww2, import.clone());
        assert!(q.is_import() && q.base.level.is_empty() && q.operations.is_empty());
        let text = q.to_json().unwrap();
        assert!(text.contains("\"type\": \"raw16\"") && text.contains("\"little_endian\": true"));
        let back = MapProject::from_json(&text).unwrap();
        assert_eq!(back.terrain_import, Some(import));
        assert_eq!(back, q);
    }

    #[test]
    fn log_undo_redo_and_files() {
        let base = crate::write::tests::level();
        let mut p = MapProject::new("Harvest Day: north", "test", "levels/TEST", &base, Profile::Bfbc2);
        assert_eq!(p.file_name(), "Harvest Day_ north.mapedit.json");
        let a = "AAAAAAAA-0000-0000-0000-000000000001";
        let mv = |x: f32| Op::Move {
            items: vec![Moved {
                guid: a.into(),
                from: crate::rows_at([10.0, 2.0, 3.0]),
                to: crate::rows_at([x, 2.0, 3.0]),
                placement: None,
            }],
        };
        p.push(mv(11.0), "");
        let g = p.new_guid(a);
        p.push(
            Op::Duplicate {
                items: vec![Duplicated {
                    source: a.into(),
                    guid: g.clone(),
                    transform: crate::rows_at([1.0, 2.0, 3.0]),
                    owned: Vec::new(),
                }],
            },
            "",
        );
        p.push(Op::Delete { guids: vec![g.clone()] }, "");
        assert_eq!(p.operations.iter().map(|o| o.n).collect::<Vec<_>>(), [1, 2, 3]);
        // undo, redo, redo after undo, a new operation clears the redo stack
        assert_eq!(p.undo().unwrap().n, 3);
        assert_eq!(p.undo().unwrap().n, 2);
        assert_eq!(p.redo().unwrap().n, 2);
        assert_eq!(p.operations.len(), 2);
        assert_eq!(p.redo.len(), 1);
        let dir = std::env::temp_dir().join(format!("bc2_levelwrite_project_{}", std::process::id()));
        let path = dir.join(p.file_name());
        p.save(&path).unwrap();
        let back = MapProject::load(&path).unwrap();
        assert_eq!(back, p);
        let mut q = back.clone();
        assert_eq!(q.redo().unwrap().n, 3);
        assert!(q.redo().is_none());
        q.go_to(0);
        assert!(q.operations.is_empty() && q.redo.len() == 3);
        q.go_to(3);
        assert_eq!(q.operations.len(), 3);
        q.undo();
        q.push(mv(12.0), "Nudge");
        assert!(q.redo.is_empty());
        assert_eq!(q.operations.last().unwrap().n, 4);
        assert_eq!(q.operations.last().unwrap().name(), "Nudge (Move 1 object 2.00 m)");
        // crash recovery: an autosave that differs from the saved file is offered, a save removes it
        assert!(MapProject::recover(&path).is_none());
        q.autosave(&path).unwrap();
        assert_eq!(MapProject::recover(&path).unwrap(), q);
        q.save(&path).unwrap();
        assert!(!autosave_path(&path).exists());
        assert!(MapProject::recover(&path).is_none());
        // base changed: the base itself is fine; another partition without A names the operations
        let r = q.base_report(&base);
        assert!(!r.changed && r.missing.is_empty(), "{r:?}");
        let mut w = crate::write::LevelWriter::parse(&base).unwrap();
        w.delete(a).unwrap();
        let r = q.base_report(&w.write().unwrap());
        assert!(r.changed);
        assert_eq!(r.missing.iter().map(|m| m.0).collect::<Vec<_>>(), [1, 2, 4]);
        // another format or a newer version is refused
        assert!(matches!(
            MapProject::from_json("{\"format\":\"x\"}"),
            Err(ProjectError::Format(_))
        ));
        let newer = q.to_json().unwrap().replace("\"version\": 1", "\"version\": 9");
        assert!(matches!(MapProject::from_json(&newer), Err(ProjectError::Version(9))));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
