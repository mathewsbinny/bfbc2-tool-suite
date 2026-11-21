//! A level's whole drawable world (round R5b): its placed objects and model-group members with their shader-set
//! overrides ([`crate::placement`]), the terrain patches, the baked road and decal strips, the visual
//! water, the weather's light and DICE's post-process chain - what the editor's game-accurate view and the examples
//! draw.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use crate::error::RenderError;
use crate::files::{Files, level_folders};
use crate::frame::{self, Camera, Frame, Instance};
use crate::material::{self, Db, Prepared, TextureFiles};
use crate::placement::{self, Placements};
use crate::post;
use crate::terrain::Terrain;
use crate::weather::{self, Post, Values};

/// A placed thing the level draws: a level-partition instance or a group member.
#[derive(Debug, Clone)]
pub struct Placed {
    /// The instance's GUID (a group member's is ignored).
    pub guid: String,
    /// Its blueprint (`objects/...`).
    pub blueprint: String,
    /// The stored transform (rows right, up, forward, translation).
    pub rows: [f64; 16],
    /// A group member's group file (`StaticModelGroupEntity_HavokAsset.dbx`) and item.
    pub group: Option<(String, usize)>,
}

/// A mesh's levels of detail past the first and where the game switches between them (round R5d).
#[derive(Debug, Clone)]
pub struct Lods {
    /// The mesh set's LOD settings.
    pub settings: material::LodSettings,
    /// Levels 1, 2 ... prepared (`None`: that level draws nothing).
    pub levels: Vec<Option<Arc<Prepared>>>,
}

/// What was drawn, counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    /// Placed objects drawn (level partition).
    pub objects: usize,
    /// Group members drawn.
    pub members: usize,
    /// Different prepared meshes (mesh and shader set).
    pub meshes: usize,
    /// Objects and members drawn with a shader-set override.
    pub overridden: usize,
    /// Strip draws.
    pub strips: usize,
    /// Water draws.
    pub water: usize,
}

/// Patches drawn on each side of the patch under the camera's target when the terrain is drawn as a window
/// ([`World::terrain_radius`]; rounds R5 to R5c drew only this window: 49 x 49 patches of 16 m, 784 m across).
pub const TERRAIN_RADIUS: usize = 24;

/// The level, ready to draw the game's way.
pub struct World {
    /// The level's files.
    pub files: Files,
    /// Its Dx11 shader database.
    pub db: Db,
    /// Texture resources read.
    pub textures: TextureFiles,
    /// The weather's engine light values.
    pub light: Values,
    /// The weather's post-process settings.
    pub post: Post,
    /// The weather state drawn (its partitions read).
    pub weather: weather::Weather,
    /// The level's weather as data (round VW2: its system's states, `ActiveState`, the spawn switch), when it
    /// resolves.
    pub level_weather: Option<weather::LevelWeather>,
    /// The state [`World::set_state`] chose (`None`: the one the load resolved, the settings' `ActiveState`).
    pub state_shown: Option<usize>,
    /// The clear colour (the fog colour; the sky is not drawn yet).
    pub background: [f32; 3],
    /// No weather was found: the level is drawn under [`weather::neutral`] (round GW1; the editor says
    /// [`weather::NO_WEATHER`]).
    pub default_weather: bool,
    /// The placed objects and group members.
    pub objects: Vec<Instance>,
    /// The terrain.
    pub terrain: Option<Terrain>,
    /// The visual water.
    pub water: Option<Arc<Prepared>>,
    /// The strips, one per road block.
    pub strips: Vec<Arc<Prepared>>,
    /// The weather's sky dome (`EnvironmentMain`, round R5d), when the database and the weather have one.
    pub sky: Option<Arc<Prepared>>,
    /// Draw the sky dome ([`World::sky`]) in [`World::sources`]: off by default - the C++ harness (`shaderview`) and
    /// R5b's per-frame list, which the live list is proven against, draw no sky; the editor turns it on.
    pub sky_pass: bool,
    /// DICE's post shaders (`None`: the startup bundle could not be read; the harness tonemap is used).
    pub post_shaders: Option<post::Shaders>,
    /// The level partition's facts.
    pub placements: Placements,
    /// What was found while loading.
    pub notes: Vec<String>,
    /// What is drawn.
    pub counts: Counts,
    /// Each object's place in the placed list it was made from (`objects[k]` is `placed[object_of[k]]`).
    pub object_of: Vec<usize>,
    /// Each object's identity (its GUID, or its group file and item).
    pub object_keys: Vec<String>,
    /// Each object's further levels of detail (round R5d; `None`: level 0 only).
    pub object_lods: Vec<Option<Arc<Lods>>>,
    /// `None` (the default): every patch of the terrain is a source, the frame's culling keeps those in view.
    /// `Some(r)`: only the window of patches `r` on each side of the patch under the camera's target, as rounds R5 to
    /// R5c drew it (their recorded pictures; [`TERRAIN_RADIUS`]).
    pub terrain_radius: Option<usize>,
    patches: HashMap<(usize, usize), Arc<Prepared>>,
    no_raw: HashMap<String, crate::terrain::RawTexture>,
    meshes: HashMap<(String, Option<String>), Option<Arc<Prepared>>>,
    lods: HashMap<(String, Option<String>), Option<Arc<Lods>>>,
    blueprints: HashMap<String, Option<String>>,
    envmap: String,
}

