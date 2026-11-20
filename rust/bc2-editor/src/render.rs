//! The 3D view's renderer on wgpu: the scene's geometry on the GPU, drawn into an offscreen colour target with an
//! object-id target beside it (picking reads the id under the cursor), the selected object into a mask, and a
//! composite that outlines the selection in the `view-selection` colour. The result is a texture the window shows
//! through egui, and that the headless tests read back.
//!
//! Geometry: each blueprint's collision is uploaded once in its own space and drawn instanced, one instance per
//! placed entity (its transform's rows), as `bc2_scene` keeps it; the terrain and water are one mesh each; entities
//! without collision are marker boxes (sizes as the Map Inspector draws them), spawns with a facing line; outlines
//! (areas, routes) are lines. Colours, all tokens: team-owned entities take their team's `view-team-*` colour,
//! gameplay no team owns `view-unowned` (both lit evenly, as overlays), everything else a muted `scene-*` colour
//! ([`SceneColors`]).
//!
//! Round SV1: once the level's render meshes are read ([`SceneGpu::set_visuals`], `bc2_render::simple`: the meshes
//! the game-accurate view draws, as plain geometry), each object is drawn with its render mesh instead of its
//! collision, also instanced per blueprint: the alpha-tested parts (leaf cards, fences) cut by their diffuse
//! texture's alpha (a small copy in one atlas), the scene-lit objects tinted by their texture's mean colour (trees
//! green, rocks grey). Objects the game does not show (invisible walls, blueprints without a mesh) are their own
//! group, [`INVISIBLE`], hidden at the start. The collision view mode still draws collision; the surface queries
//! (snapping, placing, the ruler) use collision as before.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use bc2_render::simple::{Shape, Simple, SimpleMesh, State, part_shown};

use crate::display::{Display, InvisibleAs, ShowAs};
use bc2_scene::scene::{Entity, Scene, Source};
use eframe::egui::Color32;
use eframe::egui_wgpu::wgpu;
use wgpu::util::DeviceExt;

use crate::theme::Palette;
use crate::workspace::ViewMode;

/// The id buffer's value where nothing was drawn.
pub const ID_NONE: u32 = 0;
/// The terrain's id.
pub const ID_TERRAIN: u32 = u32::MAX - 1;
/// The water's id.
pub const ID_WATER: u32 = u32::MAX - 2;

/// The id an entity is drawn with.
#[must_use]
pub fn entity_id(index: usize) -> u32 {
    index as u32 + 1
}

/// The entity an id stands for.
#[must_use]
pub fn id_entity(id: u32) -> Option<usize> {
    (id != ID_NONE && id < ID_WATER).then(|| id as usize - 1)
}

/// The colour target's format (gamma-space values, as egui draws).
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const ID_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R32Uint;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MASK_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// The selection outline's width in pixels.
pub const OUTLINE_WIDTH: u32 = 2;

/// The group of the objects the game does not show (invisible walls, blueprints without a mesh): hidden at the
/// start, shown with the view menu's "Invisible objects".
pub const INVISIBLE: &str = "invisible";

/// The atlas's cells (texels a side): each part's texture at its level no larger ([`bc2_render::simple`]).
const CELL: u32 = bc2_render::simple::TEXTURE_WIDTH;
/// Cells in an atlas row.
const ATLAS_COLUMNS: u32 = 32;
/// A render-mesh vertex: position, texture coordinates, material.
const VISUAL_VERTEX: u64 = 24;
/// A render-mesh vertex's attributes (`view.wgsl`'s `vs_visual`).
const VISUAL_ATTRIBUTES: [wgpu::VertexAttribute; 3] =
    wgpu::vertex_attr_array![0 => Float32x3, 7 => Float32x2, 8 => Uint32];
/// A material: tint, atlas tile, alpha test (three `vec4<f32>`).
const MATERIAL_SIZE: usize = 48;

/// What the simplified view shows of an entity, from the level's render meshes: `None` when they do not say (not
/// read yet, or a blueprint placed later).
#[must_use]
pub fn shape_of(simple: &Simple, e: &Entity) -> Option<Shape> {
    match e.source {
        Source::GroupMember { item, .. } => simple
            .members
            .get(&format!("{}#{item}", e.group.as_deref()?.to_lowercase()))
            .copied(),
        Source::Instance { .. } => simple.blueprints.get(&e.blueprint.as_deref()?.to_lowercase()).copied(),
    }
}

/// Whether a shape is one the game does not show.
#[must_use]
pub fn is_invisible(simple: &Simple, shape: Option<Shape>) -> bool {
    match shape {
        Some(Shape::Nothing) => true,
        Some(Shape::Mesh(k)) => simple.meshes.get(k).is_some_and(|m| m.invisible()),
        None => false,
    }
}

/// The GPU the view draws on.
#[derive(Clone, Debug)]
pub struct Gpu {
    /// The device.
    pub device: wgpu::Device,
    /// Its queue.
    pub queue: wgpu::Queue,
}

impl Gpu {
    /// A device without a window (tests, screenshots): the system's adapter, else its software one.
    ///
    /// # Errors
    ///
    /// No adapter or device.
    pub fn headless() -> Result<(Self, String), String> {
        let instance = wgpu::Instance::default();
        let mut adapter = None;
        for fallback in [false, true] {
            if let Ok(a) = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: fallback,
                compatible_surface: None,
                ..Default::default()
            })) {
                adapter = Some(a);
                break;
            }
        }
        let adapter = adapter.ok_or("no graphics adapter")?;
        let info = adapter.get_info();
        // the game-accurate view's shared frame (round R5c), when the adapter can
        let desc = wgpu::DeviceDescriptor {
            required_features: crate::gameview::device_features(&adapter),
            ..Default::default()
        };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&desc)).map_err(|e| format!("no graphics device: {e}"))?;
        Ok((Self { device, queue }, format!("{} ({:?})", info.name, info.backend)))
    }
}

/// The scene's own colours (the `scene-*` tokens: sky, terrain, water, objects by kind): muted, so the map's
/// overlays (the `view-*` tokens) and the selection stand out.
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(missing_docs)]
pub struct SceneColors {
    pub background: Color32,
    pub terrain: Color32,
    pub water: Color32,
    pub object: Color32,
    pub destructible: Color32,
    pub dynamic: Color32,
    pub ladder: Color32,
    pub group_member: Color32,
    pub effect: Color32,
    pub area_point: Color32,
    pub other: Color32,
}

impl SceneColors {
    /// The `scene-*` colours of `palette`.
    #[must_use]
    pub fn new(palette: &Palette) -> Self {
        Self {
            background: palette.scene_sky,
            terrain: palette.scene_terrain,
            water: palette.scene_water,
            object: palette.scene_object,
            destructible: palette.scene_object_destructible,
            dynamic: palette.scene_object_dynamic,
            ladder: palette.scene_ladder,
            group_member: palette.scene_object_group,
            effect: palette.scene_effect,
            area_point: palette.scene_area_point,
            other: palette.scene_other,
        }
    }
}

/// The gameplay categories: drawn as overlays (a `view-*` colour, evenly lit) whether a team owns them or not.
pub const GAMEPLAY: [&str; 6] = [
    "soldier spawn",
    "vehicle spawn",
    "capture point",
    "objective",
    "combat area",
    "ai order",
];

/// Whether an entity is drawn as an overlay: a team owns it, or it is gameplay (`view.wgsl` lights overlays
/// evenly, so their colour reads on the terrain from every side).
#[must_use]
pub fn is_overlay(e: &Entity) -> bool {
    e.team.is_some() || GAMEPLAY.contains(&e.category.as_str())
}

/// An entity's colour: its team's (`view-team-*`) when it has one, `view-unowned` for gameplay no team owns, else
/// its kind's `scene-*` colour.
#[must_use]
pub fn entity_color(e: &Entity, palette: &Palette, colors: &SceneColors) -> Color32 {
    if let Some(t) = e.team {
        return palette.view_team(t);
    }
    match e.category.as_str() {
        c if GAMEPLAY.contains(&c) => palette.view_unowned,
        "destructible" => colors.destructible,
        "dynamic object" => colors.dynamic,
        "ladder" => colors.ladder,
        "group member" => colors.group_member,
        "effect" => colors.effect,
        "area point" => colors.area_point,
        "placed object" => colors.object,
        _ => colors.other,
    }
}

