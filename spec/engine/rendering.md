# Rendering: what the PC renderer draws, and how

How BFBC2's Direct3D 11 renderer turns a level's data into a frame: which shader database a level draws with, how a
mesh part becomes a draw, the engine's constants, the draw order and the depth pass, the terrain patches, the water,
the weather's light and sky, the post-process chain and the shadows; and the crashes the renderer is known to give on
bad data. The byte layouts are layer 1 (`formats/shaderdb.md`, `formats/terrain.md`, `formats/water.md`,
`formats/mesh.md`, `formats/texture.md`); this file is what the engine does with them.

**Sources.** Round R5 of the map editor read the game's data and reproduced its frame (`bfbc2-map-editor/EDITOR_ARCHITECTURE.md`
3.11; `rust/BENCHMARKS.md` "R5" and "R5c"): `rust/bc2-render` draws DICE's own compiled shaders on a Direct3D 11
device and is held pixel for pixel against the suite's older C++ harness (`renderqa.py` / `shaderview.cpp`), which
had been compared with the game's look (15 of 16 stock meshes, 33 of 33 terrain patch entries of mp_009cq, DICE's
road strips). **No in-game capture has been taken yet**: the values the engine computes per frame (the sun's
direction, the shadow cascades, the sky pass's constants, the exposure) are the suite's placeholders, listed under
Open questions. A rule about what the engine does is therefore **observed** (data, executable) unless a test checks
the suite's reproduction of it. Crash addresses come from Project OPFOR BFBC2's records. Counts are MP_005's unless
the rule says otherwise.

## 1. Shader databases

### RNDR-DB-1: A level draws with its own database, found by its name

Status: **verified** · Variants: PC

Every mesh, terrain patch, strip and water surface of a level is drawn with a program from the level's own shader
database, `Levels/<LV>/Shaders/Dx11_Single.res` (Dx10 and Dx9 have their own files beside it), found by the level's
name (PART-NAME-3). A level made under a new name needs its own copy of the collection and its databases; programs
for a mesh the database has no entry for cannot be drawn (RNDR-DRAW-6).

[code: rust/bc2-render/src/world.rs:217] [test: rust/bc2-render/tests/world.rs::mp_005_world_draws_and_matches_the_harness]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:596-599] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:841-845]

### RNDR-DB-2: The databases hold compiled code only

Status: **observed** · Variants: PC

The Dx10 / Dx11 databases hold DXBC (SHDB-DXBC-1), the Dx9 database Direct3D 9 shader-model-3 tokens: every Dx9
vertex shader is `vs_3_0` and every pixel shader `ps_3_0`, none shader model 2. No HLSL source ships; all of DICE's
code was built by one compiler, "Microsoft (R) HLSL Shader Compiler 9.27.952.3022". Each Dx11 shader carries its own
reflection (`RDEF`): its `$Globals` variables, textures and samplers by name and register.

[data: MP_005 Dx11_Single.res: 1,714 entries, 12,850 programs, 1,527 vertex and 2,166 pixel shaders, all vs_5_0 / ps_5_0 from that compiler, 2,112 parameter blocks, 114 vertex layouts, 31 render-state words]
[data: Wake Island's built Dx9_Single.res: 1,723 vertex and 4,668 pixel shaders, all vs_3_0 / ps_3_0]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:596-612] [doc: opfor:research/WW2_DX9_SHADER_PORT_PLAN_2026-11-15.md:26-32]

### RNDR-DB-3: Dx9 limits what a program may use

Status: **observed** · Variants: PC

A program for the Dx9 renderer is bound to shader model 3: at most 16 pixel samplers (`s0` is the shadow map in DICE's
programs), constants in `c` registers from the shader's constant table, and on Dx9 the terrain's height is a vertex
stream (`MainBuffer`), not a vertex-texture fetch as on Dx10 / Dx11 (`MainVTex`). A program translated from another
platform that needs more samplers than that (BF1943's 6-7 layer terrain patches) has no Dx9 form; OPFOR's Pacific War
levels draw DICE donor programs there.

[data: Wake Island, Iwo Jima and Guadalcanal: 33 BF1943 terrain patch entries declare 17-22 pixel samplers; DICE's Dx9 programs: s0 the shadow map in 4,360 pixel shaders; patches with stream 0 SHORT2 grid and stream 1 SHORT2N height]
[doc: opfor:research/WW2_DX9_SHADER_PORT_PLAN_2026-11-15.md:33-40] [doc: opfor:research/WW2_DX9_SHADER_PORT_PLAN_2026-11-15.md:60]

## 2. From a mesh part to a draw

### RNDR-DRAW-1: One draw per part: the entry `<mesh>/<shader>`, the main-pass program for the part's layout

Status: **verified** · Variants: PC

For each part of the mesh set's chosen shader set (MSET-LOD-4), the renderer takes the database entry
`<mesh>/<shader>` (SHDB-NAME-1) and, among its programs (SHDB-PROG-1), the main-pass one (not depth-only, not
instanced) for the part's vertex layout (SHDB-NAME-3, SHDB-LAY-1); a program with a point light is a variant of the
same draw. The program gives the vertex and pixel shader, the input layout, the parameter blocks (constants,
textures, the pixel shader's sampler states) and the render state.

[code: rust/bc2-render/src/material.rs:503-528] [code: rust/bc2-render/src/material.rs:674-695]
[test: rust/bc2-render/tests/warehouse.rs::warehouse_as_the_harness_draws_it]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:610-616]

### RNDR-DRAW-2: A placement may choose another shader set

Status: **verified** · Variants: PC

A placed object's `MeshShaderSet` (and, in a model group, each instance's own `SpawnDatas` record) names one of the
mesh set's shader sets other than the default by the set's own name `<partition>/<GUID>`, the very string the mesh
set lists; the draw then uses that set's shaders (a container's colour plates, a fence's tarp).

[code: rust/bc2-render/src/placement.rs:84-155] [test: rust/bc2-render/tests/world.rs::mp_005_world_draws_and_matches_the_harness]
[data: MP_005: 554 of 3,915 drawn objects and members with an override (31 concrete blocks, 193 fences among the group members)]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:719-723]

### RNDR-DRAW-3: Constants by name, material values by register

Status: **verified** · Variants: PC

A shader's constant buffer is filled from its own reflection: each engine value by its variable name
(`viewProjMatrix`, `worldMatrix`, `cameraPos`, `outdoorLightDir`, `outdoorLightKeyColor`, `boneMatrices`, `time`,
`invScreenSize` ...; the binding tables, SHDB-BIND-1, say which engine value a register takes), the material's
constants from the program's second parameter block by register (SHDB-PB-1). Textures the engine binds itself
(`heightmap`, `maskTileMap`, `DepthBuffer`, the shadow map, `outdoorLightReflectionEnvmap`) are named in the register
tables (SHDB-REG-1).

[code: rust/bc2-render/src/frame.rs:192-228] [test: rust/bc2-render/src/frame.rs::cbuffer_places_values_by_name_and_material_by_register]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:601-609]

