# The engine's other modules, record by record

**Sources.** Round SPG4-rest (2026-11-27): `spec/probes/typeinfo.py` over `BFBC2Game.exe` (RTTI-REG-1, RTTI-MEM-1).
One rule per module of ENG-TI-3 other than `Core` (`core.md`), `GameSharedResources` (`game_shared.md`) and `GameDll`
(`gamedll.md`): first its enumerations and value types, then its classes, with parents, sizes and their own members'
run-time offsets. The tables are generated (do not edit them by hand). A class registered without members (RTTI-REG-1's
0x500660) is a run-time object: an interface (`I...`), a resource the loader makes (`OccluderMesh`, `SootMesh`,
`ImpulseResponse`, OCC-TRI-1, SOOT-FLAG-1, IR-PART-1), an entity or a node.

## 1. Gameplay and logic

### RTTI-ENT-1: `Entity`

Status: **verified** · Variants: PC

The entity model's 74 records: `GameObjectData` and `EntityData` (every placed object's root: `Name`, `Components`,
`Enumeration`, the `Transform`), the event and data connections (`EventConnection`, `DataConnection`) and their specs, the sequences and their property tracks, the logic and effect entities'
bases, the world and its parts (`WorldData`, `WorldPartData`), the material grid and roads, the vector shapes.

<!-- typeinfo:entity-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AimStateOverride | enumeration | - | 4 | AsoNoChange 0, AsoAiming 1, AsoNotaiming 2 |
| ConnectionPair | value type | - | 8 | To: Uint32 @0, From: Uint32 @4 |
| DataConnection | value type | - | 32 | SourceField: String @0, TargetField: String @12, Source: GameObjectData @24, Target: GameObjectData @28 |
| EventConnection | value type | - | 40 | SourceEvent: EventSpec @0, TargetEvent: EventSpec @16, Source: GameObjectData @32, Target: GameObjectData @36 |
| EventSpec | value type | - | 16 | Id: Int32 @0, Name: String @4 |
| GameModeTypes | enumeration | - | 4 | SinglePlayer 0, Multiplayer_Day1 1, Multiplayer_Day2 2 |
| InterpolationType | enumeration | - | 4 | None 0, Linear 1 |
| PropertyKeyData | value type | - | 12 | SortKey: Int32 @0, Time: Int32 @4, InterpolationType: InterpolationType @8 |
| PropertySpec | value type | - | 16 | PropertyType: PropertyTypeEnum @0, PropertyName: String @4 |
| PropertyTypeEnum | enumeration | - | 4 | PropertyType_Invalid 0, PropertyType_String 1, PropertyType_Bool 2, PropertyType_Float32 3, PropertyType_Vec3 4, PropertyType_Vec4 5, PropertyType_Int8 6, PropertyType_Uint8 7, PropertyType_Int16 8, PropertyType_Uint16 9, PropertyType_Int32 10, PropertyType_Uint32 11, PropertyType_Int64 12, PropertyType_Uint64 13 |
| RoadPointData | value type | - | 12 | UserMaskRight: Float32 @0, UserMaskLeft: Float32 @4, Width: Float32 @8 |
| RotationAxis | enumeration | - | 4 | raX 0, raY 1, raZ 2 |
| SequenceEventData | value type | - | 28 | Time: Int32 @0, SortKey: Int32 @4, Target: DataContainer @8, Event: EventSpec @12 |
| SpeedStateOverride | enumeration | - | 4 | SSONoChange 0, SSOSneaking 1, SSOWalking 2, SSORunning 3, SSOSprinting 4 |
| StanceOverride | enumeration | - | 4 | SONoChange 0, SOStanding 1, SOCrouching 2 |
| StateContainerEntry | value type | - | 16 | Identifier: String @0, Instance: DataContainer @12 |
| VectorShapePointData | value type | - | 16 | Position: Vec3 @0 |
| WorldPartPlatform | enumeration | - | 4 | WppWin32 0, WppXenon 1, WppPs3 2 |
<!-- /typeinfo -->

<!-- typeinfo:entity-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AabbGeometryEntityData | class | GeometryEntityData | 144 | Extents: Vec3 @112, Weight: Float32 @128 |
| AreaGeometryEntityData | class | GeometryEntityData | 144 | Weight: Float32 @112, Waypoint: WaypointData @116, Height: Float32 @120, Next: AreaGeometryEntityData @124, Previous: AreaGeometryEntityData @128 |
| AssemblyDescriptorData | class | GameObjectData | 80 | Properties: PropertySpec[] @48, Events: String[] @64 |
| BasicEntityEvent | class | EntityEvent | 20 | - |
| BasicLogicEntityEvent | class | BasicEntityEvent | 20 | - |
| BoolPropertyTrackData | class | PropertyTrackData | 60 | Values: Boolean[] @44 |
| ComponentData | class | LogicEntityData | 128 | - |
| CounterEntityData | class | LogicEntityData | 144 | InitialValue: Int32 @128, OneShot: Boolean @132 |
| DataUpdateEvent | class | EntityEvent | 28 | - |
| DelayEntityData | class | LogicEntityData | 144 | Delay: Float32 @128 |
| EffectAsset | class | ObjectBlueprint | 84 | - |
| EffectEntity | class | Entity | 128 | - |
| EffectEntityData | class | EntityData | 160 | PostEffectBrightnessOffset: Vec3 @112, PostEffectFadeInTime: Float32 @128, PostEffectDistanceFallof: Float32 @132, PostEffectSizeScale: Float32 @136, PostEffectFadeOutTime: Float32 @140, MaxInstanceCount: Uint32 @144, CullDistance: Float32 @148, PostEffectContrastOffset: Float32 @152, ResetInstanceWhenStarted: Boolean @156, TriggerPostEffect: Boolean @157 |
| Entity | class | - | 12 | - |
| EntityBusData | class | Editable | 80 | DataConnections: DataConnection[] @24, Descriptor: AssemblyDescriptorData @40, Objects: GameObjectData[] @44, EventConnections: EventConnection[] @64 |
| EntityData | class | GameObjectData | 112 | Transform: LinearTransform @48 |
| EntityEvent | class | - | 12 | - |
| FloatPropertyTrackData | class | PropertyTrackData | 60 | Values: Float32[] @44 |
| GameObjectData | class | DataContainer | 48 | Name: String @12, Components: GameObjectData[] @24, Enumeration: Uint32 @44 |
| GenericPropertyTrackData | class | PropertyTrackData | 60 | Values: String[] @44 |
| GeometryEntityData | class | EntityData | 112 | - |
| GraphGeometryEntityData | class | EntityData | 144 | ConnectedNodes: GraphGeometryEntityData[] @112 |
| IntPropertyTrackData | class | PropertyTrackData | 60 | Values: Int32[] @44 |
| LinearMovementVisualTransformModifierData | class | VisualTransformModifierData | 64 | MaxOffset: Vec3 @16, MinOffset: Vec3 @32, Frequency: Float32 @48 |
| LogicEntity | class | Entity | 36 | - |
| LogicEntityData | class | EntityData | 128 | Enabled: Boolean @112 |
| MaterialData | class | DataContainer | 80 | DebugColor: Vec3 @16, Hash: Uint32 @32, Name: String @36, Category: String @48, NoCollisionResponseCombined: Boolean @60, ClientDestructible: Boolean @61, NoCollisionResponse: Boolean @62, Penetrable: Boolean @63, SeeThrough: Boolean @64 |
| MaterialGridAsset | class | Asset | 24 | - |
| MaterialGridData | class | Editable | 84 | MaterialRelations: MaterialRelationData[] @24, MaterialGridLayers: MaterialGridLayerData[] @44, Materials: MaterialData[] @64 |
| MaterialGridLayerData | class | DataContainer | 32 | MaterialRelations: MaterialRelationData[] @12 |
| MaterialMapAsset | class | Asset | 24 | - |
| MaterialRelationData | class | DataContainer | 40 | Material2: MaterialData @12, Material1: MaterialData @16, Data: MaterialRelationPropertyData[] @20 |
| MaterialRelationPropertyData | class | DataContainer | 16 | SourceMaterial: MaterialData @12 |
| ObjectBlueprint | class | EntityBusData | 84 | Object: GameObjectData @80 |
| ObjectSet | class | ComponentData | 144 | Connections: ConnectionPair[] @128 |
| PingPongRotationVisualTransformModifierData | class | VisualTransformModifierData | 32 | MaxAngleInDegrees: Float32 @16, Frequency: Float32 @20, RotationAxis: RotationAxis @24, MinAngleInDegrees: Float32 @28 |
| PropertyTrackData | class | DataContainer | 44 | PropertyName: String @12, Target: DataContainer @24, PropertyKeys: PropertyKeyData[] @28 |
| PulseEntityData | class | LogicEntityData | 144 | Interval: Float32 @128 |
| RandomPulseEntityData | class | LogicEntityData | 144 | MaxInterval: Float32 @128, MinInterval: Float32 @132 |
| ReferenceObjectData | class | GameObjectData | 176 | Transform: LinearTransform @48, MeshShaderSet: Asset @112, PhysicsMaterialSet: Asset @116, ReferencedObject: ObjectBlueprint @120, ObjectSets: ObjectSet[] @124, VisualTransformModifiers: VisualTransformModifierData[] @144, OverrideScreenAreaCulling: Boolean @164 |
| RoadData | class | VectorShapeAttachment | 76 | EndIndex: Int32 @52, StartIndex: Int32 @56, RoadPoints: RoadPointData[] @60 |
| RotationVisualTransformModifierData | class | VisualTransformModifierData | 28 | DegreesPerSecond: Float32 @16, RotationAxis: RotationAxis @20, IsStaticRotation: Boolean @24 |
| SequenceData | class | LogicEntityData | 192 | SequenceLength: Int32 @128, Events: SequenceEventData[] @132, Targets: DataContainer[] @148, PropertyTracks: PropertyTrackData[] @168, Looping: Boolean @188, IsLoggingEnabled: Boolean @189 |
| SequenceEntity | class | LogicEntity | 76 | - |
| SequenceNotificationEvent | class | BasicLogicEntityEvent | 28 | - |
| SphereGeometryEntityData | class | GeometryEntityData | 128 | Weight: Float32 @112, Radius: Float32 @116 |
| StateContainer | class | DataContainer | 28 | Entries: StateContainerEntry[] @12 |
| TreeBase | class | Asset | 24 | - |
| TreeNodeBase | class | DataContainer | 24 | NodeName: String @12 |
| VectorShapeAttachment | class | GameObjectData | 52 | Shape: VectorShapeData @48 |
| VectorShapeData | class | GameObjectData | 72 | Tension: Float32 @48, Points: VectorShapePointData[] @52, IsGrounded: Boolean @68, IsClosed: Boolean @69 |
| VisualTransformModifierData | class | DataContainer | 16 | UseRandomTiming: Boolean @12 |
| WaypointData | class | DataContainer | 36 | Stance: StanceOverride @12, SpeedState: SpeedStateOverride @16, AimState: AimStateOverride @20, Speed: Float32 @24, Radius: Float32 @28, PauseTime: Float32 @32 |
| WorldData | class | EntityBusData | 124 | MaterialGrid: MaterialGridData @80, TrashShaderIndex: Int32 @84, AuxAssets: Asset[] @88, Modes: String[] @108 |
| WorldObjectListData | class | DataContainer | 44 | Name: String @12, Objects: GameObjectData[] @24 |
| WorldPartData | class | EntityBusData | 160 | EnabledPlatforms: WorldPartPlatform[] @80, Parent: WorldPartData @96, ObjectLists: WorldObjectListData[] @100, Parts: WorldPartData[] @120, GameModes: GameModeTypes[] @140, Enabled: Boolean @156 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Entity 74 records]

### RTTI-AI-1: `Aidll`

Status: **verified** · Variants: PC

The AI library's 24 run-time classes (no data class: the AI's data lives in `GameSharedResources`): the orders
(`Order` and the eleven `Order...` kinds a squad or soldier receives, AIO-*), the order receivers, the navigation
(`Navigation`, `KynapseNavigation`, `AerialNavigation`, `NoNavigation`), `Squad` and `SquadMember`, and the plan, goal and behaviour creators
(`IPlan::IPlanCreator` ...). None has reflected members.

<!-- typeinfo:aidll-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AerialNavigation | class | Navigation | 112 | - |
| ControlEntry::ControlEntryCreator | class | - | 16 | - |
| IBehaviour::IBehaviourCreator | class | - | 28 | - |
| IGoal::IGoalCreator | class | - | 28 | - |
| IOrderReceiver | class | - | 12 | - |
| IPlan::IPlanCreator | class | - | 16 | - |
| ISquadBehaviour::ISquadBehaviourCreator | class | - | 28 | - |
| KynapseNavigation | class | Navigation | 240 | - |
| Navigation | class | - | 12 | - |
| NoNavigation | class | Navigation | 28 | - |
| Order | class | - | 48 | - |
| OrderAimAt | class | Order | 48 | - |
| OrderChange | class | Order | 56 | - |
| OrderFireAtArea | class | Order | 64 | - |
| OrderFollowPlayer | class | Order | 72 | - |
| OrderHoldGround | class | Order | 56 | - |
| OrderInterlude | class | Order | 48 | - |
| OrderMoveTo | class | Order | 56 | - |
| OrderPatrol | class | Order | 1136 | - |
| OrderRandomWalk | class | Order | 56 | - |
| OrderSearchAndDestroy | class | Order | 72 | - |
| OrderShootAtObject | class | Order | 80 | - |
| Squad | class | IOrderReceiver | 208 | - |
| SquadMember | class | IOrderReceiver | 128 | - |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Aidll 24 records]

### RTTI-GAME-1: `Game`

Status: **verified** · Variants: PC

The game's settings objects (SET-ENG-1: `GameSettings`, `ServerSettings`, `ClientSettings`, `NetworkSettings`,
`PhysicsSettings`, `DemoSettings`, `MonitoringSettings`, `GameRenderSettings`, `GameSootSettings`), `BundleMountPoint`,
`Main` and `SelectedRenderer`. `ServerSettings.KillCam` (a `Boolean` at 166) is the kill cam's switch (HUD-KILL-2).

<!-- typeinfo:game-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| BundleMountPoint | value type | - | 28 | VfsPath: String @0, Bundle: String @12, IsResourcePath: Boolean @24 |
| SelectedRenderer | enumeration | - | 4 | SelectedRenderer_None 0, SelectedRenderer_Dx9 1, SelectedRenderer_Dx10 2, SelectedRenderer_Dx10_1 3, SelectedRenderer_Dx11 4, SelectedRenderer_Xenon 5, SelectedRenderer_Ps3 6 |
<!-- /typeinfo -->

<!-- typeinfo:game-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| ClientSettings | class | DataContainer | 252 | DuplicationChance: Float32 @12, CorruptionChance: Float32 @16, ReorderingChance: Float32 @20, Kit: Uint32 @24, Team: Uint32 @28, CameraProximityEffectRadius: Float32 @32, MaxCameraProximityEffectCount: Int32 @36, LoadingTimeout: Float32 @40, ScreenshotFilename: String @44, MaxLatency: Float32 @56, AimScale: Float32 @60, DropSpikeChance: Float32 @64, IncomingFrequency: Float32 @68, MinLatency: Float32 @72, PacketDrops: Float32 @76, SpawnPoint: String @80, Level: String @92, MaxDropDuration: Float32 @104, MinDropDuration: Float32 @108, SecondaryServerIp: String @112, LoadedTimeout: Float32 @124, Ps3SoundMaxRsxMemoryUsage: String @128, Ps3SoundCellCacheSize: Uint32 @140, Ps3SoundMinRsxAllocation: Uint32 @144, StreamingSoundPath: String @148, MaxSoundCount: Uint32 @160, ServerIp: String @164, IngameTimeout: Float32 @176, ResetStatsOnConnect: Boolean @180, UsePS3VoipCompression: Boolean @181, HavokVisualDebugger: Boolean @182, AutoUnspawnDynamicObjects: Boolean @183, HavokVDBShowsEffectsWorld: Boolean @184, DisableTelemetry: Boolean @185, LimitedMainMenu: Boolean @186, AlwaysShowOverlay: Boolean @187, DebugCollectibles: Boolean @188, ThreadedLoadingEnable: Boolean @189, CheckDeviceEntitlements: Boolean @190, EnableCollectibleNotifications: Boolean @191, LocalVehicleSimulationEnabled: Boolean @192, ShowBuildId: Boolean @193, AsyncFollowWheels: Boolean @194, PreloadingEnabled: Boolean @195, ExtractPersistenceInformation: Boolean @196, KeepEffects: Boolean @197, EnableRestTool: Boolean @198, TextureBakerEnable: Boolean @199, UnrestrictedUnlocks: Boolean @200, UseScriptedInput: Boolean @201, UseMouseAndKeyboardSystem: Boolean @202, HavokCaptureToFile: Boolean @203, MemoryStatsEnable: Boolean @204, UseGlobalGamePadInput: Boolean @205, EnableAdvertisements: Boolean @206, IsSpecialEdition: Boolean @207, IsAchievementsEnabled: Boolean @208, DebugAdvertisements: Boolean @209, PauseGameOnStartUp: Boolean @210, IsEffectsDisabled: Boolean @211, IsNetworkStatsEnabled: Boolean @212, IsMultiplayer: Boolean @213, IsStatsEnabled: Boolean @214, IsLipSyncEnabled: Boolean @215, AsyncClientBulletEntity: Boolean @216, IsPadRumbleEnabled: Boolean @217, IsEmittersDisabled: Boolean @218, IsHavokEmittersDisabled: Boolean @219, IsSoundEnabled: Boolean @220, Ps3OnDemandSoundInRsxEnable: Boolean @221, IsWaterPhysicsEnabled: Boolean @222, IsTerrainDisabled: Boolean @223, IsSootEnabled: Boolean @224, IsVariedVoiceOversEnabled: Boolean @225, IsOvergrowthEnabled: Boolean @226, IsSubtitlesEnabled: Boolean @227, EnableAudioTweakHack: Boolean @228, IsOnDemandSoundsEnabled: Boolean @229, IsUITracesEnabled: Boolean @230, InvertFreeCamera: Boolean @231, RenderTags: Boolean @232, DebugMenuOnLThumb: Boolean @233, BypassMatchmaking: Boolean @234, DebugMatchmaking: Boolean @235, InvertYaw: Boolean @236, IsInternetSimulationEnabled: Boolean @237, InvertPitch: Boolean @238, VisualFrameInterpolation: Boolean @239, IsFastStartupEnabled: Boolean @240, DisableMovies: Boolean @241, DisableHudMovies: Boolean @242, IsHudDisabled: Boolean @243, IsMenuDevOptsEnabled: Boolean @244, ScreenshotToFile: Boolean @245, IsPresenceSilent: Boolean @246, IsHintsEnabled: Boolean @247, IsPresenceEnabled: Boolean @248, DisableMenuFlow: Boolean @249, LoadMenu: Boolean @250 |
| DemoSettings | class | DataContainer | 40 | RecordDemoFileName: String @12, PlaybackDemoFileName: String @24, TimeDemo: Boolean @36, PauseOnStartup: Boolean @37 |
| GameRenderSettings | class | DataContainer | 164 | ViewDistance: Float32 @12, NearPlane: Float32 @16, EdgeModelScreenAreaScale: Float32 @20, ForceFov: Float32 @24, SkipMipmapCount: Uint32 @28, Ps3FrameLocalBufferSize: Uint32 @32, JobPriority: Uint32 @36, JobThreadIndex: Uint32 @40, EdgeModelDefaultMinScreenArea: Float32 @44, ForceBlurAmount: Float32 @48, StaticModelCullJobCount: Uint32 @52, ForceGlobalFadeAmount: Float32 @56, ForceWorldFadeAmount: Float32 @60, EdgeModelLodScale: Float32 @64, EdgeModelForceLod: Int32 @68, StaticModelPartOcclusionMaxScreenArea: Float32 @72, EdgeModelMaxVisibleInstanceCount: Uint32 @76, MultisampleQuality: Uint32 @80, XenonPresentImmediateThreshold: Uint32 @84, Dx9RefreshRate: Uint32 @88, Dx9AdapterIndex: Int32 @92, XenonRingBufferSize: Uint32 @96, MultisampleCount: Uint32 @100, Ps3FrameMainBufferSize: Uint32 @104, Renderer: SelectedRenderer @108, StaticModelPartCullEnable: Boolean @112, EdgeModelDrawStats: Boolean @113, StaticModelPartFrustumCullEnable: Boolean @114, EdgeModelOcclusionCullEnable: Boolean @115, StaticModelPartOcclusionCullEnable: Boolean @116, Dx10PlusEnable: Boolean @117, EdgeModelFrustumCullEnable: Boolean @118, EdgeModelCullEnable: Boolean @119, EdgeModelDrawBoxes: Boolean @120, Dx10Enable: Boolean @121, EdgeModelAdditionalCullEnable: Boolean @122, FadeWaitingEnable: Boolean @123, FadeEnable: Boolean @124, ResetLockedView: Boolean @125, BlurEnable: Boolean @126, ForegroundBlurEnable: Boolean @127, ShadowsEnable: Boolean @128, LockView: Boolean @129, StaticModelDrawStats: Boolean @130, StaticModelDrawBoxes: Boolean @131, StaticModelPartShadowCullEnable: Boolean @132, StaticModelCullSpuJobEnable: Boolean @133, Fullscreen: Boolean @134, ForceVSyncEnable: Boolean @135, BuildJobSyncEnable: Boolean @136, XenonBufferTwoFramesEnable: Boolean @137, JobEnable: Boolean @138, EntityRenderEnable: Boolean @139, EmittersEnable: Boolean @140, DecalsEnable: Boolean @141, DrawInfo: Boolean @142, GcmHudEnable: Boolean @143, Ps3CellMemoryTexturesEnable: Boolean @144, Ps3LinearFrameCmdBufEnable: Boolean @145, DrawShaderStats: Boolean @146, PerfOverlayVisible: Boolean @147, PerfOverlayEnable: Boolean @148, DebugRendererEnable: Boolean @149, EdgeModelSpuInstancingEnable: Boolean @150, Dx11Enable: Boolean @151, Dx11DebugInfoEnable: Boolean @152, EdgeModelUseLodBox: Boolean @153, Dx10DebugInfoEnable: Boolean @154, EdgeModelUseMainLodEnable: Boolean @155, EdgeModelCastShadowsEnable: Boolean @156, NvPerfHUDEnable: Boolean @157, GpuProfilerEnable: Boolean @158, InitialClearEnable: Boolean @159, EdgeModelsEnable: Boolean @160, CompositeMeshBatchingEnable: Boolean @161, RigidMeshBatchingEnable: Boolean @162 |
| GameSettings | class | DataContainer | 164 | svpunkbuster: Uint32 @12, clpunkbuster: Uint32 @16, TeamKillValueDecreasePerSecond: Uint32 @20, JobThreadCount: Uint32 @24, InputTapMaxTime: Float32 @28, BundleMountPoints: BundleMountPoint[] @32, LogHistory: Uint32 @48, OnlineListenPort: Uint32 @52, NoInteractivityTimeoutTime: Uint32 @56, PS3ContentRatingAge: Uint32 @60, NoInteractivityThresholdLimit: Float32 @64, TeamKillValueIncrease: Uint32 @68, TeamKillValueForKick: Uint32 @72, TeamKillCountForKick: Uint32 @76, MaxVariableFps: Float32 @80, MinSmoothVariableFps: Float32 @84, ForceDeltaTime: Float32 @88, MaxFps: Float32 @92, GameVersion: Uint32 @96, MaxPlayerCount: Uint32 @100, NumProcessors: Int32 @104, EnabledLanguages: String @108, InputConfiguration: String @120, InputHoldMinTime: Float32 @132, ForceDeltaTicks: Int32 @136, AdjustVehicleCenterOfMass: Boolean @140, IsBeta: Boolean @141, ToggleCrouch: Boolean @142, RotateLogs: Boolean @143, HoldToZoom: Boolean @144, UseCustomKit: Boolean @145, EnableDumpLevelChecksumOnDisconnect: Boolean @146, Ps3HDDGameDataEnable: Boolean @147, UseBundles: Boolean @148, EnableMemoryDumpStreamingOnFirstSpawn: Boolean @149, VsyncEnable: Boolean @150, LogFileEnable: Boolean @151, FastStartupEnable: Boolean @152, ResourceReloadingEnable: Boolean @153, ReplayDebug: Boolean @154, XenonJobThread5Enable: Boolean @155, AutoAimEnabled: Boolean @156, RetailServers: Boolean @157, XenonJobThread4Enable: Boolean @158, XenonJobThread1Enable: Boolean @159, XenonJobThread0Enable: Boolean @160, XenonJobThread3Enable: Boolean @161, XenonJobThread2Enable: Boolean @162 |
| GameSootSettings | class | DataContainer | 32 | ExpandAroundExplosionMin: Float32 @12, ExpandAroundExplosionMax: Float32 @16, ExpandAroundPartMin: Float32 @20, ExpandAroundPartMax: Float32 @24, DrawPartBounds: Boolean @28, SootForBulletsEnable: Boolean @29, SootForExplosionsEnable: Boolean @30 |
| Main | class | ApplicationBase | 1088 | - |
| MonitoringSettings | class | DataContainer | 44 | LogBufferSize: Uint32 @12, ConnectionRetryInterval: Uint32 @16, ReceiveTransactionTimeout: Uint32 @20, HubPort: Uint32 @24, HubAddress: String @28, Enable: Boolean @40, LogEnable: Boolean @41 |
| NetworkSettings | class | DataContainer | 164 | PlaybackDemoFileName: String @12, ClientConnectionDebugFilePrefix: String @24, RecordDemoFileName: String @36, MaxServerFrameSize: Uint32 @48, SingleplayerFrameHistoryTime: Float32 @52, PlasmaAccountPersona: String @56, MultiplayerFrameHistoryTime: Float32 @68, PlasmaAccountPassword: String @72, ServerConnectionDebugFilePrefix: String @84, PlasmaAccountName: String @96, MaxClientFrameSize: Uint32 @108, ClientPort: Uint32 @112, ServerPort: Uint32 @116, TitleId: Uint32 @120, Version: String @124, ProtocolVersion: Uint32 @136, MaxQueriesPerSecond: Uint32 @140, QueryPortOffset: Uint32 @144, MaxGhostCount: Uint32 @148, MaxClientCount: Uint32 @152, SkipEntitlementCheck: Boolean @156, TimeDemo: Boolean @157, UseFrameManager: Boolean @158, PlasmaAutoLogin: Boolean @159, PlasmaCreateAccount: Boolean @160 |
| PhysicsSettings | class | DataContainer | 32 | CollideJobCount: Uint32 @12, IntegrateJobCount: Uint32 @16, NumberOfThreads: Uint32 @20, HeightfieldRSXStreaming: Boolean @24, SingleStepCharacter: Boolean @25, EnableJobs: Boolean @26, EnableAIRigidBody: Boolean @27, ForestEnable: Boolean @28 |
| ServerSettings | class | DataContainer | 204 | DropSpikeChance: Float32 @12, PacketDrops: Float32 @16, MaxDropDuration: Float32 @20, MinDropDuration: Float32 @24, MaxLatency: Float32 @28, DifficultyMode: String @32, TimeoutTime: Float32 @44, MinLatency: Float32 @48, PlayerCountNeededForMultiplayer: Uint32 @52, ReorderingChance: Float32 @56, DedicatedServerCpu: Int32 @60, Playlist: String @64, FilterStatUploads: Uint32 @76, RemoteAdminPassword: String @80, OutgoingFrequency: Float32 @92, GameConfigurationTimeout: Float32 @96, DuplicationChance: Float32 @100, IngameTimeout: Float32 @104, LoadingTimeout: Float32 @108, SavePoint: String @112, Level: String @124, CorruptionChance: Float32 @136, DesertingAllowed: Boolean @140, DisableTelemetry: Boolean @141, DisableCheckpoints: Boolean @142, FilterProfanity: Boolean @143, AutoBalance: Boolean @144, IsHardcore: Boolean @145, UseNewVoip: Boolean @146, InstantSpawn: Boolean @147, ForcePlaylist: Boolean @148, IsGodMode: Boolean @149, IsReconfigurable: Boolean @150, IsWaterPhysicsEnabled: Boolean @151, RegulatedAIThrottle: Boolean @152, AutokillSoldiers: Boolean @153, AutoUnspawnBangers: Boolean @154, DisableGlobalStats: Boolean @155, DisableFinalize: Boolean @156, ThirdPersonVehicleCameras: Boolean @157, DisableGeneralStats: Boolean @158, IsFastStartupEnabled: Boolean @159, UseFakeAnimatedCollision: Boolean @160, DisableClientStats: Boolean @161, MinimapSpotting: Boolean @162, RemoteAdministration: Boolean @163, FriendlyFireEnabled: Boolean @164, IsRenderPhysicsWater: Boolean @165, KillCam: Boolean @166, ThreeDSpotting: Boolean @167, Crosshair: Boolean @168, Minimap: Boolean @169, IsJesusMode: Boolean @170, IsSpotWhenZoomedEnabled: Boolean @171, AllowDestructionOutsideCombatArea: Boolean @172, ShowHoldGroundFailures: Boolean @173, IsInternetSimulationEnabled: Boolean @174, IsTreeDestructionEnabled: Boolean @175, IsFragalyzerLogEnabled: Boolean @176, IsFragalyzerLocalLogEnabled: Boolean @177, AILooksIntoCamera: Boolean @178, HavokCaptureToFile: Boolean @179, HavokVisualDebugger: Boolean @180, DisableCutscenes: Boolean @181, ShowTriggerDebugText: Boolean @182, IsStaticModelsIterable: Boolean @183, LoadSavePoint: Boolean @184, TimeoutGame: Boolean @185, IsDestructionEnabled: Boolean @186, DrawActivePhysicsObjects: Boolean @187, LoopDuringLoadingJobEnable: Boolean @188, RunAfterClientEnable: Boolean @189, UnrestrictedUnlocks: Boolean @190, IsRenderDamageEvents: Boolean @191, HasUnlimitedAmmo: Boolean @192, ScoringLogEnabled: Boolean @193, JobEnable: Boolean @194, IsStatsEnabled: Boolean @195, IsNetworkStatsEnabled: Boolean @196, IsAiEnabled: Boolean @197, RespawnOnDeathPosition: Boolean @198, ThreadedLoadingEnable: Boolean @199, PreloadingEnabled: Boolean @200, HasUnlimitedMags: Boolean @201 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Game 12 records]