/// A placed thing's identity: its GUID (lower case), or a group member's group file and item.
#[must_use]
pub fn placed_key(p: &Placed) -> String {
    match &p.group {
        Some((g, k)) => format!("{}#{k}", g.to_lowercase()),
        None => p.guid.to_lowercase(),
    }
}

/// The blueprint's `Mesh` reference (`path/guid` -> path), read from its partition.
#[must_use]
pub fn mesh_of(files: &Files, blueprint: &str) -> Option<String> {
    let xml = bc2_formats::dbx::to_xml(&files.read(&format!("{blueprint}.dbx"))?).ok()?;
    let at = xml.find("name=\"Mesh\"")?;
    let head = xml.get(..at)?;
    let r = head.get(head.rfind("ref=\"")? + 5..)?;
    let r = r.get(..r.find('"')?)?;
    (r != "null").then(|| r.get(..r.rfind('/')?).map(str::to_owned))?
}

/// The mesh a placed thing draws and the shader set it is drawn with: a group member's from its group's facts, else
/// its blueprint's `Mesh` (read once into `blueprints`) with the placement's override. One rule for both of the
/// editor's views ([`World::set_placed`]; [`crate::simple`], round SV1).
#[allow(clippy::implicit_hasher, reason = "the world's own cache")]
pub fn mesh_and_set(
    files: &Files,
    placements: &Placements,
    blueprints: &mut HashMap<String, Option<String>>,
    p: &Placed,
) -> (Option<String>, Option<String>) {
    let member = p.group.as_ref().and_then(|(f, k)| placements.member(f, *k));
    let set = match &p.group {
        Some(_) => member.and_then(|m| m.shader_set.clone()),
        None => placements.objects.get(&p.guid.to_lowercase()).cloned(),
    };
    let mesh = member.and_then(|m| m.mesh.clone()).or_else(|| {
        blueprints
            .entry(p.blueprint.clone())
            .or_insert_with(|| mesh_of(files, &p.blueprint))
            .clone()
    });
    (mesh, set)
}

