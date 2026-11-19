//! The levels that live only in the game copy's `game\Package` (round G3): DICE's Vietnam levels (`nam_*`), Project
//! OPFOR BFBC2's Pacific War (WW2) levels converted from Battlefield 1943, and DICE's other Package-only levels (the
//! Bad Company 1 maps, the extra mode layouts such as `mp_005gr`). NavKit's prepare lists only `game\Dist` levels,
//! so these are prepared here, in Rust, without unpacking whole bundles.
//!
//! **Which bundles a level loads** is the game's own registry, `game\Dist\win32\package.mft`: for each level's
//! `level-00.fbrb` and `terrain-00.fbrb`, the bundles in the order the game looks a name up in them
//! (`package:levels/nam_mp_002cq`, then `package:levels/nam_common`, then `game:levels/mp_common`). The first bundle
//! that lists a name decides it: its data, or - for a `*deleted*` marker - nothing (a Vietnam level's markers hide
//! `nam_common`'s other terrains; `nam_common`'s hide `mp_common`'s partitions, as `wake_install.masked_names` says).
//! A level is listed when the registry names it and every bundle of its level stack is in the game copy: Pacific War
//! (an optional subproject) and Vietnam (opt-in owned content) simply are not listed when they are not installed.
//!
//! **Preparing** ([`prepare`]) reads, in memory, only what [`bc2_scene::scene::load`] opens: the level partition and
//! its static groups, then the partitions of the placed blueprints and their collision packfiles (`*_Havok.res`),
//! the second-level blueprints, the packfiles the blueprints' `Mesh` aliases name, and the terrain's height field and
//! water. Each bundle's payload is streamed once per round, the entries kept and the rest passed over; nothing else
//! is decompressed to disk. The files are written under `work\editor\levels\<level>\` ([`cache_dir`]), one folder per
//! bundle of the stack (`00_nam_mp_002cq`, `01_nam_common`, `02_mp_common`) so the scene's own first-wins lookups see
//! them in the game's order, with each placed blueprint's readable XML beside its partition (the collision index
//! counts a partition by its XML, as NavKit's unpack has it), and `editor-level.json` ([`MANIFEST`]): the bundles' sizes
//! and times it was made from and every file written. A copy whose bundles changed, or with a file missing, is
//! prepared again ([`ready`]).
//!
//! **Not `work\levels`:** several suite tests and tools enumerate every folder of `work\levels` against recorded
//! corpora (the cull tree's, the geometry's, the shader tests' goldens), so the editor's copies stay out of it. The
//! folder is a cache in the suite's sense (`bfbc2core.caches`, the project cache `editor`): everything in it is made
//! again from the game copy on its next use, `python -m bfbc2core.caches --list` shows it and `--prune` / `--clear`
//! trim it; the editor itself keeps it under [`CAP_ENV`] (default [`DEFAULT_CAP_MB`] MiB), dropping the least
//! recently opened levels first ([`prune`]).

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use bc2_formats::dbx;
use bc2_formats::fbrb::{self, Header};
use bc2_scene::scene::{self, LoadOptions};
use serde::{Deserialize, Serialize};

use crate::factions::Profile;
use crate::levels::{self, Found};

/// The prepared copy's record, in its folder.
pub const MANIFEST: &str = "editor-level.json";
/// The record's format: a copy written by another format is prepared again.
pub const FORMAT: u32 = 1;
/// The environment variable capping the editor's copies (MiB).
pub const CAP_ENV: &str = "BFBC2_EDITOR_CACHE_MB";
/// The default cap of the editor's copies (MiB).
pub const DEFAULT_CAP_MB: u64 = 2048;

/// Where the editor keeps the Package levels it prepared: `work\editor\levels` (not `work\levels`: see the module).
#[must_use]
pub fn cache_root(opfor: &Path) -> PathBuf {
    opfor.join("work").join("editor").join("levels")
}

/// A Package level's prepared copy: `work\editor\levels\<level>`.
#[must_use]
pub fn cache_dir(opfor: &Path, lv: &str) -> PathBuf {
    cache_root(opfor).join(lv.to_lowercase())
}

// ------------------------------------------------------------------------------------------------ the registry

/// The game's bundle registry (`game\Dist\win32\package.mft`): each source bundle and the bundles it loads, in order.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Registry {
    stacks: HashMap<String, Vec<String>>,
}

impl Registry {
    /// The registry's text read: `BundleSource <ref>` starts a block, each `BundleTarget <ref>` adds to it, a blank
    /// line ends it. References are kept as written (`package:levels/nam_common/level-00.fbrb`).
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let mut stacks: HashMap<String, Vec<String>> = HashMap::new();
        let mut current: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(src) = line.strip_prefix("BundleSource ") {
                let key = src.trim().to_lowercase();
                stacks.entry(key.clone()).or_default();
                current = Some(key);
            } else if let Some(t) = line.strip_prefix("BundleTarget ") {
                if let Some(list) = current.as_ref().and_then(|k| stacks.get_mut(k)) {
                    list.push(t.trim().to_owned());
                }
            } else if line.is_empty() {
                current = None;
            }
        }
        Self { stacks }
    }

    /// The game copy's registry, or `None` when it is missing or unreadable.
    #[must_use]
    pub fn read(game: &Path) -> Option<Self> {
        let data = fs::read(game.join("Dist").join("win32").join("package.mft")).ok()?;
        Some(Self::parse(&String::from_utf8_lossy(&data)))
    }

    /// The bundles a level's `<kind>-00.fbrb` loads (`kind`: `level`, `terrain`), in the game's order.
    #[must_use]
    pub fn stack(&self, lv: &str, kind: &str) -> Option<&[String]> {
        self.stacks
            .get(&format!("game:levels/{}/{kind}-00.fbrb", lv.to_lowercase()))
            .map(Vec::as_slice)
    }
}

