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
//! - the mask atlas (round R5d, TERR-VT-7): a cell's tiled mask k is tile `tiles[k]` (column `t % per row`, row
//!   `t / per row`, inside its one-pixel border), and the tile spans `flags[k]` x `flags[k]` cells (8 or 4) of the
//!   patch map, aligned to multiples of the flag: a cell reads the part of the tile its place in that block gives.
//!   Neighbouring cells of one tile name the same tile (DICE's `MP_009`: tile 31 over cells 24 - 31), so the masks run
//!   on across cells. Rounds R5 to GW1 stretched the whole tile over every cell: each cell repeated the block's
//!   mask, and WW2 shores drew ragged at every cell's edge.
//!
//! **Not yet the engine's** (`EDITOR_ARCHITECTURE.md` 3.11, the capture asked for): the grid is one vertex per height
//! sample (the game's terrain LOD is not known; the view's own: [`patch_lods`]); the dynamic (destruction) mask is
//! empty.

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
    /// Per cell, each mask's flag: 0 for no mask, else how many cells its atlas tile spans on a side (8 or 4; round
    /// R5d, TERR-VT-7).
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

/// One baked road or decal strip (`terrain.road_strips`' dicts): its entry, a range of its block's indices.
#[derive(Debug, Clone, PartialEq)]
pub struct Strip {
    /// Its shader entry (`Terrains/MP_005/Shaders/Road_Asphalt_01`).
    pub shader: String,
    /// A second pass's entry (`..._Zonly`), or empty.
    pub second: String,
    /// Its mask's name, or empty.
    pub mask: String,
    /// The first index.
    pub first: u32,
    /// Triangles.
    pub triangles: u32,
    /// The stored bounds (6 floats).
    pub bounds: [f32; 6],
    /// The layer (draw order among strips).
    pub layer: u32,
}

/// A road block: its strips, vertices (28 bytes each: position float3, texture coordinate float2, 4 x f16) and indices.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RoadBlock {
    /// The strips.
    pub strips: Vec<Strip>,
    /// The vertices.
    pub vertices: Vec<u8>,
    /// The indices (`u32`).
    pub indices: Vec<u32>,
}

/// Bytes per road vertex.
pub const ROAD_STRIDE: u32 = 28;

/// The strip vertex layout (`renderqa.STRIP_LAYOUT`).
pub const STRIP_LAYOUT: [u8; 16] = [
    0xbc, 0xd3, 0x77, 0x55, 0xdf, 0xa1, 0x71, 0x4a, 0x6f, 0x0d, 0x12, 0x6a, 0xed, 0x6a, 0xd4, 0xed,
];

/// A `VisualTerrain`'s road blocks, patch shaders and patch maps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct VisualTerrain {
    /// Each terrain block's full-detail roads, then each one's low-detail ones.
    pub roads: Vec<RoadBlock>,
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
    let mut out = VisualTerrain::default();
    for _ in 0..2 * n {
        let mut block = RoadBlock::default();
        for _ in 0..w.u32()? {
            let (shader, second, mask) = (w.string()?, w.string()?, w.string()?);
            let (first, triangles) = (w.u32()?, w.u32()?);
            let mut bounds = [0f32; 6];
            for b in &mut bounds {
                *b = f32_at(data, w.o);
                w.skip(4)?;
            }
            let layer = w.u32()?;
            block.strips.push(Strip {
                shader,
                second,
                mask,
                first,
                triangles,
                bounds,
                layer,
            });
        }
        let (v, b, i) = (w.u32()?, w.u32()?, w.u32()?);
        if b != ROAD_STRIDE * v {
            return Err(format!(
                "visual terrain: road vertices of {b} bytes for {v} (not 28 each)"
            ));
        }
        let at = w.o;
        w.skip(28 * v as usize)?;
        block.vertices = data.get(at..w.o).unwrap_or_default().to_vec();
        let at = w.o;
        w.skip(4 * i as usize)?;
        block.indices = (0..i as usize).map(|k| u32_at(data, at + 4 * k)).collect();
        out.roads.push(block);
    }
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

