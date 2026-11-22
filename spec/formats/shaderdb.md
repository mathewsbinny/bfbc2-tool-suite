# Shader databases and shader programs

Every level has one compiled shader database per PC renderer: `Levels/<LV>/Shaders/Dx9_Single.res`,
`Dx10_Single.res`, `Dx11_Single.res` (FBRB types `Dx9ShaderDatabase`, `Dx10ShaderDatabase`, `Dx11ShaderDatabase`),
named by the level's `shaders.dbx` (a `ShaderDatabaseCollection`, one group per render path). BFBC2's Xbox 360 levels
and BF1943 have one, `Levels/<LV>/Shaders/Single.res`, listed as `Dx9ShaderDatabase`. The database holds every
compiled program the level's meshes, terrain and effects draw with, and it is also where a mesh part's **vertex
layout** lives (`formats/mesh.md`).

Other shader resources (`Dx9/10/11PixelShader`, `Dx9/10/11VertexShader`, the post-process shader containers) hold
single compiled shaders; on Dx10 / 11 they are plain DXBC (SHDB-DXBC-1).

Sources: `rust/bc2-formats/src/shaderdb.rs` (reader and writer) and `shaderdb/query.rs` (parameter blocks, checks),
`rust/bc2-render/src/{material,dxbc}.rs` (what the renderer reads from it), `rust/bc2-shader` (Xenos microcode),
`bfbc2-shader-tools/FORMAT.md` and `XENOS.md` (the original descriptions, with the executable's loader addresses).

## Container

### SHDB-HDR-1: Primitives and stream form

Status: **verified** · Variants: PC, Xbox 360, BF1943

PC files are little-endian; Xbox 360 and BF1943 files big-endian (SHDB-X-1). `guid` is 16 bytes, `cstr` bytes up
to a zero byte. The file is a stream of sections read front to back: records follow each other with no padding and
no offsets, nothing is stored about the file's total size, and the file ends exactly after the last section. A
reader must consume every byte.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:354-419] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [test: rust/bc2-formats/src/shaderdb.rs::errors] [test: rust/bc2-formats/tests/shaderdb_robustness.rs::databases_read_back] [doc: bfbc2-shader-tools/FORMAT.md:20-24]

### SHDB-HDR-2: Header and section order

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Order | Content |
|---|---|
| 1 | u32 version: 120 BFBC2 (PC and Xbox 360), 117 BF1943 |
| 2 | u8 flag (0) |
| 3 | section 1, parameter blocks (SHDB-PB-1) |
| 4 | section 2, binding tables (SHDB-BIND-1) |
| 5 | section 3, register tables (SHDB-REG-1) |
| 6 | section 4, vertex shaders (SHDB-SH-1, SHDB-SH-2) |
| 7 | section 5, pixel shaders (SHDB-SH-1) |
| 8 | section 6, programs then program keys (SHDB-PROG-1, SHDB-KEY-1) |
| 9 | section 7, vertex layouts (SHDB-LAY-1) |
| 10 | section 8, three name lists then the entry groups (SHDB-NAME-1, SHDB-GRP-1) |

Every section is `u32 n` then *n* records.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:354-405] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [data: game copy 2026-11-19: version 120 and flag 0 in all 80 distinct Dx9, 80 Dx10 and 80 Dx11 databases read (bundles over 350 MB of payload not read); 270 level databases parse to their last byte (doc)] [doc: bfbc2-shader-tools/FORMAT.md:26-39]

### SHDB-HDR-3: The three PC kinds differ only in vertex inputs and code

Status: **verified** · Variants: PC

