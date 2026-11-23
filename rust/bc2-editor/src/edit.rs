//! Editing a map (round E2): a map project ([`bc2_levelwrite::MapProject`]) open on its base level. The [`Doc`] keeps
//! the base scene as loaded and derives from it, by the project's log, the scene the views show and the level
//! partition levelwrite writes ([`bc2_levelwrite::write::LevelWriter`]): a new operation is applied to both at once;
//! an undo or redo replays the log from the nearest checkpoint (round XF1: the state after a mode choice and after
//! every [`CHECKPOINT_EVERY`] operations is kept; the base is never edited), and a mode choice's donor level is read
//! from the game copy once per document.
//!
//! Every change is autosaved beside the project at once (crash recovery), since round OP1 as one line appended to the
//! project's journal ([`bc2_levelwrite::journal`]); **Save** writes the project and removes the autosave. Brush strokes
//! (round OP1, for E5) are kept as tiles beside the project ([`bc2_levelwrite::delta`]) and named by the log. Entities are named by guid in the log and by index in the scene: base entities keep their index,
//! deleted ones stay in the scene's list (left out of the views, the outliner and the queries), copies are added at
//! its end.

use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::error::EditorError;
use bc2_levelwrite::delta::{DirStore, Raster, TileStore as _};
use bc2_levelwrite::files::{FileEdit, LevelFiles, Sources, Target};

use crate::error::{Problem, ProblemCode, Severity};
use bc2_levelwrite::journal::{Entry, Journal};
use bc2_levelwrite::ops::Organisation;
use bc2_levelwrite::project::BaseReport;
use bc2_levelwrite::write::{Applied, LevelWriter};
use bc2_levelwrite::{FieldType, FieldValue, MapProject, Op, Operation, Profile, Schema};
use bc2_scene::scene::{Entity, Scene, Shape, Source};

/// One field as the properties panel edits it.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldRow {
    /// `Name`, `Struct.Field`.
    pub path: String,
    /// Its type, from the schema.
    pub ty: FieldType,
    /// Its value now.
    pub value: Option<FieldValue>,
    /// For a text field: the texts the level's data uses there (an enum's names), most used first.
    pub choices: Vec<String>,
}

/// A checkpoint is kept after every this many operations (and after each mode choice).
pub const CHECKPOINT_EVERY: usize = 16;
/// At most this many checkpoints are kept (the oldest ordinary ones go first; a mode choice's stays).
pub const CHECKPOINTS: usize = 8;

/// The document's state after the log's first `applied` operations (round XF1: undo replays from here).
#[derive(Clone)]
struct Checkpoint {
    applied: usize,
    /// The last of those operations' number: a checkpoint is good while the log still has it at that place.
    last_n: u64,
    /// After a mode choice (kept before ordinary checkpoints when there are too many).
    mode: bool,
    /// What the operations change in the scene: its entities, outlines and collision placements (the terrain, the
    /// water and the geometry stay the base's, shared).
    entities: Vec<Entity>,
    shapes: Vec<Shape>,
    meshes: Option<Vec<(usize, String)>>,
    deleted: BTreeSet<usize>,
    by_guid: HashMap<String, usize>,
    /// Every file's writer (round OP1): a multi-file operation is undone as one step.
    files: LevelFiles,
    problems: Vec<Problem>,
}

/// A map project open on its level.
pub struct Doc {
    /// The project.
    pub project: MapProject,
    /// Its file.
    pub path: PathBuf,
    base: Scene,
    /// The scene with the log applied.
    pub scene: Scene,
    /// Entities deleted by the log (their indices stay taken).
    pub deleted: BTreeSet<usize>,
    by_guid: HashMap<String, usize>,
    /// Layers, folders, groups and placements by the log.
    pub org: Organisation,
    base_bytes: Option<Vec<u8>>,
    /// The level's files as the log leaves them, a writer per file (round OP1, audit AU1 R18): the partition, the
    /// static group files the log edits.
    files: LevelFiles,
    /// The base partition's writer, parsed once (round PR1b-0): a replay from the base shares it until its first
    /// edit copies it.
    base_writer: Option<std::sync::Arc<LevelWriter>>,
    /// The field types (the level partition's data, `dbxcatalogue`'s rules).
    pub schema: Schema,
    /// What did not apply, and the base-changed report, in words.
    pub problems: Vec<Problem>,
    /// Saved since the last change.
    pub saved: bool,
    /// The base-changed check made when the project was opened.
    pub base_report: BaseReport,
    /// The last autosave's error, if it failed.
    pub autosave_error: Option<String>,
    /// Counts the changes to the scene and the partition (an operation applied, the log replayed): what the
    /// property tree and the links were read at (round GR1).
    pub revision: u64,
    /// Blueprints the level does not place, as the content browser described them (category, object type,
    /// destructible).
    known: HashMap<String, (String, Option<String>, bool)>,
    /// The game copy (round E3d): a mode choice's donor level is read from it ([`Doc::set_game`]).
    game: Option<bc2_levelwrite::export::Game>,
    /// Donor levels' partitions read from the game copy, by level (round XF1: read once, not per replay).
    donors: RefCell<HashMap<String, Option<Vec<u8>>>>,
    /// States the log passed through, oldest first (round XF1).
    checkpoints: Vec<Checkpoint>,
    /// Operations the last rebuild, undo, redo or History jump applied (the rest came from a checkpoint).
    pub last_replayed: usize,
    /// The entities the view's mode layer filter hides (round VW2, [`crate::modeview`]): left out of the views, the
    /// outliner and the queries like a hidden layer's, never edited by it.
    pub filtered: std::collections::HashSet<usize>,
    /// The project file as this editor last read or wrote it: a save checks it first (round QW2, audit AU1 R4).
    disk: Option<bc2_levelwrite::project::DiskStamp>,
    /// The project's lock file while this editor has it open (round QW2; `None` for documents made by tests and
    /// headless runs, which take no lock).
    lock: Option<bc2_levelwrite::project::ProjectLock>,
    /// The autosave journal since the last save (round OP1, audit AU1 R17): started at the first change.
    journal: Option<Journal>,
    /// The project's tile folder (round OP1: brush strokes), its recent tiles in memory.
    tiles: DirStore,
    /// The last operation [`Doc::refusal`] found would apply, with the files it made (round PR1b-0).
    trial: RefCell<Option<Trial>>,
}

