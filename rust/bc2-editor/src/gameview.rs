//! The 3D view's "Game-accurate" mode (rounds R5, R5b and R5c, `EDITOR_ARCHITECTURE.md` 3.11): the level drawn as
//! BFBC2's Direct3D 11 renderer draws it - DICE's own compiled shaders, textures, material constants and render
//! states from the level's `Dx11_Single.res`, under its weather's light, then DICE's own post-process (`bc2_render`).
//!
//! Drawn ([`bc2_render::world::World`]): the placed objects and the model groups' members with their shader-set
//! overrides, the whole terrain (every patch; the frame's culling keeps those in view; round RV1), the baked road and
//! decal strips, the visual water; then DICE's exposure, bloom and filmic tonemap passes.
//!
//! Round R5c made it interactive ([`bc2_render::live`]): the draw list is built once and kept; a frame writes only
//! the camera's values and leaves out what the camera cannot see (the same picture). Edits to the scene (moved,
//! turned, added, removed or duplicated objects) re-plan only those objects. The finished frame goes to egui's
//! device through a shared texture (no read back; `BC2_GAMEVIEW_COPY=1` or [`GameView::copy`] reads it back and
//! uploads it instead, as before). A click picks the object the game frame shows under the pointer, and the
//! selection is outlined on the game frame with its depth (seen edges solid, hidden ones faint); the gizmo and the
//! labels stay egui's overlays above it.

use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::Path;
use std::time::Instant;

use bc2_render::files::Files;
use bc2_render::frame;
use bc2_render::live::{Live, Options};
use bc2_render::world::{Placed, World};
use bc2_scene::scene::{Scene, Source};
use eframe::egui::{self, Color32, ColorImage, Pos2, Rect, TextureHandle, TextureOptions};
use eframe::egui_wgpu::wgpu;

use crate::camera::Camera;
use crate::error::EditorError;

#[cfg(windows)]
use bc2_render::d3d::Device;

/// Without Direct3D 11 the mode only says why.
#[cfg(not(windows))]
struct Device;

#[cfg(not(windows))]
impl Device {
    fn new(_warp: bool) -> Result<Self, EditorError> {
        Err(EditorError::Gpu(
            "the game-accurate view needs Direct3D 11 (Windows)".into(),
        ))
    }

    fn forget_texture(&mut self, _name: &str) {}
}

/// The far plane, metres.
pub const FAR: f64 = 4000.0;
/// The environment variable that turns the shared texture off (the frame read back and uploaded instead).
pub const COPY_ENV: &str = "BC2_GAMEVIEW_COPY";
/// The selection's outline on the game frame (the view's selection orange): seen edges, hidden edges.
pub const OUTLINE: ([f32; 4], [f32; 4]) = ([1.0, 0.45, 0.0, 1.0], [1.0, 0.45, 0.0, 0.4]);

/// What the last frame cost, in seconds.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Timings {
    /// Building the frame's draws (the camera's values, culling, the blended draws' order).
    pub build: f64,
    /// Making the draw calls (constant upload included).
    pub submit: f64,
    /// Waiting for the GPU (the frame finished and shared), or the read back of the copy fallback.
    pub readback: f64,
    /// The post-process passes' submit.
    pub tonemap: f64,
    /// Handing the picture to egui (opening the shared textures, or the upload).
    pub upload: f64,
    /// Draw calls.
    pub draws: usize,
    /// Draws in the whole list.
    pub listed: usize,
    /// The frame went through the shared texture.
    pub shared: bool,
}

impl Timings {
    /// The whole frame.
    #[must_use]
    pub fn total(&self) -> f64 {
        self.build + self.submit + self.readback + self.tonemap + self.upload
    }
}

/// The shared textures opened on egui's device.
struct Opened {
    generation: u32,
    textures: Vec<(wgpu::Texture, egui::TextureId)>,
    /// egui's renderer they are registered with (round DF1, audit AU4-CQ-2: freed when the view goes).
    renderer: std::sync::Arc<eframe::epaint::mutex::RwLock<eframe::egui_wgpu::Renderer>>,
}

