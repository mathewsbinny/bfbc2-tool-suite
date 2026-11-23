# Terrain generation: a new BFBC2 terrain from a seed and DICE's assets

What our own random height-map and terrain generator (the user's goal, 2026-11-23) must produce for BFBC2 to load,
draw, collide with and navigate a new terrain, which of DICE's assets it can reuse per biome, what DICE's terrains look
like in numbers a generator must match, the generator's design, how the output is proved, and a feasibility verdict
with the rounds that follow. Written by round TG0 (feasibility and spec, no generator code beyond a throw-away
prototype). The formats are [formats/terrain.md](../formats/terrain.md) and [formats/water.md](../formats/water.md);
what an edit writes is [terrain_edit.md](terrain_edit.md) (TEDIT-*); this file builds on both and repeats neither.

The probes: `spec/probes/terrain_shape.py` measures DICE's 30 terrains (the corpus of terrain_edit.md: the game
copy's DICE bundles dated before 2013, each resource once) and the 61 multiplayer level partitions with a combat area
that OPFOR's `maps.ini` lists (`--ww2` adds OPFOR's four WW2 terrains, printed apart); `spec/probes/terrain_gen_proto.py`
is round TG0's prototype generator and writer, checked against the suite's readers and against DICE's 30 main blocks.
Both run in the game tier (`core/tests/test_spec_probes.py`). Numbers are of 2026-11-23.

Biomes are named here as the data names them: **arid** (desert and dry levels), **jungle**, **snow**, **temperate**
(DICE's `autumn` set), **vietnam** (DICE's `nam` set); BFBC1's two maps fall into arid (Oasis) and temperate (Harvest
Day); BF1943's WW2 maps (OPFOR's conversions) are a sixth, **pacific**, outside DICE's corpus.

## 1. What a valid BFBC2 terrain is, end to end

### TGEN-REQ-1: A level's terrain is eight kinds of data, most of them in the terrain bundle

Status: **verified** · Variants: PC

