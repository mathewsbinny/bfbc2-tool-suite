# Terrain: TerrainHeightfield, TerrainMaterialMap, VisualTerrain, Terrain

A level's terrain lives in its `terrain-00.fbrb` as four resource types: the height field (one main block and,
on some levels, a ring of sub-terrain blocks), the material map (which physics material each terrain cell has),
the visual terrain (roads, the patch shaders and the patch maps that place the mask tiles) and a zero-byte
`Terrain` entry. Water (`WaterMesh`, `VisualWater`, in the same bundle) is in [water.md](water.md).

All PC values are little-endian; the Xbox 360 and BF1943 files are big-endian with the same grammar unless a rule
says otherwise. The suite reads the PC files (`rust/bc2-formats/src/terrain.rs`, `rust/bc2-render/src/terrain.rs`)
and converts the Xbox 360 / BF1943 ones to them (`rust/bc2-convert/src/terrain.rs`, the reference grammar in
`bfbc2-console-convert/terrain.py`'s docstring).

Counts below are of the game copy (2026-11-19): 527 TerrainHeightfield, 53 TerrainMaterialMap, 97 VisualTerrain,
53 Terrain entries with data or markers, all in `terrain-00.fbrb` bundles.

## 1. Names and bundles

### TERR-NAME-1: Terrain resources live in the level's terrain bundle

Status: **observed**

Every TerrainHeightfield, TerrainMaterialMap, VisualTerrain, WaterMesh and VisualWater entry with data is in a
bundle named `terrain-00.fbrb` (the level's own, or a shared `_common` level's). Names follow
`Terrains/<Terrain>/<Terrain>.Heightfield-<nnn>.res` (block `000` the main block, `001` ... the sub-terrains),
`<Terrain>.Visual.res`, `<Terrain>.Water.Mesh.res`, `<Terrain>.Water.res`; the material map is a plain `.res`.

[data: game copy: 527 / 527 heightfields, 53 / 53 material maps, 97 / 97 visual terrains, 53 water meshes and 52 visual waters, all in terrain-00.fbrb; heightfield suffixes -000 (71 files) to -036]
[doc: rust/bc2-formats/src/terrain.rs:1-3]

### TERR-NAME-2: The Terrain entry has no data

Status: **observed**

Every entry of type `Terrain` is zero bytes long (but is not a `*deleted*` marker: its type is `Terrain`). The
terrain's settings are in the level's DBX partitions; this entry only names the resource.

[data: game copy: 53 Terrain entries, 53 of them 0 bytes, type Terrain, name `<Terrain>.res`]

## 2. TerrainHeightfield

### TERR-HF-1: Header layout

Status: **verified** · Variants: PC

| Offset | Type | Field |
|---|---|---|
| 0x00 | 6 x f32 | world bounds: min x, y, z, max x, y, z |
| 0x18 | u32 | n: samples per side, a power of two |
| 0x1C | u32 | log2(n) - 3 |
| 0x20 | u32 | log2(n) - 2 on a main block; 0 on a sub-terrain |
| 0x24 | u16 | 0 |
| 0x26 | u16 | n / 256 on a main block (0 on a sub-terrain) |
| 0x28 | u32 | 0 |
| 0x2C | u8 | 0 |
| 0x2D | u32 | A: length in bytes of the tail's row offsets, depth runs and padding (TERR-HF-4; 0 on a sub-terrain) |
| 0x31 | n x n u16 | the heights (TERR-HF-2) |

The header is 49 bytes (0x31); a file shorter than that, or whose n is 0, not a power of two or larger than the
file holds, is not a height field.

[code: rust/bc2-formats/src/terrain.rs:163-195]
[test: rust/bc2-formats/src/terrain.rs::refuses_what_the_reference_refuses]
[test: rust/bc2-formats/src/terrain.rs::tails]
[test: core/tests/test_geometry_native.py::main]
[data: game copy: 527 height fields, header words (n, 0x1C, 0x20, 0x24, 0x26, 0x28, 0x2C) = (64, 3, 0, 0, 0, 0, 0) x 312, (128, 4, 0, 0, 0, 0, 0) x 132, (256, 5, 0, 0, 0, 0, 0) x 12, (1024, 7, 8, 0, 4, 0, 0) x 26, (2048, 8, 9, 0, 8, 0, 0) x 41; 4 exceptions in TERR-HF-6]

### TERR-HF-2: Heights

Status: **verified** · Variants: PC, Xbox 360, BF1943

The heights are n x n u16 values, row by row: rows run along z, columns along x (sample (i, j) is value
`j * n + i`). A value v is the height `min y + v * ((max y - min y) / 65535)`. The grid spacing is
`(max x - min x) / n` along x and `(max z - min z) / n` along z: the bounds span n squares, not n - 1.

The suite computes the height in f64 and rounds to f32 (its retired NumPy reference, bit for bit).

[code: rust/bc2-formats/src/terrain.rs:220-251]
[test: rust/bc2-formats/src/terrain.rs::heights_mesh_and_lookup]
[test: core/tests/test_terrain.py::check]
[data: game copy: grid squares of 1 m on all 71 main blocks; 4, 8, 16, 32, 64 and 128 m on sub-terrains]

### TERR-HF-3: Triangulation along the (i, j)-(i+1, j+1) diagonal

Status: **verified**

Each grid square is split along its (i, j)-(i+1, j+1) diagonal; the height at a point inside it is linear on the
half it falls in. The suite's terrain matches the game's in-game samples within 0.1 m on at least 99.97 % of
samples per level only with this split.

[code: rust/bc2-formats/src/terrain.rs:257-278]
[test: core/tests/test_terrain.py::check]
[doc: DESIGN.md:503]

### TERR-HF-4: Main block tail: the depth map, a min / max tree, a lower envelope

Status: **verified** · Variants: PC, Xbox 360, BF1943

On a main block (n = 1024 or 2048) the heights are followed by four parts, decoded in round SPG2-rest:

| Part | Size | Content |
|---|---|---|
| row offsets | n x u32 | per row of the depth map, where its runs start, in u16 units after the offsets (TERR-HF-9) |
| depth runs | 4 bytes a run | the depth map, run-length coded row by row (TERR-HF-9) |
| padding | 0 - 15 zero bytes | to a multiple of 16 bytes from the start of the heights (0x31) |
| min / max tree | 10,922 u16 at n = 1024; 43,690 at 2048 | TERR-HF-10 |
| lower envelope | 22,363 u16 at n = 1024; 88,412 at 2048 | TERR-HF-11 |

Header word A (0x2D) is the length of the first three parts together (the offsets included). The file ends exactly
after the envelope. Until SPG2-rest the suite split the tail as "n offsets, then A bytes, then a fixed part of 62,474 /
256,012 bytes" (the reader's `Tail::Sections`, unchanged): the same bytes, but that split puts the offsets twice and
cuts the min / max tree 2n values in (A minus 4n bytes is where the tree starts; 62,474 = 2 x (10,922 + 22,363) -
4 x 1024 and 256,012 = 2 x (43,690 + 88,412) - 4 x 2048). On DICE's 30 terrains the depth runs, the padding and the
tree re-encode from the depth map and the heights byte for byte; the envelope is checked as TERR-HF-11 says.

[code: rust/bc2-formats/src/terrain.rs:198-216]
[code: rust/bc2-formats/src/terrain.rs:61]
[test: rust/bc2-formats/src/terrain.rs::tails]
[test: core/tests/test_spec_probes.py::height_field_tails]
[data: game copy: 71 of 71 main blocks (26 at 1024, 45 at 2048) have exactly this tail's length]
[data: spec/probes/terrain.py, 2026-11-22: DICE's 30 main blocks: offsets, runs, padding and tree re-encoded byte for byte on 30 of 30; A = 4n + runs + padding on 30]

### TERR-HF-5: Sub-terrain tail: a min / max quadtree

Status: **verified** · Variants: PC, Xbox 360, BF1943

A sub-terrain block (the ring of `Heightfield-001` and up around the main block) has A = 0, header word 0x20 = 0,
and after its heights a min / max quadtree of 16-sample cells: (min u16, max u16) per node, the root first, then
each level's nodes, `1 + 4 + ... + (n / 16)^2` nodes (21 at n = 64, 85 at 128, 341 at 256). The file ends there.

[code: rust/bc2-formats/src/terrain.rs:152-155]
[code: rust/bc2-formats/src/terrain.rs:211-214]
[test: rust/bc2-formats/src/terrain.rs::tails]
[data: game copy: 456 of 456 sub-terrain blocks (312 at n = 64, 132 at 128, 12 at 256) have exactly this tail]

### TERR-HF-6: Header values outside the usual pattern

Status: **observed**

Four main blocks of the game copy depart from TERR-HF-1's 0x24 / 0x26 values: three 2048 blocks have 0x24 = 1, one
has 0x26 = 16 (not n / 256 = 8). The suite's reader does not depend on these words; what the game does with them
is not known (Open questions).

[data: game copy: (2048, 8, 9, 1, 8, 0, 0) x 3, (2048, 8, 9, 0, 16, 0, 0) x 1]

### TERR-HF-7: Xbox 360 height field

Status: **verified** · Variants: Xbox 360

The Xbox 360 height field has the PC layout big-endian: each field is swapped by its width (f32 / u32 by 4, u16 by
2, the byte at 0x2C as it is). The Xbox sub-terrains have 128 samples where DICE's PC copies of the same level have
64. The converter checks that A equals the tail's real first-section length.

[code: rust/bc2-convert/src/terrain.rs:146-188]
[code: rust/bc2-convert/src/terrain.rs:114-139]
[test: bfbc2-console-convert/tests/test_terrain.py::main]
[test: bfbc2-console-convert/tests/test_console_f2a_native.py::quick]
[data: BFBC2's Xbox / PC twins: the converted Xbox height field equals the PC file byte for byte on 7 levels; on 13 others the heights differ where DICE edited the PC terrain, the structure is the same]

### TERR-HF-8: BF1943 height field header

Status: **verified** · Variants: BF1943

BF1943's height field header has no A field: the heights start at 0x2D, not 0x31. Its 0x28 and 0x2C hold other
values than BFBC2's (Wake Island: 707 and 0x40). Converting to BFBC2 computes A from the tail and writes 0x28 and
0x2C as BFBC2 has them (0). The tails are the same grammar (the fixed part's last block holds 4,098 values in both
games).

[code: rust/bc2-convert/src/terrain.rs:154-182]
[test: bfbc2-console-convert/tests/test_console_f2a_native.py::heightfield_x]
[doc: bfbc2-console-convert/terrain.py:24-26]

### TERR-HF-9: The depth map: one byte per sample, run-length coded by rows

Status: **verified** · Variants: PC

The tail's first part is an n x n map of bytes over the main block's samples (rows along z, as the heights), stored
row by row as runs: each run is two u16, (count, value), with count 1 - n and value 0 - 255; a row's runs add up to n
samples exactly and are maximal (two neighbouring runs never have the same value); row r's runs start at the u16 index
`offsets[r]` (the first 0, each row right after the one before). It is the editor's depth map: the main block's
`TerrainHeightfieldData.DepthFiles` names `Terrains/<T>/<T>-DepthMap.png` on all 30 DICE terrains (the editor source,
not shipped, like TEDIT-SRC-1's). DICE's maps are mostly 255 (26 terrains) or mostly 0 (MP_001 96 %, MP_004 98 %,
SP_01 100 %, NAM_MP_006 66 %), with soft edges; on MP_005 the 0 cells are the town's building footprints. What the
value limits (presumably how deep a crater may cut, `DefaultFullDepthEnable` being TerrainData's default for it,
DSTR-TERR-4) is not traced in the executable.

[test: core/tests/test_spec_probes.py::height_field_tails]
[data: spec/probes/terrain.py, 2026-11-22: 30 main blocks: every row's runs sum to n, maximal on 30 of 30; share of 255 / 0 per terrain as printed; DepthFiles one DepthMap.png per main TerrainHeightfieldData on 30 terrains]
[inferred: the meaning, from the editor field's name, MP_005's footprints and DSTR-TERR-4; nothing read in the executable]

### TERR-HF-10: The main block's min / max tree

Status: **verified** · Variants: PC

After the padding comes a min / max tree of the heights, as a sub-terrain's (TERR-HF-5) but over the main block:
(min u16, max u16) per node, the root first, then each level's nodes row by row (rows along z), down to cells of 16
samples (64 x 64 cells at n = 1024: 5,461 nodes; 128 x 128 at 2048: 21,845). A node of side s covers its s x s samples
and the row and column after them (the next cell's first, where the block has one), so neighbouring nodes overlap by
one sample and bound the triangles between them. A writer computes it from the heights: on 30 of 30 DICE terrains the
computed tree equals the file's.

[test: core/tests/test_spec_probes.py::height_field_tails]
[data: spec/probes/terrain.py, 2026-11-22: computed trees equal the stored ones on 30 of 30 main blocks]

### TERR-HF-11: The lower envelope: grids of 2 to n / 8 + 1 vertices a side

Status: **verified** · Variants: PC

The tail ends with grids of u16 heights (the same scale as TERR-HF-2), the coarsest first: sides 2, 3, 5, 9, ...,
n / 8 + 1 (2 - 129 at n = 1024, 22,363 values; 2 - 257 at 2048, 88,412), vertex (i, j) at sample (i, j) times
n / (side minus 1), rows along z. Each grid is a surface under the terrain: drawn as triangles split like TERR-HF-3, the finest grid
lies at or under every height sample on 15 of DICE's 30 terrains and on at least 99.48 % of samples on the other 15
(where it pokes through, presumably DICE edited the heights after the envelope was built); coarser grids lie lower
still (the 2 x 2 grid of MP_001 starts 7,600 units, 58 m, under its lowest sample). How DICE computes it is not
reproduced: a vertex is at or under the sample it stands on everywhere but on SP_02, and greedy lowering of the
samples (each square's overshoot taken off its corners) matches only 17 - 24 % of the vertices. Its use is not
traced either; a lower bound of this kind is what a terrain occluder or a coarse collision test needs. A writer of a
new main block (E7) writes any surface that stays under the heights (TEDIT-HF-3).

[test: core/tests/test_spec_probes.py::height_field_tails]
[data: spec/probes/terrain.py, 2026-11-22: sizes 22,363 / 88,412 values on 30 of 30; share of samples at or under each grid per terrain as printed; the finest grid under all samples on 15]
[inferred: stale envelopes from edited heights; the use from the shape of the data (a conservative lower surface per level of detail)]

## 3. TerrainMaterialMap

### TERR-MM-1: Layout

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Part | Type | Content |
|---|---|---|
| width | u32 | 2048 or 4096 |
| block count b | u16 | 32 or 128 |
| b blocks | u32 length, length bytes, 256 x u16 | a block's 256 coded rows (TERR-MM-2), then each row's length in bytes |
| names | u32 length, length bytes, to the end of the file | the material names, each ending in a NUL (TERR-MM-4) |

The names section ends exactly at the end of the file. The Xbox 360 file swaps the u32 / u16 fields and the
256-entry tables; the data bytes and names are the same on both. A block's 256 lengths add up to its byte length.

[code: rust/bc2-convert/src/terrain.rs:208-240]
[code: rust/bc2-convert/src/terrain.rs:246]
[test: bfbc2-console-convert/tests/test_console_f2a_native.py::material_map_x]
[test: bfbc2-console-convert/tests/test_terrain.py::main]
[data: game copy: (width, blocks) = (4096, 128) x 41, (2048, 32) x 12; all 20 BFBC2 Xbox / PC twin levels convert byte for byte; BF1943's Wake Island, Iwo Jima, Guadalcanal read the same way]
[data: spec/probes/terrain.py, 2026-11-22: DICE's 30 material maps: the row lengths add up to each block's length on 30 of 30]

### TERR-MM-2: A row: pairs of 4-bit cells, a repeated byte starts a run

Status: **verified** · Variants: PC

A block is 512 x 256 cells: 256 rows of 512 cells, each row coded on its own. Each byte holds two neighbouring cells'
material indices, the first (lower x) in the low four bits. A byte equal to the byte just before it is not a cell
pair: it is followed by a count byte c, and the pair is repeated c times more, so `X X c` stands for c + 1 pairs (at
most 256, a whole row); after a run the next byte starts afresh. DICE writes every maximal run of two or more equal
pairs as `X X c` (longer than 256 pairs split into such runs) and a single pair as its byte, which re-encodes all 30
DICE maps byte for byte.

[test: core/tests/test_spec_probes.py::material_maps]
[data: spec/probes/terrain.py, 2026-11-22: 30 of 30 DICE material maps decode to 256 pairs on every row and re-encode byte for byte]

### TERR-MM-3: The cells cover the main block, rows along z

Status: **verified** · Variants: PC

The blocks tile a width x width map row by row (width / 512 blocks a row: 8 x 16 blocks at 4,096, 4 x 8 at 2,048);
cell rows run along z and columns along x like the heights (TERR-HF-2), and the map covers the main block's bounds
exactly, so a cell is n / width metres: 0.5 m on 27 of DICE's 30 terrains (4,096 cells on a 2,048 m terrain, 2,048 on a
1,024 m one) and 0.25 m on MP_001, MP_002 and MP_008 (4,096 on 1,024 m). The orientation is the one under which the
sea-bed material lies where the ground is low: on six terrains the correlation of SeaBed cells with height is -0.55 to
-0.89 so, against -0.21 to +0.60 transposed or spread over twice the area.

[test: core/tests/test_spec_probes.py::material_maps]
[data: spec/probes/terrain.py, 2026-11-22: (width, rows) (4096, 4096) x 22, (2048, 2048) x 8; cells per metre 2 x 27, 4 x 3]
[data: SPG2-rest scratch measurement, 2026-11-22: SeaBed against height on mp_001, MP_002, MP_004, MP_005, MP_008, MP_009, for the four placements]

### TERR-MM-4: Indices name materials of the level's grid by name

Status: **verified** · Variants: PC

A cell's index is a position in the map's own names list (the resource's names, the same list as the editor partition
`Terrains/<T>/<T>-MaterialMap`'s `TerrainMaterialMapAsset.MaterialNames`, whose `MaterialMapFileName` names the
editor's `<T>-terrainmaterials.png`). Index 0 is `defaultmaterial` on 30 of 30; maps name 1 - 10 materials (4 bits
allow 16). Every name is a `MaterialData` name of the level's material grid (MAT-GRID-1), matched without case: the
grids of the 31 levels whose bundle holds one terrain hold every name of that terrain's map, never at the same
indices (MAT-GRID-2), so the engine resolves the names to the grid when the level loads. Over DICE's maps the 201
names are 146 of the Terrain category (grass, dirt, sand, tarmac, gravel, seabed, snow, ...), 30 Default
(`defaultmaterial`), 25 Construction (stone 22, concrete 3). Painting a material (TEDIT-MM-1) writes its index; a material the map lacks is
added to the names, and must be in the grid of every level that uses the terrain.

[test: core/tests/test_spec_probes.py::material_maps]
[data: spec/probes/terrain.py and SPG2-rest's scratch scan, 2026-11-22: 30 maps, 1 - 10 names, defaultmaterial first on 30; 31 paired grids hold all names, same indices on 0; the 201 names' categories in DICE's grids as given]

## 4. VisualTerrain

### TERR-VT-1: Layout

Status: **verified** · Variants: PC, Xbox 360, BF1943

Strings are NUL-terminated.

| Part | Content |
|---|---|
| u32 N | terrain blocks: 1, or the main block and its ring of sub-terrains (1 + 12k) |
| 2N road blocks | each block's full-detail roads, then each block's low-detail roads (TERR-VT-2) |
| u32 m, m patch shaders | TERR-VT-3 |
| u32 N, N patch maps | TERR-VT-4; the count must equal the first N |

The file ends exactly after the last patch map.

[code: rust/bc2-render/src/terrain.rs:164-275]
[code: rust/bc2-convert/src/terrain.rs:396-468]
[test: rust/bc2-render/src/terrain.rs::a_minimal_visual_terrain_reads]
[test: bfbc2-console-convert/tests/test_terrain.py::visual_terrain]
[data: game copy: 97 visual terrains, N = 1 x 43, 13 x 43, 25 x 6, 37 x 4, 49 x 1]

### TERR-VT-2: Road blocks

Status: **verified** · Variants: PC, Xbox 360

A road block: u32 n strips, each (shader, second shader, mask shader strings; u32 first index; u32 triangles;
6 x f32 bounds; u32 layer 0-7), then u32 V vertices, u32 vertex bytes (must equal 28 V), u32 I indices; V
vertices of 28 bytes (5 x f32, 4 x f16); I x u32 indices. An empty block is 16 bytes.

[code: rust/bc2-render/src/terrain.rs:164-205]
[test: rust/bc2-render/src/terrain.rs::a_minimal_visual_terrain_reads]
[test: bfbc2-console-convert/tests/test_console_f2a_native.py::visual_terrain_x]

### TERR-VT-3: Patch shaders

Status: **verified** · Variants: PC, Xbox 360

A patch shader: its patch mask shader entry name, its undergrowth mask shader name (may be empty), u32 layer
count (1-8), u32 tiled-mask count (0-4), u32 g groups, each u32 k undergrowth meshes of (mesh name, a second string,
a shader name, 12 x f32, u8 1, u8 0): the fields of the engine's `TerrainUndergrowthType` (VEG-UG-3). The second
string is empty on every DICE terrain but NAM_MP_003, whose 30 entries name `<mesh>/<guid>` there; the suite's reader
took it as one byte until round SPG2-rest and so refused NAM_MP_003's visual terrain (VEG-UG-2). The shader name is
empty on 1,978 of 3,069 entries.

A layer is either **tiled** (it reads a mask-atlas tile; the tiled count says how many) or a **rule** layer (base,
slope, destruction, sea floor: computed, no tile).

[code: rust/bc2-render/src/terrain.rs:245-280]
[test: rust/bc2-render/src/terrain.rs::a_minimal_visual_terrain_reads]
[test: rust/bc2-render/src/terrain.rs::undergrowth_entries_read_their_asset_string]
[test: rust/bc2-render/tests/visual_terrains.rs::every_dice_visual_terrain_reads_with_its_undergrowth]
[doc: rust/bc2-render/src/terrain.rs:1-15]
[data: spec/probes/terrain.py, 2026-11-22: 30 DICE visual terrains, 1,515 patch shaders, 3,069 undergrowth entries, every one ending 01 00; second string set on 30 (NAM_MP_003), shader name on 1,091]

### TERR-VT-4: Patch maps

Status: **verified** · Variants: PC, Xbox 360

One patch map per terrain block:

| Part | Content |
|---|---|
| bounds | 6 x f32 |
| header | 5 x u32: tile size (66), border (1), tiles per row - 1, rows - 1, W cells per side |
| tiles | W x W cells x 4 x u16: the cell's mask-atlas tiles |
| flags | W x W cells x 4 x u8: per mask, 0 where it is not tiled, else the cells its tile spans on a side (8 or 4; TERR-VT-7) |
| shader map | u32 S, then S x S u8: each cell's patch shader index |
| names | the block's height field name, the mask-tile atlas name |

Cells run row by row with rows along z. A cell's non-zero flags equal its patch shader's tiled-mask count, except
that a cell drawn by a rule-only shader may keep one leftover flag (TEDIT-VT-1). W is 128 or 64 for a main block,
4, 8 or 16 (PC; 8 on the Xbox 360) for a sub-terrain; the shader map's side S equals W on every DICE map.

[code: rust/bc2-render/src/terrain.rs:225-268]
[test: rust/bc2-render/src/terrain.rs::a_minimal_visual_terrain_reads]
[data: MP_005: header (66, 1, 15, 7, 128); its 16,384 cells' tiled counts equal their non-zero flags]
[data: spec/probes/terrain.py, 2026-11-22: 318 DICE patch maps; (W, S) = (128, 128) x 19, (64, 64) x 11, (16, 16) x 12, (8, 8) x 84, (4, 4) x 192; 358,282 of 367,872 cells' non-zero flags equal the tiled count, the rest rule-only cells with one flag]

### TERR-VT-5: Tiled layers must match the mask atlas's layout

Status: **observed**

The patch map's header gives the mask atlas's size in 64-pixel tiles minus one (15, 7: a 1024 x 512 atlas). A level
must draw its patch shaders with an atlas of that layout: patch shaders cloned from a donor level whose atlas has
another layout (2048 x 1024 on a 1024 x 512 level) draw every tiled cell near the camera black. A donor must also
have the same tiled count and the same first-layer tiled-ness. Atlas contents, header flags (0 / 2), tile
resolution bytes and `heightmapWorldScaleY` (always 500, set by the engine) were ruled out as the cause.

[data: OPFOR's Wake Island conversion 2026-11-07: an Oasis-layout donor drew black tiled cells, an MP_009-layout donor (1024 x 512) drew correctly]
[inferred: the tile index in each cell is resolved against the atlas's tiles per row and rows, so another layout addresses other tiles]

### TERR-VT-7: A mask tile spans flag x flag cells

Status: **verified** · Variants: PC

A cell's mask flag is the number of cells its atlas tile spans on a side (8 or 4 on the three levels the test reads;
2 to 64 over DICE's 30 terrains: TEDIT-ATL-2), and the cells that name a tile form one block of flag x flag cells
aligned to multiples of the flag in the patch map (on 8 terrains one shared tile is named by several blocks): a cell
reads the part of the tile its place in the block gives (its column `x mod flag`, its row `z mod flag`, each a
`1 / flag` of the tile's 64 inner pixels), so a mask runs on from cell to cell without a seam. Stretching the whole
tile over each cell (the suite's reading until round R5d) repeats the block's mask in every cell and draws ragged
edges at each cell's border (OPFOR's WW2 shores).

[test: rust/bc2-render/tests/r5d.rs::mask_tiles_span_aligned_blocks_and_read_on_across_cells]
[code: rust/bc2-render/src/terrain.rs:776-793]
[data: DICE's MP_009: 757 tiled masks in 51 tiles' blocks, MP_005: 1,033 in 59; OPFOR's Wake Island: 998 in 78 (flags 8 and 4); no tile named outside its block]
[data: spec/probes/terrain.py, 2026-11-22: flags over DICE's 30 terrains 2 x 5,367, 4 x 11,712, 8 x 56,352, 16 x 13,750, 32 x 4,864, 64 x 2,558]

### TERR-VT-6: PC and Xbox 360 visual terrain differ only where DICE edited the PC

Status: **verified** · Variants: PC, Xbox 360

The converted Xbox visual terrain equals the PC file byte for byte on MP_003, MP_005 and MP_005SR; elsewhere every
part equals it but DICE's PC edits (rebuilt roads with patches reordered on MP_001, MP_004, MP_006, MP_008,
MP_012; MP_008's patch shaders; the sub-terrain patch maps, 4 x 4 on the PC against the Xbox's 8 x 8). The
converter keeps the Xbox's 8 x 8 sub-terrain maps, which match the Xbox's 128-sample sub-terrain height fields.

[code: rust/bc2-convert/src/terrain.rs:461-468]
[test: bfbc2-console-convert/tests/test_terrain.py::visual_terrain]

## 5. The other terrain-family resources

### TERR-SOOT-1: SootMesh converts by the same rules

Status: **verified** · Variants: Xbox 360, BF1943

The Xbox 360 `SootMesh` converts to the PC by swapping its fields by width (the converter's `soot_mesh`); an empty
soot mesh is the 16-byte header alone. Its full grammar is not part of this file (the mesh family's file).

[code: rust/bc2-convert/src/terrain.rs:645]
[test: rust/bc2-convert/src/terrain.rs::empty_soot]
[test: bfbc2-console-convert/tests/test_soot.py::main]

## Open questions

- The main block's tail (TERR-HF-4) is decoded; left: what the engine does with the depth map's values
  (TERR-HF-9) and the lower envelope (TERR-HF-11), and how DICE built the envelope (the suite's readers still
  split the tail the old way, `Tail::Sections`; a reader of the four parts is for the round that writes main
  blocks, E7).
- Header words 0x24 (= 1 on three blocks) and 0x26 (= 16 on one 2048 block): what the game reads them as.
- Whether the game accepts main block sizes other than 1024 / 2048 (the converter refuses them; 256-sample blocks
  exist only as sub-terrains in the game copy).
- The material map is decoded (TERR-MM-2 - TERR-MM-4); left: whether the engine reads the cell under a contact
  or interpolates, and what `MaterialMaskStepSize` (1 on all 30) changes.
- The undergrowth entries' fields are named from the executable's type information (VEG-UG-3); which of the
  three flags the two closing bytes are is inferred.
- The engine's rule for a tiled mask's atlas coordinate is the renderer's reading, not confirmed by a capture
  (EDITOR_ARCHITECTURE 3.11).
- The `Terrain` entry: whether the game needs it to exist (it has no data).
