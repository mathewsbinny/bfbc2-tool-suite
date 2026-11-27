# The shader translation: BF1943's Xenos shaders on BFBC2's PC renderers

How Project OPFOR BFBC2 draws BF1943's ported meshes with BF1943's own shader code: the Xbox 360 microcode of
BF1943's databases decoded, translated to HLSL, compiled for BFBC2's Dx10 / Dx11 renderers and written into the
level's PC databases beside what BFBC2 needs around the code (binding tables, input layouts, parameter blocks, program
keys). And how entries move between databases (the merge). Written by round SPG2 (2026-11-22) from the suite's
translator (`bfbc2-shader-tools/xenos.py`, `xenos_hlsl.py` over `rust/bc2-shader`, `pcbuild.py`), OPFOR's driver
(`src/tools/ww2/wake_translate.py`, `wake_shaders.py`, `wake_install.py`) and its notes; the database format is
[formats/shaderdb.md](../formats/shaderdb.md) (SHDB-*, XENOS-*), the drawing [engine/rendering.md](../engine/rendering.md)
(RNDR-DB, RNDR-DRAW). The oracle is the Wake Island carrier, verified correct in game on 2026-11-07; the rest of the
translated objects were checked in OPFOR's offline preview and in playtests of 2026-11-08 to 2026-11-13.

## 1. What is translated, and what is not

### XSH-SCOPE-1: Dx10 / Dx11 entries of ported objects, terrain patches and strips

Status: **implemented** · Variants: PC, BF1943

For every entry of a ported level's PC databases that BF1943's database also has (default filter: every ported object
under `Objects/` but the hand-held weapons), the programs are rebuilt one per (mesh-part layout, permutation the entry
had), each with BF1943's vertex and pixel shader of the same pass (main or depth only) and point-light flag,
translated. The terrain's patch shaders (BF1943's 34 `PatchMaskShaders` entries) and its baked strips (8 entries) are
translated on every run. Not translated: Dx9 (XSH-DX-1); materials whose look depends on engine values BFBC2 does not
send (`KEEP_DONOR`: rotors, propellers, glass keep the donor BFBC2 material); entries the translator refuses (a
screen-position input ...), which keep their programs and are reported; hand-held weapons and soldiers (queued:
"translate weapons and soldiers").

[code: opfor:src/tools/ww2/wake_translate.py:1-45]
[code: opfor:src/tools/ww2/wake_translate.py:72-77]

### XSH-SCOPE-2: The translation runs in the suite's Rust core

Status: **verified** · Variants: BF1943

`xenos_hlsl.translate(shader, interface)` turns a decoded Xenos shader into HLSL with an entry point `main`, in
`rust/bc2-shader` (round E3f); the Python twin was retired on 2026-11-18 and its recorded results are the oracle
(`xenos_golden.json`, `xenos_golden_game.json`). The interface says what the PC side provides: vertex inputs,
interpolators, material constant values, per-sampler fix-ups, the block constants the engine writes by name.

[code: bfbc2-shader-tools/xenos_hlsl.py:1-22]
[test: bfbc2-shader-tools/tests/test_xenos_native.py::main]

## 2. The code

### XSH-ALU-1: Multiplication follows Direct3D 9: 0 times anything is 0

Status: **verified** · Variants: BF1943

Xenos multiplies as Direct3D 9 does: 0 x anything is 0, also 0 x infinity and 0 x NaN; IEEE gives NaN. Every
translated `mul` and `mad` goes through `xmul`, which returns 0 when either factor is 0. Without it BF1943's shaders
produced NaN where they multiply an unused infinite or undefined value by a zero weight: black surfaces and white
bloom in game.

[code: rust/bc2-shader/src/hlsl.rs:61-64]
[code: rust/bc2-shader/src/hlsl.rs:556-563]
[test: bfbc2-shader-tools/tests/test_xenos_native.py::main]

### XSH-ALU-2: Alpha test moves from render state into the pixel shader

Status: **implemented** · Variants: PC, BF1943

The Xbox applied alpha test in render state; BFBC2's Dx10 / 11 renderer applies none (SHDB-RS-2). A translated pixel
shader for an entry whose BFBC2 donor program discards gets `clip`: kept where alpha > 0.5 (BF1943's alpha is biased:
its kept pixels are above 0.5). Without it palm fronds and their shadow cards drew as solid squares. A strip or patch
keeps BF1943's render state and takes the template's alpha test only where the template has that same state; an entry
whose BF1943 state OPFOR takes (wake_material_states) keeps BF1943's own reference.

[code: opfor:src/tools/ww2/wake_translate.py:763-776]
[test: opfor:src/tools/tests/test_ww2_kitbags.py::test_cutout_and_textures_by_role]

## 3. Around the code

