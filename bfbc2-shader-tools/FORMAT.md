# BC2 shader databases (Dx9_Single.res, Dx10_Single.res, Dx11_Single.res)

The format of Battlefield: Bad Company 2's per-level compiled shader databases (PC build 795745), as `shaderdb.py`
reads and writes it. Every level bundle has three, one per renderer: `Levels/<LV>/Shaders/Dx9_Single.res`,
`Dx10_Single.res`, `Dx11_Single.res` (FbrbTool types `Dx9ShaderDatabase`, `Dx10ShaderDatabase`,
`Dx11ShaderDatabase`). The level's `Levels/<LV>/shaders.dbx` (a `Render.ShaderDatabaseCollection`) names them, one
group per database with its render path (`ShaderRenderPath_Dx9` / `_Dx10` / `_Dx11`).

How it was found: the loaders in `BFBC2Game.exe` were read with short, targeted disassembly (exeref.py), then the
layout was checked on data. Every database in the game copy's level bundles (270: 81 of each kind, plus copies in
OPFOR's own bundles) parses to its last byte, writes back to the same bytes, and passes every check in section 4
(`shaderdb.py roundtrip --levels`). The Xbox 360 databases of BFBC2 and Battlefield 1943 are the same layout
big-endian, with a few differences (section 8).

| | Dx9 | Dx10 | Dx11 |
|---|---|---|---|
| loader (vtable slot 4 / called by the factory) | `FUN_009d4250` | `FUN_009d3fd0` | `FUN_009d4110` |
| section readers 1-8 | 009cf190, 009cd690, 009cd730, 009d1660, 009d1960, 009cf230, 009d2db0, 009d3d70 | 009cd7d0, 009cd690, 009cd730, 009d3080, 009cd8a0, 009cda90, 009d2cb0, 009d3d70 | 009ce4a0, 009cd690, 009cd730, 009d3460, 009ce570, 009ce760, 009d2d30, 009d3d70 |

## 1. Primitives

Little-endian (Xbox 360 databases: big-endian, section 8). `u8`, `u16`, `u32`. `guid` = 16 bytes (read as u32, u16, u16, 8 bytes; stored as they are).
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

Inside a block (read from the data; true for every block of the PC and Xbox databases read, `shaderdb.py`
`block_textures`, `block_constants`, `block_engine_textures`):

```
0x00  u32 size                 the whole block, a multiple of 16 (zero padding at the end)
0x04  u32 0x20, u32 0x20       (always)
0x0c  u32 textures end         = constants start; textures start at 0x20
0x10  u32 constants end        = engine textures start
0x14  u32 engine textures end  = sampler states start
0x18  u8, u8, u8 0             (not interpreted)
0x1b  u8 textures              count of 0x8c-byte texture records
0x1c  u8 constants             count of 0x38-byte constant records
0x1d  u8 engine textures       count of 0x28-byte records
0x1e  u8 sampler states        count of 8-byte records
0x1f  u8 0
0x20  textures:  u8 sampler register (s<N>), u8 cube (1: a cube map - environment maps, sky; else 0), 2 zero bytes,
                 0x80-byte name (zero-padded), 8 zero bytes
      constants: 0x20-byte name, u32 (a hash of the name), u8 register (c<N>), u8 components used (1-4), 2 zero,
                 4 floats (the value)
      engine textures: 0x20-byte parameter name (heightmap, maskTileMap, DepthBuffer, BaseTexture1 ...), u32 (hash),
                 u16 sampler register, 2 zero - textures the engine binds itself; no texture name
      sampler states: 8 bytes each (sampler register first; not interpreted)
```

Programs (section 6) have two blocks: the first one holds no textures in any file seen; the textures an entry uses
are in its programs' second blocks. The shaders' own blocks (section 4, 5) hold sampler states only. **Nothing names
what a texture is for**: the record gives only the sampler register, and the shaders' constant tables call the
samplers `sampler_Texture`, `sampler_Texture2` ... The texture names follow DICE's suffixes (`_C`/`_Df` color, `_Nm`/
`_N` normal, `_Sp` specular, `_Gm` gloss mask, `_Occlusion` occlusion), which is what `texture_kind` goes by. The
programs of one entry (permutations) normally bind the same textures to the same samplers; some (BF1943's
`DetailsMetal_t02`) put the same textures on other samplers in different programs.

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
Xbox:    u32 k, k x 12 bytes, 16 bytes Xenon D3DVERTEXELEMENT9 (u16 stream, u16 offset, u32 type, u8 method,
                                       usage, usage index, pad; big-endian), then a u8 stride per stream (section 8)
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
- Parameter blocks: bytes 0x18 and 0x19 of the header, the u32 after a constant's or engine texture's name (a hash
  of it, not checked against any function) and the 8 bytes of a sampler state.

## 8. Xbox 360 databases (BFBC2's Xbox levels, Battlefield 1943)

BFBC2's Xbox levels have one database each, `Levels/<LV>/Shaders/Single.res`; Battlefield 1943 (Xbox only) the same
(`Levels/Wake_island_s/Shaders/Single.res`, 4,019,811 bytes). FbrbTool lists both as `Dx9ShaderDatabase`. Found by
reading BFBC2's Xbox and PC databases of the same levels side by side (no console code was disassembled). Every
BFBC2 Xbox level database (20) and BF1943 Wake's parse to their last byte, write back to the same bytes and pass
section 4's checks (`test_shaderdb.py --xbox`).