### RTTI-SCENE-1: `Scene` and `World`

Status: **verified** · Variants: PC

The scene (ENG-SCENE-1: `SceneData`, `SceneSettings`, `SceneObjectData` ...) and the world's static cull tree
(`StaticCullNode`, `StaticCullSettings`, CULL-*).

<!-- typeinfo:scene-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| WorldSubSection | value type | - | 16 | Max: Vec2 @0, Min: Vec2 @8 |
<!-- /typeinfo -->

<!-- typeinfo:scene-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| SceneData | class | WorldData | 140 | Settings: SceneSettings @124, StaticCullTree: StaticCullTreeData @128, InputController: SceneObjectData @132, CameraController: SceneObjectData @136 |
| SceneObjectData | class | EntityData | 128 | Visible: Boolean @112 |
| SceneSettings | class | DataContainer | 128 | CameraTransform: LinearTransform @16, RenderSettings: WorldRenderSettings @80, MouseSensitivityY: Float32 @84, EmitterSystemMetrics: EmitterSystemMetrics @88, MouseSensitivityX: Float32 @96, FieldOfView: Float32 @100, NearPlane: Float32 @104, ViewDistance: Float32 @108, CameraTranslationScale: Float32 @112, CameraForceHeight: Float32 @116, LockCamera: Boolean @120, ResetLockedCamera: Boolean @121, UseCamera: Boolean @122, CameraForceHeightEnable: Boolean @123 |
| WorldBuildView | class | Asset | 44 | Sections: WorldSubSection[] @24, Level: WorldData @40 |
<!-- /typeinfo -->

<!-- typeinfo:world-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| StaticCullNode | value type | - | 16 | Right: Uint32 @0, Max: Float32 @4, SphereKeyX: Int16 @8, SphereKeyY: Int16 @10, SphereKeyZ: Int16 @12, SphereKeyW: Int16 @14 |
<!-- /typeinfo -->

<!-- typeinfo:world-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| StaticCullSettings | class | DataContainer | 20 | MergePassCount: Uint32 @12, UseOldCullAgainstOcclusion: Boolean @16, Enable: Boolean @17 |
| StaticCullTreeData | class | DataContainer | 32 | EnumOffset: Uint32 @12, Nodes: StaticCullNode[] @16 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Scene 5, World 3 records]

## 2. Simulation

### RTTI-PHYS-1: `Physics`

Status: **verified** · Variants: PC

The physics module's 85 records: the character's physics (`CharacterPhysicsData`, its poses and its states:
`CharacterStateData` and its kinds, SOLD-PHYS-2), the vehicles' physics parts (engine, wheels, hull and float
physics, propellers, rotors, the input tweaks), the rigid and moving bodies, the proximity sensing, the material
sets and the node trees.

<!-- typeinfo:physics-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AabbData | value type | - | 32 | Max: Vec3 @0, Min: Vec3 @16 |
| AntiRollBars | value type | - | 8 | Rear: AntiRollBar @0, Front: AntiRollBar @4 |
| BaseHavokParams | value type | - | 28 | ExtraRadius: Float32 @12, ShapeType: String @16 |
| CharacterEntityPoseEnum | enumeration | - | 4 | Stand 0, Crouch 1, Prone 2 |
| CharacterPoseConstraintsData | value type | - | 3 | PronePose: Boolean @0, CrouchPose: Boolean @1, StandPose: Boolean @2 |
| CharacterRumbleData | value type | - | 16 | RumbleDuration: Float32 @0, RumbleImpulseClamp: Float32 @4, LowRumbleModifier: Float32 @8, HighRumbleModifier: Float32 @12 |
| EndPointData | value type | - | 20 | SpringAcceleration: Float32 @0, SpringDamping: Float32 @4, SpringLength: Float32 @8, Pos: Float32 @12, EndDamping: Float32 @16 |
| ForceMagnitudeInputType | enumeration | - | 4 | FMITYaw 0, FMITPitch 1, FMITRoll 2, FMITThrottle 3 |
| ImportShapeType | enumeration | - | 4 | IST_Box 0, IST_Sphere 1, IST_Capsule 2, IST_Cylinder 3, IST_Hull 4, IST_Mesh 5 |
| LookConstraintsData | value type | - | 16 | MinLookPitch: Float32 @0, MaxLookPitch: Float32 @4, MinLookYaw: Float32 @8, MaxLookYaw: Float32 @12 |
| MaterialSet | value type | - | 16 | Mappings: MaterialSetMapping[] @0 |
| MaterialSetMapping | value type | - | 28 | ReferencedMaterialIndex: Uint32 @0, MaterialName: String @4, MappingName: String @16 |
| NodeParameterObject | value type | - | 12 | Name: String @0 |
| NodeTreeNode | value type | - | 28 | NodeParams: NodeParameterObject[] @0, Name: String @16 |
| NodeTreeRoot | value type | - | 28 | Nodes: NodeTreeNode[] @0, Name: String @16 |
| PartInfoData | value type | - | 80 | Translation: Vec3 @0, Aabb: AabbData @16, Name: String @48, MaterialIndex: Uint32 @60, HashName: Int32 @64, MaterialName: String @68 |
| PhysicsMaterialMapping | value type | - | 24 | To: String @0, From: String @12 |
| PropellerType | enumeration | - | 4 | Regular 0, Rotor 1 |
| ProximityObjectType | enumeration | - | 4 | PotProximityDisabled 0, PotVaultableLow 1, PotVaultableHigh 2, PotSupportedShooting 3, PotInteractWith 4 |
| RigidBodyData | value type | - | 40 | Restitution: Float32 @0, ShapeDatas: ShapeData[] @4, Friction: Float32 @20, Name: String @24, Mass: Float32 @36 |
| SensitivityAtVelocity | value type | - | 8 | Velocity: Float32 @0, SteeringSensitivity: Float32 @4 |
| ShapeData | value type | - | 28 | ExtraRadius: Float32 @0, ShapeType: String @4, Name: String @16 |
| ShapeType | enumeration | - | 4 | STBox 0, STSphere 1, STCapsule 2, STCapsuleX 3, STCapsuleY 4, STCapsuleZ 5, STCylinder 6, STCylinderX 7, STCylinderY 8, STCylinderZ 9, ST_SIZE 10 |
| SphereCollisionData | value type | - | 12 | CounterNormalBrakeForceMod: Float32 @0, ExtraRadius: Float32 @4, Enabled: Boolean @8 |
| SpringData | value type | - | 28 | ProgressiveExponent: Float32 @0, VisualClipOffset: Float32 @4, AttachOffsetY: Float32 @8, ProgressiveStartRatio: Float32 @12, Length: Float32 @16, Stiffness: Float32 @20, Damping: Float32 @24 |
| VehicleInputData | value type | - | 80 | YawInertiaMinRatio: Float32 @0, PitchInertiaOutDuration: Float32 @4, YawInertiaInDuration: Float32 @8, BrakeInertiaMinRatio: Float32 @12, YawInertiaOutDuration: Float32 @16, RollInertiaInDuration: Float32 @20, RollInertiaMinRatio: Float32 @24, RollInertiaOutDuration: Float32 @28, PitchInertiaInDuration: Float32 @32, PitchInertiaMinRatio: Float32 @36, PitchDeadzone: Float32 @40, RollDeadzone: Float32 @44, YawDeadzone: Float32 @48, ThrottleDeadzone: Float32 @52, BrakeDeadzone: Float32 @56, BrakeInertiaOutDuration: Float32 @60, BrakeInertiaInDuration: Float32 @64, ThrottleInertiaMinRatio: Float32 @68, ThrottleInertiaOutDuration: Float32 @72, ThrottleInertiaInDuration: Float32 @76 |
| VehicleInputTweakType | enumeration | - | 4 | CombinedTimedSpeedTweakType 0, CombinedSpeedTweakType 1, SpeedTweakType 2, ScaledSpeedTweakType 3 |
| VehicleMode | enumeration | - | 4 | VmIdle 0, VmEntering 1, VmEntered 2, VmStarting 3, VmStarted 4, VmStopping 5, VmLeaving 6 |
<!-- /typeinfo -->

<!-- typeinfo:physics-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AeroDynamicPhysicsData | class | DataContainer | 80 | BodyDragOffsetXZ: Vec3 @16, BodyDragOffsetXY: Vec3 @32, BodyDrag: Vec3 @48, BodyDragOffsetYZ: Vec3 @64 |
| AnimationControlledStateData | class | CharacterStateData | 36 | UpNormalTolerance: Float32 @32 |
| AntiRollBar | class | DataContainer | 20 | TorqueActivationLimit: Float32 @12, Stiffness: Float32 @16 |
| BoatFloatPhysicsData | class | HullFloatPhysicsData | 160 | FrontRatio: Float32 @144 |
| BoxFloatPhysicsData | class | FloatPhysicsData | 20 | - |
| CharacterEntity | class | PhysicsEntityBase | 720 | - |
| CharacterPhysicsData | class | Asset | 144 | LadderAcceptAnglePitch: Float32 @24, JumpPenaltyTime: Float32 @28, InputAcceleration: Float32 @32, LadderAcceptAngle: Float32 @36, SpeedPenaltyOnDamage: Float32 @40, SpeedPenaltyOnDamageTime: Float32 @44, JumpPenaltyFactor: Float32 @48, Rumble: CharacterRumbleData @52, WaterDepthLimit: Float32 @68, Sprint: CharacterSprintData @72, Material: String @76, Poses: CharacterPoseData[] @88, States: CharacterStateData[] @108, MaxAscendAngle: Float32 @128, PhysicalRadius: Float32 @132, PushableObjectWeight: Int32 @136, Mass: Float32 @140 |
| CharacterPoseData | class | DataContainer | 80 | EyePosition: Vec3 @16, TransitionTime: Float32 @32, LookConstraints: LookConstraintsData @36, PoseType: CharacterEntityPoseEnum @52, ThrottleModifierCurve: Vec2[] @56, StepHeight: Float32 @72, Height: Float32 @76 |
| CharacterSprintData | class | DataContainer | 24 | SprintMinimumPower: Float32 @12, SprintPowerIncreasePerSecond: Float32 @16, SprintPowerDecreasePerSecond: Float32 @20 |
| CharacterStateData | class | DataContainer | 32 | PoseInfo: CharacterStatePoseInfo[] @12 |
| CharacterStatePoseInfo | class | DataContainer | 40 | StrafeDecelerationGain: Float32 @12, SprintGain: Float32 @16, SprintMultiplier: Float32 @20, DecelerationGain: Float32 @24, PoseType: CharacterEntityPoseEnum @28, Velocity: Float32 @32, AccelerationGain: Float32 @36 |
| ClimbingStateData | class | CharacterStateData | 56 | AttractionVelocity: Float32 @32, PushAwayVelocity: Float32 @36, ClimbHeightOffset: Float32 @40, LateralInputScale: Float32 @44, DownAngleLimit: Float32 @48, DropOffAngle: Float32 @52 |
| CombustionEngineConfigData | class | EngineConfigData | 112 | - |
| CompositeHavokAsset | class | Asset | 76 | Enums: Uint32[] @24, Assets: HavokAsset[] @40, Transforms: LinearTransform[] @60 |
| CompositePhysicsEntity | class | PhysicsEntityBase | 160 | - |
| Curve2D | class | DataContainer | 28 | Curve: Vec2[] @12 |
| EngineConfigData | class | DataContainer | 112 | Position: Vec3 @16, InternalDecFactor: Float32 @32, InternalAccFactor: Float32 @36, BoostStrengthModifier: Float32 @40, BoostRecovery: Float32 @44, BoostDuration: Float32 @48, EnginePowerMultiplier: Float32 @52, TorqueCurvePoints: Float32[] @56, RpmCurvePoints: Float32[] @72, RpmMin: Float32 @88, RpmCut: Float32 @92, RpmMax: Float32 @96 |
| FakeHingeData | class | FakePhysicsData | 128 | ExtensionAxis: Vec3 @48, RotationAxis: Vec3 @64, Pivot: Vec3 @80, PullbackAcceleration: Float32 @96, ProgressiveExponent: Float32 @100, InertiaModifier: Float32 @104, MinAngle: Float32 @108, MaxAngle: Float32 @112, AngularDampening: Float32 @116 |
| FakePhysicsData | class | DataContainer | 44 | MinSpeed: Float32 @12, EndDampening: Float32 @16, Mass: Float32 @20, MaxSpeed: Float32 @24, StartSpeed: Float32 @28, ChildFakePhysics: FakePhysicsData @32, StartDampening: Float32 @36, GravityModifier: Float32 @40 |
| FakeSpringData | class | FakePhysicsData | 80 | Direction: Vec3 @48, ProgressiveExponent: Float32 @64, Damping: Float32 @68, Length: Float32 @72, Acceleration: Float32 @76 |
| FallingStateData | class | CharacterStateData | 32 | - |
| FloatPhysicsData | class | DataContainer | 20 | FilledDensity: Float32 @12, Density: Float32 @16 |
| GearboxConfigData | class | DataContainer | 112 | OppositeDirGearChangeMaxSpeed: Float32 @12, GearDownSpeedFactor: Float32 @16, GearChangeTime: Float32 @20, TransmissionEfficiency: Float32 @24, ClutchSpeedFactor: Float32 @28, OppositeDirGearChangeTime: Float32 @32, ReverseGearRatios: Float32[] @36, ForwardGearSpeeds: Float32[] @52, ForwardGearRatios: Float32[] @68, GearboxMode: Int32 @84, GearboxType: Int32 @88, ReverseGearSpeeds: Float32[] @92, UseNeutralGear: Boolean @108, UseClassicGearBoxAutoClutch: Boolean @109, UseAutoClutch: Boolean @110 |
| HavokAsset | class | Asset | 200 | HkxDataFilePath: String @24, Mass: Float32 @36, RigidBodyDatas: RigidBodyData[] @40, DetailNode: String @56, CollisionNode: String @68, HkxImgFilePath: String @80, ParticleAllotment: Float32 @92, ReferencedMaterials: String[] @96, PhysicsMaterialSets: PhysicsMaterialSetAsset[] @112, MaterialMappings: String[] @132, MaterialSets: MaterialSet[] @148, FaceMaterials: String[] @164, PartInfoDatas: PartInfoData[] @180, IsStatic: Boolean @196, IsGroupable: Boolean @197, IsDynamic: Boolean @198 |
| HavokPhysicsData | class | - | 84 | - |
| HovercraftFloatPhysicsData | class | HullFloatPhysicsData | 192 | LandFrictionAxisMod: Vec3 @144, LandResistanceAxisMod: Vec3 @160, SideLength: Float32 @176, FrontLength: Float32 @180 |
| HullFloatPhysicsData | class | FloatPhysicsData | 144 | Offset: Vec3 @32, WaterFrictionAxisMod: Vec3 @48, WaterResistanceAxisMod: Vec3 @64, WaterDampeningMod: Float32 @80, NonEngineSteerMaxSpeed: Float32 @84, NonEngineSteerMinSpeed: Float32 @88, LiftModifier: Float32 @92, FrictionThrottleModifier: Float32 @96, AngularDampening: Float32 @100, SupportSizeMod: Float32 @104, Width: Float32 @108, Depth: Float32 @112, SubSurfaceSplits: Int32 @116, Length: Float32 @120, NonEngineSteer: Float32 @124, SideCurveDegree: Float32 @128, FrontCurveDegree: Float32 @132 |
| InAirStateData | class | CharacterStateData | 36 | FreeFallVelocity: Float32 @32 |
| JetEngineConfigData | class | EngineConfigData | 160 | PowerFadeOutRange: Vec2 @112, PitchAngleOffset: Float32 @120, MaxVelocity: Float32 @124, DirectionVectorIndex: Uint32 @128, ForceMagnitudeMultiplier: Float32 @132, AngleInputYMultiplier: Float32 @136, AngleInputPitchMultiplier: Float32 @140, IsTurnable: Boolean @144, IsWaterJetEngine: Boolean @145 |
| JumpStateData | class | CharacterStateData | 40 | JumpEffectSize: Float32 @32, JumpHeight: Float32 @36 |
| LinearMovingBodyData | class | MovingBodyData | 96 | Axis: Vec3 @32, End: EndPointData @48, Start: EndPointData @68 |
| LoosePartPhysicsData | class | DataContainer | 16 | FakePhysics: FakePhysicsData @12 |
| MaterialPropertyPhysicsData | class | MaterialRelationPropertyData | 28 | RestitutionModifier: Float32 @16, StaticFrictionModifier: Float32 @20, DynamicFrictionModifier: Float32 @24 |
| MotionDampingData | class | DataContainer | 48 | LinearModifier: Vec3 @16, Roll: Float32 @32, Linear: Float32 @36, Pitch: Float32 @40, Yaw: Float32 @44 |
| MovingBodyData | class | DataContainer | 20 | Inertia: Float32 @12, Mass: Float32 @16 |
| NodeTreeAsset | class | Asset | 40 | RootNodes: NodeTreeRoot[] @24 |
| OnGroundStateData | class | CharacterStateData | 48 | AllowedDistanceFromGround: Float32 @32, JumpStaminaPenalty: Float32 @36, JumpDelay: Float32 @40, LimitStrafeVelocity: Boolean @44, LimitDownwardVelocity: Boolean @45, GroundHugging: Boolean @46 |
| ParachuteStateData | class | CharacterStateData | 40 | VerticalGain: Float32 @32, VerticalVelocity: Float32 @36 |
| PhysicsEntity | class | PhysicsEntityBase | 192 | - |
| PhysicsEntityBase | class | Entity | 64 | - |
| PhysicsEntityData | class | DataContainer | 64 | InertiaModifier: Vec3 @16, Restitution: Float32 @32, Friction: Float32 @36, Proximity: ProximityData @40, Asset: HavokAsset @44, FloatPhysics: FloatPhysicsData @48, Mass: Float32 @52, AllowZeroMayaMass: Boolean @56 |
| PhysicsMaterialSetAsset | class | Asset | 40 | MaterialMappings: PhysicsMaterialMapping[] @24 |
| PhysicsReportingAsset | class | Asset | 28 | ReportCount: Uint32 @24 |
| PropellerEngineConfigData | class | EngineConfigData | 272 | HorisontalForceOffset: Vec3 @112, SPSidewaysStrength: Float32 @128, SPForwardStrength: Float32 @132, PitchStrength: Float32 @136, SPVerticalStrength: Float32 @140, StabilizerMod: Float32 @144, AttackAngleMod: Float32 @148, HorisontalMinEffectMod: Float32 @152, HorisontalMinEffectVelocity: Float32 @156, PitchLimit: Float32 @160, PitchUpWhenBankStrength: Float32 @164, BankingLimit: Float32 @168, GravityMod: Float32 @172, PitchUpWhenBankLimit: Float32 @176, VelocityFromPitch: Float32 @180, PitchFromVel: Float32 @184, BankingStrength: Float32 @188, RollStrength: Float32 @192, DefaultAngleOfAttack: Float32 @196, LiftForceDampingConstant: Float32 @200, LiftForceSpringConstant: Float32 @204, CyclicInputScaleRoll: Float32 @208, CyclicRollStrafeMod: Float32 @212, CyclicRollLiftMod: Float32 @216, RotorConfig: RotorParameters @220, PropellerType: PropellerType @224, ForceMagnitudeInputType: ForceMagnitudeInputType @228, ForceMagnitudeMultiplier: Float32 @232, DirectionVectorIndex: Uint32 @236, CyclicPitchStrafeBrakeMod: Float32 @240, CollectiveBrakeInputScale: Float32 @244, CollectiveThrottleInputScale: Float32 @248, CollectiveInputIdle: Float32 @252, CyclicInputScalePitch: Float32 @256, CyclicPitchLiftMod: Float32 @260, CyclicPitchStrafeMod: Float32 @264, SPAllowed: Boolean @268, EnableNewHelicopter: Boolean @269, SPDefault: Boolean @270 |
| ProximityData | class | DataContainer | 16 | ProximityType: ProximityObjectType @12 |
| RagdollAsset | class | Asset | 28 | UseServerRagdoll: Boolean @24 |
| RagdollEntity | class | PhysicsEntityBase | 320 | - |
| RagdollResource | class | - | 32 | - |
| RotationBodyData | class | MovingBodyData | 52 | AngularConstraintMin: Float32 @20, AngularConstraintMax: Float32 @24, RotationAxis: Int32 @28, AngularMomentumDampingDeacc: Float32 @32, AngularMomentumThreshold: Float32 @36, AngularMomentumDampingAcc: Float32 @40, AngularMomentumMultiplier: Float32 @44, UseAngularMomentumDamping: Boolean @48, UseAngularConstraint: Boolean @49, UsePostSatisfyConstraints: Boolean @50, UseAngularMomentumThreshold: Boolean @51 |
| RotorParameters | class | DataContainer | 60 | AngleOfAttack: Vec2[] @12, HorizontalForceModifier: Float32 @28, AdditionalGravityModifier: Float32 @32, CyclicFadeOutOffset: Float32 @36, CollectiveInputIdle: Float32 @40, CyclicInputScalePitch: Float32 @44, CyclicInputScaleRoll: Float32 @48, CollectiveBrakeInputScale: Float32 @52, CollectiveThrottleInputScale: Float32 @56 |
| SlidingStateData | class | CharacterStateData | 40 | GravityScale: Float32 @32, HorizontalInputScale: Float32 @36 |
| StabilizerData | class | DataContainer | 32 | YawStrength: Float32 @12, VerticalVelocityStrength: Float32 @16, PitchStrength: Float32 @20, RollStrength: Float32 @24, StabilizeUnoccupiedVehicle: Boolean @28, Advanced: Boolean @29, AdvancedYaw: Boolean @30 |
| SwimmingStateData | class | CharacterStateData | 36 | BodyUnderWater: Float32 @32 |
| VehicleConfigData | class | DataContainer | 272 | InertiaModifier: Vec3 @16, CenterOfMassHandlingOffset: Vec3 @32, CenterOfMass: Vec3 @48, StaticFrictionBreakCollisionMod: Float32 @64, StaticFrictionBreakVelocityMod: Float32 @68, CoefficientOfAirFriction: Float32 @72, VehicleModeChangeStoppingTime: Float32 @76, VehicleModeChangeLeavingTime: Float32 @80, StandStillLowSpeedTimeLimit: Float32 @84, AirDensity: Float32 @88, WindResistanceVelocityFactorMin: Float32 @92, WindResistanceVelocityFactorMax: Float32 @96, AntiRollBars: AntiRollBars @100, AirDragArea: Float32 @108, WindResistanceBaseFactor: Float32 @112, WindResistanceVelocityFactor: Float32 @116, VehicleModeChangeStartingTime: Float32 @120, FloatPhysics: FloatPhysicsData @124, Stabilizer: StabilizerData @128, VehicleModeAtReset: VehicleMode @132, AeroDynamicPhysics: AeroDynamicPhysicsData @136, MotionDamping: MotionDampingData @140, Input: VehicleInputData @144, BodyMass: Float32 @224, DownForceBaseFactor: Float32 @228, DownForceWheelFactor: Float32 @232, VehicleModeChangeEnteringTime: Float32 @236, GravityModifier: Float32 @240, YawMin: Float32 @244, YawMax: Float32 @248, UseMotorcycleControl: Boolean @252, InvertPitchAllowed: Boolean @253, UseGearbox: Boolean @254, UseWindResistance: Boolean @255, UseTurnAroundForce: Boolean @256, UseDownForceWheelFactor: Boolean @257, UseDownForce: Boolean @258, UseStandStillSleep: Boolean @259, UseStandStillBrake: Boolean @260 |
| VehicleInputTweakData | class | DataContainer | 32 | MinSpeedScale: Float32 @12, MaxSpeedScale: Float32 @16, MaxSpeed: Float32 @20, InputTweakType: VehicleInputTweakType @24, MinSpeed: Float32 @28 |
| WheelConfigData | class | DataContainer | 352 | Inertia: Vec3 @16, Offset: Vec3 @32, BrakeFactor: Float32 @48, FrictionMomentMaxFactor: Float32 @52, FrictionMomentMultiplier: Float32 @56, BrakeForce: Float32 @60, LateralPosK: Float32 @64, HandBrakeForce: Float32 @68, HandBrakeFactor: Float32 @72, FrictionMomentVelocityMax: Float32 @76, WheelSlipRatioMaxSlipCondition: Float32 @80, WheelVelocityXMinSlipCondition: Float32 @84, WheelVelocityZMinSlipCondition: Float32 @88, LongitudinalFrictionForceMaxFactor: Float32 @92, FrictionMomentVelocityMin: Float32 @96, FxTorqueRadiusMultiplier: Float32 @100, LateralFrictionForceMaxFactor: Float32 @104, SteeringAngleIndex: Int32 @108, RotationDirectionIndex: Int32 @112, FrictionMethod: Int32 @116, PacejkaConfigIndex: Int32 @120, CollisionMaterial: String @124, AckermanDeviceType: Int32 @136, EngineIndex: Int32 @140, SteeringType: Int32 @144, LongitudinalNegK: Float32 @148, LongitudinalPosK: Float32 @152, LateralNegK: Float32 @156, AlignMomScale: Float32 @160, DrivingType: Int32 @164, WheelBaseLongitudinal: Float32 @168, WheelBaseLateral: Float32 @172, EngineBrakeMinFactor: Float32 @176, EngineBrakeVelocityFactor: Float32 @180, RollingResistanceVelocityFactor: Float32 @184, EngineBrakeMaxFactor: Float32 @188, MidSpeedSteeringSensitivity: Float32 @192, LowSpeedSteeringSensitivity: Float32 @196, DiffGearRatio: Float32 @200, RollingResistanceBaseFactor: Float32 @204, Spring: SpringData @208, SphereCollision: SphereCollisionData @236, RotationBody: RotationBodyData @248, Mass: Float32 @252, RollingResistanceInternalBaseFactor: Float32 @256, Width: Float32 @260, Radius: Float32 @264, SlopeGripMaxAngle: Float32 @268, SlopeGripMinAngle: Float32 @272, SteeringSensitivity: SensitivityAtVelocity[] @276, SlopeGripExponent: Int32 @292, AngularVelocityMinSlipCondition: Float32 @296, SideSlipAngleMaxSlipCondition: Float32 @300, OffGroundGravityModifier: Float32 @304, SteerInertia: Curve2D @308, LowSpeedSteeringSensitivityLimit: Float32 @312, SensitivityRangeScale: Float32 @316, HighSpeedSteeringSensitivity: Float32 @320, MidSpeedSteeringSensitivityLimit: Float32 @324, TrackedForwardSpeedLimit: Float32 @328, TrackedTurnSpeedLimit: Float32 @332, HighSpeedSteeringSensitivityLimit: Float32 @336, UseLowSpeedAutoBrake: Boolean @340, UseFrictionMoment: Boolean @341, UseRollingResistanceBaseFactor: Boolean @342, AdjustWheelRotation: Boolean @343, UseEngineBrake: Boolean @344, UseRollingResistanceVelocityFactor: Boolean @345, HasSteeringInverted: Boolean @346, IsAllowedToSpin: Boolean @347 |
| WingPhysicsData | class | DataContainer | 56 | InputForFlap: Int32 @12, BaseAngleOfAttack: Float32 @16, FlapTurnSpeed: Float32 @20, MaxFlapAngleScaleFactor: Curve2D @24, VisualFlapTurnSpeed: Float32 @28, DragCoefficient: Curve2D @32, FlapLift: Float32 @36, Lift: Float32 @40, LiftCoefficient: Curve2D @44, FlapDrag: Float32 @48, Drag: Float32 @52 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Physics 85 records]

