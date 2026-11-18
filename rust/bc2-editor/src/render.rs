//! The 3D view's renderer on wgpu: the scene's geometry on the GPU, drawn into an offscreen colour target with an
//! object-id target beside it (picking reads the id under the cursor), the selected object into a mask, and a
//! composite that outlines the selection in the `selection` colour. The result is a texture the window shows
//! through egui, and that the headless tests read back.
//!
//! Geometry: each blueprint's collision is uploaded once in its own space and drawn instanced, one instance per
//! placed entity (its transform's rows), as `bc2_scene` keeps it; the terrain and water are one mesh each; entities
//! without collision are marker boxes (sizes as the Map Inspector draws them), spawns with a facing line; outlines
//! (areas, routes) are lines. Colours: team-owned entities take their team's token colour; everything else a muted
//! scene colour ([`SceneColors`]).

use std::collections::HashMap;
use std::ops::Range;

use bc2_scene::scene::{Entity, Scene};
use eframe::egui::Color32;
use eframe::egui_wgpu::wgpu;
use wgpu::util::DeviceExt;

use crate::theme::Palette;

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
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
            .map_err(|e| format!("no graphics device: {e}"))?;
        Ok((Self { device, queue }, format!("{} ({:?})", info.name, info.backend)))
    }
}

/// The scene colours that are not tokens (terrain, water, objects by kind): muted, so the map's overlays and the
/// selection stand out.
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
    /// The colours with `palette`'s background (`ground`).
    #[must_use]
    pub fn new(palette: &Palette) -> Self {
        Self {
            background: palette.ground,
            terrain: Color32::from_rgb(0x6b, 0x78, 0x5c),
            water: Color32::from_rgb(0x2e, 0x5f, 0x96),
            object: Color32::from_rgb(0x9e, 0x99, 0x8f),
            destructible: Color32::from_rgb(0xb8, 0x9c, 0x7c),
            dynamic: Color32::from_rgb(0x9a, 0x80, 0x66),
            ladder: Color32::from_rgb(0xc8, 0xc0, 0x6a),
            group_member: Color32::from_rgb(0x8c, 0x88, 0x82),
            effect: Color32::from_rgb(0x8f, 0xb4, 0xcf),
            area_point: Color32::from_rgb(0xa8, 0xa8, 0xc8),
            other: Color32::from_rgb(0x90, 0x90, 0x90),
        }
    }
}

