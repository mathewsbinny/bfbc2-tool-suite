//! The map project: `<name>.mapedit.json` in a folder of the user's choosing (`EDITOR_ARCHITECTURE.md` 3.1), after
//! `navtune.json`'s pattern - the base (the level, its partition, the partition's sha1, the content profile), the
//! operations each with `by` and `when`, undo and redo as the log plus a redo stack, and the base-changed report
//! (operations whose target is gone are named, never dropped). The file is data only: guids, transforms, values and
//! names; no game content (section 6, ruling 2).
//!
//! Crash recovery: every change is written at once to `<name>.mapedit.json.autosave` beside the project, since round
//! OP1 as an append-only journal of the changes ([`crate::journal`]); a clean save removes it. A project opened with
//! an autosave that differs from it offers to recover it ([`MapProject::recover`]; an autosave of the whole project,
//! as editors before OP1 wrote it, still reads).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::ops::{Op, Operation, Organisation};

/// The file's `format`.
pub const FORMAT: &str = "bfbc2-mapedit";
/// The file's `version` (this build reads it and older; older files are migrated as they load: [`migrate`]).
///
/// - 1: rounds E2 - E3d (objects, layers, folders, groups, the export's strip and mode choices);
/// - 2: round GR2, the navigation edits (`nav-add`, `nav-remove`, `nav-load`); nothing else changed, so a version-1
///   file loads as it is;
/// - 3: round EX1, the exported level's names (`set-names`); a version-2 file loads as it is;
/// - 4: round VW2, the exported level's weather (`set-weather`); a version-3 file loads as it is;
/// - 5: round XF1, the typed gameplay operations (`set-team`, `link-spawner`, `set-tickets`, `insert-point`,
///   `remove-point`); a version-4 file loads as it is.
/// - 6: round OP1, operations on any file of the level: the multi-file operation (`files`, [`crate::files`]), the
///   brush strokes stored as delta tiles beside the project (`stroke`, [`crate::delta`]), the base files' sha1s
///   (`base.files`) and the lowest version that reads the file (`min_reader`). A version-5 file is migrated: its
///   `base` gets an empty `files`.
pub const VERSION: u32 = 6;

/// The lowest version that reads an operation of each kind (`op`), the kinds of version 1 left out: what a file's
/// `min_reader` is made from ([`MapProject::reader_needed`]).
pub const OP_VERSIONS: [(&str, u32); 12] = [
    ("nav-add", 2),
    ("nav-remove", 2),
    ("nav-load", 2),
    ("set-names", 3),
    ("set-weather", 4),
    ("set-team", 5),
    ("link-spawner", 5),
    ("set-tickets", 5),
    ("insert-point", 5),
    ("remove-point", 5),
    ("files", 6),
    ("stroke", 6),
];

/// A project file's JSON brought from its `version` to [`VERSION`], one step at a time (round GR2; round OP1 made
/// the first step that changes the file).
#[must_use]
pub fn migrate(mut v: serde_json::Value, from: u32) -> serde_json::Value {
    let mut at = from.max(1);
    while at < VERSION {
        // 1 -> 2: the navigation operations were added; a version-1 log has none
        // 2 -> 3: the names operation was added (round EX1); a version-2 log has none
        // 3 -> 4: the weather operation was added (round VW2); a version-3 log has none
        // 4 -> 5: the typed gameplay operations were added (round XF1); a version-4 log has none
        // 5 -> 6: the base records its other files' sha1s (round OP1); a version-5 project edited none
        if at == 5
            && let Some(b) = v.get_mut("base").and_then(serde_json::Value::as_object_mut)
        {
            b.entry("files").or_insert_with(|| serde_json::json!({}));
        }
        at += 1;
    }
    if let Some(o) = v.as_object_mut() {
        o.insert("version".to_owned(), serde_json::Value::from(VERSION));
    }
    v
}
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
    /// The level's other files the log edits (round OP1: static group files, height fields), each as it was when
    /// first edited, by [`crate::files::Target::base_key`]: the base-changed check of each.
    #[serde(default)]
    pub files: std::collections::BTreeMap<String, BaseFile>,
}

