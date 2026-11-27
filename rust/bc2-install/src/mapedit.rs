//! The map export's install (round OR6, "OPFOR in Rust": Project OPFOR's `mapedit_install.py`, which now runs
//! this through `bfbc2 install mapedit`): a level the map editor exported (`mapexport build`, a stage folder with
//! `stage.json`) put into a game copy as a NEW multiplayer level, and taken out again byte for byte. The editor's
//! Install dialog calls it in process. A stock level is never replaced: the level gets its own folder, and only an
//! earlier install of an export (its folder holds [`RECORD`]) may be replaced.
//!
//! What an install changes in the game copy (spec/transforms/install.md section 2):
//! 1. `Package\levels\<lv>\`: [`RECORD`] first (the sha256 of each bundle; a revert refuses a bundle changed since),
//!    the stage's own copy of what the shared patches are built from ([`KEPT`]: `stage.json`, the description, the
//!    pictures), then the level's bundles (`level` / `terrain` / `ingame` / `loader-00.fbrb`, a WW2 donor's
//!    `vo\en-00.fbrb`);
//! 2. the level's path data (`stage.json`'s `path_data`) through the ledger as owner `mapedit-ai:<lv>`;
//! 3. the shared files in one journalled commit of owner [`OWNER`]: `startup-00` (every exported level's
//!    description and level list items), for levels with their own names `overlay-00` (their texts in every
//!    language) and `ondemand_awards-00` (their pictures), `maps.ini` (a section per level after [`MAPS_MARK`]), and
//!    last the registration (`package.mft`, `bundleManifest`): a level registered while its bundles are missing
//!    crashes the start-up checksum.
//!
//! The shared files are built from the ledger's copy from before this owner while it wrote them last, so every
//! installed export is in one patch, each from its own kept copy (audit AU4-DS-12: never from the editor's live
//! stage folder, which a later export rewrites). A revert of one level among several writes the patch without it;
//! of the last, every shared file goes back as it was before this owner. Every check comes before the first write;
//! a kill at any write step is finished by the next run of the same command.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use bc2_gamefs::sha256;
use bc2_levelwrite::{export, installpatch};
use bc2_pyrt::fmt::{str_repr, thousands};
use bc2_pyrt::json::{self, Value};
use bc2_pyrt::py::join;
use regex::Regex;

use crate::pytext::{latin1, relpath, strip, to_latin1};
use crate::stages::{Stages, Steps, Stop, abspath};
use crate::text::{MapsLevel, maps_with_levels, mft_parse, mft_text};

/// The install ledger's owner of the shared files this installer writes.
pub const OWNER: &str = "mapedit";
/// An installed export's record, in its level folder.
pub const RECORD: &str = "opfor-mapedit-installed.json";
/// The folder, beside the record, holding the stage's own copy of what the shared patches are built from
/// (`stage.json`, the description partition, the pictures; audit AU4-DS-12).
pub const KEPT: &str = "opfor-mapedit-stage";
/// The stage's format.
pub const STAGE_FORMAT: &str = "bfbc2-mapexport-stage";
/// The marker line in `maps.ini` before the exported levels' sections (kept as Project OPFOR's installer wrote it, so
/// the sections of an install made by either are found).
pub const MAPS_MARK: &str =
    "; --- Map editor: exported levels (mapedit_install.py writes this part; --revert takes it out) ---";
/// The null GUID of a `bundleManifest` line.
pub const ZERO: &str = "00000000-0000-0000-0000-000000000000";

/// The game copy's files this installer reads and writes.
#[derive(Debug, Clone)]
pub struct Game {
    /// The game copy written (its path as given: report lines name files under it).
    pub root: String,
    /// The game copy the stage is checked against (only read): an install on a scratch copy of the shared files is
    /// checked against the real game copy.
    pub check_root: String,
}

impl Game {
    /// The game copy `root`, its stage checked against `check_root` (None: `root`).
    #[must_use]
    pub fn new(root: &str, check_root: Option<&str>) -> Self {
        Self {
            root: root.to_owned(),
            check_root: check_root.unwrap_or(root).to_owned(),
        }
    }

    fn file(&self, parts: &[&str]) -> String {
        join(&self.root, parts)
    }

    /// `Package\async\startup-00.fbrb`.
    #[must_use]
    pub fn startup(&self) -> String {
        self.file(&["Package", "async", "startup-00.fbrb"])
    }

    /// `Dist\win32\overlay-00.fbrb`.
    #[must_use]
    pub fn overlay(&self) -> String {
        self.file(&["Dist", "win32", "overlay-00.fbrb"])
    }

    /// `Package\async\ondemand_awards-00.fbrb`.
    #[must_use]
    pub fn awards(&self) -> String {
        self.file(&["Package", "async", "ondemand_awards-00.fbrb"])
    }

    /// `Dist\win32\package.mft`.
    #[must_use]
    pub fn mft(&self) -> String {
        self.file(&["Dist", "win32", "package.mft"])
    }

    /// `Dist\win32\bundleManifest`.
    #[must_use]
    pub fn bm(&self) -> String {
        self.file(&["Dist", "win32", "bundleManifest"])
    }

    /// `maps.ini`.
    #[must_use]
    pub fn maps(&self) -> String {
        self.file(&["maps.ini"])
    }

    /// The shared files, in install order (the registration last).
    #[must_use]
    pub fn shared(&self) -> Vec<String> {
        vec![
            self.startup(),
            self.overlay(),
            self.awards(),
            self.maps(),
            self.mft(),
            self.bm(),
        ]
    }

