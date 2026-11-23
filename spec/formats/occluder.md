# Occluder meshes (`OccluderMesh`)

A building or large object that hides what is behind it ships a low-polygon stand-in, `<asset>_occluderMesh.res`
(bundle type `OccluderMesh`), beside its mesh set: the renderer's software occlusion test draws these into a depth
buffer and skips the draws they hide (the executable's settings `OccluderMeshEnable`, `OccluderAlphaMeshEnable`,
`OccluderJobEnable`, `OccluderMeshJobCount`, `DrawProcessedOccluders`, `DrawOccluderMeshesWireframe`). The suite copies
the resource; this file describes its layout as round SPG4 decoded it from DICE's files (`spec/probes/resources.py`).
All values little-endian.

## 1. Layout

### OCC-HDR-1: A box, a length and one part header

Status: **verified** · Variants: PC

| Offset | Type | Content |
|---|---|---|
| 0 | 3 x f32 | the box's minimum (x, y, z) |
| 12 | 3 x f32 | the box's maximum |
| 24 | u32 | the length of the rest of the file (file size - 28) |
| 28 | u16 | vertex count v |
| 30 | u16 | index count i (a multiple of 3: a triangle list) |
| 32 | u32 | the vertices' offset, from byte 28 |
| 36 | u32 | the indices' offset, from byte 28 |
| 40 | u32 | the triangle block's offset, from byte 28 (OCC-TRI-1); 0 when there is none |
| 28 + vertex offset | v x 16 bytes | vertices (OCC-VTX-1) |
| 28 + index offset | i x u16 | indices; the file ends after the last |

The vertices end where the indices begin, every vertex lies inside the box (to 1e-3), and every index is below v.
One DICE occluder is empty (no vertices or indices).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py on DICE's bundles, 2026-11-23: 361 distinct OccluderMesh entries (636 live); 361 of 361 fit the layout to the last byte, the length word = size - 28 in 361, vertices inside the box in 361, indices below v in 360 (1 empty)]
[exe: BFBC2Game.exe names the resource suffix `_occluderMesh` and the settings `OccluderMeshEnable`, `OccluderAlphaMeshEnable`, `OccluderJobEnable`, `OccluderSpuJobEnable`, `OccluderMeshJobCount`, `HighPriorityOccluderMesh`, `DrawProcessedOccluders`, `DrawOccluderMeshesWireframe`; typeinfo record `OccluderMesh` (module Render, 64 bytes) at 0x1BF6050]

### OCC-VTX-1: A vertex is four floats; the fourth is one value per mesh

Status: **verified** · Variants: PC

A vertex is x, y, z and a fourth f32 that is the same for every vertex of a mesh: 1.0 in each of the 321 meshes with a
triangle block, and a multiple of 12.75 (255 / 20: 76.5, 102, 127.5, 140.25, 153, 255 ...) in each of the 39 without
one. What the fourth word means is not decoded (inferred: a w of 1 for the destructible form, a per-mesh priority or
depth bias for the static form).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: one fourth-word value per mesh in 360 of 360 non-empty meshes: 1.0 with a triangle block 321, k x 12.75 without one 39]

### OCC-TRI-1: The triangle block: 24-byte records, at least one per triangle

Status: **observed** · Variants: PC

When the part header's third offset is not 0, a block of 24-byte records sits between the part header and the vertices
(it starts at 28 + 16). 253 of the 321 blocks hold exactly one record per triangle; the other 68 hold up to 14 % more.
Most records are zero; the non-zero ones look like bit sets (single low bits, then sparse higher words), so the block
is read as per-triangle membership of the asset's destruction parts (which parts' loss removes the triangle). That
reading is inferred, not decoded: the suite copies the block.

[data: spec/probes/resources.py, 2026-11-23: block present 321 (+1 present and empty), absent 39; records = triangles in 253, more in 68]
[inferred: the meshes with a block are the destructible buildings' (their records' set bits differ triangle by triangle); those without are static (bridges, silos), whose fourth vertex word differs instead]

## Open questions

- The triangle block's records: which bits name which part, and why some blocks have more records than triangles.
- The fourth vertex word of the static occluders.
- Where the engine reads the resource (the typeinfo's 64-byte `OccluderMesh` object; its factory is not traced).
