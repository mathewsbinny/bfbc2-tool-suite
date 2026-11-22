//! `meshview.render` and `stack_models` ported (round GR3): a z-buffered picture of laid-out meshes, textured and lit
//! by the triangles' facing, drawn triangle by triangle with NumPy's arithmetic (float32 screen positions, float64
//! barycentric weights and depths, NumPy 2's promotion of Python numbers) so the side / top / front pictures are the
//! Python's pixel for pixel. [`Canvas::fill`] is the triangle loop itself; the map editor's asset preview draws its
//! orbiting camera with it too, so what it shows is what the picture command line draws.

#![allow(
    clippy::indexing_slicing,
    reason = "[f32; 3] vectors indexed by their axes (0..3, the view's axes a / b / d of 0..3)"
)]
#![allow(
    clippy::cast_precision_loss,
    reason = "pixel coordinates and picture sizes, far below 2^23, as NumPy converts them"
)]
#![allow(
    clippy::many_single_char_names,
    clippy::neg_cmp_op_on_partial_ord,
    reason = "render's loop as the Python writes it: x, y, z, w; `!(z < depth)` keeps NaN out as NumPy's mask does"
)]

use super::{Image, Part};

/// A view of `render`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum View {
    /// The models' long horizontal axis across, up up.
    #[default]
    Side,
    /// From above.
    Top,
    /// Along the long axis.
    Front,
}

impl View {
    /// Every view, in the command line's order.
    pub const ALL: [Self; 3] = [Self::Side, Self::Top, Self::Front];

    /// Its name (`--view`'s word).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Side => "side",
            Self::Top => "top",
            Self::Front => "front",
        }
    }

    /// The view a word names.
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|v| v.name() == s)
    }
}

/// A model laid out: the parts drawn and the offset added to their positions.
pub type Laid<'a> = (Vec<&'a Part>, [f32; 3]);

/// The light `render` shades with: `[0.4, 0.8, 0.45]` normalised (float64).
#[must_use]
pub fn light() -> [f64; 3] {
    let l = [0.4f64, 0.8, 0.45];
    let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt();
    l.map(|v| v / n)
}

/// `stack_models(models)`: each model's offset - centred on its own bounds, one above the other (y), the first at the
/// top. `None` for a model without positions.
#[must_use]
pub fn stack_models(models: &[&[Part]]) -> Vec<[f32; 3]> {
    let mut out = Vec::with_capacity(models.len());
    let mut y = 0.0f32;
    for parts in models {
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        for p in *parts {
            for v in &p.positions {
                for k in 0..3 {
                    lo[k] = lo[k].min(v[k]);
                    hi[k] = hi[k].max(v[k]);
                }
            }
        }
        let mut off: [f32; 3] = std::array::from_fn(|k| -(lo[k] + hi[k]) / 2.0);
        off[1] = y - hi[1];
        out.push(off);
        let size = (0..3).map(|k| hi[k] - lo[k]).fold(f32::NEG_INFINITY, f32::max);
        let gap = 0.02f64.max(0.15 * f64::from(size));
        y -= (hi[1] - lo[1]) + gap as f32;
    }
    out
}