Dx9, Dx10 and Dx11 databases have the same layout except the end of a vertex shader record (SHDB-SH-2) and the code
(SHDB-SH-1). A file read with the other kind's layout does not parse. A level's three files have the same entries
(names and GUIDs) and the same numbers of fragments, mesh parts and layouts, but different fragment and layout GUIDs,
and Dx11 compiles fewer programs than Dx9 / Dx10. A database is merged only with databases of its own kind.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:278-296] [test: rust/bc2-formats/src/shaderdb.rs::layouts_disagree] [code: rust/bc2-formats/src/shaderdb/open.rs:38-73] [test: rust/bc2-formats/src/shaderdb/open.rs::opens_as_shaderdb_py] [test: tests/test_sd1_native.py::area_parse] [doc: bfbc2-shader-tools/FORMAT.md:276-283]

## Sections

### SHDB-PB-1: Parameter blocks (section 1)

Status: **verified** · Variants: PC, Xbox 360, BF1943

Each record: u32 size (counting itself, at least 4, a multiple of 16), then size - 4 bytes. Block layout:

| Offset | Type | Field |
|---|---|---|
| 0x00 | u32 | size |
| 0x04 | u32 x 2 | 0x20, 0x20 |
| 0x0C | u32 | textures end = constants start (textures start at 0x20) |
| 0x10 | u32 | constants end = engine textures start |
| 0x14 | u32 | engine textures end = sampler states start |
| 0x18 | u8 x 2 | the shader's constant registers (Dx10 / 11: `$Globals` size / 16) |
| 0x1A | u8 | 0 |
| 0x1B | u8 | texture records (0x8C bytes: u8 sampler register, u8 cube flag, 2 zero, 0x80-byte name, 8 zero) |
| 0x1C | u8 | constant records (0x38 bytes: 0x20-byte name, u32 name hash, u8 register, u8 components 1-4, 2 zero, 4 x f32) |
| 0x1D | u8 | engine texture records (0x28 bytes: 0x20-byte parameter name, u32 hash, u16 sampler register, 2 zero) |
| 0x1E | u8 | sampler states (60 bytes: u32 sampler register, the 52-byte `D3D11_SAMPLER_DESC`) |
| 0x1F | u8 | 0 |

Offsets inside a block count from the block's start, so a block is copied between levels unchanged. A program's
textures are in its second block (SHDB-PROG-1); a shader's own block holds sampler states (and on terrain strips,
engine-named constants). Nothing names a texture's role: only its sampler register.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:298-308] [code: rust/bc2-formats/src/shaderdb/query.rs:32-167] [code: rust/bc2-render/src/material.rs:197-212] [test: rust/bc2-formats/src/shaderdb/query.rs::blocks] [test: bfbc2-shader-tools/tests/test_shaderdb.py::quick] [doc: bfbc2-shader-tools/FORMAT.md:41-91]

### SHDB-PB-2: Name hashes are Adler-32 of the lower-case name

Status: **observed** · Variants: PC, BF1943

The u32 after a constant's or engine texture's name is Adler-32 of the name in lower case.

Evidence: [data: all 16,062 records of mp_005, mp_006, sp_01 Dx9 / Dx10 and BF1943's Wake (doc)] [doc: bfbc2-shader-tools/FORMAT.md:72-73]

### SHDB-BIND-1: Binding tables (section 2)

Status: **verified** · Variants: PC, Xbox 360, BF1943

Each record: u16 k, u16 first (the first register after the engine's: where a material's constants start; Dx10 /
11 the `$Globals` size in registers when there are none), then k x 4 bytes: u8 engine parameter id, u8 register
(Dx9 c<N>; Dx10 / 11 the byte offset in `$Globals` / 16), u8 index (0xFF, or the point light / shadow cascade number),
u8 count (1; the declared element count for bone arrays). The ids are the renderer's per-draw values (view and
projection matrices, world matrix, bones, camera, time, lights; the table is in the doc cited).

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:309-316] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [test: rust/bc2-render/src/dxbc.rs::globals_size_rounds_to_registers] [doc: bfbc2-shader-tools/FORMAT.md:93-110]

### SHDB-REG-1: Register tables (section 3)

Status: **verified** · Variants: PC, Xbox 360, BF1943

