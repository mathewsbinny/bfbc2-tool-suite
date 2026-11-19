//! A frame: the camera, the engine constants each shader is given (`renderqa.cbuffer`: engine values by name, the
//! material's constants by register inside the shader's own `constants` array) and the draw list in the renderer's
//! order (`renderqa.draw_order`: opaque, then alpha-tested, then blended back to front).
//!
//! Matrices are row by row for column vectors (`clip = M * p`), as the harness writes them into `$Globals`.

use std::sync::Arc;

use crate::dxbc::Reflection;
use crate::material::{BLENDED, Draw, Element, Prepared, RenderState, identity_bones};
use crate::weather::Values;

/// A 4 x 4 matrix, row by row.
pub type Mat = [f64; 16];

/// The identity.
pub const IDENTITY: Mat = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];

/// `a * b`.
#[must_use]
pub fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut out = [0.0; 16];
    for (k, o) in out.iter_mut().enumerate() {
        let (r, c) = (k / 4, k % 4);
        *o = (0..4)
            .map(|i| a.get(r * 4 + i).copied().unwrap_or(0.0) * b.get(i * 4 + c).copied().unwrap_or(0.0))
            .sum();
    }
    out
}

/// An entity's stored transform (rows right, up, forward, translation: a row vector times it) as a matrix for column
/// vectors (its transpose). Only the 4 x 3 part is the transform: the fourth value of each stored row is padding
/// (often DICE's uninitialised -431602080), so the bottom row is (0, 0, 0, 1).
#[must_use]
pub fn from_rows(m: &[f64; 16]) -> Mat {
    let mut out = [0.0; 16];
    for (k, o) in out.iter_mut().enumerate().take(12) {
        *o = m.get((k % 4) * 4 + k / 4).copied().unwrap_or(0.0);
    }
    out[15] = 1.0;
    out
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn norm(a: [f64; 3]) -> [f64; 3] {
    let l = dot(a, a).sqrt().max(1e-12);
    a.map(|v| v / l)
}

/// A perspective camera as the game's: view space x right, y up, z forward; Direct3D depth 0 at near, 1 at far.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    /// Where it is.
    pub eye: [f64; 3],
    /// Right.
    pub right: [f64; 3],
    /// Up.
    pub up: [f64; 3],
    /// Forward (where it looks).
    pub forward: [f64; 3],
    /// Vertical field of view, radians.
    pub fov: f64,
    /// Width / height.
    pub aspect: f64,
    /// Near plane.
    pub near: f64,
    /// Far plane.
    pub far: f64,
}

impl Camera {
    /// Looking from `eye` at `target` (`renderqa.look`; right = forward x up).
    #[must_use]
    pub fn look(eye: [f64; 3], target: [f64; 3], fov: f64, near: f64, far: f64, aspect: f64) -> Self {
        let forward = norm(sub(target, eye));
        let right = norm(cross(forward, [0.0, 1.0, 0.0]));
        let up = cross(right, forward);
        Self {
            eye,
            right,
            up,
            forward,
            fov,
            aspect,
            near,
            far,
        }
    }

    /// The harness's orbit around bounds (`renderqa.camera`): azimuth 0 looks at the front (from +z), elevation up
    /// from the horizon; 45 degrees, near / far from the bounds' radius.
    #[must_use]
    pub fn orbit(lo: [f32; 3], hi: [f32; 3], azimuth: f64, elevation: f64) -> Self {
        let lo = lo.map(f64::from);
        let hi = hi.map(f64::from);
        let centre = [
            f64::midpoint(lo[0], hi[0]),
            f64::midpoint(lo[1], hi[1]),
            f64::midpoint(lo[2], hi[2]),
        ];
        let d = sub(hi, lo);
        let radius = (dot(d, d).sqrt() / 2.0).max(0.05);
        let (az, el) = (azimuth.to_radians(), elevation.to_radians());
        let dir = [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()];
        let eye = [
            centre[0] + radius * 2.4 * dir[0],
            centre[1] + radius * 2.4 * dir[1],
            centre[2] + radius * 2.4 * dir[2],
        ];
        Self::look(eye, centre, 45f64.to_radians(), radius * 0.05, radius * 10.0, 1.0)
    }