    /// The shared files that hold content of `levels`: the localization and the pictures only while one of them has
    /// its own name (a stage's `strings` / `awards`), the others while there is any.
    #[must_use]
    pub fn needed(&self, levels: &[Value]) -> Vec<String> {
        let (overlay, awards) = (self.overlay(), self.awards());
        self.shared()
            .into_iter()
            .filter(|p| {
                if *p == overlay {
                    levels.iter().any(|l| truthy(l, "strings"))
                } else if *p == awards {
                    levels.iter().any(|l| truthy(l, "awards"))
                } else {
                    !levels.is_empty()
                }
            })
            .collect()
    }

    /// An exported level's folder (`Package\levels\<lv>`).
    #[must_use]
    pub fn folder(&self, lv: &str) -> String {
        self.file(&["Package", "levels", &lv.to_lowercase()])
    }

    /// A file's name relative to the game copy, as the report lines give it (`Package\async\startup-00.fbrb`).
    #[must_use]
    pub fn rel(&self, path: &str) -> String {
        relpath(&abspath(path), &abspath(&self.root))
    }
}

// ------------------------------------------------------------------------------------------------ records

fn get<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
    v.get(key)
}

fn truthy(v: &Value, key: &str) -> bool {
    get(v, key).is_some_and(Value::truthy)
}

fn text<'a>(v: &'a Value, key: &str) -> &'a str {
    get(v, key).and_then(Value::as_str).unwrap_or_default()
}

fn list<'a>(v: &'a Value, key: &str) -> &'a [Value] {
    get(v, key).and_then(Value::as_list).unwrap_or_default()
}

fn strs(v: &Value) -> Vec<String> {
    v.as_list()
        .unwrap_or_default()
        .iter()
        .map(|x| x.as_str().unwrap_or_default().to_owned())
        .collect()
}

/// Python's `str()` of a record's value as a report line shows it.
fn shown(v: Option<&Value>, default: &str) -> String {
    match v {
        None => default.to_owned(),
        Some(Value::Str(s)) => s.clone(),
        Some(Value::Int(i)) => i.to_string(),
        Some(other) => json::dumps(other, None),
    }
}

fn size(v: &Value) -> String {
    thousands(get(v, "size").and_then(Value::as_int).unwrap_or(0))
}

/// The level id of a record.
#[must_use]
pub fn level_of(rec: &Value) -> &str {
    text(rec, "level")
}

fn read_json(path: &str) -> Result<Value, Stop> {
    let d = fs::read(path).map_err(|e| format!("{path}: {e}"))?;
    json::loads_utf8(&d).ok_or_else(|| format!("{path}: not JSON"))
}

/// A stage's record (`stage.json`).
///
/// # Errors
/// No record, not JSON, or not a stage.
pub fn read_stage(stage: &str) -> Result<Value, Stop> {
    let rec = read_json(&join(stage, &["stage.json"]))?;
    if text(&rec, "format") != STAGE_FORMAT {
        return Err(format!("{stage}: not a map export stage"));
    }
    Ok(rec)
}

/// The exported levels installed in the game copy: (level id, its record), in the order of their folders' names.
///
/// # Errors
/// A record that cannot be read.
pub fn installed(game: &Game) -> Result<Vec<(String, Value)>, Stop> {
    let base = game.file(&["Package", "levels"]);
    let mut names: Vec<String> = match fs::read_dir(&base) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    };
    names.sort();
    let mut out: Vec<(String, Value)> = Vec::new();
    for d in names {
        let p = join(&base, &[&d, RECORD]);
        if Path::new(&p).is_file() {
            let rec = read_json(&p)?;
            let lv = level_of(&rec).to_owned();
            if let Some(slot) = out.iter_mut().find(|(k, _)| *k == lv) {
                slot.1 = rec;
            } else {
                out.push((lv, rec));
            }
        }
    }
    Ok(out)
}

/// The Server Browser's row of an exported level, as OPFOR's DLL writes it (`src\dll\server_browser.cpp`:
/// `OPFOR - <maps.ini name> (<mode>)`, cut at 159 characters; the section is found by the level's id).
#[must_use]
pub fn browser_row(rec: &Value) -> String {
    format!("OPFOR - {} ({})", text(rec, "title"), text(rec, "mode"))
        .chars()
        .take(159)
        .collect()
}

/// The install ledger's owner of an exported level's path data.
#[must_use]
pub fn ai_owner(lv: &str) -> String {
    format!("{OWNER}-ai:{}", lv.to_lowercase())
}

// ------------------------------------------------------------------------------------------------ the text files

fn own_pattern(prefix: &str, drop: &BTreeSet<String>) -> Option<Regex> {
    if drop.is_empty() {
        return None;
    }
    let alts: Vec<String> = drop.iter().map(|d| regex::escape(d)).collect();
    Regex::new(&format!("(?i)^{prefix}(?:{})/", alts.join("|"))).ok()
}

/// `package.mft` with every block of the levels in `drop` (lower-case ids) out and each level's blocks at the end
/// (the WW2 install's rules, [`mft_text`]: blocks one blank line apart, CRLF). DICE's own file ends with a blank line
/// (CRLF CRLF): the file's run of line ends at its end is kept, so on DICE's file only the levels' blocks change
/// (XINST-MAP-7; OPFOR's Python wrote one CRLF).
#[must_use]
pub fn mft_with(text: &str, levels: &[Value], drop: &BTreeSet<String>) -> String {
    let (head, blocks) = mft_parse(text);
    let own = own_pattern(&regex::escape("BundleSource game:levels/"), drop);
    let mut keep: Vec<Vec<String>> = blocks
        .into_iter()
        .filter(|b| {
            !own.as_ref()
                .is_some_and(|r| r.is_match(strip(b.first().map_or("", String::as_str))))
        })
        .collect();
    for lv in levels {
        keep.extend(list(lv, "mft_blocks").iter().map(strs));
    }
    let out = mft_text(&head, &keep);
    let body = text.trim_end_matches(['\r', '\n']);
    let tail = text.get(body.len()..).unwrap_or_default();
    if tail.len() > 2 && tail.replace("\r\n", "").is_empty() {
        out.trim_end_matches(['\r', '\n']).to_owned() + tail
    } else {
        out
    }
}

