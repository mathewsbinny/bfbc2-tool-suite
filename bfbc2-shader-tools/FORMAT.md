# BC2 shader databases (Dx9_Single.res, Dx10_Single.res, Dx11_Single.res)

The format of Battlefield: Bad Company 2's per-level compiled shader databases (PC build 795745), as `shaderdb.py`
reads and writes it. Every level bundle has three, one per renderer: `Levels/<LV>/Shaders/Dx9_Single.res`,
`Dx10_Single.res`, `Dx11_Single.res` (FbrbTool types `Dx9ShaderDatabase`, `Dx10ShaderDatabase`,
`Dx11ShaderDatabase`). The level's `Levels/<LV>/shaders.dbx` (a `Render.ShaderDatabaseCollection`) names them, one
group per database with its render path (`ShaderRenderPath_Dx9` / `_Dx10` / `_Dx11`).

How it was found: the loaders in `BFBC2Game.exe` were read with short, targeted disassembly (exeref.py), then the
layout was checked on data. Every database in the game copy's level bundles (270: 81 of each kind, plus copies in
OPFOR's own bundles) parses to its last byte, writes back to the same bytes, and passes every check in section 4
(`shaderdb.py roundtrip --levels`).

| | Dx9 | Dx10 | Dx11 |
|---|---|---|---|
| loader (vtable slot 4 / called by the factory) | `FUN_009d4250` | `FUN_009d3fd0` | `FUN_009d4110` |
| section readers 1-8 | 009cf190, 009cd690, 009cd730, 009d1660, 009d1960, 009cf230, 009d2db0, 009d3d70 | 009cd7d0, 009cd690, 009cd730, 009d3080, 009cd8a0, 009cda90, 009d2cb0, 009d3d70 | 009ce4a0, 009cd690, 009cd730, 009d3460, 009ce570, 009ce760, 009d2d30, 009d3d70 |

## 1. Primitives

Little-endian. `u8`, `u16`, `u32`. `guid` = 16 bytes (read as u32, u16, u16, 8 bytes; stored as they are).
`cstr` = bytes up to a 0 byte (read one byte at a time). Records follow each other with no padding and no offsets:
the file is a stream, read front to back.

## 2. Layout

```
u32  version            120 in every file (read and not used)
u8   flag               0 in every file (kept at object+0xEC)
section 1  parameter blocks
section 2  binding tables
section 3  register tables
section 4  vertex shaders
section 5  pixel shaders
section 6  programs, then program keys
section 7  vertex layouts
section 8  names: entries, fragments, mesh parts; entry groups
```

### Section 1: parameter blocks

`u32 n`, then n x { `u32 size` (counting itself), `size - 4` bytes }. The loader keeps each block as one piece of
memory (size in its first word) and then prepares it in place (`FUN_009bc7d0` for Dx9, `FUN_009bb4b0` for Dx10).
The offset-like fields inside (0x20 ...) count from the block's start, and the blocks used by one entry are
byte-identical in different levels' files, so a block can be copied unchanged. Blocks hold the shader's
parameters, among them texture names in fixed 0x80-byte fields (`Characters/US/US_Textures/Holster_Beretta_Nm`).
Referenced by index from shaders (section 4, 5) and programs (section 6).

### Section 2: binding tables

`u32 n`, then n x { `u16 k`, `4k + 2` bytes }. Kept whole. Referenced by index from shaders.

### Section 3: register tables

`u32 n`, then n x { `u32 k`, `4k` bytes }. Kept whole (3 in every file seen). Referenced by index from shaders.

### Section 4: vertex shaders

`u32 n`, then n x:

```
guid   id
u32    size
size   code         Dx9: 1 flag byte (bit 0: only created when a device capability is present) + D3D9 bytecode;
                    Dx10/11: DXBC bytecode (starts "DXBC")
u32    parameter block (section 1 index)
u32    binding table (section 2 index)
u32    register table (section 3 index)
Dx9:     u32 k, k x 8 bytes            D3DVERTEXELEMENT9 (CreateVertexDeclaration)
Dx10/11: u32 n, n bytes                (kept with the input layout; probably the input signature)
         u32 m, m x 0x1c bytes         input element descriptions (D3D10/11_INPUT_ELEMENT_DESC, name pointer field unused)
         u32 k, k x cstr               their semantic names
```

### Section 5: pixel shaders

`u32 n`, then n x { `guid`, `u32 size`, `code` (as above), `u32` parameter block, `u32` binding table, `u32`
register table }.

### Section 6: programs and their keys

`u32 n`, then n x 0x30-byte program records:

| offset | |
|---|---|
| 0x00 | guid: program id. Programs are sorted by it (byte order) in every file; the loader inserts them into a sorted vector map (`FUN_009cc520`), so an id must not repeat (a repeat would be dropped) |
| 0x10, 0x14 | u32, u32 (not interpreted) |
| 0x18 | u32: overwritten while loading with a checksum of the program's key (the stored value does not matter) |
| 0x1c | u32 (0) |
| 0x20 | u32 vertex shader (section 4 index) |
| 0x24 | u32 pixel shader (section 5 index) |
| 0x28, 0x2c | u32, u32 parameter blocks (section 1 indices) |

Then `u32 n` again (the same count) and n x 0x58-byte program keys, key i belonging to program i:

| offset | |
|---|---|
| 0x00 | guid of the entry the program belongs to (section 8.1) |
| 0x10 | guid of a shader fragment (section 8.2), or zero |
| 0x20 | guid of a vertex layout (section 7) |
| 0x30 | 40 bytes: the permutation (flags, counts) |

The keys are read into a temporary buffer, checksummed into the programs' +0x18 and freed.

