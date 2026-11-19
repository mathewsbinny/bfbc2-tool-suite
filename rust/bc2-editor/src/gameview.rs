//! The 3D view's "Game-accurate" mode (round R5, `EDITOR_ARCHITECTURE.md` 3.11): the level drawn as BFBC2's Direct3D
//! 11 renderer draws it - DICE's own compiled shaders, textures, material constants and render states from the level's
//! `Dx11_Single.res`, under its weather's light (`bc2_render`) - off screen, then shown in the view as an image.
//!
//! Phase 1 draws the placed objects (`objects/...` blueprints with a `Mesh`) at their transforms and the terrain
//! patches around the camera's target; the selection, labels and gizmo stay egui's overlays. The frame is drawn again
//! only when the camera or the view's size changes. On a machine without Direct3D 11 the mode says why and the view
//! stays as it was.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use bc2_render::files::{Files, level_folders};
use bc2_render::frame::{self, Instance};
use bc2_render::material::{self, Db, Prepared, TextureFiles};
use bc2_render::terrain::Terrain;
use bc2_render::weather::{self, Post, Values};
use bc2_scene::scene::Scene;
use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions};

use crate::camera::Camera;

#[cfg(windows)]
use bc2_render::d3d::Device;

/// Without Direct3D 11 the mode only says why.
#[cfg(not(windows))]
struct Device;

#[cfg(not(windows))]
impl Device {
    fn new(_warp: bool) -> Result<Self, String> {
        Err("the game-accurate view needs Direct3D 11 (Windows)".into())
    }

    fn render(
        &mut self,
        _frame: &frame::Frame,
        _files: &TextureFiles,
        _raw: &HashMap<String, bc2_render::terrain::RawTexture>,
    ) -> Result<frame::Rendered, String> {
        Err("the game-accurate view needs Direct3D 11 (Windows)".into())
    }
}

/// Patches drawn on each side of the patch under the camera's target.
pub const TERRAIN_RADIUS: usize = 24;
/// The far plane, metres.
pub const FAR: f64 = 4000.0;

/// What the last frame cost, in seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timings {
    /// Building the draw list (constants per draw).
    pub build: f64,
    /// Making the draw calls.
    pub submit: f64,
    /// Waiting for the GPU and reading the frame back.
    pub readback: f64,
    /// The post-process (tonemap).
    pub tonemap: f64,
    /// Handing the picture to egui.
    pub upload: f64,
    /// Draw calls.
    pub draws: usize,
}

impl Timings {
    /// The whole frame.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.build + self.submit + self.readback + self.tonemap + self.upload
    }
}

/// The level, ready to draw the game's way.
pub struct GameView {
    files: Files,
    textures: TextureFiles,
    objects: Vec<Instance>,
    terrain: Option<Terrain>,
    patches: HashMap<(usize, usize), Arc<Prepared>>,
    light: Values,
    post: Post,
    background: [f32; 3],
    device: Device,
    texture: Option<TextureHandle>,
    drawn: Option<([u64; 8], [u32; 2])>,
    /// The last frame's costs.
    pub timings: Timings,
    /// Loading took (seconds).
    pub load_seconds: f64,
    /// What was found while loading (missing textures, patch shaders without a program ...).
    pub notes: Vec<String>,
    /// The last frame's error.
    pub error: Option<String>,
}

/// The blueprint's `Mesh` reference (`path/guid` -> path), read from its partition.
fn mesh_of(files: &Files, blueprint: &str) -> Option<String> {
    let xml = bc2_formats::dbx::to_xml(&files.read(&format!("{blueprint}.dbx"))?).ok()?;
    let at = xml.find("name=\"Mesh\"")?;
    let head = xml.get(..at)?;
    let r = head.get(head.rfind("ref=\"")? + 5..)?;
    let r = r.get(..r.find('"')?)?;
    (r != "null").then(|| r.get(..r.rfind('/')?).map(str::to_owned))?
}

/// The level's `StaticEnvmapEntityData`'s texture (the reflection cube the engine binds), if it has one.
fn static_envmap(files: &Files, scene_name: &str) -> Option<String> {
    let xml = bc2_formats::dbx::to_xml(&files.read(&format!("levels/{scene_name}.dbx"))?).ok()?;
    let at = xml.find("WorldRender.StaticEnvmapEntityData")?;
    let rest = xml.get(at..)?;
    let f = rest.find("name=\"StaticEnvmap\"")?;
    let head = rest.get(..f)?;
    let r = head.get(head.rfind("ref=\"")? + 5..)?;
    r.get(..r.rfind('/')?).map(str::to_owned)
}

