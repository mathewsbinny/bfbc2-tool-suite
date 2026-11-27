# Engine internals: the type information, memory, jobs, scenes

**Sources.** Round SPG4 (2026-11-23): `BFBC2Game.exe` (PC build 795745, image base 0x400000) read as data on a
scratch copy - its sections and its `typeinfo` records, walked by `spec/probes/census.py` - and the census of DICE's
partitions (`spec/census.json`). No code was run. The settings objects are SET-ENG-1; the data model's view of types
is `model/types.md`.

## 1. The type information

### ENG-TI-1: The `typeinfo` section: one 24-byte record per reflected type

Status: **verified** · Variants: PC

Frostbite's reflection lives in two sections of the executable: `typeinfo` (VA 0x1BF0000, 0x22D18 bytes) and
`fieldinf` (VA 0x1C13000, 0x34E60 bytes: the members' descriptors, `{name, 0, type record, 0, offset, 0}` records of
24 bytes as SET-GS-2 and the enumeration readers use them). A class's or a primitive's record is 24 bytes, a value
type's or an enumeration's 28:

| Offset | Type | Content |
|---|---|---|
| 0 | u32 | pointer to the type's name (a C string in `.rdata`) |
| 4 | u16 | flags (ENG-TI-2) |
| 6 | u16 | the instance size in bytes |
| 8 | u32 | pointer to the module record, whose first word points to the module's name |
| 12 | u8, u8, u16 | the alignment, the member count (a class's own members, RTTI-MEM-1; a value type's fields: `Vec2` 2, `LinearTransform` 4; an enumeration's values), then 1 (0 on `Core`'s math types) |
| 16, 20 | 2 x u32 | the size again on value types, else 0; 0 |
| 24 | u32 | value types and enumerations only: the member table in `fieldinf` (`Vec2`'s is the section's first record, 0x1C13000) |

A class record holds no member-table pointer: a data class's table is an argument of the static initialiser that
registers it (RTTI-REG-1; CapturePointEntityData's 0x1C256F0, 35 descriptors; round SPG4-rest corrected the address
SPG4 gave, 16 descriptors in). Records follow each other: 1,215 of the
class records are 24 bytes from the next record, 498 of the value types and enumerations 28.

The census walks the section word by word and keeps a record when its name pointer and its module's name pointer
both lead to identifiers: 1,900 records. Every one of the 630 DBX instance classes of DICE's partitions has a record
(630 of 630), so the type information is a superset of the data's classes: 1,270 records are types the data never
instantiates (run-time entities, interfaces, value types and enumerations).

[test: core/tests/test_spec_probes.py::census]
[data: spec/probes/census.py --measure, 2026-11-23 (spec/census.json engine): 1,900 records; 630 of 630 DBX classes among them]
[exe: BFBC2Game.exe section table: typeinfo VA 0x1BF0000 size 0x22D18, fieldinf VA 0x1C13000 size 0x34E60; e.g. GameEntityData's record 0x1BFAF98, CapturePointEntityData's 0x1BFD8C4 with its field table 0x1C256F0]

### ENG-TI-2: Kinds: classes, value types, enumerations, arrays, primitives

Status: **verified** · Variants: PC

A record's kind is (flags >> 4) & 0x1F: 3 a class (flags 0x35, or 0x2035 for the client / world-render entity classes
with a run-time part), 2 a value type (0x29; the math types `Vec2` - `Vec4`, `Quat`, `LinearTransform` carry 0xD029),
0x17 an enumeration (0x179), 4 an array base (0x45: `ArrayBase`, `StringArrayBase`, `RefArrayBase`), 0 `Void`; the
fourteen other records each have a kind of their own and are the primitives `Boolean`, `Int8` - `Int64`,
`Uint8` - `Uint64`, `Float32`, `Float64`, `String`, `FileRef`, `Guid`. DICE's 1,900: 1,362 classes, 289 value types,
231 enumerations, 3 arrays, 14 primitives, 1 void. A DBX field's kind (TYPE-VAL-1) is its member's record's kind: an
enumeration is stored by name (TYPE-VAL-4), a value type is a `complex`, a class reference a `ref`.

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: kinds class 1,362, value type 289, enumeration 231, array 3, primitive 14, void 1]

### ENG-TI-3: Twenty-two modules

Status: **verified** · Variants: PC

The records name 22 modules (the engine's libraries and the game's): `GameSharedResources` 777 records (350 of them
DBX classes), `GameDll` 262 (the game's run-time entities: `Server*` / `Client*`), `Render` 128, `Audio` 125,
`Animation` 124, `Physics` 85, `Emitter` 76, `Entity` 74, `UI` 45, `Core` 40, `Terrain` 34, `Weather` 31,
`WorldRender` 30, `Aidll` 24, `Game` 12, `Decal` 10, `TerrainRender` 8, `Scene` 5, `EmitterRender` 3, `World` 3,
`OvergrowthRender` 2, `Overgrowth` 2. `spec/CENSUS.md` section 4 lists them with their kinds and which records the
specification names; the census fails when a module has no record any rule names.

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the modules and their record counts]

