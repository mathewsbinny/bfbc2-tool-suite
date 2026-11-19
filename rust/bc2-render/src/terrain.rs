//! Terrain patches drawn with their level's `PatchMaskShaders` entries, on the level's own height map and mask-tile
//! atlas.
//!
//! What is from the data (read, and checked on `MP_005`):
//! - the `VisualTerrain` (`<lv>.Visual.res`, the grammar of `bfbc2-console-convert/terrain.py`): the patch shaders
//!   (name, layer count, tiled-mask count) and one patch map per terrain block: its bounds, `[tile size, border,
//!   tiles per row, rows, cells per side]` (`MP_005`: 66, 1, 15, 7, 128), per cell four `u16` mask tiles and four `u8`
//!   flags, and the cell's patch shader. Rows run along z: `MP_005`'s sea-floor patches lie 37 m lower than the rest
//!   that way round, not transposed; a cell's tiled-mask count equals its non-zero flags on all 16,384 cells.
//! - the shaders' inputs and constants (their disassembly): the vertex shader reads the grid point (`R16G16_SINT`)
//!   and mask-atlas coordinates (`R16G16B16A16_SNORM`, two masks each), samples `heightmap` at `grid *
//!   heightmapTexCoordScaleAndOffset.xy + .zw` and places the vertex at `(x, h, z) * posScale + posOffset`; the pixel
//!   shader takes the world back from that height-map coordinate with `heightmapScaleAndOffsetXZ`, its normal from
//!   neighbouring samples `heightmapPixelSizeAndAspect.x` apart scaled by `.y`, the colour map over the whole level,
//!   and each tiled layer's mask from `maskTileMap` at the interpolated atlas coordinate.
//!
//! **The prototype's reading, not yet the engine's** (`EDITOR_ARCHITECTURE.md` 3.11, the capture asked for): the
//! atlas coordinate of tiled mask k is tile `tiles[k]` (column `t % per row`, row `t / per row`) inside its border,
//! running 0..1 across the cell along x and z; the grid is one vertex per height sample; the dynamic (destruction)
//! mask is empty.

use std::collections::HashMap;
use std::sync::Arc;

use bc2_formats::shaderdb::query;
use bc2_formats::terrain::Heightfield;

use crate::dxbc;
use crate::files::Files;
use crate::material::{self, Db, Draw, Element, Prepared, RenderState, TextureFiles, sampler_states};
use crate::{cstr_at, f32_at, u32_at};

/// The patch vertex layout with mask tiles (`renderqa.PATCH_LAYOUT`).
pub const PATCH_LAYOUT: [u8; 16] = [
    0x3d, 0x4f, 0xc7, 0x00, 0xfb, 0xe0, 0x98, 0x96, 0x5f, 0xbf, 0x73, 0x7f, 0xc0, 0x0c, 0xbf, 0xac,
];
/// The grid-only layout DICE compiles for rule-only patches (`renderqa.PATCH_LAYOUT_NO_TILES`).
pub const PATCH_LAYOUT_NO_TILES: [u8; 16] = [
    0x52, 0xdc, 0x29, 0xa1, 0x9c, 0xa9, 0x5d, 0x0c, 0x5f, 0xb9, 0x72, 0x22, 0x7e, 0x2f, 0x7b, 0xf7,
];
/// Bytes per patch vertex: the grid point (2 x `i16`) at 0, mask coordinates (4 x `snorm16`) at 4 and 12.
pub const PATCH_STRIDE: u32 = 20;

/// A patch shader of the `VisualTerrain`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatchShader {
    /// Its entry (`Terrains/MP_005/MP_005/PatchMaskShaders/Dunes_Asphalt_Destruction`).
    pub name: String,
    /// Its undergrowth shader (may be empty).
    pub undergrowth: String,
    /// Layers.
    pub layers: u32,
    /// Tiled masks.
    pub tiled: u32,
}

/// One terrain block's patch map.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchMap {
    /// min x, y, z, max x, y, z.
    pub bounds: [f32; 6],
    /// Tile size, border, tiles per row, rows, cells per side.
    pub header: [u32; 5],
    /// Per cell (rows along z), the tiled masks' atlas tiles.
    pub tiles: Vec<[u16; 4]>,
    /// Per cell, each mask's flag (8 where the mask is tiled).
    pub flags: Vec<[u8; 4]>,
    /// The patch shader of each cell of the shader grid (`shader_side` per side).
    pub shader: Vec<u8>,
    /// Cells per side of the shader grid.
    pub shader_side: usize,
    /// The height field it is drawn on.
    pub heightfield: String,
    /// The mask-tile atlas.
    pub atlas: String,
}