/// The level's weather as the game finds it (round GW1). The level has a weather entity, a
/// `WeatherSystemEntityData`: an instance of the level partition itself (`SP_004_B`, OPFOR's Coral Sea and Wake
/// Island) or the object of a placed blueprint (`ReferencedObject`: `MP_005`'s
/// `weather/r_mp005/weathersystementity_mp005`, Guadalcanal's `weather/wake_island/guadal_canal_e`). The entity names
/// its `WeatherSystem` and `Settings` partitions by path, each read wherever the level's stack holds it (the first
/// bundle naming it, a deleted marker hiding it). The settings' `ActiveState` picks the state (RNDR-WTHR-1). A level
/// partition's enabled `WeatherStateEntityData` with `StartTransitionOnSpawn` (another `State`: Coral Sea 4, `MP_003`
/// 2, `ONS_MP_002` 5) is only noted: whether and when the engine applies it is not known yet (no capture). The first
/// entity that resolves is used (the level partition's own, then the placed ones in order).
/// Only a level whose entity does not resolve falls back to `folder` (the caller's) read by name; with nothing,
/// [`weather::neutral`] and a note starting with [`weather::NO_WEATHER`].
///
/// Until GW1 a folder was the only way: the entity's blueprint path up to `/weathersystementity`, the partition's
/// weather folder, a folder a placed blueprint lies in, then `weather/r_<lv without _>`, each searched for partitions
/// named `weathersettings*` and `weathersystem*`. OPFOR's Guadalcanal and Iwo Jima name their settings after the
/// level (`weather/wake_island/guadal_canal`), so the view found no weather and failed.
#[must_use]
pub fn resolve_weather(
    files: &Files,
    level: &str,
    placed: &[Placed],
    folder: Option<&str>,
) -> (weather::Weather, Option<String>) {
    let mut tried = Vec::new();
    let partition = format!("levels/{level}");
    let level_xml = files
        .read(&format!("{partition}.dbx"))
        .and_then(|d| bc2_formats::dbx::to_xml(&d).ok());
    let spawn = level_xml.as_deref().and_then(weather::spawn_state_in);
    let mut entities: Vec<(String, weather::EntityRefs)> = level_xml
        .as_deref()
        .map_or_else(Vec::new, weather::entities_in)
        .into_iter()
        .map(|e| (partition.clone(), e))
        .collect();
    let mut seen = std::collections::HashSet::new();
    for p in placed.iter().filter(|p| p.group.is_none()) {
        let b = p.blueprint.to_lowercase();
        if seen.insert(b.clone())
            && let Some(refs) = weather::entity(files, &b)
        {
            entities.push((b, refs));
        }
    }
    for (k, (at, e)) in entities.iter().enumerate() {
        match weather::read_system(files, &e.system, &e.settings, None) {
            Ok(w) => {
                let mut notes = Vec::new();
                if entities.len() > 1 {
                    notes.push(format!(
                        "weather entity of {at} used ({} of {} weather entities)",
                        k + 1,
                        entities.len()
                    ));
                }
                if let Some(s) = spawn {
                    notes.push(format!(
                        "the level's WeatherStateEntityData moves to state {s} on spawn (not applied: the settings' \
                         ActiveState is drawn)"
                    ));
                }
                return (w, (!notes.is_empty()).then(|| notes.join("; ")));
            }
            Err(e) => tried.push(format!("{at}: {e}")),
        }
    }
    if entities.is_empty() {
        tried.push("no weather entity (WeatherSystemEntityData)".to_owned());
    }
    if let Some(f) = folder {
        match weather::read(files, f) {
            Ok(w) => {
                return (
                    w,
                    Some(format!("weather read by folder name ({f}): {}", tried.join("; "))),
                );
            }
            Err(e) => tried.push(e.to_string()),
        }
    }
    (
        weather::neutral(),
        Some(format!("{} ({})", weather::NO_WEATHER, tried.join("; "))),
    )
}

/// The level's weather as data (round VW2): the first weather entity that resolves, as [`resolve_weather`] finds it
/// (the level partition's own, then the placed ones in order), with its system's states, the settings'
/// `ActiveState` and the level's spawn-time switch.
///
/// # Errors
/// No weather entity resolves (what was tried, in words).
pub fn level_weather(files: &Files, level: &str, placed: &[Placed]) -> Result<weather::LevelWeather, RenderError> {
    let partition = format!("levels/{level}");
    let level_xml = files
        .read(&format!("{partition}.dbx"))
        .and_then(|d| bc2_formats::dbx::to_xml(&d).ok());
    let spawn = level_xml.as_deref().and_then(weather::spawn_state_in);
    let mut entities: Vec<(String, weather::EntityRefs)> = level_xml
        .as_deref()
        .map_or_else(Vec::new, weather::entities_in)
        .into_iter()
        .map(|e| (partition.clone(), e))
        .collect();
    let mut seen = std::collections::HashSet::new();
    for p in placed.iter().filter(|p| p.group.is_none()) {
        let b = p.blueprint.to_lowercase();
        if seen.insert(b.clone())
            && let Some(refs) = weather::entity(files, &b)
        {
            entities.push((b, refs));
        }
    }
    let mut tried = Vec::new();
    for (at, e) in &entities {
        match weather::LevelWeather::read(files, at, e, spawn) {
            Ok(w) if !w.states.is_empty() => return Ok(w),
            Ok(_) => tried.push(format!("{at}: {} has no states", e.system)),
            Err(x) => tried.push(format!("{at}: {x}")),
        }
    }
    if entities.is_empty() {
        tried.push("no weather entity (WeatherSystemEntityData)".to_owned());
    }
    Err(RenderError::Data(tried.join("; ")))
}

