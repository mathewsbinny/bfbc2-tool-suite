//! A level's scene: its entities (types, transforms, blueprints, categories, teams), outlines (areas and routes),
//! collision, terrain and water, loaded as `core/bfbc2core/level.py`'s `load` builds a `Level`, step for step, on the
//! compact [`DbxTree`] and on all cores.
//!
//! What stays with the caller: finding the level's folders and files, preparing a level that is not prepared, and
//! unpacking its terrain bundle (they run tools and write files); [`LoadOptions`] names what they found. The Python
//! loader keeps its own element trees for `Entity.element`; [`Source`] says where each entity's element is.
//!
//! For editing (`EDITOR_ARCHITECTURE.md` 3.2): entities are owned and mutable ([`Scene::set_transform`] marks an
//! entity changed, [`Scene::changed`]), collision is kept once per blueprint in local space ([`Collision`]) and placed
//! by each entity's transform ([`Scene::world_triangles`]), so moving an object never copies its triangles; the
//! partitions' bytes are kept ([`Scene::file`]) so a partition can be re-read ([`Scene::tree`]) or converted to an
//! editable `DbxFile` when it is written.
//!
//! ```no_run
//! use bc2_scene::scene::{self, LoadOptions};
//!
//! let options = LoadOptions::new("`MP_005`", r"work\levels\mp_005\dist\levels\`MP_005`.dbx", vec![
//!     r"work\levels\mp_005\pkg".into(),
//!     r"work\levels\mp_005\dist".into(),
//! ]);
//! let level = scene::load(&options)?;
//! let flags = level.entities.iter().filter(|e| e.category == "capture point").count();
//! # Ok::<(), bc2_scene::scene::SceneError>(())
//! ```

use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bc2_formats::dbx::{self, DbxTree, Node};
use bc2_formats::havok::{self, Triangle};
use bc2_formats::terrain::{self, Heightfield};
use bc2_formats::water;
use rayon::prelude::*;
use thiserror::Error;

use crate::collision;

/// Entity categories by type suffix (DESIGN.md open question 6), in `level.CATEGORIES`' order.
const CATEGORIES: [(&str, &[&str]); 8] = [
    ("soldier spawn", &["SoldierSpawnEntityData", "AlternateSpawnEntityData"]),
    ("vehicle spawn", &["VehicleSpawnEntityData"]),
    ("objective", &["MissionObjectiveEntityData", "ArmDisarmAwardEntityData"]),
    ("capture point", &["CapturePointEntityData"]),
    (
        "combat area",
        &["CombatAreaTriggerEntityData", "DamageAreaTriggerEntityData"],
    ),
    (
        "ai order",
        &[
            "AreaOrderEntityData",
            "PatrolOrderEntityData",
            "InterludeOrderEntityData",
            "FireAtAreaOrderEntityData",
        ],
    ),
    ("area point", &["AreaGeometryEntityData"]),
    ("placed object", &["ReferenceObjectData"]),
];

/// Placed objects by what their blueprint's object is (`level.OBJECT_CATEGORIES`).
const OBJECT_CATEGORIES: [(&str, &str); 5] = [
    ("LadderEntityData", "ladder"),
    ("DynamicModelEntityData", "dynamic object"),
    ("RigidModelEntityData", "dynamic object"),
    ("BangerEntityData", "dynamic object"),
    ("EffectEntityData", "effect"),
];

/// The fields holding outlines, and what they are (`level.SHAPE_FIELDS`).
const SHAPE_FIELDS: [(&str, &str); 3] = [
    ("Geometry", "area"),
    ("SurroundingGeometry", "outer area"),
    ("RouteWaypoints", "route"),
];

/// `level.category_of`: an entity's category from its type.
#[must_use]
pub fn category_of(type_name: &str) -> &'static str {
    for (name, suffixes) in CATEGORIES {
        for s in suffixes {
            if let Some(head) = type_name.strip_suffix(s)
                && head.ends_with('.')
            {
                return name;
            }
        }
    }
    "other"
}

/// A height field's mesh: vertices (x, height, z) and triangles.
pub type TerrainMesh = (Vec<[f32; 3]>, Vec<[u32; 3]>);

/// Why a level could not be loaded (where the Python loader raises).
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SceneError {
    /// A file could not be read.
    #[error("{path}: {source}")]
    Io {
        /// The file.
        path: PathBuf,
        /// The error.
        source: std::io::Error,
    },
    /// The level partition or a static-group file is empty (a bundle's *deleted* marker), not a binary `.dbx`, or
    /// damaged.
    #[error("{path}: {why}")]
    Dbx {
        /// The file.
        path: PathBuf,
        /// What is wrong (the DBX reader's message).
        why: String,
    },
    /// An element the loader reads has a value it cannot read (a transform or height that is not 4-byte numbers).
    #[error("{path}: {why}")]
    Value {
        /// The partition.
        path: PathBuf,
        /// What is wrong.
        why: String,
    },
    /// A blueprint's Havok packfile is damaged.
    #[error("{path}: damaged Havok packfile ({source})")]
    Havok {
        /// The packfile.
        path: PathBuf,
        /// The reader's error.
        source: havok::HavokError,
    },
}

