# The soldier: movement, stances, sprint, cameras, aim assistance

**Sources.** Round SPG4 (2026-11-23): the census of DICE's partitions (`spec/probes/census.py`, `spec/census.json`:
every instance class with its instances and member names, re-measured by the game tier) and `BFBC2Game.exe`'s names
(its `typeinfo` records, ENG-TI-1, and its strings). This file is the data side: which classes and members describe
the player's soldier and where DICE keeps them. Values are not listed; the tables' member lists are the members DICE's
instances carry (the de facto schema, TYPE-SCH-1), less `Name`, `MemoryReportGroup`, `Components`, `Enabled`,
`Enumeration` and `Transform`. The kit side (meshes, licences, the kit as a `SoldierEntityData`) is ANIM-KIT-1 - 3; the
bots' use of these classes is `engine/bot_ai.md`.

## 1. The character controller

### SOLD-PHYS-1: One character physics per soldier family, a state per movement mode

Status: **verified** · Variants: PC

`SoldierEntityData.CharacterPhysics` names a `CharacterPhysicsData` (DICE has two, in two partitions under
`Objects/Soldiers/`, one of them `Objects/Soldiers/Common/DefaultSoldierPhysics`). It holds the body (`Mass`,
`PhysicalRadius`, `MaxAscendAngle`, `WaterDepthLimit`, ladder angles), the input (`InputAcceleration`), the damage
and jump penalties, the `Poses` (stand, crouch, prone: CharacterPoseData) and the `States`, one per movement mode; each
state has a `PoseInfo` array of `CharacterStatePoseInfo` (per pose: velocity, acceleration / deceleration gains,
sprint gain and multiplier). The modes are ground, jump, air, falling, climbing (ladders), sliding, swimming,
parachuting and animation-controlled (a vault or a scripted move); the parachute itself (mesh, offsets, tilt and
turning) is the soldier's `SoldierParachuteData`:

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| CharacterPhysicsData (Physics) | 2 | 2 | `objects/soldiers` | InputAcceleration, JumpPenaltyFactor, JumpPenaltyTime, LadderAcceptAngle, LadderAcceptAnglePitch, Mass, Material, MaxAscendAngle, PhysicalRadius, Poses, PushableObjectWeight, Rumble, SpeedPenaltyOnDamage, SpeedPenaltyOnDamageTime, Sprint, States, WaterDepthLimit |
| CharacterPoseData (Physics) | 4 | 2 | `objects/soldiers` | EyePosition, Height, LookConstraints, PoseType, StepHeight, ThrottleModifierCurve, TransitionTime |
| CharacterStatePoseInfo (Physics) | 30 | 2 | `objects/soldiers` | AccelerationGain, DecelerationGain, PoseType, SprintGain, SprintMultiplier, StrafeDecelerationGain, Velocity |
| OnGroundStateData (Physics) | 2 | 2 | `objects/soldiers` | AllowedDistanceFromGround, GroundHugging, JumpDelay, JumpStaminaPenalty, LimitDownwardVelocity, LimitStrafeVelocity, PoseInfo |
| JumpStateData (Physics) | 2 | 2 | `objects/soldiers` | JumpEffectSize, JumpHeight, PoseInfo |
| InAirStateData (Physics) | 2 | 2 | `objects/soldiers` | FreeFallVelocity, PoseInfo |
| FallingStateData (Physics) | 2 | 2 | `objects/soldiers` | PoseInfo |
| ClimbingStateData (Physics) | 2 | 2 | `objects/soldiers` | AttractionVelocity, ClimbHeightOffset, DownAngleLimit, DropOffAngle, LateralInputScale, PoseInfo, PushAwayVelocity |
| SlidingStateData (Physics) | 2 | 2 | `objects/soldiers` | GravityScale, HorizontalInputScale, PoseInfo |
| SwimmingStateData (Physics) | 2 | 2 | `objects/soldiers` | BodyUnderWater, PoseInfo |
| ParachuteStateData (Physics) | 2 | 2 | `objects/soldiers` | PoseInfo, VerticalGain, VerticalVelocity |
| AnimationControlledStateData (Physics) | 2 | 2 | `objects/soldiers` | PoseInfo, UpNormalTolerance |
| SoldierParachuteData (GameSharedResources) | 2 | 2 | `objects/soldiers` | AimingConstraints, AnimTree, MaxTiltAngle, MaxTiltAtSpeed, ParachuteMesh, ParachuteMeshShaderSet, ParachuteOffset1p, ParachuteOffset3p, ParachuteTurningSpeed, RandomMaxTiltAngle, RandomTiltAxisOffset, RandomTiltFrequency, TiltSpeed |

