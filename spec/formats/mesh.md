# Meshes: MeshData and mesh sets

A mesh is two kinds of resource entry in a bundle (`formats/fbrb.md`, `formats/res.md`):

- the **mesh set**, `<mesh>.res`, FBRB type `RigidMeshSet`, `CompositeMeshSet`, `SkinnedMeshSet` or `TreeMeshSet`:
  the mesh's LODs, each LOD's shader names per shader set, bounds, and (by kind) parts or bones;
- one **MeshData** per LOD, `<mesh>_lod<N>_data.res`, FBRB type `MeshData`: the LOD's parts, then its vertex and
  index buffers.

The vertex layout of a part is **not** in either file: it is in the level's shader database (`formats/shaderdb.md`,
SHDB-NAME-3, SHDB-LAY-1).

Byte order: PC little-endian, Xbox 360 big-endian, the same grammar on both (BF1943 is Xbox 360). `cstr` is bytes up
to a zero byte. No field is aligned: records follow each other with no padding.

Sources: `rust/bc2-formats/src/meshdata.rs` (reader and PC writer), `rust/bc2-convert/src/{mesh,meshconv,meshport,
skinmesh}.rs` (vertex conversion, decoding, relayout), `bfbc2-console-convert/FORMAT.md` sections 3 and 4 (the
original description and its checks).

## MeshData

### MESH-HDR-1: MeshData header

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Offset | Type | Field |
|---|---|---|
| 0 | u32 | `h0`: the mesh kind (MESH-HDR-2) |
| 4 | u8 x 2 | `h1`: two bytes kept as they are (`00 00` or `01 00` on PC) |
| 6 | u8 | part count *n* (at most 255) |
| 7 | | *n* part records (MESH-PART-1) |
| | | four part lists (MESH-LIST-1) |
| | u32 | vertex bytes *V* |
| | u32 | index bytes *I* |
| | u32 | wide indices: 1 = 32-bit indices, 0 = 16-bit |
| | *V* bytes | the vertex buffer |
| | *I* bytes | the index buffer |

The u32 and u16 fields are in the platform's byte order.

Evidence: [code: rust/bc2-formats/src/meshdata.rs:117-155] [test: rust/bc2-formats/src/meshdata.rs::mesh_header_round_trip] [test: bfbc2-console-convert/tests/test_meshdata_native.py::main] [data: 2,255 distinct PC MeshData resources of the game copy parsed with this grammar, 0 failures (2026-11-19); 7,276 game MeshData and 2,571 Xbox 360 Onslaught MeshData parse (doc)] [doc: bfbc2-console-convert/FORMAT.md:162-171] [doc: rust/BENCHMARKS_ARCHIVE_1.md:3158]

### MESH-HDR-2: `h0` is the mesh kind

Status: **verified** · Variants: PC

`h0` names the kind of the mesh set the LOD belongs to: **0** rigid, **1** skinned, **2** composite, **4** tree. No
other value occurs. The readers keep it as an opaque word.

Evidence: [data: game copy 2026-11-19: every MeshData named `<set>_lod<i>_data.res` of 1,507 PC mesh sets: RigidMeshSet 1,505 LODs all 0, SkinnedMeshSet 378 all 1, CompositeMeshSet 471 all 2, TreeMeshSet 86 all 4; over 2,255 distinct MeshData: 0 x 1,451, 2 x 466, 1 x 255, 4 x 83] [test: rust/bc2-formats/tests/spec_corpus.rs::meshes] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 4,265 distinct MeshData: h0 0 x 2,610, 2 x 879, 1 x 618, 4 x 158; every mesh set LOD's MeshData has its set's kind (rigid 0, skinned 1, composite 2, tree 4)]

### MESH-HDR-3: The file is exactly header + V + I

Status: **verified** · Variants: PC, Xbox 360, BF1943

