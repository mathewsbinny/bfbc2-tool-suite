//! The live draw list (round R5c, `EDITOR_ARCHITECTURE.md` 3.11 phase 3): a level's draws built once and kept, so a
//! frame only writes what the camera changes.
//!
//! [`crate::frame::draw_calls`] builds every draw's constant buffers from a value map each frame. Here each placed
//! instance's draws are planned once ([`Plan`]): the bytes [`crate::frame::cbuffer`] would write, with the values the
//! camera and the view's size decide ([`Dyn`]: `viewProjMatrix`, `viewMatrix`, `cameraPos`, `worldViewProjMatrix`,
//! `invScreenSize`, `g_screenSizeInv`, `g_depthScaleFactors`) left as holes filled per frame. The order is the
//! renderer's ([`crate::frame::draw_calls`]: opaque, alpha-tested, then blended farthest first, the given order among
//! equals), kept as indices; only the blended draws are sorted again per frame.
//!
//! **Culling** ([`Options::cull`]) leaves out the draws whose bounds lie wholly outside the view's frustum: the
//! rasterizer clips to the same planes (depth clipping on), so what is drawn is unchanged; bounds are the parts' own
//! positions (through their bones at rest, for bone-placed parts) with a margin, and a draw without bounds (strips,
//! water) is always drawn. Instances sit in a grid of [`CELL`]-metre cells (the spatial index) tested first. The
//! engine's depth pass ([`crate::water::DEPTH`]) runs where the first draw reading it is in the whole order, culled or
//! not, so the depth it copies is the same.
//!
//! Edits ([`Live::set`]) keep every instance whose key, mesh and transform are unchanged; a moved instance is planned
//! again, a new one planned, a removed one dropped.

use std::collections::HashMap;
use std::sync::Arc;

use crate::dxbc::Reflection;
use crate::frame::{Camera, Mat, mul, placed_bones};
use crate::material::{BLENDED, Draw, Prepared, identity_bones};
use crate::weather::Values;

/// A value the engine writes per frame (from the camera and the view's size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Dyn {
    /// `viewProjMatrix`.
    ViewProj,
    /// `viewMatrix`.
    View,
    /// `cameraPos`.
    CameraPos,
    /// `invScreenSize`.
    InvScreen,
    /// `g_screenSizeInv`.
    ScreenInv,
    /// `g_depthScaleFactors`.
    DepthFactors,
    /// `worldViewProjMatrix` (per instance).
    WorldViewProj,
}

impl Dyn {
    /// The engine's name of the value.
    #[must_use]
    pub fn of(name: &str) -> Option<Self> {
        Some(match name {
            "viewProjMatrix" => Self::ViewProj,
            "viewMatrix" => Self::View,
            "cameraPos" => Self::CameraPos,
            "invScreenSize" => Self::InvScreen,
            "g_screenSizeInv" => Self::ScreenInv,
            "g_depthScaleFactors" => Self::DepthFactors,
            "worldViewProjMatrix" => Self::WorldViewProj,
            _ => return None,
        })
    }