/// The marker box's size (metres) for an entity without collision, or `None` when it is not drawn without it
/// (the Map Inspector's `LAYERS`).
#[must_use]
pub fn marker_size(category: &str) -> Option<f32> {
    match category {
        "placed object" | "destructible" | "ladder" | "ai order" => Some(1.0),
        "dynamic object" | "effect" | "area point" | "other" => Some(0.6),
        "soldier spawn" => Some(1.2),
        "vehicle spawn" => Some(2.5),
        "objective" | "capture point" => Some(2.0),
        "combat area" => Some(1.5),
        _ => None,
    }
}

/// How the editor view draws the scene besides the theme (round E2b): its display mode, colours that replace some
/// entities' own (the cull-tree mode's slot colours), and extra lines (the navmesh outlines, the cull tree's spheres).
#[derive(Debug, Clone, Default)]
pub struct Look {
    /// The display mode.
    pub mode: ViewMode,
    /// Entity colours that replace their own.
    pub tint: HashMap<usize, Color32>,
    /// Extra lines in world space, each with its colour.
    pub overlay: Vec<([[f32; 3]; 2], Color32)>,
}

/// A colour of its own for each blueprint (the collision mode): a hue from the name's hash, muted.
#[must_use]
pub fn blueprint_colour(name: &str) -> Color32 {
    let mut h: u32 = 2_166_136_261;
    for b in name.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(16_777_619);
    }
    let hue = f32::from((h % 360) as u16) / 360.0;
    let c = eframe::egui::ecolor::Hsva::new(hue, 0.45, 0.78, 1.0);
    Color32::from(c)
}

/// The edges of a triangle list (three vertices a triangle), as a line list.
fn edges_of(tris: &[[f32; 3]]) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity(tris.len() * 2);
    for [a, b, c] in tris.as_chunks::<3>().0 {
        out.extend([*a, *b, *b, *c, *c, *a]);
    }
    out
}

/// The edges of an indexed mesh, as a line list.
fn edges_of_indexed(v: &[[f32; 3]], t: &[[u32; 3]]) -> Vec<[f32; 3]> {
    let at = |i: u32| v.get(i as usize).copied().unwrap_or_default();
    let mut out = Vec::with_capacity(t.len() * 6);
    for [a, b, c] in t {
        out.extend([at(*a), at(*b), at(*b), at(*c), at(*c), at(*a)]);
    }
    out
}

/// One instance: rows (right, up, forward, translation), colour, id.
#[derive(Debug, Clone, Copy)]
struct Instance {
    rows: [[f32; 4]; 4],
    color: [u8; 4],
    id: u32,
    /// The group (category index) it belongs to, for hiding.
    group: u16,
    mesh: u32,
    /// Drawn as its mesh's edges (round SV1: the collision over a mesh, wireframes, bounds).
    wire: bool,
}

const INSTANCE_SIZE: u64 = 72;
const LINE_SIZE: u64 = 20;

impl Instance {
    fn bytes(&self, out: &mut Vec<u8>) {
        for r in &self.rows {
            for v in r {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        out.extend_from_slice(&self.color);
        out.extend_from_slice(&self.id.to_le_bytes());
    }
}

/// One line vertex.
#[derive(Debug, Clone, Copy)]
struct LineVertex {
    p: [f32; 3],
    color: [u8; 4],
    id: u32,
    group: u16,
}

impl LineVertex {
    fn bytes(&self, out: &mut Vec<u8>) {
        for v in &self.p {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.color);
        out.extend_from_slice(&self.id.to_le_bytes());
    }
}

/// A mesh on the GPU: positions, and indices when it has them.
#[derive(Debug)]
struct Mesh {
    vertices: wgpu::Buffer,
    indices: Option<wgpu::Buffer>,
    count: u32,
    /// A render mesh (vertices of [`VISUAL_VERTEX`] bytes, drawn with the materials), else positions only.
    visual: bool,
}

fn rgba(c: Color32) -> [u8; 4] {
    [c.r(), c.g(), c.b(), 255]
}

/// An entity's instance colour: its colour, the alpha byte telling `view.wgsl` how to light it (255 the scene's
/// light, 0 an overlay's even light).
fn entity_rgba(e: &Entity, palette: &Palette, colors: &SceneColors) -> [u8; 4] {
    let c = entity_color(e, palette, colors);
    [c.r(), c.g(), c.b(), if is_overlay(e) { 0 } else { 255 }]
}

fn buffer(gpu: &Gpu, label: &str, data: &[u8], usage: wgpu::BufferUsages) -> wgpu::Buffer {
    // never empty: a zero-size buffer cannot be bound
    let mut padded;
    let contents = if data.len() < 16 {
        padded = data.to_vec();
        padded.resize(16, 0);
        padded.as_slice()
    } else {
        data
    };
    gpu.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some(label),
        contents,
        usage,
    })
}