impl Drop for Opened {
    fn drop(&mut self) {
        let mut r = self.renderer.write();
        for (_, id) in &self.textures {
            r.free_texture(id);
        }
    }
}

/// The level, ready to draw the game's way.
pub struct GameView {
    world: World,
    live: Live,
    device: Device,
    texture: Option<TextureHandle>,
    opened: Option<Opened>,
    shown: Option<egui::TextureId>,
    drawn: Option<u64>,
    scene: Option<u64>,
    /// The patch the terrain window is around ([`World::terrain_window`]; `None` while the whole terrain is drawn),
    /// when the live list has been given its sources.
    window: Option<(usize, usize)>,
    sourced: bool,
    /// Each placed thing's entity (the placed list is the scene's entities with a blueprint and a transform).
    entity_of: Vec<usize>,
    /// Read the frame back and upload it (no shared texture).
    pub copy: bool,
    /// Leave out what the camera cannot see (the same picture; off for proofs).
    pub cull: bool,
    /// The last frame's costs.
    pub timings: Timings,
    /// Loading took (seconds).
    pub load_seconds: f64,
    /// What was found while loading (missing textures, patch shaders without a program ...).
    pub notes: Vec<String>,
    /// The last frame's error.
    pub error: Option<String>,
    /// The last frame's camera, size and depth (round R5d): what [`GameView::hidden`] tests labels against.
    depth: Option<(frame::Camera, [u32; 2], Vec<f32>)>,
    /// The entities left out of the frame (round VW2): deleted or hidden by the project, or filtered out by the mode
    /// view layer; [`GameView::sync`] follows it.
    pub skip: HashSet<usize>,
    /// The Direct3D device was removed (round DF1, audit AU4-BUG-9: a driver reset, update or GPU switch), why: the
    /// view cannot draw again and is loaded anew ([`GameView::lost`]).
    lost: Option<String>,
}

/// A level's drawable world read for the game-accurate view, without a device (round OP1: read on a worker, so the
/// editor's window keeps drawing while a large level loads; audit AU4 CQ-6's other half).
pub struct ReadWorld {
    world: World,
    entity_of: Vec<usize>,
    fingerprint: u64,
    seconds: f64,
}

/// Reads a level's world for the game-accurate view: its shader database, weather (the level's spawn-time state),
/// placed objects, group members, terrain, strips, water and the post shaders.
///
/// # Errors
/// The level's database cannot be read.
pub fn read_world(opfor: &Path, lv: &str, scene: &Scene) -> Result<ReadWorld, EditorError> {
    let t = Instant::now();
    // the weather comes from the placed weather entity's references (`World::load`'s `resolve_weather`); this
    // folder is read by name only when the level places no entity that resolves
    let weather = scene
        .entities
        .iter()
        .filter_map(|e| e.blueprint.as_deref())
        .find_map(|b| b.find("/weathersystementity").and_then(|i| b.get(..i)));
    let (placed, entity_of) = placed_entities(scene);
    // a level of game\Package only (Vietnam, the Pacific War ...): read from its bundles in place (round R5c)
    let package = is_package(opfor, lv);
    let mut world = if package {
        World::load_package(opfor, lv, &scene.name, &placed, weather)?
    } else {
        World::load(opfor, lv, &scene.name, &placed, weather)?
    };
    // the weather's sky dome behind the level (round R5d)
    world.sky_pass = true;
    // round VW2: the level's own state, the one players see as it starts - its spawn-time switch when it names
    // another state than the settings' ActiveState (spec RNDR-WTHR-6)
    if let Some(lw) = world.level_weather.clone()
        && lw.starts_in() != lw.active
    {
        match world.set_state(lw.starts_in()) {
            Ok(()) => world.notes.push(format!(
                "weather: the level's spawn-time switch {} drawn (its settings' ActiveState is {} {})",
                lw.name(lw.starts_in()),
                lw.active,
                lw.name(lw.active)
            )),
            Err(e) => world.notes.push(format!("weather: the spawn-time switch's state: {e}")),
        }
    }
    Ok(ReadWorld {
        world,
        entity_of,
        fingerprint: fingerprint(scene, &HashSet::new()),
        seconds: t.elapsed().as_secs_f64(),
    })
}