### RNDR-DRAW-4: Engine values written over block placeholders

Status: **implemented** · Variants: PC

Three constants sit in a material's parameter block with the value 0 and are the engine's to write per frame:
`g_screenSizeInv`, `g_depthScaleFactors` (the water) and `sunDir`. A draw that keeps the placeholder is wrong
(the water's thickness and colour come from `g_depthScaleFactors`).

[code: rust/bc2-render/src/material.rs:307-320] [doc: rust/bc2-render/src/water.rs:14-19]

### RNDR-DRAW-5: Bone-placed programs carry the world in their bones

Status: **verified** · Variants: PC

Programs of bone-transformed and skinned meshes (trees and other kind-1 meshes among them) place a vertex by
`boneMatrices` and `viewProjMatrix` alone: no world matrix is read, so each bone is the placement's world matrix
times the bone at rest. Drawn with only the rest bones, such a mesh stands at the origin.

[code: rust/bc2-render/src/frame.rs:286-305] [test: rust/bc2-render/src/frame.rs::bones_are_placed_by_the_world_matrix]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:724-725]

### RNDR-DRAW-6: A part with no program for its layout crashes the renderer (0x98BD6E)

Status: **observed** · Variants: PC

When a part is drawn whose vertex layout has no program in its entry, the renderer crashes at 0x98BD6E. Two causes are
recorded: a ported mesh's parts registered with no vertex layout (BFBC2's WallWooden in OPFOR's Wake Island port,
crash near the wall), and a model-group member drawn instanced whose entry has no program for the instancing variant
of its layout (`StreamInstancingFloat`: the vertex's own elements plus the instance streams). Isla Inocentes' console
road and stop signs crash at the same address when drawn (console-only entries, open). A part's programs must cover
every way it is drawn: its own layout, and the instanced layout when it is a group member.

The faulting instruction is in the draw-record builder at 0x98BCE0 (cdecl: the part's resolved draw state, its mesh
part, the 0x58-byte record it zeroes and fills): it copies 16 bytes from `[state + 8] + 8` with no test, where the
state's +4 pointer one instruction earlier is tested for null. A part whose layout found no program leaves `+8` null:
the read of address 8 is the crash. A mesh with no entry at all in the level's databases (SHDB-LVL-1) has no program
for any layout. An export therefore adds the entries of every mesh it brings in (SHDB-LVL-2).

[exe: BFBC2Game.exe 0x98BD6E, renderer; 0x98BCE0-0x98BD7B: `mov edx,[ebx+8]` ... `movups xmm0,[edx+8]`]
[data: OPFOR: the WallWooden port (fixed by giving its parts a same-stride layout and its instanced variant); ONS_MP_004's road / stop signs (open)]
[doc: opfor:src/tools/ww2/wake_install.py:2119-2132] [doc: opfor:DEVELOPMENT_NOTES.md:1350] [doc: opfor:research/BFBC2_DECOMP_RESEARCH.md:586]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1371]

### RNDR-DRAW-7: A mesh set's LOD settings must resolve (0x8D59FB)

Status: **observed** · Variants: PC

A mesh set resource names partitions by name, among them its LOD settings; the client's mesh set-up at load reads
them, and one present in no loaded bundle is a null read at 0x8D59FB. Every partition name a mesh set holds must
therefore be in a bundle the level loads (BF1943's `BeachHouseLodSettings_1`, `Trenches_LodSettings` were the cases
in OPFOR's port; Fill_Tree's `Vegetation_Lod1` exists nowhere).

[exe: BFBC2Game.exe 0x8D59FB, null read in the client's mesh set-up]
[doc: opfor:src/tools/ww2/wake_install.py:1244-1245] [doc: opfor:research/WW2_SUBSTITUTION_AUDIT.md:48]

### RNDR-DRAW-8: A program's material registers are its block header's window

Status: **verified** · Variants: PC, BF1943

The material constants of a program's second parameter block go to the registers `[first, end)` its header gives
(bytes 0x19 and 0x18, SHDB-PB-1), whatever the pixel shader calls them. On DICE's own programs the window is the
shader's `constants` array: 1,840 programs on MP_005 and 1,790 on SP_01, every one whose shader has the array (the
other 4 and 1 read none of their block constants, compiled out). OPFOR's WW2 levels hold two more kinds: terrain
patch and road shaders that declare the block's constants one by one at the records' registers
(`heightmapScaleAndOffsetXZ` ...), and the translated BF1943 water (`wake_water`, `guadal_canal_water`,
`iwo_jima_water`, `cs_water`), whose material array is `waterMaterial` (`heightmapWorld*` and
`g_depthScaleFactors`, RNDR-WATER-2). The view wrote the material only inside a variable named `constants` until round
R5e, so the WW2 water never received its `g_depthScaleFactors` (RNDR-DRAW-4): its depth factors 0, its water depth 0,
its alpha 0 everywhere (RNDR-WATER-3).

[test: rust/bc2-render/tests/deep_sea_materials.rs::block_windows_are_the_material_registers]
[test: rust/bc2-render/src/frame.rs::a_shader_without_constants_takes_the_material_in_its_block_window]
[test: rust/bc2-formats/src/shaderdb/query.rs::material_registers]
[code: rust/bc2-render/src/frame.rs:181-228] [code: rust/bc2-formats/src/shaderdb/query.rs:151-162]
[data: the six levels' Dx11 databases: every program with a `constants` array has the header window of its registers]

## 3. Order, depth and culling

### RNDR-ORD-1: Three draw classes

Status: **implemented** · Variants: PC

A draw is **blended** when its render state (SHDB-RS-1) blends or is transparent; otherwise **tested** when the state
asks for alpha test or alpha to coverage, or its pixel shader discards (on Dx10 / 11 the alpha test is in the shader,
SHDB-RS-2); otherwise **opaque**.

[code: rust/bc2-render/src/material.rs:322-337] [code: rust/bc2-render/src/dxbc.rs:121]
[test: rust/bc2-render/src/dxbc.rs::discard_found_in_the_instruction_stream]

### RNDR-ORD-2: Opaque, then tested, then blended farthest first

Status: **verified** · Variants: PC

Draws go opaque first, then tested, then blended sorted farthest first by their centre; among equals the given order
is kept (a stable sort). Nothing else is reordered: sorting by state would change depth ties and blending, and the
suite's pictures stop matching.

[code: rust/bc2-render/src/frame.rs:400-401] [code: rust/bc2-render/src/live.rs:613]
[test: rust/bc2-render/tests/world.rs::mp_005_world_draws_and_matches_the_harness]
[test: rust/bc2-render/tests/live.rs::mp_005_live_list_is_the_frame_edits_and_picks]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:751-763]

### RNDR-ORD-3: The depth buffer holds view distances, made before its first reader

Status: **verified** · Variants: PC

`DepthBuffer`, which water and other effects read, is not the hardware depth: DICE's `psUnprojectDepthBuffer` turns
the scene's depth into view distances, `d = x / (1 - z / y)` with the factors the projection's (`near`,
`far / (far - near)`), and it runs before the first draw that reads it. The water inverts its own depth the same way
with `g_depthScaleFactors` and subtracts (its thickness).

[code: rust/bc2-render/src/live.rs:648-649] [code: rust/bc2-render/src/water.rs:36-48]
[test: rust/bc2-render/src/water.rs::depth_factors_invert_the_projection]
[doc: rust/bc2-render/src/post.rs:10-12] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:726-737]

