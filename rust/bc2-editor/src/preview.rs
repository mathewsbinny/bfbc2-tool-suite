//! The asset browser's preview (round GR3: the mesh viewer's page folded into the editor). A blueprint picked in the
//! asset browser, or any mesh of the game copy found by name (the browser's Meshes mode: `meshview.py find`), is read
//! as the mesh viewer reads it ([`bc2_convert::meshview`]: the same index and cache, parts, vertices, colour textures)
//! and drawn on the CPU with the mesh viewer's own triangle loop - so its Side, Top and Front pictures are
//! `meshview.py --png`'s pixel for pixel, and the orbiting camera draws the same way.
//!
//! What it shows besides the mesh viewer's page: each level of detail, the blueprint's destruction states (its
//! health states' mesh parts, as the simplified view picks them), every texture of each part's shader by channel
//! (colour, normal, specular, mask, occlusion, detail, environment), wireframe, normals, the bounds and their
//! measurements, the selected part in the viewport's selection colour, several meshes stacked for comparison, and
//! Save picture.
//!
//! Reading happens on a worker thread; the index is read once per process (from `work\tools_out\meshview_index.pkl`,
//! or built and cached when the game copy changed).

#![allow(clippy::indexing_slicing, reason = "[f32; 3] vectors indexed by their axes (0..3)")]

use crate::error::EditorError;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use bc2_convert::meshview::render::{Canvas, Laid, Look, PartJob, View, frame, light, stack_models};
use bc2_convert::meshview::{self, Channel, Image, Index, Mesh, Part};
use bc2_render::simple::{Component, State, part_shown};

/// What the preview shows.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Subject {
    /// A blueprint (`objects/props/crate/crate`): its `Mesh`, its destruction states.
    Blueprint(String),
    /// A mesh by name (`Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh`).
    Mesh(String),
}

impl Subject {
    /// Its name for the panel (the last part of the path).
    #[must_use]
    pub fn label(&self) -> &str {
        let p = match self {
            Self::Blueprint(p) | Self::Mesh(p) => p.as_str(),
        };
        p.rsplit('/').next().unwrap_or(p)
    }
}

/// A shader's textures, each with its channel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Material {
    /// The surface (lower case) and its shader.
    pub surface: String,
    /// The shader (`<mesh>/<shader>` names its database entry).
    pub shader: String,
    /// Its textures with their channels, in the database's order.
    pub textures: Vec<(String, Channel)>,
}

/// One mesh read for the preview.
#[derive(Debug, Clone)]
pub struct Model {
    /// What was asked for.
    pub subject: Subject,
    /// The mesh as the bundles spell it.
    pub mesh: String,
    /// The levels of detail it has.
    pub lods: Vec<usize>,
    /// The level read and its parts.
    pub read: Mesh,
    /// The blueprint's destructible parts (empty: none, or a mesh asked for by name).
    pub components: Vec<Component>,
    /// Each surface's shader and textures.
    pub materials: Vec<Material>,
}

impl Model {
    /// Triangles of every part.
    #[must_use]
    pub fn triangles(&self) -> usize {
        self.read.parts.iter().map(|p| p.indices.len()).sum()
    }

    /// Vertices of every part.
    #[must_use]
    pub fn vertices(&self) -> usize {
        self.read.parts.iter().map(|p| p.positions.len()).sum()
    }
}

/// The index, read once per process.
static INDEX: Mutex<Option<Arc<Index>>> = Mutex::new(None);

/// The game copy's mesh index (read or built once per process; [`Index::load`]).
///
/// # Errors
/// As [`Index::load`].
pub fn index(opfor: &Path) -> Result<Arc<Index>, String> {
    let mut slot = INDEX.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(ix) = slot.as_ref() {
        return Ok(Arc::clone(ix));
    }
    // os.path.abspath as the Python mesh viewer takes it (Windows' GetFullPathNameW, which std::path::absolute calls):
    // the cache's signature holds the bundle paths made from this root
    let root = std::path::absolute(opfor).map_err(|e| format!("{}: {e}", opfor.display()))?;
    let ix = Arc::new(Index::load(&root.to_string_lossy(), &mut |_| {})?);
    *slot = Some(Arc::clone(&ix));
    Ok(ix)
}

/// The index when it was already read (no waiting).
#[must_use]
pub fn index_ready() -> Option<Arc<Index>> {
    INDEX.lock().unwrap_or_else(PoisonError::into_inner).clone()
}