/// A multi-file operation tried by [`Doc::refusal`]: the files it made from the document's at `revision`; a commit
/// of the same operation at the same revision takes them (the files are those the apply would make: the same
/// operation on the same files, sources and schema).
struct Trial {
    revision: u64,
    op: Op,
    files: LevelFiles,
}

/// Where the document's level files come from (round OP1): the scene as loaded (static group files, the height field)
/// and the project's tile folder.
struct DocSources<'a> {
    base: &'a Scene,
    terrain: Option<&'a Target>,
    tiles: &'a DirStore,
}

impl Sources for DocSources<'_> {
    fn file(&self, target: &Target) -> Option<Vec<u8>> {
        base_file(self.base, target)
    }

    fn raster(&self, target: &Target) -> Option<Raster> {
        if Some(target) != self.terrain {
            return None;
        }
        let hf = self.base.heightfield.as_ref()?;
        Some(Raster::from_samples(hf.size, hf.size, &hf.raw))
    }

    fn tile(&self, sha1: &str) -> Option<Vec<u8>> {
        self.tiles.get(sha1)
    }
}

/// A blueprint's path (lower case, as the scene keeps it) from its reference (`objects/x/x/<guid>`).
#[must_use]
pub fn blueprint_path(reference: &str) -> String {
    let r = reference.trim_end_matches('/');
    let looks_guid = |s: &str| s.len() == 36 && s.chars().filter(|c| *c == '-').count() == 4;
    match r.rsplit_once('/') {
        Some((head, last)) if looks_guid(last) => head.to_lowercase(),
        _ => r.to_lowercase(),
    }
}

/// A level file's bytes as the scene loaded it (round OP1): a static group file by its name; `None` for the other
/// targets.
fn base_file(scene: &Scene, target: &Target) -> Option<Vec<u8>> {
    let Target::Group { file } = target else { return None };
    (1..)
        .map_while(|k| scene.file(k))
        .find(|(p, _)| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(file))
        })
        .map(|(_, b)| b.to_vec())
}

fn index_of(scene: &Scene) -> HashMap<String, usize> {
    scene
        .entities
        .iter()
        .map(|e| (e.guid.to_ascii_uppercase(), e.index))
        .collect()
}

impl Doc {
    /// A new project, "New map from an existing map": `name` in `folder`, on the loaded level `scene` (of level
    /// `level`); saved at once.
    ///
    /// # Errors
    /// A project of that name is already in the folder, or it cannot be written.
    pub fn create(name: &str, folder: &Path, level: &str, scene: Scene) -> Result<Self, EditorError> {
        Self::create_kind(name, folder, level, scene, None)
    }