impl PatchMap {
    /// Cells per side.
    #[must_use]
    pub fn side(&self) -> usize {
        self.header[4] as usize
    }
}

/// A `VisualTerrain`'s patch shaders and patch maps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VisualTerrain {
    /// The patch shaders.
    pub shaders: Vec<PatchShader>,
    /// One per terrain block.
    pub maps: Vec<PatchMap>,
}

struct Walk<'a> {
    b: &'a [u8],
    o: usize,
}

impl Walk<'_> {
    fn u32(&mut self) -> Result<u32, String> {
        let v = u32_at(self.b, self.o);
        self.skip(4)?;
        Ok(v)
    }
    fn skip(&mut self, n: usize) -> Result<(), String> {
        if self.o + n > self.b.len() {
            return Err(format!("visual terrain: runs past the end at {:#x}", self.o));
        }
        self.o += n;
        Ok(())
    }
    fn string(&mut self) -> Result<String, String> {
        let s = cstr_at(self.b, self.o);
        self.skip(s.len() + 1)?;
        Ok(s)
    }
}

/// Reads a PC `VisualTerrain`'s patch shaders and patch maps (roads are skipped).
///
/// # Errors
/// The file does not follow the grammar.
pub fn parse_visual(data: &[u8]) -> Result<VisualTerrain, String> {
    let mut w = Walk { b: data, o: 0 };
    let n = w.u32()?;
    if !(1..=256).contains(&n) {
        return Err(format!("visual terrain: {n} terrain blocks"));
    }
    for _ in 0..2 * n {
        for _ in 0..w.u32()? {
            w.string()?;
            w.string()?;
            w.string()?;
            w.skip(4 * 9)?;
        }
        let (v, _, i) = (w.u32()?, w.u32()?, w.u32()?);
        w.skip(28 * v as usize + 4 * i as usize)?;
    }
    let mut out = VisualTerrain::default();
    for _ in 0..w.u32()? {
        let (name, undergrowth) = (w.string()?, w.string()?);
        let (layers, tiled) = (w.u32()?, w.u32()?);
        for _ in 0..w.u32()? {
            for _ in 0..w.u32()? {
                w.string()?;
                w.skip(1)?;
                w.string()?;
                w.skip(4 * 12 + 2)?;
            }
        }
        out.shaders.push(PatchShader {
            name,
            undergrowth,
            layers,
            tiled,
        });
    }
    if w.u32()? != n {
        return Err("visual terrain: the patch maps do not match the road blocks in number".into());
    }
    for _ in 0..n {
        let mut bounds = [0f32; 6];
        for b in &mut bounds {
            *b = f32_at(data, w.o);
            w.skip(4)?;
        }
        let mut header = [0u32; 5];
        for h in &mut header {
            *h = w.u32()?;
        }
        let side = header[4] as usize;
        let cells = side * side;
        let at = w.o;
        w.skip(8 * cells)?;
        let u16_at =
            |k: usize| u16::from_le_bytes([data.get(k).copied().unwrap_or(0), data.get(k + 1).copied().unwrap_or(0)]);
        let tiles = (0..cells)
            .map(|c| [0, 1, 2, 3].map(|k| u16_at(at + 8 * c + 2 * k)))
            .collect();
        let at = w.o;
        w.skip(4 * cells)?;
        let flags = (0..cells)
            .map(|c| [0, 1, 2, 3].map(|k| data.get(at + 4 * c + k).copied().unwrap_or(0)))
            .collect();
        let shader_side = w.u32()? as usize;
        let at = w.o;
        w.skip(shader_side * shader_side)?;
        let shader = data
            .get(at..at + shader_side * shader_side)
            .unwrap_or_default()
            .to_vec();
        let (heightfield, atlas) = (w.string()?, w.string()?);
        out.maps.push(PatchMap {
            bounds,
            header,
            tiles,
            flags,
            shader,
            shader_side,
            heightfield,
            atlas,
        });
    }
    if w.o != data.len() {
        return Err(format!(
            "visual terrain: the data ends at {:#x}, the file at {:#x}",
            w.o,
            data.len()
        ));
    }
    Ok(out)
}

