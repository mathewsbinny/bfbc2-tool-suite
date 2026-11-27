# Vegetation: forests, middlegrowth and undergrowth

How BFBC2 places and draws its plants, for the map editor's vegetation round (E6) and the WW2 port. Three systems
place plants (VEG-CLS-1): the level's **forest** (trees, `ForestEntityData`), its **middlegrowth** groups (bushes and
small props, `MiddlegrowthModelGroupEntityData`), both model groups in the sense of LEVEL-MG-1, and the terrain's
**undergrowth** (grass and ground clutter scattered by the patch shaders at draw time, with no placement data at
all). Written by round SPG2 (2026-11-22); measured by `spec/probes/terrain.py` over the DICE bundles of the game copy
(74 level partitions, 30 visual terrains, each resource once). Meshes: [formats/mesh.md](../formats/mesh.md)
(`TreeMeshSet`, MSET-PART-2); destruction of trees: [destruction.md](destruction.md); the terrain the undergrowth
grows on: [transforms/terrain_edit.md](../transforms/terrain_edit.md).

## 1. The three systems

### VEG-CLS-1: Forest, middlegrowth and undergrowth

Status: **verified** · Variants: PC

| System | Data | DICE's levels |
|---|---|---|
| forest | one `ForestEntityData` per level (`Levels/<LV>/ForestEntity`), its `MemberDatas` `TreeMemberData`, each drawing a `TreeMeshEntityData` | 74 of 74 level partitions; 396 member types, 97,190 instances |
| middlegrowth | `MiddlegrowthModelGroupEntityData` (`MemberDatas` of `MiddlegrowthModelGroupMemberData`) | 93 groups in 71 level partitions; 341 member types, 82,908 instances |
| undergrowth | the visual terrain's patch shaders: an undergrowth mask shader and mesh entries (VEG-UG-1); no partition places it | 1,068 of 1,515 patch shaders over 30 terrains |

Forest and middlegrowth members point at blueprints under `vegetation/` (293 tree and 284 middlegrowth member types),
`objects/` (89, 41) and `terrains/` (14, 16).

