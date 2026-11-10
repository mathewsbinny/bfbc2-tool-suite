# Shader tools

`shaderdb.py` reads, checks, lists and merges Battlefield: Bad Company 2's per-level compiled shader databases:
`Levels/<LV>/Shaders/Dx9_Single.res`, `Dx10_Single.res` and `Dx11_Single.res` in each level bundle (FbrbTool types
`Dx9ShaderDatabase`, `Dx10ShaderDatabase`, `Dx11ShaderDatabase`). The format is in [FORMAT.md](FORMAT.md).

It also **reads Xbox 360 databases** - BFBC2's Xbox levels' `Levels/<LV>/Shaders/Single.res` and Battlefield 1943's
(`Levels/Wake_island_s/Shaders/Single.res`; both listed as `Dx9ShaderDatabase`): the same layout big-endian, with
Xenon shader code (FORMAT.md section 8). They are recognised from their first bytes; `info`, `list`, `textures` and
`roundtrip` work on them. They are read for what they name - entries, mesh-part names, vertex layouts and the textures
each entry uses - so that PC entries can be built for console meshes; an Xbox database is never merged into a PC one.

Why: a level's database only has entries for the meshes that level places, each keyed by
`<mesh path>/<shader path>`. A mesh with no entry draws invisibly. When a level is given objects its database was
not built for (an Onslaught map made from PC Rush data has capture flags, other vehicles, other kits), the entries
can be taken from other levels' databases: **merge** builds a database that is one level's plus the entries other
levels have for the meshes it lacks.

Python 3.10 or newer, nothing else. It reads your own game copy (straight from its bundles, or files you unpacked
with FbrbTool) and writes only the file you name; it never writes into the game folder.

## Commands

```
python shaderdb.py info  level:mp_008cq
python shaderdb.py list  "C:\...\Levels\ONS_MP_008\Shaders\Dx9_Single.res" flag_
python shaderdb.py cover "C:\...\Levels\ONS_MP_008\Shaders\Dx9_Single.res" "C:\...\levels_ons_mp_008_level-00"
python shaderdb.py needs "C:\...\levels_ons_mp_008_level-00" -o needs.txt
python shaderdb.py find  needs.txt --base level:ons_mp_008
python shaderdb.py merge level:ons_mp_008 level:mp_008cq level:mp_001 --need needs.txt -o "C:\...\work\shaderdb\ons_mp_008\Dx9_Single.res"
python shaderdb.py --dx 10 merge level:ons_mp_008 level:mp_008cq level:mp_001 --need needs.txt -o "...\Dx10_Single.res"
python shaderdb.py roundtrip --levels
python shaderdb.py textures level:mp_005 Humvee --constants
python shaderdb.py textures "C:\...\work\xbox\dist\xenon\levels\mp_005\level-00.fbrb" Humvee
python shaderdb.py textures "C:\...\Wake_island_s\Shaders\Single.res" Hangar_t01
```

| Command | Does |
|---|---|
| `info <db>` | section counts, the consistency checks (FORMAT.md section 4), and whether writing the file again gives the same bytes; exit code 1 if not |
| `list <db> [filter] [--parts]` | the entries as `mesh \| shader` with their program count and shader bytes; `--parts` the mesh-part names and their vertex layouts |
| `textures <db> [filter] [--constants]` | for each entry, the textures its parameter blocks name: sampler register (`s3`), texture name, and what it is for as its name's suffix suggests (`_C`/`_Df` color, `_Nm`/`_N` normal, `_Sp` specular, `_Gm` gloss, `_Occlusion` occlusion; cube maps marked); `--constants` adds the named shader constants and their values |
| `cover <db> <level folder> [--all]` | the meshes the level's `.dbx` files use that have no entry; references it cannot classify are listed apart |
| `needs <level folder> [-o file]` | those meshes, one per line: the list `merge --need` takes |
| `find <needs> [--base <db>]` | for each needed mesh (with `--base`, each the base lacks), the game copy's levels whose database has entries for it |
| `merge <base> <donor>... --need <needs> -o <out> [--all-names]` | the merged database and a report `<out>.merge.txt` |
| `roundtrip <db>... [--levels]` | parse and write again, byte for byte, with the checks; `--levels` does every database in the game copy |

A database (`<db>`, base, donor) is given as:

| You give | Read from |
|---|---|
| a path to a `.res` | that file (unpacked with `FbrbTool unpack`) |
| `level:mp_008cq` | the game copy's `levels\mp_008cq\level-00.fbrb` (Package over Dist), its own `Levels/MP_008CQ/Shaders/` database |
| `<bundle.fbrb>[:<entry>]` | any bundle; name the entry when it has several databases of the kind |