/// A game-accurate view being read on a worker (round OP1).
pub struct GameLoad {
    rx: crate::task::Task<ReadWorld>,
    /// Since when.
    pub since: Instant,
}

impl GameLoad {
    /// Starts reading the level's world on a worker (the scene's placed things taken now).
    #[must_use]
    pub fn start(opfor: &Path, lv: &str, scene: &Scene) -> Self {
        let (opfor, lv, scene) = (opfor.to_path_buf(), lv.to_owned(), scene.clone());
        Self {
            rx: crate::task::Task::spawn("The game-accurate view", move || read_world(&opfor, &lv, &scene)),
            since: Instant::now(),
        }
    }

    /// The view, once read (`None`: still reading); a worker that stopped without an answer is an error.
    #[must_use]
    pub fn poll(&self) -> Option<Result<GameView, EditorError>> {
        self.rx.poll().map(|r| r.and_then(GameView::finish))
    }

    /// Waits for the view (headless runs and tests).
    ///
    /// # Errors
    /// As [`GameView::load`].
    pub fn wait(self) -> Result<GameView, EditorError> {
        self.rx.wait().and_then(GameView::finish)
    }
}

/// The game-accurate view of an open level (round OP1: one state for "loading" and "loaded").
#[derive(Default)]
pub enum GameSlot {
    /// Not loaded.
    #[default]
    Off,
    /// Being read on a worker.
    Loading(GameLoad),
    /// Loaded.
    Ready(Box<GameView>),
}

impl GameSlot {
    /// The loaded view.
    #[must_use]
    pub fn as_ref(&self) -> Option<&GameView> {
        match self {
            Self::Ready(g) => Some(g),
            _ => None,
        }
    }

    /// The loaded view, to change.
    pub fn as_mut(&mut self) -> Option<&mut GameView> {
        match self {
            Self::Ready(g) => Some(g),
            _ => None,
        }
    }

    /// Whether no view is loaded (also while one is being read).
    #[must_use]
    pub fn is_none(&self) -> bool {
        !matches!(self, Self::Ready(_))
    }

    /// Whether a view is loaded.
    #[must_use]
    pub fn is_some(&self) -> bool {
        matches!(self, Self::Ready(_))
    }

    /// Whether one is being read.
    #[must_use]
    pub fn loading(&self) -> bool {
        matches!(self, Self::Loading(_))
    }
}

/// The scene's placed things as the renderer takes them - the level partition's instances (by GUID) and the group
/// members (by group file and item) - and each one's entity.
#[must_use]
pub fn placed_entities(scene: &Scene) -> (Vec<Placed>, Vec<usize>) {
    placed_without(scene, &HashSet::new())
}

/// [`placed_entities`] leaving out the entities in `skip` (round VW2: those the project deletes, hides or the mode
/// view layer filters out).
#[must_use]
#[allow(clippy::implicit_hasher, reason = "the view's own set")]
pub fn placed_without(scene: &Scene, skip: &HashSet<usize>) -> (Vec<Placed>, Vec<usize>) {
    scene
        .entities
        .iter()
        .filter(|e| !skip.contains(&e.index))
        .filter_map(|e| {
            Some((
                Placed {
                    guid: e.guid.clone(),
                    blueprint: e.blueprint.clone()?,
                    rows: e.matrix?,
                    group: match e.source {
                        Source::GroupMember { item, .. } => Some((e.group.clone()?, item)),
                        Source::Instance { .. } => None,
                    },
                },
                e.index,
            ))
        })
        .unzip()
}

