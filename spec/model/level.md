# The level

A level is a partition, `Levels/<LV>.dbx`, in the level's own bundle, plus the partitions and resources it reaches
by reference and by name. This file states its structure: the `LevelData` instance and its description, the
`Objects` list, event connections, placed objects and their cull slots, model groups and their `HavokAsset`
partitions, and the terrain, environment map and water objects.

**Corpus.** "The level partitions" are the 32 level partitions `Levels/<LV>.dbx` read from the
`level-00.fbrb` bundles of OPFOR's `game\Dist\win32\levels` (stock Dist levels plus OPFOR's Onslaught levels; the
counts of some rules cover the first 28 of them), 2026-11-19; structure only.

## 1. The level partition

### LEVEL-PART-1: The level partition and its primary instance

Status: **observed**

The level `<LV>` is the partition `Levels/<LV>.dbx` of the bundle `levels/<lv>/level-00.fbrb`. Its primary
instance is its only `LevelData` instance, it is the partition's first instance, and its `id` is `<LV>`.

[data: 28 level partitions: primary instance type LevelData in 28, at index 0 in 28, id equal to the level name in 28, exactly one LevelData in 28]
[code: rust/bc2-levelwrite/src/write.rs:143-145]

### LEVEL-PART-2: No layers or sub-worlds

Status: **observed**

A Frostbite 1 / BFBC2 level has no layer or sub-world partitions: the level partition holds the level's entities
directly, and the only other partitions it owns are its model-group placements (LEVEL-MG-2) and the root files
found by name (PART-NAME-3). Game-mode content is not a separate layer either; a mode layout is a whole level of
its own (`MP_005GR`) that loads its base level's bundles.

[data: 32 level partitions: no instance type containing SubWorld or Layer; the World-named types are WorldObjectListData 159 and BigWorldSetting 6]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:843-845]

### LEVEL-PART-3: Instances in the level partition

Status: **observed**

The level partition holds every placed entity of the level (placed objects, spawns, triggers, areas, orders, sounds,
effects, event logic), the model groups and their members, the terrain, environment map and forests, the terrain
splines and their points, and helper data (area geometry points, voice-over lines, object sets). It holds 1,500 to
8,500 `Objects` items per level.

[data: 28 level partitions: Objects items per level between 1,500 and 8,500; top listed types ReferenceObjectData 58,054, AreaGeometryEntityData 14,621, GameEffectEntityData 6,089]

## 2. LevelData

### LEVEL-DATA-1: LevelData's members

Status: **observed**

`LevelData` has, in every level partition, these members among others: `Name` (text), `Objects` (array),
`EventConnections` (array), `LevelDescription` (complex), `MaterialGrid`, `ExclusiveStaticCullTree`,
`ClientStaticCullTree`, `ClientTreeStaticCullTree`, `SpawnPointManager`, `SubtitleAsset`, `WaypointAnimations`,
`AmbientSounds`, `SoundStates` (references), `HasAI`, `HasFlyingAI`, `StartLevelFadedToBlack` (booleans),
`DefaultFOV`, `InfantryFOVMultiplier`, `FriendlyDamageModifier` (floats), `MinimapSettings`,
`EmitterSystemMetrics` (complexes), `VoiceOvers`, `VoiceOverLines` (arrays).

[data: 28 level partitions: each of these members present in 28 of 28 LevelData instances, with the same kind in all]

### LEVEL-DATA-2: LevelData.Name is the level's path

Status: **implemented**

`LevelData.Name` is `Levels/<LV>`; a level under a new name sets it, the primary instance's `id` and the
description's `LevelName` to the new name.

[code: rust/bc2-levelwrite/src/mpconvert.rs:1062-1075] [code: rust/bc2-levelwrite/src/export.rs:880-881]
[test: rust/bc2-levelwrite/src/mpconvert.rs::names_follow_the_level_and_case]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:839-840]

### LEVEL-DATA-3: Campaign and multiplayer values

Status: **observed**

A campaign level sets `HasAI` true, `StartLevelFadedToBlack`, a `SubtitleAsset`, `WaypointAnimations`,
`PathfindingTypes`, a `SpawnPointManager` and `InfantryFOVMultiplier` 0.82; a multiplayer level has false, null,
empty and 1.