/// The file a registry reference names: `game:` is `game\Dist\win32`, `package:` is `game\Package`.
#[must_use]
pub fn bundle_path(game: &Path, reference: &str) -> Option<PathBuf> {
    let (root, rest) = if let Some(r) = reference.strip_prefix("game:") {
        (game.join("Dist").join("win32"), r)
    } else {
        (game.join("Package"), reference.strip_prefix("package:")?)
    };
    fbrb::entry_path(&root, rest).ok()
}

/// A bundle of a level's stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bundle {
    /// The registry's reference (`package:levels/nam_common/level-00.fbrb`).
    pub reference: String,
    /// Its file.
    pub path: PathBuf,
}

impl Bundle {
    /// Its short name: the level folder and file (`nam_common/level-00.fbrb`).
    #[must_use]
    pub fn label(&self) -> String {
        let r = self
            .reference
            .split_once(':')
            .map_or(self.reference.as_str(), |(_, r)| r);
        r.strip_prefix("levels/").unwrap_or(r).to_lowercase()
    }

    /// The folder of the prepared copy for this bundle: its position and its level folder (`01_nam_common`).
    #[must_use]
    pub fn folder(&self, k: usize) -> String {
        let label = self.label();
        let name = label.split('/').next().unwrap_or("bundle");
        format!("{k:02}_{name}")
    }
}

/// A level that lives only in `game\Package`, installed: the bundles its level and terrain load.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLevel {
    /// The level's name, lower case (`nam_mp_002cq`).
    pub lv: String,
    /// The bundles its `level-00.fbrb` loads, in the game's order (all in the game copy).
    pub level: Vec<Bundle>,
    /// The bundles its `terrain-00.fbrb` loads, in order (those in the game copy).
    pub terrain: Vec<Bundle>,
    /// Its content profile, from the bundles it loads: `ww2_common` is Pacific War, `nam_common` Vietnam; otherwise
    /// as [`Profile::of`] names it.
    pub profile: Profile,
}

fn lower_name(p: &Path) -> String {
    p.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_lowercase())
}

fn entries(dir: &Path) -> Vec<PathBuf> {
    fs::read_dir(dir).map_or_else(|_| Vec::new(), |r| r.flatten().map(|e| e.path()).collect())
}

/// The profile of a level stack.
#[must_use]
pub fn profile_of_stack(lv: &str, stack: &[Bundle]) -> Profile {
    let has = |common: &str| stack.iter().any(|b| b.label().starts_with(&format!("{common}/")));
    if has("ww2_common") {
        Profile::Ww2
    } else if has("nam_common") {
        Profile::Vietnam
    } else {
        Profile::of(lv)
    }
}

/// The installed Package-only levels of the game copy at `opfor`, sorted by name: every folder of
/// `game\Package\levels` with a `level-00.fbrb` that `game\Dist\win32\levels` does not have (a Dist level's Package
/// folder is its overlay, prepared with it by NavKit), except the `_common` folders and OPFOR's `opfor_*` bundles,
/// registered in `package.mft` with every bundle of its level stack present. Nothing at all without a registry.
#[must_use]
pub fn package_levels(opfor: &Path) -> Vec<PackageLevel> {
    let game = opfor.join("game");
    let Some(reg) = Registry::read(&game) else {
        return Vec::new();
    };
    let dist: HashSet<String> = entries(&game.join("Dist").join("win32").join("levels"))
        .into_iter()
        .filter(|d| d.join("level-00.fbrb").is_file())
        .map(|d| lower_name(&d))
        .collect();
    let mut out: Vec<PackageLevel> = entries(&game.join("Package").join("levels"))
        .into_iter()
        .filter(|d| d.join("level-00.fbrb").is_file())
        .map(|d| lower_name(&d))
        .filter(|lv| !lv.ends_with("_common") && !lv.starts_with("opfor_") && !dist.contains(lv))
        .filter_map(|lv| {
            let bundles = |kind: &str| -> Option<Vec<Bundle>> {
                reg.stack(&lv, kind)?
                    .iter()
                    .map(|r| {
                        bundle_path(&game, r).map(|path| Bundle {
                            reference: r.clone(),
                            path,
                        })
                    })
                    .collect()
            };
            let level = bundles("level")?;
            if level.is_empty() || !level.iter().all(|b| b.path.is_file()) {
                return None;
            }
            let terrain: Vec<Bundle> = bundles("terrain")
                .unwrap_or_default()
                .into_iter()
                .filter(|b| b.path.is_file())
                .collect();
            Some(PackageLevel {
                profile: profile_of_stack(&lv, &level),
                lv,
                level,
                terrain,
            })
        })
        .collect();
    out.sort_by(|a, b| a.lv.cmp(&b.lv));
    out
}