/// A texture the engine makes (not a resource file): a format and its texels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawTexture {
    /// Width.
    pub width: u32,
    /// Height.
    pub height: u32,
    /// `DXGI_FORMAT` (56 `R16_UNORM`, 41 `R32_FLOAT`).
    pub format: u32,
    /// Bytes per texel.
    pub texel: u32,
    /// The texels, row by row.
    pub data: Vec<u8>,
}

/// The engine's height map: the height field's stored values as `R16_UNORM` (height = min y + value * (max y - min
/// y), the posScale / posOffset below).
#[must_use]
pub fn heightmap(hf: &Heightfield) -> RawTexture {
    RawTexture {
        width: hf.size,
        height: hf.size,
        format: 56,
        texel: 2,
        data: hf.raw.iter().flat_map(|v| v.to_le_bytes()).collect(),
    }
}

/// A 1 x 1 texture of 0 (`R32_FLOAT`): the empty dynamic (destruction) mask.
#[must_use]
pub fn zero() -> RawTexture {
    RawTexture {
        width: 1,
        height: 1,
        format: 41,
        texel: 4,
        data: vec![0; 4],
    }
}

/// The key of the height map texture.
pub const HEIGHTMAP: &str = "engine:heightmap";
/// The key of the empty texture.
pub const ZERO: &str = "engine:zero";

/// The terrain renderer's named constants for a height field (`pcbuild.ENGINE_NAMED`), as this prototype reads the
/// shaders (module docs).
#[must_use]
pub fn engine_values(hf: &Heightfield) -> HashMap<&'static str, [f32; 4]> {
    let n = hf.size as f32;
    let (cx, cz) = hf.cell();
    let (cx, cz) = (cx as f32, cz as f32);
    let range = hf.max[1] - hf.min[1];
    let mut v = HashMap::new();
    v.insert("posScale", [cx, range, cz, 0.0]);
    v.insert("posOffset", [hf.min[0], hf.min[1], hf.min[2], 0.0]);
    v.insert("heightmapTexCoordScaleAndOffset", [1.0 / n, 1.0 / n, 0.5 / n, 0.5 / n]);
    v.insert(
        "heightmapScaleAndOffsetXZ",
        [
            1.0 / (cx * n),
            1.0 / (cz * n),
            0.5 / n - hf.min[0] / (cx * n),
            0.5 / n - hf.min[2] / (cz * n),
        ],
    );
    v.insert("heightmapPixelSizeAndAspect", [1.0 / n, range / cx, 0.0, 0.0]);
    v.insert("heightmapWorldOffsetXZ", [hf.min[0], hf.min[2], 0.0, 0.0]);
    v.insert("dynamicMaskAtlasOffset", [0.0; 4]);
    v.insert("dynamicMaskAtlasParams", [1.0; 4]);
    v.insert("dynamicMaskTileBorderParams", [1.0, 0.0, 0.0, 0.0]);
    v.insert("heightmapDecalOffsetY", [0.0; 4]);
    v
}

/// The input element of a patch stream by its semantic (`renderqa.PATCH_FORMATS` / `PATCH_OFFSETS`).
fn patch_element(semantic: u32) -> Option<Element> {
    let (format, offset) = match semantic {
        0 => (38, 0),
        1 => (13, 4),
        2 => (13, 12),
        _ => return None,
    };
    Some(Element {
        semantic,
        format,
        slot: 0,
        offset,
        instance: 0,
    })
}