fn f32_bytes(v: &[[f32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 12);
    for p in v {
        for c in p {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out
}

fn u32_bytes(v: &[[u32; 3]]) -> Vec<u8> {
    let mut out = Vec::with_capacity(v.len() * 12);
    for t in v {
        for c in t {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
    out
}

/// A unit cube's triangles, centred on the origin.
fn cube() -> Vec<[f32; 3]> {
    let h = 0.5f32;
    let c = |i: usize| -> [f32; 3] {
        [
            if i & 4 == 0 { -h } else { h },
            if i & 2 == 0 { -h } else { h },
            if i & 1 == 0 { -h } else { h },
        ]
    };
    let faces = [
        (0, 1, 3, 2),
        (4, 6, 7, 5),
        (0, 4, 5, 1),
        (2, 3, 7, 6),
        (0, 2, 6, 4),
        (1, 5, 7, 3),
    ];
    let mut out = Vec::with_capacity(36);
    for (a, b, cc, d) in faces {
        out.extend([c(a), c(b), c(cc), c(a), c(cc), c(d)]);
    }
    out
}

fn rows_of(m: &[f64; 16]) -> [[f32; 4]; 4] {
    std::array::from_fn(|r| std::array::from_fn(|c| m.get(r * 4 + c).copied().unwrap_or(0.0) as f32))
}

/// The indices of a render mesh's drawn parts ([`SimpleMesh::drawn_parts`]) in a destruction state of its parts.
fn drawn_indices(m: &SimpleMesh, parts: &[bc2_render::simple::Component], state: State) -> Vec<u32> {
    let mut out = Vec::new();
    for p in m.drawn_parts().filter(|p| part_shown(parts, &p.name, state)) {
        let r = p.first as usize..(p.first + p.count) as usize;
        out.extend_from_slice(m.indices.get(r).unwrap_or_default());
    }
    out
}

/// The box of the vertices some indices use.
fn box_of(v: &[[f32; 3]], idx: impl Iterator<Item = usize>) -> ([f32; 3], [f32; 3]) {
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    let mut any = false;
    for p in idx.filter_map(|i| v.get(i)) {
        any = true;
        for ((l, h), x) in lo.iter_mut().zip(hi.iter_mut()).zip(p) {
            *l = l.min(*x);
            *h = h.max(*x);
        }
    }
    if any { (lo, hi) } else { ([0.0; 3], [0.0; 3]) }
}

/// An instance's rows for a box `b` of its mesh's space: the unit cube scaled and moved onto it.
fn box_rows(rows: &[[f32; 4]; 4], b: Box3) -> [[f32; 4]; 4] {
    let [r0, r1, r2, r3] = *rows;
    let mut out = [r0, r1, r2, r3];
    let mut t = r3;
    for (((row, axis), lo), hi) in out.iter_mut().zip([r0, r1, r2]).zip(b.0).zip(b.1) {
        let size = (hi - lo).max(0.01);
        let centre = f32::midpoint(lo, hi);
        *row = axis.map(|v| v * size);
        for (tc, ac) in t.iter_mut().zip(axis).take(3) {
            *tc += ac * centre;
        }
    }
    let [a, b2, c, _] = out;
    [a, b2, c, t]
}

/// A render mesh on the GPU (position, texture coordinates and its part's material a vertex; the drawn parts'
/// indices), its radius and its box.
fn upload_visual(gpu: &Gpu, m: &SimpleMesh, materials: &[u32], idx: &[u32]) -> (Mesh, f64, ([f32; 3], [f32; 3])) {
    // each part's vertices are its own (`bc2_render::simple` adds them draw by draw): its material on them
    let mut material = vec![0u32; m.positions.len()];
    for (p, mat) in m.parts.iter().zip(materials) {
        for i in m
            .indices
            .get(p.first as usize..(p.first + p.count) as usize)
            .unwrap_or_default()
        {
            if let Some(slot) = material.get_mut(*i as usize) {
                *slot = *mat;
            }
        }
    }
    let mut v = Vec::with_capacity(m.positions.len() * VISUAL_VERTEX as usize);
    let mut radius = 0f64;
    for ((p, t), mat) in m.positions.iter().zip(&m.uvs).zip(&material) {
        for c in p.iter().chain(t) {
            v.extend_from_slice(&c.to_le_bytes());
        }
        v.extend_from_slice(&mat.to_le_bytes());
        radius = radius.max(f64::from(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt());
    }
    let mut ib = Vec::with_capacity(idx.len() * 4);
    for i in idx {
        ib.extend_from_slice(&i.to_le_bytes());
    }
    (
        Mesh {
            vertices: buffer(gpu, &m.mesh, &v, wgpu::BufferUsages::VERTEX),
            indices: Some(buffer(gpu, &m.mesh, &ib, wgpu::BufferUsages::INDEX)),
            count: idx.len() as u32,
            visual: true,
        },
        radius,
        box_of(&m.positions, idx.iter().map(|i| *i as usize)),
    )
}

/// The render meshes' bind group layout (group 1): the texture atlas and the materials.
fn visual_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("render-mesh materials"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Storage { read_only: true },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            },
        ],
    })
}

/// A scene on the GPU: meshes uploaded once (the marker cube, terrain, water, one per blueprint), the instances and
/// lines built again by [`SceneGpu::refresh`] after an edit (cheap: no geometry is uploaded again).
#[derive(Debug)]
pub struct SceneGpu {
    meshes: Vec<Mesh>,
    /// Each blueprint's mesh and radius.
    blueprint_mesh: HashMap<String, (u32, f64, Box3)>,
    terrain_mesh: Option<u32>,
    water_mesh: Option<u32>,
    /// The selected entities (outlined).
    selection: Vec<usize>,
    instances: Vec<Instance>,
    lines: Vec<LineVertex>,
    /// The visible instances, by mesh: (mesh, range in the instance buffer).
    batches: Vec<(u32, Range<u32>)>,
    instance_buf: wgpu::Buffer,
    line_buf: wgpu::Buffer,
    line_count: u32,
    /// The selection's instances and lines.
    mask_batches: Vec<(u32, Range<u32>)>,
    mask_instance_buf: Option<wgpu::Buffer>,
    mask_line_buf: Option<wgpu::Buffer>,
    mask_line_count: u32,
    /// The group names, by index (categories, then `terrain` and `water`).
    pub groups: Vec<String>,
    hidden: Vec<bool>,
    /// The bulk of the geometry: the 1-99 % range of the positions (`viewer.Scene.bounds`).
    pub bounds: Option<([f64; 3], [f64; 3])>,
    /// Triangles drawn when everything is shown.
    pub triangles: u64,
    /// The size of each entity's drawn shape (metres; for focusing).
    pub sizes: Vec<f64>,
    /// The display mode, colours and extra lines ([`SceneGpu::refresh`] reads it).
    pub look: Look,
    /// The wireframe mode's edge meshes, by the mesh they outline.
    edges: HashMap<u32, u32>,
    /// The instances drawn as edges, by edge mesh: (edge mesh, range in the instance buffer).
    wire_batches: Vec<(u32, Range<u32>)>,
    /// The selection's instances drawn as edges.
    mask_wire_batches: Vec<(u32, Range<u32>)>,
    /// The level's render meshes (round SV1), once read.
    visuals: Option<Arc<Simple>>,
    /// Each render mesh's GPU mesh, radius and box by (mesh, level of detail, destruction state), uploaded when first
    /// drawn.
    visual_mesh: HashMap<VisualKey, (u32, f64, Box3)>,
    /// Each (render mesh, level of detail)'s parts' materials.
    part_material: HashMap<(usize, usize), Vec<u32>>,
    /// The atlas and the materials, bound as group 1 of the render-mesh pipelines.
    visual_bg: Option<wgpu::BindGroup>,
    /// Draw with the render meshes (off: collision, as before round SV1; the before / after pictures).
    pub render_meshes: bool,
    /// How the objects are shown (round SV1: the view's, the objects', the blueprints', their variants).
    pub display: Display,
}

/// A render mesh as uploaded: (its place in the level's render meshes, level of detail, destruction state).
type VisualKey = (usize, usize, State);
/// A box in a mesh's space: its lowest and highest corners.
type Box3 = ([f32; 3], [f32; 3]);
/// An entity's collision mesh as drawn: (GPU mesh, radius, triangles, box, blueprint).
type CollisionMesh<'a> = (u32, f64, u64, Box3, &'a str);

/// What an entity is drawn with ([`SceneGpu::drawn`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Drawn {
    /// Its render mesh (else its collision, or a marker box).
    pub render_mesh: bool,
    /// As edges.
    pub wire: bool,
    /// The mesh's indices (a render mesh) or vertices (collision, a marker: three a triangle).
    pub count: u32,
}

impl SceneGpu {
    /// Uploads a scene.
    #[must_use]
    pub fn new(gpu: &Gpu, scene: &Scene, palette: &Palette) -> Self {
        let c = cube();
        let mut out = Self {
            meshes: vec![Mesh {
                vertices: buffer(gpu, "cube", &f32_bytes(&c), wgpu::BufferUsages::VERTEX),
                indices: None,
                count: c.len() as u32,
                visual: false,
            }],
            visuals: None,
            visual_mesh: HashMap::new(),
            part_material: HashMap::new(),
            visual_bg: None,
            render_meshes: true,
            display: Display::default(),
            wire_batches: Vec::new(),
            mask_wire_batches: Vec::new(),
            blueprint_mesh: HashMap::new(),
            terrain_mesh: None,
            water_mesh: None,
            selection: Vec::new(),
            instances: Vec::new(),
            lines: Vec::new(),
            batches: Vec::new(),
            instance_buf: buffer(gpu, "instances", &[], wgpu::BufferUsages::VERTEX),
            line_buf: buffer(gpu, "lines", &[], wgpu::BufferUsages::VERTEX),
            line_count: 0,
            mask_batches: Vec::new(),
            mask_instance_buf: None,
            mask_line_buf: None,
            mask_line_count: 0,
            // the objects the game does not show: hidden until asked for
            groups: vec![INVISIBLE.to_owned()],
            hidden: vec![true],
            bounds: None,
            triangles: 0,
            sizes: Vec::new(),
            look: Look::default(),
            edges: HashMap::new(),
        };
        out.refresh(gpu, scene, palette, &|_| false);
        out
    }