### RTTI-ANIM-1: `Animation`

Status: **verified** · Variants: PC

The animation module's 124 records: the anim tree's node data classes (the 26 node types of ATREE-NODE-1 are their
run-time side), its groups and parameters, the skeletons and animation assets, the aim and hand IK data, the
movement blend nodes, the expressions.

<!-- typeinfo:animation-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AimControlData | value type | - | 208 | AnimationLimitLeft: Vec3 @0, AnimationLimitUp: Vec3 @16, AnimationLimitRight: Vec3 @32, AnimationLimitDown: Vec3 @48, ExponentialDelay: Float32 @64, LimitADown: Float32 @68, LimitAUp: Float32 @72, AimStrength: Float32 @76, LodProceduralCorrection: LodInterval @80, PrimaryBonesProcCount: Uint32 @88, PrimaryInfluenceMapBonesProcCount: Uint32 @92, AimDirections: Vec3[] @96, LodNear: LodInterval @112, LodFar: LodInterval @120, PrimaryAnimationBones: Uint32[] @128, MinPrimaryAnimationBone: Uint32 @144, PrimaryBones: Uint32[] @148, InfluenceMap: InfluenceMapData @164, PositionAnimatedBones: InfluenceMapData @168, LimitALeft: Float32 @172, LimitARight: Float32 @176, Animation: AimAnimationAsset @180, AnimationFramesPerRow: Int32 @184, AimBones: AimNode2AimBoneData[] @188 |
| AnimatedPropertyData | value type | - | 24 | ValueOn: Float32 @0, ValueOff: Float32 @4, BoneName: String @8, DataIndex: Uint32 @20 |
| AnimationMotionType | enumeration | - | 4 | AMT_CONST_SPEED 1, AMT_REMOVE_MOTION 2, AMT_WORLDSPACE 4, AMT_LOCALSPACE 8, AMT_LOCALSPACE2D 16 |
| AnimationParamType | enumeration | - | 4 | APT_Bool 0, APT_Int 1, APT_Float 2, APT_Vec3 3, APT_Mat4 4, APT_Count 5 |
| AnimationSettings | value type | - | 6 | PreferSPUSampling: Boolean @0, EnableFeetIk: Boolean @1, EnableHandsIk: Boolean @2, EnableStats: Boolean @3, EnableJobs: Boolean @4, EnableFullCulling: Boolean @5 |
| BaseAnimationControlData | value type | - | 12 | NodeIndex: Uint32 @0, InfluenceMap: InfluenceMapData @4, AnimationIndex: Uint32 @8 |
| ComparisonType | enumeration | - | 4 | CT_Equals 0, CT_NotEquals 1, CT_Less 2, CT_More 3, CT_LessEqual 4, CT_MoreEqual 5 |
| DisableInterval | value type | - | 16 | DisableTime2: Float32 @0, EnableTime2: Float32 @4, EnableTime1: Float32 @8, DisableTime1: Float32 @12 |
| DisablePropertyCurve | value type | - | 28 | PropertyName: String @0, DisableIntervals: DisableInterval[] @12 |
| HandIkInfo | value type | - | 80 | LocalPosition: LinearTransform @0, HandIndex: Int32 @64 |
| HandInfoData | value type | - | 56 | HandInfluenceMap: InfluenceMapData @0, BoneIndices: Int32[] @4, HandBoneName: String @20, HandIkProperty: AnimatedPropertyData @32 |
| LegInfoData | value type | - | 96 | LegBone2Name: String @0, LegIkProperty: AnimatedPropertyData @12, BoneIndices: Int32[] @36, LegBone1LowestBones: Vec3[] @52, LegBone2LowestBones: Vec3[] @68, LegBone1Name: String @84 |
| LimbCount | enumeration | - | 4 | LC_MAX_HAND_COUNT 2, LC_MAX_LEG_COUNT 2 |
| LodInterval | value type | - | 8 | Min: Float32 @0, Max: Float32 @4 |
| NodeAligmentData | value type | - | 36 | Size: Uint32 @0, Fields: NodeFieldAligmentData[] @4, NodeType: Uint32 @20, Name: String @24 |
| NodeFieldAligmentData | value type | - | 20 | Name: String @0, Offset: Uint32 @12, Size: Uint32 @16 |
| ParameterTypeGroupData | value type | - | 20 | Parameters: BaseParameter[] @0 |
| PropertyCurveType | enumeration | - | 4 | PCT_NO_EFFECT 0, PCT_ALWAYS_ON 1, PCT_ALWAYS_OFF 2, PCT_ANIMATED 3, PCT_MANUAL_INPUT 4 |
| ReplacementAnimationPairData | value type | - | 8 | ReplacementAnimation: BaseAnimationAsset @0, Animation: BaseAnimationAsset @4 |
| SkeletonPoseData | value type | - | 48 | Rotation: Vec3 @0, Translation: Vec3 @16, Name: String @32 |
| TimeSynchronizationType | enumeration | - | 4 | TST_SYNCH 0, TST_TRANSLATION 1, TST_ROTATION 2 |
| TripleBool | enumeration | - | 4 | TB_FALSE 0, TB_TRUE 1, TB_NONE 2 |
<!-- /typeinfo -->

<!-- typeinfo:animation-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AbstractMovementNodeData | class | BaseNodeData | 88 | MovementAngleParameter: FloatParameter @84 |
| AimAnimation | class | - | 20 | - |
| AimAnimationAsset | class | BaseAnimationAsset | 28 | - |
| AimNode2AimBoneData | class | AnimNodePortGroupData | 176 | AimBoneWorldRotation: LinearTransform @16, AimBoneIndex: Int32 @80, AimPositionBoneBaseTransforms: Vec3[] @84, AimPositionBoneIndex: Int32 @100, AimDirections: Vec3[] @104, AimPositionBaseBoneIndex: Int32 @120, AimPositionBoneName: String @124, AimPositionBaseBoneName: String @136, AimBoneName: String @148, CorrectDirection: Boolean @160, AlwaysAnimate: Boolean @161 |
| AimNode2Data | class | BaseNodeData | 148 | LodFar: LodInterval @84, LodNear: LodInterval @92, AimAnimation: AimAnimationAsset @100, LodProceduralCorrection: LodInterval @104, AimStrength: Float32 @112, AimTargetPositionParameter: Vec3Parameter @116, AimBones: AimNode2AimBoneData[] @120, ExponentialDelay: Float32 @140, PositionAnimatedBones: InfluenceMapData @144 |
| AlwaysNodeGroupData | class | AnimNodePortGroupData | 16 | NodeData: BaseNodeData @12 |
| AndExpressionData | class | CompositeExpressionData | 32 | - |
| AnimNodePortGroupData | class | DataContainer | 12 | - |
| AnimTreeData | class | AnimTreeDataAsset | 140 | AbsoluteTimeParameter: FloatParameter @56, Skeleton: SkeletonAsset @60, DeltaTimeParameter: FloatParameter @64, PoseParameter: IntParameter @68, RootNode: BaseNodeData @72, NodeList: BaseNodeData[] @76, ParameterGroupData: ParameterGroupData @96, ReplacementAnimationPairs: ReplacementAnimationPairData[] @100, Preprocessors: BasePreprocessorData[] @116, IgnorePriority: Boolean @136 |
| AnimTreeDataAsset | class | Editable | 56 | BlendInTime: Float32 @24, BlendOutTime: Float32 @28, ResultAnimTrees: ResultAnimTreeData[] @32, Priority: Int32 @52 |
| AnimTreeInfo | class | - | 36 | - |
| AnimTreeReplacementData | class | Asset | 120 | AnimationList: BaseAnimationAsset[] @24, Controls: BaseAnimationControlData[] @44, ControlMap: Int32[] @60, AnimTree: AnimTreeData @76, ReplacementAnimations: BaseAnimationAsset[] @80, Nodes: AnimationNodeData[] @100 |
| AnimTreeReplacementInfo | class | - | 20 | - |
| AnimationAsset | class | BaseAnimationAsset | 32 | MotionType: AnimationMotionType @28 |
| AnimationExportQuality | class | Asset | 60 | OrientationTolerance: QualityMapData @24, PositionTolerance: QualityMapData @28, OrientationJumpTolerance: QualityMapData @32, PositionJumpTolerance: QualityMapData @36, DefaultOrientationTolerance: Float32 @40, JointOversampling: QualityMapData @44, Oversampling: Int32 @48, DefaultPositionTolerance: Float32 @52, Uncompressed: Boolean @56 |
| AnimationNodeData | class | BaseNodeData | 136 | WeaponIkAnimation: PropertyCurveType @84, WeaponIkDelta: PropertyCurveType @88, HandsIk: PropertyCurveType @92, AimingManualInput: DisablePropertyCurve_ManualInput @96, Aiming: PropertyCurveType @100, LegsIk: PropertyCurveType @104, IsCyclic: TripleBool @108, ScaledAnimationTimeParameter: FloatParameter @112, Animation: BaseAnimationAsset @116, NormalizedTimeParameter: FloatParameter @120, CallbackId: Int32 @124, PlayFromBegining: TripleBool @128, FixLegsDistanceFromGround: Boolean @132 |
| AutoEndingEnumeratedSwitchNodeData | class | BaseNodeData | 116 | NodeSelectorParameter: IntParameter @84, ElseNode: BaseNodeData @88, Nodes: AutoEndingEnumeratedSwitchNodeGroupData[] @92, AnimationStartParameter: BoolParameter @112 |
| AutoEndingEnumeratedSwitchNodeGroupData | class | AnimNodePortGroupData | 16 | NodeData: BaseNodeData @12 |
| AutoEndingLoopNodeData | class | BaseNodeData | 100 | Parameter: BoolParameter @84, ElseNode: BaseNodeData @88, Node: BaseNodeData @92, RestartAnimation: Boolean @96 |
| BaseAnimation | class | - | 68 | - |
| BaseAnimationAsset | class | Asset | 28 | Skeleton: SkeletonAsset @24 |
| BaseExpressionData | class | DataContainer | 12 | - |
| BaseHandsIkNodeData | class | BaseNodeData | 120 | HandsIkInfo: HandIkInfo[] @84, Pose: AnimationNodeData @100, DisabledHandsIk: String[] @104 |
| BaseInfluenceMapData | class | Asset | 48 | ResultInfluenceMaps: ResultInfluenceMapData[] @24, Skeleton: BaseSkeletonAsset @44 |
| BaseNodeData | class | DataContainer | 84 | ParentNodeIndex: Int32 @12, NodeIndex: Uint32 @16, BlendOutTime: Float32 @20, Skeletons: SkeletonAsset[] @24, InfluenceMapData: InfluenceMapData @44, InfluenceMapEffects: InfluenceMapEffectData[] @48, BlendInTime: Float32 @68, NodeWeightParameter: FloatParameter @72, OriginalInfluenceMap: InfluenceMapData @76, EnabledBySkeleton: Boolean @80 |
| BaseParameter | class | DataContainer | 28 | DataIndex: Uint32 @12, Name: String @16 |
| BasePreprocessorData | class | DataContainer | 12 | - |
| BaseSkeletonAsset | class | Asset | 24 | - |
| BlendShapeAnimationAsset | class | Asset | 48 | RootNode: String @24, SourceFile: FileRef @36 |
| BlendShapeSkeletonAsset | class | Asset | 48 | RootNode: String @24, SourceFile: FileRef @36 |
| BonePriority | class | DataContainer | 44 | BoneIndex: Uint32 @12, EualizeToBoneIndex: Uint32 @16, BoneName: String @20, EualizeToBoneName: String @32 |
| BoneToBoneIkNodeData | class | BaseNodeData | 208 | LocalPosition: LinearTransform @96, TargetBoneIndex: Uint32 @160, BoneIndex: Uint32 @164, Pose: BaseAnimationAsset @168, BoneName: String @172, TargetBoneName: String @184 |
| BoolNodeGroupData | class | AnimNodePortGroupData | 20 | Parameter: BoolParameter @12, NodeData: BaseNodeData @16 |
| BoolParameter | class | BaseParameter | 32 | InitialValue: Boolean @28 |
| BoolParameterExpressionData | class | BaseExpressionData | 20 | Parameter: BoolParameter @12, Value: Boolean @16 |
| BoolValueGroupData | class | AnimNodePortGroupData | 20 | Parameter: BoolParameter @12, Invert: Boolean @16 |
| CompositeExpressionData | class | BaseExpressionData | 32 | SubExpressions: BaseExpressionData[] @12 |
| CompositeSkeletonAsset | class | BaseSkeletonAsset | 44 | Skeletons: SkeletonAsset[] @24 |
| DeltaAnimation | class | BaseAnimation | 84 | - |
| DeltaAnimationAsset | class | BaseAnimationAsset | 36 | ReferenceFrame: Uint32 @28, ReferencePose: BaseAnimationAsset @32 |
| DisablePropertyCurve_ManualInput | class | DataContainer | 12 | - |
| DisablePropertyCurve_ManualInput_EndsAbsoluteTime | class | DisablePropertyCurve_ManualInput | 28 | DisableInterval: DisableInterval @12 |
| DisablePropertyCurve_ManualInput_NormalizedTime | class | DisablePropertyCurve_ManualInput | 28 | DisableIntervals: DisableInterval[] @12 |
| DisablePropertyCurve_ManualInput_OrAnimated | class | DisablePropertyCurve_ManualInput | 16 | Curve: DisablePropertyCurve_ManualInput @12 |
| EnumeratedSwitchNodeData | class | BaseNodeData | 132 | NodeSelectorParameter: IntParameter @84, DisableTransitionsParameter: BoolParameter @88, Nodes: EnumeratedSwitchNodeGroupData[] @92, Transitions: TransitionInfoData[] @112 |
| EnumeratedSwitchNodeGroupData | class | AnimNodePortGroupData | 16 | NodeData: BaseNodeData @12 |
| FloatNodeGroupData | class | AnimNodePortGroupData | 28 | Max: Float32 @12, Min: Float32 @16, NodeData: BaseNodeData @20, Parameter: FloatParameter @24 |
| FloatParameter | class | BaseParameter | 32 | InitialValue: Float32 @28 |
| FloatParameterExpressionData | class | BaseExpressionData | 24 | Comparison: ComparisonType @12, Value: Float32 @16, Parameter: FloatParameter @20 |
| FloatValueGroupData | class | AnimNodePortGroupData | 28 | Min: Float32 @12, Max: Float32 @16, Parameter: FloatParameter @20, Invert: Boolean @24 |
| GenericHandsIkNodeData | class | BaseHandsIkNodeData | 148 | ObjectPose: AnimationAsset @120, PoseFrame: Uint32 @124, AnimationPose: AnimationAsset @128, TargetObjectWorldPosition: Mat4Parameter @132, TargetObjectName: String @136 |
| GrannyAnimation | class | BaseAnimation | 68 | - |
| GrannyModel | class | - | 24 | - |
| GroupNodeData | class | BaseNodeData | 104 | Nodes: GroupNodeGroupData[] @84 |
| GroupNodeGroupData | class | AnimNodePortGroupData | 16 | Node: BaseNodeData @12 |
| IfNodeData | class | BaseNodeData | 156 | ThenNode: BaseNodeData @84, ElseNode: BaseNodeData @88, Expression: BaseExpressionData @92, BoolValues: BoolValueGroupData[] @96, FloatValues: FloatValueGroupData[] @116, IntValues: IntValueGroupData[] @136 |
| InfluenceMapBoneData | class | DataContainer | 28 | Weight: Float32 @12, BoneName: String @16 |
| InfluenceMapData | class | BaseInfluenceMapData | 72 | InfluenceMapEffects: InfluenceMapEffectData[] @48, Dynamic: Boolean @68 |
| InfluenceMapEffectData | class | DataContainer | 20 | EffectToInfluenceMap: InfluenceMapData @12, InfluenceMap: InfluenceMapData @16 |
| IntNodeGroupData | class | AnimNodePortGroupData | 28 | Max: Int32 @12, Min: Int32 @16, NodeData: BaseNodeData @20, Parameter: IntParameter @24 |
| IntParameter | class | BaseParameter | 32 | InitialValue: Int32 @28 |
| IntParameterExpressionData | class | BaseExpressionData | 24 | Comparison: ComparisonType @12, Value: Int32 @16, Parameter: IntParameter @20 |
| IntValueGroupData | class | AnimNodePortGroupData | 28 | Min: Int32 @12, Max: Int32 @16, Parameter: IntParameter @20, Invert: Boolean @24 |
| LerpNodeData | class | BaseNodeData | 104 | MinValue: Float32 @84, MaxValue: Float32 @88, LerpParameter: FloatParameter @92, Node1: BaseNodeData @96, Node2: BaseNodeData @100 |
| Mat4Parameter | class | BaseParameter | 96 | InitialValue: Mat4 @32 |
| Movement4WayNodeData | class | AbstractMovementNodeData | 104 | LeftNormal: BaseNodeData @88, RightNormal: BaseNodeData @92, Forward: BaseNodeData @96, Backward: BaseNodeData @100 |
| MovementBlendNodeData | class | AbstractMovementNodeData | 108 | Nodes: MovementBlendNodeGroupData[] @88 |
| MovementBlendNodeGroupData | class | AnimNodePortGroupData | 20 | WeightParameter: FloatParameter @12, Animation: BaseNodeData @16 |
| MovementNodeData | class | AbstractMovementNodeData | 116 | RightNormal: BaseNodeData @88, RightInverse: BaseNodeData @92, FrontParameter: BoolParameter @96, LeftInverse: BaseNodeData @100, Forward: BaseNodeData @104, Backward: BaseNodeData @108, LeftNormal: BaseNodeData @112 |
| NodesAligmentHelperAsset | class | Asset | 40 | Nodes: NodeAligmentData[] @24 |
| NotExpressionData | class | BaseExpressionData | 16 | SubExpression: BaseExpressionData @12 |
| OrExpressionData | class | CompositeExpressionData | 32 | - |
| ParameterGroupData | class | DataContainer | 48 | ParametersTypeGroups: ParameterTypeGroupData[] @12, Parameters: BaseParameter[] @28 |
| PingPongNodeData | class | BaseNodeData | 100 | TransitionNodeData: BaseNodeData @84, StateParameter: FloatParameter @88, StateANodeData: BaseNodeData @92, StateBNodeData: BaseNodeData @96 |
| ProceduralLeaningNodeData | class | BaseNodeData | 224 | MaxAccelerationTranslation: Vec3 @96, MaxTurnTranslation: Vec3 @112, UphillMaxTranslation: Vec3 @128, DownhillMaxTranslation: Vec3 @144, AccelerationLimit: Float32 @160, MaxAccelerationRotation: Float32 @164, DownhillMinSlope: Float32 @168, AccelerationLeaningTime: Float32 @172, MinRotationSpeed: Float32 @176, MaxRotationSpeed: Float32 @180, MaxTurnRotation: Float32 @184, DownhillMaxSlope: Float32 @188, RotationSpeedParameter: FloatParameter @192, MovementAngleParameter: FloatParameter @196, MovementSpeedParameter: FloatParameter @200, UphillMaxRotation: Float32 @204, DownhillMaxRotation: Float32 @208, UphillMinSlope: Float32 @212, UphillMaxSlope: Float32 @216 |
| QualityMapData | class | BaseInfluenceMapData | 48 | - |
| RagdollNodeData | class | BaseNodeData | 84 | - |
| RandomSelectorGroupData | class | AnimNodePortGroupData | 24 | UpperTime: Float32 @12, LowerTime: Float32 @16, NodeData: BaseNodeData @20 |
| RandomSelectorMainGroupData | class | AnimNodePortGroupData | 28 | UpperTime: Float32 @12, RelativeValue: Int32 @16, NodeData: BaseNodeData @20, LowerTime: Float32 @24 |
| RandomSelectorNodeData | class | BaseNodeData | 128 | RandomNodes: RandomSelectorGroupData[] @84, MainNodes: RandomSelectorMainGroupData[] @104, ResetRandomTimes: Boolean @124 |
| ResultAnimTreeData | class | DataContainer | 204 | PoseParameter: IntParameter @12, Name: String @16, AbsoluteTimeParameter: FloatParameter @28, DeltaTimeParameter: FloatParameter @32, NodeCount: Int32 @36, BlendOutTime: Float32 @40, RagdollInfluenceMapIndex: Int32 @44, Priority: Int32 @48, BlendInTime: Float32 @52, ParameterGroupData: ParameterGroupData @56, AimAnimationList: AimAnimationAsset[] @60, Controls: BaseAnimationControlData[] @80, Nodes: BaseNodeData[] @96, AnimationList: BaseAnimationAsset[] @116, AimControls: AimControlData[] @136, RootNode: BaseNodeData @152, Skeleton: SkeletonAsset @156, EffectInfluenceMaps: InfluenceMapData[] @160, Preprocessors: BasePreprocessorData[] @180, IgnorePriority: Boolean @200 |
| ResultInfluenceMapData | class | DataContainer | 32 | Skeleton: SkeletonAsset @12, Weights: Float32[] @16 |
| SelectorByPoseNodeData | class | BaseNodeData | 128 | LegPositionsTransition: Vec3[] @84, LegPositionsIdle: Vec3[] @100, Idle: AnimationNodeData @116, Transition: AnimationNodeData @120, BlendNodes: Boolean @124 |
| SelectorNodeData | class | BaseNodeData | 164 | FloatNodes: FloatNodeGroupData[] @84, IntNodes: IntNodeGroupData[] @104, AlwaysNodes: AlwaysNodeGroupData[] @124, BoolNodes: BoolNodeGroupData[] @144 |
| SkeletonAsset | class | BaseSkeletonAsset | 128 | LODErrorFactors: QualityMapData @24, MinBoneName: String @28, MainChildBones: String[] @40, MaxMainBoneIndex: Int32 @56, MinBoneIndex: Uint32 @60, LODSettings: QualityMapData @64, MaxMainBonesUpdateTime: Int32 @68, PropertiesRootBoneName: String @72, IkInfo: SoldierIKData @84, ExportBonesSelection: InfluenceMapData @88, ProceduralLeaningInfo: SoldierProceduralLeaningData @92, MinMainBonesUpdateTime: Int32 @96, AimingProperty: AnimatedPropertyData @100, AccumulateMotion: Boolean @124 |
| SoldierIKData | class | DataContainer | 228 | HandsIkLod: LodInterval @12, FeetIkLod: LodInterval @20, WeaponInfluenceMap: InfluenceMapData @28, WeaponHandsIkInfluenceMap: InfluenceMapData @32, WeaponBoneIndex: Int32 @36, PrimaryHandBones: Uint32[] @40, PrimaryLegBones: Uint32[] @56, WeaponHandsIkInfluenceMapEffects: InfluenceMapEffectData[] @72, PrimaryHandLegBones: Uint32[] @92, WeaponBoneName: String @108, FeetAngularCorrection: Float32 @120, Legs: LegInfoData[] @124, Hands: HandInfoData[] @140, FeetMinAngularY: Float32 @156, WeaponIkDeltaProperty: AnimatedPropertyData @160, WeaponIkAnimationProperty: AnimatedPropertyData @184, FeetMaxNegativeCorrection: Float32 @208, FeetMaxPositiveCorrection: Float32[] @212 |
| SoldierProceduralLeaningData | class | DataContainer | 52 | HipBoneIndex: Int32 @12, PrimaryBones: Uint32[] @16, HipBoneName: String @32, Lod: LodInterval @44 |
| SpecificAnimTreeData | class | AnimTreeDataAsset | 84 | Skeleton: SkeletonAsset @56, AnimTree: AnimTreeData @60, ReplacmentAnimations: BaseAnimationAsset[] @64 |
| StrafePreprocessorData | class | BasePreprocessorData | 36 | LeftBackAngle: Float32 @12, MovementAngleParameter: FloatParameter @16, FrontParameter: BoolParameter @20, RightFrontAngle: Float32 @24, RightBackAngle: Float32 @28, LeftFrontAngle: Float32 @32 |
| StringSelectorNodeData | class | BaseNodeData | 144 | SelectionParameter: IntParameter @84, FolderSubstringTemplate: String @88, TemplateNodeData: BaseNodeData @100, NodesInfoMap: StringSelectorNodeGroupData[] @104, RealNodeData: BaseNodeData[] @124 |
| StringSelectorNodeGroupData | class | AnimNodePortGroupData | 28 | FolderSubstring: String @12, SelectionIndex: Int32 @24 |
| TimeControllerNodeData | class | BaseNodeData | 100 | MinValue: Float32 @84, MaxValue: Float32 @88, Animation: BaseNodeData @92, Parameter: FloatParameter @96 |
| TimeSynchronizerNodeData | class | BaseNodeData | 108 | IsCyclic: TripleBool @84, PlayFromBegining: TripleBool @88, NormalizedTimeParameter: FloatParameter @92, Node: BaseNodeData @96, SpeedParameter: FloatParameter @100, SynchType: TimeSynchronizationType @104 |
| TimeSynchronizerToParentNodeData | class | BaseNodeData | 88 | Node: BaseNodeData @84 |
| TransitionInfoData | class | AnimNodePortGroupData | 24 | ToValue: Int32 @12, FromValue: Int32 @16, Transition: BaseNodeData @20 |
| TransitionNodeData | class | BaseNodeData | 120 | PoseAfterTransitionParameter: IntParameter @84, DisableTransitionsParameter: BoolParameter @88, PoseSelectionParameter: IntParameter @92, Transitions: TransitionInfoData[] @96, ElseNode: BaseNodeData @116 |
| TripleStateNodeData | class | BaseNodeData | 108 | ElseNode: BaseNodeData @84, ActivateNodeParameter: BoolParameter @88, PostNode: BaseNodeData @92, PreNode: BaseNodeData @96, LoopNode: BaseNodeData @100, CrossFadeEnabled: Boolean @104, ContinuousTime: Boolean @105 |
| TurnNodeData | class | BaseNodeData | 112 | TurnSpeedParameter: FloatParameter @84, TurnAngleParameter: FloatParameter @88, TurnTimeParameter: FloatParameter @92, TurnRightLong: AnimationNodeData @96, TurnLeft: AnimationNodeData @100, TurnRight: AnimationNodeData @104, TurnLeftLong: AnimationNodeData @108 |
| Vec3Parameter | class | BaseParameter | 48 | InitialValue: Vec3 @32 |
| WeaponHandsIkNodeData | class | BaseHandsIkNodeData | 224 | WeaponBoneDelta: LinearTransform @128, PositionAligmentBoneName: String @192, ApplyWeaponDelta: BaseExpressionData @204, GeneralPose: AnimationAsset @208, EnableWeaponDelta: Boolean @212, EnableWeaponAnimation: Boolean @213 |
| WeightFactorNodeData | class | BaseNodeData | 116 | Factor: Float32 @84, Node: BaseNodeData @88, ParameterValues: FloatValueGroupData[] @92, Invert: Boolean @112 |
| XorSelectorNodeData | class | BaseNodeData | 108 | ElseNode: BaseNodeData @84, Nodes: BoolNodeGroupData[] @88 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Animation 124 records]