    /// How many floats the value has.
    #[must_use]
    pub fn len(self) -> usize {
        match self {
            Self::ViewProj | Self::View | Self::WorldViewProj => 16,
            _ => 4,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

/// A run of floats of a constant buffer that a per-frame value fills.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Hole {
    /// The first float (in the buffer).
    pub at: u32,
    /// The value.
    pub key: Dyn,
    /// Its first component written.
    pub from: u8,
    /// How many.
    pub count: u8,
}

/// A constant buffer's bytes with its per-frame values as holes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The bytes (the holes zero).
    pub bytes: Vec<u8>,
    /// The holes.
    pub holes: Vec<Hole>,
}

/// A material constant: stored, or a per-frame value by name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Item {
    /// Stored values.
    Const([f32; 4]),
    /// The frame's value (its first four components).
    Named(Dyn),
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Src {
    Const(f32),
    Dyn(Dyn, u8),
}

/// [`crate::frame::cbuffer`] as a plan: the same writes in the same order (engine values by name, the material by
/// register inside the shader's `constants`, then `extra`), the per-frame values as holes.
#[must_use]
pub fn plan(r: &Reflection, values: &Values, material: &[(u8, Item)], extra: &[(u8, [f32; 4])]) -> Plan {
    let mut buf = vec![Src::Const(0.0); r.globals_size() / 4];
    let constants = r
        .var("constants")
        .map(|v| (v.offset as usize / 16, (v.offset + v.size) as usize / 16));
    for v in &r.globals {
        let at = v.offset as usize / 4;
        let n = v.size as usize / 4;
        if let Some(k) = Dyn::of(&v.name) {
            for (i, d) in buf.iter_mut().skip(at).take(n.min(k.len())).enumerate() {
                *d = Src::Dyn(k, i as u8);
            }
        } else if let Some(x) = values.get(&v.name) {
            for (d, s) in buf.iter_mut().skip(at).take(n).zip(x) {
                *d = Src::Const(*s);
            }
        }
    }
    let mut put = |reg: u8, item: Item| {
        for (i, d) in buf.iter_mut().skip(usize::from(reg) * 4).take(4).enumerate() {
            *d = match item {
                Item::Const(v) => Src::Const(v.get(i).copied().unwrap_or(0.0)),
                Item::Named(k) => Src::Dyn(k, i as u8),
            };
        }
    };
    if let Some((lo, hi)) = constants {
        for (reg, item) in material {
            if (lo..hi).contains(&usize::from(*reg)) {
                put(*reg, *item);
            }
        }
    }
    for (reg, vals) in extra {
        put(*reg, Item::Const(*vals));
    }
    let mut out = Plan {
        bytes: Vec::with_capacity(buf.len() * 4),
        holes: Vec::new(),
    };
    for (i, s) in buf.iter().enumerate() {
        match *s {
            Src::Const(v) => out.bytes.extend(v.to_le_bytes()),
            Src::Dyn(k, c) => {
                out.bytes.extend(0f32.to_le_bytes());
                match out.holes.last_mut() {
                    Some(h) if h.key == k && h.at + u32::from(h.count) == i as u32 && h.from + h.count == c => {
                        h.count += 1;
                    }
                    _ => out.holes.push(Hole {
                        at: i as u32,
                        key: k,
                        from: c,
                        count: 1,
                    }),
                }
            }
        }
    }
    out
}

/// What an instance is.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Key {
    /// A placed object or group member (its identity: GUID, or group file and item).
    Object(String),
    /// A terrain patch.
    Patch(usize, usize),
    /// A road block's strips.
    Strip(usize),
    /// The visual water.
    Water,
}

/// A prepared mesh placed in the world, as the live list takes it.
#[derive(Debug, Clone)]
pub struct Source {
    /// The mesh.
    pub prepared: Arc<Prepared>,
    /// Its world matrix.
    pub world: Mat,
    /// What it is.
    pub key: Key,
    /// What picking it answers (0: not pickable).
    pub pick: u32,
}

/// An axis-aligned box in the world.
pub type Aabb = ([f64; 3], [f64; 3]);

/// One draw of an instance, planned.
#[derive(Debug, Clone)]
pub struct InstDraw {
    /// The vertex shader's constants.
    pub vs: Plan,
    /// The pixel shader's constants.
    pub ps: Plan,
    /// World bounds (`None`: always drawn).
    pub bounds: Option<Aabb>,
    /// A blended draw's centre in the world (its order).
    pub centre: Option<[f64; 3]>,
    /// The draw class.
    pub class: u8,
    /// It reads the scene's linear depth.
    pub reads_depth: bool,
}

/// An instance and its planned draws.
#[derive(Debug, Clone)]
pub struct Inst {
    /// The source it was made from.
    pub source: Source,
    /// Its draws (the prepared mesh's, in order).
    pub draws: Vec<InstDraw>,
    /// All its draws' bounds (`None`: one has none).
    pub bounds: Option<Aabb>,
}

