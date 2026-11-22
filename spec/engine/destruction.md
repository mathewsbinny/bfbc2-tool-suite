# Destruction: parts, health states, destroyed meshes, bangers, model groups, terrain

How BFBC2's destructible objects are described: a blueprint's **parts** (`PartComponentData`), each stepping
through **health states** (`HealthStateData`) that name the mesh and collision part it shows, the effects it plays
and what it spawns (a **banger**: a loose physics object, a falling tree, a toppled sign); the object-level collapse
settings; the same parts inside **static model groups**; and the terrain's **destruction layer**, where explosions
leave craters. Which material a hit damages and how much is `engine/materials.md`'s (MAT-SLOT-5, MAT-GRID-7,
MAT-GRID-8); the physics wrapper's part words and material bytes are `formats/havok.md`'s (HAVOK-WRAP-1,
HAVOK-RUN-1, HAVOK-RUN-3); the building-collapse kill zones are GAME-DMG-1.

**Corpus.** DICE's level bundles of Project OPFOR BFBC2's game copy (`game/**/levels/*/*.fbrb` dated before 2013:
444 bundles), each partition once from the first bundle that holds it. "The blueprints" are the 1,262
`<blueprint>.dbx` partitions that have a `<blueprint>_Havok.dbx` beside them; "the level partitions" the 74
`Levels/<LV>.dbx` with a `LevelData`. Counted by `spec/probes/destruction.py` (2026-11-22) unless a rule says
otherwise. Executable findings are OPFOR's static RE (`opfor:research/MATERIAL_SLOTS_RE.md`); nothing was run in game.

## 1. Health states

### DSTR-STATE-1: A destructible part is a PartComponentData with health states

Status: **verified**

A blueprint's object (`StaticModelEntityData` in 1,035 of the 1,262 blueprints; also `DynamicModelEntityData` 49,
`MiddlegrowthModelEntityData` 28, `FenceModelEntityData` 14, `TreeBangerEntityData` 13, `BangerEntityData` 11,
`DetailedTreeBangerEntityData` 9, `LadderEntityData` 8, `RigidModelEntityData` 5) lists its destructible parts in
`Components`: each a `GameSharedResources.PartComponentData` with `Name` (`part_1`, `ruin` ...), `HealthStates`
(references to `HealthStateData` instances of the same partition), `NeighbourLinks` and five flags (DSTR-PART-1). A
`HealthStateData` holds `Health`, `MeshName`, `Effects`, `Explosion`, `SpawnedBanger`, `CopyDamageToBanger`,
`SpawnedBangerImpulseParams`, `LoosePartPhysics`, `LoosePartPhysicsArray` and `PreferedShapeType`.

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py on the game copy, 2026-11-22: 1,262 blueprints, 599 with PartComponentData parts (1 - 107 each; 1 part in 313, 2 in 65), 3,135 parts; object classes as above, 90 without an object instance]

### DSTR-STATE-2: Two states per part, stored lowest health first

Status: **verified**

2,720 of 3,135 parts have two health states, 304 one, 64 three, 4 four, 43 none. A part's states are stored in
increasing `Health`: in every part with two or more states the first is the lowest, and its `Health` is 0 in 2,786 of
the 2,788.

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: states per part 0 x 43, 1 x 304, 2 x 2,720, 3 x 64, 4 x 4; lowest first in 2,788 of 2,788 multi-state parts; the lowest at Health 0 in 2,786]

### DSTR-STATE-3: A state's Health is where it begins; the healthiest is the part's start

Status: **verified**

A part shows its healthiest state (the highest `Health`) when the level starts and steps down through the others as
it loses health; `Health` is the health at which a state begins, so the healthiest state's value is the part's
starting health (100 on 1,545 parts; 0 - 100,000 in all). The suite orders a part's states healthiest first and shows
the intact object, the destroyed one, or any step between (stage k: each part's k-th state, or its last).

[code: rust/bc2-render/src/simple.rs:354-389]
[code: rust/bc2-render/src/simple.rs:311-324]
[test: rust/bc2-render/src/simple.rs::destruction_states_pick_their_parts]
[data: spec/probes/destruction.py, 2026-11-22: healthiest state Health 0 - 100,000, most 100 x 1,545, 1 x 313, 10 x 217, 5 x 152]

### DSTR-STATE-4: The last state shows nothing and plays the destruction effects

Status: **verified**

In parts with two or more states the lowest state shows no mesh (`MeshName` empty) in 2,512 of 2,788 and carries the
destruction effects (`Effects`: smoke, debris emitters) in 2,366; the healthiest state has effects in only 70. The
effects are thus played on entering the destroyed state.

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: 2,788 multi-state parts: lowest state MeshName empty 2,512, Effects 2,366; healthiest state Effects 70; Effects items 4,133 over 2,482 states]