| Part | Where | Rules | A generator |
|---|---|---|---|
| main height field `Terrains/<T>/<T>.Heightfield-000.res` | terrain bundle | TERR-HF-1 - 4, 9 - 11 | generates the heights; derives the tree and the envelope; writes the depth map (all 255) |
| sub-terrain ring `-001` ... (optional) | terrain bundle | TERR-HF-5, TEDIT-BLK-1 | writes none at first (N = 1, as 14 of DICE's 30); a ring of 12 is a later step (TGEN-REQ-3) |
| material map `<T>-MaterialMap.res` | terrain bundle | TERR-MM-1 - 4, TEDIT-MM-1 | paints the cells; takes the names from the level's material grid (MAT-GRID-2) |
| visual terrain `<T>.Visual.res` | terrain bundle | TERR-VT-1 - 7, TEDIT-VT-1, TEDIT-ATL-1/2 | writes the patch maps; copies a donor's patch shaders with their undergrowth (TGEN-VIS-1) |
| water `<T>.Water.Mesh.res`, `<T>.Water.res` | terrain bundle | WATER-MESH-1, WATER-VIS-1 - 5 | writes the bodies; the visual water's shader slots from a donor (WATER-VIS-3) |
| `Terrain` entry `<T>.res` | terrain bundle | TERR-NAME-2 | writes the 0-byte entry |
| terrain partition `Terrains/<T>/<T>.dbx` | level bundles | TEDIT-SRC-1 | clones the donor's, renamed: the level's `TerrainEntityData.TerrainAsset` names it |
| patch / undergrowth shader entries, the colour / occlusion map, the mask atlas `<T>.MaskTileAtlas.res` | the level's Dx9 / 10 / 11 databases; the atlas in the terrain bundle | RNDR-TERR-1 - 5, TEDIT-SH-1 - 3, VEG-UG-4, LIGHT-BAKE-2 | clones the donor's entries renamed and retargeted; generates the colour / occlusion map; an atlas only for tiled layers (TGEN-VIS-1) |

On all 30 DICE terrains the terrain bundle holds the main block, the material map, the visual terrain and a water mesh
together (30 of 30 water meshes). Everything else a level places on the ground -
forests, middlegrowth, rocks, buildings, gameplay - is the level partition's, not the terrain's (TGEN-VEG-1).

[test: core/tests/test_spec_probes.py::terrain_shapes]
[data: spec/probes/terrain_shape.py, 2026-11-23: 30 DICE terrains, each with a main block, a material map, a visual terrain and a water mesh read from the same bundle; terrain_edit.md's corpus counts for the rest (TEDIT-SRC-1, TEDIT-BLK-1)]

### TGEN-REQ-2: DICE's limits: 1 or 2 km squares of 1 m samples, centred on the origin, heights from 0

Status: **verified** · Variants: PC

Every DICE main block is n = 1,024 (11 terrains) or 2,048 (19) samples of 1 m, its bounds centred on the origin
(-n / 2 .. n / 2 along x and z) with min y 0; the height range is 395 - 1,700 m (TEDIT-HF-2: 500 m on 16). The
material map has 0.5 m cells (27) or 0.25 m (3, TERR-MM-3) and 1 - 10 names of the 16 its 4 bits allow; the patch
map W = 64 or 128 cells of 16 m (TEDIT-VT-1); the mask atlas one of three layouts (TEDIT-ATL-1). A generator writes
inside these limits: no other main size, spacing or placement is known to load (formats/terrain.md, Open questions;
the Create Level form already refuses other sizes: `bc2-editor/src/heightmap.rs`). The import form's `map_size_m`
(`TerrainImport`) must therefore be 1,024 or 2,048 m until a playtest shows another spacing loads.

[test: core/tests/test_spec_probes.py::terrain_shapes]
[data: spec/probes/terrain_shape.py, 2026-11-23: sizes (n, spacing) (1024, 1 m) x 11, (2048, 1 m) x 19; 30 of 30 bounds -n/2 .. n/2 with min y 0]
[code: rust/bc2-editor/src/heightmap.rs:26-27]

### TGEN-REQ-3: Generated, derived, reused, or the engine's

Status: **inferred** · Variants: PC

| Kind | Data |
|---|---|
| generated (seed and settings) | heights; material cells; patch-map shader indices (and, for tiled layers, tiles, flags and atlas tiles); water bodies; the colour / occlusion map; forest, middlegrowth and prop placements |
| derived from the generated data | the min / max tree (TERR-HF-10), the lower envelope (TERR-HF-11, built under the heights: TGEN-OUT-1), the depth runs and padding (TERR-HF-9), the material rows' coding (TERR-MM-2), a ring's quadtrees (TERR-HF-5), the visual water's vertices from the water mesh, path data (NavKit, TGEN-REQ-6), cull-tree slots of placed props (XCULL-RES-1) |
| reused from DICE (one donor per biome, TGEN-BIOME-4) | patch shaders and their undergrowth mask shaders and meshes (TEDIT-SH-1/2, VEG-UG-1), the mask atlas layout, material names (the donor level's grid), the visual water's shader slots, tree / middlegrowth / rock / building blueprints, the weather states, the irradiance volumes of buildings (LIGHT-IRR-2: per building, shared) |
| the engine's at run time | patch normals (RNDR-TERR-2), the height scale (RNDR-TERR-5, TEDIT-HF-4), the destruction mask (DSTR-TERR-2), undergrowth instances (VEG-UG-3), direct light, shadows, fog and sky (LIGHT-BAKE-1) |

The sub-terrain ring (distant low-detail land around the square) is optional: 14 of DICE's 30 terrains have none and
show nothing past their edge; a generator adds a ring later by writing 12 blocks of 64 - 256 samples at 4 - 128 m with
their quadtrees (TERR-HF-5), the same writer as the main block's.

[inferred: TEDIT-SRC-2's table extended to a whole new terrain; TERR-VT-3 (rule layers are computed), VEG-UG-3 (undergrowth generated at run time), LIGHT-BAKE-1 / -2 (what is baked), XCULL-RES-1 (the export reslots new objects)]

### TGEN-REQ-4: A new terrain takes its own name everywhere

Status: **inferred** · Variants: PC

A generated terrain is named `Terrains/<LV>/<LV>` after its level (DICE names each terrain after its map), and every
name that carries the terrain's is written with it: the resources (TERR-NAME-1), the patch maps' height-field names
(TEDIT-BLK-1), the visual terrain's patch-shader and undergrowth-mask-shader names and the database entries they name
(`Terrains/<T>/<T>/PatchMaskShaders/<layers>`, `.../UgMaskShaders/<layers>`, `Terrains/<T>/Shaders/..._Cache`:
TERR-VT-3, VEG-UG-2), the mask atlas and colour / occlusion texture names, the terrain partition and the level's
`TerrainEntityData.TerrainAsset`. Keeping the donor's name instead would make two different terrains answer to one
name wherever both bundles are loaded (a mode level loads its base map's bundles: LVL-MODE-2).

[inferred: TERR-NAME-1 and TERR-VT-3's naming; LVL-MODE-2; OPFOR's Wake Island install retargets cloned patch shaders by name the same way (TEDIT-SH-2)]

### TGEN-REQ-5: The export registers the new level's own terrain bundle

Status: **inferred** · Variants: PC

A generated level is exported as a new level (XEXP-PRE-1) built on a donor of its mode (XEXP-PRE-2) for everything that
is not terrain - commons, mode layer, shader databases, weather - with a terrain bundle of its own holding TGEN-REQ-1's
terrain-bundle parts, registered on server and client (XEXP-REG-1, memory note new-level-registration: a missing
bundleManifest line is a checksum crash). The export today reads the base's other bundles at run time
(XEXP-BLD-2), the base's terrain bundle among them: a generated level must not register the base's terrain bundle
(or the base's terrain is loaded beside the new one for nothing), a change to the export's bundle list that round TG3
makes and `export::check` must then accept ("the terrain's height field is loaded", XEXP-CHK-1, naming the level's
own).

[inferred: XEXP-BLD-2 (base bundles read at run time), XEXP-REG-1, XEXP-CHK-1; LVL-MODE-2]

### TGEN-REQ-6: Collision, water, navigation and cull trees

Status: **inferred** · Variants: PC

- **Collision**: the terrain's collision is its height field (COLL-TERR-1); a generated terrain needs no Havok
  resource. Placed objects bring their blueprints' Havok as any placement does (COLL-IDX-1, COLL-MG-2).
- **Water**: one water mesh body per terrain on DICE's 53 (WATER-MESH-1); a generator writes one flat body over the
  ground under its water level, and the visual water from it with a donor's shader slots. What uses the water mesh in
  game (swimming, boats) is open (COLL open questions).
- **Navigation**: path data is built by NavKit from samples of the level's collision; `navsample_offline.py stage`
  samples an export's stage offline (round NK6) and round NKS-deep C brings the Rust sampler at 0.2 m, so a generated
  level gets path data without starting the game (XNAV-BLD-1 - 4).
- **Cull trees**: the terrain is in no cull tree (CULL-LVL-1: the trees hold static objects); props the generator
  places are new objects and get cull-tree leaves through the export's reslot (XCULL-RES-1 - 3).

[inferred: COLL-TERR-1, WATER-MESH-1, CULL-LVL-1, XCULL-RES-1; bfbc2-auto-navkit/tools/navsample_offline.py's `stage` command (round NK6)]
[doc: bfbc2-auto-navkit/tools/navsample_offline.py:19-21]

### TGEN-REQ-7: Lighting: one map to generate, nothing to re-bake

Status: **inferred** · Variants: PC

Nothing DICE baked depends on the sun (LIGHT-BAKE-1), so a generated terrain takes any donor weather state unchanged.
The one terrain-specific baked datum is the colour / occlusion map (LIGHT-BAKE-2: 2,048 texels over 2,048 m, sampled at
`world / 2048 + 0.5`, red channel multiplying the albedo). DICE painted it by hand; for a new terrain there is nothing
to keep, so the generator computes it: red from sky visibility and slope (WX0's fit, 36 % of MP_005's red explained:
the computed map is plausible, not DICE's look), blue the layer mask the patch shaders read. Buildings keep their own
occlusion textures and irradiance volumes (LIGHT-BAKE-3, LIGHT-IRR-2); E5's occlusion / colour paint layer edits the
map afterwards (LIGHT-BAKE-6).

[inferred: LIGHT-BAKE-1, LIGHT-BAKE-2, LIGHT-BAKE-6, LIGHT-IRR-2]

## 2. Biomes and the assets they reuse

### TGEN-BIOME-1: A terrain's biome is its levels' vegetation set

Status: **verified** · Variants: PC

A terrain's biome is the `vegetation/<set>/` folder (jungle, autumn, snow, nam) its levels' forest and middlegrowth
members come from, by instances, when that set holds at least a fifth of them; a terrain whose levels use no set that
much (only the shared `objects/vegetation` and their own props) is arid. The material maps agree on all 30: every snow
terrain names `snow`, no other does, and no arid one is mostly grass or forest.

| Biome | Terrains |
|---|---|
| arid (5) | MP_005, MP_006, SP_006, SP_009, BC1_Oasis |
| jungle (9) | MP_001, MP_002, MP_009, SP_01, SP_03, SP_003_B, SP_007, SP_008, SP_008_B |
| snow (6) | MP_007, MP_008, MP_012, SP_02, SP_04, SP_004_B |
| temperate (5) | MP_003, MP_004, SP_005, SP_005_B, BC1_Harvest_Day |
| vietnam (5) | NAM_MP_002R, NAM_MP_003, NAM_MP_005, NAM_MP_006, NAM_MP_007 |

[test: core/tests/test_spec_probes.py::terrain_shapes]
[code: spec/probes/terrain_shape.py:48-63]
[data: spec/probes/terrain_shape.py, 2026-11-23: biomes {arid 5, jungle 9, snow 6, temperate 5, vietnam 5}; materials agree on 30 of 30]

### TGEN-BIOME-2: What each biome offers a generator

Status: **observed** · Variants: PC

Per biome, the union over its terrains and the levels on them (instances placed; "types" distinct blueprints):

| Biome | Terrain materials | Atlas layouts | Undergrowth meshes | Forest | Middlegrowth | Rocks / cliffs | Placed blueprints |
|---|---|---|---|---|---|---|---|
| arid | sand, stone, gravel, dirt, mud, rock, seabed, tarmac, field, grass | 1024 x 512 x 3, 1024 x 1024, 2048 x 1024 | 42 | 9 types / 9,971 (`objects/vegetation/trees`, BC1_Oasis's own palms) | 4 / 12,864 | 30 / 5,695 | 549 / 52,825 |
| jungle | grass, jungle, forest, stone, dirt, mud, sand, gravel, field, concrete, tarmac, seabed | 1024 x 512 x 5, 1024 x 1024 x 3, 2048 x 1024 | 46 | 14 / 23,876 (`vegetation/jungle`) | 6 / 22,616 | 79 / 6,841 | 792 / 53,237 |
| snow | snow, stone, rock, gravel, dirt, concrete, tarmac, water, seabed | 1024 x 1024 x 3, 2048 x 1024 x 2, 1024 x 512 | 24 | 5 / 36,142 (`objects/vegetation/trees`, `vegetation/snow`) | 2 / 14,549 | 26 / 4,878 | 497 / 42,639 |
| temperate | forest, grass, field, rock, stone, gravel, dirt, sand, tarmac, water, seabed | 1024 x 1024 x 3, 1024 x 512, 2048 x 1024 | 26 | 6 / 19,262 (`vegetation/autumn`) | 6 / 11,635 | 31 / 6,740 | 434 / 51,565 |
| vietnam | grass, dirt, mud, forest, stone, gravel, water_solid, seabed | 1024 x 512 x 3, 1024 x 1024 x 2 | 6 | 11 / 7,948 (`vegetation/nam`) | 12 / 21,252 | 26 / 6,192 | 266 / 36,139 (`objects/vietnam` 27,492) |

Every terrain's undergrowth meshes are its own (`terrains/<T>/undergrowth/`, 1 - 12 a terrain; Vietnam's
`vegetation/nam/undergrowth` and two shared `objects/undergrowth` meshes the exceptions), drawn only through the
visual terrain (VEG-UG-1), so a generator reuses them by reusing the donor's patch shaders. Rocks and cliffs are
ordinary blueprints placed by the level (named rock, cliff, stone or boulder); the shared `objects/objects`,
`street_objects` and `house_objects` folders carry most props in every biome.

[data: spec/probes/terrain_shape.py, 2026-11-23: "TGEN-ASSET per biome" (materials, atlas layouts, undergrowth meshes, forest / middlegrowth / blueprint types and instances by folder, rock / cliff names) over the 30 terrains and the 74 DICE level partitions on them]

### TGEN-BIOME-3: The pacific biome is OPFOR's conversions of BF1943

Status: **observed** · Variants: PC, BF1943

OPFOR's four WW2 terrains (Wake Island, Guadalcanal, Iwo Jima, Coral Sea; BF1943's, converted) are 2,048-sample
blocks of 250 - 1,000 m range whose ground is mostly sea (91 - 96 % under the water level, against DICE's 0 - 83 %),
with 12 - 34 patch shaders, two undergrowth meshes that are DICE stand-ins (`beach_grass_01`, `fern02`: VEG-OPF-1),
5 - 6 tree types and one middlegrowth type under `objects/vegetation`, 2 - 4 rock types, and BF1943's
`objects/beach_objects` and `objects/architecture` props. Their main blocks keep BF1943's min y (Wake Island 165.9 m,
Guadalcanal 125.4 m), not DICE's 0. A WW2 generated terrain reuses this set under WW2 mode (memory note
ww2-navmesh-planning: no modern kits); it is read from OPFOR's build, read-only, and never part of DICE's checks.

[data: spec/probes/terrain_shape.py --ww2, 2026-11-23: per terrain n, range, under-water share 0.915 - 0.963, atlas, patch shaders, undergrowth meshes, materials, forest / middlegrowth / blueprint folders as printed]

### TGEN-BIOME-4: One donor visual terrain per generated terrain

Status: **inferred** · Variants: PC

A generated terrain draws with the patch shaders of one donor terrain of its biome, copied whole with their
undergrowth (the mask shader entries and mesh entries travel together: VEG-UG-1, VEG-UG-4), its mask atlas layout kept
in every patch map's header (TERR-VT-5, TEDIT-ATL-1), its material names taken from the donor level's grid
(MAT-GRID-2). Mixing two donors' shaders in one terrain is allowed only between terrains of the same atlas layout and
with each cloned shader's tiled layers matching (TEDIT-SH-2), and doubles the textures the level carries; a single donor
keeps every rule of TEDIT-SH-1 - 3 satisfied by construction. Donor candidates per biome are the table's terrains whose
levels are multiplayer (the editor's base for the mode layer, XEXP-PRE-2).

[inferred: TERR-VT-5, TEDIT-SH-1 - 3, VEG-UG-1, VEG-UG-4, MAT-GRID-2; the Wake Island install's single donor (MP_009CQ)]

### TGEN-VIS-1: Rule-only patch shaders draw a terrain with no atlas tiles

Status: **verified** · Variants: PC

Every DICE terrain has at least one patch shader with no tiled layer (1 - 20 a terrain; MP_005: `Dunes_Destruction`,
`Dunes_Mountain_Destruction`, `Dunes_Seafloor_Destruction`, `Dunes_Seafloor_Mountain_Destruction`), whose layers - base,
slope ("Mountain"), sea floor, destruction - are computed in the shader from the height and slope (TERR-VT-3,
RNDR-TERR-3). A generated terrain's first visual tier therefore needs no mask atlas tiles: every patch-map cell names
the donor's rule-only shader with the most layers, with zero tiles and flags (TEDIT-VT-1's rule holds: a rule-only
cell's non-zero flags are 0), and the shader's own slope and sea-floor rules paint rock on steep ground and sea floor
under the water. The prototype writes such a visual terrain from MP_005's (one block, empty road blocks, MP_005's 50
patch shaders, a 64 x 64 patch map all naming shader 13) and the probe's reader reads it back whole. Tiled layers
(paths, fields, grass patches) are the second tier: they need atlas tiles painted (TEDIT-ATL-1), E5's paint tool.

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[code: spec/probes/terrain_gen_proto.py:251-271]
[data: spec/probes/terrain_shape.py, 2026-11-23: rule-only patch shaders (tiled count 0) on 30 of 30 terrains, 1 (MP_006, NAM_MP_006, SP_005, SP_005_B) to 20 (SP_01)]

### TGEN-VIS-2: The donor's rule layers expect the donor's heights

Status: **inferred** · Variants: PC

A patch shader's layer weights are literals compiled into it (RNDR-TERR-2), and BF1943's sea-floor layers darken by
depth under a water height of their own (RNDR-TERR-5's Wake Island case). A generated terrain drawn with a donor's
rule shaders should therefore put its water at the donor's water level and keep its ground in the donor's height band
(MP_005: water 58 m, range 500 m), until a capture shows which heights the rules read from the engine and which are
literals.

[inferred: RNDR-TERR-2 (literal weights), RNDR-TERR-5 (Wake Island's sea floor shaded against its water at 215 m); the donor's water level from spec/probes/terrain_shape.py]

## 3. How DICE's terrains are shaped

All measured on the main blocks at 1 m (slopes from central differences) or on a 2 m mean grid (spectrum, relief).

### TGEN-SHAPE-1: Elevation: low basins and a long upper tail

Status: **verified** · Variants: PC

Per biome (medians over its terrains, then the range of the terrains):

| Biome | Height range m | Hypsometric integral | Median slope deg | Share over 30 deg | Relief 10 m / 66 m (m, RMS) | Spectrum slope beta | Under water | Coast km | Border minus centre m |
|---|---|---|---|---|---|---|---|---|---|
| arid | 500 (395 - 1,000) | 0.27 (0.24 - 0.49) | 8.0 (1.2 - 14.9) | 0.12 (0 - 0.23) | 0.20 / 1.98 | 4.02 (3.36 - 4.18) | 0.12 (0 - 0.40) | 4.9 | 35 (-9 - 75) |
| jungle | 750 (500 - 1,000) | 0.41 (0.18 - 0.48) | 22.2 (10.3 - 34.5) | 0.40 (0.15 - 0.58) | 0.68 / 4.67 | 4.07 (3.58 - 4.53) | 0.20 (0.04 - 0.42) | 14.7 | 15 (-112 - 55) |
| snow | 1,000 (500 - 1,700) | 0.41 (0.21 - 0.57) | 30.5 (2.6 - 36.4) | 0.51 (0.16 - 0.61) | 0.46 / 4.91 | 4.38 (3.57 - 4.56) | 0.42 (0 - 0.58) | 2.9 | 10 (-64 - 337) |
| temperate | 500 (500 - 650) | 0.36 (0.31 - 0.47) | 26.0 (10.2 - 30.6) | 0.42 (0.03 - 0.51) | 4.77 / 13.16 | 3.71 (3.16 - 4.36) | 0.03 (0.02 - 0.27) | 14.8 | 37 (3 - 59) |
| vietnam | 500 | 0.21 (0.12 - 0.30) | 14.2 (0 - 26.7) | 0.10 (0.07 - 0.43) | 0.41 / 3.35 | 3.94 (3.60 - 4.40) | 0.18 (0.06 - 0.83) | 6.2 | 7 (0 - 58) |

Over all 30 the hypsometric integral is 0.12 - 0.57 (most under 0.45): the ground is low basins, valleys and sea floor
with a long tail of ridges. The range has headroom: the 99th height percentile uses 2 - 90 % of it (median 48 %). A
generator's height remapping must reproduce the skew (a plain fractal surface sits near 0.5).

[test: core/tests/test_spec_probes.py::terrain_shapes]
[code: spec/probes/terrain_shape.py:107-138]
[data: spec/probes/terrain_shape.py, 2026-11-23: per terrain elevation percentiles, hypsometric integral, slope percentiles and shares, relief, beta, water level, under-water share, coastline, border minus centre; per biome medians and ranges as tabled; 99th height percentile over the range 0.02 - 0.90, median 0.48]

### TGEN-SHAPE-2: Slopes: a flat floor and a heavy steep tail

Status: **verified** · Variants: PC

The median of the terrains' median slopes is 20.4 degrees; within a terrain the distribution is wide and skewed: the
10th percentile is 0 - 10 degrees (flat floors, sea bed, roads), the 90th 4 - 71 degrees and the 99th 8 - 84 (DICE's
cliffs: MP_004 83, SP_007 84). The steep tail is what a fractal surface lacks: the prototype matches each biome's
median slope exactly and its spectrum within 0.1 but reaches a 99th-percentile slope of 20 - 56 degrees where the same
biomes' DICE medians are 53 - 75, and half or less of DICE's 66 m relief (TGEN-GEN-3 names the fix; round TG1's
generator reaches 52 - 76 degrees and 70 - 100 % of the relief on four biomes: TGEN-GEN-4).

[test: core/tests/test_spec_probes.py::terrain_shapes]
[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[data: spec/probes/terrain_shape.py, 2026-11-23: median of median slopes 20.4; per terrain p10 / p90 / p99 as printed; biome medians of the 99th percentile arid 52.8, jungle 75.0, snow 71.1, temperate 75.1, vietnam 64.6; the prototype's five biomes (seed 7, n 1,024): p99 20.1 / 45.9 / 56.1 / 48.3 / 33.5 degrees, 66 m relief 0.87 / 2.46 / 3.23 / 2.80 / 1.52 m]

### TGEN-SHAPE-3: The spectrum is steep and the same in every biome

Status: **verified** · Variants: PC

The radially averaged power spectrum of the heights (plane removed, Hann window, wavelengths 16 - 256 m) falls as
k^-beta with beta 3.2 - 4.6 over the 30 terrains (medians 3.7 - 4.4 by biome): smoother than the beta 2 of white
fractal noise's profile and close to the 4 of an integrated random walk - long smooth slopes with sharp breaks. A
generator synthesises at its biome's beta (spectral synthesis takes it as a parameter) and then breaks slopes by
erosion and terracing (TGEN-GEN-3); the 10 m relief (0.03 - 7.8 m RMS) separates rough ground (temperate MP_003,
MP_004, SP_005, SP_005_B 2.7 - 7.8 m, jungle SP_007 4.6 m) from smooth sculpted fields (25 of 30 under 1.5 m).

[test: core/tests/test_spec_probes.py::terrain_shapes]
[code: spec/probes/terrain_shape.py:79-96]
[data: spec/probes/terrain_shape.py, 2026-11-23: beta 3.16 (SP_005) - 4.56 (SP_004_B, SP_04); 10 m relief per terrain as printed]

### TGEN-SHAPE-4: Water and the border

Status: **observed** · Variants: PC

27 of the 30 terrains have ground under their water level (SP_006 and SP_009's water meshes are empty, SP_02's water
covers 0.2 %); the share under water is 0 - 83 % (NAM_MP_005 a flooded plain at 83 %), the coastline 0.4 - 47 km
(NAM_MP_006's paddies 47). The water level is a flat height per terrain (4 - 578 m: sea levels near 0 on the coastal
maps, lakes and rivers higher). The border ring (the outer 1 / 16 of the square) lies a median 19 m above the centre:
DICE raises the edges into hills or mountains to close the view (SP_02 337 m), or sinks them into sea (SP_01 -112 m,
islands). A generator writes one of those two edges, chosen by biome and the user.

[data: spec/probes/terrain_shape.py, 2026-11-23: under-water share, coastline length, water level and border minus centre per terrain as printed; water meshes with no vertices on SP_006 and SP_009]

## 4. Where the playable area and the objectives sit

### TGEN-PLAY-1: Combat areas are 1 - 22 % of the terrain, roughly convex, near the middle

Status: **verified** · Variants: PC

The union of a level's combat-area outlines (LEVEL-ENT-2) on DICE's 61 multiplayer levels:

| Mode | Levels | Area, thousand m2 (min, median, max) | Share of the terrain % | Box aspect | Convexity % (area / hull) | Centre offset % of the side | Relief inside m |
|---|---|---|---|---|---|---|---|
| Conquest | 16 | 44, 99, 479 | 1.0, 8.7, 16.4 | 1.0, 1.5, 3.1 | 49, 88, 99 | 2, 11, 28 | 11, 29, 63 |
| Rush | 16 | 54, 128, 359 | 1.4, 7.4, 21.6 | 1.0, 1.6, 2.9 | 49, 82, 93 | 2, 13, 25 | 11, 44, 84 |
| Squad Rush | 14 | 21, 39, 56 | 0.5, 3.1, 4.6 | 1.1, 1.4, 2.8 | 73, 91, 101 | 2, 15, 31 | 11, 21, 53 |
| Squad Deathmatch | 15 | 15, 40, 84 | 0.4, 2.4, 8.0 | 1.0, 1.3, 1.9 | 86, 97, 101 | 2, 17, 26 | 12, 19, 42 |

Inside the combat area the ground is flatter than outside on 52 of 61 levels (median of the levels' median slopes
inside 8 degrees in Conquest against 14 outside); 12 - 17 % of it lies under water in the median level. So a generator
reserves a playable region of a fifth of the square or less, near (not at) the centre, roughly convex, flatter than its
surroundings, and spends the rest on the closing border (TGEN-SHAPE-4). The Conquest median agrees with the placement
study L1 (99 k m2 with the WW2 levels).

[test: core/tests/test_spec_probes.py::terrain_shapes]
[code: spec/probes/terrain_shape.py:286-344]
[data: spec/probes/terrain_shape.py, 2026-11-23: 61 levels (modes from OPFOR's maps.ini); per mode as tabled; inside flatter than outside on 14 / 14 / 10 / 14 levels by mode; under-water share inside, median 0.17 / 0.17 / 0.12 / 0.15]
[doc: bfbc2-map-editor/placement/l1_distributions.md:42]

### TGEN-PLAY-2: Conquest flags stand on flat ground at any height

Status: **verified** · Variants: PC

DICE's 50 Conquest flags whose entity stands on the ground (within 5 m) lie on slopes of 0 / 1.5 / 7.7 degrees (10th
/ 50th / 90th percentile; 92 % under 10), at any height of their combat area (height percentile 0.2 / 0.5 / 0.9:
neither hilltops nor valley floors are preferred), level with their 100 m surroundings (-7.7 / -1.0 / 7.1 m) and 7 /
49 / 182 m from water. A team's base and Rush objectives are not measured here (their entities stand 10 - 90 m above
the ground on most levels; L1 and A3 measure them from their spawners and M-COMs). A generator therefore flattens a pad under each
objective site rather than choosing sites by height; where the sites go is A3's (structures, travel time), which the
generated terrain must leave room for.

[test: core/tests/test_spec_probes.py::terrain_shapes]
[data: spec/probes/terrain_shape.py, 2026-11-23: 50 flags of 16 Conquest levels: slope, height percentile, relief against 100 m and distance to water percentiles as given; 92 % under 10 degrees]

### TGEN-PLAY-3: The generator flattens objective pads

Status: **verified** · Variants: PC

The prototype levels a disc of 24 m radius at each requested site to the median height under it and blends it into the
ground over the next 24 m; the pad's middle varies by under 0.5 m (TGEN-PLAY-2's 1.5 degree median slope). Pads are
placed before the water level is set, so a pad never ends under water unless the site asks for it.

Round TG1's generator does the same (`Settings::pads`, world metres, radius 24 by default in `bfbc2 terrain
generate --pad`), after the relief is scaled and before the water level is set; the test levels three pads on an arid
terrain and finds each middle under 0.5 m.

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
[code: spec/probes/terrain_gen_proto.py:111-117]
[code: rust/bc2-terragen/src/generate.rs:649-676]

## 5. The generator

### TGEN-GEN-1: A seed and settings give the same terrain, byte for byte

Status: **verified** · Variants: PC

The generator takes a seed and its settings (size, biome, donor, relief, water share, edge, objective sites) and is a
pure function of them: the same inputs give the same bytes of every written resource, another seed other bytes. The
project records the seed, the settings and the generator's version (a change to the generator is a new version, never a
silent change of old projects' terrain); the random source is a counter-based generator (the prototype: NumPy's
PCG64; TG1 in Rust: a seeded generator with no platform or thread-count dependence, so parallel generation stays
deterministic).

Round TG1's generator (`bc2_terragen`): every random value is a hash of (seed, step, index) (SplitMix64's mix), every
parallel step computes each value on its own and every sum runs in a fixed order, and hydraulic erosion's droplets run
one after the other; the same settings give the same bytes on one thread and on four. `bfbc2 terrain generate` writes
`terrain.json` beside the resources with the seed, the settings and `generator_version` (1).

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
[code: spec/probes/terrain_gen_proto.py:94-122]
[code: rust/bc2-terragen/src/rng.rs:1-46]
[code: rust/bc2-cli/src/terrain_cli.rs:120-151]

### TGEN-GEN-2: The prototype's pipeline matches DICE's medians

Status: **verified** · Variants: PC

The prototype: (1) Gaussian noise filtered to the biome's spectrum slope (TGEN-SHAPE-3); (2) a border term raising the
edge by the biome's border-minus-centre; (3) the relief scaled until the median slope is the biome's (TGEN-SHAPE-2);
(4) thermal erosion at a 42 degree talus; (5) objective pads (TGEN-PLAY-3); (6) the water level put where the biome's
share of ground lies under it (TGEN-SHAPE-4); (7) the height range rounded up to 50 m, at least the biome's. On seed 7
at 1,024 samples it hits every biome's beta within 0.1, median slope within 0.1 degree and under-water share exactly;
the tails it misses are TGEN-SHAPE-2's.

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[code: spec/probes/terrain_gen_proto.py:46-122]

### TGEN-GEN-3: What the real generator adds (round TG1)

Status: **implemented** · Variants: PC

Round TG1's generator (`rust/bc2-terragen`, `bc2_terragen::generate`) reaches DICE's tails and skew (TGEN-SHAPE-1 /
-2) with, in order, each step measured with terrain_shape.py's statistics against the biome's DICE medians before it
was kept (TGEN-GEN-4):

1. **spectral synthesis** at the biome's beta (TGEN-SHAPE-3), periodic on the square (two FFTs);
2. **height remapping and slope matching**, five rounds: the heights remapped by a power to the biome's hypsometric
   integral, then each sample's gradient given the magnitude its rank takes in the biome's slope distribution (the
   10th, 50th, 90th and 99th percentiles of TGEN-SHAPE-2) and the surface rebuilt from the gradients by a periodic
   Poisson solve (least squares, by FFT); each round measures the percentiles as the probe does (central differences)
   and corrects the distribution it aims at. This step is what gives DICE's steep tail: a fractal surface's slopes are
   near-Gaussian, DICE's are a flat floor and a heavy tail;
3. **hydraulic erosion**: 125 particle droplets per 1,000 samples (Beyer's model: inertia, capacity, erosion over a
   radius-3 brush, deposition) carve valleys;
4. **the edge** (TGEN-SHAPE-4): a bowl added until the border ring's median minus the centre's is the biome's (or the
   settings': negative sinks it into sea);
5. **the sea floor flattened** where the 10th slope percentile would stay more than 2 degrees over the biome's (the
   ground under the coming water level keeps a share of its relief found by bisection: DICE's sea and lake floors are
   flat), the hypsometric integral remapped once more, the relief scaled to the biome's median slope, the edge
   corrected again;
6. **objective pads** (TGEN-PLAY-3), the heights from 0, the range rounded up to 50 m (at least the biome's), the
   water level where the biome's (or the settings') share of the ground lies under it.

Not done in TG1: ridged / terraced noise (TGEN-GEN-4's one gap left, temperate's rough relief), the playable area kept
flatter (TGEN-PLAY-1; A3 / TG5 give the region), shores and fords, the sub-terrain ring (TGEN-REQ-3). A 1,024 terrain
takes 1 - 2.5 s, a 2,048 one 7 - 10 s on this PC (rust/BENCHMARKS.md TG1).

[code: rust/bc2-terragen/src/generate.rs:146-203]
[code: rust/bc2-terragen/src/generate.rs:209-419]
[code: rust/bc2-terragen/src/generate.rs:426-524]
[code: rust/bc2-terragen/src/generate.rs:564-676]

### TGEN-GEN-4: TG1's terrains fall on DICE's biome medians

Status: **verified** · Variants: PC

terrain_shape.py's `shape()` on the main blocks `bfbc2 terrain generate` writes (seed 7, 1,024 samples, one pad at the
centre; the prototype at the same seed), against the biome's DICE median (TGEN-SHAPE-1, -2, -4):

| Biome | | Slope p10 / p50 / p90 / p99 deg | HI | Relief 10 m / 66 m (m) | beta | Border - centre m | Under water |
|---|---|---|---|---|---|---|---|
| arid | DICE | 1.2 / 8.0 / 32.0 / 52.8 | 0.27 | 0.20 / 1.98 | 4.02 | 35.1 | 0.12 |
| | prototype | 3.1 / 8.0 / 14.5 / 20.0 | 0.60 | 0.12 / 0.87 | 3.94 | -4.0 | 0.12 |
| | TG1 | 1.8 / 8.7 / 30.0 / 51.9 | 0.29 | 0.28 / 1.63 | 4.09 | 35.0 | 0.12 |
| jungle | DICE | 0.6 / 22.2 / 54.9 / 75.0 | 0.41 | 0.68 / 4.67 | 4.07 | 14.8 | 0.20 |
| | prototype | 9.0 / 22.2 / 37.2 / 45.9 | 0.60 | 0.32 / 2.45 | 4.00 | -41.3 | 0.20 |
| | TG1 | 3.9 / 22.3 / 56.6 / 76.0 | 0.42 | 0.90 / 4.67 | 4.00 | 14.9 | 0.20 |
| snow | DICE | 6.6 / 30.5 / 58.0 / 71.1 | 0.41 | 0.46 / 4.91 | 4.38 | 9.8 | 0.42 |
| | prototype | 12.7 / 30.5 / 47.4 / 55.9 | 0.60 | 0.31 / 3.21 | 4.32 | -89.9 | 0.42 |
| | TG1 | 7.9 / 30.5 / 60.6 / 74.6 | 0.41 | 0.83 / 4.55 | 4.33 | 9.7 | 0.42 |
| temperate | DICE | 6.2 / 26.0 / 55.7 / 75.1 | 0.36 | 4.77 / 13.16 | 3.71 | 37.2 | 0.03 |
| | prototype | 10.8 / 26.2 / 41.7 / 48.3 | 0.58 | 0.50 / 2.80 | 3.63 | -4.3 | 0.03 |
| | TG1 | 6.6 / 25.9 / 56.4 / 75.3 | 0.36 | 0.94 / 4.43 | 3.76 | 37.3 | 0.03 |
| vietnam | DICE | 0.0 / 14.2 / 29.4 / 64.6 | 0.21 | 0.41 / 3.35 | 3.94 | 6.9 | 0.18 |
| | prototype | 5.6 / 14.2 / 25.0 / 33.5 | 0.59 | 0.22 / 1.52 | 3.86 | -23.4 | 0.18 |
| | TG1 | 0.8 / 14.2 / 32.9 / 62.7 | 0.22 | 0.45 / 2.36 | 3.98 | 7.0 | 0.18 |

The prototype's gap is closed: the 99th-percentile slope is 52 - 76 degrees (DICE's medians 53 - 75; the prototype
20 - 56), the 90th within 4 degrees, the 66 m relief 70 - 100 % of DICE's median on four biomes (the prototype 44 -
65 %), the hypsometric integral within 0.02 (the prototype 0.58 - 0.60 on all five), the border within 0.2 m. Left:
temperate's 10 m / 66 m relief (0.94 / 4.43 m against 4.77 / 13.16: the median of its five terrains sits on three rocky
ones, MP_003, MP_004, SP_005; BC1_Harvest_Day has 0.13 / 1.33) wants the ridged / terraced option; jungle's 10th slope
percentile 3.9 against 0.6 (DICE's range 0 - 7). A 2,048 jungle terrain (seed 11) measures 2.3 / 22.2 / 56.2 / 75.6,
HI 0.41, relief 0.85 / 4.09 m, beta 3.93, border 14.8 m; its file is 8,669,245 bytes, the size of DICE's 2,048 blocks.
The test holds every biome at seed 7 near its medians (median slope within 1 degree, p90 within 6, p99 within 5, HI
within 0.03, beta within 0.3, border within 3 m, under-water share within 0.01).

[test: rust/bc2-terragen/tests/generated.rs::every_biome_lands_on_its_dice_medians]
[data: spec/probes/terrain_shape.py's shape() on bfbc2 terrain generate's main blocks and the prototype's terrains, 2026-11-23, as tabled; DICE's medians from the same probe's run on the 30 terrains]

### TGEN-GEN-5: The library the editor and the command line call

Status: **implemented** · Variants: PC

`bc2_terragen` is a crate of its own (bc2-formats and rayon only), so the map editor's Create Level and sculpt tools
(E5, TG3) and `bfbc2 terrain generate` share it: `Settings` (biome, size, seed, border, water share, pads, erosion) ->
`Terrain::new` (the height field as stored values with its range and water level, the 0.5 m material cells, the
names) -> `Terrain::resources(name)` (the terrain bundle's `Terrains/<T>/<T>.Heightfield-000.res`,
`<T>-MaterialMap.res`, `<T>.Water.Mesh.res`, the 0-byte `<T>.res`, each with its resource type), the blocks written
through `bc2_formats::terrain_codec` (round E5's codec: DICE's 30 main blocks and material maps byte for byte);
`paint::paint` repaints the materials of any height field (a sculpted one too). The command line:
`bfbc2 terrain generate --biome <b> --size 1024|2048 --seed <n> --out <folder> [--name <T>] [--border <m>]
[--water <share>] [--pad <x>,<z>[,<r>]] ... [--erosion <k>]`, which writes the resources and `terrain.json`
(TGEN-GEN-1) and refuses other sizes (TGEN-REQ-2).

[code: rust/bc2-terragen/src/lib.rs:118-139]
[code: rust/bc2-cli/src/terrain_cli.rs:153-216]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]

### TGEN-MAT-1: Material names come from the donor level's grid

Status: **implemented** · Variants: PC

A generated material map names 1 - 16 materials, `defaultmaterial` first (TERR-MM-4), every one a `MaterialData` of the
material grid of each level that uses the terrain (MAT-GRID-2): the donor level's grid, which the export carries. The
prototype's per-biome lists are names DICE's terrains of that biome use (TGEN-BIOME-2): ground, rock, sea bed and a
slope material.

[code: spec/probes/terrain_gen_proto.py:53-59]
[code: rust/bc2-terragen/src/biome.rs:114-122]

### TGEN-MAT-2: Materials are painted from height and slope; visual layers follow the same rules

Status: **implemented** · Variants: PC

The prototype paints each 0.5 m cell from the height field: under the water the sea bed, over 35 degrees stone or rock,
25 - 35 degrees gravel or dirt, else the biome's ground; the visual terrain's rule layers make the same split in the
shader (TGEN-VIS-1), so what a soldier walks on and what is drawn agree. TG2 adds the biome's other materials by noise
and distance to water, the visual tiled layers painted to match (TEDIT-MM-1: the editor pairs visual layers with
materials by its own table), roads and paths as both.

[code: spec/probes/terrain_gen_proto.py:208-219]
[code: rust/bc2-terragen/src/paint.rs:27-53]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]

### TGEN-VEG-1: Vegetation and props are placed as the level's model groups

Status: **inferred** · Variants: PC

Undergrowth needs no placement: it grows from the donor's patch shaders (VEG-UG-1, VEG-UG-3). Trees, bushes and rocks
are placed by the generator as members of the level's forest and middlegrowth groups and as static model-group members
(VEG-CLS-1, VEG-CLS-2: the editor's group edits apply unchanged), each kind from its biome's blueprints (TGEN-BIOME-2),
with density by slope, height, distance to water and the playable area (DICE's biomes place 8 - 36 thousand forest and
12 - 23 thousand middlegrowth instances over their levels), seeded like the terrain. Each placed member brings its
physics (COLL-MG-2) and the export reslots it into the cull trees (XCULL-RES-1).

[inferred: VEG-CLS-1 / -2, VEG-UG-1 / -3, COLL-MG-2, XCULL-RES-1; the per-biome instance counts of TGEN-BIOME-2]

## 6. Output and proof

### TGEN-OUT-1: A generated main block reads back in the suite's readers

Status: **verified** · Variants: PC

The prototype's main block (1,024 samples in the check, 2,048 by hand) is read by the suite's Rust reader (`bfbc2core.terrain.parse`,
`bc2_formats::terrain`) with every height within one u16 step, its tail taken as a main block's (`Tail::Sections`);
the probe's DICE-checked reader re-encodes its depth runs, padding and min / max tree byte for byte; every lower
envelope grid, built with each vertex the lowest sample of the cells around it, lies at or under every sample
(TERR-HF-11's requirement, TEDIT-HF-3). The 2,048 file is 8,669,245 bytes, the size of DICE's 2,048 blocks.

Round TG1's main blocks (`bc2_terragen`, written by `bc2_formats::terrain_codec`) read in `bc2_formats::terrain::parse`
with the stored values the generator made, DICE's header words, the tail taken as a main block's, decode in the codec
and re-encode byte for byte with a depth map of 255 and the envelope under every sample (`under_share` 1.0); the
1,024 file has DICE's 1,024 size and the 2,048 one (8,669,245 bytes) DICE's 2,048 size.

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
[code: spec/probes/terrain_gen_proto.py:127-178]

### TGEN-OUT-2: A generated material map decodes and re-encodes byte for byte

Status: **verified** · Variants: PC

The prototype's material map (2,048 cells a side for a 1,024 m terrain) decodes to the painted cells and its names, and
re-encodes byte for byte with DICE's coding (TERR-MM-2). Round TG1's material maps (the codec's `MaterialMap`)
decode to the painted cells and the biome's names and re-encode byte for byte; an arid terrain uses all four painted
materials (ground, steep, slope, under water).

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
[code: spec/probes/terrain_gen_proto.py:192-205]

### TGEN-OUT-3: A generated water mesh reads in the Rust reader

Status: **verified** · Variants: PC

A one-body water mesh (a quad over the square at the water level) reads in `bfbc2core.water.parse_mesh` (Rust).
Round TG1's (`bc2_terragen::water::water_mesh`) reads in `bc2_formats::water::parse_mesh`: 4 vertices, 2 triangles.

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
[code: rust/bc2-terragen/src/water.rs:7-23]
[code: spec/probes/terrain_gen_proto.py:222-227]

### TGEN-OUT-4: The writer canary: DICE's 30 main blocks written again

Status: **verified** · Variants: PC

The prototype's block writer, given a DICE main block's heights, depth map, bounds and header words, writes the
block byte for byte on 30 of 30 with DICE's own envelope; with an envelope it builds itself the file is identical up
to the envelope and the built envelope lies under every sample on 30 of 30. So the writer adds nothing of its own to
a terrain except the envelope, and TG1's Rust writer is held to the same canary. TG1 writes through round E5's codec
(`bc2_formats::terrain_codec`), whose corpus test is the canary in Rust: DICE's 30 main blocks and 30 material maps
decoded and written again byte for byte, 288 sub-terrain quadtrees computed equal to the stored ones.

[test: core/tests/test_spec_probes.py::terrain_generator_prototype]
[code: spec/probes/terrain_gen_proto.py:181-189]
[code: spec/probes/terrain_gen_proto.py:331-353]

### TGEN-OUT-5: The proof plan

Status: **inferred** · Variants: PC

| Step | Proves | Round |
|---|---|---|
| suite readers: height field (Rust), tail re-encoded, envelope under, material map byte for byte, water mesh, visual terrain (probe's reader; bc2-render's `VisualTerrain` reader in Rust) | TGEN-OUT-1 - 3, TGEN-VIS-1 | TG0 (done, Python); TG1 in Rust |
| the writer canary on DICE's 30 (TGEN-OUT-4), and a DICE terrain through the whole new-terrain export: its NavKit build from the offline samples equals the one from DICE's own terrain | the writer perturbs nothing | TG1, TG3 |
| `export::check` on a stage with a generated terrain (XEXP-CHK-1: every bundle opens, references resolve, the terrain's height field is loaded, shader entries present, cull check) | the level is complete as data | TG3 |
| NavKit from the offline sampler (`navsample_offline.py stage`, the Rust sampler at 0.2 m from NKS-deep C): the build checks (hpdcheck, roundtrip) and NavKit's bot measures on the playable area (`connectivity`, `ai_coverage` over the combat area) | bots can use it | TG3 |
| the editor's game-accurate view (bc2-render: patches, rule layers, undergrowth, water) and screenshots against the donor level's look | it draws | TG2 |
| in game (when tests resume): terrain_edit.md section 5's step 7 (load, walk the edges, RenderDoc), then collision, materials (footsteps), undergrowth, bots | everything TEDIT-SRC-2 marks "not known" | after the in-game pause |

Unproven until an in-game test: that the engine loads a main block whose envelope is the generator's (TERR-HF-11's
use), a terrain other than the donor's under the donor's rule-layer literals (TGEN-VIS-2), a level without the base's
terrain bundle (TGEN-REQ-5), a water mesh of our own (what the game uses it for), and that the depth map of 255 lets
craters form as on DICE's ground (TERR-HF-9).

[inferred: terrain_edit.md section 5; XEXP-CHK-1; XNAV-BLD-1 - 4; NavKit's parity target (navkit.md)]

## 7. Feasibility and the rounds that follow

### TGEN-PLAN-1: Verdict per part, and rounds TG1 - TG5

Status: **inferred** · Variants: PC

| Part | Verdict | Effort |
|---|---|---|
| height field writer (main block, ring) | main block done (round E5's codec, round TG1's generator: TGEN-OUT-1 / -4); the ring left | small |
| material map writer | feasible now: TGEN-OUT-2 | small |
| water mesh | feasible now (TGEN-OUT-3); the visual water needs its donor slots copied (WATER-VIS-3) | small |
| generator to DICE's medians | feasible now (TGEN-GEN-2) | small |
| generator to DICE's tails and skew | done in round TG1 (TGEN-GEN-3 / -4: slope matching, remapping, erosion); temperate's rough relief left | medium |
| visual terrain, rule layers only | feasible now (TGEN-VIS-1); unproven in game | small |
| visual terrain, tiled layers and atlas | feasible (TEDIT-ATL-1 / TEDIT-VT-1 decoded); needs E5's paint path | medium |
| patch / undergrowth shader entries renamed and retargeted in three databases | feasible: OPFOR's Wake Island clone does it (TEDIT-SH-2) | medium |
| colour / occlusion map | feasible: WX0's sky-visibility model (TGEN-REQ-7) | small |
| vegetation and props | feasible: model-group members (TGEN-VEG-1, SG1's edits) | medium |
| export as a new level | needs the export without the base's terrain bundle (TGEN-REQ-5) | medium |
| path data | feasible with NK6's stage sampling; quality bounded by NavKit's parity | small (wiring) |
| in-game acceptance | blocked by the in-game pause (TGEN-OUT-5) | - |

Rounds: **TG1** (done 2026-11-23: `rust/bc2-terragen`, `bfbc2 terrain generate`; the writers are round E5's codec)
the Rust generator and writers: main block, material map, water mesh, the canary and the readers' round trip, erosion
and remapping tuned to the biome statistics (terrain_shape.py as the scorer); the ring and the visual water move to
TG2. **TG2** the visual side: one donor's patch and undergrowth
shaders renamed into a new terrain's databases, rule-layer patch maps, the colour / occlusion map, the editor's
game-accurate view of a generated terrain. **TG3** E7's Create Level path A wired to the generator (a "Generate"
choice beside "From a height map": the same `TerrainImport` then written by the generator), the export with the
level's own terrain bundle, `export::check`, NavKit from the offline sampler. **TG4** vegetation, rocks and props
placed per biome, with E5's sculpt and paint working on the generated terrain (the same writers). **TG5** gameplay on
the generated terrain through A3 (objectives on its pads, combat areas from its flat region), and the WW2 (pacific)
biome. Each round reports the TGEN-SHAPE / TGEN-PLAY statistics of its output against DICE's.

[inferred: the verdicts from the rules cited per row; the order from the dependencies (writer, then drawing, then export, then content)]

## Open questions

- What the engine does with a generated lower envelope and with the depth map's values (TERR-HF-9, TERR-HF-11): a
  playtest (terrain_edit.md section 5).
- Which heights a donor's rule layers read as literals (TGEN-VIS-2): a RenderDoc capture of a donor's sea-floor patch
  with the water moved.
- Whether a level may leave out its base's terrain bundle (TGEN-REQ-5) and load its own alone.
- Whether main blocks other than 1,024 / 2,048 samples of 1 m load (TGEN-REQ-2).
- The engine's use of the water mesh (swimming, boats) for a body the generator writes.
