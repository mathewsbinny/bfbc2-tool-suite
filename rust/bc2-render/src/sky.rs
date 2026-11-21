//! The sky (round R5d): DICE's `Systems/Weather/Shaders/Surface/EnvironmentMain` drawn on a dome around the camera,
//! as the weather renderer draws it (spec RNDR-SKY-1 to -3).
//!
//! What is from the data (the level's own database, read on `MP_005`, Wake Island and the other levels):
//! - the entry's main-pass programs (key byte 0x30 = 0) come in four variants (key byte 0x37): 3 reads the scene's
//!   linear depth (`DepthBuffer`), the panoramic cloud texture and its alpha, 0 the depth and the texture, 1 the depth
//!   alone, 2 nothing; all with the state `0x481` (blended `SRC_ALPHA` / `INV_SRC_ALPHA`, depth tested, not written,
//!   back faces culled) and one vertex layout: `TEXCOORD0` half4 position at 0, `TEXCOORD1` half4 at 8, `TEXCOORD2`
//!   float4 at 16, `TEXCOORD3` float3 at 32 (44 bytes);
//! - the vertex shader places a unit dome at `0.9 x g_viewDistance.x` through `worldViewProjMatrix` and passes its
//!   world position (`worldMatrix`); the pixel shader takes the view direction from `cameraPos` minus that position:
//!   a cubic horizon gradient in the elevation per channel (`g_horizontalCoefficientsR/G/B`, times
//!   `g_horizontalGainAndMore.x`), a Gaussian halo about the sun (`g_muAndSigma`, a power `.z`, an amount `.y`, the
//!   halo colour times `.w`; the back side's gain `g_haloBleedPowerAndMore.y`; a bleed `.x` with
//!   `g_haloColorAndHaloBleedGain.w`), the haze colour below `g_hazeColorAndHazeFadeMin.w` .. `
//!   g_sunDirectionAndHazeFadeMax.w` of elevation, a Henyey-Greenstein sun disc (`g = 1 - g_mainPassAndMore.z`, colour
//!   `g_sunColorAndGain.xyz` times `.w` squared, only when `g_mainPassAndMore.x > 0.5`), and the panoramic clouds
//!   (azimuth and polar angle into `g_panoramicUVRanges`, times `g_tileFactor.x`, less `g_additiveThreshhold.x`, times
//!   `g_panoramicTransparency.x`); where the scene drew something (`DepthBuffer` taken back to a depth below
//!   0.999999 with `g_depthFactors.zw`) only the haze and horizon colour, no clouds and no sun. Alpha 1.
//! - the values: the weather state's `Sky` (`HorizontalGain`, `HaloAlphaGain`, `HaloAlphaPower`, `HaloGain`,
//!   `HaloBleedPower`, `HaloBackReflectionGain`, `HaloColor`, `HaloBleedGain`, `HazeColor`, `HazeFadeMin` / `Max`,
//!   `Mu`, `Sigma`, `HorizontalCoefficientsR/G/B`), `Sun` (`Color`, `Gain`, `Size`), `Clouds` (its
//!   `PanoramicCloudLayer`: UV ranges, `TileFactor`, `AdditiveThreshhold`, `Trancparency`, `Rotation`), the
//!   settings' `SkydomeEnable` and `PanoramicCloudBlendMode` and the static cloud layer's panoramic textures
//!   ([`crate::weather::Dome`]).
//!
//! **Read by name, not proven** (the capture): which `Sky` field goes in which component (the names above follow the
//! constants' names and the shader's use), the UV ranges from the layer's 0..1 ranges (azimuth `x 2 pi`, the polar
//! angle `(1 - y) x pi / 2`), the dome's tessellation (the engine's own dome mesh is not in the data: a UV sphere
//! here), `g_viewDistance` ([`VIEW_DISTANCE`], the views' far plane) and the sun direction (the light's placeholder,
//! [`crate::weather::NOON_DIR`]).