## 3. Sound

### RTTI-AUDIO-1: `Audio`

Status: **verified** · Variants: PC

The sound module's 125 records: the sound graph's node classes and their port groups (SGR-*: the nodes' data; the
run-time side is SGR-NODE-2), the voice-over tree's nodes, the waves and their metadata, the impulse
responses, the sound areas and the mixer's buses. The enumerations fix the numbers the nodes compare (`MathOp`,
`ConditionType`, `FaderType`, `LoopType`, `OscillatorMode` ...).

<!-- typeinfo:audio-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AudioPortConnection | value type | - | 4 | TargetId: Uint16 @0, SourceId: Uint16 @2 |
| BusConnection | enumeration | - | 4 | GameMixMain 0, GameMixAlt 1, DirectOut 2 |
| ConditionType | enumeration | - | 4 | And 0, Or 1, XOr 2, Equal 3, Less 4, Greater 5, LessOrEqual 6, GreaterOrEqual 7 |
| DebugRenderType | enumeration | - | 4 | Text 0, Plot 1, Bar 2 |
| DistanceAttenuationType | enumeration | - | 4 | DatLinear 0, DatLogarithmic 1 |
| EffectId | enumeration | - | 4 | EiReceive 0, EiSend 1, EiOutput 2, EiMonophonicSampler 3, EiFourChannelSampler 4, EiSampler 5, EiFollowEnvelope 6, EiOscillator 7, EiLfo 8, EiMixer 9, EiCrossfader 10, EiCrossfader2 11, EiMultiCrossfader 12, EiSimpleTransform 13, EiLinearTransform 14, EiNumberGenerator 15, EiScaleClamp 16, EiPhysics 17, EiLowPassIir2 18, EiHighPassIir2 19, EiDelay 20, EiUserMusicArbiter 21, EiDeMux 22, EiSequenceGenerator 23, EiAdsr 24, EiMinMax 25, EiCondition 26, EiDelta 27, EiParameterFilterLp 28, EiParameterFilterLinear 29, EiCar 30, EiSimpleCar 31, EiTank 32, EiHelicopter 33, EiCamera 34, EiRumble 35, EiValueDebug 36, EiEventDebug 37, EiJoypad 38, EiNone 39, EffectCount 40 |
| FaderType | enumeration | - | 4 | FtLinear 0, FtCosine 1 |
| LoopType | enumeration | - | 4 | LtNone 0, LtForward 1, LtRelease 2, LtInstantRelease 3, LtBounce 4 |
| MathOp | enumeration | - | 4 | Add 0, Sub 1, Mult 2, Div 3 |
| MixType | enumeration | - | 4 | MtGeneral 0, MtMusic 1, MtDialogue 2, MtCarRadio 3 |
| OscillatorMode | enumeration | - | 4 | OmRandom 0 |
| PointEnvelopePoint | value type | - | 8 | Y: Float32 @0, X: Float32 @4 |
| RandomType | enumeration | - | 4 | GTRandomUniform 0 |
| ResamplingQuality | enumeration | - | 4 | SincBest 0, SincMedium 1, SincFastest 2, ZeroOrderHold 3, LinearQuality 4 |
| RuntimeDialogLineNode | value type | - | 20 | WaveCount: Uint32 @0, Path: String @4, HashedName: Uint32 @16 |
| SoundAreaType | enumeration | - | 4 | SatDefault 0, SatUrban 1, SatCanyon 2, SatForest 3, SatIndoor 4, SatCount 5 |
| SoundNodePortType | enumeration | - | 4 | SgptAudio 0, SgptEvent 1, SgptParam 2 |
| TimePair | value type | - | 8 | StopTime: Float32 @0, StartTime: Float32 @4 |
| VehicleInput | enumeration | - | 4 | Throttle 0, RPM 1, Velocity 2, InDriverPos 3, InteriorCam 4, Distance 5, Tilt 6, Gear 7, SpringCompression 8, Slip 9, AngularVelocity 10 |
| WavePriority | enumeration | - | 4 | WpNone 0, WpLow 1, WpNormal 2, WpHigh 3, WpPermanent 4 |
| WaveSegment | enumeration | - | 4 | WsLeadIn 0, WsLoop 1, WsLeadOut 2, WaveSegmentCount 3 |
| WireConnection | enumeration | - | 4 | WireNotConnectedIndex 4294967295 |
<!-- /typeinfo -->

<!-- typeinfo:audio-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AdsrNode | class | SoundNode | 48 | S: SoundNodePort @16, D: SoundNodePort @20, Value: SoundNodePort @24, R: SoundNodePort @28, Release: SoundNodePort @32, Trigger: SoundNodePort @36, A: SoundNodePort @40, InitialValue: SoundNodePort @44 |
| AudioMasterSettingsAsset | class | Asset | 44 | SoundSystemSettings: SoundSystemSettings[] @24 |
| AudioMaterialDefinitions | class | Editable | 40 | OnRoadMaterialNameHashes: Uint32[] @24 |
| CameraNode | class | SoundNode | 28 | Roll: SoundNodePort @16, Yaw: SoundNodePort @20, Pitch: SoundNodePort @24 |
| CarNode | class | SoundNode | 496 | Cruise_Rpm_Pitch: Vec4 @16, Accellerate_Slip_Pitch: Vec4 @32, Accellerate_AngularVel_Pitch: Vec4 @48, Trans_Vel_Pitch: Vec4 @64, Release_Tilt_Pitch: Vec4 @80, Reverse_Idle_Tilt_Pitch: Vec4 @96, Reverse_Idle_Rpm_Pitch: Vec4 @112, Reverse_Idle_Rpm_Amp: Vec4 @128, Idle_Rpm_Pitch: Vec4 @144, Idle_Tilt_Pitch: Vec4 @160, Stuck_Vel_Amp: Vec4 @176, Stuck_Rpm_Amp: Vec4 @192, Accellerate_Tilt_Pitch: Vec4 @208, InteriorStrain_Vel_Amp: Vec4 @224, Trans_Vel_Amp: Vec4 @240, InteriorStrain_Vel_Pitch: Vec4 @256, DeaccellerateOut: SoundNodePort @272, StuckOut: SoundNodePort @276, SuspensionBounceOut: SoundNodePort @280, InteriorStrainOut: SoundNodePort @284, PipeAttackOut: SoundNodePort @288, TransmissionOut: SoundNodePort @292, InteriorOut: SoundNodePort @296, Gear1MaxVelocity: Float32 @300, StuckVelocityThreshold: Float32 @304, PipeAttackMaxVelocity: Float32 @308, PipeAtatckThrottleDeltaThreshold: Float32 @312, Trans_Reverse_Amp_Factor: Float32 @316, Gear2MaxVelocity: Float32 @320, Gear3MaxVelocity: Float32 @324, StuckIdleTimeThreshold: Float32 @328, AccellerateOut: SoundNodePort @332, Release: WaveAsset @336, Gear3Release: WaveAsset @340, Gear2Release: WaveAsset @344, TransmissionLoop: WaveAsset @348, StuckLoop: WaveAsset @352, InteriorStrainLoop: WaveAsset @356, InteriorLoop: WaveAsset @360, Gear1Release: WaveAsset @364, IdleLoop: WaveAsset @368, Stop: WaveAsset @372, Start: WaveAsset @376, Gear1: WaveAsset @380, CruiseLoop: WaveAsset @384, Gear3: WaveAsset @388, Gear2: WaveAsset @392, PipeAttacks: WaveAsset[] @396, AngularVelocity: SoundNodePort @416, Slip: SoundNodePort @420, Tilt: SoundNodePort @424, ReleaseEvent: SoundNodePort @428, IdleOut: SoundNodePort @432, StopOut: SoundNodePort @436, StartOut: SoundNodePort @440, Gear: SoundNodePort @444, RPM: SoundNodePort @448, Throttle: SoundNodePort @452, SuspensionBounces: WaveAsset[] @456, Velocity: SoundNodePort @476, SpringCompression: SoundNodePort @480, InteriorCam: SoundNodePort @484, InDriverPos: SoundNodePort @488 |
| CharacterNode | class | VoiceOverRootNode | 44 | Children: VoiceOverChildNode[] @24 |
| CompressorSettings | class | DataContainer | 28 | Attack: Float32 @12, Release: Float32 @16, Ratio: Float32 @20, Threshold: Float32 @24 |
| ConditionGroup | class | SoundNodePortGroup | 28 | True: SoundNodePort @12, Condition: ConditionType @16, X: SoundNodePort @20, Y: SoundNodePort @24 |
| ConditionNode | class | SoundNode | 36 | Conditions: ConditionGroup[] @16 |
| Crossfader2Node | class | SoundNode | 28 | CtrlOut2: SoundNodePort @16, CtrlOut1: SoundNodePort @20, Ctrl: SoundNodePort @24 |
| CrossfaderNode | class | SoundNode | 32 | In2: SoundNodePort @16, Out: SoundNodePort @20, Ctrl: SoundNodePort @24, In1: SoundNodePort @28 |
| DeMuxNode | class | SoundNode | 48 | Value: SoundNodePort @16, Trigger: SoundNodePort @20, Outputs: DeMuxOutput[] @24, WrapValue: Boolean @44 |
| DeMuxOutput | class | SoundNodePortGroup | 16 | Trigger: SoundNodePort @12 |
| DebugEventInput | class | SoundNodePortGroup | 28 | Name: String @12, E: SoundNodePort @24 |
| DebugValueInput | class | SoundNodePortGroup | 40 | Min: Float32 @12, Max: Float32 @16, RenderType: DebugRenderType @20, V: SoundNodePort @24, Name: String @28 |
| DelayNode | class | SoundNode | 32 | In: SoundNodePort @16, Out: SoundNodePort @20, DelayTime: SoundNodePort @24, Feedback: SoundNodePort @28 |
| DeltaGroup | class | SoundNodePortGroup | 20 | Delta: SoundNodePort @12, In: SoundNodePort @16 |
| DeltaNode | class | SoundNode | 36 | Deltas: DeltaGroup[] @16 |
| DialogLineNode | class | VoiceOverChildNode | 48 | Priority: Int32 @24, TalkTimes: TimePair[] @28, BlockAllSoundsWithLowerPriority: Boolean @44 |
| EqualizerSettings | class | DataContainer | 32 | HighShelfGain: Float32 @12, HpCutoffFrequency: Float32 @16, HighShelfFrequency: Float32 @20, LowShelfFrequency: Float32 @24, LowShelfGain: Float32 @28 |
| EventDebugNode | class | SoundNode | 36 | Events: DebugEventInput[] @16 |
| FollowEnvelopeNode | class | SoundNode | 32 | Base: Float32 @16, Scale: Float32 @20, X: SoundNodePort @24, Y: SoundNodePort @28 |
| FourChannelSamplerGroup | class | SamplerGroup | 76 | - |
| FourChannelSamplerNode | class | SamplerNode | 36 | - |
| HdrSetting | class | DataContainer | 52 | WindowAttackTime: Float32 @12, WindowReleaseTime: Float32 @16, WindowSize: Float32 @20, Headroom: Float32 @24, CompressFactor: Float32 @28, RmsAttackTime: Float32 @32, RmsReleaseTime: Float32 @36, RmsSmoothFactor: Float32 @40, WindowMinBottom: Float32 @44, WindowMinTop: Float32 @48 |
| HelicopterNode | class | SoundNode | 240 | Engine_Tilt_Pitch: Vec4 @16, Rotor_Velocity_Pitch: Vec4 @32, Rotor_Close_Distant_Fade: Vec2 @48, Engine_Velocity_Pitch: Vec4 @64, Rotor_Velocity_Amp: Vec4 @80, HfNoise_Velocity_Pitch: Vec4 @96, Turbine_Velocity_Amp: Vec4 @112, Turbine_Velocity_Pitch: Vec4 @128, InteriorCam: SoundNodePort @144, InDriverPos: SoundNodePort @148, Velocity: SoundNodePort @152, Tilt: SoundNodePort @156, Start: WaveAsset @160, ReleaseEvent: SoundNodePort @164, Distance: SoundNodePort @168, InteriorOut: SoundNodePort @172, HfNoise: WaveAsset @176, Turbine: WaveAsset @180, Interior: WaveAsset @184, RotorDistant: WaveAsset @188, Stop: WaveAsset @192, Engine: WaveAsset @196, RotorClose: WaveAsset @200, RotorDistantOut: SoundNodePort @204, HfNoiseOut: SoundNodePort @208, TurbineOut: SoundNodePort @212, RotorCloseOut: SoundNodePort @216, StartOut: SoundNodePort @220, StopOut: SoundNodePort @224, EngineOut: SoundNodePort @228 |
| HighPassIir2Node | class | SoundNode | 28 | Out: SoundNodePort @16, In: SoundNodePort @20, Frequency: SoundNodePort @24 |
| IWave | class | - | 4 | - |
| ImpulseResponse | class | - | 20 | - |
| IrAsset | class | WaveAsset | 36 | - |
| JoypadNode | class | SoundNode | 40 | RightAnalogY: SoundNodePort @16, AButton: SoundNodePort @20, BButton: SoundNodePort @24, LeftAnalogX: SoundNodePort @28, LeftAnalogY: SoundNodePort @32, RightAnalogX: SoundNodePort @36 |
| LfoGroup | class | SoundNodePortGroup | 36 | Mode: OscillatorMode @12, Min: Float32 @16, Max: Float32 @20, Hz: SoundNodePort @24, Amplitude: SoundNodePort @28, Out: SoundNodePort @32 |
| LfoNode | class | SoundNode | 36 | LfoGroups: LfoGroup[] @16 |
| LinearTransformNode | class | SoundNode | 32 | M: SoundNodePort @16, Y: SoundNodePort @20, X: SoundNodePort @24, K: SoundNodePort @28 |
| LowPassIir2Node | class | SoundNode | 28 | Out: SoundNodePort @16, In: SoundNodePort @20, Frequency: SoundNodePort @24 |
| MinMaxNode | class | SoundNode | 32 | Max: SoundNodePort @16, Min: SoundNodePort @20, X: SoundNodePort @24, Y: SoundNodePort @28 |
| MixGroup | class | DataContainer | 48 | Parent: MixGroup @12, Wildcard: String @16, Path: String @28, Gain: Float32 @40, IsWildcard: Boolean @44 |
| MixerAsset | class | Editable | 44 | Groups: MixGroup[] @24 |
| MixerGroup | class | SoundNodePortGroup | 20 | Amplitude: SoundNodePort @12, In: SoundNodePort @16 |
| MixerNode | class | SoundNode | 40 | Out: SoundNodePort @16, MixerGroups: MixerGroup[] @20 |
| MonophonicSamplerGroup | class | SamplerGroup | 76 | - |
| MonophonicSamplerNode | class | SamplerNode | 36 | - |
| MultiCrossfaderGroup | class | SoundNodePortGroup | 40 | FadeBegin: Float32 @12, FadeEnd: Float32 @16, FadeType: FaderType @20, FadeAmplitude: Float32 @24, Amplitude: SoundNodePort @28, Start: SoundNodePort @32, Stop: SoundNodePort @36 |
| MultiCrossfaderNode | class | SoundNode | 52 | Stop: SoundNodePort @16, Control: SoundNodePort @20, CrossfaderGroups: MultiCrossfaderGroup[] @24, Start: SoundNodePort @44, LockControlValue: Boolean @48 |
| NumberGeneratorNode | class | SoundNode | 36 | NumberType: RandomType @16, Y: SoundNodePort @20, Max: Float32 @24, Trigger: SoundNodePort @28, Min: Float32 @32 |
| OscillatorGroup | class | SoundNodePortGroup | 48 | PhaseOffset: Float32 @12, Max: Float32 @16, PreviousOutput: Float32 @20, Phase: Float32 @24, Min: Float32 @28, Shape: SoundNodePort @32, Hz: SoundNodePort @36, Out: SoundNodePort @40, Amplitude: SoundNodePort @44 |
| OscillatorNode | class | SoundNode | 36 | OscillatorGroups: OscillatorGroup[] @16 |
| OutputEntry | class | SoundNodePortGroup | 20 | Output: SoundOutput @12, In: SoundNodePort @16 |
| OutputNode | class | SoundNode | 60 | Dialog: SoundNodePort @16, BypassHeadroom: SoundNodePort @20, ConnectTo: BusConnection @24, DialogSoundEmitter: SoundSourceDialogEmitter @28, Entries: OutputEntry[] @32, Lfe: SoundNodePort @52, LfeSoundEmitter: SoundSourceLfeEmitter @56 |
| ParameterFilterLinearNode | class | SoundNode | 32 | AttackSpeed: SoundNodePort @16, ReleaseSpeed: SoundNodePort @20, In: SoundNodePort @24, Out: SoundNodePort @28 |
| ParameterFilterLpNode | class | SoundNode | 28 | Hz: SoundNodePort @16, Out: SoundNodePort @20, In: SoundNodePort @24 |
| PhysicsNode | class | SoundNode | 24 | Velocity: SoundNodePort @16, Distance: SoundNodePort @20 |
| PointEnvelope | class | DataContainer | 28 | Points: PointEnvelopePoint[] @12 |
| ReceiveEntry | class | SoundNodePortGroup | 28 | Parameter: Float32 @12, SavedValue: Float32 @16, Out: SoundNodePort @20, Source: SoundGraphParameter @24 |
| ReceiveNode | class | SoundNode | 36 | Entries: ReceiveEntry[] @16 |
| ReverbAsset | class | DataContainer | 32 | MaxArea: Float32 @12, Gain: Float32 @16, AreaType: SoundAreaType @20, ImpulseResponse: IrAsset @24, Indoor: Boolean @28 |
| ReverbSettingsAsset | class | Editable | 44 | Reverbs: ReverbAsset[] @24 |
| RumbleNode | class | SoundNode | 24 | Right: SoundNodePort @16, Left: SoundNodePort @20 |
| SamplerGroup | class | SoundNodePortGroup | 76 | Release: SoundNodePort @12, Trigger: SoundNodePort @16, Amplitude: SoundNodePort @20, WaveMap: SamplerWaveMapEntry[] @24, Finished: SoundNodePort @44, Output: SoundNodePort @48, Pitch: SoundNodePort @52, RandomOnDemandVariationCount: Uint32 @56, Wave: SoundNodePort @60, Delay: SoundNodePort @64, Offset: SoundNodePort @68, ChannelCount: Int8 @72, ZeroLatency: Boolean @73, Random: Boolean @74 |
| SamplerNode | class | SoundNode | 36 | Groups: SamplerGroup[] @16 |
| SamplerWaveMapEntry | class | DataContainer | 24 | Loop: LoopType @12, BasePitch: Float32 @16, Wave: WaveAsset @20 |
| ScaleClampNode | class | SoundNode | 40 | OutMin: SoundNodePort @16, OutMax: SoundNodePort @20, Out: SoundNodePort @24, In: SoundNodePort @28, InMin: SoundNodePort @32, InMax: SoundNodePort @36 |
| SceneNode | class | VoiceOverChildNode | 44 | Children: VoiceOverChildNode[] @24 |
| SequenceGeneratorNode | class | SoundNode | 40 | Max: SoundNodePort @16, Step: SoundNodePort @20, Value: SoundNodePort @24, Trigger: SoundNodePort @28, Reset: SoundNodePort @32, Min: SoundNodePort @36 |
| SimpleCarNode | class | SoundNode | 128 | Drive_Vel_Pitch: Vec4 @16, Drive_Tilt_Pitch: Vec4 @32, Idle_Vel_Pitch: Vec4 @48, Idle_Drive_Vel_Fade: Vec2 @64, IdleOut: SoundNodePort @72, StopOut: SoundNodePort @76, Start: WaveAsset @80, DriveOut: SoundNodePort @84, StartOut: SoundNodePort @88, Drive: WaveAsset @92, Idle: WaveAsset @96, Stop: WaveAsset @100, Velocity: SoundNodePort @104, ReleaseEvent: SoundNodePort @108, Tilt: SoundNodePort @112, InDriverPos: SoundNodePort @116 |
| SimpleTransformNode | class | SoundNode | 32 | Z: SoundNodePort @16, Operation: MathOp @20, X: SoundNodePort @24, Y: SoundNodePort @28 |
| SoundAsset | class | Editable | 24 | - |
| SoundEntity | class | EffectEntity | 144 | - |
| SoundEntityData | class | EffectEntityData | 176 | SoundTemplate: SoundTemplate @160 |
| SoundGraphEvent | class | SoundGraphParameter | 20 | - |
| SoundGraphParameter | class | DataContainer | 20 | WireIndex: Int32 @12, Id: Uint32 @16 |
| SoundNode | class | DataContainer | 16 | EffectDescCachedIndex: Uint32 @12 |
| SoundNodeConnection | class | DataContainer | 28 | SourcePort: SoundNodePort @12, TargetPort: SoundNodePort @16, SourceNode: SoundNode @20, TargetNode: SoundNode @24 |
| SoundNodePort | class | DataContainer | 20 | UnconnectedValue: Float32 @12, WireIndex: Int32 @16 |
| SoundNodePortGroup | class | DataContainer | 12 | - |
| SoundOutput | class | DataContainer | 20 | Source: SoundSource @12, Emitter: SoundSourceEmitter @16 |
| SoundSource | class | DataContainer | 32 | Emitters: SoundSourceEmitter[] @12 |
| SoundSourceBypassHdrEmitter | class | SoundSourceEmitter | 84 | - |
| SoundSourceConeEmitter | class | SoundSourceEmitter | 144 | Direction: Vec3 @96, Position: Vec3 @112, OutsideVolume: Float32 @128, OuterAngle: Float32 @132, InnerAngle: Float32 @136 |
| SoundSourceDialogEmitter | class | SoundSourceEmitter | 88 | DialogLoudness: Float32 @84 |
| SoundSourceEmitter | class | GameObjectData | 84 | HFDampingOcclusion: Float32 @48, HFDampingObstruction: Float32 @52, Gain: Float32 @56, HFDampingAngle: Float32 @60, HFDampingDistance: Float32 @64, DistanceSlope: Float32 @68, MinDistance: Float32 @72, AttenuationType: DistanceAttenuationType @76, Solo: Boolean @80 |
| SoundSourceFlatEmitter | class | SoundSourceEmitter | 92 | Bleed: Float32 @84, Angle: Float32 @88 |
| SoundSourceLfeEmitter | class | SoundSourceEmitter | 88 | LfeLoudness: Float32 @84 |
| SoundState | class | DataContainer | 52 | FadeOutTime: Float32 @12, FadeInTime: Float32 @16, HdrSetting: HdrSetting @20, BypassSound: SoundTemplate @24, LpCutoffFrequency: Float32 @28, Name: String @32, Duration: Float32 @44, SendLpCutoffFrequencyToVo: Boolean @48 |
| SoundStateSettingsAsset | class | Asset | 52 | ScreamThreshold: Float32 @24, ImpairedHearingImpulseThreshold: Float32 @28, SoundStates: SoundState[] @32 |
| SoundSystemSettings | class | DataContainer | 48 | MainMixVolume: Float32 @12, ReverbVolume: Float32 @16, PostEffectsGain: Float32 @20, DistortionClipLevel: Float32 @24, MasterDialogGain: Float32 @28, Compressor: CompressorSettings @32, Equalizer: EqualizerSettings @36, MasterLfeGain: Float32 @40, MasterVolume: Float32 @44 |
| SoundTemplate | class | SoundAsset | 188 | LfeLoudness: Float32 @24, DialogLoudness: Float32 @28, ObstructionRadius: Float32 @32, DopplerFactor: Float32 @36, ReverbLoudness: Float32 @40, PreventDuplicateSpawnDistance: Float32 @44, PreventDuplicateSpawnFrameCount: Int32 @48, MasterDistanceSlope: Float32 @52, MasterPitch: Float32 @56, MasterMinDistance: Float32 @60, PublicParameters: SoundGraphParameter[] @64, PublicEvents: SoundGraphEvent[] @84, Nodes: SoundNode[] @104, AudioConnections: AudioPortConnection[] @124, Outputs: SoundOutput[] @140, WireCount: Int32 @160, Loudness: Float32 @164, AILoudness: Float32 @168, MaxInstanceCount: Uint32 @172, SoundSource: SoundSource @176, ReceiveNode: SoundNode @180, SkipDuplicateSpawns: Boolean @184 |
| StreamPool | class | Editable | 72 | StreamReadBlockCountWin32: Uint32 @24, StreamReadBlockCountPs3: Uint32 @28, StreamCountXenon: Uint32 @32, StreamCountWin32: Uint32 @36, StreamCountPs3: Uint32 @40, StreamReadBlockSizeXenon: Uint32 @44, StreamPoolName: String @48, StreamReadBlockSizePs3: Uint32 @60, StreamReadBlockCountXenon: Uint32 @64, StreamReadBlockSizeWin32: Uint32 @68 |
| StreamPools | class | Editable | 44 | StreamPool: StreamPool[] @24 |
| TankNode | class | SoundNode | 272 | Load_UnLoad_RPM_Pitch: Vec4 @16, Int_IntStrain_Vel_Fade: Vec2 @32, Load_UnLoad_Vel_Pitch: Vec4 @48, Idle_TheRest_Vel_Fade: Vec2 @64, Load_UnLoad_Throttle_Fade: Vec2 @72, TrackSlow_Fast_Vel_Pitch: Vec4 @80, Idle_RPM_Pitch: Vec4 @96, IntStrain_Vel_Pitch: Vec4 @112, TrackSlow_Fast_Vel_Fade: Vec2 @128, AngularVelocity: SoundNodePort @136, Velocity: SoundNodePort @140, RPM: SoundNodePort @144, PipeAttackOut: SoundNodePort @148, Throttle: SoundNodePort @152, AngularVelocityToThrottleScale: Float32 @156, AngularVelocityToVelocityScale: Float32 @160, ReleaseEvent: SoundNodePort @164, InDriverPos: SoundNodePort @168, InteriorCam: SoundNodePort @172, InteriorStrain: WaveAsset @176, Interior: WaveAsset @180, TracksFast: WaveAsset @184, TracksSlow: WaveAsset @188, UnLoaded: WaveAsset @192, Stop: WaveAsset @196, Start: WaveAsset @200, Loaded: WaveAsset @204, Idle: WaveAsset @208, PipeAttacks: WaveAsset[] @212, InteriorStrainOut: SoundNodePort @232, InteriorOut: SoundNodePort @236, TracksSlowOut: SoundNodePort @240, TracksFastOut: SoundNodePort @244, UnLoadedOut: SoundNodePort @248, StartOut: SoundNodePort @252, IdleOut: SoundNodePort @256, LoadedOut: SoundNodePort @260, StopOut: SoundNodePort @264 |
| UserMusicArbiterNode | class | SoundNode | 24 | Out: SoundNodePort @16, In: SoundNodePort @20 |
| ValueDebugNode | class | SoundNode | 36 | Values: DebugValueInput[] @16 |
| VoiceOverChildNode | class | VoiceOverNodeBase | 24 | - |
| VoiceOverLeafNode | class | VoiceOverNodeBase | 24 | - |
| VoiceOverNodeBase | class | TreeNodeBase | 24 | - |
| VoiceOverRootNode | class | VoiceOverNodeBase | 24 | - |
| VoiceOverTree | class | TreeBase | 68 | RadioSoundTemplate: SoundTemplate @24, ScriptedUnpositionedSoundTemplate: SoundTemplate @28, Language: LanguageFormat @32, RadioEffectSoundTemplate: SoundTemplate @36, TalkSoundTemplate: SoundTemplate @40, VoiceOverEntries: RuntimeDialogLineNode[] @44, ScriptedSoundTemplate: SoundTemplate @60, ScreamSoundTemplate: SoundTemplate @64 |
| Wave | class | IWave | 60 | - |
| WaveAsset | class | Asset | 36 | LengthInSeconds: Float32 @24, NameHash: Uint32 @28, IsStreaming: Boolean @32, Seekable: Boolean @33 |
| WaveAssetNode | class | VoiceOverLeafNode | 28 | WaveAsset: WaveAsset @24 |
| WaveClassification | class | Editable | 28 | WaveProperty: WaveProperty @24 |
| WaveMetaData | class | DataContainer | 16 | LengthInSeconds: Float32 @12 |
| WaveProperty | class | DataContainer | 28 | StreamPool: StreamPool @12, Name: String @16 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Audio 125 records]

