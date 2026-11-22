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

Status: **observed** · Variants: PC

| System | Data | DICE's levels |
|---|---|---|
| forest | one `ForestEntityData` per level (`Levels/<LV>/ForestEntity`), its `MemberDatas` `TreeMemberData`, each drawing a `TreeMeshEntityData` | 74 of 74 level partitions; 396 member types, 97,190 instances |
| middlegrowth | `MiddlegrowthModelGroupEntityData` (`MemberDatas` of `MiddlegrowthModelGroupMemberData`) | 93 groups in 71 level partitions; 341 member types, 82,908 instances |
| undergrowth | the visual terrain's patch shaders: an undergrowth mask shader and mesh entries (VEG-UG-1); no partition places it | 1,068 of 1,515 patch shaders over 30 terrains |

Forest and middlegrowth members point at blueprints under `vegetation/` (293 tree and 284 middlegrowth member types),
`objects/` (89, 41) and `terrains/` (14, 16).

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
[code: rust/bc2-render/src/terrain.rs:206-224]
[doc: opfor:src/tools/ww2/wake_audit.py:107]

### VEG-UG-2: The undergrowth mesh entry

Status: **verified** · Variants: PC

An undergrowth mesh entry is: the mesh name, a second string, a shader name, 12 x f32, then the bytes 01 00 (3,069 of
3,069). The second string is empty on every DICE terrain but NAM_MP_003, whose 30 entries hold `<mesh>/<guid>` (the
mesh's asset reference); the shader name is set on 1,091 entries. The suite's reader takes the second string as one
zero byte (TERR-VT-3's grammar until this round), so it misreads NAM_MP_003's visual terrain: the editor's
game-accurate view and anything that parses that level's terrain must read it as a string.

[test: core/tests/test_spec_probes.py::visual_terrains]
[data: spec/probes/terrain.py, 2026-11-22: 3,069 entries, tail 01 00 on all; second string set on 30, all in NAM_MP_003 (Package nam_common); shader name on 1,091]
[code: rust/bc2-render/src/terrain.rs:213-220]

### VEG-UG-3: The entry's 12 numbers

Status: **observed** · Variants: PC

DICE's values per position over 3,069 entries (the meaning is inferred from the ranges and pairs, not from the
executable):

| # | Range | Most | Reading |
|---|---|---|---|
| 0 | 0.1 - 2.2 | 1, 0.4, 0.8 | density |
| 1, 3 | 0.906 - 1, 0.603 - 1 | 1 | colour or brightness variation |
| 2 | 0 - 1 | 0, 0.99 | slope or mask threshold |
| 4, 5 | 0.3 - 55 | 0.5, 1 | a min / max pair (size) |
| 6, 7 | 0.5 - 80 | 1, 1.2, 2 | a min / max pair (scale) |
| 8, 9 | 0.3 - 1.2 | 1 (always equal) | a min / max pair |
| 10 | 0 - 0.3 | 0, 0.1 | sink into the ground |
| 11 | 18 - 100 | 55 (1,907) | draw distance, metres |

A port that uses another level's undergrowth mesh copies that level's 12 numbers for it (OPFOR: VEG-OPF-1); a new
entry starts from the values DICE uses most for a similar mesh.

[data: spec/probes/terrain.py, 2026-11-22: the 12 positions' ranges and most frequent values over 3,069 entries]
[inferred: the meanings, from the ranges alone: pairs whose two values are equal or ordered, a last value of metre-like size (18 - 100); nothing in the executable or a playtest confirms them]

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

- The 12 numbers' meanings (VEG-UG-3): the engine's undergrowth code is not traced.
- How the undergrowth mask shader chooses where each mesh grows (per layer weight, a noise, the mask tiles) and what
  the per-entry shader name changes.
- The undergrowth cache: whether the engine bakes instance positions per patch at load (a cache resource) or each
  frame; no undergrowth resource besides the meshes was found.
- Density per level: the forest and middlegrowth totals are counted, not yet per level (E6's defaults).
- Trees shot down: `PartsToBangerParts` and the banger blueprints (DSTR) for trees are not decoded here.