/// The margin added to a part's bounds: a tenth of its size, at least a metre (vertex animation, wind).
fn margin(b: Aabb) -> Aabb {
    let d = ((b.1[0] - b.0[0]).powi(2) + (b.1[1] - b.0[1]).powi(2) + (b.1[2] - b.0[2]).powi(2)).sqrt();
    let m = (d * 0.1).max(1.0);
    (b.0.map(|v| v - m), b.1.map(|v| v + m))
}

/// A box through a 3 x 4 transform (`rows`: `[r0, r1, r2]`, each `[x, y, z, t]`).
fn transform_box(rows: &[[f64; 4]; 3], lo: [f32; 3], hi: [f32; 3]) -> Aabb {
    let (mut a, mut b) = ([f64::MAX; 3], [f64::MIN; 3]);
    for k in 0..8 {
        let p = [
            f64::from(if k & 1 == 0 { lo[0] } else { hi[0] }),
            f64::from(if k & 2 == 0 { lo[1] } else { hi[1] }),
            f64::from(if k & 4 == 0 { lo[2] } else { hi[2] }),
        ];
        for (r, row) in rows.iter().enumerate() {
            let v = row[0] * p[0] + row[1] * p[1] + row[2] * p[2] + row[3];
            a[r] = a[r].min(v);
            b[r] = b[r].max(v);
        }
    }
    (a, b)
}

fn union(a: Option<Aabb>, b: Aabb) -> Aabb {
    match a {
        None => b,
        Some(a) => (
            [a.0[0].min(b.0[0]), a.0[1].min(b.0[1]), a.0[2].min(b.0[2])],
            [a.1[0].max(b.1[0]), a.1[1].max(b.1[1]), a.1[2].max(b.1[2])],
        ),
    }
}

/// A draw's world bounds: its part's box through the world matrix, and for a part with bones through each bone
/// placed by it (bone-placed programs take the placement from the bones).
fn draw_bounds(d: &Draw, p: &Prepared, world: &Mat) -> Option<Aabb> {
    let (lo, hi) = d.bounds.or(if p.draws.len() == 1 { p.bounds } else { None })?;
    let w = |r: usize| [world[r * 4], world[r * 4 + 1], world[r * 4 + 2], world[r * 4 + 3]];
    let mut b = transform_box(&[w(0), w(1), w(2)], lo, hi);
    if let Some(rest) = &d.bones {
        for bone in placed_bones(rest, world).chunks_exact(3) {
            let r = |i: usize| bone.get(i).map_or([0.0; 4], |v| v.map(f64::from));
            b = union(Some(b), transform_box(&[r(0), r(1), r(2)], lo, hi));
        }
    }
    Some(margin(b))
}

/// The per-instance, per-draw values written beside the light (`renderqa.bone_values`, as
/// [`crate::frame::draw_calls`] writes them).
fn bone_rows(d: &Draw, world: &Mat) -> Vec<f32> {
    let rows: Vec<[f32; 4]> = if d.kind == 9 {
        vec![[1.0; 4]; 180]
    } else {
        let rest = d.bones.clone().unwrap_or_else(identity_bones);
        if matches!(d.kind, 1 | 4) && *world != crate::frame::IDENTITY {
            placed_bones(&rest, world)
        } else {
            rest
        }
    };
    rows.iter().flatten().copied().collect()
}

fn floats(m: &Mat) -> Vec<f32> {
    m.iter().map(|&v| v as f32).collect()
}