/// Whether a level lives only in the game copy's `game\Package` (Vietnam, the Pacific War ...): read from its
/// bundles in place, not from a prepared `work\levels` folder.
#[must_use]
pub fn is_package(opfor: &Path, lv: &str) -> bool {
    !opfor
        .join("work")
        .join("levels")
        .join(lv.to_lowercase())
        .join("dist")
        .is_dir()
        && crate::package::find(opfor, lv).is_some()
}

/// The scene's placed things as the renderer takes them.
#[must_use]
pub fn placed(scene: &Scene) -> Vec<Placed> {
    placed_entities(scene).0
}

/// What the placed things of a scene are, as a number: changes when one is moved, added or removed.
fn fingerprint(scene: &Scene, skip: &HashSet<usize>) -> u64 {
    let mut h = DefaultHasher::new();
    for e in scene.entities.iter().filter(|e| !skip.contains(&e.index)) {
        if let (Some(b), Some(m)) = (&e.blueprint, &e.matrix) {
            e.index.hash(&mut h);
            e.guid.hash(&mut h);
            b.hash(&mut h);
            e.group.hash(&mut h);
            for v in m {
                v.to_bits().hash(&mut h);
            }
        }
    }
    h.finish()
}

/// The graphics features the game-accurate view asks of egui's device when the adapter has them: Vulkan's
/// external memory (the shared frame).
#[must_use]
pub fn device_features(adapter: &wgpu::Adapter) -> wgpu::Features {
    adapter.features() & wgpu::Features::VULKAN_EXTERNAL_MEMORY_WIN32
}