/// The engine's height map: the height field's stored 16-bit values v as `R32_FLOAT` v / 65535 (what `R16_UNORM`
/// reads; height = min y + value * (max y - min y), the posScale / posOffset below). Held as floats so the C++
/// harness, whose engine textures are float, filters the very same texels (the engine's own format is the capture's
/// to tell).
#[must_use]
pub fn heightmap(hf: &Heightfield) -> RawTexture {
    RawTexture {
        width: hf.size,
        height: hf.size,
        format: 41,
        texel: 4,
        data: hf
            .raw
            .iter()
            .flat_map(|&v| (f32::from(v) / 65535.0).to_le_bytes())
            .collect(),
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
    // round R5d (spec RNDR-TERR-5): the height map's sample (0..1) times heightmapWorldScaleY is the world height, as
    // posScale.y places the vertex - the records store a placeholder 500 (every database read), which is the range of
    // Vietnam's and Isla Inocentes' height fields but half of Wake Island's 1,000 m: read as 500, Wake's beach and sea
    // floor (BF1943's sea-floor layers shade by height against the water at 215 m) drew black
    v.insert("heightmapWorldScaleY", [range, 0.0, 0.0, 0.0]);
    // the water's height-map look-up (DICE's NAM_HaLongWaterShader, OPFOR's Wake water): uv = (x + 0.5) * scale +
    // offset, the texel centres at the samples' world positions
    let (sx, sz) = (1.0 / (cx * n), 1.0 / (cz * n));
    v.insert("heightmapWorldScaleXZ", [sx, sz, 0.0, 0.0]);
    v.insert(
        "heightmapWorldOffsetXZ",
        [
            0.5 / n - (hf.min[0] + 0.5) * sx,
            0.5 / n - (hf.min[2] + 0.5) * sz,
            0.0,
            0.0,
        ],
    );
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
/// What a terrain draw is drawn on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum On {
    /// A patch whose cell has this many tiled masks.
    Patch(u32),
    /// A road or decal strip.
    Strip,
}

/// One terrain shader's draw (everything but the vertices), made once per entry: a patch shader's
/// (`renderqa.prepare_patch`) or a strip's (`renderqa.prepare_strip`, with the level's own engine values).
#[allow(clippy::too_many_arguments, reason = "the draw's inputs, as renderqa's")]
fn terrain_draw(
    files: &Files,
    db: &Db,
    entry: &str,
    on: On,
    values: &HashMap<&'static str, [f32; 4]>,
    atlas: &str,
    envmap: &str,
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
    let k = match on {
        On::Patch(tiled) => (if tiled == 0 { main(&PATCH_LAYOUT_NO_TILES) } else { None })
            .or_else(|| main(&PATCH_LAYOUT))
            .ok_or_else(|| format!("{entry}: no main-pass program for the patch layout"))?,
        On::Strip => {
            main(&STRIP_LAYOUT).ok_or_else(|| format!("{entry}: no main-pass program for the strip layout"))?
        }
    };
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
    let elements: Vec<Element> = match on {
        On::Patch(_) => material::vs_elements(&vs.tail)
            .iter()
            .filter_map(|e| patch_element(e.semantic).filter(|p| p.format == e.format && e.instance == 0))
            .collect(),
        On::Strip => material::vs_elements(&vs.tail)
            .into_iter()
            .filter(|e| e.slot == 0)
            .collect(),
    };
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
        } else if r.name == "texture_outdoorLightReflectionEnvmap" {
            if material::texture(files, cache, envmap).is_some() {
                textures.push((r.point, envmap.to_owned()));
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
        stride: if on == On::Strip { ROAD_STRIDE } else { PATCH_STRIDE },
        index_count: 0,
        first_index: 0,
        kind: 0,
        bones: None,
        material: query::block_constants(block, false)
            .iter()
            .map(|c| (c.register, c.values))
            .collect(),
        named: Vec::new(),
        vs_extra: extra(query::block_constants(vblock, false)),
        ps_extra: extra(ps_named),
        textures,
        vs_textures,
        samplers: sampler_states(db.param(ps.param)),
        vs_samplers: sampler_states(vblock),
        class: material::draw_class(state.as_ref(), &ps.code),
        state,
        centre: None,
        bounds: None,
    })
}