## 2. Memory and jobs

### ENG-MEM-1: Memory report groups, heaps and pools

Status: **observed** · Variants: PC

Every asset class carries `MemoryReportGroup` (117 of the 630 DBX classes: the assets, not the entities): the budget
group its memory is reported under. Fixed heaps are named by settings (`CutsceneLargeHeapSize`,
`CutsceneSmallHeapSize`, `WeaponAsyncHeapSize`, `AllowAllAnimationsHeaps`), small allocations go through a
small-block allocator (`SmallBlockAllocatorPool`, `ThreadCachedSmallBlockAllocator`), and subsystems size their own
pools (the sound's `StreamPool`s, SGR-MIX-1; the AI's `ObstaclePoolSize`, `PolylinePoolSize`, `LocalMapPoolSize`,
`PathObjectsPool`; the renderer's `FrameVertexBufferPool`; terrain's `DynamicMaskPoolWidth` / `Height`). Where each is
set and its default are not traced; a crash from an exhausted pool is in `engine/crashes.md` when one was met.

[data: spec/census.json, 2026-11-23: MemoryReportGroup a member of 117 classes]
[exe: BFBC2Game.exe strings CutsceneLargeHeapSize, CutsceneSmallHeapSize, WeaponAsyncHeapSize, AllowAllAnimationsHeaps, SmallBlockAllocatorPool, ThreadCachedSmallBlockAllocator, ObstaclePoolSize, PolylinePoolSize, LocalMapPoolSize, PathObjectsPool, FrameVertexBufferPool, DynamicMaskPoolWidth, DynamicMaskPoolHeight]

### ENG-JOB-1: The job system's switches; the console paths are compiled in

Status: **observed** · Variants: PC

Work is spread over job threads (`JobThreadCount`, `JobEnable` / `JobsEnable` / `EnableJobs`, `JobPriority`,
`RenderJobAffinity`, `ThreadedLoadingEnable`, `LoopDuringLoadingJobEnable`) with per-subsystem switches and counts:
the occluders (`OccluderJobEnable`, `OccluderMeshJobCount`, OCC-HDR-1), the culling (`PrioritizeCullJobsEnable`),
the undergrowth (`UndergrowthBuildJobCount`, `UndergrowthUpdateJobsEnable`), the sort and build passes
(`SortJobEnable`, `BuildJobCount`, `BuildDrawJobEnable`), the rasteriser and collision (`RasterizeJobCount`,
`CollideJobCount`). The PC executable still names the PS3's SPU switches (`SpuJobEnable`, `CullSpuJobEnable`,
`OccluderSpuJobEnable` ...) and the Xbox 360's job threads (`XenonJobThread0Enable` - `5`, `XenonDispatchJobCount`):
one code base, the other platforms' settings inert on PC (inferred).

[exe: BFBC2Game.exe strings JobThreadCount, JobEnable, JobsEnable, EnableJobs, JobPriority, RenderJobAffinity, ThreadedLoadingEnable, OccluderMeshJobCount, PrioritizeCullJobsEnable, UndergrowthBuildJobCount, SortJobEnable, BuildJobCount, RasterizeJobCount, CollideJobCount, SpuJobEnable, XenonJobThread0Enable ... XenonJobThread5Enable]

## 3. Scenes

### ENG-SCENE-1: The menu background is a scene, not a level

Status: **verified** · Variants: PC

