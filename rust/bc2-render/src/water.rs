//! The level's visual water (round R5b): `Terrains/<lv>/<lv>.Water.res` (`VisualWater`) drawn range by range with
//! its slots' shader entries, as the terrain renderer draws it.
//!
//! What is from the data (read, and checked on `MP_005`):
//! - the file: `u32` 10, `u32` vertex stride (36), `u32` vertex bytes, the vertices, `u32` index bytes, the `u32`
//!   indices, the slots' shader entries and the draw ranges (`bc2_formats::water::parse_visual`: slot, triangles,
//!   first index, index count). `MP_005`: 1,902 vertices, two ranges - `Terrains/MP_005/Shaders/wakes` (608
//!   triangles) and `Terrains/MP_005/Shaders/Water` (2,589); `Shaders/WaterOcean` and `WaterLakeOutlet` are slots no
//!   range draws there.
//! - each entry's programs: the vertex layout DICE compiled for the water vertex (`fca15f81...`: half4 position,
//!   half4 normal and tangent at 8 and 16, half2 texture coordinate at 24), the world positions as stored (the vertex
//!   shader reads `viewProjMatrix`, no world matrix), the state word (`0x585` water: blended, depth written, two
//!   sided; `0x581` wakes) and the program block's textures (`mp05_waterFoamTile_Nm`, `mp05_wake`) and constants.
//! - the engine values the water reads beside the light: the scene's linear depth (`DepthBuffer`, an engine texture
//!   the block names; [`DEPTH`]), `invScreenSize`, `viewportZMinMax`, `time`, and two named block constants the engine
//!   writes over their placeholders (0): `g_screenSizeInv` and `g_depthScaleFactors`. The pixel shader turns its own
//!   depth into a view distance with `g_depthScaleFactors.y / (z - g_depthScaleFactors.x)` and subtracts it from
//!   `DepthBuffer` (the water's thickness, its colour and alpha): so `DepthBuffer` holds view distances, as DICE's
//!   `psUnprojectDepthBuffer` makes them, and the factors are the projection's ([`depth_factors`]).
//!
//! **Choices the capture checks** (`EDITOR_ARCHITECTURE.md` 3.11): the program is the first main-pass one without a
//! point light (key byte 0x37 is a variant number - 0 reads `DepthBuffer`, 1 does not; variant 0 drawn), `time` 0.

use std::collections::HashMap;
use std::sync::Arc;

use bc2_formats::shaderdb::query;
use bc2_formats::water;

use crate::dxbc;
use crate::files::Files;
use crate::material::{self, Db, Draw, Prepared, RenderState, TextureFiles, sampler_states};
use crate::u32_at;

/// The key of the scene's linear depth (`DepthBuffer`), made by the device from the depth drawn so far.
pub const DEPTH: &str = "engine:depthbuffer";

/// `g_depthScaleFactors` (x, y) of a projection (`z` its depth 0 at `near`, 1 at `far`): view distance =
/// `y / (z - x)`, the inverse of `z = far / (far - near) - near far / ((far - near) d)`.
#[must_use]
pub fn depth_factors(near: f64, far: f64) -> [f32; 4] {
    let q = far / (far - near).max(1e-9);
    [q as f32, (-near * q) as f32, 0.0, 0.0]
}

/// The visual water of a terrain, ready to draw: its vertices, indices and one draw per range.
///
/// # Errors
/// The file is missing or does not read.
#[allow(
    clippy::implicit_hasher,
    reason = "the terrain's engine values (`terrain::engine_values`)"
)]
pub fn load(
    files: &Files,
    db: &Db,
    terrain: &str,
    envmap: &str,
    values: &HashMap<&'static str, [f32; 4]>,
    cache: &mut TextureFiles,
) -> Result<Prepared, String> {
    let name = format!("Terrains/{terrain}/{terrain}.Water.res");
    let data = files.read(&name).ok_or_else(|| format!("{name}: not found"))?;
    let visual = water::parse_visual(&data).map_err(|e| format!("{name}: {e}"))?;
    let (stride, vbytes) = (u32_at(&data, 4), u32_at(&data, 8) as usize);
    let vertices = data
        .get(12..12 + vbytes)
        .ok_or_else(|| format!("{name}: vertices run past the file"))?
        .to_vec();
    let mut out = Prepared {
        mesh: name.clone(),
        vertices: Arc::new(vertices),
        indices: Arc::new(visual.indices.iter().flat_map(|v| v.to_le_bytes()).collect()),
        wide: true,
        ..Prepared::default()
    };
    let half = |at: usize| {
        crate::half(u16::from_le_bytes([
            out.vertices.get(at).copied().unwrap_or(0),
            out.vertices.get(at + 1).copied().unwrap_or(0),
        ]))
    };
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for v in 0..vbytes / (stride.max(1) as usize) {
        for k in 0..3 {
            let p = half(v * stride as usize + 2 * k);
            if let (Some(l), Some(h)) = (lo.get_mut(k), hi.get_mut(k)) {
                *l = l.min(p);
                *h = h.max(p);
            }
        }
    }
    if lo.iter().zip(&hi).all(|(a, b)| a <= b) {
        out.bounds = Some((lo, hi));
    }
    let mut made: HashMap<u32, Option<Draw>> = HashMap::new();
    for &[slot, triangles, first, count] in &visual.ranges {
        let entry = visual.slots.get(slot as usize).cloned().unwrap_or_default();
        let draw = made
            .entry(slot)
            .or_insert_with(|| match water_draw(files, db, &entry, envmap, values, cache) {
                Ok(d) => Some(d),
                Err(e) => {
                    out.notes.push(format!("water: {e}"));
                    None
                }
            });
        let Some(mut d) = draw.clone() else { continue };
        d.stride = stride;
        d.first_index = first;
        d.index_count = count.min(triangles * 3);
        d.centre = out.bounds.map(|(l, h)| {
            let mut c = [0f32; 3];
            for ((c, a), b) in c.iter_mut().zip(l).zip(h) {
                *c = f32::midpoint(a, b);
            }
            c
        });
        out.draws.push(d);
    }
    if out.draws.is_empty() && out.notes.is_empty() {
        out.notes.push("water: no range to draw".into());
    }
    Ok(out)
}