/// A base file besides the partition, as it was when the project first edited it (round OP1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseFile {
    /// Its sha1.
    pub sha1: String,
    /// Its size in bytes.
    pub size: u64,
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
    #[error("made by a newer editor version (project format {0}); this editor reads up to {VERSION}")]
    Version(u32),
    /// The file's `version` is missing or not a version number (round OP1, audit AU4 BUG-13: never read as 0 or
    /// cut to 32 bits).
    #[error("not a map project this editor reads: its version is {0}")]
    BadVersion(String),
    /// An operation of a kind this editor does not know (a newer editor's, or a damaged file).
    #[error(
        "operation {index} of the log is of a kind this editor does not know ({op:?}); it was made by a newer editor or the file is damaged"
    )]
    UnknownOperation {
        /// Its place in the log.
        index: usize,
        /// Its kind (`op`).
        op: String,
    },
    /// The file changed on disk after this editor read or last wrote it (round QW2, audit AU1 R4): not saved.
    #[error(
        "{0} was changed on disk after this editor opened or last saved it (another editor or tool wrote it), so it was not saved and neither version is lost. Your changes are in the project's autosave: reopen the project to compare, or close the other editor and save again."
    )]
    ChangedOnDisk(String),
    /// Another editor holds the project's lock file (round QW2, audit AU1 R4).
    #[error(
        "{path} is already open in another editor ({by}). Close it there first. If no editor has it open (the PC restarted or the editor stopped), delete {lock} and open it again."
    )]
    Locked {
        /// The project file.
        path: String,
        /// Who holds it, in words (the PC, the process, since when).
        by: String,
        /// The lock file.
        lock: String,
    },
}

/// The lock file's extension, after the project file's name (round QW2).
pub const LOCK: &str = ".lock";

/// The lock file of a project file.
#[must_use]
pub fn lock_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(LOCK);
    PathBuf::from(s)
}

/// What a save checks before it writes (round QW2): the project file's modified time and length as this editor last
/// read or wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiskStamp {
    modified: Option<std::time::SystemTime>,
    len: u64,
}

/// The project file's stamp now (`None`: no file).
#[must_use]
pub fn disk_stamp(path: &Path) -> Option<DiskStamp> {
    let m = std::fs::metadata(path).ok()?;
    Some(DiskStamp {
        modified: m.modified().ok(),
        len: m.len(),
    })
}

/// A project held open by this editor (round QW2, audit AU1 R4 / G-N17): `<project>.lock` beside the file, naming the
/// PC, the process and the time; removed when the lock is dropped (if it is still this lock's). A second editor
/// refuses the project while the holder runs; a lock left by a process that is gone (on this PC) is taken over.
#[derive(Debug)]
pub struct ProjectLock {
    path: PathBuf,
    token: String,
}

#[derive(Serialize, Deserialize)]
struct LockFile {
    pc: String,
    pid: i64,
    since: String,
    token: String,
}

impl ProjectLock {
    /// Takes the lock of the project at `project` (the project file need not exist yet).
    ///
    /// # Errors
    /// [`ProjectError::Locked`] while another running editor holds it; [`ProjectError::Io`] when the lock file
    /// cannot be written.
    pub fn acquire(project: &Path) -> Result<Self, ProjectError> {
        let path = lock_path(project);
        if let Some(by) = Self::held_by(project) {
            return Err(ProjectError::Locked {
                path: project.display().to_string(),
                by,
                lock: path.display().to_string(),
            });
        }
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let pid = std::process::id();
        let token = format!("{pid}-{nanos}");
        let file = LockFile {
            pc: author(),
            pid: i64::from(pid),
            since: now(),
            token: token.clone(),
        };
        write_whole(&path, &serde_json::to_string(&file)?)?;
        Ok(Self { path, token })
    }

