# The console conversion: Xbox 360 -> PC

Two layers:

- **Resources** (`bc2-convert`, `bfbc2-console-convert/convert.py`, `bfbc2.exe convert / audio`): one Xbox 360
  resource in, its PC form out - HavokPhysicsData, textures (XenonTexture -> ITexture), MeshData and mesh sets,
  terrain and water resources, soot meshes, Wave sounds (EA-XMA -> EA-XAS). The formats are layer 1's
  (`formats/havok.md`, `texture.md`, `mesh.md`, `terrain.md`, `water.md`, `audio.md`).
- **Levels** (Project OPFOR's `src\tools\onslaught\console_port.py`): a console Onslaught level from the user's own
  Xbox 360 package, built on the PC map's Linux dedicated-server level in fourteen stages, each a separate tool with
  `--plan` / `--apply` / `--revert` under the stage protocol (`transforms/install.md`, XINST-STG-1 to XINST-STG-4).

The ground truth is the **twin**: a resource the console and PC copies of BFBC2 both ship. A conversion is right
when the converted console file equals its PC twin, byte for byte or in every value the game reads.

## 1. Resources

### XCONS-HK-1: Havok: wrapper identical, packfile equivalent to the PC twin

Status: **verified** · Variants: Xbox 360

The converted HavokPhysicsData's wrapper is byte-identical to the PC twin's (HAVOK-WRAP-2), and its packfile equal
in every byte but values the game does not read or rebuilds at load, which differ between the PC's own files too
(index padding, invalidated broadphase volumes, triangle offsets computed at load, an unused motion index). Class
layouts come from the user's `BFBC2Game.exe` (HAVOK-CLS-1); a byte the layouts would convert at two widths, or a
class the executable lacks, is refused.

[code: rust/bc2-convert/src/havok.rs:577-645] [code: rust/bc2-convert/src/havok.rs:745]
[test: bfbc2-console-convert/tests/test_havok.py::main] [test: rust/bc2-convert/src/havok_tests.rs::a_byte_converted_as_two_widths_is_refused] [test: rust/bc2-convert/src/havok_tests.rs::refusals_and_the_reference]
[data: 400 HavokPhysicsData shared by the four Onslaught builds and the PC: all equivalent; all 594 of the four console levels convert]
[doc: bfbc2-console-convert/FORMAT.md:58-73]

### XCONS-TEX-1: Textures: the layout's length, the twin's picture, the console's flags

Status: **verified** · Variants: Xbox 360

A converted texture is exactly as long as the PC layout of its size and format says (TEX-HDR-2); its blocks are the
console's, untiled and byte-swapped (TEX-X-6), so it is the twin's picture, not its bytes (each platform was
compressed separately, TEX-X-8); its mip chain is no worse than the PC file's own; it keeps the console header's
flags (TEX-FLAG-1). Formats the converter does not know are refused with the reference's message (TEX-X-7).

[code: rust/bc2-convert/src/texture.rs:272-310]
[test: bfbc2-console-convert/tests/test_texture.py::main] [test: rust/bc2-convert/src/texture_tests.rs::conversions_as_the_reference] [test: rust/bc2-convert/src/texture_tests.rs::refusals_carry_the_reference_messages]
[data: 420 console textures of the Onslaught builds convert; of their PC twins of the same size and format 4 byte-identical, 188 the same picture re-encoded, 1 different art]
[doc: bfbc2-console-convert/FORMAT.md:118-134]

### XCONS-MESH-1: Meshes and mesh sets: values swapped, strips unrolled

Status: **verified** · Variants: Xbox 360, BF1943

A converted MeshData has the PC header, its vertex values swapped per value width (MESH-CONV-1), console-only
element formats decoded to half4 (MESH-CONV-2), triangle strips unrolled into lists (MESH-PRIM-2) and gaps between
vertex windows zeroed; a mesh set is rewritten by the PC writer (MSET-WRITE-1). A layout the reference did not
convert is refused.

[code: rust/bc2-convert/src/mesh.rs:236] [test: rust/bc2-convert/src/mesh_tests.rs::swaps_by_value_width] [test: rust/bc2-convert/src/mesh_tests.rs::refuses_what_the_reference_does_not_convert]
[test: rust/bc2-convert/src/mesh_tests.rs::vertices_in_order_with_zeroed_gaps] [test: bfbc2-console-convert/tests/test_console_f2a_native.py::meshes]

### XCONS-TERR-1: Terrain, water and soot: the twin's bytes

Status: **verified** · Variants: Xbox 360, BF1943

Height fields, material maps, visual terrain, water meshes and visual water convert by swapping each value at its
width with the quadtree's node count recomputed; a converted soot mesh equals its PC twin byte for byte.

[code: rust/bc2-convert/src/terrain.rs:146-245] [code: rust/bc2-convert/src/terrain.rs:645]
[test: bfbc2-console-convert/tests/test_terrain.py::main] [test: bfbc2-console-convert/tests/test_soot.py::main] [test: rust/bc2-convert/src/terrain.rs::quadtree]
[data: soot: 30 of 30 BFBC2 twins byte for byte; BF1943 404 of 406 (16-byte vertices refused)]

### XCONS-SND-1: Sounds: EA-XMA in, EA-XAS out, the PC wrapper

Status: **verified** · Variants: Xbox 360, BF1943

A console Wave is decoded (EA-XMA, through ffmpeg) and re-encoded as EA-XAS by the suite's encoder (XAS-ENC-1) in
DICE's block layout (XAS-BLK-1), held in memory in a PC Wave (WAVE-WR-1); its structure (segments, block lengths,
sample counts) is that of the PC twins.

