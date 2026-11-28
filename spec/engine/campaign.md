# The single-player campaign: missions, saves, cutscenes, triggers

**Sources.** Round SPG4 (2026-11-23): the census of DICE's partitions (`spec/census.json`, re-measured by the game
tier) and `BFBC2Game.exe`'s names. The campaign is thirteen levels (`SP_01` - `SP_04`, `SP_003_B`, `SP_004_B`,
`SP_005`, `SP_005_B`, `SP_006`, `SP_007`, `SP_008`, `SP_008_B`, `SP_009`, with `sp_common`) driven by their own event graphs (`engine/scripting.md`, LEVEL-EVT); this file lists the classes those graphs
use for the campaign's flow and where DICE uses them. The tables list the members DICE's instances carry, less `Name`,
`MemoryReportGroup`, `Components`, `Enabled`, `Enumeration` and `Transform`. Most of these classes are also usable in
a multiplayer level (DICE uses a few there: the triggers, `ClearAreaTriggerEntityData` in BC1's maps,
`CutsceneCameraData` in MP_012's).

## 1. Flow: objectives, saves, fades, restrictions

### CAMP-FLOW-1: Objectives and saves

Status: **verified** · Variants: PC

A mission's goals are `MissionObjectiveEntityData` (parent and child objectives, brief / objective / success
strings, the HUD camera, map markers, a team or squad) placed in the level; the game saves at
`SaveEntityData` points (183, 17 - 21 per campaign level: a save file name, the level name, a save-name string, the
save screen's texture and a delay) - these are the campaign's checkpoints (the executable's `Checkpoint`,
`DisableCheckpoints`, `canRestartFromCheckpoint`, `PersistentValueWritePolicy_Checkpoint`). SP_01's one
`ContinueEntityData` is the resume point after loading (fade from / to black, immortality timers, ammo, look at the
nearest map marker). `ConsumableTriggerEntityData` (24: a `Persistence` reference) and `StatEventTriggerEntityData`
(12: a stat event with two parameters) feed the career (PERS-DATA), the collectibles and the difficulty statistics
(`StatCategoriesTree`, 13: `Awards/DifficultyCategories`, `Awards/SPWeaponCategories`). The difficulty levels are
`AI/Difficulty`'s `DifficultyModesData` (BOT-DIF).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SaveEntityData (GameSharedResources) | 183 | 13 | `levels/` | LevelName, SaveDelayTime, SaveFileName, SaveNameSID, SaveScreenTexture |
| ContinueEntityData (GameSharedResources) | 1 | 1 | `levels/` | FadeFromBlack, FadeToBlack, LookAtNearestMapMarker, SoldierImmortalTimer, StayBlack, VehicleImmortalTimer, WeaponAmmo |
| ConsumableTriggerEntityData (GameSharedResources) | 24 | 9 | `levels/` | Persistence |
| StatEventTriggerEntityData (GameSharedResources) | 12 | 9 | `levels/` | ParamX, ParamY, StatEvent |
| StatCategoriesTree (GameSharedResources) | 13 | 13 | `awards/difficultycategories.dbx`, `awards/spweaponcategories.dbx`, `awards/weaponcategories.dbx` | Categories, General, ParamX, ParamY |
| DifficultyModesData (GameSharedResources) | 1 | 1 | `ai/difficulty.dbx` | DifficultyModes |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes above; MissionObjectiveEntityData's members]
[exe: BFBC2Game.exe strings Checkpoint, DisableCheckpoints, canRestartFromCheckpoint, PersistentValueWritePolicy_Checkpoint]

### CAMP-FLOW-2: Fades, input restrictions, videos and scripted fire

Status: **verified** · Variants: PC

`ScreenFadeEntityData` (50) fades the screen (and optionally the UI and the game's sound), waiting on black while
streaming; `InputRestrictionEntityData` (128) blocks inputs one by one (fire, sprint, jump, pose change, vehicle
change, gadgets, menus ...) for a scripted passage; `PlayVideoEntityData` (4) plays a Bink movie (BINK-OUT-1) over the
game; `ArtilleryStrikeEntityData` (36) is scripted artillery (strike counts, delays, projectiles, radius around a
soldier); `ForceAreaEntityData` (1, SP_003_B) pushes soldiers, vehicles, ragdolls, bangers and particles;
`PredestructionEntityData` (895, SP_008 / SP_008_B mostly) destroys what is in its radius before the player arrives.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| ScreenFadeEntityData (GameSharedResources) | 50 | 13 | `levels/` | ControlledByClient, FadeInGameSound, FadeTime, FadeUI, WaitOnBlackForStreaming |
| InputRestrictionEntityData (GameSharedResources) | 128 | 73 | `levels/` | AltFire, Brake, ChangeEntry, ChangePose, ChangeVehicle, CycleRadioChannel, DynamicGadget1, DynamicGadget2, Fire, FireCountermeasure, GrenadeLauncher, HandBrake, IngameMenu, Jump, MeleeAttack, Order, OverridePreviousInputRestriction, Pitch, Reload, Roll, ScoreboardMenu, SnapView, Sprint, StaticGadget, Strafe, SwitchPrimaryWeapon, Throttle, ThrowGrenade, ToggleCamera, ToggleParachute, Yaw, Zoom |
| PlayVideoEntityData (GameSharedResources) | 4 | 3 | `levels/` | BinkMovie, KeepBlackScreen |
| ArtilleryStrikeEntityData (GameSharedResources) | 36 | 3 | `levels/` | DurationOfArtilleryStrike, FollowSoldier, Geometry, HeightOffsetWhenFollowingSoldier, MaxDelayBetweenProjectiles, MaxDelayBetweenStrikes, MaxProjectilesPerStrike, MinDelayBetweenProjectiles, MinDelayBetweenStrikes, MinProjectilesPerStrike, MinRadiusToSoldier, MissileData, MissileSpawnRadius, NumberOfStrikes, RadiusWhenFollowingSoldier, Soldier, Squad, StrikeType |
| ForceAreaEntityData (GameSharedResources) | 1 | 1 | `levels/` | Areas, BangerForceMultiplier, Force, ForceType, ParticleForceMultiplier, RagdollForceMultiplier, SoldierForceMultiplier, VehicleForceMultiplier |
| PredestructionEntityData (GameSharedResources) | 895 | 53 | `levels/` | Radius |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 2. Triggers

### CAMP-TRIG-1: The trigger entities

Status: **verified** · Variants: PC

Besides the generic event graph (SCR-EVT), the campaign fires on these triggers (each with `Delay` and `RunOnce`
where it has them): a static model's parts destroyed (`StaticModelTriggerEntityData`, 1,617: a part name or count,
points to the destroyer), the player's state (`CharacterStateTriggerEntityData`, 421: pose, health, weapon, vehicle
position; it can also set health, ammo and position), coming under fire (`UnderFireTriggerEntityData`, 49), looking
at an object (`LookAtTriggerEntityData`, 33: distance, FOV, zoom level, binoculars), chance (`RandomTriggerEntityData`,
14). `ClearAreaTriggerEntityData` (1,270; 90 each in BC1's Oasis layers) clears soldiers, vehicles, static models,
bangers and effects from areas; `DeathAreaTriggerEntityData` (7, MP_012's layers) kills after a time outside.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| StaticModelTriggerEntityData (GameSharedResources) | 1,617 | 61 | `levels/` | Delay, GivePointsToDamageGiver, PartCount, PartCountInPerCent, PartName, RunOnce, StaticModel, UseCurrentPartDestroyedOnEnable |
| CharacterStateTriggerEntityData (GameSharedResources) | 421 | 17 | `levels/` | AmmoInClipToSet, AmmoInPoolToSet, CharacterPose, CheckAllWeapons, CheckHealthOnAndAbove, CheckVehiclePosition, ContinueToCheckIfHoldingWrongWeapon, Damage, DamageInterval, Delay, EntryInputAction, ExcludeConnectedVehicles, ForcePlayerEnterEntry, HealthToCheck, HealthToSet, NewPosition, NewVehiclePositionOnCharacterMove, Pitch, PlayerToCheck, PoseToCheck, RunOnce, SuppressInputOnInputTriggerd, UsePitch, VehiclePosition, Vehicles, Weapon |
| UnderFireTriggerEntityData (GameSharedResources) | 49 | 9 | `levels/` | Area, CoolDownTime, Delay, DisableIfTrackEntityIsLost, Radius, RunOnce, TrackEntity, TriggerOnMaterialOnly, TriggeredBy |
| LookAtTriggerEntityData (GameSharedResources) | 33 | 10 | `levels/` | BinocularsMode, Delay, FOV, MaxDistanceToObject, MinDistanceToObject, ObjectToFollow, RunOnce, Weapon, ZoomLevel |
| RandomTriggerEntityData (GameSharedResources) | 14 | 9 | `levels/` | Delay, RandomEventWeight, RunOnce |
| ClearAreaTriggerEntityData (GameSharedResources) | 1,270 | 49 | `levels/` | Action, Area, Areas, Delay, ExcludeImmortalSoldiersInTeam, RemoveBangers, RemoveEffects, RemoveSoldiers, RemoveStaticModels, RemoveVehicles, RunOnce, SoldiersToExclude, TeamOfImmortalSoldiers |
| DeathAreaTriggerEntityData (GameSharedResources) | 7 | 7 | `levels/` | Damage, Geometry, TimeToReturn |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 3. Cutscenes

### CAMP-CUT-1: A cutscene is an entity with soldier, vehicle, object and camera tracks

Status: **verified** · Variants: PC

A `CutsceneEntityData` (553) plays a `Sequence` of tracks with its sound, subtitles (TEXT-SUB-1), screen fades
(`CutsceneEntityScreenFadeData`: pre / post fades and black delays) and camera (`CutsceneEntityCameraData`: an
animated camera bone, depth-of-field and FOV bones, peek-around limits), and can be skippable, mute the game or open a
menu after it. Its actors are `CutsceneSoldierEntityData` (979: character, facial and eyes / yaw animations, blend
from the player's soldier, AI off, interlude orders), `CutsceneVehicleEntityData` (556), `CutsceneObjectEntityData`
(512: animated meshes) and `CutsceneCameraEntityData` (124); a property track (`GenericPropertyTrackData`, 577: a
target's property keyed over time) animates any other field. The heaps are the executable's `CutsceneLargeHeapSize`,
`CutsceneSmallHeapSize` (ENG-MEM-1). MP_012's intro cameras are `CutsceneCameraData` (4).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| CutsceneEntityData (GameSharedResources) | 553 | 49 | `levels/` | AfterCutsceneMenuId, AllowAllAnimationsHeaps, Camera, CameraEntitySpace, CutsceneMenuId, ForceMuteSoundSetting, FreezeOnFinish, IsSkippable, KeepGameSoundMuted, LocalizedSound, MuteGameSound, MuteVoiceOver, NormalizedTimeEnabled, Radius, ScreenFade, Sequence, Sound, SoundLength, SoundTemplate, Subtitle |
| CutsceneEntityScreenFadeData (GameSharedResources) | 127 | 48 | `levels/` | PostCutsceneFadeInTime, PostCutsceneFadeOutTime, PostCutsceneMinBlackDelay, PreCutsceneFadeInTime, PreCutsceneFadeOutTime, PreCutsceneMinBlackDelay, StartPostFadeOutBeforeEndTime |
| CutsceneEntityCameraData (GameSharedResources) | 124 | 48 | `levels/` | AnimTree, Animation, CameraBone, CameraLeaps, DofBone, FadeDuringJump, FadeTime, FadeWaitTime, FovBone, HudFx, OcclusionRayOffset, PeekAroundMaxPitch, PeekAroundMaxYaw, PeekAroundSnapBack, PreFadeTime, ShadowViewDistance, Skeleton, SoundOcclusion, StayFadedWhileStreaming, TotalLength, UseDof |
| CutsceneSoldierEntityData (GameSharedResources) | 979 | 13 | `levels/` | AnimTree, AnimateWeapon, AnimationCullingEnabled, BlendInFromLocalSoldierTime, CancelInterludeOrder, CharacterAnimation, CharacterAnimationLength, CollisionEnabled, CutsceneBlendInTime, DisableAI, EntitySpace, EyesAndYawAnimation, EyesAndYawAnimationLength, FacialAnimation, FacialAnimationLength, InitialRootBoneTransform, InterludeOrder, IsLooping, LeaveVehicleEntry, ScaleAnimationPlaybackTime, ScaleAnimationPlaybackTimeFade, SnapSoldierToTerrain, SoldierImmortalDuringCutscene, SoldierPositionAfterCutscene, SpawnPoint, StaticEntitySpace, TotalLength, UseNormalizedTime, Weapon |
| CutsceneVehicleEntityData (GameSharedResources) | 556 | 48 | `levels/` | AnimTree, Animation, AnimationFadeInTime, AnimationFarPlaybackspeedDistance, AnimationNearPlaybackspeedDistance, AnimationScalePlaybackSpeed, EntitySpace, InitialRootBoneTransform, IsLooping, PlayOnClient, RootBone, ScaleAnimationPlaybackTime, ScaleAnimationPlaybackTimeFade, SpawnPoint, TotalLength, UseNormalizedTime |
| CutsceneObjectEntityData (GameSharedResources) | 512 | 36 | `levels/` | AdditionalMeshes, AnimTree, Animation, AnimationIndex, AnimationSequence, BoundingBoxModifier, EntitySpace, InitialRootBoneTransform, IsLooping, KeepMeshAfterAnimation, Mesh, OriginalAnimationIndex, ScaleAnimationPlaybackTime, ScaleAnimationPlaybackTimeFade, SnapToTerrain, SnapToTerrainOffset, TotalLength, TrackBone, UseNormalizedTime |
| CutsceneCameraEntityData (GameSharedResources) | 124 | 48 | `levels/` | ActiveFromStart, AnimTree, Camera, EntitySpace, InitialRootBoneTransform, IsLooping, ScaleAnimationPlaybackTime, ScaleAnimationPlaybackTimeFade, TotalLength, UseNormalizedTime |
| CutsceneCameraData (GameSharedResources) | 4 | 2 | `levels/` | AnimTree, Animation, CameraBone, CameraLeaps, DofBone, FadeDuringJump, FadeTime, FadeWaitTime, FovBone, HudFx, OcclusionRayOffset, PreFadeTime, ShadowViewDistance, Skeleton, SoundOcclusion, StayFadedWhileStreaming, TotalLength, UseDof |
| GenericPropertyTrackData (Entity) | 577 | 22 | `levels/` | PropertyKeys, PropertyName, Target, Values |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 4. The scripted world: waypoints, ambient models, targets

### CAMP-WORLD-1: Waypoints, animated soldiers, ambient models and fire targets

Status: **verified** · Variants: PC

Scripted soldiers walk `WaypointData` (119) and `AnimatedWaypointData` (193: an animation in / out with its pose and
angle) paths, with `WaypointAnimationData` (13) the trees they use; `AnimationEntityData` (10) plays an animation on a
soldier. Ambient life (birds, boats, aircraft passing) is `AmbientModelEntityData` (722: a mesh moving along waypoint
splines); `StaticPositionEntityData` (634) is a bare named position the graphs refer to. The AI's fire targets are
`TargetObjectData` (284: an area, a sequence number, a strength) grouped in `TargetObjectDataGroup` (179). Spawned
campaign squads are `SquadGhostEntityData` (SQD-SQ-1).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| WaypointData (Entity) | 119 | 12 | `levels/` | AimState, PauseTime, Radius, Speed, SpeedState, Stance |
| AnimatedWaypointData (GameSharedResources) | 193 | 17 | `levels/` | AimState, AnimationIn, AnimationInDuration, AnimationInEndAngle, AnimationInEndPose, AnimationInMovement, AnimationIndex, AnimationOut, Loop, PauseTime, Radius, Speed, SpeedState, Stance |
| WaypointAnimationData (GameSharedResources) | 13 | 13 | `levels/` | AnimTrees, LoopParamName, WeightParamName |
| AnimationEntityData (GameSharedResources) | 10 | 3 | `levels/` | AnimTree, Animation, LoopAnimation, Soldier |
| AmbientModelEntityData (GameSharedResources) | 722 | 34 | `levels/` | BoundingBoxModifier, DisableOnLastWaypoint, Effect, InterpolationType, Mesh, MeshShaderSet, OffsetFromPath, PathWaypoints, RotationType, Speed, Splines, TimeBetweenLods |
| StaticPositionEntityData (GameSharedResources) | 634 | 33 | `levels/` |  |
| TargetObjectData (GameSharedResources) | 284 | 11 | `levels/` | Area, SequenceNumber, SpawnEntity, StrengthType |
| TargetObjectDataGroup (GameSharedResources) | 179 | 12 | `levels/` | TargetObjects |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 5. Descriptions and presence

### CAMP-DESC-1: Level descriptions and rich presence

Status: **verified** · Variants: PC

Each level's `<level>_Description` partition has a `LevelDescriptionAsset` (74: name, description, the level's name;
MENU-CAT-1 builds the catalogue from them) and its `RichPresenceConnection`s (the console's presence strings: context
and property pairs into `Persistence/RichPresence`'s `RichPresenceData`, with its `RichPresenceContext`s,
`RichPresenceContextValue`s and `RichPresenceString`s). Rich presence is an Xbox Live / PSN feature; the PC build
carries the data (inferred unused on PC).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| LevelDescriptionAsset (GameSharedResources) | 74 | 74 | `levels/` | Description, LevelName |
| RichPresenceData (GameSharedResources) | 1 | 1 | `persistence/richpresence.dbx` | Contexts, Default, Inactive, Properties, Strings |
| RichPresenceConnection (GameSharedResources) | 221 | 147 | `levels/` | ContextPairs, PropertyPairs, RichPresence, RichPresenceIndex |
| RichPresenceContext (GameSharedResources) | 138 | 75 | `persistence/richpresence.dbx`, `levels/` | ContextType, Index, PossibleValues |
| RichPresenceContextValue (GameSharedResources) | 1,529 | 75 | `persistence/richpresence.dbx`, `levels/` | Index |
| RichPresenceString (GameSharedResources) | 78 | 75 | `persistence/richpresence.dbx`, `levels/` | Index |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### CAMP-SAVE-1: The campaign save: a `header` and a `body` stream in `GameSettings.bin`

Status: **verified** · Variants: PC

A save is written when a `SaveEntityData` entity receives its `Save` input event (the handler compares the hash
0x7C8A0784 at 0x67AB37; the entity's `SaveDelayTime` is 0 in 179 of DICE's 183, 1.5 in 2, 0.1 and 1 in one each).
`ServerSaveEntity`'s save 0x687EC0 writes two little-endian streams (stream vtable 0x1412888: a u32 at slot +0x24, a
bool byte +0x2C, a u16 +0x3C, a u64 +0x28, raw bytes +0xC; a string is its bytes and the NUL, 0x50E4A0; a GUID is
u32, u16, u16 and 8 bytes, 0x507480), which 0x6E5930 stores as the two `GstBinary` settings `header` and `body`
of `GameSettings.bin` (SET-BIN-1):

| Stream | In order |
|---|---|
| `body` (a 1,200,000-byte buffer) | u32 `Game.GameVersion`; u32 70 (the save entity's version, set by its constructor 0x67AAD0 at 0x67AB11 and written nowhere else); the save entity's GUID (+0x14); a u32 count, then per persistent entry its GUID, a bool and the object's own serialisation (vtable +0x88); a manager's state (0x66EFF0: flags, counts, bools); three subsystems' states (vtable +0xC); an optional global's state (0x5B1DF0); the player's state (vtable +0x88) |
| `header` | u64 the save time, `_time64()` + 210,866,803,200 (seconds counted from 2,440,588 days before 1970, a Julian-day epoch; 0x509400); a string, the level to load (the game context's level object +0x48); a string, `SaveEntityData.SaveNameSID` (+0x80); a bool and, when the player's +0x9C is set, a string; a u32 from 0x18903EC's slot +0x34 (default 1 when there is no AI; the slot, 0x595B60, reads the AI's difficulty object +0x4A4: inferred the current difficulty); u32 3 (0x6882A0: inferred the header's version) |

The readers (0x55D250, 0x951E00; 0x75FFA0 on `onStartLoadingSP`) read the header's time and level string, load that
level (0x952280 -> 0x55C780) and pass the body on as a message (channel 0x28); the body's entries are CAMP-SAVE-2,
its restore CAMP-SAVE-3. SET-BIN-1's
file holds a 50-byte `header`, which fits this layout (17 fixed bytes and two short strings; inferred, the file's
strings were not read). So a profile holds one save, the last checkpoint's: a new save replaces both settings (inferred from the two fixed names).

[test: core/tests/test_spec_probes.py::run_time]
[data: spec/probes/runtime.py on DICE's partitions, 2026-11-27: SaveEntityData.SaveDelayTime 0 x 179, 1.5 x 2, 0.1 x 1, 1 x 1]
[exe: BFBC2Game.exe the Save hash `push 0x7C8A0784` at 0x67AB37; ServerSaveEntity's constructor 0x67AAD0 (`mov dword [esi+0x50], 70` at 0x67AB11) and save 0x687EC0 (GameVersion 0x688002, header's last `push 3` at 0x6882A0); the time 0x509400 (`add ecx, 0x18A41200; adc edx, 0x31` at 0x50941A); stream vtable 0x1412888, string 0x50E4A0, GUID 0x507480; the settings store 0x6E5930; readers 0x55D250, 0x951E00, 0x75FFA0; the difficulty slot: the AI singleton's vtable 0x141A168, slot +0x34 = 0x595B60 (`mov eax, [ecx+0x4A4]` at 0x595B63)]

### CAMP-SAVE-2: The body's entries: the server's entity map, each class's own record

Status: **observed** · Variants: PC

The persistent entries are the server's entity map, a GUID-keyed tree of weak references at the server context
(the object 0x547BF0 returns, 0x01566B18) + 0x2C, walked from its first node (+0x34) to its head (+0x30). Per entry
the save writes the GUID, a bool (the object is still alive) and, when alive, the object's own record (vtable +0x88;
its reader is vtable +0x84). Every record starts with the `ServerEntity` base (writer 0x68F6B0, reader 0x68F650):
a bool, the entity enabled (+0x26 bit 3), and a u32, the entity's update mode (+0x24, the value 0x627640 sets); the
reader enables or disables the entity (vtable +0x7C / +0x80) and restores the update mode. Paired writers and readers
give the stream slots: the writer's f32 +0x1C, u32 +0x24, bool +0x2C; the reader's f32 +0x1C, bool +0x24, u32 +0x3C.

The classes with a record (the base's callers): with a writer and a reader `ServerOrderEntity`, `ServerEventGateEntity`,
`ServerEventIfSwitchEntity`, `ServerPlayVideoEntity`, `ServerWeatherStateEntity`, `ServerBangerEntity`,
`ServerEventMemoryEntity`, `ServerSquadEntity`, `ServerInputRestrictionEntity`, `ServerMapMarkerEntity`,
`ServerVehicleSpawnEntity`, `ServerDeathAreaTriggerEntity` and `ServerSoldierSpawnEntity`; with a reader found and a
writer not (no direct call of the base writer: inferred through a thunk or inline) `ServerInteractionEntity`,
`ServerSoldierEntity`, `ServerArtilleryStrikeEntity`, `ServerLogicUtilEntity` (so the save entities themselves),
`ServerCutsceneObjectEntityBase`, `ServerAmmoCrateEntity`, `ServerCutsceneEntity`, `ServerPickupEntity` and
`ServerVehicleEntity`. Examples: a weather state writes its fade time so far (f32 +0x80), transitioning (bool +0x88),
its state (u32 +0x84) and finished (bool +0x89); an event gate an f32 and a u32; a play-video entity a bool; a
cutscene a bool (it has played) and an f32.

After the entries, in order:

| Part | Writer / reader | What |
|---|---|---|
| the server context's records | 0x66EFF0 / 0x66F160 | a u32 flags word (+0x4C); when its bit 0 is set, a u32 count and per 64-byte record a bool (in use) and, in use, a bool (its object live), a position and a rotation (16 bytes each), an f32 and a GUID; a live record is refreshed from its object first (vtable +0x4C, +0x2C). Which objects these are is not identified |
| three subsystems | the save entity's +0x3C: its +0x14, +0x18, +0x1C, vtable +0xC / reset +0x3C then read +0x10 | not traced |
| the AI | 0x5B1DF0 / 0x599820, when the AI exists ([0x01567178]) and `Server.IsAiEnabled` | u32 0x1926FFE0; per bot of the AI system ([0x01890324] +0x428 - +0x42C) a bool, its GUID and the bot's state (0xE469A0); two GUID lists collected from the bots (each a u32 count, then the GUIDs; 0x5ADA10); u32 0x0911E4E5 |
| one more record | vtable +0x88 | CAMP-SAVE-1's last part; the restore 0x67E030 reads nothing after the AI (open) |

[exe: BFBC2Game.exe save 0x687EC0 (the map 0x531430 = +0x2C; per entry GUID 0x507480, the bool at slot +0x2C, vtable +0x88); ServerEntity base 0x68F6B0 / 0x68F650; ServerWeatherStateEntity 0x66A490 / 0x66EE90; ServerEventGateEntity 0x668D10 / 0x668CE0; ServerPlayVideoEntity 0x669C40 / 0x669C20; ServerCutsceneEntity reader 0x6A9410; context records 0x66EFF0 / 0x66F160; AI 0x5B1DF0 (markers `push 0x1926FFE0`, `push 0x911E4E5`), 0x58F770, 0x5ADA10, reader 0x599820]
[doc: private:pseudo/0068.c:5953-6150] [doc: private:pseudo/0068.c:11976-12014] [doc: private:pseudo/0066.c:7831-7844]
[doc: private:pseudo/0066.c:11422-11444] [doc: private:pseudo/0066.c:11490-11559] [doc: private:pseudo/005B.c:1542-1566]
[doc: private:pseudo/0058.c:11182-11219] [doc: private:pseudo/005A.c:11497-11607]

### CAMP-SAVE-3: The restore runs at the server's level load and does not replay what had happened

Status: **observed** · Variants: PC

`continueSP` reads the header, loads its level and passes the body on (CAMP-SAVE-1). At the server's level load
(0x574FB0), 0x54EDB0 checks that the body is not empty, finds the level's save entity in the entity map (the first
object of the save entity's class, type object 0x1BF1800) and runs its restore 0x67E030, which:

1. reads the two u32 version words (the game version and 70) and compares neither (no test in the function:
   inferred, a save from another build is read as it is);
2. marks the save entity (+0x54) and the server (+0x64) as restoring, reads the writing save entity's GUID (looked up
   into +0x38) and registers it on the update channels 0x18 and 0x15;
3. reads the count and per entry the GUID (looked up by 0x568710) and the bool: alive, the entity reads its record
   (vtable +0x84); not alive (the entity was gone when the game saved), the entity is destroyed (0x62D810) when it is
   of the class with type object 0x1BF0EB0 and not of the one with 0x1BF1B80;
4. reads the context's records (0x66F160), resets and reads the three subsystems, reads the AI (0x599820), and
   notifies channel 0x11.

The entities' readers keep a restored state from replaying: a weather state being read sets +0x8B, so its spawn does
not start its transition again (WTH-STATE-6); a cutscene read as played is set to its finished state (+0x44 = 2, time
-1.0) and not replayed; a logic-util entity (the save entity) re-registers on channel 0x18.

[exe: BFBC2Game.exe 0x574FB0 -> 0x54EDB0 (the type test against 0x1BF1800) -> 0x67E030 (the reads through slot +0x3C at 0x67E0BB; +0x54 = 1, server +0x64 = 1; 0x568710; vtable +0x84; 0x62D810 with the types 0x1BF0EB0 / 0x1BF1B80; 0x66F160; 0x599820); ServerCutsceneEntity 0x6A9410; ServerLogicUtilEntity 0x698FF0]
[doc: private:pseudo/0054.c:13459-13496] [doc: private:pseudo/0067.c:12261-12371]

### CAMP-SAVE-4: A save needs a living soldier and finalizes the stats

Status: **observed** · Variants: PC

The save 0x687EC0 writes only when the local player (0x53F560) has a soldier (player vtable +0x30) whose object
+0xC0 answers false to its vtable +0x2C (inferred: the soldier is not dead or dying): a checkpoint reached while the
player is down writes nothing. Before writing it notifies the server's dispatcher (0x18) and, when the server's stats
manager (+0x4C) exists, runs the stats finalize (0x84CB90, PERS-SAVE-2), so every campaign save also hands the player's
changed stats to the stats listener. After writing it sets the save entity's update mode 1 and +0x55, then writes the
header. The executable's `Server.DisableCheckpoints` (the name CAMP-FLOW-1 lists) gates the stats checkpoint
(0x84CAE0), not the campaign's saves.

[exe: BFBC2Game.exe 0x687EC0 (0x53F560, vtable +0x30, [+0xC0] vtable +0x2C; server +0x4C then 0x84CB90; `Server.DisableCheckpoints` 0x14334DC read in 0x84CAE0)]
[doc: private:pseudo/0068.c:5953-6150] [doc: private:pseudo/0084.c:11007-11075]

## Open questions

- The save's body (CAMP-SAVE-2): which objects the server context's 64-byte records are, the save entity's three
  subsystems, the classes with a reader but no writer found, and the last record the restore does not read; a save
  and its reload in game (paused) would confirm the header's difficulty word.
- How the level order of the campaign is defined (the menus' mission list, MENU-CAT-1, against the levels' own
  descriptions).
