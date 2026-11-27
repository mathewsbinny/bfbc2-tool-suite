# `GameDll`: the run-time entities and the factories that make them

**Sources.** Round SPG4-rest (2026-11-27): `spec/probes/typeinfo.py` over `BFBC2Game.exe`: the 262 `GameDll` records
(RTTI-REG-1: run-time classes, registered with their parent and no reflected member) and the entity factories of
SCR-EVT-3 followed to the constructors they call. The tables are generated (do not edit them by hand).

### RTTI-FAC-1: Which run-time class each data class makes, from the factories' constructors

Status: **verified** · Variants: PC

A factory of the server's registry (148 registrations) or the client's (100, SCR-EVT-3) is a small object whose
vtable's slot 1 returns the data class's class info (`mov eax, <info>; ret`), slot 3 the size of the entity it makes
(`mov eax, <size>; ret 4`) and whose construct slot (the server's slot 4, the client's slot 6: the slots its create
function 0x711740 / 0x8A07F0 calls after allocating) calls the entity's constructor. The constructor stores the
vtables of the class's ancestors and then its own; each class's vtable holds its `getType`, `mov eax, <class info>;
ret`. So the last vtable a constructor stores names the run-time class, and the size slot is checked against that
class's record size (RTTI-REG-1). 110 of the server's factories and 75 of the client's resolve this way (size slot
equal to the record's size, or no constant size slot): 173 run-time classes are made by a factory (158 `GameDll`, 10
`WorldRender` lights and meshes, `SequenceEntity` and `EffectEntity`, `ClientEmitterEntity`, `SoundEntity`,
`WeatherSystemEntity`). Most are named `Server` / `Client` + the data class's stem; the others:

| Data class | Server | Client |
|---|---|---|
| `GameSoundEntityData` / `GameEffectEntityData` / `GameInteractionEntityData` | `ServerSoundEntity` / `ServerEffectEntity` / `ServerInteractionEntity` | `ClientSoundEntity` / `ClientEffectEntity` / `ClientInteractionEntity` |
| `WeaponPickupEntityData`, `KitPickupEntityData`, `DynamicWeaponPickupEntityData` | `ServerPickupEntity` | `ClientPickupEntity` |
| `FenceModelEntityData` (and the client's `InteractableStaticModelEntityData`) | `ServerStaticModelEntity` | `ClientStaticModelEntity` |
| `DetailedTreeBangerEntityData` | `ServerTreeBangerEntity` | - |
| `ForbiddenAIAreaData` / `CSITextData` | `ServerForbiddenAreaEntity` / `ServerCSITextEntity` | - |
| `StaticPositionEntityData` / `SequenceData` | `StaticPositionEntity` / `SequenceEntity` | the same |
| `SoundAreaEntityData`, `SoundEntityData`, `EffectEntityData`, the light, mesh and weather data | - | `SoundAreaEntity`, `SoundEntity`, `EffectEntity`, `DirLightEntity` ... `WeatherSystemEntity` |

Not resolved: the server's 16 order factories (they make `ServerOrderEntity`, 124 bytes; one factory class whose slot 1
returns the data class's info from the factory object, `mov eax, [ecx+4]` at 0x7065D0, so the data class is not
read here; the order classes are AIO-*'s), the component factories (the
server's 22, the client's 22 `...ComponentData`: a different factory shape, whose objects the vehicle and soldier
entities make; their run-time classes are the `...Component` rows of RTTI-GDLL-1 and 2 by name) and the client's
three banger factories (`BangerEntityData`, `TreeBangerEntityData`, `DetailedTreeBangerEntityData`: their
constructors store no vtable this walk recognises; `ClientBangerEntity` and `ClientTreeBangerEntity` by name).

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/probes/typeinfo.py --measure, 2026-11-27: factories server 148, client 100; resolved pairs server 110, client 75; 173 classes made]
[exe: BFBC2Game.exe factory registries 0x157041C (register 0x71C790) and 0x1572768 (0x8B6A60); the server's create 0x711740 calls slot 4 (e.g. the factory vtable 0x1427568: slot 3 `mov eax, 0x30; ret 4`, slot 4 0x6FBDA0 allocates 0x30 and jumps to the constructor 0x586AC0), the client's create 0x8A07F0 calls slot 6 (`call [eax+0x18]` at 0x8A08ED)]