Each record: u32 k, then k x 4 bytes: u8 engine id, u8 kind (0x40 2D, 0x42 cube), u8 texture slot, u8 index (0xFF).
They bind the engine's own textures (the shadow map, the reflection cube) to a shader's slots. A translated Dx10 / 11
shader must take its slots from its own compiled resource bindings, not a donor's table.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:317-324] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [doc: bfbc2-shader-tools/FORMAT.md:112-124]

### SHDB-SH-1: Shader records (sections 4 and 5)

Status: **verified** · Variants: PC, Xbox 360, BF1943

Each shader: guid, u32 code size, the code, u32 parameter block index, u32 binding table index, u32 register table
index (each index inside its section). The code: Dx9, one flag byte (bit 0: created only with a device capability)
then D3D9 bytecode; Dx10 / 11, a DXBC container; Xbox 360, Xenos microcode in the XDK container (XENOS-CON-1), no flag
byte. A vertex shader record continues with its vertex input description (SHDB-SH-2); a pixel shader record ends there.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:261-273] [code: rust/bc2-formats/src/shaderdb.rs:406-417] [test: rust/bc2-formats/src/shaderdb.rs::errors] [doc: bfbc2-shader-tools/FORMAT.md:127-150]

### SHDB-SH-2: Vertex input description

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Kind | After the vertex shader's three indices |
|---|---|
| Dx9 | u32 k, k x 8-byte `D3DVERTEXELEMENT9` |
| Dx10 / 11 | u32 n, n bytes (probably the input signature); u32 m, m x 0x1C-byte `D3D10/11_INPUT_ELEMENT_DESC` (the name pointer unused); u32 k, k x cstr semantic names |
| Xbox 360 | u32 k, k x 12-byte Xenon vertex elements (u16 stream, u16 offset, u32 type, u8 method, usage, usage index, pad; ending with the end element), then 16 bytes: one u8 stride per stream |

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:275-296] [code: rust/bc2-render/src/material.rs:177-195] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [doc: bfbc2-shader-tools/FORMAT.md:139-145]

### SHDB-PROG-1: Program records (section 6a)

Status: **verified** · Variants: PC, Xbox 360, BF1943

n x 0x30 bytes:

| Offset | Field |
|---|---|
| 0x00 | guid: program id; unique; programs sorted by it (by PC byte order: SHDB-X-2) |
| 0x10 | u32 render state word (SHDB-RS-1) |
| 0x14 | u32: low byte the alpha-test reference |
| 0x18 | u32 overwritten at load with a checksum of the program's key (the stored value does not matter) |
| 0x1C | u32 0 |
| 0x20 | u32 vertex shader index |
| 0x24 | u32 pixel shader index |
| 0x28, 0x2C | u32, u32 parameter block indices (the second holds the textures) |

The loader inserts programs into a sorted map by id: a repeated id would be dropped.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:37-38] [code: rust/bc2-formats/src/shaderdb/query.rs:170-178] [test: rust/bc2-formats/src/shaderdb/query.rs::program_indices] [exe: 0x9CC520 the sorted vector-map insert (FORMAT.md)] [doc: bfbc2-shader-tools/FORMAT.md:152-164]

### SHDB-KEY-1: Program keys (section 6b)

Status: **verified** · Variants: PC, Xbox 360, BF1943

`u32 n` again - it must equal the program count - then n keys, key *i* belonging to program *i*: 0x58 bytes (version
120), 0x5C bytes (version 117, BF1943: 44 permutation bytes). 0x00 the guid of the entry the program belongs to;
0x10 the guid of a shader fragment, or zero; 0x20 the guid of a vertex layout; 0x30 the permutation: byte 0 the pass
(0 main, 5 depth only), byte 3 the mesh kind (0 rigid, 1 bone-transformed, 4 skinned character, 9 destructible
composite: one visibility register per part), byte 4 instanced (1: stream 1 carries the instance's three world
rows), bytes 8 and 14 a point light. The renderer looks a program up by the part's layout guid and the permutation;
an entry with no matching program draws with something else, silently.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:39-42] [code: rust/bc2-formats/src/shaderdb.rs:167-171] [code: rust/bc2-formats/src/shaderdb.rs:385-390] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [test: rust/bc2-formats/src/shaderdb.rs::errors] [doc: bfbc2-shader-tools/FORMAT.md:166-175]

