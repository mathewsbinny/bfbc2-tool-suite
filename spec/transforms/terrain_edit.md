# Terrain editing: what a sculpt, a paint or a new terrain must write

What the map editor's terrain tools (round E5: sculpt and paint; round E7: Create Level from a height map) must
write for the game to draw, collide with and shade an edited terrain, and what the game computes itself. Written by
round SPG2 (2026-11-22) before any terrain writer exists: most rules are what DICE's data holds (measured by
`spec/probes/terrain.py` over the 30 DICE terrains of the game copy) and what that implies for a writer; the
formats are [formats/terrain.md](../formats/terrain.md), the drawing [engine/rendering.md](../engine/rendering.md)
section 4, the baked colour / occlusion map [engine/lighting.md](../engine/lighting.md) section 1, the destruction
layer [engine/destruction.md](../engine/destruction.md) section 4, undergrowth [engine/vegetation.md](../engine/vegetation.md),
roads and terrain decals [engine/decals.md](../engine/decals.md).

The corpus: the DICE bundles of the game copy (`game/**/levels/*/*.fbrb` dated before 2013), each resource once: 30
terrains (30 main height-field blocks, 288 sub-terrain blocks, 30 visual terrains, 30 terrain partitions, 74 level
partitions). Counted 2026-11-22.

## 1. Where the terrain lives, and what the game reads

### TEDIT-SRC-1: The terrain partition is DICE's editor source; the game draws the baked resources

Status: **verified** · Variants: PC

A terrain's partition `Terrains/<T>/<T>.dbx` holds DICE's editor sources, not what the game draws: its paint layers
are `TerrainFullDecal` instances whose masks are editor PNG files (`MaskTextures` items naming
`Terrains\<T>\MaskTextures\DecalMask-<layer>+0+0.png`), its roads `TerrainSplineDecal`s naming a road spline
partition, its height `TerrainHeightfieldData.Files` editor `.raw` files. None of what those decals name ships: 314
of 314 full-decal materials, 416 of 416 spline-decal materials and splines, 40 of 40 quad-decal materials name a
partition no bundle holds. The game draws the baked resources of the terrain bundle (`terrain-00.fbrb`: TERR-NAME-1)
and the patch shaders of the level's databases. So an edit is written into the resources; the partition's decals may
stay as DICE has them (OPFOR's Wake Island keeps BF1943's the same way).

[test: core/tests/test_spec_probes.py::terrain_partitions]
[data: spec/probes/terrain.py, 2026-11-22: 30 terrain partitions: TerrainData 30, TerrainHeightfieldData 53, TerrainHeightfieldGridCellInfo 43, TerrainFullDecal 314 (MaskTextureType TmttStandard 194, TmttArea 69, TmttNone 51), TerrainGroupDecal 80, TerrainSplineDecal 416, TerrainQuadDecal 40; every decal material / spline reference in no bundle]
[doc: opfor:src/tools/ww2/wake_install.py:473-481]

### TEDIT-SRC-2: What an edit writes, and what the game makes itself

Status: **inferred** · Variants: PC

| Edit | Writes | Computed by the game, not written |
|---|---|---|
| sculpt | the height field's heights (main block; the sub-terrain ring where the edit reaches it) and their derived parts (TEDIT-HF-2, TEDIT-HF-3); the colour / occlusion map's texels under the edit (LIGHT-BAKE-6); the physics material map if materials move (TEDIT-MM-1) | the patch normals (the pixel shader takes them from neighbouring height samples, RNDR-TERR-2); `heightmapWorldScaleY` and the other named terrain constants (RNDR-TERR-5, TEDIT-HF-4); the destruction mask (DSTR-TERR-2) |
| paint | the patch map cells (shader index, tiles, flags: TEDIT-VT-1), the mask-tile atlas's tiles (TEDIT-ATL-1), a patch shader entry in every Dx database for a layer combination the level lacks (TEDIT-SH-1), the undergrowth that patch shader names (VEG-UG-1) | the rule layers (base, slope, sea floor, destruction: computed in the shader, TERR-VT-3) |
| new terrain (E7) | all of the above, a new main block of 1,024 or 2,048 samples (TEDIT-HF-1), a material map, and the level's registration | as above |

What the game does with a stale part is not known: no edited terrain has been loaded in game.

