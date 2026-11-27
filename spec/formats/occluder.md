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
one. The occlusion job transforms the positions with w = 1.0 and copies the mesh's (first vertex's) fourth word into the
w lane of every triangle it emits (0x9765F4, 0x97686C - 0x976896); what the triangle list's consumer does with it is
not traced (inferred: a per-mesh priority or depth bias for the static form).

[test: core/tests/test_spec_probes.py::resources]
[data: spec/probes/resources.py, 2026-11-23: one fourth-word value per mesh in 360 of 360 non-empty meshes: 1.0 with a triangle block 321, k x 12.75 without one 39]

### OCC-TRI-1: The triangle block: one 192-bit part mask per triangle, padded to an even count

Status: **verified** · Variants: PC

When the part header's third offset is not 0 (it is then 16: the block starts at 28 + 16), a record of six u32 - a
192-bit mask, bit p for the asset's part p - follows for each triangle, in index order. The count is rounded up to an
even number with a zero record, so that the vertices (at 28 + 16 + 24 x records) stay 16-byte aligned for the SSE
transform: this is SPG4's "more records than triangles" (the 68 meshes with an odd triangle count). The game keeps a
192-bit mask of hidden parts per occluder instance, cleared when the instance is made; hiding a mesh part sets its
bit there and showing it clears it, and the occlusion job drops every triangle whose mask shares a bit with the
instance's. A zero record is a triangle no part removes (17,186 of DICE's 22,998); without the block every triangle
is always drawn. The bit index is the part index the mesh's show / hide passes to the mesh, the soot (SOOT-SLOT-1)
and the occluder alike.

The loader (factory vtable 0x141B640: type 0x971C60, load 0x976900) makes a 64-byte object (vtable 0x1441F24), reads
the box into +0x10 / +0x20 and the length word into +0x30, allocates a 16-byte aligned buffer of that length (+0x34),
reads the part header into it and the rest of the file in one block at buffer + (the triangle offset, or the vertex
offset when there is none) (0x976A28 - 0x976A6B).

[test: core/tests/test_spec_probes.py::resources]
[test: core/tests/test_spec_probes.py::run_time]
[data: spec/probes/resources.py on DICE's bundles, 2026-11-27: the first payload at 16 in 361 of 361; 321 blocks, records = triangles rounded up to even in 321 of 321, the padding record zero in 321 of 321 (records = triangles in 253, one more in 68); 82 blocks with set bits, 5,812 of 22,998 triangles, highest bit 106]
[exe: BFBC2Game.exe loader 0x976900 (the reads 0x976A28 - 0x976A6B); instance set-up 0x96B170 (data +0x7C, the masks +0x60 - +0x74 cleared); job 0x9848A0 -> 0x9765C0: the triangle's six words tested against the instance's masks 0x976805 - 0x976846 (`test [esi+0x60], edx` at 0x976808), a hit skips the triangle, else three float4 go to the list at ctx +0x230; the part bit setter 0x971D80 (item x 0x80 + 0x60 + (part >> 6) x 8, flag 0 sets, else clears), called from the mesh's part show / hide 0xB99380 (0xB993EA) and 0x8E258B]

## Open questions

- The fourth vertex word of the static occluders (k x 12.75): the occlusion job copies the mesh's first vertex's
  fourth word into the w lane of every triangle it emits (0x9765F4, 0x97686C - 0x976896); what the triangle list's
  consumer does with it is not traced.
- Which named part a mask bit is: the asset's part list (the mesh set's parts) in the order the show / hide uses.
