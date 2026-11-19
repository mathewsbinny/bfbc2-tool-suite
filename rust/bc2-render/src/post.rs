//! DICE's own post-process (round R5b, `EDITOR_ARCHITECTURE.md` 3.11 phase 2): the engine's compiled Direct3D 11
//! post shaders from `Dist/win32/async/startup-00.fbrb` (`Systems/PostProcess/ShaderContainer/*_d3d11`, 112 pixel and
//! 14 vertex shaders, each a bare DXBC resource) run unchanged as full-screen passes after the scene.
//!
//! **What the shaders determine** (read from their code and reflection): every post shader shares one `$Globals`
//! layout (the container's: `invPixelSize`, `colorScale`, `luminanceVector`, `middleGray`, `minExposure` ...); the
//! pixel shaders read `TEXCOORD0` in register 2, as `vs2D` writes it (`POSITION` in pixels, `COLOR`, `TEXCOORD0`;
//! clip x = 2 x `invPixelSize.x` - 1, y = 1 - 2 y `invPixelSize.y`). The chain drawn:
//!
//! 1. `psUnprojectDepthBuffer` (mid-scene, before the first draw that reads `DepthBuffer`): view distance =
//!    `depthScaleFactors.x / (1 - z / depthScaleFactors.y)`, so the factors are the projection's (`near`,
//!    `far / (far - near)`).
//! 2. `psDownsampleHalfGrayscaleLog` (the scene to half size: the mean of four taps' `ln(max(dot(c,
//!    luminanceVector), 1e-5))`), then `psDownsampleHalf` (four taps' mean, times `colorScale`) down to 1 x 1: the
//!    frame's log-average luminance.
//! 3. `psExposure`: `clamp(middleGray / (exp(log average) x exposureColorScale + 0.001), minExposure, maxExposure)`,
//!    blended with the last frame's by `exposureFactor` (the weather's `MiddleGray`, `MinExposure`, `MaxExposure`).
//! 4. Bloom: `psDownsampleQuarter` (four taps `invTexelSize` apart), `psSeperableFilter15H` / `V` (15 taps of
//!    `seperableKernelWeights`).
//! 5. `psFilmicTonemap`: `colorMatrix x vignette x (filmic(exposure x colorScale x (scene + bloom x bloomScale)) +
//!    flare)`; the filmic curve (with its display gamma) is DICE's literal one, `bloomScale` the weather's
//!    `BloomScale`.
//!
//! **Placeholders** (per-frame or per-pass engine values not in the data; [`PLACEHOLDERS`] lists them for the
//! capture): `exposureFactor` 1 (a converged exposure), `exposureColorScale` 1, `colorScale` (1, 1, 1, 1),
//! `luminanceVector` (0.3, 0.59, 0.11) (psExposure's own literal), the pass chain's sizes, `invPixelSize` per pass
//! (the target's in the vertex shader, the source's in the pixel shader), `invTexelSize` (the source's), the bloom
//! kernel (a Gaussian, deviation 3), `colorMatrix` from `Brightness` / `Contrast` / `Saturation` (identity at the
//! neutral 1 / 1 / 1 every stock state has on `MP_005`), no vignette (`vignetteColor.w` 0), no lens flare, the
//! samplers (linear, clamp). Not wired: HBAO (`HorizonBasedAO*`: every setting - `HsaoRadius`, `HsaoAngleBias`,
//! `HsaoAttenuation`, step and direction counts ... - is the exe's, and how its result meets the frame is engine
//! code), depth of field and film grain (the stock states have them off).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bc2_formats::fbrb;

use crate::dxbc::{self, Reflection};
use crate::frame::{Camera, cbuffer};
use crate::weather::{Post, Values};