/// `_long_axis(laid)`: 0 (x) when the models are at least as long along x as along z, else 2.
#[must_use]
pub fn long_axis(laid: &[Laid<'_>]) -> usize {
    let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
    for (parts, off) in laid {
        for p in parts {
            for v in &p.positions {
                for k in [0, 2] {
                    let x = v[k] + off[k];
                    lo[k] = lo[k].min(x);
                    hi[k] = hi[k].max(x);
                }
            }
        }
    }
    if hi[0] - lo[0] >= hi[2] - lo[2] { 0 } else { 2 }
}

/// A picture being drawn: its pixels and depths.
#[derive(Debug, Clone)]
pub struct Canvas {
    /// The picture.
    pub image: Image,
    /// The depth of each pixel (`inf` where nothing was drawn).
    pub depth: Vec<f64>,
    /// The part drawn at each pixel (`u32::MAX`: none) - the preview's pick.
    pub owner: Vec<u32>,
}

/// One part as [`Canvas::fill_parts`] draws it: the part, its vertices in the world and on screen, its look.
pub type PartJob<'a> = (&'a Part, &'a [[f32; 3]], &'a [[f32; 3]], Look<'a>);

/// How [`Canvas::fill`] colours a part.
#[derive(Debug, Clone, Copy)]
pub struct Look<'a> {
    /// The texture drawn (`None`: the flat colour).
    pub texture: Option<&'a Image>,
    /// The flat colour (`render`'s 170 grey).
    pub flat: [f32; 3],
    /// Shade by the triangles' facing (`render`: when the part has normals).
    pub shade: bool,
    /// The id written to [`Canvas::owner`].
    pub id: u32,
}

/// NumPy's `np.mod(x, 1.0)` (the sign of the divisor).
fn np_mod1(x: f64) -> f64 {
    let m = x % 1.0;
    if m != 0.0 && m < 0.0 { m + 1.0 } else { m }
}

impl Canvas {
    /// A `w` x `h` picture of one grey.
    #[must_use]
    pub fn new(w: usize, h: usize, background: [u8; 3]) -> Self {
        Self {
            image: Image {
                w,
                h,
                rgb: background.repeat(w * h),
            },
            depth: vec![f64::INFINITY; w * h],
            owner: vec![u32::MAX; w * h],
        }
    }

    /// `render`'s triangle loop for one part: its vertices on screen (`sx`, `sy` float32 pixels, `sz` the depth,
    /// smaller nearer) and in the world (`pos`, for the facing), drawn where nearer than what is there.
    pub fn fill(&mut self, part: &Part, pos: &[[f32; 3]], screen: &[[f32; 3]], look: &Look<'_>) {
        let (w, h) = (self.image.w as i64, self.image.h as i64);
        let mut band = Band {
            rgb: &mut self.image.rgb,
            depth: &mut self.depth,
            owner: &mut self.owner,
            rows: (0, h),
        };
        fill_band(&mut band, w, h, part, pos, screen, look);
    }

    /// [`Canvas::fill`] for each part in order, the picture drawn in horizontal bands at once on all cores (round
    /// PR1b-0, audit AU5 ALG-9: the asset preview's orbit). Each band draws every triangle in the same order and
    /// each pixel's arithmetic is [`Canvas::fill`]'s, so the picture, its depths and owners are the same, bit for
    /// bit.
    pub fn fill_parts(&mut self, parts: &[PartJob<'_>]) {
        use rayon::prelude::*;
        let (w, h) = (self.image.w as i64, self.image.h as i64);
        let wu = self.image.w.max(1);
        // about 32 rows a band, at most 64 bands
        let rows = (self.image.h.div_ceil(64)).max(32);
        let bands: Vec<Band<'_>> = self
            .image
            .rgb
            .chunks_mut(rows * wu * 3)
            .zip(self.depth.chunks_mut(rows * wu))
            .zip(self.owner.chunks_mut(rows * wu))
            .enumerate()
            .map(|(k, ((rgb, depth), owner))| {
                let y0 = (k * rows) as i64;
                let n = (depth.len() / wu) as i64;
                Band {
                    rgb,
                    depth,
                    owner,
                    rows: (y0, y0 + n),
                }
            })
            .collect();
        bands.into_par_iter().for_each(|mut band| {
            for (part, pos, screen, look) in parts {
                fill_band(&mut band, w, h, part, pos, screen, look);
            }
        });
    }

    /// A line from `a` to `b` (pixels), in front of what is drawn where `depth` says so (`None`: over everything).
    pub fn line(&mut self, a: [f32; 3], b: [f32; 3], colour: [u8; 3], depth_bias: Option<f64>) {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let steps = dx.abs().max(dy.abs()).ceil().max(1.0);
        if !steps.is_finite() || steps > 20_000.0 {
            return;
        }
        let n = steps as usize;
        for i in 0..=n {
            let t = i as f32 / steps;
            let (x, y) = (a[0] + dx * t, a[1] + dy * t);
            if x < 0.0 || y < 0.0 {
                continue;
            }
            let (x, y) = (x as usize, y as usize);
            if x >= self.image.w || y >= self.image.h {
                continue;
            }
            let k = y * self.image.w + x;
            if let Some(bias) = depth_bias {
                let z = f64::from(a[2] + (b[2] - a[2]) * t);
                if self.depth.get(k).is_some_and(|d| z - bias > *d) {
                    continue;
                }
            }
            if let Some(dst) = self.image.rgb.get_mut(k * 3..k * 3 + 3) {
                dst.copy_from_slice(&colour);
            }
        }
    }
}

/// Rows `rows.0 .. rows.1` of a canvas: its pixels, depths and owners from the first of those rows.
struct Band<'a> {
    rgb: &'a mut [u8],
    depth: &'a mut [f64],
    owner: &'a mut [u32],
    rows: (i64, i64),
}