/// A level's files for its weather alone (round VW2: the editor's weather picker before the game-accurate view is
/// loaded): a prepared level's folders, else its `game\Package` bundles read in place, in rounds until the weather's
/// partitions are all read. The level's weather with them.
///
/// # Errors
/// The level is in neither, or its weather does not resolve.
pub fn weather_files(
    opfor: &Path,
    lv: &str,
    level: &str,
    placed: &[Placed],
    package: bool,
) -> Result<(Files, weather::LevelWeather), RenderError> {
    let files = if package {
        let stack = crate::files::package_stack(&opfor.join("game"), lv).ok_or_else(|| {
            RenderError::Data(format!(
                "{lv}: not in the game's bundle registry (package.mft), or a bundle is missing"
            ))
        })?;
        Files::bundles(&stack)?
    } else {
        Files::new(&level_folders(opfor, lv))
    };
    for _ in 0..12 {
        let got = level_weather(&files, level, placed).inspect(|w| {
            // every state's partitions read once, so a later read finds them
            for k in 0..w.states.len() {
                let _ = w.state(&files, k);
            }
        });
        let missed = files.missed();
        if !package || missed.is_empty() {
            return got.map(|w| (files, w));
        }
        files.fetch(&missed)?;
    }
    level_weather(&files, level, placed).map(|w| (files, w))
}

impl World {
    /// The state the world is drawn under now (round VW2): `index` of the level's weather system's states. The
    /// light, the post-process, the clear colour and the sky dome follow; the caller plans its live list again
    /// ([`crate::live::Live::new`] with [`World::light`]).
    ///
    /// # Errors
    /// The level's weather is not known, or the state's partitions are not read.
    pub fn set_state(&mut self, index: usize) -> Result<(), RenderError> {
        let lw = self
            .level_weather
            .as_ref()
            .ok_or(RenderError::Data("the level's weather is not known".into()))?;
        let w = lw.state(&self.files, index)?;
        self.set_weather(&w);
        self.state_shown = Some(index);
        Ok(())
    }

    /// Draws the world under a weather state's values (round VW2).
    pub fn set_weather(&mut self, w: &weather::Weather) {
        let (light, post) = weather::light(w, weather::NOON_DIR);
        self.light = light;
        self.post = post;
        let fog = w.fog.get("Color").cloned().unwrap_or_else(|| vec![0.32, 0.36, 0.42]);
        self.background = [0, 1, 2].map(|k| fog.get(k).copied().unwrap_or(0.4));
        self.sky = crate::sky::load(
            &self.files,
            &self.db,
            w,
            weather::NOON_DIR,
            crate::sky::CLOUDS,
            &mut self.textures,
        )
        .ok()
        .map(Arc::new);
        self.weather = w.clone();
    }

    /// Loads a level (`lv`: its folder under `work\levels`; `level`: its partition's name, `MP_005`): database,
    /// weather (the placed weather entity's references; `weather`: a folder to read by name when the level places
    /// none, [`resolve_weather`]), the placed things, terrain, strips, water and the post shaders.
    ///
    /// # Errors
    /// The level's database cannot be read (a level without weather is drawn under a default one).
    pub fn load(
        opfor: &Path,
        lv: &str,
        level: &str,
        placed: &[Placed],
        weather: Option<&str>,
    ) -> Result<Self, RenderError> {
        Self::load_from(Files::new(&level_folders(opfor, lv)), opfor, lv, level, placed, weather)
    }