### XSH-BIND-1: Binding tables are generated from the compiled code

Status: **implemented** · Variants: PC

The binding table (SHDB-BIND-1) of a translated shader is built from the compiled shader's own constants: each engine
constant the HLSL keeps by name gets its id (`viewProjMatrix` 2, `worldViewProjMatrix` 6, `worldMatrix` 8,
`boneMatrices` 23, `cameraPos` 24, point light 27 - 29, outdoor light 30 - 37, `outdoorLightShadowTransform` 36 with the
cascade as index, `transparentStartAndEndAndClamp` 45 ...: taken from every BFBC2 Dx9 database's binding tables against
their constant tables), its register and count (bone arrays divided by the mesh kind's divisor, 3 for kinds 1 and 4),
and `first` is the first register after them. Resource tables come from the compiled resource bindings, never a
donor's (SHDB-REG-1).

[code: bfbc2-shader-tools/pcbuild.py:26-34]
[code: bfbc2-shader-tools/pcbuild.py:472-482]
[test: bfbc2-shader-tools/tests/test_pcbuild_batch.py::quick]

### XSH-BIND-2: The terrain's named constants stay named

Status: **implemented** · Variants: PC

Block constants the engine writes by name at draw time (the terrain's `heightmapScaleAndOffsetXZ`,
`heightmapDecalOffsetY`, `heightmapWorldScaleY`, `posScale`, `posOffset`, the dynamic mask's parameters: 13 names)
become `$Globals` variables, recorded in the PC parameter blocks at the registers the compiler gives them; the engine
textures (`heightmap`, `dynamicMaskTileMap`, `dynamicMaskIndirectionMap`, `maskTileMap`) are recorded by parameter.
Baked as the Xbox block's placeholders (0) every strip vertex lay at the decal offset 0 and every patch vertex at the
origin.

[code: bfbc2-shader-tools/pcbuild.py:36-55]
[test: opfor:src/tools/tests/test_ww2_terrain_patches.py::test_strip_and_patch_position_contracts]

### XSH-KEY-1: A program's key: the part's own layout and BF1943's mesh kind

Status: **implemented** · Variants: PC

Each rebuilt program's key (SHDB-KEY-1) names the layout GUID the mesh part really uses (the engine finds a part's
program by its layout; an entry with no program for it falls back silently: the carrier's "blotchy deck") and the
permutation's mesh-kind byte BF1943's: 0 rigid, 1 vehicle (engine bone matrices), 4 character, 9 destructible
composite. BFBC2's kind 9 uses `boneMatrices` only as a per-part visibility flag where BF1943's uses real part
matrices; BFBC2's composite vertices hold the composite part number (not a palette slot).

[code: opfor:src/tools/ww2/wake_translate.py:1-12]
[code: opfor:src/tools/ww2/wake_translate.py:240-245]
[code: rust/bc2-convert/src/shadertranslate.rs:1043-1054]
[data: OPFOR's Wake Island carrier, 2026-11-07: the deck drew blotchy until the key's layout GUID was the part's; verified in game after]

### XSH-FETCH-1: A BF1943 program is picked by fetch format, not by offset alone

Status: **implemented** · Variants: BF1943

BF1943 compiles each pass once per vertex format, so an entry holds several vertex programs. The one used for a part
must fetch each element of the part's BF1943 vertex at its offset and in its format (`XENOS_FETCH`: PC layout format
3, 43, 6, 12, 7 against Xenos fetch format 57, 16, 31, 6, 32): two of the Zero's 8-element programs share offsets, one
fetching the part index where the other fetches a texture coordinate, and the wrong one drew the propeller inside the
plane (49 records over 20 objects re-picked). With no program matching by offset (a vertex with more texture
coordinate sets than any of the material's programs) one is wired by role: its k-th fetch of a format takes the
layout's k-th element of that format.

[code: opfor:src/tools/ww2/wake_translate.py:14-23]
[code: rust/bc2-convert/src/shadertranslate.rs:896-913]
[code: rust/bc2-convert/src/shadertranslate.rs:457-474]

### XSH-FETCH-2: Overlapping PC layout elements are skipped

Status: **implemented** · Variants: PC

A PC layout can carry an element lying inside the one before it (an R16, usage 7, over the position's w at offset 6);
pairing BF1943's elements with the PC layout's by index then shifts every later element (the landing craft's UVs read
from its tangent: black hull). The pairing takes the stream-0 elements in offset order and leaves out any that starts
inside the previous one (85 entries changed on 2026-11-08). BFBC2's same-named models can hold more texture
coordinate sets than BF1943's vertex: the k-th element of a kind pairs with the k-th, extras unread.

[code: rust/bc2-convert/src/shadertranslate.rs:271-283]

### XSH-PATCH-1: Terrain patches translate onto BFBC2's two patch layouts

Status: **implemented** · Variants: PC

BF1943's patch entries are compiled for its one patch layout; the translation makes BF1943's programs for both of
BFBC2's (grid with the mask-atlas stream, and grid alone where the donor, a rule-only patch, has it: TEDIT-SH-3), with
the template's vertex inputs, BF1943's own textures and sampler states on BF1943's samplers, and BF1943's render state.

[code: opfor:src/tools/ww2/wake_translate.py:32-40]
[test: opfor:src/tools/tests/test_ww2_terrain_patches.py::test_patch_template_inputs]

## 4. Moving entries between databases

### XSH-MERGE-1: The merge copies entries by content and renumbers only what it adds

Status: **verified** · Variants: PC

`shaderdb::merge` (round SD1's `shaderdb.py merge`, in `bc2-formats` since round XF1) adds to a base database every
donor entry of the same kind whose name a needed mesh covers and the base lacks, with its programs, shaders, parameter
blocks, fragments and vertex layouts (by content, shared where the base already has the same bytes) and the mesh-part
names of those meshes. The base's records are kept as its bytes; only the new program records are made (their four
indices renumbered into the merged file), the programs re-sorted by id and the entry groups renumbered. The map
export uses it to add the entries a level reaches (SHDB-LVL-2); OPFOR's installs use the Python `Merger`'s
`clone_entry` (an entry under a new name, textures retargeted) and `add_part` (a part layout from a donor).

[code: rust/bc2-formats/src/shaderdb/merge.rs:1-6]
[code: rust/bc2-formats/src/shaderdb/merge.rs:329-340]
[code: bfbc2-shader-tools/shaderdb.py:1003-1010]
[code: bfbc2-shader-tools/shaderdb.py:1128-1130]
[test: rust/bc2-levelwrite/tests/export_xf1.rs::a_cross_set_object_gets_its_entries_and_shape]
[test: bfbc2-shader-tools/tests/test_shaderdb_native.py::run_merges]

### XSH-MERGE-2: A cloned entry must cover its parts' layouts, or the install re-clones it

Status: **implemented** · Variants: PC

An entry cloned from a donor compiled for other vertex layouts has no program for some of its parts' layouts; the
WW2 install's `fix_layouts` clones such entries again from a stock entry of the same shader template whose programs
cover every layout their parts use (textures replaced sampler for sampler), choosing on Dx9 and using the same donors
for Dx10 / 11, and skips entries whose translated programs already cover their standard layouts. A depth-only slot
needs a material whose donor has a program for the depth layout, else the fix re-clones from any covering entry (a
kind 4 soldier entry once made the Type 38 invisible).

[code: opfor:src/tools/ww2/wake_install.py:1640-1644]

## 5. Dx9, Dx10 and Dx11

### XSH-DX-1: Dx10 and Dx11 take the same translation; Dx9 keeps donors

Status: **observed** · Variants: PC

Dx10 and Dx11 share the patch layouts and the translated code (profiles 4_0 / 5_0). Dx9 is not translated: it draws
DICE donor programs with BF1943's textures by role. What differs on Dx9 (the queued Dx9 port): the terrain's named
constants sit inside the `constants` array (as on the Xbox), patches use fragment `MainBuffer` with the height as a
vertex stream (not a vertex-texture fetch), only `outdoorLightShadowTransform0` is ever declared, at most 16 pixel
samplers (33 BF1943 patch entries need 17 - 22), and the shadow map is a hardware comparison texture.

[data: OPFOR's Dx9 survey of Wake Island's Dx9_Single.res, 2026-11-15: 1,723 VS, 4,668 PS, 27,397 programs; the differences as listed]
[doc: opfor:research/WW2_DX9_SHADER_PORT_PLAN_2026-11-15.md:26-60]

### XSH-DX-2: Each Dx database needs its own textures collected

Status: **observed** · Variants: PC

Dx10 / Dx11 programs sample textures the Dx9 ones of the same entry do not (the terrain patches' normal maps), so an
install collects the textures every Dx database names, not Dx9's alone.

[data: OPFOR's Wake Island conversion, 2026-11-07: Dx10 / 11 patch programs sample normal maps the Dx9 programs lack]
[code: opfor:src/tools/ww2/wake_shaders.py:258-260]

## Open questions

- Preview parity gaps (shadows, fog, bloom, blend states, destruction-state parts, scene exposure) and the
  unexplained colour of the LCVP and the distant island (OPFOR's notes).
- The distant island's tree cards drawn as solid squares: the kept billboard donors or BFBC2's far-tree path.
- Dx9's shadow-map format and cascade binding (the Dx9 plan's phase 0).
- Five entries without their own parts; the hand-held weapons and soldiers still on donor shaders.
