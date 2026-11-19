//! The 3D view's "Game-accurate" mode (rounds R5 and R5b, `EDITOR_ARCHITECTURE.md` 3.11): the level drawn as BFBC2's
//! Direct3D 11 renderer draws it - DICE's own compiled shaders, textures, material constants and render states from
//! the level's `Dx11_Single.res`, under its weather's light, then DICE's own post-process (`bc2_render`) - off screen,
//! then shown in the view as an image.
//!
//! Drawn ([`bc2_render::world::World`]): the placed objects and the model groups' members with their shader-set
//! overrides, the terrain patches around the camera's target, the baked road and decal strips, the visual water;
//! then DICE's exposure, bloom and filmic tonemap passes (the harness's CPU tonemap where the post shaders cannot be
//! read). The selection, labels and gizmo stay egui's overlays. The frame is drawn again only when the camera or the
//! view's size changes. On a machine without Direct3D 11 the mode says why and the view stays as it was.

use std::path::Path;
use std::time::Instant;

use bc2_render::files::Files;
use bc2_render::frame;
use bc2_render::world::{Placed, World};
use bc2_scene::scene::{Scene, Source};
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
        _files: &bc2_render::material::TextureFiles,
        _raw: &std::collections::HashMap<String, bc2_render::terrain::RawTexture>,
    ) -> Result<frame::Rendered, String> {
        Err("the game-accurate view needs Direct3D 11 (Windows)".into())
    }
}

/// Patches drawn on each side of the patch under the camera's target.
pub const TERRAIN_RADIUS: usize = bc2_render::world::TERRAIN_RADIUS;
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
    /// The post-process: DICE's passes' submit (their GPU time is in `readback`), or the CPU tonemap.
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
    world: World,
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

/// The scene's placed things as the renderer takes them: the level partition's instances (by GUID) and the group
/// members (by group file and item).
#[must_use]
pub fn placed(scene: &Scene) -> Vec<Placed> {
    scene
        .entities
        .iter()
        .filter_map(|e| {
            Some(Placed {
                guid: e.guid.clone(),
                blueprint: e.blueprint.clone()?,
                rows: e.matrix?,
                group: match e.source {
                    Source::GroupMember { item, .. } => Some((e.group.clone()?, item)),
                    Source::Instance { .. } => None,
                },
            })
        })
        .collect()
}

impl GameView {
    /// Loads a level's draws: its shader database, weather, placed objects, group members, terrain, strips, water and
    /// the post shaders.
    ///
    /// # Errors
    /// No Direct3D 11 device, or the level's database or weather cannot be read.
    pub fn load(opfor: &Path, lv: &str, scene: &Scene) -> Result<Self, String> {
        let t = Instant::now();
        let device = Device::new(false)?;
        let weather = scene
            .entities
            .iter()
            .filter_map(|e| e.blueprint.as_deref())
            .find_map(|b| b.find("/weathersystementity").and_then(|i| b.get(..i)));
        let world = World::load(opfor, lv, &scene.name, &placed(scene), weather)?;
        Ok(Self {
            notes: world.notes.clone(),
            world,
            device,
            texture: None,
            drawn: None,
            timings: Timings::default(),
            load_seconds: t.elapsed().as_secs_f64(),
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
        let f = self.world.frame(&cam, camera.target, size, true)?;
        let build = t.elapsed().as_secs_f64();
        let r = self.device.render(&f, &self.world.textures, self.world.raw())?;
        let t2 = Instant::now();
        let ldr = if r.ldr.is_empty() {
            let w = &self.world;
            let bg = bc2_render::tonemap::map(w.background, 1.0, &w.post).map(|v| (v * 255.0).round() as u8);
            bc2_render::tonemap::tonemap(&r.hdr, &r.mask(), &w.post, bg).0
        } else {
            r.ldr
        };
        let tonemap = r.post + t2.elapsed().as_secs_f64();
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
                "Game-accurate (Direct3D 11, DICE's shaders and post-process) - {} draws, {:.1} ms: submit {:.1}, GPU + \
                 read back {:.1}, post {:.1}, upload {:.1}",
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
        &self.world.files
    }

    /// Placed objects and group members drawn.
    #[must_use]
    pub fn object_count(&self) -> usize {
        self.world.objects.len()
    }

    /// The world drawn.
    #[must_use]
    pub fn world(&self) -> &World {
        &self.world
    }
}