    /// Who holds the project's lock, in words, when another running editor does (`None`: free, a lock this
    /// process holds, a lock whose process is gone on this PC, or a lock file that does not read).
    #[must_use]
    pub fn held_by(project: &Path) -> Option<String> {
        let text = std::fs::read_to_string(lock_path(project)).ok()?;
        let f: LockFile = serde_json::from_str(&text).ok()?;
        if f.pc == author() {
            if f.pid == i64::from(std::process::id()) || !bc2_formats::gameledger::pid_alive(f.pid) {
                return None;
            }
            return Some(format!("on this PC, process {}, since {}", f.pid, f.since));
        }
        Some(format!("on the PC {}, process {}, since {}", f.pc, f.pid, f.since))
    }

    /// The lock file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for ProjectLock {
    fn drop(&mut self) {
        // only this lock's own file: another editor may have taken a stale one over meanwhile
        let ours = std::fs::read_to_string(&self.path)
            .ok()
            .and_then(|t| serde_json::from_str::<LockFile>(&t).ok())
            .is_some_and(|f| f.token == self.token);
        if ours {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

/// A map project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MapProject {
    /// [`FORMAT`].
    pub format: String,
    /// [`VERSION`].
    pub version: u32,
    /// The lowest editor version that reads the file (round OP1): the newest operation kind of its log
    /// ([`MapProject::reader_needed`]); a reader newer than that but older than `version` still reads it.
    #[serde(default)]
    pub min_reader: u32,
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
        let id = bc2_formats::guid::new_guid(crate::NAMESPACE, &["project", name, level, &created, &by]);
        Self {
            format: FORMAT.into(),
            version: VERSION,
            min_reader: 1,
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
                files: std::collections::BTreeMap::new(),
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

    /// The exported level's names as the log's last choice left them (round EX1), `None` before one is made.
    #[must_use]
    pub fn names(&self) -> Option<&crate::names::LevelNames> {
        self.operations.iter().rev().find_map(|o| match &o.op {
            Op::SetNames { to } => Some(to.as_ref()),
            _ => None,
        })
    }

    /// The exported level's weather as the log's last choice left it (round VW2), `None`: the level's own.
    #[must_use]
    pub fn weather(&self) -> Option<&crate::weather::WeatherChoice> {
        self.operations
            .iter()
            .rev()
            .find_map(|o| match &o.op {
                Op::SetWeather { to } => Some(to.as_deref()),
                _ => None,
            })
            .flatten()
    }

    /// The navigation edits the log leaves (round GR2: the AI workspace's edits, in the side file's form).
    #[must_use]
    pub fn nav_edits(&self) -> crate::navedit::NavEdits {
        let mut out = crate::navedit::NavEdits::default();
        for o in &self.operations {
            out.apply(&o.op);
        }
        out
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
            for part in o.op.parts() {
                let gone = match part {
                    Op::Delete { guids } => guids.as_slice(),
                    Op::SetMode { to } => to.removed.as_slice(),
                    _ => &[],
                };
                for g in gone {
                    known.remove(&g.to_ascii_uppercase());
                }
            }
        }
        out
    }

    /// The lowest editor version that reads this project: the newest operation kind in its log and redo stack
    /// ([`OP_VERSIONS`]), and 6 when it records base files (round OP1).
    #[must_use]
    pub fn reader_needed(&self) -> u32 {
        let of = |o: &Operation| {
            serde_json::to_value(&o.op)
                .ok()
                .and_then(|v| v.get("op").and_then(|k| k.as_str().map(str::to_owned)))
                .and_then(|k| OP_VERSIONS.iter().find(|(n, _)| *n == k).map(|x| x.1))
                .unwrap_or(1)
        };
        let ops = self.operations.iter().chain(&self.redo).map(of).max().unwrap_or(1);
        if self.base.files.is_empty() { ops } else { ops.max(6) }
    }

    /// Records a base file the log is about to edit for the first time (round OP1): its sha1 and size, by
    /// [`crate::files::Target::base_key`]. `false` when it was recorded already or the target has no base file.
    pub fn note_file(&mut self, target: &crate::files::Target, bytes: &[u8]) -> bool {
        let Some(key) = target.base_key() else { return false };
        if self.base.files.contains_key(&key) {
            return false;
        }
        self.base.files.insert(
            key,
            BaseFile {
                sha1: sha1_hex(bytes),
                size: bytes.len() as u64,
            },
        );
        true
    }

    /// The project as its file holds it (`version` the current one, `min_reader` made from the log).
    ///
    /// # Errors
    /// Never in practice (`serde_json` on these types).
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let mut out = self.clone();
        out.version = VERSION;
        out.min_reader = self.reader_needed();
        let mut s = serde_json::to_string_pretty(&out)?;
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
        let version = read_version(v.get("version"))?;
        // a newer file is read when its log uses nothing newer than this editor reads (`min_reader`)
        let min_reader = match v.get("min_reader") {
            None => version,
            some => read_version(some)?,
        };
        if min_reader > VERSION {
            return Err(ProjectError::Version(version));
        }
        let v = migrate(v, version.min(VERSION));
        // an operation of an unknown kind is named, not a bare serde error (nor dropped)
        for key in ["operations", "redo"] {
            for (index, o) in v
                .get(key)
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .enumerate()
            {
                if let Err(e) = serde_json::from_value::<Operation>(o.clone())
                    && e.to_string().contains("unknown variant")
                {
                    let op = o.get("op").and_then(serde_json::Value::as_str).unwrap_or("").to_owned();
                    return Err(ProjectError::UnknownOperation { index, op });
                }
            }
        }
        let mut p: Self = serde_json::from_value(v)?;
        p.min_reader = p.reader_needed();
        Ok(p)
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

    /// [`MapProject::save`] only when the file on disk is still what this editor last read or wrote (`expected`:
    /// its [`disk_stamp`] then, `None` when there was no file); the new stamp (round QW2, audit AU1 R4: a second or
    /// a stale editor can no longer overwrite the project without a word).
    ///
    /// # Errors
    /// [`ProjectError::ChangedOnDisk`] when the file changed (nothing written), else as [`MapProject::save`].
    pub fn save_checked(&self, path: &Path, expected: Option<DiskStamp>) -> Result<Option<DiskStamp>, ProjectError> {
        if disk_stamp(path) != expected {
            return Err(ProjectError::ChangedOnDisk(path.display().to_string()));
        }
        self.save(path)?;
        Ok(disk_stamp(path))
    }

    /// Loads a project file.
    ///
    /// # Errors
    /// The file does not read, or is not a project this editor reads.
    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        Self::from_json(&std::fs::read_to_string(path)?)
    }

    /// Writes the autosave beside the project file: a journal holding the project as it is now (the editor appends
    /// its changes to it: [`crate::journal::Journal`]).
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn autosave(&self, path: &Path) -> Result<(), ProjectError> {
        drop(crate::journal::Journal::start(&autosave_path(path), self)?);
        Ok(())
    }

    /// The project an autosave holds: a journal up to its last good line, or (written before round OP1) the whole
    /// project's JSON. `None` when it does not read.
    #[must_use]
    pub fn read_autosave(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(autosave_path(path)).ok()?;
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) == Some(&b'{') {
            return Self::from_json(std::str::from_utf8(&bytes).ok()?).ok();
        }
        crate::journal::replay(&bytes).project
    }