### SHDB-LAY-1: Vertex layouts (section 7)

Status: **verified** · Variants: PC, Xbox 360, BF1943

n x 0x59 bytes: guid, then a 0x49-byte body. Body: 16 element slots of 4 bytes (u8 format, u8 usage, u8 offset, u8
stream; formats and usages in `formats/mesh.md` MESH-VTX-1 / MESH-VTX-2), the element count (1 - 16) at body byte 64
followed by a zero byte, and stream 0's stride at body byte 69. Layout GUIDs are unique in a file and named only by
GUID (program keys, mesh parts).

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:43-44] [code: rust/bc2-convert/src/meshconv.rs:86-98] [code: rust/bc2-convert/src/meshconv.rs:383-444] [test: bfbc2-shader-tools/tests/test_shaderdb.py::xbox] [doc: bfbc2-shader-tools/FORMAT.md:218-221] [doc: bfbc2-console-convert/FORMAT.md:183-185]

### SHDB-LAY-2: Layout and mesh part strides agree

Status: **implemented** · Variants: PC, Xbox 360

A part's stride (MESH-PART-1) is the stride of the layout its mesh-part name maps to; the console's packed formats
give Xbox 360 layouts smaller strides (0x0C) than their PC twins (0x10), and then different GUIDs.

Evidence: [code: rust/bc2-convert/src/meshconv.rs:251] [doc: bfbc2-shader-tools/FORMAT.md:345-351]

### SHDB-NAME-1: Entries (section 8.1)

Status: **verified** · Variants: PC, Xbox 360, BF1943

`u32 n`, n x {cstr name, guid}. An entry's name is `<mesh asset name>/<shader asset name>` (or a shader name alone for
decals and terrain patches), in the assets' own case; names are unique. **The guid is MD5 of the name's bytes**
(Xbox 360: SHDB-X-2). An entry is found only by its exact name.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:393-403] [code: rust/bc2-formats/src/shaderdb/query.rs:323-330] [code: rust/bc2-formats/src/shaderdb/query.rs:468-485] [test: rust/bc2-formats/src/shaderdb/query.rs::md5_vectors] [test: bfbc2-shader-tools/tests/test_shaderdb.py::game] [doc: bfbc2-shader-tools/FORMAT.md:225-232]

### SHDB-NAME-2: Fragments (section 8.2)

Status: **verified** · Variants: PC, Xbox 360, BF1943

The same shape: cstr fragment name, guid. Every non-zero fragment guid a program key names is listed here.

Evidence: [code: rust/bc2-formats/src/shaderdb/query.rs:533-546] [test: bfbc2-shader-tools/tests/test_shaderdb.py::quick] [doc: bfbc2-shader-tools/FORMAT.md:233-234]

### SHDB-NAME-3: Mesh parts (section 8.3)

Status: **verified** · Variants: PC, Xbox 360, BF1943

The same shape: cstr `<mesh>_lod<N>/<part>/<technique>` (techniques such as `Standard`, `StreamInstancingFloat`; a
few stand-alone names), guid of a vertex layout in section 7. The same part name maps to the same layout in every
level. PC adds a `StreamInstancingFloat` name for every part; the Xbox 360 file has `Standard` and
`StreamInstancingHalf`.

Evidence: [code: rust/bc2-formats/src/shaderdb/query.rs:547-560] [test: bfbc2-shader-tools/tests/test_shaderdb.py::xbox] [doc: bfbc2-shader-tools/FORMAT.md:235-238] [doc: bfbc2-shader-tools/FORMAT.md:345-351]

