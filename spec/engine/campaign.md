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

## Open questions

- The save file's layout (`SaveFileName`) and where it is written (PERS-SAVE).
- How the level order of the campaign is defined (the menus' mission list, MENU-CAT-1, against the levels' own
  descriptions).