## 4. Rendering and effects

### RTTI-REND-1: `Render`

Status: **verified** · Variants: PC

The renderer's 128 records: the shader assets, databases and their enumerations (render path, skinning, shadow map
method, instancing), the meshes (`MeshSet`, `RigidMesh`, `SkinnedMesh`, `CompositeMesh`, `OccluderMesh`), the
textures and their formats (`D3DFORMAT`, `TextureUsage`), the display settings of each Direct3D version, the buffers'
interfaces, the post-processing and the CRT screen effects.

<!-- typeinfo:render-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AlphaResultType | enumeration | - | 4 | ArtUnchanged 1033, ArtHeight 1034, ArtSetToZero 1035, ArtSetToOne 1036 |
| BlendMode | enumeration | - | 4 | BmNone 0, BmLerp 1, BmAdditive 2, BmSubtractive 3, BmRevSubtractive 4, BmMultiply 5, BmMin 6, BmMax 7, BmAdditiveDestOpacity 8 |
| BlurFilter | enumeration | - | 4 | BfNone 0, BfGaussian3Pixels 1, BfGaussian5Pixels 2, BfGaussian7Pixels 3, BfGaussian9Pixels 4, BfGaussian15Pixels 5, BfGaussian31Pixels 6 |
| ColorTintData | value type | - | 64 | Saturation: Vec3 @0, Brightness: Vec3 @16, Contrast: Vec3 @32, Hue: Float32 @48 |
| CrtGhostData | value type | - | 16 | Displacement: Vec2 @0, SubValue: Float32 @8, Strength: Float32 @12 |
| CrtGhostsData | value type | - | 48 | Ghost2: CrtGhostData @0, Ghost1: CrtGhostData @16, Ghost0: CrtGhostData @32 |
| CrtInterpolatedBrightnessData | value type | - | 12 | StrengthRecursiveScale: Float32 @0, StrengthDeviation: Float32 @4, BaseStrength: Float32 @8 |
| CrtNoiseData | value type | - | 56 | TextureSTScale: Vec2 @0, OnScreenRatio: Float32 @8, NumBars: Uint32 @12, MinStrength: Float32 @16, Texture: TextureAsset @20, MaxStrength: Float32 @24, TimeToLiveBase: Float32 @28, StrengthDeviation: Float32 @32, StrengthBase: Float32 @36, HeightDeviation: Float32 @40, HeightBase: Float32 @44, TimeToLiveDeviation: Float32 @48, TextureFilter: Boolean @52 |
| CrtRollingBarData | value type | - | 24 | TextureSTScale: Vec2 @0, Texture: TextureAsset @8, Velocity: Float32 @12, WrapPos: Float32 @16, Strength: Float32 @20 |
| CrtScanLinesData | value type | - | 28 | TextureSTOffset: Vec2 @0, TextureSTScale: Vec2 @8, Strength: Float32 @16, Texture: TextureAsset @20, TextureFilter: Boolean @24 |
| D3DFORMAT | enumeration | - | 4 | D3DFMT_UNKNOWN 0, D3DFMT_R8G8B8 20, D3DFMT_A8R8G8B8 21, D3DFMT_X8R8G8B8 22, D3DFMT_R5G6B5 23, D3DFMT_X1R5G5B5 24, D3DFMT_A1R5G5B5 25, D3DFMT_A4R4G4B4 26, D3DFMT_R3G3B2 27, D3DFMT_A8 28, D3DFMT_A8R3G3B2 29, D3DFMT_X4R4G4B4 30, D3DFMT_A2B10G10R10 31, D3DFMT_A8B8G8R8 32, D3DFMT_X8B8G8R8 33, D3DFMT_G16R16 34, D3DFMT_A2R10G10B10 35, D3DFMT_A16B16G16R16 36, D3DFMT_A8P8 40, D3DFMT_P8 41, D3DFMT_L8 50, D3DFMT_A8L8 51, D3DFMT_A4L4 52, D3DFMT_V8U8 60, D3DFMT_L6V5U5 61, D3DFMT_X8L8V8U8 62, D3DFMT_Q8W8V8U8 63, D3DFMT_V16U16 64, D3DFMT_A2W10V10U10 67, D3DFMT_D16_LOCKABLE 70, D3DFMT_D32 71, D3DFMT_D15S1 73, D3DFMT_D24S8 75, D3DFMT_D24X8 77, D3DFMT_D24X4S4 79, D3DFMT_D16 80, D3DFMT_L16 81, D3DFMT_D32F_LOCKABLE 82, D3DFMT_D24FS8 83, D3DFMT_VERTEXDATA 100, D3DFMT_INDEX16 101, D3DFMT_INDEX32 102, D3DFMT_Q16W16V16U16 110, D3DFMT_R16F 111, D3DFMT_G16R16F 112, D3DFMT_A16B16G16R16F 113, D3DFMT_R32F 114, D3DFMT_G32R32F 115, D3DFMT_A32B32G32R32F 116, D3DFMT_CxV8U8 117 |
| D3DSAMPLERSTATETYPE | enumeration | - | 4 | D3DSAMP_ADDRESSU 1, D3DSAMP_ADDRESSV 2, D3DSAMP_ADDRESSW 3, D3DSAMP_BORDERCOLOR 4, D3DSAMP_MAGFILTER 5, D3DSAMP_MINFILTER 6, D3DSAMP_MIPFILTER 7, D3DSAMP_MIPMAPLODBIAS 8, D3DSAMP_MAXMIPLEVEL 9, D3DSAMP_MAXANISOTROPY 10 |
| Dx10DisplayAdapterInfo | value type | - | 48 | AdapterLuid: Uint64 @0, DedicatedSystemMemory: Uint32 @8, DedicatedVideoMemory: Uint32 @12, Description: String @16, SharedSystemMemory: Uint32 @28, DeviceId: Uint32 @32, VendorId: Uint32 @36, Revision: Uint32 @40, SubSysId: Uint32 @44 |
| Dx11DisplayAdapterInfo | value type | - | 48 | AdapterLuid: Uint64 @0, DedicatedSystemMemory: Uint32 @8, DedicatedVideoMemory: Uint32 @12, Description: String @16, SharedSystemMemory: Uint32 @28, DeviceId: Uint32 @32, VendorId: Uint32 @36, Revision: Uint32 @40, SubSysId: Uint32 @44 |
| Dx9DisplayAdapterInfo | value type | - | 48 | AdapterLuid: Uint64 @0, DedicatedSystemMemory: Uint32 @8, DedicatedVideoMemory: Uint32 @12, Description: String @16, SharedSystemMemory: Uint32 @28, DeviceId: Uint32 @32, VendorId: Uint32 @36, Revision: Uint32 @40, SubSysId: Uint32 @44 |
| FogFunctionType | enumeration | - | 4 | FftNone 0, FftLinear 1, FftExponential 2, FftExponentialSquared 3, FftAnalytical 4, FftHybrid 5, FftVolumetricHeight 6 |
| HeightConversionMethod | enumeration | - | 4 | HcmAlphaChannel 1009, HcmAverageRGB 1010, HcmBiasedRGB 1011, HcmRed 1012, HcmGreen 1013, HcmBlue 1014, HcmMaxRGB 1015, HcmColorSpace 1016, HcmNormalize 1017, HcmNormalMapToHeightMap 1018 |
| IndexBufferFormat | enumeration | - | 4 | Ibf16Bit 0, Ibf32Bit 1 |
| MeshPortalDesc | value type | - | 24 | SectorB: Int32 @0, SectorA: Int32 @4, Winding: Vec3[] @8 |
| MeshSectorDesc | value type | - | 64 | Box: AxisAlignedBox @0, Id: Int32 @32, Parts: Int32[] @36 |
| MeshSurfaceShader | value type | - | 16 | Shader: ShaderAsset @0, Name: String @4 |
| NormalMapFilter | enumeration | - | 4 | Nmf4x 1040, Nmf3x3 1041, Nmf5x5 1042, Nmf7x7 1043, Nmf9x9 1044, NmfDuDv 1045 |
| NormalMapGenerationParams | value type | - | 28 | FilterKernel: NormalMapFilter @0, HeightConversionMethod: HeightConversionMethod @4, Scale: Float32 @8, AlphaResult: AlphaResultType @12, MinZ: Int32 @16, InvertZ: Boolean @20, AddHeightMap: Boolean @21, NormalMapSwapRGB: Boolean @22, InvertY: Boolean @23, NormalMapConversionEnabled: Boolean @24, Wrap: Boolean @25, InvertX: Boolean @26 |
| PostProcessDebugMode | enumeration | - | 4 | PpdmNormal 0, PpdmBloom 1, PpdmDofBlur 2, PpdmBlur 3, PpdmBlurStep 4, PpdmDepth 5 |
| ProceduralNoiseType | enumeration | - | 4 | PntPerlin 0, PntSimplex 1, PntSimulation 2 |
| Ps3MemoryType | enumeration | - | 4 | PmtDefault 0, PmtCell 1, PmtRsx 2 |
| RenderMapMode | enumeration | - | 4 | RmmRead 1, RmmWrite 2, RmmReadWrite 3, RmmWriteDiscard 4, RmmWriteNoOverwrite 5 |
| RenderResourceUsage | enumeration | - | 4 | RruImmutable 0, RruStatic 1, RruDynamic 2 |
| ShaderColorizeMode | enumeration | - | 4 | ScmNone 0, ScmDrawBlock 1, ScmSurfaceShader 2, ScmVertexShader 3, ScmPixelShader 4, ScmSamplerStateUsage 5 |
| ShaderContainerElement | value type | - | 24 | Ps3ExtraCmdLine: String @0, Name: String @12 |
| ShaderDatabaseGroup | value type | - | 16 | Path: ShaderRenderPath @0, Name: String @4 |
| ShaderGeometrySpace | enumeration | - | 4 | SgsObject 0, SgsWorld 1, SgsScreen 2 |
| ShaderInstancingMethod | enumeration | - | 4 | ShaderInstancingMethod_None 0, ShaderInstancingMethod_WorldTransform4x3Float 1, ShaderInstancingMethod_WorldTransform4x3Half 2 |
| ShaderRenderMode | enumeration | - | 4 | ShaderRenderMode_Normal 0, ShaderRenderMode_Shadowbuffer 1, ShaderRenderMode_ShadowbufferZTrans 2, ShaderRenderMode_Shadowmap 3, ShaderRenderMode_PointShadowbuffer 4, ShaderRenderMode_ZOnly 5, ShaderRenderMode_Velocity 6, ShaderRenderMode_Simple 7, ShaderRenderMode_SimpleLighting 8, ShaderRenderMode_Overdraw 9 |
| ShaderRenderPath | enumeration | - | 4 | ShaderRenderPath_All 0, ShaderRenderPath_Dx9 1, ShaderRenderPath_Dx10 2, ShaderRenderPath_Dx10_1 3, ShaderRenderPath_Dx11 4, ShaderRenderPath_Xenon 5, ShaderRenderPath_Ps3 6 |
| ShaderShadowmapMethod | enumeration | - | 4 | ShaderShadowmapMethod_None 0, ShaderShadowmapMethod_ManualPcf 1, ShaderShadowmapMethod_HardwarePcf 2, ShaderShadowmapMethod_CascadedBox3 3, ShaderShadowmapMethodBitCount 3, ShaderShadowmapMethod_CascadedBox4 4, ShaderShadowmapMethodCount 5 |
| ShaderShadowmapQuality | enumeration | - | 4 | ShaderShadowmapQuality_Pcf2x2 0, ShaderShadowmapQuality_Pcf4x4 1, ShaderShadowmapQualityCount 2 |
| ShaderSkinningMethod | enumeration | - | 4 | ShaderSkinningMethod_None 0, ShaderSkinningMethod_1Bone 1, ShaderSkinningMethod_2Bone 2, ShaderSkinningMethod_4Bone 4, ShaderSkinningMethod_8Bone 8, ShaderSkinningMethod_Null 9 |
| SootType | enumeration | - | 4 | StDisabled 0, StPerVertex 1, StPerPixel 2 |
| StencilMaskState | enumeration | - | 4 | SmsNone 0, SmsWrite 1, SmsRead 2 |
| TextureAssetGamma | enumeration | - | 4 | TagDefault 0, TagLinear 1, TagSrgb 2 |
| TextureOutputFormat | enumeration | - | 4 | TofDefault 0, TofDXT1 1, TofDXT3 2, TofDXT5 3, TofRGB565 4, TofARGB1555 5, TofARGB4444 6, TofARGB8888 7, TofA8 8, TofL8 9, TofL16 10, TofABGR16 11, TofABGR16F 12, TofABGR32F 13, TofR16F 14, TofR32F 15, TofDXN 16, TofNormalmapDXT 17, TofNormalmapDXT1 18, TofNormalmapDXT5RGA 19, TofRGBE_8888 20, TofDXT5A 21, TofGR16 22 |
| TextureUsage | enumeration | - | 4 | TuDefault 0, TuTangentSpaceNormalmap 1, TuObjectSpaceNormalmap 2, TuWorldSpaceNormalmap 3 |
| XenonShaderTessellationMode | enumeration | - | 4 | XstmDiscrete 0, XstmContinuous 1, XstmPerEdge 2 |
<!-- /typeinfo -->

<!-- typeinfo:render-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| BaseShaderAsset | class | Asset | 72 | FunctionName: String @24, Ps3ExtraCmdLine: String @36, SourceFile: FileRef @48, Source: String @60 |
| BinkMemory | class | IBinkMemory | 20 | - |
| BinkMemoryAsset | class | Asset | 40 | Bink: FileRef @24, DecompressAlphaChannel: Boolean @36 |
| BinkTexture | class | IBinkTexture | 196 | - |
| BinkTextureAsset | class | TextureBaseAsset | 36 | Usage: TextureUsage @24, Gamma: TextureAssetGamma @28, DecompressAlphaChannel: Boolean @32, StreamFromMemory: Boolean @33 |
| ColorTintScopeFilterData | class | ScopeFilterData | 112 | ColorTintData: ColorTintData @48 |
| CompositeMesh | class | Mesh | 144 | - |
| CompositeMeshAsset | class | MeshAsset | 76 | Sectorization: MeshSectorizationAsset @64, MaterialMaskingMesh: SkinnedMeshAsset @68, SootType: SootType @72 |
| CompositeMeshSet | class | MeshSet | 96 | - |
| CrtAnalogLinkHudFxData | class | HudFxData | 272 | ColorTint: ColorTintData @32, RollingBar: CrtRollingBarData @96, InterpolatedBrightness: CrtInterpolatedBrightnessData @120, Ghosts: CrtGhostsData @132, SegmentHeight: Float32 @180, Noise: CrtNoiseData @184, ScanLines: CrtScanLinesData @240 |
| CrtHudFxData | class | HudFxData | 224 | ColorTint: ColorTintData @32, RollingBar: CrtRollingBarData @96, InterpolatedBrightness: CrtInterpolatedBrightnessData @120, SegmentHeight: Float32 @132, Noise: CrtNoiseData @136, ScanLines: CrtScanLinesData @192 |
| Dx10DisplaySettings | class | DataContainer | 60 | WindowSize: Vec2 @12, Adapters: Dx10DisplayAdapterInfo[] @20, RefreshRate: Float32 @36, RenderAheadLimit: Uint32 @40, HWnd: Uint32 @44, RenderJobAffinity: Uint32 @48, Dx10PlusEnable: Boolean @52, FullScreen: Boolean @53, Dx10DotOneEnable: Boolean @54, NvPerfHudEnable: Boolean @55, DebugInfoEnable: Boolean @56 |
| Dx10IndexBuffer | class | IIndexBuffer | 56 | - |
| Dx10PixelShader | class | IPixelShader | 36 | - |
| Dx10ShaderDatabase | class | ShaderDatabase | 256 | - |
| Dx10Texture | class | ITexture | 72 | - |
| Dx10VertexBuffer | class | IVertexBuffer | 52 | - |
| Dx10VertexShader | class | IVertexShader | 36 | - |
| Dx11ComputeShader | class | IComputeShader | 36 | - |
| Dx11DisplaySettings | class | DataContainer | 56 | WindowSize: Vec2 @12, RenderAheadLimit: Uint32 @20, Adapters: Dx11DisplayAdapterInfo[] @24, RefreshRate: Float32 @40, HWnd: Uint32 @44, RenderJobAffinity: Uint32 @48, NvPerfHudEnable: Boolean @52, FullScreen: Boolean @53, DebugInfoEnable: Boolean @54 |
| Dx11IndexBuffer | class | IIndexBuffer | 56 | - |
| Dx11PixelShader | class | IPixelShader | 36 | - |
| Dx11RenderBuffer | class | IRenderBuffer | 32 | - |
| Dx11ShaderDatabase | class | ShaderDatabase | 256 | - |
| Dx11Texture | class | ITexture | 72 | - |
| Dx11VertexBuffer | class | IVertexBuffer | 52 | - |
| Dx11VertexShader | class | IVertexShader | 36 | - |
| Dx9DisplaySettings | class | DataContainer | 56 | WindowSize: Vec2 @12, RenderAheadLimit: Uint32 @20, Adapters: Dx9DisplayAdapterInfo[] @24, RefreshRate: Float32 @40, HWnd: Uint32 @44, RenderJobAffinity: Uint32 @48, NvPerfHudEnable: Boolean @52, UseVsync: Boolean @53, FullScreen: Boolean @54, DebugInfoEnable: Boolean @55 |
| Dx9IndexBuffer | class | IIndexBuffer | 36 | - |
| Dx9PixelShader | class | IPixelShader | 20 | - |
| Dx9ShaderDatabase | class | ShaderDatabase | 256 | - |
| Dx9Texture | class | ITexture | 80 | - |
| Dx9VertexBuffer | class | IVertexBuffer | 32 | - |
| Dx9VertexShader | class | IVertexShader | 24 | - |
| DynamicPostProcessData | class | Asset | 208 | Gamma: Vec3 @32, FadeParams: Vec2 @48, DepthFactors: Vec4 @64, Saturation: Vec3 @80, Brightness: Vec3 @96, Contrast: Vec3 @112, BloomScale: Vec3 @128, DofBlurFilterDeviation: Float32 @144, DofFocusDistance: Float32 @148, DofNearDistanceScale: Float32 @152, Hue: Float32 @156, DofScale: Float32 @160, DofFarDistanceScale: Float32 @164, DofBlurFilter: BlurFilter @168, MinExposure: Float32 @172, MiddleGray: Float32 @176, BloomExponent: Float32 @180, MaxExposure: Float32 @184, BlurAdd: Float32 @188, DebugBlurStep: Uint32 @192, TonemapAdjustTime: Float32 @196, DofEnable: Boolean @200 |
| EnlightenSystem | class | - | 80 | - |
| GlobalPostProcessSettings | class | DataContainer | 432 | ForceBloomScale: Vec3 @16, ForceVignetteScale: Vec2 @32, Ps3Saturation: Vec3 @48, PCBrightness: Vec3 @64, ViewSize: Vec2 @80, PCSaturation: Vec3 @96, PCContrast: Vec3 @112, FilmGrainTextureScale: Vec2 @128, FilmGrainColorScale: Vec3 @144, Brightness: Vec3 @160, Saturation: Vec3 @176, ForceVignetteColor: Vec4 @192, Ps3Contrast: Vec3 @208, Ps3Brightness: Vec3 @224, Contrast: Vec3 @240, HsaoFogAmount: Float32 @256, HsaoBilateralBlurRadius: Float32 @260, HsaoFogDistOfHalfStrength: Float32 @264, HsaoPowerExponent: Float32 @268, HsaoRandomTextureWidth: Uint32 @272, Hue: Float32 @276, HsaoGaussianBlurSize: BlurFilter @280, HsaoDirCount: Float32 @284, HsaoBilateralBlurEdgeThreshold: Float32 @288, HsaoBilateralBlurSharpness: Float32 @292, DebugMode: PostProcessDebugMode @296, HsaoRadius: Float32 @300, BloodEffectFrameSize: Float32 @304, HsaoRandomDirVariationCount: Uint32 @308, HsaoStepCount: Float32 @312, HsaoQuality: Uint32 @316, PCHue: Float32 @320, Ps3Hue: Float32 @324, HsaoMaxFootprintRadius: Float32 @328, HsaoContrast: Float32 @332, HsaoAngleBias: Float32 @336, HsaoAttenuation: Float32 @340, ForceDofFarDistanceScale: Float32 @344, ForceDofScale: Float32 @348, ForceDofBlurFilterDeviation: Float32 @352, ForceDofNearDistanceScale: Float32 @356, LdrBloomRange: Float32 @360, ForceBlurAdd: Float32 @364, ForceDofFocusDistance: Float32 @368, ChromostereopsisScale: Float32 @372, ChromostereopsisOffset: Uint32 @376, ForceDofEnable: Int32 @380, ForceVignetteExponent: Float32 @384, DownsampleAverageStartMipmap: Uint32 @388, DirectExposureEnable: Boolean @392, DrawDebugInfo: Boolean @393, QuarterDownsamplingEnable: Boolean @394, BloomBlurEnable: Boolean @395, BlurBlendEnable: Boolean @396, DownsampleLogAverageEnable: Boolean @397, ExposureClampEnable: Boolean @398, HdrBloomEnable: Boolean @399, HsaoGaussianBlurEnable: Boolean @400, HsaoGaussianBlurHalfResEnable: Boolean @401, HdrBlurEnable: Boolean @402, HsaoBilateralBlurEnable: Boolean @403, HsaoBilateralBlurVectorizedEnable: Boolean @404, Ldr16BitBloomEnable: Boolean @405, HsaoOnlyEnable: Boolean @406, HsaoEnable: Boolean @407, HsaoHalfResEnable: Boolean @408, FilmicTonemappingEnable: Boolean @409, FilmGrainLinearFilteringEnable: Boolean @410, FilmGrainEnable: Boolean @411, ChromostereopsisEnable: Boolean @412, HsaoHalfResDepthEnable: Boolean @413, PostTonemapBlurEnable: Boolean @414, DownsampleBeforeBlurEnable: Boolean @415, DownsampleAverageFromBloomEnable: Boolean @416, BloomEnable: Boolean @417, HsaoNormalTextureEnable: Boolean @418, HsaoVectorizedEnable: Boolean @419, Ps3BackBufferAsLdrTargetEnable: Boolean @420 |
| HudFxData | class | DataContainer | 32 | ScissorRegion: Vec4 @16 |
| IBinkMemory | class | - | 4 | - |
| IBinkTexture | class | - | 4 | - |
| IComputeShader | class | - | 4 | - |
| IIndexBuffer | class | IRenderBuffer | 4 | - |
| IPixelShader | class | - | 4 | - |
| IRenderBuffer | class | IRenderResource | 4 | - |
| IRenderResource | class | - | 4 | - |
| IShaderDatabase | class | - | 4 | - |
| ITexture | class | IRenderResource | 4 | - |
| IVertexBuffer | class | IRenderBuffer | 4 | - |
| IVertexShader | class | - | 4 | - |
| IrradianceVolume | class | - | 80 | - |
| IrradianceVolumeSettings | class | DataContainer | 36 | Fade: Float32 @12, ScaleDefault: Float32 @16, MinVal: Float32 @20, ScaleVehicle: Float32 @24, ScaleSoldier: Float32 @28, Enable: Boolean @32, Debug: Boolean @33 |
| Mesh | class | - | 112 | - |
| MeshAsset | class | Asset | 64 | RadiosityAlbedoTexture: TextureAsset @24, ShaderSets: MeshShaderSetAsset[] @28, LodSettings: MeshLodSettingsAsset @48, Ps3SootEdgeScoreErrorWorkaroundEnable: Boolean @52, Ps3SootEdgeEnable: Boolean @53, RadiosityMesh: Boolean @54, Ps3EdgeEnable: Boolean @55, OccluderMeshEnable: Boolean @56, IrradianceVolumeEnable: Boolean @57, HighPriorityOccluderMesh: Boolean @58, Ps3EdgeScoreErrorWorkaroundEnable: Boolean @59, StreamingEnable: Boolean @60 |
| MeshData | class | - | 68 | - |
| MeshLodSettingsAsset | class | Asset | 60 | LodDistance3: Float32 @24, LodDistance4: Float32 @28, MinScreenArea: Float32 @32, LodDistance2: Float32 @36, Ps3LodScale: Float32 @40, LodDistance0: Float32 @44, LodDistance1: Float32 @48, Lod4Enabled: Boolean @52, Enable: Boolean @53, Lod3Enabled: Boolean @54, Lod0Enabled: Boolean @55, Lod1Enabled: Boolean @56, Lod2Enabled: Boolean @57 |
| MeshSectorizationAsset | class | Asset | 60 | WorldScale: Float32 @24, Sectors: MeshSectorDesc[] @28, Portals: MeshPortalDesc[] @44 |
| MeshSet | class | - | 96 | - |
| MeshShaderSetAsset | class | Asset | 24 | - |
| MeshStreamingSettings | class | DataContainer | 56 | MaxPendingLoadCount: Uint32 @12, SimulatedSeekTime: Float32 @16, ListViewPageIndex: Uint32 @20, ListViewSortOrder: Uint32 @24, SimulatedMbPerSecond: Float32 @28, PoolSize: Uint32 @32, Ps3CellPoolSize: Uint32 @36, DrawLoadingListEnable: Boolean @40, DrawMeshListEnable: Boolean @41, DrawNonStreamedListEnable: Boolean @42, SimulationEnable: Boolean @43, Enable: Boolean @44, DrawPriorityListEnable: Boolean @45, PrioritizeVisibleMeshesFirstEnable: Boolean @46, PrioritizeVisibleLodsFirstEnable: Boolean @47, UpdateEnable: Boolean @48, SpuJobEnable: Boolean @49, DrawStatsEnable: Boolean @50, DrawMissingListEnable: Boolean @51, PrioritizeVisibleLoadsEnable: Boolean @52, ClosestPointDistanceEnable: Boolean @53 |
| NoiseTextureAsset | class | TextureBaseAsset | 56 | Octaves: Uint32 @24, Period: Uint32 @28, Frequency: Float32 @32, Size: Uint32 @36, NoiseType: ProceduralNoiseType @40, OutputFormat: TextureOutputFormat @44, Dimensions: Uint32 @48, GenerateMipmaps: Boolean @52, Turbulence: Boolean @53 |
| OccluderMesh | class | - | 64 | - |
| OcclusionSettings | class | DataContainer | 60 | ScreenWidth: Uint32 @12, SkipClipTriangleDistance: Float32 @16, OccluderMeshJobCount: Uint32 @20, RasterizeJobCount: Uint32 @24, CoverageViewDistance: Float32 @28, NormalViewDistance: Float32 @32, ScreenHeigth: Uint32 @36, HighPriorityViewDistance: Float32 @40, TerrainViewDistance: Float32 @44, DrawOccludedDynamicEntities: Boolean @48, DrawCoverageBuffer: Boolean @49, DrawZbuffer: Boolean @50, DrawOccludedStaticEntities: Boolean @51, DrawProcessedOccluders: Boolean @52, DrawOccluderMeshesWireframe: Boolean @53, OccluderMeshEnable: Boolean @54, JobsEnable: Boolean @55, Enable: Boolean @56, RasterizeAlphaEnable: Boolean @57, RasterizeEnable: Boolean @58, OccluderAlphaMeshEnable: Boolean @59 |
| PixelShaderAsset | class | BaseShaderAsset | 72 | - |
| RigidMesh | class | Mesh | 128 | - |
| RigidMeshAsset | class | MeshAsset | 84 | SootSplitableTriangleEdgeLength: Float32 @64, Sectorization: MeshSectorizationAsset @68, SootType: SootType @72, SootVoxelSize: Float32 @76, UndergrowthShadowEnable: Boolean @80 |
| RigidMeshSet | class | MeshSet | 96 | - |
| ScopeFilterData | class | Asset | 48 | ScissorRegion: Vec4 @32 |
| ShaderAsset | class | Editable | 24 | - |
| ShaderContainerAsset | class | Asset | 80 | SourceFile: FileRef @24, Source: String @36, VertexShaders: ShaderContainerElement[] @48, PixelShaders: ShaderContainerElement[] @64 |
| ShaderDatabase | class | IShaderDatabase | 152 | - |
| ShaderDatabaseAsset | class | Asset | 36 | LevelName: String @24 |
| ShaderDatabaseCollection | class | Asset | 40 | Groups: ShaderDatabaseGroup[] @24 |
| ShaderSystemSettings | class | DataContainer | 240 | DebugNonFiniteColor: Vec3 @16, Ps3DispatchOnlyJob: Int32 @32, XenonTrilinearThreshold: Uint32 @36, DrawCallMultiplier: Uint32 @40, Ps3DispatchJobSpuCount: Uint32 @44, Ps3DispatchBlocksPerJob: Uint32 @48, Ps3DispatchMinJobBlockCount: Uint32 @52, MipmapBias: Float32 @56, ZOnlyMaxAnisotropy: Uint32 @60, MaxAnisotropy: Uint32 @64, Ps3DispatchJobCount: Uint32 @68, XenonDispatchBlocksPerJob: Uint32 @72, XenonDispatchJobCount: Uint32 @76, XenonDispatchOnlyJob: Int32 @80, XenonDispatchMinJobBlockCount: Uint32 @84, XenonDispatchCmdBufferSize: Uint32 @88, OverdrawMaxLayerCount: Uint32 @92, XenonDispatchCmdBufferSegmentSize: Uint32 @96, Ps3GeometryJobCount: Uint32 @100, Ps3GeometryJobSpuCount: Uint32 @104, Ps3ZCullBehindLimit: Uint32 @108, Ps3ZCullInFrontLimit: Uint32 @112, Ps3MaxMipmap: Uint32 @116, Ps3MinMipmap: Uint32 @120, FrameMemoryBufferSize: Uint32 @124, Path: ShaderRenderPath @128, ForceAlphaRef: Int32 @132, ColorizeMode: ShaderColorizeMode @136, Ps3DrawEdgeGeometryEnable: Boolean @140, Ps3DrawNonEdgeGeometryEnable: Boolean @141, Ps3GeometryJobWithoutDispatchJobEnable: Boolean @142, Ps3GeometryDebugRenderEnable: Boolean @143, Ps3TriangleCullZPassEnable: Boolean @144, Ps3TriangleBackfaceCullEnable: Boolean @145, Ps3TriangleCullEnable: Boolean @146, Ps3TriangleFrustumCullEnable: Boolean @147, Ps3CmdBufClearEnable: Boolean @148, Ps3FinishEnable: Boolean @149, Ps3PixelShaderRedundancyCheckingEnable: Boolean @150, Dx11DelayedShaderLoadingEnable: Boolean @151, Dx10DelayedShaderLoadingEnable: Boolean @152, Ps3GeometryJobEnable: Boolean @153, Ps3FlushEnable: Boolean @154, Ps3DispatchJobEnable: Boolean @155, Ps3PixelShaderHashStoreEnable: Boolean @156, Ps3CmdBufTexturesInlineEnable: Boolean @157, Ps3CmdBufVertexShaderInlineEnable: Boolean @158, Ps3TxpDemotionEnable: Boolean @159, Ps3SpuClipPlaneRejectEnable: Boolean @160, Ps3SpuCullVolumesEnable: Boolean @161, Ps3ZCullEnable: Boolean @162, Ps3DispatchSpuBufferLinkingEnable: Boolean @163, Ps3DispatchFrameBlockEnable: Boolean @164, Ps3SCullEnable: Boolean @165, Ps3DispatchFrameGpuBlockEnable: Boolean @166, Ps3SpuInstancingEnable: Boolean @167, Ps3TriangleCull4BoneSkinningEnable: Boolean @168, Ps3TrianglePartCullEnable: Boolean @169, Ps3TriangleCullDoubleSidedEnable: Boolean @170, Ps3TriangleCull1BoneSkinningEnable: Boolean @171, Ps3CmdBufTexturesEnable: Boolean @172, Ps3PartitionClipPlaneCullEnable: Boolean @173, Ps3PartitionVolumeCullEnable: Boolean @174, Ps3DrawBlockCullEnable: Boolean @175, Ps3PartitionFrustumCullEnable: Boolean @176, DepthEnable: Boolean @177, DepthWriteEnable: Boolean @178, DepthTestEnable: Boolean @179, ForceDoubleSided: Boolean @180, ForcePointFiltering: Boolean @181, ZeroViewportEnable: Boolean @182, UseHasZPass: Boolean @183, XenonZPassEnable: Boolean @184, XenonDirectConstantEnable: Boolean @185, ClipPlanesEnable: Boolean @186, SinglePrimitiveEnable: Boolean @187, DrawCallEnable: Boolean @188, ForceAlphaToMask: Boolean @189, DrawTransparent: Boolean @190, DrawTransparentDecal: Boolean @191, DrawTransparentDepthWrite: Boolean @192, FlushEnable: Boolean @193, DrawZOnly: Boolean @194, DrawOpaque: Boolean @195, AlphaBlendEnable: Boolean @196, AlphaToMaskEnable: Boolean @197, ForceAlphaTest: Boolean @198, DrawOffset: Boolean @199, StreamingSimpleShadingEnable: Boolean @200, AlphaTestEnable: Boolean @201, SortJobEnable: Boolean @202, SortSpuJobEnable: Boolean @203, PixelConstantsEnable: Boolean @204, StatesEnable: Boolean @205, PixelShaderSrgbConversionEnable: Boolean @206, BoneConstantOptimizationEnable: Boolean @207, DispatchExecuteEnable: Boolean @208, XenonDispatchJobEnable: Boolean @209, XenonDispatchBlockOnCmdBufEnable: Boolean @210, VertexConstantsEnable: Boolean @211, PixelConstantRedundancyCheckingEnable: Boolean @212, VertexConstantRedundancyCheckingEnable: Boolean @213, PseudoInstancingEnable: Boolean @214, SortBlocksEnable: Boolean @215, SimpleTexturesEnable: Boolean @216, SimpleTextureFilteringEnable: Boolean @217, XenonGprChangeEnable: Boolean @218, PixBlockEventsEnable: Boolean @219, GcmReplayMarkersEnable: Boolean @220, ShaderPixScopeEnable: Boolean @221, SimplePixelShaderEnable: Boolean @222, SimpleVertexShaderEnable: Boolean @223, LogEnable: Boolean @224, SingleFrameLogEnable: Boolean @225, SingleFrameBlockLogEnable: Boolean @226 |
| SkinnedMesh | class | Mesh | 128 | - |
| SkinnedMeshAsset | class | MeshAsset | 68 | SkinDecals: Boolean @64 |
| SkinnedMeshSet | class | MeshSet | 96 | - |
| SniperLensScopeFilterData | class | ScopeFilterData | 208 | ChromaticAberrationDisplacement1: Vec2 @48, ChromaticAberrationDisplacement2: Vec2 @56, RadialBlendDistanceCoefficients: Vec2 @64, ChromaticAberrationColor2: Vec3 @80, BlurCenter: Vec2 @96, ChromaticAberrationStrengths: Vec2 @104, ChromaticAberrationColor1: Vec3 @112, ColorTintData: ColorTintData @128, BlurScale: Float32 @192 |
| TextureAsset | class | TextureBaseAsset | 24 | - |
| TextureBaseAsset | class | Asset | 24 | - |
| TreeMesh | class | CompositeMesh | 160 | - |
| TreeMeshAsset | class | CompositeMeshAsset | 112 | LodScale: Float32 @76, BranchesAffectedByWind: Float32 @80, CoverageValue: Float32 @84, CutMaterialCount: Uint32 @88, AffectedByWind: Float32 @92, BillboardResolutionX: Uint32 @96, BillboardShader: ShaderAsset @100, BillboardResolutionY: Uint32 @104, Bendable: Boolean @108 |
| TreeMeshSet | class | MeshSet | 96 | - |
| VertexShaderAsset | class | BaseShaderAsset | 72 | - |
| VertexShaderFragmentAsset | class | Asset | 48 | Function: String @24, SourceFile: FileRef @36 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Render 128 records]