/// An entity's colour: its team's when it has one, else its kind's scene colour.
#[must_use]
pub fn entity_color(e: &Entity, palette: &Palette, colors: &SceneColors) -> Color32 {
    if let Some(t) = e.team {
        return palette.team(t);
    }
    match e.category.as_str() {
        "soldier spawn" | "vehicle spawn" | "capture point" | "objective" | "combat area" | "ai order" => {
            palette.team_neutral
        }
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

/// One instance: rows (right, up, forward, translation), colour, id.
#[derive(Debug, Clone, Copy)]
struct Instance {
    rows: [[f32; 4]; 4],
    color: [u8; 4],
    id: u32,
    /// The group (category index) it belongs to, for hiding.
    group: u16,
    mesh: u32,
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
}

fn rgba(c: Color32) -> [u8; 4] {
    [c.r(), c.g(), c.b(), 255]
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

/// A scene on the GPU: meshes uploaded once (the marker cube, terrain, water, one per blueprint), the instances and
/// lines built again by [`SceneGpu::refresh`] after an edit (cheap: no geometry is uploaded again).
#[derive(Debug)]
pub struct SceneGpu {
    meshes: Vec<Mesh>,
    /// Each blueprint's mesh and radius.
    blueprint_mesh: HashMap<String, (u32, f64)>,
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
            }],
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
            groups: Vec::new(),
            hidden: Vec::new(),
            bounds: None,
            triangles: 0,
            sizes: Vec::new(),
        };
        out.refresh(gpu, scene, palette, &|_| false);
        out
    }

    /// Builds the instances and lines again from the scene (after an edit: moved, added or deleted entities, a
    /// layer hidden), leaving out the entities `skip` says yes to; meshes are uploaded only for blueprints not seen
    /// before. Hidden groups and the selection stay.
    pub fn refresh(&mut self, gpu: &Gpu, scene: &Scene, palette: &Palette, skip: &dyn Fn(usize) -> bool) {
        let colors = SceneColors::new(palette);
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
            }
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
            });
        }
        if let Some(w) = &scene.water
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
            });
        }
        // collision: one mesh per blueprint, one instance per placed entity
        let mut sizes = vec![2.0f64; scene.entities.len()];
        let mut with_mesh = vec![false; scene.entities.len()];
        let by_blueprint = &mut self.blueprint_mesh;
        if let Some(col) = &scene.collision {
            for (index, bp) in &col.meshes {
                if skip(*index) {
                    continue;
                }
                let Some(e) = scene.entities.get(*index) else { continue };
                let Some(m) = e.matrix else { continue };
                let Some(Some(tris)) = col.blueprints.get(bp) else {
                    continue;
                };
                if tris.is_empty() {
                    continue;
                }
                let (mesh, radius) = *by_blueprint.entry(bp.clone()).or_insert_with(|| {
                    let flat: Vec<[f32; 3]> = tris.iter().flat_map(|t| t.iter().copied()).collect();
                    let r = flat
                        .iter()
                        .map(|p| f64::from(p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt())
                        .fold(0.0, f64::max);
                    meshes.push(Mesh {
                        vertices: buffer(gpu, "blueprint", &f32_bytes(&flat), wgpu::BufferUsages::VERTEX),
                        indices: None,
                        count: flat.len() as u32,
                    });
                    ((meshes.len() - 1) as u32, r)
                });
                triangles += tris.len() as u64;
                if let Some(s) = sizes.get_mut(*index) {
                    *s = radius * 2.0;
                }
                if let Some(w) = with_mesh.get_mut(*index) {
                    *w = true;
                }
                sample.push([m[12], m[13], m[14]]);
                instances.push(Instance {
                    rows: rows_of(&m),
                    color: rgba(entity_color(e, palette, &colors)),
                    id: entity_id(*index),
                    group: group_of(&e.category),
                    mesh,
                });
            }
        }
        // markers for the entities without collision, and spawns' facing
        for e in &scene.entities {
            if with_mesh.get(e.index).copied().unwrap_or(false) || skip(e.index) {
                continue;
            }
            let (Some(m), Some(size)) = (e.matrix, marker_size(&e.category)) else {
                continue;
            };
            let p = [m[12] as f32, m[13] as f32, m[14] as f32];
            let colour = rgba(entity_color(e, palette, &colors));
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
            if skip(s.entity) {
                continue;
            }
            let Some(e) = scene.entities.get(s.entity) else {
                continue;
            };
            let colour = rgba(entity_color(e, palette, &colors));
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
        if self.bounds.is_none() {
            self.bounds = percentile_bounds(&mut sample);
        }
        let n = self.groups.len();
        self.hidden.resize(n, false);
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
        order.sort_by_key(|i| i.mesh);
        let mut bytes = Vec::with_capacity(order.len() * INSTANCE_SIZE as usize);
        self.batches.clear();
        for (k, i) in order.iter().enumerate() {
            i.bytes(&mut bytes);
            let k = k as u32;
            match self.batches.last_mut() {
                Some((m, r)) if *m == i.mesh => r.end = k + 1,
                _ => self.batches.push((i.mesh, k..k + 1)),
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

    /// Marks entities as the selection (their instances and lines go to the mask); an empty slice for none.
    pub fn set_selection(&mut self, gpu: &Gpu, entities: &[usize]) {
        self.mask_batches.clear();
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
            self.mask_batches.push((i.mesh, k..k + 1));
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
        let mask_mesh = pipeline(
            "mask mesh",
            "vs_mesh",
            "fs_mask_mesh",
            &mesh_buffers,
            tri,
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
        for v in [0.35f32, 0.85, 0.40, 0.0] {
            g.extend_from_slice(&v.to_le_bytes());
        }
        gpu.queue.write_buffer(&self.globals, 0, &g);
        let sel = palette.selection;
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
            pass.set_pipeline(&self.mesh);
            pass.set_vertex_buffer(1, scene.instance_buf.slice(..));
            draw_batches(&mut pass, &scene.meshes, &scene.batches);
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
                draw_batches(&mut pass, &scene.meshes, &scene.mask_batches);
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