/// Plans an instance's draws with the static values (`base`: the light and `sunDir`).
#[must_use]
pub fn instance(source: Source, base: &Values) -> Inst {
    let mut vals = base.clone();
    vals.insert("worldMatrix".into(), floats(&source.world));
    let p = Arc::clone(&source.prepared);
    let w = source.world;
    let mut all: Option<Aabb> = None;
    let mut unbounded = false;
    let draws = p
        .draws
        .iter()
        .map(|d| {
            let bones = bone_rows(d, &w);
            vals.insert("boneMatrices".into(), bones.clone());
            vals.insert("boneVectors".into(), bones);
            let named: Vec<(u8, Item)> = d
                .named
                .iter()
                .filter_map(|(reg, n)| {
                    if let Some(k) = Dyn::of(n) {
                        return Some((*reg, Item::Named(k)));
                    }
                    let v = vals.get(n)?;
                    Some((*reg, Item::Const([0, 1, 2, 3].map(|k| v.get(k).copied().unwrap_or(0.0)))))
                })
                .collect();
            let material: Vec<(u8, Item)> = d
                .material
                .iter()
                .map(|(r, v)| (*r, Item::Const(*v)))
                .chain(named)
                .collect();
            let bounds = draw_bounds(d, &p, &w);
            match bounds {
                Some(b) => all = Some(union(all, b)),
                None => unbounded = true,
            }
            let centre = match (d.class, d.centre) {
                (BLENDED, Some(c)) => Some([0, 1, 2].map(|r| {
                    (0..3)
                        .map(|k| w.get(r * 4 + k).copied().unwrap_or(0.0) * f64::from(c.get(k).copied().unwrap_or(0.0)))
                        .sum::<f64>()
                        + w.get(r * 4 + 3).copied().unwrap_or(0.0)
                })),
                _ => None,
            };
            InstDraw {
                vs: plan(&d.vs_reflect, &vals, &material, &d.vs_extra),
                ps: plan(&d.ps_reflect, &vals, &material, &d.ps_extra),
                bounds,
                centre,
                class: d.class,
                reads_depth: d.textures.iter().any(|(_, n)| n == crate::water::DEPTH),
            }
        })
        .collect();
    Inst {
        source,
        draws,
        bounds: if unbounded { None } else { all },
    }
}

/// The light's values as every draw sees them (`sunDir` from `outdoorLightDir`).
#[must_use]
pub fn base_values(light: &Values) -> Values {
    let mut base = light.clone();
    if let Some(d) = light.get("outdoorLightDir") {
        base.insert("sunDir".into(), d.clone());
    }
    base
}

/// The size of a spatial-index cell, metres.
pub const CELL: f64 = 64.0;

/// How a frame is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Leave out the draws outside the view (exact: the picture is the same).
    pub cull: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { cull: true }
    }
}

/// One draw of a frame: the instance and draw, and where its constants are in [`Built::arena`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Step {
    /// The instance.
    pub inst: u32,
    /// Its draw.
    pub draw: u16,
    /// The vertex shader's constants: offset and length in the arena.
    pub vs: (u32, u32),
    /// The pixel shader's.
    pub ps: (u32, u32),
}

/// A frame's draws, built.
#[derive(Debug, Clone, Default)]
pub struct Built {
    /// The draws in order.
    pub steps: Vec<Step>,
    /// The engine's depth pass runs before this step (`None`: no draw reads the depth).
    pub depth_at: Option<usize>,
    /// The constant buffers, each at a 256-byte boundary.
    pub arena: Vec<u8>,
    /// Draws in the whole list.
    pub total: usize,
    /// Instances tested and left out.
    pub culled_instances: usize,
}

/// Constant buffers start at multiples of this (`VSSetConstantBuffers1`: 16 constants).
pub const ALIGN: usize = 256;

/// The six planes of a view-projection (`a x + b y + c z + d >= 0` inside), Direct3D's depth 0..1.
#[must_use]
pub fn planes(vp: &Mat) -> [[f64; 4]; 6] {
    let row = |r: usize| [vp[r * 4], vp[r * 4 + 1], vp[r * 4 + 2], vp[r * 4 + 3]];
    let (r0, r1, r2, r3) = (row(0), row(1), row(2), row(3));
    let add = |a: [f64; 4], b: [f64; 4], s: f64| [a[0] + s * b[0], a[1] + s * b[1], a[2] + s * b[2], a[3] + s * b[3]];
    [
        add(r3, r0, 1.0),
        add(r3, r0, -1.0),
        add(r3, r1, 1.0),
        add(r3, r1, -1.0),
        r2,
        add(r3, r2, -1.0),
    ]
}

/// Whether a box lies wholly outside one of the planes.
#[must_use]
pub fn outside(planes: &[[f64; 4]; 6], b: &Aabb) -> bool {
    planes.iter().any(|p| {
        let x = if p[0] >= 0.0 { b.1[0] } else { b.0[0] };
        let y = if p[1] >= 0.0 { b.1[1] } else { b.0[1] };
        let z = if p[2] >= 0.0 { b.1[2] } else { b.0[2] };
        p[0] * x + p[1] * y + p[2] * z + p[3] < 0.0
    })
}