    /// Builds the instances and lines again from the scene (after an edit: moved, added or deleted entities, a
    /// layer hidden), leaving out the entities `skip` says yes to; meshes are uploaded only for blueprints not seen
    /// before. Hidden groups and the selection stay.
    pub fn refresh(&mut self, gpu: &Gpu, scene: &Scene, palette: &Palette, skip: &dyn Fn(usize) -> bool) {
        let colors = SceneColors::new(palette);
        let mode = self.look.mode;
        let tint = self.look.tint.clone();
        let tinted = |index: usize, c: [u8; 4]| tint.get(&index).map_or(c, |t| [t.r(), t.g(), t.b(), 0]);
        let collision_only = mode == ViewMode::Collision;
        let groups = &mut self.groups;
        let mut group_of = |name: &str| -> u16 {
            if let Some(k) = groups.iter().position(|g| g == name) {
                k as u16
            } else {
                groups.push(name.to_owned());
                (groups.len() - 1) as u16
            }
        };
        let meshes = &mut self.meshes;
        let mut instances = Vec::new();
        let mut lines = Vec::new();
        let mut triangles = 0u64;
        let mut sample: Vec<[f64; 3]> = Vec::new();
        let ident = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let indexed = |gpu: &Gpu, name: &str, v: &[[f32; 3]], t: &[[u32; 3]]| -> Mesh {
            Mesh {
                vertices: buffer(gpu, name, &f32_bytes(v), wgpu::BufferUsages::VERTEX),
                indices: Some(buffer(gpu, name, &u32_bytes(t), wgpu::BufferUsages::INDEX)),
                count: (t.len() * 3) as u32,
                visual: false,
            }
        };
        // the render meshes (round SV1): not in the collision mode
        let visuals = self.visuals.clone().filter(|_| !collision_only && self.render_meshes);
        let display = self.display.clone();
        let visual_mesh = &mut self.visual_mesh;
        let part_material = &self.part_material;
        // an entity's render mesh at its level of detail and destruction state: (GPU mesh, radius, triangles, box)
        let mut visual_of = |meshes: &mut Vec<Mesh>, k: usize, e: &Entity| -> Option<(u32, f64, u64, Box3)> {
            let read = visuals.as_ref()?;
            let lod0 = read.meshes.get(k)?;
            let lod = display.lod_of(e).min(lod0.lods.len());
            let parts = e.blueprint.as_deref().map_or(&[][..], |b| read.components_of(b));
            let state = if parts.is_empty() {
                State::All
            } else {
                display.state_of(e).state()
            };
            let m = lod0.at_lod(lod);
            let (mesh, radius, b) = *visual_mesh.entry((k, lod, state)).or_insert_with(|| {
                let idx = drawn_indices(m, parts, state);
                let mats = part_material.get(&(k, lod)).map_or(&[][..], Vec::as_slice);
                let (gm, r, b) = upload_visual(gpu, m, mats, &idx);
                meshes.push(gm);
                ((meshes.len() - 1) as u32, r, b)
            });
            let tris = meshes.get(mesh as usize).map_or(0, |g| u64::from(g.count / 3));
            Some((mesh, radius, tris, b))
        };
        if let Some((v, t)) = &scene.terrain
            && !t.is_empty()
        {
            let mesh = *self.terrain_mesh.get_or_insert_with(|| {
                meshes.push(indexed(gpu, "terrain", v, t));
                (meshes.len() - 1) as u32
            });
            triangles += t.len() as u64;
            let step = (v.len() / 4000).max(1);
            sample.extend(v.iter().step_by(step).map(|p| p.map(f64::from)));
            instances.push(Instance {
                rows: ident,
                color: rgba(colors.terrain),
                id: ID_TERRAIN,
                group: group_of("terrain"),
                mesh,
                wire: false,
            });
        }
        if let Some(w) = scene.water.as_ref().filter(|_| !collision_only)
            && !w.mesh.triangles.is_empty()
        {
            let mesh = *self.water_mesh.get_or_insert_with(|| {
                meshes.push(indexed(gpu, "water", &w.mesh.vertices, &w.mesh.triangles));
                (meshes.len() - 1) as u32
            });
            triangles += w.mesh.triangles.len() as u64;
            instances.push(Instance {
                rows: ident,
                color: rgba(colors.water),
                id: ID_WATER,
                group: group_of("water"),
                mesh,
                wire: false,
            });
        }
        // collision: one mesh per blueprint (uploaded once), each entity's blueprints
        let mut sizes = vec![2.0f64; scene.entities.len()];
        let mut with_mesh = vec![false; scene.entities.len()];
        let by_blueprint = &mut self.blueprint_mesh;
        let mut collision: HashMap<usize, Vec<CollisionMesh<'_>>> = HashMap::new();
        if let Some(col) = &scene.collision {
            for (index, bp) in &col.meshes {
                let Some(Some(tris)) = col.blueprints.get(bp) else {
                    continue;
                };
                if tris.is_empty() || skip(*index) {
                    continue;
                }
                let (mesh, radius, b) = *by_blueprint.entry(bp.clone()).or_insert_with(|| {
                    let flat: Vec<[f32; 3]> = tris.iter().flat_map(|t| t.iter().copied()).collect();
                    let r = flat
                        .iter()
                        .map(|p| f64::from(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
                        .fold(0.0, f64::max);
                    let b = box_of(&flat, 0..flat.len());
                    meshes.push(Mesh {
                        vertices: buffer(gpu, "blueprint", &f32_bytes(&flat), wgpu::BufferUsages::VERTEX),
                        indices: None,
                        count: flat.len() as u32,
                        visual: false,
                    });
                    ((meshes.len() - 1) as u32, r, b)
                });
                collision
                    .entry(*index)
                    .or_default()
                    .push((mesh, radius, tris.len() as u64, b, bp.as_str()));
            }
        }
        // each placed object as the display says (round SV1): its render mesh, its collision, both, edges, its box
        for e in &scene.entities {
            if skip(e.index) {
                continue;
            }
            let Some(m) = e.matrix else { continue };
            let cols = collision.get(&e.index).map_or(&[][..], Vec::as_slice);
            let shape = visuals.as_ref().and_then(|s| shape_of(s, e));
            let invisible = visuals.as_ref().is_some_and(|s| is_invisible(s, shape));
            let vis = match shape {
                // an invisible object's own mesh too: its outline when it has no collision
                Some(Shape::Mesh(k)) => visual_of(meshes, k, e),
                _ => None,
            };
            if cols.is_empty() && vis.is_none() {
                continue;
            }
            if let Some(w) = with_mesh.get_mut(e.index) {
                *w = true;
            }
            sample.push([m[12], m[13], m[14]]);
            let rows = rows_of(&m);
            let id = entity_id(e.index);
            let colour = if invisible {
                rgba(colors.other)
            } else {
                tinted(e.index, entity_rgba(e, palette, &colors))
            };
            let group = group_of(if invisible && !collision_only {
                INVISIBLE
            } else {
                &e.category
            });
            let show = if collision_only {
                ShowAs::Collision
            } else if invisible {
                match display.invisible {
                    InvisibleAs::Outline => ShowAs::Wireframe,
                    InvisibleAs::Hidden | InvisibleAs::Solid => ShowAs::Collision,
                }
            } else {
                display.show_of(e)
            };
            let mut size: Option<f64> = None;
            let mut put = |mesh: u32, rows: [[f32; 4]; 4], color: [u8; 4], wire: bool| {
                instances.push(Instance {
                    rows,
                    color,
                    id,
                    group,
                    mesh,
                    wire,
                });
            };
            // the collision mode's colour: each blueprint's own hue
            let hue = |bp: &str| {
                let c = blueprint_colour(bp);
                [c.r(), c.g(), c.b(), if collision_only { 255 } else { 0 }]
            };
            let solid_collision = |put: &mut dyn FnMut(u32, [[f32; 4]; 4], [u8; 4], bool), tris: &mut u64| {
                for (mesh, _, t, _, bp) in cols {
                    put(*mesh, rows, if collision_only { hue(bp) } else { colour }, false);
                    *tris += t;
                }
            };
            let mut tris = 0u64;
            match (show, vis) {
                (ShowAs::Hidden, _) => {}
                (ShowAs::Visual, Some((vm, r, t, _))) => {
                    put(vm, rows, colour, false);
                    tris += t;
                    size = Some(r * 2.0);
                }
                (ShowAs::Both, Some((vm, r, t, _))) => {
                    put(vm, rows, colour, false);
                    tris += t;
                    size = Some(r * 2.0);
                    for (mesh, _, _, _, bp) in cols {
                        put(*mesh, rows, hue(bp), true);
                    }
                }
                (ShowAs::Collision | ShowAs::Visual | ShowAs::Both, None) => {
                    solid_collision(&mut put, &mut tris);
                }
                (ShowAs::Collision, Some((vm, r, t, _))) => {
                    if cols.is_empty() {
                        // nothing to collide with: the mesh, so the object does not vanish
                        put(vm, rows, colour, false);
                        tris += t;
                        size = Some(r * 2.0);
                    } else {
                        solid_collision(&mut put, &mut tris);
                    }
                }
                (ShowAs::Wireframe, _) => match vis {
                    Some((vm, r, _, _)) => {
                        put(vm, rows, colour, true);
                        size = Some(r * 2.0);
                    }
                    None => {
                        for (mesh, _, _, _, _) in cols {
                            put(*mesh, rows, colour, true);
                        }
                    }
                },
                (ShowAs::Bounds, _) => {
                    let b = vis.map(|v| v.3).or_else(|| cols.first().map(|c| c.3));
                    if let Some(b) = b {
                        put(0, box_rows(&rows, b), colour, true);
                    }
                }
            }
            triangles += tris;
            if let Some(s) = sizes.get_mut(e.index) {
                *s = size.or_else(|| cols.first().map(|c| c.1 * 2.0)).unwrap_or(2.0);
            }
        }
        // markers for the entities without collision, and spawns' facing
        for e in &scene.entities {
            if collision_only || with_mesh.get(e.index).copied().unwrap_or(false) || skip(e.index) {
                continue;
            }
            let (Some(m), Some(size)) = (e.matrix, marker_size(&e.category)) else {
                continue;
            };
            let p = [m[12] as f32, m[13] as f32, m[14] as f32];
            let colour = tinted(e.index, entity_rgba(e, palette, &colors));
            let group = group_of(&e.category);
            let centre = [p[0], p[1] + size / 2.0, p[2]];
            instances.push(Instance {
                rows: [
                    [size, 0.0, 0.0, 0.0],
                    [0.0, size, 0.0, 0.0],
                    [0.0, 0.0, size, 0.0],
                    [centre[0], centre[1], centre[2], 1.0],
                ],
                color: colour,
                id: entity_id(e.index),
                group,
                mesh: 0,
                wire: false,
            });
            triangles += 12;
            if let Some(s) = sizes.get_mut(e.index) {
                *s = f64::from(size);
            }
            sample.push([m[12], m[13], m[14]]);
            if matches!(e.category.as_str(), "soldier spawn" | "vehicle spawn") {
                let f = [m[8] as f32, m[9] as f32, m[10] as f32];
                let tip = [
                    centre[0] + f[0] * size * 2.5,
                    centre[1] + f[1] * size * 2.5,
                    centre[2] + f[2] * size * 2.5,
                ];
                for q in [centre, tip] {
                    lines.push(LineVertex {
                        p: q,
                        color: colour,
                        id: entity_id(e.index),
                        group,
                    });
                }
            }
        }
        // outlines: areas (with their prism) and routes, in their owner's colour, a little above the ground
        for s in &scene.shapes {
            if collision_only || skip(s.entity) {
                continue;
            }
            let Some(e) = scene.entities.get(s.entity) else {
                continue;
            };
            let colour = entity_rgba(e, palette, &colors);
            let group = group_of(&e.category);
            let id = entity_id(s.entity);
            let pts: Vec<[f32; 3]> = s
                .points
                .iter()
                .map(|p| [p[0] as f32, p[1] as f32 + 0.05, p[2] as f32])
                .collect();
            let mut seg = |a: [f32; 3], b: [f32; 3]| {
                for p in [a, b] {
                    lines.push(LineVertex {
                        p,
                        color: colour,
                        id,
                        group,
                    });
                }
            };
            let lift = |p: [f32; 3]| [p[0], p[1] + s.height as f32, p[2]];
            for w in pts.windows(2) {
                if let [a, b] = w {
                    seg(*a, *b);
                    if s.height > 0.0 {
                        seg(lift(*a), lift(*b));
                    }
                }
            }
            if s.closed
                && pts.len() > 2
                && let (Some(a), Some(b)) = (pts.last(), pts.first())
            {
                seg(*a, *b);
                if s.height > 0.0 {
                    seg(lift(*a), lift(*b));
                }
            }
            if s.height > 0.0 {
                for p in &pts {
                    seg(*p, lift(*p));
                }
            }
        }
        // the mode's extra lines: navmesh outlines, cull-tree spheres
        if !self.look.overlay.is_empty() {
            let group = group_of("overlay");
            for (seg, c) in &self.look.overlay {
                for p in seg {
                    lines.push(LineVertex {
                        p: *p,
                        color: rgba(*c),
                        id: ID_NONE,
                        group,
                    });
                }
            }
        }
        // the edges drawn (the wireframe mode's every mesh, else the instances drawn as edges): an edge mesh for
        // each, made once
        let mut wanted: Vec<u32> = instances
            .iter()
            .filter(|i| i.wire || mode == ViewMode::Wireframe)
            .map(|i| i.mesh)
            .collect();
        wanted.sort_unstable();
        wanted.dedup();
        wanted.retain(|m| !self.edges.contains_key(m));
        if !wanted.is_empty() {
            let blueprint_of: HashMap<u32, &str> = self
                .blueprint_mesh
                .iter()
                .map(|(name, (m, _, _))| (*m, name.as_str()))
                .collect();
            let render_of: HashMap<u32, VisualKey> = self.visual_mesh.iter().map(|(k, (m, _, _))| (*m, *k)).collect();
            for m in wanted {
                if self.edges.contains_key(&m) {
                    continue;
                }
                let lines_of = if m == 0 {
                    edges_of(&cube())
                } else if Some(m) == self.terrain_mesh {
                    scene
                        .terrain
                        .as_ref()
                        .map_or_else(Vec::new, |(v, t)| edges_of_indexed(v, t))
                } else if Some(m) == self.water_mesh {
                    scene
                        .water
                        .as_ref()
                        .map_or_else(Vec::new, |w| edges_of_indexed(&w.mesh.vertices, &w.mesh.triangles))
                } else if let Some(&(k, lod, state)) = render_of.get(&m)
                    && let Some(read) = self.visuals.as_ref()
                    && let Some(lod0) = read.meshes.get(k)
                {
                    let m = lod0.at_lod(lod);
                    let parts = scene
                        .entities
                        .iter()
                        .find_map(|e| match shape_of(read, e) {
                            Some(Shape::Mesh(j)) if j == k => e.blueprint.as_deref(),
                            _ => None,
                        })
                        .map_or(&[][..], |b| read.components_of(b));
                    let tris: Vec<[u32; 3]> = drawn_indices(m, parts, state)
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|t| [t[0], t[1], t[2]])
                        .collect();
                    edges_of_indexed(&m.positions, &tris)
                } else {
                    blueprint_of
                        .get(&m)
                        .and_then(|bp| scene.collision.as_ref()?.blueprints.get(*bp)?.clone())
                        .map_or_else(Vec::new, |tris| {
                            let flat: Vec<[f32; 3]> = tris.iter().flat_map(|t| t.iter().copied()).collect();
                            edges_of(&flat)
                        })
                };
                self.meshes.push(Mesh {
                    vertices: buffer(gpu, "edges", &f32_bytes(&lines_of), wgpu::BufferUsages::VERTEX),
                    indices: None,
                    count: lines_of.len() as u32,
                    visual: false,
                });
                self.edges.insert(m, (self.meshes.len() - 1) as u32);
            }
        }
        if self.bounds.is_none() {
            self.bounds = percentile_bounds(&mut sample);
        }
        let n = self.groups.len();
        self.hidden.resize(n, false);
        // the objects the game does not show: hidden unless the display asks for them
        if let Some(k) = self.groups.iter().position(|g| g == INVISIBLE)
            && let Some(h) = self.hidden.get_mut(k)
        {
            *h = self.display.invisible == InvisibleAs::Hidden;
        }
        self.instances = instances;
        self.lines = lines;
        self.triangles = triangles;
        self.sizes = sizes;
        self.upload(gpu);
        let sel = std::mem::take(&mut self.selection);
        self.set_selection(gpu, &sel);
    }

    /// The instances and lines of the visible groups, to the GPU.
    fn upload(&mut self, gpu: &Gpu) {
        let hidden = &self.hidden;
        let visible = |g: u16| !hidden.get(usize::from(g)).copied().unwrap_or(false);
        let mut order: Vec<&Instance> = self.instances.iter().filter(|i| visible(i.group)).collect();
        // the solid ones first, the render meshes after the rest (one switch of pipeline), then the edges
        let meshes = &self.meshes;
        order.sort_by_key(|i| (i.wire, meshes.get(i.mesh as usize).is_some_and(|m| m.visual), i.mesh));
        let mut bytes = Vec::with_capacity(order.len() * INSTANCE_SIZE as usize);
        self.batches.clear();
        self.wire_batches.clear();
        for (k, i) in order.iter().enumerate() {
            i.bytes(&mut bytes);
            let k = k as u32;
            let list = if i.wire {
                &mut self.wire_batches
            } else {
                &mut self.batches
            };
            match list.last_mut() {
                Some((m, r)) if *m == i.mesh => r.end = k + 1,
                _ => list.push((i.mesh, k..k + 1)),
            }
        }
        self.instance_buf = buffer(gpu, "instances", &bytes, wgpu::BufferUsages::VERTEX);
        let mut lb = Vec::new();
        let mut n = 0u32;
        for v in self.lines.iter().filter(|v| visible(v.group)) {
            v.bytes(&mut lb);
            n += 1;
        }
        self.line_buf = buffer(gpu, "lines", &lb, wgpu::BufferUsages::VERTEX);
        self.line_count = n;
    }

    /// Shows or hides a group (a category, `terrain` or `water`).
    pub fn set_hidden(&mut self, gpu: &Gpu, group: &str, hide: bool) {
        if let Some(k) = self.groups.iter().position(|g| g == group)
            && let Some(h) = self.hidden.get_mut(k)
            && *h != hide
        {
            *h = hide;
            self.upload(gpu);
        }
    }

    /// Whether a group is hidden.
    #[must_use]
    pub fn is_hidden(&self, group: &str) -> bool {
        self.groups
            .iter()
            .position(|g| g == group)
            .and_then(|k| self.hidden.get(k).copied())
            .unwrap_or(false)
    }

    /// Takes the level's render meshes (round SV1): their textures into one atlas, their parts' materials; the
    /// meshes themselves are uploaded when first drawn. [`SceneGpu::refresh`] draws with them.
    pub fn set_visuals(&mut self, gpu: &Gpu, simple: Arc<Simple>) {
        let max_tiles = (ATLAS_COLUMNS * (8192 / CELL)) as usize;
        let mut tiles: HashMap<String, (u32, [u32; 2])> = HashMap::new();
        let mut pictures = Vec::new();
        let every_lod = || {
            simple.meshes.iter().enumerate().flat_map(|(k, m)| {
                std::iter::once(m.as_ref())
                    .chain(m.lods.iter().map(AsRef::as_ref))
                    .map(move |l| (k, l))
            })
        };
        for p in every_lod().flat_map(|(_, m)| &m.parts) {
            if let (Some(n), Some(t)) = (&p.texture_name, &p.texture)
                && !tiles.contains_key(n)
                && tiles.len() < max_tiles
            {
                tiles.insert(n.clone(), (tiles.len() as u32, t.size));
                pictures.push(Arc::clone(t));
            }
        }
        let n = pictures.len() as u32;
        let size = [
            (n.clamp(1, ATLAS_COLUMNS)) * CELL,
            n.div_ceil(ATLAS_COLUMNS).max(1) * CELL,
        ];
        let atlas = texture(
            gpu,
            "render-mesh textures",
            size,
            COLOR_FORMAT,
            wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        );
        for (k, p) in pictures.iter().enumerate() {
            let k = k as u32;
            let [w, h] = [p.size[0].min(CELL), p.size[1].min(CELL)];
            if p.rgba.len() < (p.size[0] * p.size[1] * 4) as usize || w == 0 || h == 0 {
                continue;
            }
            gpu.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &atlas,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: (k % ATLAS_COLUMNS) * CELL,
                        y: (k / ATLAS_COLUMNS) * CELL,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &p.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(p.size[0] * 4),
                    rows_per_image: None,
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        let mut materials: Vec<u8> = Vec::new();
        let mut part_material = HashMap::new();
        for (k, m) in every_lod() {
            let mut mats = Vec::with_capacity(m.parts.len());
            for p in &m.parts {
                mats.push((materials.len() / MATERIAL_SIZE) as u32);
                let tint = p.tint.map_or([0.0; 4], |c| {
                    [
                        f32::from(c[0]) / 255.0,
                        f32::from(c[1]) / 255.0,
                        f32::from(c[2]) / 255.0,
                        1.0,
                    ]
                });
                let tile = p
                    .texture_name
                    .as_ref()
                    .and_then(|n| tiles.get(n))
                    .map_or([0.0; 4], |(k, s)| {
                        [
                            ((k % ATLAS_COLUMNS) * CELL) as f32,
                            ((k / ATLAS_COLUMNS) * CELL) as f32,
                            s[0].min(CELL) as f32,
                            s[1].min(CELL) as f32,
                        ]
                    });
                // the alpha test: its reference where the part is tested and has its texture, else none (-1)
                let test = if p.tested && tile[2] > 0.0 {
                    f32::from(p.alpha_ref) / 255.0
                } else {
                    -1.0
                };
                for v in tint.iter().chain(&tile).chain(&[test, 0.0, 0.0, 0.0]) {
                    materials.extend_from_slice(&v.to_le_bytes());
                }
            }
            part_material.insert((k, m.lod), mats);
        }
        let materials = buffer(gpu, "materials", &materials, wgpu::BufferUsages::STORAGE);
        let view = atlas.create_view(&wgpu::TextureViewDescriptor::default());
        let layout = visual_layout(&gpu.device);
        self.visual_bg = Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("render-mesh materials"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: materials.as_entire_binding(),
                },
            ],
        }));
        self.part_material = part_material;
        self.visual_mesh.clear();
        self.edges.clear();
        self.visuals = Some(simple);
    }

    /// The level's render meshes, once read.
    #[must_use]
    pub fn visuals(&self) -> Option<&Arc<Simple>> {
        self.visuals.as_ref()
    }

    /// What an entity is drawn with now: its instances' meshes (empty when it is not drawn: hidden, or nothing to
    /// draw).
    #[must_use]
    pub fn drawn(&self, entity: usize) -> Vec<Drawn> {
        let id = entity_id(entity);
        let hidden = &self.hidden;
        self.instances
            .iter()
            .filter(|i| i.id == id && !hidden.get(usize::from(i.group)).copied().unwrap_or(false))
            .filter_map(|i| {
                let m = self.meshes.get(i.mesh as usize)?;
                Some(Drawn {
                    render_mesh: m.visual,
                    wire: i.wire,
                    count: m.count,
                })
            })
            .collect()
    }

    /// The levels of detail of an entity's render mesh (0: none read).
    #[must_use]
    pub fn lod_count(&self, e: &Entity) -> usize {
        match self.visuals.as_ref().and_then(|s| Some((s, shape_of(s, e)?))) {
            Some((s, Shape::Mesh(k))) => s.meshes.get(k).map_or(0, |m| m.lods.len() + 1),
            _ => 0,
        }
    }

    /// An entity's destructible parts (empty: it has none, or the render meshes are not read).
    #[must_use]
    pub fn components(&self, e: &Entity) -> &[bc2_render::simple::Component] {
        match (&self.visuals, e.blueprint.as_deref()) {
            (Some(s), Some(b)) => s.components_of(b),
            _ => &[],
        }
    }

    /// Marks entities as the selection (their instances and lines go to the mask); an empty slice for none.
    pub fn set_selection(&mut self, gpu: &Gpu, entities: &[usize]) {
        self.mask_batches.clear();
        self.mask_wire_batches.clear();
        self.mask_instance_buf = None;
        self.mask_line_buf = None;
        self.mask_line_count = 0;
        self.selection = entities.to_vec();
        if entities.is_empty() {
            return;
        }
        let ids: std::collections::HashSet<u32> = entities.iter().map(|&k| entity_id(k)).collect();
        let mut bytes = Vec::new();
        for (k, i) in self.instances.iter().filter(|i| ids.contains(&i.id)).enumerate() {
            i.bytes(&mut bytes);
            let k = k as u32;
            if i.wire {
                self.mask_wire_batches.push((i.mesh, k..k + 1));
            } else {
                self.mask_batches.push((i.mesh, k..k + 1));
            }
        }
        if !bytes.is_empty() {
            self.mask_instance_buf = Some(buffer(gpu, "selection", &bytes, wgpu::BufferUsages::VERTEX));
        }
        let mut lb = Vec::new();
        for v in self.lines.iter().filter(|v| ids.contains(&v.id)) {
            v.bytes(&mut lb);
            self.mask_line_count += 1;
        }
        if !lb.is_empty() {
            self.mask_line_buf = Some(buffer(gpu, "selection lines", &lb, wgpu::BufferUsages::VERTEX));
        }
    }
}