### SHDB-GRP-1: Entry groups

Status: **verified** · Variants: PC, Xbox 360, BF1943

`u32 n`, n x {guid of an entry, u32 k, k x u16 program index}. One group per entry and one entry per group; groups
sorted by guid, each list ascending; every program in exactly one group, the group of the entry its key names.
Because the indices are u16, a database holds at most 65,535 programs (the largest seen: 24,932).

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:330-351] [code: rust/bc2-formats/src/shaderdb/query.rs:416-417] [code: rust/bc2-formats/src/shaderdb/query.rs:493-531] [test: bfbc2-shader-tools/tests/test_shaderdb.py::quick] [doc: bfbc2-shader-tools/FORMAT.md:240-242]

### SHDB-CHK-1: What a modified database must satisfy

Status: **verified** · Variants: PC, Xbox 360, BF1943

Each section read to its end and nothing after section 8; every index inside its section (shaders -> 1, 2, 3;
programs -> 4, 5, 1; groups -> 6); at most 65,535 programs; keys equal programs in number; program ids unique and
sorted; entry guids MD5 of their names, names unique; groups and entries one to one, sorted, every program in exactly
one group and that group the entry its key names; every fragment and layout guid a key or mesh part names present.
Program, shader, layout and fragment ids are copied, never computed. Every DICE database passes.

Evidence: [code: rust/bc2-formats/src/shaderdb/query.rs:437-562] [test: rust/bc2-formats/src/shaderdb/query.rs::needed_and_coverage] [test: bfbc2-shader-tools/tests/test_shaderdb.py::quick] [test: bfbc2-shader-tools/tests/test_shaderdb_native.py::main] [doc: bfbc2-shader-tools/FORMAT.md:259-274]

### SHDB-WRITE-1: Writing gives the bytes back

Status: **verified** · Variants: PC, Xbox 360, BF1943

The suite keeps every record as the file's bytes in the file's byte order; writing a parsed database gives the same
bytes. Every PC level database of the game copy, every BFBC2 Xbox 360 level database (20) and BF1943's Wake
round-trip.

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:474-531] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [test: bfbc2-shader-tools/tests/test_shaderdb.py::xbox] [test: bfbc2-shader-tools/tests/test_shaderdb.py::wake] [doc: bfbc2-shader-tools/FORMAT.md:9-13]

### SHDB-LVL-1: A level draws through its own databases only

Status: **verified** · Variants: PC

Each level bundle holds its own `levels/<lv>/shaders/dx{9,10,11}_single.res`; no shared bundle (`mp_common`,
`sp_common`, `nam_common`, `ww2_common`) holds a shader database. A level's own files carry the entries of the meshes
it draws wherever the mesh lives: of the meshes MP_005 reaches (its own partitions and every partition they name),
384 of `mp_common`'s 431 have entries in MP_005's database, SP_01 157 of `sp_common`'s 163. The meshes a level
reaches without entries are pieces and variants it never draws (MP_005 69 of 489 with OPFOR's own Package
additions, SP_01 7 of 329). A mesh brought in from another level or set has no entry until one is added: Guadalcanal's
radar bunker (2 entries, 12 mesh-part names there) has none in SP_01's files.

[test: rust/bc2-levelwrite/tests/export_xf1.rs::stock_levels_carry_their_meshes_entries]
[data: the game copy, 2026-11-21: SP_01, MP_005, MP_002 reached and covered by bundle; mp_common, sp_common, ww2_common, nam_common: no database]
[doc: rust/BENCHMARKS.md]

### SHDB-LVL-2: An export adds the entries of the meshes it brings in

Status: **verified** · Variants: PC