### RTTI-WREND-1: `WorldRender`

Status: **verified** · Variants: PC

The world renderer's 30 records: the light entities and their data (directional, point, projector, environment map,
ground map, outdoor), the mesh entities (rigid, composite, animated), the static environment map, the world render
settings and view modes. RTTI-FAC-1: the client's factories make the light and mesh entities directly.

<!-- typeinfo:worldrender-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| ActivePart | value type | - | 16 | partName: String @0, active: Boolean @12 |
| AnimationGroupData | value type | - | 20 | Animations: AnimationAsset[] @0 |
| EnvmapLightType | enumeration | - | 4 | EltDiffuse 0, EltSpecular 1, EltReflection 2 |
| MipmapFilterMode | enumeration | - | 4 | MfmBox 0, MfmRenormalize 1, MfmPoisson13 2, MfmPoisson13Clamped 3 |
| ProjectorLightProjection | enumeration | - | 4 | PlpPerspective 0, PlpOrthographic 1 |
| WorldViewMode | enumeration | - | 4 | WvmNormal 0, WvmSimple 1, WvmOverdraw 2 |
<!-- /typeinfo -->

<!-- typeinfo:worldrender-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AnimatedMeshEntity | class | Entity | 112 | - |
| AnimatedMeshEntityData | class | EntityData | 128 | MeshShaderSet: MeshShaderSetAsset @112, AnimationController: AnimationControllerAsset @116, Mesh: SkinnedMeshAsset @120 |
| AnimationControllerAsset | class | Asset | 44 | Skeleton: SkeletonAsset @24, AnimationGroups: AnimationGroupData[] @28 |
| CompositeMeshEntity | class | Entity | 160 | - |
| CompositeMeshEntityData | class | EntityData | 128 | MeshShaderSet: MeshShaderSetAsset @112, LightMapResolution: Uint32 @116, Mesh: CompositeMeshAsset @120, EnableGroundmapLighting: Boolean @124, LightMapEnable: Boolean @125 |
| DirLightEntity | class | Entity | 80 | - |
| DirLightEntityData | class | LightEntityData | 176 | Color: Vec3 @128, ContributionName: String @144, Intensity: Float32 @156, SpecularEnable: Boolean @160, DiffuseEnable: Boolean @161 |
| EnvmapLightEntity | class | Entity | 96 | - |
| EnvmapLightEntityData | class | LightEntityData | 176 | Color: Vec3 @128, ContributionName: String @144, EnvmapType: EnvmapLightType @156, Texture: TextureAsset @160 |
| GroundmapLightEntity | class | Entity | 96 | - |
| GroundmapLightEntityData | class | LightEntityData | 192 | Offset: Vec2 @128, Size: Vec2 @136, Color: Vec3 @144, AttenuationSize: Float32 @160, BleedSize: Float32 @164, ContributionName: String @168, Texture: TextureAsset @180, SideScale: Float32 @184, Intensity: Float32 @188 |
| LightEntityData | class | EntityData | 128 | Visible: Boolean @112 |
| OutdoorLightEntity | class | Entity | 112 | - |
| OutdoorLightEntityData | class | LightEntityData | 320 | TopColor: Vec3 @128, KeySpecularColor: Vec3 @144, EnlightenSkyColor: Vec3 @160, BottomColor: Vec3 @176, KeyColor: Vec3 @192, BillboardBottomColor: Vec3 @208, BottomColorIntensity: Float32 @224, TopColorIntensity: Float32 @228, KeySpecularColorIntensity: Float32 @232, CloudShadowScale: Float32 @236, CloudShadowIntensity: Float32 @240, EnlightenSkyColorIntensity: Float32 @244, KeyColorIntensity: Float32 @248, HemisphereLightAngleFactor: Float32 @252, ShCoefficientsShadow: Vec4[] @256, ShCoefficientsLightDelta: Vec4[] @272, DiffuseEnvmap: TextureAsset @288, ReflectionEnvmap: TextureAsset @292, ShCoefficientsLight: Vec4[] @296 |
| PointLightEntity | class | Entity | 96 | - |
| PointLightEntityData | class | LightEntityData | 192 | Color: Vec3 @128, ShadowbufferResolution: Uint32 @144, ShadowbufferOffset: Float32 @148, ContributionName: String @152, ShadowmapResolution: Uint32 @164, Radius: Float32 @168, Intensity: Float32 @172, FadeEndDistance: Float32 @176, FadeStartDistance: Float32 @180, Shadowbuffer: Boolean @184, Shadowmap: Boolean @185, DiffuseEnable: Boolean @186, SpecularEnable: Boolean @187 |
| ProjectorLightEntity | class | - | 96 | - |
| ProjectorLightEntityData | class | LightEntityData | 224 | Color: Vec3 @128, FieldOfView: Float32 @144, OrthoHeight: Float32 @148, OrthoWidth: Float32 @152, ViewDistance: Float32 @156, NearPlane: Float32 @160, Aspect: Float32 @164, Intensity: Float32 @168, ContributionName: String @172, Projection: ProjectorLightProjection @184, Texture: TextureAsset @188, ShadowbufferOffset: Float32 @192, ShadowbufferResolution: Uint32 @196, ShadowmapResolution: Uint32 @200, Shadowbuffer: Boolean @204, Shadowmap: Boolean @205, QuadraticAttenuation: Boolean @206, DiffuseEnable: Boolean @207, SpecularEnable: Boolean @208 |
| RigidMeshEntity | class | Entity | 128 | - |
| RigidMeshEntityData | class | EntityData | 128 | MeshShaderSet: MeshShaderSetAsset @112, LightMapResolution: Uint32 @116, Mesh: RigidMeshAsset @120, EnableGroundmapLighting: Boolean @124, LightMapEnable: Boolean @125 |
| StaticEnvmapEntity | class | Entity | 96 | - |
| StaticEnvmapEntityData | class | EntityData | 128 | InfluenceRadiusOuter: Float32 @112, InfluenceRadiusInner: Float32 @116, StaticEnvmap: TextureAsset @120 |
| VisualWorldState | class | DataContainer | 48 | FogColor: Vec3 @16, FogClamp: Float32 @32, FogEnd: Float32 @36, FogStart: Float32 @40, FogEnable: Boolean @44 |
| WorldRenderSettings | class | DataContainer | 256 | ClearColor: Vec3 @16, DrawDebugDynamicCubemapMipLevel: Int32 @32, MaxPointLightCount: Uint32 @36, OnlyShadowmapSlice: Int32 @40, MemoryBufferSize: Uint32 @44, ViewMode: WorldViewMode @48, GenericEntityMaxStaticEntityCount: Uint32 @52, GenericEntityMaxDynamicEntityCount: Uint32 @56, MultisampleCount: Uint32 @60, ReflectionCubemapSize: Uint32 @64, MultisampleQuality: Uint32 @68, DynamicCubemapFilterWidth: Float32 @72, Ps3ZPassSlopeScaleBias: Float32 @76, Dx9ShadowSlopeScaleBias: Float32 @80, Ps3ZPassDepthBias: Float32 @84, Ps3ShadowDepthBias: Float32 @88, Ps3ShadowSlopeScaleBias: Float32 @92, DynamicCubemapSidesPerFrameCount: Uint32 @96, DynamicCubemapFilterMode: MipmapFilterMode @100, ScopeFilterColorScale: Float32 @104, Dx9ZPassDepthBias: Float32 @108, Dx9ZPassSlopeScaleBias: Float32 @112, XenonFastHdrColorScale: Float32 @116, XenonFixedHdrColorScale: Float32 @120, ShadowmapHeight: Uint32 @124, ShadowmapSliceSchemeWeight: Float32 @128, ShadowmapSliceCount: Uint32 @132, ShadowmapWidth: Uint32 @136, ShadowViewportScale: Float32 @140, ShadowmapMinFov: Float32 @144, ViewportScale: Float32 @148, MinScreenArea: Float32 @152, ShadowMinScreenArea: Float32 @156, ShadowmapExtrusionLength: Float32 @160, ShadowmapMinScreenArea: Float32 @164, ShadowmapSizeZScale: Float32 @168, ShadowmapViewDistance: Float32 @172, ShadowmapFirstSliceScale: Float32 @176, ShadowmapSliceOverlapFactor: Float32 @180, ForegroundEnable: Boolean @184, BackgroundEnable: Boolean @185, ForegroundZPassEnable: Boolean @186, ForegroundBlurEnable: Boolean @187, BilateralHalfResCompositeEnable: Boolean @188, ForegroundDepthClearEnable: Boolean @189, ForegroundBlurFilterEnable: Boolean @190, ScopeFilterEnable: Boolean @191, GenericEntityRendererEnable: Boolean @192, JobsEnable: Boolean @193, FogEnable: Boolean @194, ColoredTilingEnable: Boolean @195, DynamicCullZBufferTestEnable: Boolean @196, DynamicCullSpuJobEnable: Boolean @197, ManualTilingEnable: Boolean @198, DynamicCubemapUpdateEnable: Boolean @199, DynamicCubemapEnable: Boolean @200, ZBufferShadowTestEnable: Boolean @201, TilePredicationEnable: Boolean @202, IrradianceVolumeEnable: Boolean @203, DynamicCubemapMipmapGenEnable: Boolean @204, DynamicCullDrawOccludedBoxesEnable: Boolean @205, WeatherSystemHalfResEnable: Boolean @206, HdrEnable: Boolean @207, XenonFastHdrEnable: Boolean @208, ColoredShadowmapSlicesEnable: Boolean @209, Enable: Boolean @210, XenonFastHdrResolveScaleEnable: Boolean @211, Ps3SCullEnable: Boolean @212, LinearZ: Boolean @213, Ps3TilingEnable: Boolean @214, Ps3ZCullEnable: Boolean @215, Ps3Shadowmap16BitEnable: Boolean @216, Ps3ShadowmapTilingEnable: Boolean @217, ShadowmapFixedMovementEnable: Boolean @218, ShadowmapFixedDepthEnable: Boolean @219, CascadedBoxShadowmapEnable: Boolean @220, GenerateShadowmapsEnable: Boolean @221, SimpleShadowmapsEnable: Boolean @222, CascadedBoxShadowmapClipMethodEnable: Boolean @223, ApplyShadowmapsEnable: Boolean @224, DrawDebugShadowmaps: Boolean @225, DrawDebugDynamicCubemap: Boolean @226, DrawFrustums: Boolean @227, DrawShadowFrustums: Boolean @228, DrawDebugLUT: Boolean @229, DrawDebugVisibleEntityTypes: Boolean @230, WireframeEnable: Boolean @231, DrawDebugZBufferEnable: Boolean @232, DrawDebugHalfRes: Boolean @233, TonemapEnable: Boolean @234, ShadowExtrusionCullingEnable: Boolean @235, GammaCorrectionEnable: Boolean @236, ShadowmapsEnable: Boolean @237, DrawLightSources: Boolean @238, DrawLineBoundingBoxes: Boolean @239, DrawBoundingSpheres: Boolean @240, DrawPointLightInfo: Boolean @241, DrawSolidBoundingBoxes: Boolean @242 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: WorldRender 30 records]

### RTTI-EMIT-1: `Emitter` and `EmitterRender`

Status: **verified** · Variants: PC

The particle system's records (FX-*): the emitter documents, templates and their processor and evaluator data, the
expression components, the emitter constants and vertex semantics, and the client's emitter entity.

<!-- typeinfo:emitter-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| EmittableField | enumeration | - | 4 | EfSpawnAnimationSpeed 72, EfSpawnAnimationFrameIndex 76, EfVelocity 80, EfParameters 96, EfNormTime 112, EfRotation 116, EfSpeed 120, EfUserDefined 121, EfConstantFloat 122, EfConstantVec 123, EfNone 124 |
| EmittableType | enumeration | - | 4 | Point 0, Quad 1, ScreenAlignedQuad 2, DirectionAlignedQuad 3, Trail 4, ParticleMesh 6, EmittableTypeCount 7 |
| EmitterConstants | enumeration | - | 4 | EmitterMaxCellCount1D 2 |
| EmitterHavokInfo | value type | - | 12 | ReserveCount: Int32 @0, FloatPhysics: FloatPhysicsData @4, HavokAsset: HavokAsset @8 |
| EmitterParameter | enumeration | - | 4 | EmitterParameterNone 0, EmitterParameter1 1, EmitterParameter2 2, EmitterParameter3 3, EmitterParameter4 4 |
| EmitterSystemMetrics | value type | - | 8 | HavokParticlePartCount: Int32 @0, HavokParticleCount: Int32 @4 |
| EmitterTextureInfo | value type | - | 44 | NmTextureOriginU: Float32 @0, AnimationStepV: Float32 @4, NmTextureOriginV: Float32 @8, NmAnimationStepV: Float32 @12, NmAnimationStepU: Float32 @16, AnimationFrameColumnCount: Float32 @20, AnimationFrameCount: Float32 @24, TextureOriginU: Float32 @28, AnimationStepU: Float32 @32, TextureOriginV: Float32 @36, EnableAnimation: Boolean @40 |
| EvaluatorType | enumeration | - | 4 | EtDefault 0, EtBox 1, EtMesh 2, EtPolynomial 3, EtPolynomialOperator 4, EtRandom 5, EtRandomXYZ 6, EtRotateVector 7, EtSampleTexture 8, EtSphere 9, EtConstant 10, EtPolynomialColorInterp 11, EvaluatorTypeCount 12 |
| ExpressionOperator | enumeration | - | 4 | ExpressionOperatorNone 0, ExpressionOperatorAdd 1, ExpressionOperatorSub 2, ExpressionOperatorMul 3 |
| ExpressionType | enumeration | - | 4 | PolynomialExpression 0, SineExpression 1, CosineExpression 2 |
| PolynomialOperation | enumeration | - | 4 | Multiplication 0, Addition 1, Subtraction 2 |
| PolynomialTempData | value type | - | 32 | Coefficients: Vec4 @0, MaxClamp: Float32 @16, MinClamp: Float32 @20, ScaleValue: Float32 @24 |
| ProcessorType | enumeration | - | 4 | PtBaseEmitter 0, PtSpawnRate 1, PtPreSpawn 2, PtSpawnCompositeMeshParts 3, PtSpawnSpeed 4, PtSpawnPosition 5, PtSpawnDirection 6, PtSpawnSize 7, PtSpawnAnimation 8, PtSpawnAnimationFrame 9, PtSpawnRotation 10, PtSpawnRotationSpeed 11, PtUpdatePosition 12, PtUpdateAge 13, PtTurbulance 14, PtGravity 15, PtLocalForce 16, PtAirResistance 17, PtUpdateLinearVelocity 18, PtUpdateCollision 19, PtUpdateOrientation 20, PtEmitter 21, PtUpdateBoundingBox 22, PtUpdateColor 23, PtUpdateColorLeaf 24, PtUpdateTransparency 25, PtUpdateTextureCoords 26, PtCycleAnimations 27, PtUpdateRotation 28, PtUpdateSizeX 29, PtUpdateSizeY 30, PtUpdateSizeZ 31, PtUpdateSize 32, PtUpdateAlphaLevelMin 33, PtUpdateAlphaLevelMax 34, PtUpdateAlphaLevelScale 35, PtUpdateClipScale 36, ProcessorTypeCount 37 |
| VertexSemantics | enumeration | - | 4 | ParticleColor 0, ParticleWidthHeight 4, ParticleTextureCoords1 7, ParticleTextureCoords2 9, ParticleTextureCoords3 11, ParticleTextureCoords4 13 |
<!-- /typeinfo -->