[data: DICE's MP_SP_005CQ against SP_005, SP_01, MP_005 and MP_009CQ, instance type by instance type (round E3)]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:805-810]

### LEVEL-DESC-1: The level's description

Status: **observed**

A level is described twice: the `LevelDescription` complex inside `LevelData`, and a description partition
`Levels/<LV>_description.dbx` in the startup bundle (`async/startup-00.fbrb`), which the menus read before the level
loads: `GameMode` (a reference to `gamemodes/<mode>/<guid>`), `FactionTeam1` / `FactionTeam2`, `KitImages`, the
`Name` loc key. Without the description partition the load crashes in session state 13; without its two
`LevelListQuery` items (`<lv>.dbx`, `<lv>_description.dbx`) the level is in no menu.

[exe: state 13 crash, _stricmp in the name table FUN_00BD2D30 (OPFOR BFBC2_NAPISAL_RESEARCH.md)]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:823-827] [code: rust/bc2-levelwrite/src/export.rs:49-50]

### LEVEL-DESC-2: A GameMode that does not resolve crashes

Status: **observed**

A description whose `GameMode` reference does not resolve crashes the game at 0x74EAB3 (Coral Sea).

[exe: BFBC2Game.exe 0x74EAB3] [doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:1126]

## 3. The Objects list

### LEVEL-OBJ-1: What is listed

Status: **observed**

`LevelData.Objects` is a reference array of **local** items: the level's top-level entities. Placed objects,
spawns, triggers, orders, effects, sounds, event logic, the terrain, model groups, forests and the static
environment map are listed. Not listed: model-group and tree members, mesh entity data, `HavokAsset`, spline points,
area geometry points that only belong to an area, voice-over lines, object sets, and `LevelData` itself; they are
reached through the listed ones.

[data: 28 level partitions: 103,749 Objects items, all local; unlisted types include TerrainSplinePlanePointData 17,847, StaticModelGroupMemberData 1,240, ObjectSet 1,154, HavokAsset 129, LevelData 28]
[data: world objects in 32 level partitions: TerrainEntityData 32, StaticModelGroupEntityData 54, MiddlegrowthModelGroupEntityData 44, RigidModelGroupEntityData 16, ForestEntityData 32, StaticEnvmapEntityData 28, all listed]

### LEVEL-OBJ-2: No null or dangling item

Status: **verified**

Every `Objects` item must name an instance of the partition. The game reads the list without a check
(FUN_006EFBC0): a `null` item, or one naming no instance, crashes it at 0x006EFF8A.

[exe: BFBC2Game.exe FUN_006EFBC0, fault at 0x006EFF8A]
[code: rust/bc2-formats/src/dbxedit/check.rs:317-343] [test: core/tests/test_dbxedit.py::cases]
[doc: bfbc2-dbx-tools/FORMAT.md:165-166]

### LEVEL-OBJ-3: World objects must be listed to load

Status: **verified**

Terrain, model groups, forests and the static environment map that are not in `Objects` are not loaded (missing
terrain or buildings in game). DbxEdit's `check` warns about them.

[code: rust/bc2-formats/src/dbxedit/check.rs:250-255] [code: rust/bc2-formats/src/dbxedit/check.rs:370-412]
[test: core/tests/test_dbxedit.py::cases] [doc: bfbc2-dbx-tools/FORMAT.md:171-172]

### LEVEL-OBJ-4: A new instance gets an Objects item

Status: **verified**

A copy of a listed instance is listed too: the level writer adds an `Objects` item for each duplicated or placed
instance whose original has one, and removes the item of a deleted instance (REF-DEL-1).

[code: rust/bc2-levelwrite/src/write.rs:346-410] [test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set]
[test: rust/bc2-levelwrite/src/write.rs::deep_duplicate_and_place]

## 4. Event connections

### LEVEL-EVT-1: Where and what

Status: **observed**

A level's event connections are one array, `LevelData.EventConnections`, of unnamed `complex` elements; no other
instance of the level partition has `EventConnections`. Each connection has four members in this order: `Source`
and `Target` (references), `SourceEvent` and `TargetEvent` (complexes, each a `Name` text and an `Id` 4-byte
number): "when Source fires SourceEvent, send TargetEvent to Target".

[data: 28 level partitions: EventConnections only on LevelData (28), 26,892 connections, every one with the members Source, Target, SourceEvent, TargetEvent; all 53,784 event complexes are Name (text), Id (numbers)]
[code: rust/bc2-levelwrite/src/write.rs:194-212]

### LEVEL-EVT-2: Connection ends

Status: **observed**

A connection's ends are local references, except that a `Target` (rarely a `Source`) may be an external
reference to an instance of another partition (a shared entity).

[data: 28 level partitions: Source local 26,889, external 3; Target local 26,517, external 375; no null end]