    /// The view matrix (rows right, up, forward; then the eye moved to the origin).
    #[must_use]
    pub fn view(&self) -> Mat {
        let (r, u, f, e) = (self.right, self.up, self.forward, self.eye);
        [
            r[0],
            r[1],
            r[2],
            -dot(r, e),
            u[0],
            u[1],
            u[2],
            -dot(u, e),
            f[0],
            f[1],
            f[2],
            -dot(f, e),
            0.0,
            0.0,
            0.0,
            1.0,
        ]
    }

    /// The projection (`renderqa.look`, with the aspect).
    #[must_use]
    pub fn projection(&self) -> Mat {
        let t = 1.0 / (self.fov / 2.0).tan();
        let (n, f) = (self.near, self.far);
        let mut p = [0.0; 16];
        p[0] = t / self.aspect.max(1e-6);
        p[5] = t;
        p[10] = f / (f - n);
        p[11] = -n * f / (f - n);
        p[14] = 1.0;
        p
    }

    /// Projection times view.
    #[must_use]
    pub fn view_proj(&self) -> Mat {
        mul(&self.projection(), &self.view())
    }
}

fn floats(m: &Mat) -> Vec<f32> {
    m.iter().map(|&v| v as f32).collect()
}

/// A shader's `$Globals` (`renderqa.cbuffer` / `strip_cbuffer`): engine values by name; the material's constants by
/// register, only inside the shader's own `constants` array; then `extra` at their registers (the engine-named block
/// constants of a terrain shader).
#[must_use]
pub fn cbuffer(r: &Reflection, values: &Values, material: &[(u8, [f32; 4])], extra: &[(u8, [f32; 4])]) -> Vec<u8> {
    let mut buf = vec![0f32; r.globals_size() / 4];
    let constants = r
        .var("constants")
        .map(|v| (v.offset as usize / 16, (v.offset + v.size) as usize / 16));
    for v in &r.globals {
        let Some(x) = values.get(&v.name) else { continue };
        let at = v.offset as usize / 4;
        for (d, s) in buf.iter_mut().skip(at).take(v.size as usize / 4).zip(x) {
            *d = *s;
        }
    }
    let mut put = |reg: u8, vals: &[f32; 4]| {
        for (d, s) in buf.iter_mut().skip(usize::from(reg) * 4).take(4).zip(vals) {
            *d = *s;
        }
    };
    if let Some((lo, hi)) = constants {
        for (reg, vals) in material {
            if (lo..hi).contains(&usize::from(*reg)) {
                put(*reg, vals);
            }
        }
    }
    for (reg, vals) in extra {
        put(*reg, vals);
    }
    buf.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// One draw call of a frame.
#[derive(Debug, Clone)]
pub struct DrawCall {
    /// Vertex shader code.
    pub vs: Arc<Vec<u8>>,
    /// Pixel shader code.
    pub ps: Arc<Vec<u8>>,
    /// Input layout (stream 0).
    pub elements: Vec<Element>,
    /// Vertex data.
    pub vertices: Arc<Vec<u8>>,
    /// Where the draw's vertices start.
    pub vertex_offset: u32,
    /// Bytes per vertex.
    pub stride: u32,
    /// Index data.
    pub indices: Arc<Vec<u8>>,
    /// 32-bit indices.
    pub wide: bool,
    /// Indices drawn.
    pub index_count: u32,
    /// First index.
    pub first_index: u32,
    /// The vertex shader's `$Globals`.
    pub cb_vs: Vec<u8>,
    /// The pixel shader's `$Globals`.
    pub cb_ps: Vec<u8>,
    /// Pixel shader samplers: (register, `D3D11_SAMPLER_DESC`).
    pub samplers: Vec<(u32, Vec<u8>)>,
    /// Vertex shader samplers (else the pixel shader's).
    pub vs_samplers: Vec<(u32, Vec<u8>)>,
    /// Pixel shader textures: (t slot, texture key).
    pub textures: Vec<(u32, String)>,
    /// Vertex shader textures (else the pixel shader's).
    pub vs_textures: Vec<(u32, String)>,
    /// The program's render state.
    pub state: Option<RenderState>,
    /// The draw class (the order; the coverage stencil).
    pub class: u8,
}

/// A frame: its size, clear colour and draw calls in order.
#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// The clear colour (HDR).
    pub clear: [f32; 4],
    /// The draws.
    pub draws: Vec<DrawCall>,
}