/// `bundleManifest` with the dropped levels' lines (and empty lines) out and each level's lines at the end.
#[must_use]
pub fn bm_with(text: &str, levels: &[Value], drop: &BTreeSet<String>) -> String {
    let own = own_pattern("levels/", drop);
    let mut lines: Vec<String> = text
        .split('\n')
        .filter(|l| !l.is_empty() && !own.as_ref().is_some_and(|r| r.is_match(l)))
        .map(str::to_owned)
        .collect();
    for lv in levels {
        lines.extend(strs(get(lv, "bundle_manifest").unwrap_or(&Value::Null)));
    }
    lines.join("\n") + "\n"
}

/// `maps.ini` with the marker and the dropped levels' sections out, then the marker and a section per level
/// (`[<LV>]`, `name=<title>`, `<mode>=Levels/<LV>`).
#[must_use]
pub fn maps_with(text: &str, levels: &[Value], drop: &BTreeSet<String>) -> String {
    let rows: Vec<MapsLevel> = levels
        .iter()
        .map(|l| MapsLevel {
            name: level_of(l).to_owned(),
            title: text_of(l, "title"),
            browser_mode: text_of(l, "mode"),
        })
        .collect();
    let strip_names: Vec<String> = drop.iter().cloned().collect();
    maps_with_levels(text, &rows, &strip_names, MAPS_MARK)
}

fn text_of(v: &Value, key: &str) -> String {
    text(v, key).to_owned()
}

/// A text file's bytes as text: UTF-8 when they are, else latin-1 (every byte one character); and back the same way.
/// What an install adds (ids, titles, registration lines) is plain ASCII by the export's name rules (XEXP-NAME), so
/// either way every byte outside the installer's lines stays as it was.
fn decode(data: &[u8]) -> (String, bool) {
    match String::from_utf8(data.to_vec()) {
        Ok(s) => (s, true),
        Err(_) => (latin1(data), false),
    }
}

fn encode(text: &str, utf8: bool, what: &str) -> Result<Vec<u8>, Stop> {
    if utf8 {
        Ok(text.as_bytes().to_vec())
    } else {
        to_latin1(text).ok_or_else(|| format!("{what}: a character the file's encoding (latin-1) cannot hold"))
    }
}

// ------------------------------------------------------------------------------------------------ the installer

/// A text file's patch: (its text, the levels, the dropped ids) -> the new text.
type Patch = fn(&str, &[Value], &BTreeSet<String>) -> String;

/// The shared files built for an install set: (game file, built file), in install order.
pub type Built = Vec<(String, String)>;

/// What [`Installer::plan`] found: the stage's record and the shared files built.
#[derive(Debug, Clone)]
pub struct Planned {
    /// The stage's record.
    pub rec: Value,
    /// The stage folder (as given).
    pub stage: String,
    /// The shared files built for the install set.
    pub built: Built,
}

/// One run of the installer on a game copy: its lines go to `log`.
pub struct Installer<'a> {
    game: Game,
    work: String,
    stages: Stages,
    log: &'a mut dyn FnMut(&str),
    checked: bool,
}

impl std::fmt::Debug for Installer<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Installer")
            .field("game", &self.game)
            .field("work", &self.work)
            .finish_non_exhaustive()
    }
}

fn tool_failed(cmd: &str, e: &dyn std::fmt::Display) -> Stop {
    format!("mapexport {cmd} failed:\n\nmapexport: {e}\n")
}

fn sha(p: &str) -> Result<String, Stop> {
    sha256::file(p).map_err(|e| format!("{p}: {e}"))
}

fn is_file(p: &str) -> bool {
    Path::new(p).is_file()
}

fn file_size(p: &str) -> u64 {
    fs::metadata(p).map_or(0, |m| m.len())
}

/// `src` copied over `dst` atomically (a temporary copy beside it, flushed, moved over it); a dead writer's
/// temporary files beside `dst` cleared first.
fn copy_atomic(src: &str, dst: &str) -> Result<(), Stop> {
    let dst_p = Path::new(dst);
    bc2_gamefs::atomic::remove_dead_temps(dst_p, |pid| bc2_gamefs::liveness::pid_alive(i64::from(pid)));
    let tmp = bc2_gamefs::atomic::temp_name(dst_p);
    let r = (|| -> std::io::Result<()> {
        fs::copy(src, &tmp)?;
        fs::OpenOptions::new().write(true).open(&tmp)?.sync_all()?;
        bc2_gamefs::atomic::replace(&tmp, dst_p)
    })();
    if r.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    r.map_err(|e| format!("{src} -> {dst}: {e}"))
}

fn write_atomic(dst: &str, data: &[u8]) -> Result<(), Stop> {
    let p = Path::new(dst);
    bc2_gamefs::atomic::remove_dead_temps(p, |pid| bc2_gamefs::liveness::pid_alive(i64::from(pid)));
    bc2_gamefs::atomic::write(p, &[data]).map_err(|e| format!("{dst}: {}", e.io()))
}