    /// The autosave of the project at `path`, when there is one that reads and differs from the saved project (the
    /// editor stopped before a save): what to offer to recover.
    #[must_use]
    pub fn recover(path: &Path) -> Option<Self> {
        let auto = Self::read_autosave(path)?;
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

/// A file's `version` (or `min_reader`): a whole number from 1 to `u32::MAX`, never cut or defaulted (audit AU4
/// BUG-13).
fn read_version(v: Option<&serde_json::Value>) -> Result<u32, ProjectError> {
    let bad = || ProjectError::BadVersion(v.map_or_else(|| "missing".to_owned(), ToString::to_string));
    let n = v.and_then(serde_json::Value::as_u64).ok_or_else(bad)?;
    u32::try_from(n).ok().filter(|n| *n >= 1).ok_or_else(bad)
}

/// The autosave of a project file.
#[must_use]
pub fn autosave_path(path: &Path) -> PathBuf {
    let mut s = path.as_os_str().to_owned();
    s.push(AUTOSAVE);
    PathBuf::from(s)
}

fn write_whole(path: &Path, text: &str) -> std::io::Result<()> {
    write_bytes(path, text.as_bytes())
}

/// Writes a file whole or not at all (round OP1, audit AU4 DS-20 / BUG-12): the bytes go to a temporary file beside
/// it, flushed to the disk (`sync_all`, so a power cut cannot leave the new name on empty data), then moved over the
/// file, the move tried again while another process holds the file for a moment ([`bc2_formats::atomic::replace`]:
/// a virus scanner, a sync client). On any failure the temporary file is removed.
///
/// # Errors
/// The folder cannot be made, or the file written or moved.
pub fn write_bytes(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write as _;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    let mut tmp = path.as_os_str().to_owned();
    let k = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    tmp.push(format!(".tmp-{}-{k}", std::process::id()));
    let tmp = PathBuf::from(tmp);
    let written = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        drop(f);
        bc2_formats::atomic::replace(&tmp, path)
    })();
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written
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