/// The engine values of the chain that are not in the data, by name, for the capture to fill.
pub const PLACEHOLDERS: [(&str, &str); 12] = [
    (
        "exposureFactor",
        "1: the exposure converged (the engine blends with the last frame's by frame time)",
    ),
    ("exposureColorScale", "1"),
    ("colorScale", "(1, 1, 1, 1) in the downsamples and the tonemap"),
    ("luminanceVector", "(0.3, 0.59, 0.11), psExposure's own literal"),
    (
        "pass sizes",
        "half size down to 1 x 1 for the luminance, quarter size for the bloom",
    ),
    (
        "invPixelSize",
        "the target's 1 / size in vs2D, the source's in the pixel shaders",
    ),
    (
        "invTexelSize",
        "the source's 1 / size (psDownsampleQuarter's tap offsets)",
    ),
    ("seperableKernelWeights", "a 15-tap Gaussian, deviation 3, normalised"),
    (
        "colorMatrix0..2",
        "built from Brightness / Contrast / Saturation (identity at 1 / 1 / 1)",
    ),
    ("vignetteParams / vignetteColor", "no vignette (vignetteColor.w 0)"),
    ("flareTexture", "black (no lens flare)"),
    ("samplers", "linear filtering, clamp, for every post pass"),
];

/// A compiled post shader and its reflection.
#[derive(Debug, Clone)]
pub struct Shader {
    /// DXBC.
    pub code: Arc<Vec<u8>>,
    /// Its `$Globals` and bindings.
    pub reflect: Arc<Reflection>,
}

/// The engine's post shaders by short name (`psFilmicTonemap`, `vs2D` ...).
#[derive(Debug, Clone, Default)]
pub struct Shaders {
    by_name: HashMap<String, Shader>,
}

/// The shaders the chain uses.
pub const USED: [&str; 8] = [
    "vs2D",
    "psUnprojectDepthBuffer",
    "psDownsampleHalfGrayscaleLog",
    "psDownsampleHalf",
    "psExposure",
    "psDownsampleQuarter",
    "psSeperableFilter15H",
    "psSeperableFilter15V",
];

/// The startup bundle holding the PC post shaders: `game\Dist\win32\async\startup-00.fbrb` (read only).
#[must_use]
pub fn startup_bundle(opfor: &Path) -> PathBuf {
    opfor
        .join("game")
        .join("Dist")
        .join("win32")
        .join("async")
        .join("startup-00.fbrb")
}

impl Shaders {
    /// Reads the chain's shaders ([`USED`] and `psFilmicTonemap`) from the startup bundle.
    ///
    /// # Errors
    /// The bundle cannot be read or a shader is missing.
    pub fn load(bundle: &Path) -> Result<Self, String> {
        let names: Vec<&str> = USED.iter().copied().chain(["psFilmicTonemap"]).collect();
        let files: Vec<String> = names
            .iter()
            .map(|n| format!("Systems/PostProcess/ShaderContainer/{n}_d3d11.res"))
            .collect();
        let got = fbrb::read_path_uncached(bundle, &files).map_err(|e| format!("{}: {e}", bundle.display()))?;
        let mut by_name = HashMap::new();
        for (n, code) in names.iter().zip(got) {
            if code.get(0..4) != Some(b"DXBC") {
                return Err(format!("{n}_d3d11: not a compiled shader"));
            }
            let reflect = Arc::new(dxbc::reflect(&code));
            by_name.insert(
                (*n).to_owned(),
                Shader {
                    code: Arc::new(code),
                    reflect,
                },
            );
        }
        Ok(Self { by_name })
    }

    /// A shader by short name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Shader> {
        self.by_name.get(name)
    }
}

/// One full-screen pass: `vs2D` and a post pixel shader into a new target.
#[derive(Debug, Clone)]
pub struct Pass {
    /// The pixel shader's short name.
    pub name: String,
    /// `vs2D`.
    pub vs: Arc<Vec<u8>>,
    /// The pixel shader.
    pub ps: Arc<Vec<u8>>,
    /// `vs2D`'s `$Globals`.
    pub cb_vs: Vec<u8>,
    /// The pixel shader's `$Globals`.
    pub cb_ps: Vec<u8>,
    /// Textures by t slot: [`SCENE`], [`SCENE_DEPTH`], an earlier pass's output, or an engine texture's key.
    pub inputs: Vec<(u32, String)>,
    /// Its output's key.
    pub output: String,
    /// Its output's size.
    pub size: [u32; 2],
    /// Its output's `DXGI_FORMAT`.
    pub format: u32,
}

