# Soot meshes (`SootMesh`)

A destructible building's mesh LOD that can be blackened by explosions carries `<asset>_Mesh_lod<N>_Soot.res` (bundle
type `SootMesh`): a copy of the LOD's surface the soot shaders draw over (`SootPerPixel`, `SootPerVertex`, DSTR-MESH-4;
the executable's `GameSootSettings`: `SootForBulletsEnable`, `SootForExplosionsEnable`, `SootVoxelSize`,
`SootSplitableTriangleEdgeLength`). The suite converts the Xbox 360 form (TERR-SOOT-1); this file is the PC layout as
round SPG4 read it from DICE's files (`spec/probes/resources.py`) and round SPG4-rest decoded it from the loader. All
values little-endian.

## 1. Layout

### SOOT-HDR-1: Header, boxes, buffer sizes, buffers, a 256-byte tail

Status: **verified** · Variants: PC, Xbox 360

| Part | Type | Content |
|---|---|---|
| 0 | u32 | 18 in every DICE file (a version; the loader reads and drops it) |
| 4 | u32 | the vertex stride: 12 or 16 (SOOT-VTX-1) |
| 8, 12 | 2 x u32 | per-pixel soot and half positions, 0 or 1 (SOOT-FLAG-1) |
| 16 | u32 | n: the number of boxes (2 - 27) |
| 20 | n x 6 x f32 | the boxes: minimum then maximum (x, y, z) |
| 20 + 24n | 4 x u32 | the sub-mesh count 1, then the sub-mesh's vertex bytes, index bytes and an index-buffer word 0 (SOOT-SLOT-1) |
| then | vertex bytes | the vertices |
| then | index bytes | u16 triangle-list indices, every one below the vertex count |
| last 256 | 64 x u32 | the sub-mesh's trailer (SOOT-TAIL-1, SOOT-SLOT-1) |

The file ends exactly after the tail; a 16-byte file is the empty form (TERR-SOOT-1). This is the grammar the suite's
console converter walks (`soot_mesh`), confirmed on every PC file.

[test: core/tests/test_spec_probes.py::resources]
[code: rust/bc2-convert/src/terrain.rs:647-704]
[data: spec/probes/resources.py on DICE's bundles, 2026-11-23: 40 distinct SootMesh entries (90 live), 40 of 40 fit to the last byte; first word 18 in all; boxes 2, 4, 8, 10 or 27; indices below the vertex count in 40]

### SOOT-VTX-1: Two vertex forms: half positions or float positions

Status: **verified** · Variants: PC, Xbox 360, BF1943

A 12-byte vertex is four f16 (x, y, z and 1.0) and a u32; a 16-byte vertex is three f32 and the same u32. The u32 is
two u16, the vertex's slot and its box (SOOT-SLOT-1). DICE's PC files use 12 bytes in 34 and 16 in 6 (old bridge and
industrial props). The console converter accepts only the 12-byte form ("every BFBC2 soot mesh has it"): true of the
Xbox 360 files it converts, not of the PC set, so a 16-byte console file (BF1943's warehouse) is refused rather than
converted.

[test: core/tests/test_spec_probes.py::resources]
[code: rust/bc2-convert/src/terrain.rs:678-683]
[data: spec/probes/resources.py, 2026-11-23: stride 12 x 34, 16 x 6]

### SOOT-TAIL-1: The tail: the box count, the triangle count, 60 words

Status: **verified** · Variants: PC

The 64-word tail begins 0, n (the box count), the triangle count (index bytes / 6) and 60, then 60 words: the part
indices of the mesh's slots followed by 0xFFFFFFFF padding (SOOT-SLOT-1). It is the trailer of the file's one
sub-mesh.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: tail = (0, n, triangles, 60) in 40 of 40]

### SOOT-FLAG-1: The two flag words pick the soot vertex declaration

Status: **verified** · Variants: PC

Word 8 is per-pixel soot (1) or per-vertex (0), word 12 half-float positions (1: the 12-byte vertex) or float ones
(0: the 16-byte vertex). The loader looks the vertex declaration up by name: `SootPerVertex` (0, 0),
`SootPerVertex_Half` (0, 1), `SootPerPixel` (1, 0), `SootPerPixel_Half` (1, 1); a stride of 0 ends the load (the empty
16-byte form). DICE's files: per-pixel half 22, per-vertex half 12, per-pixel float 6; word 12 is 1 exactly when the
stride is 12. A converter writing the 16-byte form must clear word 12, or the declaration will not match the stride.

[test: core/tests/test_spec_probes.py::resources]
[test: core/tests/test_spec_probes.py::run_time]
[data: spec/probes/resources.py on DICE's bundles, 2026-11-27: (word 8, word 12, stride) = (1, 1, 12) x 22, (0, 1, 12) x 12, (1, 0, 16) x 6]
[exe: BFBC2Game.exe factory vtable 0x147E990 (type 0xC409E0, load 0xC48670: a 0x80-byte object, constructor 0xC45850, vtable 0x147E9CC, its loader slot +0xC 0xC4EDD0); the stride into +0x40, the flags +0x44 / +0x48, the name chosen 0xC4EE44 - 0xC4EE9A (strings 0x147EAE8, 0x147EAD4, 0x147EAC4, 0x147EAB0), the declaration into +0x4C]

### SOOT-SLOT-1: Each vertex names a box and a part slot; the trailer maps slots to parts

Status: **verified** · Variants: PC

The header's boxes are n records (minimum, maximum; the loader keeps them as 32-byte Vec3 pairs at +0x64, n at
+0x60). After the boxes come the sub-mesh count (1) and, per sub-mesh, the vertex bytes, the index bytes and a word
the index buffer's description takes (0 in DICE's files); after its buffers each sub-mesh has two words (0 and n:
inferred its first box and box count), its triangle count (the draw's primitive count) and a count (60) of slot
words: the asset's part indices, then 0xFFFFFFFF. A vertex's last u32 is two u16: its slot (low) and its box (high),
and the vertex lies inside its box. At draw time the game fills 60 constants, one per slot: 1.0 for a padding slot or
for a part set both in the instance's part mask (the mesh's shown parts, copied on every show / hide, as the
occluder's mask of OCC-TRI-1) and in a second mask (+0x9C, not traced), else 0.0. So the soot drawn on a destroyed
part goes with it.

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-27: in 40 of 40: part indices first then padding, distinct, below 192; every vertex's slot below the used count and every used slot referenced; every vertex's box below n, every box referenced, every vertex inside its box]
[exe: BFBC2Game.exe loader 0xC4EDD0: boxes into +0x60 / +0x64 (0xC4EF00 - 0xC4EF36), sub-meshes +0x50 (0x24 bytes each), the buffers through the manager's slots +0x58 / +0x5C, the two words +0x1C / +0x20, the primitive count 0xC4F1F6, the slot words into +0xC; the draw 0xC4D44C (60 constants) -> 0xC431C0 (masks +0x84 and +0x9C); the mask copy 0xC3CF40 from the mesh's part show / hide 0xB99380]

## Open questions

- The second instance mask (+0x9C) and the sub-mesh's two words (0 and n); how the boxes are used at draw time
  (0xC4D4A8) and the constants' layout (12 floats a slot, 0xC40A80).
- How the engine fills the soot (the voxel size and the per-vertex / per-pixel choice of `GameSootSettings`).