/// The result type of this module.
pub type Result<T, E = SceneError> = std::result::Result<T, E>;

/// Where an entity's element is in the partitions the scene read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// An instance of the level partition (file 0): its position among the partition's instances.
    Instance {
        /// The instance's position among the partition's `instance` elements.
        instance: usize,
    },
    /// A member of a static model group: the group file (1 ...), its instance, and the item of its `Assets`.
    GroupMember {
        /// The file (1: the first group file).
        file: usize,
        /// The instance's position among the file's `instance` elements.
        instance: usize,
        /// The item's position in the instance's `Assets`.
        item: usize,
    },
}

/// An entity, as `level.Entity`.
#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    /// Its position in [`Scene::entities`].
    pub index: usize,
    /// The instance's guid (`<group instance guid>#<item>` for a group member).
    pub guid: String,
    /// The full type (`Entity.ReferenceObjectData`, `StaticGroupMember`).
    pub type_name: String,
    /// The type after its last `.`.
    pub short_type: String,
    /// The category (`level.category_of`, then refined by the blueprint for placed objects).
    pub category: String,
    /// The instance's `id` attribute.
    pub id: Option<String>,
    /// Its `Name` (a group member: its blueprint's last path part).
    pub name: Option<String>,
    /// The partition it comes from (`levels/<Level>`, or the group file's name without `.dbx`).
    pub partition: String,
    /// 4 x 4, row by row (rows: right, up, forward, translation), from the stored singles.
    pub matrix: Option<[f64; 16]>,
    /// The lower-case blueprint path.
    pub blueprint: Option<String>,
    /// The static-group file a member comes from.
    pub group: Option<String>,
    /// Placed objects: the type of the blueprint's object.
    pub blueprint_type: Option<String>,
    /// Placed objects: the blueprint has health states or breakable parts.
    pub destructible: bool,
    /// 1, 2, or 0 (neutral).
    pub team: Option<u8>,
    /// Soldier spawns: AI soldiers (`true`) or human players.
    pub ai: Option<bool>,
    /// Where its element is.
    pub source: Source,
}

impl Entity {
    /// The translation row of its transform.
    #[must_use]
    pub fn position(&self) -> Option<[f64; 3]> {
        self.matrix.map(|m| [m[12], m[13], m[14]])
    }
}

/// An outline, as `level.Shape`.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// The entity whose field it is.
    pub entity: usize,
    /// `area`, `outer area` or `route`.
    pub kind: &'static str,
    /// World positions in chain order.
    pub points: Vec<[f64; 3]>,
    /// Areas are rings; routes are open.
    pub closed: bool,
    /// Areas: the prism height above the points (0: flat).
    pub height: f64,
    /// The field (`Geometry`, `SurroundingGeometry`, `RouteWaypoints`).
    pub field: &'static str,
}

/// The level's collision, kept once per blueprint in local space.
#[derive(Debug, Clone, Default)]
pub struct Collision {
    /// The local-space triangles of each blueprint that has a packfile (`None`: the packfile has none, or its
    /// reader refuses it).
    pub blueprints: HashMap<String, Option<Arc<Vec<Triangle>>>>,
    /// The placed entities with collision, in entity order: (entity, blueprint).
    pub meshes: Vec<(usize, String)>,
    /// The blueprints of placed entities without collision, sorted.
    pub missing: Vec<String>,
    /// Round VW1: each blueprint's destructible parts in its packfile ([`havok::Parts`]: the part names' hashes and
    /// each triangle's part, aligned with [`Collision::blueprints`]), for the blueprints whose triangles were read.
    pub parts: HashMap<String, Arc<havok::Parts>>,
}

/// The water, as `water.Water`.
#[derive(Debug, Clone, PartialEq)]
pub struct Water {
    /// The surface.
    pub mesh: water::WaterMesh,
    /// The shader of each triangle and the slots, when the `VisualWater` matches the mesh.
    pub shaders: Option<(Vec<String>, Vec<String>)>,
}

/// What [`load`] reads; the paths are found by the caller (`level.load`).
#[derive(Debug, Clone)]
pub struct LoadOptions {
    /// The level's name as its partition is named (`MP_005`).
    pub name: String,
    /// The level partition.
    pub partition: PathBuf,
    /// The static-group files (`*_HavokAsset.dbx`), in order; read only with `geometry`.
    pub groups: Vec<PathBuf>,
    /// The folders blueprints are looked up in, in order (Package, Dist, the shared data).
    pub bases: Vec<PathBuf>,
    /// Collision, terrain and water too (`level.load(geometry=True)`).
    pub geometry: bool,
    /// The terrain's height field file, when there is one.
    pub heightfield: Option<PathBuf>,
    /// The height field mesh's stride.
    pub terrain_stride: usize,
    /// The water's `.Water.Mesh.res` and `.Water.res`, when there is a mesh.
    pub water: Option<(PathBuf, Option<PathBuf>)>,
    /// The folder of the mesh-alias cache ([`collision::index_cached`]); `None`: every partition is read.
    pub alias_cache: Option<PathBuf>,
}