### DSTR-STATE-5: Each health state has its own part material

Status: **observed**

The game reads a destructible part's material per health state: a state's mesh is its own physics part
(DSTR-MESH-1), whose `PartInfoDatas` entry and part byte carry its material, so a damaged state can take damage by
another material than the intact one.

[exe: the part material is read per health state at 0x6D84D0 (vtable+0xE0 at 0xAAED90 for the part)]
[doc: opfor:research/MATERIAL_SLOTS_RE.md:17-21]

### DSTR-STATE-6: Object collapse settings

Status: **verified**

A destructible object carries `DestroyRuinPartPercentage` (75 on 495 of 505), `ManyPartsDestroyedPercentage` (80 on
489) and `AboutToBeDestroyedTime` (0 on 494; 0.5 - 5 s on the rest), and `PartLinks` (`PartLinkData`: two part
indices and names) on 93 of 568; 80 parts are named `ruin*` (a hangar's `part_1` linked to its `ruin`).

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: 505 objects with the percentages: DestroyRuinPartPercentage 35 - 100 (75 x 495), ManyPartsDestroyedPercentage 40 - 101 (80 x 489), AboutToBeDestroyedTime 0 - 5 (0 x 494); PartLinks 0 - 165 per object, none on 475 of 568]

### DSTR-STATE-7: The ruin takes over when enough parts are gone

Status: **inferred**

When `DestroyRuinPartPercentage` percent of an object's parts are destroyed the object collapses: its remaining parts
go and its `ruin` part steps to its downed state (`ruin_downed`), the building-collapse kill zone fires on the
object's `onDestroyed` (GAME-DMG-1); `ManyPartsDestroyedPercentage` is a second, later threshold.

[inferred: the field names, DICE's values (75 / 80 %), the ruin parts' two states (`ruin` / `ruin_downed`) and GAME-DMG-1's onDestroyed triggers; the collapse code was not traced in the executable]

## 2. Parts

### DSTR-PART-1: Part flags and links

Status: **observed**

A part's flags: `CanCollapse` (set on 3,103 of 3,135), `CanTriggerCollapse` (2,904), `IsFragile` (196),
`IsSupported` (181), `IsLoadBearing` (1). `NeighbourLinks` lists the part's neighbours by index.

[data: spec/probes/destruction.py, 2026-11-22: 3,135 parts, the flag counts above]

### DSTR-PART-2: Bangers

Status: **observed**

A **banger** is a separate blueprint a health state spawns (`SpawnedBanger`, a reference to its partition): a loose,
simulated piece (a fallen sign, a tree top, a mast). 36 states spawn one (35 distinct targets, 20 references to
blueprints named `*_Banger`); 44 blueprints have `banger` in their name. `CopyDamageToBanger` is true on 5,949
states. Other state contents are rare: `Explosion` 22, `LoosePartPhysics` 20, `LoosePartPhysicsArray` items 96;
`PreferedShapeType` is `STBox` on 5,945 of 5,952.

[data: spec/probes/destruction.py, 2026-11-22: SpawnedBanger on 36 states, 35 targets (20 named *_banger, 16 references to other names); blueprints whose name contains banger 44; the state counts above]

### DSTR-PART-3: A part loses health by its material's relation

Status: **inferred**

A hit or explosion on a part takes the source's damage through the relation between the source's material and the
part's current material (MAT-SLOT-5, MAT-GRID-8) and subtracts it from the part's health; at a state's `Health` the
part steps to that state. A part on a material whose relations are x0 (Metal, Concrete, the `*_indestructible`
materials) never changes state.

[inferred: the part damage paths (0x722990, 0x8D6730, 0x6A19A0) read the part material and the damage relation (0x6E44A0); the health bookkeeping itself was not traced]
[doc: opfor:research/PHYSICS_MATERIALS.md:75-79]

## 3. Destroyed meshes

### DSTR-MESH-1: A state's MeshName is its mesh part and its collision part

Status: **verified**

A health state shows the mesh parts named `<MeshName>` or `<MeshName>_<suffix>` (the longest state name that fits:
`ruin_downed_Concrete` is `ruin_downed`'s, not `ruin`'s) and the collision part whose name hashes (33-xor, MAT-GRID-3)
to the wrapper's part word for `<MeshName>`; an empty `MeshName` shows nothing. A mesh or collision part no state
names always shows. Of DICE's non-empty `MeshName`s, 3,423 of 3,431 are a `PartInfoDatas` name of the blueprint's
physics asset: the destroyed and damaged meshes are parts of the same mesh and physics files, not other assets.

[code: rust/bc2-render/src/simple.rs:395-408]
[code: rust/bc2-render/src/simple.rs:330-341]
[test: rust/bc2-render/src/simple.rs::destruction_states_pick_their_parts]
[data: spec/probes/destruction.py, 2026-11-22: 3,423 of 3,431 non-empty MeshNames are PartInfoDatas names of the blueprint's _Havok partition]

### DSTR-MESH-2: State naming

Status: **verified**

The intact state shows the part's own name (in 2,303 of 2,788 multi-state parts); a damaged state the part name plus
a suffix (215 states: `_dest` 80, `_ruin` 65, `_destruction` 46, `_downed` 9, `_damaged` 5) or another name (642,
`ruin_downed` among them); the destroyed state nothing (2,521 empty names over all states).

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: MeshName over all states: the part name 2,574, part name plus suffix 215, other 642, empty 2,521]

### DSTR-MESH-3: Wrecks and debris are blueprints of their own

Status: **verified**

Whole destroyed objects placed as such (vehicle wrecks, debris piles) are separate blueprints, not states:
31 of the blueprints have `wreck` in their name and 9 `debris`; only 2 blueprint names contain `ruin` (ruins are
parts, DSTR-STATE-6).

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: blueprint names (last part) containing banger 44, wreck 31, debris 9, ruin 2, destroyed 0]