/// The 1-99 % range of the positions, per axis.
fn percentile_bounds(p: &mut [[f64; 3]]) -> Option<([f64; 3], [f64; 3])> {
    if p.is_empty() {
        return None;
    }
    let mut lo = [0.0; 3];
    let mut hi = [0.0; 3];
    for axis in 0..3 {
        let mut v: Vec<f64> = p.iter().map(|q| q.get(axis).copied().unwrap_or(0.0)).collect();
        v.sort_by(f64::total_cmp);
        let at = |f: f64| {
            v.get(((v.len() - 1) as f64 * f).round() as usize)
                .copied()
                .unwrap_or(0.0)
        };
        if let (Some(l), Some(h)) = (lo.get_mut(axis), hi.get_mut(axis)) {
            *l = at(0.01);
            *h = at(0.99);
        }
    }
    Some((lo, hi))
}

/// The view's targets: colour, ids, depth, the selection mask, and the composed result.
#[derive(Debug)]
pub struct Target {
    /// Width and height in pixels.
    pub size: [u32; 2],
    color: wgpu::TextureView,
    id: wgpu::Texture,
    id_view: wgpu::TextureView,
    depth: wgpu::TextureView,
    mask: wgpu::TextureView,
    /// The composed image (what the window shows).
    pub output: wgpu::Texture,
    /// Its view.
    pub output_view: wgpu::TextureView,
    composite: wgpu::BindGroup,
}