<!-- typeinfo:emitter-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| AirResistanceData | class | ProcessorData | 64 | DragFactor: Float32 @48 |
| BaseEmitterData | class | ProcessorData | 64 | EmitterAsset: EmitterDocument @48 |
| BoxEvaluatorData | class | EvaluatorData | 64 | Pivot: Vec3 @32, Dimensions: Vec3 @48 |
| ConstantEvaluatorData | class | EvaluatorData | 28 | Scale: Float32 @24 |
| EmitterAsset | class | Editable | 24 | - |
| EmitterComponentConnection | class | DataContainer | 20 | Dest: EmitterComponentData @12, Source: EmitterComponentData @16 |
| EmitterComponentData | class | DataContainer | 16 | Pre: EmitterComponentData @12 |
| EmitterData | class | ProcessorData | 64 | Mesh: RigidMeshAsset @48, Emitter: EmitterDocument @52 |
| EmitterDocument | class | EmitterAsset | 52 | TemplateData: EmitterTemplateData @24, EmitterComponents: EmitterComponentData[] @28, RootProcessor: ProcessorData @48 |
| EmitterEntity | class | EffectEntity | 144 | - |
| EmitterEntityData | class | EffectEntityData | 192 | SpawnProbability: Float32 @160, Emitter: EmitterAsset @164, CompositeMeshPartIndex: Int32 @168, Pause: Boolean @172, MoveEmitterInSpiral: Boolean @173, SpawnCompositeMeshPart: Boolean @174, ShootEmitterFromCamera: Boolean @175, StartStop: Boolean @176 |
| EmitterShaderAsset | class | Asset | 36 | Filename: String @24 |
| EmitterSystemAsset | class | Asset | 44 | HavokMeshCount: Int32 @24, HavokMeshes: EmitterHavokInfo[] @28 |
| EmitterTemplateData | class | DataContainer | 240 | PointLightPivot: Vec3 @16, PointLightIntensity: Vec4 @32, PointLightColor: Vec3 @48, AlphaAsColorBlendFactor: Float32 @64, PointLightFadeStartDistance: Float32 @68, PointLightFadeEndDistance: Float32 @72, PointLightRadius: Float32 @76, DistanceScaleMinClamp: Float32 @80, MaxDistanceForScaling: Float32 @84, SoftParticlesFadeDistanceMultiplier: Float32 @88, DistanceScaleMaxClamp: Float32 @92, MinScreenArea: Float32 @96, MaxSpawnDistance: Float32 @100, ZOcclusionLookup: Float32[] @104, MeshCullingDistance: Float32 @120, ParticleCullingFactor: Float32 @124, PointLightRandomIntensityMax: Float32 @128, PointLightRandomIntensityMin: Float32 @132, PointLightMaxClamp: Float32 @136, PointLightMinClamp: Float32 @140, EmittableType: EmittableType @144, VisibleAfterDistance: Float32 @148, Mesh: MeshAsset @152, Physics: HavokAsset @156, CompositeMeshPartNames: Uint32[] @160, MaxCount: Uint32 @176, RootProcessor: ProcessorData @180, Name: String @184, LifetimeFrameCount: Uint32 @196, Lifetime: Float32 @200, AmbientLightBlendFactor: Float32 @204, VertexPixelLightingBlendFactor: Float32 @208, GlobalLocalNormalBlendFactor: Float32 @212, FloatPhysics: FloatPhysicsData @216, PhysicsData: PhysicsEntityData @220, LocalSpace: Boolean @224, ForceNiceSorting: Boolean @225, KillParticlesWithEmitter: Boolean @226, FreezeParticleOnSleep: Boolean @227, Opaque: Boolean @228, OverrideScreenAreaCulling: Boolean @229, ActAsPointLight: Boolean @230, Emissive: Boolean @231, FollowSpawnSource: Boolean @232 |
| EmitterTextureAsset | class | Asset | 64 | ScenePath: String @24, Metrics: EmitterSystemMetrics @36, EmitterAssets: EmitterAsset[] @44 |
| EvaluatorData | class | EmitterComponentData | 24 | EmitterParameter: EmitterParameter @16, EvaluatorType: EvaluatorType @20 |
| Expression | class | DataContainer | 32 | Components: ExpressionComponent[] @12 |
| ExpressionCollectionData | class | Editable | 44 | Expressions: Expression[] @24 |
| ExpressionComponent | class | DataContainer | 64 | Coefficients: Vec4 @16, MinClamp: Float32 @32, MaxClamp: Float32 @36, ExpressionType: ExpressionType @40, Operator: ExpressionOperator @44, ScaleValue: Float32 @48 |
| GravityData | class | ProcessorData | 64 | Gravity: Float32 @48 |
| LocalForceData | class | ProcessorData | 64 | LocalForce: Vec3 @48 |
| MeshEvaluatorData | class | EvaluatorData | 60 | MinNormalizedIsoValue: Float32 @24, MaxNormalizedIsoValue: Float32 @28, UniformPointCount: Uint32 @32, DistributionData: Vec3[] @36, MeshAsset: MeshAsset @52, UseDirectionData: Boolean @56 |
| MultiplyData | class | EvaluatorData | 24 | - |
| PolynomialColorInterpData | class | EvaluatorData | 80 | Coefficients: Vec4 @32, Color1: Vec3 @48, Color0: Vec3 @64 |
| PolynomialData | class | EvaluatorData | 64 | Coefficients: Vec4 @32, MaxClamp: Float32 @48, MinClamp: Float32 @52, ScaleValue: Float32 @56 |
| PolynomialOperatorData | class | EvaluatorData | 112 | MinClampResult: Float32 @24, MaxClampResult: Float32 @28, Operation: PolynomialOperation @32, FirstOperand: PolynomialTempData @48, SecondOperand: PolynomialTempData @80 |
| ProcessorConnection | class | DataContainer | 20 | Dest: ProcessorData @12, Source: ProcessorData @16 |
| ProcessorData | class | EmitterComponentData | 48 | DefaultValues: Vec4 @16, NextProcessor: ProcessorData @32, EvaluatorInput: EmittableField @36, ProcessorType: ProcessorType @40, Enable: Boolean @44, AcceptsEvaluator: Boolean @45, Edited: Boolean @46 |
| RandomEvaluatorData | class | EvaluatorData | 32 | Min: Float32 @24, Max: Float32 @28 |
| RandomXYZEvaluatorData | class | EvaluatorData | 48 | MinY: Float32 @24, MaxZ: Float32 @28, MinZ: Float32 @32, MaxX: Float32 @36, MinX: Float32 @40, MaxY: Float32 @44 |
| RotateVectorData | class | EvaluatorData | 32 | Angle: Float32 @24, RotateWithinPlane: Boolean @28 |
| SampleTextureData | class | EvaluatorData | 112 | ColorIntensityMin: Vec3 @32, ColorIntensityMax: Vec3 @48, TextureDimensions: Vec2 @64, TextureOriginU: Float32 @72, TextureOriginV: Float32 @76, GradientData: Vec4[] @80, Texture: TextureAsset @96 |
| SpawnAnimationData | class | ProcessorData | 64 | AnimationSpeed: Float32 @48 |
| SpawnAnimationFrameData | class | ProcessorData | 64 | AnimationFrame: Uint32 @48 |
| SpawnCompositeMeshPartsData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| SpawnDirectionData | class | ProcessorData | 64 | InheritSpeedAndDirectionFromEmitter: Boolean @48, WaitingForUbbe: Boolean @49 |
| SpawnPositionData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| SpawnRateData | class | ProcessorData | 64 | DynamicFrequencyAreaClamp: Float32 @48, TrailSegmentLength: Float32 @52, SpawnRate: Float32 @56, DistributeOverDistance: Boolean @60, DistributeOverTime: Boolean @61 |
| SpawnRotationData | class | ProcessorData | 64 | Rotation: Float32 @48 |
| SpawnRotationSpeedData | class | ProcessorData | 64 | RotationSpeed: Float32 @48 |
| SpawnSizeData | class | ProcessorData | 64 | Size: Float32 @48 |
| SpawnSpeedData | class | ProcessorData | 64 | Speed: Float32 @48 |
| SphereEvaluatorData | class | EvaluatorData | 80 | Pivot: Vec3 @32, Scale: Vec3 @48, Radius: Float32 @64 |
| TurbulanceData | class | ProcessorData | 64 | Intensity: Float32 @48 |
| UpdateAgeData | class | ProcessorData | 64 | Lifetime: Float32 @48 |
| UpdateAlphaLevelMaxData | class | ProcessorData | 64 | MaxLevel: Float32 @48 |
| UpdateAlphaLevelMinData | class | ProcessorData | 64 | MinLevel: Float32 @48 |
| UpdateAlphaLevelScaleData | class | ProcessorData | 64 | Exponent: Float32 @48 |
| UpdateBoundingBoxData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| UpdateClipScaleData | class | ProcessorData | 64 | Lookup: Float32[] @48 |
| UpdateCollisionData | class | ProcessorData | 224 | Elasticity: Float32 @48, Penetration: Float32 @52 |
| UpdateColorData | class | ProcessorData | 64 | Color: Vec3 @48 |
| UpdateLinearVelocityData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| UpdateOrientationData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| UpdateRotationData | class | ProcessorData | 96 | TextureInfo1: EmitterTextureInfo @48 |
| UpdateSizeData | class | ProcessorData | 64 | Pivot: Vec2 @48 |
| UpdateSizeXData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| UpdateSizeYData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| UpdateSizeZData | class | ProcessorData | 64 | WaitingForUbbe: Boolean @48 |
| UpdateTextureCoordsData | class | ProcessorData | 96 | TextureBlendValue: Float32 @48, TextureInfo1: EmitterTextureInfo @52 |
| UpdateTransparencyData | class | ProcessorData | 64 | CullThreshold: Float32 @48 |
| ZRotate2DData | class | EvaluatorData | 24 | - |
<!-- /typeinfo -->

<!-- typeinfo:emitterrender-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| ClientEmitterEntity | class | EmitterEntity | 160 | - |
| DebrisContainerEntity | class | Entity | 12 | - |
| EmitterSystemSettings | class | DataContainer | 120 | QuadHalfResSlice1ThresholdLow: Float32 @12, QuadZOcclusionBias: Float32 @16, QuadHalfResSlice1ThresholdHigh: Float32 @20, FreezeAtEmittableCount: Uint32 @24, MinScreenArea: Float32 @28, QuadTechnique: Int32 @32, QuadPs3CullingFactorMultiplier: Float32 @36, QuadKeptParticlesAlphaBoost: Float32 @40, QuadProjectionScaleRampEnd: Float32 @44, QuadProjectionScaleRampStart: Float32 @48, DrawBoundingBoxes: Uint32 @52, MeshStreamingPriorityMultiplier: Float32 @56, MeshDrawCountLimit: Uint32 @60, GlobalResetStartTimeInterval: Float32 @64, QuadHalfResSlice2ThresholdHigh: Float32 @68, TimeScale: Float32 @72, MeshCullingDistance: Float32 @76, DrawStats: Uint32 @80, DebugOverdrawCount: Int32 @84, QuadHalfResSlice2ThresholdLow: Float32 @88, QuadGroupsJoinNone: Boolean @92, MeshDrawCullStats: Boolean @93, QuadGroupsJoinNiceAndSimple: Boolean @94, MeshDrawBoundingBoxes: Boolean @95, MeshShadowEnable: Boolean @96, MeshViewCullingEnable: Boolean @97, MeshDrawTransforms: Boolean @98, QuadVertexShadowsEnable: Boolean @99, MeshRenderingEnable: Boolean @100, MeshHavokRenderingEnable: Boolean @101, QuadGroupsJoinAll: Boolean @102, EnableRendering: Boolean @103, EnableRealtimeTweaks: Boolean @104, QuadNiceRenderingEnable: Boolean @105, QuadEnableRendering: Boolean @106, EnableFixedTimeStep: Boolean @107, Enable: Boolean @108, EnableJobs: Boolean @109, EnableFixedDelta: Boolean @110, QuadSimpleRenderingEnable: Boolean @111, QuadEnableProjectionScaleCulling: Boolean @112, QuadEnableWireframe: Boolean @113, QuadHalfResEnable: Boolean @114, QuadEnableZOcclusion: Boolean @115, QuadEnableOverdrawRendering: Boolean @116, QuadEnableOpaque: Boolean @117, QuadEnableSorting: Boolean @118, QuadColorShaderCostsEnable: Boolean @119 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Emitter 76, EmitterRender 3 records]

### RTTI-DECAL-1: `Decal`

Status: **verified** · Variants: PC

The decals and the soot (DSTR-*, SOOT-*): decal templates and sets, the soot templates and material data, the
`SootMesh` resource object.

<!-- typeinfo:decal-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| ContainerEntity | class | Entity | 28 | - |
| DecalSetEntity | class | Entity | 28 | - |
| DecalSettings | class | DataContainer | 56 | PendingDecalsRingBufferSize: Uint32 @12, ClipAngle: Float32 @16, AlphaFadeAt: Float32 @20, ActiveDecalsRingBufferSize: Uint32 @24, DecalSetRingBufferSize: Uint32 @28, Bias: Float32 @32, ProximityDistance: Float32 @36, PendingImpactsRingBufferSize: Uint32 @40, NormalOffset: Float32 @44, DrawDebugMemUsage: Boolean @48, DrawDebugTransforms: Boolean @49, UseVisualMeshes: Boolean @50, DrawEnable: Boolean @51, Enable: Boolean @52, DumpDebugInfo: Boolean @53 |
| DecalTemplateAsset | class | Asset | 24 | - |
| DecalTemplateData | class | DecalTemplateAsset | 68 | RandomRotation: Float32 @24, TimeToLive: Float32 @28, ShaderAssets: ShaderAsset[] @32, Size: Float32 @52, RandomSize: Float32 @56, Rotation: Float32 @60, IsStatic: Boolean @64 |
| MaterialRelationDecalData | class | MaterialRelationPropertyData | 20 | Decal: DecalTemplateData @16 |
| SootMaterialData | class | DataContainer | 24 | NotDarkerThan: Float32 @12, MaterialMaskingThreshold: Float32 @16, SootEnabled: Boolean @20 |
| SootMesh | class | - | 128 | - |
| SootTemplateAsset | class | Asset | 24 | - |
| SootTemplateData | class | SootTemplateAsset | 76 | RandomRadius: Float32 @24, Rotation: Float32 @28, RandomRotation: Float32 @32, Radius: Float32 @36, Name: String @40, ShaderAsset: String @52, TextureAsset: String @64 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Decal 10 records]

## 5. Terrain, vegetation and weather

### RTTI-TERR-1: `Terrain` and `TerrainRender`

Status: **verified** · Variants: PC

The terrain's records (TERR-*): the terrain data and its layers, the height field and material map resources, the
decals and spline planes, the water, the mask textures; the visual terrain entity and its view modes.

<!-- typeinfo:terrain-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| TerrainDecalTexCoordParams | value type | - | 20 | TileCountX: Uint32 @0, TileCountY: Uint32 @4, TileIndexX: Uint32 @8, TileIndexY: Uint32 @12, FlipY: Boolean @16, FlipX: Boolean @17 |
| TerrainMaskFunction | value type | - | 16 | MinSlope: Float32 @0, MaxSlope: Float32 @4, MinAltitude: Float32 @8, MaxAltitude: Float32 @12 |
| TerrainMaskTexture | value type | - | 32 | PosMax: Vec2 @0, PosMin: Vec2 @8, File: FileRef @16, Enable: Boolean @28 |
| TerrainMaskTextureType | enumeration | - | 4 | TmttNone 0, TmttStandard 1, TmttArea 2 |
| TerrainQuadDecalConnection | value type | - | 16 | Point2: Vec2 @0, Point1: Vec2 @8 |
| TerrainSplinePlaneType | enumeration | - | 4 | Road 0, River 1, FillConvex 2, FilledInner 3, Lake 4 |
<!-- /typeinfo -->

<!-- typeinfo:terrain-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| HeightfieldProxy | class | DataContainer | 12 | - |
| ITerrain | class | - | 4 | - |
| IWaterMesh | class | - | 4 | - |
| Terrain | class | ITerrain | 56 | - |
| TerrainAsset | class | Asset | 24 | - |
| TerrainData | class | TerrainAsset | 116 | SizeXZ: Float32 @24, MaterialMaskStepSize: Uint32 @28, MaskTileSize: Uint32 @32, TerrainShadowMap: TextureAsset @36, MaterialMap: TerrainMaterialMapAsset @40, ConvertedTerrainSplines: TerrainSplineData[] @44, TerrainSplines: TerrainSplineData[] @64, Heightfields: TerrainHeightfieldData[] @84, RootDecal: TerrainGroupDecal @104, Water: WaterData @108, NotLoadedInGame: Boolean @112, DefaultFullDepthEnable: Boolean @113 |
| TerrainDecal | class | DataContainer | 36 | TessellationTriangleSize: Float32 @12, Name: String @16, Material: TerrainDecalMaterial @28, MatchTerrainTesselationEnable: Boolean @32, Enable: Boolean @33 |
| TerrainDecalData | class | DataContainer | 12 | - |
| TerrainDecalMaterial | class | Asset | 96 | DebugColor: Vec3 @32, Shader: ShaderAsset @48, ZOnlyShader: ShaderAsset @52, MaskFunctions: TerrainMaskFunction[] @56, UndergrowthTypes: TerrainUndergrowthType[] @72, Enable: Boolean @92, BaseMaterialEnable: Boolean @93, UndergrowthEnable: Boolean @94, OpaqueMaskEnable: Boolean @95 |
| TerrainFullDecal | class | TerrainDecal | 68 | GridCellSize: Float32 @36, GridCellResolution: Uint32 @40, MaskPixelsPerMeter: Float32 @44, MaskTextures: TerrainMaskTexture[] @48, MaskTextureType: TerrainMaskTextureType @64 |
| TerrainGroupDecal | class | TerrainDecal | 56 | Decals: TerrainDecal[] @36 |
| TerrainHeightfield | class | - | 288 | - |
| TerrainHeightfieldData | class | DataContainer | 80 | InnerResolution: Uint32 @12, SizeXZ: Float32 @16, SizeY: Float32 @20, Resolution: Uint32 @24, Files: FileRef[] @28, FileGrid: TerrainHeightfieldGridCellInfo[] @44, DepthFiles: FileRef[] @64 |
| TerrainHeightfieldGridCellInfo | class | DataContainer | 64 | BottomRightCorner: Vec3 @16, TopLeftCorner: Vec3 @32, File: FileRef @48 |
| TerrainLayerData | class | DataContainer | 12 | - |
| TerrainMaterialMap | class | - | 128 | - |
| TerrainMaterialMapAsset | class | Asset | 52 | MaterialMapFileName: FileRef @24, MaterialNames: String[] @36 |
| TerrainQuadDecal | class | TerrainDecal | 160 | UserMasks: Vec4 @48, Transform: LinearTransform @64, TexCoord: TerrainDecalTexCoordParams @128, Template: TerrainQuadDecalTemplate @148 |
| TerrainQuadDecalTemplate | class | Asset | 60 | TexCoord: TerrainDecalTexCoordParams @24, Connections: TerrainQuadDecalConnection[] @44 |
| TerrainQuadGeometryData | class | DataContainer | 12 | - |
| TerrainSplineData | class | GameObjectData | 100 | AttachedEnd: TerrainSplinePointData @48, InnerSpline: TerrainSplineData @52, AttachedStart: TerrainSplinePointData @56, Points: TerrainSplinePointData[] @60, Planes: TerrainSplinePlaneData[] @80 |
| TerrainSplineDecal | class | TerrainDecal | 48 | InnerSpline: TerrainSplineData @36, Spline: TerrainSplineData @40, UseInnerSpline: Boolean @44 |
| TerrainSplinePlaneData | class | DataContainer | 144 | DefaultUserMasksRight: Vec4 @16, DefaultUserMasksLeft: Vec4 @32, DefaultWaveHeightLeft: Float32 @48, DefaultWaveHeightRight: Float32 @52, DefaultFoamRight: Float32 @56, DefaultLift: Float32 @60, DefaultFoamLeft: Float32 @64, FilledSplineRepeatY: Float32 @68, FilledSplineRotation: Float32 @72, FilledSplineRepeatX: Float32 @76, BaseSpeed: Float32 @80, NoiseWidth: Float32 @84, StartIndex: Int32 @88, EndIndex: Int32 @92, PlaneType: TerrainSplinePlaneType @96, Points: TerrainSplinePlanePointData[] @100, Parent: TerrainSplineData @120, DefaultRightEdge: Float32 @124, DefaultTextureStretch: Float32 @128, DefaultLeftEdge: Float32 @132, DefaultSegments: Int32 @136, DefaultAlpha: Float32 @140 |
| TerrainSplinePlanePointData | class | DataContainer | 96 | UserMasksRight: Vec4 @16, UserMasksLeft: Vec4 @32, UserMasks: Vec4 @48, RightEdge: Float32 @64, WaterSpeed: Float32 @68, LeftEdge: Float32 @72, AffectedPoint: TerrainSplinePointData @76, Segments: Int32 @80 |
| TerrainSplinePointData | class | DataContainer | 48 | Position: Vec3 @16, Parent: TerrainSplineData @32 |
| TerrainUndergrowthType | class | DataContainer | 76 | MinMaskScaleFactor: Vec2 @12, MaxScale: Vec2 @20, MinScale: Vec2 @28, MaxMask: Float32 @36, MinMask: Float32 @40, ViewDistance: Float32 @44, WindScale: Float32 @48, Variation: Float32 @52, MeshShaderSet: MeshShaderSetAsset @56, Mesh: RigidMeshAsset @60, Density: Float32 @64, CacheShader: ShaderAsset @68, SkewEnable: Boolean @72, RotateEnable: Boolean @73, Enable: Boolean @74 |
| WaterData | class | Asset | 68 | LakeShader: ShaderAsset @24, RiverShader: ShaderAsset @28, LakeOutletShader: ShaderAsset @32, OceanShader: ShaderAsset @36, OceanLevel: Float32 @40, DeepDepth: Float32 @44, ShallowExtrude: Float32 @48, ShallowDepth: Float32 @52, DeepExtrude: Float32 @56, OceanRadius: Float32 @60, OceanEnable: Boolean @64 |
| WaterMesh | class | IWaterMesh | 44 | - |
<!-- /typeinfo -->

<!-- typeinfo:terrainrender-types -->
| Enumeration | Values |
|---|---|
| TerrainViewMode | TvmDefault 0, TvmSimple 1, TvmMaterialCount 2, TvmMaterialDebugColor 3, TvmMaterialCombo 4, TvmMaskCount 5, TvmOverdrawMask 6, TvmOverdrawCoverage 7 |
<!-- /typeinfo -->

<!-- typeinfo:terrainrender-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| IVisualTerrain | class | ITerrain | 4 | - |
| IVisualWater | class | - | 4 | - |
| VisualTerrain | class | IVisualTerrain | 9472 | - |
| VisualTerrainEntity | class | Entity | 24 | - |
| VisualTerrainEntityData | class | EntityData | 128 | Settings: VisualTerrainSettings @112, Terrain: TerrainData @116 |
| VisualTerrainSettings | class | DataContainer | 224 | OccludedMinDistance: Float32 @12, PatchSlotCount: Uint32 @16, PatchSampleSize: Uint32 @20, MemoryBufferSize: Uint32 @24, UndergrowthUpdateCellCount: Uint32 @28, UndergrowthBuildJobCount: Uint32 @32, UndergrowthPoplessDistanceRatio: Float32 @36, DynamicMaskPoolWidth: Uint32 @40, DynamicMaskTileSize: Uint32 @44, BuildJobCount: Uint32 @48, DynamicMaskPoolHeight: Uint32 @52, MaxNonVisibleTextureUpdateCount: Uint32 @56, DynamicMaskDecalWidthScale: Float32 @60, DynamicMaskPixelsPerMeter: Uint32 @64, UndergrowthCellSize: Float32 @68, UndergrowthViewDistanceScale: Float32 @72, UndergrowthDensityScale: Float32 @76, UndergrowthSinkSafeRadius: Float32 @80, UndergrowthViewDistanceScaleMax: Float32 @84, UndergrowthMaxCullInstanceCount: Uint32 @88, UndergrowthInstanceCullJobCount: Uint32 @92, UndergrowthOverlapDensityScale: Float32 @96, UndergrowthForceCellSideCount: Int32 @100, UndergrowthCellCacheTextureSize: Uint32 @104, UndergrowthCellMaskTextureSize: Uint32 @108, UndergrowthMaxCellSideCount: Uint32 @112, UndergrowthSinkVelocity: Float32 @116, UndergrowthWindSpeed: Float32 @120, UndergrowthMaxHeight: Float32 @124, DecalZPassDistance: Float32 @128, DecalBias: Float32 @132, ForceGeometryLodLevel: Int32 @136, ZPassDistance: Float32 @140, SlotReuseWaitCount: Uint32 @144, GeometryLodScale: Float32 @148, ViewMode: TerrainViewMode @152, DecalOffsetY: Float32 @156, DecalZPassBias: Float32 @160, DrawPatchesEnable: Boolean @164, DrawEnable: Boolean @165, DrawMainPatchesEnable: Boolean @166, DrawDecalEnable: Boolean @167, DrawSurroundingPatchesEnable: Boolean @168, DrawPatchBoxesEnable: Boolean @169, DrawTextureTileBoxesEnable: Boolean @170, UndergrowthBuildSpuJobEnable: Boolean @171, DrawDecalZPassEnable: Boolean @172, DrawOnlyDecalZPassEnable: Boolean @173, DrawUndergrowthDecalsEnable: Boolean @174, Enable: Boolean @175, Ps3UndergrowthInstanceCellMemoryEnable: Boolean @176, UndergrowthInstanceCullDynamicAllocEnable: Boolean @177, DrawUndergrowthInstanceBoxesEnable: Boolean @178, OccluderJobEnable: Boolean @179, CullSampleBoundingBoxHeightEnable: Boolean @180, OccludedEnable: Boolean @181, OccluderSpuJobEnable: Boolean @182, UndergrowthInstanceAdditionalCullEnable: Boolean @183, WireframeEnable: Boolean @184, UndergrowthInstanceCullEnable: Boolean @185, UndergrowthSpuInstancingEnable: Boolean @186, UndergrowthInstanceCullSpuJobEnable: Boolean @187, UndergrowthInstanceOcclusionCullEnable: Boolean @188, UndergrowthInstanceFrustumCullEnable: Boolean @189, UndergrowthInstanceCullBoxTestEnable: Boolean @190, UndergrowthUpdateJobsEnable: Boolean @191, SlotDebugOutputEnable: Boolean @192, ZeroViewportEnable: Boolean @193, DynamicMaskEnable: Boolean @194, BuildDrawJobEnable: Boolean @195, PatchMaterialSortingEnable: Boolean @196, DrawWaterEnable: Boolean @197, SimpleWater: Boolean @198, PatchLodTransitionsEnable: Boolean @199, BuildSpuJobEnable: Boolean @200, UpdateJobsEnable: Boolean @201, CullSpuJobEnable: Boolean @202, PrepareBuildSpuJobEnable: Boolean @203, ModifiersEnable: Boolean @204, HeightModificationEnable: Boolean @205, PrioritizeCullJobsEnable: Boolean @206, RegenerateTexturesEnable: Boolean @207, UndergrowthTerrainColoringEnable: Boolean @208, UndergrowthGenerateEnable: Boolean @209, DrawUndergrowthDebugCacheTexturesEnable: Boolean @210, DrawUndergrowthStatsEnable: Boolean @211, UndergrowthSimpleShaderEnable: Boolean @212, DrawDebugTextEnable: Boolean @213, UndergrowthRegenerateEnable: Boolean @214, UndergrowthZPassEnable: Boolean @215, DrawDebugTexturesEnable: Boolean @216, UndergrowthEnable: Boolean @217, HeightfieldMipmapsEnable: Boolean @218, CastShadowsEnable: Boolean @219, DrawUndergrowthCellBoxesEnable: Boolean @220, DrawUndergrowthDebugMaskTexturesEnable: Boolean @221, UndergrowthCastShadowsEnable: Boolean @222, DrawUndergrowthEnable: Boolean @223 |
| VisualWater | class | - | 96 | - |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Terrain 34, TerrainRender 8 records]

### RTTI-OVG-1: `Overgrowth` and `OvergrowthRender`

Status: **verified** · Variants: PC

The trees' records (VEG-*): the tree mesh entity data, its dummy, the overgrowth's settings and run-time tree
mesh entity.

<!-- typeinfo:overgrowth-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| TreeMeshDummyData | class | DataContainer | 12 | - |
| TreeMeshEntityData | class | EntityData | 128 | Mesh: TreeMeshAsset @112, EnableLeaves: Boolean @116 |
<!-- /typeinfo -->

<!-- typeinfo:overgrowthrender-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| OvergrowthSettings | class | DataContainer | 116 | AdaptiveLodMVertexBudget: Float32 @12, GlobalLodScale: Float32 @16, BillboardShadowOffset: Float32 @20, MaxBillboardCount: Uint32 @24, BillboardScale: Float32 @28, AdaptiveLodSpeed: Float32 @32, InputsPerJob: Int32 @36, InputsPerSpuJob: Int32 @40, AdaptiveLodMaxModifier: Float32 @44, AdaptiveLodSpeed2: Float32 @48, AdaptiveLodMinModifier: Float32 @52, ForceTechnique: Int32 @56, BranchWindAnimationSpeed: Float32 @60, BranchConstWind: Float32 @64, HiQualityLod: Int32 @68, AdaptiveLodMargin: Float32 @72, ShadowMaxLod: Uint32 @76, BranchWindPower: Float32 @80, ZPassLod: Int32 @84, ForceBendingType: Int32 @88, LodBlendOverlap: Float32 @92, LodBlendBillboardOverlap: Float32 @96, UseMaxShadowLOD: Boolean @100, WindEnable: Boolean @101, AlmostOnlyBillboardsEnable: Boolean @102, OnlyBillboardsEnable: Boolean @103, HiLodEnable: Boolean @104, InstancingEnable: Boolean @105, LodBlendEnable: Boolean @106, CastShadowsEnable: Boolean @107, ZeroViewportEnable: Boolean @108, AdaptiveLodEnable: Boolean @109, JobsEnable: Boolean @110, BillboardsEnable: Boolean @111, Enable: Boolean @112 |
| TreeMeshEntity | class | Entity | 96 | - |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Overgrowth 2, OvergrowthRender 2 records]

### RTTI-WEATH-1: `Weather`

Status: **verified** · Variants: PC

The weather's records (WTH-*): the sky, sun, cloud layers (static and dynamic, plane, volume, panoramic), lens
flares, the weather states and their interpolation, the weather system entity.