    /// [`Doc::create`] with the base's kind recorded (round G4: multiplayer or campaign, from its description).
    ///
    /// # Errors
    /// A project of that name is already in the folder, or it cannot be written.
    pub fn create_kind(
        name: &str,
        folder: &Path,
        level: &str,
        scene: Scene,
        kind: Option<bc2_levelwrite::BaseKind>,
    ) -> Result<Self, EditorError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(EditorError::Project("Give the new map a name.".into()));
        }
        let bytes = scene.file(0).map_or_else(Vec::new, |(_, b)| b.to_vec());
        let mut project = MapProject::new(
            name,
            level,
            &format!("levels/{}", scene.name),
            &bytes,
            // the editor's recognition (a Package level's bundles: ww2_common, nam_common), else by name
            match crate::factions::Profile::of(level) {
                crate::factions::Profile::Vietnam => Profile::Vietnam,
                crate::factions::Profile::Ww2 => Profile::Ww2,
                crate::factions::Profile::Bfbc2 => Profile::of_level(level),
            },
        );
        project.base.kind = kind;
        let path = folder.join(project.file_name());
        if path.exists() {
            return Err(EditorError::Project(format!(
                "{} is already in this folder. Open it with \"Open project\", or choose another name.",
                path.display()
            )));
        }
        let lock =
            bc2_levelwrite::project::ProjectLock::acquire(&path).map_err(|e| EditorError::Project(e.to_string()))?;
        project.save(&path).map_err(|e| {
            EditorError::Project(format!("The project could not be written to {}: {e}", path.display()))
        })?;
        let mut d = Self::with(project, path, scene);
        d.lock = Some(lock);
        Ok(d)
    }

    /// A project on its loaded level (the project's base level).
    #[must_use]
    pub fn with(project: MapProject, path: PathBuf, scene: Scene) -> Self {
        let base_bytes = scene.file(0).map(|(_, b)| b.to_vec());
        let writer = base_bytes
            .as_deref()
            .and_then(|b| LevelWriter::parse(b).ok())
            .map(std::sync::Arc::new);
        let schema = Schema::scan(writer.as_ref().map(|w| &w.d));
        let base_report = base_bytes
            .as_deref()
            .map_or_else(BaseReport::default, |b| project.base_report(b));
        let mut d = Self {
            project,
            path,
            by_guid: index_of(&scene),
            scene: scene.clone(),
            base: scene,
            deleted: BTreeSet::new(),
            org: Organisation::default(),
            base_bytes,
            files: LevelFiles::default(),
            base_writer: writer,
            schema,
            problems: Vec::new(),
            saved: true,
            base_report,
            autosave_error: None,
            revision: 0,
            known: HashMap::new(),
            game: None,
            donors: RefCell::new(HashMap::new()),
            checkpoints: Vec::new(),
            last_replayed: 0,
            filtered: std::collections::HashSet::new(),
            disk: None,
            lock: None,
            journal: None,
            tiles: DirStore::of(Path::new("")),
            trial: RefCell::new(None),
        };
        d.tiles = DirStore::of(&d.path);
        d.disk = bc2_levelwrite::project::disk_stamp(&d.path);
        d.rebuild();
        d
    }

    /// The scene and the partition again from the base and the whole log (the checkpoints made again).
    pub fn rebuild(&mut self) {
        self.checkpoints.clear();
        self.replay_from_base();
    }

    /// Whether the log still passes through a checkpoint: it has the checkpoint's last operation at its place.
    fn passes(&self, c: &Checkpoint) -> bool {
        c.applied <= self.project.operations.len()
            && c.applied
                .checked_sub(1)
                .and_then(|i| self.project.operations.get(i))
                .is_some_and(|o| o.n == c.last_n)
    }

    /// The scene and the partition for the log as it is now, from the newest checkpoint it still passes through
    /// (else from the base): undo, redo and History (round XF1, audit AU1 D1).
    fn restore(&mut self) {
        let Some(c) = self.checkpoints.iter().rev().find(|c| self.passes(c)).cloned() else {
            self.replay_from_base();
            return;
        };
        self.revision += 1;
        // the base's scene with the checkpoint's entities and outlines (the base's are not copied first); the
        // terrain, the files and the ray structures are shared with the base (round PR1b-0, audit AU5 ALG-1)
        self.scene = self.base.with_entities(c.entities, c.shapes);
        if let (Some(col), Some(m)) = (self.scene.collision.as_mut(), c.meshes) {
            col.meshes = m;
        }
        self.deleted = c.deleted;
        self.by_guid = c.by_guid;
        self.files = c.files;
        self.problems = c.problems;
        let rest: Vec<Operation> = self.project.operations.get(c.applied..).unwrap_or_default().to_vec();
        self.last_replayed = rest.len();
        for (i, o) in rest.iter().enumerate() {
            self.apply(o);
            self.checkpoint_after(c.applied + i + 1, o);
        }
        self.org = self.project.organisation();
    }

    /// Keeps the state after `applied` operations (the last `o`) when that is a checkpoint's place: after a mode
    /// choice, and after every [`CHECKPOINT_EVERY`] operations. Checkpoints the log no longer passes through, or at
    /// or after this place, are dropped first; at most [`CHECKPOINTS`] are kept.
    fn checkpoint_after(&mut self, applied: usize, o: &Operation) {
        let mode = matches!(o.op, Op::SetMode { .. });
        if !mode && !applied.is_multiple_of(CHECKPOINT_EVERY) {
            return;
        }
        let keep: Vec<bool> = self
            .checkpoints
            .iter()
            .map(|c| c.applied < applied && self.passes(c))
            .collect();
        let mut keep = keep.into_iter();
        self.checkpoints.retain(|_| keep.next().unwrap_or(false));
        self.checkpoints.push(Checkpoint {
            applied,
            last_n: o.n,
            mode,
            entities: self.scene.entities.clone(),
            shapes: self.scene.shapes.clone(),
            meshes: self.scene.collision.as_ref().map(|c| c.meshes.clone()),
            deleted: self.deleted.clone(),
            by_guid: self.by_guid.clone(),
            files: self.files.clone(),
            problems: self.problems.clone(),
        });
        while self.checkpoints.len() > CHECKPOINTS {
            let drop = self.checkpoints.iter().position(|c| !c.mode).unwrap_or(0);
            self.checkpoints.remove(drop);
        }
    }

    /// The whole log replayed from the base.
    fn replay_from_base(&mut self) {
        self.revision += 1;
        self.scene = self.base.clone();
        self.deleted.clear();
        self.by_guid = index_of(&self.base);
        // the base partition parsed once, shared until the first edit copies it (round PR1b-0)
        self.files = LevelFiles {
            partition: self.base_writer.clone(),
            ..LevelFiles::default()
        };
        self.problems.clear();
        let r = &self.base_report;
        if r.changed {
            self.problems.push(Problem::new(
                Severity::Warning,
                ProblemCode::BaseChanged,
                format!(
                    "The level changed since this project was made (a game patch, or a rebuilt level): its partition \
                     is not the one the edits were made on. {} operation(s) name objects it no longer has; they are \
                     kept, not dropped.",
                    r.missing.len()
                ),
            ));
        }
        for (n, name, guid) in &r.missing {
            self.problems.push(
                Problem::new(
                    Severity::Error,
                    ProblemCode::MissingObject,
                    format!("Operation {n} ({name}) names {guid}, which the level does not have."),
                )
                .at(*n)
                .naming(vec![guid.clone()]),
            );
        }
        let ops = self.project.operations.clone();
        self.last_replayed = ops.len();
        for (i, o) in ops.iter().enumerate() {
            self.apply(o);
            self.checkpoint_after(i + 1, o);
        }
        self.org = self.project.organisation();
    }

    /// The game copy the project's mode choice reads its donor from (round E3d); the log applied again.
    pub fn set_game(&mut self, game: Option<bc2_levelwrite::export::Game>) {
        self.game = game;
        self.donors.borrow_mut().clear();
        self.rebuild();
    }

    /// The game copy, when the editor found one.
    #[must_use]
    pub fn game(&self) -> Option<&bc2_levelwrite::export::Game> {
        self.game.as_ref()
    }

    fn apply(&mut self, o: &Operation) {
        // the files a refusal check made for this very operation on the files as they are now (round PR1b-0, audit
        // AU5 ALG-3): taken instead of applying it a second time
        let trial = self
            .trial
            .get_mut()
            .take()
            .filter(|t| t.revision == self.revision && t.op == o.op);
        self.revision += 1;
        // a donor level's partition is read from the game copy once (round XF1, audit AU1 D2)
        let (game, cache) = (&self.game, &self.donors);
        let donors = |lv: &str| {
            let key = lv.to_ascii_lowercase();
            if let Some(b) = cache.borrow().get(&key) {
                return b.clone();
            }
            let b = game
                .as_ref()
                .and_then(|g| bc2_levelwrite::export::level_partition(g, lv).ok());
            cache.borrow_mut().insert(key, b.clone());
            b
        };
        // the level's other files are read from the scene as loaded, a stroke's tiles from beside the project (OP1)
        let terrain = self.terrain_target();
        let sources = DocSources {
            base: &self.base,
            terrain: terrain.as_ref(),
            tiles: &self.tiles,
        };
        let applied = match trial {
            Some(t) => {
                self.files = t.files;
                Ok(())
            }
            None => self
                .files
                .apply(o, &self.schema, &mut Applied::default(), &donors, &sources),
        };
        if let Err(e) = &applied {
            self.problems
                .push(Problem::new(Severity::Error, ProblemCode::NotApplied, e.to_string()).at(e.n));
        }
        match &o.op {
            // a multi-file operation: the scene follows only when every edit applied (round OP1)
            Op::Files { edits } if applied.is_ok() => {
                for e in edits {
                    match e {
                        FileEdit::Partition { op } => self.apply_scene(op),
                        FileEdit::MemberTransform { instance, item, to, .. } => {
                            let member = bc2_levelwrite::members::member_guid(instance, *item);
                            if let Some(k) = self.index(&member) {
                                self.scene.set_transform(k, bc2_levelwrite::matrix_of(to));
                            }
                        }
                        // round SG1: a member deleted is left out like a deleted object; an added one is a copy
                        // of the member it copies, named by its own number
                        FileEdit::MemberDelete { instance, item, .. } => {
                            let member = bc2_levelwrite::members::member_guid(instance, *item);
                            self.apply_scene(&Op::Delete { guids: vec![member] });
                        }
                        FileEdit::MemberAdd {
                            instance,
                            item,
                            like,
                            rows,
                            ..
                        } => {
                            let source = bc2_levelwrite::members::member_guid(instance, *like);
                            let guid = bc2_levelwrite::members::member_guid(instance, *item);
                            if let Some(k) = self.index(&source) {
                                self.copy_entity(k, &guid, Some(bc2_levelwrite::matrix_of(rows)));
                            }
                        }
                        // a group file's field: nothing the scene shows
                        FileEdit::GroupField { .. } => {}
                    }
                }
            }
            op => self.apply_scene(op),
        }
    }

    /// What an operation of the partition changes in the scene.
    fn apply_scene(&mut self, op: &Op) {
        match op {
            Op::Move { items } | Op::Rotate { items } => {
                for m in items {
                    if let Some(k) = self.index(&m.guid) {
                        self.scene.set_transform(k, bc2_levelwrite::matrix_of(&m.to));
                    }
                }
            }
            Op::SetMode { to } => {
                // the old layer out of the views, the new one in, read from the edited partition
                for g in &to.removed {
                    if let Some(k) = self.index(g) {
                        self.deleted.insert(k);
                        self.by_guid.remove(&g.to_ascii_uppercase());
                    }
                }
                for it in &to.items {
                    self.add_layer_entity(&it.guid);
                }
            }
            Op::Delete { guids } => {
                for g in guids {
                    if let Some(k) = self.index(g) {
                        self.deleted.insert(k);
                        if let Some(c) = self.scene.collision.as_mut() {
                            c.meshes.retain(|(i, _)| *i != k);
                        }
                        self.by_guid.remove(&g.to_ascii_uppercase());
                    }
                }
            }
            Op::Duplicate { items } => {
                for d in items {
                    let Some(k) = self.index(&d.source) else { continue };
                    let old = self.scene.entities.get(k).and_then(|e| e.matrix);
                    let to = bc2_levelwrite::matrix_of(&d.transform);
                    let Some(n) = self.copy_entity(k, &d.guid, Some(to)) else {
                        continue;
                    };
                    self.copy_owned(k, n, old, to, &d.owned);
                }
            }
            Op::Place { items } => {
                for p in items {
                    self.place_in_scene(p);
                }
            }
            Op::PlaceUnit { item } => self.place_in_scene(item),
            // round E4: a team set shows in the views' team colours
            Op::SetTeam { items } => {
                for t in items {
                    if let Some(k) = self.index(&t.guid) {
                        self.field_to_scene(k, "Team", &FieldValue::Text(t.to.data().to_owned()));
                    }
                }
            }
            Op::SetField { items } => {
                for f in items {
                    if let Some(k) = self.index(&f.guid) {
                        self.field_to_scene(k, &f.field, &f.to);
                    }
                }
            }
            _ => {}
        }
    }

    /// A placed object in the scene (a placement, or a gameplay unit's: round E4): a copy of its template with the
    /// sub-instances copied with it, its fields, and its blueprint's kind.
    fn place_in_scene(&mut self, p: &bc2_levelwrite::ops::Placed) {
        let Some(k) = self.index(&p.template) else { return };
        let old = self.scene.entities.get(k).and_then(|e| e.matrix);
        let to = bc2_levelwrite::matrix_of(&p.transform);
        let Some(n) = self.copy_entity(k, &p.guid, Some(to)) else {
            return;
        };
        self.copy_owned(k, n, old, to, &p.owned);
        for f in &p.fields {
            self.field_to_scene(n, &f.field, &f.value);
        }
        let path = blueprint_path(&p.blueprint);
        let same = self.scene.entities.get(k).and_then(|e| e.blueprint.clone()) == Some(path.clone());
        // a spawner's vehicle or a template kept as it is: the entity stays the template's kind
        if !same && !p.blueprint.is_empty() && p.ref_field.is_none() {
            let info = self.blueprint_info(&path);
            if let Some(c) = self.scene.collision.as_mut() {
                c.meshes.retain(|(i, _)| *i != n);
                if c.blueprints.contains_key(&path) {
                    c.meshes.push((n, path.clone()));
                }
            }
            if let Some(e) = self.scene.entities.get_mut(n) {
                e.blueprint = Some(path);
                e.name = None;
                if let Some((category, bt, destructible)) = info {
                    e.category = category;
                    e.blueprint_type = bt;
                    e.destructible = destructible;
                }
            }
        }
    }

    /// What a field set changes in the scene: a name, a team.
    fn field_to_scene(&mut self, k: usize, field: &str, to: &FieldValue) {
        let Some(e) = self.scene.entities.get_mut(k) else {
            return;
        };
        match (field, to) {
            ("Name", FieldValue::Text(s)) => e.name = Some(s.clone()),
            ("Team", FieldValue::Int(t)) if e.team.is_some() && (0..=2).contains(t) => {
                e.team = Some(*t as u8);
            }
            ("Team" | "InitialOwnerTeam", FieldValue::Text(t)) if e.team.is_some() => {
                e.team = match t.as_str() {
                    "Team1" => Some(1),
                    "Team2" => Some(2),
                    "TeamNeutral" => Some(0),
                    _ => e.team,
                };
            }
            _ => {}
        }
    }

    /// Copies entity `k` as `guid` at `matrix` (its own when `None`) at the end of the scene, with its collision;
    /// the copy's index.
    /// An entity for an instance a mode choice brought in (round E3d), from the edited partition: its type, name,
    /// transform, blueprint and team, as the scene loader reads them.
    fn add_layer_entity(&mut self, guid: &str) -> Option<usize> {
        let w = self.files.partition.as_ref()?;
        let e = w.find(guid)?;
        let type_name = w.type_of(e);
        let short = type_name.rsplit('.').next().unwrap_or(&type_name).to_owned();
        let text = |f: &[u8]| {
            w.d.child(e, f)
                .and_then(|x| w.d.text(x))
                .map(|t| String::from_utf8_lossy(t).into_owned())
        };
        let blueprint =
            w.d.child(e, b"ReferencedObject")
                .and_then(|x| w.d.attr(x, b"ref"))
                .map(|r| blueprint_path(&String::from_utf8_lossy(r)));
        let team = text(b"Team")
            .or_else(|| text(b"InitialOwnerTeam"))
            .and_then(|t| match t.as_str() {
                "Team1" => Some(1),
                "Team2" => Some(2),
                "TeamNeutral" => Some(0),
                _ => None,
            });
        let n = self.scene.entities.len();
        let partition = self
            .scene
            .entities
            .first()
            .map_or_else(|| self.project.base.partition.clone(), |x| x.partition.clone());
        let matrix = w.transform(guid).map(|r| bc2_levelwrite::matrix_of(&r));
        self.scene.entities.push(Entity {
            index: n,
            guid: guid.to_ascii_uppercase(),
            category: bc2_scene::scene::category_of(&type_name).to_owned(),
            type_name,
            short_type: short,
            id: None,
            name: text(b"Name").filter(|t| !t.is_empty()),
            partition,
            matrix,
            blueprint,
            group: None,
            blueprint_type: None,
            destructible: false,
            team,
            ai: None,
            source: Source::Instance { instance: usize::MAX },
        });
        self.by_guid.insert(guid.to_ascii_uppercase(), n);
        Some(n)
    }

    fn copy_entity(&mut self, k: usize, guid: &str, matrix: Option<[f64; 16]>) -> Option<usize> {
        let mut e = self.scene.entities.get(k).cloned()?;
        let n = self.scene.entities.len();
        e.index = n;
        guid.clone_into(&mut e.guid);
        e.id = None;
        if matrix.is_some() {
            e.matrix = matrix;
        }
        self.scene.entities.push(e);
        if let Some(c) = self.scene.collision.as_mut()
            && let Some(bp) = c.meshes.iter().find(|(i, _)| *i == k).map(|(_, b)| b.clone())
        {
            c.meshes.push((n, bp));
        }
        self.by_guid.insert(guid.to_ascii_uppercase(), n);
        Some(n)
    }

    /// The sub-instances copied with entity `k` (as `n`): their entities, and `k`'s outlines (a flag's capture
    /// area) carried from `old` to `to`.
    fn copy_owned(
        &mut self,
        k: usize,
        n: usize,
        old: Option<[f64; 16]>,
        to: [f64; 16],
        owned: &[bc2_levelwrite::ops::SubCopy],
    ) {
        for s in owned {
            if let Some(sk) = self.index(&s.source) {
                self.copy_entity(sk, &s.guid, s.transform.as_ref().map(bc2_levelwrite::matrix_of));
            }
        }
        if owned.is_empty() {
            return;
        }
        let Some(old) = old else { return };
        let copies: Vec<Shape> = self
            .scene
            .shapes
            .iter()
            .filter(|s| s.entity == k)
            .map(|s| {
                let mut c = s.clone();
                c.entity = n;
                c.points = c.points.iter().map(|p| crate::math::carry(&old, &to, *p)).collect();
                c
            })
            .collect();
        self.scene.shapes.extend(copies);
    }

    /// The level partition being edited (levelwrite's writer), when the scene has one.
    #[must_use]
    pub fn writer(&self) -> Option<&LevelWriter> {
        self.files.partition.as_deref()
    }

    /// The level's files as the log leaves them (round OP1): the partition and the static group files it edits.
    #[must_use]
    pub fn files(&self) -> &LevelFiles {
        &self.files
    }

    /// A file of the level as it was loaded (round OP1): a static group file's bytes from the scene; `None` for the
    /// partition (see [`Doc::base_bytes`]) and the files the scene did not read.
    #[must_use]
    pub fn base_file(&self, target: &Target) -> Option<Vec<u8>> {
        base_file(&self.base, target)
    }

    /// What a blueprint is (category, the type of its object, destructible): from a placement of it in the scene,
    /// else what the content browser told ([`Doc::know_blueprint`]).
    fn blueprint_info(&self, path: &str) -> Option<(String, Option<String>, bool)> {
        self.base
            .entities
            .iter()
            .find(|e| e.blueprint.as_deref() == Some(path))
            .map(|e| (e.category.clone(), e.blueprint_type.clone(), e.destructible))
            .or_else(|| self.known.get(path).cloned())
    }

    /// Tells the doc what a blueprint the level does not place is (the content browser's catalogue), so a placed
    /// copy is drawn and listed as one.
    pub fn know_blueprint(&mut self, path: &str, category: &str, object_type: Option<String>, destructible: bool) {
        self.known
            .insert(path.to_owned(), (category.to_owned(), object_type, destructible));
    }

    /// The sub-instances entity `k` owns (a deep duplicate copies them): [`LevelWriter::owned`]; empty without a
    /// partition.
    #[must_use]
    pub fn owned(&self, k: usize) -> Vec<String> {
        match (self.files.partition.as_ref(), self.guid(k)) {
            (Some(w), Some(g)) => w.owned(g),
            _ => Vec::new(),
        }
    }

    /// The instance a new placement of `blueprint` (a reference) is made from, and the fields it takes from the
    /// level's data; `None` without a partition or a placed object to make it from.
    #[must_use]
    pub fn template_for(&self, blueprint: &str) -> Option<(String, Vec<bc2_levelwrite::ops::FieldIs>)> {
        let w = self.files.partition.as_ref()?;
        let t = w.template_for(blueprint)?;
        let f = w.place_defaults(&self.schema, &t, blueprint);
        Some((t, f))
    }

    /// Changes the last operation (number `n`) to `op`, for a gesture that goes on (a sink wheel burst, a slider
    /// drag): still one operation in History. Applied on top (a move sets transforms: the same as a replay).
    /// `false` when the last operation is not `n`.
    pub fn amend(&mut self, n: u64, op: Op) -> bool {
        let op = crate::memberedit::lower(self, op);
        if !self.project.amend_last(n, op) {
            return false;
        }
        // a checkpoint after the amended operation holds its old version
        let len = self.project.operations.len();
        self.checkpoints.retain(|c| c.applied < len);
        if let Some(o) = self.project.operations.last().cloned() {
            self.apply(&o);
        }
        self.org = self.project.organisation();
        if let Some(op) = self.project.operations.last().cloned() {
            self.changed(&[Entry::Amend { op }]);
        }
        true
    }

    /// The number of the last operation.
    #[must_use]
    pub fn last_n(&self) -> Option<u64> {
        self.project.operations.last().map(|o| o.n)
    }

    /// Why an operation would not apply now (round SG1: a member's delete or add is checked on copies of the files
    /// before it is added, so a refused one never enters the log): `None` when it applies.
    #[must_use]
    pub fn refusal(&self, op: &Op) -> Option<String> {
        let op = crate::memberedit::lower(self, op.clone());
        if !matches!(op, Op::Files { .. }) {
            return None;
        }
        let mut files = self.files.clone();
        let terrain = self.terrain_target();
        let sources = DocSources {
            base: &self.base,
            terrain: terrain.as_ref(),
            tiles: &self.tiles,
        };
        let (game, cache) = (&self.game, &self.donors);
        let donors = |lv: &str| {
            cache
                .borrow()
                .get(&lv.to_ascii_lowercase())
                .cloned()
                .flatten()
                .or_else(|| {
                    game.as_ref()
                        .and_then(|g| bc2_levelwrite::export::level_partition(g, lv).ok())
                })
        };
        let o = Operation {
            n: self.project.next_n(),
            by: String::new(),
            when: String::new(),
            label: String::new(),
            op,
        };
        match files.apply(&o, &self.schema, &mut Applied::default(), &donors, &sources) {
            // kept: committing this operation next takes these files instead of applying it again (round PR1b-0)
            Ok(()) => {
                *self.trial.borrow_mut() = Some(Trial {
                    revision: self.revision,
                    op: o.op,
                    files,
                });
                None
            }
            Err(e) => Some(e.error.to_string()),
        }
    }

    /// The guids of the entities the last operation made (copies, placed objects, added group members).
    #[must_use]
    pub fn created_by_last(&self) -> Vec<String> {
        self.project
            .operations
            .last()
            .map_or_else(Vec::new, |o| crate::memberedit::created(&o.op))
    }

    /// Adds an operation (applied to the scene and the partition at once, autosaved); its number. An operation that
    /// names static group members is made their group files' edits first ([`crate::memberedit::lower`]).
    pub fn commit(&mut self, op: Op, label: &str) -> u64 {
        let op = crate::memberedit::lower(self, op);
        // a base file edited for the first time is recorded with its sha1 (round OP1: its base-changed check)
        let mut entries = Vec::new();
        for t in op.files() {
            if let (Some(b), Some(key)) = (base_file(&self.base, &t), t.base_key())
                && self.project.note_file(&t, &b)
                && let Some(file) = self.project.base.files.get(&key)
            {
                entries.push(Entry::BaseFile {
                    key,
                    file: file.clone(),
                });
            }
        }
        let n = self.project.push(op, label);
        if let Some(o) = self.project.operations.last().cloned() {
            self.apply(&o);
            let len = self.project.operations.len();
            self.checkpoint_after(len, &o);
            // the organisation follows the new operation (round OP1, audit AU1 R17: not the whole log again)
            self.org.apply(&o.op);
            entries.push(Entry::Push { op: o });
        }
        self.changed(&entries);
        n
    }

    /// The project changed: unsaved, and the change appended to the autosave journal (started with the project as it
    /// is now at the first change since the last save, compacted when it grows long).
    fn changed(&mut self, entries: &[Entry]) {
        self.saved = false;
        let written = match self.journal.as_mut() {
            Some(j) => entries.iter().try_for_each(|e| j.append(e)).and_then(|()| {
                if j.needs_compaction() {
                    j.compact(&self.project)
                } else {
                    Ok(())
                }
            }),
            None => Journal::start(&bc2_levelwrite::project::autosave_path(&self.path), &self.project)
                .map(|j| self.journal = Some(j)),
        };
        self.autosave_error = written.err().map(|e| {
            // a journal that failed is started afresh (whole) at the next change
            self.journal = None;
            e.to_string()
        });
    }

    /// Lets the autosave journal go without a save (the user chose not to save: the caller removes its file).
    pub fn forget_autosave(&mut self) {
        self.journal = None;
    }

    /// The autosave journal, while one is being written (round OP1).
    #[must_use]
    pub fn journal(&self) -> Option<&Journal> {
        self.journal.as_ref()
    }

    /// The height field's target (round OP1): the level's main height field by its terrain reference (`heightfield`
    /// when the partition names none), `None` when the scene read no height field.
    #[must_use]
    pub fn terrain_target(&self) -> Option<Target> {
        self.base.heightfield.as_ref()?;
        let file = self
            .files
            .partition
            .as_deref()
            .and_then(bc2_levelwrite::export::heightfield_name)
            .unwrap_or_else(|| "heightfield".into());
        Some(Target::Terrain { file })
    }

    /// A raster as the log leaves it (round OP1): a height field's stored values, a paint layer's weights; `None`
    /// before the first stroke on a paint layer, or for a height field the scene did not read.
    #[must_use]
    pub fn raster(&self, target: &Target) -> Option<Raster> {
        if let Some(r) = self.files.rasters.get(target) {
            return Some(r.clone());
        }
        let terrain = self.terrain_target();
        DocSources {
            base: &self.base,
            terrain: terrain.as_ref(),
            tiles: &self.tiles,
        }
        .raster(target)
    }

    /// A brush stroke (round OP1, for E5's brushes): the raster `target` as `after` holds it, from the raster as the
    /// log leaves it (a paint layer starts at 0, `after`'s size). Only the tiles that changed are kept, beside the
    /// project; the log's operation names them. `None` when nothing changed.
    ///
    /// # Errors
    /// No such raster, another size, or a tile not written.
    pub fn stroke(&mut self, target: &Target, after: &Raster, label: &str) -> Result<Option<u64>, EditorError> {
        let before = match (self.raster(target), target) {
            (Some(r), _) => r,
            (None, Target::Paint { .. }) => Raster::new(after.width(), after.height(), 0),
            (None, other) => {
                return Err(EditorError::Project(format!(
                    "{} is not in this level",
                    other.describe()
                )));
            }
        };
        let tiles = bc2_levelwrite::delta::stroke(&before, after, &self.tiles)
            .map_err(|e| EditorError::Project(e.to_string()))?;
        if tiles.is_empty() {
            return Ok(None);
        }
        let n = self.commit(
            Op::Stroke {
                target: target.clone(),
                size: [after.width(), after.height()],
                tiles,
            },
            label,
        );
        // applied: the raster takes `after`'s tiles (the same samples), so the brush's next copy shares them and the
        // next stroke compares only the tiles it touches
        if let Some(r) = self.files.rasters.get_mut(target)
            && r == after
        {
            *r = after.clone();
        }
        Ok(Some(n))
    }

    /// Undoes the last operation; `false` when there is none.
    pub fn undo(&mut self) -> bool {
        if self.project.undo().is_none() {
            return false;
        }
        self.restore();
        self.changed(&[Entry::Undo]);
        true
    }

    /// Redoes the last undone operation; `false` when there is none.
    pub fn redo(&mut self) -> bool {
        if self.project.redo().is_none() {
            return false;
        }
        self.restore();
        self.changed(&[Entry::Redo]);
        true
    }

    /// Undoes or redoes until `applied` operations are in the log (History: "go to this point").
    pub fn go_to(&mut self, applied: usize) {
        if applied != self.project.operations.len() {
            self.project.go_to(applied);
            self.restore();
            self.changed(&[Entry::GoTo { applied }]);
        }
    }

    /// Saves the project (and removes the autosave).
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn save(&mut self) -> Result<(), EditorError> {
        // the journal is closed first: the save removes its file (a failed save leaves it; the next change starts a
        // whole one)
        if let Some(mut j) = self.journal.take() {
            let _ = j.sync();
        }
        self.disk = self
            .project
            .save_checked(&self.path, self.disk)
            .map_err(|e| EditorError::Project(e.to_string()))?;
        self.saved = true;
        Ok(())
    }

    /// Takes the project's lock file (round QW2, audit AU1 R4): an editor that opens a project holds it until the
    /// document is closed, so a second editor refuses the project in words instead of overwriting it.
    ///
    /// # Errors
    /// Another running editor holds it, or the lock file cannot be written.
    pub fn take_lock(&mut self) -> Result<(), EditorError> {
        if self.lock.is_none() {
            self.lock = Some(
                bc2_levelwrite::project::ProjectLock::acquire(&self.path)
                    .map_err(|e| EditorError::Project(e.to_string()))?,
            );
        }
        Ok(())
    }

    /// Moves entities in the scene only, while a drag is under way (the operation is added when it ends).
    pub fn preview(&mut self, transforms: &[(usize, [f64; 16])]) {
        for (k, m) in transforms {
            self.scene.set_transform(*k, *m);
        }
    }

    /// The scene index of a guid (`None` when not in the scene, or deleted).
    #[must_use]
    pub fn index(&self, guid: &str) -> Option<usize> {
        self.by_guid.get(&guid.to_ascii_uppercase()).copied()
    }

    /// An entity's guid.
    #[must_use]
    pub fn guid(&self, k: usize) -> Option<&str> {
        self.scene.entities.get(k).map(|e| e.guid.as_str())
    }

    /// Whether an entity is drawn and listed: not deleted, its layer shown, and not hidden by the view's mode layer
    /// filter (round VW2).
    #[must_use]
    pub fn shown(&self, k: usize) -> bool {
        !self.deleted.contains(&k) && !self.filtered.contains(&k) && self.guid(k).is_some_and(|g| self.org.shown(g))
    }

    /// Why an entity cannot be moved, turned, copied or deleted (`None`: it can).
    #[must_use]
    pub fn why_not(&self, k: usize) -> Option<&'static str> {
        let e = self.scene.entities.get(k)?;
        if self.deleted.contains(&k) {
            return Some("It is deleted");
        }
        if e.type_name.ends_with(".LevelData") {
            return Some("The level itself");
        }
        if e.matrix.is_none() {
            return Some("It has no transform");
        }
        if !self.org.editable(&e.guid) {
            return Some("Its layer is locked");
        }
        None
    }

    /// The members of an entity's group (indices), or the entity alone.
    #[must_use]
    pub fn with_group(&self, k: usize) -> Vec<usize> {
        let Some(g) = self.guid(k) else { return vec![k] };
        match self.org.group_of(g) {
            Some((_, members)) => members.iter().filter_map(|m| self.index(m)).collect(),
            None => vec![k],
        }
    }

    /// The entity's fields, typed from the schema (empty for entities outside the level partition).
    #[must_use]
    pub fn fields(&self, k: usize) -> Vec<FieldRow> {
        let (Some(w), Some(e)) = (self.files.partition.as_ref(), self.scene.entities.get(k)) else {
            return Vec::new();
        };
        w.fields(&self.schema, &e.guid)
            .into_iter()
            .map(|(path, ty, value)| {
                let choices = if ty == FieldType::Text {
                    self.schema.texts(&e.type_name, &format!(".{path}"))
                } else {
                    Vec::new()
                };
                FieldRow {
                    path,
                    ty,
                    value,
                    choices,
                }
            })
            .collect()
    }

    /// Whether the doc has the level partition (the demo scene has none: its edits change the scene only).
    #[must_use]
    pub fn has_partition(&self) -> bool {
        self.files.partition.is_some()
    }

    /// The base level's partition as it was loaded (round EX1: Compare with base).
    #[must_use]
    pub fn base_bytes(&self) -> Option<&[u8]> {
        self.base_bytes.as_deref()
    }

    /// The base level's scene as it was read (its collision, terrain and water: round A1's site).
    #[must_use]
    pub fn base_scene(&self) -> &Scene {
        &self.base
    }

    /// The edited partition's bytes.
    #[must_use]
    pub fn partition(&self) -> Option<Vec<u8>> {
        self.files.partition.as_ref().and_then(|w| w.write().ok())
    }

    /// Checks the edits: the whole log applied to the base afresh gives what the editor holds; a summary in words.
    #[must_use]
    pub fn check(&self) -> String {
        let Some(base) = self.base_bytes.as_deref() else {
            return "This scene has no level partition (the demo): its edits change the view only.".into();
        };
        let sources = |t: &Target| base_file(&self.base, t);
        let donors = |lv: &str| self.donors.borrow().get(&lv.to_ascii_lowercase()).cloned().flatten();
        match bc2_levelwrite::files::apply_files(base, &self.project.operations, &self.schema, &donors, &sources) {
            Ok(all) => {
                let a = all.partition;
                let same =
                    self.partition().is_some_and(|p| p == a.bytes) && self.files.edited().is_ok_and(|f| f == all.files);
                let changes = match (
                    bc2_formats::dbx::edit::EditDbx::parse(base),
                    bc2_formats::dbx::edit::EditDbx::parse(&a.bytes),
                ) {
                    (Ok(x), Ok(y)) => bc2_levelwrite::diff::diff(&x, &y).len(),
                    _ => 0,
                };
                format!(
                    "{} operation(s) apply cleanly to {}: {} transform(s), {} copy(ies), {} placed, {} deleted, {} \
                     field(s); {} change(s) in the partition{}.",
                    a.operations,
                    self.project.base.partition,
                    a.transforms,
                    a.duplicated,
                    a.placed,
                    a.deleted,
                    a.fields,
                    changes,
                    if same {
                        ""
                    } else {
                        " (the live copy differs: reopen the project)"
                    }
                )
            }
            Err(e) => format!("The edits do not apply: {e}"),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use bc2_levelwrite::ops::{Duplicated, Moved};

    #[test]
    fn the_log_drives_the_scene() {
        let dir = std::env::temp_dir().join(format!("bc2_editor_doc_{}", std::process::id()));
        let mut d = Doc::create("Demo edit", &dir, "demo", crate::demo::scene()).unwrap();
        assert!(Doc::create("Demo edit", &dir, "demo", crate::demo::scene()).is_err());
        let g1 = d.guid(1).unwrap().to_owned();
        let m = d.scene.entities[1].matrix.unwrap();
        let to = bc2_levelwrite::rows_of(&crate::math::translate(&m, [3.0, 0.0, 0.0]));
        d.commit(
            Op::Move {
                items: vec![Moved {
                    guid: g1.clone(),
                    from: bc2_levelwrite::rows_of(&m),
                    to,
                    placement: None,
                }],
            },
            "",
        );
        assert_eq!(d.scene.entities[1].position().unwrap()[0], -22.0);
        let copy = d.project.new_guid(&g1);
        d.commit(
            Op::Duplicate {
                items: vec![Duplicated {
                    source: g1.clone(),
                    guid: copy.clone(),
                    transform: bc2_levelwrite::rows_at([0.0, 0.0, 40.0]),
                    owned: Vec::new(),
                }],
            },
            "",
        );
        assert_eq!(d.scene.entities.len(), 6);
        assert_eq!(d.index(&copy), Some(5));
        // the house's copy gets its collision
        let house = d.guid(0).unwrap().to_owned();
        let hc = d.project.new_guid(&house);
        d.commit(
            Op::Duplicate {
                items: vec![Duplicated {
                    source: house.clone(),
                    guid: hc.clone(),
                    transform: bc2_levelwrite::rows_at([30.0, 0.0, 0.0]),
                    owned: Vec::new(),
                }],
            },
            "",
        );
        assert!(d.scene.collision.as_ref().unwrap().meshes.iter().any(|(k, _)| *k == 6));
        d.commit(
            Op::Delete {
                guids: vec![house.clone()],
            },
            "",
        );
        assert!(!d.shown(0) && d.why_not(0).is_some() && d.index(&house).is_none());
        assert!(
            crate::query::surface_below(&d.scene, 1.0, 1.0)
                .unwrap()
                .entity
                .is_none()
        );
        assert!(!d.saved);
        assert!(bc2_levelwrite::project::autosave_path(&d.path).exists());
        // undo and redo replay the log
        assert!(d.undo());
        assert!(d.shown(0));
        assert!(d.undo() && d.undo());
        assert_eq!(d.scene.entities.len(), 5);
        assert!(d.redo());
        assert_eq!(d.scene.entities.len(), 6);
        d.go_to(4);
        assert_eq!(d.scene.entities.len(), 7);
        assert!(!d.shown(0));
        d.save().unwrap();
        assert!(d.saved && !bc2_levelwrite::project::autosave_path(&d.path).exists());
        // reopened, the same scene
        let p = MapProject::load(&d.path).unwrap();
        let e = Doc::with(p, d.path.clone(), crate::demo::scene());
        assert_eq!(e.scene.entities.len(), 7);
        assert_eq!(e.scene.entities[1].position(), d.scene.entities[1].position());
        assert!(!e.has_partition());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
