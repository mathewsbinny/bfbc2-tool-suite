# Water: WaterMesh and VisualWater

A level's water is two resources in its `terrain-00.fbrb`: `<Terrain>.Water.Mesh.res` (type `WaterMesh`, the
water bodies' triangles: what the physics and the suite's tools use) and `<Terrain>.Water.res` (type
`VisualWater`, the drawn copy: vertices with normals and tangents, the shader slots and draw ranges). PC values
are little-endian; the Xbox 360 files are big-endian with a narrower VisualWater vertex.

The suite reads both on the PC (`rust/bc2-formats/src/water.rs`), draws the visual water
(`rust/bc2-render/src/water.rs`) and converts the Xbox 360 files (`rust/bc2-convert/src/terrain.rs`).

## 1. WaterMesh

### WATER-MESH-1: Layout

Status: **verified** · Variants: PC, Xbox 360

| Part | Type | Content |
|---|---|---|
| bodies | u32 | the number of water bodies |
| per body: vertex count v | u32 | |
| per body: vertices | v x 3 x f32 | x, y, z |
| per body: index count c | u32 | a multiple of 3 |
| per body: indices | c x u32 | triangles, indices into this body's vertices |

The file ends exactly after the last body; leftover bytes, an index count that is not a multiple of 3, or a
field past the end make the file unreadable.

[code: rust/bc2-formats/src/water.rs:128-151]
[test: rust/bc2-formats/src/water.rs::bodies_are_joined]
[test: rust/bc2-formats/src/water.rs::damaged_meshes]
[test: core/tests/test_water.py::main]
[data: game copy: 53 water meshes, each read to its last byte; every one has 1 body]

### WATER-MESH-2: Joining bodies

Status: **implemented**

Readers that join the bodies into one mesh move each body's indices past the vertices of the bodies before it
(indices wrap at 2^32, as u32 arithmetic does). The file itself stores body-local indices.

[code: rust/bc2-formats/src/water.rs:141-144]
[test: rust/bc2-formats/src/water.rs::bodies_are_joined]

### WATER-MESH-3: Xbox 360 WaterMesh

Status: **verified** · Variants: Xbox 360

The Xbox 360 WaterMesh is the PC layout with every 4-byte value reversed: the file is all 4-byte values, so the
conversion swaps each word. A file whose length is not a multiple of 4 is refused.

[code: rust/bc2-convert/src/terrain.rs:193-204]
[test: rust/bc2-convert/src/terrain.rs::water_mesh_swaps_words]
[test: bfbc2-console-convert/tests/test_console_f2a_native.py::quick]

## 2. VisualWater

### WATER-VIS-1: Layout

Status: **verified** · Variants: PC, Xbox 360

| Part | Type | Content |
|---|---|---|
| format | u32 | 10 |
| stride | u32 | vertex stride: 36 on the PC, 28 on the Xbox 360 |
| vertex bytes | u32 | |
| vertices | vertex bytes | WATER-VIS-2 |
| index bytes | u32 | |
| indices | index bytes / 4 x u32 | |
| slot count s | u32 | 4 in BFBC2's levels |
| slots | s x (shader entry name, second shader name) | NUL-terminated strings; the second is often empty |
| fragment | string | the water fragment shader's entry name |
| range count r | u32 | |
| ranges | r x 4 x u32 | slot, triangle count, first index, index count |
| far plane | f32 | 8192 (6000 on SP_004_B alone) |

[code: rust/bc2-formats/src/water.rs:161-210]
[test: rust/bc2-formats/src/water.rs::shaders_per_triangle]
[test: rust/bc2-formats/src/water.rs::damaged_visuals]
[test: bfbc2-console-convert/tests/test_terrain.py::visual_water]
[data: game copy: 52 visual waters, (format, stride) = (10, 36) on all 52]
[doc: bfbc2-console-convert/terrain.py:59-68]

### WATER-VIS-2: Vertex formats

Status: **verified** · Variants: PC, Xbox 360

| Platform | Stride | Vertex |
|---|---|---|
| PC | 36 | 4 x f16 position, 4 x f16 normal, 4 x f16 tangent, 6 x f16 |
| Xbox 360 | 28 | 4 x f16 position, u32 normal (DEC3N), u32 tangent (DEC3N), 6 x f16 |