The vertex buffer starts right after the wide-indices word, and the file ends exactly after the index buffer: the
header's end + *V* + *I* = the file length. A reader must refuse a file for which this does not hold.

Evidence: [code: rust/bc2-formats/src/meshdata.rs:142-146] [test: rust/bc2-formats/src/meshdata.rs::mesh_header_round_trip] [data: 2,255 of 2,255 distinct PC MeshData of the game copy (2026-11-19)]

### MESH-HDR-4: PC MeshData use 16-bit indices

Status: **verified** · Variants: PC

Every PC MeshData has wide indices 0 (16-bit, LE) and an even *I*. Xbox 360 MeshData may have wide indices 1
(32-bit BE); the converter narrows them to 16 bits (MESH-CONV-3), and the PC writer always writes 0.

Evidence: [data: wide = 0 and I even in 2,255 of 2,255 distinct PC MeshData (2026-11-19)] [code: rust/bc2-formats/src/meshdata.rs:192-194] [doc: bfbc2-console-convert/FORMAT.md:197-198] [test: rust/bc2-formats/tests/spec_corpus.rs::meshes] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: wide = 0 and I even in 4,265 of 4,265 distinct PC MeshData]

### MESH-PART-1: Part record

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Field | Type | Meaning |
|---|---|---|
| name | cstr | the part's name (the mesh-part names of the shader database are `<mesh>_lod<N>/<name>/<technique>`, SHDB-NAME-3) |
| triangles | u32 | primitives drawn (MESH-PRIM-1) |
| vertices | u32 | vertex count of the part's window |
| first index | u32 | where the part's indices start, counted in indices from the start of the index buffer |
| vertex offset | u32 | byte offset of the part's vertex 0 in the vertex buffer |
| stride | u8 | bytes per vertex |
| k | u8 | primitive type (MESH-PRIM-1) |
| fa | u8 | kept as it is |
| fb | u8 | the part's bone count (equals the u16 below) |
| bone count | u16 | *b* |
| bones | u16 x *b* | skeleton bone indices the part's vertices use |

Evidence: [code: rust/bc2-formats/src/meshdata.rs:123-135] [test: rust/bc2-formats/src/meshdata.rs::mesh_header_round_trip] [doc: bfbc2-console-convert/FORMAT.md:166-167]

### MESH-PART-2: `fb` equals the bone count

Status: **verified** · Variants: PC

A part's `fb` byte equals its u16 bone count.

Evidence: [data: 7,191 of 7,191 parts of 2,255 distinct PC MeshData (2026-11-19)] [doc: bfbc2-console-convert/FORMAT.md:167] [test: rust/bc2-formats/tests/spec_corpus.rs::meshes] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 13,355 of 13,355 parts of 4,265 distinct PC MeshData]

### MESH-PART-3: Vertex addressing

Status: **implemented** · Variants: PC, Xbox 360, BF1943

