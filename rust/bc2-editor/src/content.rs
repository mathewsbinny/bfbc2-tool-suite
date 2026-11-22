//! The asset browser's catalogue ("Place Objects", round E2b; the user's refinement): every placeable map object of
//! the level's content set (BFBC1, BFBC2, Vietnam, WW2), not only the open level's.
//!
//! - **The level's placements** first (from its partition: each `ReferenceObjectData`'s `ReferencedObject`, with how
//!   many times; their references known).
//! - **The level's folders on disk** (its prepared bundles and the shared ones NavKit or G3 unpacked): each
//!   blueprint read, its object's type and whether it breaks up.
//! - **Every bundle of the content set in the game copy** (`game\Dist` and `game\Package`: the set's levels and its
//!   commons - `mp_common` / `sp_common`, `nam_common`, `ww2_common`): only the bundles' headers are read (the names
//!   of their blueprints, `<name>\<name>.dbx`), so the whole set is listed in about a second; an entry is resolved
//!   (its partition read from its bundle: the primary instance's guid and type) when it is placed ([`resolve`]).
//!   An entry outside the open level's bundles says so: the build adds its bundle (a later round).
//!
//! Categories come from the blueprint's data where it was read ([`Category::of_blueprint`]: its object's type, its
//! health states) and from its path otherwise. Thumbnails: a type icon until rendered ones exist; the renderer
//! (round R5's `bc2-render`) writes them to [`thumbnail_file`] and the browser shows a file there instead of the
//! icon - the hook for those rounds. The safety checks of `EDITOR_ARCHITECTURE.md` 3.3 (a cull shape somewhere,
//! collision, references that resolve) belong to a later round; until then an entry the level does not place says so.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use bc2_formats::dbx;
use bc2_scene::scene::Scene;

use crate::error::EditorError;
use crate::levels::ContentSet;
use crate::workspace::Category;

/// A placeable blueprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The reference a placement takes (`objects/props/crate/crate/<guid>`, lower case as the partitions write it);
    /// empty until [`resolve`]d for an entry listed from a bundle's header.
    pub reference: String,
    /// The blueprint's path (`objects/props/crate/crate`).
    pub path: String,
    /// Its name (the last folder).
    pub name: String,
    /// Its category.
    pub category: Category,
    /// The type of the blueprint's object (`StaticModelEntityData` ...), when known.
    pub object_type: Option<String>,
    /// Whether it breaks up (health states, part links).
    pub destructible: bool,
    /// How many times the level places it.
    pub used: usize,
    /// Where it comes from: `This level`, a folder (`mp_common` ...), a bundle's level (`mp_012`).
    pub source: String,
    /// Why it cannot be placed as an object, or `None`.
    pub why_not: Option<&'static str>,
    /// Listed from a bundle's header: the bundle and the partition's name in it (read when placed).
    pub bundle: Option<(PathBuf, String)>,
    /// In the open level's own or shared bundles (else the build has to add its bundle).
    pub in_stack: bool,
}

impl Entry {
    /// Whether a search matches it: every word in its name, path, category, source or object type, ignoring case.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!(
            "{} {} {} {} {}",
            self.name,
            self.path,
            self.category.name(),
            self.source,
            self.object_type.as_deref().unwrap_or("")
        )
        .to_lowercase();
        query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
    }
}

/// Why a path cannot be placed as an object, from where it lies (vehicles go through spawners).
fn why_not_by_path(path: &str) -> Option<&'static str> {
    path.starts_with("vehicles/")
        .then_some("Vehicles are placed through a vehicle spawner (the Gameplay workspace's tools, a later round)")
}

/// The level's own placements: one entry per blueprint, the most placed first.
#[must_use]
pub fn level_entries(scene: &Scene) -> Vec<Entry> {
    let Some((_, data)) = scene.file(0) else {
        return Vec::new();
    };
    let Ok(tree) = dbx::parse_tree(data) else {
        return Vec::new();
    };
    // what the scene learnt of each blueprint (its object's type, destructible)
    let info: HashMap<&str, (&Option<String>, bool)> = scene
        .entities
        .iter()
        .filter_map(|e| Some((e.blueprint.as_deref()?, (&e.blueprint_type, e.destructible))))
        .collect();
    let mut by: BTreeMap<String, usize> = BTreeMap::new();
    for i in tree.instances() {
        if !i.attr("type").is_some_and(|t| t.ends_with(".ReferenceObjectData")) {
            continue;
        }
        if let Some(r) = i.child("ReferencedObject").and_then(|c| c.attr("ref"))
            && r.contains('/')
        {
            *by.entry(r.to_lowercase()).or_default() += 1;
        }
    }
    let mut out: Vec<Entry> = by
        .into_iter()
        .map(|(reference, used)| {
            let path = crate::edit::blueprint_path(&reference);
            let (bt, destructible) = info
                .get(path.as_str())
                .map_or((None, false), |(t, d)| ((*t).clone(), *d));
            Entry {
                name: bc2_levelwrite::ops::blueprint_name(&path).to_owned(),
                category: Category::of_blueprint(&path, bt.as_deref(), destructible),
                object_type: bt,
                destructible,
                used,
                source: "This level".into(),
                why_not: None,
                path,
                reference,
                bundle: None,
                in_stack: true,
            }
        })
        .collect();
    out.sort_by(|a, b| b.used.cmp(&a.used).then(a.name.cmp(&b.name)));
    out
}

