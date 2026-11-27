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
[doc: opfor:src/tools/ww2/wake_install.py:390-398]

### TEDIT-SRC-2: What an edit writes, and what the game makes itself

Status: **inferred** · Variants: PC

| Edit | Writes | Computed by the game, not written |
|---|---|---|
| sculpt | the height field's heights (main block; the sub-terrain ring where the edit reaches it) and their derived parts (TEDIT-HF-2, TEDIT-HF-3); the colour / occlusion map's texels under the edit (LIGHT-BAKE-6); the physics material map if materials move (TEDIT-MM-1); the min / max trees and the lower envelope (TEDIT-HF-3) | the patch normals (the pixel shader takes them from neighbouring height samples, RNDR-TERR-2); `heightmapWorldScaleY` and the other named terrain constants (RNDR-TERR-5, TEDIT-HF-4); the destruction mask (DSTR-TERR-2) |
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

Round E5-rest widens it (the editor's "Widen range", the log's `terrain-range` operation): every stored height scaled
by old / new range so the ground stays where it was, the lower envelope's values floored by the same ratio, the tree
and envelope rebuilt over the whole block, a ring's blocks rescaled with it (TEDIT-RING-1); min y stays 0. On DICE's 7
single-ring terrains of `Dist\win32` a range of 1.5 times DICE's keeps every sample within 0.0038 - 0.0076 m (less than
one stored unit of the new range) and reads back (XVAL-TERR-2).

[test: core/tests/test_spec_probes.py::height_fields]
[test: rust/bc2-levelwrite/tests/terrain_layers.rs::dice_rings_stitched_after_a_sculpt]
[code: rust/bc2-levelwrite/src/terrainedit.rs:47-64]
[data: spec/probes/terrain.py, 2026-11-22: per main block (terrain, n, range) as listed; TerrainHeightfieldData.SizeY counts as listed]

### TEDIT-HF-3: The derived parts of a height field are rewritten from the heights

Status: **verified** · Variants: PC

After the heights change, every part of a block that is computed from them is rewritten:

- a sub-terrain block's min / max quadtree (TERR-HF-5) and the main block's min / max tree (TERR-HF-10), recomputed:
  both are fully decoded, and the main block's recomputes byte for byte on DICE's 30 terrains;
- the main block's lower envelope (TERR-HF-11), lowered wherever the new heights dip under it: DICE's own generator is
  not reproduced, so a writer keeps DICE's grids where they still lie under the ground and lowers the rest (a new
  block, E7, writes grids built to lie under its heights); since DICE's envelopes already poke through on 15 of 30
  terrains, a surface under the ground is what the data asks, not an exact copy;