use std::collections::HashMap;
use std::sync::Arc;

use bc2_formats::shaderdb::query;

use crate::dxbc;
use crate::files::Files;
use crate::material::{self, Db, Draw, Prepared, RenderState, TextureFiles, sampler_states};
use crate::weather::{Fields, Weather};

/// The weather renderer's sky entry.
pub const ENTRY: &str = "Systems/Weather/Shaders/Surface/EnvironmentMain";

/// `g_viewDistance`: the dome stands at 0.9 of it. The views' far plane (the editor's `FAR`, the examples' 4,000 m).
pub const VIEW_DISTANCE: f32 = 4000.0;

/// Whether the views draw the panoramic cloud layer ([`load`]'s `clouds`): not until its composite is proven.
pub const CLOUDS: bool = false;

/// Bytes per dome vertex (the entry's layout).
pub const STRIDE: u32 = 44;

/// Rings from the zenith to the nadir, and segments around, of the dome.
const RINGS: u32 = 24;
const SEGMENTS: u32 = 48;

/// `f32` as IEEE half (round to nearest; the dome's coordinates lie in -1..1).
#[must_use]
pub fn to_half(v: f32) -> u16 {
    let b = v.to_bits();
    let sign = ((b >> 16) & 0x8000) as u16;
    let exp = ((b >> 23) & 0xff) as i32 - 127 + 15;
    let man = b & 0x7f_ffff;
    if exp <= 0 {
        if exp < -10 {
            return sign;
        }
        let m = (man | 0x80_0000) >> (1 - exp);
        return sign | ((m + 0x1000) >> 13) as u16;
    }
    if exp >= 31 {
        return sign | 0x7c00;
    }
    let h = (u32::from(sign) << 16) | ((exp as u32) << 23) | man;
    sign | ((((h >> 13) & 0x7fff) + ((man >> 12) & 1)) as u16)
}

/// A unit sphere seen from inside (counter-clockwise on screen from its centre), in the entry's layout: vertices and
/// 32-bit indices.
#[must_use]
pub fn dome() -> (Vec<u8>, Vec<u8>) {
    let mut vb = Vec::new();
    for r in 0..=RINGS {
        let polar = std::f32::consts::PI * r as f32 / RINGS as f32;
        for s in 0..=SEGMENTS {
            let az = std::f32::consts::TAU * s as f32 / SEGMENTS as f32;
            let p = [polar.sin() * az.cos(), polar.cos(), polar.sin() * az.sin(), 1.0];
            for c in p {
                vb.extend(to_half(c).to_le_bytes());
            }
            vb.resize(vb.len() + (STRIDE as usize - 8), 0);
        }
    }
    let row = SEGMENTS + 1;
    let mut ib = Vec::new();
    for r in 0..RINGS {
        for s in 0..SEGMENTS {
            let a = r * row + s;
            for i in [a, a + row, a + 1, a + 1, a + row, a + row + 1] {
                ib.extend(i.to_le_bytes());
            }
        }
    }
    (vb, ib)
}

fn first(f: &Fields, k: &str, default: f32) -> f32 {
    f.get(k).and_then(|v| v.first().copied()).unwrap_or(default)
}

fn four(f: &Fields, k: &str) -> [f32; 4] {
    let v = f.get(k);
    [0, 1, 2, 3].map(|i| v.and_then(|v| v.get(i).copied()).unwrap_or(0.0))
}