/// The installed Package-only level `lv`, if it is one.
#[must_use]
pub fn find(opfor: &Path, lv: &str) -> Option<PackageLevel> {
    let lv = lv.to_lowercase();
    // cheap first: a level of game\Dist is never one
    let dist = opfor
        .join("game")
        .join("Dist")
        .join("win32")
        .join("levels")
        .join(&lv)
        .join("level-00.fbrb");
    if dist.is_file() {
        return None;
    }
    package_levels(opfor).into_iter().find(|l| l.lv == lv)
}

// ------------------------------------------------------------------------------------------------ the record

/// A bundle's size and modification time, as the copy was made from it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stamp {
    /// The registry reference.
    pub reference: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its modification time, nanoseconds since 1970.
    pub modified_ns: u64,
}

impl Stamp {
    fn of(b: &Bundle) -> Option<Self> {
        let m = fs::metadata(&b.path).ok()?;
        let ns = m.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_nanos();
        Some(Self {
            reference: b.reference.clone(),
            size: m.len(),
            modified_ns: u64::try_from(ns).unwrap_or(u64::MAX),
        })
    }
}

/// A file of the copy: its path from the copy's folder (`/`-separated) and size.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileRecord {
    /// The path from the copy's folder.
    pub path: String,
    /// Its size.
    pub size: u64,
}

/// What preparing found and read (the progress log's and the benchmarks' numbers).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Stats {
    /// Entries read from the bundles.
    pub entries: usize,
    /// Their bytes.
    pub bytes: u64,
    /// Placed blueprints (the keys collision is looked up by).
    pub placed: usize,
    /// Blueprint partitions written.
    pub partitions: usize,
    /// Collision packfiles written.
    pub packfiles: usize,
    /// Placed blueprints whose collision comes through their `Mesh` alias.
    pub aliases: usize,
    /// Names a `*deleted*` marker hid from a later bundle of the stack.
    pub masked: usize,
}

/// The record of a prepared copy (`editor-level.json`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Prepared {
    /// [`FORMAT`].
    pub format: u32,
    /// The level, lower case.
    pub level: String,
    /// The level partition's name as the bundle spells it (`NAM_MP_002CQ`).
    pub name: String,
    /// The content profile's word.
    pub profile: String,
    /// The bundles it was made from: the level stack, then the terrain stack.
    pub bundles: Vec<Stamp>,
    /// The folders of the level stack's bundles, in order (the scene's bases).
    pub layers: Vec<String>,
    /// The level partition.
    pub partition: String,
    /// The terrain's name (`nam_mp_002r`), when the level names one.
    pub terrain_name: Option<String>,
    /// The height field, when the terrain bundles have it.
    pub heightfield: Option<String>,
    /// The water mesh and its visual water, when the terrain bundles have them.
    pub water: Option<(String, Option<String>)>,
    /// Every file written.
    pub files: Vec<FileRecord>,
    /// What the level opens without, in words.
    pub notes: Vec<String>,
    /// The numbers.
    pub stats: Stats,
    /// How long preparing took.
    pub seconds: f64,
}

fn stamps(level: &PackageLevel) -> Vec<Stamp> {
    level.level.iter().chain(&level.terrain).filter_map(Stamp::of).collect()
}

fn read_record(dir: &Path) -> Result<Prepared, String> {
    let data = fs::read(dir.join(MANIFEST)).map_err(|_| "it is not prepared yet".to_owned())?;
    let p: Prepared = serde_json::from_slice(&data).map_err(|_| "its prepared copy's record is damaged".to_owned())?;
    if p.format != FORMAT {
        return Err("its prepared copy is from another version of the editor".into());
    }
    Ok(p)
}

/// The record of `level`'s copy when it was made from the bundles as they are now (sizes and times), without
/// checking its files ([`ready`] does): the start screen's quick look.
///
/// # Errors
///
/// Why it is not ready, in words.
pub fn recorded(opfor: &Path, level: &PackageLevel) -> Result<Prepared, String> {
    let p = read_record(&cache_dir(opfor, &level.lv))?;
    if p.level != level.lv {
        return Err("its prepared copy is another level's".into());
    }
    if p.bundles != stamps(level) {
        return Err("its bundles changed since it was prepared".into());
    }
    Ok(p)
}

/// The record of `level`'s copy when it can be opened as it is: made from the bundles as they are now, and every
/// file it wrote there with its size.
///
/// # Errors
///
/// Why it has to be prepared (again), in words.
pub fn ready(opfor: &Path, level: &PackageLevel) -> Result<Prepared, String> {
    let p = recorded(opfor, level)?;
    let dir = cache_dir(opfor, &level.lv);
    for f in &p.files {
        let ok = fs::metadata(dir.join(&f.path)).is_ok_and(|m| m.is_file() && m.len() == f.size);
        if !ok {
            return Err(format!("its prepared copy lost {}", f.path));
        }
    }
    Ok(p)
}

// ------------------------------------------------------------------------------------------------ reading bundles

/// What preparing reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// Step `n` of `of` starts.
    Step {
        /// From 1.
        n: usize,
        /// How many.
        of: usize,
        /// What it does.
        what: String,
    },
    /// A line for the log.
    Log(String),
}

const CANCELLED: &str = "cancelled";

/// A reader that stops with an error once `cancel` is set (a bundle's payload is streamed for seconds).
struct Cancellable<'a, R> {
    inner: R,
    cancel: &'a AtomicBool,
}