/// Whether a level folder without [`RECORD`] is what an interrupted install left before its record was written
/// (empty, or only the record's temporary files): an install may use it.
fn interrupted_start(folder: &str) -> bool {
    let Ok(rd) = fs::read_dir(folder) else {
        return false;
    };
    let low = RECORD.to_lowercase();
    rd.flatten().all(|e| {
        let n = e.file_name().to_string_lossy().to_lowercase();
        bc2_gamefs::atomic::temp_writer(&low, &n).is_some() || n.starts_with(&format!("{low}.opfor-tmp-"))
    })
}

impl<'a> Installer<'a> {
    /// An installer on `game`, building the shared files under `work` (`<opfor>\work\mapedit`: `<lv>\shared`,
    /// `_revert`, removed when the run ends), its lines given to `log`; `BC2_INSTALL_CRASH_AT` read for the tests.
    #[must_use]
    pub fn new(game: Game, work: &str, log: &'a mut dyn FnMut(&str)) -> Self {
        Self::with_steps(game, work, Steps::from_env(), log)
    }

    /// [`Installer::new`] with its write steps counted (and stopped) by `steps` (tests).
    #[must_use]
    pub fn with_steps(game: Game, work: &str, steps: Steps, log: &'a mut dyn FnMut(&str)) -> Self {
        let stages = Stages::new(&game.root, steps);
        Self {
            game,
            work: work.to_owned(),
            stages,
            log,
            checked: true,
        }
    }

    /// The stage's check left out (tests on synthetic stages, which `mapexport check`'s rules on real bundles would
    /// refuse; OPFOR's Python tests installed without it the same way).
    #[doc(hidden)]
    #[must_use]
    pub fn without_check(mut self) -> Self {
        self.checked = false;
        self
    }

    /// The game copy.
    #[must_use]
    pub fn game(&self) -> &Game {
        &self.game
    }

    /// The write steps counted so far.
    #[must_use]
    pub fn steps(&self) -> u32 {
        self.stages.steps().count()
    }

    fn say(&mut self, line: &str) {
        (self.log)(line);
    }

    fn step(&self, label: &str) -> Result<(), Stop> {
        self.stages.steps().step(label)
    }

    fn source(&mut self, path: &str) -> Result<String, Stop> {
        self.stages.source(OWNER, path, &mut *self.log)
    }

    /// The stage a level's shared content is built from: its kept copy ([`KEPT`]) when the install made one, else
    /// (an install from before round OR6) the stage folder its record names, with a note.
    fn stage_of(&mut self, lv: &str, rec: &Value) -> String {
        let kept = join(&self.game.folder(lv), &[KEPT]);
        if is_file(&join(&kept, &["stage.json"])) {
            return kept;
        }
        let live = text_of(rec, "stage");
        self.say(&format!(
            "  note  {lv}: installed without a kept copy of its stage - its description, texts and pictures are read \
             from {live}"
        ));
        live
    }

    /// The shared files with exactly `levels` installed, each built from the ledger's copy from before this owner
    /// while it wrote the file last: built into `out`. `drop`: the level ids (lower case) whose old content goes.
    fn build_shared(
        &mut self,
        levels: &[Value],
        stages: &BTreeMap<String, String>,
        drop: &BTreeSet<String>,
        out: &str,
    ) -> Result<Built, Stop> {
        fs::create_dir_all(out).map_err(|e| format!("{out}: {e}"))?;
        let mut built = Built::new();
        let need = self.game.needed(levels);
        let remove: Vec<String> = drop.iter().map(|d| d.to_uppercase()).collect();
        let unpack = join(out, &["_unpack"]);
        let stage = |lv: &Value| stages.get(level_of(lv)).cloned().unwrap_or_default();
        // startup-00: every level's description and its level list items
        let src = self.source(&self.game.startup())?;
        let mut add = Vec::new();
        for lv in levels {
            let desc = get(lv, "description")
                .and_then(Value::as_list)
                .and_then(|d| d.first())
                .and_then(Value::as_str)
                .unwrap_or_default();
            let p = join(&stage(lv), &[desc]);
            let bytes = fs::read(&p).map_err(|e| tool_failed("startup", &format!("{p}: {e}")))?;
            add.push((level_of(lv).to_owned(), bytes));
        }
        let dst = join(out, &["startup-00.fbrb"]);
        export::patch_startup(Path::new(&src), &add, &remove, Path::new(&unpack), Path::new(&dst))
            .map_err(|e| tool_failed("startup", &e))?;
        built.push((self.game.startup(), dst));
        // the levels' own names (every language) and pictures: built from the copy before this owner, like startup
        for (path, cmd, key) in [
            (self.game.overlay(), "overlay", "strings"),
            (self.game.awards(), "awards", "awards"),
        ] {
            if !need.contains(&path) {
                continue;
            }
            let name = Path::new(&path)
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let dst = join(out, &[&name]);
            let src = self.source(&path)?;
            let with: Vec<PathBuf> = levels
                .iter()
                .filter(|l| truthy(l, key))
                .map(|l| PathBuf::from(stage(l)))
                .collect();
            let (s, w, d) = (Path::new(&src), Path::new(&unpack), Path::new(&dst));
            if cmd == "overlay" {
                installpatch::overlay(s, &with, &remove, w, d).map_err(|e| tool_failed(cmd, &e))?;
            } else {
                installpatch::awards(s, &with, &remove, w, d).map_err(|e| tool_failed(cmd, &e))?;
            }
            built.push((path, dst));
        }
        // the text files
        let texts: [(String, Patch); 3] = [
            (self.game.maps(), maps_with),
            (self.game.mft(), mft_with),
            (self.game.bm(), bm_with),
        ];
        for (path, f) in texts {
            let src = self.source(&path)?;
            let data = fs::read(&src).map_err(|e| format!("{src}: {e}"))?;
            let (old, utf8) = decode(&data);
            let new = encode(&f(&old, levels, drop), utf8, &path)?;
            let name = Path::new(&path)
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let p = join(out, &[&name]);
            fs::write(&p, new).map_err(|e| format!("{p}: {e}"))?;
            built.push((path, p));
        }
        Ok(built)
    }