/// Reads one subject at a level of detail (`None`: the finest it has).
///
/// # Errors
/// The blueprint names no mesh, or the mesh is not in the stock bundles.
pub fn read_model(ix: &Index, subject: &Subject, lod: Option<usize>) -> Result<Model, String> {
    let (given, components) = match subject {
        Subject::Mesh(m) => (m.clone(), Vec::new()),
        Subject::Blueprint(bp) => {
            let data = ix.read(&format!("{bp}.dbx")).ok_or_else(|| {
                format!("{bp}: not in the stock game's bundles (the mesh index covers the stock game only)")
            })?;
            let xml = bc2_formats::dbx::to_xml(&data).map_err(|e| format!("{bp}: {e}"))?;
            let mesh = meshview::blueprint_mesh(&xml)
                .ok_or_else(|| format!("{bp}: its blueprint names no mesh (a logic, trigger or effect blueprint)"))?;
            (mesh, bc2_render::simple::components_in(&data))
        }
    };
    let probe = ix.mesh_name(&given, 0);
    let lods = ix.lods(&probe);
    let lod = lod
        .filter(|l| lods.contains(l))
        .or_else(|| lods.first().copied())
        .unwrap_or(0);
    let mesh = ix.mesh_name(&given, lod);
    let read = meshview::load_mesh(ix, &mesh, lod)?;
    let materials = meshview::mesh_surfaces(ix, &mesh)
        .into_iter()
        .map(|(surface, shader)| Material {
            textures: ix
                .textures(&format!("{mesh}/{shader}"))
                .iter()
                .map(|t| (t.clone(), Channel::of(t)))
                .collect(),
            surface,
            shader,
        })
        .collect();
    Ok(Model {
        subject: subject.clone(),
        mesh,
        lods,
        read,
        components,
        materials,
    })
}

/// How the preview looks at the models.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Camera {
    /// The mesh viewer's picture framing: the models' long axis across.
    Preset(View),
    /// Turning about the models' centre.
    Orbit {
        /// Turn about the up axis, radians.
        yaw: f32,
        /// Up / down, radians.
        pitch: f32,
        /// Distance in radii of the models' bounds.
        distance: f32,
        /// Moved across the view (pixels at the panel's size).
        pan: [f32; 2],
        /// Perspective (else orthographic).
        perspective: bool,
    },
}

impl Default for Camera {
    fn default() -> Self {
        Self::orbit()
    }
}

impl Camera {
    /// The orbit camera's start: from the front right, a little above.
    #[must_use]
    pub fn orbit() -> Self {
        Self::Orbit {
            yaw: 0.6,
            pitch: 0.35,
            distance: 2.6,
            pan: [0.0; 2],
            perspective: true,
        }
    }
}

/// How the parts are drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shading {
    /// The chosen channel's texture, lit by the facing (the mesh viewer's).
    #[default]
    Textured,
    /// One grey, lit by the facing.
    Flat,
    /// The triangles' edges only.
    Wireframe,
}

impl Shading {
    /// Every shading, in the toolbar's order.
    pub const ALL: [Self; 3] = [Self::Textured, Self::Flat, Self::Wireframe];

    /// Its name.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Textured => "Textured",
            Self::Flat => "Flat",
            Self::Wireframe => "Wireframe",
        }
    }
}

/// The colours the preview draws its overlays in (theme tokens).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colours {
    /// The background.
    pub background: [u8; 3],
    /// Edges over the shading.
    pub edges: [u8; 3],
    /// The selected part (`view-selection`).
    pub selection: [u8; 3],
    /// Bounds and normals (the accent).
    pub accent: [u8; 3],
}

impl Default for Colours {
    fn default() -> Self {
        Self {
            background: [40; 3],
            edges: [20; 3],
            selection: [255, 140, 0],
            accent: [64, 140, 255],
        }
    }
}

