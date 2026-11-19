//! A frame as a job for the suite's C++ render harness (`bfbc2-shader-tools/native/shaderview.cpp`): the same draws -
//! shaders, input layouts, buffers, `$Globals`, textures, sampler states, render states, draw classes - for
//! `shaderview` to draw, so that its picture can be held against [`crate::d3d::Device::render`]'s pixel for pixel.
//!
//! Every input goes into one pack file (`pack <file>`, each input a `*<offset>:<size>` slice). Texture resources go as
//! `texres` (uploaded by `shaderview` as the game stores them), the engine's own textures as `tex` / `vtex` (RGBA
//! float texels: `shaderview`'s raw format). Engine textures made on the GPU mid-frame (the scene's linear depth
//! the water reads) are given by the caller, read back from the device.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::Arc;

use crate::frame::{DrawCall, Frame};
use crate::material::TextureFiles;
use crate::terrain::RawTexture;

/// A float texture for `tex`: width, height, RGBA texels.
pub type FloatTexture = (u32, u32, Vec<f32>);

/// A job: its text and its pack.
#[derive(Debug, Clone, Default)]
pub struct Job {
    /// The job file's lines.
    pub text: String,
    /// The pack file's bytes.
    pub pack: Vec<u8>,
    /// Draws written.
    pub draws: usize,
    /// Draws left out (a texture the job cannot give), with why.
    pub skipped: Vec<String>,
}

struct Packer {
    pack: Vec<u8>,
    seen: HashMap<(usize, usize), String>,
    named: HashMap<String, String>,
}

impl Packer {
    fn add(&mut self, data: &[u8]) -> String {
        let at = self.pack.len();
        self.pack.extend_from_slice(data);
        format!("*{at}:{}", data.len())
    }

    fn shared(&mut self, data: &Arc<Vec<u8>>) -> String {
        let key = (Arc::as_ptr(data) as usize, data.len());
        if let Some(s) = self.seen.get(&key) {
            return s.clone();
        }
        let s = self.add(data);
        self.seen.insert(key, s.clone());
        s
    }

    fn by_name(&mut self, name: &str, make: impl FnOnce() -> Option<Vec<u8>>) -> Option<String> {
        if let Some(s) = self.named.get(name) {
            return Some(s.clone());
        }
        let s = self.add(&make()?);
        self.named.insert(name.to_owned(), s.clone());
        Some(s)
    }
}

/// `shaderview`'s raw float texture: `u32` width, height, faces (1), then RGBA `f32` texels.
fn float_file(w: u32, h: u32, rgba: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(12 + rgba.len() * 4);
    for v in [w, h, 1] {
        out.extend(v.to_le_bytes());
    }
    out.extend(rgba.iter().flat_map(|v| v.to_le_bytes()));
    out
}

/// An engine texture (one channel: `R32_FLOAT`) as RGBA floats, its value in every channel.
fn raw_rgba(r: &RawTexture) -> Option<Vec<f32>> {
    if r.format == 2 && r.texel == 16 {
        return Some(
            r.data
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| f32::from_le_bytes(*c))
                .collect(),
        );
    }
    if r.format != 41 || r.texel != 4 {
        return None;
    }
    Some(
        r.data
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|c| [f32::from_le_bytes(*c); 4])
            .collect(),
    )
}

/// The render-state lines of a draw, as the device sets them (`renderqa.state_lines`).
fn state_lines(d: &DrawCall, out: &mut String) {
    let (blend, depth, cull) = match &d.state {
        None => ([0, 0, 2, 1, 1, 2, 1, 1, 15], [1, 1, 2], 1),
        Some(st) => {
            let [(cs, cd, co), (a_s, ad, ao)] = st.factors();
            (
                [
                    u32::from(st.alpha_to_coverage),
                    u32::from(st.blend()),
                    cs,
                    cd,
                    co,
                    a_s,
                    ad,
                    ao,
                    15,
                ],
                [u32::from(st.depth_test), u32::from(st.depth_write), 4],
                if st.two_sided { 1 } else { 3 },
            )
        }
    };
    let b: Vec<String> = blend.iter().map(u32::to_string).collect();
    let _ = writeln!(out, "blendstate {}", b.join(" "));
    let _ = writeln!(out, "depthstate {} {} {}", depth[0], depth[1], depth[2]);
    let _ = writeln!(out, "raster {cull} 1");
    let _ = writeln!(out, "class {}", d.class);
}