impl<R: Read> Read for Cancellable<'_, R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(io::Error::other(CANCELLED));
        }
        self.inner.read(buf)
    }
}

impl<R: Seek> Seek for Cancellable<'_, R> {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        self.inner.seek(pos)
    }
}

/// One bundle of a stack: its header, its live names (lower case: the later entry of a name listed twice) and the
/// names it has only as `*deleted*` markers.
struct Layer {
    bundle: Bundle,
    header: Header,
    live: HashMap<String, usize>,
    deleted: HashSet<String>,
}

impl Layer {
    fn open(bundle: &Bundle) -> Result<Self, String> {
        let header = File::open(&bundle.path)
            .map_err(|e| e.to_string())
            .and_then(|mut f| fbrb::read_header(&mut f).map_err(|e| e.to_string()))
            .map_err(|e| {
                format!(
                    "Could not read {} ({e}). The game copy may be damaged.",
                    bundle.path.display()
                )
            })?;
        let mut live = HashMap::new();
        let mut deleted = HashSet::new();
        for (k, e) in header.entries.iter().enumerate() {
            let n = e.name.to_ascii_lowercase();
            if e.is_deleted() {
                deleted.insert(n);
            } else {
                live.insert(n, k);
            }
        }
        deleted.retain(|n| !live.contains_key(n));
        Ok(Self {
            bundle: bundle.clone(),
            header,
            live,
            deleted,
        })
    }

    fn name(&self, entry: usize) -> &str {
        self.header.entries.get(entry).map_or("", |e| e.name.as_str())
    }
}

/// Where a name is in a stack: the bundle (index) and entry; `Err(true)` when a `*deleted*` marker hides it, `Err(false)`
/// when no bundle has it.
fn resolve(layers: &[Layer], name: &str) -> Result<(usize, usize), bool> {
    let name = name.to_ascii_lowercase();
    for (k, l) in layers.iter().enumerate() {
        if let Some(&e) = l.live.get(&name) {
            return Ok((k, e));
        }
        if l.deleted.contains(&name) {
            return Err(true);
        }
    }
    Err(false)
}

/// One bundle's read: its index, the entries asked for, their data or the error, the seconds it took.
type ReadOne = (usize, Vec<usize>, Result<Vec<Vec<u8>>, String>, f64);