/// A prepared mesh placed in the world.
#[derive(Debug, Clone)]
pub struct Instance {
    /// The mesh.
    pub prepared: Arc<Prepared>,
    /// Its world matrix.
    pub world: Mat,
}

/// The engine's per-draw values: bones (`renderqa.bone_values`).
fn bone_values(d: &Draw, v: &mut Values) {
    let rows: Vec<[f32; 4]> = if d.kind == 9 {
        vec![[1.0; 4]; 180] // a destructible composite: boneMatrices[i].x > 0 shows part i
    } else {
        d.bones.clone().unwrap_or_else(identity_bones)
    };
    let flat: Vec<f32> = rows.iter().flatten().copied().collect();
    v.insert("boneMatrices".into(), flat.clone());
    v.insert("boneVectors".into(), flat);
}

/// The draw calls of placed meshes seen by a camera, in the renderer's order: opaque, alpha-tested, then blended
/// farthest first (by each part's centre), the given order among equals.
#[must_use]
pub fn draw_calls(instances: &[Instance], light: &Values, cam: &Camera, size: [u32; 2]) -> Vec<DrawCall> {
    let vp = cam.view_proj();
    let mut base = light.clone();
    base.insert("viewProjMatrix".into(), floats(&vp));
    base.insert("viewMatrix".into(), floats(&cam.view()));
    base.insert(
        "cameraPos".into(),
        vec![cam.eye[0] as f32, cam.eye[1] as f32, cam.eye[2] as f32, 0.0],
    );
    base.insert(
        "invScreenSize".into(),
        vec![1.0 / size[0].max(1) as f32, 1.0 / size[1].max(1) as f32, 0.0, 0.0],
    );
    let mut keyed = Vec::new();
    // one map for every draw: the per-instance and per-draw values are written over the last ones
    let mut vals = base;
    for inst in instances {
        vals.insert("worldMatrix".into(), floats(&inst.world));
        vals.insert("worldViewProjMatrix".into(), floats(&mul(&vp, &inst.world)));
        let p = &inst.prepared;
        for d in &p.draws {
            let far = match (d.class, d.centre) {
                (BLENDED, Some(c)) => {
                    let w = &inst.world;
                    let c = [0, 1, 2].map(|r| {
                        (0..3)
                            .map(|k| {
                                w.get(r * 4 + k).copied().unwrap_or(0.0) * f64::from(c.get(k).copied().unwrap_or(0.0))
                            })
                            .sum::<f64>()
                            + w.get(r * 4 + 3).copied().unwrap_or(0.0)
                    });
                    let e = sub(c, cam.eye);
                    dot(e, e).sqrt()
                }
                _ => 0.0,
            };
            bone_values(d, &mut vals);
            let call = DrawCall {
                vs: Arc::clone(&d.vs),
                ps: Arc::clone(&d.ps),
                elements: d.elements.clone(),
                vertices: Arc::clone(&p.vertices),
                vertex_offset: d.vertex_offset,
                stride: d.stride,
                indices: Arc::clone(&p.indices),
                wide: p.wide,
                index_count: d.index_count,
                first_index: d.first_index,
                cb_vs: cbuffer(&d.vs_reflect, &vals, &d.material, &d.vs_extra),
                cb_ps: cbuffer(&d.ps_reflect, &vals, &d.material, &d.ps_extra),
                samplers: d.samplers.clone(),
                vs_samplers: d.vs_samplers.clone(),
                textures: d.textures.clone(),
                vs_textures: d.vs_textures.clone(),
                state: d.state,
                class: d.class,
            };
            keyed.push(((d.class, -far), call));
        }
    }
    // a stable sort keeps the given order among equals
    keyed.sort_by(|a, b| a.0.0.cmp(&b.0.0).then(a.0.1.total_cmp(&b.0.1)));
    keyed.into_iter().map(|(_, c)| c).collect()
}