Vertex *i* of a part (the value of an index) is at byte `vertex offset + i x stride` of the vertex buffer. A part
uses the vertices from its lowest to its highest index; parts may share a vertex window (a depth-only part drawing
other parts' vertices), and a depth-only part's index range may cover several parts' triangles. The vertex offset
is usually, but not always, a whole number of the part's own strides.

Evidence: [code: rust/bc2-convert/src/skinmesh.rs:72] [code: rust/bc2-convert/src/meshconv.rs:251] [data: vertex offset a multiple of the stride in 7,058 of 7,191 PC parts, not in 133 (2026-11-19)] [doc: bfbc2-console-convert/FORMAT.md:173-181]

### MESH-PART-4: Strides

Status: **observed** · Variants: PC

PC part strides are multiples of 4 between 16 and 80 bytes; the stride equals the layout record's stream-0 stride
(SHDB-LAY-2) of the part's layout.

Evidence: [data: PC strides over 7,191 parts: 48 x 3,650, 16 x 1,817, 32 x 988, 64 x 546, 68 x 95, 72 x 65, 76 x 24, 80 x 6 (2026-11-19)] [code: rust/bc2-convert/src/meshconv.rs:88-98]

### MESH-PRIM-1: Primitive type `k`

Status: **verified** · Variants: PC, Xbox 360, BF1943

`k` = **3**: a triangle list, the part's indices are `3 x triangles`. `k` = **5**: a triangle strip, the indices are
`triangles + 2`; odd triangles reverse their winding; repeated indices join strips with degenerate triangles; there
is no restart index. BFBC2's PC meshes are lists only (k = 3 in every part); BF1943 draws tree bark and some
depth-only parts as strips (81 parts).

Evidence: [code: rust/bc2-convert/src/meshconv.rs:100-130] [test: rust/bc2-convert/src/meshconv.rs::strips] [test: bfbc2-console-convert/tests/test_trees.py::quick] [data: k = 3 in 7,191 of 7,191 PC parts (2026-11-19)] [doc: bfbc2-console-convert/FORMAT.md:173-181]

### MESH-PRIM-2: A strip as a list

Status: **verified** · Variants: BF1943, Xbox 360

A strip `s` becomes a list by taking each window `(s[i], s[i+1], s[i+2])`, dropping it when two of its indices are
equal (degenerate), and writing it as `(a, b, c)` for even `i` and `(b, a, c)` for odd `i`. The game's PC renderer
needs lists (MESH-PRIM-1), so a ported strip part must be unrolled; the converter appends the unrolled indices after
the existing ones so no other part's first index moves.

Evidence: [code: rust/bc2-convert/src/meshconv.rs:114-130] [test: rust/bc2-convert/src/meshconv.rs::strips] [test: bfbc2-console-convert/tests/test_trees.py::game] [doc: bfbc2-console-convert/FORMAT.md:177-180]

### MESH-LIST-1: The four part lists

Status: **verified** · Variants: PC, Xbox 360, BF1943

After the parts: four lists, each `u32 n` then `n` bytes, each byte a part index. Their meaning is not known
(MESH-OQ). Observed: the first list is most often non-empty (1 to 10 entries), the second and third mostly empty, the
fourth mostly one entry.

Evidence: [code: rust/bc2-formats/src/meshdata.rs:136-141] [test: rust/bc2-formats/src/meshdata.rs::mesh_header_round_trip] [data: 2,255 PC MeshData: list 0 empty in 85, list 1 empty in 1,983, list 2 empty in 2,199, list 3 one entry in 1,951 (2026-11-19)]

### MESH-WRITE-1: The PC writer

Status: **verified** · Variants: PC

The suite writes a MeshData as MESH-HDR-1 in little-endian, `h0`, `h1`, the parts and lists as read, *V* and *I*
from the buffers given, wide indices 0. A header read from a PC file and written with its own buffers gives the
same bytes.

Evidence: [code: rust/bc2-formats/src/meshdata.rs:170-198] [test: rust/bc2-formats/src/meshdata.rs::mesh_header_round_trip] [test: bfbc2-console-convert/tests/test_meshdata_native.py::main]

## Vertex elements

### MESH-VTX-1: Element formats

Status: **verified** · Variants: PC, Xbox 360, BF1943

A layout element's format byte (SHDB-LAY-1) gives its size:

| Format | Value | Bytes | Platforms |
|---|---|---|---|
| 0x02 | float2 | 8 | both |
| 0x03 | float3 | 12 | both |
| 0x04 | float4 | 16 | both |
| 0x05 | half | 2 | both |
| 0x06 | half2 | 4 | both |
| 0x07 | half3 | 6 | both |
| 0x08 | half4 | 8 | both |
| 0x0C | ubyte4 (bone indices, weights, colours) | 4 | both |
| 0x0D | unsigned normalized bytes w, z, y, x | 4 | Xbox 360 only |
| 0x2B | signed normalized 11:11:10 (x bits 0-10, y 11-21, z 22-31) | 4 | Xbox 360 only |

Any other format is refused by the converter.

Evidence: [code: rust/bc2-convert/src/mesh.rs:37-47] [code: rust/bc2-convert/src/meshconv.rs:161-173] [test: rust/bc2-convert/src/mesh_tests.rs::refuses_what_the_reference_does_not_convert] [doc: bfbc2-console-convert/FORMAT.md:185-187]

### MESH-VTX-2: Element usages

Status: **implemented** · Variants: PC, Xbox 360, BF1943

The usage byte: 0 texture coordinate, 1 position, 2 bone indices, 3 bone weights, 4 normal, 5 tangent, 6 binormal,
7 position w (an R16 that may alias position.w at the same offset). A part's texture coordinates are the first
usage-0 stream-0 element of format half2 or float2, else the first usage-0 stream-0 element of another float
format (soldiers start with a half4 that is not the texture's).

Evidence: [code: rust/bc2-convert/src/skinmesh.rs:40-54] [code: rust/bc2-convert/src/meshconv.rs:132-150] [test: tests/test_converter_units.py::texcoords] [doc: bfbc2-console-convert/FORMAT.md:187-190]

### MESH-VTX-3: PC weapons

Status: **observed** · Variants: PC

PC weapon meshes bind one bone per vertex (all four indices equal, weights 255, 0, 0, 0); normal, tangent and
binormal have w = 1; the binormal is -(normal x tangent), + where the texture is mirrored.

Evidence: [data: DICE's PC weapon meshes, recorded in the converter's format notes] [doc: bfbc2-console-convert/FORMAT.md:191-192]

## Xbox 360 -> PC conversion

### MESH-CONV-1: Shared formats are byte-swapped per value

Status: **verified** · Variants: Xbox 360 -> PC

An element of a format both platforms share converts by reversing each value's bytes: 4-byte values for the float
formats (0x02-0x04), 2-byte values for the half formats (0x05-0x08), nothing for ubyte4 (0x0C) - except a colour
ubyte4, whose four bytes are reversed as one word.

Evidence: [code: rust/bc2-convert/src/mesh.rs:52-59] [code: rust/bc2-convert/src/mesh.rs:196-217] [test: rust/bc2-convert/src/mesh_tests.rs::swaps_by_value_width]

### MESH-CONV-2: Console-only formats decode to half4

Status: **verified** · Variants: Xbox 360 -> PC

0x2B (signed 11:11:10, big-endian word) becomes PC half4 (x / 1023, y / 1023, z / 511, 1), each clamped to -1 and
rounded to a half to nearest, ties to even, from the double. 0x0D (bytes w, z, y, x) becomes half4 (x, y, z, w) /
255.

Evidence: [code: rust/bc2-convert/src/mesh.rs:173-232] [test: rust/bc2-convert/src/mesh_tests.rs::packed_normals_and_bytes] [test: rust/bc2-convert/src/mesh_tests.rs::halves_round_to_nearest_even_from_the_double]

### MESH-CONV-3: Buffers and regions

Status: **verified** · Variants: Xbox 360 -> PC

Each part is converted with a (console layout, PC layout) pair, matched element by element. Parts sharing a console
region and a PC layout share one converted region with unchanged indices; parts given different PC layouts for one
console region each get the window of vertices they use, at an offset that keeps their indices. Bytes of the PC
vertex no element fills are zero. The index buffer is converted whole: byte-swapped, 32-bit console indices narrowed
to 16 bits.

Evidence: [code: rust/bc2-convert/src/mesh.rs:236] [test: rust/bc2-convert/src/mesh_tests.rs::vertices_in_order_with_zeroed_gaps] [test: bfbc2-console-convert/tests/test_mesh_decode.py::bfbc2_xbox] [doc: bfbc2-console-convert/FORMAT.md:194-200] [doc: rust/BENCHMARKS_ARCHIVE_1.md:766-767]

## Mesh sets

### MSET-HDR-1: Mesh set header

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Field | Type | Meaning |
|---|---|---|
| a | u32 | kept as it is (42 in every PC mesh set) |
| z | u8 | kept as it is (0 in every PC mesh set) |
| LOD settings | cstr | the name of the mesh's LOD settings |
| LOD count | u32 | then that many LOD records (MSET-LOD-1) |

The file ends exactly after the last LOD: a reader refuses bytes after it.

Evidence: [code: rust/bc2-formats/src/meshdata.rs:277-351] [test: rust/bc2-formats/src/meshdata.rs::mesh_sets_round_trip] [test: bfbc2-console-convert/tests/test_meshdata_native.py::main] [data: 1,428 distinct PC mesh sets of the game copy (1,006 rigid, 196 composite, 176 skinned, 50 tree) read to their last byte; a = 42 and z = 0 in all (2026-11-19)] [doc: bfbc2-console-convert/FORMAT.md:206-216]

### MSET-LOD-1: LOD record

Status: **verified** · Variants: PC, Xbox 360, BF1943

| Field | Type | Kinds |
|---|---|---|
| x | u8 (0 in every PC set) | all |
| shader count *n* | u8 | all |
| set count *s* | u8 | all |
| *s* shader sets | cstr name (`<mesh asset>/<shader set guid>`), then *n* x cstr shader name (one per part, in part order) | all |
| bounds | 6 x f32: min x, y, z, max x, y, z | all |
| size | u32 (MSET-LOD-2) | all |
| extra | u32 (0 in every PC set) | all |
| flag | u8 | Rigid, Composite, Tree (not BF1943's rigid sets: MSET-LOD-3) |
| parts | u32 count, then per part: u32 hash (MSET-PART-1), cstr name, 6 x f32 bounds, 12 x f32 transform | Composite, Tree |
| part masks | *n* x 3 x u64: per shader, the parts that use it | Composite, Tree |
| billboard | cstr billboard shader ('' when none), u32 (0 or 1; meaning unknown) | Tree |
| bones | u32 count *b*, *b* x u32 skeleton bones, *b* x u32 bone-name hashes | Skinned (no flag byte) |

Evidence: [code: rust/bc2-formats/src/meshdata.rs:285-345] [test: rust/bc2-formats/src/meshdata.rs::mesh_sets_round_trip] [data: PC LODs 2026-11-19: x = 0 and extra = 0 in all 2,246; rigid flag 0 x 1,431 / 1 x 20, composite 0 x 370 / 1 x 95, tree 0 x 83; tree value 0 x 79 / 1 x 4] [doc: bfbc2-console-convert/FORMAT.md:206-216]

### MSET-LOD-2: A LOD's size is its MeshData's buffers + 12

Status: **verified** · Variants: PC, Xbox 360

A LOD's `size` is its MeshData's vertex bytes + index bytes + 12, not the MeshData file's length. A writer that
changes a MeshData's buffers must update the set's `size`.

Evidence: [data: game copy 2026-11-19: 2,426 of 2,440 PC LODs with their MeshData (rigid 1,505 / 1,505, composite 471 / 471, tree 86 / 86, skinned 364 / 378 - 14 skinned LODs differ); all 2,463 Xbox 360 and 619 PC mp_common LODs (doc)] [doc: bfbc2-console-convert/FORMAT.md:218-219] [test: rust/bc2-formats/tests/spec_corpus.rs::meshes] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 4,265 of 4,265 distinct mesh set LODs with their MeshData (rigid 2,610, composite 879, skinned 618, tree 158): no exception in DICE's files (the 14 skinned LODs of the 2026-11-19 count were not DICE's)]

### MSET-LOD-3: The flag byte by game

Status: **verified** · Variants: PC, Xbox 360, BF1943

BFBC2's rigid, composite and tree sets (PC and Xbox 360) have the flag byte in each LOD; skinned sets never do;
BF1943's rigid sets do not (each LOD one byte shorter). A set read without it is written with flag 0 in BFBC2's
form.

Evidence: [code: rust/bc2-formats/src/meshdata.rs:278] [code: rust/bc2-formats/src/meshdata.rs:296-302] [test: bfbc2-console-convert/tests/test_meshdata_native.py::main] [doc: bfbc2-console-convert/FORMAT.md:211]

### MSET-LOD-4: Shader names per part

Status: **verified** · Variants: PC

A LOD's shader count *n* equals its MeshData's part count, and the *i*-th shader name of a shader set belongs to part
*i*. The renderer finds a part's shader-database entry by the mesh set's shader name (SHDB-NAME-1), not by the
partition's material.

Evidence: [data: n equals the part count in 2,440 of 2,440 PC LODs (2026-11-19)] [inferred: the Type 38 port kept a donor's shader names in the .res and drew with the donor's entry, though its partition named its own (OPFOR's 2026-11-08 weapon port notes)] [test: rust/bc2-formats/tests/spec_corpus.rs::meshes] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: n equals the part count in 4,265 of 4,265 LODs]

### MSET-PART-1: Composite and tree part hashes

Status: **verified** · Variants: PC

A composite or tree part's u32 hash is djb2 with XOR (`h = 5381; h = (h * 33) ^ byte`, mod 2^32) over the bytes of
the part name's last `|`-separated segment, lower-cased (the whole name when it has no `|`).

Evidence: [data: 6,916 of 6,916 parts of the game copy's distinct PC composite (6,579) and tree (337) mesh sets (2026-11-19)] [test: rust/bc2-formats/tests/spec_corpus.rs::meshes] [data: DICE's bundles of the game copy (dated before 2013: 1,021), round SPV1 2026-11-22: 10,894 of 10,894 parts of the distinct composite and tree mesh sets]

### MSET-PART-2: A tree's parts are its levels

Status: **observed** · Variants: PC, BF1943

A tree mesh set's parts are its levels (trunk, branches, fronds), named `lod<N>|<level name>`; the billboard
shader names the card drawn far away.

Evidence: [data: BFBC2's 92 PC tree mesh sets and BF1943's 13 (doc)] [doc: bfbc2-console-convert/FORMAT.md:221-223]

### MSET-WRITE-1: The PC writer

Status: **verified** · Variants: PC

A mesh set is written in little-endian in MSET-HDR-1 / MSET-LOD-1 order, the flag byte for every kind but skinned.
BFBC2's PC tree sets and the Garand / Type 5 PC sets read and write back byte-identical; every console rigid,
composite and skinned set writes back at the same length.

Evidence: [code: rust/bc2-formats/src/meshdata.rs:366-420] [test: rust/bc2-formats/src/meshdata.rs::mesh_sets_round_trip] [test: bfbc2-console-convert/tests/test_trees.py::game] [doc: bfbc2-console-convert/FORMAT.md:219-223]

## Open questions

- MeshData `h1` (`00 00` in 2,131 PC files, `01 00` in 124) and part bytes `fa`: meaning unknown.
- The four part lists (MESH-LIST-1): which renderer pass or feature each serves.
- Mesh set `a` (42), `z`, LOD `x`, `extra`, the flag byte and the tree's trailing u32: constant or near constant,
  meaning unknown.
- The 14 skinned LODs whose size is not *V* + *I* + 12 (MSET-LOD-2): what the other value counts.
- The 133 parts whose vertex offset is not a multiple of their stride: shared windows in another layout, or a
  different addressing rule.
- Skinned bone-name hashes: the hash function (not yet tested against djb2 / MSET-PART-1).
- Xbox 360 MeshData were checked through conversion, not surveyed for these value ranges here (the game copy holds
  PC files only).