DEC3N: x, y, z in the low 30 bits as three signed 10-bit values divided by 511, w in the top 2 bits. Converting
to the PC writes each as f16; a zero vector (all 30 low bits 0) becomes (-1.5, -1.5, -1.5, 0) as DICE's PC files
have it. The PC's vertex shader reads the position as half4, normal and tangent at bytes 8 and 16, a half2 texture
coordinate at 24 (the vertex layout DICE compiled for water).

[code: rust/bc2-convert/src/terrain.rs:542-557]
[code: rust/bc2-convert/src/terrain.rs:606-633]
[test: rust/bc2-convert/src/terrain.rs::dec3n_zero_vector]
[test: bfbc2-console-convert/tests/test_terrain.py::visual_water]
[data: MP_005: the PC visual water is 15,216 bytes larger than the Xbox's, 8 bytes more per vertex over 1,902 vertices; across 20 levels everything but normals and tangents converts byte for byte (whole files on 6), and 91 % of the decoded components are exact, the rest within 1 / 511]

### WATER-VIS-3: Slot strings

Status: **implemented**

Each slot's names are read up to their 0 byte, with one more 0 byte skipped after each slot's first name when
present (the empty second name); the fragment shader's name follows the last slot. Bytes outside ASCII are read
as U+FFFD by the suite.

[code: rust/bc2-formats/src/water.rs:172-200]
[test: rust/bc2-formats/src/water.rs::damaged_visuals]

### WATER-VIS-4: The two files' indices agree

Status: **verified**

The VisualWater index buffer is the WaterMesh's triangles, index for index, on every level with water but SP_04.
Each draw range then gives triangles `first index / 3` to `first index / 3 + triangle count` its slot's shader. A
range past the mesh is cut at its end; a triangle no range covers has no shader. On SP_04 the two files disagree
(the visual water has 4,818 vertices for the mesh's 18) and no shader is assigned per triangle. SP_006 and SP_009
have no water.

[code: rust/bc2-formats/src/water.rs:216-235]
[test: rust/bc2-formats/src/water.rs::shaders_per_triangle]
[test: core/tests/test_water.py::main]

### WATER-VIS-5: World positions as stored

Status: **implemented**

The visual water's positions are world positions: its vertex shader reads the view-projection matrix and no world
matrix. The engine supplies the scene's linear depth (`DepthBuffer`, view distances), `invScreenSize`,
`viewportZMinMax`, `time`, `g_screenSizeInv` and `g_depthScaleFactors` (the projection's factors: the shader turns
its depth into a view distance as `y / (z - x)`).

[code: rust/bc2-render/src/water.rs:1-22]
[test: rust/bc2-render/src/water.rs::depth_factors_invert_the_projection]

### WATER-VIS-6: Open water's vertices are their position, up, along x, and x / 512, z / 512

Status: **verified** · Variants: PC

A visual water decodes whole and encodes byte for byte (`bc2_formats::water::VisualWaterFile`: each slot a pair of
names, the second empty but the ocean slot's water fragment shader; 30 of 30, round TG2). Every PC vertex whose
normal is (0, 1, 0, 0) and tangent (1, 0, 0, 0) - the open water, away from the shore strips - is its position (x, y,
z, 1), that normal and tangent, the texture coordinate (x / 512, z / 512) and four zeros, as halves: 23,820 of 23,820
such vertices over DICE's visual waters are `water::open_water_vertex` of their position byte for byte. The shore and
river strips (slot 0 on most levels) carry a strip's coordinates instead (0 - 1 across, the length along). A
generated terrain's water is written with it (TGEN-WATER-1).

[test: rust/bc2-formats/tests/terrain_visual_corpus.rs::dice_visual_terrains_and_waters_round_trip]
[code: rust/bc2-formats/src/water.rs:240-360]

## Open questions

- The meaning of VisualWater's 6 trailing f16 per vertex beyond the half2 texture coordinate the shader reads.
- The final f32 (8192; 6000 on SP_004_B): presumably a far distance or tiling extent; not read by the suite.
- Whether the game reads the WaterMesh at all for gameplay (swimming, boat buoyancy) or only the DBX water
  entities; the carriers of the Pacific War stand on no water because the open sea is not in the visual water.
- What the format word (10) selects, and whether other values exist (none seen).
- SP_04's mismatch: which of the two files the game uses for what.
