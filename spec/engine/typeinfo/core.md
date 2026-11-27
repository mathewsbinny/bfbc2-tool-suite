# The executable's reflection, record by record: registration, members, DICE's data

**Sources.** Round SPG4-rest (2026-11-27): `BFBC2Game.exe` (PC build 795745, image base 0x400000) read as data on a
scratch copy, its static initialisers and the entity factories followed with capstone (REA's Ghidra provider timed
out on the executable in round SPG3-rest); `spec/probes/typeinfo.py` re-reads it into `spec/typeinfo.json` and writes
the tables of this folder (between the `typeinfo:` markers: do not edit them by hand). ENG-TI-1 to 3
(`engine/internals.md`) are the records themselves; this folder names every one of the 1,900 with its parent, its
size, its own members and their offsets, or its values. The tables hold the executable's identifiers, sizes and
offsets only.

Files: this one (the mechanism, DICE's data against the members, `Core`); `game_shared.md` (`GameSharedResources`);
`gamedll.md` (`GameDll`: the run-time entities and the factories that make them); `modules.md` (the engine's other
modules).

## 1. Registration and members

### RTTI-REG-1: Every record is registered by a static initialiser, five ways

Status: **verified** · Variants: PC

Each of the 1,900 records (ENG-TI-1) is registered once, by a static initialiser of the form `push <arguments>; push
<record>; mov ecx, <class info>; call <register>`. The class info is a second, run-time object per type (in the
`typeinfo` section too); the register function says what the type is:

| Register | Records | Arguments before the record (pushed last to first) |
|---|---|---|
| 0x500880 | 961 data classes (the DBX classes and the other data containers) | the member table (`fieldinf`), a creator function, the parent's class info |
| 0x500660 | 400 run-time classes (entities, components, interfaces, resources) | 0, 0, the parent's class info |
| 0x43C390 | 289 value types | none: the member table is the record's word at +24 |
| 0x5008E0 | 231 enumerations | none: the value table is the record's word at +24 |
| 0x4FE260 | 19 built-ins: the 14 primitives, `ArrayBase`, `RefArrayBase`, `StringArrayBase`, `Void`, `DataContainer` | - |

A root class names its own class info as its parent: 47 classes have no parent (`DataContainer` with 961
descendants, `Entity` with 257, `WeaponFiringEffects` with 26, the interfaces and resources). A sixth function, 0x5007C0, registers the template records
(`ScriptOrderHandler::InstantOrder<T>` ...), which ENG-TI-1's walk does not count. So a class record's member table,
which ENG-TI-1 found no pointer to, is its registration's argument: `CapturePointEntityData`'s initialiser 0x13ACAA0
pushes its table 0x1C256F0, its creator 0xB4A890 and `GameEntityData`'s class info 0x1BFB2A4; `ServerSoldierEntity`'s
0x13710A0 pushes 0, 0 and `ServerControllableEntity`'s class info 0x1BF23C8.

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/probes/typeinfo.py --measure, 2026-11-27 (spec/typeinfo.json): 1,900 of 1,900 records registered: data class 961, class 400, value type 289, enumeration 231, built-in 19]
[exe: BFBC2Game.exe register functions 0x500880, 0x500660, 0x43C390, 0x5008E0, 0x4FE260 (templates 0x5007C0); CapturePointEntityData's initialiser 0x13ACAA0 (record 0x1BFD8C4, class info 0x1BFE0FC), ServerSoldierEntity's 0x13710A0 (record 0x1BF1D90, class info 0x1BF2390), CharacterPoseType's 0x13D1220 (0x5008E0), Vec3's 0x136F3D0 (0x43C390)]

### RTTI-MEM-1: A member is a 24-byte descriptor; a record lists only its own

Status: **verified** · Variants: PC

A member table is an array of 24-byte descriptors, as many as the record's member count (its byte 13, ENG-TI-1):

| Offset | Type | Content |
|---|---|---|
| 0 | u32 | the member's name (a C string) |
| 4 | u32 | flags: 0, or 0x48 for an array (bit 0x40) |
| 8 | u32 | the member's type: a record, or a class info for a class reference |
| 12 | u32 | an array's element type (a record or a class info), else 0 |
| 16 | u32 | the member's offset in the object; an enumeration's value |
| 20 | u32 | 0 |