An exported level's databases (the base's, renamed) get, for every mesh the level reaches that they have no entry
for, the entries and mesh-part names of that mesh from the database of the same kind of the first level that has
them: the mode's donor, the base, the levels the closure copied from, then the game copy's levels with DICE's
(`Dist`) first. The merge (SHDB-CHK-1's checks on the result) copies each program, shader, parameter block,
fragment and vertex layout by content, so an added entry draws as in its source; a mesh no level's database has is
listed. The check refuses a mesh the level reaches without entries unless the stock base reaches it without them too
or no database has it.

[code: rust/bc2-levelwrite/src/shaders.rs:218-316] [code: rust/bc2-formats/src/shaderdb/merge.rs:275-306]
[code: rust/bc2-levelwrite/src/export.rs:3385-3447]
[test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]

### SHDB-LVL-3: A cloned entry is the donor's under a new name

Status: **implemented** · Variants: PC, Xbox 360

A mesh the databases never drew (a converted or ported mesh, a weapon variant) takes a donor entry again under its
own name: the entry's GUID is MD5 of the new name (SHDB-NAME rules), each of its programs gets the id MD5(new name +
the donor program's id) - a program id never repeats - and a key naming the new entry, with the donor key's fragment,
layout and flags; the programs' shaders, tables, fragments and vertex layouts are shared by content, and their
parameter blocks are copies with texture names replaced in place (a whole name at the start of its 0x80-byte field,
padded with zeros; all renames at once). A rename the entry does not use, a new name longer than its field, a
program, fragment or layout the files do not hold refuses the clone and nothing is added. A mesh part's layout is
registered by name (`<mesh>_lod<N>/<part>/<technique>`), the layout copied from the donor when the file lacks it.
How OPFOR's WW2 and Onslaught ports give their meshes entries (21 call sites); the files they make match the WW2
oracle.

[code: rust/bc2-formats/src/shaderdb/merge.rs:123-201] [code: rust/bc2-formats/src/shaderdb/merge.rs:697-863]
[code: rust/bc2-formats/src/shaderdb/merge.rs:918-950]
[test: rust/bc2-formats/src/shaderdb/merge.rs::clone_entry_renames_ids_keys_and_textures]
[test: rust/bc2-formats/src/shaderdb/merge.rs::clone_entry_refusals_add_nothing]
[test: bfbc2-shader-tools/tests/test_shaderdb_merge.py::main]

## Render state

### SHDB-RS-1: The program's state word

Status: **verified** · Variants: PC, Xbox 360, BF1943

As the Dx10 / Dx11 renderer reads program +0x10:

| Bit(s) | Meaning |
|---|---|
| 0x1 | transparent: no depth write unless 0x4 |
| 0x2 | set on decal and overlay layers; not read by the Dx10 / 11 state builders |
| 0x4 | depth write even when transparent |
| 0x10 | alpha test, compiled into the pixel shader on Dx10 / 11 (not a render state there) |
| 0x20 | alpha to coverage |
| 0x40 | two-sided (cull none); otherwise cull back; front faces counter-clockwise |
| 0x80 | depth test, less-or-equal |
| 0x100 | depth write when not transparent |
| 0x400 - 0x10000 | colour blend mode 1 - 7 from the lowest set bit |
| 0x20000 - 0x800000 | alpha blend mode 1 - 7 the same way |

Blend modes 1 - 7: lerp (premultiplied: one, inverse source alpha), additive, subtractive, reverse subtractive,
multiply (destination colour / alpha, zero), min, max. A state of (0, 0) means none recorded. The stock levels' Dx10
and Dx11 databases use 57 distinct words over 1,940,372 programs; Xbox 360 databases use the same encoding.

Evidence: [code: rust/bc2-render/src/material.rs:203-307] [test: rust/bc2-render/src/material.rs::render_states_as_dice_encodes_them] [exe: draw setup 0x9BC230; blend 0x98C6D0 (Dx10 0x98C220), depth-stencil 0x99FF00, rasterizer 0x99FD80; factor tables 0x1445C28 / 0x1445C98 (FORMAT.md)] [doc: bfbc2-shader-tools/FORMAT.md:177-216]

### SHDB-RS-2: Alpha test is in the Dx10 / 11 pixel shader

Status: **observed** · Variants: PC

On Dx10 / 11 nothing applies state bit 0x10 or the reference byte: DICE's compiler bakes `alpha > reference / 255`
into the pixel shader as a compare and `discard`. A translated or new pixel shader for an alpha-tested material must
discard itself.

Evidence: [data: stock levels' unblended main-pass Dx11 programs with 0x10: 75,718 with the literal test, 432 another discard form, 17,348 none (alpha output literal 1) (doc)] [exe: the Dx10 state device's alpha-test call is an empty `ret 8` (vtable 0x1449898 + 0x20)] [code: rust/bc2-render/src/dxbc.rs:121] [doc: bfbc2-shader-tools/FORMAT.md:188]

## Compiled code

### SHDB-DXBC-1: Dx10 / 11 code is DXBC

Status: **verified** · Variants: PC

Dx10 / 11 shader code is a DXBC container (`DXBC` magic, chunk table); the suite reads the `RDEF` chunk for
`$Globals` variables and resource bindings (the `$Globals` size rounded up to whole 16-byte registers) and scans the
instruction stream for `discard`.

Evidence: [code: rust/bc2-render/src/dxbc.rs:61-121] [test: rust/bc2-render/src/dxbc.rs::globals_size_rounds_to_registers] [test: rust/bc2-render/src/dxbc.rs::discard_found_in_the_instruction_stream]

## Xbox 360 differences

### SHDB-X-1: Byte order and version

Status: **verified** · Variants: Xbox 360, BF1943

The layout is the PC's (SHDB-HDR-2) with every number big-endian: the version, every count and size, the shader and
program indices, the groups' u16 indices, the parameter blocks' header words and floats, an engine texture's u16
sampler. Bytes stay bytes. A file is big-endian when its first byte is 0 and its fourth is not. Version 120 for
BFBC2's Xbox 360 files, 117 for BF1943 (keys 0x5C bytes).

Evidence: [code: rust/bc2-formats/src/shaderdb.rs:173-178] [test: rust/bc2-formats/src/shaderdb.rs::big_endian_detection] [code: rust/bc2-formats/src/shaderdb/open.rs:38-73] [test: rust/bc2-formats/src/shaderdb/open.rs::opens_as_shaderdb_py] [test: tests/test_sd1_native.py::area_parse] [test: rust/bc2-formats/src/shaderdb.rs::round_trips] [doc: bfbc2-shader-tools/FORMAT.md:318-333]

### SHDB-X-2: GUIDs have big-endian fields

Status: **verified** · Variants: Xbox 360, BF1943

A GUID's first three fields (u32, u16, u16) are stored byte-reversed against the PC's bytes, the last 8 bytes as they
are. So a console entry's GUID is MD5(name) with its first 4, 2 and 2 bytes reversed, the same asset has the swapped
GUID of its PC twin, and programs and groups are sorted by the swapped (PC-order) bytes, not the stored bytes.

Evidence: [code: rust/bc2-formats/src/shaderdb/query.rs:376-386] [code: rust/bc2-formats/src/shaderdb/query.rs:420-423] [test: bfbc2-shader-tools/tests/test_shaderdb.py::xbox] [doc: bfbc2-shader-tools/FORMAT.md:324-327]

### XENOS-CON-1: The XDK shader container

Status: **verified** · Variants: Xbox 360, BF1943

All words big-endian u32. 0x00 magic: 0x102A1100 pixel, 0x102A1101 vertex; 0x04 virtual size (everything before the
microcode section); 0x08 physical size (the microcode section) - virtual + physical = the blob's size; 0x0C 0; 0x10
constant table offset (0x24); 0x14 literal (definition) table offset or 0; 0x18 shader header offset; 0x1C, 0x20 0.
The constant table, literal table and header tile `[0x24, virtual size)` without gaps. The header's physical offset +
physical size must equal the physical section: the microcode ends the blob.

Evidence: [code: rust/bc2-shader/src/xenos.rs:1914-1975] [test: rust/bc2-shader/tests/robust.rs::a_blob_cut_anywhere_is_refused_not_a_panic] [test: bfbc2-shader-tools/tests/test_xenos_native.py::quick] [data: all 735 vertex and 815 pixel shaders of BF1943's Wake database (doc)] [doc: bfbc2-shader-tools/XENOS.md:11-23]

### XENOS-CON-2: Constant and literal tables

Status: **implemented** · Variants: Xbox 360, BF1943

The constant table is u32 size then a big-endian D3DX shader constant table (offsets relative to its own start;
registers are D3D numbers, float4 c#, sampler s#). The literal table holds float definitions as {u16 register, u16
float count, u32 offset into the physical section}; a pixel shader's register is the GPU index - 256. BF1943 has one
float definition list per shader ending at c255 (vertex) or c251 (pixel); integer and bool literals never occur.

Evidence: [code: rust/bc2-shader/src/xenos.rs:1105] [doc: bfbc2-shader-tools/XENOS.md:25-50]

### XENOS-HDR-1: The shader header

Status: **implemented** · Variants: Xbox 360, BF1943

At the header offset: physical offset of the microcode (16 x the literal vec4 count), its size, a program-control
word (temporaries, export counts), a context word, the interpolator count; then for pixel shaders the interpolator
mask, an output mask (oC0 - oC3, oDepth) and one word per interpolator, and for vertex shaders the vertex-element
words (fetch slot, usage, usage index), interpolator words and export-slot words. Vertex fetches are unpatched
templates (fetch constant 95, format 0, stride 0): the real stream, offset and type come from the vertex shader's
input description (SHDB-SH-2) matched by usage and index.

Evidence: [code: rust/bc2-shader/src/xenos.rs:1237] [test: bfbc2-shader-tools/tests/test_xenos_native.py::game] [data: 3,602 vertex fetches of BF1943's Wake all templates (doc)] [doc: bfbc2-shader-tools/XENOS.md:52-80] [doc: bfbc2-shader-tools/XENOS.md:178-187]

### XENOS-UC-1: Microcode

Status: **verified** · Variants: Xbox 360, BF1943

The microcode is 12-byte slots: control-flow instructions first, two per slot (48 bits each), ending at the lowest
exec address; then ALU and fetch instructions, 3 words each, decoded as Xenia documents them (`XENOS.md` 2.1 - 2.3).
BF1943 uses only exec, exec_end, predicated execs, predicated jumps and alloc: no loops, calls or memexport. The suite
decodes every program of BF1943's and BFBC2's Xbox 360 databases and translates them to HLSL.

Evidence: [code: rust/bc2-shader/src/xenos.rs:1448] [test: rust/bc2-shader/tests/robust.rs::random_microcode] [test: bfbc2-shader-tools/tests/test_xenos_native.py::game] [doc: bfbc2-shader-tools/XENOS.md:82-110]

## Open questions

- Program state bit 0x2 (decal layers), most permutation bytes of a key, and how the renderer chooses among an entry's
  programs at draw time (not traced).
- The 0x49-byte layout body beyond the elements, count and stride (bytes 65-68, 70-72).
- Parameter block bytes 0x18 / 0x19 versus the binding table's `first`; the Dx9 renderer's use of the state word.
- Limits besides the u16 program index (number of shaders, blocks); what the engine does with an entry whose
  textures are not in the level.
- How DICE made program ids (not MD5 of the key); not needed, copied as they are.
- Xenos: vertex element bits 20-23, the vertex header word 0x290, interpolator bits 12-15, the vertex texture sampler
  to fetch constant mapping (tf22 for s0), int / bool literal encoding (`XENOS.md` 4).