**The layout is the PC one (section 2) with these differences:**

- Numbers are **big-endian**: the version, every count and size, the shader / program indices (section 4, 5, 6),
  the entry groups' u16 program indices, and the parameter blocks' header offsets and float values. Bytes stay
  bytes (a texture record's sampler and cube flag, a constant's register and components, the layout records of
  section 7, whose element bytes read the same way on both); an engine texture's sampler is a u16 and big-endian.
- **GUIDs are stored with big-endian fields**: the first three fields (u32, u16, u16) byte-swapped against the PC
  file's bytes, the last 8 bytes as they are (`swap_guid`). So an entry's GUID is MD5(name) with its first 4, 2 and 2
  bytes reversed, and the same entry, fragment or layout has the swapped GUID of its PC twin. Programs and entry
  groups are **sorted by the GUID's PC byte order** (that is, by the swapped bytes), not by the stored bytes.
- Version: 120 for BFBC2's Xbox files (as on PC), **117 for BF1943**, whose **program keys are 0x5c bytes** (44
  permutation bytes instead of 40); every other record has the PC size.
- Shader code is Xenon microcode (vertex shaders start `10 2A 11 01`, pixel shaders `10 2A 11 00`, then a constant
  table and the compiler's .pdb path); there is no flag byte before it. The code is not comparable with PC code.
- A vertex shader's input description (section 4's tail) is u32 k, k x 12-byte Xenon vertex elements (ending with
  the D3DDECL_END element), then 16 bytes: one u8 stride per vertex stream.

**What matches a PC twin** (BFBC2 mp_002, mp_005, mp_008: Xbox `Single.res` against PC `Dx9_Single.res`):

| | mp_002 | mp_005 | mp_008 |
|---|---|---|---|
| entries Xbox / PC / in both | 1,659 / 1,688 / 1,656 | 1,670 / 1,714 / 1,669 | 1,763 / 1,794 / 1,761 |
| entries in both naming the same textures on the same samplers | 1,622 | 1,655 | 1,760 |
| Xbox `Standard` mesh parts in PC with the same layout elements (usage, stream) | 3,086 of 3,097 | 3,011 of 3,016 | 3,298 of 3,303 |

- Entry names are the same apart from a few (PC's `All_White` character shaders, Xbox's `LaserDesignatorScope`
  edge shader), and their GUIDs are the swapped PC ones.
- Mesh-part names: the Xbox file has the `Standard` (and `StreamInstancingHalf`) parts; PC adds a
  `StreamInstancingFloat` part for every part. A layout lists the same elements (usage, stream) but in console
  formats where the PC has wider ones (packed normals `0x2B` for PC half4 `0x08` ...), and then its GUID differs:
  16 of mp_005's 64 Xbox layouts have the (swapped) GUID of a PC layout, 12 of them with the same bytes and 4
  differing only in byte 0x45 of the body (the vertex stride: 0x0c on Xbox, 0x10 on PC). The Xbox
  `StreamInstancingHalf` layouts have one element more (stream 2). `bfbc2-console-convert`'s `console_layouts`
  finds the same layout for every part name it finds (BF1943 Wake: all 1,341).
- Textures: the differences are terrain patch shaders (the Xbox ones bind one or two normal maps more, `_N`) and
  `Shaders/WaterOcean` (PC binds the foam texture on two samplers). Named constants are the same names and values
  (on other registers: the compiled shaders differ); the parameter blocks are not byte-identical.
- Programs, shaders and parameter blocks differ in number (mp_005: 6,440 programs and 2,134 parameter blocks on
  Xbox, 19,764 and 4,148 on PC): the console compiles fewer permutations.

**Battlefield 1943's Wake Island** (`Levels/Wake_island_s/Shaders/Single.res`): 544 entries (400 Objects, 63
Terrains, 42 Characters, 21 Shaders, 18 Systems), 3,539 programs, 735 vertex and 815 pixel shaders, 1,016 parameter
blocks, 55 vertex layouts, 1,354 mesh-part names, 13 fragments. 514 entries name textures, 428 textures in all,
e.g. `Objects/Architecture/Beach/Hangar_t01/Hangar_t01_Mesh/Objects/Architecture/Shaders_Beach/Metal/CorrugatedMetal_t01`:
s1 `Textures/Metal/CorrugatedMetal_t02_Nm`, s2 `Textures/Ambient_Occlusion/Hangar_t01_Mesh_Occlusion`, s3
`Textures/Metal/Metal_t12_Df`, s4 `Textures/Metal/CorrugatedMetal_t02_Df`, s5 `Textures/Decals/Trash_02_C`, s6
`Textures/Terrain/Forest/barr_C`, s7 `Textures/Masks/Rust_t01_Gm`, s8
`Textures/Specular_Generic/512/Generic_t01_Sp`.

Writing Xbox databases is possible in the code (`to_bytes` keeps the byte order) but is not a goal and was never
tried on a console; merging refuses to mix an Xbox database with a PC one.