/// The top folders blueprints are looked for in.
const TOPS: [&str; 4] = ["objects", "vehicles", "vegetation", "fx"];

fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map_or_else(|_| Vec::new(), |r| r.flatten().map(|e| e.path()).collect())
}

/// Whether a partition's path (`objects/props/crate/crate`) is a blueprint's: under a top folder, its name its
/// folder's (the game's layout of a blueprint and its parts).
#[must_use]
pub fn is_blueprint_path(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    match parts.as_slice() {
        [top, .., folder, name] => TOPS.contains(top) && folder == name,
        _ => false,
    }
}

/// The blueprint files under a folder: `<name>\<name>.dbx`.
fn blueprint_files(dir: &Path, out: &mut Vec<PathBuf>, depth: usize) {
    if depth > 6 {
        return;
    }
    let name = dir.file_name().map(|n| n.to_string_lossy().to_lowercase());
    for p in entries(dir) {
        if p.is_dir() {
            blueprint_files(&p, out, depth + 1);
        } else if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("dbx"))
            && p.file_stem().map(|s| s.to_string_lossy().to_lowercase()) == name
        {
            out.push(p);
        }
    }
}

/// A blueprint partition's facts: the reference of its primary instance, its object's type, whether it breaks up,
/// and whether it can be placed as an object.
fn blueprint_facts(data: &[u8], path: &str) -> Option<(String, Option<String>, bool, Option<&'static str>)> {
    let tree = dbx::parse_tree(data).ok()?;
    let part = tree.partition()?;
    let primary = part.attr("primaryInstance")?.to_lowercase();
    let mut types: HashMap<String, (String, Option<String>)> = HashMap::new();
    let mut destructible = false;
    for i in tree.instances() {
        let g = i.attr("guid").unwrap_or("").to_lowercase();
        let t = i.attr("type").unwrap_or("").to_owned();
        let object = i.child("Object").and_then(|o| o.attr("ref")).map(str::to_lowercase);
        destructible |= ["HealthStates", "PartLinks"]
            .iter()
            .any(|n| i.child(n).is_some_and(|c| c.children().next().is_some()));
        types.insert(g, (t, object));
    }
    let (ptype, object) = types.get(&primary)?.clone();
    let short = |t: &str| t.rsplit('.').next().unwrap_or(t).to_owned();
    let object_type = object
        .and_then(|o| types.get(&o).map(|(t, _)| short(t)))
        .or_else(|| Some(short(&ptype)));
    let why_not = if ptype.ends_with("ObjectBlueprint") {
        None
    } else {
        Some(
            "Not an object blueprint: vehicles, effects and sounds are placed through their spawners and emitters \
             (a later round)",
        )
    };
    Some((format!("{path}/{primary}"), object_type, destructible, why_not))
}

/// One blueprint file on disk read: its entry.
fn read_blueprint(file: &Path, base: &Path, source: &str) -> Option<Entry> {
    let data = std::fs::read(file).ok()?;
    let rel = file.strip_prefix(base).ok()?.with_extension("");
    let path = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("/");
    let (reference, object_type, destructible, why_not) = blueprint_facts(&data, &path)?;
    Some(Entry {
        reference,
        name: bc2_levelwrite::ops::blueprint_name(&path).to_owned(),
        category: Category::of_blueprint(&path, object_type.as_deref(), destructible),
        object_type,
        destructible,
        used: 0,
        source: source.to_owned(),
        why_not,
        path,
        bundle: None,
        in_stack: true,
    })
}