### RNDR-ORD-4: Frustum culling changes nothing drawn

Status: **verified** · Variants: PC

Leaving out a draw whose bounds lie wholly outside the view frustum (with a margin) gives the same picture, since the
rasterizer clips to the same planes; strips and water have no bounds and are always drawn. The game's own visibility
of static objects follows the cull trees (CULL-TRV-1); an object left at cull slot 0 is drawn see-through
(CULL-SLOT-3).

[code: rust/bc2-render/src/live.rs:436-461] [test: rust/bc2-render/src/live.rs::boxes_outside_the_frustum]
[test: rust/bc2-render/tests/live.rs::mp_005_live_list_is_the_frame_edits_and_picks]

## 4. Terrain

### RNDR-TERR-1: Patches of 16 m, each cell's shader from the patch map

Status: **verified** · Variants: PC

The terrain is drawn as 16 x 16 m patches. Each cell of the visual terrain's patch map (TERR-VT-4) names its patch
shader, a `PatchMaskShaders/<layers>` entry of the level's database (TERR-VT-3; MP_005: 50), and up to four mask
tiles. The patches meet without seams.

[code: rust/bc2-render/src/terrain.rs:164-299] [test: rust/bc2-render/tests/whole_terrain.rs::the_whole_terrain_is_drawn]
[test: rust/bc2-render/src/terrain.rs::a_minimal_visual_terrain_reads]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:624-640]

### RNDR-TERR-2: Height from the height map in the vertex shader

Status: **implemented** · Variants: PC

On Dx10 / Dx11 a patch's vertex shader reads a grid point (`R16G16_SINT`) and two masks' atlas coordinates per
`R16G16B16A16_SNORM` stream, samples `heightmap` and places the vertex at `(x, h, z) * posScale + posOffset`; the pixel
shader takes the world position back through `heightmapScaleAndOffsetXZ`, its normal from neighbouring height
samples, the level's colour / occlusion map over the whole level, and per layer a colour, normal and detail texture
blended by DICE's literal weights compiled into each patch shader, then the dynamic (destruction) mask. The engine
sets these named constants from the height field (TERR-HF-1).

[code: rust/bc2-render/src/terrain.rs:334-360] [doc: rust/bc2-render/src/terrain.rs:4-20]
[data: MP_005's patch shaders' disassembly: the inputs, constants and per-layer weights (0.7025 / 0.6755 / 0.648 on one patch)]

### RNDR-TERR-3: Tiled layers read the mask atlas; rule layers are computed

Status: **observed** · Variants: PC

A patch's tiled layers read their masks from the level's mask-tile atlas (`<lv>.MaskTileAtlas.res`; MP_005: L8,
1024 x 512, 66-pixel tiles with a one-pixel border) at the tiles the cell names; the other layers (base, slope,
height, noise, sea floor) are computed in the shader. A tile spans a block of 8 x 8 or 4 x 4 cells and each cell reads
its part of it (TERR-VT-7). A tiled cell drawn against an atlas of another layout draws black (TERR-VT-5). Round R5c's
black Wake Island ground in the suite's view was not this rule: the sea-floor layers shade by height and read a
placeholder height scale (RNDR-TERR-5), the lagoon's water did not read the height map (RNDR-WATER-2), and the deep
floor's `Wake_Grey` layer is black by design under at least 11.6 m of water.

[data: MP_005 patch map header (66, 1, 15, 7, 128); Wake Island in R5c: tiled-mask cells black, everything else drawn]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:627-632] [doc: rust/BENCHMARKS_ARCHIVE_1.md:3672-3680]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1298]

### RNDR-TERR-4: Roads and decals are baked strips; undergrowth has its own entries

Status: **verified** · Variants: PC

Roads and terrain decals are strips baked into the visual terrain (TERR-VT-2), drawn with their own entries over the
patches; undergrowth is drawn from the `UgMaskShaders` entries a patch shader names.

[code: rust/bc2-render/src/world.rs:286-289] [test: rust/bc2-render/tests/world.rs::mp_005_world_draws_and_matches_the_harness]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:639-640]

### RNDR-TERR-5: The height map's world scale is the height field's range