/// The pipelines and fixed buffers of the view.
#[derive(Debug)]
pub struct Renderer {
    globals: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    comp_uniform: wgpu::Buffer,
    comp_layout: wgpu::BindGroupLayout,
    mesh: wgpu::RenderPipeline,
    line: wgpu::RenderPipeline,
    mask_mesh: wgpu::RenderPipeline,
    mask_line: wgpu::RenderPipeline,
    composite: wgpu::RenderPipeline,
    /// The wireframe mode: meshes' edge lists placed by their instances.
    wire: wgpu::RenderPipeline,
    /// The render meshes (round SV1): alpha-tested, tinted.
    visual: wgpu::RenderPipeline,
    /// The render meshes into the selection mask (alpha-tested).
    mask_visual: wgpu::RenderPipeline,
    /// Edges into the selection mask (an object shown as edges or bounds).
    mask_wire: wgpu::RenderPipeline,
}

fn mesh_layouts() -> [wgpu::VertexBufferLayout<'static>; 2] {
    const POS: [wgpu::VertexAttribute; 1] = wgpu::vertex_attr_array![0 => Float32x3];
    const INST: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4, 5 => Unorm8x4, 6 => Uint32];
    [
        wgpu::VertexBufferLayout {
            array_stride: 12,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &POS,
        },
        wgpu::VertexBufferLayout {
            array_stride: INSTANCE_SIZE,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &INST,
        },
    ]
}