impl LoadOptions {
    /// Options for `name`'s partition and bases: entities only, stride 2.
    pub fn new(name: impl Into<String>, partition: impl Into<PathBuf>, bases: Vec<PathBuf>) -> Self {
        Self {
            name: name.into(),
            partition: partition.into(),
            groups: Vec::new(),
            bases,
            geometry: false,
            heightfield: None,
            terrain_stride: 2,
            water: None,
            alias_cache: None,
        }
    }
}

/// A loaded level.
#[derive(Debug, Clone, Default)]
pub struct Scene {
    /// The level's name.
    pub name: String,
    /// Every entity: the level partition's instances in order, then the static groups' members.
    pub entities: Vec<Entity>,
    /// Outlines: areas and routes.
    pub shapes: Vec<Shape>,
    /// Collision (with geometry).
    pub collision: Option<Collision>,
    /// The terrain height field (with geometry, when it was read); shared by the scene's clones (an undo's scene is
    /// a clone of the base's).
    pub heightfield: Option<Arc<Heightfield>>,
    /// The height field's mesh at the stride asked for: vertices and triangles; shared by the scene's clones, and
    /// the key of its ray grid ([`crate::raygrid::RayCache`]). An edit replaces it (a new `Arc`).
    pub terrain: Option<Arc<TerrainMesh>>,
    /// The water (with geometry, when the level has a water mesh and it was read).
    pub water: Option<Water>,
    /// What could not be read, as `Level.notes`.
    pub notes: Vec<String>,
    /// The partitions as read: shared by the scene's clones (never edited).
    files: Arc<Vec<(PathBuf, Vec<u8>)>>,
    changed: BTreeSet<usize>,
    /// The ray structures (round HG1: [`crate::raygrid`]), built when a ray first needs them; shared by the scene's
    /// clones (each structure is kept with what it was built from).
    rays: crate::raygrid::RayCache,
}

impl Scene {
    /// A clone of the scene with `entities` and `shapes` in place of its own, which are not copied (an undo's
    /// scene: the base's, with a checkpoint's entities). Each entity with a transform that differs from this
    /// scene's is marked changed, as [`Scene::set_transform`] marks it.
    #[must_use]
    pub fn with_entities(&self, entities: Vec<Entity>, shapes: Vec<Shape>) -> Self {
        let mut changed = self.changed.clone();
        for (k, e) in entities.iter().enumerate() {
            if e.matrix.is_some() && self.entities.get(k).is_some_and(|b| b.matrix != e.matrix) {
                changed.insert(k);
            }
        }
        Self {
            name: self.name.clone(),
            entities,
            shapes,
            collision: self.collision.clone(),
            heightfield: self.heightfield.clone(),
            terrain: self.terrain.clone(),
            water: self.water.clone(),
            notes: self.notes.clone(),
            files: Arc::clone(&self.files),
            changed,
            rays: self.rays.clone(),
        }
    }

    /// The partitions the entities come from (the level partition, then the group files): path and bytes.
    #[must_use]
    pub fn file(&self, k: usize) -> Option<(&Path, &[u8])> {
        self.files.get(k).map(|(p, d)| (p.as_path(), d.as_slice()))
    }

    /// Partition `k`'s tree, read again from its kept bytes.
    ///
    /// # Errors
    ///
    /// The reader's error (it read the same bytes when the scene was loaded).
    #[must_use]
    pub fn tree(&self, k: usize) -> Option<dbx::Result<DbxTree<'_>>> {
        self.files.get(k).map(|(_, d)| dbx::parse_tree(d))
    }

    /// Moves entity `index` (a new 4 x 4 transform, row by row) and marks it changed; `false` when there is no
    /// such entity.
    pub fn set_transform(&mut self, index: usize, matrix: [f64; 16]) -> bool {
        let Some(e) = self.entities.get_mut(index) else {
            return false;
        };
        e.matrix = Some(matrix);
        self.changed.insert(index);
        true
    }

    /// The entities changed since the scene was loaded.
    #[must_use]
    pub fn changed(&self) -> &BTreeSet<usize> {
        &self.changed
    }

    /// The terrain triangles (indices into [`Scene::terrain`]'s triangles, ascending) a ray from `origin` along
    /// `dir` can meet: those of the grid cells its plan passes over ([`crate::raygrid::TerrainGrid`], built on the
    /// first call). `None` when every triangle is to be tested (the ray is not finite); empty without a terrain.
    #[must_use]
    pub fn terrain_candidates(&self, origin: [f64; 3], dir: [f64; 3]) -> Option<Vec<u32>> {
        match &self.terrain {
            Some(mesh) => self.rays.terrain(mesh).candidates(origin, dir),
            None => Some(Vec::new()),
        }
    }

    /// The farthest corner of a blueprint's collision triangles from its origin ([`crate::raygrid::radius_of`]),
    /// computed once per triangle list.
    #[must_use]
    pub fn collision_radius(&self, tris: &Arc<Vec<Triangle>>) -> f64 {
        self.rays.radius(tris)
    }

    /// Entity `index`'s collision in world space (its blueprint's triangles placed by its transform), if it has any.
    #[must_use]
    pub fn world_triangles(&self, index: usize) -> Option<Vec<[[f64; 3]; 3]>> {
        let e = self.entities.get(index)?;
        let m = e.matrix?;
        let t = self
            .collision
            .as_ref()?
            .blueprints
            .get(e.blueprint.as_deref()?)?
            .as_ref()?;
        Some(t.iter().map(|tri| tri.map(|v| place(&m, v))).collect())
    }

    /// The blueprints' collision shapes (radius and run bounds, [`crate::raygrid::Shape`]), each measured once per
    /// triangle list; locked while the answer is held (a ray's walk over the placed objects).
    #[must_use]
    pub fn collision_shapes(&self) -> crate::raygrid::Shapes<'_> {
        self.rays.shapes()
    }
}