### RTTI-GDLL-1: The server's run-time classes

Status: **verified** · Variants: PC

The hierarchy: `Entity` -> `LogicEntity` -> `GameEntity` -> `ServerEntity` (115 descendants in `GameDll`; 44 bytes),
`LogicEntity` -> `Component` -> `ServerComponent` (24), and `WeaponFiringEffects` -> `ServerWeaponFiringEffects` (the
firing effects of each weapon kind, made by the weapons, not by a factory). The size is the object the factory
allocates (RTTI-FAC-1); the controllable entities (`ServerSoldierEntity` 976 bytes, `ServerVehicleEntity`) derive
from `ServerControllableEntity` -> `ServerPhysicsEntity` -> `ServerEntity`. A row without a factory is a base class, a component, a
firing effect, an event (`ServerPlayerEvent`, `ServerSoldierEvent`, `ServerParameterEvent`: below `EntityEvent`, not `Entity`) or an entity another
entity makes (`ServerSpawnEntity`'s soldiers, `ServerOrderEntity`).

<!-- typeinfo:gamedll-server -->
| Class | Parent | Size | Made from (factory) |
|---|---|---|---|
| Component | LogicEntity | 44 | - |
| GameEntity | LogicEntity | 44 | - |
| ProgressEvent | BasicLogicEntityEvent | 36 | - |
| ServerAdvertisementComponent | ServerPartComponent | 100 | - |
| ServerAmbientModelEntity | ServerEntity | 44 | server: AmbientModelEntityData |
| ServerAmmoBoxEntity | ServerExplosionPackEntity | 560 | server: AmmoBoxEntityData |
| ServerAmmoBoxFiringEffects | ServerWeaponFiringEffects | 304 | - |
| ServerAmmoCrateEntity | ServerEntity | 172 | server: AmmoCrateEntityData |
| ServerAnimationEntity | ServerEntity | 48 | server: AnimationEntityData |
| ServerAreaTriggerEntity | ServerTriggerEntity | 96 | server: AreaTriggerEntityData |
| ServerArmDisarmAwardEntity | ServerEntity | 44 | server: ArmDisarmAwardEntityData |
| ServerArtilleryStrikeEntity | ServerEntity | 64 | server: ArtilleryStrikeEntityData |
| ServerArtilleryStrikeFiringEffects | ServerWeaponFiringEffects | 432 | - |
| ServerAutoTeamEntity | ServerEntity | 52 | server: AutoTeamEntityData |
| ServerBangerEntity | ServerBangerEntityBase | 336 | server: BangerEntityData |
| ServerBangerEntityBase | ServerPhysicsEntity | 304 | - |
| ServerBinocularsFiringEffects | ServerWeaponFiringEffects | 304 | - |
| ServerBulletEntity | ServerProjectileEntity | 176 | server: BulletEntityData |
| ServerCSITextEntity | ServerEntity | 48 | server: CSITextData |
| ServerCallbackFiringEffects | ServerWeaponFiringEffects | 304 | - |
| ServerCameraComponent | ServerComponent | 144 | - |
| ServerCapturePointEntity | ServerEntity | 180 | server: CapturePointEntityData |
| ServerCharacterStateTriggerEntity | ServerTriggerEntity | 72 | server: CharacterStateTriggerEntityData |
| ServerChildComponent | ServerPartComponent | 192 | - |
| ServerClearAreaTriggerEntity | ServerEntity | 64 | server: ClearAreaTriggerEntityData |
| ServerCombatActionTriggerEntity | ServerTriggerEntity | 60 | server: CombatActionTriggerEntityData |
| ServerCombatAreaTriggerEntity | ServerEntity | 148 | server: CombatAreaTriggerEntityData |
| ServerComponent | Component | 44 | - |
| ServerConsoleCommandEntity | ServerEntity | 44 | server: ConsoleCommandEntityData |
| ServerConsumableTriggerEntity | ServerEntity | 48 | server: ConsumableTriggerEntityData |
| ServerContinueEntity | ServerEntity | 240 | server: ContinueEntityData |
| ServerControllableEntity | ServerPhysicsEntity | 180 | - |
| ServerCutsceneCameraEntity | ServerCutsceneObjectEntityBase | 188 | server: CutsceneCameraEntityData |
| ServerCutsceneEntity | ServerEntity | 204 | server: CutsceneEntityData |
| ServerCutsceneObjectEntity | ServerCutsceneObjectEntityBase | 172 | server: CutsceneObjectEntityData |
| ServerCutsceneObjectEntityBase | ServerEntity | 172 | - |
| ServerCutsceneSoldierEntity | ServerCutsceneObjectEntityBase | 180 | server: CutsceneSoldierEntityData |
| ServerCutsceneVehicleEntity | ServerCutsceneObjectEntityBase | 272 | server: CutsceneVehicleEntityData |
| ServerDamageAreaTriggerEntity | ServerEntity | 80 | server: DamageAreaTriggerEntityData |
| ServerDeathAreaTriggerEntity | ServerEntity | 60 | server: DeathAreaTriggerEntityData |
| ServerDebugTextEntity | ServerEntity | 128 | server: DebugTextEntityData |
| ServerDefibrillatorFiringEffects | ServerWeaponFiringEffects | 304 | - |
| ServerDelayTriggerEntity | ServerTriggerEntity | 56 | server: DelayTriggerEntityData |
| ServerDetonatedFiringEffects | ServerWeaponFiringEffects | 320 | - |
| ServerDisableTeamEntity | ServerEntity | 44 | server: DisableTeamEntityData |
| ServerEffectEntity | ServerEntity | 52 | server: GameEffectEntityData |
| ServerEndOfRoundSoldierSpawnEntity | ServerEntity | 44 | server: EndOfRoundSoldierSpawnEntityData |
| ServerEngineComponent | ServerComponent | 76 | - |
| ServerEntity | GameEntity | 44 | - |
| ServerEntryComponent | ServerComponent | 288 | - |
| ServerEventGateEntity | ServerEntity | 52 | server: EventGateEntityData |
| ServerEventIfSwitchEntity | ServerEntity | 48 | server: EventIfSwitchEntityData |
| ServerEventMemoryEntity | ServerEntity | 48 | server: EventMemoryEntityData |
| ServerEventSequenceTriggerEntity | ServerTriggerEntity | 76 | server: EventSequenceTriggerEntityData |
| ServerEventSplitterEntity | ServerLogicUtilEntity | 56 | server: EventSplitterEntityData |
| ServerExplosionEntity | ServerEntity | 17744 | server: ExplosionEntityData |
| ServerExplosionPackEntity | ServerGhostProjectileEntity | 544 | server: ExplosionPackEntityData |
| ServerForbiddenAreaEntity | ServerEntity | 48 | server: ForbiddenAIAreaData |
| ServerForceAreaEntity | ServerEntity | 128 | server: ForceAreaEntityData |
| ServerForestEntity | ServerGroupEntity | 180 | server: ForestEntityData |
| ServerGameSettingEntity | ServerEntity | 44 | server: GameSettingEntityData |
| ServerGameTestEntity | ServerEntity | 60 | server: GameTestEntityData |
| ServerGhostProjectileEntity | ServerProjectileEntity | 384 | - |
| ServerGrenadeEntity | ServerGhostProjectileEntity | 576 | server: GrenadeEntityData |
| ServerGroupEntity | ServerPhysicsEntity | 72 | - |
| ServerHudCounterEntity | ServerEntity | 136 | server: HudCounterEntityData |
| ServerHudEntity | ServerEntity | 44 | server: HudEntityData |
| ServerHumanPlayerEntity | ServerEntity | 72 | server: HumanPlayerEntityData |
| ServerInputRestrictionEntity | ServerEntity | 68 | server: InputRestrictionEntityData |
| ServerInteractableStaticModelEntity | ServerStaticModelEntity | 112 | server: InteractableStaticModelEntityData |
| ServerInteractionEntity | ServerEntity | 148 | server: GameInteractionEntityData |
| ServerKillCounterEntity | ServerEntity | 140 | server: KillCounterEntityData |
| ServerLadderEntity | ServerStaticModelEntity | 116 | server: LadderEntityData |
| ServerLaserDesignatorFiringEffects | ServerLockingWeaponFiringEffects | 400 | - |
| ServerLifeCounterEntity | ServerEntity | 176 | server: LifeCounterEntityData |
| ServerLockingWeaponFiringEffects | ServerWeaponFiringEffects | 336 | - |
| ServerLogicUtilEntity | ServerEntity | 56 | - |
| ServerLookAtTriggerEntity | ServerTriggerEntity | 56 | server: LookAtTriggerEntityData |
| ServerMapMarkerEntity | ServerEntity | 164 | server: MapMarkerEntityData |
| ServerMedicBagEntity | ServerExplosionPackEntity | 560 | server: MedicBagEntityData |
| ServerMedicBagFiringEffects | ServerWeaponFiringEffects | 304 | - |
| ServerMedkitFiringEffects | ServerWeaponFiringEffects | 304 | - |
| ServerMeleeFiringEffects | ServerWeaponFiringEffects | 320 | - |
| ServerMenuEntity | ServerEntity | 64 | server: MenuEntityData |
| ServerMessageEntity | ServerEntity | 52 | server: MessageEntityData |
| ServerMiddlegrowthModelGroupEntity | ServerGroupEntity | 80 | server: MiddlegrowthModelGroupEntityData |
| ServerMissileEntity | ServerGhostProjectileEntity | 576 | server: MissileEntityData |
| ServerMissionEntity | ServerEntity | 48 | server: MissionEntityData |
| ServerMissionObjectiveEntity | ServerLogicUtilEntity | 148 | server: MissionObjectiveEntityData |
| ServerMortarStrikeFiringEffects | ServerWeaponFiringEffects | 368 | - |
| ServerMotionSensorEntity | ServerGrenadeEntity | 592 | server: MotionSensorEntityData |
| ServerMultipleTriggerEntity | ServerTriggerEntity | 64 | server: MultipleTriggerEntityData |
| ServerOrderEntity | ServerEntity | 124 | - |
| ServerParameterEvent | BasicLogicEntityEvent | 24 | - |
| ServerPartComponent | ServerComponent | 100 | - |
| ServerPhysicsEntity | ServerEntity | 60 | - |
| ServerPickupEntity | ServerEntity | 384 | server: DynamicWeaponPickupEntityData, KitPickupEntityData, PickupEntityData, WeaponPickupEntityData |
| ServerPlayVideoEntity | ServerEntity | 52 | server: PlayVideoEntityData |
| ServerPlayerEntryComponent | ServerEntryComponent | 304 | - |
| ServerPlayerEvent | BasicLogicEntityEvent | 48 | - |
| ServerPowerToolFiringEffects | ServerWeaponFiringEffects | 320 | - |
| ServerPreRoundEntity | ServerEntity | 152 | server: PreRoundEntityData |
| ServerPredestructionEntity | ServerEntity | 48 | server: PredestructionEntityData |
| ServerProjectileEntity | ServerPhysicsEntity | 128 | - |
| ServerProjectileImpulseTestEntity | ServerEntity | 68 | server: ProjectileImpulseTestEntityData |
| ServerRadarSweepComponent | ServerComponent | 44 | - |
| ServerRadioComponent | ServerComponent | 64 | - |
| ServerRandomTriggerEntity | ServerTriggerEntity | 56 | server: RandomTriggerEntityData |
| ServerRemoteEntryComponent | ServerEntryComponent | 304 | - |
| ServerRepairToolFiringEffects | ServerPowerToolFiringEffects | 336 | - |
| ServerRigidModelEntity | ServerPhysicsEntity | 68 | server: RigidModelEntityData |
| ServerRigidModelGroupEntity | ServerGroupEntity | 76 | server: RigidModelGroupEntityData |
| ServerRotorComponent | ServerComponent | 68 | - |
| ServerSaveEntity | ServerLogicUtilEntity | 88 | server: SaveEntityData |
| ServerScreenEffectEntity | ServerEntity | 52 | server: ScreenEffectEntityData |
| ServerScreenFadeEntity | ServerEntity | 44 | server: ScreenFadeEntityData |
| ServerServerConnectTriggerEntity | ServerEntity | 44 | server: ServerConnectTriggerEntityData |
| ServerSkinnedMeshComponent | ServerComponent | 48 | - |
| ServerSoldierEntity | ServerControllableEntity | 976 | server: SoldierEntityData |
| ServerSoldierEntryComponent | ServerEntryComponent | 320 | - |
| ServerSoldierEvent | BasicLogicEntityEvent | 24 | - |
| ServerSoldierGripComponent | ServerComponent | 44 | - |
| ServerSoldierSpawnEntity | ServerSpawnEntity | 232 | server: SoldierSpawnEntityData |
| ServerSoundEntity | ServerEntity | 48 | server: GameSoundEntityData |
| ServerSpawnEntity | ServerEntity | 48 | - |
| ServerSpectateEntity | ServerEntity | 148 | server: SpectateEntityData |
| ServerSpeedEventGateEntity | ServerEntity | 52 | server: SpeedEventGateEntityData |
| ServerSquadEntity | ServerEntity | 48 | server: SquadEntityData |
| ServerStatEventTriggerEntity | ServerEntity | 44 | server: StatEventTriggerEntityData |
| ServerStaticModelEntity | ServerPhysicsEntity | 112 | server: FenceModelEntityData, StaticModelEntityData |
| ServerStaticModelGroupEntity | ServerGroupEntity | 96 | server: StaticModelGroupEntityData |
| ServerStaticModelTriggerEntity | ServerTriggerEntity | 68 | server: StaticModelTriggerEntityData |
| ServerTacticalObjectiveEntity | ServerEntity | 72 | server: TacticalObjectiveEntityData |
| ServerTargetEntity | ServerPhysicsEntity | 60 | server: TargetEntityData |
| ServerTeamEntity | ServerEntity | 44 | server: TeamEntityData |
| ServerTeamFilterEntity | ServerEntity | 44 | server: TeamFilterEntityData |
| ServerTerrainEntity | ServerPhysicsEntity | 96 | server: TerrainEntityData |
| ServerTicketCounterEntity | ServerEntity | 156 | server: TicketCounterEntityData |
| ServerTooltipEntity | ServerEntity | 44 | server: TooltipEntityData |
| ServerTracerDartEntity | ServerGhostProjectileEntity | 400 | server: TracerDartEntityData |
| ServerTrackComponent | ServerSkinnedMeshComponent | 56 | - |
| ServerTrackWheelComponent | ServerWheelComponent | 224 | - |
| ServerTreeBangerEntity | ServerBangerEntityBase | 368 | server: DetailedTreeBangerEntityData, TreeBangerEntityData |
| ServerTriggerEntity | ServerLogicUtilEntity | 56 | - |
| ServerUnderFireTriggerEntity | ServerTriggerEntity | 72 | server: UnderFireTriggerEntityData |
| ServerVehicleComponent | ServerPartComponent | 368 | - |
| ServerVehicleEntity | ServerControllableEntity | 464 | server: VehicleEntityData |
| ServerVehicleExitPointComponent | ServerComponent | 44 | - |
| ServerVehicleProjectileEntity | ServerProjectileEntity | 144 | server: VehicleProjectileEntityData |
| ServerVehicleSpawnEntity | ServerSpawnEntity | 928 | server: VehicleSpawnEntityData |
| ServerVehicleStateTriggerEntity | ServerTriggerEntity | 60 | server: VehicleStateTriggerEntityData |
| ServerVoiceOverEntity | ServerEntity | 64 | server: VoiceOverEntityData |
| ServerWeaponCallbackEntity | ServerEntity | 44 | server: WeaponCallbackEntityData |
| ServerWeaponComponent | ServerComponent | 224 | - |
| ServerWeaponFiringEffects | WeaponFiringEffects | 304 | - |
| ServerWeaponInputRouterComponent | ServerComponent | 80 | - |
| ServerWeatherStateEntity | ServerEntity | 140 | server: WeatherStateEntityData |
| ServerWheelComponent | ServerPartComponent | 208 | - |
| ServerWingComponent | ServerPartComponent | 116 | - |
| ServerWorldInteractionComponent | ServerComponent | 48 | - |
| SoundAreaEntity | ClientEntity | 56 | client: SoundAreaEntityData |
| StaticPositionEntity | GameEntity | 44 | server: StaticPositionEntityData, client: StaticPositionEntityData |
| WeaponFiringEffects | - | 208 | - |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: GameDll records not named Client* 163]