/// [`Canvas::fill`]'s triangle loop on the rows of `band` (of a `w` x `h` canvas).
#[allow(
    clippy::too_many_lines,
    clippy::similar_names,
    clippy::many_single_char_names,
    reason = "render's loop as the Python writes it"
)]
fn fill_band(band: &mut Band<'_>, w: i64, h: i64, part: &Part, pos: &[[f32; 3]], screen: &[[f32; 3]], look: &Look<'_>) {
    let light = light();
    let uv = part.uvs.as_deref();
    for t in &part.indices {
        let [i0, i1, i2] = t.map(|i| i as usize);
        let (Some(a), Some(b), Some(c)) = (screen.get(i0), screen.get(i1), screen.get(i2)) else {
            continue;
        };
        let ([x0, y0, z0], [x1, y1, z1], [x2, y2, z2]) = (*a, *b, *c);
        let area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0);
        if area.abs() < 1e-9_f32 || area.is_nan() {
            continue;
        }
        // int(max(min(x0, x1, x2), 0)), int(min(max(x0, x1, x2), w - 1)) + 1: Python's min / max of the floats
        let lo_x = x0.min(x1).min(x2);
        let hi_x = x0.max(x1).max(x2);
        let lo_y = y0.min(y1).min(y2);
        let hi_y = y0.max(y1).max(y2);
        let clamp_lo = |v: f32| if 0.0 > v { 0 } else { v as i64 };
        let clamp_hi = |v: f32, m: i64| if (m as f32) < v { m } else { v as i64 };
        let (xa, xb) = (clamp_lo(lo_x), clamp_hi(hi_x, w - 1) + 1);
        let (ya, yb) = (clamp_lo(lo_y), clamp_hi(hi_y, h - 1) + 1);
        if xa >= xb || ya >= yb {
            continue;
        }
        // the band's rows of it
        let (ya, yb) = (ya.max(band.rows.0), yb.min(band.rows.1));
        if ya >= yb {
            continue;
        }
        let area64 = f64::from(area);
        let (fx0, fx1, fx2) = (f64::from(x0), f64::from(x1), f64::from(x2));
        let (fy0, fy1, fy2) = (f64::from(y0), f64::from(y1), f64::from(y2));
        let (fz0, fz1, fz2) = (f64::from(z0), f64::from(z1), f64::from(z2));
        // the facing's shade: computed once per triangle (the Python's, after its first pixel hit)
        let mut shade: Option<f32> = None;
        let shade_of = || -> f32 {
            if !look.shade {
                return 1.0;
            }
            let (Some(p0), Some(p1), Some(p2)) = (pos.get(i0), pos.get(i1), pos.get(i2)) else {
                return 1.0;
            };
            let e1 = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
            let e2 = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
            let n = [
                e1[1] * e2[2] - e1[2] * e2[1],
                e1[2] * e2[0] - e1[0] * e2[2],
                e1[0] * e2[1] - e1[1] * e2[0],
            ];
            let nl = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if nl > 0.0 {
                let d = f64::from(n[0]) * light[0] + f64::from(n[1]) * light[1] + f64::from(n[2]) * light[2];
                0.35f32 + 0.65f32 * ((d as f32) / nl).abs()
            } else {
                1.0
            }
        };
        for py in ya..yb {
            let gy = py as f64 + 0.5;
            for px in xa..xb {
                let gx = px as f64 + 0.5;
                let w0 = ((fx1 - gx) * (fy2 - gy) - (fx2 - gx) * (fy1 - gy)) / area64;
                let w1 = ((fx2 - gx) * (fy0 - gy) - (fx0 - gx) * (fy2 - gy)) / area64;
                let w2 = 1.0 - w0 - w1;
                if !(w0 >= 0.0 && w1 >= 0.0 && w2 >= 0.0) {
                    continue;
                }
                let z = w0 * fz0 + w1 * fz1 + w2 * fz2;
                let k = ((py - band.rows.0) * w + px) as usize;
                let Some(slot) = band.depth.get_mut(k) else { continue };
                if !(z < *slot) {
                    continue;
                }
                *slot = z;
                if let Some(o) = band.owner.get_mut(k) {
                    *o = look.id;
                }
                let s = *shade.get_or_insert_with(shade_of);
                let col: [f32; 3] = match (look.texture, uv) {
                    (Some(tex), Some(uv)) => {
                        let at = |i: usize| uv.get(i).copied().unwrap_or([0.0; 2]);
                        let (t0, t1, t2) = (at(i0), at(i1), at(i2));
                        let u = w0 * f64::from(t0[0]) + w1 * f64::from(t1[0]) + w2 * f64::from(t2[0]);
                        let v = w0 * f64::from(t0[1]) + w1 * f64::from(t1[1]) + w2 * f64::from(t2[1]);
                        let tx = (np_mod1(u) * (tex.w as f64 - 1.0)) as usize;
                        let ty = (np_mod1(v) * (tex.h as f64 - 1.0)) as usize;
                        tex.at(tx, ty).map(f32::from)
                    }
                    _ => look.flat,
                };
                let out = col.map(|c| (c * s).clamp(0.0, 255.0) as u8);
                if let Some(dst) = band.rgb.get_mut(k * 3..k * 3 + 3) {
                    dst.copy_from_slice(&out);
                }
            }
        }
    }
}