/// A blueprint's local point placed by an entity's transform (row by row: three axes, then the position), as
/// [`Scene::world_triangles`] places each corner.
#[must_use]
pub fn place(m: &[f64; 16], v: [f32; 3]) -> [f64; 3] {
    std::array::from_fn(|c| {
        let col = |r: usize| m.get(r * 4 + c).copied().unwrap_or(0.0);
        f64::from(v[0]) * col(0) + f64::from(v[1]) * col(1) + f64::from(v[2]) * col(2) + col(3)
    })
}

// ------------------------------------------------------------------------------------------------ reading

fn read(path: &Path) -> Result<Vec<u8>> {
    std::fs::read(path).map_err(|source| SceneError::Io {
        path: path.to_owned(),
        source,
    })
}

/// `dbx.parse`'s checks and tree: an empty file is a *deleted* marker, a file without the magic is not binary.
fn tree_of<'a>(path: &Path, data: &'a [u8]) -> Result<DbxTree<'a>> {
    let why = |why: String| SceneError::Dbx {
        path: path.to_owned(),
        why,
    };
    if data.is_empty() {
        return Err(why("empty file: a *deleted* bundle entry".into()));
    }
    if data.len() <= 24 || !data.starts_with(b"{binary}") {
        return Err(why("not a binary .dbx file".into()));
    }
    dbx::parse_tree(data).map_err(|e| why(e.to_string()))
}

fn ref_of(node: Option<Node<'_, '_>>, name: &str) -> Option<String> {
    node?.child(name)?.attr("ref").map(str::to_owned)
}

/// `dbx.floats` of a numbers element, as the loader calls it: an error where the Python reader raises.
fn floats_of(node: &Node<'_, '_>, path: &Path, what: &str) -> Result<Vec<f32>> {
    node.floats().ok_or_else(|| SceneError::Value {
        path: path.to_owned(),
        why: format!("{what}: not 4-byte numbers"),
    })
}

fn matrix_of(values: &[f32]) -> Option<[f64; 16]> {
    (values.len() == 16).then(|| std::array::from_fn(|k| values.get(k).copied().map_or(0.0, f64::from)))
}

/// `rsplit('/', 1)[0]`.
fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(p, _)| p)
}

/// `re.sub(suffix + '$', '', s)`: the suffix at the end, or just before a final newline, removed.
fn strip_end(s: &str, suffix: &str) -> String {
    if let Some(p) = s.strip_suffix(suffix) {
        p.to_owned()
    } else if let Some(p) = s.strip_suffix('\n').and_then(|t| t.strip_suffix(suffix)) {
        format!("{p}\n")
    } else {
        s.to_owned()
    }
}

/// What the entity steps after their creation read from an entity's element.
#[derive(Debug, Default)]
struct Facts {
    referenced_object: Option<String>,
    alternatives: Vec<String>,
    shape_refs: [Vec<String>; 3],
    next: Option<String>,
    height: Option<std::result::Result<f64, String>>,
}

fn facts_of(n: &Node<'_, '_>) -> Facts {
    let refs = |name: &str| -> Vec<String> {
        n.child(name).map_or_else(Vec::new, |c| {
            c.children().map(|i| i.attr("ref").unwrap_or("").to_owned()).collect()
        })
    };
    let height = n.child("Height").filter(|h| h.kind() == dbx::NUMBERS).map(|h| {
        h.floats()
            .and_then(|v| v.first().copied())
            .map(f64::from)
            .ok_or_else(|| "Height: not 4-byte numbers or empty".to_owned())
    });
    Facts {
        referenced_object: n
            .child("ReferencedObject")
            .and_then(|c| c.attr("ref").map(str::to_owned)),
        alternatives: refs("AlternativeSpawnPoints"),
        shape_refs: SHAPE_FIELDS.map(|(f, _)| refs(f)),
        next: n.child("Next").map(|c| c.attr("ref").unwrap_or("").to_owned()),
        height,
    }
}

fn team_of(n: &Node<'_, '_>) -> (Option<u8>, Option<bool>) {
    let mut team = None;
    for name in ["Team", "InitialOwnerTeam"] {
        if let Some(c) = n.child(name)
            && c.kind() == dbx::TEXT
            && let Some(t) = match c.text() {
                "Team1" => Some(1),
                "Team2" => Some(2),
                "TeamNeutral" => Some(0),
                _ => None,
            }
        {
            team = Some(t);
            break;
        }
    }
    let ai = n
        .child("PlayerType")
        .filter(|c| c.kind() == dbx::TEXT && !c.text().is_empty())
        .map(|c| c.text() != "SSHumanPlayer");
    (team, ai)
}