fn line_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTR: [wgpu::VertexAttribute; 3] = wgpu::vertex_attr_array![0 => Float32x3, 1 => Unorm8x4, 2 => Uint32];
    wgpu::VertexBufferLayout {
        array_stride: LINE_SIZE,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &ATTR,
    }
}

fn texture(
    gpu: &Gpu,
    label: &str,
    size: [u32; 2],
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

/// A read back of a texture region: rows of `width` texels of `bpp` bytes.
fn read_texture(gpu: &Gpu, tex: &wgpu::Texture, origin: [u32; 2], size: [u32; 2], bpp: u32) -> Option<Vec<u8>> {
    let row = size[0] * bpp;
    let padded = row.div_ceil(256) * 256;
    let buf = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: u64::from(padded) * u64::from(size[1]),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut enc = gpu.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("readback"),
    });
    enc.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: 0,
            origin: wgpu::Origin3d {
                x: origin[0],
                y: origin[1],
                z: 0,
            },
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buf,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded),
                rows_per_image: None,
            },
        },
        wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([enc.finish()]);
    let slice = buf.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r.is_ok());
    });
    let _ = gpu.device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: None,
    });
    if !rx.recv().unwrap_or(false) {
        return None;
    }
    let data = slice.get_mapped_range().ok()?;
    let mut out = Vec::with_capacity((row * size[1]) as usize);
    for y in 0..size[1] as usize {
        let start = y * padded as usize;
        out.extend_from_slice(data.get(start..start + row as usize)?);
    }
    drop(data);
    buf.unmap();
    Some(out)
}