    /// Round QW2 (audit AU1 R4): the lock file and the check before a save.
    #[test]
    fn lock_and_stamp() {
        let dir = std::env::temp_dir().join(format!("bc2_levelwrite_lock_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let p = MapProject::new("Locked", "mp_001", "levels/mp_001/mp_001", b"base", Profile::Bfbc2);
        let path = dir.join(p.file_name());
        // a save that expects no file writes one; the next save must expect what it wrote
        let first = p.save_checked(&path, None).unwrap();
        assert!(first.is_some());
        let again = p.save_checked(&path, first).unwrap();
        // another writer (a second editor, a stale one): refused, in words, the other version kept
        let mut other = p.clone();
        other.name = "Changed elsewhere".into();
        std::thread::sleep(std::time::Duration::from_millis(20));
        other.save(&path).unwrap();
        let e = p.save_checked(&path, again).unwrap_err();
        assert!(matches!(e, ProjectError::ChangedOnDisk(_)));
        assert!(
            e.to_string()
                .contains("was changed on disk after this editor opened or last saved it"),
            "{e}"
        );
        assert_eq!(MapProject::load(&path).unwrap().name, "Changed elsewhere");
        let e = p.save_checked(&dir.join("gone.mapedit.json"), again).unwrap_err();
        assert!(
            matches!(e, ProjectError::ChangedOnDisk(_)),
            "a removed file is a change too"
        );

        // the lock: free, taken, this process may take it again (reopening), dropped -> removed
        assert!(ProjectLock::held_by(&path).is_none());
        let lock = ProjectLock::acquire(&path).unwrap();
        assert!(lock.path().is_file());
        assert!(
            ProjectLock::held_by(&path).is_none(),
            "this process's own lock does not refuse it"
        );
        let reopened = ProjectLock::acquire(&path).unwrap();
        drop(lock); // the older lock no longer owns the file: it stays
        assert!(reopened.path().is_file());
        drop(reopened);
        assert!(!lock_path(&path).exists());
        // another running process on this PC holds it: refused, naming it and the lock file
        let holder = std::process::Command::new("cmd")
            .args(["/c", "ping", "-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn();
        if let Ok(mut child) = holder {
            let held = format!(
                "{{\"pc\":{:?},\"pid\":{},\"since\":\"2026-11-21T00:00:00Z\",\"token\":\"x\"}}",
                author(),
                child.id()
            );
            std::fs::write(lock_path(&path), &held).unwrap();
            let e = ProjectLock::acquire(&path).unwrap_err();
            let _ = child.kill();
            let _ = child.wait();
            assert!(matches!(e, ProjectError::Locked { .. }));
            let text = e.to_string();
            assert!(
                text.contains("is already open in another editor (on this PC, process"),
                "{text}"
            );
            assert!(text.contains(&lock_path(&path).display().to_string()), "{text}");
            // the same process gone: a stale lock, taken over
            let lock = ProjectLock::acquire(&path).unwrap();
            drop(lock);
            assert!(!lock_path(&path).exists());
        }
        // another PC's lock: refused (its process cannot be checked from here)
        std::fs::write(
            lock_path(&path),
            "{\"pc\":\"ANOTHER-PC\",\"pid\":4,\"since\":\"2026-11-21T00:00:00Z\",\"token\":\"x\"}",
        )
        .unwrap();
        assert!(ProjectLock::held_by(&path).unwrap().contains("on the PC ANOTHER-PC"));
        // a lock file that does not read is taken over
        std::fs::write(lock_path(&path), "not json").unwrap();
        drop(ProjectLock::acquire(&path).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Round OP1: the recorded projects of the earlier versions (`tests/data/projects`, synthetic: guids and
    /// transforms of the test level only) load as the current version with every operation kept as it was written,
    /// and their logs replay to the same partition as they always did (its sha1 recorded) and to no other file.
    #[test]
    fn old_projects_load_and_replay_the_same() {
        let corpus = [
            (1, include_str!("../tests/data/projects/v1.mapedit.json"), 7, 0),
            (2, include_str!("../tests/data/projects/v2.mapedit.json"), 8, 0),
            (4, include_str!("../tests/data/projects/v4.mapedit.json"), 9, 0),
            (5, include_str!("../tests/data/projects/v5.mapedit.json"), 9, 1),
        ];
        let base = crate::write::tests::level();
        let schema = crate::Schema::scan([&crate::write::LevelWriter::parse(&base).unwrap().d]);
        let mut replayed = Vec::new();
        for (version, text, ops, redo) in corpus {
            let raw: serde_json::Value = serde_json::from_str(text).unwrap();
            assert_eq!(raw["version"], version);
            let p = MapProject::from_json(text).unwrap();
            assert_eq!((p.version, p.operations.len(), p.redo.len()), (VERSION, ops, redo));
            assert!(p.base.files.is_empty());
            // every operation as it was written: the same JSON again
            for (o, r) in p.operations.iter().chain(&p.redo).zip(
                raw["operations"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .chain(raw["redo"].as_array().unwrap()),
            ) {
                assert_eq!(&serde_json::to_value(o).unwrap(), r, "v{version}");
            }
            let a =
                crate::files::apply_files(&base, &p.operations, &schema, &|_| None, &crate::files::NoSources).unwrap();
            assert_eq!(a.partition, crate::write::apply(&base, &p.operations, &schema).unwrap());
            assert!(a.files.is_empty());
            assert_eq!(p.reader_needed(), version, "v{version}: its newest kind in the log");
            replayed.push(sha1_hex(&a.partition.bytes));
            // saved again: the current version, and it loads back the same
            let again = MapProject::from_json(&p.to_json().unwrap()).unwrap();
            assert_eq!(again, p);
        }
        // the edits are the same in every version's log (only kinds of later versions were added after them)
        assert!(replayed.iter().all(|s| *s == replayed[0]), "{replayed:?}");
        assert_eq!(replayed[0], OLD_REPLAY_SHA1);
    }

    /// The test level after the corpus's partition edits (a move, a duplicate, a field set), as the version-5 editor
    /// (main at 082f2d2, before round OP1) replays the same four files: checked there when the corpus was made.
    const OLD_REPLAY_SHA1: &str = "9eb95861781c3387b0aab68b318af4faa4b46888";

    /// Round OP1: versions read whole (audit AU4 BUG-13), the 5 -> 6 migration, `min_reader` from the log's kinds.
    #[test]
    fn versions_and_min_reader() {
        let a = "AAAAAAAA-0000-0000-0000-000000000001";
        let mut q = MapProject::new("Versions", "test", "levels/TEST", b"base", Profile::Bfbc2);
        q.push(Op::Delete { guids: vec![a.into()] }, "");
        // round OP1 (audit AU4 BUG-13): a version that is missing, 0, or wider than 32 bits is refused, never cut
        let current = format!("\"version\": {VERSION}");
        for bad in [
            "\"version\": 4294967301",
            "\"version\": 0",
            "\"version\": \"6\"",
            "\"vers\": 6",
        ] {
            let text = q.to_json().unwrap().replace(&current, bad);
            let e = MapProject::from_json(&text).unwrap_err();
            assert!(matches!(e, ProjectError::BadVersion(_)), "{bad}: {e}");
        }
        assert!(
            MapProject::from_json(&q.to_json().unwrap().replace(&current, "\"vers\": 6"))
                .unwrap_err()
                .to_string()
                .ends_with("its version is missing")
        );
        // a version-5 file gets its base's `files` (5 -> 6); the operations' kinds set `min_reader`
        let mut v5: serde_json::Value = serde_json::from_str(&q.to_json().unwrap()).unwrap();
        v5["version"] = 5.into();
        v5["base"].as_object_mut().unwrap().remove("files");
        v5.as_object_mut().unwrap().remove("min_reader");
        let migrated = migrate(v5.clone(), 5);
        assert_eq!(migrated["base"]["files"], serde_json::json!({}));
        assert_eq!(migrated["version"], VERSION);
        assert_eq!(MapProject::from_json(&v5.to_string()).unwrap().operations, q.operations);
        let mut r = q.clone();
        assert_eq!(r.reader_needed(), 1);
        r.push(Op::RemovePoint { guid: a.into() }, "");
        assert_eq!(r.reader_needed(), 5);
        r.note_file(&crate::files::Target::group("x_havokasset.dbx"), b"group");
        assert_eq!(r.reader_needed(), 6);
        assert!(r.to_json().unwrap().contains("\"group:x_havokasset.dbx\""));
    }

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
        // round OP1: a newer file whose log needs nothing newer than this editor (`min_reader`) is read
        let readable = q
            .to_json()
            .unwrap()
            .replace(&format!("\"version\": {VERSION}"), "\"version\": 9");
        assert!(readable.contains("\"min_reader\": 1"), "{readable}");
        assert_eq!(MapProject::from_json(&readable).unwrap().operations, q.operations);
        let newer = readable.replace("\"min_reader\": 1", "\"min_reader\": 9");
        let e = MapProject::from_json(&newer).unwrap_err();
        assert!(matches!(e, ProjectError::Version(9)));
        assert_eq!(
            e.to_string(),
            format!("made by a newer editor version (project format 9); this editor reads up to {VERSION}")
        );
        // a version-1 file (the format before GR2's navigation operations) loads unchanged, as the current version
        let v1 = q
            .to_json()
            .unwrap()
            .replace(&format!("\"version\": {VERSION}"), "\"version\": 1");
        let back = MapProject::from_json(&v1).unwrap();
        assert_eq!(back.version, VERSION);
        assert_eq!(back.operations, q.operations);
        // an operation of an unknown kind is named
        let odd = q.to_json().unwrap().replacen("\"op\": \"move\"", "\"op\": \"warp\"", 1);
        let e = MapProject::from_json(&odd).unwrap_err();
        assert!(
            matches!(&e, ProjectError::UnknownOperation { op, .. } if op == "warp"),
            "{e}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
