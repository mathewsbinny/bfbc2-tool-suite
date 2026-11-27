# Terrain generation: a new BFBC2 terrain from a seed and DICE's assets

What our own random height-map and terrain generator (the user's goal, 2026-11-23) must produce for BFBC2 to load,
draw, collide with and navigate a new terrain, which of DICE's assets it can reuse per biome, what DICE's terrains look
like in numbers a generator must match, the generator's design, how the output is proved, and a feasibility verdict
with the rounds that follow. Written by round TG0 (feasibility and spec, no generator code beyond a throw-away
prototype). The formats are [formats/terrain.md](../formats/terrain.md) and [formats/water.md](../formats/water.md);
what an edit writes is [terrain_edit.md](terrain_edit.md) (TEDIT-*); this file builds on both and repeats neither.

The probes: `spec/probes/terrain_shape.py` measures DICE's 30 terrains (the corpus of terrain_edit.md: the game
copy's DICE bundles dated before 2013, each resource once) and the 61 multiplayer level partitions with a combat area
that OPFOR's `maps.ini` lists (`--ww2` adds OPFOR's four WW2 terrains, printed apart); it runs in the game tier
(`core/tests/test_spec_probes.py`). Round TG0's throw-away prototype (`spec/probes/terrain_gen_proto.py`) was retired
by round TG2 once its visual-terrain writer had a Rust twin: the generator, its writers and their proofs are
`rust/bc2-terragen` and `bc2_formats` (sections 5 - 6). Numbers are of 2026-11-23.

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
show nothing past their edge; round TG2's generator writes one ring of 12 blocks of 64 samples stitched to the main
block as DICE's single rings are (TGEN-RING-1).