[inferred: from RNDR-TERR-2 (normals from height samples), RNDR-TERR-5 (the engine writes the height scale), DSTR-TERR-2 (the engine writes the destruction mask), TERR-VT-3 (rule layers have no tile) and TEDIT-SRC-1 (the resources are what is drawn)]

## 2. The height field

### TEDIT-HF-1: DICE's main blocks are 1,024 or 2,048 samples of 1 m, starting at height 0

Status: **verified** · Variants: PC

Every DICE main block is n = 1,024 (11) or 2,048 (19) samples per side at 1 m spacing, with min y = 0. A new terrain
(E7) takes one of these sizes (formats/terrain.md: no other main size is known to load) and its heights from 0
upwards.

[test: core/tests/test_spec_probes.py::height_fields]
[data: spec/probes/terrain.py, 2026-11-22: 30 main blocks: n 1,024 x 11, 2,048 x 19; grid spacing 1 m on 30; min y 0 on 30]

### TEDIT-HF-2: The height range is per level; DICE's are 395 - 1,700 m

Status: **verified** · Variants: PC

The main block's height range (max y - min y, TERR-HF-2's scale) is chosen per terrain, not fixed: 500 m on 16
terrains, 1,000 m on 8 (MP_006, MP_007, MP_008, SP_008, SP_008_B, SP_009, SP_01, SP_02), 750 m on 2 (SP_003_B,
SP_03), 1,700 m (SP_004_B), 1,500 m (SP_04), 650 m (BC1_Harvest_Day) and 395 m (BC1_Oasis). The editor partition's
`TerrainHeightfieldData.SizeY` holds the same values (500 x 28, 1,000 x 12, 1,700 x 3, 1,500 x 3, 650 x 3, 750 x 2
over its 53 instances). A sculpt that raises ground above the range must widen it (and rescale every u16 height) or
clamp: widening changes no other resource (TEDIT-HF-4).

[test: core/tests/test_spec_probes.py::height_fields]
[data: spec/probes/terrain.py, 2026-11-22: per main block (terrain, n, range) as listed; TerrainHeightfieldData.SizeY counts as listed]

### TEDIT-HF-3: The derived parts of a height field are rewritten from the heights

Status: **inferred** · Variants: PC

After the heights change, a sub-terrain block's min / max quadtree (TERR-HF-5: per 16-sample cell, root first) must
be recomputed from its heights, since it is fully decoded and a stale one would bound the old ground. The main
block's tail (TERR-HF-4: n offsets, the first section, the fixed part) is not decoded: a writer that changes heights
keeps it as it is (the converters do the same) and the game is not known to care. Until the tail is decoded a sculpt
is a risk to test in game first; E7's new terrain cannot write one at all (formats/terrain.md Open questions).