    /// Checks the stage (`mapexport check`'s rules against the check game copy) and builds the shared files with it
    /// installed beside the other installed exports. Nothing in the game copy is written.
    ///
    /// # Errors
    /// The stage is not one, its check fails, its folder is a stock or other level's, a build fails.
    pub fn plan(&mut self, stage: &str) -> Result<Planned, Stop> {
        let rec = read_stage(stage)?;
        let lv = level_of(&rec).to_owned();
        if self.checked {
            self.check(stage)?;
        }
        let folder = self.game.folder(&lv);
        if Path::new(&folder).exists() && !is_file(&join(&folder, &[RECORD])) && !interrupted_start(&folder) {
            return Err(format!(
                "{folder} exists and is not an exported level: a stock or other level is never replaced"
            ));
        }
        let have = installed(&self.game)?;
        let mut stages = BTreeMap::new();
        let mut others: Vec<(String, Value)> = have.iter().filter(|(k, _)| *k != lv).cloned().collect();
        others.sort_by(|a, b| a.0.cmp(&b.0));
        for (k, v) in &others {
            let s = self.stage_of(k, v);
            stages.insert(k.clone(), s);
        }
        stages.insert(lv.clone(), stage.to_owned());
        let mut levels: Vec<Value> = others.into_iter().map(|(_, v)| v).collect();
        levels.push(rec.clone());
        let mut drop: BTreeSet<String> = have.iter().map(|(k, _)| k.to_lowercase()).collect();
        drop.insert(lv.to_lowercase());
        let out = join(&self.work, &[&lv.to_lowercase(), "shared"]);
        let built = self.build_shared(&levels, &stages, &drop, &out)?;
        Ok(Planned {
            rec,
            stage: stage.to_owned(),
            built,
        })
    }

    /// The stage's check (`mapexport check`): its lines, then `check: ok`, or a stop with every line.
    fn check(&mut self, stage: &str) -> Result<(), Stop> {
        let g = export::Game::open(Path::new(&self.game.check_root)).map_err(|e| tool_failed("check", &e))?;
        let r = export::check(&g, Path::new(stage)).map_err(|e| tool_failed("check", &e))?;
        let mut lines: Vec<String> = r
            .lines()
            .map(|(code, ok, what)| export::CheckReport::line(code, ok, what))
            .collect();
        lines.push(if r.ok() { "check: ok" } else { "check: FAILED" }.to_owned());
        if !r.ok() {
            return Err(format!("mapexport check failed:\n{}\n\n", lines.join("\n")));
        }
        for l in &lines {
            self.say(l);
        }
        Ok(())
    }

    /// The shared files this owner wrote that the new patch leaves out (no installed export needs them any more):
    /// they go back as they were before this owner.
    fn restored(&self, built: &Built) -> Result<Vec<String>, Stop> {
        let held: BTreeSet<String> = self.stages.installed(OWNER)?.iter().map(|r| r.to_lowercase()).collect();
        Ok(self
            .game
            .shared()
            .into_iter()
            .filter(|p| {
                !built.iter().any(|(g, _)| g == p) && held.contains(&self.game.rel(p).replace('\\', "/").to_lowercase())
            })
            .collect())
    }