- the depth map (TERR-HF-9) is not derived: a sculpt keeps it; a new block writes one (all 255, presumably full
  depth, as most of DICE's), run-length coded with maximal runs, padded to 16 bytes from the heights' start, with A
  (header 0x2D) the length of the offsets, runs and padding.

With these an E7 writer can write a whole main block of 1,024 or 2,048 samples; what the game does with a stale or
synthetic envelope is not known until a playtest.

Round E5 implements it (`bc2_formats::terrain_codec`, shared with TG1's generator): the main block decodes and encodes
byte for byte on DICE's 30 terrains and the computed quadtree equals the stored one on all 288 sub-terrain blocks; an
edit recomputes the tree's nodes and lowers the envelope's vertices over the edited region (one sample beyond it), each
vertex at or under every sample of the grid squares it is a corner of; nothing edited writes DICE's bytes. On MP_005
and SP_01 a raised hill and a dug pit write blocks whose edited samples all lie over the envelope.

[test: core/tests/test_spec_probes.py::height_field_tails]
[test: rust/bc2-formats/tests/terrain_corpus.rs::dice_terrains_round_trip]
[test: rust/bc2-levelwrite/tests/terrain_edits.rs::terrain_resources_written_from_strokes]
[code: rust/bc2-formats/src/terrain_codec.rs:1-40]
[code: rust/bc2-levelwrite/src/terrainedit.rs:1-60]
[inferred: TERR-HF-4 - TERR-HF-11's grammars; the console and BF1943 converters keep the tails as they are (TERR-HF-7, TERR-HF-8)]

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
A painter must keep the first: a cell whose shader reads more tiles than it names reads an empty slot. Round
E5-rest's visual paint writes every cell it rewrites so (`bc2_levelwrite::terrainpaint::paint`; XVAL-TERR-3 checks it). Main blocks'
maps have W = 64 or 128 cells a side (a cell is 16 m of a 1,024 or 2,048 m terrain); sub-terrain maps 4, 8 or 16 and
carry no tiles (one exception: 139 flags of 4 on W = 4 maps).

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: (W, non-zero flags = tiled count): (128, yes) 305,154, (128, no) 6,142, (64, yes) 41,608, (64, no) 3,448, (16 / 8 / 4, yes) 3,072 / 5,376 / 3,072; every mismatch a rule-only shader (tiled 0) with 1 flag (MP_002 343, SP_009 4,804, NAM_MP_002R 3,000 ...)]
[test: rust/bc2-levelwrite/tests/terrain_layers.rs::mp_005_paint_written]

### TEDIT-VT-2: A shader's layers come from its name; a cell's tile slots are its tiled layers in that order

Status: **verified** · Variants: PC

A patch shader's name lists its layers (`.../PatchMaskShaders/Dunes_Asphalt_DryGrass_Destruction`) in one order per
terrain; a layer's name may hold underscores (`Cliff_m`, `SP_03_Cliff`), so a name is split into as many layers as the
visual terrain stores for it. A layer of a rule-only shader (tiled 0) is a rule layer; where every layer has one kind,
each shader's tiled count is its other layers', and a cell's tile slot k is the k-th tiled layer in the name's order:
every aligned block of flag x flag cells then names one tile per layer (1,131 of 1,131 blocks and layers), and not
when the order is reversed (1,063 of 1,463). Only 7 of `Dist\win32`'s 23 terrains give each layer one kind (MP_005,
MP_006, MP_007, SP_005, SP_005_B, SP_006, SP_04): 6 have names this split does not settle (MP_001, MP_003, MP_004,
MP_009, SP_007, SP_004_B) and 10 use a layer as tiled in some shaders and rule-only in others (MP_002, MP_008, MP_012,
SP_003_B, SP_008, SP_008_B, SP_009, SP_01, SP_02, SP_03), so their tiled layers are per shader, not per layer. Round
E5-rest's visual paint writes the first 7 and refuses the others in words (XVAL-EDIT-5).

[test: rust/bc2-levelwrite/tests/terrain_layers.rs::dice_paint_layers]
[code: rust/bc2-levelwrite/src/terrainpaint.rs:75-112]
[data: rust/bc2-levelwrite/tests/terrain_layers.rs on the game copy, 2026-11-23: 23 Dist visual terrains, 7 one kind per layer; slot order 1,131 of 1,131 blocks and layers in name order, 1,063 of 1,463 reversed]

### TEDIT-ATL-1: One mask-tile atlas per terrain, laid out as its patch maps' header says

Status: **verified** · Variants: PC

A visual terrain names one mask-tile atlas (30 of 30), an L8 texture (format 10, one mip) of 1,024 x 512 (13
terrains), 1,024 x 1,024 (12) or 2,048 x 1,024 (5); its patch maps' header holds the atlas size in 64-pixel tiles
minus one (15 / 7, 15 / 15, 31 / 15) on 30 of 30. A tile is 66 pixels with a 1-pixel border (header 66, 1). A paint
that needs more tiles than the atlas holds must grow the atlas to another of DICE's layouts and rewrite every patch
map's header, and then the level's patch shaders must be built for that layout (TERR-VT-5, TEDIT-SH-2).

Round E5-rest writes a paint's tiles into slots no cell outside the painted blocks names (the atlas is not grown: a
paint that needs more is refused) and gives each tile a border of its nearest inside texel, as 94.9 % of DICE's border
texels are (270,434 of 284,928 over the 7 terrains of TEDIT-VT-2; the rest follow the neighbouring block).

[test: core/tests/test_spec_probes.py::visual_terrains]
[test: rust/bc2-levelwrite/tests/terrain_layers.rs::dice_paint_layers]
[code: rust/bc2-levelwrite/src/terrainpaint.rs:289-300]
[data: spec/probes/terrain.py, 2026-11-22: 30 visual terrains, 1 atlas each; atlas (size, format, mips) against the header: 2048 x 1024 fmt 10 1 mip / (31, 15) x 5, 1024 x 512 / (15, 7) x 13, 1024 x 1024 / (15, 15) x 12, each = size / 64 - 1]

### TEDIT-ATL-2: A tile covers a block of flag x flag cells; DICE's flags are 2 - 64