/// `render(laid, size, view)`: the picture `meshview.py --png` saves, on `render`'s grey (40).
#[must_use]
pub fn render(laid: &[Laid<'_>], size: usize, view: View) -> Image {
    render_on(laid, size, view, [40; 3])
}

/// The parts of laid-out models on screen for a view: the picture's size, each part's vertices in the world (its
/// model's offset added) and on screen (`render`'s float32 pixels and depth), in the laid order.
#[derive(Debug, Clone)]
pub struct Frame {
    /// Width.
    pub w: usize,
    /// Height (`render`: the models' height at the width's scale, 20 pixels above and below).
    pub h: usize,
    /// Each part's vertices in the world.
    pub world: Vec<Vec<[f32; 3]>>,
    /// Each part's vertices on screen.
    pub screen: Vec<Vec<[f32; 3]>>,
}

/// `render`'s framing of `laid` for `view` at `size` pixels wide; `None` when no triangle uses a vertex.
#[must_use]
pub fn frame(laid: &[Laid<'_>], size: usize, view: View) -> Option<Frame> {
    let la = long_axis(laid);
    let other = 2 - la;
    let (a, b, d, sgn) = match view {
        View::Side => (la, 1, other, 1.0f32),
        View::Top => (la, other, 1, -1.0),
        View::Front => (other, 1, la, -1.0),
    };
    let world: Vec<Vec<[f32; 3]>> = laid
        .iter()
        .flat_map(|(parts, off)| {
            parts.iter().map(move |p| {
                p.positions
                    .iter()
                    .map(|v| std::array::from_fn(|k| v[k] + off[k]))
                    .collect()
            })
        })
        .collect();
    let parts = laid.iter().flat_map(|(parts, _)| parts.iter());
    // the vertices triangles use
    let (mut lo, mut hi) = ([f32::INFINITY; 2], [f32::NEG_INFINITY; 2]);
    for (p, pos) in parts.zip(&world) {
        for t in &p.indices {
            for &i in t {
                if let Some(v) = pos.get(i as usize) {
                    lo = [lo[0].min(v[a]), lo[1].min(v[b])];
                    hi = [hi[0].max(v[a]), hi[1].max(v[b])];
                }
            }
        }
    }
    if !lo[0].is_finite() || !lo[1].is_finite() {
        return None;
    }
    let span = f64::from((hi[0] - lo[0]).max(hi[1] - lo[1]));
    let scale = (size as f64 - 40.0) / span.max(1e-6);
    let scale32 = scale as f32;
    let h = ((hi[1] - lo[1]) * scale32) as usize + 40;
    let hf = h as f32;
    let screen = world
        .iter()
        .map(|pos| {
            pos.iter()
                .map(|v| {
                    let sx = (v[a] - lo[0]) * scale32 + 20.0;
                    let sy = hf - ((v[b] - lo[1]) * scale32 + 20.0);
                    [sx, sy, -sgn * v[d]]
                })
                .collect()
        })
        .collect();
    Some(Frame {
        w: size,
        h,
        world,
        screen,
    })
}

/// [`render`] on another background.
#[must_use]
pub fn render_on(laid: &[Laid<'_>], size: usize, view: View, background: [u8; 3]) -> Image {
    let Some(f) = frame(laid, size, view) else {
        return Image {
            w: size,
            h: 40,
            rgb: background.repeat(size * 40),
        };
    };
    let mut canvas = Canvas::new(f.w, f.h, background);
    let parts = laid.iter().flat_map(|(parts, _)| parts.iter());
    for (k, ((p, pos), screen)) in parts.zip(&f.world).zip(&f.screen).enumerate() {
        let look = Look {
            texture: p.image.as_deref(),
            flat: [170.0; 3],
            shade: p.normals.is_some(),
            id: k as u32,
        };
        canvas.fill(p, pos, screen, &look);
    }
    canvas.image
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    fn quad() -> Part {
        Part {
            name: "q".into(),
            surface: None,
            shader: None,
            positions: vec![[0.0, 0.0, 0.0], [2.0, 0.0, 0.0], [2.0, 1.0, 0.0], [0.0, 1.0, 0.0]],
            normals: None,
            uvs: None,
            indices: vec![[0, 1, 2], [0, 2, 3]],
            texture: None,
            image: None,
        }
    }

    #[test]
    fn a_quad_side_on() {
        let q = [quad()];
        let off = stack_models(&[&q]);
        assert_eq!(off, vec![[-1.0, -1.0, 0.0]]);
        let img = render(&[(q.iter().collect(), off[0])], 120, View::Side);
        // 80 px for 2 m: 40 px high plus the margins
        assert_eq!((img.w, img.h), (120, 80));
        assert_eq!(img.at(60, 40), [170; 3]);
        assert_eq!(img.at(5, 5), [40; 3]);
    }

    #[test]
    fn mod_as_numpy() {
        assert!((np_mod1(-0.25) - 0.75).abs() < 1e-12);
        assert!((np_mod1(1.25) - 0.25).abs() < 1e-12);
        assert!(np_mod1(-1e-20) <= 1.0);
    }
}
