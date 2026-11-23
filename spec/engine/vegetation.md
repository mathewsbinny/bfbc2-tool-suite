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

Status: **inferred** · Variants: PC

Matching TERR-VT-3's 12 floats to VEG-UG-3's fields by count, order and DICE's values (3,069 entries):

| # | Field | Range | Most | Why |
|---|---|---|---|---|
| 0 | `Density` | 0.1 - 2.2 | 1, 0.4, 0.8 | a density per patch area |
| 1 | `Variation` | 0.906 - 1 | 1 (2,895) | always at or over #2 |
| 2 | `MinMask` | 0 - 0.9999 | 0, 0.99 | under #1 and #3 on every entry |
| 3 | `MaxMask` | 0.603 - 1 | 1 (2,882) | at or over #2 on every entry |
| 4, 5 | `MinScale` (x, y) | 0.3 - 55 | 0.5, 1 | #5 never over #4; correlated 0.94 with #6, #7 |
| 6, 7 | `MaxScale` (x, y) | 0.5 - 80 | 1, 1.2, 2 | over #4, #5 on 99.5 % |
| 8, 9 | `MinMaskScaleFactor` (x, y) | 0.3 - 1.2 | 1 | #8 = #9 on every entry |
| 10 | `WindScale` | 0 - 0.3 | 0, 0.1 | under every other number on every entry |
| 11 | `ViewDistance` | 18 - 100 | 55 (1,907) | metres, over every other number |

The order follows the type's field list after the strings (`Density`, `Variation`, `MinMask`, `MaxMask`, then the
two-float fields, `WindScale`, `ViewDistance`); the closing bytes 01 00 are presumably `RotateEnable` and
`SkewEnable` (rotated, not skewed), `Enable` being left out (an entry that is written is enabled). A port that uses
another level's undergrowth mesh copies that level's 12 numbers for it (OPFOR: VEG-OPF-1); a new entry starts from the
values DICE uses most for a similar mesh.

[data: spec/probes/terrain.py, 2026-11-22: the 12 positions' ranges, most frequent values and orderings over 3,069 entries]
[inferred: the fields of VEG-UG-3 matched to the positions by the count (6 floats and 3 pairs), the order of the type's field list, and DICE's ranges and orderings; the loader that reads the entry was not found (VEG-UG-6), so the order inside a pair and the two flags are not confirmed]

### VEG-UG-6: The type's field list does not give the file's order, and the loader keeps no copy of the type

Status: **observed** · Variants: PC

`TerrainUndergrowthType`'s field descriptors list the fields in this order: `MinScale`, `MaxScale`,
`MinMaskScaleFactor` (the three two-float fields, one Vec2 type), `Mesh`, `MeshShaderSet`, `CacheShader`, `Density`,
`Variation`, `MinMask`, `MaxMask`, `WindScale`, `ViewDistance`, `Enable`, `RotateEnable`, `SkewEnable`. The file's entry
(VEG-UG-2) puts the three strings first and ends with two flag bytes, so it is not written in the descriptors' order
(nor in field-offset order): VEG-UG-5's assignment, which puts the pairs after the four single floats, is not
contradicted by it, and DICE's data still favours it (#8 = #9 on every entry fits a two-float scale factor). The
visual terrain is the `IVisualTerrain` resource (`<terrain>.Visual`, requested at 0xA15AA0; the concrete class's
vtable 0x144FCF0); no code in the terrain module reads or writes the type's flag bytes (+0x48 - +0x4A), and no factory
makes a 0x4C-byte object with them: the run-time undergrowth keeps its own layout, so the loader cannot be found by the
type's offsets. Which of the 19 slots parses the entry, and therefore the order inside a pair and the two flags, stays
open (VEG-UG-5).

[exe: BFBC2Game.exe field descriptors 0x1C3C940 - 0x1C3CA90 (name, type, offset; MinScale +0x1C, MaxScale +0x14, MinMaskScaleFactor +0x0C with type 0x155D02C; Mesh +0x3C, MeshShaderSet +0x38, CacheShader +0x44, Density +0x40, Variation +0x34, MinMask +0x28, MaxMask +0x24, WindScale +0x30, ViewDistance +0x2C, Enable +0x4A, RotateEnable +0x49, SkewEnable +0x48); ".Visual" 0x144F98C pushed at 0xA15AAD; IVisualTerrain type 0x1BF808C (registered 0x13759A0), the VisualTerrain type 0x1BF80FC (0x1375A20, getType 0xA01830 in vtable 0x144FCF0); round SPG2-rest2's sweep of 0x9F0000 - 0xA40000 for byte accesses at +0x48 - +0x4A (one constructor, 0x9E9530, of another class) and of every `push 0x4C` factory]

### VEG-UG-4: Undergrowth layouts must be in the level's databases

Status: **observed** · Variants: PC, BF1943

The undergrowth meshes draw with instanced-only techniques: a level's databases must hold each mesh's entries and part
layouts. BF1943's undergrowth meshes have part layouts that none of BF1943's shader databases describe, so they cannot
be ported as BF1943 has them (VEG-OPF-1).

[data: OPFOR's Wake Island port, 2026-11-08 onward: BF1943's beach grass and fern meshes have no part layouts in BF1943's five level databases]
[doc: opfor:src/tools/ww2/wake_install.py:716-719]

## 3. OPFOR's undergrowth step

### VEG-OPF-1: BF1943's undergrowth is DICE's, by stand-in

Status: **implemented** · Variants: PC, BF1943

OPFOR's Wake Island install draws BF1943's two undergrowth meshes with DICE's of the same kind under BF1943's names
(`UG_STANDIN`: Isla Inocentes' beach grass for `Beach_grass_01_Mesh`, the campaign beach's fern for `Fern02_Mesh`):
the donor mesh sets copied with their set names renamed, their LOD data as DICE's, their entries and part layouts
cloned into Wake's databases under the new names, and each use's 12 numbers set to the block the donor's own level
uses most for that mesh (MP_004 uses one block in all 40 of its grass's patch groups, SP_01 one in its fern's 14).

[code: opfor:src/tools/ww2/wake_install.py:716-768]
[code: opfor:src/tools/ww2/wake_install.py:2722-2741]
[code: opfor:src/tools/ww2/wake_install.py:2751-2769]

## Open questions

- The 12 numbers' fields are named (VEG-UG-3, VEG-UG-5) but matched by data, not by the loader's code; which
  component of a two-float scale is the height, and the two closing flags, are open.
- How the undergrowth mask shader chooses where each mesh grows (per layer weight, a noise, the mask tiles) and what
  the per-entry shader name changes.
- The undergrowth cache: whether the engine bakes instance positions per patch at load (a cache resource) or each
  frame; no undergrowth resource besides the meshes was found.
- Density per level: the forest and middlegrowth totals are counted, not yet per level (E6's defaults).
- Trees shot down: `PartsToBangerParts` and the banger blueprints (DSTR) for trees are not decoded here.