### Section 7: vertex layouts

`u32 n`, then n x { `guid`, 0x49 bytes } (a vertex stream description). Named by GUID only (program keys, mesh
parts).

### Section 8: names and entry groups

Three name lists with the same shape, `u32 n`, then n x { `cstr name`, `guid` } (`FUN_009d3830`, each loaded into
a map keyed by name):

1. **entries**: `<mesh asset Name>/<shader asset Name>`, e.g.
   `Characters/RUS/RUS_Assault/RUS_Assault_Head_Mesh/Characters/RUS/Shaders/All_White`, and a few without a mesh
   (`Shaders/Decals/...`, terrain patch shaders). The names keep the case of the assets' Name fields, which is not
   consistent (`Objects/Objects/MP_FlagPole_Mobile_01/MP_FlagPole_Mobile_01_Mesh/objects/objects/mp_flagpole_mobile_01/shaders/...`).
   **The guid is MD5 of the name's bytes** (true for every entry of every file), so it can be computed.
2. **fragments**: shader fragments (`Systems/Emitter/ShaderFragment`, `Systems/Overgrowth/LODFade` ...) -> the
   guid program keys name at +0x10. 14 in the files seen.
3. **mesh parts**: `<mesh>_lod<N>/<part>/<technique>` -> a vertex layout guid (section 7), e.g.
   `.../RUS_Assault_Head_Mesh_lod0/export_head|lod0|head_geo|head_geoShape_Face/Standard`; techniques `Standard`,
   `StreamInstancingFloat` ...; a few names stand alone (`Decal`, `Emitter`, `OvergrowthBillboard`). The same
   part name has the same layout guid in every level.

Then the **entry groups** (`FUN_009d3a30`, a map keyed by guid): `u32 n`, then n x { `guid` (an entry's), `u32 k`,
k x `u16` program index }. Sorted by guid, each list ascending; one group per entry; every program in exactly one
group, the one whose guid its key names.

## 3. How a name reaches its shaders

```
entry name --(8.1)--> entry guid (= MD5(name)) --(8.4)--> program indices --> program records (6)
                                                               |                 |-> vertex shader (4), pixel shader (5)
                                                               |                 |-> parameter blocks (1); shaders -> binding (2), register (3) tables
                                                               '-> program keys: entry guid, fragment guid (8.2), vertex layout guid (7)
mesh part name --(8.3)--> vertex layout guid (7)
```

Nothing in the file is an offset. Records point to each other by index (programs -> shaders and parameter blocks,
shaders -> tables, entry groups -> programs) or by guid (keys -> entries, fragments, layouts; mesh parts ->
layouts). How the renderer chooses among an entry's programs at draw time was not traced (no further disassembly);
it starts from the mesh's and shader's names, so an entry is found only when its exact name is in the file.

## 4. What must hold for a modified file (the checks)

`shaderdb.py info` and every merge check these; DICE's files all pass:

- each section is read to its end and the file ends after section 8 (no total size or checksum is stored);
- every index is inside its section (shaders -> 1, 2, 3; programs -> 4, 5, 1; entry groups -> 6), and entry groups
  index programs with **u16**: at most 65,535 programs (the largest file has 24,932);
- the key count equals the program count; program ids are unique; programs sorted by id (DICE's order; the loader
  would accept another order, but keep it);
- each entry guid is MD5 of its name, names unique;
- entry groups and entries match one to one, groups sorted by guid, each program in exactly one group, and that
  group is the entry its key names;
- every fragment guid (other than zero) and vertex layout guid named by a key or a mesh part is in section 8.2 / 7.

Nothing else is computed from the content: program ids, shader ids, layout ids and fragment ids are copied as they
are. (The program id is not MD5 of the key; how DICE made it is unknown, and not needed.)

## 5. Dx9, Dx10 and Dx11

The same layout for all three apart from section 4's end (D3DVERTEXELEMENT9 list for Dx9; byte block, input
elements and semantic names for Dx10/11) and the code (a flag byte before the D3D9 bytecode for Dx9; plain DXBC for
Dx10/11). The three files of a level have the same entries (names and guids) and the same numbers of fragments,
mesh parts and vertex layouts, but the fragment and layout guids differ between the kinds (112 of mp_008cq's 114
layout guids are shared by Dx9 and Dx11). Dx9 and Dx10 have the same number of programs, Dx11 fewer (22,157 /
22,157 / 14,007 for Nelson Bay). A database is merged only with databases of its own kind.

## 6. Merging (what shaderdb.py merge does)

The base file's records stay where they are, except the programs, which are re-sorted by id after the new ones are
added; the entry groups are renumbered to match. For each donor entry taken: its programs are added (one already in
the file with the same id and content is shared), with their shaders, parameter blocks and tables appended unless an
identical record is already there (same bytes; a shader counts as identical only with identical tables); its
fragment names, vertex layouts and the mesh-part names of its mesh are added when missing. The entry name and guid
are copied as they are.

Evidence that entries move between levels unchanged: for the entries Nelson Bay's Rush and Conquest databases share
(1,771), each draws with the same programs, shaders, parameter blocks and tables by content; programs with the same
id are identical; the same mesh-part name has the same layout. After every merge the tool checks that each entry of
the result draws with exactly what it drew with in its source file.

## 7. Not known

- The meaning of program +0x10 / +0x14, the binding and register tables, the 40 permutation bytes of a key and the
  0x49 bytes of a layout (all copied as they are).
- Whether the engine has limits other than the u16 program index (for example on the number of shaders), and what
  it does with an entry whose textures (named in its parameter blocks) are not in the level: neither was traced and
  no merged file has been loaded in the game.