/// The entry's constants by name for a weather state (`towards`: towards the sun; the shader takes the light's
/// travel, its negation).
#[must_use]
pub fn values(w: &Weather, towards: [f32; 3]) -> HashMap<&'static str, [f32; 4]> {
    let (sky, sun, clouds) = (&w.sky, &w.sun, &w.clouds);
    let len = towards.iter().map(|v| v * v).sum::<f32>().sqrt().max(1e-6);
    let d = towards.map(|v| -v / len);
    let halo = four(sky, "HaloColor");
    let haze = four(sky, "HazeColor");
    let mu = four(sky, "Mu");
    let sigma = four(sky, "Sigma");
    let sun_colour = four(sun, "Color");
    let (half_pi, tau) = (std::f32::consts::FRAC_PI_2, std::f32::consts::TAU);
    let mut v = HashMap::new();
    v.insert(
        "g_horizontalGainAndMore",
        [
            first(sky, "HorizontalGain", 1.0),
            first(sky, "HaloAlphaGain", 1.0),
            first(sky, "HaloAlphaPower", 1.0),
            first(sky, "HaloGain", 1.0),
        ],
    );
    v.insert(
        "g_haloBleedPowerAndMore",
        [
            first(sky, "HaloBleedPower", 1.0),
            first(sky, "HaloBackReflectionGain", 1.0),
            0.0,
            0.0,
        ],
    );
    v.insert(
        "g_haloColorAndHaloBleedGain",
        [halo[0], halo[1], halo[2], first(sky, "HaloBleedGain", 0.0)],
    );
    v.insert(
        "g_sunDirectionAndHazeFadeMax",
        [d[0], d[1], d[2], first(sky, "HazeFadeMax", 0.0)],
    );
    v.insert(
        "g_hazeColorAndHazeFadeMin",
        [haze[0], haze[1], haze[2], first(sky, "HazeFadeMin", -0.25)],
    );
    v.insert("g_muAndSigma", [mu[0], mu[1], sigma[0].max(1e-3), sigma[1].max(1e-3)]);
    for c in ["R", "G", "B"] {
        let k: &'static str = match c {
            "R" => "g_horizontalCoefficientsR",
            "G" => "g_horizontalCoefficientsG",
            _ => "g_horizontalCoefficientsB",
        };
        v.insert(k, four(sky, &format!("HorizontalCoefficients{c}")));
    }
    v.insert("g_mainPassAndMore", [1.0, 0.0, first(sun, "Size", 0.01), 0.0]);
    v.insert(
        "g_sunColorAndGain",
        [sun_colour[0], sun_colour[1], sun_colour[2], first(sun, "Gain", 1.0)],
    );
    if !clouds.is_empty() {
        v.insert("g_panoramicRotation", [first(clouds, "Rotation", 0.0), 0.0, 0.0, 0.0]);
        v.insert(
            "g_panoramicUVRanges",
            [
                first(clouds, "PanoramicUVMinX", 0.0) * tau,
                first(clouds, "PanoramicUVMaxX", 1.0) * tau,
                (1.0 - first(clouds, "PanoramicUVMaxY", 1.0)) * half_pi,
                (1.0 - first(clouds, "PanoramicUVMinY", 0.0)) * half_pi,
            ],
        );
        v.insert("g_tileFactor", [first(clouds, "TileFactor", 1.0), 0.0, 0.0, 0.0]);
        v.insert(
            "g_additiveThreshhold",
            [first(clouds, "AdditiveThreshhold", 0.0), 0.0, 0.0, 0.0],
        );
        v.insert(
            "g_panoramicTransparency",
            [first(clouds, "Trancparency", 1.0), 0.0, 0.0, 0.0],
        );
    }
    v
}

/// The variant (key byte 0x37) for what the dome has: the panoramic texture and its alpha (3), the texture (0), or
/// neither (1: the depth alone).
#[must_use]
pub fn variant(texture: bool, alpha: bool) -> u8 {
    match (texture, alpha) {
        (true, true) => 3,
        (true, false) => 0,
        _ => 1,
    }
}