Status: **verified** · Variants: PC

A patch's pixel shader takes the world height back as the height map's sample (0 .. 1) times `heightmapWorldScaleY`,
the same height its vertex shader places with `posScale.y`: the engine writes the height field's range there. The
records store a placeholder 500 in every database read (64 records on MP_005, 19 on MP_009, all of Wake Island's
patch shaders), which is the range of 16 of DICE's 30 height fields (0 .. 500 m; the others 395 - 1,700 m: TEDIT-HF-2)
and half of Wake Island's (0 .. 1,000 m): read as stored, BF1943's sea-floor layers (which darken by depth below the water at 215 m) put the
island's beach and shallows 100 m deep and drew them black.

[test: rust/bc2-render/tests/r5d.rs::wake_island_draws_its_beach_and_lagoon] [code: rust/bc2-render/src/terrain.rs:359-364]
[data: Wake Island's lagoon view: 52.8 % near-black pixels before round R5d, 0.0 % after; Coral Sea's 10.2 % -> 0.3 %]
[data: Wake Island's `Wake_Grey` cells (12,803, one constant black layer) lie 165.9 - 203.4 m: 11.6 m or more under the water]
[data: spec/probes/terrain.py, 2026-11-22: DICE main blocks' ranges 500 x 16, 1,000 x 8, 750 x 2, 1,700, 1,500, 650, 395]

## 5. Water

### RNDR-WATER-1: Visual water, range by range, in world positions

Status: **verified** · Variants: PC

A level's water is its `VisualWater` (`<lv>.Water.res`, WATER-VIS-1) drawn range by range, each range with its slot's
shader entry (MP_005: `wakes` 608 triangles, `Water` 2,589; `WaterOcean` and `WaterLakeOutlet` are slots no range
draws). The vertices are world positions (WATER-VIS-5): the vertex shader reads `viewProjMatrix` and no world
matrix. The water is blended (state `0x585`, `0x581` for wakes), reads `DepthBuffer` (RNDR-ORD-3) and `time` for the
waves.

[code: rust/bc2-render/src/water.rs:50-116] [test: rust/bc2-render/tests/world.rs::mp_005_world_draws_and_matches_the_harness]
[doc: rust/bc2-render/src/water.rs:1-22] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:642-644]

### RNDR-WATER-2: Water that reads the height map takes the terrain's world scale and offset

Status: **verified** · Variants: PC

A water shader may read the terrain's height map (the engine texture `heightmap`) for the water's depth over the
ground (the shore's foam and absorption): DICE's Vietnam `NAM_HaLongWaterShader` and `NAM_WaterGreen`, OPFOR's Wake
Island water (BF1943's ocean shader, its `waterMaterial[0..3]` recorded under the same names). Its block names
`heightmapWorldScaleXZ`, `heightmapWorldOffsetXZ` and `heightmapWorldScaleY` (stored 0): the height map's coordinate
is `(x + 0.5) * scale + offset` (DICE's code), so the engine's values are `1 / (cell size x samples)` and the offset
putting texel centres at the samples' world positions; the height scale is RNDR-TERR-5's. Unbound and left at 0 (the
suite's view until round R5d) the lagoon's water reads one corner sample and lets the floor show through.

[test: rust/bc2-render/tests/r5d.rs::wake_island_draws_its_beach_and_lagoon] [code: rust/bc2-render/src/water.rs:168-190]
[data: NAM_MP_005R's two water entries and Wake Island's `Wake_Water` bind `heightmap`; MP_005's water reads none]

### RNDR-WATER-3: The WW2 water's alpha is its depth below the water, from the depth factors

Status: **verified** · Variants: PC, BF1943

OPFOR's WW2 water (BF1943's ocean colour programs translated, `wake_water.py`) writes its colour premultiplied by
its alpha, and the alpha is `min(1, ((1 - n.v)^8 + smoothstep((t + 30) / 85)) x smoothstep(t / 5))`, with `t` the
distance from the water to what lies behind it: the scene's view distance (`DepthBuffer`, RNDR-ORD-3) less the
water's own, both through `g_depthScaleFactors` (`waterMaterial[3]`). With the factors written (RNDR-DRAW-8) the open
sea is opaque deep blue past about 55 m of water, with the sun's highlight and the reflection cube
(`Sky_Beach_Wake`); with them 0 both distances read 0, `t` is 0 and the water draws nothing anywhere: the suite's
view until round R5e showed the black `Wake_Grey` floor (RNDR-TERR-3) to the horizon, and the green lagoon it showed
after round R5d was the terrain's sea floor, not the water.

[test: rust/bc2-render/tests/deep_sea_materials.rs::the_ww2_open_sea_is_drawn]
[code: rust/bc2-render/src/water.rs:193-226]
[data: Wake Island 600 m out: 53.5 % near-black pixels before round R5e, 0.0 % after; a deep-sea pixel (0, 0, 0, 0) -> (0.034, 0.110, 0.220, 1.0) HDR; Guadalcanal and Iwo Jima 1,200 m out: 48.6 % and 56.7 % of the view darker than 40 / 255 -> 0.0 %]
[data: the Wake water's pixel shader disassembled (`entry_probe`, `fxc /dumpbin`): `o0 = (colour x a, a)`; `t` from `texture_DepthBuffer` and `cb0[13]`]

## 6. Weather, light and sky

### RNDR-WTHR-1: The active weather state names the light, fog, sky and post-process

Status: **verified** · Variants: PC

A level's weather entity (RNDR-WTHR-5) names a `WeatherSystem` and its settings (a `WeatherSystemSettings`);
the settings' `ActiveState` indexes the system's `States`, and the active state names its `Sun` (theta, phi),
`OutdoorLight` (key, top and bottom colours with intensities, specular scale, hemisphere), `PostProcess`, `Fog`
(start, end, colour, density, height fall-off) and `Sky` (horizon coefficients, halo, haze, Mie / Rayleigh
constants) partitions. DICE's stock folders name the two `WeatherSettings_*` and `WeatherSystem_*` side by side,
but that is a naming habit, not the rule (OPFOR's Guadalcanal: `weather/wake_island/guadal_canal`).

[code: rust/bc2-render/src/weather.rs:231-265] [test: rust/bc2-render/src/weather.rs::fields_and_refs_read_as_renderqa_does]
[test: rust/bc2-render/tests/warehouse.rs::warehouse_as_the_harness_draws_it]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:646-649]

### RNDR-WTHR-2: Light colours are colour times intensity

Status: **verified** · Variants: PC

The light colours the shaders receive (`outdoorLightKeyColor`, `TopColor`, `BottomColor`) are the state's colours
multiplied by their intensities, as OPFOR measured in the game.

[code: rust/bc2-render/src/weather.rs:315-357] [test: rust/bc2-render/src/weather.rs::light_multiplies_colours_by_their_intensities]

### RNDR-WTHR-3: No object reads fog; the sky and haze are one pass over the depth

Status: **observed** · Variants: PC

No object shader reads a fog constant. The sky and the atmosphere are drawn by
`Systems/Weather/Shaders/Surface/EnvironmentMain` (and its half-resolution and cube variants), a pass that reads the
depth buffer and the level's panoramic sky texture with twelve `g_*` constants (horizon coefficients, halo, haze, sun
direction, Mie mu / sigma): haze and fog are laid over the drawn scene by depth. The suite's view draws the sky dome
since round R5d (RNDR-SKY-1); the haze over the scene (`EnvironmentHalfResMain` with its fog segments) is not drawn
yet.

[data: MP_005's Dx11 database: no main-pass program's reflection names a fog variable; EnvironmentMain's reflection: the twelve g_* constants, the depth buffer and mp05_Skydome]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:649-653] [doc: rust/BENCHMARKS_ARCHIVE_1.md:3678-3679]

### RNDR-WTHR-4: The sun's direction is the engine's

Status: **observed** · Variants: PC

`outdoorLightDir` is computed by the engine from the active state's `Sun` theta and phi; no partition or parameter
block holds it. OPFOR's formula measured on Wake Island does not carry over (it points below the horizon on MP_005's
states), so the mapping is open.

[data: MP_005's states: no stored light direction; the Wake formula gives a direction below the horizon]
[doc: rust/bc2-render/src/weather.rs:6-9] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:671-677]

### RNDR-WTHR-5: The weather is found through the level's weather entity, by reference

Status: **verified** · Variants: PC

A level's weather is a `Weather.WeatherSystemEntityData`: an instance of the level partition itself (SP_004_B,
OPFOR's Coral Sea and Wake Island) or the `Object` of a blueprint a `ReferenceObjectData` places through its
`ReferencedObject` (MP_005's `weather/r_mp005/weathersystementity_mp005`, OPFOR's Guadalcanal
`weather/wake_island/guadal_canal_e`). Its `WeatherSystem` and `Settings` fields name their partitions by path; each
is found like any other partition, by name through the level's bundle stack (PREC-ORD-1, deleted markers hiding
shared copies: BNDL-DEL-2), in whatever folder it lies. The partition names follow no pattern: Guadalcanal's and Iwo
Jima's settings are named after the level (`weather/wake_island/guadal_canal`, `.../iwo_jima`) and share
`weathersystem_wake`'s five states with Wake Island and Coral Sea (which use `weathersettings_wake`). A view that
looked for `weathersettings*` in a guessed folder found no weather for Guadalcanal and Iwo Jima (round GW1). On the
37 prepared stock and Onslaught levels the reference chain gives the state the folder read gave on the 36 where the
folder read worked (SP_004_B's folder, `weather/r_sp04`, is not the level's name).

[code: rust/bc2-render/src/world.rs:161-228] [code: rust/bc2-render/src/weather.rs:151-196]
[test: rust/bc2-render/tests/weather_refs.rs::ww2_levels_find_their_weather_through_the_entity]
[test: rust/bc2-render/tests/weather_refs.rs::stock_levels_resolve_as_their_folder_did]
[test: rust/bc2-editor/tests/game_view_ww2.rs::ww2_levels_open_in_the_game_accurate_view]
[data: OPFOR's game copy: guadal_canal's level-00.fbrb holds weather/wake_island/guadal_canal_e, guadal_canal (ActiveState 2) and weathersystem_wake (states sunny_day, iwo_jima, guadal_canal, default, coral_sea); coral_sea's level partition holds its own WeatherSystemEntityData naming weathersettings_wake (ActiveState 0)]

### RNDR-WTHR-6: A level's state entities may move the weather on spawn

Status: **observed** · Variants: PC

Most level partitions hold `GameSharedResources.WeatherStateEntityData` instances (`Enabled`, `DefaultState`,
`State`, `FadeTime`, `StartTransitionOnSpawn`); in 34 of 43 levels read (37 prepared, 2 Package, OPFOR's 4 WW2),
the first enabled one with `StartTransitionOnSpawn` true names a `State` (Coral Sea 4, `coral_sea`, where its
settings say 0; MP_005 2; SP_008_B 36, more states than its system has, so such entities belong to scripted logic
too). Research round WX0 (all 83 levels): 70 have such a switch, and in 45 it names another state than `ActiveState`;
the `ActiveState` placeholders (`default`, `state1`, `deafult`) are dark or unlit by value, so the switch's state is
what players see. Whether the engine applies the transition as the level loads, and which entity wins when several
do, is still open (no capture, no exe trace). Since round VW2 the editor's game-accurate view opens on the switch's
state (MP_005: `day_1`, not `default`) and notes it, and an export sets both (RNDR-WTHR-8).

[test: rust/bc2-render/src/weather.rs::weather_entities_and_spawn_states_read_from_a_partition]
[test: rust/bc2-editor/tests/vw2.rs::game_view_opens_on_the_spawn_switch]
[test: rust/bc2-levelwrite/tests/vw2.rs::levels_weather_as_the_export_reads_it]
[data: weather_refs.rs's run on OPFOR's game copy and prepared levels: the first spawn-transition State per level, printed beside the drawn state]
[data: WX0's inventory of 83 levels: 70 with an enabled spawn switch, 45 naming another state than ActiveState]
[doc: rust/bc2-render/src/weather.rs:163-179]

### RNDR-WTHR-7: A state's look from its values

Status: **verified** · Variants: PC

A weather state's look is its values: the key light (colour times intensity, RNDR-WTHR-2) and the sky (top) colour
light the ground; the post-process exposes the frame (`MiddleGray` over its luminance, within `MinExposure` ..
`MaxExposure`) and tonemaps it. A grey ground lit so predicts how a state draws: on MP_002's seven states the
game-accurate view's frames follow the prediction's brightness (Pearson 0.86) and its colour (blue over red, Spearman
0.79); the Onslaught night (`ons_mp_002_night`: key (0.17, 0.46, 0.99) x 1, middle grey 0.02, minimum exposure 0) is a
third of midday's brightness and blue. The editor's simplified view tints its scene-lit colours by the prediction
against a neutral day.

[code: rust/bc2-render/src/weather.rs:415-447] [code: rust/bc2-render/src/world.rs:338-370]
[test: rust/bc2-editor/tests/vw2.rs::weather_states_in_both_views]

### RNDR-WTHR-8: An exported level's weather: ActiveState and the spawn switch together, in partitions of its own

Status: **verified** · Variants: PC

To start a level in another state the suite's export sets the settings' `ActiveState` and the first enabled
spawn switch's `State` to it (a level without a switch: `ActiveState` alone), in copies under `weather/<level>/` in
the level's own bundle: the settings (and, for a placed weather entity, its blueprint, the placement re-pointed); the
base's stock partitions, shared with its other modes, are never changed. Another level's state is appended to a copy
of the system (the level keeps its Setup: its sky panorama and clouds) or the donor's whole system is swapped in (its
sky); either is renamed under the level's folder with new guids, so no donor name can collide with the level's. A
system the export writes has its compiled `WeatherSystem` resource beside it (`<system>.res`, 4 bytes, 72 - the value
of all 28 of DICE's systems, whatever their state count). The other state entities (round end, scripted) keep their
indices, which stay right for an appended state and change meaning for a swapped system (listed in the report).
Untested in game: whether the engine starts in the state (RNDR-WTHR-6).

[code: rust/bc2-levelwrite/src/weather.rs:546-815] [code: rust/bc2-levelwrite/src/weather.rs:969-1060]
[test: rust/bc2-levelwrite/tests/vw2.rs::levels_weather_as_the_export_reads_it]
[data: examples/vw2_canaries.rs on OPFOR's game copy: six canaries, each stage against its baseline byte for byte but the intended changes, every mapexport check line passing; OPFOR's mapedit_install --apply and --revert on a scratch copy of the shared files, back byte for byte]
[data: WX0: the WeatherSystem resource of all 28 systems is 4 bytes, value 72]

### RNDR-WTHR-9: Night: DICE's one night state as the template

Status: **verified** · Variants: PC

BFBC2 ships one night state, `weather/r_mp02/states/ons_mp_002_night` in MP_002's system (the Onslaught Package's
`Package\levels\mp_002\level-00.fbrb`). Nothing baked depends on the sun (WX0): a night needs only the state's values.
The suite's night export appends it to the level's system, scales the state's `ReflectionEnvmapScale` down (the level's
reflection cube is a capture of the day sky) and its panoramic layer's `Trancparency` (DICE's spelling: the level's own
day panorama, darkened). Lamps are point-light effect emitters (`ActAsPointLight`) placed with the editor's tools,
never by the export. Night and rain exports are off by default and marked not yet seen in game.

[code: rust/bc2-levelwrite/src/weather.rs:50-62] [code: rust/bc2-levelwrite/src/weather.rs:856-890]
[test: rust/bc2-levelwrite/tests/vw2.rs::the_night_template_resolves]

### RNDR-LIGHT-1: Baked occlusion, irradiance volumes and Enlighten instead of lightmaps

Status: **observed** · Variants: PC

There are no classic lightmaps. Buildings carry baked occlusion textures per mesh (`_Oc`, `_OcRGB`, `_Occlusion`,
`_AO`) and sky occlusion (`_SkyOc`), and an `IrradianceVolume` (`<mesh>_Irradiance.res`); the databases hold
Enlighten's entries (`Systems/Enlighten/DirectIllumination`, `DiffuseDuster`). The level's reflection cube is its
`StaticEnvmapEntityData` (`outdoorLightReflectionEnvmap`; MP_005 radius 5,000 m).

[data: MP_005's 587 textures by DICE's suffixes; 19 IrradianceVolume resources in mp_common; MP_005's EnlightenSkyColorIntensity 0]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:614-622] [doc: rust/bc2-render/src/placement.rs:14-15]

### RNDR-SKY-1: The sky is EnvironmentMain on a dome at 0.9 of the view distance

Status: **verified** · Variants: PC

`Systems/Weather/Shaders/Surface/EnvironmentMain`'s main-pass programs (key byte 0x30 = 0) come in four variants (key
byte 0x37): 3 reads the scene's linear depth, the panoramic cloud texture and its alpha, 0 the depth and the texture,
1 the depth alone, 2 nothing. All have the state `0x481` (blended, depth tested and not written, back faces culled)
and one vertex layout (`TEXCOORD0` half4 position, 44 bytes). The vertex shader scales a unit dome by
`0.9 x g_viewDistance.x` around the camera (`worldViewProjMatrix`, `worldMatrix`); the pixel shader takes the view
direction from `cameraPos` and gives the horizon gradient, the sun's halo and disc and the panoramic clouds where the
scene drew nothing (the depth taken back with `g_depthFactors.zw` at 0.999999 or more), else the haze and horizon
colour alone; alpha 1. The engine's dome mesh is not in the data (the suite draws a UV sphere); it is drawn after the
opaque and tested draws, first of the blended ones.

[test: rust/bc2-render/tests/r5d.rs::the_sky_dome_is_drawn_behind_the_level] [code: rust/bc2-render/src/sky.rs:220-345]
[data: MP_005's and Wake Island's databases: 8 programs each, the four main-pass variants, state 0x481, the 44-byte layout]

### RNDR-SKY-2: The sky's constants are the state's Sky, Sun and Clouds values by name

Status: **inferred** · Variants: PC

The entry's block names its constants (`g_horizontalGainAndMore`, `g_haloBleedPowerAndMore`,
`g_haloColorAndHaloBleedGain`, `g_sunDirectionAndHazeFadeMax`, `g_hazeColorAndHazeFadeMin`, `g_muAndSigma`,
`g_horizontalCoefficientsR/G/B`, `g_mainPassAndMore`, `g_sunColorAndGain`, `g_panoramicRotation`,
`g_panoramicUVRanges`, `g_tileFactor`, `g_additiveThreshhold`, `g_panoramicTransparency`), stored 0 but for the
panoramic layer's defaults. The suite fills them from the weather state's `Sky` (`HorizontalGain`, `HaloAlphaGain`,
`HaloAlphaPower`, `HaloGain`, `HaloBleedPower`, `HaloBackReflectionGain`, `HaloColor`, `HaloBleedGain`, `HazeColor`,
`HazeFadeMin` / `Max`, `Mu`, `Sigma`, `HorizontalCoefficients*`), `Sun` (`Color`, `Gain`, `Size`), `Clouds`
(`PanoramicCloudLayer`) and the light's direction (RNDR-WTHR-4's placeholder).

[inferred: the constants' names pair with the fields' names, and the shader's use fits each (a cubic horizon gradient in the elevation, a Gaussian halo with a power, a Henyey-Greenstein sun disc of g = 1 - Size)]
[test: rust/bc2-render/src/sky.rs::values_follow_the_sky_state_by_name] [code: rust/bc2-render/src/sky.rs:122-200]

### RNDR-SKY-3: The panoramic cloud layer's composite is open

Status: **observed** · Variants: PC

The weather setup's static cloud layer names the panoramic texture and its alpha (MP_005: `mp05_Skydome` 2048 x 512
and `mp05_Skydome_A`, both sRGB, DXT1), and the settings' `PanoramicCloudBlendMode` (`PcbmMultiplyAddAlpha`) picks
variant 3, whose pixel shader gives `sky + alpha x ((texture - AdditiveThreshhold) x (sky + 1) - sky)`. With the
state's threshold 0.3 and the texture read as sRGB (0.17 on average) most of the panorama goes below zero and draws a
black band the game does not show: a value is read differently by the engine. The suite's view leaves the clouds out
until a capture settles it.

[data: MP_005's Sunny state: AdditiveThreshhold 0.3, Trancparency 1, UV range 0 .. 0.3; the texture's rows 111 - 130 of 255 on average; the alpha texture white]

## 7. Post-process and shadows

### RNDR-POST-1: The post shaders are in the startup bundle

Status: **implemented** · Variants: PC

The engine's own post-process and shadow shaders are not in any level's database: they are standalone `Dx11PixelShader`
/ `Dx11VertexShader` resources in `async/startup-00.fbrb` (`Systems/PostProcess/ShaderContainer/*`), shared by every
level.

[code: rust/bc2-render/src/post.rs:104-143]
[data: Dist startup-00: 112 Dx11PixelShader and 14 Dx11VertexShader resources, among them psFilmicTonemap and its variants, HorizonBasedAO / HorizonBasedAOBlur, psDrawParticleVertexShadows*, psDrawEnlightenShadows]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:655-661]

### RNDR-POST-2: The chain: luminance, exposure, bloom, filmic tonemap

Status: **verified** · Variants: PC

After the scene: `psDownsampleHalfGrayscaleLog` then `psDownsampleHalf` down to one pixel (the frame's log-average
luminance), `psExposure` (`middleGray` over that luminance, clamped to the state's `MinExposure` / `MaxExposure`),
bloom (`psDownsampleQuarter`, the separable 15-tap filters), and `psFilmicTonemap` (DICE's literal filmic curve,
`BloomScale` from the state, a colour matrix from `Brightness` / `Contrast` / `Saturation`, identity at the neutral
values every MP_005 state has). Every post shader shares one `$Globals` layout and the `vs2D` full-screen vertex
shader.

[code: rust/bc2-render/src/post.rs:257-357] [test: rust/bc2-render/src/post.rs::neutral_settings_give_the_identity_matrix]
[test: rust/bc2-render/src/post.rs::the_chain_goes_down_to_one_pixel]
[test: rust/bc2-render/tests/world.rs::mp_005_world_draws_and_matches_the_harness]
[doc: rust/bc2-render/src/post.rs:1-32]

### RNDR-POST-3: Ambient occlusion is HBAO with settings in the executable

Status: **observed** · Variants: PC

The game's screen-space ambient occlusion is `HorizonBasedAO` / `HorizonBasedAOBlur` from the startup bundle; its
settings (`HsaoRadius`, `HsaoAngleBias`, `HsaoAttenuation`, step and direction counts) are the executable's, not
level data. Depth of field and film grain are off in the stock states.

[exe: BFBC2Game.exe: the Hsao* setting names and defaults]
[doc: rust/bc2-render/src/post.rs:30-32] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:658-659]

### RNDR-SHAD-1: Three shadow cascades drawn with the depth-only programs

Status: **observed** · Variants: PC

Shadows are three cascades (`outdoorLightShadowTransform0..2`), sampled through a comparison sampler on `s0`, and
drawn with each entry's depth-only programs (pass 5 of the program key, SHDB-KEY-1): the shadow casters need nothing
outside the databases. DICE's objects sample them with four taps around the shadow coordinate (hardware
comparison).