Status: **verified** · Variants: PC

A mask's flag is the side, in cells, of the block its tile covers (TERR-VT-7); over DICE's terrains the flags are 2
(5,367), 4 (11,712), 8 (56,352), 16 (13,750), 32 (4,864) and 64 (2,558), not only 4 and 8. On 22 of the 30 terrains
every tile lies in one block aligned to multiples of its flag; on the other 8 one tile each is named by several blocks
(SP_009: 1,170 blocks at flag 2; MP_006 19; NAM_MP_002R 42; BC1_Oasis 14; MP_001, MP_003, SP_008, NAM_MP_006 4 to 8):
a shared tile, presumably uniform. A painter writes each block's tile once and keeps the blocks aligned; a uniform
block may share one tile. Round E5-rest's paint uses the map's most common flag for the blocks it paints (8 on
MP_005), makes one tile per block and layer from the old masks resampled with the paint laid over them, and writes equal
tiles (the empty one) once.

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py and a per-terrain count (SPG2 scratch), 2026-11-22: 94,603 tiled masks, 79,973 in their tile's one block; per terrain the flags and the one shared tile as listed]
[code: rust/bc2-render/src/terrain.rs:800-811]

### TEDIT-SH-1: A painted layer combination needs a patch shader entry in every database

Status: **implemented** · Variants: PC

A cell draws with the patch shader its index names, a `PatchMaskShaders/<layers>` entry of each of the level's Dx9,
Dx10 and Dx11 databases (RNDR-TERR-1); its layers' order fixes which tile slot each tiled layer reads. A paint that
makes a combination the terrain has no patch shader for needs a new entry: DICE's terrains carry 7 - 109 patch
shaders (most 50), of 1 - 8 layers (4 on 424, 3 on 344), 0 - 4 of them tiled. The suite has no Frostbite shader
compiler: a new entry is a clone of an existing one with the same tiled layers, from the same level or a donor that
satisfies TEDIT-SH-2, its textures retargeted (OPFOR's Wake Island patches). Round TG2's
`bc2_formats::shaderdb::terrain::clone_terrain` makes such clones into a level's three databases (TGEN-VIS-4); the
paint's new combination is `clone_entries` with an entry of the level's own terrain and the new combination's name.

Round E5-rest implements it: a painted cell takes the terrain's own shader for its new layer set; else a shader with
the same layers and more tiled ones, whose extra masks are the empty tile (it draws the set exactly); else a clone of a
shader with the same layer count that differs in one tiled layer at the same place in the order (its tile slots line
up), that layer's own textures (those every entry with it samples and no entry without it does) renamed to the
painted layer's, added to the visual terrain without undergrowth and cloned into the level's Dx9 / 10 / 11 databases.
On MP_005, a tiled layer painted over each shader's cells: 67 combinations are the terrain's own, 19 drawn by a wider
shader, 7 cloned, 55 refused (DryGrass and Rubble have no texture of their own: they share theirs). A clone draws the
donor layer's constants (tiling, colour) with the painted layer's textures: whether it looks right is a playtest's.

[code: rust/bc2-levelwrite/src/terrainpaint.rs:475-532]
[test: rust/bc2-levelwrite/tests/terrain_layers.rs::mp_005_paint_written]
[test: rust/bc2-levelwrite/tests/terrain_edits.rs::painted_export_stages_and_checks]

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

### TEDIT-MM-1: Painting a physics material writes the material map's cells

Status: **verified** · Variants: PC

Which physics material a terrain cell has (footsteps, impacts, vehicle dust, crater size: DSTR-TERR-3, MAT-FX-1,
MAT-FX-2) is the `TerrainMaterialMap`, now decoded (TERR-MM-2 - TERR-MM-4): a paint that changes what the ground is
made of writes the cells under the brush (0.5 m, or 0.25 m on MP_001, MP_002 and MP_008: TERR-MM-3) to the index of
the material's name, re-codes each touched row (and so the block's row lengths and byte length) as DICE does, and adds
the name to the names list when the map lacks it (at most 16 names; the level's grid must hold the material, MAT-GRID-2,
for every level that uses the terrain). A visual paint (TEDIT-VT-1) and the material paint are separate writes: the
patch layers are not tied to materials in the data, so the editor pairs them (asphalt patch -> `tarmac`) by its own
table. DICE's cells re-encode byte for byte, so an untouched block stays as DICE has it.