/// The preview's settings and what it shows.
#[derive(Default)]
#[allow(clippy::struct_excessive_bools, reason = "the toolbar's switches, one field each")]
pub struct Preview {
    /// What is shown first, and the others stacked below it for comparison.
    pub subjects: Vec<Subject>,
    /// The level of detail asked for.
    pub lod: usize,
    /// The models read (in [`Preview::subjects`]' order).
    pub models: Vec<Model>,
    /// Reading, or what went wrong.
    pub status: String,
    /// The camera.
    pub camera: Camera,
    /// How parts are drawn.
    pub shading: Shading,
    /// The texture channel drawn (Textured).
    pub channel: Option<Channel>,
    /// Edges over the shading.
    pub wireframe: bool,
    /// The vertices' normals as lines.
    pub normals: bool,
    /// The bounds' box.
    pub bounds: bool,
    /// The destruction state (blueprints with health states).
    pub state: State,
    /// Hidden parts: (model, part name).
    pub hidden: HashSet<(usize, String)>,
    /// The selected part: (model, part index).
    pub selected: Option<(usize, usize)>,
    /// Textures decoded for the other channels, by name.
    textures: HashMap<String, Option<Arc<Image>>>,
    rx: Option<crate::task::Task<Vec<Model>>>,
    /// Bumped whenever what is drawn changes (the panel redraws then).
    pub revision: u64,
}

impl Preview {
    /// Shows `subject` alone (the asset browser's pick).
    pub fn show(&mut self, opfor: &Path, subject: Subject) {
        if self.subjects.len() == 1 && self.subjects.first() == Some(&subject) {
            return;
        }
        self.subjects = vec![subject];
        self.hidden.clear();
        self.selected = None;
        self.state = State::Intact;
        self.reload(opfor);
    }

    /// Adds `subject` below the others (compare).
    pub fn add(&mut self, opfor: &Path, subject: Subject) {
        if !self.subjects.contains(&subject) {
            self.subjects.push(subject);
            self.reload(opfor);
        }
    }

    /// Reads the subjects again (at [`Preview::lod`]) on a worker thread.
    pub fn reload(&mut self, opfor: &Path) {
        let subjects = self.subjects.clone();
        let lod = self.lod;
        let opfor = opfor.to_path_buf();
        self.status = if index_ready().is_some() {
            "Reading the mesh\u{2026}".into()
        } else {
            "Reading the game copy's mesh index (the first time: building it takes minutes)\u{2026}".into()
        };
        self.rx = Some(crate::task::Task::spawn("the preview's reader", move || {
            index(&opfor)
                .and_then(|ix| {
                    subjects
                        .iter()
                        .map(|s| read_model(&ix, s, Some(lod)))
                        .collect::<Result<Vec<_>, String>>()
                })
                .map_err(EditorError::Tool)
        }));
    }

    /// Whether a read is under way.
    #[must_use]
    pub fn reading(&self) -> bool {
        self.rx.is_some()
    }

    /// Takes the worker's models when they are read; true when something arrived.
    pub fn poll(&mut self) -> bool {
        let Some(r) = self.rx.as_ref().and_then(crate::task::Task::poll) else {
            return false;
        };
        match r {
            Ok(models) => {
                let skipped: usize = models.iter().map(|m| m.read.notes.len()).sum();
                self.status = if skipped > 0 {
                    format!("{skipped} parts skipped (no vertex layout in any level's shader database)")
                } else {
                    String::new()
                };
                self.models = models;
                if let Some(m) = self.models.first() {
                    self.lod = m.read.lod;
                }
                self.rx = None;
                self.revision += 1;
                true
            }
            Err(e) => {
                self.status = e.to_string();
                self.models.clear();
                self.rx = None;
                self.revision += 1;
                true
            }
        }
    }