    /// Loads a level that lives only in the game copy's `game\Package` (Vietnam, the Pacific War, the other
    /// Package-only levels; round R5c) from its bundles in place ([`Files::bundles`] over
    /// [`crate::files::package_stack`]): [`World::load_from`] in rounds, each round's missing files read from the
    /// bundles (one streamed pass a bundle), until nothing more is asked for. Nothing is written to disk.
    ///
    /// # Errors
    /// The registry does not name the level, a bundle cannot be read, or the level's database or weather cannot be
    /// read.
    pub fn load_package(
        opfor: &Path,
        lv: &str,
        level: &str,
        placed: &[Placed],
        weather: Option<&str>,
    ) -> Result<Self, RenderError> {
        let stack = crate::files::package_stack(&opfor.join("game"), lv).ok_or_else(|| {
            RenderError::Data(format!(
                "{lv}: not in the game's bundle registry (package.mft), or a bundle is missing"
            ))
        })?;
        let files = Files::bundles(&stack)?;
        let mut rounds = 0;
        loop {
            rounds += 1;
            let made = Self::load_from(files.clone(), opfor, lv, level, placed, weather);
            let missed = files.missed();
            if missed.is_empty() || rounds >= 40 {
                let mut w = made?;
                if !missed.is_empty() {
                    w.notes
                        .push(format!("{} files still unread after {rounds} rounds", missed.len()));
                }
                w.notes.push(format!(
                    "read from {} bundles in place ({}): {} rounds, {:.0} MB",
                    stack.len(),
                    stack
                        .iter()
                        .filter_map(|p| Some(format!(
                            "{}/{}",
                            p.parent()?.file_name()?.to_string_lossy(),
                            p.file_name()?.to_string_lossy()
                        )))
                        .collect::<Vec<_>>()
                        .join(", "),
                    rounds,
                    files.bundled_bytes() as f64 / 1e6
                ));
                return Ok(w);
            }
            files.fetch(&missed)?;
        }
    }

    /// [`World::load`] from a set of files (`opfor`: where the post shaders' startup bundle is).
    ///
    /// # Errors
    /// The level's database or weather cannot be read.
    pub fn load_from(
        files: Files,
        opfor: &Path,
        lv: &str,
        level: &str,
        placed: &[Placed],
        weather: Option<&str>,
    ) -> Result<Self, RenderError> {
        let lv = lv.to_lowercase();
        let db = Db::parse(
            &files
                .read(&format!("levels/{lv}/shaders/dx11_single.res"))
                .ok_or_else(|| RenderError::Data(format!("{lv}: no Dx11 shader database in its prepared files")))?,
        )?;
        let mut notes_early = Vec::new();
        let placements = placement::read(&files, level).unwrap_or_else(|e| {
            notes_early.push(format!("placements: {e}"));
            Placements::default()
        });
        let (w, weather_note) = resolve_weather(&files, level, placed, weather);
        let (light, post) = weather::light(&w, weather::NOON_DIR);
        let mut sky_notes = Vec::new();
        let fog = w.fog.get("Color").cloned().unwrap_or_else(|| vec![0.32, 0.36, 0.42]);
        let background = [0, 1, 2].map(|k| fog.get(k).copied().unwrap_or(0.4));
        let default_weather = weather_note
            .as_deref()
            .is_some_and(|n| n.starts_with(weather::NO_WEATHER));
        let mut notes = vec![format!("weather: {}", w.state)];
        notes.extend(weather_note);
        notes.append(&mut notes_early);
        let envmap = placements
            .envmap
            .clone()
            .unwrap_or_else(|| "Textures/Sky/Sky_Beach_Generic_RGBA16f".into());
        let mut textures = TextureFiles::new();
        let mut counts = Counts::default();
        // the level's own terrain (its TerrainEntityData), else the first visual terrain its files hold
        let terrain_name = placements.terrain.clone().or_else(|| {
            files
                .names()
                .filter(|n| n.starts_with("terrains/") && n.ends_with(".visual.res"))
                .min()
                .and_then(|n| n.rsplit('/').next())
                .map(|n| n.trim_end_matches(".visual.res").to_owned())
        });
        let terrain = match terrain_name
            .as_deref()
            .map(|n| Terrain::load(&files, &db, n, &envmap, &mut textures))
        {
            Some(Ok(t)) => {
                for (k, d) in t.draws.iter().enumerate() {
                    if let Err(e) = d {
                        notes.push(format!("patch shader {k}: {e}"));
                    }
                }
                Some(t)
            }
            Some(Err(e)) => {
                notes.push(format!("terrain: {e}"));
                None
            }
            None => None,
        };
        let strips: Vec<Arc<Prepared>> = terrain
            .as_ref()
            .map_or_else(Vec::new, |t| t.strips.iter().cloned().map(Arc::new).collect());
        for s in &strips {
            notes.extend(s.notes.iter().cloned());
            counts.strips += s.draws.len();
        }
        // the water reads the terrain's height map with the terrain renderer's values (round R5d)
        let water_values = terrain
            .as_ref()
            .map_or_else(HashMap::new, |t| crate::terrain::engine_values(&t.hf));
        let water = match terrain_name
            .as_deref()
            .map(|n| crate::water::load(&files, &db, n, &envmap, &water_values, &mut textures))
        {
            Some(Ok(p)) => {
                notes.extend(p.notes.iter().cloned());
                counts.water = p.draws.len();
                (!p.draws.is_empty()).then(|| Arc::new(p))
            }
            Some(Err(e)) => {
                notes.push(format!("water: {e}"));
                None
            }
            None => None,
        };
        let sky = match crate::sky::load(&files, &db, &w, weather::NOON_DIR, crate::sky::CLOUDS, &mut textures) {
            Ok(p) => {
                sky_notes.extend(p.notes.iter().cloned());
                Some(Arc::new(p))
            }
            Err(e) => {
                sky_notes.push(format!("sky: {e}"));
                None
            }
        };
        notes.append(&mut sky_notes);
        let post_shaders = match post::Shaders::load(&post::startup_bundle(opfor)) {
            Ok(s) => Some(s),
            Err(e) => {
                notes.push(format!("post-process: {e} (the harness tonemap is used)"));
                None
            }
        };
        let level_weather = level_weather(&files, level, placed).ok();
        let mut world = Self {
            files,
            db,
            textures,
            light,
            post,
            weather: w,
            level_weather,
            state_shown: None,
            background,
            default_weather,
            objects: Vec::new(),
            terrain,
            water,
            strips,
            sky,
            sky_pass: false,
            post_shaders,
            placements,
            notes: Vec::new(),
            counts,
            object_of: Vec::new(),
            object_keys: Vec::new(),
            object_lods: Vec::new(),
            lods: HashMap::new(),
            terrain_radius: None,
            patches: HashMap::new(),
            no_raw: HashMap::new(),
            meshes: HashMap::new(),
            blueprints: HashMap::new(),
            envmap,
        };
        notes.extend(world.set_placed(placed));
        let counts = world.counts;
        let (o, g) = world.placements.override_counts();
        notes.push(format!(
            "{} objects and {} group members drawn ({} meshes; {} drawn with a shader-set override, of {} + {} named), \
             {} strip draws, {} water draws, terrain {}",
            counts.objects,
            counts.members,
            counts.meshes,
            counts.overridden,
            o,
            g,
            counts.strips,
            counts.water,
            if world.terrain.is_some() { "drawn" } else { "not drawn" }
        ));
        world.notes = notes;
        Ok(world)
    }