[test: core/tests/test_spec_probes.py::level_partitions] [data: spec/probes/level.py, round SPV1 2026-11-22, DICE's 74 level partitions (Package's copy where DICE's): ForestEntityData 74 in 74, TreeMemberData 396 (InstanceCount sum 97,199), TreeMeshEntityData 737, MiddlegrowthModelGroupEntityData 93 in 71, MiddlegrowthModelGroupMemberData 341 (82,916); the member folders as listed (the table's 97,190 and 82,908 are the Dist copies'); the undergrowth row is VEG-UG-1's check] [test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: 74 DICE level partitions: ForestEntityData 74, TreeMemberData 396 (InstanceCount sum 97,190), TreeMeshEntityData 737, MiddlegrowthModelGroupEntityData 93 (in 71 partitions), MiddlegrowthModelGroupMemberData 341 (82,908); member types by the MemberType's first folder as listed]

### VEG-CLS-2: Forests and middlegrowth are model groups and keep the group rules

Status: **verified** · Variants: PC

A forest or a middlegrowth group is laid out as every model group (LEVEL-MG-5 - LEVEL-MG-8): the group entity's
member types in order, the group file `Levels/<LV>/<Group>_HavokAsset.dbx` holding the instances' transforms and
leaves, the group's physics per member type. Every DICE group of the levels read keeps these rules, forests and
middlegrowth included; the editor's group edits (round SG1: delete, move, add a member) therefore apply to trees and
bushes unchanged. `TreeMemberData` adds `PartsToBangerParts` (a tree's parts mapped to the banger that replaces it
when shot down); `MiddlegrowthModelGroupMemberData` adds `SoundCollisionScale`.

[test: rust/bc2-levelwrite/tests/model_group_members.rs::every_stock_group_keeps_the_rules]
[code: rust/bc2-levelwrite/src/members.rs:185-211]
[code: rust/bc2-render/src/placement.rs:8-14]
[data: spec/probes/terrain.py, 2026-11-22: the two member classes' fields: FirstPhysicsPart, InstanceCount, MemberType, MeshEntityType, Name, PhysicsPartCount, PhysicsPartCountPerInstance, SpawnDatas, plus PartsToBangerParts (trees) / SoundCollisionScale (middlegrowth)]

### VEG-TREE-1: A tree is a TreeMeshSet drawn through a TreeMeshEntityData

Status: **observed** · Variants: PC, BF1943

A tree member's `MeshEntityType` is an `Overgrowth.TreeMeshEntityData` (`Mesh`, `EnableLeaves`), whose mesh is a
`TreeMeshSet` (92 PC tree mesh sets) built from a `TreeMeshAsset`: its parts are the tree's levels (MSET-PART-2), its
far form a billboard (`BillboardShader`, `BillboardResolutionX` / `Y`), with wind fields (`AffectedByWind`,
`BranchesAffectedByWind`, `Bendable`). BF1943's tree mesh sets are a composite LOD plus a billboard name (the
conversion of 2026-11-08).

[data: spec/probes/terrain.py, 2026-11-22: 92 TreeMeshSet resources, 737 TreeMeshEntityData; TreeMeshAsset's 50 fields as listed]
[doc: bfbc2-console-convert/FORMAT.md:221-223]

## 2. Undergrowth

### VEG-UG-1: A patch shader with undergrowth names an undergrowth mask shader and its meshes

Status: **verified** · Variants: PC

A patch shader (TERR-VT-3) that grows undergrowth names a `<terrain>/UgMaskShaders/<layers>` entry and 1 - 12 mesh
entries; one without undergrowth names neither: (mask named, meshes) on 1,068 patch shaders, (neither) on 447, never
one without the other. The undergrowth's density follows the patch's layers (the mask shader is per layer
combination), so painting a layer paints its undergrowth (TEDIT-SH-1): a painter that adds a patch shader must add
its mask shader entry and mesh entries with it. The meshes are `RigidMeshSet`s in `undergrowth` folders (149
resources; 142 distinct meshes named by DICE's terrains), drawn instanced only: no partition references them, so a
level's closure finds them only through the visual terrain.

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: 1,515 patch shaders: (UgMaskShaders named, meshes) 1,068, (neither) 447; mesh entries per patch shader 0 - 12 (0: 447, 1: 372, 4: 231, 2: 182, 5: 99); 142 distinct meshes; 149 RigidMeshSet in undergrowth folders]
[code: rust/bc2-render/src/terrain.rs:245-280]
[doc: opfor:src/tools/ww2/wake_audit.py:107]

### VEG-UG-2: The undergrowth mesh entry

Status: **verified** · Variants: PC

An undergrowth mesh entry is: the mesh name, a second string, a shader name, 12 x f32, then the bytes 01 00 (3,069 of
3,069). The second string is empty on every DICE terrain but NAM_MP_003, whose 30 entries hold `<mesh>/<guid>` (a
reference into the mesh's partition, presumably its shader set: VEG-UG-3); the shader name, set on 1,091 entries, is a
`Terrains/<T>/Shaders/..._Cache` entry. The suite's visual-terrain reader took the second string as one zero byte
until round SPG2-rest, so it refused NAM_MP_003's visual terrain: the game-accurate view of nam_mp_003cq, r, sdm and sr
drew no terrain patches and no road strips. It reads the string now and keeps every entry (`PatchShader::meshes`); all
30 DICE visual terrains read, and the other 33 visual terrains of the game copy read as before. `bc2-convert`'s
visual-terrain walk (the Xbox / BF1943 conversion and `visual_terrain_parts`; console-convert's `terrain.py` goes
through it) read the same one byte until round SPG2-rest2 and reads the string now: its Xbox and BF1943 inputs all
have the empty string, so their output is unchanged, and the PC walk reaches the end of NAM_MP_003 too.

[test: core/tests/test_spec_probes.py::visual_terrains]
[test: rust/bc2-render/src/terrain.rs::undergrowth_entries_read_their_asset_string]
[test: rust/bc2-render/tests/visual_terrains.rs::every_dice_visual_terrain_reads_with_its_undergrowth]
[data: spec/probes/terrain.py, 2026-11-22: 3,069 entries, tail 01 00 on all; second string set on 30, all in NAM_MP_003 (Package nam_common); shader name on 1,091]
[data: SPG2-rest, 2026-11-22: old and new reader over the game copy's 34 visual terrains: the same shaders, maps and roads on 33, NAM_MP_003 refused before ("runs past the end at 0x2f65c") and read now; Terrain::load of nam_mp_003cq prepares its 53 patch shaders]
[code: rust/bc2-render/src/terrain.rs:245-280]
[test: rust/bc2-convert/src/terrain.rs::undergrowth_second_string_is_a_string]
[test: bfbc2-console-convert/tests/test_terrain.py::pc_visual_terrains]
[data: SPG2-rest2, 2026-11-22: bc2-convert's walk over DICE's 30 PC visual terrains: main's 29 (NAM_MP_003 refused, "runs past the end at 0x2f65c"), this round's 30; the console twins (20 levels) and BF1943's Wake, Iwo Jima, Guadalcanal convert as before]
[code: rust/bc2-convert/src/terrain.rs:427-436]

### VEG-UG-3: An entry is the engine's TerrainUndergrowthType

Status: **observed** · Variants: PC

The executable's type information has a class `TerrainUndergrowthType` of 0x4C bytes (named by `TerrainDecalMaterial`'s
`UndergrowthTypes` array), with 15 fields: `Mesh` (+0x3C), `MeshShaderSet` (+0x38), `CacheShader` (+0x44), `Density`
(+0x40), `Variation` (+0x34), `MinMask` (+0x28), `MaxMask` (+0x24), `WindScale` (+0x30), `ViewDistance` (+0x2C), three
two-float fields `MinScale` (+0x1C), `MaxScale` (+0x14) and `MinMaskScaleFactor` (+0x0C) (one field type, eight bytes
apart), and three byte flags `SkewEnable` (+0x48), `RotateEnable` (+0x49), `Enable` (+0x4A). That is three strings,
6 + 3 x 2 = 12 floats and the flags: the entry's grammar (VEG-UG-2) less one flag. Next to it, `TerrainMaskFunction`
has `MinAltitude`, `MaxAltitude`, `MinSlope`, `MaxSlope`. The render settings scale every type at run time
(`UndergrowthDensityScale`, `UndergrowthViewDistanceScale` / `ScaleMax`, `UndergrowthPoplessDistanceRatio`,
`UndergrowthCellSize`, `UndergrowthMaxHeight`, `UndergrowthWindSpeed`, `UndergrowthSinkVelocity`,
`UndergrowthSinkSafeRadius` ...), and the engine keeps an undergrowth cache and mask textures of its own
(`Terrain/Undergrowth/Cache`, `.../Mask`): the instances are generated at run time, not stored.

[exe: BFBC2Game.exe type information: TerrainUndergrowthType's record at file offset 0x11640B0 (size 0x4C), its field descriptors (name, type, offset) at 0x1195940 - 0x1195AA7; the names at 0x10AA880 - 0x10AA9A0; the Undergrowth* settings at 0x10500A0 - 0x10505B4; the cache and mask resource names at 0x104FBBC - 0x104FC4C]

### VEG-UG-5: The 12 numbers' fields

Status: **verified** · Variants: PC

The visual terrain's loader (VEG-UG-6) reads an entry's 12 floats one by one into a 0x50-byte run-time record, in file
order, and the code that uses them names each one (VEG-UG-7):

| # | Record | Field | Range (DICE) | What the code does with it |
|---|---|---|---|---|
| 0 | +0x00 | `Density` | 0.1 - 2.2 | samples per metre along each axis of the cell (times a render setting) |
| 1 | +0x04 | `Variation` | 0.906 - 1 | scales each instance's random offset from its grid point |
| 2 | +0x08 | `MinMask` | 0 - 0.9999 | subtracted from the mask |
| 3 | +0x0C | `MaxMask` | 0.603 - 1 | the loader stores 1 / (MaxMask - MinMask) at +0x30 (1.0 when they are equal) |
| 4, 5 | +0x10, +0x14 | `MinScale` (x, y) | 0.3 - 55 | the scale at random r = 0 |
| 6, 7 | +0x18, +0x1C | `MaxScale` (x, y) | 0.5 - 80 | the scale at random r = 1 |
| 8, 9 | +0x20, +0x24 | `MinMaskScaleFactor` (x, y) | 0.3 - 1.2 | the scale factor at mask 0 (1 at mask 1) |
| 10 | +0x28 | `WindScale` | 0 - 0.3 | the third value of the undergrowth shader's `windParams` (VEG-UG-9) |
| 11 | +0x2C | `ViewDistance` | 18 - 100 | metres: the terrain keeps the largest over its types, the draw scales it |

Then two bytes: the first (+0x34, 1 on all 3,069 DICE entries) turns each instance's up axis to the terrain's normal
(presumably `SkewEnable`); the second (+0x35, 0 on all) has no reader in the executable: round SPG2-rest5 found it
only in the loader and the record copies (VEG-UG-9; presumably `RotateEnable`, which the generator ignores: every
instance turns by one of 32 fixed angles about the vertical). Within a pair the
first float is x. MaxMask is over MinMask on every DICE entry, by 0.0001 at least (MinMask 0.9999, MaxMask 1, which
makes the mask a near step); an entry with the two equal gets the factor 1.0, so its mask ramps over a width of 1 from
MinMask, not a step. A port that uses another level's undergrowth mesh copies that level's 12 numbers for it (OPFOR:
VEG-OPF-1); a new entry starts from the values DICE uses most for a similar mesh.

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: the 12 positions' ranges, most frequent values and orderings over 3,069 entries; round SPG2-rest4: MaxMask - MinMask > 0 on 3,069 of 3,069, smallest 0.0001]
[exe: BFBC2Game.exe loader 0xA2BA20: the entry's strings 0xA2BC2A / 0xA2BC39 (0x5144F0) and 0xA2BC95 (0x506A00, into a 0x200-byte buffer; a reference to record +0x38 when not empty), the floats 0xA2BD8E - 0xA2BE49 (stream vtable +0x1C into +0x00 - +0x2C, the pairs by two-step loops), the bytes 0xA2BE4B - 0xA2BE6B (vtable +0x24 into +0x34, +0x35), +0x30 = 1 / (+0x0C - +0x08) or 1.0 (0x14D94F0) at 0xA2BE6D - 0xA2BEA3; record stride 0x50 at 0xA2BEE2]

### VEG-UG-6: The entry is read by the visual terrain's factory, not by the class's slots

Status: **observed** · Variants: PC

`TerrainUndergrowthType`'s field descriptors list the fields in another order (`MinScale`, `MaxScale`,
`MinMaskScaleFactor`, `Mesh`, `MeshShaderSet`, `CacheShader`, `Density`, `Variation`, `MinMask`, `MaxMask`, `WindScale`,
`ViewDistance`, `Enable`, `RotateEnable`, `SkewEnable`) and the run-time keeps none of the type's 0x4C-byte objects:
the loader fills a 0x50-byte record of its own (VEG-UG-5), which is why round SPG2-rest2 found no code at the type's
offsets. None of the `VisualTerrain` class's 19 slots (vtable 0x144FCF0) parses the file either (round SPG2-rest3 read
them all). The registration at 0x1375A20 only makes the class's type object (parent `IVisualTerrain`, no factory
argument); the parser is reached from the resource factory 0xA2F440, which appends `.visual` to the name, allocates the
0x2500-byte `VisualTerrain` (constructor 0xA27D80, which sets the vtable 0x144FCF0 and two interface vtables at +0x0C
and +0x10) and calls the loader 0xA2BA20 with the stream. The loader walks the file as TERR-VT-1 has it: per patch
shader a 0x20-byte record (its two names as pending references at +0x00 and +0x04, the layer and tiled counts at +0x08
and +0x0C, the groups at +0x10, 0x10 bytes each), per group the entries (0x50 bytes each, VEG-UG-5). The entry's mesh
name and second string are queued together as one reference (0xA145F0, into the terrain's list at +0x14); the shader
name becomes a reference resolved into record +0x38. The records copy as a unit (0xA0AF30: +0x00 - +0x35 by value,
+0x38 a counted reference, then +0x3C and +0x44).

[exe: BFBC2Game.exe 0x1375A20 (0x500660 with the type 0x1BF80FC, parent 0x1BF808C, the last two arguments zero); factory 0xA2F440 (".visual" 0x144FF28, `push 0x2500` at 0xA2F4EB, 0xA27D80 at 0xA2F4FE, the loader 0xA2BA20 at 0xA2F522); constructor 0xA27D80 (vtables 0x144FCF0, +0x0C 0x144FCE0, +0x10 0x144FCDC); loader 0xA2BA20: patch shaders 0xA2BA99 - 0xA2BF65 (count into +0xD08 at 0xA2BA80, stride 0x20 at 0xA2BF55), groups stride 0x10 at 0xA2BF03, entries stride 0x50 at 0xA2BEE2; copies 0xA04CB0, 0xA0AF30]
[exe: BFBC2Game.exe field descriptors 0x1C3C940 - 0x1C3CA90 (round SPG2-rest2's list); vtable 0x144FCF0's 19 slots as round SPG2-rest3 read them (0xA017E0 ... 0xA091D0)]

### VEG-UG-7: How the generator uses an undergrowth type

Status: **observed** · Variants: PC

The cell generator 0xA0DDC0 places one type's instances in a terrain cell:

- **Grid**: `Density` times `UndergrowthDensityScale` (VEG-UG-8) times the cell's x and z extents gives the sample
  counts along each axis,
  so the instances per area grow with the square of `Density`; each sample moves from its grid point by a random
  offset scaled by `Variation`.
- **Mask**: m = clamp((mask byte / 255 - `MinMask`) x the stored 1 / (`MaxMask` - `MinMask`), 0, 1), from the cell's
  undergrowth mask; a sample is kept when its random threshold (4 bits of the instance's random word: 0 - 240 in steps
  of 16) is below m x 255.
- **Scale**: r = 5 random bits / 31; scale = lerp(`MinScale`, `MaxScale`, r) x (m + (1 - m) x `MinMaskScaleFactor`),
  per component: the full factor at mask 0, none at mask 1. The x component scales the instance's first axis, y the
  other two.
- **Orientation**: one of 32 rotations about the vertical, picked by 5 random bits, always; with the first flag byte
  set the up axis is the terrain normal (three bytes of the normal map) and the other two are rebuilt from it.
- **Distance**: the terrain keeps the largest `ViewDistance` over its types (0xA2460A, 0xA278F0); the draw multiplies
  a type's by `UndergrowthViewDistanceScale` (0xA1ECBE; VEG-UG-8).

So a generated terrain that clones a donor's entries gets the donor's density per metre, mask response and sizes; the
mask the patch's layers give decides where (VEG-UG-1).

[exe: BFBC2Game.exe generator 0xA0DDC0 (called from 0xA0ECCF): Density 0xA0DDF7 - 0xA0DE94, Variation 0xA0E0D6 - 0xA0E13E, the mask 0xA0E2E3 - 0xA0E326 (1/255 at 0x144F770) and the keep test 0xA0E34A - 0xA0E371 (255.0 at 0x14116F8), the scale 0xA0E57B - 0xA0E66F (1/31 at 0x144F778), the 32 rotations 0xA0DF5B - 0xA0E027, the normal 0xA0E386 - 0xA0E570; the same mask formula in 0xA08580; ViewDistance 0xA2460A, 0xA278F0, 0xA1ECBE (times setting +0x48)]

### VEG-UG-8: The render settings that scale the undergrowth

Status: **observed** · Variants: PC

The undergrowth renderer reads `VisualTerrainSettings` (TerrainRender, 0xE0 bytes; its pointer at renderer +0xDC).
Round SPG2-rest5 traced these fields to their use:

| Setting | Offset | Use |
|---|---|---|
| `UndergrowthDensityScale` | +0x4C | copied at set-up into the generator's parameters (+0x10); samples per metre = `Density` x this (VEG-UG-7) |
| `UndergrowthViewDistanceScale` | +0x48 | the draw's distance: D = floor(`ViewDistance` x this / `UndergrowthCellSize`) x `UndergrowthCellSize` |
| `UndergrowthPoplessDistanceRatio` | +0x24 | the fade: a cell at distance x draws trunc((1 - t) x (n + 0.99)) of its n instances, t = clamp((x - pD) / (D - pD), 0, 1), p this ratio |
| `UndergrowthViewDistanceScaleMax` | +0x54 | the cell grid's reach: side = trunc(2 x the largest `ViewDistance` x this / `UndergrowthCellSize`), made odd |
| `UndergrowthForceCellSideCount` | +0x64 | replaces that side count unless -1 |
| `UndergrowthMaxCellSideCount` | +0x70 | caps the side count |
| `UndergrowthOverlapDensityScale` | +0x60 | a cell's instance capacity: `UndergrowthCellSize`^2 x the largest `Density` x `UndergrowthDensityScale` x this, rounded up to even |
| `UndergrowthCellSize` | +0x44 | metres per cell (the divisor above) |
| `UndergrowthUpdateCellCount` | +0x1C | cells kept beyond side^2 |
| `UndergrowthCellMaskTextureSize` | +0x6C | the mask texture's size |
| `UndergrowthMaxHeight` | +0x7C | copied into the generator's parameters (+0x0C) each frame; the height added to the top of each generated cell's bounding box (VEG-UG-11) |
| `UndergrowthWindSpeed` | +0x78 | the second value of `windParams` (VEG-UG-9) |
| `UndergrowthGenerateEnable`, `UndergrowthRegenerateEnable` | +0xD1, +0xD6 | the generator's flags (bit 0, bit 1), set each frame |

So a level cannot raise its undergrowth's density or reach in its own data beyond the 12 numbers: the scales are the
engine's settings (`-VisualTerrain.UndergrowthDensityScale` style overrides: SET-CLI-1, untested), the same for every
level.

[exe: BFBC2Game.exe field descriptors of VisualTerrainSettings 0x1C19CA8 - 0x1C1A590 (UndergrowthDensityScale +0x4C 0x1C1A3F8, ViewDistanceScale +0x48 0x1C1A410, WindSpeed +0x78 0x1C1A3B0 ...); set-up 0xA24370: +0x4C into renderer +0x20 at 0xA24585 - 0xA24591, the largest Density and ViewDistance over the types 0xA245D0 - 0xA246A9, +0x54 / +0x60 at 0xA246B5 - 0xA246E1, +0x1C 0xA246E7, +0x44 0xA246ED, the side count 0xA24742 - 0xA247B8 (+0x64, odd, +0x70), +0x6C 0xA247BD, the capacity 0xA247AE - 0xA24802; per frame 0xA1E880: +0x7C into renderer +0x1C 0xA1E8EC, +0xD6 / +0xD1 flags 0xA1E8F8 - 0xA1E910, the generator job 0xA0EB10 at 0xA1E9EE; the draw 0xA1EA70: ViewDistance x +0x48 0xA1ECBE, floor by the cell size 0xA1ECD2 - 0xA1ED3F, +0x24 fade 0xA1ED4D - 0xA1EDE3 (0.99 at 0x14D96E0)]

### VEG-UG-9: WindScale feeds the undergrowth shader's windParams; the second flag byte has no reader

Status: **observed** · Variants: PC

When the renderer draws an undergrowth type it sets the shader constant `windParams` to (the renderer's clock,
`UndergrowthWindSpeed`, the type's `WindScale`, 0). The clock (renderer +0xE4) adds each frame's time step and starts
again from 0 past 10,000 s. So `WindScale` is a per-type sway amount the shader applies with the global wind speed;
the shaders' use of the three values is VEG-UG-10. The entry's second flag byte (+0x35) is read by nothing but the
loader and the two record copies (round SPG2-rest5's search of the terrain module for byte reads at +0x35), so it
changes nothing at run time.

[exe: BFBC2Game.exe the constant names 0xA01480 - 0xA0157C ("windParams" 0x144F5B4 at handle +0x28, renderer +0x98); the draw 0xA0EE10: clock +0xE4 0xA0F0AB, UndergrowthWindSpeed (settings +0x78) 0xA0F0CF, WindScale (type +0x28) 0xA0F0DD, the vector constant set 0x5BB850 at 0xA0F112; the clock 0xA1E88F - 0xA1E8B4 (10,000.0 at 0x14D9480); +0x35 only at 0xA04D08 (copy), 0xA0B9CB / 0xA0BA89 (copies), 0xA2BE63 (loader)]

### VEG-UG-10: The undergrowth vertex programs sway each vertex sideways by its height

Status: **verified** · Variants: PC

Every DICE vertex program with a `windParams` constant (the undergrowth's) moves the mesh's vertex, in the mesh's own
space and before the instance's transform, by

```
s = sin(((x + y + z) x 1000 + w.x) x w.y) x max(y, 0) x w.z
x' = x + s,  z' = z + s,  y' = y
```

with w = `windParams` = (clock, `UndergrowthWindSpeed`, `WindScale`, 0) (VEG-UG-9). So:

- the sway is diagonal in the mesh's x-z plane (the same s on both axes), and the instance's random rotation about
  the vertical (VEG-UG-7) turns it with the mesh;
- its size grows linearly with the vertex's height above the mesh's origin: nothing below y = 0 moves, and `WindScale`
  is metres of sway per metre of height (DICE's 0 - 0.3: VEG-UG-5); a type with `WindScale` 0 is still;
- `UndergrowthWindSpeed` is the angular frequency (radians per second of the clock) and scales the per-vertex phase
  as well; the phase, (x + y + z) x 1000 radians, is in effect a different random offset for each vertex, so the
  vertices of one plant do not move together;
- the clock's wrap at 10,000 s (VEG-UG-9) makes every vertex jump once in about 2.8 hours.

A port that draws undergrowth with another level's mesh keeps the donor's `WindScale` (VEG-OPF-1); a mesh whose origin
is not at its base sways from wherever y = 0 lies.

[test: core/tests/test_spec_probes.py::undergrowth_sway]
[data: spec/probes/terrain.py, 2026-11-27: DICE's 74 Dx9 / 74 Dx10 / 74 Dx11 level shader databases (all of each) hold vertex programs whose parameter block has a `windParams` constant (Dx11 780 programs, 25 distinct; the constant at register 9, 21 or 25); 25 of 25 distinct Dx11 programs carry the instruction sequence above (dp3 of the position with (1, 1, 1), mad by 1000.0 plus w.x, mul by w.y, sincos, max(y, 0), mul by w.z, mad into x and z)]
[exe: BFBC2Game.exe "windParams" 0x144F5B4 registered as a named constant 0xA0156F (vtable 0x1572B18 +0x58); written per type at the draw 0xA0F112 (VEG-UG-9)]

### VEG-UG-11: UndergrowthMaxHeight is the height of a cell's bounding box above its highest instance

Status: **observed** · Variants: PC

The generator job (0xA0EB10) keeps, over all the instances it places in a cell, the lowest and highest base heights
(the generator 0xA0DDC0 writes them through its 8th and 9th arguments, started at FLT_MAX / -FLT_MAX). When the cell
is done the job writes the cell's bounding box: bottom = the lowest base, top = the highest base +
`UndergrowthMaxHeight` (the renderer's copy, set each frame: VEG-UG-8), and marks the cell generated (state 5). The
draw tests that box against each view's frustum (0x4174D0) and takes the distance for the view distance and fade from
the box's centre (0x40F920; VEG-UG-8). The generator itself does not read it: no instance is cut or scaled by it.

So `UndergrowthMaxHeight` is the engine's bound on how tall an undergrowth plant may be: a mesh taller than it can be
culled while its top is still on screen, and a larger value moves the box's centre up by half the difference, which
shifts the fade distances slightly. It is one engine setting for every level, not a level's data.

[exe: BFBC2Game.exe per frame 0xA1E8EC - 0xA1E8F5 (settings +0x7C into renderer +0x1C = the job's parameters +0x0C, the parameters at renderer +0x10 passed at 0xA1E92D); the job 0xA0EB10: min / max started at 0xA0EC12 - 0xA0EC44, the generator called 0xA0ECCF (arguments 8 / 9 = the min / max), the generator's min / max updates 0xA0E7C6 - 0xA0E823; the box 0xA0ED08 - 0xA0ED25 (cell +0x04 = min, +0x14 = max + parameters +0x0C, +0x28 = 5); the draw 0xA1EB9A - 0xA1EC77 (cell = renderer +0x54 + 0x30 x index; frustum 0x4174D0, box centre 0x40F920)]

### VEG-UG-4: Undergrowth layouts must be in the level's databases

Status: **observed** · Variants: PC, BF1943

The undergrowth meshes draw with instanced-only techniques: a level's databases must hold each mesh's entries and part
layouts. BF1943's undergrowth meshes have part layouts that none of BF1943's shader databases describe, so they cannot
be ported as BF1943 has them (VEG-OPF-1).

[data: OPFOR's Wake Island port, 2026-11-08 onward: BF1943's beach grass and fern meshes have no part layouts in BF1943's five level databases]
[doc: rust/bc2-install/src/levelbuild/levelfiles.rs:405-411]

## 3. OPFOR's undergrowth step

### VEG-OPF-1: BF1943's undergrowth is DICE's, by stand-in

Status: **implemented** · Variants: PC, BF1943

OPFOR's Wake Island install draws BF1943's two undergrowth meshes with DICE's of the same kind under BF1943's names
(`UG_STANDIN`: Isla Inocentes' beach grass for `Beach_grass_01_Mesh`, the campaign beach's fern for `Fern02_Mesh`):
the donor mesh sets copied with their set names renamed, their LOD data as DICE's, their entries and part layouts
cloned into Wake's databases under the new names, and each use's 12 numbers set to the block the donor's own level
uses most for that mesh (MP_004 uses one block in all 40 of its grass's patch groups, SP_01 one in its fern's 14).

[code: rust/bc2-install/src/levelbuild/levelfiles.rs:294-415]
[code: rust/bc2-install/src/levelbuild/terrainfiles.rs:76-113]
[code: rust/bc2-install/src/levelbuild/terrainfiles.rs:125-172]

## Open questions

- Answered by round SPG2-rest6: the undergrowth shaders' sway (VEG-UG-10) and `UndergrowthMaxHeight` (VEG-UG-11).
  Still open: the Dx9 programs' sway was counted, not read (the Dx11 ones were); what the engine's default
  `UndergrowthWindSpeed` and `UndergrowthMaxHeight` are (a settings dump in a playtest).
- Whether the engine settings can be set from the command line (SET-CLI-1) and so raise the undergrowth's density
  and reach for every level (VEG-UG-8): a test start (paused).
- How the undergrowth mask shader fills the mask the generator reads (VEG-UG-7: per layer weight, a noise, the mask
  tiles) and what the per-entry shader name changes.
- The undergrowth cache: the generator (VEG-UG-7) runs per cell from the terrain's jobs; whether its output is kept in
  the `Terrain/Undergrowth/Cache` resource between frames was not followed.
- Density per level: measured per multiplayer terrain and biome by round TG4 (transforms/terrain_gen.md TGEN-VEG-2:
  density by distance outside the playable area, clustering, slope and height preferences, poses); E6's defaults
  for a placement brush can start from them.
- Trees shot down: `PartsToBangerParts` and the banger blueprints (DSTR) for trees are not decoded here.
