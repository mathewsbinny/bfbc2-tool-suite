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
| meshes (MeshData, mesh sets) | planned | |

Folders are unpacked bundles (FbrbTool). Format notes and the checks: [FORMAT.md](FORMAT.md). Tests:
`tests\test_havok.py`, `tests\test_texture.py` (game tier: your game copy and console files; skipped without them).
The texture check needs `numpy`.

Developed for Project OPFOR BFBC2. Havok is a trademark of Microsoft; Xbox is a trademark of Microsoft;
Battlefield: Bad Company 2 is © Electronic Arts / DICE.