[code: rust/bc2-convert/src/wavport.rs:205-254] [code: rust/bc2-convert/src/audio.rs:85-97]
[test: bfbc2-console-convert/tests/test_audio.py::main] [test: rust/bc2-convert/src/audio_tests.rs::groups_as_the_reference] [test: rust/bc2-convert/src/audio_tests.rs::layout_of_frames]
[data: 35,811 BFBC2 twins' wrappers compared]

### XCONS-PAR-1: The Rust converters give the retired Python's bytes

Status: **verified**

Every converter in `bc2-convert` gives, for every console and BF1943 resource of the game copy and OPFOR's work
folders and for built and damaged inputs, the bytes (or the refusal) the retired Python converters gave, recorded
before their deletion (`f0_*_replay*.json`, `f2a_console_replay*.json`).

[test: bfbc2-console-convert/tests/test_console_f2a_native.py::quick] [test: bfbc2-console-convert/tests/test_console_f2a_native.py::textures_game]
[test: bfbc2-console-convert/tests/test_texture_native.py::game]

## 2. Levels: the Onslaught port's stages

### XCONS-STG-1: Fourteen stages in a fixed order

Status: **implemented** · Variants: Xbox 360

`console_port.py <level> --xbox <package> --linux <levels> [--from N]` runs, in order: 1 the level from the console
level and the PC map, with registration; 2 the console gameplay on the Linux server level; 3 the Onslaught mode;
4 the `mp2_common` files the level takes copied in; 5 vehicle spawns without PC physics left out; 6 console soldiers
and kits; 7 shader databases covering every placed and reached mesh; 8 the Mark V (one level); 9 PC input for
console seats; 10 every referenced but unloaded PC partition copied in; 11 cull-tree places; 12 the loading video;
13 the HUD; 14 the Mark V's HUD icon (one level). The order is load-bearing: stage 7 follows 6 (the kits' weapons),
9 and 10 follow 8. A level needing a per-map entry stops at stage 4 with the partitions to list.

[code: opfor:src/tools/onslaught/console_port.py:1-29] [code: opfor:src/tools/onslaught/console_port.py:74-92]
[doc: opfor:DEVELOPMENT_NOTES.md:1256-1279]

### XCONS-STG-2: What the stages guard against

Status: **observed** · Variants: Xbox 360

Three stages exist because the unconverted data crashed the PC game: console-only vehicle builds (stage 5,
0x5A5A61), console action maps (stage 9, a reference without a value dereferenced at 0x4EB538), and console road
and stop signs drawn without a PC vertex layout (0x98BD6E, still open on Isla Inocentes). The engine side of each:
LOAD-PART-3, LOAD-PART-2, RNDR-DRAW-6.

[exe: BFBC2Game.exe 0x5A5A61 (console-only vehicle mesh and physics), 0x4EB538 (null string of a value-less reference), 0x98BD6E (renderer)]
[doc: opfor:DEVELOPMENT_NOTES.md:1266-1270] [doc: opfor:DEVELOPMENT_NOTES.md:1350]

## Open questions

- 0x98BD6E on ONS_MP_004's console road / stop signs: which vertex layout or program the renderer lacks (layer 3,
  rendering).
- XMA decoding depends on ffmpeg; a native EA-XMA decoder would remove the dependency and make the sound conversion
  reproducible without it.