/// The entities of the level partition and the group files (`level.load`'s first steps), with their facts.
/// The entities, their facts, and the terrain's name (`level.terrain_name`) when the partition has a terrain entity.
type Entities = (Vec<Entity>, Vec<Facts>, Option<Option<String>>);

fn entities_of(name: &str, files: &[(PathBuf, Vec<u8>)]) -> Result<Entities> {
    let trees: Vec<DbxTree<'_>> = files
        .par_iter()
        .map(|(p, d)| tree_of(p, d))
        .collect::<Vec<_>>()
        .into_iter()
        .collect::<Result<_>>()?;
    let mut entities = Vec::new();
    let mut facts = Vec::new();
    let mut terrain: Option<Option<String>> = None;
    let (Some(level), Some((level_path, _))) = (trees.first(), files.first()) else {
        return Ok((entities, facts, terrain));
    };
    let partition = format!("levels/{name}");
    for (k, el) in level.instances().enumerate() {
        let type_name = el.attr("type").unwrap_or("").to_owned();
        if terrain.is_none() && type_name.ends_with(".TerrainEntityData") {
            // level.terrain_name: the first TerrainEntityData's TerrainAsset (terrains/<name>/<name>)
            let a = el.child("TerrainAsset").and_then(|a| a.attr("ref")).unwrap_or("");
            let parts: Vec<&str> = a.split('/').collect();
            terrain = Some(parts.get(1).filter(|_| parts.len() >= 3).map(|p| p.to_lowercase()));
        }
        let matrix = match el.child("Transform").filter(|t| t.kind() == dbx::NUMBERS) {
            Some(t) => matrix_of(&floats_of(&t, level_path, "Transform")?),
            None => None,
        };
        let blueprint = el
            .child("ReferencedObject")
            .and_then(|r| r.attr("ref"))
            .filter(|r| !r.is_empty() && r.contains('/'))
            .map(|r| parent(r).to_lowercase());
        let (team, ai) = team_of(&el);
        entities.push(Entity {
            index: entities.len(),
            guid: el.attr("guid").unwrap_or("").to_owned(),
            short_type: type_name.rsplit('.').next().unwrap_or("").to_owned(),
            category: category_of(&type_name).to_owned(),
            type_name,
            id: el.attr("id").map(str::to_owned),
            name: el
                .child("Name")
                .filter(|c| c.kind() == dbx::TEXT)
                .map(|c| c.text().to_owned()),
            partition: partition.clone(),
            matrix,
            blueprint,
            group: None,
            blueprint_type: None,
            destructible: false,
            team,
            ai,
            source: Source::Instance { instance: k },
        });
        facts.push(facts_of(&el));
    }
    group_members(&trees, files, &mut entities, &mut facts)?;
    Ok((entities, facts, terrain))
}

