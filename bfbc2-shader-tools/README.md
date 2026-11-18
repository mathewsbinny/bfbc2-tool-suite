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
sd.entry_textures(db)                   # {entry name: [(sampler, cube, texture name)]}; (db, {name}) for some
sd.entry_signatures(db)                 # {entry name: (sorted constant names, sampler count)}: its shader template
sd.entry_layouts(db)                    # {entry name: {vertex layout GUIDs its programs are compiled for}}
sd.coverage(db, meshes)                 # {mesh: entries drawing it}
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
header checks, sampler states, the light, the camera, constant placement and the command line; render states: the
state words decoded (opaque, alpha-tested, two-sided, glass, multiply, additive, alpha-only blend, forced depth write,
alpha to coverage, no depth test), the draw order, back faces culled and two-sided drawn, a blended quad (colour,
alpha, stencil coverage), a pixel shader's own alpha test cutting a ramp, `alpha-test-unbaked`, a texture on t0, the
coverage picture, `combine` and `--opaque`. `--game` (game tier)
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
  depth, point taps are texel loads clamped to the texture (the Xbox's clamp addressing; Load alone reads 0 outside
  it), each cube-map fetch samples the direction its own (s, t, face) names, instanced draws rebuild the world
  matrix from the instance rows. Not handled yet: loop-relative addressing, Dx9; the Xbox-only engine constants
  `xenonTilingScaleAndOffsetNormalized` (35 BF1943 post / water pixel shaders) and
  `xenonInstancingIndexCountAndStart` (10 vertex shaders) are given no value (none of Wake's drawn entries uses them).
- Both run in the suite's Rust core (`rust/bc2-shader`, round E3f) when it is built: the same trees, the same HLSL
  text and errors on every program of BF1943's and BFBC2's Xbox databases (`tests/test_xenos_native.py`), about 6x
  faster one program at a time and 14x with `xenos.parse_many` + `xenos_hlsl.translate_many` (all cores). A tree's
  microcode (`cf`, `instructions` ...) is built on its first read; such a tree, the Python decoder's, copies and
  trees made by hand are handed to the Rust translator attribute by attribute. `parse_shader_python` is the
  decoder's reference (a format reader, kept for good; `BFBC2_PURE_PYTHON=1` or `xenos.use_native(False)` decodes
  in Python). The translation's Python (`translate_python`, `_Emitter`) was retired on 2026-11-18: the module is
  required to translate, and `tests/test_xenos_native.py` checks the results recorded from the Python
  (`xenos_golden.json`, `xenos_golden_game.json`). `xenos_hlsl.helpers()` gives the HLSL helper functions.
- `pcbuild.py` compiles with the system's `d3dcompiler_47.dll` and builds what a PC database stores beside the code:
  the binding table (FORMAT.md section 2), the Dx10/11 input layout, parameter blocks. `compile_hlsl` does not
  compile what it compiled before (round E3d): a source, entry, profile, defines and name already compiled come from
  memory, or from the disk cache in `%LOCALAPPDATA%\bfbc2-tool-suite\cache\hlsl` (one folder per compiler DLL, by
  its SHA-256; `BFBC2_HLSL_CACHE` names another folder, `0` turns it off; `pcbuild.use_cache`); the compiler is
  deterministic, so a cached result is the compile's bytes. `compile_many(jobs)` compiles a batch on all CPU threads
  (`BFBC2_HLSL_WORKERS`), results in input order; `prefetch(jobs)` starts one and returns, and a later
  `compile_hlsl` of a job takes its result or waits for it. `compile_hlsl_uncached` always compiles.
  `tests/test_pcbuild_batch.py`.

Texture-fetch fidelity: 2D texel coordinates marked unnormalized are normalized before sampling or computing
filter weights. Explicit LOD includes the instruction's bias; disabled computed LOD and vertex-stage fetches use
`SampleLevel`. Explicit point fetches use the selected mip's dimensions and clamp the mip to the available chain.
Cube fetches also retain their instruction's LOD bias. Explicit gradients and unnormalized cube coordinates are
refused rather than silently translated with different semantics. General sampler overrides and implicit point-fetch
mip selection remain limited by the existing sampler/point-load policy.

### The translator's differential test (xdiff.py, xenos_exec.py)