impl Renderer {
    /// The pipelines.
    #[must_use]
    pub fn new(gpu: &Gpu) -> Self {
        let d = &gpu.device;
        let shader = d.create_shader_module(wgpu::include_wgsl!("view.wgsl"));
        let globals = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: 80,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let globals_bg = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            }],
        });
        let tex_entry = |binding: u32| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: false },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let comp_layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("composite"),
            entries: &[
                tex_entry(0),
                tex_entry(1),
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let comp_uniform = d.create_buffer(&wgpu::BufferDescriptor {
            label: Some("composite"),
            size: 32,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let scene_pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("scene"),
            bind_group_layouts: &[Some(&globals_layout)],
            immediate_size: 0,
        });
        let comp_pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("composite"),
            bind_group_layouts: &[Some(&comp_layout)],
            immediate_size: 0,
        });
        let depth = |write: bool| {
            Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(write),
                depth_compare: Some(wgpu::CompareFunction::GreaterEqual),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            })
        };
        let scene_targets = [
            Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
            Some(wgpu::ColorTargetState {
                format: ID_FORMAT,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            }),
        ];
        let mask_targets = [Some(wgpu::ColorTargetState {
            format: MASK_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let [ml0, ml1] = mesh_layouts();
        let mesh_buffers = [Some(ml0), Some(ml1)];
        let line_buffers = [Some(line_layout())];
        let pipeline = |label: &str,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout<'_>>],
                        topology: wgpu::PrimitiveTopology,
                        depth_stencil: Option<wgpu::DepthStencilState>,
                        targets: &[Option<wgpu::ColorTargetState>]| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&scene_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology,
                    ..Default::default()
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let tri = wgpu::PrimitiveTopology::TriangleList;
        let lin = wgpu::PrimitiveTopology::LineList;
        let mesh = pipeline(
            "mesh",
            "vs_mesh",
            "fs_mesh",
            &mesh_buffers,
            tri,
            depth(true),
            &scene_targets,
        );
        let line = pipeline(
            "line",
            "vs_line",
            "fs_line",
            &line_buffers,
            lin,
            depth(true),
            &scene_targets,
        );
        let wire = pipeline(
            "wire",
            "vs_mesh",
            "fs_wire",
            &mesh_buffers,
            lin,
            depth(true),
            &scene_targets,
        );
        let mask_mesh = pipeline(
            "mask mesh",
            "vs_mesh",
            "fs_mask_mesh",
            &mesh_buffers,
            tri,
            None,
            &mask_targets,
        );
        let mask_wire = pipeline(
            "mask wire",
            "vs_mesh",
            "fs_mask_mesh",
            &mesh_buffers,
            lin,
            None,
            &mask_targets,
        );
        let mask_line = pipeline(
            "mask line",
            "vs_line",
            "fs_mask_line",
            &line_buffers,
            lin,
            None,
            &mask_targets,
        );
        // the render meshes (round SV1): their vertices, the instances, and group 1's atlas and materials
        let vis_layout = visual_layout(d);
        let visual_pl = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("render meshes"),
            bind_group_layouts: &[Some(&globals_layout), Some(&vis_layout)],
            immediate_size: 0,
        });
        let [_, inst] = mesh_layouts();
        let visual_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: VISUAL_VERTEX,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &VISUAL_ATTRIBUTES,
            }),
            Some(inst),
        ];
        let vpipeline = |label: &str,
                         fs: &str,
                         depth_stencil: Option<wgpu::DepthStencilState>,
                         targets: &[Option<wgpu::ColorTargetState>]| {
            d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&visual_pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_visual"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &visual_buffers,
                },
                primitive: wgpu::PrimitiveState {
                    topology: tri,
                    ..Default::default()
                },
                depth_stencil,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let visual = vpipeline("render mesh", "fs_visual", depth(true), &scene_targets);
        let mask_visual = vpipeline("mask render mesh", "fs_mask_visual", None, &mask_targets);
        let composite = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("composite"),
            layout: Some(&comp_pl),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_full"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_composite"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        Self {
            globals,
            globals_bg,
            comp_uniform,
            comp_layout,
            mesh,
            line,
            mask_mesh,
            mask_line,
            composite,
            wire,
            visual,
            mask_visual,
            mask_wire,
        }
    }

    /// Targets of `size` pixels.
    #[must_use]
    pub fn target(&self, gpu: &Gpu, size: [u32; 2]) -> Target {
        use wgpu::TextureUsages as U;
        let size = [size[0].max(1), size[1].max(1)];
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        let color = texture(
            gpu,
            "view colour",
            size,
            COLOR_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        );
        let id = texture(gpu, "view ids", size, ID_FORMAT, U::RENDER_ATTACHMENT | U::COPY_SRC);
        let depth = texture(gpu, "view depth", size, DEPTH_FORMAT, U::RENDER_ATTACHMENT);
        let mask = texture(
            gpu,
            "selection mask",
            size,
            MASK_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING,
        );
        let output = texture(
            gpu,
            "view",
            size,
            COLOR_FORMAT,
            U::RENDER_ATTACHMENT | U::TEXTURE_BINDING | U::COPY_SRC,
        );
        let (color, mask) = (view(&color), view(&mask));
        let composite = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("composite"),
            layout: &self.comp_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&mask),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: self.comp_uniform.as_entire_binding(),
                },
            ],
        });
        Target {
            size,
            id_view: view(&id),
            id,
            depth: view(&depth),
            mask,
            output_view: view(&output),
            output,
            color,
            composite,
        }
    }

    /// Draws the scene with a view-projection matrix into the target.
    pub fn render(&self, gpu: &Gpu, scene: &SceneGpu, target: &Target, view_proj: &[[f32; 4]; 4], palette: &Palette) {
        let colors = SceneColors::new(palette);
        let mut g = Vec::with_capacity(80);
        for col in view_proj {
            for v in col {
                g.extend_from_slice(&v.to_le_bytes());
            }
        }
        // the light's fourth value: 1 draws unlit (flat colours)
        let unlit = if scene.look.mode == ViewMode::Unlit { 1.0 } else { 0.0 };
        for v in [0.35f32, 0.85, 0.40, unlit] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        gpu.queue.write_buffer(&self.globals, 0, &g);
        let sel = palette.view_selection;
        let mut c = Vec::with_capacity(32);
        for v in [
            f32::from(sel.r()) / 255.0,
            f32::from(sel.g()) / 255.0,
            f32::from(sel.b()) / 255.0,
            1.0,
            OUTLINE_WIDTH as f32,
            0.0,
            0.0,
            0.0,
        ] {
            c.extend_from_slice(&v.to_le_bytes());
        }
        gpu.queue.write_buffer(&self.comp_uniform, 0, &c);
        let bg = colors.background;
        let clear = wgpu::Color {
            r: f64::from(bg.r()) / 255.0,
            g: f64::from(bg.g()) / 255.0,
            b: f64::from(bg.b()) / 255.0,
            a: 1.0,
        };
        let mut enc = gpu
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("view") });
        let attach = attachment;
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[
                    attach(&target.color, clear),
                    attach(&target.id_view, wgpu::Color::TRANSPARENT),
                ],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            pass.set_vertex_buffer(1, scene.instance_buf.slice(..));
            let edges = |batches: &[(u32, Range<u32>)]| -> Vec<(u32, Range<u32>)> {
                batches
                    .iter()
                    .filter_map(|(m, r)| Some((*scene.edges.get(m)?, r.clone())))
                    .collect()
            };
            if scene.look.mode == ViewMode::Wireframe {
                pass.set_pipeline(&self.wire);
                draw_batches(&mut pass, &scene.meshes, &edges(&scene.batches));
            } else {
                pass.set_pipeline(&self.mesh);
                self.draw_mixed(&mut pass, scene, &scene.batches, &self.mesh, &self.visual);
            }
            // what is drawn as edges (round SV1: the collision over a mesh, wireframes, bounds)
            if !scene.wire_batches.is_empty() {
                pass.set_pipeline(&self.wire);
                draw_batches(&mut pass, &scene.meshes, &edges(&scene.wire_batches));
            }
            if scene.line_count > 0 {
                pass.set_pipeline(&self.line);
                pass.set_vertex_buffer(0, scene.line_buf.slice(..));
                pass.draw(0..scene.line_count, 0..1);
            }
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("selection mask"),
                color_attachments: &[attach(&target.mask, wgpu::Color::TRANSPARENT)],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.globals_bg, &[]);
            if let Some(b) = &scene.mask_instance_buf {
                pass.set_pipeline(&self.mask_mesh);
                pass.set_vertex_buffer(1, b.slice(..));
                self.draw_mixed(
                    &mut pass,
                    scene,
                    &scene.mask_batches,
                    &self.mask_mesh,
                    &self.mask_visual,
                );
                let edges: Vec<(u32, Range<u32>)> = scene
                    .mask_wire_batches
                    .iter()
                    .filter_map(|(m, r)| Some((*scene.edges.get(m)?, r.clone())))
                    .collect();
                if !edges.is_empty() {
                    pass.set_pipeline(&self.mask_wire);
                    draw_batches(&mut pass, &scene.meshes, &edges);
                }
            }
            if let Some(b) = &scene.mask_line_buf {
                pass.set_pipeline(&self.mask_line);
                pass.set_vertex_buffer(0, b.slice(..));
                pass.draw(0..scene.mask_line_count, 0..1);
            }
        }
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("composite"),
                color_attachments: &[attach(&target.output_view, clear)],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.composite);
            pass.set_bind_group(0, &target.composite, &[]);
            pass.draw(0..3, 0..1);
        }
        gpu.queue.submit([enc.finish()]);
    }

    /// Draws batches of collision, marker and render meshes, each with its pipeline (the render meshes with the
    /// scene's atlas and materials); the pass starts with `plain` set.
    #[allow(clippy::unused_self, reason = "one of the renderer's draw steps")]
    fn draw_mixed(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        scene: &SceneGpu,
        batches: &[(u32, Range<u32>)],
        plain: &wgpu::RenderPipeline,
        visual: &wgpu::RenderPipeline,
    ) {
        let mut on_visual = false;
        for (m, range) in batches {
            let Some(mesh) = scene.meshes.get(*m as usize) else {
                continue;
            };
            if mesh.visual != on_visual {
                if mesh.visual {
                    let Some(bg) = &scene.visual_bg else { continue };
                    pass.set_pipeline(visual);
                    pass.set_bind_group(1, bg, &[]);
                } else {
                    pass.set_pipeline(plain);
                }
                on_visual = mesh.visual;
            }
            draw_batches(pass, &scene.meshes, &[(*m, range.clone())]);
        }
    }

    /// The id drawn at a pixel: an entity's there, else the nearest entity's within `radius` pixels (thin lines and
    /// small markers), else what is there (terrain, water, nothing).
    #[must_use]
    pub fn pick(&self, gpu: &Gpu, target: &Target, px: [u32; 2], radius: u32) -> u32 {
        let [w, h] = target.size;
        if px[0] >= w || px[1] >= h {
            return ID_NONE;
        }
        let x0 = px[0].saturating_sub(radius);
        let y0 = px[1].saturating_sub(radius);
        let x1 = (px[0] + radius + 1).min(w);
        let y1 = (px[1] + radius + 1).min(h);
        let Some(data) = read_texture(gpu, &target.id, [x0, y0], [x1 - x0, y1 - y0], 4) else {
            return ID_NONE;
        };
        let ids: Vec<u32> = data.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect();
        let width = (x1 - x0) as usize;
        let at = |x: u32, y: u32| {
            ids.get((y - y0) as usize * width + (x - x0) as usize)
                .copied()
                .unwrap_or(ID_NONE)
        };
        let centre = at(px[0], px[1]);
        if id_entity(centre).is_some() {
            return centre;
        }
        let mut best: Option<(u32, u32)> = None;
        for y in y0..y1 {
            for x in x0..x1 {
                let id = at(x, y);
                if id_entity(id).is_some() {
                    let d = x.abs_diff(px[0]).pow(2) + y.abs_diff(px[1]).pow(2);
                    if best.is_none_or(|(bd, _)| d < bd) {
                        best = Some((d, id));
                    }
                }
            }
        }
        best.map_or(centre, |(_, id)| id)
    }

    /// The id drawn at every pixel, rows top to bottom (the tests: which pixels are which object's).
    #[must_use]
    pub fn read_ids(&self, gpu: &Gpu, target: &Target) -> Option<Vec<u32>> {
        let data = read_texture(gpu, &target.id, [0, 0], target.size, 4)?;
        Some(data.as_chunks::<4>().0.iter().map(|b| u32::from_le_bytes(*b)).collect())
    }

    /// The composed image, RGBA rows top to bottom.
    #[must_use]
    pub fn read_output(&self, gpu: &Gpu, target: &Target) -> Option<Vec<u8>> {
        read_texture(gpu, &target.output, [0, 0], target.size, 4)
    }
}

#[allow(clippy::unnecessary_wraps, reason = "the pass takes its attachments as Options")]
fn attachment(view: &wgpu::TextureView, clear: wgpu::Color) -> Option<wgpu::RenderPassColorAttachment<'_>> {
    Some(wgpu::RenderPassColorAttachment {
        view,
        depth_slice: None,
        resolve_target: None,
        ops: wgpu::Operations {
            load: wgpu::LoadOp::Clear(clear),
            store: wgpu::StoreOp::Store,
        },
    })
}

fn draw_batches(pass: &mut wgpu::RenderPass<'_>, meshes: &[Mesh], batches: &[(u32, Range<u32>)]) {
    for (m, range) in batches {
        let Some(mesh) = meshes.get(*m as usize) else { continue };
        pass.set_vertex_buffer(0, mesh.vertices.slice(..));
        if let Some(ib) = &mesh.indices {
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, range.clone());
        } else {
            pass.draw(0..mesh.count, range.clone());
        }
    }
}

/// A whole RGBA8 texture, read back as rows top to bottom.
#[must_use]
pub fn read_rgba(gpu: &Gpu, tex: &wgpu::Texture, size: [u32; 2]) -> Option<Vec<u8>> {
    read_texture(gpu, tex, [0, 0], size, 4)
}

/// Writes RGBA rows as a PNG file.
///
/// # Errors
///
/// The file cannot be written.
pub fn write_png(path: &std::path::Path, size: [u32; 2], rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size[0], size[1]);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut w = enc.write_header().map_err(|e| e.to_string())?;
    w.write_image_data(rgba).map_err(|e| e.to_string())?;
    w.finish().map_err(|e| e.to_string())
}