/// One patch shader's draw (everything but the vertices), made once per entry.
fn patch_draw(
    files: &Files,
    db: &Db,
    entry: &str,
    tiled: u32,
    values: &HashMap<&'static str, [f32; 4]>,
    atlas: &str,
    cache: &mut TextureFiles,
) -> Result<Draw, String> {
    let progs = db
        .entry_programs(entry)
        .ok_or_else(|| format!("{entry}: not in the database"))?;
    let main = |layout: &[u8; 16]| {
        let all: Vec<usize> = progs
            .iter()
            .copied()
            .filter(|&k| {
                db.programs
                    .get(k)
                    .is_some_and(|p| p.key_byte(0x30) == 0 && p.key_byte(0x34) == 0 && &p.layout() == layout)
            })
            .collect();
        let lit: Vec<usize> = all
            .iter()
            .copied()
            .filter(|&k| db.programs.get(k).is_some_and(|p| p.key_byte(0x38) == 0))
            .collect();
        lit.first().or(all.first()).copied()
    };
    let k = (if tiled == 0 { main(&PATCH_LAYOUT_NO_TILES) } else { None })
        .or_else(|| main(&PATCH_LAYOUT))
        .ok_or_else(|| format!("{entry}: no main-pass program for the patch layout"))?;
    let prog = db.programs.get(k).ok_or("program")?;
    let (vs, ps) = (db.vs.get(prog.vs).ok_or("vs")?, db.ps.get(prog.ps).ok_or("ps")?);
    let (vblock, block) = (db.param(vs.param), db.param(prog.block2));
    let extra = |records: Vec<query::Constant<'_>>| -> Vec<(u8, [f32; 4])> {
        records
            .iter()
            .filter_map(|c| {
                let n: String = c.name.iter().map(|&b| char::from(b)).collect();
                values.get(n.as_str()).map(|v| (c.register, *v))
            })
            .collect()
    };
    let mut ps_named = query::block_constants(block, false);
    ps_named.extend(query::block_constants(db.param(ps.param), false));
    let elements: Vec<Element> = material::vs_elements(&vs.tail)
        .iter()
        .filter_map(|e| patch_element(e.semantic).filter(|p| p.format == e.format && e.instance == 0))
        .collect();
    let state = RenderState::of(prog.state.0, prog.state.1);
    let ps_reflect = Arc::new(dxbc::reflect(&ps.code));
    let vs_reflect = Arc::new(dxbc::reflect(&vs.code));
    let records: HashMap<u32, String> = query::block_textures(block, false)
        .iter()
        .map(|t| (u32::from(t.sampler), t.name.iter().map(|&b| char::from(b)).collect()))
        .collect();
    let engine: HashMap<u32, String> = query::block_engine_textures(block, false)
        .iter()
        .map(|(n, s)| (u32::from(*s), n.iter().map(|&b| char::from(b)).collect()))
        .collect();
    let engine_key = |n: &str| match n {
        "heightmap" => Some(HEIGHTMAP.to_owned()),
        "maskTileMap" => Some(atlas.to_owned()),
        "dynamicMaskTileMap" | "dynamicMaskIndirectionMap" => Some(ZERO.to_owned()),
        _ => None,
    };
    let mut textures = Vec::new();
    for r in ps_reflect.resources.iter().filter(|r| r.kind == 2) {
        if let Some(t) = records.get(&r.point) {
            if material::texture(files, cache, t).is_some() {
                textures.push((r.point, t.clone()));
            }
        } else if let Some(key) = engine.get(&r.point).and_then(|n| engine_key(n)) {
            if key == atlas && material::texture(files, cache, &key).is_none() {
                continue;
            }
            textures.push((r.point, key));
        }
    }
    let vs_engine: HashMap<u32, String> = query::block_engine_textures(vblock, false)
        .iter()
        .map(|(n, s)| (u32::from(*s), n.iter().map(|&b| char::from(b)).collect()))
        .collect();
    let vs_textures = vs_reflect
        .resources
        .iter()
        .filter(|r| r.kind == 2)
        .filter_map(|r| Some((r.point, engine_key(vs_engine.get(&r.point)?)?)))
        .collect();
    Ok(Draw {
        part: entry.rsplit('/').next().unwrap_or(entry).to_owned(),
        entry: entry.to_owned(),
        program: k,
        vs: Arc::clone(&vs.code),
        ps: Arc::clone(&ps.code),
        vs_reflect,
        ps_reflect,
        elements,
        vertex_offset: 0,
        stride: PATCH_STRIDE,
        index_count: 0,
        first_index: 0,
        kind: 0,
        bones: None,
        material: query::block_constants(block, false)
            .iter()
            .map(|c| (c.register, c.values))
            .collect(),
        vs_extra: extra(query::block_constants(vblock, false)),
        ps_extra: extra(ps_named),
        textures,
        vs_textures,
        samplers: sampler_states(db.param(ps.param)),
        vs_samplers: sampler_states(vblock),
        class: material::draw_class(state.as_ref(), &ps.code),
        state,
        centre: None,
    })
}