### LEVEL-EVT-3: Deleting an entity deletes its connections

Status: **verified**

Removing an instance removes every connection whose `Source` or `Target` names it (before DbxEdit's drop, which
would otherwise leave the connection with a `null` end); copying an instance copies its connections with the ends
re-pointed.

[code: rust/bc2-levelwrite/src/write.rs:219-238] [test: rust/bc2-levelwrite/src/write.rs::move_delete_duplicate_set]

## 5. Placed objects and cull slots

### LEVEL-SLOT-1: Enumeration is the static cull slot

Status: **verified**

A placed object's (`ReferenceObjectData`) `Enumeration` is a one-value 4-byte number: its slot in the level's static
cull trees (the trees `LevelData` names). It is `EnumOffset` + the object's first leaf in the exclusive tree, and the
object owns consecutive leaves from there. 0 means the object is not in the trees (dynamic props).

[code: rust/bc2-formats/src/dbxedit/check.rs:64-68] [code: rust/bc2-levelwrite/src/culltree.rs:5-10]
[test: core/tests/test_dbxedit.py::cases]
[data: 32 level partitions: 66,633 placed objects, 7,986 with slot 0]

### LEVEL-SLOT-2: Nonzero slots are unique

Status: **observed**

Within a level no two placed objects have the same nonzero slot. Objects that share one are shown and hidden
together (see-through houses and cars when moving); an object at slot 0 draws see-through and cannot be destroyed.

[data: 32 level partitions: duplicate nonzero slots 0]
[code: rust/bc2-formats/src/dbxedit/check.rs:415-440] [doc: opfor:research/BFBC2_NAPISAL_RESEARCH.md:282-286]

### LEVEL-SLOT-3: Slots belong to a tree

Status: **verified**

Slots are only meaningful against the cull trees they were built for. Moving a level's objects to another level's
cull trees requires every object to take the slot of the same object in that level: by GUID, else by blueprint and
position to 0.1 m, else the nearest unused same-blueprint object within 50 m, else 0 (DbxEdit `fixslots`).

[code: rust/bc2-formats/src/dbxedit/check.rs:142-218] [test: core/tests/test_dbxedit.py::cases]
[doc: bfbc2-dbx-tools/FORMAT.md:182-189]

### LEVEL-ENT-1: Teams are enumerations

Status: **verified**

An entity's team (`Team`, or `InitialOwnerTeam` on a capture point) is the text `Team1`, `Team2` or `TeamNeutral`; a
soldier spawn's `PlayerType` is `SSHumanPlayer` for players, another value for AI.

[code: rust/bc2-scene/src/scene.rs:448-473] [test: rust/bc2-scene/tests/scene.rs::loads_a_level]

### LEVEL-ENT-2: Outlines are linked point instances

Status: **verified**

Areas and routes are lists of `AreaGeometryEntityData` point instances: `Geometry`, `SurroundingGeometry` or
`RouteWaypoints` reference the points; each point's `Next` names the following one. An area is closed, a route
open.

[code: rust/bc2-scene/src/scene.rs:62-66] [code: rust/bc2-scene/src/scene.rs:748-790]
[test: rust/bc2-scene/tests/scene.rs::loads_a_level]

## 6. Model groups

### LEVEL-MG-1: Groups and members

Status: **observed**

A model group (`StaticModelGroupEntityData`, `RigidModelGroupEntityData`, `MiddlegrowthModelGroupEntityData`,
`ForestEntityData`) is a listed instance of the level partition with a `Transform`, an `Enumeration`, a local
`PhysicsData` (a `PhysicsEntityData`) and `MemberDatas`, a reference array of its member instances (unlisted). A
member (`StaticModelGroupMemberData`, `RigidModelGroupMemberData`, `MiddlegrowthModelGroupMemberData`,
`TreeMemberData`) names its blueprint by `MemberType` (external, under `objects/`, `vegetation/` or `terrains/`),
its mesh by `MeshEntityType` (local: `RigidMeshEntityData`, `CompositeMeshEntityData` or `TreeMeshEntityData`), and
counts `InstanceCount`, `FirstPhysicsPart`, `PhysicsPartCount`, `PhysicsPartCountPerInstance`.

[data: 32 level partitions: 146 groups, 1,676 members; PhysicsData local PhysicsEntityData on all 146; MeshEntityType local on all members; MemberType external on all members]

### LEVEL-MG-2: Placements live in `_HavokAsset` partitions

Status: **verified**