/// Opens a Direct3D 11 texture shared by NT handle on egui's device (Vulkan, with external memory).
#[cfg(windows)]
#[allow(
    unsafe_code,
    reason = "wgpu's hal interop: a Vulkan image on the Direct3D 11 texture's memory"
)]
fn open_shared(device: &wgpu::Device, handle: isize, size: [u32; 2]) -> Option<wgpu::Texture> {
    use wgpu::hal;
    if !device.features().contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_WIN32) {
        return None;
    }
    let extent = wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    };
    let hal_desc = hal::TextureDescriptor {
        label: Some("game-accurate view (shared)"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUses::RESOURCE,
        memory_flags: hal::MemoryFlags::empty(),
        view_formats: Vec::new(),
    };
    let desc = wgpu::TextureDescriptor {
        label: Some("game-accurate view (shared)"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    };
    // SAFETY: the handle is an NT handle of a live R8G8B8A8_UNORM texture of `size` (bc2_render's `share`), kept
    // open by the Direct3D device for as long as this view lives; the image is wrapped as uninitialised, so wgpu
    // moves it out of its undefined layout itself; the Direct3D device has finished writing (an event query waited
    // for) before egui samples it, and writes the three textures in turn, each only after the editor's device has
    // finished every frame sent before (round DF1: `draw` waits for it)
    unsafe {
        let hal_tex = {
            let hal_dev = device.as_hal::<hal::api::Vulkan>()?;
            hal_dev
                .texture_from_d3d11_shared_handle(windows::Win32::Foundation::HANDLE(handle as _), &hal_desc)
                .ok()?
        };
        Some(device.create_texture_from_hal::<hal::api::Vulkan>(hal_tex, &desc, wgpu::TextureUses::UNINITIALIZED))
    }
}

impl GameView {
    /// Loads a level's draws: its shader database, weather, placed objects, group members, terrain, strips, water and
    /// the post shaders.
    ///
    /// # Errors
    /// No Direct3D 11 device, or the level's database cannot be read. A level without weather data opens under a
    /// default sky ([`GameView::status`]).
    pub fn load(opfor: &Path, lv: &str, scene: &Scene) -> Result<Self, EditorError> {
        Self::finish(read_world(opfor, lv, scene)?)
    }

    /// The view made from a world read on a worker ([`GameLoad`]): the Direct3D device is made here, on the thread
    /// that draws with it.
    ///
    /// # Errors
    /// No Direct3D 11 device.
    pub fn finish(w: ReadWorld) -> Result<Self, EditorError> {
        let t = Instant::now();
        let device = Device::new(false)?;
        let ReadWorld {
            world,
            entity_of,
            fingerprint: scene_fp,
            seconds,
        } = w;
        Ok(Self {
            notes: world.notes.clone(),
            live: Live::new(&world.light),
            world,
            device,
            texture: None,
            opened: None,
            shown: None,
            drawn: None,
            scene: Some(scene_fp),
            window: None,
            sourced: false,
            entity_of,
            copy: std::env::var_os(COPY_ENV).is_some(),
            cull: true,
            timings: Timings::default(),
            load_seconds: seconds + t.elapsed().as_secs_f64(),
            error: None,
            depth: None,
            skip: HashSet::new(),
            lost: None,
        })
    }

    /// Draws the level under state `index` of its weather system (round VW2): the light, post-process, clear colour
    /// and sky of that state; every draw is planned again with the new light at the next frame.
    ///
    /// # Errors
    /// The level's weather is not known or the state's partitions are not read.
    pub fn set_state(&mut self, index: usize) -> Result<(), EditorError> {
        self.world.set_state(index)?;
        self.live = Live::new(&self.world.light);
        self.sourced = false;
        self.drawn = None;
        Ok(())
    }

    /// The terrain's stored heights replaced (round E5: a sculpt, its undo): the height map drawn anew at the next
    /// frame. Returns whether anything changed.
    pub fn set_terrain_heights(&mut self, raw: &[u16]) -> bool {
        if !self.world.set_terrain_heights(raw) {
            return false;
        }
        self.device.forget_texture(bc2_render::terrain::HEIGHTMAP);
        self.sourced = false;
        self.drawn = None;
        true
    }

    /// Whether a world point lies behind what the last game frame drew (round R5d): the labels drawn over the game
    /// view leave such points out ([`bc2_render::frame::occluded`]). `false` before a frame, or off the frame.
    #[must_use]
    pub fn hidden(&self, p: [f64; 3]) -> bool {
        self.depth
            .as_ref()
            .and_then(|(cam, size, d)| frame::occluded(cam, *size, d, p))
            .unwrap_or(false)
    }

    /// What the status line says once the view has loaded: the load time, and when the level has no weather data the
    /// view could read, that a default sky is shown (round GW1: such a level opens, it is no longer a failure).
    #[must_use]
    pub fn status(&self) -> String {
        let loaded = format!("Game-accurate view: loaded in {:.1} s", self.load_seconds);
        if self.world.default_weather {
            format!("{loaded}. {}", bc2_render::weather::NO_WEATHER)
        } else {
            loaded
        }
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

    /// Follows the scene: its placed things moved, added or removed since the last look are drawn so (only those
    /// planned again). Returns whether anything changed.
    pub fn sync(&mut self, scene: &Scene) -> bool {
        let fp = fingerprint(scene, &self.skip);
        if self.scene == Some(fp) {
            return false;
        }
        self.scene = Some(fp);
        let (placed, entity_of) = placed_without(scene, &self.skip);
        self.notes.extend(self.world.set_placed(&placed));
        self.entity_of = entity_of;
        // the instances again (the unchanged ones kept as planned) at the next frame
        self.sourced = false;
        self.drawn = None;
        true
    }

    /// The live list follows the objects (and the terrain window, when the world draws one).
    fn update_sources(&mut self, target: [f64; 3]) {
        let under = self.world.terrain_window(target);
        if !self.sourced || self.window != under {
            self.sourced = true;
            self.window = under;
            let sources = self.world.sources(target);
            self.live.set(sources);
        }
    }

    fn key(camera: &Camera, size: [u32; 2], selection: &[usize], cull: bool) -> u64 {
        let mut h = DefaultHasher::new();
        for v in camera.target {
            v.to_bits().hash(&mut h);
        }
        for v in [camera.yaw, camera.pitch, camera.distance, camera.fov] {
            v.to_bits().hash(&mut h);
        }
        size.hash(&mut h);
        selection.hash(&mut h);
        cull.hash(&mut h);
        h.finish()
    }

    /// Draws the frame for a camera (when it, the size, the selection or the scene changed) and returns the
    /// picture's texture.
    pub fn frame(
        &mut self,
        ctx: &egui::Context,
        gpu: Option<&crate::app::GpuCtx>,
        camera: &Camera,
        size: [u32; 2],
        selection: &[usize],
    ) -> Option<egui::TextureId> {
        let key = Self::key(camera, size, selection, self.cull);
        if self.drawn != Some(key) {
            self.drawn = Some(key);
            match self.draw(ctx, gpu, camera, size, selection) {
                Ok(()) => self.error = None,
                Err(e) => {
                    self.lost = self.removed();
                    self.error = Some(e.to_string());
                }
            }
        }
        self.shown
    }

    /// Why the view's Direct3D device was removed (`None`: it works): such a view is dropped and loaded again on a
    /// new device (round DF1, audit AU4-BUG-9).
    #[must_use]
    pub fn lost(&self) -> Option<&str> {
        self.lost.as_deref()
    }

    #[cfg(windows)]
    fn removed(&self) -> Option<String> {
        self.device.removed()
    }

    #[cfg(not(windows))]
    #[allow(clippy::unused_self, reason = "the Windows view's signature")]
    fn removed(&self) -> Option<String> {
        None
    }

    /// The pick numbers (the placed list's places plus one) of entities.
    fn picks(&self, entities: &[usize]) -> Vec<u32> {
        self.entity_of
            .iter()
            .enumerate()
            .filter(|(_, e)| entities.contains(e))
            .map(|(k, _)| k as u32 + 1)
            .collect()
    }

    #[cfg(windows)]
    fn draw(
        &mut self,
        ctx: &egui::Context,
        gpu: Option<&crate::app::GpuCtx>,
        camera: &Camera,
        size: [u32; 2],
        selection: &[usize],
    ) -> Result<(), EditorError> {
        let t = Instant::now();
        let cam = Self::camera(camera, size);
        self.update_sources(camera.target);
        self.live.build(
            &cam,
            size,
            Options {
                cull: self.cull,
                lod: true,
            },
        );
        let (depth, post) = self.world.passes(&cam, size, true)?;
        let build = t.elapsed().as_secs_f64();
        let raw = self.world.raw();
        let st = self.device.render_live(
            &self.live,
            size,
            self.world.clear(),
            depth.as_ref(),
            &post,
            &self.world.textures,
            raw,
        )?;
        // the frame's depth for the labels (round R5d), before the outline replays into the depth-stencil target
        let td = Instant::now();
        self.depth = self.device.read_depth().ok().map(|(s, d)| (cam, s, d));
        let depth_read = td.elapsed().as_secs_f64();
        let out = post.last().ok_or(EditorError::Gpu(
            "the post-process shaders are missing (startup-00.fbrb): no picture to show".into(),
        ))?;
        let picks = self.picks(selection);
        if !picks.is_empty() {
            self.device
                .outline(&self.live, &picks, out, OUTLINE.0, OUTLINE.1, &self.world.textures, raw)?;
        }
        let t2 = Instant::now();
        let mut shared = false;
        let mut upload = 0.0;
        if !self.copy
            && let Some(g) = gpu
            && g.gpu
                .device
                .features()
                .contains(wgpu::Features::VULKAN_EXTERNAL_MEMORY_WIN32)
        {
            // round DF1 (audit AU4-CON-11): every frame egui has sent so far has finished sampling before Direct3D
            // writes the next shared texture (no frame of the editor's device still reads the slot written now;
            // the Direct3D side's own writes are waited for by `share`'s event query)
            let _ = g.gpu.device.poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(std::time::Duration::from_secs(1)),
            });
            let f = self.device.share(out)?;
            let t3 = Instant::now();
            if self.opened.as_ref().is_none_or(|o| o.generation != f.generation) {
                self.open(g, f.generation, size)?;
            }
            self.shown = self
                .opened
                .as_ref()
                .and_then(|o| o.textures.get(f.slot))
                .map(|(_, id)| *id);
            shared = self.shown.is_some();
            upload = t3.elapsed().as_secs_f64();
        }
        let readback;
        if shared {
            readback = t2.elapsed().as_secs_f64() - upload;
        } else {
            let mut ldr = self.device.read_output(out)?;
            readback = t2.elapsed().as_secs_f64();
            // the frame is opaque (the tonemap's alpha is not coverage), as the shared texture's
            for px in ldr.as_chunks_mut::<4>().0 {
                px[3] = 255;
            }
            let t3 = Instant::now();
            let image = ColorImage::from_rgba_unmultiplied([size[0] as usize, size[1] as usize], &ldr);
            match &mut self.texture {
                Some(h) => h.set(image, TextureOptions::LINEAR),
                None => self.texture = Some(ctx.load_texture("game-accurate view", image, TextureOptions::LINEAR)),
            }
            self.shown = self.texture.as_ref().map(TextureHandle::id);
            upload = t3.elapsed().as_secs_f64();
        }
        self.timings = Timings {
            build,
            submit: st.making + st.upload + st.submit,
            readback: readback + depth_read,
            tonemap: st.post,
            upload,
            draws: st.draws,
            listed: self.live.draw_count(),
            shared,
        };
        Ok(())
    }

    #[cfg(not(windows))]
    #[allow(
        clippy::unused_self,
        clippy::unnecessary_wraps,
        reason = "the Windows view's signature"
    )]
    fn draw(
        &mut self,
        _ctx: &egui::Context,
        _gpu: Option<&crate::app::GpuCtx>,
        _camera: &Camera,
        _size: [u32; 2],
        _selection: &[usize],
    ) -> Result<(), EditorError> {
        Err(EditorError::Gpu(
            "the game-accurate view needs Direct3D 11 (Windows)".into(),
        ))
    }

    /// Opens the device's three shared textures on egui's device and registers them with egui.
    #[cfg(windows)]
    fn open(&mut self, g: &crate::app::GpuCtx, generation: u32, size: [u32; 2]) -> Result<(), EditorError> {
        let handles = self.device.shared_handles();
        // the old frames freed first (their drop takes the renderer's lock itself)
        drop(self.opened.take());
        let mut r = g.egui.write();
        let mut textures = Vec::new();
        for h in handles {
            let Some(tex) = open_shared(&g.gpu.device, h, size) else {
                for (_, id) in &textures {
                    r.free_texture(id);
                }
                return Err(EditorError::Gpu(
                    "the shared frame could not be opened on the editor's device".into(),
                ));
            };
            let view = tex.create_view(&wgpu::TextureViewDescriptor::default());
            let id = r.register_native_texture(&g.gpu.device, &view, wgpu::FilterMode::Linear);
            textures.push((tex, id));
        }
        drop(r);
        self.opened = Some(Opened {
            generation,
            textures,
            renderer: std::sync::Arc::clone(&g.egui),
        });
        Ok(())
    }

    /// The entity the game frame shows at a pixel (`at`, in the frame's pixels of a frame of `size`): `None` for
    /// terrain, water, sky or anything not placed. The next frame is drawn again (picking replays the frame's draws
    /// into its depth and stencil).
    ///
    /// # Errors
    /// No frame drawn yet, or a Direct3D call failed.
    #[cfg(windows)]
    pub fn pick(&mut self, at: [u32; 2], size: [u32; 2]) -> Result<Option<usize>, EditorError> {
        let Some(cam) = self.live.camera else {
            return Err(EditorError::Gpu("no game-accurate frame drawn yet".into()));
        };
        let (depth, _) = self.world.passes(&cam, size, false)?;
        let id = self.device.pick(
            &self.live,
            size,
            at,
            depth.as_ref(),
            &self.world.textures,
            self.world.raw(),
        )?;
        self.drawn = None;
        Ok((id as usize)
            .checked_sub(1)
            .and_then(|k| self.entity_of.get(k).copied()))
    }

    /// Without Direct3D 11 nothing is picked.
    ///
    /// # Errors
    /// Always: no frame.
    #[cfg(not(windows))]
    pub fn pick(&mut self, _at: [u32; 2], _size: [u32; 2]) -> Result<Option<usize>, EditorError> {
        Err(EditorError::Gpu(
            "the game-accurate view needs Direct3D 11 (Windows)".into(),
        ))
    }

    /// Paints the view: the frame (following the scene's edits, the selection outlined), and its costs in the
    /// corner.
    #[allow(clippy::too_many_arguments, reason = "the view's inputs")]
    pub fn paint(
        &mut self,
        ui: &egui::Ui,
        gpu: Option<&crate::app::GpuCtx>,
        rect: Rect,
        camera: &Camera,
        size: [u32; 2],
        scene: &Scene,
        selection: &[usize],
    ) {
        let ctx = ui.ctx().clone();
        self.sync(scene);
        if let Some(tex) = self.frame(&ctx, gpu, camera, size, selection) {
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
                "Game-accurate (Direct3D 11, DICE's shaders and post-process) - {} of {} draws, {:.1} ms: build {:.1}, \
                 submit {:.1}, GPU {:.1}, post {:.1}, {} {:.1}",
                t.draws,
                t.listed,
                t.total() * 1e3,
                t.build * 1e3,
                t.submit * 1e3,
                t.readback * 1e3,
                t.tonemap * 1e3,
                if t.shared { "shared" } else { "upload" },
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

    /// Draws the frame again at the next paint (after a switch such as [`GameView::copy`]).
    pub fn redraw(&mut self) {
        self.drawn = None;
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

    /// The world drawn, to draw parts of it on another device (tests and proofs: the terrain patches alone, round
    /// GW1). The view's own live list is not changed.
    pub fn world_mut(&mut self) -> &mut World {
        &mut self.world
    }

    /// The live draw list.
    #[must_use]
    pub fn live(&self) -> &Live {
        &self.live
    }

    /// Draws the frame for a camera now and reads it back (tests and proofs): the scene's HDR colour, depth and draw
    /// classes, and the picture shown (the selection outlined).
    ///
    /// # Errors
    /// A Direct3D call failed.
    #[cfg(windows)]
    pub fn read_back(
        &mut self,
        camera: &Camera,
        size: [u32; 2],
        selection: &[usize],
    ) -> Result<frame::Rendered, EditorError> {
        let cam = Self::camera(camera, size);
        self.update_sources(camera.target);
        self.live.build(
            &cam,
            size,
            Options {
                cull: self.cull,
                lod: true,
            },
        );
        let (depth, post) = self.world.passes(&cam, size, true)?;
        let raw = self.world.raw();
        self.device.render_live(
            &self.live,
            size,
            self.world.clear(),
            depth.as_ref(),
            &post,
            &self.world.textures,
            raw,
        )?;
        let picks = self.picks(selection);
        if let Some(out) = post.last()
            && !picks.is_empty()
        {
            self.device
                .outline(&self.live, &picks, out, OUTLINE.0, OUTLINE.1, &self.world.textures, raw)?;
        }
        self.drawn = None;
        Ok(self.device.read_scene(post.last())?)
    }
}