[inferred: TEDIT-SRC-2's table extended to a whole new terrain; TERR-VT-3 (rule layers are computed), VEG-UG-3 (undergrowth generated at run time), LIGHT-BAKE-1 / -2 (what is baked), XCULL-RES-1 (the export reslots new objects)]

### TGEN-REQ-4: A new terrain takes its own name everywhere

Status: **verified** · Variants: PC

A generated terrain is named `Terrains/<LV>/<LV>` after its level (DICE names each terrain after its map), and every
name that carries the terrain's is written with it: the resources (TERR-NAME-1), the patch maps' height-field names
(TEDIT-BLK-1), the visual terrain's patch-shader and undergrowth-mask-shader names and the database entries they name
(`Terrains/<T>/<T>/PatchMaskShaders/<layers>`, `.../UgMaskShaders/<layers>`, `Terrains/<T>/Shaders/..._Cache`:
TERR-VT-3, VEG-UG-2), the mask atlas and colour / occlusion texture names, the terrain partition and the level's
`TerrainEntityData.TerrainAsset`. Keeping the donor's name instead would make two different terrains answer to one
name wherever both bundles are loaded (a mode level loads its base map's bundles: LVL-MODE-2).

DICE did this for its own twin terrain: `SP_008_B` (a copy of `SP_008`) names its patch shaders
`Terrains/SP_008_B/SP_008_B/PatchMaskShaders/<layers>`, with `SP_008`'s program keys on every shared layer combination
(16 of 16 in each of the three databases), and its patch shaders sample its own colour / occlusion map
(`Terrains/SP_008_B/Textures/sp08_ambientocclusion`, 15 of 16). Round TG2 renames the same way
(`bc2_formats::visual_terrain::rename`, `shaderdb::terrain::clone_terrain`, TGEN-VIS-4): the visual terrain's patch,
undergrowth-mask and cache shader names, the patch maps' height fields and atlas, the visual water's slots, the
cloned database entries and the colour / occlusion map; the donor's layer textures, undergrowth meshes and the
generic water shaders (`Shaders/WaterOcean`) keep their names (reused assets of the donor level's bundles).

[test: rust/bc2-formats/tests/terrain_visual_corpus.rs::a_donor_terrains_entries_clone_under_a_new_name]
[test: rust/bc2-terragen/tests/visual.rs::a_generated_terrain_draws_with_mp_005s_shaders]
[code: rust/bc2-formats/src/visual_terrain.rs:344-395]
[inferred: TERR-NAME-1 and TERR-VT-3's naming; LVL-MODE-2; OPFOR's Wake Island install retargets cloned patch shaders by name the same way (TEDIT-SH-2)]

### TGEN-REQ-5: The export registers the new level's own terrain bundle

Status: **implemented** · Variants: PC

A generated level is exported as a new level (XEXP-PRE-1) built on a donor of its mode (XEXP-PRE-2) for everything that
is not terrain - commons, mode layer, shader databases, weather - with a terrain bundle of its own holding TGEN-REQ-1's
terrain-bundle parts, registered on server and client (XEXP-REG-1, memory note new-level-registration: a missing
bundleManifest line is a checksum crash). The export today reads the base's other bundles at run time
(XEXP-BLD-2), the base's terrain bundle among them: a generated level must not register the base's terrain bundle
(or the base's terrain is loaded beside the new one for nothing), a change to the export's bundle list that round TG3
makes and `export::check` must then accept ("the terrain's height field is loaded", XEXP-CHK-1, naming the level's
own).

Round TG3 implements it (with E7's height-map path, TEDIT-NEW-2): the staged terrain bundle holds the new terrain's
resources and the base bundle's other entries, never the base terrain's; the terrain block of `package.mft` names the
level's own bundle and the commons, no bundle of the base level; the base terrain's partitions (`<T>.dbx`,
`<T>-MaterialMap.dbx`) are copied under the new name into the level bundle; `export::check` passes on MP_005 with a
generated terrain and with MP_005's own heights imported (XVAL-TERR-4, XVAL-LVL-3 naming the level's own height
field). Whether the engine loads it stays a playtest's.

[inferred: XEXP-BLD-2 (base bundles read at run time), XEXP-REG-1, XEXP-CHK-1; LVL-MODE-2]
[test: rust/bc2-levelwrite/tests/new_terrain.rs::new_level_exports_and_checks]
[code: rust/bc2-levelwrite/src/export.rs:1920-1932]

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
map afterwards (LIGHT-BAKE-6). Round TG2 makes it (TGEN-OCC-1): DICE's maps are painted more than computed, so the
generated map takes the donor map's own values, placed by the terrain's sky visibility.

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
under the water. Round TG2's writer (`bc2_terragen::visual::visual_terrain`, through `bc2_formats::visual_terrain`:
DICE's 30 visual terrains decode and encode byte for byte, TERR-VT-8) writes such a visual terrain from a donor's: N =
13 blocks with the ring (1 without), 2N empty road blocks, the donor's patch shaders renamed (TGEN-REQ-4), a patch map
of 16 m cells per block whose every cell names a rule-only shader by TGEN-VIS-3, no tiles, no flags, the donor's atlas
layout in every header (TERR-VT-5). From MP_005 on a 1,024 arid terrain: 13 blocks, 50 patch shaders, the main
block's 4,096 cells drawn by `Dunes_Destruction` (1,735), `Dunes_Mountain_Destruction` (1,792) and
`Dunes_Seafloor_Destruction` (569). Tiled layers (paths, fields, grass patches) are the second tier: they need atlas
tiles painted (TEDIT-ATL-1), E5's paint tool.

[test: rust/bc2-terragen/tests/visual.rs::a_generated_terrain_draws_with_mp_005s_shaders]
[code: rust/bc2-terragen/src/visual.rs:356-430]
[data: spec/probes/terrain_shape.py, 2026-11-23: rule-only patch shaders (tiled count 0) on 30 of 30 terrains, 1 (MP_006, NAM_MP_006, SP_005, SP_005_B) to 20 (SP_01)]

### TGEN-VIS-2: The donor's rule layers expect the donor's heights

Status: **verified** · Variants: PC

A patch shader's layer weights are literals compiled into it (RNDR-TERR-2), and BF1943's sea-floor layers darken by
depth under a water height of their own (RNDR-TERR-5's Wake Island case). The game-accurate view (`bc2-render`:
DICE's own compiled Dx11 programs, round TG2) shows MP_005's rule layers shading by the ground's height, not by the
water the level writes: drawn over MP_005's files under MP_005's weather, its plain rule-only cells' dry ground is
0.28 bright (luminance, HDR) in the first 20 m over MP_005's water and 0.15 - 0.20 above; on a generated arid terrain
whose heights start at 0 (its water at 2.7 m) the same cells are 0.46 - 0.50 at every height (1.5 - 3.2 times
MP_005's), while the same terrain raised so its water lies at MP_005's 58 m comes within 11 - 15 % of MP_005's band
by band. So a generated terrain drawn with a donor's rule shaders puts its water at the donor's water level
(`Settings::water_level_m`; `bfbc2 terrain generate --donor` takes the donor's by default). Two measures stay off:
the generated terrain's shallow sea floor (0 - 5 m under the water) is 1.74 times MP_005's and greener (its flattened
floor, TGEN-GEN-3, faces the sun where MP_005's shelves); and MP_005's own heights drawn through the generated visual
side (the identity) match its sea floor at every depth within 15 % and its first 20 m of dry ground within 5 %, but
its higher dry ground at 0.53 - 0.74 (the computed colour / occlusion map is darker there than DICE's painted one,
TGEN-OCC-1). Unproven until a playtest: the engine's own drawing (the view is DICE's shaders, not the game).

[test: rust/bc2-render/tests/generated_terrain.rs::a_generated_terrain_draws_with_its_donors_look]
[data: rust/bc2-render/tests/generated_terrain.rs on OPFOR's prepared MP_005, 2026-11-23: per 20 m band over the water and 5 m band under it, luminance ratios and hues as given; rust/BENCHMARKS.md TG2]
[inferred: RNDR-TERR-2 (literal weights), RNDR-TERR-5 (Wake Island's sea floor shaded against its water at 215 m); the donor's water level from spec/probes/terrain_shape.py]

### TGEN-VIS-3: Each cell draws the rule-only shader its ground asks for

Status: **verified** · Variants: PC

A rule-only patch shader's layers are named in its entry (`Dunes_Seafloor_Mountain_Destruction`): a sea-floor layer
(`Seafloor`, `Riverbed`, `Underwater`), a slope layer (`Mountain`, `Cliff`, `Slopes`), a destruction layer
(`Destruction`, `Destr`, `dest`, `dst`). A generated cell (16 m) asks for the sea floor where a sample under it lies
under the water and for the slope layer where one is steeper than 35 degrees (the material paint's rock, TGEN-MAT-2);
it draws the donor's rule-only shader with the fewest misses (a sea floor missed counts double), then one with a
destruction layer, then the one the donor's own map uses most (`bc2_terragen::visual::pick`).

Measured on DICE's own cells with DICE's heights and water: the sea floor is where DICE put it on 38,713 of 47,177
rule-only cells of MP_005, MP_001, MP_009, MP_003 and MP_004 (0.80, 0.97, 0.89, 0.96 and 0.67); the slope layer is
painted (no slope threshold or statistic reproduces DICE's choice better than "never": MP_005 0.73 at a share of 0.73
without it), so the rule puts it where the shader's own slope weighting will draw rock. DICE's rings draw a rule-only
shader on every cell (most of them the plain one the rule picks for flat dry ground: 5 of 6 multiplayer rings; MP_001
adds a pavement variant, MP_007 a mountain one); the generated ring's 4 x 4 cells take the same rule on the ring's
heights.

[test: rust/bc2-terragen/tests/visual.rs::the_cell_rule_against_dices_own_cells]
[code: rust/bc2-terragen/src/visual.rs:29-95]
[data: rust/bc2-terragen/tests/visual.rs on DICE's MP_005, MP_001, MP_009, MP_003, MP_004 and six rings, 2026-11-23, as given]

### TGEN-VIS-4: The donor's entries are cloned into the new terrain's databases

Status: **verified** · Variants: PC

The new level's Dx9, Dx10 and Dx11 databases are the donor level's with every entry the new visual terrain and visual
water name added again under the new terrain's name (TGEN-REQ-4): the patch and undergrowth mask shaders
(`Terrains/<D>/<D>/PatchMaskShaders/`, `.../UgMaskShaders/`), the undergrowth cache shaders and the water slots
(`Terrains/<D>/Shaders/...`). A clone keeps its programs' keys, vertex and pixel shaders and vertex layouts (the suite
compiles no shader: TEDIT-SH-1; a donor terrain's own entries already satisfy TEDIT-SH-2 / -3), renames each program
and its key's entry, and copies the parameter blocks with the colour / occlusion map renamed to the new terrain's
(`bc2_formats::shaderdb::terrain::clone_terrain`, the merge of round OR3a). MP_005's 98 patch, undergrowth, cache and
water entries cloned into MP_005's own databases keep the donor's keys and shaders (98 of 98 in each), sample the
renamed map in 51 entries, and leave the donor's entries as they were (Dx9 0.38 s, Dx10 0.09 s, Dx11 0.06 s); DICE's
twin `SP_008_B` has `SP_008`'s keys under its own name (TGEN-REQ-4). E5-rest's paint of a new layer combination is
the same merge with explicit names (`shaderdb::terrain::clone_entries`: an entry of the level's own terrain cloned
under the new combination's name).

[test: rust/bc2-formats/tests/terrain_visual_corpus.rs::a_donor_terrains_entries_clone_under_a_new_name]
[test: rust/bc2-terragen/tests/visual.rs::a_generated_terrain_draws_with_mp_005s_shaders]
[code: rust/bc2-formats/src/shaderdb/terrain.rs:70-159]

### TGEN-WATER-1: The visual water is one open-water quad over the outer square

Status: **verified** · Variants: PC

A generated terrain's water is one flat body (TGEN-REQ-6): the water mesh and the visual water are one quad at the
water level over the outer square (the ring's, twice the main block's side, else the main block's), the visual
water's indices the mesh's (WATER-VIS-4), its four vertices DICE's open-water vertices (WATER-VIS-6), drawn by the
donor's open-water slot (the slot whose ranges hold most open-water vertices: MP_005's `Water`), the donor's slots
renamed (TGEN-REQ-4; `Shaders/WaterOcean` and its fragment kept) and its far plane kept. Shores (DICE's slot-0 wake
strips along the coast) are not written.

[test: rust/bc2-terragen/tests/visual.rs::a_generated_terrain_draws_with_mp_005s_shaders]
[code: rust/bc2-terragen/src/visual.rs:280-336]

### TGEN-OCC-1: The colour / occlusion map takes the donor's values where the terrain's sky visibility puts them

Status: **verified** · Variants: PC

DICE's terrain maps (LIGHT-BAKE-2) are painted: the best fit of computed sky visibility (16 directions, 2 - 256 m),
slope and curvature explains 27 % of MP_005's red channel, 22 % of MP_009's, 3 % of MP_001's and 12 % of SP_01's, with
coefficients of other signs on each (round TG2's measure; WX0's 36 %). So the generated map is the donor's, rearranged:
the terrain's cosine-weighted sky visibility (16 directions, horizons 2 - 64 texels) on the donor map's grid over
2,048 m, ranked, each texel given the donor's red at its rank (the donor's distribution exactly); green and blue the
donor's medians, or the red where the donor's map is grey (L8, or R = G = B on every texel); written in the donor's
format, size, flags and full mip chain (`bc2_formats::itexture`: DXT1 and L8, DICE's level layout on all 13 terrain
maps of those formats; DICE's maps decoded and encoded again within a mean of 0.0 - 0.9 levels). On MP_005 the
generated map's red p10 / p50 / p90 are DICE's (153 / 173 / 206). The donor's map is found as the texture under
`Terrains/<D>/` with `occ` in its name that the most patch shaders sample (MP_005 `mp05_AmbientOcclusion`, MP_008
`MP008_Occlusion`, SP_005 `sp05_snowocc`; MP_003 and MP_004 sample none). E5-rest's re-bake writes the same texture
format through the same encoder (LIGHT-BAKE-6).

[test: rust/bc2-terragen/tests/visual.rs::a_generated_terrain_draws_with_mp_005s_shaders]
[test: rust/bc2-formats/tests/terrain_visual_corpus.rs::dice_occlusion_maps_encode_as_dice_lays_them_out]
[code: rust/bc2-terragen/src/visual.rs:160-275]
[code: rust/bc2-formats/src/itexture.rs:1-30]
[data: round TG2's scratch fit on DICE's maps (MP_005, MP_009, MP_001, SP_01) against sky visibility, slope and curvature at 2 m, 2026-11-23: r2 0.27 / 0.22 / 0.03 / 0.12]

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

[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
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

[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
[code: rust/bc2-terragen/src/rng.rs:1-46]
[code: rust/bc2-cli/src/terrain_cli.rs:120-151]

### TGEN-GEN-2: The prototype's pipeline matches DICE's medians

Status: **observed** · Variants: PC

The prototype: (1) Gaussian noise filtered to the biome's spectrum slope (TGEN-SHAPE-3); (2) a border term raising the
edge by the biome's border-minus-centre; (3) the relief scaled until the median slope is the biome's (TGEN-SHAPE-2);
(4) thermal erosion at a 42 degree talus; (5) objective pads (TGEN-PLAY-3); (6) the water level put where the biome's
share of ground lies under it (TGEN-SHAPE-4); (7) the height range rounded up to 50 m, at least the biome's. On seed 7
at 1,024 samples it hits every biome's beta within 0.1, median slope within 0.1 degree and under-water share exactly;
the tails it misses are TGEN-SHAPE-2's. (Round TG0's prototype, `spec/probes/terrain_gen_proto.py`, retired by round
TG2: its pipeline is TGEN-GEN-3's first version.)

[data: spec/probes/terrain_gen_proto.py (retired in round TG2), 2026-11-23: seed 7, 1,024 samples, the five biomes' beta, median slope and under-water share as given; rust/BENCHMARKS.md TG0]

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

Round TG2 added temperate's rough relief (TGEN-GEN-6), the water at a given height (TGEN-VIS-2) and the sub-terrain
ring (TGEN-RING-1). Not done: the playable area kept flatter (TGEN-PLAY-1; A3 / TG5 give the region), shores and
fords. A 1,024 terrain takes 1 - 2.5 s, a 2,048 one 7 - 10 s on this PC (rust/BENCHMARKS.md TG1).

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
| | TG2 | 6.5 / 26.1 / 57.1 / 79.0 | 0.33 | 4.24 / 13.70 | 3.13 | 37.5 | 0.03 |
| vietnam | DICE | 0.0 / 14.2 / 29.4 / 64.6 | 0.21 | 0.41 / 3.35 | 3.94 | 6.9 | 0.18 |
| | prototype | 5.6 / 14.2 / 25.0 / 33.5 | 0.59 | 0.22 / 1.52 | 3.86 | -23.4 | 0.18 |
| | TG1 | 0.8 / 14.2 / 32.9 / 62.7 | 0.22 | 0.45 / 2.36 | 3.98 | 7.0 | 0.18 |

The prototype's gap is closed: the 99th-percentile slope is 52 - 76 degrees (DICE's medians 53 - 75; the prototype
20 - 56), the 90th within 4 degrees, the 66 m relief 70 - 100 % of DICE's median on four biomes (the prototype 44 -
65 %), the hypsometric integral within 0.02 (the prototype 0.58 - 0.60 on all five), the border within 0.2 m.
Temperate's 10 m / 66 m relief (TG1 0.94 / 4.43 m against 4.77 / 13.16: the median of its five terrains sits on three
rocky ones, MP_003, MP_004, SP_005; BC1_Harvest_Day has 0.13 / 1.33) is round TG2's escarpment's (TGEN-GEN-6: 4.24 /
13.70 m, its 99th slope percentile 79.0 and beta 3.13, DICE's rough SP_005's 75.1 and 3.16); the other four biomes
measure as TG1's (the ring's stitching changes only the main block's outer rows). Left: jungle's 10th slope
percentile 3.9 against 0.6 (DICE's range 0 - 7). A 2,048 jungle terrain (seed 11) measures 2.3 / 22.2 / 56.2 / 75.6,
HI 0.41, relief 0.85 / 4.09 m, beta 3.93, border 14.8 m; its file is 8,669,245 bytes, the size of DICE's 2,048 blocks.
The test holds every biome at seed 7 near its medians (median slope within 1 degree, p90 within 6, p99 within 5, HI
within 0.03, beta within 0.3 (temperate 0.65: the escarpment's step), the 66 m relief 65 - 130 % of DICE's, border
within 3 m, under-water share within 0.01).

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

Round TG2: `Settings` also takes the rough relief (TGEN-GEN-6), the water's height (TGEN-VIS-2) and the ring
(TGEN-RING-1); `Terrain::resources` adds the ring's `Heightfield-001` .. `-012`; `visual::build` makes the visual
side from a donor's files (TGEN-VIS-1, -3, -4, TGEN-WATER-1, TGEN-OCC-1); the command line adds `--rough`,
`--no-ring`, `--water-level` and `--donor <level> [--opfor <folder>]` (the visual side read from that DICE level of the
game copy, the water at the donor's level unless given).

[code: rust/bc2-terragen/src/lib.rs:118-139]
[code: rust/bc2-cli/src/terrain_cli.rs:153-216]

### TGEN-GEN-6: Temperate's rough relief is an escarpment over smooth slopes

Status: **verified** · Variants: PC

DICE's rough terrains are rough in few places, very: on MP_004, SP_005 and SP_005_B 1.5 - 2.3 % of the 2 m samples lie
more than 5 m off their 10 m mean, by up to 138 - 201 m (cliffs and escarpments over smooth slopes; MP_003 0.6 % by up
to 62 m; MP_009 0.07 % by 15 m). TG1's generator reached temperate's slopes (p90 56.4, p99 75.3 degrees) with a fifth
of its 10 m / 66 m relief (0.94 / 4.43 m against 4.77 / 13.16). Round TG2's escarpment (`Settings::rough`, temperate
1 by default, 0 for the other biomes): after the relief is scaled, the ground where a smooth mask from the seed (beta
6) lies in its top 30 % is raised by 120 m (x the strength), the face spanning 0.2 % of the ground, then the border is
corrected again. Tried first and dropped (seed 7, rust/BENCHMARKS.md TG2): terraces of 14 m steps over half the
ground (90th slope percentile 67 against 56, relief 1.67 / 5.74 m), a riser on every 60 - 120 m contour (90th
percentile 63 - 79), a mask of beta 3 (its face frayed over a tenth of the ground: 90th percentile 86). Temperate at
seed 7 (1,024): slope p10 / p50 / p90 / p99 6.5 / 26.1 / 57.1 / 79.0 degrees, HI 0.33, relief 4.24 / 13.70 m, beta
3.13 (DICE's rough SP_005: 3.16; a step's spectrum falls as k^-3), border 37.5 m.

[test: rust/bc2-terragen/tests/generated.rs::every_biome_lands_on_its_dice_medians]
[code: rust/bc2-terragen/src/generate.rs:600-645]
[data: round TG2's scratch measure of DICE's MP_003, MP_004, SP_005, SP_005_B, SP_007 and MP_009 with spec/probes/terrain_shape.py's statistics, 2026-11-23, as given]

### TGEN-RING-1: One ring of twelve 64-sample blocks, stitched as DICE stitches its rings

Status: **verified** · Variants: PC

DICE's 12 single-ring terrains (BC1_Oasis, MP_001, MP_006, MP_007, MP_008, MP_009, MP_012, NAM_MP_002R, NAM_MP_003,
NAM_MP_006, NAM_MP_007, SP_006) have twelve blocks of 64 samples, each half the main block's side, tiling the square
twice the main block's side around it, in one order (TERR-HF-12), and are stitched to the main block on all 12
(512 of 512 seam samples, 768 of 768 ring-to-ring samples, the main edges straight within half a stored unit). Round
TG2's ring (`bc2_terragen::ring`): the main block's border continued outwards and blended over a quarter of its side
into distant hills (a beta 4 field from the seed around the border's median, spread 0.4 of the main block's 10 - 90 %
range), then DICE's stitching. Its blocks read in the suite's reader and decode and encode byte for byte
(`terrain_codec::SubBlock`), and the same checker finds them stitched as DICE's: on arid 1,024 and snow 2,048 terrains
512 of 512 and 768 of 768 seam samples, the edges within 0.5. The ring's median lies 6 - 63 m from the border's
(DICE's -77 .. +79 m) with a 10 - 90 % spread of 40 - 372 m (DICE's 25 - 432 m). The visual terrain gives each ring
block a 4 x 4 patch map (TGEN-VIS-3), the water covers the ring (TGEN-WATER-1). `Settings::ring = false` (`--no-ring`)
writes the main block alone, as 14 of DICE's 30.

[test: rust/bc2-terragen/tests/ring.rs::a_generated_ring_reads_back_stitched_as_dices]
[test: rust/bc2-terragen/tests/ring.rs::dices_single_ring_terrains_are_stitched_so]
[code: rust/bc2-terragen/src/ring.rs:1-150]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]

### TGEN-MAT-1: Material names come from the donor level's grid

Status: **implemented** · Variants: PC

A generated material map names 1 - 16 materials, `defaultmaterial` first (TERR-MM-4), every one a `MaterialData` of the
material grid of each level that uses the terrain (MAT-GRID-2): the donor level's grid, which the export carries. The
prototype's per-biome lists are names DICE's terrains of that biome use (TGEN-BIOME-2): ground, rock, sea bed and a
slope material.

[code: rust/bc2-terragen/src/biome.rs:114-122]

### TGEN-MAT-2: Materials are painted from height and slope; visual layers follow the same rules

Status: **implemented** · Variants: PC

The prototype paints each 0.5 m cell from the height field: under the water the sea bed, over 35 degrees stone or rock,
25 - 35 degrees gravel or dirt, else the biome's ground; the visual terrain's rule layers make the same split in the
shader (TGEN-VIS-1), so what a soldier walks on and what is drawn agree. Round TG4 lays the donor's tiled layers and paints
their paired materials under them (TGEN-MAT-3); the biome's other materials by noise and distance to water, and roads
and paths as both (RD1), are later.

[code: rust/bc2-terragen/src/paint.rs:27-53]
[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]

### TGEN-MAT-3: The donor's tiled layers are laid on new ground as the donor lays them

Status: **verified** · Variants: PC

Round TG4 (`bc2_levelwrite::newterrain::paint_tiled`): each tiled layer of the donor (its weights read back from its
patch maps and mask atlas, `terrainpaint::layer_weights`) is measured on the donor's own ground - its mean weight
over the dry ground, its weight's share per slope and height-over-water band (TGEN-VEG-2's bands) over the ground's,
and its paired material (TEDIT-MM-2, when over half of its strong mask lies on one) - and laid on the new terrain: a
seeded value noise (four octaves, 128 - 16 m) plus the logs of the layer's preferences, its edge 0.5 wide, the
threshold found so its mean weight over the dry ground is the donor's. The layers go through E5-rest's paint
(`terrainpaint::paint`: each reached cell the donor's shader of its new combination, or a clone, TEDIT-SH-1 / -2;
tiles in free atlas slots, TEDIT-ATL-1 / -2): a layer it refuses on its own (no shader to clone) is left out, and
when the atlas has no room every layer's coverage is cut by a third until it fits. Where a layer's weight is 200 or
more its paired material is painted in the material map (the name added when the map lacks it: the donor's names
are its level grid's, MAT-GRID-2), so what a soldier walks on follows what is drawn. A donor whose layers are not
one kind each (TEDIT-VT-2: 16 of DICE's 23) paints nothing and draws with its rule layers (TGEN-VIS-1).

MP_005 (paintable: its layers one kind each) as the donor of a generated arid 1,024 terrain (seed 7, round TG4's
export test): Dirtfloor, DryGrass, Rubble and Trash laid over 307 cells in 21 blocks, 47 tiles written (105 atlas
slots left), no shader cloned, 42 cells drawn by a shader with more tiled layers (their extra masks empty); Asphalt
left out (its cells with the sea floor need `Dunes_Asphalt_Seafloor_Destruction`, which MP_005 has not and cannot
clone); `Dirt` added to the material map for Dirtfloor's pair; `export::check` passes XVAL-TERR-4 (every patch map's
shader in each staged database). How the painted layers look in game is unproven until a playtest.

[test: rust/bc2-levelwrite/tests/vegetation.rs::a_generated_level_with_vegetation_exports_and_checks]
[code: rust/bc2-levelwrite/src/newterrain.rs:889-1276]
[code: rust/bc2-levelwrite/src/terrainpaint.rs:842-885]

### TGEN-VEG-1: Vegetation and props are placed as the level's model groups

Status: **implemented** · Variants: PC

Undergrowth needs no placement: it grows from the donor's patch shaders (VEG-UG-1, VEG-UG-3). Trees, bushes, rocks
and natural props are the level's model-group members (VEG-CLS-1, VEG-CLS-2), and a new terrain's are its base
level's placed again (round TG4, `bc2_levelwrite::vegetation`): a member's kind is its group's and its blueprint's -
every member of the forest (`ForestEntityData`) a tree, of a middlegrowth group a bush, a static or rigid group's
member a rock when its blueprint is named rock, cliff, stone or boulder (TGEN-BIOME-2) and a prop when its blueprint
lies under `vegetation/`, `objects/vegetation/` or a terrain's folder (stumps, logs, dead plants); every other member
(buildings, man-made props, gameplay) stays where the base has it (TG5 / A3). The base's member types of the kinds
are the placement's types: no type is added (a new type needs its collision in the group's physics), each type's
share of its kind is the base's, and each keeps at least one instance (DICE ships no type without one, LEVEL-MG-7).
The placement is written as round SG1's member edits after the project's own (each type's members moved first, so
they keep their leaves, then the rest deleted or its first copied: XLOG-MEM-2), so the export's group staging gives
the added members leaves, lays the groups' physics out again and reslots the cull trees (XCULL-RES-1) as for any
member edit, and `export::check`'s group rules hold them (TGEN-VEG-4). Each placed member brings its physics by its
type (COLL-MG-2). A Create Level project asks for it with `terrain_import.vegetation` (project format 14; the
editor's Vegetation box, on by default): density (a scale of every band's count) and seed (the terrain's by default).

[code: rust/bc2-levelwrite/src/vegetation.rs:23-45]
[code: rust/bc2-levelwrite/src/vegetation.rs:378-590]
[code: rust/bc2-levelwrite/src/export.rs:1566-1624]
[inferred: VEG-CLS-1 / -2, LEVEL-MG-7, XLOG-MEM-2, COLL-MG-2, XCULL-RES-1]

### TGEN-VEG-2: DICE's vegetation per biome: dense in the playable area, clustered, by slope and height

Status: **verified** · Variants: PC

Measured on DICE's 19 multiplayer terrains (the 30 of TGEN-BIOME-1 that have multiplayer levels; per terrain its
multiplayer level with the most forest and middlegrowth instances, the playable area the union of all its
multiplayer levels' combat areas): per kind, the instances per hectare of dry ground inside the playable area and in
bands 0 - 50, 50 - 150, 150 - 300, 300 - 600 and beyond 600 m outside it; the nearest-neighbour distance; the
neighbour ratio (Ripley's K over the occupied 64 m cells' density) at 3, 10, 30 and 100 m; the preference for slope
(0 - 5, 5 - 10, 10 - 20, 20 - 30, 30 - 45, over 45 degrees) and for height over the water (0 - 2, 2 - 10, 10 - 30,
30 - 60, 60 - 120, over 120 m), each the instances' share over the ground's; the up axis's tilt from the vertical and
from the ground's normal; the yaw; the scale; the height over the ground. The biome's statistics are the medians
over its terrains (`bc2_terragen::vegetation::BiomeStats::dice`, the table the game tier measures again and holds
equal):

| Biome | Kind | Terrains placing | Count | Per ha inside / 0-50 / 50-150 / 150-300 m | NN p10 / p50 / p90 m | K at 3 / 10 / 30 m | Slope preference 0-5 / 5-10 / 10-20 / 20-30 / 30-45 / 45+ | Tilt p50 / p90 deg |
|---|---|---|---|---|---|---|---|---|
| arid | forest | 1.00 | 444 | 14.8 / 2.6 / 0.6 / 0.1 | 1.7 / 6.2 / 16.3 | 4.65 / 2.32 / 1.57 | 0.77 / 1.13 / 1.26 / 1.40 / 1.07 / 0.38 | 2 / 11 |
| arid | middlegrowth | 1.00 | 576 | 20.3 / 2.5 / 0.4 / 0.0 | 2.2 / 5.1 / 15.9 | 5.98 / 4.09 / 2.31 | 0.90 / 1.10 / 1.26 / 1.18 / 0.68 / 0.34 | 1 / 14 |
| arid | rock | 0.67 | 68 | 2.5 / 0.3 / 0.0 / 0.0 | 4.0 / 14.6 / 108.2 | 3.76 / 2.60 / 1.38 | 0.27 / 0.80 / 1.20 / 1.31 / 2.62 / 0.68 | 78 / 118 |
| arid | prop | 0.33 | 66 | 2.5 / 0.0 / 0.0 / 0.0 | 2.8 / 11.0 / 27.9 | 6.57 / 1.87 / 1.41 | 1.59 / 0.00 / 0.10 / 0.00 / 0.00 / - | 0 / 3 |
| jungle | forest | 1.00 | 1534 | 87.9 / 62.6 / 2.0 / 0.6 | 1.3 / 2.6 / 7.0 | 4.60 / 2.52 / 1.54 | 0.60 / 0.95 / 1.23 / 1.50 / 1.08 / 0.57 | 0 / 2 |
| jungle | middlegrowth | 1.00 | 1649 | 99.6 / 38.0 / 1.0 / 0.0 | 1.5 / 2.9 / 5.2 | 4.03 / 2.47 / 1.68 | 0.73 / 1.10 / 1.18 / 1.40 / 1.04 / 0.49 | 0 / 16 |
| jungle | rock | 0.67 | 59 | 4.7 / 0.0 / 0.0 / 0.0 | 4.2 / 16.9 / 36.3 | 2.99 / 1.40 / 1.06 | 0.52 / 1.18 / 1.61 / 1.36 / 1.43 / 0.07 | 2 / 37 |
| jungle | prop | 0.33 | 2 | 0.1 / 0.0 / 0.0 / 0.0 | 3.0 / 3.0 / 3.0 | 0.00 / 6.19 / 0.69 | 2.43 / 0.00 / 0.00 / 0.00 / 0.00 / 0.00 | 0 / 0 |
| snow | forest | 1.00 | 2764 | 85.3 / 70.8 / 40.6 / 15.8 | 2.5 / 4.4 / 7.3 | 1.24 / 2.11 / 1.70 | 0.71 / 1.05 / 1.23 / 1.24 / 0.91 / 0.33 | 0 / 0 |
| snow | middlegrowth | 1.00 | 942 | 67.6 / 22.2 / 6.1 / 5.0 | 2.4 / 4.9 / 10.3 | 2.06 / 2.33 / 1.83 | 0.79 / 1.22 / 1.42 / 1.34 / 0.89 / 0.36 | 1 / 2 |
| snow | rock | 1.00 | 135 | 14.3 / 1.4 / 0.0 / 0.0 | 2.3 / 6.1 / 21.2 | 4.81 / 3.37 / 1.76 | 0.47 / 1.17 / 1.46 / 1.45 / 0.90 / 0.36 | 42 / 96 |
| temperate | forest | 1.00 | 1359 | 44.6 / 27.9 / 5.5 / 1.3 | 2.2 / 4.0 / 8.9 | 3.60 / 3.12 / 1.74 | 0.57 / 0.87 / 1.21 / 1.33 / 1.07 / 0.58 | 1 / 4 |
| temperate | middlegrowth | 0.75 | 1387 | 25.7 / 6.2 / 1.4 / 0.1 | 0.8 / 2.4 / 10.8 | 9.77 / 4.18 / 2.25 | 0.68 / 0.88 / 1.09 / 1.13 / 1.19 / 0.54 | 0 / 9 |
| temperate | rock | 0.25 | 1 | 0.0 / 0.0 / 0.0 / 0.0 | 0.0 / 0.0 / 0.0 | 0.00 / 0.00 / 0.00 | 0.00 / 0.00 / 12.19 / 0.00 / 0.00 / 0.00 | 8 / 8 |
| temperate | prop | 0.25 | 110 | 3.1 / 0.0 / 0.0 / 0.0 | 1.3 / 3.5 / 13.6 | 24.93 / 6.79 / 3.07 | 2.06 / 0.90 / 0.15 / 0.09 / 0.00 / 0.00 | 6 / 97 |
| vietnam | forest | 1.00 | 363 | 41.3 / 7.7 / 0.4 / 0.0 | 1.5 / 5.5 / 14.9 | 3.53 / 2.10 / 1.32 | 0.52 / 0.90 / 1.24 / 1.17 / 0.89 / 0.57 | 5 / 13 |
| vietnam | middlegrowth | 1.00 | 1119 | 116.4 / 17.1 / 0.7 / 0.0 | 1.3 / 2.7 / 7.2 | 4.34 / 2.42 / 1.63 | 0.73 / 0.76 / 1.13 / 1.31 / 1.21 / 0.53 | 0 / 20 |
| vietnam | rock | 1.00 | 68 | 13.1 / 1.1 / 0.0 / 0.0 | 6.3 / 10.5 / 30.3 | 0.86 / 2.06 / 0.92 | 0.41 / 0.82 / 0.91 / 1.20 / 1.41 / 0.58 | 85 / 117 |

DICE never scales a member (every axis 1.0 long on all 19 terrains: there is no scale range to learn), turns its
trees and bushes about the vertical with no favoured direction (the yaw's mean resultant length 0.03 - 0.42; tilt
p50 0 - 15 degrees, nearer the vertical than the ground's normal on every terrain) and its rocks freely (tilt p50
40 - 120 degrees outside the jungle: a rock's pose is its own), sinks its rocks 0.4 - 4.4 m (medians) and stands
its trees and bushes on the ground (median within 0.3 m but MP_003's and SP_005's sunk trees). Everything falls off
steeply outside the playable area (forest: 14.8 / 2.6 / 0.6 per hectare inside / to 50 m / to 150 m on arid; snow's
forests the exception, still 41 per hectare 50 - 150 m out), is clustered (neighbour ratios 1.2 - 10 at 3 m and
1.9 - 4.2 at 10 m, Clark and Evans 0.4 - 0.9), avoids flat ground under 5 degrees (preference 0.5 - 0.9) and
slopes over 45 (0.3 - 0.7) and likes 20 - 30 degrees (1.2 - 1.5). Props are rare (one or two terrains a biome).

[test: rust/bc2-levelwrite/tests/vegetation.rs::dice_biome_statistics]
[code: rust/bc2-terragen/src/vegetation.rs:490-690]
[data: rust/bc2-levelwrite/tests/vegetation.rs on the game copy, 2026-11-27: 19 terrains (arid 3, jungle 3, snow 4, temperate 4, vietnam 5), each kind's statistics per terrain and the medians as tabled; rust/BENCHMARKS.md TG4]

### TGEN-VEG-3: The placement falls on its biome's statistics

Status: **verified** · Variants: PC

`bc2_terragen::vegetation::place_biome`: per kind DICE's biome places and the level's types hold, as many instances
in each band as the band's DICE density gives on the new terrain's dry ground (outside the level's combat areas; a
level with none takes the central square of half the side, TGEN-PLAY-1), each from a cluster (a Thomas process: a
parent uniform on the band's dry ground, offspring at a Gaussian spread around it) kept with the ground's slope and
height preference (over its largest; each cluster as much larger as the thinning keeps less) and when no other
instance of the kind is nearer than half DICE's 10th-percentile nearest-neighbour distance; its type by the base's
shares, its pose one of the type's own DICE instances' on the base's ground (its axes in the vertical frame, or the
normal's when the type's instances stand nearer the normal, and its height over the ground; instances more than 1 m
over the ground, on structures, left out) turned to a uniform yaw. The cluster's count and spread start from the
Thomas fit to DICE's neighbour ratios and are calibrated on the new ground in six rounds (the count by the
neighbour ratio's excess at 10 m, the spread by the nearest-neighbour median; the round nearest DICE's kept), since
the thinning and the bands change a Thomas process's clustering. Every random value is a seeded stream's
(TGEN-GEN-1): the same terrain, types and seed give the same instances.

On generated 1,024 terrains of every biome (seed 7, the central square; three types a kind), measured as DICE's
levels are (generated / DICE's median; 632 - 5,224 instances in 1.9 - 3.2 s a terrain): the playable area's density
within 10 % (a fifth for kinds of under 100 instances), the nearest neighbour within a factor 1.6 and the neighbour
ratio's excess at 10 m within a factor 3 for every kind of 50 or more (the test's bounds). The slope medians differ
by the ground (a generated snow terrain is steeper than DICE's playable areas: TGEN-PLAY-1's flatter playable area
is TG5's); the preference is DICE's.

| Biome | Kind | Per ha inside (generated / DICE) | NN p50 m | K at 10 m | K at 30 m | Slope p50 deg |
|---|---|---|---|---|---|---|
| arid | forest | 14.5 / 14.8 | 5.9 / 6.2 | 4.22 / 2.32 | 1.43 / 1.57 | 7.8 / 10.3 |
| arid | middlegrowth | 18.8 / 20.3 | 5.1 / 5.1 | 4.31 / 4.09 | 2.03 / 2.31 | 5.5 / 8.4 |
| arid | rock | 2.2 / 2.5 | 17.3 / 14.6 | 2.58 / 2.60 | 0.65 / 1.38 | 10.3 / 19.9 |
| arid | prop | 2.4 / 2.5 | 10.2 / 11.0 | 2.65 / 1.87 | 1.22 / 1.41 | 3.2 / 0.8 |
| jungle | forest | 88.1 / 87.9 | 3.4 / 2.6 | 3.53 / 2.52 | 2.42 / 1.54 | 20.4 / 19.8 |
| jungle | middlegrowth | 98.2 / 99.6 | 3.3 / 2.9 | 3.02 / 2.47 | 2.31 / 1.68 | 17.8 / 20.2 |
| jungle | rock | 4.5 / 4.7 | 12.3 / 16.9 | 1.28 / 1.40 | 1.53 / 1.06 | 14.1 / 16.5 |
| jungle | prop | 0.1 / 0.1 | 2.0 / 3.0 | 5.79 / 6.19 | 0.64 / 0.69 | 4.8 / 0.0 |
| snow | forest | 78.9 / 85.3 | 4.6 / 4.4 | 2.12 / 2.11 | 1.95 / 1.70 | 28.7 / 18.2 |
| snow | middlegrowth | 65.5 / 67.6 | 5.4 / 4.9 | 3.04 / 2.33 | 2.55 / 1.83 | 24.8 / 17.0 |
| snow | rock | 14.1 / 14.3 | 7.1 / 6.1 | 3.38 / 3.37 | 2.37 / 1.76 | 25.1 / 15.3 |
| temperate | forest | 45.1 / 44.6 | 4.9 / 4.0 | 2.99 / 3.12 | 1.67 / 1.74 | 20.7 / 15.9 |
| temperate | middlegrowth | 25.6 / 25.7 | 2.9 / 2.4 | 3.66 / 4.18 | 1.55 / 2.25 | 20.0 / 14.6 |
| temperate | prop | 3.1 / 3.1 | 3.2 / 3.5 | 14.52 / 6.79 | 2.38 / 3.07 | 4.4 / 1.6 |
| vietnam | forest | 41.1 / 41.3 | 5.6 / 5.5 | 2.24 / 2.10 | 1.53 / 1.32 | 13.2 / 18.2 |
| vietnam | middlegrowth | 115.4 / 116.4 | 2.9 / 2.7 | 2.48 / 2.42 | 1.63 / 1.63 | 13.0 / 13.9 |
| vietnam | rock | 12.8 / 13.1 | 10.2 / 10.5 | 1.91 / 2.06 | 1.66 / 0.92 | 15.7 / 23.4 |

[test: rust/bc2-terragen/tests/vegetation.rs::every_biome_falls_on_dices_statistics]
[test: rust/bc2-terragen/tests/vegetation.rs::a_placement_is_its_seeds]
[test: rust/bc2-levelwrite/tests/vegetation.rs::a_generated_level_with_vegetation_exports_and_checks]
[code: rust/bc2-terragen/src/vegetation.rs:692-1230]

### TGEN-VEG-4: DICE's model groups written again through the member writer are DICE's bytes

Status: **verified** · Variants: PC

The vegetation step's writer is round SG1's member edits. DICE's groups of one multiplayer level per biome (MP_005,
MP_009CQ, MP_007, MP_004, NAM_MP_003CQ: forests, middlegrowth, static and rigid groups) written again through it -
every type's members but its first deleted, then each added back at DICE's transform in DICE's order as a copy of
the first (its `SpawnDatas` record copied, `InstanceCount` and the cumulative part and network numbers made again),
each added member given DICE's leaf - give DICE's group file and level partition byte for byte, and the group's
physics laid out again for the result is DICE's: 17 of 17 groups, 15,699 members, 15,499 of them added again. A copy
takes its source's `SpawnDatas` record, and DICE's records of one type are not all alike: on MP_005's static group 17
records name another `MeshShaderSet` than their type's first, so a copy of the type's first member differed there
(the first version of the test). So the step copies, for each instance it adds, the DICE instance whose pose it took
(its asset item and record), and adds before it deletes.

[test: rust/bc2-levelwrite/tests/vegetation.rs::dice_groups_round_trip_through_the_member_writer]
[code: rust/bc2-levelwrite/src/members.rs:420-560]
[data: rust/bc2-levelwrite/tests/vegetation.rs on the game copy, 2026-11-27: 17 of 17 groups of MP_005, MP_009CQ, MP_007, MP_004 and NAM_MP_003CQ byte for byte (group file, partition, physics), 15,499 members added again, in 200 s]

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

[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]

### TGEN-OUT-2: A generated material map decodes and re-encodes byte for byte

Status: **verified** · Variants: PC

The prototype's material map (2,048 cells a side for a 1,024 m terrain) decodes to the painted cells and its names, and
re-encodes byte for byte with DICE's coding (TERR-MM-2). Round TG1's material maps (the codec's `MaterialMap`)
decode to the painted cells and the biome's names and re-encode byte for byte; an arid terrain uses all four painted
materials (ground, steep, slope, under water).

[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]

### TGEN-OUT-3: A generated water mesh reads in the Rust reader

Status: **verified** · Variants: PC

A one-body water mesh (a quad over the square at the water level) reads in `bfbc2core.water.parse_mesh` (Rust).
Round TG1's (`bc2_terragen::water::water_mesh`) reads in `bc2_formats::water::parse_mesh`: 4 vertices, 2 triangles.

[test: rust/bc2-terragen/tests/generated.rs::a_generated_terrain_reads_back_and_is_its_seeds]
[code: rust/bc2-terragen/src/water.rs:7-23]

### TGEN-OUT-4: The writer canary: DICE's 30 main blocks written again

Status: **verified** · Variants: PC

The prototype's block writer, given a DICE main block's heights, depth map, bounds and header words, writes the
block byte for byte on 30 of 30 with DICE's own envelope; with an envelope it builds itself the file is identical up
to the envelope and the built envelope lies under every sample on 30 of 30. So the writer adds nothing of its own to
a terrain except the envelope, and TG1's Rust writer is held to the same canary. TG1 writes through round E5's codec
(`bc2_formats::terrain_codec`), whose corpus test is the canary in Rust: DICE's 30 main blocks and 30 material maps
decoded and written again byte for byte, 288 sub-terrain quadtrees computed equal to the stored ones; round TG2's
sub-terrain writer (`terrain_codec::SubBlock`) writes DICE's 288 sub-terrain blocks again byte for byte.

[test: rust/bc2-formats/tests/terrain_corpus.rs::dice_terrains_round_trip]
[code: rust/bc2-formats/src/terrain_codec.rs:559-660]

### TGEN-OUT-5: The proof plan

Status: **inferred** · Variants: PC

| Step | Proves | Round |
|---|---|---|
| suite readers: height field (Rust), tail re-encoded, envelope under, material map byte for byte, water mesh, visual terrain (probe's reader; bc2-render's `VisualTerrain` reader in Rust) | TGEN-OUT-1 - 3, TGEN-VIS-1 | TG0 (done, Python); TG1 in Rust |
| the writer canary on DICE's 30 (TGEN-OUT-4), and a DICE terrain through the whole new-terrain export: its NavKit build from the offline samples equals the one from DICE's own terrain | the writer perturbs nothing | TG1, TG3 |
| `export::check` on a stage with a generated terrain (XEXP-CHK-1: every bundle opens, references resolve, the terrain's height field is loaded, shader entries present, cull check) | the level is complete as data | TG3 |
| NavKit from the offline sampler (`navsample_offline.py stage`, the Rust sampler at 0.2 m from NKS-deep C): the build checks (hpdcheck, roundtrip) and NavKit's bot measures on the playable area (`connectivity`, `ai_coverage` over the combat area) | bots can use it | TG3 |
| the editor's game-accurate view (bc2-render: patches, rule layers, undergrowth, water) and screenshots against the donor level's look | it draws | TG2 (done: TGEN-VIS-2, `bc2-render`'s `generated_terrain` test; undergrowth not drawn by the view) |
| in game (when tests resume): terrain_edit.md section 5's step 7 (load, walk the edges, RenderDoc), then collision, materials (footsteps), undergrowth, bots | everything TEDIT-SRC-2 marks "not known" | after the in-game pause |

Unproven until an in-game test: that the engine loads a main block whose envelope is the generator's (TERR-HF-11's
use), a terrain other than the donor's under the donor's rule-layer literals in the engine itself (TGEN-VIS-2 holds in the
game-accurate view), the sub-terrain ring (TGEN-RING-1), the visual water and the colour / occlusion map, a level without the base's
terrain bundle (TGEN-REQ-5), a water mesh of our own (what the game uses it for), that the depth map of 255 lets
craters form as on DICE's ground (TERR-HF-9), and round TG4's vegetation and tiled layers (their look, collision and
the trees' destruction on new ground; the bots among them beyond the path data's measures).

[inferred: terrain_edit.md section 5; XEXP-CHK-1; XNAV-BLD-1 - 4; NavKit's parity target (navkit.md)]

## 7. Feasibility and the rounds that follow

### TGEN-PLAN-1: Verdict per part, and rounds TG1 - TG5

Status: **inferred** · Variants: PC

| Part | Verdict | Effort |
|---|---|---|
| height field writer (main block, ring) | done (round E5's codec, round TG1's generator: TGEN-OUT-1 / -4; round TG2's ring: TGEN-RING-1) | small |
| material map writer | feasible now: TGEN-OUT-2 | small |
| water mesh | done (TGEN-OUT-3; the visual water: TGEN-WATER-1) | small |
| generator to DICE's medians | feasible now (TGEN-GEN-2) | small |
| generator to DICE's tails and skew | done in round TG1 (TGEN-GEN-3 / -4: slope matching, remapping, erosion); temperate's rough relief in round TG2 (TGEN-GEN-6) | medium |
| visual terrain, rule layers only | done (TGEN-VIS-1, -3; drawn in the game-accurate view, TGEN-VIS-2); unproven in game | small |
| visual terrain, tiled layers and atlas | done in round TG4 for donors whose layers are one kind each (TGEN-MAT-3, E5-rest's paint); unproven in game | medium |
| patch / undergrowth shader entries renamed and retargeted in three databases | done (TGEN-VIS-4) | medium |
| colour / occlusion map | done (TGEN-OCC-1: the donor's values placed by sky visibility) | small |
| vegetation and props | done in round TG4 (TGEN-VEG-1 - 4: DICE's statistics per biome, the placement, SG1's member edits); unproven in game | medium |
| export as a new level | done in round TG3 (TGEN-REQ-5, TEDIT-NEW-2, XVAL-TERR-4); unproven in game | medium |
| path data | done in round TG3 (TEDIT-NEW-3: sampled on the stage, built, measured); quality bounded by NavKit's parity | small (wiring) |
| in-game acceptance | blocked by the in-game pause (TGEN-OUT-5) | - |

Rounds: **TG1** (done 2026-11-23: `rust/bc2-terragen`, `bfbc2 terrain generate`; the writers are round E5's codec)
the Rust generator and writers: main block, material map, water mesh, the canary and the readers' round trip, erosion
and remapping tuned to the biome statistics (terrain_shape.py as the scorer); the ring and the visual water move to
TG2. **TG2** (done 2026-11-23) the ring, temperate's rough relief and the visual side: one donor's patch and undergrowth
shaders renamed into a new terrain's databases, rule-layer patch maps, the visual water, the colour / occlusion map,
the game-accurate view of a generated terrain (`bc2_render::world::World::load_generated`, the editor's
`gameview::read_generated`); the prototype retired. **TG3** E7's Create Level path A wired to the generator (a "Generate"
choice beside "From a height map": the same `TerrainImport` then written by the generator), the export with the
level's own terrain bundle, `export::check`, NavKit from the offline sampler. **TG4** (done 2026-11-27) vegetation, rocks and
props placed per biome (TGEN-VEG-1 - 4), the donor's tiled layers (TGEN-MAT-3), E5's sculpt and paint working on the
generated terrain (TEDIT-NEW-4). **TG5** gameplay on
the generated terrain through A3 (objectives on its pads, combat areas from its flat region), and the WW2 (pacific)
biome. Each round reports the TGEN-SHAPE / TGEN-PLAY statistics of its output against DICE's.

[inferred: the verdicts from the rules cited per row; the order from the dependencies (writer, then drawing, then export, then content)]

## Open questions

- What the engine does with a generated lower envelope and with the depth map's values (TERR-HF-9, TERR-HF-11): a
  playtest (terrain_edit.md section 5).
- Which heights a donor's rule layers read as literals (TGEN-VIS-2: they shade by height in DICE's own shaders as the
  game-accurate view draws them; whether the engine passes the level's water height anywhere): a RenderDoc capture of
  a donor's sea-floor patch with the water moved.
- Why the generated terrain's shallow sea floor draws 1.74 times as bright as MP_005's (TGEN-VIS-2): its flattened
  floor's normals or a layer weight by depth.
- Whether a level may leave out its base's terrain bundle (TGEN-REQ-5) and load its own alone.
- Whether main blocks other than 1,024 / 2,048 samples of 1 m load (TGEN-REQ-2).
- The engine's use of the water mesh (swimming, boats) for a body the generator writes.