/// The sky dome of a level, ready to draw (one draw, its centre none: [`crate::live::Key::Sky`] follows the camera).
/// `clouds`: with the panoramic cloud layer (variants 3 / 0). Off in the views ([`CLOUDS`]): with the data's values
/// read by name, `MP_005`'s `PcbmMultiplyAddAlpha` clouds (`(texture - AdditiveThreshhold 0.3) x (sky + 1)`, the
/// texture an sRGB view: 0.17 on average) draw a black band over most of the panorama, which the game does not show -
/// the layer's composite is the capture's to settle (spec RNDR-SKY-3).
///
/// # Errors
/// The settings turn the dome off, or the database has no main-pass program of the entry.
pub fn load(
    files: &Files,
    db: &Db,
    w: &Weather,
    towards: [f32; 3],
    clouds: bool,
    cache: &mut TextureFiles,
) -> Result<Prepared, String> {
    if w.dome.enabled == Some(false) {
        return Err("the weather's settings turn the sky dome off (SkydomeEnable false)".into());
    }
    let progs = db
        .entry_programs(ENTRY)
        .ok_or_else(|| format!("{ENTRY}: not in the database"))?;
    let has = |name: &Option<String>, cache: &mut TextureFiles| {
        name.as_ref()
            .filter(|n| material::texture(files, cache, n).is_some())
            .cloned()
    };
    let panoramic = if clouds { has(&w.dome.panoramic, cache) } else { None };
    let alpha = if panoramic.is_some() && w.dome.blend_mode.contains("Alpha") {
        has(&w.dome.panoramic_alpha, cache)
    } else {
        None
    };
    let want = variant(panoramic.is_some(), alpha.is_some());
    let main: Vec<usize> = progs
        .iter()
        .copied()
        .filter(|&k| db.programs.get(k).is_some_and(|p| p.key_byte(0x30) == 0))
        .collect();
    let k = main
        .iter()
        .copied()
        .find(|&k| db.programs.get(k).is_some_and(|p| p.key_byte(0x37) == want))
        .or_else(|| main.first().copied())
        .ok_or_else(|| format!("{ENTRY}: no main-pass program"))?;
    let prog = db.programs.get(k).ok_or("program")?;
    let (vs, ps) = (db.vs.get(prog.vs).ok_or("vs")?, db.ps.get(prog.ps).ok_or("ps")?);
    let block = db.param(prog.block2);
    let state = RenderState::of(prog.state.0, prog.state.1);
    let ps_reflect = Arc::new(dxbc::reflect(&ps.code));
    let vs_reflect = Arc::new(dxbc::reflect(&vs.code));
    let vals = values(w, towards);
    let name = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
    let constants = query::block_constants(block, false);
    // the stored values (DICE's: the UV ranges, tiling, transparency) with the state's written over them by name
    let material: Vec<(u8, [f32; 4])> = constants
        .iter()
        .map(|c| (c.register, vals.get(name(c.name).as_str()).copied().unwrap_or(c.values)))
        .collect();
    let engine: HashMap<u32, String> = query::block_engine_textures(block, false)
        .iter()
        .map(|(n, s)| (u32::from(*s), name(n)))
        .collect();
    let mut textures = Vec::new();
    for r in ps_reflect.resources.iter().filter(|r| r.kind == 2) {
        let key = match engine.get(&r.point).map(String::as_str) {
            Some("DepthBuffer") => Some(crate::water::DEPTH.to_owned()),
            Some("g_panoramicTexture") => panoramic.clone(),
            Some("g_panoramicAlpha") => alpha.clone(),
            _ => None,
        };
        if let Some(key) = key {
            textures.push((r.point, key));
        }
    }
    // the vertex shader's own block: g_viewDistance (the dome's radius over 0.9) and the cloud masks
    let vs_extra = query::block_constants(db.param(vs.param), false)
        .iter()
        .filter(|c| name(c.name) == "g_viewDistance")
        .map(|c| (c.register, [VIEW_DISTANCE, 0.0, 0.0, 0.0]))
        .collect();
    let (vertices, indices) = dome();
    let draw = Draw {
        part: "sky".to_owned(),
        entry: ENTRY.to_owned(),
        program: k,
        vs: Arc::clone(&vs.code),
        ps: Arc::clone(&ps.code),
        vs_reflect,
        ps_reflect,
        elements: material::vs_elements(&vs.tail)
            .into_iter()
            .filter(|e| e.slot == 0)
            .collect(),
        vertex_offset: 0,
        stride: STRIDE,
        index_count: (indices.len() / 4) as u32,
        first_index: 0,
        kind: 0,
        bones: None,
        material,
        vs_extra,
        ps_extra: Vec::new(),
        named: material::engine_named(&constants),
        textures,
        vs_textures: Vec::new(),
        samplers: sampler_states(db.param(ps.param)),
        vs_samplers: Vec::new(),
        class: material::draw_class(state.as_ref(), &ps.code),
        state,
        centre: None,
        bounds: None,
    };
    let mut notes = Vec::new();
    if clouds && w.dome.panoramic.is_some() && panoramic.is_none() {
        notes.push("sky: the panoramic cloud texture is not readable; drawn without clouds".to_owned());
    }
    Ok(Prepared {
        mesh: ENTRY.to_owned(),
        draws: vec![draw],
        notes,
        vertices: Arc::new(vertices),
        indices: Arc::new(indices),
        wide: true,
        bounds: None,
        shader_set: String::new(),
        lods: Vec::new(),
    })
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    #[test]
    fn halves_round_trip() {
        for v in [0.0f32, 1.0, -1.0, 0.5, 0.333_333, -0.6, 0.999] {
            let back = crate::half(to_half(v));
            assert!((back - v).abs() <= v.abs() * 1e-3 + 1e-6, "{v} -> {back}");
        }
    }

    #[test]
    fn the_dome_is_a_closed_unit_sphere() {
        let (vb, ib) = dome();
        let n = vb.len() / STRIDE as usize;
        assert_eq!(n, ((RINGS + 1) * (SEGMENTS + 1)) as usize);
        assert_eq!(ib.len() / 4, (RINGS * SEGMENTS * 6) as usize);
        for v in 0..n {
            let h = |k: usize| crate::half(u16::from_le_bytes([vb[v * 44 + 2 * k], vb[v * 44 + 2 * k + 1]]));
            let r = (h(0) * h(0) + h(1) * h(1) + h(2) * h(2)).sqrt();
            assert!((r - 1.0).abs() < 2e-3, "vertex {v}: radius {r}");
            assert_eq!(h(3), 1.0);
        }
    }

    // spec: RNDR-SKY-2
    #[test]
    fn values_follow_the_sky_state_by_name() {
        let mut w = Weather::default();
        w.sky.insert("HorizontalGain".into(), vec![2.0]);
        w.sky.insert("HazeColor".into(), vec![0.25, 0.5, 0.75, -4.0]);
        w.sky.insert("HazeFadeMin".into(), vec![-0.25]);
        w.sky
            .insert("HorizontalCoefficientsB".into(), vec![0.0, 0.0, -0.1, 0.3]);
        w.sun.insert("Size".into(), vec![0.007]);
        w.clouds.insert("PanoramicUVMaxY".into(), vec![0.3]);
        let v = values(&w, [0.0, 2.0, 0.0]);
        assert_eq!(v["g_horizontalGainAndMore"][0], 2.0);
        assert_eq!(v["g_hazeColorAndHazeFadeMin"], [0.25, 0.5, 0.75, -0.25]);
        assert_eq!(v["g_horizontalCoefficientsB"], [0.0, 0.0, -0.1, 0.3]);
        assert_eq!(v["g_sunDirectionAndHazeFadeMax"][1], -1.0);
        assert_eq!(v["g_mainPassAndMore"], [1.0, 0.0, 0.007, 0.0]);
        let uv = v["g_panoramicUVRanges"];
        assert!(
            (uv[1] - std::f32::consts::TAU).abs() < 1e-6 && (uv[2] - 0.7 * std::f32::consts::FRAC_PI_2).abs() < 1e-6
        );
        assert_eq!(variant(true, true), 3);
        assert_eq!(variant(true, false), 0);
        assert_eq!(variant(false, false), 1);
    }
}