<!-- typeinfo:weather-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| DynamicCloudPlaneLayerData | value type | - | 96 | AmbientColorThin: Vec3 @0, SimpleLayerTiling: Vec2 @16, Gradient: Vec4 @32, AmbientColorDense: Vec3 @48, LevelMid: Float32 @64, Ka: Float32 @68, LevelHigh: Float32 @72, GlowSize: Float32 @76, Ksc: Float32 @80, LevelLow: Float32 @84 |
| DynamicCloudVolumeLayerData | value type | - | 128 | AmbientColorDense: Vec3 @0, AmbientColorThin: Vec3 @16, Gradient: Vec4 @32, VolumetricLayerTiling: Vec2 @48, ParallaxScale: Float32 @56, LevelMid: Float32 @60, NormalWrapBias: Float32 @64, Ka: Float32 @68, NormalMipBias: Float32 @72, MinSamples: Float32 @76, MaxSamples: Float32 @80, Ksc: Float32 @84, Krim: Float32 @88, Kd: Float32 @92, Kg: Float32 @96, LevelLow: Float32 @100, LevelHigh: Float32 @104, GlowSize: Float32 @108, WrapAmount: Float32 @112 |
| DynamicPanoramicCloudLayerData | value type | - | 32 | AdditiveThreshhold: Float32 @0, TileFactor: Float32 @4, Rotation: Float32 @8, Trancparency: Float32 @12, PanoramicUVMaxX: Float32 @16, PanoramicUVMinX: Float32 @20, PanoramicUVMaxY: Float32 @24, PanoramicUVMinY: Float32 @28 |
| FogType | enumeration | - | 4 | FtDefault 0, FtLinearSegments 1 |
| PanoramicCloudBlendMode | enumeration | - | 4 | PcbmAdd 0, PcbmMultiplyAddAlpha 1 |
| StaticCloudPlaneLayerData | value type | - | 4 | Alphamap: TextureAsset @0 |
| StaticCloudVolumeLayerData | value type | - | 4 | Alphamap: TextureAsset @0 |
| StaticPanoramicCloudLayerData | value type | - | 8 | PanoramicAlphaTexture: TextureAsset @0, PanoramicTexture: TextureAsset @4 |
| WeatherstateInterpolationMode | enumeration | - | 4 | WimLinear 0, WimCubic 1, WimCatmullRom 2 |
<!-- /typeinfo -->

<!-- typeinfo:weather-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| DynamicCloudLayerData | class | Asset | 432 | UV01Offset: Vec4 @32, UV23Offset: Vec4 @48, Transform: LinearTransform @64, DistanceFadeRatio: Float32 @128, VolumetricCloudPlane: DynamicCloudVolumeLayerData @144, SimpleCloudPlane: DynamicCloudPlaneLayerData @272, PanoramicCloudLayer: DynamicPanoramicCloudLayerData @368, UV2WindInfluence: Float32 @400, UV1WindInfluence: Float32 @404, UV0WindInfluence: Float32 @408, UV3WindInfluence: Float32 @412, LinearMaskRotation: Float32 @416, LinearMaskMax: Float32 @420, LinearMaskMin: Float32 @424 |
| DynamicFogData | class | Asset | 144 | Color: Vec3 @32, ColorSun: Vec3 @48, LinearPoint1: Vec2 @64, LinearPoint3: Vec2 @72, LinearPoint2: Vec2 @80, ExponentialFogExponent: Float32 @88, LinearFogMultiplier: Float32 @92, Start: Float32 @96, FogBlendModifier: Float32 @100, Eccentrity: Float32 @104, ColorSunPower: Float32 @108, Clamp: Float32 @112, End: Float32 @116, HeightOffset: Float32 @120, HeightFalloff: Float32 @124, Density: Float32 @128 |
| DynamicLensFlareData | class | Asset | 160 | TexCoord1: Vec2 @24, Color4: Vec3 @32, TexCoord2: Vec2 @48, TexCoord4: Vec2 @56, TexCoord3: Vec2 @64, Color1: Vec3 @80, SunBleedColor: Vec3 @96, Color3: Vec3 @112, Color2: Vec3 @128, BloomFlareCount: Int32 @144, BlurScale: Float32 @148, BloomThreshold: Float32 @152 |
| DynamicOutdoorLightData | class | Asset | 240 | EnlightenSkyColor: Vec3 @32, Transform: LinearTransform @48, Direction: Vec3 @112, BillboardBottomColor: Vec3 @128, KeyColor: Vec3 @144, TopColor: Vec3 @160, BottomColor: Vec3 @176, DiffuseEnvmapScale: Float32 @192, ShadowScale: Float32 @196, CloudShadowScale: Float32 @200, BottomColorIntensity: Float32 @204, CloudShadowIntensity: Float32 @208, TopColorIntensity: Float32 @212, KeySpecularScale: Float32 @216, KeyColorIntensity: Float32 @220, ReflectionEnvmapScale: Float32 @224, HemisphereLightAngleFactor: Float32 @228, EnlightenSkyColorIntensity: Float32 @232 |
| DynamicSkyData | class | Asset | 240 | GroundColorReflection: Vec3 @32, Mu: Vec2 @48, Sigma: Vec2 @56, HazeColor: Vec3 @64, HaloColor: Vec3 @80, HorizontalCoefficientsR: Vec4 @96, HorizontalCoefficientsG: Vec4 @112, HorizontalCoefficientsB: Vec4 @128, HazeHeight: Float32 @144, EarthRadius: Float32 @148, Exposure: Float32 @152, Eccentrity: Float32 @156, Turbidity: Float32 @160, Nair: Float32 @164, Pair: Float32 @168, JungeExponent: Float32 @172, LambdaR: Float32 @176, LambdaG: Float32 @180, LambdaB: Float32 @184, ZenithHeight: Float32 @188, HaloAlphaGain: Float32 @192, HaloBleedGain: Float32 @196, HaloAlphaPower: Float32 @200, HorizontalGain: Float32 @204, HaloGain: Float32 @208, HaloBleedPower: Float32 @212, GroundFadeMinReflection: Float32 @216, GroundFadeMaxReflection: Float32 @220, HazeFadeMax: Float32 @224, HaloBackReflectionGain: Float32 @228, HazeFadeMin: Float32 @232 |
| DynamicSunData | class | Asset | 80 | Color: Vec3 @32, Direction: Vec3 @48, Theta: Float32 @64, Gain: Float32 @68, Phi: Float32 @72, Size: Float32 @76 |
| IWeatherSystem | class | - | 4 | - |
| LensFlareSpriteData | class | Asset | 96 | Color: Vec3 @32, BottomRightUV: Vec2 @48, TopLeftUV: Vec2 @56, RotateOnAngleExponent: Float32 @64, ColorExponent: Float32 @68, AngleTransparencyExponent: Float32 @72, SizeX: Float32 @76, SizeY: Float32 @80, Position: Float32 @84 |
| StaticCloudLayerData | class | Asset | 40 | PanoramicCloudLayer: StaticPanoramicCloudLayerData @24, VolumetricCloudPlane: StaticCloudVolumeLayerData @32, SimpleCloudPlane: StaticCloudPlaneLayerData @36 |
| StaticFogData | class | Asset | 28 | FunctionType: FogFunctionType @24 |
| StaticLensFlareData | class | Asset | 28 | LensFlareAtlas: TextureAsset @24 |
| StaticOutdoorLightData | class | Asset | 24 | - |
| StaticSkyData | class | Asset | 24 | - |
| StaticSunData | class | Asset | 24 | - |
| WeatherSystem | class | IWeatherSystem | 752 | - |
| WeatherSystemData | class | Asset | 48 | States: WeatherSystemState[] @24, Setup: WeatherSystemSetup @44 |
| WeatherSystemEntity | class | Entity | 36 | - |
| WeatherSystemEntityData | class | EntityData | 128 | Settings: WeatherSystemSettings @112, WeatherSystem: WeatherSystemData @116 |
| WeatherSystemInstanceData | class | DataContainer | 20 | Settings: WeatherSystemSettings @12, WeatherSystem: WeatherSystemData @16 |
| WeatherSystemSettings | class | Asset | 48 | FogType: FogType @24, PanoramicCloudBlendMode: PanoramicCloudBlendMode @28, StateFadeTimeSeconds: Float32 @32, InterpolationMode: WeatherstateInterpolationMode @36, ActiveState: Uint32 @40, Enable: Boolean @44, SkydomeEnable: Boolean @45, VolumetricCloudsEnable: Boolean @46 |
| WeatherSystemSetup | class | Asset | 56 | OutdoorLight: StaticOutdoorLightData @24, LensFlare: StaticLensFlareData @28, Fog: StaticFogData @32, Clouds: StaticCloudLayerData @36, R: Float32 @40, Sky: StaticSkyData @44, Sun: StaticSunData @48, ExtendedTerrainShadowsEnable: Boolean @52 |
| WeatherSystemState | class | Asset | 132 | Wind: Vec2 @24, TerrainShadowFrustumExtrusion: Float32 @32, FadeTransparentStart: Float32 @36, YOffset: Float32 @40, FadeTransparentEnd: Float32 @44, FadeTransparentClamp: Float32 @48, TerrainShadowFrustumTranslation: Float32 @52, SootTransparentFadeStart: Float32 @56, SootTransparentFadeEnd: Float32 @60, OvergrowthLodDistanceScale: Float32 @64, LowResShadowViewDistanceMultiplier: Float32 @68, LodDistanceScale: Float32 @72, MinScreenAreaScale: Float32 @76, OutdoorLight: DynamicOutdoorLightData @80, Fog: DynamicFogData @84, PostProcess: DynamicPostProcessData @88, Sky: DynamicSkyData @92, Sun: DynamicSunData @96, Clouds: DynamicCloudLayerData @100, WorldViewDistance: Float32 @104, Zmin: Float32 @108, Zmax: Float32 @112, ShadowViewDistance: Float32 @116, LensFlare: DynamicLensFlareData @120, ViewDistance: Float32 @124, ApplyFog: Boolean @128 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: Weather 31 records]

## 6. Interface

### RTTI-UI-1: `UI`

Status: **verified** · Variants: PC

The UI module's 45 records (MENU-*, HUD-*): the Flash movie interface, the minimap's configuration, shapes and zoom
states, the HUD icons, the 3D camera and geometry animations of the menus, the character sets, the text anchors.

<!-- typeinfo:ui-types -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| MinimapData | value type | - | 80 | OverlayColor: Vec3 @0, CameraPosition: Vec3 @16, Size: Vec2 @32, Position: Vec2 @40, CameraRotation: Float32 @48, OverlayAlpha: Float32 @52, CameraFov: Float32 @56, CameraDistance: Float32 @60, CameraLookDistance: Float32 @64, PositionFromPlayer: Boolean @68, RotationFromPlayer: Boolean @69 |
| UI3DCameraAnimation | value type | - | 112 | ProjectionMatrix: Mat4 @0, Samples: UI3DCameraSampledFrame[] @64, FromName: String @80, ToName: String @92, UseLookAt: Boolean @104 |
| UI3DCameraSampledFrame | value type | - | 64 | LookAtPosition: Vec3 @0, Rotation: Quat @16, Position: Vec3 @32, Time: Float32 @48 |
| UI3DGeometryAnimation | value type | - | 80 | MinBoundingCorner: Vec3 @0, MaxBoundingCorner: Vec3 @16, FromName: String @32, ToName: String @44, Samples: UI3DGeometrySampledFrame[] @56 |
| UI3DGeometrySampledFrame | value type | - | 64 | Scale: Vec3 @0, Position: Vec3 @16, Rotation: Quat @32, Opacity: Float32 @48, Time: Float32 @52 |
| UICharacterSet | enumeration | - | 4 | UICharacterSet_Latin1 0, UICharacterSet_Cyrillic 1, UICharacterSet_Kanji 2 |
| UIFileWrapper | value type | - | 12 | SourceFile: FileRef @0 |
| UIFontEffect | enumeration | - | 4 | UIFontEffect_None 0, UIFontEffect_Glow 1 |
| UIFontMapProxy | value type | - | 24 | Fonts: UIFontProxyAsset[] @0, Language: LanguageFormat @20 |
| UIFontStyle | enumeration | - | 4 | UIFontStyle_Regular 0, UIFontStyle_Bold 1, UIFontStyle_Italic 2 |
| UIHudIcon | enumeration | - | 4 | UIHudIcon_LocalPlayer 0, UIHudIcon_FriendlyPlayer 1, UIHudIcon_EnemyPlayer 2, UIHudIcon_NeutralPlayer 3, UIHudIcon_SquadMember 4, UIHudIcon_SquadLeader 5, UIHudIcon_Vehicle 6, UIHudIcon_PrimaryObjective 7, UIHudIcon_PrimaryObjectiveBlink 8, UIHudIcon_SecondaryObjective 9, UIHudIcon_AreaMapMarker 10, UIHudIcon_ObjectiveDestroy 11, UIHudIcon_ObjectiveScout 12, UIHudIcon_ObjectiveDefend 13, UIHudIcon_ObjectiveGoto 14, UIHudICon_UAV 15, UIHudIcon_AmmoCrate 16, UIHUDIcon_C4 17, UIHUDIcon_C4_small 18, UIHUDIcon_ATMine 19, UIHudIcon_StationaryWeapon 20, UIHudIcon_North 21, UIHudIcon_South 22, UIHudIcon_West 23, UIHudIcon_East 24, UIHudIcon_NeutralFlag 25, UIHudIcon_FriendlyFlag 26, UIHudIcon_EnemyFlag 27, UIHudIcon_NeutralFlagLit 28, UIHudIcon_FriendlyFlagLit 29, UIHudIcon_EnemyFlagLit 30, UIHudIcon_SelectedSpawnPoint 31, UIHudIcon_FriendlyFlagUnderAttack 32, UIHudIcon_EnemyFlagUnderAttack 33, UIHudIcon_OrderAttack 34, UIHudIcon_OrderDefend 35, UIHudIcon_BoatNeutral 36, UIHudIcon_BoatFriendly 37, UIHudIcon_BoatEnemy 38, UIHudIcon_BoatSquad 39, UIHudIcon_CarNeutral 40, UIHudIcon_CarFriendly 41, UIHudIcon_CarEnemy 42, UIHudIcon_CarSquad 43, UIHudIcon_HeliNeutral 44, UIHudIcon_HeliFriendly 45, UIHudIcon_HeliEnemy 46, UIHudIcon_HeliSquad 47, UIHudIcon_TankNeutral 48, UIHudIcon_TankFriendly 49, UIHudIcon_TankEnemy 50, UIHudIcon_TankSquad 51, UIHudIcon_Stationary 52, UIHudIcon_Strategic 53, UIHudIcon_MotionRadarSweep 54, UIHudIcon_NeedBackup 55, UIHudIcon_NeedAmmo 56, UIHudIcon_NeedMedic 57, UIHudIcon_NeedPickup 58, UIHudIcon_NeedRepair 59, UIHudIcon_KitAssault 60, UIHudIcon_KitDemolition 61, UIHudIcon_KitRecon 62, UIHudIcon_KitSpecialist 63, UIHudIcon_KitSupport 64, UIHudIcon_KitPickupAssault 65, UIHudIcon_KitPickupDemolition 66, UIHudIcon_KitPickupRecon 67, UIHudIcon_KitPickupSpecialist 68, UIHudIcon_KitPickupSupport 69, UIHudIcon_Pickup 70, UIHudIcon_TaggedVehicle 71, UIHudIcon_HeliTargetEnemy 72, UIHudIcon_HeliTargetFriendly 73, UIHudIcon_ArtilleryTarget 74, UIHudIcon_NeutralFlagAttacker 75, UIHudIcon_FriendlyFlagAttacker 76, UIHudIcon_EnemyFlagAttacker 77, UIHudIcon_LaserTarget 78, UIHudIcon_ObjectiveAttacker 79, UIHudIcon_ObjectiveDefender 80, UIHudIcon_RadarSweepComponent 81, UIHudIcon_Blank 82, UIHudIcon_LocalPlayerBigIcon 83, UIHudIcon_LocalPlayerOutOfMap 84, UIHudIcon_PrimaryObjectiveLarge 85, UIHudIcon_RpgTargetUnlocked 86, UIHudIcon_RpgTargetLocked 87, UIHudIcon_RpgTargetLocking 88, UIHudIcon_ArtilleryStrikeNametag 89, UIHudIcon_ArtilleryStrikeMinimap 90, UIHudIcon_ConquestFlagNeutral 91, UIHudIcon_ConquestFlagFriendly 92, UIHudIcon_ConquestFlagEnemy 93, UIHudIcon_ConquestFlagFriendlyUncap 94, UIHudIcon_ConquestFlagEnemyUncap 95, UIHudIcon_ConquestFlagFriendlySelected 96, UIHudIcon_ConquestFlagFriendlySquadSelected 97, UIHudIcon_Grenade 98, UIHudIcon_GrenadeArrow 99, UIHudIcon_FocusPointAim 100, UIHudIcon_FocusPointLocked 101, UIHudIcon_FocusPointAttack 102, UIHudIcon_FocusPointDefend 103, UIHudIcon_FlynnCrate 104, UIHudIcon_TeamObjectMedikit 105, UIHudIcon_TeamObjectAmmo 106, UIHudIcon_TeamObjectDeadSoldier 107, UIHudIcon_TeamObjectArrow 108, UIHudIcon_RadioOrderLetMeHeal 109, UIHudIcon_RadioOrderRequestHeal 110, UIHudIcon_RadioOrderRequestAmmo 111, UIHudIcon_RadioOrderLetMeGiveAmmo 112, UIHudIcon_RadioOrderLetMeRepair 113, UIHudIcon_RadioOrderRequestRide 114, UIHudIcon_RadioOrderSocialise 115, UIHudIcon_RadioOrderHighlight 116, UIHudIcon_MPObjectiveArrow 117, UIHudIcon_UAVNeutral 118, UIHudIcon_UAVFriendly 119, UIHudIcon_UAVEnemy 120, UIHudIcon_UAVSquad 121, UIHudIcon_StationaryUAV 122, UIHudIcon_SPSniperMapMarker 123, UIHudIcon_AirplaneNeutral 124, UIHudIcon_AirplaneFriendly 125, UIHudIcon_AirplaneEnemy 126, UIHudIcon_AirplaneSquad 127, UIHudIcon_SPObjectiveArrow 128, UIHudIcon_SDMObjective 129, UIHudIcon_HealthbarBackgroundWhite 130, UIHudIcon_HealthbarWhite 131, UIHudIcon_HealthbarBackgroundGreen 132, UIHudIcon_HealthbarGreen 133, UIHudIcon_HealthbarBackgroundBlue 134, UIHudIcon_HealthbarBlue 135, UIHudIcon_HealthbarBackgroundRed 136, UIHudIcon_HealthbarRed 137, UIHudIcon_StickOnBorderSpotedEnemy 138, UIHudIcon_StickOnBorderPrimaryObjective 139, UIHudIcon_TeamObjectDeadSoldierSquad 140, UIHudIcon_RadioOrderLetMeHealRequest 141, UIHudIcon_RadioOrderRequestHealRequest 142, UIHudIcon_RadioOrderRequestAmmoRequest 143, UIHudIcon_RadioOrderLetMeGiveAmmoRequest 144, UIHudIcon_RadioOrderLetMeRepairRequest 145, UIHudIcon_RadioOrderRequestRideRequest 146, UIHudIcon_SP01ObjectiveArrow 147, UIHudIcon_SP01PrimaryObjectiveLarge 148, UIHudIcon_Count 149 |
| UIMinimapConfig | value type | - | 72 | ZoomConversion: UIMinimapZoomConversion @0, VelocityModifier: Float32 @16, MaxVelocity: Float32 @20, DiamondFrameTexture: TextureAsset @24, CombatAreaDetailTexture: TextureAsset @28, CompassTexture: TextureAsset @32, CombatAreaFadeSpeed: Float32 @36, CameraInterpolationTime: Float32 @40, CombatAreaCameraDelay: Float32 @44, MinVelocity: Float32 @48, CameraPanSensitivity: Float32 @52, ZoomStates: UIMinimapZoomConfig[] @56 |
| UIMinimapShape | value type | - | 8 | AlphaMaskTexture: TextureAsset @0, ShapeType: UIMinimapShapeType @4 |
| UIMinimapShapeType | enumeration | - | 4 | UIMinimapShapeType_Rectangle 0, UIMinimapShapeType_Circle 1, UIMinimapShapeType_Square 2, UIMinimapShapeType_Diamond 3, UIMinimapShapeType_Count 4 |
| UIMinimapZoomConfig | value type | - | 112 | Shape: UIMinimapShape @0, Data: MinimapData @16, State: UIMinimapZoomState @96 |
| UIMinimapZoomConversion | value type | - | 16 | CameraMaxDistance: Float32 @0, MaxZoom: Float32 @4, CameraMinDistance: Float32 @8, MinZoom: Float32 @12 |
| UIMinimapZoomState | enumeration | - | 4 | UIMinimapZoomState_Default 0, UIMinimapZoomState_Overview 1, UIMinimapZoomState_CombatArea 2, UIMinimapZoomState_Mission 3, UIMinimapZoomState_Artillery 4, UIMinimapZoomState_Spawn 5, UIMinimapZoomState_None 6, UIMinimapZoomState_Default_Nam 7 |
| UIMovieViewport | value type | - | 24 | Height: Float32 @0, TargetWidth: Float32 @4, TargetHeight: Float32 @8, X: Float32 @12, Y: Float32 @16, Width: Float32 @20 |
| UITaggedVehicleCrosshair | value type | - | 16 | MinSize: Int32 @0, MaxSize: Int32 @4, MinDistance: Float32 @8, MaxDistance: Float32 @12 |
| UITextAnchor | enumeration | - | 4 | UITextAnchor_None 0, UITextAnchor_Left 1, UITextAnchor_Right 2, UITextAnchor_Top 3, UITextAnchor_Bottom 4, UITextAnchor_Center 5 |
<!-- /typeinfo -->

<!-- typeinfo:ui-classes -->
| Type | Kind | Parent | Size | Own members (name: type @offset) or values |
|---|---|---|---|---|
| ISwfMovie | class | - | 4 | - |
| LocalizationAsset | class | Asset | 84 | EulaFontMaps: UIFontMapAsset[] @24, FontMaps: UIFontMapAsset[] @44, LocalizedTexts: UITextDatabase[] @64 |
| SwfMovie | class | ISwfMovie | 20 | - |
| UI3DSystemAsset | class | Asset | 56 | GeometryAnimations: UI3DGeometryAnimation[] @24, CameraAnimations: UI3DCameraAnimation[] @40 |
| UIAnimatedTextureData | class | DataContainer | 32 | Groups: UIAnimatedTextureGroup[] @12 |
| UIAnimatedTextureFrame | class | DataContainer | 48 | UVRect: Vec4 @16, Time: Float32 @32 |
| UIAnimatedTextureGroup | class | DataContainer | 112 | IconColor: Vec4 @16, VerticalTextAnchor: UITextAnchor @32, RotationSpeed: Float32 @36, HorizontalTextAnchor: UITextAnchor @40, MaxTime: Float32 @44, MaxTimeBlink: Float32 @48, BlinkFrames: UIAnimatedTextureFrame[] @52, Frames: UIAnimatedTextureFrame[] @72, Width: Int32 @92, Icon: UIHudIcon @96, Height: Int32 @100, StickOnBorder: Boolean @104, Rotation: Boolean @105 |
| UIAnimatedTextureManagerAsset | class | Asset | 56 | OutputAtlasFile: String @24, Groups: UIAnimatedTextureGroup[] @36 |
| UIAsset | class | Asset | 48 | AtlasCount: Int32 @24, Config: UIConfig @28, SourceFile: FileRef @32, AtlasTextureFormat: TextureOutputFormat @44 |
| UIConfig | class | DataContainer | 20 | Hud: UIHudConfig @12, Menu: UIMenuConfig @16 |
| UIFontAsset | class | Asset | 92 | GlowBlurHeight: Int32 @24, GlowBlurWidth: Int32 @28, GlowGaussianBase: Float32 @32, GenerateCharacterSet: UICharacterSet @36, TextDatabase: UITextDatabase[] @40, GlowBorderSize: Int32 @60, PointSize: Uint32 @64, SourceFile: FileRef @68, Style: UIFontStyle @80, Effect: UIFontEffect @84, BorderSize: Int32 @88 |
| UIFontData | class | DataContainer | 96 | TextureHeight: Uint32 @12, TextureWidth: Uint32 @16, Effect: UIFontEffect @20, BorderSize: Int32 @24, Height: Uint32 @28, Kerning: UIFontGlyphKerning[] @32, Glyphs: UIFontGlyph[] @52, KerningFile: String @72, TextureFile: String @84 |
| UIFontGlyph | class | DataContainer | 60 | MinY: Int32 @12, MaxX: Int32 @16, MinX: Int32 @20, Advance: Int32 @24, YOffset: Int32 @28, MaxY: Int32 @32, Y: Int32 @36, X: Int32 @40, Glyph: Uint32 @44, Rows: Int32 @48, Pitch: Int32 @52, Width: Int32 @56 |
| UIFontGlyphKerning | class | DataContainer | 24 | Kerning: Int32 @12, Second: Int32 @16, First: Int32 @20 |
| UIFontMapAsset | class | Asset | 44 | Proxies: UIFontMapProxy[] @24, Language: LanguageFormat @40 |
| UIFontProxyAsset | class | DataContainer | 52 | SDPixelOffset: Vec2 @12, Style: UIFontStyle @20, Tracking: Float32 @24, Font: UIFontAsset @28, FontSD: UIFontAsset @32, FontName: String @36, PointSize: Uint32 @48 |
| UIHudConfig | class | DataContainer | 168 | Minimap: UIMinimapConfig @12, TaggedVehicleCrosshair: UITaggedVehicleCrosshair @84, ReviveDistanceThreshold: Float32 @100, CriticalHealthThreshold: Float32 @104, MaxEnemyNametagDistance: Float32 @108, RepairVehicleThreshold: Float32 @112, ShowMedicHealthThreshold: Float32 @116, MedicboxDistanceThreshold: Float32 @120, InnerNametagRadius: Float32 @124, Nametags: UINametag[] @128, MaxOrderIconDistance: Float32 @148, GrenadeDistanceThreshold: Float32 @152, AmmoboxDistanceThreshold: Float32 @156, OuterNametagRadius: Float32 @160, NametagHealthVisibleForAll: Boolean @164 |
| UIMenuConfig | class | DataContainer | 16 | DefaultScene: Boolean @12 |
| UIMovieAsset | class | Asset | 60 | SubtitleMovie: UIMovieData @24, Music: WaveAsset @28, Texture: TextureAsset @32, Language: LanguageFormat @36, Description: String @40, Movie: UIMovieData @52, IsSpecialEdition: Boolean @56, Looping: Boolean @57, OverrideBackgroundMusic: Boolean @58 |
| UIMovieData | class | DataContainer | 44 | Viewport: UIMovieViewport @12, MovieFromMemory: BinkMemoryAsset @36, Movie: BinkTextureAsset @40 |
| UINametag | class | DataContainer | 112 | TextOffset: Vec2 @12, TextColor_CB: Vec3 @32, TextColor: Vec3 @48, HealthbarOffset: Vec2 @64, IconOffset: Vec2 @72, Icon: UIHudIcon @80, HealthMaxDistance: Float32 @84, IconMaxDistance: Float32 @88, IconFadeDistance: Float32 @92, TextFadeDistance: Float32 @96, HealthFadeDistance: Float32 @100, TextMaxDistance: Float32 @104 |
| UIShaderAsset | class | Asset | 24 | - |
| UITextDatabase | class | Asset | 52 | HistogramFile: FileRef @24, BinaryFile: FileRef @36, Language: LanguageFormat @48 |
| UITextureAtlasData | class | DataContainer | 32 | Infos: UITextureAtlasInfo[] @12 |
| UITextureAtlasInfo | class | DataContainer | 48 | MaxU: Float32 @12, MinV: Float32 @16, MaxV: Float32 @20, MinU: Float32 @24, TextureFile: String @28, TextureWidth: Int32 @40, TextureHeight: Int32 @44 |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: UI 45 records]