/// The terrain of a level, ready to draw patch by patch.
#[derive(Debug, Clone)]
pub struct Terrain {
    /// The height field.
    pub hf: Heightfield,
    /// The visual terrain.
    pub visual: VisualTerrain,
    /// The atlas's width and height.
    pub atlas_size: [u32; 2],
    /// Each patch shader's draw (by index), or why it has none.
    pub draws: Vec<Result<Draw, String>>,
    /// The engine's textures: the height map and the empty mask.
    pub raw: HashMap<String, RawTexture>,
    /// The shared index list of a patch.
    pub indices: Arc<Vec<u8>>,
}

impl Terrain {
    /// Reads a level's terrain (`Terrains/<name>/<name>`): its visual terrain, height field and the patch shaders'
    /// programs in `db`.
    ///
    /// # Errors
    /// A file is missing or unreadable.
    pub fn load(files: &Files, db: &Db, name: &str, cache: &mut TextureFiles) -> Result<Self, String> {
        let base = format!("Terrains/{name}/{name}");
        let vt = files
            .read(&format!("{base}.Visual.res"))
            .ok_or_else(|| format!("{base}.Visual.res: not found"))?;
        let visual = parse_visual(&vt)?;
        let map = visual.maps.first().ok_or("visual terrain: no patch map")?;
        let hfb = files
            .read(&format!("{}.res", map.heightfield))
            .ok_or_else(|| format!("{}.res: not found", map.heightfield))?;
        let hf = bc2_formats::terrain::parse(&hfb).map_err(|e| e.to_string())?;
        let atlas = map.atlas.clone();
        let atlas_size =
            material::texture(files, cache, &atlas).map_or([1, 1], |b| [u32_at(b, 16).max(1), u32_at(b, 20).max(1)]);
        let values = engine_values(&hf);
        let draws = visual
            .shaders
            .iter()
            .map(|s| patch_draw(files, db, &s.name, s.tiled, &values, &atlas, cache))
            .collect();
        let q = (hf.size as usize / map.side().max(1)).max(1);
        let mut idx = Vec::with_capacity(q * q * 6);
        let row = q as u32 + 1;
        for z in 0..q as u32 {
            for x in 0..q as u32 {
                let i = z * row + x;
                // counter-clockwise seen from above (the renderer's front faces; patch programs cull the back)
                idx.extend([i, i + row, i + 1, i + 1, i + row, i + row + 1]);
            }
        }
        let mut raw = HashMap::new();
        raw.insert(HEIGHTMAP.to_owned(), heightmap(&hf));
        raw.insert(ZERO.to_owned(), zero());
        Ok(Self {
            hf,
            visual,
            atlas_size,
            draws,
            raw,
            indices: Arc::new(idx.iter().flat_map(|v| v.to_le_bytes()).collect()),
        })
    }

    /// Height samples per patch side.
    #[must_use]
    pub fn samples_per_patch(&self) -> usize {
        let side = self.visual.maps.first().map_or(1, PatchMap::side).max(1);
        (self.hf.size as usize / side).max(1)
    }