impl GameView {
    /// Loads a level's draws: its shader database, weather, placed objects and terrain.
    ///
    /// # Errors
    /// No Direct3D 11 device, or the level's database or weather cannot be read.
    pub fn load(opfor: &Path, lv: &str, scene: &Scene) -> Result<Self, String> {
        let t = Instant::now();
        let device = Device::new(false)?;
        let files = Files::new(&level_folders(opfor, lv));
        let lv = lv.to_lowercase();
        let db = Db::parse(
            &files
                .read(&format!("levels/{lv}/shaders/dx11_single.res"))
                .ok_or_else(|| format!("{lv}: no Dx11 shader database in its prepared files"))?,
        )?;
        let folder = scene
            .entities
            .iter()
            .filter_map(|e| e.blueprint.as_deref())
            .find_map(|b| b.find("/weathersystementity").and_then(|i| b.get(..i)))
            .map_or_else(|| format!("weather/r_{}", lv.replace('_', "")), str::to_owned);
        let w = weather::read(&files, &folder)?;
        let (light, post) = weather::light(&w, weather::NOON_DIR);
        let fog = w.fog.get("Color").cloned().unwrap_or_else(|| vec![0.32, 0.36, 0.42]);
        let background = [0, 1, 2].map(|k| fog.get(k).copied().unwrap_or(0.4));
        let mut notes = vec![format!("weather: {}", w.state)];
        let envmap =
            static_envmap(&files, &scene.name).unwrap_or_else(|| "Textures/Sky/Sky_Beach_Generic_RGBA16f".into());
        let mut textures = TextureFiles::new();
        let mut meshes: HashMap<String, Option<Arc<Prepared>>> = HashMap::new();
        let mut blueprints: HashMap<String, Option<String>> = HashMap::new();
        let mut objects = Vec::new();
        for e in &scene.entities {
            let (Some(bp), Some(m)) = (e.blueprint.as_deref(), e.matrix.as_ref()) else {
                continue;
            };
            if !bp.starts_with("objects/") {
                continue;
            }
            let mesh = blueprints
                .entry(bp.to_owned())
                .or_insert_with(|| mesh_of(&files, bp))
                .clone();
            let Some(mesh) = mesh else { continue };
            let prepared = meshes
                .entry(mesh.clone())
                .or_insert_with(|| {
                    let p = material::prepare(&files, &db, &mesh, &envmap, &mut textures);
                    notes.extend(p.notes.iter().map(|n| format!("{mesh}: {n}")));
                    (!p.draws.is_empty()).then(|| Arc::new(p))
                })
                .clone();
            if let Some(prepared) = prepared {
                objects.push(Instance {
                    prepared,
                    world: frame::from_rows(m),
                });
            }
        }
        let terrain_name = files
            .names()
            .filter(|n| n.starts_with("terrains/") && n.ends_with(".visual.res"))
            .min()
            .and_then(|n| n.rsplit('/').next())
            .map(|n| n.trim_end_matches(".visual.res").to_owned());
        let terrain = match terrain_name.map(|n| Terrain::load(&files, &db, &n, &mut textures)) {
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
        notes.push(format!(
            "{} objects drawn ({} meshes), terrain {}",
            objects.len(),
            meshes.values().filter(|m| m.is_some()).count(),
            if terrain.is_some() { "drawn" } else { "not drawn" }
        ));
        Ok(Self {
            files,
            textures,
            objects,
            terrain,
            patches: HashMap::new(),
            light,
            post,
            background,
            device,
            texture: None,
            drawn: None,
            timings: Timings::default(),
            load_seconds: t.elapsed().as_secs_f64(),
            notes,
            error: None,
        })
    }

    /// The game's camera for the view's camera.
    #[must_use]
    pub fn camera(camera: &Camera, size: [u32; 2]) -> frame::Camera {
        let v = camera.view([f64::from(size[0]), f64::from(size[1])]);
        frame::Camera {
            eye: v.eye,
            right: v.right,
            up: v.up,
            forward: v.back.map(|c| -c),
            fov: 2.0 * (1.0 / v.focal).atan(),
            aspect: f64::from(size[0]) / f64::from(size[1].max(1)),
            near: v.near.max(0.1),
            far: FAR,
        }
    }

    /// The instances of a frame: the objects and the terrain patches around `target`.
    fn instances(&mut self, target: [f64; 3]) -> Vec<Instance> {
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
        out
    }

    /// Draws the frame for a camera (when it or the size changed) and returns the picture's texture.
    pub fn frame(&mut self, ctx: &egui::Context, camera: &Camera, size: [u32; 2]) -> Option<egui::TextureId> {
        let key = [
            camera.target[0].to_bits(),
            camera.target[1].to_bits(),
            camera.target[2].to_bits(),
            camera.yaw.to_bits(),
            camera.pitch.to_bits(),
            camera.distance.to_bits(),
            camera.fov.to_bits(),
            0,
        ];
        if self.drawn != Some((key, size)) {
            self.drawn = Some((key, size));
            match self.draw(ctx, camera, size) {
                Ok(()) => self.error = None,
                Err(e) => self.error = Some(e),
            }
        }
        self.texture.as_ref().map(TextureHandle::id)
    }

    fn draw(&mut self, ctx: &egui::Context, camera: &Camera, size: [u32; 2]) -> Result<(), String> {
        let t = Instant::now();
        let cam = Self::camera(camera, size);
        let instances = self.instances(camera.target);
        let clear = [self.background[0], self.background[1], self.background[2], 0.0];
        let f = frame::Frame {
            width: size[0],
            height: size[1],
            clear,
            draws: frame::draw_calls(&instances, &self.light, &cam, size),
        };
        let build = t.elapsed().as_secs_f64();
        let none = HashMap::new();
        let raw = self.terrain.as_ref().map_or(&none, |t| &t.raw);
        let r = self.device.render(&f, &self.textures, raw)?;
        let t2 = Instant::now();
        let bg = bc2_render::tonemap::map(self.background, 1.0, &self.post).map(|v| (v * 255.0).round() as u8);
        let (ldr, _) = bc2_render::tonemap::tonemap(&r.hdr, &r.mask(), &self.post, bg);
        let tonemap = t2.elapsed().as_secs_f64();
        let t3 = Instant::now();
        let image = ColorImage::from_rgba_unmultiplied([size[0] as usize, size[1] as usize], &ldr);
        match &mut self.texture {
            Some(h) => h.set(image, TextureOptions::LINEAR),
            None => self.texture = Some(ctx.load_texture("game-accurate view", image, TextureOptions::LINEAR)),
        }
        self.timings = Timings {
            build,
            submit: r.submit,
            readback: r.readback,
            tonemap,
            upload: t3.elapsed().as_secs_f64(),
            draws: r.draws,
        };
        Ok(())
    }

    /// Paints the view: the frame, and its costs in the corner.
    pub fn paint(&mut self, ui: &egui::Ui, rect: Rect, camera: &Camera, size: [u32; 2]) {
        let ctx = ui.ctx().clone();
        if let Some(tex) = self.frame(&ctx, camera, size) {
            ui.painter().image(
                tex,
                rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let t = &self.timings;
        let text = match &self.error {
            Some(e) => format!("Game-accurate view failed: {e}"),
            None => format!(
                "Game-accurate (Direct3D 11, DICE's shaders) - {} draws, {:.1} ms: submit {:.1}, GPU + read back {:.1}, \
                 tonemap {:.1}, upload {:.1}",
                t.draws,
                t.total() * 1e3,
                t.submit * 1e3,
                t.readback * 1e3,
                t.tonemap * 1e3,
                t.upload * 1e3
            ),
        };
        ui.painter().text(
            rect.left_top() + egui::vec2(8.0, 8.0),
            egui::Align2::LEFT_TOP,
            text,
            egui::FontId::proportional(11.0),
            Color32::WHITE,
        );
    }

    /// The files the view reads.
    #[must_use]
    pub fn files(&self) -> &Files {
        &self.files
    }

    /// Placed objects drawn.
    #[must_use]
    pub fn object_count(&self) -> usize {
        self.objects.len()
    }
}