/// The drawn scene (HDR colour).
pub const SCENE: &str = "scene:hdr";
/// The scene's depth buffer (device depth).
pub const SCENE_DEPTH: &str = "scene:depth";
/// The last frame's exposure (1 x 1): the chain's start value.
pub const OLD_EXPOSURE: &str = "engine:oldexposure";
/// A 1 x 1 black texture (the lens flare).
pub const BLACK: &str = "engine:black";
/// `DXGI_FORMAT_R32_FLOAT`.
pub const R32F: u32 = 41;
/// `DXGI_FORMAT_R16G16B16A16_FLOAT`.
pub const RGBA16F: u32 = 10;
/// `DXGI_FORMAT_R32G32B32A32_FLOAT`.
pub const RGBA32F: u32 = 2;
/// `DXGI_FORMAT_R8G8B8A8_UNORM`.
pub const RGBA8: u32 = 28;

fn put(v: &mut Values, k: &str, x: &[f32]) {
    v.insert(k.to_owned(), x.to_vec());
}

/// The shared constants of every pass: the weather's post-process and the placeholders.
#[must_use]
pub fn values(post: &Post) -> Values {
    let mut v = Values::new();
    put(&mut v, "colorScale", &[1.0; 4]);
    put(&mut v, "luminanceVector", &[0.3, 0.59, 0.11]);
    put(&mut v, "middleGray", &[post.middle_gray]);
    put(&mut v, "minExposure", &[post.min_exposure]);
    put(&mut v, "maxExposure", &[post.max_exposure.max(post.min_exposure)]);
    put(&mut v, "exposureFactor", &[1.0]);
    put(&mut v, "exposureColorScale", &[1.0]);
    put(&mut v, "downsampleLogAverageEnable", &[f32::from_bits(1)]);
    put(&mut v, "bloomScale", &post.bloom_scale);
    put(&mut v, "vignetteParams", &[1.0, 1.0, 1.0]);
    put(&mut v, "vignetteColor", &[1.0, 1.0, 1.0, 0.0]);
    let [m0, m1, m2] = color_matrix(post);
    put(&mut v, "colorMatrix0", &m0);
    put(&mut v, "colorMatrix1", &m1);
    put(&mut v, "colorMatrix2", &m2);
    let w = gaussian(15, 3.0);
    let mut weights = [0f32; 32];
    for (d, s) in weights.iter_mut().zip(&w) {
        *d = *s;
    }
    put(&mut v, "seperableKernelWeights", &weights);
    v
}

/// The colour matrix rows (r, g, b, constant) of `Brightness` / `Contrast` / `Saturation` (a placeholder for the
/// engine's own construction: saturation about the luminance, contrast about 0.5, then brightness; identity at 1).
#[must_use]
pub fn color_matrix(post: &Post) -> [[f32; 4]; 3] {
    const L: [f32; 3] = [0.3, 0.59, 0.11];
    let mut rows = [[0f32; 4]; 3];
    let channels = post.saturation.iter().zip(&post.contrast).zip(&post.brightness);
    for (k, (row, ((&s, &c), &b))) in rows.iter_mut().zip(channels).enumerate() {
        for (j, (v, l)) in row.iter_mut().zip(L).enumerate() {
            let sat = l * (1.0 - s) + if j == k { s } else { 0.0 };
            *v = sat * c * b;
        }
        row[3] = (0.5 - 0.5 * c) * b;
    }
    rows
}

/// A normalised Gaussian of `n` taps.
#[must_use]
pub fn gaussian(n: usize, deviation: f32) -> Vec<f32> {
    let mid = (n / 2) as f32;
    let w: Vec<f32> = (0..n)
        .map(|i| (-((i as f32 - mid).powi(2)) / (2.0 * deviation * deviation)).exp())
        .collect();
    let sum: f32 = w.iter().sum();
    w.iter().map(|x| x / sum).collect()
}

fn half(s: [u32; 2]) -> [u32; 2] {
    [s[0].div_ceil(2).max(1), s[1].div_ceil(2).max(1)]
}