/// The data of the entries `wanted` (bundle, entry), each bundle's payload streamed once, the bundles at once.
fn read_entries(
    layers: &[Layer],
    wanted: &BTreeSet<(usize, usize)>,
    cancel: &AtomicBool,
    log: &mut dyn FnMut(Event),
    stats: &mut Stats,
) -> Result<HashMap<(usize, usize), Vec<u8>>, String> {
    let mut by_layer: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for &(l, e) in wanted {
        by_layer.entry(l).or_default().push(e);
    }
    let results: Vec<ReadOne> = std::thread::scope(|s| {
        let handles: Vec<_> = by_layer
            .iter()
            .filter_map(|(&l, idx)| layers.get(l).map(|layer| (l, idx, layer)))
            .map(|(l, idx, layer)| {
                s.spawn(move || {
                    let t = Instant::now();
                    let r = (|| -> Result<Vec<Vec<u8>>, String> {
                        let file = File::open(&layer.bundle.path).map_err(|e| e.to_string())?;
                        let mut r = Cancellable {
                            inner: io::BufReader::with_capacity(1 << 20, file),
                            cancel,
                        };
                        let entries: Vec<&fbrb::Entry> =
                            idx.iter().filter_map(|&e| layer.header.entries.get(e)).collect();
                        fbrb::read_payload(&mut r, &layer.header, &entries).map_err(|e| e.to_string())
                    })();
                    (l, idx.clone(), r, t.elapsed().as_secs_f64())
                })
            })
            .collect();
        handles.into_iter().filter_map(|h| h.join().ok()).collect()
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(CANCELLED.into());
    }
    let mut out = HashMap::new();
    for (l, idx, r, secs) in results {
        let Some(layer) = layers.get(l) else { continue };
        let data = r.map_err(|e| {
            format!(
                "Could not read {} ({e}). The game copy may be damaged.",
                layer.bundle.path.display()
            )
        })?;
        let bytes: u64 = data.iter().map(|d| d.len() as u64).sum();
        stats.entries += data.len();
        stats.bytes += bytes;
        log(Event::Log(format!(
            "  {} entries ({}) from {} in {secs:.1} s",
            group_digits(data.len() as u64),
            megabytes(bytes),
            layer.bundle.label()
        )));
        for (e, d) in idx.into_iter().zip(data) {
            out.insert((l, e), d);
        }
    }
    Ok(out)
}

fn group_digits(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (k, c) in s.chars().enumerate() {
        if k > 0 && (s.len() - k).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn megabytes(bytes: u64) -> String {
    format!("{:.1} MB", bytes as f64 / 1e6)
}

/// `rsplit('/', 1)[0]`.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(p, _)| p)
}

/// The packfile names that index a blueprint `key` (`levelobjects.havok_index`: `<key>_Havok.res`, and
/// `<key>_Havok_Havok.res`, which indexes both `<key>_havok` and `<key>`).
fn packfile_names(key: &str) -> [String; 2] {
    [format!("{key}_havok.res"), format!("{key}_havok_havok.res")]
}

// ------------------------------------------------------------------------------------------------ preparing

/// The copy being written: a folder beside the final one, moved over it at the end.
struct Writer {
    root: PathBuf,
    files: Vec<FileRecord>,
    written: HashSet<String>,
}

impl Writer {
    fn write(&mut self, folder: &str, name: &str, data: &[u8]) -> Result<String, String> {
        let base = self.root.join(folder);
        let path = fbrb::entry_path(&base, name).map_err(|e| format!("{name}: {e}"))?;
        let rel = path
            .strip_prefix(&self.root)
            .map_or_else(|_| name.to_owned(), |r| r.to_string_lossy().replace('\\', "/"));
        if !self.written.insert(rel.to_lowercase()) {
            return Ok(rel);
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).map_err(|e| self.io(dir, &e))?;
        }
        fs::write(&path, data).map_err(|e| self.io(&path, &e))?;
        self.files.push(FileRecord {
            path: rel.clone(),
            size: data.len() as u64,
        });
        Ok(rel)
    }

    fn io(&self, path: &Path, e: &io::Error) -> String {
        format!(
            "Could not write the editor's copy ({}: {e}). Is the disk full? The copy goes in {}.",
            path.display(),
            self.root.parent().map_or_else(String::new, |p| p.display().to_string())
        )
    }
}

/// The names a level partition and its group files need next: the blueprints the level places (with a transform:
/// collision is looked up by them), the partitions `ReferenceObjectData` names (their types), and the terrain's name.
struct Needs {
    placed: BTreeSet<String>,
    referenced: BTreeSet<String>,
    terrain: Option<String>,
    entities: usize,
}

fn needs_of(name: &str, partition: &Path, groups: &[PathBuf]) -> Result<Needs, String> {
    let options = LoadOptions {
        groups: groups.to_vec(),
        geometry: true,
        ..LoadOptions::new(name, partition, Vec::new())
    };
    let s = scene::load(&options).map_err(|e| format!("The level partition could not be read: {e}"))?;
    let mut placed = BTreeSet::new();
    let mut referenced = BTreeSet::new();
    for e in &s.entities {
        let Some(bp) = e.blueprint.as_deref() else { continue };
        if e.matrix.is_some() {
            placed.insert(bp.to_owned());
        }
        if e.short_type == "ReferenceObjectData" {
            referenced.insert(bp.to_owned());
        }
    }
    let terrain = scene::terrain_name(partition).ok().flatten();
    Ok(Needs {
        placed,
        referenced,
        terrain,
        entities: s.entities.len(),
    })
}

/// The second-level blueprint partitions a blueprint partition names (each instance's `Object`).
fn objects_named(data: &[u8]) -> Vec<String> {
    let Ok(tree) = dbx::parse_tree(data) else {
        return Vec::new();
    };
    tree.instances()
        .filter_map(|i| i.child("Object").and_then(|o| o.attr("ref")).map(str::to_owned))
        .filter(|r| r != "null" && r.contains('/'))
        .map(|r| parent(&r).to_lowercase())
        .collect()
}

/// The blueprint a partition's `Mesh` names (`levelobjects.mesh_aliases`: the mesh's partition without `_mesh`).
fn mesh_alias(data: &[u8]) -> Option<String> {
    if !dbx::is_binary(data) {
        return None;
    }
    let r = dbx::field_ref(data, "Mesh").ok()??;
    let r = r.replace("\r\n", "\n").replace('\r', "\n");
    if r.is_empty() || r == "null" {
        return None;
    }
    let mesh = parent(&r).to_lowercase();
    Some(mesh.strip_suffix("_mesh").map_or(mesh.clone(), str::to_owned))
}

/// The entry of the first bundle of `layers` holding a file named `file` (any folder, any case): the first such entry
/// of that bundle, or its last with `last` (`terrain.find` / `water.find_files`); `None` when no bundle has one or a
/// `*deleted*` marker of that name comes first.
fn find_file(layers: &[Layer], file: &str, last: bool) -> Option<(usize, usize)> {
    let file = file.to_ascii_lowercase();
    let base = |n: &str| n.rsplit(['/', '\\']).next().unwrap_or(n).to_ascii_lowercase();
    for (k, l) in layers.iter().enumerate() {
        let live: Vec<usize> = l
            .header
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| !e.is_deleted() && base(&e.name) == file)
            .map(|(i, _)| i)
            .collect();
        let found = if last { live.last() } else { live.first() };
        if let Some(&i) = found {
            return Some((k, i));
        }
        if l.header.entries.iter().any(|e| e.is_deleted() && base(&e.name) == file) {
            return None;
        }
    }
    None
}