/// A patch's index lists (round R5d, spec RNDR-VIEW-2): the full grid of `q` x `q` cells (counter-clockwise seen
/// from above, the renderer's front faces; patch programs cull the back), then for each coarser step `s` = 2, 4, ...
/// up to `q / 2` the grid taken every `s`th sample inside a ring whose outer edges keep every sample - so a patch of
/// any level meets a neighbour of any other without a crack (the shared edge is the full-detail one on both sides).
/// Returns the indices and each coarser level's (first index, index count).
#[must_use]
pub fn patch_lods(q: usize) -> (Vec<u32>, Vec<(u32, u32)>) {
    let row = q as u32 + 1;
    let v = |i: usize, j: usize| (j as u32) * row + i as u32;
    let mut idx = Vec::with_capacity(q * q * 6 * 2);
    for z in 0..q as u32 {
        for x in 0..q as u32 {
            let i = z * row + x;
            idx.extend([i, i + row, i + 1, i + 1, i + row, i + row + 1]);
        }
    }
    // a triangle wound as the full grid's (the cross product of its edges in x, z negative)
    let tri = |out: &mut Vec<u32>, a: (usize, usize), b: (usize, usize), c: (usize, usize)| {
        let cross = (b.0 as f64 - a.0 as f64) * (c.1 as f64 - a.1 as f64)
            - (b.1 as f64 - a.1 as f64) * (c.0 as f64 - a.0 as f64);
        if cross.abs() < 1e-9 {
            return;
        }
        if cross < 0.0 {
            out.extend([v(a.0, a.1), v(b.0, b.1), v(c.0, c.1)]);
        } else {
            out.extend([v(a.0, a.1), v(c.0, c.1), v(b.0, b.1)]);
        }
    };
    let mut lods = Vec::new();
    let mut s = 2;
    while s <= q / 2 && q >= 4 {
        let first = idx.len() as u32;
        // the inside, every s-th sample
        let mut x = s;
        while x + s <= q - s {
            let mut z = s;
            while z + s <= q - s {
                tri(&mut idx, (x, z), (x, z + s), (x + s, z));
                tri(&mut idx, (x + s, z), (x, z + s), (x + s, z + s));
                z += s;
            }
            x += s;
        }
        // the ring: each side zipped between its full-detail edge and the inside's border (one point when s = q / 2)
        let inner: Vec<usize> = (0..=((q - 2 * s) / s)).map(|k| s + k * s).collect();
        for side in 0..4 {
            // a point on the side at `t` along it, `d` in from it
            let at = |t: usize, d: usize| -> (usize, usize) {
                match side {
                    0 => (t, d),         // z = 0
                    1 => (q - d, t),     // x = q
                    2 => (q - t, q - d), // z = q
                    _ => (d, q - t),     // x = 0
                }
            };
            let (mut a, mut b) = (0usize, 0usize);
            while a < q || b + 1 < inner.len() {
                let next_outer = if a < q { Some(a + 1) } else { None };
                let next_inner = inner.get(b + 1).copied();
                let take_outer = match (next_outer, next_inner) {
                    (Some(o), Some(i)) => o <= i,
                    (Some(_), None) => true,
                    _ => false,
                };
                let here = inner.get(b).copied().unwrap_or(s);
                if take_outer {
                    tri(&mut idx, at(a, 0), at(a + 1, 0), at(here, s));
                    a += 1;
                } else {
                    tri(&mut idx, at(a, 0), at(next_inner.unwrap_or(here), s), at(here, s));
                    b += 1;
                }
            }
        }
        lods.push((first, idx.len() as u32 - first));
        s *= 2;
    }
    (idx, lods)
}

/// The step a patch is drawn with (round R5d): every sample within [`LOD_DISTANCE`] of the camera, then every 2nd,
/// 4th ... sample at each doubling of the distance, up to `q / 2`; the index into [`Prepared::lods`] plus one (0: the
/// full grid). The editor's rule, not DICE's: the game's terrain LOD (its distances and morphing) is the capture's.
#[must_use]
pub fn lod_for(distance: f64, levels: usize) -> usize {
    let mut k = 0;
    let mut d = LOD_DISTANCE;
    while k < levels && distance >= d {
        k += 1;
        d *= 2.0;
    }
    k
}

/// The distance, metres, out to which a patch is drawn in full ([`lod_for`]).
pub const LOD_DISTANCE: f64 = 200.0;

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
    /// The shared index list of a patch: the full grid, then its coarser levels ([`patch_lods`]).
    pub indices: Arc<Vec<u8>>,
    /// The coarser levels' index ranges in [`Terrain::indices`].
    pub lods: Vec<(u32, u32)>,
    /// The baked road and decal strips of each terrain block's full-detail road block, one prepared mesh per block
    /// (its strips in layer order; a strip has no centre, so blended strips keep that order, after the other blended
    /// draws).
    pub strips: Vec<Prepared>,
}