/// The live draw list of a level.
#[derive(Debug, Default)]
pub struct Live {
    /// The instances, in the renderer's given order.
    pub instances: Vec<Inst>,
    base: Values,
    /// (instance, draw) by class, the given order among equals.
    order: Vec<(u32, u16)>,
    /// Where the blended draws start and end in `order`.
    blended: (usize, usize),
    /// The grid: cell -> (its bounds, instances); and the instances without bounds.
    grid: Vec<(Aabb, Vec<u32>)>,
    always: Vec<u32>,
    /// The last frame.
    pub built: Built,
    /// The last frame's camera.
    pub camera: Option<Camera>,
    visible: Vec<bool>,
}

impl Live {
    /// An empty list for a level's light.
    #[must_use]
    pub fn new(light: &Values) -> Self {
        Self {
            base: base_values(light),
            ..Self::default()
        }
    }

    /// Sets the instances: an unchanged one (key, mesh and transform) is kept as planned, the others planned. Returns
    /// how many were planned.
    pub fn set(&mut self, sources: Vec<Source>) -> usize {
        let mut old: HashMap<Key, Vec<Inst>> = HashMap::new();
        for i in std::mem::take(&mut self.instances).into_iter().rev() {
            old.entry(i.source.key.clone()).or_default().push(i);
        }
        let mut planned = 0;
        self.instances = sources
            .into_iter()
            .map(|s| {
                let kept = old.get_mut(&s.key).and_then(|v| {
                    let at = v.iter().rposition(|i| {
                        Arc::ptr_eq(&i.source.prepared, &s.prepared)
                            && i.source.world.iter().zip(&s.world).all(|(a, b)| a.to_bits() == b.to_bits())
                    })?;
                    Some(v.remove(at))
                });
                match kept {
                    Some(mut i) => {
                        i.source.pick = s.pick;
                        i
                    }
                    None => {
                        planned += 1;
                        instance(s, &self.base)
                    }
                }
            })
            .collect();
        self.index();
        planned
    }

    /// The order and the grid, made again after the instances changed.
    fn index(&mut self) {
        let mut order: Vec<(u8, u32, u16)> = Vec::new();
        for (i, inst) in self.instances.iter().enumerate() {
            for (k, d) in inst.draws.iter().enumerate() {
                order.push((d.class, i as u32, k as u16));
            }
        }
        // a stable sort keeps the given order among equals
        order.sort_by_key(|o| o.0);
        let b0 = order.iter().position(|o| o.0 == BLENDED).unwrap_or(order.len());
        let b1 = order.iter().rposition(|o| o.0 == BLENDED).map_or(b0, |p| p + 1);
        self.blended = (b0, b1);
        self.order = order.into_iter().map(|o| (o.1, o.2)).collect();
        let mut cells: HashMap<(i64, i64), (Aabb, Vec<u32>)> = HashMap::new();
        self.always.clear();
        for (i, inst) in self.instances.iter().enumerate() {
            match inst.bounds {
                Some(b) => {
                    let c = (
                        ((b.0[0] + b.1[0]) / 2.0 / CELL).floor() as i64,
                        ((b.0[2] + b.1[2]) / 2.0 / CELL).floor() as i64,
                    );
                    let e = cells.entry(c).or_insert((b, Vec::new()));
                    e.0 = union(Some(e.0), b);
                    e.1.push(i as u32);
                }
                None => self.always.push(i as u32),
            }
        }
        let mut grid: Vec<((i64, i64), (Aabb, Vec<u32>))> = cells.into_iter().collect();
        grid.sort_by_key(|g| g.0);
        self.grid = grid.into_iter().map(|g| g.1).collect();
    }

    /// Draws in the list.
    #[must_use]
    pub fn draw_count(&self) -> usize {
        self.order.len()
    }