Round E5 implements the cell paint (`MaterialMap` in `bc2_formats::terrain_codec`; the editor's Material brush paints
the map's own names). Round E5-rest adds a name: one of the level grid's `Terrain`, `Default` or `Construction`
materials (155 of 155 names of DICE's material maps are such a material of their level's grid: `Stone` and `Concrete`
are construction materials), appended after the base's names (the log's `material-add`), at most 16.

[test: rust/bc2-levelwrite/tests/terrain_layers.rs::dice_paint_layers]
[code: rust/bc2-levelwrite/src/terrainedit.rs:135-180]

[test: core/tests/test_spec_probes.py::material_maps]
[test: rust/bc2-formats/tests/terrain_corpus.rs::dice_terrains_round_trip]
[test: rust/bc2-levelwrite/tests/terrain_edits.rs::terrain_resources_written_from_strokes]
[code: rust/bc2-formats/src/terrain_codec.rs:1-40]
[inferred: TERR-MM-2 - TERR-MM-4; the pairing of visual layers with materials is the editor's choice (no field links them in DICE's resources)]

### TEDIT-MM-2: The visual / material pairing is measured on the terrain's own cells

Status: **verified** · Variants: PC

No field ties a visual layer to a physics material (TEDIT-MM-1), so the editor pairs them from the terrain's own data:
for each tiled layer, the material map's cell under every fourth texel of its mask tiles where the mask is 200 or more;
the most common material is the layer's pair. On MP_005: Asphalt -> Tarmac (85 % of 639 samples), Dirtfloor -> Dirt
(63 % of 482), DryGrass -> Sand (98 % of 250), Rubble -> Sand (98 % of 51), Trash -> Gravel (99 % of 92). The Paint
layer brush's "Paired material" paints the pair's cells under its layer in the same stroke.

[test: rust/bc2-levelwrite/tests/terrain_layers.rs::dice_paint_layers]
[code: rust/bc2-levelwrite/src/terrainpaint.rs:858-870]
[data: rust/bc2-levelwrite/tests/terrain_layers.rs on the game copy, 2026-11-23: MP_005's five tiled layers paired as listed]

## 4. The rest of the terrain bundle

### TEDIT-BLK-1: Main block, then a ring of 12 per ring

Status: **verified** · Variants: PC

A visual terrain has N blocks: 1 (14 terrains, no sub-terrains) or 1 + 12k (13: 12 terrains, 25: 1, 37: 2, 49: 1),
each with its road blocks and its patch map in the same order; patch maps name their block's height field
(`Terrains/...`, 318 of 318) and the atlas. A new terrain (E7) without a sub-terrain ring writes N = 1, as 14 of
DICE's do.

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: blocks per terrain {1: 14, 13: 12, 25: 1, 37: 2, 49: 1}; 318 patch maps]

### TEDIT-RING-1: A sculpt that reaches the ring is stitched as DICE stitches its rings

Status: **verified** · Variants: PC

