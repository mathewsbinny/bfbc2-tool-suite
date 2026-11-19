//! A level's whole drawable world (round R5b): its placed objects and model-group members with their shader-set
//! overrides ([`crate::placement`]), the terrain patches around a point, the baked road and decal strips, the visual
//! water, the weather's light and DICE's post-process chain - what the editor's game-accurate view and the examples
//! draw.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

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

/// Patches drawn on each side of the patch under the camera's target.
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
    /// The clear colour (the fog colour; the sky is not drawn yet).
    pub background: [f32; 3],
    /// The placed objects and group members.
    pub objects: Vec<Instance>,
    /// The terrain.
    pub terrain: Option<Terrain>,
    /// The visual water.
    pub water: Option<Arc<Prepared>>,
    /// The strips, one per road block.
    pub strips: Vec<Arc<Prepared>>,
    /// DICE's post shaders (`None`: the startup bundle could not be read; the harness tonemap is used).
    pub post_shaders: Option<post::Shaders>,
    /// The level partition's facts.
    pub placements: Placements,
    /// What was found while loading.
    pub notes: Vec<String>,
    /// What is drawn.
    pub counts: Counts,
    patches: HashMap<(usize, usize), Arc<Prepared>>,
    no_raw: HashMap<String, crate::terrain::RawTexture>,
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

impl World {
    /// Loads a level (`lv`: its folder under `work\levels`; `level`: its partition's name, `MP_005`): database,
    /// weather (`weather`: its folder, else `weather/r_<lv without _>`), the placed things, terrain, strips, water and
    /// the post shaders.
    ///
    /// # Errors
    /// The level's database or weather cannot be read.
    pub fn load(opfor: &Path, lv: &str, level: &str, placed: &[Placed], weather: Option<&str>) -> Result<Self, String> {
        let files = Files::new(&level_folders(opfor, lv));
        let lv = lv.to_lowercase();
        let db = Db::parse(
            &files
                .read(&format!("levels/{lv}/shaders/dx11_single.res"))
                .ok_or_else(|| format!("{lv}: no Dx11 shader database in its prepared files"))?,
        )?;
        let mut notes_early = Vec::new();
        // the weather: the folder given (the WeatherSystemEntity's), else the one the partition's weather references
        // name (`SP_004_B`'s `weather/r_sp04`), else a weather folder a placed blueprint lies in, else
        // `weather/r_<lv without _>`
        let placements = placement::read(&files, level).unwrap_or_else(|e| {
            notes_early.push(format!("placements: {e}"));
            Placements::default()
        });
        let mut folders: Vec<String> = weather
            .map(str::to_owned)
            .into_iter()
            .chain(placements.weather.clone())
            .collect();
        for p in placed {
            let mut parts = p.blueprint.split('/');
            if let (Some("weather"), Some(f)) = (parts.next(), parts.next()) {
                let f = format!("weather/{f}");
                if !folders.contains(&f) {
                    folders.push(f);
                }
            }
        }
        folders.push(format!("weather/r_{}", lv.replace('_', "")));
        let mut tried = Vec::new();
        let w = folders
            .iter()
            .find_map(|f| weather::read(&files, f).map_err(|e| tried.push(e)).ok())
            .ok_or_else(|| tried.join("; "))?;
        let (light, post) = weather::light(&w, weather::NOON_DIR);
        let fog = w.fog.get("Color").cloned().unwrap_or_else(|| vec![0.32, 0.36, 0.42]);
        let background = [0, 1, 2].map(|k| fog.get(k).copied().unwrap_or(0.4));
        let mut notes = vec![format!("weather: {}", w.state)];
        notes.append(&mut notes_early);
        let envmap = placements
            .envmap
            .clone()
            .unwrap_or_else(|| "Textures/Sky/Sky_Beach_Generic_RGBA16f".into());
        let mut textures = TextureFiles::new();
        let mut meshes: HashMap<(String, Option<String>), Option<Arc<Prepared>>> = HashMap::new();
        let mut blueprints: HashMap<String, Option<String>> = HashMap::new();
        let mut objects = Vec::new();
        let mut counts = Counts::default();
        for p in placed {
            if !p.blueprint.starts_with("objects/") {
                continue;
            }
            let member = p.group.as_ref().and_then(|(f, k)| placements.member(f, *k));
            let set = match &p.group {
                Some(_) => member.and_then(|m| m.shader_set.clone()),
                None => placements.objects.get(&p.guid.to_lowercase()).cloned(),
            };
            let mesh = member.and_then(|m| m.mesh.clone()).or_else(|| {
                blueprints
                    .entry(p.blueprint.clone())
                    .or_insert_with(|| mesh_of(&files, &p.blueprint))
                    .clone()
            });
            let Some(mesh) = mesh else { continue };
            let prepared = meshes
                .entry((mesh.to_lowercase(), set.clone()))
                .or_insert_with(|| {
                    let pr = material::prepare_set(&files, &db, &mesh, set.as_deref(), &envmap, &mut textures);
                    notes.extend(pr.notes.iter().map(|n| format!("{mesh}: {n}")));
                    (!pr.draws.is_empty()).then(|| Arc::new(pr))
                })
                .clone();
            if let Some(prepared) = prepared {
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
            }
        }
        counts.meshes = meshes.values().filter(|m| m.is_some()).count();
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
            .map(|t| t.strips.iter().cloned().map(Arc::new).collect())
            .unwrap_or_default();
        for s in &strips {
            notes.extend(s.notes.iter().cloned());
            counts.strips += s.draws.len();
        }
        let water = match terrain_name
            .as_deref()
            .map(|n| crate::water::load(&files, &db, n, &envmap, &mut textures))
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
        let post_shaders = match post::Shaders::load(&post::startup_bundle(opfor)) {
            Ok(s) => Some(s),
            Err(e) => {
                notes.push(format!("post-process: {e} (the harness tonemap is used)"));
                None
            }
        };
        let (o, g) = placements.override_counts();
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
            if terrain.is_some() { "drawn" } else { "not drawn" }
        ));
        Ok(Self {
            files,
            db,
            textures,
            light,
            post,
            background,
            objects,
            terrain,
            water,
            strips,
            post_shaders,
            placements,
            notes,
            counts,
            patches: HashMap::new(),
            no_raw: HashMap::new(),
        })
    }

    /// The instances of a frame: the objects, the strips, the water and the terrain patches around `target`.
    pub fn instances(&mut self, target: [f64; 3]) -> Vec<Instance> {
        let mut out = self.objects.clone();
        if let Some(t) = &self.terrain
            && let Some((px, pz)) = t.patch_at(target[0], target[2])
        {
            for j in pz.saturating_sub(TERRAIN_RADIUS)..=pz + TERRAIN_RADIUS {
                for i in px.saturating_sub(TERRAIN_RADIUS)..=px + TERRAIN_RADIUS {
                    let p = self
                        .patches
                        .entry((i, j))
                        .or_insert_with(|| Arc::new(t.patch(i, j).unwrap_or_default()));
                    if !p.draws.is_empty() {
                        out.push(Instance {
                            prepared: Arc::clone(p),
                            world: frame::IDENTITY,
                        });
                    }
                }
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
    pub fn frame(&mut self, cam: &Camera, target: [f64; 3], size: [u32; 2], post: bool) -> Result<Frame, String> {
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