The executable's poses and transitions are named `Stand`/`Crouch`/`Prone` with `StandToCrouch`, `CrouchToProne`,
`ProneToStand`, `ProneToStandOnJump` and the input concepts `ConceptCrouch`, `ToggleCrouch`, `ConceptSprint`
(engine/input.md).

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json (DICE's partitions, round SPG4, 2026-11-23): the classes' instances, partitions and members above]
[exe: BFBC2Game.exe typeinfo module Physics: CharacterPhysicsData, CharacterPoseData, CharacterStatePoseInfo and the nine *StateData classes; strings StandToCrouch, CrouchToProne, ProneToCrouch, ProneToStand, ProneToStandOnJump, StandToProne, CrouchToStand]

### SOLD-PHYS-2: The controller's nine states: their ids, how they are built, their transitions

Status: **verified** · Variants: PC

The character's state manager is built from `CharacterPhysicsData.States` (+0x6C, RTTI-PHYS-1) by 0xB08CE0: each
entry is matched by its class (its type against each state's data class, `isKindOf` 0x4E6100; the first test pushes
`OnGroundStateData`'s class info at 0xB08D5C), not by its position in the array, and its state object is added to
the manager (+0x48, built 0xD4DD40, added 0xD4DDD0); an entry of no known class is skipped. The context (+0x44,
0xD40AC0) starts in state 0 and takes `OnGroundStateData.JumpDelay` (+0x28). The states and their ids (each state's
slot 3: `mov eax, <id>; ret`; Havok's character state numbering, with falling in Havok's "flying" slot):

| Id | State (data class) | Its update | Goes to |
|---|---|---|---|
| 0 | OnGround (`OnGroundStateData`) | 0xAB7BD0 | 2 (0xAB7E41); 8 when the ground is steeper than `MaxAscendAngle` (+0x80, degrees, 0xAB8178 - 0xAB81BF) |
| 1 | Jump (`JumpStateData`) | 0xA9D5A0 | 2 after two updates (its counter +0x54 against 2 at 0xA9D700) |
| 2 | InAir (`InAirStateData`) | 0xAB44E0 | 0, 3, 4, 6, 8 |
| 3 | Climbing (`ClimbingStateData`) | 0xAACB90 | 0, 2, 6 |
| 4 | Falling (`FallingStateData`) | 0xAB42B0 | 0, 5, 6, 8 |
| 5 | Parachute (`ParachuteStateData`) | 0xAA4D60 | 0, 6, 8 |
| 6 | Swimming (`SwimmingStateData`) | 0xAAD3E0 | 0, 3 |
| 7 | AnimationControlled (`AnimationControlledStateData`) | 0xAA3BA0 | none of its own |
| 8 | Sliding (`SlidingStateData`) | 0xAA4FC0 | 0, 2, 6 |

The transitions are each update's calls of the context's `setState` (0xD40B80). From outside the controller the
character's `setState` (0xB095C0) is called by: the parachute's open (0x6F7900: 5) and close (0x6F7930: 5 becomes
4, falling); scripted moves (0x6977F4, 0x69E80F: 7); returns to the ground (0x68DA3E, 0x68DD52, 0x8ADDE4: 0;
0x6C17A2 only from 7); and the network (0xB0F760 applies a snapshot's state, +0x54), so the state is replicated.
Each of DICE's two `CharacterPhysicsData` lists all nine state classes once. For a converter or OPFOR's controller
patch: a `States` array may be in any order, a missing state class is simply absent (its transitions then find no
state; not traced), and a tenth class would be ignored.

[test: core/tests/test_spec_probes.py::run_time]
[data: spec/probes/runtime.py on DICE's partitions, 2026-11-27: the nine state classes 2 each among the States of DICE's 2 CharacterPhysicsData]
[exe: BFBC2Game.exe state builder 0xB08CE0 (`push 0x1BF90D8` at 0xB08D5C), state ids at 0xAB7B70 (`xor eax, eax; ret`), 0xA9D590, 0xA9D580, 0xA9D150, 0xA9D570, 0xA9DAC0, 0xA9DAE0, 0xA9CCC0, 0xA9DAD0 (`mov eax, 1` - `8; ret`); factories 0xAB7B00, 0xAB7A90, 0xAB7A20, 0xAB7920, 0xAB79B0, 0xAB8310, 0xAB83F0, 0xABA950, 0xAB8380; context setState 0xD40B80, character setState 0xB095C0]

### SOLD-SPR-1: Sprint is a power budget and a field-of-view change

Status: **verified** · Variants: PC

Sprinting draws on a power that drains and recovers per second (`CharacterSprintData`, inside the character physics:
`SprintMinimumPower`, `SprintPowerDecreasePerSecond`, `SprintPowerIncreasePerSecond`); the soldier's
`SoldierSprintSettingsData` sets the view's sprint field of view and its in / out delays and times, the recovery time,
whether throttle input is required and which actions interrupt a sprint. The per-pose speed multiplier is the state's
`SprintMultiplier` (SOLD-PHYS-1). The sound side is `SoldierSoundData`'s `SprintSound`, `SprintRecoverySound` and
`SprintTimeForRecovery`.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| CharacterSprintData (Physics) | 2 | 2 | `objects/soldiers` | SprintMinimumPower, SprintPowerDecreasePerSecond, SprintPowerIncreasePerSecond |
| SoldierSprintSettingsData (GameSharedResources) | 2 | 2 | `objects/soldiers` | Fov, FovInDelay, FovInTime, FovOutDelay, FovOutTime, InterruptingActions, RecoverTime, ThrottleInputRequired |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: CharacterSprintData 2, SoldierSprintSettingsData 2 (soldiers and Vietnam soldiers)]

## 2. Collision, sound and the ragdoll

### SOLD-COL-1: The head and bone collision, fake springs, the soldier's sounds

Status: **verified** · Variants: PC

Hits are tested against a head sphere per pose (`SoldierHeadCollisionData`: `HeadRadius`, `PoseStates`) and bone
collision volumes (`SoldierBoneCollisionData`, and for animated stand-ins `FakeAnimatedSoldierCollisionData`); the
hit material per part is MAT-SLOT-1's. Gear that dangles (radio antennas, straps) and vehicle antennas are
`FakeSpringData` chains (`SoldierEntityData.BoneFakePhysics`). The soldier's own sounds are one `SoldierSoundData`
per family (footsteps by animation trigger, breath, sprint, swim, healing, deploy and zoom sounds). The ragdoll is
SOLD-RAG-1.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SoldierHeadCollisionData (GameSharedResources) | 2 | 2 | `objects/soldiers` | HeadRadius, PoseStates |
| SoldierBoneCollisionData (GameSharedResources) | 4 | 4 | `objects/soldiers` | BoneCollisionData |
| FakeAnimatedSoldierCollisionData (GameSharedResources) | 1 | 1 | `objects/soldiers` | FakeBoneInfo, FakePoses, SaveLatencyInfoCount, SaveLatencyInfoTime, TimeEnableAimAfterShooting |
| FakeSpringData (Physics) | 106 | 29 | `objects/soldiers`, `objects/vehicles`, `objects/weapons` | Acceleration, ChildFakePhysics, Damping, Direction, EndDampening, GravityModifier, Length, Mass, MaxSpeed, MinSpeed, ProgressiveExponent, StartDampening, StartSpeed |
| SoldierSoundData (GameSharedResources) | 3 | 3 | `objects/soldiers` | BreathSound, BulletImpactSound, BulletImpactsOnLocalSoldiersOnly, BulletImpactsPauseTime, ChangePoseSound, ClimbSound, ClothesSound, DeathSound, FootStepAnimTrigPositions3P, FreeFallingSound, HealingFinishedSound, HealingSound, JumpSound, ParachuteActiveSound, SprintRecoverySound, SprintSound, SprintTimeForRecovery, SwimSound, TimeBetweenTriggers, VegetationCollisionPauseTime, VegetationCollisionSound, VegetationCollisionVelocityThreshold, WeaponDeploySound, WeaponZoomSound |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### SOLD-RAG-1: Ragdoll settings

Status: **verified** · Variants: PC

`SoldierEntityData.RagdollSettings` names a `SoldierRagdollSettingsData` (one, shared) and the animated soldiers an
`AnimatedSoldierRagdollSettingsData`; both point at the one ragdoll (ANIM-RAG-1, HAVOK-RAG-1). The bots' animation
states are an `AIStateAnimationData` per family (no members set: a marker the AI anim tree is wired by).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SoldierRagdollSettingsData (GameSharedResources) | 1 | 1 | `objects/soldiers` | BulletImpulseMultiplier, CollisionImpulseMultiplier, DeathImpulseMultiplier, DeathSpasms, ExplosionImpulseMultiplier, HitBoneImpulseModifier, MaxImpulse, StaticBulletImpulse, StaticCollisionImpulse, StaticDeathImpulse, StaticExplosionImpulse |
| AnimatedSoldierRagdollSettingsData (GameSharedResources) | 1 | 1 | `objects/soldiers` | InstantRagdollImpulseThreshold, InstantRagdollSpeedThreshold, Ragdoll, RagdollMaterial |
| AIStateAnimationData (GameSharedResources) | 2 | 2 | `objects/soldiers` |  |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 3. Cameras and view recoil

### SOLD-CAM-1: The first-person camera and its springs

Status: **verified** · Variants: PC

A hand-held weapon's first-person camera (`FirstPersonCameraData`, 4: one per weapon family's camera partition under
`Objects/Weapons/Handheld/Common/`) moves and rotates the view with the soldier's movement and turning, separately
when zoomed (`Zoom*` members), and couples it to the weapon by springs (`CameraSpringEffect`, `WeaponSpringEffect`);
`WeaponLagSpringEffectData` is the weapon's lag on jump, land and pose changes. View recoil is a damped spring per
weapon (`CameraRecoilData`: `SpringConstant`, `SpringDamping`, `SpringMinThresholdAngle`; 82 instances, 80 of them
weapons'), separate from the weapon's own recoil (WPN-SWAY). The soldier's strafe tilt is
`SoldierEntityData.CameraStrafeTiltStrength`.

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| FirstPersonCameraData (GameSharedResources) | 4 | 4 | `objects/weapons` | CameraSpringEffect, MoveForwardModifier, MoveStrafeModifier, Offset, OffsetReleaseModifier, ReleaseModifier, ReleaseModifierPitch, ReleaseModifierRoll, ReleaseModifierYaw, RotatePitchModifier, RotateRollModifier, RotateYawModifier, Rotation, RotationAdditionToOffset, WeaponSpringEffect, ZoomMoveForwardModifier, ZoomMoveStrafeModifier, ZoomOffsetReleaseModifier, ZoomReleaseModifierPitch, ZoomReleaseModifierRoll, ZoomReleaseModifierYaw, ZoomRotatePitchModifier, ZoomRotateRollModifier, ZoomRotateYawModifier |
| WeaponLagSpringEffectData (GameSharedResources) | 8 | 4 | `objects/weapons` | JumpForceZoomModifier, JumpForces, LandForceZoomModifier, LandForces, OffsetSprings, PoseDownForces, PoseUpForces, RotationSprings |
| CameraRecoilData (GameSharedResources) | 82 | 82 | `objects/weapons`, `objects/vehicles` | SpringConstant, SpringDamping, SpringMinThresholdAngle |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

### SOLD-FOV-1: The field of view: a level default, an infantry multiplier, the sprint and zoom changes

Status: **verified** · Variants: PC

The view's field of view is the level's `LevelData.DefaultFOV` scaled for infantry by `InfantryFOVMultiplier`, then
changed by sprint (SOLD-SPR-1's `Fov`), by a weapon's zoom (ANIM-ZOOM-2's specialisations) and by a vehicle camera's
alternate view (`AlternateCameraViewData.FieldOfView`, SOLD-CAM-2). The user's own FOV is `settings.ini`'s
`[Graphics] Fov` (SET-INI-1). How the engine combines them is not traced.

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: LevelData members DefaultFOV and InfantryFOVMultiplier; SoldierSprintSettingsData member Fov; AlternateCameraViewData member FieldOfView]
[exe: BFBC2Game.exe strings DefaultFOV, InfantryFOVMultiplier, FovModifier, ForceFov, RenderFov, ZoomRenderFov, FovInDelay, FovOutDelay, FovOutTime]

### SOLD-CAM-2: Vehicle and turret cameras: chase, static, alternate views

Status: **verified** · Variants: PC

A vehicle seat's or a stationary weapon's view is a `ChaseCameraData` (third person: distance, angle, collision pill,
fades, target offset; 294 instances) or a `StaticCameraData` (a mounted view with yaw / pitch limits and curves, input
actions and zoomed / unzoomed sensitivities; 161), each naming its `Hud` and `HudFx`. An `AlternateCameraViewData` is
a seat's second view (a gunner's optics: field of view, mesh, fades; 72). The UAV and TOW views' screen effects are
`CrtHudFxData` / `CrtAnalogLinkHudFxData` (HUD-FX-1).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| ChaseCameraData (GameSharedResources) | 294 | 91 | `objects/vehicles`, `levels/` | AwayFromTargetForceScale, CollisionRadius, CriticalHudFx, FadeDuringJump, FadeTime, FadeWaitTime, ForceFieldForceScale, ForceFieldRadius, HasCollision, Hud, HudFx, ImpactHudFxInfo, InheritTargetVelocity, KeepTargetPitch, LookDistanceInFrontOfTarget, LookDistanceScale, MaxDistance, MaxVelocity, MaxViewRotationAngleDeg, OcclusionRayOffset, PillExpandSizeSpeedAcceleration, PillMaximumCollisionRadius, PillMinimumCollisionLength, PillMinimumCollisionRadius, PreFadeTime, ShadowViewDistance, ShouldRollWithTarget, SnapDistance, SoundOcclusion, SoundShakeFactor, StayFadedWhileStreaming, TargetOffset, TargetRotationOffset, ToWantedPositionScale, UpdateRate, VelocityDrag, WantedAngleDeg, WantedDistance |
| StaticCameraData (GameSharedResources) | 161 | 89 | `objects/vehicles`, `objects/weapons`, `levels/` | AccumulateInput, AverageFilterFrames, CriticalHudFx, DownCurve, DownPitchAngle, FadeDuringJump, FadeTime, FadeWaitTime, Hud, HudFx, ImpactHudFxInfo, ImpulseSettings, LeftCurve, LeftYawAngle, LoosePartPhysicsArray, MirrorCurve, OcclusionRayOffset, PitchInputAction, PitchSensitivityNonZoomed, PitchSensitivityZoomed, PreFadeTime, ResetAccumulatedInputOnViewChange, RightCurve, RightYawAngle, ShadowViewDistance, SoundOcclusion, SoundShakeFactor, StayFadedWhileStreaming, UpCurve, UpPitchAngle, YawInputAction, YawSensitivityNonZoomed, YawSensitivityZoomed |
| AlternateCameraViewData (GameSharedResources) | 72 | 52 | `objects/vehicles`, `objects/weapons` | BlackDuration, FadeInDuration, FadeOutDuration, FadeToBlack, FieldOfView, FovTransitionTime, Hud, InputSuppression, Mesh, MeshOffset, ToggleViewChange |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 4. Aim assistance and switching

### SOLD-AIM-1: Auto-aim, the halo aimer and the aiming constraints

Status: **verified** · Variants: PC

Aim assistance is two layers: `SoldierAutoAimData` (per pose) and the `HaloAimerData` stick aimer (7: snap, sticky and
"suck" zones, input polynomials, yaw / pitch speeds, its zoomed multipliers), the pad's aim assist that OPFOR's
controller work builds on (INP-PAD-1). `SoldierAimingConstraintsData` limits the aim while swimming;
`WeaponSwitchingData` maps switches and the gadget cool-down. The executable's `AimAssist` game setting is hidden
from the menus (SET-GS-1).

| Class | Instances | Partitions | Where | Members |
|---|---|---|---|---|
| SoldierAutoAimData (GameSharedResources) | 2 | 2 | `objects/soldiers` | Poses |
| HaloAimerData (GameSharedResources) | 7 | 1 | `objects/soldiers` | AccelerationDamping, AccelerationInputThreshold, AccelerationMultiplier, InputPolynomial, PitchSpeed, SnapBoxScale, SnapDistanceScale, StickyBoxScale, StickyDistanceScale, SuckDistanceFallOff, SuckOwnRequiredMovementForMaximumSuck, SuckOwnSpeedInfluence, SuckPitch, SuckSoftZone, SuckStartInputThreshold, SuckTargetSpeedInfluence, SuckUserInputMultiplier, SuckYaw, SuckZoomingMultiplier, SuckZoomingPostTime, UsePitchAcceleration, UseYawAcceleration, YawSpeed, ZoomedInputPolynomial |
| SoldierAimingConstraintsData (GameSharedResources) | 1 | 1 | `objects/soldiers` | SwimmingConstraints |
| WeaponSwitchingData (GameSharedResources) | 1 | 1 | `objects/soldiers` | SwitchMap, SwitchToGadgetCooldownTime |

[test: core/tests/test_spec_probes.py::census]
[data: spec/census.json, 2026-11-23: the classes above]

## 5. Spotting

### SOLD-SPOT-1: Spotting is per-soldier timing and per-weapon cone settings

Status: **observed** · Variants: PC

A soldier is shown to the enemy when spotted for `SoldierEntityData.ActiveSpottedTime` (spotted by a player),
`PassiveSpottedTime` (by a motion sensor or a UAV) and `LastKnownSpottedTime` (the fading last-known marker), and is
spotted automatically when damaged if `IsSpottedWhenDamaged`. The executable's other spotting names: the cone
(`SpottingConeFov`, `SpottingConeFalloff`, `SpottingDistance`), automatic spotting on fire and zoom (`AutoSpotOnFire`,
`AutoSpotOnZoom`, `IsSpotWhenZoomedEnabled`), the minimap (`MinimapSpotting`), the sound (`SpottingSound`), squad
orders (`CanSpotAndOrder`), the score (`SpotAssistScore`) and the voice-over events `ETSpottedNewEnemy`,
`ETSpottedKnownEnemy`, `ETSpottedMine`, `ETSpottedExplosionPack`, `ETSpottedObject`, `ETEnemySpottedBehindPlayer`
(SND-VO). The cones' angles are each weapon's `...WeaponEffectsData` (`ActiveSpotConeAngle`, `FocusConeAngle`,
`OrderConeAngle`, `CanSpotAndOrder`: DATA-WPN-1).

[data: spec/census.json, 2026-11-23: SoldierEntityData members ActiveSpottedTime, PassiveSpottedTime, LastKnownSpottedTime, IsSpottedWhenDamaged]
[exe: BFBC2Game.exe strings SpottingConeFov, SpottingConeFalloff, SpottingDistance, AutoSpotOnFire, AutoSpotOnZoom, IsSpotWhenZoomedEnabled, MinimapSpotting, SpottingSound, CanSpotAndOrder, SpotAssistScore, OnSpottedType, SoldierOnSpottedType]

## Open questions

- The order the engine applies the FOV sources (SOLD-FOV-1) and whether `settings.ini`'s `Fov` replaces or scales
  `DefaultFOV`.
- How the spotting cone's angles combine with `SpottingConeFov` / `SpottingDistance` (SOLD-SPOT-1).
- The character controller (SOLD-PHYS-2): where Jump (1) is entered (OnGround's input path), the conditions behind
  InAir's and Falling's exits (`WaterDepthLimit` to swimming), what a `States` array missing a class does; a playtest
  of the parachute closing into falling.
