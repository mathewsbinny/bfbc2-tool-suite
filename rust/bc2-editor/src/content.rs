//! The content browser's catalogue (round E2b): the blueprints a project can place - those the level already places
//! (from its partition: each `ReferenceObjectData`'s `ReferencedObject`, with how many times), then the blueprints
//! of the level's own folders and its shared bundles (the profile's: a Vietnam level's `nam_common`, a Pacific War
//! level's `ww2_common`, `mp_common` ...), read on a thread. Each entry has its category from its data
//! ([`Category::of_blueprint`]) and whether it can be placed as an object, with why not.
//!
//! The safety checks of `EDITOR_ARCHITECTURE.md` 3.3 (a cull shape somewhere, collision, references that resolve)
//! belong to the asset browser round; until then an entry the level does not place yet says so.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use bc2_formats::dbx;
use bc2_scene::scene::Scene;

use crate::workspace::Category;

/// A placeable blueprint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The reference a placement takes (`objects/props/crate/crate/<guid>`, lower case as the partitions write it).
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
    /// Where it comes from: `This level`, or the folder (`mp_common` ...).
    pub source: String,
    /// Why it cannot be placed as an object, or `None`.
    pub why_not: Option<&'static str>,
}

impl Entry {
    /// Whether a search matches it: every word in its name, path, category or object type, ignoring case.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!(
            "{} {} {} {}",
            self.name,
            self.path,
            self.category.name(),
            self.object_type.as_deref().unwrap_or("")
        )
        .to_lowercase();
        query.to_lowercase().split_whitespace().all(|w| hay.contains(w))
    }
}

/// The level's own placements: one entry per blueprint, the most placed first.
#[must_use]
pub fn level_entries(scene: &Scene) -> Vec<Entry> {
    let Some((_, data)) = scene.file(0) else { return Vec::new() };
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
            let (bt, destructible) = info.get(path.as_str()).map_or((None, false), |(t, d)| ((*t).clone(), *d));
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
            }
        })
        .collect();
    out.sort_by(|a, b| b.used.cmp(&a.used).then(a.name.cmp(&b.name)));
    out
}

/// The top folders blueprints are looked for in, under each of the level's folders.
const TOPS: [&str; 4] = ["objects", "vehicles", "vegetation", "fx"];

fn entries(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir).map_or_else(|_| Vec::new(), |r| r.flatten().map(|e| e.path()).collect())
}

/// The blueprint files under a folder: `<name>\<name>.dbx` (the game's layout of a blueprint and its parts).
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

/// One blueprint file read: its entry (`None` for a partition that is not a blueprint).
fn read_blueprint(file: &Path, base: &Path, source: &str) -> Option<Entry> {
    let data = std::fs::read(file).ok()?;
    let tree = dbx::parse_tree(&data).ok()?;
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
    let rel = file.strip_prefix(base).ok()?.with_extension("");
    let path = rel
        .components()
        .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
        .collect::<Vec<_>>()
        .join("/");
    let why_not = if !ptype.ends_with("ObjectBlueprint") {
        Some("Not an object blueprint: vehicles, effects and sounds are placed through their spawners and emitters (a later round)")
    } else {
        None
    };
    Some(Entry {
        reference: format!("{path}/{primary}"),
        name: bc2_levelwrite::ops::blueprint_name(&path).to_owned(),
        category: Category::of_blueprint(&path, object_type.as_deref(), destructible),
        object_type,
        destructible,
        used: 0,
        source: source.to_owned(),
        why_not,
        path,
    })
}

/// The blueprints of the level's folders (its own, then its shared bundles'), each once (the first folder naming it
/// wins, as the game's stack), leaving out those in `known` (the level's own placements).
#[must_use]
pub fn base_entries(bases: &[PathBuf], known: &[String]) -> Vec<Entry> {
    let mut seen: std::collections::HashSet<String> = known.iter().cloned().collect();
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
            let n = top.file_name().map(|n| n.to_string_lossy().to_lowercase()).unwrap_or_default();
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
    out.sort_by(|a, b| a.category.cmp(&b.category).then(a.name.cmp(&b.name)));
    out
}

/// The catalogue: the level's placements and, once read, its folders' blueprints.
#[derive(Debug, Default)]
pub struct Catalogue {
    /// Every entry: the level's first.
    pub entries: Vec<Entry>,
    /// The folders' entries are still being read.
    pub reading: bool,
    rx: Option<std::sync::mpsc::Receiver<Vec<Entry>>>,
}

impl Catalogue {
    /// The level's placements at once, its folders' blueprints on a thread.
    #[must_use]
    pub fn start(scene: &Scene, bases: Vec<PathBuf>) -> Self {
        let entries = level_entries(scene);
        let known: Vec<String> = entries.iter().map(|e| e.path.clone()).collect();
        let rx = (!bases.is_empty()).then(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(base_entries(&bases, &known));
            });
            rx
        });
        Self {
            entries,
            reading: rx.is_some(),
            rx,
        }
    }

    /// Takes the folders' entries when they are read.
    pub fn poll(&mut self) {
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(more) => {
                    self.entries.extend(more);
                    self.rx = None;
                    self.reading = false;
                }
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.rx = None;
                    self.reading = false;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
            }
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
}