/// The blueprints of the level's folders on disk (its own, then its shared bundles'), each once, leaving out those
/// in `seen` (which grows).
#[must_use]
pub(crate) fn base_entries(bases: &[PathBuf], seen: &mut HashSet<String>) -> Vec<Entry> {
    let mut out = Vec::new();
    for base in bases {
        let source = base
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        // a level's own folders are "pkg" and "dist": name them by their level
        let source = match source.as_str() {
            "pkg" | "dist" => format!(
                "{} ({source})",
                base.parent()
                    .and_then(|p| p.file_name())
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            ),
            _ => source,
        };
        let mut files = Vec::new();
        for top in entries(base) {
            let n = top
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().to_lowercase());
            if top.is_dir() && TOPS.contains(&n.as_str()) {
                blueprint_files(&top, &mut files, 0);
            }
        }
        files.sort();
        for f in files {
            if let Some(e) = read_blueprint(&f, base, &source)
                && seen.insert(e.path.clone())
            {
                out.push(e);
            }
        }
    }
    out
}

/// The shared folders a content set's levels load.
#[must_use]
pub fn commons(set: ContentSet) -> &'static [&'static str] {
    match set {
        ContentSet::Bfbc2 => &["mp_common", "sp_common"],
        ContentSet::Bfbc1 => &["mp_common"],
        ContentSet::Vietnam => &["nam_common", "mp_common"],
        ContentSet::Ww2 => &["ww2_common"],
    }
}

/// The game copy's bundles of a content set: the set's levels' folders and its commons, in `game\Dist` and
/// `game\Package`; each with its folder's name and whether it is the open level's (its own or a common).
#[must_use]
pub fn set_bundles(game: &Path, set: ContentSet, levels: &[String], own: &str) -> Vec<(PathBuf, String, bool)> {
    let tops = [
        game.join("Dist").join("win32").join("levels"),
        game.join("Package").join("levels"),
    ];
    let mut folders: Vec<(String, bool)> = commons(set).iter().map(|c| ((*c).to_owned(), true)).collect();
    folders.extend(levels.iter().map(|l| (l.to_lowercase(), l.eq_ignore_ascii_case(own))));
    let mut out = Vec::new();
    for (folder, mine) in folders {
        for top in &tops {
            let mut files: Vec<PathBuf> = entries(&top.join(&folder))
                .into_iter()
                .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("fbrb")))
                .collect();
            files.sort();
            out.extend(files.into_iter().map(|f| (f, folder.clone(), mine)));
        }
    }
    out
}

/// Every blueprint the bundles' headers name, each once (the first bundle naming it), leaving out those in `seen`.
#[must_use]
pub(crate) fn bundle_entries(bundles: &[(PathBuf, String, bool)], seen: &mut HashSet<String>) -> Vec<Entry> {
    let mut out = Vec::new();
    for (file, folder, mine) in bundles {
        let Ok(f) = std::fs::File::open(file) else { continue };
        let Ok(h) = bc2_formats::fbrb::read_header(&mut std::io::BufReader::new(f)) else {
            continue;
        };
        for e in &h.entries {
            if e.is_deleted() || e.size == 0 {
                continue;
            }
            let lower = e.name.to_lowercase();
            let Some(path) = lower.strip_suffix(".dbx") else {
                continue;
            };
            if !is_blueprint_path(path) || !seen.insert(path.to_owned()) {
                continue;
            }
            out.push(Entry {
                reference: String::new(),
                name: bc2_levelwrite::ops::blueprint_name(path).to_owned(),
                category: Category::of_blueprint(path, None, false),
                object_type: None,
                destructible: false,
                used: 0,
                source: folder.clone(),
                why_not: why_not_by_path(path),
                path: path.to_owned(),
                bundle: Some((file.clone(), e.name.clone())),
                in_stack: *mine,
            });
        }
    }
    out
}

/// Reads an entry listed from a bundle's header: its partition from its bundle (the payload streamed until it),
/// its reference, object type and category. Nothing to do for an entry already read.
///
/// # Errors
/// The bundle does not read, or the partition is not a blueprint.
pub fn resolve(e: &mut Entry) -> Result<(), EditorError> {
    if !e.reference.is_empty() {
        return Ok(());
    }
    let Some((file, name)) = e.bundle.clone() else {
        return Err(EditorError::Level(format!("{}: no bundle to read it from", e.path)));
    };
    let f = std::fs::File::open(&file).map_err(|x| EditorError::Level(format!("{}: {x}", file.display())))?;
    let data = bc2_formats::fbrb::read(&mut std::io::BufReader::new(f), &[name.as_str()])
        .map_err(|x| EditorError::Level(format!("{}: {name}: {x}", file.display())))?
        .into_iter()
        .next()
        .ok_or_else(|| EditorError::Level(format!("{}: {name} is not in it", file.display())))?;
    let (reference, object_type, destructible, why_not) = blueprint_facts(&data, &e.path)
        .ok_or_else(|| EditorError::Level(format!("{} is not a blueprint partition", e.path)))?;
    e.reference = reference;
    e.category = Category::of_blueprint(&e.path, object_type.as_deref(), destructible);
    e.object_type = object_type;
    e.destructible = destructible;
    e.why_not = e.why_not.or(why_not);
    Ok(())
}