[data: MP_005's binding tables: outdoorLightShadowTransform0..2; entries' depth-only programs; Dx9 objects: four texldp taps averaged]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:659-661] [doc: opfor:research/WW2_DX9_SHADER_PORT_PLAN_2026-11-15.md:39]

### RNDR-LOD-1: A mesh's levels of detail switch at its LOD settings' distances

Status: **inferred** · Variants: PC

Every mesh set names a `Render.MeshLodSettingsAsset` (MP_005: 32 different, from `Systems/Mesh/DefaultMeshLodSettings`
to per-object ones) with `LodDistance0..4`, `Lod0Enabled..Lod4Enabled`, `Enable` and `MinScreenArea`. The suite draws
level `k` from `LodDistance(k-1)` to `LodDistance(k)` (the instance's position to the camera), a level switched off
or drawing nothing keeping the one before it, the last level to any distance. `MinScreenArea` (0.01 - 0.035 on
`ObjectsLodSettings_*`, -1 elsewhere) is read but not applied: as a share of the screen it would cull a 10 m object
at 100 m, which the game does not, so its unit is open. Meshes with alpha-tested or blended parts (the vegetation)
keep level 0 in the suite's view: their far levels are thin cards (MP_005's `mebush_01` LOD 1: 54 indices) whose
look may need engine values.