`Scenes/UIBackground` is the front end's 3D background: a `SceneData` (the `Scene` module's level-like root: objects,
parts, event and data connections, a camera and an input controller, its own static cull tree) with `SceneSettings`
(camera, field of view, view distance, mouse sensitivity), a `WorldRenderSettings` (the renderer's switches, mostly
debug draws: the only instance in DICE's data), an `OutdoorLightEntityData` and its own `ShaderDatabaseCollection`
(75: one per level and the scene, the shader groups a level's databases hold, SHDB-TYPE-1). The scene's other record
types (`SceneObjectData`, `WorldBuildView`, `WorldSubSection`) have no instance.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SceneData (Scene) | 1 | 1 | `scenes/uibackground.dbx` | AuxAssets, CameraController, DataConnections, Descriptor, EventConnections, GenerateStaticCullTree, InputController, MaterialGrid, Modes, Objects, Parts, Settings, StaticCullTree, TrashShaderIndex |
| SceneSettings (Scene) | 1 | 1 | `scenes/uibackground.dbx` | CameraForceHeight, CameraForceHeightEnable, CameraTransform, CameraTranslationScale, EmitterSystemMetrics, FieldOfView, LockCamera, MouseSensitivityX, MouseSensitivityY, NearPlane, RenderSettings, ResetLockedCamera, UseCamera, ViewDistance |
| WorldRenderSettings (WorldRender) | 1 | 1 | `scenes/uibackground.dbx` | ApplyShadowmapsEnable, BackgroundEnable, BilateralHalfResCompositeEnable, CascadedBoxShadowmapClipMethodEnable, CascadedBoxShadowmapEnable, ClearColor, ColoredShadowmapSlicesEnable, ColoredTilingEnable, DrawBoundingSpheres, DrawDebugDynamicCubemap, DrawDebugDynamicCubemapMipLevel, DrawDebugHalfRes, DrawDebugLUT, DrawDebugShadowmaps, DrawDebugVisibleEntityTypes, DrawDebugZBufferEnable, DrawFrustums, DrawLightSources, DrawLineBoundingBoxes, DrawPointLightInfo, DrawShadowFrustums, DrawSolidBoundingBoxes, Dx9ShadowSlopeScaleBias, Dx9ZPassDepthBias, Dx9ZPassSlopeScaleBias, DynamicCubemapEnable, DynamicCubemapFilterMode, DynamicCubemapFilterWidth, DynamicCubemapMipmapGenEnable, DynamicCubemapSidesPerFrameCount, DynamicCubemapUpdateEnable, DynamicCullDrawOccludedBoxesEnable, DynamicCullSpuJobEnable, DynamicCullZBufferTestEnable, Enable, FogEnable, ForegroundBlurEnable, ForegroundBlurFilterEnable, ForegroundDepthClearEnable, ForegroundEnable, ForegroundZPassEnable, GammaCorrectionEnable, GenerateShadowmapsEnable, GenericEntityMaxDynamicEntityCount, GenericEntityMaxStaticEntityCount, GenericEntityRendererEnable, HdrEnable, IrradianceVolumeEnable, JobsEnable, LinearZ, ManualTilingEnable, MaxPointLightCount, MemoryBufferSize, MinScreenArea, MultisampleCount, MultisampleQuality, OnlyShadowmapSlice, Ps3SCullEnable, Ps3ShadowDepthBias, Ps3ShadowSlopeScaleBias, Ps3Shadowmap16BitEnable, Ps3ShadowmapTilingEnable, Ps3TilingEnable, Ps3ZCullEnable, Ps3ZPassDepthBias, Ps3ZPassSlopeScaleBias, ReflectionCubemapSize, ScopeFilterColorScale, ScopeFilterEnable, ShadowExtrusionCullingEnable, ShadowMinScreenArea, ShadowViewportScale, ShadowmapExtrusionLength, ShadowmapFirstSliceScale, ShadowmapFixedDepthEnable, ShadowmapFixedMovementEnable, ShadowmapHeight, ShadowmapMinFov, ShadowmapMinScreenArea, ShadowmapSizeZScale, ShadowmapSliceCount, ShadowmapSliceOverlapFactor, ShadowmapSliceSchemeWeight, ShadowmapViewDistance, ShadowmapWidth, ShadowmapsEnable, SimpleShadowmapsEnable, TilePredicationEnable, TonemapEnable, ViewMode, ViewportScale, WeatherSystemHalfResEnable, WireframeEnable, XenonFastHdrColorScale, XenonFastHdrEnable, XenonFastHdrResolveScaleEnable, XenonFixedHdrColorScale, ZBufferShadowTestEnable |
| OutdoorLightEntityData (WorldRender) | 1 | 1 | `scenes/uibackground.dbx` | BillboardBottomColor, BottomColor, BottomColorIntensity, CloudShadowIntensity, CloudShadowScale, DiffuseEnvmap, EnlightenSkyColor, EnlightenSkyColorIntensity, HemisphereLightAngleFactor, KeyColor, KeyColorIntensity, KeySpecularColor, KeySpecularColorIntensity, ReflectionEnvmap, ShCoefficientsLight, ShCoefficientsLightDelta, ShCoefficientsShadow, TopColor, TopColorIntensity, Visible |
| ShaderDatabaseCollection (Render) | 75 | 75 | `scenes/uibackground`, `levels/<level>` | Groups |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes above; the Scene module's records SceneData, SceneObjectData, SceneSettings, WorldBuildView, WorldSubSection]

### ENG-REND-1: Effect rendering and overgrowth settings records

Status: **observed** · Variants: PC

The small render modules hold settings and run-time entities only: `EmitterRender`'s `EmitterSystemSettings` (120
bytes), `ClientEmitterEntity` and `DebrisContainerEntity`, and `OvergrowthRender`'s `OvergrowthSettings` (116) and
`TreeMeshEntity` - the run-time side of FX-CLS and VEG-TREE (`TreeMeshEntityData`, `Overgrowth`). None is a DBX class.

[exe: BFBC2Game.exe typeinfo EmitterSystemSettings 0x1C08604, ClientEmitterEntity 0x1C08654, DebrisContainerEntity 0x1C0866C, OvergrowthSettings 0x1C086F4, TreeMeshEntity 0x1C08744]

## Open questions

- The type record's fourth word (0x10000 or a larger value) and how a type record reaches its `fieldinf` table.
- The memory groups' budgets and the job counts' defaults.