[inferred: TERR-HF-4 / TERR-HF-5's grammars; the console and BF1943 converters keep the tails (TERR-HF-7, TERR-HF-8)]

### TEDIT-HF-4: The height scale the patch shaders use is the engine's, not the database's

Status: **inferred** · Variants: PC

The patch shaders' `heightmapWorldScaleY` and the vertex shaders' `posScale.y` are written by the engine from the
height field's range at draw time; the databases hold a placeholder 500 (RNDR-TERR-5). So widening a terrain's
range (TEDIT-HF-2) needs no shader database change, and a donor level's patch shaders work on a terrain of another
range: Wake Island (1,000 m) draws with patch shaders cloned from Isla Inocentes (500 m).

[inferred: RNDR-TERR-5; pcbuild.ENGINE_NAMED lists the constant among those the engine writes by name]
[code: bfbc2-shader-tools/pcbuild.py:47-52]
[data: OPFOR's Wake Island install, 2026-11-07 onward: its patch shaders cloned from mp_009cq (range 500 m) draw its 1,000 m terrain; spec/probes/terrain.py: MP_009 500 m]

## 3. Painting: the patch map, the atlas and the patch shaders

### TEDIT-VT-1: A painted cell writes its shader index, its tiles and its flags together

Status: **verified** · Variants: PC

Each patch-map cell (TERR-VT-4) holds the index of its patch shader (into the visual terrain's patch shader list:
318 of 318 maps index inside it), four mask-tile numbers and four flags. A cell's non-zero flags equal its patch
shader's tiled count on 358,282 of 367,872 cells; on the other 9,590 (16 terrains, all on main blocks) the patch
shader has no tiled layer and the cell keeps exactly one non-zero flag, a leftover the rule-only shader never reads.
A painter must keep the first: a cell whose shader reads more tiles than it names reads an empty slot. Main blocks'
maps have W = 64 or 128 cells a side (a cell is 16 m of a 1,024 or 2,048 m terrain); sub-terrain maps 4, 8 or 16 and
carry no tiles (one exception: 139 flags of 4 on W = 4 maps).

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: (W, non-zero flags = tiled count): (128, yes) 305,154, (128, no) 6,142, (64, yes) 41,608, (64, no) 3,448, (16 / 8 / 4, yes) 3,072 / 5,376 / 3,072; every mismatch a rule-only shader (tiled 0) with 1 flag (MP_002 343, SP_009 4,804, NAM_MP_002R 3,000 ...)]

### TEDIT-ATL-1: One mask-tile atlas per terrain, laid out as its patch maps' header says

Status: **verified** · Variants: PC

A visual terrain names one mask-tile atlas (30 of 30), an L8 texture (format 10, one mip) of 1,024 x 512 (13
terrains), 1,024 x 1,024 (12) or 2,048 x 1,024 (5); its patch maps' header holds the atlas size in 64-pixel tiles
minus one (15 / 7, 15 / 15, 31 / 15) on 30 of 30. A tile is 66 pixels with a 1-pixel border (header 66, 1). A paint
that needs more tiles than the atlas holds must grow the atlas to another of DICE's layouts and rewrite every patch
map's header, and then the level's patch shaders must be built for that layout (TERR-VT-5, TEDIT-SH-2).

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: 30 visual terrains, 1 atlas each; atlas (size, format, mips) against the header: 2048 x 1024 fmt 10 1 mip / (31, 15) x 5, 1024 x 512 / (15, 7) x 13, 1024 x 1024 / (15, 15) x 12, each = size / 64 - 1]

### TEDIT-ATL-2: A tile covers a block of flag x flag cells; DICE's flags are 2 - 64

Status: **verified** · Variants: PC

A mask's flag is the side, in cells, of the block its tile covers (TERR-VT-7); over DICE's terrains the flags are 2
(5,367), 4 (11,712), 8 (56,352), 16 (13,750), 32 (4,864) and 64 (2,558), not only 4 and 8. On 22 of the 30 terrains
every tile lies in one block aligned to multiples of its flag; on the other 8 one tile each is named by several blocks
(SP_009: 1,170 blocks at flag 2; MP_006 19; NAM_MP_002R 42; BC1_Oasis 14; MP_001, MP_003, SP_008, NAM_MP_006 4 to 8):
a shared tile, presumably uniform. A painter writes each block's tile once and keeps the blocks aligned; a uniform
block may share one tile.

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py and a per-terrain count (SPG2 scratch), 2026-11-22: 94,603 tiled masks, 79,973 in their tile's one block; per terrain the flags and the one shared tile as listed]
[code: rust/bc2-render/src/terrain.rs:800-811]

### TEDIT-SH-1: A painted layer combination needs a patch shader entry in every database

Status: **inferred** · Variants: PC

A cell draws with the patch shader its index names, a `PatchMaskShaders/<layers>` entry of each of the level's Dx9,
Dx10 and Dx11 databases (RNDR-TERR-1); its layers' order fixes which tile slot each tiled layer reads. A paint that
makes a combination the terrain has no patch shader for needs a new entry: DICE's terrains carry 7 - 109 patch
shaders (most 50), of 1 - 8 layers (4 on 424, 3 on 344), 0 - 4 of them tiled. The suite has no Frostbite shader
compiler: a new entry is a clone of an existing one with the same tiled layers, from the same level or a donor that
satisfies TEDIT-SH-2, its textures retargeted (OPFOR's Wake Island patches).

[data: spec/probes/terrain.py, 2026-11-22: patch shaders per terrain 7 - 109; layer counts 1: 48, 2: 194, 3: 344, 4: 424, 5: 328, 6: 152, 7: 24, 8: 1; tiled counts 0: 177, 1: 410, 2: 478, 3: 321, 4: 129]
[inferred: RNDR-TERR-1 (the entry per cell) and SHDB-LVL-1 (a level draws through its own databases only)]
[code: opfor:src/tools/ww2/wake_shaders.py:226-245]

### TEDIT-SH-2: A donor patch shader must match the tiled layers and the atlas layout

Status: **implemented** · Variants: PC

A patch shader cloned from another level must have the same number of tiled layers, the same first-layer tiled-ness
and the same tiled order as the patch it replaces, and come from a level whose mask atlas has the same layout
(TERR-VT-5: an Oasis donor, 2,048 x 1,024, drew Wake Island's tiled cells black; MP_009CQ's 1,024 x 512 works). The
rule layers are matched as far as the donor has them; the donor's own level textures (its occlusion map, mud mask,
normal maps, caustics) must all be replaced by the target's. Dx10 / Dx11 patch programs sample normal maps the Dx9
ones do not, so the textures needed are collected from every database.