impl Terrain {
    /// Reads a level's terrain (`Terrains/<name>/<name>`): its visual terrain, height field and the patch shaders'
    /// programs in `db`.
    ///
    /// # Errors
    /// A file is missing or unreadable.
    pub fn load(files: &Files, db: &Db, name: &str, envmap: &str, cache: &mut TextureFiles) -> Result<Self, String> {
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
            .map(|s| terrain_draw(files, db, &s.name, On::Patch(s.tiled), &values, &atlas, envmap, cache))
            .collect();
        let mut strip_draws: HashMap<String, Result<Draw, String>> = HashMap::new();
        let mut strips = Vec::new();
        for block in visual.roads.iter().take(visual.maps.len()) {
            let mut p = Prepared {
                mesh: format!("{base} roads"),
                vertices: Arc::new(block.vertices.clone()),
                indices: Arc::new(block.indices.iter().flat_map(|v| v.to_le_bytes()).collect()),
                wide: true,
                ..Prepared::default()
            };
            let mut order: Vec<&Strip> = block.strips.iter().collect();
            order.sort_by_key(|s| s.layer);
            for st in order {
                let made = strip_draws
                    .entry(st.shader.to_lowercase())
                    .or_insert_with(|| terrain_draw(files, db, &st.shader, On::Strip, &values, &atlas, envmap, cache));
                match made {
                    Ok(d) => {
                        let mut d = d.clone();
                        d.first_index = st.first;
                        d.index_count = st.triangles * 3;
                        p.draws.push(d);
                    }
                    Err(e) => p.notes.push(format!("strip: {e}")),
                }
            }
            p.notes.sort();
            p.notes.dedup();
            strips.push(p);
        }
        let q = (hf.size as usize / map.side().max(1)).max(1);
        let (idx, lods) = patch_lods(q);
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
            lods,
            strips,
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
        let flags = map.flags.get(pz * side + px).copied().unwrap_or_default();
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
                for (t, f) in tiles.into_iter().zip(flags) {
                    let t = u32::from(t);
                    let (col, row) = (t % per_row.max(1), t / per_row.max(1));
                    // round R5d (spec TERR-VT-7): a mask tile spans `flag` x `flag` cells (8 or 4), aligned to
                    // multiples of the flag in the patch map; this cell reads its part of the tile
                    let f = usize::from(f.max(1));
                    let (ox, oz) = ((px % f) as f32, (pz % f) as f32);
                    let k = f as f32;
                    let u = ((col * tile + border) as f32 + (ox + fx) / k * span) / aw;
                    let v = ((row * tile + border) as f32 + (oz + fz) / k * span) / ah;
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
            shader_set: String::new(),
            lods: self.lods.clone(),
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

    // spec: RNDR-VIEW-2
    #[test]
    fn patch_levels_cover_the_patch_and_keep_full_edges() {
        for q in [8usize, 16, 32] {
            let (idx, lods) = patch_lods(q);
            assert_eq!(lods.len(), (q / 2).trailing_zeros() as usize, "q {q}");
            let row = q + 1;
            let xz = |v: u32| ((v as usize % row) as f64, (v as usize / row) as f64);
            let mut ranges = vec![(0u32, (q * q * 6) as u32)];
            ranges.extend(lods.iter().copied());
            for (k, (first, count)) in ranges.into_iter().enumerate() {
                let tris = &idx[first as usize..(first + count) as usize];
                let mut area = 0.0;
                let mut edges = std::collections::HashSet::new();
                for t in tris.chunks(3) {
                    let (a, b, c) = (xz(t[0]), xz(t[1]), xz(t[2]));
                    let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
                    assert!(cross < 0.0, "q {q} level {k}: a triangle wound the other way");
                    area += -cross / 2.0;
                    for (u, v) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                        edges.insert((u.min(v), u.max(v)));
                    }
                }
                assert!((area - (q * q) as f64).abs() < 1e-6, "q {q} level {k}: area {area}");
                // every unit segment of the patch's border is an edge: no crack against any neighbour
                for t in 0..q as u32 {
                    let r = row as u32;
                    let q32 = q as u32;
                    for (u, v) in [
                        (t, t + 1),
                        (q32 * r + t, q32 * r + t + 1),
                        (t * r, (t + 1) * r),
                        (t * r + q32, (t + 1) * r + q32),
                    ] {
                        assert!(edges.contains(&(u.min(v), u.max(v))), "q {q} level {k}: border {u}-{v}");
                    }
                }
                if k > 0 {
                    assert!(count < (q * q * 6) as u32 / 2, "q {q} level {k}: {count} indices");
                }
            }
        }
        assert_eq!(lod_for(10.0, 3), 0);
        assert_eq!(lod_for(LOD_DISTANCE, 3), 1);
        assert_eq!(lod_for(LOD_DISTANCE * 4.0, 3), 3);
        assert_eq!(lod_for(1e6, 3), 3);
    }

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