    /// Builds the frame for a camera: the visible draws in order, their constants in the arena.
    pub fn build(&mut self, cam: &Camera, size: [u32; 2], opts: Options) -> &Built {
        self.camera = Some(*cam);
        let vp = cam.view_proj();
        let inv = [1.0 / size[0].max(1) as f32, 1.0 / size[1].max(1) as f32, 0.0, 0.0];
        let frame: [Vec<f32>; 6] = [
            floats(&vp),
            floats(&cam.view()),
            vec![cam.eye[0] as f32, cam.eye[1] as f32, cam.eye[2] as f32, 0.0],
            inv.to_vec(),
            inv.to_vec(),
            crate::water::depth_factors(cam.near, cam.far).to_vec(),
        ];
        // what is seen
        let n = self.instances.len();
        self.visible.clear();
        self.visible.resize(n, !opts.cull);
        let mut culled = 0;
        let pl = planes(&vp);
        if opts.cull {
            for &i in &self.always {
                if let Some(v) = self.visible.get_mut(i as usize) {
                    *v = true;
                }
            }
            for (cb, list) in &self.grid {
                if outside(&pl, cb) {
                    culled += list.len();
                    continue;
                }
                for &i in list {
                    let seen = self
                        .instances
                        .get(i as usize)
                        .and_then(|x| x.bounds)
                        .is_none_or(|b| !outside(&pl, &b));
                    if seen {
                        if let Some(v) = self.visible.get_mut(i as usize) {
                            *v = true;
                        }
                    } else {
                        culled += 1;
                    }
                }
            }
        }
        // the blended draws, farthest first (the given order among equals)
        let (b0, b1) = self.blended;
        if let Some(seg) = self.order.get_mut(b0..b1) {
            let inst = &self.instances;
            let far = |o: &(u32, u16)| -> f64 {
                inst.get(o.0 as usize)
                    .and_then(|i| i.draws.get(usize::from(o.1)))
                    .and_then(|d| d.centre)
                    .map_or(0.0, |c| {
                        let e = [c[0] - cam.eye[0], c[1] - cam.eye[1], c[2] - cam.eye[2]];
                        (e[0] * e[0] + e[1] * e[1] + e[2] * e[2]).sqrt()
                    })
            };
            // the given order: the instance's position, then the draw's
            seg.sort_by_key(|o| (o.0, o.1));
            let mut keyed: Vec<(f64, (u32, u16))> = seg.iter().map(|o| (-far(o), *o)).collect();
            keyed.sort_by(|a, b| a.0.total_cmp(&b.0));
            for (s, k) in seg.iter_mut().zip(keyed) {
                *s = k.1;
            }
        }
        let built = &mut self.built;
        built.steps.clear();
        built.arena.clear();
        built.depth_at = None;
        built.total = self.order.len();
        built.culled_instances = culled;
        let mut wvp: HashMap<u32, Vec<f32>> = HashMap::new();
        for &(i, k) in &self.order {
            let Some(inst) = self.instances.get(i as usize) else { continue };
            let Some(d) = inst.draws.get(usize::from(k)) else { continue };
            if d.reads_depth && built.depth_at.is_none() {
                built.depth_at = Some(built.steps.len());
            }
            if !self.visible.get(i as usize).copied().unwrap_or(false)
                || (opts.cull && d.bounds.is_some_and(|b| outside(&pl, &b)))
            {
                continue;
            }
            let needs_wvp = d.vs.holes.iter().chain(&d.ps.holes).any(|h| h.key == Dyn::WorldViewProj);
            if needs_wvp && !wvp.contains_key(&i) {
                wvp.insert(i, floats(&mul(&vp, &inst.source.world)));
            }
            let mut put = |p: &Plan| -> (u32, u32) {
                let at = built.arena.len();
                built.arena.extend_from_slice(&p.bytes);
                for h in &p.holes {
                    let src: &[f32] = if h.key == Dyn::WorldViewProj {
                        wvp.get(&i).map_or(&[], Vec::as_slice)
                    } else {
                        frame.get(h.key.index()).map_or(&[], Vec::as_slice)
                    };
                    for c in 0..usize::from(h.count) {
                        let v = src.get(usize::from(h.from) + c).copied().unwrap_or(0.0);
                        let o = at + (h.at as usize + c) * 4;
                        if let Some(dst) = built.arena.get_mut(o..o + 4) {
                            dst.copy_from_slice(&v.to_le_bytes());
                        }
                    }
                }
                let len = p.bytes.len();
                built.arena.resize(at + len.div_ceil(ALIGN).max(1) * ALIGN, 0);
                (at as u32, len as u32)
            };
            let vs = put(&d.vs);
            let ps = put(&d.ps);
            built.steps.push(Step { inst: i, draw: k, vs, ps });
        }
        &self.built
    }