/// A frame drawn and read back.
#[derive(Debug, Clone, Default)]
pub struct Rendered {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// RGBA, 32-bit float, row by row.
    pub hdr: Vec<f32>,
    /// Depth (1: nothing drawn).
    pub depth: Vec<f32>,
    /// The draw class last to keep each pixel (0: nothing drawn).
    pub classes: Vec<u8>,
    /// Draw calls made.
    pub draws: usize,
    /// Seconds spent making the draw calls (objects made on first use included).
    pub submit: f64,
    /// Seconds spent waiting for the GPU and reading back.
    pub readback: f64,
}

impl Rendered {
    /// Where something was drawn.
    #[must_use]
    pub fn mask(&self) -> Vec<bool> {
        self.classes.iter().map(|&c| c > 0).collect()
    }
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;
    use crate::dxbc::Var;

    #[test]
    fn transforms_and_cameras() {
        let a = [
            2.0, 0.0, 0.0, 1.0, 0.0, 3.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        assert_eq!(mul(&IDENTITY, &a), a);
        // a stored transform's translation row becomes the column
        let mut rows = IDENTITY;
        rows[12] = 5.0;
        rows[3] = -431_602_080.0;
        rows[15] = -431_602_080.0;
        let w = from_rows(&rows);
        assert!((w[3] - 5.0).abs() < 1e-12);
        assert_eq!(&w[12..], &[0.0, 0.0, 0.0, 1.0]);
        let cam = Camera::look([0.0, 0.0, -10.0], [0.0; 3], 1.0, 1.0, 100.0, 1.0);
        let vp = cam.view_proj();
        // the target lands at the centre, at depth between 0 and 1
        let p = [0.0, 0.0, 0.0, 1.0];
        let clip: Vec<f64> = (0..4).map(|r| (0..4).map(|c| vp[r * 4 + c] * p[c]).sum()).collect();
        assert!(clip[0].abs() < 1e-9 && clip[1].abs() < 1e-9);
        let z = clip[2] / clip[3];
        assert!(z > 0.0 && z < 1.0);
        // right = forward x up: looking along +z, right is -x (the game's right-handed data)
        assert!((cam.right[0] + 1.0).abs() < 1e-12);
    }

    #[test]
    fn cbuffer_places_values_by_name_and_material_by_register() {
        let r = Reflection {
            globals: vec![
                Var {
                    name: "cameraPos".into(),
                    offset: 0,
                    size: 12,
                },
                Var {
                    name: "constants".into(),
                    offset: 32,
                    size: 32,
                },
            ],
            resources: Vec::new(),
        };
        let mut v = Values::new();
        v.insert("cameraPos".into(), vec![1.0, 2.0, 3.0, 9.0]);
        let b = cbuffer(&r, &v, &[(1, [7.0; 4]), (2, [5.0; 4]), (9, [4.0; 4])], &[(1, [6.0; 4])]);
        let f: Vec<f32> = b
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        assert_eq!(f.len(), 16);
        assert_eq!(&f[..4], &[1.0, 2.0, 3.0, 0.0]); // the size cuts the fourth
        assert_eq!(&f[4..8], &[6.0; 4]); // extra at its register
        assert_eq!(&f[8..12], &[5.0; 4]); // material inside constants
    }
}