/// The chain's passes for a scene of `size`.
///
/// # Errors
/// A shader of the chain is missing.
pub fn chain(shaders: &Shaders, post: &Post, size: [u32; 2]) -> Result<Vec<Pass>, String> {
    let base = values(post);
    let vs = shaders.get("vs2D").ok_or("vs2D missing")?;
    let mut out = Vec::new();
    let mut pass = |name: &str, inputs: Vec<(u32, String)>, src: [u32; 2], output: &str, dst: [u32; 2], format: u32| {
        let ps = shaders.get(name).ok_or_else(|| format!("{name} missing"))?;
        let mut v = base.clone();
        let inv = |s: [u32; 2]| vec![1.0 / s[0] as f32, 1.0 / s[1] as f32];
        v.insert("invPixelSize".into(), inv(dst));
        let cb_vs = cbuffer(&vs.reflect, &v, &[], &[]);
        v.insert("invPixelSize".into(), inv(src));
        v.insert("invTexelSize".into(), inv(src));
        out.push(Pass {
            name: name.to_owned(),
            vs: Arc::clone(&vs.code),
            ps: Arc::clone(&ps.code),
            cb_vs,
            cb_ps: cbuffer(&ps.reflect, &v, &[], &[]),
            inputs,
            output: output.to_owned(),
            size: dst,
            format,
        });
        Ok::<(), String>(())
    };
    let mut s = half(size);
    pass(
        "psDownsampleHalfGrayscaleLog",
        vec![(0, SCENE.into())],
        size,
        "post:lum0",
        s,
        R32F,
    )?;
    let mut k = 0;
    while s != [1, 1] {
        let t = half(s);
        pass(
            "psDownsampleHalf",
            vec![(0, format!("post:lum{k}"))],
            s,
            &format!("post:lum{}", k + 1),
            t,
            R32F,
        )?;
        s = t;
        k += 1;
    }
    pass(
        "psExposure",
        vec![(0, OLD_EXPOSURE.into()), (1, format!("post:lum{k}"))],
        [1, 1],
        "post:exposure",
        [1, 1],
        R32F,
    )?;
    let q = [size[0].div_ceil(4).max(1), size[1].div_ceil(4).max(1)];
    pass(
        "psDownsampleQuarter",
        vec![(0, SCENE.into())],
        size,
        "post:quarter",
        q,
        RGBA16F,
    )?;
    pass(
        "psSeperableFilter15H",
        vec![(0, "post:quarter".into())],
        q,
        "post:bloomh",
        q,
        RGBA16F,
    )?;
    pass(
        "psSeperableFilter15V",
        vec![(0, "post:bloomh".into())],
        q,
        "post:bloom",
        q,
        RGBA16F,
    )?;
    pass(
        "psFilmicTonemap",
        vec![
            (0, SCENE.into()),
            (1, "post:bloom".into()),
            (2, BLACK.into()),
            (3, "post:exposure".into()),
        ],
        size,
        "post:ldr",
        size,
        RGBA8,
    )?;
    Ok(out)
}

/// `psUnprojectDepthBuffer`'s pass: the scene's device depth to view distances ([`crate::water::DEPTH`]).
///
/// # Errors
/// A shader is missing.
pub fn unproject(shaders: &Shaders, cam: &Camera, size: [u32; 2]) -> Result<Pass, String> {
    let vs = shaders.get("vs2D").ok_or("vs2D missing")?;
    let ps = shaders
        .get("psUnprojectDepthBuffer")
        .ok_or("psUnprojectDepthBuffer missing")?;
    let mut v = Values::new();
    v.insert("invPixelSize".into(), vec![1.0 / size[0] as f32, 1.0 / size[1] as f32]);
    let q = cam.far / (cam.far - cam.near).max(1e-9);
    v.insert("depthScaleFactors".into(), vec![cam.near as f32, q as f32]);
    Ok(Pass {
        name: "psUnprojectDepthBuffer".into(),
        vs: Arc::clone(&vs.code),
        ps: Arc::clone(&ps.code),
        cb_vs: cbuffer(&vs.reflect, &v, &[], &[]),
        cb_ps: cbuffer(&ps.reflect, &v, &[], &[]),
        inputs: vec![(0, SCENE_DEPTH.into())],
        output: crate::water::DEPTH.into(),
        size,
        format: R32F,
    })
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

    #[test]
    fn neutral_settings_give_the_identity_matrix() {
        let m = color_matrix(&Post::default());
        assert_eq!(m, [[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0]]);
        let g = gaussian(15, 3.0);
        assert!((g.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!(g[7] > g[6] && (g[6] - g[8]).abs() < 1e-9);
    }

    #[test]
    fn the_chain_goes_down_to_one_pixel() {
        assert_eq!(half([5, 3]), [3, 2]);
        assert_eq!(half([1, 1]), [1, 1]);
    }
}