    /// The placed objects and group members made again from a placed list (an edit: moved, added, removed
    /// things), each mesh prepared once and kept for later edits. Returns what was found preparing new meshes.
    pub fn set_placed(&mut self, placed: &[Placed]) -> Vec<String> {
        let mut notes = Vec::new();
        let mut objects = Vec::new();
        let mut object_of = Vec::new();
        let mut keys = Vec::new();
        let mut object_lods = Vec::new();
        let mut counts = Counts {
            strips: self.counts.strips,
            water: self.counts.water,
            ..Counts::default()
        };
        for (n, p) in placed.iter().enumerate() {
            // every placed blueprint with a mesh, wherever it lies (round RV1: `terrains/<lv>/props`, `vegetation/...`
            // were left out until then); one without (spawns, logic, sounds) has no `Mesh` and is not drawn
            let files = &self.files;
            let (mesh, set) = mesh_and_set(files, &self.placements, &mut self.blueprints, p);
            let Some(mesh) = mesh else { continue };
            let (db, envmap, textures) = (&self.db, &self.envmap, &mut self.textures);
            let prepared = self
                .meshes
                .entry((mesh.to_lowercase(), set.clone()))
                .or_insert_with(|| {
                    let pr = material::prepare_set(files, db, &mesh, set.as_deref(), envmap, textures);
                    notes.extend(pr.notes.iter().map(|n| format!("{mesh}: {n}")));
                    (!pr.draws.is_empty()).then(|| Arc::new(pr))
                })
                .clone();
            if let Some(prepared) = prepared {
                let lods = self
                    .lods
                    .entry((mesh.to_lowercase(), set.clone()))
                    .or_insert_with(|| {
                        // meshes with alpha-tested or blended parts keep level 0 (round R5d): the vegetation's far
                        // levels are cards that draw much thinner here (MP_005's mebush: 54 indices), whether the
                        // engine draws them with values of its own (a billboard) is the capture's
                        if prepared.draws.iter().any(|d| d.class != material::OPAQUE) {
                            return None;
                        }
                        let mut settings = material::lod_settings(files, &mesh)?;
                        if settings.levels < 2 {
                            return None;
                        }
                        let levels: Vec<Option<Arc<Prepared>>> = (1..settings.levels)
                            .map(|k| {
                                let pr =
                                    material::prepare_set_lod(files, db, &mesh, set.as_deref(), envmap, textures, k);
                                (!pr.draws.is_empty()).then(|| Arc::new(pr))
                            })
                            .collect();
                        // a level that draws nothing here (a tree's billboard level, drawn by the engine's tree
                        // renderer) keeps the level before it, as a level switched off does
                        for (k, l) in levels.iter().enumerate() {
                            if l.is_none()
                                && let Some(e) = settings.level_enabled.get_mut(k + 1)
                            {
                                *e = false;
                            }
                        }
                        Some(Arc::new(Lods { settings, levels }))
                    })
                    .clone();
                object_lods.push(lods);
                if p.group.is_some() {
                    counts.members += 1;
                } else {
                    counts.objects += 1;
                }
                if set.is_some() {
                    counts.overridden += 1;
                }
                objects.push(Instance {
                    prepared,
                    world: frame::from_rows(&p.rows),
                });
                object_of.push(n);
                keys.push(placed_key(p));
            }
        }
        counts.meshes = self.meshes.values().filter(|m| m.is_some()).count();
        self.objects = objects;
        self.object_of = object_of;
        self.object_keys = keys;
        self.object_lods = object_lods;
        self.counts = counts;
        notes
    }