### RTTI-GDLL-2: The client's run-time classes

Status: **verified** · Variants: PC

`GameEntity` -> `ClientEntity` (65 descendants), `Component` -> `ClientComponent` (24) and `WeaponFiringEffects` ->
`ClientWeaponFiringEffects`; `ClientSoldierEntity` is 1,600 bytes against the server's 976. The client makes fewer kinds than the server: no logic, trigger or order entities (those run on the server and reach the
client through the network), but the visual ones (meshes, lights, emitters, sound) and the client halves of the
gameplay entities.

<!-- typeinfo:gamedll-client -->
| Class | Parent | Size | Made from (factory) |
|---|---|---|---|
| ClientAdvertisementComponent | ClientPartComponent | 192 | - |
| ClientAmbientModelEntity | ClientEntity | 112 | client: AmbientModelEntityData |
| ClientAmmoBoxEntity | ClientExplosionPackEntity | 736 | client: AmmoBoxEntityData |
| ClientAmmoCrateEntity | ClientEntity | 100 | client: AmmoCrateEntityData |
| ClientAnimationEntity | ClientEntity | 64 | client: AnimationEntityData |
| ClientArtilleryStrikeFiringEffects | ClientWeaponFiringEffects | 1904 | - |
| ClientBangerEntity | ClientBangerEntityBase | 384 | - |
| ClientBangerEntityBase | ClientPhysicsEntity | 352 | - |
| ClientBinocularsFiringEffects | ClientWeaponFiringEffects | 1696 | - |
| ClientBulletEntity | ClientProjectileEntity | 528 | client: BulletEntityData |
| ClientCameraComponent | ClientComponent | 192 | - |
| ClientCapturePointEntity | ClientEntity | 284 | client: CapturePointEntityData |
| ClientCharacterStateTriggerEntity | ClientEntity | 52 | client: CharacterStateTriggerEntityData |
| ClientChildComponent | ClientPartComponent | 288 | - |
| ClientCombatAreaTriggerEntity | ClientEntity | 168 | client: CombatAreaTriggerEntityData |
| ClientComponent | Component | 44 | - |
| ClientControllableEntity | ClientPhysicsEntity | 288 | - |
| ClientCutsceneCameraEntity | ClientCutsceneObjectEntityBase | 240 | client: CutsceneCameraEntityData |
| ClientCutsceneEntity | ClientEntity | 268 | client: CutsceneEntityData |
| ClientCutsceneObjectEntity | ClientCutsceneObjectEntityBase | 360 | client: CutsceneObjectEntityData |
| ClientCutsceneObjectEntityBase | ClientEntity | 208 | - |
| ClientCutsceneSoldierEntity | ClientCutsceneObjectEntityBase | 220 | client: CutsceneSoldierEntityData |
| ClientCutsceneVehicleEntity | ClientCutsceneObjectEntityBase | 288 | client: CutsceneVehicleEntityData |
| ClientDebugTextEntity | ClientEntity | 100 | client: DebugTextEntityData |
| ClientDefibrillatorFiringEffects | ClientWeaponFiringEffects | 1680 | - |
| ClientDetonatedFiringEffects | ClientWeaponFiringEffects | 1696 | - |
| ClientDynamicModelEntity | ClientPhysicsEntity | 104 | client: DynamicModelEntityData |
| ClientEffectComponent | ClientComponent | 68 | - |
| ClientEffectEntity | ClientEntity | 68 | client: GameEffectEntityData |
| ClientEngineComponent | ClientComponent | 96 | - |
| ClientEntity | GameEntity | 44 | - |
| ClientEntryComponent | ClientComponent | 480 | - |
| ClientExplosionEntity | ClientEntity | 1232 | client: ExplosionEntityData |
| ClientExplosionPackEntity | ClientGhostProjectileEntity | 736 | client: ExplosionPackEntityData |
| ClientForceAreaEntity | ClientEntity | 104 | client: ForceAreaEntityData |
| ClientForestEntity | ClientGroupEntity | 156 | client: ForestEntityData |
| ClientGhostProjectileEntity | ClientProjectileEntity | 448 | - |
| ClientGrenadeEntity | ClientGhostProjectileEntity | 688 | client: GrenadeEntityData |
| ClientGroupEntity | ClientPhysicsEntity | 72 | - |
| ClientHudCounterEntity | ClientEntity | 176 | client: HudCounterEntityData |
| ClientHumanPlayerEntity | ClientEntity | 44 | client: HumanPlayerEntityData |
| ClientInteractionEntity | ClientEntity | 176 | client: GameInteractionEntityData |
| ClientKillCounterEntity | ClientEntity | 172 | client: KillCounterEntityData |
| ClientLadderEntity | ClientStaticModelEntity | 320 | client: LadderEntityData |
| ClientLaserDesignatorFiringEffects | ClientLockingWeaponFiringEffects | 1760 | - |
| ClientLifeCounterEntity | ClientEntity | 188 | client: LifeCounterEntityData |
| ClientLockingWeaponFiringEffects | ClientWeaponFiringEffects | 1696 | - |
| ClientMapMarkerEntity | ClientEntity | 188 | client: MapMarkerEntityData |
| ClientMedicBagEntity | ClientExplosionPackEntity | 736 | client: MedicBagEntityData |
| ClientMedkitFiringEffects | ClientWeaponFiringEffects | 1680 | - |
| ClientMeleeFiringEffects | ClientWeaponFiringEffects | 1696 | - |
| ClientMiddlegrowthModelGroupEntity | ClientGroupEntity | 80 | client: MiddlegrowthModelGroupEntityData |
| ClientMissileEntity | ClientGhostProjectileEntity | 960 | client: MissileEntityData |
| ClientMissionObjectiveEntity | ClientEntity | 164 | client: MissionObjectiveEntityData |
| ClientMortarStrikeFiringEffects | ClientWeaponFiringEffects | 1936 | - |
| ClientMotionSensorEntity | ClientGrenadeEntity | 704 | client: MotionSensorEntityData |
| ClientPartComponent | ClientComponent | 192 | - |
| ClientPhysicsEntity | ClientEntity | 60 | - |
| ClientPickupEntity | ClientEntity | 240 | client: DynamicWeaponPickupEntityData, KitPickupEntityData, PickupEntityData, WeaponPickupEntityData |
| ClientPlayerEntryComponent | ClientEntryComponent | 560 | - |
| ClientPowerToolFiringEffects | ClientWeaponFiringEffects | 1696 | - |
| ClientPreRoundEntity | ClientEntity | 100 | client: PreRoundEntityData |
| ClientProjectileEntity | ClientPhysicsEntity | 96 | - |
| ClientProxyProjectileEntity | ClientProjectileEntity | 272 | - |
| ClientRadarSweepComponent | ClientComponent | 76 | - |
| ClientRadioComponent | ClientComponent | 64 | - |
| ClientRemoteEntryComponent | ClientEntryComponent | 496 | - |
| ClientRigidModelEntity | ClientPhysicsEntity | 72 | client: RigidModelEntityData |
| ClientRigidModelGroupEntity | ClientGroupEntity | 76 | client: RigidModelGroupEntityData |
| ClientRotorComponent | ClientComponent | 112 | - |
| ClientScreenEffectEntity | ClientEntity | 76 | client: ScreenEffectEntityData |
| ClientScreenFadeEntity | ClientEntity | 60 | client: ScreenFadeEntityData |
| ClientSkinnedMeshComponent | ClientComponent | 84 | - |
| ClientSoldierEntity | ClientControllableEntity | 1600 | client: SoldierEntityData |
| ClientSoldierEntryComponent | ClientEntryComponent | 496 | - |
| ClientSoldierGripComponent | ClientComponent | 128 | - |
| ClientSoldierSpawnEntity | ClientEntity | 108 | client: SoldierSpawnEntityData |
| ClientSoundEntity | ClientEntity | 52 | client: GameSoundEntityData |
| ClientSpectateEntity | ClientEntity | 192 | client: SpectateEntityData |
| ClientStaticModelEntity | ClientPhysicsEntity | 304 | client: FenceModelEntityData, InteractableStaticModelEntityData, StaticModelEntityData |
| ClientStaticModelGroupEntity | ClientGroupEntity | 96 | client: StaticModelGroupEntityData |
| ClientTeamEntity | ClientEntity | 44 | client: TeamEntityData |
| ClientTerrainEntity | ClientPhysicsEntity | 108 | client: TerrainEntityData |
| ClientTicketCounterEntity | ClientEntity | 184 | client: TicketCounterEntityData |
| ClientTracerDartEntity | ClientGhostProjectileEntity | 640 | client: TracerDartEntityData |
| ClientTrackComponent | ClientSkinnedMeshComponent | 176 | - |
| ClientTrackWheelComponent | ClientWheelComponent | 272 | - |
| ClientTreeBangerEntity | ClientBangerEntityBase | 416 | - |
| ClientVehicleComponent | ClientComponent | 528 | - |
| ClientVehicleEntity | ClientControllableEntity | 496 | client: VehicleEntityData |
| ClientVehicleExitPointComponent | ClientComponent | 44 | - |
| ClientVehicleSpawnEntity | ClientEntity | 120 | client: VehicleSpawnEntityData |
| ClientVoiceOverEntity | ClientEntity | 68 | client: VoiceOverEntityData |
| ClientWeaponComponent | ClientComponent | 256 | - |
| ClientWeaponFiringEffects | WeaponFiringEffects | 1680 | - |
| ClientWeaponInputRouterComponent | ClientComponent | 96 | - |
| ClientWeatherStateEntity | ClientEntity | 96 | client: WeatherStateEntityData |
| ClientWheelComponent | ClientPartComponent | 256 | - |
| ClientWingComponent | ClientPartComponent | 240 | - |
<!-- /typeinfo -->

[test: core/tests/test_spec_probes.py::typeinfo]
[data: spec/typeinfo.json, 2026-11-27: GameDll records named Client* 99]