The kind (Dx9 / Dx10 / Dx11) comes from the file name, else `--dx` (default 9; donors take the base's kind).
Project OPFOR BFBC2's folder is found as the other tools find it (`--opfor`, `OPFOR_ROOT`, or a
`project-opfor-bfbc2` folder next to the suite).

### The needs list

`--need` takes a text file (one path per line, `#` comments) or an unpacked level folder. A line is a mesh path or
a folder: it matches every entry whose name starts with it as a whole path (`objects/vehicles/land/bmd3` covers
`BMD3_Mesh`, `BMD3_TOW_Mesh`, `BMD3Tracks_Mesh`). Case does not matter.

From a level folder, a mesh is a partition there whose primary instance is a mesh asset, or a partition named by a
field with "Mesh" in its name (`Mesh`, `Meshes1p`, `Meshes3p`, `WeaponMesh`, `MeshShaderSet` ...) - when it is
outside the folder, only if its name has "mesh" in it (others, such as edge models and shader sets, are listed by
`cover` as not checked). Meshes that come from shared bundles and that the level folder never names (kits,
vehicles spawned by shared blueprints) must be added to the list by hand. Mesh assets that only hold shader sets
(the weapons' `KitUpgrade_01B`, LodCount 0) show as missing in every level: their shaders are compiled under the
weapon meshes they are applied to.

### What merge does

- The base is kept entry for entry.
- For each needed name the base has **no** entry for, every donor entry of it is added - all its shader sets, since
  a level picks one (Nelson Bay draws the quad bike with `Quadbike_Main_Snow`; one level has the Bradley's
  `Main_Desert`, another its `Main_Snow`). An entry name more than one donor has is taken from the **first** in the
  order given (the same name had the same content in every pair of levels compared). Meshes the base has entries
  for are left alone; `--all-names` also adds their missing shader sets.
- An entry comes with everything it draws with: its programs, vertex and pixel shaders, parameter blocks, binding
  and register tables, fragment names, vertex layouts and its mesh's mesh-part names. A record already in the file
  with the same bytes is shared instead of copied.
- The result is read back and must write the same bytes, pass every check, and each entry must draw with exactly
  what it drew with in its source (by content). Otherwise nothing is written.
- The same inputs give the same bytes.

The report lists the entries added (and from which donor), programs shared, any conflicts (a donor program or
mesh-part name the file has with other content: the existing one is kept), and the needed names still without an
entry.

### Cloning an entry for a new mesh (from Python)

`Merger.clone_entry(donor, entry, new_name, textures)` adds a donor's entry again under a new name - a mesh the
database has never drawn, e.g. a mesh converted from the console (OPFOR's `console_markv.py`) or a new weapon
variant: its programs get new ids (MD5 of the new name and the old id), their keys name the new entry, and copies of
their parameter blocks carry the new texture names (`textures`: {old name: new name}). Texture names are replaced only
as whole names at the start of their field, within the field's 128 bytes; every name to replace must be one the
entry uses (exact case) - otherwise nothing is added and the error names it. `Merger.add_part(name, layout_guid,
donor)` names a mesh part's vertex layout (`<mesh>_lod<N>/<part>/<technique>`). A plain database file's kind comes
from its name (`Dx9`, `Dx10`, `Dx11`); a contradicting `--dx` or base kind is refused, not merged (Dx10 and Dx11
share a layout and would parse either way).

### Reading textures and layouts (from Python)

```python
import shaderdb as sd
db = sd.ShaderDb.parse(data)            # PC or Xbox 360, from the first bytes; db.big_endian, db.platform
sd.entry_textures(db)                   # {entry name: [(sampler, cube, texture name)]}
sd.block_textures(block, db.e)          # one parameter block's textures; sd.block_constants: its constants
sd.part_layouts(db)                     # {mesh-part name: vertex layout record body (0x49 bytes)}
sd.texture_kind(name)                   # 'color', 'normal', 'specular', 'gloss', 'occlusion' or '' (by suffix)
db.pc_guid(g)                           # a GUID of the file in PC byte order (Xbox files swap its first 3 fields)
```

The databases do not name what a texture is for: a texture record names the sampler register only, and the shaders'
constant tables call their samplers `sampler_Texture<N>`. The sampler numbers are the same in a level's Xbox and PC
databases (BFBC2's twins), so an entry with the same shader can be matched sampler for sampler; otherwise the
texture names' suffixes (DICE's naming) tell color, normal, specular and gloss maps apart.

### Putting a merged database into the game

Not done by the tool. Copy the level bundle out of the game folder, `FbrbTool unpack` it, replace the three
`Levels/<LV>/Shaders/Dx*_Single.res`, `FbrbTool pack` it and install it the way OPFOR installs bundles. Build all
three kinds from the same donors so every renderer has the same entries.

Things the tool cannot check (FORMAT.md section 7): engine limits other than the 65,535 programs an entry group can
index; and the textures an added entry's parameter blocks name must exist in the level (they normally come with the
mesh's own bundle). No merged database has been loaded in the game yet.

## Tests

`tests\test_shaderdb.py` (in `run_tests.py quick`): databases built by the test, Dx9 and Dx10/11 - round trip,
refused and broken files, merges (which donor wins, `--all-names`, shared records, renumbering, re-parse, content
check, determinism, the program limit) and the command line; built Xbox 360 databases (byte order recognised,
round trip, GUID order) and parameter blocks' textures and constants in both byte orders. `--game` adds the game
copy: mp_008cq's and ons_mp_008's databases round trip, and ONS_MP_008 merged with mp_008cq's capture flags in all
three kinds. `--xbox` (also run by `--game`; each part skipped when its files are missing) reads every BFBC2 Xbox
level database (`work\xbox\dist\xenon\levels`), compares the Xbox and PC Dx9 databases of mp_002, mp_005 and mp_008
(entries, mesh parts and layouts, each entry's textures) and reads Battlefield 1943's Wake Island database.

`tests\test_renderqa.py` (quick): a Dx11 database, mesh and texture built by the test (shaders compiled with
d3dcompiler_47) are drawn by shaderview. It checks the texel colour times the material constant, coverage, sample
counts, every preparation flag, a pixel shader that discards everything, the picture checks, the comparison, texture
header checks, sampler states, the light, the camera, constant placement and the command line. `--game` (game tier)
draws three stock meshes with their own level's database (no flag) and checks that drawing one twice gives the same
picture.

## Xbox 360 shaders on PC (xenos.py, xenos_hlsl.py, pcbuild.py)

For console content brought to PC (Battlefield 1943's), the Xbox shaders themselves can be used:

- `xenos.py` decodes an Xbox 360 shader from an Xbox database (`parse_shader(db.vs[i].code)`): its constant table,
  literal constants, interpolators and vertex elements, and the Xenos microcode (control flow, ALU, fetches);
  `disasm()` prints it. The container and microcode as implemented are in [XENOS.md](XENOS.md). Written from
  Xenia's documentation of the microcode (BSD-3) and checked against XenosRecomp's container field names (MIT);
  every shader of Battlefield 1943's Wake Island decodes.
- `xenos_hlsl.py` translates a decoded shader to HLSL for shader model 4 / 5 (BFBC2's Dx10 / Dx11 renderers):
  engine constants keep their names, material constants and literals become constants, the shadow map is read as
  depth, cube-map fetches keep their direction, instanced draws rebuild the world matrix from the instance rows.
  Not handled yet: the screen-position input (vPos), loop-relative addressing, Dx9.
- `pcbuild.py` compiles with the system's `d3dcompiler_47.dll` and builds what a PC database stores beside the code:
  the binding table (FORMAT.md section 2), the Dx10/11 input layout, parameter blocks.

Project OPFOR's `wake_translate.py` puts them together for Wake Island.

## Render harness (renderqa.py)

Checks that shaders and textures draw the right picture, without starting the game. A mesh's LOD 0 is drawn off
screen with Direct3D 11 (`native\shaderview.exe`), using the database's own compiled programs, the mesh's own vertex
and index data and the textures as the game stores them. Each mesh is drawn from three views (three-quarter, front,
top) and the pictures are checked.

```
python renderqa.py render  <mesh>... --db <db> [--stage <folder>]... [-o <folder>]
python renderqa.py compare <mesh>... --db <db> [--truth <db> | --truth auto] [--stage <folder>]... [-o <folder>]
python renderqa.py validate <mesh>... [-o <folder>]
python renderqa.py wake [--only <text>] [--limit N] [-o <folder>]
```

| Command | Does |
|---|---|
| `render` | draws each mesh with `--db`, flags what its pictures show, and writes a contact sheet with the worst first |
| `compare` | draws each mesh twice: DICE's entry (`--truth`, or with `auto` the first stock level whose Dx11 database draws the mesh, read from BFBC2's own files) and the `--db` entry (staged files first). It measures coverage overlap (IoU), mean difference, PSNR and luminance ratio, and draws a diff picture |
| `validate` | draws stock meshes with their own level's database beside the mesh viewer's texture-only render |
| `wake` | Project OPFOR: every mesh `wake_audit.drawn_meshes()` lists, drawn with the installed Wake Dx11 database (`work\ww2\install`). Meshes a stock level also draws are compared with DICE's entry; the others are only rendered and flagged |

`<db>` is what `shaderdb.py` takes (a `Dx11_Single.res` / `Dx10_Single.res`, `level:<name>`, `<bundle>:<entry>`).
`--stage` adds a folder of FbrbTool-unpacked bundles, each with its `fbrb.manifest`; these are read before the game
copy. `--list <file>` reads mesh names from a file; `--find` treats the arguments as parts of names. Output goes to
`-o`: `results.json`, `summary.txt`, `sheet_NN.png` (14 meshes a sheet, worst first) and one PNG per mesh.

### Flags

| Flag | Means |
|---|---|
| `no-mesh`, `no-layout`, `no-entry`, `no-program` | the mesh set or LOD data is missing; a part has no vertex layout, its shader has no entry, or the entry has no main-pass program |
| `layout-fallback` | the entry has no program for the part's own vertex layout (the engine silently draws it with another program) |
| `bound-empty` | the pixel shader samples a `texture_Texture*` slot that no texture record fills (in DICE's data every such slot is filled; the shadow map and `outdoorLightReflectionEnvmap` are bound by the engine) |
| `missing-texture`, `bad-texture`, `dimension` | a texture named by the entry is not in the files, cannot be uploaded as stored, or is 2D where the shader reads a cube (or the other way round) |
| `nothing-drawn`, `black`, `magenta`, `flat`, `blown-out`, `nan` | the pictures: no pixel covered; mean luminance < 0.03; > 5 % magenta; luminance spread < 0.012 (one constant colour); mean luminance > 0.85 (chrome); NaN or infinite pixels (the game shows them white with bloom) |
| `alpha-discards-all` | a part whose pixel shader keeps < 2 % of the samples its triangles cover. This is measured per draw with occlusion queries, with and without the pixel shader (an alpha test reading the wrong channel) |

In `compare`, flags DICE's own entry also has are not counted against the tested one.

### What is the game's and what is guessed

From the game data: the vertex and index buffers, the vertex layout and the program the engine picks for the part
(main pass, not instanced, no point light, the part's layout), the compiled shaders and their input layout, the
textures in their own block formats with their own mips and an sRGB view when header flag bit 0 is set, the pixel
shader's own sampler states (60-byte records: `u32` register + `D3D11_SAMPLER_DESC`), the material constants of the
program's second parameter block (placed by register, only inside each shader's own `constants` array), the light
colours and sun direction of a weather state (default: mp_005's active state, Sunny; `--weather <folder>`), and the
state's exposure, filmic curve, saturation, contrast and brightness.

Guessed or simplified:
- no shadows (the shadow map is a 1x1 depth of 1, so everything is lit), no point lights, fog, bloom or lens flare;
- the cube the engine binds as `outdoorLightReflectionEnvmap` is `Textures/Sky/Sky_Beach_Generic_RGBA16f` (`--envmap`);
- bones are at rest. Composite vehicles get their parts' transforms from the mesh set; destructible composites
  (key kind 9, `boneMatrices[i].x > 0` shows part i) show every part;
- world matrix identity (the mesh at the origin), no back-face culling, no blend states (every draw opaque), time 0;
- vertex constants that no parameter block fills are 0. The engine sets some of them: tree fronds and wind read
  `constants[...]` in the vertex shader, so a tree's fronds collapse here (its trunk draws). Shaders that sample a
  level-wide map at the world position (sp_005's rocks: `snowocc` at world XZ / 2048) read the level's centre.

### Validation

`validate` on 16 stock meshes (crates, sandbags, containers, palm and fir trees, a destructible house, the Humvee,
M1A2, M16 3p, a MEC soldier, rocks, the M1A2 wreck): 13 draw as in the game. Three are dark or incomplete, for the
reasons in the list above: the fir tree's fronds (engine-set vertex constants), the sp_005 rock formation (level
occlusion map) and the M1A2 wreck (DICE's charred material is near black). The mesh viewer's picture is a weak
reference: it ignores composite part transforms and often picks a mask atlas as the colour map.

`shaderview.exe` is built by `renderqa.py` into the suite's `build\` folder (gitignored) the first time it is needed.
It needs the Visual Studio C++ tools. It uses the GPU, and falls back to WARP (the software rasterizer) when the GPU
device fails; `--warp` forces WARP. The harness runs at below-normal priority.