fn hex(b: &[u8]) -> String {
    b.iter().fold(String::new(), |mut s, x| {
        let _ = write!(s, "{x:02x}");
        s
    })
}

/// A frame's draws as a `shaderview` job writing `out` (its colour, `.stencil` with the draw classes): `pack` is the
/// pack file's path as the job names it (forward slashes), `extra` the GPU-made engine textures by key. The post
/// passes are not part of it (`shaderview` draws `TEXCOORD` inputs only; `vs2D` reads `POSITION` and `COLOR`).
#[must_use]
pub fn write<S: std::hash::BuildHasher, S2: std::hash::BuildHasher>(
    frame: &Frame,
    textures: &TextureFiles,
    raw: &HashMap<String, RawTexture, S>,
    extra: &HashMap<String, FloatTexture, S2>,
    out: &str,
    pack: &str,
) -> Job {
    let mut p = Packer {
        pack: Vec::new(),
        seen: HashMap::new(),
        named: HashMap::new(),
    };
    let mut job = Job::default();
    let mut t = String::new();
    let _ = writeln!(t, "pack \"{pack}\"");
    let c = frame.clear;
    let _ = writeln!(
        t,
        "frame \"{out}\" {} {} {} {} {} {}",
        frame.width, frame.height, c[0], c[1], c[2], c[3]
    );
    let _ = writeln!(t, "stencil 1");
    'draws: for (k, d) in frame.draws.iter().enumerate() {
        let mut s = String::new();
        let vs = p.shared(&d.vs);
        let _ = writeln!(s, "vs {vs}");
        let ps = p.shared(&d.ps);
        let _ = writeln!(s, "ps {ps}");
        state_lines(d, &mut s);
        for e in &d.elements {
            let _ = writeln!(
                s,
                "el {} {} {} {} {}",
                e.semantic, e.format, e.slot, e.offset, e.instance
            );
        }
        let vb = p.shared(&d.vertices);
        let _ = writeln!(s, "vb {vb} {} {}", d.vertex_offset, d.stride);
        let ib = p.shared(&d.indices);
        let _ = writeln!(
            s,
            "ib {ib} 0 {} {} {}",
            if d.wide { 32 } else { 16 },
            d.index_count,
            d.first_index
        );
        if !d.cb_vs.is_empty() {
            let cb = p.add(&d.cb_vs);
            let _ = writeln!(s, "cb vs {cb}");
        }
        if !d.cb_ps.is_empty() {
            let cb = p.add(&d.cb_ps);
            let _ = writeln!(s, "cb ps {cb}");
        }
        for (reg, desc) in &d.samplers {
            let _ = writeln!(s, "samp {reg} {}", hex(desc));
        }
        for (reg, desc) in &d.vs_samplers {
            let _ = writeln!(s, "vsamp {reg} {}", hex(desc));
        }
        for (vs_stage, list) in [(false, &d.textures), (true, &d.vs_textures)] {
            for (slot, name) in list {
                let is_float =
                    extra.contains_key(name) || raw.get(name).is_some_and(|r| r.format == 41 || r.format == 2);
                let line = if is_float {
                    // made once per name (a height map is millions of texels)
                    let f = p.by_name(name, || {
                        let (w, h, px) = extra
                            .get(name)
                            .map(|(w, h, px)| (*w, *h, px.clone()))
                            .or_else(|| raw.get(name).and_then(|r| Some((r.width, r.height, raw_rgba(r)?))))?;
                        Some(float_file(w, h, &px))
                    });
                    f.map(|f| format!("{} {slot} 2d {f}", if vs_stage { "vtex" } else { "tex" }))
                } else if vs_stage {
                    None
                } else {
                    let f = p.by_name(name, || textures.get(name).cloned().flatten().map(|b| b.to_vec()));
                    f.map(|f| format!("texres {slot} {f}"))
                };
                let Some(l) = line else {
                    job.skipped
                        .push(format!("draw {k}: texture {name} not available to the job"));
                    continue 'draws;
                };
                let _ = writeln!(s, "{l}");
            }
        }
        let _ = writeln!(s, "draw");
        t.push_str(&s);
        job.draws += 1;
    }
    let _ = writeln!(t, "end");
    job.text = t;
    job.pack = p.pack;
    job
}