```
python xdiff.py [--ps N ...] [--n 1024] [--seed 1] [-o <folder>]      every BF1943 Wake pixel shader by default
python xdiff.py --ps N --bisect                                       the first instruction where the two sides differ
```

Every BF1943 pixel shader runs twice on the same random inputs (engine and material constants by name, interpolants,
the screen position, point-sampled ramp textures with clamped addressing): its original microcode in `xenos_exec.py`,
a reference interpreter written from Xenia's documentation of the instruction set (ucode.h; Xenia's DXBC translator
for the details, e.g. ps holds the unsaturated result), and xenos_hlsl's translation compiled for Direct3D 11 and
drawn by shaderview (WARP) on one-pixel triangles. Outputs and discarded pixels are compared. `--bisect` records a
register after each instruction on both sides (the HLSL inside the instruction's predicate block) and reports the
first that differs on the pixels that ran it.

Result on Battlefield 1943's Wake database (2026-11-10): 815 of 815 pixel shaders exact (from 424 before the point
taps were clamped and cube fetches given their own direction). What it does not cover: the decoder (both sides
read the instructions through xenos.py - Xenia trace replay is the check for that), vertex shaders (not yet), texture
formats, gamma and sampler states (texqa, renderqa), and the values the engine feeds in.

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
python renderqa.py strips --db <db> [--terrain <VisualTerrain>] [--stage <folder>]... [--only <text>]... [-o <folder>]
python renderqa.py patches --db <db> [--stage <folder>]... [--only <text>]... [-o <folder>]
```

| Command | Does |
|---|---|
| `render` | draws each mesh with `--db`, flags what its pictures show, and writes a contact sheet with the worst first |
| `compare` | draws each mesh twice: DICE's entry (`--truth`, or with `auto` the first stock level whose Dx11 database draws the mesh, read from BFBC2's own files) and the `--db` entry (staged files first). It measures coverage overlap (IoU), mean difference, PSNR and luminance ratio, and draws a diff picture |
| `validate` | draws stock meshes with their own level's database beside the mesh viewer's texture-only render |
| `wake` | Project OPFOR: every mesh `wake_audit.drawn_meshes()` lists, drawn with the installed Wake Dx11 database (`work\ww2\install`). Meshes a stock level also draws are compared with DICE's entry; the others are only rendered and flagged |
| `patches` | every terrain patch entry (`PatchMaskShaders`) drawn on a 16 x 16-cell patch as the engine draws one: the grid stream (R16G16_SINT) and the mask atlas streams (R16G16B16A16_SNORM) interleaved, in each patch layout the entry has a program for (`PATCH_LAYOUT` always, the grid-only `PATCH_LAYOUT_NO_TILES` where DICE compiles it for rule-only patches); `posScale` / `posOffset` / `heightmapTexCoordScaleAndOffset` / `heightmapWorldScaleY` and the engine textures (flat height map, half-covered mask tiles, empty dynamic mask) written only where a record of the shader's blocks names them, as the engine does (the vertex shader's samplers and textures from its own block: `shaderview` `vsamp` / `vtex`). Fails: `out-of-bounds` / `nothing-drawn` (a grid placed by constants no record names), `no-program`, `black`, `nan`, `foreign-texture`, `engine-unrecorded`, `layout`. `--height` sets the patch's world height (sea-floor layers shade by height). DICE's own mp_009cq patches with their database: 33 of 33 pass within 0.5 mm |
| `strips` | a VisualTerrain's baked road and decal strips (`terrain.road_strips`; `--terrain` a file or resource, default `terrains/<name>/<name>.visual.res` for `level:<name>`), each drawn alone with its entry's program for the strip layout, from above and at three-quarters, with estimated terrain constants (`strip_values`: a flat height map, an empty dynamic mask). Each covered pixel's depth is taken back to the world: a strip drawn more than 0.5 m + 2 % of its size outside its own vertices' box is `out-of-bounds` (the sheets across the sky of a shader built for another vertex layout); also `no-program`, `nothing-drawn`, `nan`, `foreign-texture` (a `Terrains/<other level>/` texture). Exit code 1 when a strip fails. DICE's own strips with their level's database pass (mp_006 QuadDecal, sp_01 Oil_puddle, mp_005 RoadTracks: within 2 mm) |

`<db>` is what `shaderdb.py` takes (a `Dx11_Single.res` / `Dx10_Single.res`, `level:<name>`, `<bundle>:<entry>`).
`--stage` adds a folder of FbrbTool-unpacked bundles, each with its `fbrb.manifest`; these are read before the game
copy. `--list <file>` reads mesh names from a file; `--find` treats the arguments as parts of names. Output goes to
`-o`: `results.json`, `summary.txt`, `sheet_NN.png` (14 meshes a sheet, worst first) and one PNG per mesh.

Each draw is made with its program's own render state (FORMAT.md section 6, "Render state"): blending, depth test and
writes, back-face culling, alpha to coverage, as BFBC2's Dx11 renderer sets them; opaque draws first, then alpha-tested
ones, then blended ones back to front (by each part's mean vertex position). `results.json` gives each part's state
words and what they mean (`state`, `draw`).

| Option | Does |
|---|---|
| `--opaque` | draws every part opaque, with no culling, in the mesh's order: the harness before render states were decoded |
| `--coverage` | also writes `<mesh>.coverage.png` (compare: `<mesh>.dice.coverage.png` too): per view, which kind of draw was last to cover each pixel - grey opaque, green alpha-tested (its holes are where the pixel shader discarded), blue blended (brighter where more alpha was kept), near black nothing - over the render target's alpha per view (black 0, white 1) |
| `--together` | render: the meshes named drawn into one picture, each at the origin (a building and the mesh laid over it) |

Blended draws do not write depth, so a picture's coverage (the mask the checks use) comes from a stencil each draw
writes where it keeps a sample, not from the depth buffer.

### Flags

| Flag | Means |
|---|---|
| `no-mesh`, `no-layout`, `no-entry`, `no-program` | the mesh set or LOD data is missing; a part has no vertex layout, its shader has no entry, or the entry has no main-pass program |
| `layout-fallback` | the entry has no program for the part's own vertex layout (the engine silently draws it with another program) |
| `bound-empty` | the pixel shader samples a `texture_Texture*` slot that no texture record fills (in DICE's data every such slot is filled; the shadow map and `outdoorLightReflectionEnvmap` are bound by the engine) |
| `missing-texture`, `bad-texture`, `dimension` | a texture named by the entry is not in the files, cannot be uploaded as stored, or is 2D where the shader reads a cube (or the other way round) |
| `nothing-drawn`, `black`, `magenta`, `flat`, `blown-out`, `nan` | the pictures: no pixel covered; mean luminance < 0.03; > 5 % magenta; luminance spread < 0.012 (one constant colour); mean luminance > 0.85 (chrome); NaN or infinite pixels (the game shows them white with bloom) |
| `alpha-discards-all` | a part whose pixel shader keeps < 2 % of the samples its triangles cover. This is measured per draw with occlusion queries, with and without the pixel shader (an alpha test reading the wrong channel) |
| `alpha-test-unbaked` | the program's state asks for an alpha test (0x10) but its pixel shader has no `discard`, and it does not blend. BFBC2's Dx10 / Dx11 renderers make no alpha test of their own (FORMAT.md section 6): the test has to be compiled into the shader, as DICE's are, or the part draws whole. A few DICE programs have it too (their alpha is constant) |

In `compare`, flags DICE's own entry also has are not counted against the tested one.

### What is the game's and what is guessed

From the game data: the vertex and index buffers, the vertex layout and the program the engine picks for the part
(main pass, not instanced, no point light, the part's layout), the compiled shaders and their input layout, the
textures in their own block formats with their own mips and an sRGB view when header flag bit 0 is set, the pixel
shader's own sampler states (60-byte records: `u32` register + `D3D11_SAMPLER_DESC`), the material constants of the
program's second parameter block (placed by register, only inside each shader's own `constants` array), the light
colours and intensities of a weather state (default: mp_005's active state, Sunny; `--weather <folder>`), and the
state's exposure, filmic curve, saturation, contrast and brightness.

Guessed or simplified:
- the key light's direction is a fixed high sun (`NOON_DIR`, about 53 degrees up, on the three-quarter camera's side).
  The shaders take `saturate(dot(normal, outdoorLightDir))`, so the vector points towards the light. `--sun weather`
  uses the state's theta / phi with the formula Project OPFOR measured on Wake Island. For mp_005's Sunny state
  (phi 53) that formula points below the horizon (y = -0.6), so it does not carry over to stock levels;
- no shadows (the shadow map is a 1x1 depth of 1, so everything is lit), no point lights, fog, bloom or lens flare;
- the cube the engine binds as `outdoorLightReflectionEnvmap` is `Textures/Sky/Sky_Beach_Generic_RGBA16f` (`--envmap`);
- bones are at rest. Composite vehicles get their parts' transforms from the mesh set; destructible composites
  (key kind 9, `boneMatrices[i].x > 0` shows part i) show every part;
- world matrix identity (the mesh at the origin), time 0;
- render states are those of a normal main-pass view: depth test and writes allowed, colour writes on, culling not
  inverted (the renderer's global switches); blended parts are sorted per part, not per object; with `--opaque`, no
  culling and no blending at all (as before);
- vertex constants that no parameter block fills are 0. The engine sets some of them: tree fronds and wind read
  `constants[...]` in the vertex shader. The fir tree's and acacia's fronds discard every pixel here (the trunk
  draws) and show as `alpha-discards-all` on DICE's own entries. Shaders that sample a level-wide map at the world
  position (sp_005's rocks: `snowocc` at world XZ / 2048) read the level's centre.

### Validation

`validate` on 16 stock meshes (crates, sandbags, containers, palm and fir trees, a destructible house, the Humvee,
M1A2, M16 3p, a MEC soldier, rocks, the M1A2 wreck), each with its own level's database: 15 draw as in the game
(the desert Humvee, the camouflaged M1A2, textured rocks). Only the fir tree's fronds are missing, because of the
engine-set vertex constants above. Where the mesh viewer picks the right colour map, the mean colour's hue agrees
with it (0.94 - 1.0: palm, house, soldier, rocks, wreck). Elsewhere the mesh viewer is a weak reference: it ignores
composite part transforms and often takes a mask atlas for the colour map. An earlier run with the key light from
the weather's theta / phi drew the rock formation and the M1A2 wreck black: they were lit from below.

`shaderview.exe` is built by `renderqa.py` into the suite's `build\` folder (gitignored) the first time it is needed. Texture resources are bound to both vertex and pixel shader stages.
Its job commands for render states (`blendstate`, `depthstate`, `raster`, `stencil`, `class`; the old `blend 1` still
works) are listed at the top of `native\shaderview.cpp`; jobs without them draw as before.
It needs the Visual Studio C++ tools. It uses the GPU, and falls back to WARP (the software rasterizer) when the GPU
device fails; `--warp` forces WARP. The harness runs at below-normal priority.

Since round E3l (2026-11-17) `renderqa.run_job` hands its jobs to `shaderview -serve` processes, one Direct3D device
for many jobs (job file paths on stdin; each job run from a cleared context and everything it made released after it,
so a job draws exactly what it draws alone - `tests\test_shaderview_serve.py`). Creating the device was most of a
job's time: ~340 ms on the GPU and ~55 ms on WARP for a 10 ms draw. Threads calling `run_job` at once get a server
each; a server whose device fails (exit code 3 / 4) is replaced, and a shaderview too old for `-serve` is run once per
job as before. `BFBC2_SHADERVIEW_SERVE=0` runs one process per job (the reference). `xdiff.py` runs its shaders on
worker threads (`--workers`, `BFBC2_XDIFF_WORKERS`, default 4); the results, files and printed lines are the same as
one at a time. Timings: `rust/BENCHMARKS.md`, "E3l".


BFBC2 PC shadow compatibility: `xenos_hlsl.Interface(pc_shadow_atlas=True)`
adapts BF1943 shadow lookups to the native PC three-cascade atlas. The raw
third fetch component carries the normalized cascade centre (1/6, 1/2,
5/6); PC object shaders place these cascades in the top-left, top-right,
and bottom-left quadrants, with a 255/256 guard scale. The same transformed
coordinates feed depth taps and interpolation weights. The option defaults
to false so console microcode differential tests retain their reference
behavior; Wake's production converter enables it.