A group's placements are not in the level partition but in `Levels/<LV>/<Group>_HavokAsset.dbx` (and its
HavokPhysicsData resource `.res`): `CompositeHavokAsset` instances with `Assets` (references to each placement's
physics asset, `<blueprint>_havok/<guid>`) and `Transforms` (a number array, 16 floats per asset, in `Assets`
order). The suite places one group member per `Assets` item.

[data: Dist level bundles: 232 _HavokAsset partitions (StaticModelGroupEntity 79, MiddlegrowthModelGroupEntity 69, ForestEntity 55, RigidModelGroupEntity 29); every instance CompositeHavokAsset with Transforms 16 per Assets item]
[code: rust/bc2-scene/src/scene.rs:535-590] [test: rust/bc2-scene/tests/scene.rs::loads_a_level]

### LEVEL-MG-3: Null PhysicsData or MeshEntityType crashes

Status: **verified**

A `null` `PhysicsData` or `MeshEntityType` on a model group, group member, forest or tree member crashes the
model-group set-up at 0x00AF7144. Terrain is exempt: its `PhysicsData` is `null` in every level, its collision comes
from the height field.

[exe: BFBC2Game.exe 0x00AF7144] [code: rust/bc2-formats/src/dbxedit/check.rs:244-256]
[test: core/tests/test_dbxedit.py::cases] [data: 32 of 32 TerrainEntityData with PhysicsData null]

### LEVEL-MG-4: The group's part counts must match the member's physics

Status: **observed**

A group's physics resource lists one entry per placement and part, so it indexes each member's physics parts. For
static, rigid and middle-growth groups the group uses exactly the member blueprint's physics part count; forest
groups use fewer (the trunk's parts) and never more. A member that resolves to a blueprint with a different part
count crashes the load at 0xAB4E58 (a part bitset set out of range).

[data: every stock level's groups: 4,356 of 4,356 member types of static, rigid and middle-growth groups use exactly the member's part count]
[exe: BFBC2Game.exe 0xAB4E58] [doc: opfor:DEVELOPMENT_NOTES.md:1465-1476]

## 7. Terrain, environment map, water

### LEVEL-TERR-1: The terrain entity

Status: **verified**

A level's terrain is a listed `TerrainEntityData` whose `TerrainAsset` is an external reference
`terrains/<name>/<name>/<guid>`; the level's terrain name is `<name>` of the first such entity. Its resources are
found by name (`Terrains/<Name>/<Name>.Heightfield-000.res`, PART-NAME-2) in the level's terrain and level stacks.

[code: rust/bc2-scene/src/scene.rs:490-496] [code: rust/bc2-levelwrite/src/export.rs:528-555]
[test: rust/bc2-scene/tests/scene.rs::loads_a_level]
[data: 32 level partitions: one TerrainEntityData each, TerrainAsset external under terrains/]

### LEVEL-ENV-1: The static environment map

Status: **observed**

`StaticEnvmapEntityData` (one per level) names its environment map texture by `StaticEnvmap`, an external reference
under `terrains/`, and must be listed in `Objects` (LEVEL-OBJ-3).

[data: 28 StaticEnvmapEntityData in 32 level partitions, all listed, StaticEnvmap external under terrains/]

## 8. The multiplayer layer

### LEVEL-MP-1: A multiplayer level needs a game-mode layer

Status: **observed**

A level hosted as a multiplayer match needs its mode's entities: teams (`AutoTeam`, three `TeamEntityData`, team
filters), ticket counters, `PreRound`, `Spectate`, squads, messages, the announcer's voice-overs, the MP
`HumanPlayer`, `Mission`, menus, the capture points with their bases and spawns, a combat area, and the event glue
between them. Without it a campaign level hosted as a match stalls in session state 14.

[data: the Conquest layer is the same set of types on every stock Conquest level; OPFOR's SP_005 hosting test stalled in state 14]
[doc: bfbc2-map-editor/EDITOR_ARCHITECTURE.md:793-804]

## Open questions

- `SourceEvent.Id` / `TargetEvent.Id`: whether the `Id` is a hash of `Name` (and which hash), and which the game
  reads.
- The meaning of `Descriptor`, `Parts`, `Modes`, `DataConnections` and `AuxAssets` on `LevelData` (empty text-kind
  arrays in every level partition read).
- Whether external event targets are resolved like other references or through a separate registry.
- The exact contents of the model group's `.res` wrapper (one entry per placement and part is known; its other
  fields are not specified here: `formats/havok.md`).
- The water object: which instance names a level's water and how `WaterMesh` / `VisualWater` are found (by the
  terrain's name, as the scene reads them, is what the suite does).
