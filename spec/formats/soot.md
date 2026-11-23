# Soot meshes (`SootMesh`)

A destructible building's mesh LOD that can be blackened by explosions carries `<asset>_Mesh_lod<N>_Soot.res` (bundle
type `SootMesh`): a copy of the LOD's surface the soot shaders draw over (`SootPerPixel`, `SootPerVertex`, DSTR-MESH-4;
the executable's `GameSootSettings`: `SootForBulletsEnable`, `SootForExplosionsEnable`, `SootVoxelSize`,
`SootSplitableTriangleEdgeLength`). The suite converts the Xbox 360 form (TERR-SOOT-1); this file is the PC layout as
round SPG4 read it from DICE's files (`spec/probes/resources.py`). All values little-endian.

## 1. Layout

### SOOT-HDR-1: Header, boxes, buffer sizes, buffers, a 256-byte tail

Status: **verified** · Variants: PC, Xbox 360

| Part | Type | Content |
|---|---|---|
| 0 | u32 | 18 in every DICE file (a version) |
| 4 | u32 | the vertex stride: 12 or 16 (SOOT-VTX-1) |
| 8, 12 | 2 x u32 | two flags, 0 or 1 (not decoded) |
| 16 | u32 | n: the number of boxes (2 - 27) |
| 20 | n x 6 x f32 | the boxes: minimum then maximum (x, y, z) |
| 20 + 24n | 4 x u32 | 1, the vertex bytes, the index bytes, 0 |
| then | vertex bytes | the vertices |
| then | index bytes | u16 triangle-list indices, every one below the vertex count |
| last 256 | 64 x u32 | the tail (SOOT-TAIL-1) |

The file ends exactly after the tail; a 16-byte file is the empty form (TERR-SOOT-1). This is the grammar the suite's
console converter walks (`soot_mesh`), confirmed on every PC file.

[test: core/tests/test_spec_probes.py::resources]
[code: rust/bc2-convert/src/terrain.rs:647-704]
[data: spec/probes/resources.py on DICE's bundles, 2026-11-23: 40 distinct SootMesh entries (90 live), 40 of 40 fit to the last byte; first word 18 in all; boxes 2, 4, 8, 10 or 27; indices below the vertex count in 40]

### SOOT-VTX-1: Two vertex forms: half positions or float positions

Status: **verified** · Variants: PC, Xbox 360, BF1943

A 12-byte vertex is four f16 (x, y, z and 1.0) and a u32 (a small number: inferred, the box or part the vertex belongs
to); a 16-byte vertex is three f32 and the same u32. DICE's PC files use 12 bytes in 34 and 16 in 6 (old bridge and
industrial props). The console converter accepts only the 12-byte form ("every BFBC2 soot mesh has it"): true of the
Xbox 360 files it converts, not of the PC set, so a 16-byte console file (BF1943's warehouse) is refused rather than
converted.

[test: core/tests/test_spec_probes.py::resources]
[code: rust/bc2-convert/src/terrain.rs:678-683]
[data: spec/probes/resources.py, 2026-11-23: stride 12 x 34, 16 x 6]

### SOOT-TAIL-1: The tail: the box count, the triangle count, 60 words

Status: **verified** · Variants: PC

The 64-word tail begins 0, n (the box count), the triangle count (index bytes / 6) and 60, then 60 words: small
numbers followed by 0xFFFFFFFF padding (inferred: the boxes' part indices, padded to 60).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: tail = (0, n, triangles, 60) in 40 of 40]

## Open questions

- The two flag words at 8 and 12, the boxes' meaning (destruction parts or soot volumes), and the tail's 60 words.
- How the engine fills the soot (the voxel size and the per-vertex / per-pixel choice of `GameSootSettings`).