/// Where a rendered thumbnail of a blueprint is kept: `work\editor\thumbs\<set>\<path with / as _>.png` (the hook
/// for the renderer's rounds; the browser shows a type icon while there is none).
#[must_use]
pub fn thumbnail_file(opfor: &Path, set: ContentSet, path: &str) -> PathBuf {
    opfor
        .join("work")
        .join("editor")
        .join("thumbs")
        .join(set.badge().to_lowercase())
        .join(format!("{}.png", path.replace('/', "_")))
}

/// Where the catalogue looks in the game copy.
#[derive(Debug, Clone)]
pub struct GameSource {
    /// Project OPFOR BFBC2's folder.
    pub opfor: PathBuf,
    /// The open level's content set.
    pub set: ContentSet,
    /// The set's levels.
    pub levels: Vec<String>,
    /// The open level.
    pub own: String,
}

/// The catalogue: the level's placements and, once read, its folders' and its content set's blueprints.
#[derive(Debug, Default)]
pub struct Catalogue {
    /// Every entry: the level's first.
    pub entries: Vec<Entry>,
    /// The folders and bundles are still being read.
    pub reading: bool,
    /// The content set (thumbnails).
    pub source: Option<GameSource>,
    rx: Option<crate::task::Task<Vec<Entry>>>,
}

impl Catalogue {
    /// The level's placements at once; its folders' and its content set's blueprints on a thread.
    #[must_use]
    pub fn start(scene: &Scene, bases: Vec<PathBuf>, game: Option<GameSource>) -> Self {
        let entries = level_entries(scene);
        let mut seen: HashSet<String> = entries.iter().map(|e| e.path.clone()).collect();
        let rx = (!bases.is_empty() || game.is_some()).then(|| {
            let g = game.clone();
            crate::task::Task::spawn_value("Reading the content browser's blueprints", move || {
                let mut out = base_entries(&bases, &mut seen);
                if let Some(g) = g {
                    let bundles = set_bundles(&g.opfor.join("game"), g.set, &g.levels, &g.own);
                    out.extend(bundle_entries(&bundles, &mut seen));
                }
                out.sort_by(|a, b| {
                    b.in_stack
                        .cmp(&a.in_stack)
                        .then(a.category.cmp(&b.category))
                        .then(a.name.cmp(&b.name))
                });
                out
            })
        });
        Self {
            entries,
            reading: rx.is_some(),
            source: game,
            rx,
        }
    }

    /// Takes the folders' entries when they are read.
    pub fn poll(&mut self) {
        if let Some(r) = self.rx.as_ref().and_then(crate::task::Task::poll) {
            // a worker that stopped without its entries leaves the level's own
            if let Ok(more) = r {
                self.entries.extend(more);
            }
            self.rx = None;
            self.reading = false;
        }
    }

    /// Waits for the folders' entries (tests), at most `limit`.
    pub fn wait(&mut self, limit: std::time::Duration) {
        let t = std::time::Instant::now();
        while self.reading && t.elapsed() < limit {
            self.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// The entries a search and a category show.
    #[must_use]
    pub fn shown(&self, query: &str, category: Option<Category>) -> Vec<&Entry> {
        self.entries
            .iter()
            .filter(|e| category.is_none_or(|c| e.category == c) && e.matches(query))
            .collect()
    }

    /// The entry of a blueprint path.
    #[must_use]
    pub fn by_path(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.path == path)
    }

    /// An entry resolved ([`resolve`]: read from its bundle when listed from a header), kept resolved.
    ///
    /// # Errors
    /// As [`resolve`]; no such entry.
    pub fn resolved(&mut self, path: &str) -> Result<Entry, EditorError> {
        let e = self
            .entries
            .iter_mut()
            .find(|e| e.path == path)
            .ok_or_else(|| EditorError::Level(format!("{path} is not in the catalogue")))?;
        resolve(e)?;
        Ok(e.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blueprint_paths() {
        assert!(is_blueprint_path("objects/props/crate/crate"));
        assert!(is_blueprint_path("vehicles/us/m1a2/m1a2"));
        assert!(!is_blueprint_path("objects/props/crate/crate_mesh"));
        assert!(!is_blueprint_path("levels/mp_005/mp_005"));
        assert!(why_not_by_path("vehicles/us/m1a2/m1a2").is_some());
        let t = thumbnail_file(Path::new("C:/o"), ContentSet::Vietnam, "objects/a/a");
        assert!(t.ends_with("vietnam/objects_a_a.png") || t.to_string_lossy().ends_with("objects_a_a.png"));
    }
}