### DSTR-MESH-4: Soot

Status: **observed**

Every DICE Dx11 shader database holds the soot programs (`SootPerPixel`, `SootPerVertex`), and the game ships
`SootMesh` resources (TERR-SOOT-1); explosion soot is presumably drawn through them on meshes near a blast.

[data: spec/probes/materials.py, 2026-11-22: SootPerPixel and SootPerVertex in 74 of 74 DICE Dx11 databases]

## 4. Terrain destruction

### DSTR-TERR-1: Destruction is a rule layer of the patch shaders

Status: **verified** · Variants: PC

The terrain's destruction look is a rule layer of the patch shaders (TERR-VT-3): a patch shader that has it is named
`.../PatchMaskShaders/<layers>_Destruction`. 21 of DICE's 30 visual terrains have such shaders (6 on every patch
shader, 9 none: presumably terrains that are not destructible).

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: 30 DICE VisualTerrain resources, 7 - 109 patch mask shaders each, 0 - 72 of them ending _Destruction; every one in 6, none in 9]

### DSTR-TERR-2: The dynamic destruction mask

Status: **implemented** · Variants: PC

The patch shaders read the destruction from an engine-bound **dynamic mask** (`texture_dynamicMaskTileMap`,
`texture_dynamicMaskIndirectionMap`, with `dynamicMaskAtlasParams` / `Offset` / `TileBorderParams`), blended after
the layers (RNDR-TERR-2); the engine writes it as craters appear. The suite's view binds an all-zero mask and neutral
atlas values: the terrain as the level starts.

[data: spec/probes/materials.py, 2026-11-22: texture_dynamicMaskTileMap, texture_dynamicMaskIndirectionMap and dynamicMaskAtlasParams in 74 of 74 DICE Dx11 databases; Destruction_factor in 41]
[code: rust/bc2-render/src/terrain.rs:384-386]
[code: rust/bc2-render/src/terrain.rs:499]

### DSTR-TERR-3: Crater size per material pair

Status: **verified**

The material grid gives a crater's size per pair of explosion and terrain material
(`MaterialRelationTerrainDestructionData`): `Width` 1 - 13 (5 on 1,307 of 3,454, 4 on 934, 3 on 726) and `Depth`
0.1 - 2 (0.5 on 1,931, 0.35 on 814), presumably metres. `MaterialPropertyTerrainData.DestructionEffect` is null on
all 363.