    /// The draws of the instances `pick` answers for (the selection), in the frame's order, with their constants
    /// from the last [`Live::build`].
    #[must_use]
    pub fn steps_of(&self, picks: &[u32]) -> Vec<Step> {
        self.built
            .steps
            .iter()
            .filter(|s| {
                self.instances
                    .get(s.inst as usize)
                    .is_some_and(|i| i.source.pick != 0 && picks.contains(&i.source.pick))
            })
            .copied()
            .collect()
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::unwrap_used,
    clippy::float_cmp,
    clippy::trivially_copy_pass_by_ref
)]
mod tests {
    use super::*;
    use crate::dxbc::Var;
    use crate::frame::cbuffer;

    #[test]
    fn a_plan_filled_gives_the_cbuffer() {
        let r = Reflection {
            globals: vec![
                Var {
                    name: "viewProjMatrix".into(),
                    offset: 0,
                    size: 64,
                },
                Var {
                    name: "cameraPos".into(),
                    offset: 64,
                    size: 12,
                },
                Var {
                    name: "light".into(),
                    offset: 80,
                    size: 16,
                },
                Var {
                    name: "constants".into(),
                    offset: 96,
                    size: 48,
                },
            ],
            resources: Vec::new(),
        };
        let mut v = Values::new();
        let vp: Vec<f32> = (0..16).map(|k| k as f32 + 0.5).collect();
        v.insert("viewProjMatrix".into(), vp.clone());
        v.insert("cameraPos".into(), vec![1.0, 2.0, 3.0, 4.0]);
        v.insert("light".into(), vec![9.0; 4]);
        let mat = [(6u8, [5.0; 4]), (7, [6.0; 4])];
        // register 8 named cameraPos: the frame's value over the stored one
        let named = [(8u8, [1.0, 2.0, 3.0, 4.0])];
        let all: Vec<(u8, [f32; 4])> = mat.iter().chain(&named).copied().collect();
        let want = cbuffer(&r, &v, &all, &[(7, [8.0; 4])]);
        let items: Vec<(u8, Item)> = mat
            .iter()
            .map(|(r, x)| (*r, Item::Const(*x)))
            .chain([(8u8, Item::Named(Dyn::CameraPos))])
            .collect();
        let p = plan(&r, &v, &items, &[(7, [8.0; 4])]);
        let mut got = p.bytes.clone();
        for h in &p.holes {
            let src = match h.key {
                Dyn::ViewProj => &vp,
                _ => &v["cameraPos"],
            };
            for c in 0..usize::from(h.count) {
                let o = (h.at as usize + c) * 4;
                got[o..o + 4].copy_from_slice(&src[usize::from(h.from) + c].to_le_bytes());
            }
        }
        assert_eq!(got, want);
        assert_eq!(p.holes.len(), 3, "{:?}", p.holes);
    }

    #[test]
    fn boxes_outside_the_frustum() {
        let cam = Camera::look([0.0, 0.0, -10.0], [0.0; 3], 1.0, 1.0, 100.0, 1.0);
        let pl = planes(&cam.view_proj());
        assert!(!outside(&pl, &([-1.0; 3], [1.0; 3])));
        assert!(outside(&pl, &([-1.0, -1.0, -30.0], [1.0, 1.0, -20.0]))); // behind
        assert!(outside(&pl, &([50.0, -1.0, -1.0], [60.0, 1.0, 1.0]))); // to the side
        assert!(outside(&pl, &([-1.0, -1.0, 200.0], [1.0, 1.0, 210.0]))); // past the far plane
    }
}