    /// The patch the terrain's window is around for a camera looking at `target`: `None` when the whole terrain is
    /// drawn ([`World::terrain_radius`] `None`, the sources do not depend on the target) or the target is off the
    /// terrain.
    #[must_use]
    pub fn terrain_window(&self, target: [f64; 3]) -> Option<(usize, usize)> {
        self.terrain_radius?;
        self.terrain.as_ref()?.patch_at(target[0], target[2])
    }

    /// The terrain patches a frame looking at `target` draws, rows of z then x: every patch of the patch map, or
    /// with [`World::terrain_radius`] the window around the patch under the target (patches past the map's edge are
    /// listed and drawn as nothing).
    fn terrain_patches(&self, target: [f64; 3]) -> Vec<(usize, usize)> {
        let Some(t) = &self.terrain else { return Vec::new() };
        let (rows, cols) = match self.terrain_radius {
            None => {
                let side = t.visual.maps.first().map_or(0, crate::terrain::PatchMap::side);
                (0..side, 0..side)
            }
            Some(r) => match t.patch_at(target[0], target[2]) {
                Some((px, pz)) => (pz.saturating_sub(r)..pz + r + 1, px.saturating_sub(r)..px + r + 1),
                None => return Vec::new(),
            },
        };
        rows.flat_map(|j| cols.clone().map(move |i| (i, j))).collect()
    }

    /// The prepared patch `(i, j)` (made on first use and kept); `None` when it draws nothing.
    fn terrain_patch(&mut self, (i, j): (usize, usize)) -> Option<Arc<Prepared>> {
        let t = self.terrain.as_ref()?;
        let p = self
            .patches
            .entry((i, j))
            .or_insert_with(|| Arc::new(t.patch(i, j).unwrap_or_default()));
        (!p.draws.is_empty()).then(|| Arc::clone(p))
    }