The editor's sculpt brushes reach the sub-terrain ring where their disc passes the main block's edge: the ring's 12
blocks are one raster of 256 x 256 samples over the outer square (`st` main samples apart), changed in the same
operation as the main block. The export lays the ring's strokes into its blocks and stitches the main block and ring
as TERR-HF-12 says (the main edges straight between the ring's knots, the ring's seams the knots, each block's last
row and column its neighbour's first), the main block's tree and envelope rebuilt where its edges moved; a block
nothing changed keeps DICE's bytes. On DICE's 7 single-ring terrains of `Dist\win32`: nothing edited gives DICE's 13
blocks byte for byte; a 30 m raise along the west edge rewrites 2 of 12 ring blocks, every seam (512 / 512, 768 /
768) holds and the edges are 0.50 stored units off straight (DICE's rounding).

[test: rust/bc2-levelwrite/tests/terrain_layers.rs::dice_rings_stitched_after_a_sculpt]
[test: rust/bc2-editor/tests/terrain_tools.rs::terrain_brushes_through_the_interface]
[code: rust/bc2-levelwrite/src/terrainedit.rs:230-290]

### TEDIT-OCC-1: The colour / occlusion map under an edit is re-baked by ratio

Status: **implemented** · Variants: PC

A sculpt or a paint changes what the terrain's colour / occlusion map (LIGHT-BAKE-2) should hold under it; DICE
painted most of that map by hand, so the editor scales only the edited texels by the ratio of new to old computed
sky visibility and leaves the rest byte for byte (LIGHT-BAKE-6).

Round E5-rest writes it: the map (`mp05_AmbientOcclusion` in MP_005's `level-00.fbrb`, DXT1 or L8) is a raster of its
level-0 texels (5:6:5 colours or grey levels) the Shade brush paints and "Re-shade sculpted area" changes: DICE's
texels scaled by the ratio of sky visibility on the sculpted ground to DICE's (`sky_visibility_over`, clamped to
1/4 - 4), only where it differs. The export writes only the 4 x 4 blocks (or L8 texels) under the change and the mip
texels below them (`itexture::splice`) and stages the map in the level's own bundle. On DICE's 18 DXT1 / L8 maps,
nothing painted gives DICE's bytes and a 64 x 64 shade rewrites 611 - 4,096 level-0 bytes; SP_04's and SP_006's maps
are another format and are not written.

[inferred: LIGHT-BAKE-2 and LIGHT-BAKE-6]
[code: rust/bc2-levelwrite/src/terrainpaint.rs:967-1030]
[test: rust/bc2-levelwrite/tests/terrain_layers.rs::dice_colour_maps_written_in_place]

### TEDIT-BR-1: The editor's brushes write stored heights and cells, clamped to the range

Status: **implemented** · Variants: PC

The map editor's Terrain workspace (round E5) sculpts the main block's stored heights - raise, lower, smooth, flatten to
the height where the stroke began, a seeded noise, a ramp between two clicked points - each with a radius, a strength
and a falloff (smooth, linear, constant), and paints the material map's cells with one of the map's names. A stroke is
one operation of the project's log: the 64 x 64 tiles of the raster it changed, kept beside the project
(XLOG-DELTA-1); undo and redo replay from the checkpoints. A height a brush would take past the level's range is held
to it and the editor says so ("Widen range" raises it: TEDIT-HF-2). Round E5-rest adds the Paint layer brush (a
visual layer's weights over the height field's samples, with its paired material in the same operation: TEDIT-VT-1,
TEDIT-MM-2), the Shade brush (TEDIT-OCC-1), sculpts reaching the ring (TEDIT-RING-1), the material map drawn around
the brush, and the game-accurate view following a stroke while it is made (a few times a second). The export writes the main block with its
derived parts rebuilt (TEDIT-HF-3) and the material map (TEDIT-MM-1) into the staged terrain bundle in place of the
base's entries; the rest of the bundle stays byte for byte (XVAL-TERR-1, XVAL-TERR-2).

[code: rust/bc2-editor/src/sculpt.rs:1-20]
[code: rust/bc2-levelwrite/src/export.rs:704-760]
[code: rust/bc2-editor/src/sculpt.rs:402-470]
[test: rust/bc2-editor/tests/terrain_tools.rs::terrain_brushes_through_the_interface]
[test: rust/bc2-levelwrite/tests/terrain_edits.rs::sculpted_export_stages_and_checks]

### TEDIT-NAV-1: A sculpted level's path data is sampled on the staged height field

Status: **verified** · Variants: PC

A sculpt changes the level's ground, so its path data must be sampled again (the AI walks the terrain the samples
describe): the export's path-data step counts a height-field stroke as a collision change and samples the stage
offline (`bfbc2 navkit sample --offline --stage`), which writes the staged terrain bundle's main height field over the
base's terrain unpack in the stage's tree before sampling. Collision in game comes from the height field itself (no
separate terrain collision resource ships in the terrain bundle); whether the engine's Havok height-field shape reads
the edited block as written waits on section 5's step 2.

Round E5-rest ran it end to end on base MP_005 (prepared for NavKit): a hill and a pit sculpted under a Conquest layer,
exported with the path-data step, sample the stage offline and build its path data in process in 34 - 59 s (25
sectors, 8,384 route nodes); `mapexport check` passes every rule but XVAL-PLACE-5 (11 of the layer's spawns and flags
off the main route network), which the same layer unsculpted on the base's offline samples fails too (10 points). The
stage's main route network has 7,341 nodes against the base samples' 15,090: why the stage sampler's network is half
the size is NavKit's question, not the sculpt's (not measured unsculpted on the stage sampler).

[test: rust/bc2-levelwrite/tests/terrain_edits.rs::sculpted_path_data_end_to_end]

[code: rust/bc2-cli/src/navkit/sample.rs:296-325]
[code: rust/bc2-levelwrite/src/pathdata.rs:350-365]
[inferred: the terrain bundle's entries (heightfield, material map, visual terrain, water, atlas) hold no collision resource; the sampler reads the height field (`inputs::heightfield_of`)]

## 5. The playtest plan (E5 / E7)

Written by round SPG2-rest2 (2026-11-22); not run: in-game tests are paused. Every rule above that a writer relies on
stays at its status until its step passes. One level throughout: a copy of MP_005 (one main block, n = 2048 (corrected by round E5: the codec reads MP_005's block as 2,048 samples), mostly
255 depth with 0 footprints, TERR-HF-9) exported by the editor as a test level of its own (registered on server and
client, never over DICE's MP_005), Conquest, offline host, one player and, from step 6, a few bots.

| Step | Terrain written | What to do in game | Settles |
|---|---|---|---|
| 0 | none (the export's canary: the terrain bundle byte for byte as DICE's) | load, look around, a RenderDoc frame from three fixed camera points | the baseline every later frame is compared with |
| 1 | the depth map only: a 64 x 64 m square set 255 -> 0 in the open, and one building footprint 0 -> 255 (heights, tree and envelope untouched) | the same explosive (C4, then a mortar) on each square and on unedited ground beside it; measure the crater (DSTR-TERR-4) | TERR-HF-9: whether the value limits crater depth, and in which direction |
| 2 | a sculpt (a 20 m hill raised, a 10 m pit dug) with the min / max tree and the envelope recomputed (TEDIT-HF-3) | walk and drive over and into both; view them from 50, 300 and 1,000 m; RenderDoc at each | the sculpt path end to end: drawing, collision, LOD; what reads the heights for collision (TEDIT open question) |
| 3 | step 2's heights with the **old** tree and envelope (stale) | the same views and walks | TERR-HF-10 / TERR-HF-11: whether stale bounds cull terrain or objects (holes, popping) or break collision; whether the game checks them |
| 4 | step 2 with an envelope raised 5 m above the heights over the pit only | view the pit from far and near; objects behind the pit's rim | TERR-HF-11: what the lower envelope is used for (occlusion: objects behind wrongly culled; collision: an invisible floor) |
| 5 | a material paint: a 4 x 4 m checkerboard of 0.5 m cells, `Mud` and `Tarmac` (both in MP_005's grid), visual layers untouched | walk along a line across it (footsteps), fire single shots across it every 0.25 m (impact effect per hit), drive a jeep across it | TEDIT-MM-1: nearest cell or blend at a contact; MAT-FX-1 / MAT-FX-2 on painted cells; MAT-TEX-20: the jeep's dirt grows on Mud and Tarmac at the materials' `DirtTriggerFactor` |
| 6 | a visual paint of a new layer combination (TEDIT-SH-1) with its patch and undergrowth mask shader entries | look at it from near and far; bots walk across it | TEDIT-VT-1, TEDIT-SH-1 / -2, VEG-UG-1 (undergrowth grows on the painted area) and the AI's path data (rebuilt for steps 2 - 4) |
| 7 | E7: a new level from a height map (N = 1, TEDIT-BLK-1) | load, walk the edges, a RenderDoc frame | TEDIT-HF-1 / -2 and the whole writer |

Each step records: the export's terrain bundle digests, the module's log, RenderDoc frames by the unattended-playtest
scripts, screenshots, and for steps 1 and 5 the positions and results in a table. A step that crashes stops the plan
and its crash goes to `engine/crashes.md` with the address. A step passes when what it watches matches the rule (then
the rule's status rises to verified with the playtest as its data) or a new rule is written for what was seen.

## Open questions

- The main block's tail (TERR-HF-4) is decoded; whether the game checks the lower envelope (TERR-HF-11) against the
  heights, and what the depth map's values do in game, wait on a playtest of a sculpted terrain.
- What reads the height field for collision and for the AI (Havok's heightfield shape, the navigation sampler):
  whether a sculpt also needs the level's collision or path data rebuilt (navigation: yes, as for any geometry).
- Whether the engine reads the material map's cell under a contact or blends neighbours (TEDIT-MM-1).
- Whether a patch map's shared tile (TEDIT-ATL-2) is always a uniform mask.
- Whether a cell's leftover flag on a rule-only patch (TEDIT-VT-1) is read by anything.
- Nothing here has been loaded in game: every rule a writer relies on must be checked by a playtest first (section
  5's plan).