/// One slot's draw (everything but the range).
fn water_draw(
    files: &Files,
    db: &Db,
    entry: &str,
    envmap: &str,
    values: &HashMap<&'static str, [f32; 4]>,
    cache: &mut TextureFiles,
) -> Result<Draw, String> {
    let progs = db
        .entry_programs(entry)
        .ok_or_else(|| format!("{entry}: not in the database"))?;
    let layout = progs
        .iter()
        .filter_map(|&k| db.programs.get(k))
        .find(|p| p.key_byte(0x30) == 0)
        .map(material::Program::layout)
        .ok_or_else(|| format!("{entry}: no main-pass program"))?;
    let (k, _) = material::pick_program(db, progs, &layout).ok_or_else(|| format!("{entry}: no program"))?;
    let prog = db.programs.get(k).ok_or("program")?;
    let (vs, ps) = (db.vs.get(prog.vs).ok_or("vs")?, db.ps.get(prog.ps).ok_or("ps")?);
    let block = db.param(prog.block2);
    let state = RenderState::of(prog.state.0, prog.state.1);
    let ps_reflect = Arc::new(dxbc::reflect(&ps.code));
    let name = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
    let records: HashMap<u32, String> = query::block_textures(block, false)
        .iter()
        .map(|t| (u32::from(t.sampler), name(t.name)))
        .collect();
    let engine: HashMap<u32, String> = query::block_engine_textures(block, false)
        .iter()
        .map(|(n, s)| (u32::from(*s), name(n)))
        .collect();
    let mut textures = Vec::new();
    for r in ps_reflect.resources.iter().filter(|r| r.kind == 2) {
        if r.point == 0 && r.name.to_lowercase().contains("shadowmap") {
            continue;
        }
        let key = if let Some(t) = records.get(&r.point) {
            t.clone()
        } else if r.name == "texture_outdoorLightReflectionEnvmap" {
            envmap.to_owned()
        } else if engine.get(&r.point).is_some_and(|n| n == "DepthBuffer") {
            textures.push((r.point, DEPTH.to_owned()));
            continue;
        } else if engine.get(&r.point).is_some_and(|n| n == "heightmap") && !values.is_empty() {
            // the terrain's height map (DICE's Vietnam water, OPFOR's Wake water: the shore's foam and absorption)
            textures.push((r.point, crate::terrain::HEIGHTMAP.to_owned()));
            continue;
        } else {
            continue;
        };
        if material::texture(files, cache, &key).is_some() {
            textures.push((r.point, key));
        }
    }
    let constants = query::block_constants(block, false);
    // the terrain renderer's values a record names (heightmapWorldScaleXZ / OffsetXZ / ScaleY), written over the
    // stored placeholders as the patches' are
    let ps_extra: Vec<(u8, [f32; 4])> = constants
        .iter()
        .filter_map(|c| {
            let n: String = c.name.iter().map(|&b| char::from(b)).collect();
            values.get(n.as_str()).map(|v| (c.register, *v))
        })
        .collect();
    Ok(Draw {
        part: entry.rsplit('/').next().unwrap_or(entry).to_owned(),
        entry: entry.to_owned(),
        program: k,
        vs: Arc::clone(&vs.code),
        ps: Arc::clone(&ps.code),
        vs_reflect: Arc::new(dxbc::reflect(&vs.code)),
        ps_reflect,
        elements: material::vs_elements(&vs.tail)
            .into_iter()
            .filter(|e| e.slot == 0)
            .collect(),
        vertex_offset: 0,
        stride: 0,
        index_count: 0,
        first_index: 0,
        kind: prog.key_byte(0x33),
        bones: None,
        material: constants.iter().map(|c| (c.register, c.values)).collect(),
        vs_extra: Vec::new(),
        ps_extra,
        named: material::engine_named(&constants),
        // OPFOR's Wake water (BF1943's, translated) reads its material in `waterMaterial`, not `constants`: without the
        // block's window its `g_depthScaleFactors` stayed 0, the water's depth 0 and its alpha 0 (round R5e)
        ps_window: query::block_registers(block),
        textures,
        vs_textures: Vec::new(),
        samplers: sampler_states(db.param(ps.param)),
        vs_samplers: Vec::new(),
        class: material::draw_class(state.as_ref(), &ps.code),
        state,
        centre: None,
        bounds: None,
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
    fn depth_factors_invert_the_projection() {
        let (n, f) = (0.5f64, 4000.0f64);
        let [x, y, ..] = depth_factors(n, f);
        for d in [1.0f64, 10.0, 333.0, 3999.0] {
            let z = f / (f - n) - n * f / ((f - n) * d);
            let back = f64::from(y) / (z - f64::from(x));
            assert!((back - d).abs() / d < 1e-3, "{d} -> {back}");
        }
    }
}