A class's table holds its own members only (CapturePointEntityData: 35); the inherited ones are its parents' (the
parent chain of RTTI-REG-1). The tables are not in offset order. Every member lies inside its record's size, and every
class's own members sit at or past its parent's size (an object is its parent's object followed by its own members):
none of the 7,087 members breaks either. 564 members are arrays; the 231 enumerations hold 1,952 values. Member types:
4,565 primitives, 1,002 class references, 646 value types, 310 enumerations. A primitive's size is its record's
(`Boolean` 1, `Int32` / `Float32` 4, `Float64` / `Int64` 8, `String` and `FileRef` 12, `Guid` 16).

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/probes/typeinfo.py, 2026-11-27: 7,087 members (564 arrays), 1,952 enumeration values; offset + size inside the record 7,087 of 7,087; own members at or past the parent's size in every class]
[exe: BFBC2Game.exe `fieldinf` VA 0x1C13000: CapturePointEntityData's table 0x1C256F0 (35 descriptors), SoldierWeaponDispersion's 0x1C205A8, CharacterPoseType's 0x1C36610]

### RTTI-DBX-1: The run-time members are DICE's DBX fields, less the pipeline's

Status: **verified** · Variants: PC

A DBX instance class's fields (TYPE-ATTR-3, as DICE's partitions hold them) against its run-time members (its own and
its parents'): the same in 398 of the 630 classes; in 230 the DBX holds more; 2 differ both ways. The 118 field names
only the DBX holds are the content pipeline's, which the loader has no member for and so cannot read: the asset
bookkeeping (`MemoryReportGroup` on 116 classes, `Deprecated` / `Post` 40, `IsObsolete` / `ReplacementObject` 25,
`SourceFile` 13), the mesh build's switches (`CompositeMeshAsset` / `TreeMeshAsset`: the occlusion texture, the
LOD count, `SootVoxelSize`, `OccluderMeshNode` ...), the sound graph nodes' `Name` (37 classes), and the wave and
impulse-response import settings (`Normalize`, `RemixToQuad`, `FrequencyCut`, `Classification`). The two that differ:
`SubtitleWaveHashPair` is `WaveName` in the data and `OriginalHashedWaveName` at run time, and `UISharedConfigAsset`
has two members DICE's data does not set, `UnlocksDataVietnam` and `WeaponDataVietnam`.

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/probes/typeinfo.py --dbx against spec/census.json (DICE's partitions), 2026-11-27: 630 classes: the same 398, DBX more 230, differ 2; 118 DBX-only names]

### RTTI-DBX-2: The pathfinding switches and the AI's sensing template are pipeline-only

Status: **verified** · Variants: PC

Of RTTI-DBX-1's pipeline fields, these change what a map editor, NavKit or the bots might expect the game to read:

- `StaticModelEntityData` (1,407 DICE instances), `FenceModelEntityData` and `LadderEntityData`: `ExcludeFromPathfinding`,
  `ForbidPathfinding`, `AllowPathdataSeeds`, `LocalPathfinding`, `NeedsDetailedPathfinding`,
  `AllowExplosiveTracking`, `EdgeModelGenerationSettings`, `DetailedTreeBanger`, `RequiresMesh`; `RigidModelEntityData`:
  `ExcludeFromPathfinding`, `ForbidPathfinding`. The game has no member for them: they act only through the path data
  and edge models the pipeline built from them (`pathdata.hpd`, NavKit's generation), so changing one in a level
  changes nothing in game until the path data is rebuilt.
- `SensingTemplateData` (17): `SeeingDistance`, `HearingDistance`, `ViewAngle`, `PeripheralViewAngle`,
  `SensingSkill`, `CombatFOVMultiplier`, `ReadyFOVMultiplier`. The run-time template keeps none of them, so a bot's
  sensing is not tuned through these fields (inferred: Kynapse's own sensing settings, or constants, decide it).
- `TeamEntityData.Squads` and `LevelData.Parts`: lists the pipeline uses to build the level; the run-time objects do
  not hold them.
- `GameModeData`'s `HudRoot`, `InGameMenu`, `Loader`, `Crosshairs`, `PrimaryLogicLayer`, `SecondaryLogicLayer`: the
  run-time uses the name members instead (`LoaderName`, `HudRootName`, MENU-LOAD-2).

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/probes/typeinfo.py --dbx, 2026-11-27: the fields above in the DBX classes and in no run-time member of the class or its parents; instances from spec/census.json]
[inferred: a member the run-time class lacks cannot be read by the loader, which fills objects through the member tables (RTTI-MEM-1)]

## 2. Core

### RTTI-CORE-1: `Core`'s records

Status: **verified** · Variants: PC

The engine core's 40 records: the built-ins (RTTI-REG-1), the math value types (`Vec2` - `Vec4`, `Quat`,
`LinearTransform`, `Mat4`, `AxisAlignedBox`, `Plane`), `DataContainer` (the root of every data class: 12 bytes, no
reflected member) and `Asset` (`Name` at 12), the applet, application, service and component bases, and the
resource manager's settings. `Int16`, `Int32`, `Uint16`, `Uint32` and `UndefinedType` are reflected primitives too
(the census's "other").

<!-- typeinfo:core -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AppletBase | class | ServiceBase | 24 | - |
| ApplicationBase | class | ComponentBase | 144 | - |
| ArrayBase | array | - | 8 | - |
| Asset | class | DataContainer | 24 | Name: String @12 |
| AxisAlignedBox | value type | - | 32 | min: Vec3 @0, max: Vec3 @16 |
| Boolean | primitive | - | 1 | - |
| ComponentBase | class | - | 24 | - |
| CoreLogLevel | enumeration | - | 4 | CllNone 0, CllEventRecord 1, CllError 2, CllAssert 3, CllFatalAssert 4, CllValidate 5, CllWarning 6, CllInfo 7, CllOutput 8, CllDebug 9 |
| CoreSettings | value type | - | 116 | RandomTimeSeed: Int32 @0, LogMaster: String @4, ResourceMgrSmbMegs: Int32 @16, DiskLayoutPathPatterns: String[] @20, HostUser: String @36, SessionId: String @48, LogLevel: CoreLogLevel @60, Host: String @64, ResourceMgrCoreBytes: Int32 @76, AsyncReadSleepMillis: Int32 @80, StudioBus: String @84, CrashDumpAsserts: Boolean @96, EnableResourceFileChangeListener: Boolean @97, CrashDumpErrors: Boolean @98, DisplayErrors: Boolean @99, ResourceMgrArenaEnable: Boolean @100, FinalBundles: Boolean @101, DisplayAsserts: Boolean @102, EnableTweaker: Boolean @103, EnableMemoryDump: Boolean @104, EnableBitstreamTypeVerification: Boolean @105, EnableResolveErrorTracking: Boolean @106, EnableSparseSerialization: Boolean @107, EnableDrone: Boolean @108, DiskSeekDelayEmulation: Boolean @109, EnableRemoteLog: Boolean @110, EnableDbErrorPolicy: Boolean @111, EnableBitstreamExtendedVerification: Boolean @112, EnableInstanceDirectory: Boolean @113, DiskLayoutEmulation: Boolean @114 |
| DataContainer | class | - | 12 | - |
| Editable | class | Asset | 24 | - |
| FileRef | primitive | - | 12 | - |
| Float32 | primitive | - | 4 | - |
| Float64 | primitive | - | 8 | - |
| Guid | primitive | - | 16 | - |
| Int16 | primitive | - | 2 | - |
| Int32 | primitive | - | 4 | - |
| Int64 | primitive | - | 8 | - |
| Int8 | primitive | - | 1 | - |
| LanguageFormat | enumeration | - | 4 | LanguageFormat_English 0, LanguageFormat_French 1, LanguageFormat_German 2, LanguageFormat_Spanish 3, LanguageFormat_Italian 4, LanguageFormat_Japanese 5, LanguageFormat_Russian 6, LanguageFormat_Polish 7, LanguageFormat_Count 8 |
| LinearTransform | value type | - | 64 | right: Vec3 @0, up: Vec3 @16, forward: Vec3 @32, trans: Vec3 @48 |
| Mat4 | value type | - | 64 | m11: Float32 @0, m12: Float32 @4, m13: Float32 @8, m14: Float32 @12, m21: Float32 @16, m22: Float32 @20, m23: Float32 @24, m24: Float32 @28, m31: Float32 @32, m32: Float32 @36, m33: Float32 @40, m34: Float32 @44, m41: Float32 @48, m42: Float32 @52, m43: Float32 @56, m44: Float32 @60 |
| MemoryReportGroup | enumeration | - | 4 | MemoryReportGroup_default 0, MemoryReportGroup_Soldiers 1, MemoryReportGroup_Vehicles 2, MemoryReportGroup_Objects 3, MemoryReportGroup_Vegetation 4, MemoryReportGroup_Weapons 5 |
| OptionDescriptor | value type | - | 80 | ValueType: String @0, DefaultValue: String @12, TargetPlatforms: String[] @24, Name: String @40, Category: String @52, Description: String @64, IsFlag: Boolean @76, IsCommandLineOption: Boolean @77 |
| Plane | value type | - | 16 | x: Float32 @0, y: Float32 @4, z: Float32 @8, w: Float32 @12 |
| Quat | value type | - | 16 | x: Float32 @0, y: Float32 @4, z: Float32 @8, w: Float32 @12 |
| RefArrayBase | array | - | 20 | - |
| ResourceManagerSettings | class | DataContainer | 16 | DeferredLoadingEnable: Boolean @12 |
| ServiceBase | class | - | 24 | - |
| String | primitive | - | 12 | - |
| StringArrayBase | array | - | 16 | - |
| Uint16 | primitive | - | 2 | - |
| Uint32 | primitive | - | 4 | - |
| Uint64 | primitive | - | 8 | - |
| Uint8 | primitive | - | 1 | - |
| UndefinedType | class | - | 12 | - |
| Vec2 | value type | - | 8 | x: Float32 @0, y: Float32 @4 |
| Vec3 | value type | - | 16 | x: Float32 @0, y: Float32 @4, z: Float32 @8 |
| Vec4 | value type | - | 16 | x: Float32 @0, y: Float32 @4, z: Float32 @8, w: Float32 @12 |
| Void | void | - | 0 | - |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Core 40 records]