    /// Waits for the read (tests), at most `limit`.
    pub fn wait(&mut self, limit: std::time::Duration) {
        let t = std::time::Instant::now();
        while self.reading() && t.elapsed() < limit {
            self.poll();
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    /// Whether a part is drawn: not hidden, and in the destruction state.
    #[must_use]
    pub fn part_drawn(&self, model: usize, part: &Part) -> bool {
        if self.hidden.contains(&(model, part.name.clone())) {
            return false;
        }
        self.models
            .get(model)
            .is_none_or(|m| part_shown(&m.components, &part.name, self.state))
    }

    /// The texture a part is drawn with in the chosen channel (Colour: the mesh viewer's).
    fn texture_for(&mut self, models: &[Model], model: usize, part: &Part) -> Option<Arc<Image>> {
        match self.channel {
            None | Some(Channel::Colour) => part.image.clone(),
            Some(ch) => {
                let m = models.get(model)?;
                let shader = part.shader.as_ref()?;
                let name = m
                    .materials
                    .iter()
                    .find(|mt| &mt.shader == shader)?
                    .textures
                    .iter()
                    .find(|(_, c)| *c == ch)?
                    .0
                    .clone();
                if let Some(t) = self.textures.get(&name) {
                    return t.clone();
                }
                let got = index_ready()
                    .and_then(|ix| ix.read(&format!("{name}.res")))
                    .and_then(|b| meshview::decode_texture(&b))
                    .map(Arc::new);
                self.textures.insert(name, got.clone());
                got
            }
        }
    }

    /// The models' offsets (stacked one above the other, as the mesh viewer lays several out).
    #[must_use]
    pub fn offsets(&self) -> Vec<[f32; 3]> {
        let all: Vec<&[Part]> = self.models.iter().map(|m| m.read.parts.as_slice()).collect();
        stack_models(&all)
    }

    /// The drawn parts' bounds in the preview's space (offsets added): (low, high).
    #[must_use]
    pub fn bounds_box(&self) -> Option<([f32; 3], [f32; 3])> {
        let offs = self.offsets();
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for (k, (m, off)) in self.models.iter().zip(&offs).enumerate() {
            for p in m.read.parts.iter().filter(|p| self.part_drawn(k, p)) {
                for t in &p.indices {
                    for &i in t {
                        if let Some(v) = p.positions.get(i as usize) {
                            for a in 0..3 {
                                lo[a] = lo[a].min(v[a] + off[a]);
                                hi[a] = hi[a].max(v[a] + off[a]);
                            }
                        }
                    }
                }
            }
        }
        lo[0].is_finite().then_some((lo, hi))
    }

    /// Draws the preview `w` pixels wide (a preset's height is its framing's; the orbit's `h`): the picture and,
    /// per pixel, the part drawn there (`(model, part)` index into [`Preview::part_ids`]).
    pub fn draw(&mut self, w: usize, h: usize, colours: Colours) -> (Image, Vec<u32>) {
        let offs = self.offsets();
        let bbox = self.bounds_box();
        let span = bbox.map_or(1.0, |(lo, hi)| (0..3).map(|a| hi[a] - lo[a]).fold(0.0f32, f32::max));
        let models = std::mem::take(&mut self.models);
        let mut ids: Vec<(usize, usize)> = Vec::new();
        let mut laid: Vec<Laid<'_>> = Vec::new();
        for (k, (m, off)) in models.iter().zip(&offs).enumerate() {
            let mut parts = Vec::new();
            for (pi, p) in m.read.parts.iter().enumerate() {
                if self.hidden.contains(&(k, p.name.clone())) || !part_shown(&m.components, &p.name, self.state) {
                    continue;
                }
                parts.push(p);
                ids.push((k, pi));
            }
            laid.push((parts, *off));
        }
        let (world, screen, cw, ch) = match self.camera {
            Camera::Preset(view) => match frame(&laid, w.max(48), view) {
                Some(f) => (f.world, f.screen, f.w, f.h),
                None => (Vec::new(), Vec::new(), w.max(48), h.max(48)),
            },
            Camera::Orbit {
                yaw,
                pitch,
                distance,
                pan,
                perspective,
            } => {
                let (world, screen) = orbit_screen(&laid, w.max(48), h.max(48), yaw, pitch, distance, pan, perspective);
                (world, screen, w.max(48), h.max(48))
            }
        };
        let mut canvas = Canvas::new(cw, ch, colours.background);
        let flat_parts: Vec<&Part> = laid.iter().flat_map(|(p, _)| p.iter().copied()).collect();
        if self.shading != Shading::Wireframe {
            // every part's look first, then the parts drawn in bands on all cores (round PR1b-0, audit AU5 ALG-9:
            // the orbit was drawn on the UI thread alone; the picture is the same, bit for bit)
            let textures: Vec<Option<Arc<Image>>> = flat_parts
                .iter()
                .enumerate()
                .map(|(k, p)| {
                    let model = ids.get(k).map_or(0, |x| x.0);
                    if self.shading == Shading::Textured {
                        self.texture_for(&models, model, p)
                    } else {
                        None
                    }
                })
                .collect();
            let jobs: Vec<PartJob<'_>> = flat_parts
                .iter()
                .zip(&world)
                .zip(&screen)
                .zip(&textures)
                .enumerate()
                .map(|(k, (((p, pos), scr), tex))| {
                    let look = Look {
                        texture: tex.as_deref(),
                        flat: [170.0; 3],
                        shade: p.normals.is_some() || self.shading == Shading::Flat,
                        id: k as u32,
                    };
                    (*p, pos.as_slice(), scr.as_slice(), look)
                })
                .collect();
            canvas.fill_parts(&jobs);
        }
        let edge_of = |canvas: &mut Canvas, p: &Part, scr: &[[f32; 3]], c: [u8; 3], bias: Option<f64>| {
            for t in &p.indices {
                let v = t.map(|i| scr.get(i as usize).copied().unwrap_or([f32::NAN; 3]));
                for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                    canvas.line(v[a], v[b], c, bias);
                }
            }
        };
        if self.wireframe || self.shading == Shading::Wireframe {
            let bias = (self.shading != Shading::Wireframe).then_some(f64::from(span) * 0.004);
            let c = if self.shading == Shading::Wireframe {
                [200; 3]
            } else {
                colours.edges
            };
            for (p, scr) in flat_parts.iter().zip(&screen) {
                edge_of(&mut canvas, p, scr, c, bias);
            }
        }
        if self.normals {
            let len = span * 0.03;
            for ((p, pos), _) in flat_parts.iter().zip(&world).zip(&screen) {
                let Some(n) = &p.normals else { continue };
                let ends: Vec<[f32; 3]> = pos
                    .iter()
                    .zip(n)
                    .take(40_000)
                    .flat_map(|(v, n)| [*v, std::array::from_fn(|a| v[a] + n[a] * len)])
                    .collect();
                let proj = self.project(&laid, &ends, cw, ch);
                for pair in proj.as_chunks::<2>().0 {
                    canvas.line(pair[0], pair[1], colours.accent, None);
                }
            }
        }
        if self.bounds
            && let Some((lo, hi)) = bbox
        {
            let corner =
                |i: usize| -> [f32; 3] { std::array::from_fn(|a| if i >> a & 1 == 0 { lo[a] } else { hi[a] }) };
            let mut ends = Vec::new();
            for i in 0..8usize {
                for a in 0..3 {
                    let j = i | 1 << a;
                    if j != i {
                        ends.extend([corner(i), corner(j)]);
                    }
                }
            }
            let proj = self.project(&laid, &ends, cw, ch);
            for pair in proj.as_chunks::<2>().0 {
                canvas.line(pair[0], pair[1], colours.accent, None);
            }
        }
        if let Some((sm, sp)) = self.selected
            && let Some(k) = ids.iter().position(|x| *x == (sm, sp))
            && let (Some(p), Some(scr)) = (flat_parts.get(k), screen.get(k))
        {
            edge_of(&mut canvas, p, scr, colours.selection, None);
        }
        drop(laid);
        self.models = models;
        let owner = canvas
            .owner
            .iter()
            .map(|&o| {
                ids.get(o as usize)
                    .map_or(u32::MAX, |(m, p)| ((*m as u32) << 20) | *p as u32)
            })
            .collect();
        (canvas.image, owner)
    }

    /// World points on the current camera's screen (the overlays'), with the same framing as the parts.
    fn project(&self, laid: &[Laid<'_>], points: &[[f32; 3]], w: usize, h: usize) -> Vec<[f32; 3]> {
        match self.camera {
            Camera::Preset(view) => {
                // the preset's framing from the parts, the points mapped with it
                let probe = Part {
                    name: String::new(),
                    surface: None,
                    shader: None,
                    positions: points.to_vec(),
                    normals: None,
                    uvs: None,
                    indices: Vec::new(),
                    texture: None,
                    image: None,
                };
                let mut with: Vec<Laid<'_>> = laid.to_vec();
                with.push((vec![&probe], [0.0; 3]));
                frame(&with, w, view)
                    .and_then(|f| f.screen.last().cloned())
                    .unwrap_or_default()
            }
            Camera::Orbit {
                yaw,
                pitch,
                distance,
                pan,
                perspective,
            } => {
                let basis = orbit_basis(laid, w, h, yaw, pitch, distance, pan, perspective);
                points.iter().map(|v| basis.apply(*v)).collect()
            }
        }
    }

    /// The part a pixel's owner value names: (model, part index).
    #[must_use]
    pub fn part_at(owner: u32) -> Option<(usize, usize)> {
        (owner != u32::MAX).then_some(((owner >> 20) as usize, (owner & 0xF_FFFF) as usize))
    }

    /// Saves a picture of the preview as a PNG: a preset at `size` pixels wide (`meshview.py --png`'s, 1200 by
    /// default), the orbit at `size` x `size` * 3 / 4.
    ///
    /// # Errors
    /// The file cannot be written.
    pub fn save_picture(&mut self, file: &Path, size: usize, colours: Colours) -> Result<(usize, usize), String> {
        let (img, _) = self.draw(size, size * 3 / 4, colours);
        write_png(file, &img)?;
        Ok((img.w, img.h))
    }

    /// Where Save picture writes: the mesh viewer's folder, `<OPFOR>\work\tools_out\meshview\<names>_<view>.png`.
    #[must_use]
    pub fn picture_file(&self, opfor: &Path) -> PathBuf {
        let stem: String = self
            .models
            .iter()
            .map(|m| m.mesh.rsplit('/').next().unwrap_or(&m.mesh).to_owned())
            .collect::<Vec<_>>()
            .join("+")
            .chars()
            .take(120)
            .collect();
        let view = match self.camera {
            Camera::Preset(v) => v.name(),
            Camera::Orbit { .. } => "orbit",
        };
        opfor.join("work").join("tools_out").join("meshview").join(format!(
            "{}_{view}.png",
            if stem.is_empty() { "preview" } else { &stem }
        ))
    }
}

/// An RGB picture written as a PNG.
///
/// # Errors
/// The file cannot be written.
pub fn write_png(file: &Path, img: &Image) -> Result<(), String> {
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let f = std::fs::File::create(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), img.w as u32, img.h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| format!("{}: {e}", file.display()))?;
    wr.write_image_data(&img.rgb)
        .map_err(|e| format!("{}: {e}", file.display()))
}

/// The orbit camera's transform from the preview's space to the screen.
#[derive(Debug, Clone, Copy)]
struct Basis {
    eye: [f32; 3],
    right: [f32; 3],
    up: [f32; 3],
    forward: [f32; 3],
    centre: [f32; 2],
    scale: f32,
    perspective: bool,
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: [f32; 3]) -> [f32; 3] {
    let l = dot(a, a).sqrt().max(1e-12);
    a.map(|v| v / l)
}

impl Basis {
    fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        let d = [v[0] - self.eye[0], v[1] - self.eye[1], v[2] - self.eye[2]];
        let (x, y, z) = (dot(d, self.right), dot(d, self.up), dot(d, self.forward));
        if self.perspective {
            if z <= 1e-4 {
                return [f32::NAN; 3];
            }
            [
                self.centre[0] + x * self.scale / z,
                self.centre[1] - y * self.scale / z,
                z,
            ]
        } else {
            [self.centre[0] + x * self.scale, self.centre[1] - y * self.scale, z]
        }
    }
}