    /// The live list's sources for a camera looking at `target`, in the order of [`World::instances`]: the objects
    /// (each picked as its place in the placed list plus one), the terrain patches (`terrain_patches`), the strips, the
    /// water.
    pub fn sources(&mut self, target: [f64; 3]) -> Vec<crate::live::Source> {
        use crate::live::{Key, Source};
        let mut out: Vec<Source> = Vec::with_capacity(self.objects.len());
        for (((o, k), n), lods) in self
            .objects
            .iter()
            .zip(&self.object_keys)
            .zip(&self.object_of)
            .zip(&self.object_lods)
        {
            // level 0 in its band, then each further level of detail as its own source (round R5d)
            let band = |level: usize| {
                lods.as_ref().map(|l| {
                    let (near, far) = l.settings.band(level);
                    crate::live::LodBand { level, near, far }
                })
            };
            out.push(Source {
                prepared: Arc::clone(&o.prepared),
                world: o.world,
                key: Key::Object(k.clone()),
                pick: *n as u32 + 1,
                lod: band(0),
            });
            for (level, p) in lods
                .iter()
                .flat_map(|l| l.levels.iter().enumerate().map(|(i, p)| (i + 1, p)))
            {
                let Some(p) = p else { continue };
                out.push(Source {
                    prepared: Arc::clone(p),
                    world: o.world,
                    key: Key::Object(format!("{k}#lod{level}")),
                    pick: *n as u32 + 1,
                    lod: band(level),
                });
            }
        }
        for (i, j) in self.terrain_patches(target) {
            if let Some(prepared) = self.terrain_patch((i, j)) {
                out.push(Source {
                    prepared,
                    world: frame::IDENTITY,
                    key: Key::Patch(i, j),
                    pick: 0,
                    lod: None,
                });
            }
        }
        for (k, s) in self.strips.iter().enumerate() {
            out.push(Source {
                prepared: Arc::clone(s),
                world: frame::IDENTITY,
                key: Key::Strip(k),
                pick: 0,
                lod: None,
            });
        }
        if let Some(w) = &self.water {
            out.push(Source {
                prepared: Arc::clone(w),
                world: frame::IDENTITY,
                key: Key::Water,
                pick: 0,
                lod: None,
            });
        }
        if let Some(s) = self.sky.as_ref().filter(|_| self.sky_pass) {
            out.push(Source {
                prepared: Arc::clone(s),
                world: frame::IDENTITY,
                key: Key::Sky,
                pick: 0,
                lod: None,
            });
        }
        out
    }

    /// The engine's passes of a frame: the depth pass the water reads, and DICE's post-process when `post`.
    ///
    /// # Errors
    /// A post shader is missing.
    pub fn passes(
        &self,
        cam: &Camera,
        size: [u32; 2],
        post: bool,
    ) -> Result<(Option<post::Pass>, Vec<post::Pass>), RenderError> {
        let Some(s) = &self.post_shaders else {
            return Ok((None, Vec::new()));
        };
        let depth = Some(post::unproject(s, cam, size)?);
        let chain = if post {
            post::chain(s, &self.post, size)?
        } else {
            Vec::new()
        };
        Ok((depth, chain))
    }

    /// The clear colour of a frame.
    #[must_use]
    pub fn clear(&self) -> [f32; 4] {
        let b = self.background;
        [b[0], b[1], b[2], 0.0]
    }

    /// The instances of a frame: the objects, the terrain patches (`terrain_patches`), the strips and the water.
    pub fn instances(&mut self, target: [f64; 3]) -> Vec<Instance> {
        let mut out = self.objects.clone();
        for at in self.terrain_patches(target) {
            if let Some(prepared) = self.terrain_patch(at) {
                out.push(Instance {
                    prepared,
                    world: frame::IDENTITY,
                });
            }
        }
        for s in self.strips.iter().chain(&self.water) {
            out.push(Instance {
                prepared: Arc::clone(s),
                world: frame::IDENTITY,
            });
        }
        out
    }

    /// A frame for a camera looking at `target`: the draws, the depth pass the water reads, and DICE's post-process
    /// (`post`; without it, or without the post shaders, [`Frame::post`] is empty and the caller tonemaps).
    ///
    /// # Errors
    /// A post shader is missing.
    pub fn frame(&mut self, cam: &Camera, target: [f64; 3], size: [u32; 2], post: bool) -> Result<Frame, RenderError> {
        let instances = self.instances(target);
        let b = self.background;
        let mut f = Frame {
            width: size[0],
            height: size[1],
            clear: [b[0], b[1], b[2], 0.0],
            draws: frame::draw_calls(&instances, &self.light, cam, size),
            depth_pass: None,
            post: Vec::new(),
        };
        if let Some(s) = &self.post_shaders {
            f.depth_pass = Some(post::unproject(s, cam, size)?);
            if post {
                f.post = post::chain(s, &self.post, size)?;
            }
        }
        Ok(f)
    }

    /// The engine textures the frame's draws read (the terrain's height map and empty mask).
    #[must_use]
    pub fn raw(&self) -> &HashMap<String, crate::terrain::RawTexture> {
        self.terrain.as_ref().map_or(&self.no_raw, |t| &t.raw)
    }
}