[code: opfor:src/tools/ww2/wake_shaders.py:226-269]
[code: opfor:src/tools/ww2/wake_shaders.py:408-410]
[doc: opfor:src/tools/ww2/fidelity_checks.py:317]
[test: opfor:src/tools/tests/test_ww2_terrain_patches.py::test_dx9_donor_textures_never_another_level]

### TEDIT-SH-3: Rule-only patches need both patch layouts

Status: **implemented** · Variants: PC

The engine draws a patch on one of two vertex layouts per renderer: the grid point with the patch's mask-atlas
coordinates (a second stream), or the grid point alone. DICE compiles the second beside the first for patch shaders
whose layers are all rule layers (every BFBC2 level's rule-only patches have both). A new or translated rule-only
patch shader must have programs for both layouts (OPFOR's `PATCH_LAYOUTS`; Dx9's are other GUIDs than Dx10 / 11's).

[code: opfor:src/tools/ww2/wake_shaders.py:273-282]
[code: opfor:src/tools/ww2/wake_shaders.py:290-293]
[test: opfor:src/tools/tests/test_ww2_terrain_patches.py::test_patch_layouts_and_tiles]

### TEDIT-MM-1: The physics material map cannot be painted yet

Status: **inferred** · Variants: PC

Which physics material a terrain cell has (footsteps, impacts, crater size: DSTR-TERR-3) is the `TerrainMaterialMap`
(TERR-MM-1: 2,048 or 4,096 wide, 32 or 128 blocks, a 256-entry table per block, the names list), whose per-block data
is not decoded. Until it is, a paint changes the look and not the material: painted asphalt still sounds like the
grass it was.

[inferred: TERR-MM-1's open encoding (formats/terrain.md Open questions)]

## 4. The rest of the terrain bundle

### TEDIT-BLK-1: Main block, then a ring of 12 per ring

Status: **verified** · Variants: PC

A visual terrain has N blocks: 1 (14 terrains, no sub-terrains) or 1 + 12k (13: 12 terrains, 25: 1, 37: 2, 49: 1),
each with its road blocks and its patch map in the same order; patch maps name their block's height field
(`Terrains/...`, 318 of 318) and the atlas. A new terrain (E7) without a sub-terrain ring writes N = 1, as 14 of
DICE's do.

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: blocks per terrain {1: 14, 13: 12, 25: 1, 37: 2, 49: 1}; 318 patch maps]

### TEDIT-OCC-1: The colour / occlusion map under an edit is re-baked by ratio

Status: **inferred** · Variants: PC

A sculpt or a paint changes what the terrain's colour / occlusion map (LIGHT-BAKE-2) should hold under it; DICE
painted most of that map by hand, so the editor scales only the edited texels by the ratio of new to old computed
sky visibility and leaves the rest byte for byte (LIGHT-BAKE-6).

[inferred: LIGHT-BAKE-2 and LIGHT-BAKE-6]

## Open questions

- The main block's tail (TERR-HF-4): what it holds and whether the game checks it after a sculpt; needed for E7's
  new terrain.
- What reads the height field for collision and for the AI (Havok's heightfield shape, the navigation sampler):
  whether a sculpt also needs the level's collision or path data rebuilt (navigation: yes, as for any geometry).
- The material map's encoding (TEDIT-MM-1).
- Whether a patch map's shared tile (TEDIT-ATL-2) is always a uniform mask.
- Whether a cell's leftover flag on a rule-only patch (TEDIT-VT-1) is read by anything.
- Nothing here has been loaded in game: every rule a writer relies on must be checked by a playtest first.
