# Console resource converter (console-convert)

Converts Battlefield: Bad Company 2 resources from the Xbox 360 to the PC's formats - for content only the console
version has (e.g. the Onslaught Mark V patrol boat), taken from your own console files. Nothing is shipped: class
layouts and comparison files are read from your game copy.

```
python convert.py havok <console.res> <out.res> [--exe <BFBC2Game.exe>]
python convert.py verify-havok <pc folder> <console folder>... [--exe <BFBC2Game.exe>] [--report <file>]
python convert.py texture <console.res> <out.res>
python convert.py verify-texture <pc folder> <console folder>... [--report <file>]
```

| Resource | Status | Checked by |
|---|---|---|
| HavokPhysicsData (physics) | done | all 400 shared console / PC files equivalent (FORMAT.md 1.3) |
| textures (XenonTexture -> ITexture) | DXT1 / DXT3 / DXT5 / BC5, 2D and cube maps | 251 shared console / PC textures: byte-identical or the same picture (FORMAT.md 2.3) |
| meshes (MeshData) and mesh sets (rigid, composite, skinned) | done (`mesh.py`, `meshset.py`; used by OPFOR's `console_markv.py`) | Mark V converted and loading on PC; every console MeshData and mesh set of the Onslaught builds parses (FORMAT.md 3-4) |
| a mesh part as arrays (`mesh.decode`), console layouts (`mesh.console_layouts`) | done (used by OPFOR's WW2 `attachments.py` for Battlefield 1943's rifle parts) | 103 BFBC2 mesh parts decode alike from the Xbox and PC copies; every BF1943 rifle part decodes in range |

`mesh.decode(meshdata, layout, part, big_endian)` gives one part's positions, normals, tangents, binormals, UVs,
weights and bones (the part's palette applied) and its triangles, for its own vertex window - from a console mesh
with the source game's layout, or a PC one with BFBC2's. `mesh.console_layouts(shader_db)` reads a console shader
database's part names and layout records (BFBC2's Xbox levels, Battlefield 1943's): names are stored as name, NUL,
layout guid, and walked in runs so a printable guid byte is not read as a name's first letter.

Folders are unpacked bundles (FbrbTool). Format notes and the checks: [FORMAT.md](FORMAT.md). Tests:
`tests\test_havok.py`, `tests\test_texture.py`, `tests\test_mesh_decode.py` (game tier: your game copy and console
files, and Battlefield 1943 in the Rising Sun workspace; skipped without them).
The texture check needs `numpy`.

Developed for Project OPFOR BFBC2. Havok is a trademark of Microsoft; Xbox is a trademark of Microsoft;
Battlefield: Bad Company 2 is © Electronic Arts / DICE.