[test: core/tests/test_spec_probes.py::spg_materials]
[data: spec/probes/materials.py, 2026-11-22: 3,454 MaterialRelationTerrainDestructionData in DICE's grids: Width 1 - 13 (8 values), Depth 0.1 - 2 (9 values)]

### DSTR-TERR-4: TerrainData's destruction fields

Status: **verified**

`Terrain.TerrainData` has `MaskTileSize` (64 on all 30 DICE terrains), `MaterialMaskStepSize` (1 on all) and
`DefaultFullDepthEnable` (true on 1 of 30).

[test: core/tests/test_spec_probes.py::terrain_partitions]
[data: spec/probes/destruction.py, 2026-11-22: 30 TerrainData instances, the values above]

## 5. Model groups

### DSTR-MG-1: Most static group members are destructible

Status: **observed**

1,837 of the 2,583 static model-group member types (80,239 placed instances) name a blueprint with destructible
parts, as do all 396 tree member types; middle-growth and rigid group members have one physics part per instance.
Static members have 1 - 14 physics parts per instance (1 x 1,725 types, 2 x 375, 5 x 174), tree members 1 - 5.

[data: spec/probes/destruction.py, 2026-11-22: 74 level partitions: member types static 2,583 (97,112 instances), tree 396 (97,190), middle-growth 341 (82,908), rigid 32 (760); PhysicsPartCountPerInstance static 1 x 1,725, 2 x 375, 3 x 91, 4 x 85, 5 x 174, 6 x 88, 8 x 10, 9 x 4, 14 x 31; tree 1 - 5; middle-growth and rigid 1]

### DSTR-MG-2: A member's parts are the type's parts, laid out per instance

Status: **verified**

A static group holds every instance's physics parts in its own `_HavokAsset.res`: one part word (the instance's
leaf) and one material byte per part, the same bytes for every instance of a type (HAVOK-RUN-3), and the type's
`PhysicsPartCountPerInstance` must equal the member blueprint's part count (LEVEL-MG-4). A member's destruction is
thus the blueprint's, part for part; adding or deleting a member re-lays only these lists.

[code: rust/bc2-levelwrite/src/members.rs:575-790]
[test: rust/bc2-levelwrite/src/members.rs::the_physics_reads_and_relays]
[test: rust/bc2-levelwrite/tests/model_group_members.rs::every_stock_group_keeps_the_rules]
[doc: rust/BENCHMARKS.md:7277-7281]

### DSTR-MG-3: The members' health-state arrays are empty in the files

Status: **verified**

`StaticModelGroupMemberData` has `FirstPartHealthStateIndices` and `FirstPartHealthStateNetworkIds`; both are empty
in every member of DICE's levels, so a member's starting state is not stored: every member starts intact. Its
`NetworkIdCountPerInstance` equals `PhysicsPartCountPerInstance` in 1,150 of 2,583 types.

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: health-state arrays empty in all 3,352 member types of 74 level partitions; NetworkIdCountPerInstance equal to PhysicsPartCountPerInstance in 1,150, not in 1,433]

### DSTR-MG-4: Collapse kill zones

Status: **verified**

DICE's levels hold 1,283 `DamageAreaTriggerEntityData` instances, the building-collapse kill zones of GAME-DMG-1.

[test: core/tests/test_spec_probes.py::spg_destruction]
[data: spec/probes/destruction.py, 2026-11-22: 1,283 DamageAreaTriggerEntityData in 74 level partitions]

## Open questions

- **The collapse** (DSTR-STATE-7): what `DestroyRuinPartPercentage` and `ManyPartsDestroyedPercentage` trigger,
  what `AboutToBeDestroyedTime` delays, and how `PartLinks`, `NeighbourLinks`, `IsSupported` / `IsLoadBearing` /
  `CanCollapse` decide which parts fall with a destroyed one; the object's destruction code (from the part damage
  paths 0x722990 / 0x8D6730 onwards) would settle it, or a playtest destroying a house part by part.
- **Health arithmetic** (DSTR-PART-3): whether a state change is at or below the state's `Health`, and what the 313
  single-state parts at Health 1 mean (presumably indestructible pieces that only need a health record).
- **Group member destruction at run time** (DSTR-MG-3): the empty health-state arrays suggest the game fills them at
  load; what the network ids per instance count when they differ from the parts.
- **The terrain's dynamic mask** (DSTR-TERR-2): its size, format and how a crater's `Width` / `Depth` (DSTR-TERR-3)
  are written into it and into the height field's collision (whether craters deform collision at all);
  `DefaultFullDepthEnable`'s meaning.
- **Soot** (DSTR-MESH-4): the `SootMesh` grammar and what draws it.
- **Banger references** (DSTR-PART-2): 17 of the 35 spawned-banger targets are not among the blueprints read (no
  `_Havok` partition beside them, or in other bundles); their classes were not checked.