[inferred: the field names; the distances rise with the level (40 / 70 / 100 / 150 / 200 m on ObjectsLodSettings_1); the per-object assets with ShortCull / LongCull variants differ only in MinScreenArea]
[data: MP_005: every placed mesh set names settings, 2 - 4 levels]
[test: rust/bc2-render/tests/r5d.rs::levels_of_detail_draw_fewer_indices] [code: rust/bc2-render/src/material.rs:615-700]

## 8. Faults not yet explained

### RNDR-CRASH-1: A jump to 0x00F30005 after a plane death (Coral Sea)

Status: **observed** · Variants: PC

Once, on OPFOR's Coral Sea after a plane's death, the game jumped to 0x00F30005, an address that is not code, on the
render path (2026-11-14). It has not recurred; the crash log records the stack's return addresses since 2026-11-15,
so a recurrence will name its caller. No rule about the data follows yet.

[exe: BFBC2Game.exe: execution at 0x00F30005, outside the image's code, render path]
[doc: opfor:research/PACIFIC_GAP_ANALYSIS.md:349-351] [doc: opfor:research/QA_SWEEP_2026-11-14.md:76]

### RNDR-CRASH-2: A released texture view left in a slot crashes the state flush (0x9BC68E)

Status: **observed** · Variants: PC

The renderer's state flush (0x9BC230) binds the dirty shader-resource slots: the pixel shader's at 0x9BC68E
(`PSSetShaderResources`), the vertex shader's at 0x9BC661 (`VSSetShaderResources`), from the view arrays of its state
cache. Wake Island's first spawn crashed inside Direct3D under the pixel shader's call: a view that had been released
was still in a slot (the flashing sky looked the same). Which data leaves a released view bound is not known; OPFOR's
DLL guards the call (a view that is not readable is logged with the program and the texture its parameter blocks
name there, and the slot emptied).

[exe: BFBC2Game.exe state flush 0x9BC230: PSSetShaderResources at 0x9BC68E, VSSetShaderResources at 0x9BC661; Wake Island's first spawn, 2026-11-06]
[doc: opfor:src/dll/levelfix.cpp:619-626]

## 9. The editor's view (the suite's own rules)

### RNDR-VIEW-1: Labels behind the game frame are hidden

Status: **verified** · Variants: PC

The editor's labels over the game-accurate view (gameplay entities, the overlays' items) leave out a point the frame
drew something in front of: the point projected with the frame's camera onto its depth, every pixel of the 3 x 3
around it nearer by more than half a metre or 2 % of the distance. The selection's label is always drawn.

[test: rust/bc2-render/src/frame.rs::a_point_behind_drawn_depth_is_occluded]
[test: rust/bc2-editor/tests/game_view_r5d.rs::game_view_draws_the_sky_and_hides_labels_behind_the_frame]
[code: rust/bc2-render/src/frame.rs:405-445]

### RNDR-VIEW-2: The view's terrain levels keep their edges in full

Status: **verified** · Variants: PC

The game's terrain LOD is not in the data. The view draws a patch every sample within 200 m of the camera, then every
2nd, 4th ... sample at each doubling of the distance up to half the patch, inside a ring whose outer edges keep every
sample: a patch of any level meets a neighbour of any other without a crack. The proofs against R5b's list and the
C++ harness draw every patch in full (`Options::exact`).

[test: rust/bc2-render/src/terrain.rs::patch_levels_cover_the_patch_and_keep_full_edges]
[test: rust/bc2-render/tests/whole_terrain.rs::the_whole_terrain_is_drawn_with_levels_of_detail]
[code: rust/bc2-render/src/terrain.rs:560-649]

## Open questions

- **The engine's per-frame values** (the capture R5 asked for): `outdoorLightDir` from the sun's theta / phi
  (RNDR-WTHR-4), the shadow cascades' transforms, `EnvironmentMain`'s twelve `g_*` values from the Sky and Fog
  states (read by name since round R5d: RNDR-SKY-2), the panoramic clouds' composite (RNDR-SKY-3), the haze over the
  scene (`EnvironmentHalfResMain`'s fog segments), the exposure's history (`exposureFactor`) and the tonemap's other
  constants (`post::PLACEHOLDERS`), tree and wind vertex constants.
- **LODs and instancing**: the mesh sets' LOD distances are read (RNDR-LOD-1) but how `MinScreenArea` culls, the
  terrain's own LOD (the view's is RNDR-VIEW-2), and when the game draws a group member with the instanced program
  (`StreamInstancingFloat`) rather than one draw per instance are open.
- **Enlighten and the irradiance volumes**: `IrradianceVolume` is not decoded; how Enlighten's entries contribute.
- **The open sea** (settled by round R5e, RNDR-WATER-3): Wake Island's `VisualWater` covers the sea (-4,236 .. 4,548
  m at 215 m); its alpha 0 was the view's unwritten `g_depthScaleFactors` (RNDR-DRAW-8), not BF1943's blend. Still
  open: the game's water reflects a live environment cube (`outdoorLightReflectionEnvmap`; the view binds the level's
  static one), the whitecaps the in-game picture shows at a distance, and thin bright seams along a few water
  triangles' edges far out (Guadalcanal, Iwo Jima). `WaterOcean` is a slot no MP_005 range draws.
- **0x98BD6E's console case**: why Isla Inocentes' road and stop signs crash when drawn (RNDR-DRAW-6), and whether
  the address is one check (no program for the layout) or several.
- **Depth ties**: 0-16 far water pixels at MP_005's west shore changed between runs in every path (R5c). Round R5d
  saw 2 pixels differ (4.7e-4) in 2 of 68 runs, then none in 400 (120 of them two processes at once), with every
  CPU-side input of the frame the same by hash in all 400 (the draws, constants, textures, samplers, buffers): not
  reproduced since, cause not settled.
- **Dx9 fog**: whether fog and the transparency fade reach Dx9 shaders through the same constants as Dx10 / 11.