    /// The patch (`px` along x, `pz` along z) as a prepared mesh: its grid and mask coordinates, drawn with its
    /// shader; `None` outside the map or without a draw.
    #[must_use]
    pub fn patch(&self, px: usize, pz: usize) -> Option<Prepared> {
        let map = self.visual.maps.first()?;
        let side = map.side();
        if px >= side || pz >= side {
            return None;
        }
        let ratio = (map.shader_side / side.max(1)).max(1);
        let shader = *map.shader.get(pz * ratio * map.shader_side + px * ratio)?;
        let mut draw = self.draws.get(usize::from(shader))?.as_ref().ok()?.clone();
        let q = self.samples_per_patch();
        let tiles = map.tiles.get(pz * side + px).copied().unwrap_or_default();
        let [tile, border, per_row, ..] = map.header;
        let (aw, ah) = (self.atlas_size[0] as f32, self.atlas_size[1] as f32);
        let span = tile.saturating_sub(2 * border) as f32;
        let snorm = |v: f32| ((v.clamp(-1.0, 1.0) * 32767.0).round() as i16).to_le_bytes();
        let mut vb = Vec::with_capacity((q + 1) * (q + 1) * PATCH_STRIDE as usize);
        for j in 0..=q {
            for i in 0..=q {
                let gx = (px * q + i) as i16;
                let gz = (pz * q + j) as i16;
                vb.extend(gx.to_le_bytes());
                vb.extend(gz.to_le_bytes());
                let (fx, fz) = (i as f32 / q as f32, j as f32 / q as f32);
                for t in tiles {
                    let t = u32::from(t);
                    let (col, row) = (t % per_row.max(1), t / per_row.max(1));
                    let u = ((col * tile + border) as f32 + fx * span) / aw;
                    let v = ((row * tile + border) as f32 + fz * span) / ah;
                    vb.extend(snorm(u));
                    vb.extend(snorm(v));
                }
            }
        }
        draw.index_count = (q * q * 6) as u32;
        let n = self.hf.size as usize;
        let (cx, cz) = self.hf.cell();
        let x0 = f64::from(self.hf.min[0]) + (px * q) as f64 * cx;
        let z0 = f64::from(self.hf.min[2]) + (pz * q) as f64 * cz;
        let (mut ylo, mut yhi) = (f32::MAX, f32::MIN);
        for j in (pz * q)..=((pz + 1) * q).min(n - 1) {
            for i in (px * q)..=((px + 1) * q).min(n - 1) {
                if let Some(h) = self.hf.sample(i, j) {
                    ylo = ylo.min(h);
                    yhi = yhi.max(h);
                }
            }
        }
        let x1 = x0 + q as f64 * cx;
        let z1 = z0 + q as f64 * cz;
        Some(Prepared {
            mesh: draw.entry.clone(),
            draws: vec![draw],
            notes: Vec::new(),
            vertices: Arc::new(vb),
            indices: Arc::clone(&self.indices),
            wide: true,
            bounds: Some(([x0 as f32, ylo, z0 as f32], [x1 as f32, yhi, z1 as f32])),
        })
    }

    /// The patch under a world point.
    #[must_use]
    pub fn patch_at(&self, x: f64, z: f64) -> Option<(usize, usize)> {
        let q = self.samples_per_patch() as f64;
        let (cx, cz) = self.hf.cell();
        let px = ((x - f64::from(self.hf.min[0])) / (cx * q)).floor();
        let pz = ((z - f64::from(self.hf.min[2])) / (cz * q)).floor();
        (px >= 0.0 && pz >= 0.0).then_some((px as usize, pz as usize))
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

    #[test]
    fn a_minimal_visual_terrain_reads() {
        let mut d = Vec::new();
        let u = |d: &mut Vec<u8>, v: u32| d.extend(v.to_le_bytes());
        u(&mut d, 1); // one block
        for _ in 0..2 {
            u(&mut d, 0); // no strips
            u(&mut d, 0);
            u(&mut d, 0);
            u(&mut d, 0);
        }
        u(&mut d, 1); // one patch shader
        d.extend(b"T/a/b/PatchMaskShaders/X\0\0");
        u(&mut d, 2);
        u(&mut d, 1);
        u(&mut d, 0); // no undergrowth groups
        u(&mut d, 1); // one patch map
        for v in [-8.0f32, 0.0, -8.0, 8.0, 100.0, 8.0] {
            d.extend(v.to_le_bytes());
        }
        for v in [66, 1, 15, 7, 2] {
            u(&mut d, v);
        }
        d.extend([0u8; 8 * 4]);
        d.extend([8u8, 0, 0, 0].repeat(4));
        u(&mut d, 2);
        d.extend([0u8; 4]);
        d.extend(b"T/hf\0T/atlas\0");
        let vt = parse_visual(&d).expect("reads");
        assert_eq!(vt.shaders.first().map(|s| (s.layers, s.tiled)), Some((2, 1)));
        let m = vt.maps.first().expect("map");
        assert_eq!((m.side(), m.shader_side, m.atlas.as_str()), (2, 2, "T/atlas"));
        assert_eq!(m.flags.first(), Some(&[8, 0, 0, 0]));
        assert!(parse_visual(&d[..d.len() - 1]).is_err());
    }
}