/// The laid parts' centre and radius (of the vertices their triangles use).
fn sphere(laid: &[Laid<'_>]) -> ([f32; 3], f32) {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for (parts, off) in laid {
        for p in parts {
            for t in &p.indices {
                for &i in t {
                    if let Some(v) = p.positions.get(i as usize) {
                        for a in 0..3 {
                            lo[a] = lo[a].min(v[a] + off[a]);
                            hi[a] = hi[a].max(v[a] + off[a]);
                        }
                    }
                }
            }
        }
    }
    if !lo[0].is_finite() {
        return ([0.0; 3], 1.0);
    }
    let c = std::array::from_fn(|a| f32::midpoint(lo[a], hi[a]));
    let r = (0..3).map(|a| (hi[a] - lo[a]) * (hi[a] - lo[a])).sum::<f32>().sqrt() / 2.0;
    (c, r.max(1e-3))
}

#[allow(clippy::too_many_arguments, reason = "the orbit camera's values")]
fn orbit_basis(
    laid: &[Laid<'_>],
    w: usize,
    h: usize,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: [f32; 2],
    perspective: bool,
) -> Basis {
    let (c, r) = sphere(laid);
    let dir = [pitch.cos() * yaw.sin(), pitch.sin(), pitch.cos() * yaw.cos()];
    let eye: [f32; 3] = std::array::from_fn(|a| c[a] + dir[a] * r * distance);
    let forward = unit([c[0] - eye[0], c[1] - eye[1], c[2] - eye[2]]);
    let right = unit(cross(forward, [0.0, 1.0, 0.0]));
    let up = cross(right, forward);
    let (wf, hf) = (w as f32, h as f32);
    // 40 degrees of field (the mesh viewer page's camera); the orthographic view fits the same sphere
    let focal = (hf / 2.0) / (20f32.to_radians()).tan();
    let scale = if perspective { focal } else { focal / (r * distance) };
    Basis {
        eye,
        right,
        up,
        forward,
        centre: [wf / 2.0 + pan[0], hf / 2.0 + pan[1]],
        scale,
        perspective,
    }
}

#[allow(clippy::too_many_arguments, reason = "the orbit camera's values")]
fn orbit_screen(
    laid: &[Laid<'_>],
    w: usize,
    h: usize,
    yaw: f32,
    pitch: f32,
    distance: f32,
    pan: [f32; 2],
    perspective: bool,
) -> (Points, Points) {
    let basis = orbit_basis(laid, w, h, yaw, pitch, distance, pan, perspective);
    let mut world = Vec::new();
    let mut screen = Vec::new();
    for (parts, off) in laid {
        for p in parts {
            let pos: Vec<[f32; 3]> = p
                .positions
                .iter()
                .map(|v| std::array::from_fn(|a| v[a] + off[a]))
                .collect();
            screen.push(pos.iter().map(|v| basis.apply(*v)).collect());
            world.push(pos);
        }
    }
    (world, screen)
}

/// Each drawn part's vertices (the world's or the screen's).
type Points = Vec<Vec<[f32; 3]>>;

/// The light the preview shades with (the mesh viewer's), for the help text.
#[must_use]
pub fn light_direction() -> [f64; 3] {
    light()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn cube() -> Part {
        let positions = (0..8)
            .map(|i| std::array::from_fn(|a| if i >> a & 1 == 0 { -0.5 } else { 0.5 }))
            .collect();
        let quads = [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ];
        let indices = quads
            .iter()
            .flat_map(|q| [[q[0], q[1], q[2]], [q[0], q[2], q[3]]])
            .collect();
        Part {
            name: "lod0|box_Wood".into(),
            surface: None,
            shader: None,
            positions,
            normals: Some(vec![[0.0, 1.0, 0.0]; 8]),
            uvs: None,
            indices,
            texture: None,
            image: None,
        }
    }

    fn preview() -> Preview {
        let mut p = Preview::default();
        p.models.push(Model {
            subject: Subject::Mesh("a/box_mesh".into()),
            mesh: "a/box_mesh".into(),
            lods: vec![0],
            read: Mesh {
                name: "a/box_mesh".into(),
                lod: 0,
                parts: vec![cube()],
                notes: vec![],
            },
            components: vec![],
            materials: vec![],
        });
        p
    }

    #[test]
    fn draws_presets_and_orbit_with_overlays_and_picks() {
        let mut p = preview();
        p.camera = Camera::Preset(View::Side);
        let (img, owner) = p.draw(200, 150, Colours::default());
        assert_eq!((img.w, img.h), (200, 200));
        assert_eq!(Preview::part_at(owner[100 * 200 + 100]), Some((0, 0)));
        assert_eq!(Preview::part_at(owner[5 * 200 + 5]), None);
        p.camera = Camera::orbit();
        p.bounds = true;
        p.normals = true;
        p.wireframe = true;
        p.selected = Some((0, 0));
        let (img, owner) = p.draw(160, 120, Colours::default());
        assert_eq!((img.w, img.h), (160, 120));
        assert!(owner.iter().any(|&o| o != u32::MAX));
        assert!(
            img.rgb.as_chunks::<3>().0.contains(&[255, 140, 0]),
            "the selection's edges"
        );
        // hidden: nothing drawn
        p.hidden.insert((0, "lod0|box_Wood".into()));
        let (_, owner) = p.draw(160, 120, Colours::default());
        assert!(owner.iter().all(|&o| o == u32::MAX));
        assert!(p.bounds_box().is_none());
    }

    #[test]
    fn a_destroyed_state_hides_the_intact_parts() {
        let mut p = preview();
        p.models[0].components = vec![Component {
            name: "box".into(),
            states: vec![(100.0, "box".into()), (0.0, "box_dest".into())],
        }];
        assert!(p.part_drawn(0, &p.models[0].read.parts[0].clone()));
        p.state = State::Destroyed;
        assert!(!p.part_drawn(0, &p.models[0].read.parts[0].clone()));
    }

    #[test]
    fn the_picture_file_is_the_mesh_viewers() {
        let mut p = preview();
        p.camera = Camera::Preset(View::Top);
        let f = p.picture_file(Path::new("C:/o"));
        assert!(f.ends_with("work/tools_out/meshview/box_mesh_top.png"));
    }
}