/// Prepares a Package level: reads what the scene opens from its bundles and writes it to [`cache_dir`] with its
/// record, reporting its steps to `report`; stops (and leaves nothing) once `cancel` is set.
///
/// # Errors
///
/// Why it stopped, in plain words: a bundle that cannot be read, a disk that cannot be written, cancelled.
pub fn prepare(
    opfor: &Path,
    level: &PackageLevel,
    report: &mut dyn FnMut(Event),
    cancel: &AtomicBool,
) -> Result<Prepared, String> {
    let t0 = Instant::now();
    let lv = level.lv.clone();
    let final_dir = cache_dir(opfor, &lv);
    let temp = cache_root(opfor).join(format!("{lv}.tmp-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp);
    let mut w = Writer {
        root: temp.clone(),
        files: Vec::new(),
        written: HashSet::new(),
    };
    let result = (|| -> Result<Prepared, String> {
        const STEPS: usize = 4;
        let mut stats = Stats::default();
        let mut notes = Vec::new();
        let stop = || -> Result<(), String> {
            if cancel.load(Ordering::Relaxed) {
                Err(CANCELLED.into())
            } else {
                Ok(())
            }
        };
        report(Event::Step {
            n: 1,
            of: STEPS,
            what: "Reading the level partition and its static groups".into(),
        });
        report(Event::Log(format!(
            "  {} loads {} (game\\Dist\\win32\\package.mft)",
            lv.to_uppercase(),
            level.level.iter().map(Bundle::label).collect::<Vec<_>>().join(", ")
        )));
        let layers: Vec<Layer> = level.level.iter().map(Layer::open).collect::<Result<_, _>>()?;
        let folders: Vec<String> = level.level.iter().enumerate().map(|(k, b)| b.folder(k)).collect();
        let folder = |l: usize| folders.get(l).cloned().unwrap_or_default();
        let part_name = format!("levels/{lv}.dbx");
        let Ok(part) = resolve(&layers, &part_name) else {
            return Err(format!(
                "{} has no level partition ({part_name}) in the bundles it loads - the level's install looks \
                 incomplete.",
                lv.to_uppercase()
            ));
        };
        // the static groups: levels/<lv>/<name>_HavokAsset.dbx in any bundle, the first that has the name
        let prefix = format!("levels/{lv}/");
        let mut group_names: BTreeSet<String> = BTreeSet::new();
        for l in &layers {
            for n in l.live.keys() {
                if let Some(rest) = n.strip_prefix(&prefix)
                    && !rest.contains('/')
                    && rest.ends_with("_havokasset.dbx")
                {
                    group_names.insert(n.clone());
                }
            }
        }
        let mut wanted: BTreeSet<(usize, usize)> = BTreeSet::from([part]);
        for g in &group_names {
            if let Ok(at) = resolve(&layers, g) {
                wanted.insert(at);
            }
        }
        let data = read_entries(&layers, &wanted, cancel, report, &mut stats)?;
        let mut partition_rel = String::new();
        let mut groups = Vec::new();
        for (&(l, e), d) in &data {
            let rel = w.write(&folder(l), layers.get(l).map_or("", |x| x.name(e)), d)?;
            if (l, e) == part {
                partition_rel = rel;
            } else {
                groups.push(temp.join(&rel));
            }
        }
        drop(data);
        groups.sort();
        let name = Path::new(&partition_rel)
            .file_stem()
            .map_or_else(|| lv.to_uppercase(), |s| s.to_string_lossy().into_owned());
        let needs = needs_of(&name, &temp.join(&partition_rel), &groups)?;
        report(Event::Log(format!(
            "  {} entities, {} static groups, {} blueprints placed",
            group_digits(needs.entities as u64),
            groups.len(),
            group_digits(needs.placed.len() as u64)
        )));
        stats.placed = needs.placed.len();
        stop()?;

        report(Event::Step {
            n: 2,
            of: STEPS,
            what: "Reading the objects' blueprints and collision".into(),
        });
        // round two: every blueprint the level names and the placed ones' packfiles
        let mut masked: BTreeSet<String> = BTreeSet::new();
        let mut want = |names: &mut dyn Iterator<Item = String>, wanted: &mut BTreeSet<(usize, usize)>| {
            for n in names {
                match resolve(&layers, &n) {
                    Ok(at) => {
                        wanted.insert(at);
                    }
                    Err(true) => {
                        masked.insert(n);
                    }
                    Err(false) => {}
                }
            }
        };
        let mut wanted = BTreeSet::new();
        want(
            &mut needs.referenced.iter().chain(&needs.placed).map(|k| format!("{k}.dbx")),
            &mut wanted,
        );
        want(&mut needs.placed.iter().flat_map(|k| packfile_names(k)), &mut wanted);
        let data = read_entries(&layers, &wanted, cancel, report, &mut stats)?;
        let mut second: BTreeSet<String> = BTreeSet::new();
        let mut alias_targets: BTreeSet<String> = BTreeSet::new();
        let has_packfile = |k: &str| packfile_names(k).iter().any(|n| resolve(&layers, n).is_ok());
        for (&(l, e), d) in &data {
            let entry = layers.get(l).map_or("", |x| x.name(e));
            let low = entry.to_ascii_lowercase();
            let rel = w.write(&folder(l), entry, d)?;
            if let Some(key) = low.strip_suffix(".dbx") {
                stats.partitions += 1;
                if needs.referenced.contains(key) {
                    second.extend(objects_named(d));
                }
                if needs.placed.contains(key) {
                    // its readable XML beside it: the collision index counts a partition by its XML (NavKit's unpack)
                    if let Ok(xml) = dbx::to_xml(d) {
                        let xml_rel = format!("{}.xml", rel.get(..rel.len().saturating_sub(4)).unwrap_or(&rel));
                        let path = temp.join(&xml_rel);
                        fs::write(&path, xml.as_bytes()).map_err(|e| w.io(&path, &e))?;
                        w.files.push(FileRecord {
                            path: xml_rel,
                            size: xml.len() as u64,
                        });
                    }
                    if !has_packfile(key)
                        && let Some(t) = mesh_alias(d)
                    {
                        stats.aliases += 1;
                        alias_targets.insert(t);
                    }
                }
            } else {
                stats.packfiles += 1;
            }
        }
        drop(data);
        stop()?;
        // round three: the second-level blueprints and the aliases' packfiles
        let mut wanted = BTreeSet::new();
        want(
            &mut second
                .iter()
                .filter(|q| !needs.referenced.contains(*q) && !needs.placed.contains(*q))
                .map(|q| format!("{q}.dbx")),
            &mut wanted,
        );
        want(&mut alias_targets.iter().flat_map(|k| packfile_names(k)), &mut wanted);
        let data = read_entries(&layers, &wanted, cancel, report, &mut stats)?;
        for (&(l, e), d) in &data {
            let entry = layers.get(l).map_or("", |x| x.name(e));
            w.write(&folder(l), entry, d)?;
            if entry.to_ascii_lowercase().ends_with(".dbx") {
                stats.partitions += 1;
            } else {
                stats.packfiles += 1;
            }
        }
        drop(data);
        stats.masked = masked.len();
        report(Event::Log(format!(
            "  {} blueprint partitions, {} collision packfiles ({} through Mesh aliases); {} names hidden by \
             *deleted* markers",
            group_digits(stats.partitions as u64),
            group_digits(stats.packfiles as u64),
            stats.aliases,
            stats.masked
        )));
        stop()?;

        report(Event::Step {
            n: 3,
            of: STEPS,
            what: "Reading the terrain and water".into(),
        });
        let tname = needs.terrain.clone().unwrap_or_else(|| lv.clone());
        let mut heightfield = None;
        let mut water = None;
        if level.terrain.is_empty() {
            notes.push("Terrain not shown: the level has no terrain bundle.".to_owned());
        } else {
            let tlayers: Vec<Layer> = level.terrain.iter().map(Layer::open).collect::<Result<_, _>>()?;
            let hf = find_file(&tlayers, &format!("{tname}.heightfield-000.res"), false);
            let mesh = find_file(&tlayers, &format!("{tname}.water.mesh.res"), true);
            let visual = find_file(&tlayers, &format!("{tname}.water.res"), true);
            let wanted: BTreeSet<(usize, usize)> = [hf, mesh, visual].into_iter().flatten().collect();
            let data = read_entries(&tlayers, &wanted, cancel, report, &mut stats)?;
            let mut put = |at: Option<(usize, usize)>| -> Result<Option<String>, String> {
                let Some((l, e)) = at else { return Ok(None) };
                let (Some(layer), Some(d)) = (tlayers.get(l), data.get(&(l, e))) else {
                    return Ok(None);
                };
                w.write("terrain", layer.name(e), d).map(Some)
            };
            heightfield = put(hf)?;
            let mesh_rel = put(mesh)?;
            let visual_rel = put(visual)?;
            water = mesh_rel.map(|m| (m, visual_rel));
            if heightfield.is_none() {
                notes.push(format!(
                    "Terrain not shown: the level's terrain bundles have no {tname} height field."
                ));
            }
            report(Event::Log(format!(
                "  terrain {tname}: height field {}, water {}",
                if heightfield.is_some() { "read" } else { "none" },
                if water.is_some() { "read" } else { "none" }
            )));
        }
        stop()?;

        report(Event::Step {
            n: 4,
            of: STEPS,
            what: "Saving the editor's copy".into(),
        });
        let record = Prepared {
            format: FORMAT,
            level: lv.clone(),
            name,
            profile: level.profile.word().to_owned(),
            bundles: stamps(level),
            layers: folders.clone(),
            partition: partition_rel,
            terrain_name: needs.terrain.clone(),
            heightfield,
            water,
            files: w.files.clone(),
            notes,
            stats,
            seconds: t0.elapsed().as_secs_f64(),
        };
        let json = serde_json::to_vec_pretty(&record).map_err(|e| e.to_string())?;
        let path = temp.join(MANIFEST);
        fs::write(&path, json).map_err(|e| w.io(&path, &e))?;
        stop()?;
        if final_dir.exists() {
            fs::remove_dir_all(&final_dir).map_err(|e| w.io(&final_dir, &e))?;
        }
        fs::rename(&temp, &final_dir).map_err(|e| w.io(&final_dir, &e))?;
        let total: u64 = record.files.iter().map(|f| f.size).sum();
        report(Event::Log(format!(
            "  {} files ({}) in work\\editor\\levels\\{lv}, {:.1} s",
            group_digits(record.files.len() as u64),
            megabytes(total),
            record.seconds
        )));
        Ok(record)
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(&temp);
    }
    match result {
        Err(e) if e == CANCELLED || cancel.load(Ordering::Relaxed) => {
            Err(format!("Preparing {} was cancelled.", lv.to_uppercase()))
        }
        Ok(p) => {
            prune(opfor, &lv);
            Ok(p)
        }
        Err(e) => Err(e),
    }
}

// ------------------------------------------------------------------------------------------------ opening

/// The scene options for a prepared Package level (as [`levels::load_options`] builds them for NavKit's levels): the
/// copy's level partition, its static groups, its bundles' folders as the bases in the game's order, the terrain and
/// the water. Marks the copy used now (the least recently used copies are pruned first).
///
/// # Errors
///
/// It is not prepared, or its copy is not current.
pub fn load_options(opfor: &Path, level: &PackageLevel) -> Result<Found, String> {
    let p = ready(opfor, level).map_err(|why| {
        format!(
            "{} cannot be opened yet ({why}): open it from the start screen, which prepares it first",
            level.lv
        )
    })?;
    let dir = cache_dir(opfor, &level.lv);
    if let Ok(f) = File::options().write(true).open(dir.join(MANIFEST)) {
        let _ = f.set_modified(SystemTime::now());
    }
    let bases: Vec<PathBuf> = p.layers.iter().map(|l| dir.join(l)).collect();
    // the static groups: each group file once, in the bundles' order (a level's own come first)
    let mut groups: Vec<PathBuf> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for b in &bases {
        for sub in ["levels", "Levels"] {
            let folder = entries(&b.join(sub))
                .into_iter()
                .find(|d| lower_name(d) == level.lv && d.is_dir());
            let Some(folder) = folder else { continue };
            for g in entries(&folder) {
                let n = lower_name(&g);
                if n.ends_with("_havokasset.dbx") && seen.insert(n) {
                    groups.push(g);
                }
            }
            break;
        }
    }
    let options = LoadOptions {
        groups,
        geometry: true,
        heightfield: p.heightfield.as_ref().map(|h| dir.join(h)),
        terrain_stride: 2,
        water: p
            .water
            .as_ref()
            .map(|(m, v)| (dir.join(m), v.as_ref().map(|v| dir.join(v)))),
        alias_cache: levels::alias_cache(),
        ..LoadOptions::new(p.name.clone(), dir.join(&p.partition), bases)
    };
    Ok(Found {
        options,
        notes: p.notes,
    })
}

// ------------------------------------------------------------------------------------------------ the cap

/// The cap in bytes ([`CAP_ENV`], MiB; [`DEFAULT_CAP_MB`] when unset or not a number).
#[must_use]
pub fn cap_bytes() -> u64 {
    std::env::var(CAP_ENV)
        .ok()
        .and_then(|v| v.trim().parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map_or(DEFAULT_CAP_MB << 20, |mb| (mb * 1_048_576.0) as u64)
}

/// Keeps the editor's copies under [`cap_bytes`]: the least recently opened levels' copies are deleted first, never
/// `keep`'s; a temporary folder left by a run that stopped (`<level>.tmp-<process>`, an hour old) goes too. Never fails
/// (a copy that cannot be deleted stays).
pub fn prune(opfor: &Path, keep: &str) {
    let root = cache_root(opfor);
    let mut copies: Vec<(SystemTime, u64, PathBuf)> = Vec::new();
    let mut total = 0u64;
    for d in entries(&root) {
        let name = lower_name(&d);
        if !d.is_dir() {
            continue;
        }
        if name.contains(".tmp-") {
            let old = fs::metadata(&d)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok())
                .is_some_and(|age| age.as_secs() > 3600);
            if old {
                let _ = fs::remove_dir_all(&d);
            }
            continue;
        }
        let size: u64 = read_record(&d).map_or(0, |p| p.files.iter().map(|f| f.size).sum());
        let used = fs::metadata(d.join(MANIFEST))
            .and_then(|m| m.modified())
            .unwrap_or(UNIX_EPOCH);
        total += size;
        if name != keep.to_lowercase() {
            copies.push((used, size, d));
        }
    }
    copies.sort();
    let cap = cap_bytes();
    for (_, size, d) in copies {
        if total <= cap {
            break;
        }
        if fs::remove_dir_all(&d).is_ok() {
            total = total.saturating_sub(size);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn registry_and_paths() {
        let r = Registry::parse(
            "BundleSource game:levels/nam_x/level-00.fbrb\r\nBundleTarget package:levels/nam_x/level-00.fbrb\r\n\
             BundleTarget package:levels/nam_common/level-00.fbrb\r\nBundleTarget game:levels/mp_common/level-00.fbrb\r\n\
             \r\nBundleSource game:levels/nam_x/terrain-00.fbrb\r\nBundleTarget package:levels/nam_common/terrain-00.fbrb\r\n",
        );
        assert_eq!(r.stack("NAM_X", "level").unwrap().len(), 3);
        assert_eq!(r.stack("nam_x", "terrain").unwrap().len(), 1);
        assert!(r.stack("nam_y", "level").is_none());
        let g = Path::new(r"C:\g");
        assert_eq!(
            bundle_path(g, "game:levels/mp_common/level-00.fbrb").unwrap(),
            Path::new(r"C:\g\Dist\win32\levels\mp_common\level-00.fbrb")
        );
        assert_eq!(
            bundle_path(g, "package:levels/nam_common/level-00.fbrb").unwrap(),
            Path::new(r"C:\g\Package\levels\nam_common\level-00.fbrb")
        );
        assert!(bundle_path(g, "other:x").is_none());
        let b = Bundle {
            reference: "package:levels/nam_common/level-00.fbrb".into(),
            path: PathBuf::new(),
        };
        assert_eq!(b.label(), "nam_common/level-00.fbrb");
        assert_eq!(b.folder(1), "01_nam_common");
        assert_eq!(profile_of_stack("anything", std::slice::from_ref(&b)), Profile::Vietnam);
        assert_eq!(packfile_names("objects/a")[1], "objects/a_havok_havok.res");
        assert_eq!(group_digits(1_234_567), "1,234,567");
    }
}