/// The static groups' members: one entity per `Assets` item of each group instance, placed by `Transforms`.
fn group_members(
    trees: &[DbxTree<'_>],
    files: &[(PathBuf, Vec<u8>)],
    entities: &mut Vec<Entity>,
    facts: &mut Vec<Facts>,
) -> Result<()> {
    for (file, (tree, (path, _))) in trees.iter().zip(files).enumerate().skip(1) {
        let base = path
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        let stem: String = base.chars().take(base.chars().count().saturating_sub(4)).collect();
        for (k, el) in tree.instances().enumerate() {
            let (Some(tr), Some(assets)) = (el.child("Transforms"), el.child("Assets")) else {
                continue;
            };
            if tr.kind() != dbx::NUMBERS || assets.children().next().is_none() {
                continue;
            }
            let vals = floats_of(&tr, path, "Transforms")?;
            let guid = el.attr("guid").unwrap_or("None");
            for (item_k, item) in assets.children().enumerate() {
                let Some(m) = vals.get(16 * item_k..16 * (item_k + 1)) else {
                    break;
                };
                let bp = strip_end(&parent(item.attr("ref").unwrap_or("")).to_lowercase(), "_havok");
                let (team, ai) = team_of(&item);
                entities.push(Entity {
                    index: entities.len(),
                    guid: format!("{guid}#{item_k}"),
                    type_name: "StaticGroupMember".into(),
                    short_type: "StaticGroupMember".into(),
                    category: "group member".into(),
                    id: None,
                    name: Some(bp.rsplit('/').next().unwrap_or("").to_owned()),
                    partition: stem.clone(),
                    matrix: matrix_of(m),
                    blueprint: Some(bp),
                    group: Some(base.clone()),
                    blueprint_type: None,
                    destructible: false,
                    team,
                    ai,
                    source: Source::GroupMember {
                        file,
                        instance: k,
                        item: item_k,
                    },
                });
                facts.push(facts_of(&item));
            }
        }
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------ blueprints

/// What the loader needs of a blueprint partition: each instance's type and `Object` reference by lower-case guid
/// (the last of a repeated guid), and whether any of those has health states or breakable part links.
#[derive(Debug, Default)]
struct Blueprint {
    instances: HashMap<String, (String, Option<String>)>,
    broken_up: bool,
}

fn blueprint_of(data: &[u8]) -> Blueprint {
    let Ok(tree) = dbx::parse_tree(data) else {
        return Blueprint::default();
    };
    if data.len() <= 24 || !data.starts_with(b"{binary}") {
        return Blueprint::default();
    }
    let mut by_guid: HashMap<String, (String, Option<String>, bool)> = HashMap::new();
    for i in tree.instances() {
        let broken = ["HealthStates", "PartLinks"]
            .iter()
            .any(|n| i.child(n).is_some_and(|c| c.children().next().is_some()));
        by_guid.insert(
            i.attr("guid").unwrap_or("").to_lowercase(),
            (
                i.attr("type").unwrap_or("").to_owned(),
                ref_of(Some(i), "Object"),
                broken,
            ),
        );
    }
    Blueprint {
        broken_up: by_guid.values().any(|(_, _, b)| *b),
        instances: by_guid.into_iter().map(|(g, (t, o, _))| (g, (t, o))).collect(),
    }
}

/// The blueprint partitions read so far, by the path the references give (`level.resolve_blueprints`' `files`).
struct Blueprints<'b> {
    bases: &'b [PathBuf],
    files: HashMap<String, Blueprint>,
}

impl Blueprints<'_> {
    /// The partitions of `wanted` not read yet, read and parsed on all cores; one that cannot be read has no
    /// instances.
    fn preload(&mut self, wanted: impl IntoIterator<Item = String>) {
        let mut todo = Vec::new();
        for path in wanted {
            if self.files.contains_key(&path) {
                continue;
            }
            self.files.insert(path.clone(), Blueprint::default());
            todo.push(path);
        }
        let bases = self.bases;
        let read: Vec<(String, Blueprint)> = todo
            .into_par_iter()
            .map(|path| {
                let file = bases
                    .iter()
                    .map(|b| b.join(format!("{path}.dbx")))
                    .find(|p| p.is_file());
                let bp = file
                    .and_then(|p| std::fs::read(p).ok())
                    .map_or_else(Blueprint::default, |d| blueprint_of(&d));
                (path, bp)
            })
            .collect();
        self.files.extend(read);
    }

    fn get(&mut self, path: &str) -> &Blueprint {
        static EMPTY: std::sync::OnceLock<Blueprint> = std::sync::OnceLock::new();
        if !self.files.contains_key(path) {
            self.preload([path.to_owned()]);
        }
        self.files
            .get(path)
            .unwrap_or_else(|| EMPTY.get_or_init(Blueprint::default))
    }
}

/// `level.resolve_blueprints`: placed objects' blueprint type, destructibility and category.
fn resolve_blueprints(entities: &mut [Entity], facts: &[Facts], bases: &[PathBuf]) {
    let mut bps = Blueprints {
        bases,
        files: HashMap::new(),
    };
    let refs: Vec<(usize, String)> = entities
        .iter()
        .zip(facts)
        .filter(|(e, _)| e.short_type == "ReferenceObjectData")
        .filter_map(|(e, f)| f.referenced_object.clone().map(|r| (e.index, r)))
        .filter(|(_, r)| !r.is_empty() && r.contains('/'))
        .collect();
    bps.preload(refs.iter().map(|(_, r)| parent(r).to_owned()));
    let mut second = Vec::new();
    for (_, x) in &refs {
        let (path, guid) = x.rsplit_once('/').unwrap_or((x, ""));
        if let Some((_, Some(oref))) = bps.get(path).instances.get(&guid.to_lowercase())
            && oref != "null"
            && oref.contains('/')
        {
            second.push(parent(oref).to_owned());
        }
    }
    bps.preload(second);
    for (index, x) in refs {
        let (path, guid) = x.rsplit_once('/').unwrap_or((&x, ""));
        let bp = bps.get(path);
        let broken = bp.broken_up;
        let found = bp.instances.get(&guid.to_lowercase()).cloned();
        let obj = match &found {
            Some((_, Some(oref))) if !oref.is_empty() && oref != "null" => match oref.rsplit_once('/') {
                Some((p2, g2)) => bps.get(p2).instances.get(&g2.to_lowercase()).cloned(),
                None => bps.get(path).instances.get(&oref.to_lowercase()).cloned(),
            },
            _ => found,
        };
        let Some((obj_type, _)) = obj else { continue };
        let Some(e) = entities.get_mut(index) else { continue };
        let bt = obj_type.rsplit('.').next().unwrap_or("").to_owned();
        e.destructible = bt == "StaticModelEntityData" && broken;
        if e.blueprint
            .as_deref()
            .unwrap_or("")
            .starts_with("objects/objects/mpobjective")
        {
            e.category = "objective".into();
        } else if e.destructible {
            e.category = "destructible".into();
        } else if let Some((_, c)) = OBJECT_CATEGORIES.iter().find(|(t, _)| *t == bt) {
            e.category = (*c).into();
        }
        e.blueprint_type = Some(bt);
    }
}

/// `level.assign_teams`' second step: alternate spawn points take their spawner's team.
fn assign_alternates(entities: &mut [Entity], facts: &[Facts], guids: &HashMap<String, usize>) {
    for (k, f) in facts.iter().enumerate() {
        for r in &f.alternatives {
            let Some(&t) = guids.get(&r.to_lowercase()) else {
                continue;
            };
            let (team, ai) = entities.get(k).map_or((None, None), |e| (e.team, e.ai));
            if let Some(target) = entities.get_mut(t)
                && target.team.is_none()
            {
                target.team = team;
                target.ai = ai;
            }
        }
    }
}

/// `level.shapes_of`: chains of points (`Next` links) referenced by the shape fields.
fn shapes_of(entities: &[Entity], facts: &[Facts], guids: &HashMap<String, usize>, path: &Path) -> Result<Vec<Shape>> {
    let mut out = Vec::new();
    for (e, f) in entities.iter().zip(facts) {
        for ((field, kind), items) in SHAPE_FIELDS.iter().zip(&f.shape_refs) {
            for r in items {
                let mut p = guids.get(&r.to_lowercase()).copied();
                let (mut pts, mut seen, mut closed, mut height) = (Vec::new(), Vec::<&str>::new(), false, 0.0f64);
                while let Some(k) = p {
                    let (Some(pe), Some(pf)) = (entities.get(k), facts.get(k)) else {
                        break;
                    };
                    let Some(pos) = pe.position() else { break };
                    if seen.contains(&pe.guid.as_str()) {
                        closed = true;
                        break;
                    }
                    seen.push(&pe.guid);
                    pts.push(pos);
                    if let Some(h) = &pf.height
                        && height == 0.0
                    {
                        height = h.clone().map_err(|why| SceneError::Value {
                            path: path.to_owned(),
                            why,
                        })?;
                    }
                    p = pf.next.as_ref().and_then(|n| guids.get(&n.to_lowercase()).copied());
                }
                if pts.len() >= 2 {
                    out.push(Shape {
                        entity: e.index,
                        kind,
                        points: pts,
                        closed: closed && *kind != "route",
                        height,
                        field,
                    });
                }
            }
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ geometry

/// The terrain and its mesh, or the note `level.load` writes.
fn terrain_of(path: &Path, stride: usize) -> std::result::Result<(Heightfield, TerrainMesh), String> {
    let note = |e: &dyn std::fmt::Display| format!("terrain height field not read: {e}");
    let data = std::fs::read(path).map_err(|e| note(&e))?;
    let hf = terrain::parse(&data).map_err(|e| note(&format!("{}: {e}", path.display())))?;
    let mesh = hf.mesh(stride);
    Ok((hf, mesh))
}

/// The water, or the note `level.load` writes.
fn water_of(mesh: &Path, visual: Option<&Path>) -> std::result::Result<Water, String> {
    let note = |e: &dyn std::fmt::Display| format!("water not read: {e}");
    let data = std::fs::read(mesh).map_err(|e| note(&e))?;
    let m = water::parse_mesh(&data).map_err(|e| note(&format!("{}: {e}", mesh.display())))?;
    let mut shaders = None;
    if let Some(v) = visual
        && !m.triangles.is_empty()
    {
        let visual = std::fs::read(v).map_err(|e| note(&e))?;
        if let Ok(vw) = water::parse_visual(&visual) {
            shaders = water::triangle_shaders(&m, &vw).map(|s| (s, vw.slots));
        }
    }
    Ok(Water { mesh: m, shaders })
}

/// A blueprint's packfile as read: its triangles and its parts.
type BlueprintCollision = (Option<Arc<Vec<Triangle>>>, Option<Arc<havok::Parts>>);

/// Each placed entity's collision (`level.load`'s collision step): the packfiles of the blueprints, read on all
/// cores; the first damaged one in entity order is the error.
fn collision_of(entities: &[Entity], havok: &collision::FirstWins<PathBuf>) -> Result<Collision> {
    let placed: Vec<&Entity> = entities.iter().filter(|e| e.matrix.is_some()).collect();
    let mut wanted: Vec<&str> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in &placed {
        if let Some(bp) = e.blueprint.as_deref()
            && seen.insert(bp)
            && havok.contains(bp)
        {
            wanted.push(bp);
        }
    }
    let read: Vec<Result<BlueprintCollision>> = wanted
        .par_iter()
        .map(|bp| {
            let path = havok.get(bp).cloned().unwrap_or_default();
            let data = read(&path)?;
            let tris = havok::triangles(&data).map_err(|source| SceneError::Havok {
                path: path.clone(),
                source,
            })?;
            // the parts only beside triangles (the same reading, so the same refusals); a part map that does not
            // line up is dropped
            let parts = match &tris {
                Some(t) => havok::parts(&data)
                    .ok()
                    .flatten()
                    .filter(|p| p.part_of.len() == t.len())
                    .map(Arc::new),
                None => None,
            };
            Ok((tris.map(Arc::new), parts))
        })
        .collect();
    let mut out = Collision::default();
    for (bp, r) in wanted.iter().zip(read) {
        let (t, parts) = r?;
        out.blueprints.insert((*bp).to_owned(), t);
        if let Some(p) = parts {
            out.parts.insert((*bp).to_owned(), p);
        }
    }
    let mut missing = BTreeSet::new();
    for e in placed {
        let Some(bp) = e.blueprint.as_deref() else { continue };
        if out.blueprints.get(bp).is_some_and(Option::is_some) {
            out.meshes.push((e.index, bp.to_owned()));
        } else {
            missing.insert(bp.to_owned());
        }
    }
    out.missing = missing.into_iter().collect();
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ load

/// What [`load`]'s geometry branch reads: the collision index, the terrain, the water (or their notes).
type Geometry = (
    Option<collision::CollisionIndex>,
    Option<std::result::Result<(Heightfield, TerrainMesh), String>>,
    Option<std::result::Result<Water, String>>,
);

/// Loads a level as `level.load` does: the entities, their blueprints, teams and outlines; with
/// [`LoadOptions::geometry`] the static groups' members, collision, terrain and water too. The partitions, the
/// collision index, the terrain and the water are read at once on all cores.
///
/// # Errors
///
/// [`SceneError`] where `level.load` raises: the level partition or a group file cannot be read, a transform or
/// height is not numbers, a blueprint's packfile is damaged. What `level.load` only notes (the mesh aliases, the
/// terrain, the water) goes to [`Scene::notes`].
pub fn load(options: &LoadOptions) -> Result<Scene> {
    let mut paths = vec![options.partition.clone()];
    if options.geometry {
        paths.extend(options.groups.iter().cloned());
    }
    let (entities, geometry): (Result<_>, Geometry) = rayon::join(
        || -> Result<_> {
            let files: Vec<(PathBuf, Vec<u8>)> = paths
                .par_iter()
                .map(|p| read(p).map(|d| (p.clone(), d)))
                .collect::<Vec<_>>()
                .into_iter()
                .collect::<Result<_>>()?;
            let (mut entities, facts, _terrain) = entities_of(&options.name, &files)?;
            resolve_blueprints(&mut entities, &facts, &options.bases);
            let mut guids = HashMap::new();
            for e in &entities {
                guids.insert(e.guid.to_lowercase(), e.index);
            }
            assign_alternates(&mut entities, &facts, &guids);
            let shapes = shapes_of(&entities, &facts, &guids, &options.partition)?;
            Ok((files, entities, shapes))
        },
        || {
            if !options.geometry {
                return (None, None, None);
            }
            let ((index, terrain), water) = rayon::join(
                || {
                    rayon::join(
                        || collision::index_cached(&options.bases, options.alias_cache.as_deref()),
                        || {
                            options
                                .heightfield
                                .as_ref()
                                .map(|p| terrain_of(p, options.terrain_stride))
                        },
                    )
                },
                || options.water.as_ref().map(|(m, v)| water_of(m, v.as_deref())),
            );
            (Some(index), terrain, water)
        },
    );
    let (files, entities, shapes) = entities?;
    let mut scene = Scene {
        name: options.name.clone(),
        entities,
        shapes,
        files: Arc::new(files),
        ..Scene::default()
    };
    let (index, terrain, water) = geometry;
    if let Some(index) = index {
        let havok = match index.resolved() {
            Ok(h) => h,
            Err(e) => {
                scene.notes.push(format!("blueprint mesh aliases not read: {e}"));
                index.havok
            }
        };
        scene.collision = Some(collision_of(&scene.entities, &havok)?);
    }
    match terrain {
        Some(Ok((hf, mesh))) => {
            scene.heightfield = Some(Arc::new(hf));
            scene.terrain = Some(Arc::new(mesh));
        }
        Some(Err(note)) => scene.notes.push(note),
        None => {}
    }
    match water {
        Some(Ok(w)) => scene.water = Some(w),
        Some(Err(note)) => scene.notes.push(note),
        None => {}
    }
    Ok(scene)
}

/// `level.terrain_name`: the terrain's name from the level partition's first `TerrainEntityData`'s `TerrainAsset`
/// (`terrains/mp_009/mp_009` for `MP_009GR`), lower case; `None` when it has none or the reference has fewer than three
/// parts.
///
/// # Errors
///
/// As [`load`] for the partition.
pub fn terrain_name(partition: &Path) -> Result<Option<String>> {
    let data = read(partition)?;
    let (_, _, t) = entities_of("", &[(partition.to_owned(), data)])?;
    Ok(t.flatten())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(category_of("Entity.SoldierSpawnEntityData"), "soldier spawn");
        assert_eq!(category_of("GameSharedResources.ReferenceObjectData"), "placed object");
        assert_eq!(category_of("SoldierSpawnEntityData"), "other");
        assert_eq!(category_of("Entity.XSoldierSpawnEntityData"), "other");
        assert_eq!(category_of(""), "other");
    }

    #[test]
    fn suffixes() {
        assert_eq!(strip_end("objects/a/a_havok", "_havok"), "objects/a/a");
        assert_eq!(strip_end("objects/a/a_havok\n", "_havok"), "objects/a/a\n");
        assert_eq!(strip_end("objects/a/a", "_havok"), "objects/a/a");
        assert_eq!(parent("a/b/c"), "a/b");
        assert_eq!(parent("abc"), "abc");
    }
}