    /// What an install of the planned stage changes in the game copy, line by line.
    ///
    /// # Errors
    /// A file that cannot be read.
    #[allow(clippy::too_many_lines, reason = "the report's lines, in their order")]
    pub fn describe(&mut self, p: &Planned) -> Result<(), Stop> {
        let rec = &p.rec;
        let lv = level_of(rec).to_owned();
        let low = lv.to_lowercase();
        let mut out = vec![format!(
            "{} ({lv}) - an install would change, in {}:",
            text(rec, "title"),
            self.game.root
        )];
        for f in list(rec, "files") {
            out.push(format!(
                "  new   Package\\levels\\{low}\\{} ({} bytes)",
                text(f, "path").replace('/', "\\"),
                size(f)
            ));
        }
        out.push(format!("  new   Package\\levels\\{low}\\{RECORD}"));
        out.push(format!(
            "  new   Package\\levels\\{low}\\{KEPT}\\ (the stage's description, texts and pictures, kept for the shared \
             patches of later installs)"
        ));
        for (path, b) in &p.built {
            let old = if is_file(path) { file_size(path) } else { 0 };
            let same = is_file(path) && sha(path)? == sha(b)?;
            out.push(format!(
                "  {} {} ({} -> {} bytes; the file before it kept by the install ledger)",
                if same { "same " } else { "write" },
                self.game.rel(path),
                thousands(i128::from(old)),
                thousands(i128::from(file_size(b)))
            ));
        }
        for path in self.restored(&p.built)? {
            out.push(format!(
                "  back  {} as it was before this tool (no installed export needs it now)",
                self.game.rel(&path)
            ));
        }
        let pairs = |v: &[Value], r: fn(&str) -> String| -> String {
            v.iter()
                .map(|kt| {
                    let kt = strs(kt);
                    format!(
                        "{} = {}",
                        kt.first().map_or("", String::as_str),
                        r(kt.get(1).map_or("", String::as_str))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ")
        };
        if truthy(rec, "strings") {
            out.push(format!(
                "  texts (every language of overlay-00): {}",
                pairs(list(rec, "strings"), str_repr)
            ));
        }
        if let Some(Value::Dict(by)) = get(rec, "strings_by_language") {
            let mut by = by.clone();
            by.sort_by(|a, b| a.0.cmp(&b.0));
            for (lang, texts) in &by {
                out.push(format!(
                    "  texts ({lang} only): {}",
                    pairs(texts.as_list().unwrap_or_default(), ascii_repr)
                ));
            }
        }
        for a in list(rec, "awards") {
            out.push(format!(
                "  picture (ondemand_awards-00): {} ({} bytes)",
                text(a, "install"),
                size(a)
            ));
        }
        out.push(format!(
            "  profile: {}; mode {}; donor {}{}",
            shown(get(rec, "profile"), "BFBC2"),
            text(rec, "mode"),
            shown(get(rec, "donor"), "?"),
            if truthy(rec, "project_mode") {
                " (the project's own mode layer)"
            } else {
                ""
            }
        ));
        out.push(format!(
            "  maps.ini: [{lv}] name={} {}=Levels/{lv}",
            text(rec, "title"),
            text(rec, "mode")
        ));
        out.push(format!(
            "  server browser row: {} (the launcher: {})",
            str_repr(&browser_row(rec)),
            str_repr(text(rec, "title"))
        ));
        out.push(format!(
            "  startup-00 level list: {}",
            strs(get(rec, "level_list").unwrap_or(&Value::Null)).join(", ")
        ));
        let closure = list(rec, "closure");
        if !closure.is_empty() {
            let mut froms: BTreeMap<String, usize> = BTreeMap::new();
            for c in closure {
                *froms.entry(text_of(c, "from")).or_default() += 1;
            }
            out.push(format!(
                "  in the level's own bundle: {} entries copied from {}",
                closure.len(),
                froms
                    .iter()
                    .map(|(k, n)| format!("{k} ({n})"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        out.push("  package.mft blocks:".to_owned());
        for b in list(rec, "mft_blocks") {
            out.push(format!("    {}", strs(b).join(" | ")));
        }
        out.push(format!(
            "  bundleManifest lines: {}",
            strs(get(rec, "bundle_manifest").unwrap_or(&Value::Null))
                .iter()
                .map(|l| l.split('\t').next().unwrap_or_default())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for l in out {
            self.say(&l);
        }
        Ok(())
    }

    /// `--plan`: the plan's lines and `plan only: nothing in the game copy changed`.
    ///
    /// # Errors
    /// As [`Installer::plan`].
    pub fn plan_only(&mut self, stage: &str) -> Result<(), Stop> {
        let p = self.plan(stage)?;
        self.describe(&p)?;
        self.drop_scratch(stage);
        self.say("plan only: nothing in the game copy changed");
        Ok(())
    }

    /// The build's scratch (`<work>\<lv>\shared`, `<work>\_revert`) removed when a run went through (XINST-STG-5: a
    /// failed run leaves it to look at); every run builds it again.
    fn drop_scratch(&self, stage: &str) {
        let _ = fs::remove_dir_all(join(&self.work, &["_revert"]));
        if let Ok(rec) = read_stage(stage) {
            let lv = join(&self.work, &[&level_of(&rec).to_lowercase()]);
            let _ = fs::remove_dir_all(join(&lv, &["shared"]));
            let _ = fs::remove_dir(lv); // only when empty: the editor's stage is beside it
        }
    }

    /// The stage's path data (`stage.json`'s `path_data`): (game file, staged file), each to its install path.
    fn path_data_writes(&self, rec: &Value, stage: &str) -> Vec<(String, String)> {
        get(rec, "path_data")
            .map_or(&[][..], |pd| list(pd, "files"))
            .iter()
            .map(|f| {
                let inst: Vec<&str> = text(f, "install").split('/').collect();
                let path: Vec<&str> = text(f, "path").split('/').collect();
                (join(&self.game.root, &inst), join(stage, &path))
            })
            .collect()
    }

    /// `--apply`: the plan, its report, every check, then the writes: the level's record, its kept copy, its bundles,
    /// its path data, the shared files (the registration last). Refused while the game runs.
    ///
    /// # Errors
    /// Any refusal (nothing written), a failed write (each ledger commit rolled back), the test hook.
    pub fn apply(&mut self, stage: &str) -> Result<(), Stop> {
        refuse_while_game_runs()?;
        self.apply_inner(stage)?;
        self.drop_scratch(stage);
        Ok(())
    }

    fn apply_inner(&mut self, stage: &str) -> Result<(), Stop> {
        let p = self.plan(stage)?;
        self.describe(&p)?;
        let rec = &p.rec;
        let lv = level_of(rec).to_owned();
        let ai = self.path_data_writes(rec, stage);
        for (dst, _) in &ai {
            let line = format!(
                "  new   {} (the level's path data, through the install ledger)",
                self.game.rel(dst)
            );
            self.say(&line);
        }
        // every check before the first write
        let order: Vec<String> = self
            .game
            .shared()
            .into_iter()
            .filter(|s| p.built.iter().any(|(g, _)| g == s))
            .collect();
        let refs: Vec<&str> = order.iter().map(String::as_str).collect();
        self.stages.require_install(OWNER, &refs, &mut *self.log)?;
        if !ai.is_empty() {
            let dsts: Vec<&str> = ai.iter().map(|(d, _)| d.as_str()).collect();
            self.stages.require_install(&ai_owner(&lv), &dsts, &mut *self.log)?;
        }
        let back = self.restored(&p.built)?;
        if !back.is_empty() {
            let refs: Vec<&str> = back.iter().map(String::as_str).collect();
            self.stages.require_revert(OWNER, Some(&refs), &mut *self.log)?;
        }
        self.write_level(rec, stage)?;
        if !ai.is_empty() {
            let mut files = Vec::new();
            for (dst, src) in &ai {
                files.push((dst.clone(), fs::read(src).map_err(|e| format!("{src}: {e}"))?));
            }
            self.stages.install(&ai_owner(&lv), &files, &mut *self.log)?; // the bots' path data
        }
        self.write_shared(&p.built, &back)?; // the registration (package.mft, bundleManifest) last
        self.say(&format!("{lv}: installed"));
        Ok(())
    }

    /// The level folder: its record first (a kill before the bundles are in leaves an install the next run finishes,
    /// or a revert takes out), the kept copy, then the bundles, each checked against the stage's sha256.
    fn write_level(&mut self, rec: &Value, stage: &str) -> Result<(), Stop> {
        let lv = level_of(rec).to_owned();
        let folder = self.game.folder(&lv);
        self.step(&format!("mkdir {folder}"))?;
        fs::create_dir_all(&folder).map_err(|e| format!("{folder}: {e}"))?;
        let mut shas: Vec<(String, Value)> = Vec::new();
        for f in list(rec, "files") {
            let path = text(f, "path");
            shas.push((path.to_owned(), Value::Str(sha(&join(stage, &[path]))?)));
        }
        let mut record = match rec {
            Value::Dict(items) => items.clone(),
            _ => Vec::new(),
        };
        set(&mut record, "stage", Value::Str(abspath(stage)));
        set(&mut record, "installed", Value::Dict(shas.clone()));
        let rp = join(&folder, &[RECORD]);
        self.step(&format!("write {rp}"))?;
        write_atomic(&rp, json::dumps(&Value::Dict(record), Some(1)).as_bytes())?;
        self.keep_stage(rec, stage, &folder)?;
        for (path, want) in &shas {
            let dst = join(&folder, &[path]);
            if let Some(d) = Path::new(&dst).parent() {
                fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?; // vo\en-00.fbrb (a WW2 donor's)
            }
            self.step(&format!("copy {dst}"))?;
            copy_atomic(&join(stage, &[path]), &dst)?;
            if Some(sha(&dst)?.as_str()) != want.as_str() {
                return Err(format!(
                    "{dst}: not the stage's bundle after the copy (the stage changed during the install?) - run the \
                     install again"
                ));
            }
        }
        Ok(())
    }

    /// The stage's own copy of what the shared patches are built from (audit AU4-DS-12): `stage.json`, the
    /// description partition and the pictures, in `<folder>\opfor-mapedit-stage` at their paths in the stage; files
    /// of an earlier install's copy no longer named are removed.
    fn keep_stage(&mut self, rec: &Value, stage: &str, folder: &str) -> Result<(), Stop> {
        let kept = join(folder, &[KEPT]);
        let mut files: Vec<String> = Vec::new();
        if let Some(d) = get(rec, "description").and_then(Value::as_list).and_then(|d| d.first()) {
            files.push(d.as_str().unwrap_or_default().to_owned());
        }
        for a in list(rec, "awards") {
            files.push(text_of(a, "path"));
        }
        // stage.json last: its presence says the copy is whole
        files.push("stage.json".to_owned());
        let want: BTreeSet<String> = files.iter().map(|f| f.replace('\\', "/").to_lowercase()).collect();
        for f in &files {
            let parts: Vec<&str> = f.split(['/', '\\']).collect();
            let dst = join(&kept, &parts);
            if let Some(d) = Path::new(&dst).parent() {
                fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
            }
            self.step(&format!("keep {dst}"))?;
            copy_atomic(&join(stage, &parts), &dst)?;
        }
        for old in walk(Path::new(&kept)) {
            let rel = old.strip_prefix(&kept).map_or_else(
                |_| String::new(),
                |r| r.to_string_lossy().replace('\\', "/").to_lowercase(),
            );
            if !want.contains(&rel) {
                self.step(&format!("remove {}", old.display()))?;
                fs::remove_file(&old).map_err(|e| format!("{}: {e}", old.display()))?;
            }
        }
        Ok(())
    }

    /// The shared files replaced by their patches in one journalled commit (install order: the registration last),
    /// then those no installed export needs any more back as they were before this owner.
    fn write_shared(&mut self, built: &Built, back: &[String]) -> Result<(), Stop> {
        let mut files = Vec::new();
        for s in self.game.shared() {
            if let Some((_, b)) = built.iter().find(|(g, _)| *g == s) {
                files.push((s.clone(), fs::read(b).map_err(|e| format!("{b}: {e}"))?));
            }
        }
        if !files.is_empty() {
            self.stages.install(OWNER, &files, &mut *self.log)?;
        }
        if !back.is_empty() {
            let refs: Vec<&str> = back.iter().map(String::as_str).collect();
            self.stages.revert(OWNER, Some(&refs), &mut *self.log)?;
        }
        Ok(())
    }

    /// `--revert`: the exported levels named (none: every one) taken out - every check first (each bundle as the
    /// install wrote it, the ledger's refusals), then the shared patch without them (or every shared file back as it
    /// was before this owner, for the last), their path data, and their folders (the record last). Refused while the
    /// game runs.
    ///
    /// # Errors
    /// Any refusal (nothing written), a failed write, the test hook.
    pub fn revert(&mut self, levels: &[String]) -> Result<(), Stop> {
        refuse_while_game_runs()?;
        self.revert_inner(levels)?;
        let _ = fs::remove_dir_all(join(&self.work, &["_revert"]));
        Ok(())
    }

    fn revert_inner(&mut self, levels: &[String]) -> Result<(), Stop> {
        let have = installed(&self.game)?;
        let gone: Vec<String> = if levels.is_empty() {
            have.iter().map(|(k, _)| k.clone()).collect()
        } else {
            levels.iter().map(|l| l.to_uppercase()).collect()
        };
        for lv in &gone {
            let Some((_, rec)) = have.iter().find(|(k, _)| k == lv) else {
                let mut names: Vec<String> = have.iter().map(|(k, _)| k.clone()).collect();
                names.sort();
                return Err(format!(
                    "{lv}: not an installed exported level ({})",
                    crate::text::py_list_repr(&names)
                ));
            };
            let folder = self.game.folder(lv);
            if let Some(Value::Dict(inst)) = get(rec, "installed") {
                for (name, want) in inst {
                    let p = join(&folder, &[name]);
                    if is_file(&p) && Some(sha(&p)?.as_str()) != want.as_str() {
                        return Err(format!("{p}: changed since the install - nothing reverted"));
                    }
                }
            }
        }
        self.stages.require_revert(OWNER, None, &mut *self.log)?; // refused (nothing written) under a later install
        for lv in &gone {
            let own = ai_owner(lv);
            if !self.stages.installed(&own)?.is_empty() {
                self.stages.require_revert(&own, None, &mut *self.log)?;
            }
        }
        let mut rest: Vec<(String, Value)> = have.iter().filter(|(k, _)| !gone.contains(k)).cloned().collect();
        rest.sort_by(|a, b| a.0.cmp(&b.0));
        if rest.is_empty() {
            self.stages.revert(OWNER, None, &mut *self.log)?; // every shared file as before this owner
        } else {
            let mut stages = BTreeMap::new();
            for (k, v) in &rest {
                let s = self.stage_of(k, v);
                stages.insert(k.clone(), s);
            }
            let drop: BTreeSet<String> = have.iter().map(|(k, _)| k.to_lowercase()).collect();
            let levels: Vec<Value> = rest.into_iter().map(|(_, v)| v).collect();
            let out = join(&self.work, &["_revert"]);
            let built = self.build_shared(&levels, &stages, &drop, &out)?;
            let back = self.restored(&built)?;
            self.write_shared(&built, &back)?;
        }
        for lv in &gone {
            let own = ai_owner(lv);
            if !self.stages.installed(&own)?.is_empty() {
                self.stages.revert(&own, None, &mut *self.log)?; // the path data back as it was (none: removed)
            }
            self.remove_folder(&self.game.folder(lv))?;
            self.say(&format!("{lv}: removed"));
        }
        Ok(())
    }

    /// A level folder removed with its record last, so a kill part way leaves a folder the next revert finishes.
    fn remove_folder(&self, folder: &str) -> Result<(), Stop> {
        let rec = PathBuf::from(join(folder, &[RECORD]));
        for f in walk(Path::new(folder)) {
            if f != rec {
                self.step(&format!("remove {}", f.display()))?;
                fs::remove_file(&f).map_err(|e| format!("{}: {e}", f.display()))?;
            }
        }
        self.step(&format!("remove {}", rec.display()))?;
        if rec.is_file() {
            fs::remove_file(&rec).map_err(|e| format!("{}: {e}", rec.display()))?;
        }
        fs::remove_dir_all(folder).map_err(|e| format!("{folder}: {e}"))
    }

    /// `--status`: one line per installed export (`<LV>: <title> (<mode>, from <base>)`).
    ///
    /// # Errors
    /// A record that cannot be read.
    pub fn status(&mut self) -> Result<(), Stop> {
        for (k, v) in installed(&self.game)? {
            let line = format!(
                "{k}: {} ({}, from {})",
                text(&v, "title"),
                text(&v, "mode"),
                text(&v, "base")
            );
            self.say(&line);
        }
        Ok(())
    }
}

/// `dict[key] = value` of an ordered dict (a new key at the end).
fn set(items: &mut Vec<(String, Value)>, key: &str, v: Value) {
    if let Some(slot) = items.iter_mut().find(|(k, _)| k == key) {
        slot.1 = v;
    } else {
        items.push((key.to_owned(), v));
    }
}

/// The files under a folder (none when it is missing), sorted.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut todo = vec![dir.to_path_buf()];
    while let Some(d) = todo.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                todo.push(p);
            } else {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

/// Python's `ascii()` of a str: its `repr` with every character past `~` escaped (`\xNN`, `\uNNNN`, `\UNNNNNNNN`).
#[must_use]
pub fn ascii_repr(s: &str) -> String {
    let mut out = String::new();
    for c in str_repr(s).chars() {
        let u = u32::from(c);
        if u < 0x80 {
            out.push(c);
        } else if u < 0x100 {
            let _ = write!(out, "\\x{u:02x}");
        } else if u < 0x1_0000 {
            let _ = write!(out, "\\u{u:04x}");
        } else {
            let _ = write!(out, "\\U{u:08x}");
        }
    }
    out
}

/// Refused while the game runs (nothing changed).
///
/// # Errors
/// The game runs.
pub fn refuse_while_game_runs() -> Result<(), Stop> {
    if bc2_gamefs::liveness::game_running() {
        return Err("the game is running - close it first (nothing was changed)".to_owned());
    }
    Ok(())
}
